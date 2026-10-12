import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject, evictDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId, password, stub } from "./fixtures.js";

const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async () => {
  await reset(); await resetStorage();
  await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll());
});
async function fixture(players = false) {
  const host = await enroll("GameStorageAuditHost");
  const created = await post("/api/games", { configuration: {
    numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: 2, spectator_capacity: 0,
  } }, { session: host.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  if (!players) return { host, id };
  const opened = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: host.session });
  expect(opened.status).toBe(200);
  const code = (await opened.json()).game_code;
  const members = [];
  for (const alias of ["First", "Second"]) {
    const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
    expect(context.status).toBe(200);
    const answer = password();
    const joined = await post(`/api/games/${id}/players`, { game_code: code, alias, recovery_answer: answer }, { session: cookie(context) });
    expect(joined.status).toBe(200);
    members.push({ alias, answer, session: cookie(joined), body: await joined.json() });
  }
  return { host, id, code, members };
}
const sync = (f, session, view = "player") => exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=${view}`, { headers: { Cookie: session } });
const recover = f => post("/api/player-recovery", { game_code: f.code, alias: f.members[0].alias, recovery_answer: f.members[0].answer });
async function stream(f, session) {
  const response = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`, {
    headers: { Cookie: session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" },
  });
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  const initial = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("audit snapshot timeout")), 3000);
    socket.addEventListener("message", event => { clearTimeout(timeout); resolve(JSON.parse(event.data)); }, { once: true });
  });
  socket.addEventListener("message", event => {
    const frame = JSON.parse(event.data);
    socket.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision }));
  });
  socket.accept();
  return { socket, initial: await initial };
}

for (const fault of ["rollback_after_denial", "ignored_write", "failed_sync"]) {
  test(`final gameplay expiry denial is durably fenced: ${fault}`, async () => {
    const f = await fixture();
    const realNow = Date.now;
    let deadline; let calls = 0; let syncs = 0;
    await runInDurableObject(stub(), (instance, state) => {
      deadline = state.storage.sql.exec("SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL").one().expires_at;
      instance.auditFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        const result = await instance.auditFetch(request);
        if (new URL(request.url).pathname === "/game-authority" && ++calls === 2) {
          let samples = 0;
          // Receiving-time proof validation succeeds; the following Game final cut observes expiry.
          Date.now = () => ++samples === 1 ? deadline - 1 : deadline;
        }
        return result;
      };
    });
    await runInDurableObject(owner(f.id), (instance, state) => {
      instance.auditSync = state.storage.sync.bind(state.storage);
      if (fault === "ignored_write") state.storage.sql.exec(`CREATE TRIGGER audit_fence BEFORE UPDATE OF last_observed_ms ON game_metadata WHEN NEW.last_observed_ms>=${deadline} BEGIN SELECT RAISE(IGNORE); END`);
      state.storage.sync = async () => {
        if (Date.now() === deadline) {
          syncs++;
          Date.now = () => deadline - 1; // Durability await must never resample the denial or revive output.
          if (fault === "failed_sync") throw new Error("test-only denial durability failure");
        }
        await instance.auditSync();
      };
    });
    try {
      const denied = await post(`/api/games/${f.id}/cancel`, { expected_state: "new", confirmed: true }, { session: f.host.session });
      const body = await denied.json();
      expect(denied.status).toBe(fault === "rollback_after_denial" ? 401 : 503);
      expect(body.receipt).toBeUndefined(); expect(body.game).toBeUndefined();
      const floor = await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);
      if (fault !== "ignored_write") expect(floor).toBe(deadline);
      if (fault === "rollback_after_denial") { expect(syncs).toBe(1); expect((await sync(f, f.host.session, "account")).status).toBe(503); }
    } finally {
      Date.now = realNow;
      await runInDurableObject(stub(), instance => { instance.fetch = instance.auditFetch; delete instance.auditFetch; });
      await runInDurableObject(owner(f.id), (instance, state) => {
        state.storage.sync = instance.auditSync;
        state.storage.sql.exec("DROP TRIGGER IF EXISTS audit_fence");
        delete instance.auditSync;
      });
    }
  });
}

test("accepted account anchors stop at the shared grant bound and cannot deadlock ending", async () => {
  const f = await fixture(true);
  const sockets = [await stream(f, f.members[0].session), await stream(f, f.members[1].session)];
  try {
    const host = await (await sync(f, f.host.session, "account")).json();
    expect((await post(`/api/games/${f.id}/start`, { expected_revision: host.view_revision }, { session: f.host.session })).status).toBe(200);
    const sessions = [];
    for (let n = 0; n < 200; n++) {
      const raw = crypto.getRandomValues(new Uint8Array(32));
      const token = btoa(String.fromCharCode(...raw)).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
      const digest = await crypto.subtle.digest("SHA-256", raw);
      const session = commandId();
      await runInDurableObject(stub(), (_instance, state) => {
        state.storage.sql.exec("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at,revoked_at) SELECT ?,account_id,?,scope,credential_epoch,issued_at,expires_at,NULL FROM account_sessions WHERE account_id=? AND revoked_at IS NULL LIMIT 1", session, digest, f.host.accountId);
      });
      const credential = `${f.host.session.split("=", 1)[0]}=${token}`;
      expect((await sync(f, credential, "account")).status).toBe(n === 199 ? 409 : 200);
      if (n < 199) sessions.push(credential);
    }
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT count(*) AS count FROM game_connection_grants").one().count)).toBe(200);
    expect((await sync(f, sessions[0], "account")).status).toBe(200);
    const ended = await post(`/api/games/${f.id}/cancel`, { expected_state: "in_progress", confirmed: true }, { session: f.host.session });
    expect(ended.status).toBe(200); expect((await ended.json()).history_available).toBe(true);
    expect((await sync(f, sessions[198], "account")).status).toBe(200);
    await runInDurableObject(directory(), (_instance, state) => { expect(state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id).toBeNull(); });
  } finally { for (const connection of sockets) connection.socket.close(1000); }
});

test("Recovery captures native accepted preparation after owner acceptance and immediate close fail; retries respect deadlines", async () => {
  const f = await fixture(true); const realNow = Date.now;
  let successor; let predecessor; let calls = 0;
  await runInDurableObject(owner(f.id), (instance, state) => {
    instance.auditAccept = state.acceptWebSocket;
    state.acceptWebSocket = function (ws, tags) {
      instance.auditAccept.call(this, ws, tags);
      ws.auditClose = ws.close;
      ws.close = () => { calls++; throw new Error("test-only accepted orphan close failure"); };
    };
    state.storage.sql.exec(`CREATE TRIGGER audit_accept BEFORE INSERT ON game_current_connections WHEN NEW.principal_id='${f.members[0].body.player_id}' BEGIN SELECT RAISE(IGNORE); END`);
  });
  try {
    const failed = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`, { headers: { Cookie: f.members[0].session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
    expect(failed.status).toBe(503);
    await runInDurableObject(owner(f.id), (instance, state) => {
      state.acceptWebSocket = instance.auditAccept; delete instance.auditAccept;
      state.storage.sql.exec("DROP TRIGGER audit_accept");
      predecessor = state.storage.sql.exec("SELECT connection_id,session_id,principal_id,epoch FROM game_connection_grants WHERE principal_id=?", f.members[0].body.player_id).one();
      expect(state.getWebSockets().filter(ws => !!ws.auditClose).map(ws => JSON.parse(ws.deserializeAttachment()).connection_id)).toEqual([predecessor.connection_id]);
    });
    const recovered = await recover(f);
    expect(recovered.status).toBe(200);
    const fresh = cookie(recovered);
    let close = await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.connection_id).one());
    expect(close).toMatchObject({ session_id: predecessor.session_id, player_id: predecessor.principal_id, epoch: predecessor.epoch, attempt_count: 1 });
    const firstAttempts = calls;
    Date.now = () => close.next_attempt_at - 1;
    // Real unrelated activity and an early alarm must not close the known fenced predecessor.
    successor = await stream(f, fresh);
    expect(successor.initial.view.player_id).toBe(f.members[0].body.player_id);
    await runInDurableObject(owner(f.id), instance => instance.alarm());
    expect(calls).toBe(firstAttempts);
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.connection_id).one())).toEqual(close);
    Date.now = () => close.next_attempt_at;
    await runInDurableObject(owner(f.id), instance => instance.alarm());
    expect(calls).toBe(firstAttempts + 1);
    const retry = await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.connection_id).one());
    expect(retry.attempt_count).toBe(2); expect(retry.next_attempt_at).toBe(close.next_attempt_at + 4000);
    // Remove the injected provider fault and reconstruct from durable intent/attachments.
    await runInDurableObject(owner(f.id), (_instance, state) => { for (const ws of state.getWebSockets()) if (ws.auditClose) { ws.close = ws.auditClose; delete ws.auditClose; } });
    await evictDurableObject(owner(f.id));
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.connection_id).one())).toEqual(retry);
    Date.now = () => retry.next_attempt_at;
    await runInDurableObject(owner(f.id), instance => instance.alarm());
    await runInDurableObject(owner(f.id), async (instance, state) => {
      for (const ws of state.getWebSockets()) {
        if (JSON.parse(ws.deserializeAttachment()).connection_id === predecessor.connection_id && ws.readyState !== 3) ws.close(1000);
      }
      await instance.alarm();
    });
    expect((await sync(f, fresh)).status).toBe(200);
    const final = await runInDurableObject(owner(f.id), (_instance, state) => ({
      pending: [...state.storage.sql.exec("SELECT connection_id FROM game_socket_closures WHERE connection_id=?", predecessor.connection_id)],
      current: [...state.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE principal_id=?", f.members[0].body.player_id)],
    }));
    expect(final.pending).toEqual([]);
    expect(final.current).toEqual([{ connection_id: successor.initial.connection_id }]);
  } finally {
    Date.now = realNow;
    await runInDurableObject(owner(f.id), (instance, state) => {
      if (instance.auditAccept) { state.acceptWebSocket = instance.auditAccept; delete instance.auditAccept; }
      state.storage.sql.exec("DROP TRIGGER IF EXISTS audit_accept");
      for (const ws of state.getWebSockets()) if (ws.auditClose) { ws.close = ws.auditClose; delete ws.auditClose; }
    });
    successor?.socket.close(1000);
  }
});

test("known predecessor messages and errors cannot bypass close backoff or poison its successor", async () => {
  const f = await fixture(true); const realNow = Date.now;
  const predecessor = await stream(f, f.members[0].session);
  let successor; let calls = 0; let staleFrames = 0;
  predecessor.socket.addEventListener("message", () => { staleFrames++; });
  await runInDurableObject(owner(f.id), (_instance, state) => {
    const socket = state.getWebSockets().find(ws => JSON.parse(ws.deserializeAttachment()).connection_id === predecessor.initial.connection_id);
    expect(socket).toBeDefined();
    socket.auditClose = socket.close;
    socket.close = () => { calls++; throw new Error("test-only predecessor close outage"); };
  });
  try {
    const recovered = await recover(f); expect(recovered.status).toBe(200);
    const fresh = cookie(recovered);
    const close = await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.initial.connection_id).one());
    expect(close.attempt_count).toBe(1);
    const afterFailure = calls;
    Date.now = () => close.next_attempt_at - 1;
    successor = await stream(f, fresh);
    await runInDurableObject(owner(f.id), instance => instance.alarm());
    expect(calls).toBe(afterFailure);
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.initial.connection_id).one())).toEqual(close);
    for (const message of [
      JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: predecessor.initial.connection_id, delivery_id: predecessor.initial.delivery_id, view_revision: predecessor.initial.view_revision }),
      "{", new Uint8Array([0, 1]).buffer,
    ]) {
      await runInDurableObject(owner(f.id), async (instance, state) => {
        const socket = state.getWebSockets().find(ws => JSON.parse(ws.deserializeAttachment()).connection_id === predecessor.initial.connection_id);
        await instance.webSocketMessage(socket, message);
      });
    }
    await runInDurableObject(owner(f.id), async (instance, state) => {
      const socket = state.getWebSockets().find(ws => JSON.parse(ws.deserializeAttachment()).connection_id === predecessor.initial.connection_id);
      await instance.webSocketError(socket, new Error("test-only predecessor callback"));
    });
    expect(calls).toBe(afterFailure);
    expect(await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.initial.connection_id).one())).toEqual(close);
    const delivered = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error("successor snapshot timeout")), 3000);
      successor.socket.addEventListener("message", event => { clearTimeout(timeout); resolve(JSON.parse(event.data)); }, { once: true });
    });
    const renamed = await exports.default.fetch(`${origin}/api/games/${f.id}/membership/alias`, {
      method: "PATCH", headers: { Origin: origin, Cookie: fresh, "Content-Type": "application/json", "Idempotency-Key": commandId() },
      body: JSON.stringify({ alias: "HealthySuccessor" }),
    });
    expect(renamed.status).toBe(200);
    expect((await delivered).connection_id).toBe(successor.initial.connection_id);
    expect(staleFrames).toBe(0);
    expect(calls).toBe(afterFailure);
    Date.now = () => close.next_attempt_at;
    await runInDurableObject(owner(f.id), instance => instance.alarm());
    expect(calls).toBe(afterFailure + 1);
    const retry = await runInDurableObject(owner(f.id), (_instance, state) => state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?", predecessor.initial.connection_id).one());
    expect(retry.attempt_count).toBe(2);
    expect(retry.next_attempt_at).toBe(close.next_attempt_at + 4000);
    // Exact absence/closed ACK is allowed while the retry is still in the future.
    await runInDurableObject(owner(f.id), (_instance, state) => {
      for (const ws of state.getWebSockets()) if (ws.auditClose) { ws.close = ws.auditClose; delete ws.auditClose; }
    });
    predecessor.socket.close(1000);
    await runInDurableObject(owner(f.id), instance => instance.alarm());
    expect(await runInDurableObject(owner(f.id), (_instance, state) => [...state.storage.sql.exec("SELECT connection_id FROM game_socket_closures WHERE connection_id=?", predecessor.initial.connection_id)])).toEqual([]);
    expect((await sync(f, fresh)).status).toBe(200);
    expect(successor.initial.view.player_id).toBe(f.members[0].body.player_id);
  } finally {
    Date.now = realNow;
    await runInDurableObject(owner(f.id), (_instance, state) => { for (const ws of state.getWebSockets()) if (ws.auditClose) { ws.close = ws.auditClose; delete ws.auditClose; } });
    predecessor.socket.close(1000); successor?.socket.close(1000);
  }
});
