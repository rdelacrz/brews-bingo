import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId } from "./fixtures.js";

beforeEach(async () => {
  await reset();
  await resetStorage();
  await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll());
});
function directory() { return env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")); }
function owner(id) { return env.GAMES.get(env.GAMES.idFromName(id)); }
async function get(path, session, headers = {}) {
  return exports.default.fetch(`${origin}${path}`, { headers: { ...headers, ...(session ? { Cookie: session } : {}) } });
}
async function lobby() {
  const actor = await enroll("DiscoveryBoundaryHost");
  const created = await post("/api/games", {}, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const opened = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(opened.status).toBe(200);
  const code = (await opened.json()).game_code;
  return { actor, id, code };
}
async function unchangedDomain(id) {
  return runInDurableObject(owner(id), (_instance, state) => ({
    record: [...state.storage.sql.exec("SELECT * FROM game_record")],
    revisions: [...state.storage.sql.exec("SELECT * FROM game_view_revisions ORDER BY view_key")],
    grants: [...state.storage.sql.exec("SELECT * FROM game_connection_grants ORDER BY grant_id")],
    memberships: [...state.storage.sql.exec("SELECT * FROM game_players ORDER BY player_id")],
    receipts: [...state.storage.sql.exec("SELECT * FROM game_receipts ORDER BY actor,command_id")],
    pending: [...state.storage.sql.exec("SELECT * FROM game_pending_work ORDER BY operation_id")],
    sessions: [...state.storage.sql.exec("SELECT * FROM game_sessions ORDER BY session_id")],
  }));
}

test("discovery normalizes approved lowercase codes and percent-encoded outer spaces", async () => {
  const { id, code } = await lobby();
  for (const spelling of [code.toLowerCase(), `%20${code.toLowerCase()}%20`, code]) {
    const response = await get(`/api/game-codes/${spelling}`);
    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({ game_id: id, game_code: code, state: "awaiting_players", can_join_spectator: true });
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(response.headers.has("Set-Cookie")).toBe(false);
  }
  const availability = await get(`/api/games/${id}/availability?view=entry&game_code=%20${code.toLowerCase()}%20`);
  expect(availability.status).toBe(200);
  expect(await availability.json()).toEqual({ view: "entry", state: "awaiting_players", can_join_player: true, can_join_spectator: true });
});

test("discovery accepts harmless browser and tracing metadata while rejecting bearer authority", async () => {
  const { actor, code } = await lobby();
  const metadata = { "Sec-CH-UA": '"Browser";v="1"', "CF-EW-Via": "15", "Traceparent": "00-00000000000000000000000000000001-0000000000000001-01" };
  for (const [path, session] of [["/api/games/current", actor.session], [`/api/game-codes/${code}`, undefined]]) {
    const response = await get(path, session, metadata);
    expect(response.status).toBe(200);
  }
  const denied = await get("/api/games/current", actor.session, { Authorization: "Bearer untrusted-authority" });
  expect(denied.status).toBe(403);
});

test("cross-host discovery is read-only and does not create result grants or renew idle deadlines", async () => {
  const { actor, id, code } = await lobby();
  const viewer = await enroll("DiscoveryBoundaryViewer");
  const before = await unchangedDomain(id);
  const current = await get("/api/games/current", viewer.session);
  expect(current.status).toBe(200);
  const summary = await current.json();
  expect(summary.slot).toBe("occupied");
  expect(summary.game).toMatchObject({ game_id: id, designated_host_id: actor.accountId, game_code: code, may_operate: false });
  const list = await get("/api/games?state=awaiting_players", viewer.session);
  expect(list.status).toBe(200);
  expect((await list.json()).games).toHaveLength(1);
  const availability = await get(`/api/games/${id}/availability?view=account`, viewer.session);
  expect(availability.status).toBe(200);
  expect(await availability.json()).toMatchObject({ view: "account", can_start: false, player_count: 0, spectator_count: 0 });
  expect(await unchangedDomain(id)).toEqual(before);
  const cancelled = await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "awaiting_players" }, { session: actor.session });
  expect(cancelled.status).toBe(200);
  expect((await get(`/api/game-codes/${code}`)).status).toBe(404);
});

test("public entry projection has no private occupancy or membership and never silently uses invalid account credentials", async () => {
  const { id, code } = await lobby();
  const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
  expect(context.status).toBe(200);
  const joined = await post(`/api/games/${id}/players`, { game_code: code, alias: "OnlyPlayer" }, { session: cookie(context) });
  expect(joined.status).toBe(200);
  const player = cookie(joined);
  for (const path of [`/api/game-codes/${code}`, `/api/games/${id}/availability?view=entry&game_code=${code}`]) {
    const response = await get(path);
    expect(response.status).toBe(200);
    const body = await response.json();
    for (const name of ["player_count", "players", "connected_player_count", "board", "recovery_enabled", "sessions", "designated_host_id"]) expect(body[name]).toBeUndefined();
    expect(JSON.stringify(body)).not.toContain("OnlyPlayer");
  }
  const own = await get(`/api/games/${id}/availability?view=player`, player);
  expect(own.status).toBe(200);
  expect(await own.json()).toEqual({ view: "player", state: "awaiting_players", can_join_player: true, can_join_spectator: true });
  const invalid = await get(`/api/games/${id}/availability?game_code=${code}`, "__Host-brews_session=invalid-selected-session");
  expect([400, 401]).toContain(invalid.status);
  const selected = await get(`/api/games/${id}/availability?view=account`, "__Host-brews_session=invalid-selected-session");
  expect(selected.status).toBe(401);
});

test("unknown code does not reserve or create a game and invalid availability proof is denied", async () => {
  const actor = await enroll("DiscoveryBoundaryUnknown");
  const unknownId = commandId();
  expect((await get("/api/game-codes/ZZZZZZZZ")).status).toBe(404);
  expect((await get(`/api/games/${unknownId}/availability?view=entry&game_code=ZZZZZZZZ`)).status).toBe(404);
  const current = await get("/api/games/current", actor.session);
  expect(current.status).toBe(200);
  expect(await current.json()).toEqual({ slot: "free", game: null });
  await runInDurableObject(directory(), (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT * FROM directory_game_index")]).toEqual([]);
    expect(state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id).toBeNull();
  });
});

test("idle expiry observed by public discovery fences a subsequent clock rollback", async () => {
  const { id, code } = await lobby();
  const due = await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT idle_due FROM game_record").one().idle_due);
  const originalNow = Date.now;
  try {
    Date.now = () => due;
    expect((await get(`/api/game-codes/${code}`)).status).toBe(404);
    await runInDurableObject(owner(id), (_instance, state) => {
      expect(state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms).toBeGreaterThanOrEqual(due);
    });
    Date.now = () => due - 1;
    expect((await get(`/api/game-codes/${code}`)).status).toBe(503);
  } finally { Date.now = originalNow; }
});

test("revocation after the initial owner account proof cannot disclose a summary", async () => {
  const { actor, id } = await lobby();
  const accounts = env.ACCOUNTS.get(env.ACCOUNTS.idFromName("accounts"));
  await runInDurableObject(accounts, (instance, state) => {
    instance.boundaryFetch = instance.fetch.bind(instance);
    instance.boundaryAuthorizations = 0;
    instance.fetch = async request => {
      const response = await instance.boundaryFetch(request.clone());
      if (new URL(request.url).pathname === "/game-authority") {
        const body = await request.clone().json();
        if (body.action === "authorize" && ++instance.boundaryAuthorizations === 2) {
          state.storage.sql.exec("UPDATE account_sessions SET revoked_at=? WHERE account_id=?", Date.now(), actor.accountId);
        }
      }
      return response;
    };
  });
  try {
    const response = await get("/api/games/current", actor.session);
    expect(response.status).toBe(401);
    expect(JSON.stringify(await response.json())).not.toContain(id);
    await runInDurableObject(accounts, instance => expect(instance.boundaryAuthorizations).toBeGreaterThanOrEqual(3));
  } finally {
    await runInDurableObject(accounts, instance => {
      instance.fetch = instance.boundaryFetch;
      delete instance.boundaryFetch;
      delete instance.boundaryAuthorizations;
    });
  }
});
