import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, evictDurableObject, listDurableObjectIds, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, inOwner, seedPending } from "./fixtures.js";

const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
const get = (path, session, extra = {}) => exports.default.fetch(`${origin}${path}`, { headers: { ...(session ? { Cookie: session } : {}), ...extra } });
async function value(path, session) {
  const response = await get(path, session);
  expect(response.status).toBe(200);
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  expect(response.headers.get("Referrer-Policy")).toBe("no-referrer");
  expect(response.headers.get("X-Content-Type-Options")).toBe("nosniff");
  expect(response.headers.has("Set-Cookie")).toBe(false);
  return response.json();
}
async function newGame() {
  const actor = await enroll();
  const response = await post("/api/games", { configuration: { numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false, free_cell_positions: [], player_capacity: 2, spectator_capacity: 5 } }, { session: actor.session });
  expect(response.status).toBe(200);
  return { actor, id: (await response.json()).game.game_id };
}
async function lobby(fixture) {
  const response = await post(`/api/games/${fixture.id}/lobby`, { expected_revision: 0 }, { session: fixture.actor.session });
  expect(response.status).toBe(200);
  return (await response.json()).game_code;
}
async function player(id, code, alias) {
  const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
  expect(context.status).toBe(200);
  const joined = await post(`/api/games/${id}/players`, { game_code: code, alias }, { session: cookie(context) });
  expect(joined.status).toBe(200);
  return cookie(joined);
}
async function stream(id, session) {
  const response = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=player`, { headers: { Cookie: session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  const initial = new Promise(resolve => socket.addEventListener("message", event => resolve(JSON.parse(event.data)), { once: true }));
  socket.accept();
  await initial;
  return socket;
}
async function rows(id) {
  return runInDurableObject(owner(id), async (_instance, state) => {
    const names = [...state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name GLOB 'game_*' ORDER BY name")].map(r => r.name);
    return { tables: Object.fromEntries(names.map(name => [name, [...state.storage.sql.exec(`SELECT * FROM ${name} ORDER BY 1,2`)]])), alarm: await state.storage.getAlarm() };
  });
}
beforeEach(async () => { await reset(); await resetStorage(); await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll()); });

test("discovery returns honest free and New slots with read-only cross-host/admin summaries", async () => {
  const actor = await enroll("DashboardOwner");
  expect((await get("/api/games/current")).status).toBe(401);
  expect(await value("/api/games/current", actor.session)).toEqual({ slot: "free", game: null });
  expect(await value("/api/games", actor.session)).toEqual({ slot: "free", games: [], next_cursor: null });
  const created = await post("/api/games", {}, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const peer = await enroll("DashboardPeer");
  const before = await rows(id);
  const current = await value("/api/games/current", peer.session);
  expect(current.slot).toBe("occupied");
  expect(current.game).toEqual({ game_id: id, state: "new", designated_host_id: actor.accountId, created_at: expect.any(Number), started_at: null, idle_due: expect.any(Number), game_code: null, may_operate: false, view_revision: 0 });
  expect(await value("/api/games?state=awaiting_players", actor.session)).toEqual({ slot: "occupied", games: [], next_cursor: null });
  expect((await value("/api/games?state=new", actor.session)).games).toHaveLength(1);
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", peer.accountId));
  expect((await value("/api/games/current", peer.session)).game.may_operate).toBe(true);
  const after = await rows(id);
  // Required Admin capture may maintain only audit, clock and alarm bookkeeping.
  for (const name of Object.keys(before.tables).filter(name => !["game_admin_audit", "game_metadata", "game_delivery_metadata"].includes(name))) expect(after.tables[name]).toEqual(before.tables[name]);
  expect(after.tables.game_admin_audit).toHaveLength(1);
  expect(after.tables.game_admin_audit[0]).toMatchObject({actor_account_id: peer.accountId, target_game_id: id, operation: "discovery_read", outcome: "succeeded"});
  expect(after.tables.game_admin_audit[0].expires_at).toBe(after.tables.game_admin_audit[0].occurred_at + 7776000000);
  expect(after.tables.game_metadata).toHaveLength(before.tables.game_metadata.length);
  expect(after.tables.game_metadata[0].schema_version).toBe(before.tables.game_metadata[0].schema_version);
  expect(after.tables.game_metadata[0].singleton).toBe(before.tables.game_metadata[0].singleton);
  expect(after.tables.game_metadata[0].command_floor_ms).toBeGreaterThanOrEqual(before.tables.game_metadata[0].command_floor_ms);
  expect(after.tables.game_metadata[0].command_floor_ms).toBeLessThanOrEqual(after.tables.game_metadata[0].last_observed_ms);
  expect(after.tables.game_metadata[0].last_observed_ms).toBeGreaterThanOrEqual(before.tables.game_metadata[0].last_observed_ms);
  expect(after.tables.game_delivery_metadata.map(row => ({...row, last_observed_ms: before.tables.game_delivery_metadata[0].last_observed_ms}))).toEqual(before.tables.game_delivery_metadata);
  expect(after.tables.game_delivery_metadata[0].last_observed_ms).toBeGreaterThanOrEqual(before.tables.game_delivery_metadata[0].last_observed_ms);
  expect(after.alarm).toBeLessThanOrEqual(after.tables.game_admin_audit[0].expires_at);
  expect((await get(`/api/games/${id}/availability?view=account`, actor.session)).status).toBe(404);
});

test("discovery grammar rejects retry, bearer, body, filters, unknown selectors and ambiguous cookies", async () => {
  const { actor, id } = await newGame();
  for (const path of ["/api/games?state=resolved", "/api/games?limit=1", "/api/games?cursor=x", "/api/games?state=new&state=new", "/api/games/current?view=account", `/api/games/${id}/availability?view=entry`, `/api/games/${id}/availability?view=account&game_code=ABCD1234`, `/api/games/${id}/availability?view=unknown`]) expect((await get(path, actor.session)).status).toBe(400);
  for (const [header, input, expected] of [["Authorization", "Bearer", 403], ["Idempotency-Key", "", 400], ["X-View", "account", 400], ["Origin", "null", 403]]) expect((await get("/api/games/current", actor.session, { [header]: input })).status).toBe(expected);
  expect((await exports.default.fetch(`http://localhost:8787/api/games/current`, { headers: { Cookie: actor.session } })).status).toBe(403);
  expect((await get(`/api/games/${id}/availability`, `${actor.session}; __Secure-brews-player=invalid-selected`)).status).toBe(400);
  expect((await get(`/api/games/${id}/availability?view=player`, actor.session)).status).toBe(401);
  const pending = await seedPending("RestrictedDiscovery");
  const redeemed = await post("/api/auth/enrollment/redeem", { enrollment_token: pending.token });
  expect((await get("/api/games/current", cookie(redeemed))).status).toBe(401);
});

test("unknown codes never contact or initialize Game and codes normalize through strict transport", async () => {
  const f = await newGame();
  await runInDurableObject(owner(f.id), instance => {
    instance.discoverySavedFetch = instance.fetch.bind(instance);
    instance.discoveryCalls = 0;
    instance.fetch = async request => { if (new URL(request.url).pathname === "/discovery") instance.discoveryCalls++; return instance.discoverySavedFetch(request); };
  });
  try {
    for (const code of ["ABCD1234", "bad", "%25", "%GG", "%2541BCD1234"]) expect((await get(`/api/game-codes/${code}`)).status).toBe(404);
    expect(await runInDurableObject(owner(f.id), instance => instance.discoveryCalls)).toBe(0);
    const code = await lobby(f);
    const entry = await value(`/api/game-codes/%20${code.toLowerCase()}%20`);
    expect(entry).toEqual({ game_id: f.id, game_code: code, state: "awaiting_players", can_join_player: true, can_join_spectator: true });
    expect(await value(`/api/games/${f.id}/availability?view=entry&game_code=${code}`)).toEqual({ view: "entry", state: "awaiting_players", can_join_player: true, can_join_spectator: true });
    expect((await get(`/api/games/${f.id}/availability?view=entry&game_code=ABCD1234`)).status).toBe(404);
  } finally { await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.discoverySavedFetch; delete instance.discoverySavedFetch; }); }
});

test("a failed initializer reserves preparing without exposing its pending identity", async () => {
  const actor = await enroll();
  await runInDurableObject(directory(), instance => {
    instance.discoverySavedFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const response = await instance.discoverySavedFetch(request.clone());
      if (new URL(request.url).pathname === "/games" && (await request.json()).action === "claim") throw new Error("test-only lost genuine claim response");
      return response;
    };
  });
  try {
    expect((await post("/api/games", {}, { session: actor.session })).status).toBe(503);
  } finally { await runInDurableObject(directory(), instance => { instance.fetch = instance.discoverySavedFetch; delete instance.discoverySavedFetch; }); }
  expect(await value("/api/games/current", actor.session)).toEqual({ slot: "preparing", game: null });
  expect(await value("/api/games", actor.session)).toEqual({ slot: "preparing", games: [], next_cursor: null });
});

test("hidden code publication and stale terminal release never masquerade as an available game", async () => {
  const f = await newGame();
  await runInDurableObject(directory(), (_instance, state) => state.storage.sql.exec("CREATE TRIGGER discovery_publish_fault BEFORE UPDATE ON directory_game_index WHEN NEW.publication_state='published' BEGIN SELECT RAISE(ABORT,'test publication fault'); END"));
  expect((await post(`/api/games/${f.id}/lobby`, { expected_revision: 0 }, { session: f.actor.session })).status).toBe(202);
  const hidden = await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT game_code FROM game_record").one().game_code);
  expect((await value("/api/games/current", f.actor.session)).game.game_code).toBeNull();
  expect((await get(`/api/game-codes/${hidden}`)).status).toBe(404);
  await runInDurableObject(directory(), (_instance, state) => state.storage.sql.exec("DROP TRIGGER discovery_publish_fault"));
  // Force the existing genuine pending publication retry rather than forge a peer ACK.
  await runInDurableObject(owner(f.id), async (instance, state) => { state.storage.sql.exec("UPDATE game_pending_work SET next_attempt_at=?", Date.now()); await instance.alarm(); });
  expect((await value(`/api/game-codes/${hidden}`)).game_id).toBe(f.id);
  await runInDurableObject(directory(), (_instance, state) => state.storage.sql.exec("CREATE TRIGGER discovery_release_fault BEFORE UPDATE ON directory_global_reservation BEGIN SELECT RAISE(ABORT,'test release fault'); END"));
  try {
    expect((await post(`/api/games/${f.id}/cancel`, { expected_state: "awaiting_players", confirmed: true }, { session: f.actor.session })).status).toBe(202);
    expect(await value("/api/games/current", f.actor.session)).toEqual({ slot: "releasing", game: null });
    expect((await get(`/api/game-codes/${hidden}`)).status).toBe(404);
    expect((await get(`/api/games/${f.id}/availability?view=entry&game_code=${hidden}`)).status).toBe(404);
  } finally { await runInDurableObject(directory(), (_instance, state) => state.storage.sql.exec("DROP TRIGGER discovery_release_fault")); }
});

test("availability counts only valid connected players, capacities and ownership without grants or presence writes", async () => {
  const f = await newGame(); const code = await lobby(f);
  const peer = await enroll("AvailabilityPeer");
  const players = [await player(f.id, code, "First"), await player(f.id, code, "Second")];
  const sockets = [await stream(f.id, players[0]), await stream(f.id, players[1])];
  try {
    const before = await rows(f.id);
    const path = `/api/games/${f.id}/availability?view=account`;
    expect(await value(path, f.actor.session)).toEqual({ view: "account", state: "awaiting_players", can_join_player: false, can_join_spectator: true, player_count: 2, connected_player_count: 2, player_capacity: 2, spectator_count: 0, spectator_capacity: 5, can_start: true });
    expect((await value(path, peer.session)).can_start).toBe(false);
    expect(await value(`/api/games/${f.id}/availability?view=player`, `${f.actor.session}; ${players[0]}`)).toEqual({ view: "player", state: "awaiting_players", can_join_player: false, can_join_spectator: true });
    expect(await rows(f.id)).toEqual(before);
    expect((await get(`/api/games/${f.id}/availability?view=player`, "__Secure-brews-player=invalid-selected")).status).toBe(401);
    // Invalid-player denial may only fence metadata; capture after it for stable reads.
    const afterDenial = await rows(f.id);
    expect(afterDenial.tables.game_record).toEqual(before.tables.game_record);
    expect(afterDenial.tables.game_view_revisions).toEqual(before.tables.game_view_revisions);
    expect(afterDenial.tables.game_connection_grants).toEqual(before.tables.game_connection_grants);
    expect(afterDenial.alarm).toEqual(before.alarm);
    const sync = await value(`/api/games/${f.id}/sync?view=account`, f.actor.session);
    const started = await post(`/api/games/${f.id}/start`, { expected_revision: sync.view_revision }, { session: f.actor.session });
    expect(started.status).toBe(200);
    await runInDurableObject(directory(), (_instance, state) => state.storage.sql.exec("UPDATE directory_game_index SET state='awaiting_players',started_at=NULL,source_revision=1 WHERE game_id=?", f.id));
    const afterStart = await rows(f.id);
    expect((await value("/api/games/current", peer.session)).game.state).toBe("in_progress");
    expect((await value(`/api/game-codes/${code}`)).can_join_player).toBe(false);
    expect((await value(path, f.actor.session)).can_start).toBe(false);
    expect(await rows(f.id)).toEqual(afterStart);
    await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("UPDATE game_sessions SET revoked_at=? WHERE player_id=(SELECT player_id FROM game_players WHERE alias='First')", Date.now()));
    expect((await value(path, f.actor.session)).connected_player_count).toBe(1);
    expect((await get(`/api/games/${f.id}/availability?view=player`, players[0])).status).toBe(401);
  } finally { for (const socket of sockets) socket.close(1000); }
});

test.each(["account", "directory", "game"])("discovery fails closed on genuine %s peer or SQL failure", async fault => {
  const f = await newGame(); const code = await lobby(f);
  if (fault === "account") await inOwner(instance => { instance.discoverySavedFetch = instance.fetch.bind(instance); instance.fetch = async request => { if (new URL(request.url).pathname === "/game-authority") throw new Error("test-only Accounts unavailable"); return instance.discoverySavedFetch(request); }; });
  else if (fault === "directory") await runInDurableObject(directory(), (_instance, state) => state.storage.sql.exec("ALTER TABLE directory_game_index RENAME TO discovery_broken_index"));
  else await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("ALTER TABLE game_players RENAME TO discovery_broken_players"));
  try {
    const response = await get("/api/games/current", f.actor.session);
    expect(response.status).toBe(503); expect(Object.keys(await response.json())).toEqual(["error"]);
    if (fault !== "account") expect((await get(`/api/game-codes/${code}`)).status).toBe(503);
  } finally { if (fault === "account") await inOwner(instance => { instance.fetch = instance.discoverySavedFetch; delete instance.discoverySavedFetch; }); }
});

test("revocation during the Directory await denies account fields at the final Game cut", async () => {
  const f = await newGame(); await lobby(f);
  await runInDurableObject(directory(), instance => {
    instance.discoverySavedFetch = instance.fetch.bind(instance); instance.discoveryRoutingReads = 0;
    instance.fetch = async request => {
      const response = await instance.discoverySavedFetch(request.clone());
      if (new URL(request.url).pathname === "/games" && (await request.json()).action === "discovery") {
        instance.discoveryRoutingReads++;
        if (instance.discoveryRoutingReads === 2) await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_sessions SET revoked_at=? WHERE account_id=?", Date.now(), f.actor.accountId));
      }
      return response;
    };
  });
  try { const response = await get("/api/games/current", f.actor.session); expect(response.status).toBe(401); expect(Object.keys(await response.json())).toEqual(["error"]); }
  finally { await runInDurableObject(directory(), instance => { instance.fetch = instance.discoverySavedFetch; delete instance.discoverySavedFetch; }); }
});

test("idle expiry is durably fenced in Workerd and cannot resurrect after clock rollback/reconstruction", async () => {
  const f = await newGame(); const code = await lobby(f);
  const due = Date.now() + 1000; const originalNow = Date.now;
  await runInDurableObject(owner(f.id), (instance, state) => { state.storage.sql.exec("UPDATE game_record SET idle_due=?", due); instance.discoverySavedAlarm = instance.alarm.bind(instance); instance.alarm = async () => new Response("test-only alarm held"); });
  try {
    Date.now = () => due;
    expect((await get(`/api/game-codes/${code}`)).status).toBe(404);
    expect(await value("/api/games/current", f.actor.session)).toEqual({ slot: "releasing", game: null });
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBe(due);
    Date.now = () => due - 1;
    await evictDurableObject(owner(f.id));
    expect((await get(`/api/game-codes/${code}`)).status).toBe(503);
    expect((await get("/api/games/current", f.actor.session)).status).toBe(503);
  } finally { Date.now = originalNow; await runInDurableObject(owner(f.id), instance => { if (instance.discoverySavedAlarm) { instance.alarm = instance.discoverySavedAlarm; delete instance.discoverySavedAlarm; } }); }
});

test.each(["ignored_write", "sync"])("discovery deadline %s failure never returns protected data", async fault => {
  const f = await newGame(); const code = await lobby(f); const due = Date.now() + 1000; const originalNow = Date.now;
  await runInDurableObject(owner(f.id), (instance, state) => {
    state.storage.sql.exec("UPDATE game_record SET idle_due=?", due);
    instance.discoverySavedAlarm = instance.alarm.bind(instance); instance.alarm = async () => new Response("test-only alarm held");
    if (fault === "ignored_write") state.storage.sql.exec("CREATE TRIGGER discovery_expiry_fault BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END");
    else { instance.discoverySavedSync = state.storage.sync.bind(state.storage); state.storage.sync = async () => { if (state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms === due) throw new Error("test-only denial sync fault"); return instance.discoverySavedSync(); }; }
  });
  try { Date.now = () => due; const response = await get(`/api/game-codes/${code}`); expect(response.status).toBe(503); expect(Object.keys(await response.json())).toEqual(["error"]); }
  finally { Date.now = originalNow; await runInDurableObject(owner(f.id), (instance, state) => { instance.alarm = instance.discoverySavedAlarm; if (instance.discoverySavedSync) state.storage.sync = instance.discoverySavedSync; state.storage.sql.exec("DROP TRIGGER IF EXISTS discovery_expiry_fault"); }); }
});

test("terminal state committed during the final Accounts await cannot disclose the hydrated summary", async () => {
  const f = await newGame(); await lobby(f);
  await inOwner(instance => {
    instance.discoverySavedFetch = instance.fetch.bind(instance); instance.discoveryProofs = 0;
    instance.fetch = async request => {
      const response = await instance.discoverySavedFetch(request.clone());
      if (new URL(request.url).pathname === "/game-authority" && (await request.json()).action === "authorize" && ++instance.discoveryProofs === 3) {
        await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("UPDATE game_record SET state='cancelled',ended_at=?,idle_due=NULL,cancellation_reason='operator_cancelled',terminal_actor_kind='account',terminal_actor_id=?,revision=revision+1", Date.now(), f.actor.accountId));
      }
      return response;
    };
  });
  try { expect(await value("/api/games/current", f.actor.session)).toEqual({ slot: "releasing", game: null }); }
  finally { await inOwner(instance => { instance.fetch = instance.discoverySavedFetch; delete instance.discoverySavedFetch; }); }
});

test.each(["account", "player"])("edge hydration rechecks %s revocation before releasing any live fields", async selected => {
  const f = await newGame(); const code = await lobby(f);
  const credential = selected === "player" ? await player(f.id, code, "RevokedDuringRead") : f.actor.session;
  await runInDurableObject(owner(f.id), instance => {
    instance.discoverySavedFetch = instance.fetch.bind(instance); instance.discoveryReads = 0;
    instance.fetch = async request => {
      const response = await instance.discoverySavedFetch(request.clone());
      if (new URL(request.url).pathname === "/discovery" && ++instance.discoveryReads === 1) {
        if (selected === "account") await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_sessions SET revoked_at=? WHERE account_id=?", Date.now(), f.actor.accountId));
        else await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("UPDATE game_sessions SET revoked_at=?", Date.now()));
      }
      return response;
    };
  });
  try { const response = await get(`/api/games/${f.id}/availability?view=${selected}`, credential); expect(response.status).toBe(401); expect(Object.keys(await response.json())).toEqual(["error"]); }
  finally { await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.discoverySavedFetch; delete instance.discoverySavedFetch; }); }
});

test("code expiry first observed at edge release persists denial before clock rollback", async () => {
  const f = await newGame(); const code = await lobby(f); const due = Date.now() + 1000; const originalNow = Date.now;
  await runInDurableObject(owner(f.id), (instance, state) => {
    state.storage.sql.exec("UPDATE game_record SET idle_due=?", due);
    instance.discoverySavedFetch = instance.fetch.bind(instance); instance.discoveryReads = 0;
    instance.discoverySavedAlarm = instance.alarm.bind(instance); instance.alarm = async () => new Response("test-only alarm held");
    instance.fetch = async request => {
      const response = await instance.discoverySavedFetch(request.clone());
      if (new URL(request.url).pathname === "/discovery" && ++instance.discoveryReads === 2) Date.now = () => due;
      return response;
    };
  });
  try {
    expect((await get(`/api/game-codes/${code}`)).status).toBe(404);
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBe(due);
    Date.now = () => due - 1;
    expect((await get(`/api/game-codes/${code}`)).status).toBe(503);
  } finally { Date.now = originalNow; await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.discoverySavedFetch; instance.alarm = instance.discoverySavedAlarm; delete instance.discoverySavedFetch; delete instance.discoverySavedAlarm; }); }
});

test("revocation during the final denial output gate cannot disclose an account slot response", async () => {
  const f = await newGame(); await lobby(f); const due = Date.now() + 1000; const originalNow = Date.now;
  await runInDurableObject(owner(f.id), (instance, state) => {
    state.storage.sql.exec("UPDATE game_record SET idle_due=?", due);
    instance.discoverySavedSync = state.storage.sync.bind(state.storage);
    instance.discoverySavedAlarm = instance.alarm.bind(instance); instance.alarm = async () => new Response("test-only alarm held");
    state.storage.sync = async () => {
      await instance.discoverySavedSync();
      if (state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms === due) await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_sessions SET revoked_at=? WHERE account_id=?", due, f.actor.accountId));
    };
  });
  await inOwner(instance => {
    instance.discoverySavedFetch = instance.fetch.bind(instance); instance.discoveryProofs = 0;
    instance.fetch = async request => {
      const response = await instance.discoverySavedFetch(request.clone());
      if (new URL(request.url).pathname === "/game-authority" && (await request.json()).action === "authorize" && ++instance.discoveryProofs === 3) Date.now = () => due;
      return response;
    };
  });
  try { const response = await get("/api/games/current", f.actor.session); expect(response.status).toBe(401); expect(Object.keys(await response.json())).toEqual(["error"]); }
  finally {
    Date.now = originalNow;
    await inOwner(instance => { instance.fetch = instance.discoverySavedFetch; delete instance.discoverySavedFetch; });
    await runInDurableObject(owner(f.id), (instance, state) => { state.storage.sync = instance.discoverySavedSync; instance.alarm = instance.discoverySavedAlarm; delete instance.discoverySavedSync; delete instance.discoverySavedAlarm; });
  }
});

test("unknown code reads leave the genuine GAMES namespace object inventory unchanged", async () => {
  const before = (await listDurableObjectIds(env.GAMES)).map(id => id.toString()).sort();
  expect((await get("/api/game-codes/ZZZZZZZZ")).status).toBe(404);
  expect((await get("/api/games/01900000-0000-7000-8000-000000000099/availability?view=entry&game_code=ZZZZZZZZ")).status).toBe(404);
  expect((await listDurableObjectIds(env.GAMES)).map(id => id.toString()).sort()).toEqual(before);
});

test("selected player expiry is fenced in actual Workerd storage and does not resurrect on rollback", async () => {
  const f = await newGame(); const code = await lobby(f); const session = await player(f.id, code, "ExpiryPlayer");
  const due = Date.now() + 1000; const originalNow = Date.now;
  await runInDurableObject(owner(f.id), (instance, state) => {
    state.storage.sql.exec("UPDATE game_sessions SET expires_at=?", due);
    instance.discoverySavedAlarm = instance.alarm.bind(instance); instance.alarm = async () => new Response("test-only alarm held");
  });
  try {
    Date.now = () => due;
    expect((await get(`/api/games/${f.id}/availability?view=player`, session)).status).toBe(401);
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBe(due);
    Date.now = () => due - 1;
    expect((await get(`/api/games/${f.id}/availability?view=player`, session)).status).toBe(503);
  } finally { Date.now = originalNow; await runInDurableObject(owner(f.id), instance => { instance.alarm = instance.discoverySavedAlarm; delete instance.discoverySavedAlarm; }); }
});

test("an unreadable socket registry attachment fails availability unavailable without presence writes", async () => {
  const f = await newGame(); const code = await lobby(f); const session = await player(f.id, code, "RegistryPlayer");
  const socket = await stream(f.id, session); const before = await rows(f.id);
  const saved = await runInDurableObject(owner(f.id), (_instance, state) => {
    const socket = state.getWebSockets()[0]; const attachment = socket.deserializeAttachment();
    socket.serializeAttachment("invalid-attachment-fixture"); return attachment;
  });
  try {
    const response = await get(`/api/games/${f.id}/availability?view=account`, f.actor.session);
    expect(response.status).toBe(503); expect(Object.keys(await response.json())).toEqual(["error"]);
    expect(await rows(f.id)).toEqual(before);
  } finally { await runInDurableObject(owner(f.id), (_instance, state) => state.getWebSockets()[0].serializeAttachment(saved)); socket.close(1000); }
});
