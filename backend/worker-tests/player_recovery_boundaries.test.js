import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject, evictDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId, password } from "./fixtures.js";

const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async () => {
  await reset(); await resetStorage();
  await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll());
});
async function fixture() {
  const actor = await enroll("RecoveryBoundaryHost");
  const response = await post("/api/games", { configuration: {
    numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: 3, spectator_capacity: 0,
  } }, { session: actor.session });
  expect(response.status).toBe(200);
  const id = (await response.json()).game.game_id;
  const opened = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(opened.status).toBe(200);
  const code = (await opened.json()).game_code;
  const answer = password();
  const players = [];
  for (const [alias, proof] of [["ExactPlayer", answer], ["exactplayer", password()]]) {
    const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
    expect(context.status).toBe(200);
    const joined = await post(`/api/games/${id}/players`, { game_code: code, alias, recovery_answer: proof }, { session: cookie(context) });
    expect(joined.status).toBe(200);
    players.push({ alias, answer: proof, session: cookie(joined), body: await joined.json() });
  }
  return { id, code, actor, players };
}
function recover(f, answer = f.players[0].answer, options = {}) {
  return post("/api/player-recovery", { game_code: f.code, alias: f.players[0].alias, recovery_answer: answer }, options);
}
async function changeAnswer(f, method, answer, session = f.players[0].session, id = commandId()) {
  return exports.default.fetch(`${origin}/api/games/${f.id}/membership/recovery-answer`, {
    method, headers: { Origin: origin, Cookie: session, "Idempotency-Key": id, "Content-Type": "application/json" },
    body: JSON.stringify(method === "PUT" ? { recovery_answer: answer } : {}),
  });
}
async function sync(f, session, view = "player") {
  return exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=${view}`, { headers: { Cookie: session } });
}
async function rows(f) {
  return runInDurableObject(owner(f.id), (_instance, state) => ({
    players: [...state.storage.sql.exec("SELECT * FROM game_players ORDER BY player_id")],
    sessions: [...state.storage.sql.exec("SELECT * FROM game_sessions ORDER BY session_id")],
    boardCells: [...state.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")],
    game: [...state.storage.sql.exec("SELECT game_id,state,idle_due,last_host_activity,started_at FROM game_record")],
  }));
}
function expectSafe(response) {
  expect(response.headers.get("cache-control")).toBe("no-store");
  expect(response.headers.get("referrer-policy")).toBe("no-referrer");
  expect(response.headers.get("x-content-type-options")).toBe("nosniff");
}

test("public recovery needs answer proof even with host or existing player credentials and never case-folds the alias", async () => {
  const f = await fixture();
  const wrong = password();
  const before = await rows(f);
  for (const session of [undefined, f.actor.session, f.players[0].session]) {
    const denied = await recover(f, wrong, { session });
    expect(denied.status).toBe(401);
    expect(denied.headers.get("set-cookie")).toBeNull();
    expect(Object.keys(await denied.json())).toEqual(["error"]);
    expectSafe(denied);
  }
  const caseMismatch = await post("/api/player-recovery", { game_code: f.code, alias: f.players[1].alias, recovery_answer: f.players[0].answer });
  expect(caseMismatch.status).toBe(401);
  expect(await rows(f)).toEqual(before);
});

test("normalized code and proof restore one identity; receipt retries still require fresh proof and never replay a cookie", async () => {
  const f = await fixture();
  const before = await rows(f);
  const id = commandId();
  const input = { game_code: ` ${f.code.toLowerCase()} `, alias: ` ${f.players[0].alias} `, recovery_answer: `\u2003${f.players[0].answer.toUpperCase()}\u00a0` };
  const first = await post("/api/player-recovery", input, { id });
  expect(first.status).toBe(200); expectSafe(first);
  const firstBody = await first.json();
  expect(first.headers.get("set-cookie")).toContain(`Path=/api/games/${f.id}`);
  expect(first.headers.get("set-cookie")).toContain("HttpOnly");
  expect(first.headers.get("set-cookie")).toContain("Secure");
  expect(first.headers.get("set-cookie")).toContain("SameSite=Strict");
  const restored = cookie(first);
  const own = await sync(f, restored);
  expect(own.status).toBe(200);
  expect((await own.json()).snapshot.player_id).toBe(f.players[0].body.player_id);
  expect((await sync(f, f.players[0].session)).status).toBe(401);
  expect((await sync(f, f.players[1].session)).status).toBe(200);
  const after = await rows(f);
  expect(after.players).toHaveLength(before.players.length);
  expect(after.game).toEqual(before.game);
  expect(after.boardCells).toEqual(before.boardCells);
  const ownPlayer = after.players.find(p => p.player_id === f.players[0].body.player_id);
  expect(ownPlayer.session_epoch).toBe(before.players.find(p => p.player_id === ownPlayer.player_id).session_epoch + 1);
  const badReplay = await post("/api/player-recovery", { ...input, recovery_answer: password() }, { id });
  expect(badReplay.status).toBe(401); expect(badReplay.headers.get("set-cookie")).toBeNull();
  const goodReplay = await post("/api/player-recovery", input, { id });
  expect(goodReplay.status).toBe(200); expect(goodReplay.headers.get("set-cookie")).toBeNull();
  expect(await goodReplay.json()).toEqual({ receipt: firstBody.receipt });
  const next = await recover(f);
  expect(next.status).toBe(200);
  expect((await sync(f, restored)).status).toBe(401);
  const final = await rows(f);
  expect(final.players.find(p => p.player_id === ownPlayer.player_id).session_epoch).toBe(ownPlayer.session_epoch + 1);
  expect(final.boardCells).toEqual(before.boardCells);
});

test("private answer PUT and DELETE preserve current sessions and remain invisible to other viewers", async () => {
  const f = await fixture();
  const before = await rows(f);
  const beforeHost = await (await sync(f, f.actor.session, "account")).json();
  const beforeOther = await (await sync(f, f.players[1].session)).json();
  const denied = await changeAnswer(f, "PUT", password(), f.actor.session);
  expect(denied.status).toBe(401);
  const replacement = password();
  const id = commandId();
  const changed = await changeAnswer(f, "PUT", replacement, f.players[0].session, id);
  expect(changed.status).toBe(200); expect(changed.headers.get("set-cookie")).toBeNull(); expectSafe(changed);
  const changedBody = await changed.json();
  expect(changedBody.recovery_enabled).toBe(true);
  const replay = await changeAnswer(f, "PUT", replacement, f.players[0].session, id);
  expect(replay.status).toBe(200); expect(replay.headers.get("set-cookie")).toBeNull();
  expect(await replay.json()).toEqual({ result: "committed", receipt: changedBody.receipt });
  expect(await rows(f)).toEqual(before);
  expect(await (await sync(f, f.actor.session, "account")).json()).toEqual(beforeHost);
  expect(await (await sync(f, f.players[1].session)).json()).toEqual(beforeOther);
  expect((await recover(f)).status).toBe(401);
  const deleted = await changeAnswer(f, "DELETE");
  expect(deleted.status).toBe(200); expect(deleted.headers.get("set-cookie")).toBeNull();
  expect((await deleted.json()).recovery_enabled).toBe(false);
  expect((await recover(f, replacement)).status).toBe(401);
  expect(await rows(f)).toEqual(before);
  expect((await sync(f, f.players[0].session)).status).toBe(200);
});

test("a committed recovery whose owner response is lost cannot resend its cookie; fresh proof and command recover again", async () => {
  const f = await fixture();
  const id = commandId();
  await runInDurableObject(owner(f.id), instance => {
    instance.lostRecoveryFetch = instance.fetch.bind(instance);
    instance.lostRecovery = false;
    instance.fetch = async request => {
      const result = await instance.lostRecoveryFetch(request.clone());
      const operation = new URL(request.url).pathname === "/command" ? (await request.clone().json()).operation : null;
      if (operation === "recover_player" && result.status === 200 && !instance.lostRecovery) {
        instance.lostRecovery = true;
        throw new Error("test-only lost recovery owner response");
      }
      return result;
    };
  });
  try {
    const lost = await recover(f, f.players[0].answer, { id });
    expect(lost.status).toBe(503); expect(lost.headers.get("set-cookie")).toBeNull();
    expect((await sync(f, f.players[0].session)).status).toBe(401);
    const retry = await recover(f, f.players[0].answer, { id });
    expect(retry.status).toBe(200); expect(retry.headers.get("set-cookie")).toBeNull();
    expect(Object.keys(await retry.json())).toEqual(["receipt"]);
    const fresh = await recover(f);
    expect(fresh.status).toBe(200);
    expect((await sync(f, cookie(fresh))).status).toBe(200);
  } finally {
    await runInDurableObject(owner(f.id), instance => {
      instance.fetch = instance.lostRecoveryFetch; delete instance.lostRecoveryFetch; delete instance.lostRecovery;
    });
  }
});

test("unknown code and unavailable enrollment fail with the same safe recovery result without modifying the reservation", async () => {
  const f = await fixture();
  const before = await runInDurableObject(directory(), (_instance, state) => ({
    reservation: [...state.storage.sql.exec("SELECT * FROM directory_global_reservation")],
    index: [...state.storage.sql.exec("SELECT * FROM directory_game_index")],
  }));
  const first = await post("/api/player-recovery", { game_code: "ZZZZZZZZ", alias: f.players[0].alias, recovery_answer: f.players[0].answer });
  expect(first.status).toBe(401); const body = await first.json();
  const deleted = await changeAnswer(f, "DELETE"); expect(deleted.status).toBe(200);
  const second = await recover(f); expect(second.status).toBe(401); expect(await second.json()).toEqual(body);
  const third = await post("/api/player-recovery", { game_code: f.code, alias: "MissingPlayer", recovery_answer: f.players[0].answer });
  expect(third.status).toBe(401); expect(await third.json()).toEqual(body);
  expect(await runInDurableObject(directory(), (_instance, state) => ({
    reservation: [...state.storage.sql.exec("SELECT * FROM directory_global_reservation")],
    index: [...state.storage.sql.exec("SELECT * FROM directory_game_index")],
  }))).toEqual(before);
});

test("owner-side supersession before reply release prevents a stale recovered credential reaching the browser", async () => {
  const f = await fixture();
  await runInDurableObject(owner(f.id), (instance, state) => {
    instance.cutRecoveryFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const result = await instance.cutRecoveryFetch(request.clone());
      const operation = new URL(request.url).pathname === "/command" ? (await request.clone().json()).operation : null;
      if (operation === "recover_player" && result.status === 200) {
        state.storage.sql.exec("UPDATE game_players SET session_epoch=session_epoch+1 WHERE player_id=?", f.players[0].body.player_id);
      }
      return result;
    };
  });
  try {
    const denied = await recover(f);
    expect(denied.status).toBe(401); expect(denied.headers.get("set-cookie")).toBeNull();
    expect(Object.keys(await denied.json())).toEqual(["error"]);
  } finally {
    await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.cutRecoveryFetch; delete instance.cutRecoveryFetch; });
  }
});

test("five failed recovery attempts block without extending the block; restart retains it and expiry removes it", async () => {
  const f = await fixture();
  const wrong = password();
  for (let i = 0; i < 5; i++) expect((await recover(f, wrong)).status).toBe(401);
  const readBuckets = () => runInDurableObject(owner(f.id), (_instance, state) => {
    const tables = [...state.storage.sql.exec("SELECT name FROM sqlite_schema WHERE type='table'")].filter(t => t.name === "game_recovery_buckets");
    expect(tables).toHaveLength(1);
    return [...state.storage.sql.exec(`SELECT * FROM ${tables[0].name} ORDER BY subject_key`)];
  });
  const before = await readBuckets(); expect(before.length).toBeGreaterThanOrEqual(2);
  expect(before.every(r => r.attempt_count === 5)).toBe(true);
  const at = Math.max(...before.map(r => r.blocked_until));
  const originalNow = Date.now;
  try {
    Date.now = () => at - 1;
    await evictDurableObject(owner(f.id));
    const blocked = await recover(f); expect(blocked.status).toBe(429); expect(blocked.headers.get("set-cookie")).toBeNull();
    expect(await readBuckets()).toEqual(before);
    Date.now = () => at;
    const restored = await recover(f); expect(restored.status).toBe(200);
    expect(await readBuckets()).toEqual([]);
  } finally { Date.now = originalNow; }
});
