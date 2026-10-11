import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject, evictDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId, password } from "./fixtures.js";

const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async () => {
  await reset();
  await resetStorage();
  await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll());
});
async function join(f, alias, answer = password()) {
  const context = await post(`/api/games/${f.id}/admission-context`, { game_code: f.code }, { id: null });
  expect(context.status).toBe(200);
  const response = await post(`/api/games/${f.id}/players`, { game_code: f.code, alias, recovery_answer: answer }, { session: cookie(context) });
  expect(response.status).toBe(200);
  return { alias, answer, session: cookie(response), body: await response.json() };
}
async function fixture() {
  const host = await enroll("LifecycleBoundaryHost");
  const made = await post("/api/games", { configuration: {
    numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: 3, spectator_capacity: 0,
  } }, { session: host.session });
  expect(made.status).toBe(200);
  const id = (await made.json()).game.game_id;
  const opened = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: host.session });
  expect(opened.status).toBe(200);
  const f = { id, code: (await opened.json()).game_code, host };
  f.players = [await join(f, "BoundaryPlayer"), await join(f, "SecondPlayer")];
  return f;
}
function rename(f, alias, { id = commandId(), session = f.players[0].session } = {}) {
  return exports.default.fetch(`${origin}/api/games/${f.id}/membership/alias`, {
    method: "PATCH", headers: { Origin: origin, Cookie: session, "Content-Type": "application/json", "Idempotency-Key": id },
    body: JSON.stringify({ alias }),
  });
}
const leave = (f, state, options = {}) => post(`/api/games/${f.id}/membership/leave`, { expected_state: state }, { session: f.players[0].session, ...options });
const recover = (f, alias = f.players[0].alias, answer = f.players[0].answer) => post("/api/player-recovery", { game_code: f.code, alias, recovery_answer: answer });
const sync = (f, session = f.players[0].session, view = "player") => exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=${view}`, { headers: { Cookie: session } });
async function snapshot(f, session = f.players[0].session, view = "player") {
  const response = await sync(f, session, view);
  expect(response.status).toBe(200);
  return response.json();
}
async function state(f) {
  return runInDurableObject(owner(f.id), (_instance, storageState) => ({
    players: [...storageState.storage.sql.exec("SELECT * FROM game_players ORDER BY player_id")],
    sessions: [...storageState.storage.sql.exec("SELECT * FROM game_sessions ORDER BY session_id")],
    game: [...storageState.storage.sql.exec("SELECT game_id,state,revision,idle_due,last_host_activity,started_at FROM game_record")],
    cells: [...storageState.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")],
    views: [...storageState.storage.sql.exec("SELECT * FROM game_view_revisions ORDER BY view_key")],
    connections: [...storageState.storage.sql.exec("SELECT * FROM game_current_connections ORDER BY session_id")],
  }));
}
async function stream(f, player = f.players[0]) {
  const response = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`, {
    headers: { Cookie: player.session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" },
  });
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  const initial = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("lifecycle snapshot timeout")), 3000);
    socket.addEventListener("message", event => {
      const frame = JSON.parse(event.data);
      if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision }));
      clearTimeout(timeout);
      resolve(frame);
    });
  });
  socket.accept();
  return { socket, initial: await initial };
}
async function started(f) {
  const sockets = [await stream(f), await stream(f, f.players[1])];
  const current = await snapshot(f, f.host.session, "account");
  expect(current.snapshot.connected_player_count).toBe(2);
  const response = await post(`/api/games/${f.id}/start`, { expected_revision: current.view_revision }, { session: f.host.session });
  expect(response.status).toBe(200);
  return sockets;
}
function safe(response) {
  expect(response.headers.get("cache-control")).toBe("no-store");
  expect(response.headers.get("referrer-policy")).toBe("no-referrer");
  expect(response.headers.get("x-content-type-options")).toBe("nosniff");
}

test("alias change preserves current authority and recovery material while no-op and exact retry have no revision effect", async () => {
  const f = await fixture();
  const before = await state(f);
  const originalOwn = await snapshot(f);
  const originalHost = await snapshot(f, f.host.session, "account");
  const other = await snapshot(f, f.players[1].session);
  const noop = await rename(f, `  ${f.players[0].alias}  `);
  expect(noop.status).toBe(200); safe(noop);
  expect(noop.headers.get("set-cookie")).toBeNull();
  expect(await state(f)).toEqual(before);
  const command = commandId();
  const changed = await rename(f, "  NewExactAlias!  ", { id: command });
  expect(changed.status).toBe(200); safe(changed);
  expect(changed.headers.get("set-cookie")).toBeNull();
  const body = await changed.json();
  expect(body.player_id).toBe(f.players[0].body.player_id);
  expect(body.alias).toBe("NewExactAlias!");
  expect(body.session_expires_at).toBe(f.players[0].body.session_expires_at);
  const after = await state(f);
  expect(after.sessions).toEqual(before.sessions);
  expect(after.players.map(p => ({ ...p, alias: before.players.find(old => old.player_id === p.player_id).alias }))).toEqual(before.players);
  const own = await snapshot(f);
  const host = await snapshot(f, f.host.session, "account");
  expect(own.snapshot.alias).toBe(body.alias);
  expect(own.view_revision).toBeGreaterThan(originalOwn.view_revision);
  expect(host.view_revision).toBeGreaterThan(originalHost.view_revision);
  expect(await snapshot(f, f.players[1].session)).toEqual(other);
  const repeated = await rename(f, "NewExactAlias!", { id: command });
  expect(repeated.status).toBe(200); expect(repeated.headers.get("set-cookie")).toBeNull();
  expect(await repeated.json()).toEqual({ result: "committed", receipt: body.receipt });
  expect(await state(f)).toEqual(after);
  const conflict = await rename(f, "DifferentAlias", { id: command });
  expect(conflict.status).toBe(409);
  expect((await recover(f)).status).toBe(401);
  const restored = await recover(f, "NewExactAlias!");
  expect(restored.status).toBe(200);
  const restoredView = await snapshot(f, cookie(restored));
  expect(restoredView.snapshot.player_id).toBe(body.player_id);
  expect(restoredView.snapshot.alias).toBe(body.alias);
  const fresh = await join(f, f.players[0].alias);
  expect(fresh.body.player_id).not.toBe(body.player_id);
});

test("mutation selects only the player cookie, not a coexisting account or arbitrary identity", async () => {
  const f = await fixture();
  expect((await rename(f, "Other", { session: f.host.session })).status).toBe(401);
  const selected = await rename(f, "SelectedPlayer", { session: `__Host-brews_session=; __Host-brews_session=invalid; ${f.players[0].session}` });
  expect(selected.status).toBe(200);
  expect((await selected.json()).player_id).toBe(f.players[0].body.player_id);
  expect((await rename(f, "Chosen", { session: `${f.host.session}; ${f.players[0].session}; ${f.players[0].session}` })).status).toBe(400);
  expect((await leave(f, "awaiting_players", { session: f.host.session })).status).toBe(401);
});

test("closed original-byte alias and Leave bodies reject duplicate, nullable, target and wrong-lifecycle claims", async () => {
  const f = await fixture();
  const before = await state(f);
  for (const [action, method, bodies] of [
    ["alias", "PATCH", ["[]", "null", "{}", '{"alias":null}', '{"alias":""}', '{"alias":"\\tName"}', '{"alias":"Name","\\u0061lias":"Other"}', '{"alias":"Name","player_id":"target"}', '{"alias":"Name","expected_revision":0}']],
    ["leave", "POST", ["[]", "null", "{}", '{"expected_state":null}', '{"expected_state":"new"}', '{"expected_state":"resolved"}', '{"expected_state":"AwaitingPlayers"}', '{"expected_state":"awaiting_players","\\u0065xpected_state":"in_progress"}', '{"expected_state":"awaiting_players","player_id":"target"}', '{"expected_state":"awaiting_players","game_code":"ZZZZZZZZ"}']],
  ]) {
    for (const body of bodies) {
      const response = await exports.default.fetch(`${origin}/api/games/${f.id}/membership/${action}`, {
        method, headers: { Origin: origin, Cookie: f.players[0].session, "Content-Type": "application/json", "Idempotency-Key": commandId() }, body,
      });
      expect(response.status, `${action} rejects closed payload`).toBe(400);
      expect(response.headers.get("set-cookie")).toBeNull(); safe(response);
    }
  }
  expect(await state(f)).toEqual(before);
});

for (const mode of ["IGNORE", "ABORT, 'test-only alias write fault'"]) {
  test(`alias transaction ${mode.startsWith("IGNORE") ? "ignored" : "aborted"} required write fails closed without claiming alias or receipt`, async () => {
    const f = await fixture();
    const before = await state(f);
    await runInDurableObject(owner(f.id), (_instance, s) => s.storage.sql.exec(`CREATE TRIGGER boundary_alias_fault BEFORE UPDATE OF alias ON game_players BEGIN SELECT RAISE(${mode}); END`));
    const command = commandId();
    const denied = await rename(f, "MustNotCommit", { id: command });
    expect(denied.status).toBe(503); expect(denied.headers.get("set-cookie")).toBeNull();
    expect(await state(f)).toEqual(before);
    expect(await runInDurableObject(owner(f.id), (_instance, s) => [...s.storage.sql.exec("SELECT command_id FROM game_receipts WHERE command_id=?", command)])).toEqual([]);
  });
}

test("lobby Leave removes all participant authority and answer; old cookie cannot replay or reclaim membership", async () => {
  const f = await fixture();
  const connection = await stream(f);
  const otherConnection = await stream(f, f.players[1]);
  expect((await snapshot(f, f.host.session, "account")).snapshot.connected_player_count).toBe(2);
  const command = commandId();
  const left = await leave(f, "awaiting_players", { id: command });
  expect(left.status).toBe(200); safe(left);
  const body = await left.json();
  expect(body.membership_retained).toBe(false);
  expect(body.fresh_admission_required).toBe(true);
  const cleared = left.headers.get("set-cookie");
  expect(cleared).toContain(`Path=/api/games/${f.id}`);
  expect(cleared).toContain("Max-Age=0"); expect(cleared).toContain("HttpOnly"); expect(cleared).toContain("Secure");
  const after = await state(f);
  expect(after.players.map(p => p.player_id)).not.toContain(f.players[0].body.player_id);
  expect(after.sessions.map(s => s.player_id)).not.toContain(f.players[0].body.player_id);
  expect(after.connections.map(c => c.principal_id)).not.toContain(f.players[0].body.player_id);
  const privateCounts = await runInDurableObject(owner(f.id), (_instance, s) => ({
    recovery: s.storage.sql.exec("SELECT count(*) AS n FROM game_recovery WHERE player_id=?", f.players[0].body.player_id).one().n,
    grants: s.storage.sql.exec("SELECT count(*) AS n FROM game_connection_grants WHERE principal_id=?", f.players[0].body.player_id).one().n,
    views: s.storage.sql.exec("SELECT count(*) AS n FROM game_view_revisions WHERE view_key=?", f.players[0].body.player_id).one().n,
  }));
  expect(privateCounts).toEqual({ recovery: 0, grants: 0, views: 0 });
  const remainingHost = await snapshot(f, f.host.session, "account");
  expect(remainingHost.snapshot.connected_player_count).toBe(1);
  expect((await post(`/api/games/${f.id}/start`, { expected_revision: remainingHost.view_revision }, { session: f.host.session })).status).toBe(409);
  expect((await sync(f)).status).toBe(401);
  expect((await recover(f)).status).toBe(401);
  const replay = await leave(f, "awaiting_players", { id: command });
  expect(replay.status).toBe(401); expect(replay.headers.get("set-cookie")).toBeNull();
  const fresh = await join(f, f.players[0].alias);
  expect(fresh.body.player_id).not.toBe(f.players[0].body.player_id);
  connection.socket.close(1000);
  otherConnection.socket.close(1000);
});

for (const mode of ["IGNORE", "ABORT, 'test-only Leave write fault'"]) {
  test(`lobby Leave ${mode.startsWith("IGNORE") ? "ignored" : "aborted"} deletion rolls back membership sessions revisions and outcome`, async () => {
    const f = await fixture();
    const before = await state(f);
    await runInDurableObject(owner(f.id), (_instance, s) => s.storage.sql.exec(`CREATE TRIGGER boundary_leave_fault BEFORE DELETE ON game_players BEGIN SELECT RAISE(${mode}); END`));
    const command = commandId();
    const response = await leave(f, "awaiting_players", { id: command });
    expect(response.status).toBe(503); expect(response.headers.get("set-cookie")).toBeNull();
    expect(await state(f)).toEqual(before);
    expect(await runInDurableObject(owner(f.id), (_instance, s) => [...s.storage.sql.exec("SELECT command_id FROM game_receipts WHERE command_id=?", command)])).toEqual([]);
  });
}

test("fresh delayed lobby Leave after Start conflicts rather than selecting in-progress meaning", async () => {
  const f = await fixture();
  const connections = await started(f);
  try {
    const before = await state(f);
    expect((await leave(f, "awaiting_players")).status).toBe(409);
    expect((await rename(f, "TooLate")).status).toBe(409);
    expect(await state(f)).toEqual(before);
  } finally { for (const c of connections) c.socket.close(1000); }
});

test("in-progress Leave retains board and fixed session; reconnect and same-command retry never erase successor presence or Leave history", async () => {
  const f = await fixture();
  const connections = await started(f);
  let successor;
  try {
    const before = await state(f);
    const own = await snapshot(f);
    const command = commandId();
    const response = await leave(f, "in_progress", { id: command });
    expect(response.status).toBe(200); expect(response.headers.get("set-cookie")).toBeNull(); safe(response);
    const body = await response.json();
    expect(body.membership_retained).toBe(true);
    expect(body.fresh_admission_required).toBe(false);
    const left = await state(f);
    expect(left.sessions).toEqual(before.sessions);
    expect(left.cells).toEqual(before.cells);
    const retained = left.players.find(p => p.player_id === f.players[0].body.player_id);
    expect(retained.last_explicit_leave_at).not.toBeNull();
    expect(retained.session_epoch).toBe(before.players.find(p => p.player_id === retained.player_id).session_epoch);
    expect(left.connections.map(c => c.principal_id)).not.toContain(retained.player_id);
    expect((await snapshot(f)).snapshot.board).toEqual(own.snapshot.board);
    successor = await stream(f);
    const successorState = await state(f);
    expect(successor.initial.session_expires_at).toBe(f.players[0].body.session_expires_at);
    const replay = await leave(f, "in_progress", { id: command });
    expect(replay.status).toBe(200); expect(replay.headers.get("set-cookie")).toBeNull();
    expect(await replay.json()).toEqual({ result: "committed", receipt: body.receipt });
    expect((await state(f)).connections).toEqual(successorState.connections);
    expect((await state(f)).players).toEqual(left.players);
    await evictDurableObject(owner(f.id));
    const host = await snapshot(f, f.host.session, "account");
    expect(host.snapshot.players.find(p => p.player_id === retained.player_id).connected).toBe(true);
    expect((await state(f)).players).toEqual(left.players);
    for (const value of ["1", "2"]) {
      const host = await snapshot(f, f.host.session, "account");
      const drawn = await post(`/api/games/${f.id}/calls/manual`, { value, expected_revision: host.view_revision }, { session: f.host.session });
      expect(drawn.status).toBe(200);
    }
    expect((await snapshot(f)).snapshot.board.cells.filter(c => c.is_matched)).toHaveLength(2);
  } finally { successor?.socket.close(1000); for (const c of connections) c.socket.close(1000); }
});

test("known durably fenced predecessor close failure cannot block valid fixed-session return", async () => {
  const f = await fixture();
  const connections = await started(f);
  let successor;
  await runInDurableObject(owner(f.id), (instance, s) => {
    const candidates = s.getWebSockets().filter(ws => {
      const hint = JSON.parse(ws.deserializeAttachment());
      return hint.viewer.kind === "player" && hint.viewer.id === f.players[0].body.player_id;
    });
    expect(candidates).toHaveLength(1);
    instance.lifecycleBoundarySocket = { socket: candidates[0], close: candidates[0].close };
    candidates[0].close = () => { throw new Error("test-only fenced predecessor transport close failure"); };
  });
  try {
    const response = await leave(f, "in_progress");
    expect([200, 503]).toContain(response.status);
    const durable = await runInDurableObject(owner(f.id), (_instance, s) => ({
      pending: [...s.storage.sql.exec("SELECT connection_id,player_id FROM game_socket_closures")],
      current: [...s.storage.sql.exec("SELECT connection_id,principal_id FROM game_current_connections WHERE principal_id=?", f.players[0].body.player_id)],
      membership: [...s.storage.sql.exec("SELECT player_id,last_explicit_leave_at FROM game_players WHERE player_id=?", f.players[0].body.player_id)],
    }));
    expect(durable.current).toEqual([]);
    expect(durable.membership[0].last_explicit_leave_at).not.toBeNull();
    expect(durable.pending).toContainEqual({ connection_id: connections[0].initial.connection_id, player_id: f.players[0].body.player_id });
    successor = await stream(f);
    expect(successor.initial.view.player_id).toBe(f.players[0].body.player_id);
    expect(successor.initial.session_expires_at).toBe(f.players[0].body.session_expires_at);
    const after = await runInDurableObject(owner(f.id), (_instance, s) => ({
      pending: [...s.storage.sql.exec("SELECT connection_id FROM game_socket_closures")],
      current: [...s.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE principal_id=?", f.players[0].body.player_id)],
    }));
    expect(after.pending.map(c => c.connection_id)).toContain(connections[0].initial.connection_id);
    expect(after.current).toEqual([{ connection_id: successor.initial.connection_id }]);
  } finally {
    await runInDurableObject(owner(f.id), instance => {
      instance.lifecycleBoundarySocket.socket.close = instance.lifecycleBoundarySocket.close;
      delete instance.lifecycleBoundarySocket;
    });
    successor?.socket.close(1000);
    for (const c of connections) c.socket.close(1000);
  }
});

test("Leave captures physically accepted prepared orphan before deleting grants, even after failed owner socket acceptance", async () => {
  const f = await fixture();
  const connections = await started(f);
  let successor;
  await runInDurableObject(owner(f.id), (instance, s) => {
    instance.lifecycleBoundaryAccept = s.acceptWebSocket;
    s.acceptWebSocket = function (ws, tags) {
      instance.lifecycleBoundaryAccept.call(this, ws, tags);
      ws.lifecycleBoundaryClose = ws.close;
      ws.close = () => { throw new Error("test-only accepted preparation close failure"); };
    };
    // Player ID is a canonical test-generated UUID, never browser SQL input.
    s.storage.sql.exec(`CREATE TRIGGER boundary_prepared_failure BEFORE INSERT ON game_current_connections WHEN NEW.principal_id='${f.players[0].body.player_id}' BEGIN SELECT RAISE(IGNORE); END`);
  });
  try {
    const failed = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`, {
      headers: { Cookie: f.players[0].session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" },
    });
    expect(failed.status).toBe(503);
    await runInDurableObject(owner(f.id), (instance, s) => {
      s.acceptWebSocket = instance.lifecycleBoundaryAccept;
      delete instance.lifecycleBoundaryAccept;
      s.storage.sql.exec("DROP TRIGGER boundary_prepared_failure");
    });
    const preparation = await runInDurableObject(owner(f.id), (_instance, s) => ({
      grants: [...s.storage.sql.exec("SELECT connection_id FROM game_connection_grants WHERE principal_id=?", f.players[0].body.player_id)],
      physical: s.getWebSockets().filter(ws => !!ws.lifecycleBoundaryClose).map(ws => JSON.parse(ws.deserializeAttachment()).connection_id),
    }));
    expect(preparation.grants).toHaveLength(1);
    expect(preparation.physical).toEqual([preparation.grants[0].connection_id]);
    const left = await leave(f, "in_progress");
    expect([200, 503]).toContain(left.status);
    const captured = await runInDurableObject(owner(f.id), (_instance, s) => [...s.storage.sql.exec("SELECT connection_id FROM game_socket_closures WHERE connection_id=?", preparation.physical[0])]);
    expect(captured).toEqual([{ connection_id: preparation.physical[0] }]);
    successor = await stream(f);
    expect(successor.initial.session_expires_at).toBe(f.players[0].body.session_expires_at);
    expect(successor.initial.view.player_id).toBe(f.players[0].body.player_id);
  } finally {
    await runInDurableObject(owner(f.id), (instance, s) => {
      if (instance.lifecycleBoundaryAccept) { s.acceptWebSocket = instance.lifecycleBoundaryAccept; delete instance.lifecycleBoundaryAccept; }
      s.storage.sql.exec("DROP TRIGGER IF EXISTS boundary_prepared_failure");
      for (const ws of s.getWebSockets()) {
        if (ws.lifecycleBoundaryClose) { ws.close = ws.lifecycleBoundaryClose; delete ws.lifecycleBoundaryClose; }
      }
    });
    successor?.socket.close(1000);
    for (const c of connections) c.socket.close(1000);
  }
});

test("lost first rename response leaves one durable alias change and an authenticated receipt-only retry", async () => {
  const f = await fixture();
  const command = commandId();
  await runInDurableObject(owner(f.id), instance => {
    instance.lifecycleBoundaryOriginal = instance.fetch.bind(instance);
    instance.lifecycleBoundaryLost = false;
    instance.fetch = async request => {
      const input = new URL(request.url).pathname === "/command" ? await request.clone().json() : {};
      const response = await instance.lifecycleBoundaryOriginal(request.clone());
      if (input.operation === "rename_alias" && response.status === 200 && !instance.lifecycleBoundaryLost) {
        instance.lifecycleBoundaryLost = true;
        throw new Error("test-only completed rename response lost");
      }
      return response;
    };
  });
  try {
    const lost = await rename(f, "LostAliasAck", { id: command });
    expect(lost.status).toBe(503); expect(lost.headers.get("set-cookie")).toBeNull();
    const after = await state(f);
    expect(after.players.find(p => p.player_id === f.players[0].body.player_id).alias).toBe("LostAliasAck");
    const replay = await rename(f, " LostAliasAck ", { id: command });
    expect(replay.status).toBe(200); expect(replay.headers.get("set-cookie")).toBeNull();
    const receipt = await replay.json();
    expect(Object.keys(receipt)).toEqual(["result", "receipt"]);
    expect(receipt.result).toBe("committed");
    expect(receipt.receipt.command_id).toBe(command);
    expect(await state(f)).toEqual(after);
  } finally {
    await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.lifecycleBoundaryOriginal; delete instance.lifecycleBoundaryOriginal; delete instance.lifecycleBoundaryLost; });
  }
});

test("terminal player access grants final view only, never new alias or Leave effects", async () => {
  const f = await fixture();
  const connections = await started(f);
  try {
    const cancelled = await post(`/api/games/${f.id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: f.host.session });
    expect(cancelled.status).toBe(200);
    const before = await state(f);
    expect((await snapshot(f)).snapshot.state).toBe("cancelled");
    const alias = await rename(f, "NoTerminalRename");
    expect(alias.status).toBe(401); expect(alias.headers.get("set-cookie")).toBeNull();
    const left = await leave(f, "in_progress");
    expect(left.status).toBe(401); expect(left.headers.get("set-cookie")).toBeNull();
    expect(await state(f)).toEqual(before);
    const exited = await post(`/api/games/${f.id}/exit?view=player`, {}, { session: f.players[0].session });
    expect(exited.status).toBe(200);
    expect((await sync(f)).status).toBe(401);
  } finally { for (const c of connections) c.socket.close(1000); }
});

test("superseded player authority after the first rename owner reply prevents private reply release", async () => {
  const f = await fixture();
  await runInDurableObject(owner(f.id), (instance, s) => {
    instance.lifecycleBoundaryOriginal = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const input = new URL(request.url).pathname === "/command" ? await request.clone().json() : {};
      const response = await instance.lifecycleBoundaryOriginal(request.clone());
      if (String(input.operation).includes("alias") && response.status === 200) s.storage.sql.exec("UPDATE game_players SET session_epoch=session_epoch+1 WHERE player_id=?", f.players[0].body.player_id);
      return response;
    };
  });
  try {
    const response = await rename(f, "CommittedNotReleased");
    expect(response.status).toBe(401); expect(response.headers.get("set-cookie")).toBeNull();
    expect(Object.keys(await response.json())).toEqual(["error"]);
  } finally {
    await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.lifecycleBoundaryOriginal; delete instance.lifecycleBoundaryOriginal; });
  }
});

test("recovered-session old Leave receipt expiry is fenced in transit without extending its original retry window", async () => {
  const f = await fixture();
  const connections = await started(f);
  const now = Date.now;
  try {
    const command = commandId();
    const first = await leave(f, "in_progress", { id: command });
    expect(first.status).toBe(200);
    const receipt = (await first.json()).receipt;
    const beforeExpiry = receipt.expires_at - 60_000;
    Date.now = () => beforeExpiry;
    const restored = await recover(f);
    expect(restored.status).toBe(200);
    const session = cookie(restored);
    await runInDurableObject(owner(f.id), instance => {
      instance.lifecycleBoundaryHolder = { fetch: instance.fetch.bind(instance), expired: false };
      instance.fetch = async request => {
        const input = await request.clone().json();
        if (input.expiry) Date.now = () => receipt.expires_at - 1;
        const response = await instance.lifecycleBoundaryHolder.fetch(request.clone());
        if (input.operation === "leave_player_output" && response.status === 200) {
          const body = await response.clone().json();
          expect(body.valid_until).toBe(receipt.expires_at);
          instance.lifecycleBoundaryHolder.expired = true;
          Date.now = () => receipt.expires_at;
        }
        return response;
      };
    });
    try {
      const denied = await leave(f, "in_progress", { id: command, session });
      expect(denied.status).toBe(401); expect(denied.headers.get("set-cookie")).toBeNull();
      expect(Object.keys(await denied.json())).toEqual(["error"]);
      const observed = await runInDurableObject(owner(f.id), (instance, s) => ({
        injected: instance.lifecycleBoundaryHolder.expired,
        floor: s.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms,
      }));
      expect(observed.injected).toBe(true);
      expect(observed.floor).toBeGreaterThanOrEqual(receipt.expires_at);
    } finally {
      await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.lifecycleBoundaryHolder.fetch; delete instance.lifecycleBoundaryHolder; });
    }
  } finally { Date.now = now; for (const c of connections) c.socket.close(1000); }
});

for (const syncCut of [3, 6]) {
  test(`reconstructed rename output rechecks current alias after second-owner storage await ${syncCut}`, async () => {
    const f = await fixture();
    await runInDurableObject(owner(f.id), (instance, s) => {
      instance.lifecycleBoundaryHolder = { fetch: instance.fetch.bind(instance), sync: s.storage.sync, active: false, cuts: 0, changed: false };
      s.storage.sync = async function () {
        const holder = instance.lifecycleBoundaryHolder;
        await holder.sync.call(this);
        if (holder.active && ++holder.cuts === syncCut) {
          s.storage.sql.exec("UPDATE game_players SET alias=? WHERE player_id=?", "ConcurrentAlias", f.players[0].body.player_id);
          s.storage.sql.exec("UPDATE game_view_revisions SET revision=revision+1 WHERE view_key=?", f.players[0].body.player_id);
          holder.changed = true;
        }
      };
      instance.fetch = async request => {
        const input = new URL(request.url).pathname === "/command" ? await request.clone().json() : {};
        if (input.operation === "rename_alias_output") instance.lifecycleBoundaryHolder.active = true;
        return instance.lifecycleBoundaryHolder.fetch(request.clone());
      };
    });
    try {
      const response = await rename(f, "CapturedAlias");
      expect(response.status).toBe(401); expect(response.headers.get("set-cookie")).toBeNull();
      expect(Object.keys(await response.json())).toEqual(["error"]);
      const observed = await runInDurableObject(owner(f.id), instance => instance.lifecycleBoundaryHolder.changed);
      expect(observed).toBe(true);
    } finally {
      await runInDurableObject(owner(f.id), (instance, s) => {
        s.storage.sync = instance.lifecycleBoundaryHolder.sync;
        instance.fetch = instance.lifecycleBoundaryHolder.fetch;
        delete instance.lifecycleBoundaryHolder;
      });
    }
  });
}

for (const fault of ["wrong_fence_sample", "wrong_fence_game", "unavailable_fence"]) {
  test(`rename observed expiry requires exact bound durable denial ACK: ${fault}`, async () => {
    const f = await fixture();
    const now = Date.now;
    await runInDurableObject(owner(f.id), instance => {
      instance.lifecycleBoundaryHolder = { fetch: instance.fetch.bind(instance), deadline: null };
      instance.fetch = async request => {
        const input = await request.clone().json();
        const holder = instance.lifecycleBoundaryHolder;
        if (input.expiry) {
          Date.now = () => holder.deadline - 1;
          return Response.json({
            game_id: fault === "wrong_fence_game" ? commandId() : input.game_id,
            checked_at: Date.now(), valid_until: null,
            outcome: fault === "unavailable_fence" ? { result: "unavailable" } : {
              result: "expiry_fenced",
              expiry: { ...input.expiry, observed_at: input.expiry.observed_at - (fault === "wrong_fence_sample" ? 1 : 0) },
            },
          });
        }
        const response = await holder.fetch(request.clone());
        if (input.operation === "rename_alias_output" && response.status === 200) {
          const reply = await response.clone().json();
          holder.deadline = reply.valid_until;
          Date.now = () => holder.deadline;
        }
        return response;
      };
    });
    try {
      const response = await rename(f, "NoUnboundDenial");
      expect(response.status).toBe(503); expect(response.headers.get("set-cookie")).toBeNull();
      expect(Object.keys(await response.json())).toEqual(["error"]);
    } finally {
      Date.now = now;
      await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.lifecycleBoundaryHolder.fetch; delete instance.lifecycleBoundaryHolder; });
    }
  });
}

for (const cut of [1, 2]) {
  test(`rename edge expiry cut ${cut} durably fences the original clock sample even if the denial await rolls back`, async () => {
    const f = await fixture();
    const now = Date.now;
    await runInDurableObject(owner(f.id), instance => {
      instance.lifecycleBoundaryHolder = { fetch: instance.fetch.bind(instance), reads: 0, deadline: null };
      instance.fetch = async request => {
        const input = await request.clone().json();
        const holder = instance.lifecycleBoundaryHolder;
        if (input.expiry && holder.deadline !== null) Date.now = () => holder.deadline - 1;
        const response = await holder.fetch(request.clone());
        if (String(input.operation).includes("alias") && response.status === 200) {
          const reply = await response.clone().json();
          if (++holder.reads === cut) {
            expect(Number.isSafeInteger(reply.valid_until)).toBe(true);
            holder.deadline = reply.valid_until;
            Date.now = () => holder.deadline;
          }
        }
        return response;
      };
    });
    try {
      const response = await rename(f, "ExpiryNotReleased");
      expect(response.status).toBe(401); expect(response.headers.get("set-cookie")).toBeNull();
      expect(Object.keys(await response.json())).toEqual(["error"]);
      const observed = await runInDurableObject(owner(f.id), (instance, s) => ({
        deadline: instance.lifecycleBoundaryHolder.deadline,
        floor: s.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms,
      }));
      expect(observed.floor).toBeGreaterThanOrEqual(observed.deadline);
      Date.now = () => observed.deadline - 1;
      const retried = await rename(f, "ExpiryNotReleased");
      expect(retried.status).toBe(503); expect(retried.headers.get("set-cookie")).toBeNull();
    } finally {
      Date.now = now;
      await runInDurableObject(owner(f.id), instance => { instance.fetch = instance.lifecycleBoundaryHolder.fetch; delete instance.lifecycleBoundaryHolder; });
    }
  });
}
