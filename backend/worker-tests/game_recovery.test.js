import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runDurableObjectAlarm, runInDurableObject } from "cloudflare:test";
import { commandId, enroll, inOwner, origin, post, resetStorage } from "./fixtures.js";

const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
async function directoryRequest(message) {
  const response = await directory().fetch("https://directory.internal/games", {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(message),
  });
  expect(response.status).toBe(200);
  return (await response.json()).outcome;
}
beforeEach(async () => {
  await reset();
  await resetStorage();
  await runInDurableObject(directory(), (_instance, state) => state.storage.deleteAll());
});

test("Directory alarm retries genuine absent known initializer without inventing Game state", async () => {
  const claimed = await directoryRequest({ action: "claim", account_id: commandId(), command_id: commandId(), fingerprint: Array(32).fill(9) });
  expect(claimed.result).toBe("work");
  const work = claimed.work;
  const priorAttempts = await runInDurableObject(directory(), async (_instance, state) => {
    const prior = state.storage.sql.exec("SELECT attempts FROM directory_game_creations WHERE game_id=?", work.game_id).one().attempts;
    state.storage.sql.exec("UPDATE directory_game_creations SET next_retry_at=? WHERE game_id=?", Date.now(), work.game_id);
    await state.storage.setAlarm(Date.now() + 60_000);
    return prior;
  });
  expect(await runDurableObjectAlarm(directory())).toBe(true);
  await runInDurableObject(directory(), async (_instance, state) => {
    const current = state.storage.sql.exec("SELECT attempts,next_retry_at,ready_revision,deadline FROM directory_game_creations WHERE game_id=?", work.game_id).one();
    expect(current.attempts).toBe(priorAttempts + 1);
    expect(current.next_retry_at).toBeGreaterThan(Date.now());
    expect(current.ready_revision).toBeNull();
    expect(current.deadline).toBe(work.deadline);
    expect(state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id).toBe(work.game_id);
    expect(await state.storage.getAlarm()).toBeGreaterThan(Date.now());
  });
  const game = env.GAMES.get(env.GAMES.idFromName(work.game_id));
  await runInDurableObject(game, (_instance, state) => {
    const tables = [...state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name='game_record'")];
    if (tables.length) expect([...state.storage.sql.exec("SELECT * FROM game_record")]).toEqual([]);
  });
});

async function users(path, actor, method = "GET") {
  const headers = { Cookie: actor.session };
  if (method !== "GET") { headers.Origin = origin; headers["Idempotency-Key"] = commandId(); }
  return exports.default.fetch(`${origin}/api/users${path}`, { method, headers });
}
async function genuineCloseBacklog() {
  const actor = await enroll();
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", actor.accountId));
  const target = await exports.default.fetch(`${origin}/api/users/hosts`, {
    method: "POST", headers: { Cookie: actor.session, Origin: origin, "Idempotency-Key": commandId(), "Content-Type": "application/json" },
    body: JSON.stringify({ username: "CloseRecoveryHost" }),
  });
  expect(target.status).toBe(200);
  const created = await target.json();
  // A genuine normal session already exists for the actor; the management mutation
  // invalidates it and queues actual close work, with no manufactured peer ACK.
  const gameId = commandId(), connectionId = commandId();
  await inOwner((_instance, state) => {
    const session = state.storage.sql.exec("SELECT session_id,credential_epoch,expires_at FROM account_sessions WHERE account_id=? AND scope='normal' AND revoked_at IS NULL", actor.accountId).one();
    state.storage.sql.exec("INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) VALUES(?,?,?,?,?,?)", actor.accountId, session.session_id, gameId, connectionId, session.credential_epoch, session.expires_at);
  });
  const reset = await users(`/${actor.accountId}/password-reset-links`, actor, "POST");
  expect(reset.status).toBe(401); // Self-reset commits but must withhold fresh Admin output.
  const other = await exports.default.fetch(`${origin}/_dev/commands`, {
    method: "POST", headers: { Authorization: ["Bearer", env.DEV_CLI_KEY].join(" "), "Content-Type": "application/json", "Idempotency-Key": commandId() },
    body: JSON.stringify({ operation: "enable_account", account_id: created.account.account_id }),
  });
  expect(other.status).toBe(200);
  await inOwner((_instance, state) => {
    const work = state.storage.sql.exec("SELECT attempt_count FROM account_socket_close_work WHERE connection_id=?", connectionId).one();
    expect(work.attempt_count).toBe(0);
    state.storage.sql.exec("UPDATE account_socket_close_work SET next_attempt_at=? WHERE connection_id=?", Date.now() + 60_000, connectionId);
  });
  return { actor, connectionId };
}

test.each(["Users", "CLI"])("ordinary %s commands dispatch genuine due socket-close work", async transport => {
  const { actor, connectionId } = await genuineCloseBacklog();
  let response;
  if (transport === "Users") {
    // A second independent Admin token, never a Developer principal in browser JSON.
    await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET username=? WHERE account_id=?", "CloseRecoveryAdmin", actor.accountId));
    const admin = await enroll();
    await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", admin.accountId));
    await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_socket_close_work SET next_attempt_at=? WHERE connection_id=?", Date.now(), connectionId));
    response = await users("", admin);
  } else {
    await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_socket_close_work SET next_attempt_at=? WHERE connection_id=?", Date.now(), connectionId));
    response = await exports.default.fetch(`${origin}/_dev/commands`, {
      method: "POST", headers: { Authorization: ["Bearer", env.DEV_CLI_KEY].join(" "), "Content-Type": "application/json" },
      body: JSON.stringify({ operation: "list_accounts", limit: 50, after: null }),
    });
  }
  expect(response.status).toBe(200);
  await inOwner((_instance, state) => {
    const retained = [...state.storage.sql.exec("SELECT attempt_count FROM account_socket_close_work WHERE connection_id=?", connectionId)];
    expect(retained.length === 0 || retained[0].attempt_count > 0).toBe(true);
  });
});
async function withDirectoryFault(action, operation) {
  await runInDurableObject(directory(), instance => {
    instance.recoveryOriginalFetch = instance.fetch.bind(instance);
    instance.recoveryOriginalAlarm = instance.alarm.bind(instance);
    // Also withhold the automatic recovery delivery until the outage is removed.
    instance.alarm = async () => new Response("test-only alarm delivery outage", { status: 503 });
    instance.recoveryBlockedAction = action;
    instance.fetch = async request => {
      if (new URL(request.url).pathname === "/games" && (await request.clone().json()).action === instance.recoveryBlockedAction) {
        return new Response("test-only Directory delivery outage", { status: 503 });
      }
      return instance.recoveryOriginalFetch(request);
    };
  });
  try { return await operation(); }
  finally {
    await runInDurableObject(directory(), async (instance, state) => {
      instance.fetch = instance.recoveryOriginalFetch;
      instance.alarm = instance.recoveryOriginalAlarm;
      delete instance.recoveryOriginalFetch;
      delete instance.recoveryOriginalAlarm;
      delete instance.recoveryBlockedAction;
      await state.storage.setAlarm(Date.now() + 60_000);
    });
  }
}

test("Directory alarm acknowledges genuine initialized New Game after withheld initializer delivery", async () => {
  const actor = await enroll();
  await withDirectoryFault("acknowledge", async () => {
    const response = await post("/api/games", {}, { session: actor.session });
    expect(response.status).toBe(503);
  });
  const gameId = await runInDurableObject(directory(), (_instance, state) => {
    const work = state.storage.sql.exec("SELECT game_id,ready_revision FROM directory_game_creations").one();
    expect(work.ready_revision).toBeNull();
    state.storage.sql.exec("UPDATE directory_game_creations SET next_retry_at=? WHERE game_id=?", Date.now(), work.game_id);
    return work.game_id;
  });
  const owner = env.GAMES.get(env.GAMES.idFromName(gameId));
  await runInDurableObject(owner, (_instance, state) => {
    expect(state.storage.sql.exec("SELECT state,game_code FROM game_record").one()).toEqual({ state: "new", game_code: null });
  });
  expect(await runDurableObjectAlarm(directory())).toBe(true);
  await runInDurableObject(directory(), (_instance, state) => {
    const current = state.storage.sql.exec("SELECT ready_revision,next_retry_at FROM directory_game_creations WHERE game_id=?", gameId).one();
    expect(current).toEqual({ ready_revision: 0, next_retry_at: null });
  });
});

test("Directory alarm publishes only a genuine committed lobby after withheld publication delivery", async () => {
  const actor = await enroll();
  const created = await post("/api/games", {}, { session: actor.session });
  expect(created.status).toBe(200);
  const gameId = (await created.json()).game.game_id;
  await withDirectoryFault("publish", async () => {
    const response = await post(`/api/games/${gameId}/lobby`, { expected_revision: 0 }, { session: actor.session });
    expect(response.status).toBe(202);
    expect(await response.json()).toEqual({ result: "pending", operation_id: expect.any(String) });
    expect(response.headers.has("Set-Cookie")).toBe(false);
    await runInDurableObject(directory(), (_instance, state) => {
      expect(state.storage.sql.exec("SELECT publication_state FROM directory_game_index WHERE game_id=?", gameId).one().publication_state).toBe("pending");
    });
  });
  const owner = env.GAMES.get(env.GAMES.idFromName(gameId));
  const code = await runInDurableObject(owner, (_instance, state) => {
    const row = state.storage.sql.exec("SELECT state,game_code FROM game_record").one();
    expect(row.state).toBe("awaiting_players");
    expect([...state.storage.sql.exec("SELECT phase FROM game_pending_work")]).not.toHaveLength(0);
    return row.game_code;
  });
  await runInDurableObject(directory(), (_instance, state) => {
    state.storage.sql.exec("UPDATE directory_game_creations SET next_retry_at=? WHERE game_id=?", Date.now(), gameId);
  });
  expect(await runDurableObjectAlarm(directory())).toBe(true);
  expect((await directoryRequest({ action: "lookup_code", game_code: code })).game_id).toBe(gameId);
  // Directory publication does not manufacture the Game owner's ACK barrier.
  await runInDurableObject(owner, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT phase FROM game_pending_work")]).not.toHaveLength(0);
    state.storage.sql.exec("UPDATE game_pending_work SET next_attempt_at=?", Date.now());
  });
  expect(await runDurableObjectAlarm(owner)).toBe(true);
  await runInDurableObject(owner, (_instance, state) => expect([...state.storage.sql.exec("SELECT phase FROM game_pending_work")]).toHaveLength(0));
});

test("allocated hidden Directory code is released by actual New Game timeout without inventing Game code", async () => {
  const actor = await enroll();
  const created = await post("/api/games", {}, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, (_instance, state) => {
    state.storage.sql.exec("CREATE TRIGGER interrupt_lobby BEFORE UPDATE OF state ON game_record WHEN NEW.state='awaiting_players' BEGIN SELECT RAISE(ABORT,'commit interrupted after code allocation'); END");
  });
  const interrupted = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(interrupted.status).toBe(503);
  const allocated = await runInDurableObject(directory(), (_instance, state) => {
    const row = state.storage.sql.exec("SELECT game_code,state,source_revision,publication_state FROM directory_game_index WHERE game_id=?", id).one();
    expect(row).toMatchObject({ state: "new", source_revision: 0, publication_state: "pending" });
    expect(row.game_code).toMatch(/^[A-Z0-9]{8}$/);
    expect(state.storage.sql.exec("SELECT ready_revision FROM directory_game_creations WHERE game_id=?", id).one().ready_revision).toBe(0);
    return row.game_code;
  });
  await runInDurableObject(owner, async (instance, state) => {
    expect(state.storage.sql.exec("SELECT state,game_code,started_at FROM game_record").one()).toEqual({ state: "new", game_code: null, started_at: null });
    state.storage.sql.exec("DROP TRIGGER interrupt_lobby");
    state.storage.sql.exec("UPDATE game_record SET idle_due=?", Date.now() - 1);
    await instance.alarm();
    expect(state.storage.sql.exec("SELECT state,game_code,started_at FROM game_record").one()).toEqual({ state: "cancelled", game_code: null, started_at: null });
    expect([...state.storage.sql.exec("SELECT * FROM game_configuration")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT operation_id FROM game_pending_work")]).toEqual([]);
  });
  await runInDurableObject(directory(), (_instance, state) => {
    expect(state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id).toBeNull();
    expect([...state.storage.sql.exec("SELECT game_id FROM directory_game_creations WHERE game_id=?", id)]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT game_id FROM directory_game_index WHERE game_id=? OR game_code=?", id, allocated)]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT game_id FROM directory_hosted_nonterminal_games WHERE game_id=?", id)]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT account_id FROM directory_retired_creations WHERE account_id=?", actor.accountId)]).toEqual([{ account_id: actor.accountId }]);
  });
});

test("Directory alarm seals only actual absent known creation at its deadline", async () => {
  const claimed = await directoryRequest({ action: "claim", account_id: commandId(), command_id: commandId(), fingerprint: Array(32).fill(9) });
  expect(claimed.result).toBe("work");
  const work = claimed.work;
  // Internal SQL clock fixture: preserve complete known identity and original lifetime.
  await runInDurableObject(directory(), (_instance, state) => {
    const created = Date.now() - 86_400_001;
    const hex = created.toString(16).padStart(12, "0");
    const oldCommand = `${hex.slice(0,8)}-${hex.slice(8)}${work.command_id.slice(13)}`;
    state.storage.sql.exec("UPDATE directory_game_creations SET command_id=?,created_at=?,deadline=?,next_retry_at=? WHERE game_id=?", oldCommand, created, created + 86_400_000, created, work.game_id);
    state.storage.sql.exec("UPDATE directory_game_index SET created_at=? WHERE game_id=?", created, work.game_id);
  });
  expect(await runDurableObjectAlarm(directory())).toBe(true);
  await runInDurableObject(directory(), (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT game_id FROM directory_game_creations")]).toHaveLength(0);
    expect(state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id).toBeNull();
  });
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(work.game_id)), (_instance, state) => {
    expect(state.storage.sql.exec("SELECT state FROM game_record").one().state).toBe("cancelled");
    expect([...state.storage.sql.exec("SELECT * FROM game_configuration")]).toHaveLength(0);
  });
});
