import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId, inOwner, seedPending } from "./fixtures.js";

beforeEach(async () => {
  await reset();
  await resetStorage();
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory, async (_instance, state) => state.storage.deleteAll());
});

async function newGame(existingActor) {
  const actor = existingActor ?? await enroll();
  const created = await post("/api/games", { configuration: {
    numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: 3, spectator_capacity: 0,
  } }, { session: actor.session });
  expect(created.status).toBe(200);
  const body = await created.json();
  return { actor, id: body.game.game_id };
}
async function sync(id, session, view = "player", known) {
  const query = `view=${view}${known === undefined ? "" : `&known_revision=${known}`}`;
  const response = await exports.default.fetch(`${origin}/api/games/${id}/sync?${query}`, { headers: { Cookie: session } });
  expect(response.status).toBe(200);
  return response.json();
}
async function stream(id, session, view = "player", acknowledge = true) {
  const response = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=${view}`, {
    headers: { Cookie: session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" },
  });
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  const queue = [];
  const readers = [];
  socket.addEventListener("message", event => {
    const frame = JSON.parse(event.data);
    if (acknowledge) socket.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision }));
    if (readers.length) readers.shift()(frame); else queue.push(frame);
  });
  const next = () => queue.length ? Promise.resolve(queue.shift()) : new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("gameplay snapshot timeout")), 3000);
    readers.push(frame => { clearTimeout(timeout); resolve(frame); });
  });
  socket.accept();
  const initial = await next();
  const atRevision = async revision => {
    for (let remaining = 0; remaining < 20; remaining++) {
      const frame = await next();
      if (frame.view_revision >= revision) return frame;
    }
    throw new Error("gameplay snapshot progression exceeded fixture bound");
  };
  return { socket, next, initial, atRevision };
}
async function startedGame(startStatus = 200, existingActor) {
  const fixture = await newGame(existingActor);
  const { id, actor } = fixture;
  const response = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(response.status).toBe(200);
  const lobby = await response.json();
  const players = [];
  for (const alias of ["First", "Second", "Offline"]) {
    const context = await post(`/api/games/${id}/admission-context`, { game_code: lobby.game_code }, { id: null });
    expect(context.status).toBe(200);
    const joined = await post(`/api/games/${id}/players`, { game_code: lobby.game_code, alias }, { session: cookie(context) });
    expect(joined.status).toBe(200);
    players.push({ session: cookie(joined), body: await joined.json() });
  }
  const connections = [await stream(id, players[0].session), await stream(id, players[1].session)];
  const host = await sync(id, actor.session, "account");
  expect(host.snapshot.connected_player_count).toBe(2);
  const started = await post(`/api/games/${id}/start`, { expected_revision: host.view_revision }, { session: actor.session });
  expect(started.status).toBe(startStatus);
  const first = await sync(id, players[0].session);
  const second = await sync(id, players[1].session);
  await connections[0].atRevision(first.view_revision);
  await connections[1].atRevision(second.view_revision);
  return { ...fixture, lobby, players, connections, startResult: await started.json() };
}
function owner(id) { return env.GAMES.get(env.GAMES.idFromName(id)); }
async function durableCalls(id) {
  return runInDurableObject(owner(id), (_instance, state) => [...state.storage.sql.exec("SELECT sequence_no,value FROM game_calls ORDER BY sequence_no")]);
}

test("manual and random calls commit through the Worker and deliver only authorized boards", async () => {
  const { id, actor, players, connections } = await startedGame();
  const host = await sync(id, actor.session, "account");
  const command = commandId();
  const input = { value: "1", expected_revision: host.view_revision };
  const response = await post(`/api/games/${id}/calls/manual`, input, { session: actor.session, id: command });
  expect(response.status).toBe(200);
  const accepted = await response.json();
  expect(accepted.result).toBe("call_accepted");
  expect(accepted.call).toEqual({ sequence_no: 1, value: "1" });
  expect(accepted.remaining_count).toBe(3);
  expect(accepted.exhausted).toBe(false);
  expect(response.headers.has("Set-Cookie")).toBe(false);
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  for (let index = 0; index < players.length; index++) {
    const view = await sync(id, players[index].session);
    expect(view.snapshot.player_id).toBe(players[index].body.player_id);
    expect(view.snapshot.calls).toEqual([accepted.call]);
    expect(view.snapshot.players).toBeUndefined();
    expect(view.snapshot.board.cells.filter(cell => cell.is_matched)).toHaveLength(1);
    expect(view.snapshot.board.cells.find(cell => cell.kind.value === "1").is_matched).toBe(true);
    if (index < connections.length) {
      const frame = await connections[index].atRevision(view.view_revision);
      expect(frame.view).toEqual(view.snapshot);
    }
  }
  const repeat = await post(`/api/games/${id}/calls/manual`, input, { session: actor.session, id: command });
  expect(repeat.status).toBe(200);
  expect(await repeat.json()).toEqual({ result: "committed", receipt: accepted.receipt });
  const stale = await post(`/api/games/${id}/calls/random`, { expected_revision: host.view_revision }, { session: actor.session });
  expect(stale.status).toBe(409);
  expect(await durableCalls(id)).toEqual([accepted.call]);
  const updated = await sync(id, actor.session, "account");
  const randomCommand = commandId();
  const randomInput = { expected_revision: updated.view_revision };
  const random = await post(`/api/games/${id}/calls/random`, randomInput, { session: actor.session, id: randomCommand });
  expect(random.status).toBe(200);
  const drawn = await random.json();
  expect(drawn.call.sequence_no).toBe(2);
  expect(drawn.call.value).not.toBe("1");
  expect(drawn.remaining_count).toBe(2);
  const randomRepeat = await post(`/api/games/${id}/calls/random`, randomInput, { session: actor.session, id: randomCommand });
  expect(randomRepeat.status).toBe(200);
  expect(await randomRepeat.json()).toEqual({ result: "committed", receipt: drawn.receipt });
  expect(await durableCalls(id)).toEqual([accepted.call, drawn.call]);
  for (const connection of connections) connection.socket.close(1000);
});

test("lost genuine Directory Start ACK recovers the current projection after an accepted first call", async () => {
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory, instance => {
    instance.startAckHolder = { fetch: instance.fetch.bind(instance) };
    instance.fetch = async request => {
      const response = await instance.startAckHolder.fetch(request.clone());
      if (new URL(request.url).pathname === "/games") {
        const original = await request.clone().json();
        if (original.action === "publish" && original.projection.state === "in_progress") {
          instance.lostStartAck = { request: original, reply: await response.clone().json(), status: response.status };
          throw new Error("test-only lost completed genuine Directory Start ACK");
        }
      }
      return response;
    };
  });
  let fixture, lost;
  try {
    fixture = await startedGame(202);
    expect(fixture.startResult).toEqual({ result: "pending", operation_id: expect.any(String) });
    lost = await runInDurableObject(directory, instance => instance.lostStartAck);
    expect(lost.status).toBe(200);
    expect(lost.reply.outcome).toMatchObject({ result: "published", published: true, source_revision: lost.request.projection.source_revision });
  } finally {
    await runInDurableObject(directory, instance => {
      instance.fetch = instance.startAckHolder.fetch;
      delete instance.startAckHolder;
    });
  }
  const { id, actor, connections } = fixture;
  const originalNow = Date.now;
  try {
    const host = await sync(id, actor.session, "account");
    const first = await post(`/api/games/${id}/calls/manual`, { value: "1", expected_revision: host.view_revision }, { session: actor.session });
    expect(first.status).toBe(200);
    expect((await first.json()).result).toBe("call_accepted");
    const before = await runInDurableObject(owner(id), (_instance, state) => ({
      revision: state.storage.sql.exec("SELECT revision FROM game_record").one().revision,
      pending: [...state.storage.sql.exec("SELECT operation_id,kind,phase,fence_revision,next_attempt_at FROM game_pending_work")],
      calls: [...state.storage.sql.exec("SELECT * FROM game_calls ORDER BY sequence_no")],
      cells: [...state.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")],
      receipts: [...state.storage.sql.exec("SELECT * FROM game_receipts ORDER BY command_id")],
    }));
    expect(before.pending).toEqual([{ operation_id: fixture.startResult.operation_id, kind: "start_projection", phase: "awaiting_acknowledgement", fence_revision: expect.any(Number), next_attempt_at: expect.any(Number) }]);
    expect(before.revision).toBeGreaterThan(lost.request.projection.source_revision);
    expect.soft(before.pending[0].fence_revision).toBe(before.revision);
    await evictDurableObject(owner(id));
    Date.now = () => before.pending[0].next_attempt_at;
    await runInDurableObject(owner(id), async (instance, state) => {
      await instance.alarm();
      expect.soft([...state.storage.sql.exec("SELECT kind FROM game_pending_work")]).toEqual([]);
      expect(state.storage.sql.exec("SELECT revision FROM game_record").one().revision).toBe(before.revision);
      expect([...state.storage.sql.exec("SELECT * FROM game_calls ORDER BY sequence_no")]).toEqual(before.calls);
      expect([...state.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")]).toEqual(before.cells);
      expect([...state.storage.sql.exec("SELECT * FROM game_receipts ORDER BY command_id")]).toEqual(before.receipts);
    });
    const published = await runInDurableObject(directory, (_instance, state) =>
      [...state.storage.sql.exec("SELECT state,source_revision FROM directory_game_index WHERE game_id=?", id)]);
    expect.soft(published).toEqual([{ state: "in_progress", source_revision: before.revision }]);
    console.log("START_ACK_FIRST_CALL_RECOVERY " + JSON.stringify({ oldSource: lost.request.projection.source_revision, currentSource: before.revision, pendingFence: before.pending[0].fence_revision, published }));
  } finally {
    Date.now = originalNow;
    for (const connection of connections) connection.socket.close(1000);
  }
});

test("rejected calls preserve progression and exhaustion never auto-ends a game", async () => {
  const fixture = await startedGame();
  const { id, actor, players, connections } = fixture;
  const host = await callAllValues(fixture);
  const before = await runInDurableObject(owner(id), (_instance, state) => ({
    calls: [...state.storage.sql.exec("SELECT * FROM game_calls ORDER BY sequence_no")],
    cells: [...state.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")],
    revisions: [...state.storage.sql.exec("SELECT * FROM game_view_revisions ORDER BY view_key")],
  }));
  const exhausted = await post(`/api/games/${id}/calls/random`, { expected_revision: host.view_revision }, { session: actor.session });
  expect(exhausted.status).toBe(409);
  const duplicate = await post(`/api/games/${id}/calls/manual`, { value: "1", expected_revision: host.view_revision }, { session: actor.session });
  expect(duplicate.status).toBe(409);
  const outside = await post(`/api/games/${id}/calls/manual`, { value: "5", expected_revision: host.view_revision }, { session: actor.session });
  expect(outside.status).toBe(409);
  const player = await post(`/api/games/${id}/calls/random`, { expected_revision: host.view_revision }, { session: players[0].session });
  expect(player.status).toBe(401);
  const unconfirmed = await post(`/api/games/${id}/cancel`, { confirmed: false, expected_state: "in_progress" }, { session: actor.session });
  expect(unconfirmed.status).toBe(400);
  const delayed = await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "awaiting_players" }, { session: actor.session });
  expect(delayed.status).toBe(409);
  const after = await runInDurableObject(owner(id), (_instance, state) => ({
    calls: [...state.storage.sql.exec("SELECT * FROM game_calls ORDER BY sequence_no")],
    cells: [...state.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")],
    revisions: [...state.storage.sql.exec("SELECT * FROM game_view_revisions ORDER BY view_key")],
  }));
  expect(after).toEqual(before);
  expect((await sync(id, actor.session, "account")).snapshot.game.state).toBe("in_progress");
  for (const connection of connections) connection.socket.close(1000);
});

async function callAllValues(fixture) {
  for (const value of ["1", "2", "3", "4"]) {
    const host = await sync(fixture.id, fixture.actor.session, "account");
    const response = await post(`/api/games/${fixture.id}/calls/manual`, { value, expected_revision: host.view_revision }, { session: fixture.actor.session });
    expect(response.status).toBe(200);
  }
  const host = await sync(fixture.id, fixture.actor.session, "account");
  expect(host.snapshot.game.state).toBe("in_progress");
  return host;
}

test.each(["new", "awaiting_players"])("confirmed %s cancellation deletes pre-start data without History and releases only its reservation", async state => {
  const { actor, id } = await newGame();
  if (state === "awaiting_players") {
    const lobby = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
    expect(lobby.status).toBe(200);
  }
  const command = commandId();
  const input = { confirmed: true, expected_state: state };
  const response = await post(`/api/games/${id}/cancel`, input, { session: actor.session, id: command });
  expect(response.status).toBe(200);
  const terminal = await response.json();
  expect(terminal).toMatchObject({ result: "terminalized", game_id: id, state: "cancelled", expected_state: state, history_available: false, history_expires_at: null, winner: null });
  expect(response.headers.has("Set-Cookie")).toBe(false);
  await runInDurableObject(owner(id), (_instance, storage) => {
    for (const table of ["game_history", "game_history_calls", "game_history_players", "game_history_board_cells", "game_players", "game_boards", "game_sessions", "game_recovery", "game_configuration", "game_connection_grants", "game_pending_work"]) {
      expect([...storage.storage.sql.exec(`SELECT * FROM ${table}`)]).toEqual([]);
    }
  });
  const nextGame = await post("/api/games", {}, { session: actor.session });
  expect(nextGame.status).toBe(200);
  const retry = await post(`/api/games/${id}/cancel`, input, { session: actor.session, id: command });
  expect(retry.status).toBe(200);
  expect(await retry.json()).toEqual({ result: "committed", receipt: terminal.receipt });
  const stillReserved = await post("/api/games", {}, { session: actor.session });
  expect(stillReserved.status).toBe(409);
});

test("confirmed InProgress ending preserves final History without a winner", async () => {
  const fixture = await startedGame();
  const { actor, id, connections, players } = fixture;
  const command = commandId();
  const input = { confirmed: true, expected_state: "in_progress" };
  const response = await post(`/api/games/${id}/cancel`, input, { session: actor.session, id: command });
  expect(response.status).toBe(200);
  const terminal = await response.json();
  expect(terminal).toMatchObject({ result: "terminalized", state: "cancelled", history_available: true, winner: null });
  expect(terminal.history_expires_at).toBeGreaterThan(terminal.ended_at);
  for (let index = 0; index < players.length; index++) {
    const final = await sync(id, players[index].session);
    expect(final.snapshot.state).toBe("cancelled");
    expect(final.snapshot.winner).toBeNull();
    expect(final.snapshot.board.cells).toHaveLength(4);
    if (index < connections.length) expect((await connections[index].atRevision(final.view_revision)).view).toEqual(final.snapshot);
  }
  const retry = await post(`/api/games/${id}/cancel`, input, { session: actor.session, id: command });
  expect(retry.status).toBe(200);
  expect(await retry.json()).toEqual({ result: "committed", receipt: terminal.receipt });
  const nextGame = await post("/api/games", {}, { session: actor.session });
  expect(nextGame.status).toBe(200);
  for (const connection of connections) connection.socket.close(1000);
});

test("lost genuine terminal release ACK remains Pending and alarm retry cannot release a newer game", async () => {
  const fixture = await startedGame();
  const { id, actor, connections } = fixture;
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory, instance => {
    instance.gameplayOriginalFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const response = await instance.gameplayOriginalFetch(request.clone());
      if (new URL(request.url).pathname === "/games" && (await request.clone().json()).action === "release") {
        throw new Error("test-only lost genuine terminal release ACK");
      }
      return response;
    };
  });
  const command = commandId();
  const input = { confirmed: true, expected_state: "in_progress" };
  let pending;
  try {
    const response = await post(`/api/games/${id}/cancel`, input, { session: actor.session, id: command });
    expect(response.status).toBe(202);
    pending = await response.json();
    expect(pending).toEqual({ result: "pending", operation_id: expect.any(String) });
    expect(response.headers.has("Set-Cookie")).toBe(false);
  } finally {
    await runInDurableObject(directory, instance => {
      instance.fetch = instance.gameplayOriginalFetch;
      delete instance.gameplayOriginalFetch;
    });
  }
  const history = await runInDurableObject(owner(id), (_instance, state) => {
    expect(state.storage.sql.exec("SELECT state FROM game_record").one().state).toBe("cancelled");
    expect(state.storage.sql.exec("SELECT kind FROM game_pending_work").one().kind).toBe("release");
    return [...state.storage.sql.exec("SELECT * FROM game_history")];
  });
  const nextGame = await post("/api/games", {}, { session: actor.session });
  expect(nextGame.status).toBe(200);
  const nextId = (await nextGame.json()).game.game_id;
  await runInDurableObject(owner(id), async (instance, state) => {
    state.storage.sql.exec("UPDATE game_pending_work SET next_attempt_at=?", Date.now());
    await instance.alarm();
    expect([...state.storage.sql.exec("SELECT * FROM game_pending_work")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT * FROM game_history")]).toEqual(history);
  });
  const retry = await post(`/api/games/${id}/cancel`, input, { session: actor.session, id: command });
  expect(retry.status).toBe(200);
  expect((await retry.json()).result).toBe("committed");
  const blocked = await post("/api/games", {}, { session: actor.session });
  expect(blocked.status).toBe(409);
  await runInDurableObject(directory, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT game_id FROM directory_hosted_nonterminal_games")]).toEqual([{ game_id: nextId }]);
  });
  for (const connection of connections) connection.socket.close(1000);
});

test("native registry uncertainty schedules exact durable recovery and releases after hibernation while enumeration still fails", async () => {
  const { id, actor, connections } = await startedGame();
  await expect.poll(() => runInDurableObject(owner(id), (_instance, state) =>
    state.storage.sql.exec("SELECT count(*) AS count FROM game_delivery_pending").one().count), { timeout: 5000 }).toBe(0);
  const host = await stream(id, actor.session, "account", false);
  const creditBefore = await runInDurableObject(owner(id), async (_instance, state) => {
    expect(await state.storage.getAlarm()).toBeGreaterThan(Date.now() + 5000);
    return [...state.storage.sql.exec("SELECT connection_id,delivery_id,view_revision,frame_bytes FROM game_delivery_pending ORDER BY delivery_id")];
  });
  expect(creditBefore).toHaveLength(1);
  expect(creditBefore[0].frame_bytes).toBeGreaterThan(0);
  const originalNow = Date.now;
  const fixedNow = Date.now();
  let mutation, recovered;
  try {
    Date.now = () => fixedNow;
    await runInDurableObject(owner(id), (instance, state) => {
      // A plain holder preserves synchronous native methods through the exported SDK proxy.
      instance.registryRecoveryHolder = { sockets: state.getWebSockets.bind(state), alarm: instance.alarm.bind(instance) };
      instance.registryRecoveryCalls = 0;
      instance.alarm = async () => new Response("test-only automatic alarm delivery held");
      state.getWebSockets = () => {
        instance.registryRecoveryCalls++;
        if (state.storage.sql.exec("SELECT state FROM game_record").one().state === "cancelled") {
          throw new Error("test-only native registry enumeration outage");
        }
        const actual = instance.registryRecoveryHolder.sockets();
        instance.genuineRegistryArray = Array.isArray(actual);
        return actual;
      };
    });
    const response = await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session });
    expect(response.status).toBe(503);
    expect(response.headers.has("Set-Cookie")).toBe(false);
    mutation = await runInDurableObject(owner(id), async (instance, state) => ({
      genuineRegistryArray: instance.genuineRegistryArray,
      registryCalls: instance.registryRecoveryCalls,
      state: state.storage.sql.exec("SELECT state FROM game_record").one().state,
      pending: [...state.storage.sql.exec("SELECT kind,next_attempt_at FROM game_pending_work")],
      credit: [...state.storage.sql.exec("SELECT connection_id,delivery_id,view_revision,frame_bytes FROM game_delivery_pending ORDER BY delivery_id")],
      history: [...state.storage.sql.exec("SELECT * FROM game_history")],
      alarm: await state.storage.getAlarm(),
    }));
    expect(mutation.genuineRegistryArray).toBe(true);
    expect(mutation.registryCalls).toBeGreaterThan(1);
    expect(mutation.state).toBe("cancelled");
    expect(mutation.pending).toEqual([{ kind: "release", next_attempt_at: expect.any(Number) }]);
    expect(mutation.credit).toEqual(creditBefore);
    const coreDue = mutation.pending[0].next_attempt_at;
    const due = coreDue <= fixedNow ? fixedNow + 1000 : coreDue;
    expect.soft(mutation.alarm).toBe(due);
    // Reconstruct the actual owner: the Rust memory retry flag cannot be the recovery authority.
    await evictDurableObject(owner(id));
    Date.now = () => due;
    recovered = await runInDurableObject(owner(id), async (instance, state) => {
      instance.registryRecoveryHolder = { sockets: state.getWebSockets.bind(state), alarm: instance.alarm.bind(instance) };
      instance.registryRecoveryCalls = 0;
      instance.alarm = async () => new Response("test-only automatic alarm delivery held");
      state.getWebSockets = () => {
        instance.registryRecoveryCalls++;
        throw new Error("test-only native registry enumeration outage persists after reconstruction");
      };
      let unavailable = false;
      try { await instance.registryRecoveryHolder.alarm(); } catch { unavailable = true; }
      return {
        unavailable, registryCalls: instance.registryRecoveryCalls,
        pending: [...state.storage.sql.exec("SELECT kind,next_attempt_at FROM game_pending_work")],
        credit: [...state.storage.sql.exec("SELECT connection_id,delivery_id,view_revision,frame_bytes FROM game_delivery_pending ORDER BY delivery_id")],
        history: [...state.storage.sql.exec("SELECT * FROM game_history")],
        alarm: await state.storage.getAlarm(),
      };
    });
    expect(recovered.unavailable).toBe(true);
    expect(recovered.registryCalls).toBeGreaterThan(0);
    expect.soft(recovered.pending).toEqual([]);
    expect.soft(recovered.alarm).toBe(due + 1000);
    expect(recovered.credit).toEqual(creditBefore);
    expect(recovered.history).toEqual(mutation.history);
    const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
    const reservation = await runInDurableObject(directory, (_instance, state) =>
      [...state.storage.sql.exec("SELECT game_id FROM directory_global_reservation WHERE game_id=?", id)]);
    expect.soft(reservation).toEqual([]);
    console.log("RUNTIME_REGISTRY_RECOVERY " + JSON.stringify({ mutation, recovered, reservation }));
  } finally {
    Date.now = originalNow;
    await runInDurableObject(owner(id), (instance, state) => {
      if (instance.registryRecoveryHolder) {
        state.getWebSockets = instance.registryRecoveryHolder.sockets;
        instance.alarm = instance.registryRecoveryHolder.alarm;
        delete instance.registryRecoveryHolder;
      }
    });
    for (const connection of [...connections, host]) connection.socket.close(1000);
  }
});

test("terminal reconnect uses only original eligible grants and never admits a new account session", async () => {
  const fixture = await startedGame();
  const { id, actor, players, connections } = fixture;
  const response = await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session });
  expect(response.status).toBe(200);
  const terminal = await response.json();
  const originalExpiry = players[2].body.session_expires_at;
  const offline = await stream(id, players[2].session);
  expect(offline.initial.view.state).toBe("cancelled");
  expect(offline.initial.session_expires_at).toBe(originalExpiry);
  expect(offline.initial.view.history_expires_at).toBe(terminal.history_expires_at);
  expect(offline.initial.view.player_id).toBe(players[2].body.player_id);
  expect(offline.initial.view.players).toBeUndefined();
  const existingHost = await stream(id, actor.session, "account");
  expect(existingHost.initial.view.state).toBe("cancelled");
  const login = await post("/api/auth/login", { username: actor.username, password: actor.password });
  expect(login.status).toBe(200);
  const newSession = cookie(login);
  const denied = await exports.default.fetch(`${origin}/api/games/${id}/sync?view=account`, { headers: { Cookie: newSession } });
  expect([401, 403, 404]).toContain(denied.status);
  const reentry = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=account`, { headers: { Cookie: newSession, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
  expect([401, 403, 404]).toContain(reentry.status);
  expect((await reentry.json()).error).toBeDefined();
  for (const connection of [...connections, offline, existingHost]) connection.socket.close(1000);
});

test("terminal player Exit revokes only its final access while preserving other viewers and History", async () => {
  const fixture = await startedGame();
  const { id, actor, players, connections } = fixture;
  const ended = await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session });
  expect(ended.status).toBe(200);
  const history = await runInDurableObject(owner(id), (_instance, state) => [...state.storage.sql.exec("SELECT * FROM game_history")]);
  const closed = new Promise(resolve => connections[0].socket.addEventListener("close", resolve, { once: true }));
  const command = commandId();
  const headers = { Origin: origin, Cookie: players[0].session, "Content-Type": "application/json", "Idempotency-Key": command };
  const response = await exports.default.fetch(`${origin}/api/games/${id}/exit?view=player`, { method: "POST", headers, body: "{}" });
  expect(response.status).toBe(200);
  expect((await response.json()).result).toBe("exited");
  expect(response.headers.has("Set-Cookie")).toBe(false);
  await closed;
  const denied = await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`, { headers: { Cookie: players[0].session } });
  expect([401, 403, 404]).toContain(denied.status);
  const repeat = await exports.default.fetch(`${origin}/api/games/${id}/exit?view=player`, { method: "POST", headers, body: "{}" });
  expect([200, 401, 403, 404]).toContain(repeat.status);
  expect(repeat.headers.has("Set-Cookie")).toBe(false);
  const stillDenied = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=player`, { headers: { Cookie: players[0].session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
  expect([401, 403, 404]).toContain(stillDenied.status);
  expect((await sync(id, players[1].session)).snapshot.state).toBe("cancelled");
  expect(await runInDurableObject(owner(id), (_instance, state) => [...state.storage.sql.exec("SELECT * FROM game_history")])).toEqual(history);
  connections[1].socket.close(1000);
});

test("a host-selected offline qualifier resolves once with immutable History and existing final access", async () => {
  const fixture = await startedGame();
  const { id, actor, players, connections } = fixture;
  const host = await callAllValues(fixture);
  const selected = players[2].body.player_id;
  expect(host.snapshot.players.find(player => player.player_id === selected).board.qualified).toBe(true);
  const command = commandId();
  const input = { player_id: selected, expected_revision: host.view_revision };
  const response = await post(`/api/games/${id}/winner`, input, { session: actor.session, id: command });
  expect(response.status).toBe(200);
  const result = await response.json();
  expect(result.result).toBe("terminalized");
  expect(result.state).toBe("resolved");
  expect(result.winner).toEqual({ player_id: selected, alias: "Offline" });
  expect(result.history_available).toBe(true);
  expect(result.history_expires_at).toBeGreaterThan(result.ended_at);
  for (let index = 0; index < players.length; index++) {
    const final = await sync(id, players[index].session);
    expect(final.snapshot.state).toBe("resolved");
    expect(final.snapshot.player_id).toBe(players[index].body.player_id);
    expect(final.snapshot.winner).toEqual(result.winner);
    expect(final.snapshot.board.cells).toHaveLength(4);
    expect(final.snapshot.players).toBeUndefined();
    expect(final.snapshot.configuration).toBeUndefined();
    if (index < connections.length) {
      const frame = await connections[index].atRevision(final.view_revision);
      expect(frame.view).toEqual(final.snapshot);
    }
  }
  const historyBefore = await runInDurableObject(owner(id), (_instance, state) => ({
    parent: [...state.storage.sql.exec("SELECT * FROM game_history")],
    calls: [...state.storage.sql.exec("SELECT sequence_no,value FROM game_history_calls ORDER BY sequence_no")],
    players: [...state.storage.sql.exec("SELECT player_id,alias FROM game_history_players ORDER BY player_id")],
    cells: [...state.storage.sql.exec("SELECT * FROM game_history_board_cells ORDER BY player_id,row,column")],
  }));
  expect(historyBefore.parent).toHaveLength(1);
  expect(historyBefore.parent[0]).toMatchObject({ outcome: "resolved", winner_player_id: selected, winner_alias: "Offline", expires_at: result.history_expires_at });
  expect(historyBefore.calls.map(call => call.value)).toEqual(["1", "2", "3", "4"]);
  expect(historyBefore.players).toHaveLength(3);
  expect(historyBefore.cells).toHaveLength(12);
  await runInDurableObject(owner(id), (_instance, state) => {
    for (const table of ["game_recovery", "game_sessions", "game_board_cells", "game_boards", "game_calls", "game_players", "game_configuration"]) {
      expect([...state.storage.sql.exec(`SELECT * FROM ${table}`)]).toEqual([]);
    }
  });
  const retry = await post(`/api/games/${id}/winner`, input, { session: actor.session, id: command });
  expect(retry.status).toBe(200);
  expect(await retry.json()).toEqual({ result: "committed", receipt: result.receipt });
  expect(retry.headers.has("Set-Cookie")).toBe(false);
  const repeated = await post(`/api/games/${id}/winner`, { ...input, player_id: players[0].body.player_id }, { session: actor.session, id: command });
  expect(repeated.status).toBe(409);
  const after = await runInDurableObject(owner(id), (_instance, state) => ({
    parent: [...state.storage.sql.exec("SELECT * FROM game_history")],
    calls: [...state.storage.sql.exec("SELECT sequence_no,value FROM game_history_calls ORDER BY sequence_no")],
    players: [...state.storage.sql.exec("SELECT player_id,alias FROM game_history_players ORDER BY player_id")],
    cells: [...state.storage.sql.exec("SELECT * FROM game_history_board_cells ORDER BY player_id,row,column")],
  }));
  expect(after).toEqual(historyBefore);
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT game_id FROM directory_game_index WHERE game_id=?", id)]).toEqual([{ game_id: id }]);
    expect([...state.storage.sql.exec("SELECT history_expires_at FROM directory_game_index WHERE game_id=?", id)]).toEqual([{ history_expires_at: result.history_expires_at }]);
  });
  const nextGame = await post("/api/games", {}, { session: actor.session });
  expect(nextGame.status).toBe(200);
  for (const connection of connections) connection.socket.close(1000);
});

test("account Exit retires every existing account-game session without changing read-only peers or History", async () => {
  const { id, actor, players, connections } = await startedGame();
  const login = await post("/api/auth/login", { username: actor.username, password: actor.password });
  expect(login.status).toBe(200);
  const secondSession = cookie(login);
  await sync(id, secondSession, "account");
  const first = await stream(id, actor.session, "account");
  const second = await stream(id, secondSession, "account");
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET username=? WHERE account_id=?", "ExistingGameplayHost", actor.accountId));
  const reader = await enroll();
  await sync(id, reader.session, "account");
  const ended = await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session });
  expect(ended.status).toBe(200);
  const history = await runInDurableObject(owner(id), (_instance, state) => [...state.storage.sql.exec("SELECT * FROM game_history")]);
  const firstClosed = new Promise(resolve => first.socket.addEventListener("close", resolve, { once: true }));
  const secondClosed = new Promise(resolve => second.socket.addEventListener("close", resolve, { once: true }));
  const exitId = commandId();
  const response = await post(`/api/games/${id}/exit?view=account`, {}, { session: actor.session, id: exitId });
  expect(response.status).toBe(200);
  const result = await response.json();
  expect(result.result).toBe("exited");
  await Promise.all([firstClosed, secondClosed]);
  for (const session of [actor.session, secondSession]) {
    const denied = await exports.default.fetch(`${origin}/api/games/${id}/sync?view=account`, { headers: { Cookie: session } });
    expect(denied.status).toBe(401);
  }
  const retry = await post(`/api/games/${id}/exit?view=account`, {}, { session: actor.session, id: exitId });
  expect(retry.status).toBe(200);
  expect(await retry.json()).toEqual({ result: "committed", receipt: result.receipt });
  expect(retry.headers.has("Set-Cookie")).toBe(false);
  const readOnlyExit = await post(`/api/games/${id}/exit?view=account`, {}, { session: reader.session });
  expect(readOnlyExit.status).toBe(200);
  expect((await sync(id, players[0].session)).snapshot.state).toBe("cancelled");
  expect(await runInDurableObject(owner(id), (_instance, state) => [...state.storage.sql.exec("SELECT * FROM game_history")])).toEqual(history);
  for (const connection of connections) connection.socket.close(1000);
});

test("failed physical player Exit close acknowledges durable revocation and schedules prompt recovery", async () => {
  const { id, actor, players, connections } = await startedGame();
  expect((await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session })).status).toBe(200);
  await runInDurableObject(owner(id), instance => {
    instance.exitOriginalAlarm = instance.alarm.bind(instance);
    instance.alarm = async () => new Response("fixture alarm delivery held");
  });
  await runInDurableObject(owner(id), (_instance, state) => {
    const target = state.getWebSockets().find(ws => JSON.parse(ws.deserializeAttachment()).connection_id === connections[0].initial.connection_id);
    target.exitOriginalClose = target.close;
    target.close = () => { throw new Error("test-only physical Exit close outage"); };
  });
  const response = await post(`/api/games/${id}/exit?view=player`, {}, { session: players[0].session });
  expect(response.status).toBe(200);
  expect((await response.json()).result).toBe("exited");
  await runInDurableObject(owner(id), async (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT session_id FROM game_terminal_views WHERE player_id=?", players[0].body.player_id)]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE principal_id=?", players[0].body.player_id)]).toEqual([]);
    expect(await state.storage.getAlarm()).toBeLessThanOrEqual(Date.now() + 3000);
    const target = state.getWebSockets().find(ws => JSON.parse(ws.deserializeAttachment()).connection_id === connections[0].initial.connection_id);
    expect(target.readyState).toBe(1);
    target.close = target.exitOriginalClose;
    delete target.exitOriginalClose;
  });
  const closed = new Promise(resolve => connections[0].socket.addEventListener("close", resolve, { once: true }));
  await runInDurableObject(owner(id), async instance => {
    instance.alarm = instance.exitOriginalAlarm;
    delete instance.exitOriginalAlarm;
    await instance.alarm();
  });
  await closed;
  expect((await sync(id, players[1].session)).snapshot.state).toBe("cancelled");
  connections[1].socket.close(1000);
});

test("History deadline alarm purges actual owner rows and matching Directory index without clearing a newer reservation", async () => {
  const { id, actor, connections } = await startedGame();
  const ended = await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session });
  expect(ended.status).toBe(200);
  const result = await ended.json();
  const next = await post("/api/games", {}, { session: actor.session });
  expect(next.status).toBe(200);
  const nextId = (await next.json()).game.game_id;
  const closed = connections.map(connection => new Promise(resolve => connection.socket.addEventListener("close", resolve, { once: true })));
  for (const connection of connections) connection.socket.close(1000);
  await Promise.all(closed);
  const originalNow = Date.now;
  try {
    Date.now = () => result.history_expires_at;
    await runInDurableObject(owner(id), async (instance, state) => {
      await instance.alarm();
      for (const table of ["game_history", "game_history_calls", "game_history_players", "game_history_board_cells", "game_terminal_views", "game_record", "game_pending_work", "game_view_revisions"]) {
        expect([...state.storage.sql.exec(`SELECT * FROM ${table}`)]).toEqual([]);
      }
      expect(state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms).toBe(result.history_expires_at);
      expect(state.storage.sql.exec("SELECT history_expires_at FROM game_terminal_route").one().history_expires_at).toBe(result.history_expires_at);
    });
    const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
    await runInDurableObject(directory, (_instance, state) => {
      expect([...state.storage.sql.exec("SELECT game_id FROM directory_game_index WHERE game_id=?", id)]).toEqual([]);
      expect([...state.storage.sql.exec("SELECT game_id FROM directory_hosted_nonterminal_games")]).toEqual([{ game_id: nextId }]);
    });
  } finally { Date.now = originalNow; }
});

test("History account reads are immutable and survive Exit with a fresh login", async () => {
  const { id, actor, players, connections } = await startedGame();
  const host = await sync(id, actor.session, "account");
  expect((await post(`/api/games/${id}/calls/manual`, { value: "1", expected_revision: host.view_revision }, { session: actor.session })).status).toBe(200);
  expect((await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session })).status).toBe(200);
  for (const connection of connections) connection.socket.close();
  const response = await exports.default.fetch(`${origin}/api/history/${id}`, { headers: { Cookie: actor.session } });
  expect(response.status).toBe(200);
  const { history } = await response.json();
  expect(Object.keys(history).sort()).toEqual(["game_id", "game_code", "designated_host_id", "outcome", "started_at", "ended_at", "expires_at", "winner", "ordered_calls", "players"].sort());
  expect(history).toMatchObject({ game_id: id, outcome: "cancelled", winner: null, ordered_calls: ["1"] });
  expect(history.players).toHaveLength(3);
  expect(history.players.map(p => p.player_id)).toEqual(history.players.map(p => p.player_id).sort());
  for (const p of history.players) {
    expect(Object.keys(p).sort()).toEqual(["player_id", "alias", "side_length", "cells"].sort());
    expect(p.cells).toHaveLength(4);
    expect(p.cells.map(c => [c.position.row, c.position.column])).toEqual([[1,1],[1,2],[2,1],[2,2]]);
  }
  const list = await exports.default.fetch(`${origin}/api/history`, { headers: { Cookie: actor.session } });
  expect(list.status).toBe(200);
  expect(await list.json()).toEqual({ games: [{ game_id: id, game_code: history.game_code, designated_host_id: history.designated_host_id, outcome: history.outcome, started_at: history.started_at, ended_at: history.ended_at, expires_at: history.expires_at, winner: null }], next_cursor: null });
  expect((await exports.default.fetch(`${origin}/api/history`, { headers: { Cookie: players[0].session } })).status).toBe(401);
  expect((await post(`/api/games/${id}/exit?view=account`, {}, { session: actor.session })).status).toBe(200);
  const login = await post("/api/auth/login", { username: actor.username, password: actor.password }, { caller: "192.0.2.77" });
  expect(login.status).toBe(200);
  const fresh = cookie(login);
  const before = await runInDurableObject(owner(id), (_instance, state) => JSON.stringify([...state.storage.sql.exec("SELECT * FROM game_history"), ...state.storage.sql.exec("SELECT * FROM game_connection_grants"), ...state.storage.sql.exec("SELECT * FROM game_view_revisions")]));
  const again = await exports.default.fetch(`${origin}/api/history/${id}`, { headers: { Cookie: fresh } });
  expect(again.status).toBe(200);
  expect(await again.json()).toEqual({ history });
  const after = await runInDurableObject(owner(id), (_instance, state) => JSON.stringify([...state.storage.sql.exec("SELECT * FROM game_history"), ...state.storage.sql.exec("SELECT * FROM game_connection_grants"), ...state.storage.sql.exec("SELECT * FROM game_view_revisions")]));
  expect(after === before).toBe(true);
  for (const r of [response, list, again]) {
    expect(r.headers.get("cache-control")).toBe("no-store");
    expect(r.headers.get("referrer-policy")).toBe("no-referrer");
    expect(r.headers.get("x-content-type-options")).toBe("nosniff");
    expect(r.headers.get("set-cookie")).toBeNull();
  }
});

async function historyGet(path, session, headers = {}) {
  return exports.default.fetch(`${origin}${path}`, { headers: { ...(session ? { Cookie: session } : {}), ...headers } });
}
async function cancelledHistory() {
  const f = await startedGame();
  expect((await post(`/api/games/${f.id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: f.actor.session })).status).toBe(200);
  const closed = f.connections.map(c => c.socket.readyState === 3 ? Promise.resolve() : new Promise(resolve => c.socket.addEventListener("close", resolve, { once: true })));
  for (const c of f.connections) c.socket.close();
  await Promise.all(closed);
  return f;
}
test("History final Accounts await expiry is durably fenced before denial and survives reconstruction", async () => {
  const { id, actor } = await cancelledHistory();
  const expiry = await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT expires_at FROM game_history").one().expires_at);
  const originalNow = Date.now;
  let session;
  try {
    Date.now = () => expiry - 1;
    const login = await post("/api/auth/login", { username: actor.username, password: actor.password }, { caller: "192.0.2.101" });
    expect(login.status).toBe(200);
    session = cookie(login);
    await runInDurableObject(owner(id), (instance, state) => {
      instance.historyExpiryHolder = { alarm: instance.alarm.bind(instance), sync: state.storage.sync.bind(state.storage) };
      instance.alarm = async () => new Response("test-only expiry alarm held");
      instance.historyExpirySyncs = [];
      state.storage.sync = async () => {
        instance.historyExpirySyncs.push(state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);
        return instance.historyExpiryHolder.sync();
      };
    });
    await inOwner(instance => {
      instance.historyExpiryHolder = { fetch: instance.fetch.bind(instance) };
      instance.historyExpiryProofs = 0;
      instance.fetch = async request => {
        const response = await instance.historyExpiryHolder.fetch(request.clone());
        if (new URL(request.url).pathname === "/game-authority" && (await request.json()).action === "authorize") {
          instance.historyExpiryProofs++;
          expect(response.status).toBe(200);
          expect((await response.clone().json()).outcome.result).toBe("authorized");
          if (instance.historyExpiryProofs === 2) Date.now = () => expiry;
        }
        return response;
      };
    });
    const response = await owner(id).fetch("https://game.internal/history", {
      method: "POST", body: JSON.stringify({ game_id: id, token: session.split("=", 2)[1], summary: false }),
    });
    expect(response.status).toBe(200);
    expect((await response.json()).outcome).toEqual({ result: "not_found" });
    expect(await inOwner(instance => instance.historyExpiryProofs)).toBe(2);
    const fenced = await runInDurableObject(owner(id), (instance, state) => ({
      now: state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms,
      syncs: instance.historyExpirySyncs,
      retained: state.storage.sql.exec("SELECT count(*) AS n FROM game_history").one().n,
    }));
    expect(fenced.retained).toBe(1);
    expect(fenced.now).toBe(expiry);
    expect(fenced.syncs).toEqual([expiry - 1, expiry]);
    await inOwner(instance => { instance.fetch = instance.historyExpiryHolder.fetch; delete instance.historyExpiryHolder; });
    Date.now = () => expiry - 1;
    await evictDurableObject(owner(id));
    const fresh = await post("/api/auth/login", { username: actor.username, password: actor.password }, { caller: "192.0.2.102" });
    expect(fresh.status).toBe(200);
    expect((await historyGet(`/api/history/${id}`, cookie(fresh))).status).toBe(503);
    expect(await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBe(expiry);
  } finally {
    Date.now = originalNow;
    await inOwner(instance => { if (instance.historyExpiryHolder) { instance.fetch = instance.historyExpiryHolder.fetch; delete instance.historyExpiryHolder; } });
    await runInDurableObject(owner(id), (instance, state) => {
      if (instance.historyExpiryHolder) {
        instance.alarm = instance.historyExpiryHolder.alarm;
        state.storage.sync = instance.historyExpiryHolder.sync;
        delete instance.historyExpiryHolder;
      }
    });
  }
});

test.each(["detail", "list"])("History %s expiry first observed after the edge Accounts await is durably fenced", async kind => {
  const { id, actor } = await cancelledHistory();
  const expiry = await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT expires_at FROM game_history").one().expires_at);
  const originalNow = Date.now;
  try {
    Date.now = () => expiry - 1;
    const login = await post("/api/auth/login", { username: actor.username, password: actor.password }, { caller: "192.0.2.103" });
    expect(login.status).toBe(200);
    const session = cookie(login);
    await runInDurableObject(owner(id), instance => {
      instance.historyEdgeAlarm = instance.alarm.bind(instance);
      instance.alarm = async () => new Response("test-only expiry alarm held");
    });
    await inOwner(instance => {
      instance.historyEdgeHolder = { fetch: instance.fetch.bind(instance) };
      instance.historyEdgeProofs = 0;
      instance.fetch = async request => {
        const before = Date.now;
        // Accounts is a distinct trusted clock; every proof is obtained from its real service.
        Date.now = () => expiry - 1;
        let response;
        try { response = await instance.historyEdgeHolder.fetch(request.clone()); }
        finally { Date.now = before; }
        if (new URL(request.url).pathname === "/game-authority" && (await request.json()).action === "authorize") {
          instance.historyEdgeProofs++;
          expect((await response.clone().json()).outcome.result).toBe("authorized");
          if (instance.historyEdgeProofs === 4) Date.now = () => expiry;
        }
        return response;
      };
    });
    const response = await historyGet(kind === "detail" ? `/api/history/${id}` : "/api/history", session);
    expect(response.status).toBe(kind === "detail" ? 404 : 200);
    if (kind === "list") expect((await response.json()).games).toEqual([]);
    expect(await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBe(expiry);
    await inOwner(instance => { instance.fetch = instance.historyEdgeHolder.fetch; delete instance.historyEdgeHolder; });
    Date.now = () => expiry - 1;
    await evictDurableObject(owner(id));
    const fresh = await post("/api/auth/login", { username: actor.username, password: actor.password }, { caller: "192.0.2.104" });
    expect(fresh.status).toBe(200);
    expect((await historyGet(`/api/history/${id}`, cookie(fresh))).status).toBe(503);
  } finally {
    Date.now = originalNow;
    await inOwner(instance => { if (instance.historyEdgeHolder) { instance.fetch = instance.historyEdgeHolder.fetch; delete instance.historyEdgeHolder; } });
    await runInDurableObject(owner(id), instance => { if (instance.historyEdgeAlarm) { instance.alarm = instance.historyEdgeAlarm; delete instance.historyEdgeAlarm; } });
  }
});

test.each(["ignore", "abort", "clock_readback", "floor_readback", "sync"])("History final expiry %s failure is unavailable rather than a nondurable NotFound", async fault => {
  const { id, actor } = await cancelledHistory();
  const originalNow = Date.now;
  const expiry = await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT expires_at FROM game_history").one().expires_at);
  try {
    Date.now = () => expiry - 1;
    const login = await post("/api/auth/login", { username: actor.username, password: actor.password }, { caller: "192.0.2.105" });
    expect(login.status).toBe(200);
    const session = cookie(login);
    expect((await historyGet(`/api/history/${id}`, session)).status).toBe(200);
    const before = await runInDurableObject(owner(id), (instance, state) => {
      instance.historyFaultHolder = { alarm: instance.alarm.bind(instance), sync: state.storage.sync.bind(state.storage) };
      instance.alarm = async () => new Response("test-only expiry alarm held");
      if (fault === "ignore") state.storage.sql.exec("CREATE TRIGGER history_expiry_fault BEFORE UPDATE ON game_metadata WHEN NEW.last_observed_ms>OLD.last_observed_ms BEGIN SELECT RAISE(IGNORE); END");
      if (fault === "abort") state.storage.sql.exec("CREATE TRIGGER history_expiry_fault BEFORE UPDATE ON game_metadata WHEN NEW.last_observed_ms>OLD.last_observed_ms BEGIN SELECT RAISE(ABORT,'test-only expiry failure'); END");
      if (fault === "clock_readback") state.storage.sql.exec("CREATE TRIGGER history_expiry_fault AFTER UPDATE ON game_metadata WHEN NEW.last_observed_ms>OLD.last_observed_ms BEGIN UPDATE game_metadata SET last_observed_ms=OLD.last_observed_ms; END");
      if (fault === "floor_readback") state.storage.sql.exec("CREATE TRIGGER history_expiry_fault AFTER UPDATE ON game_metadata WHEN NEW.last_observed_ms>OLD.last_observed_ms BEGIN UPDATE game_metadata SET command_floor_ms=OLD.command_floor_ms-1; END");
      if (fault === "sync") state.storage.sync = async () => {
        if (state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms === expiry) throw new Error("test-only final fence sync failure");
        return instance.historyFaultHolder.sync();
      };
      return JSON.stringify([...state.storage.sql.exec("SELECT * FROM game_metadata"), ...state.storage.sql.exec("SELECT * FROM game_history"), ...state.storage.sql.exec("SELECT * FROM game_connection_grants"), ...state.storage.sql.exec("SELECT * FROM game_view_revisions")]);
    });
    await inOwner(instance => {
      instance.historyFaultHolder = { fetch: instance.fetch.bind(instance) };
      instance.historyFaultProofs = 0;
      instance.fetch = async request => {
        const response = await instance.historyFaultHolder.fetch(request.clone());
        if (new URL(request.url).pathname === "/game-authority" && (await request.json()).action === "authorize") {
          instance.historyFaultProofs++;
          expect((await response.clone().json()).outcome.result).toBe("authorized");
          if (instance.historyFaultProofs === 2) Date.now = () => expiry;
        }
        return response;
      };
    });
    const response = await owner(id).fetch("https://game.internal/history", {
      method: "POST", body: JSON.stringify({ game_id: id, token: session.split("=", 2)[1], summary: false }),
    });
    expect((await response.json()).outcome).toEqual({ result: "unavailable" });
    expect(await inOwner(instance => instance.historyFaultProofs)).toBe(2);
    const after = await runInDurableObject(owner(id), (_instance, state) => JSON.stringify([...state.storage.sql.exec("SELECT * FROM game_metadata"), ...state.storage.sql.exec("SELECT * FROM game_history"), ...state.storage.sql.exec("SELECT * FROM game_connection_grants"), ...state.storage.sql.exec("SELECT * FROM game_view_revisions")]));
    if (fault !== "sync") expect(after === before).toBe(true);
    else expect(await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBe(expiry);
  } finally {
    Date.now = originalNow;
    await inOwner(instance => { if (instance.historyFaultHolder) { instance.fetch = instance.historyFaultHolder.fetch; delete instance.historyFaultHolder; } });
    await runInDurableObject(owner(id), (instance, state) => {
      if (instance.historyFaultHolder) {
        instance.alarm = instance.historyFaultHolder.alarm;
        state.storage.sync = instance.historyFaultHolder.sync;
        delete instance.historyFaultHolder;
      }
      if (fault !== "sync") state.storage.sql.exec("DROP TRIGGER IF EXISTS history_expiry_fault");
    });
  }
});

test.each([
  ["detail", "revoked"], ["list", "revoked"],
  ["detail", "sync"], ["list", "sync"],
  ["detail", "rollback"], ["list", "rollback"],
])("History %s edge expiry confirmation fails closed on %s without post-await disclosure", async (kind, fault) => {
  const { id, actor } = await cancelledHistory();
  const expiry = await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT expires_at FROM game_history").one().expires_at);
  const originalNow = Date.now;
  try {
    Date.now = () => expiry - 1;
    const login = await post("/api/auth/login", { username: actor.username, password: actor.password }, { caller: "192.0.2.106" });
    expect(login.status).toBe(200);
    const session = cookie(login);
    await runInDurableObject(owner(id), (instance, state) => {
      instance.historyConfirmHolder = { fetch: instance.fetch.bind(instance), alarm: instance.alarm.bind(instance), sync: state.storage.sync.bind(state.storage) };
      instance.alarm = async () => new Response("test-only expiry alarm held");
      instance.historyConfirmRequests = 0;
      state.storage.sync = async () => {
        if (fault === "sync" && state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms === expiry) throw new Error("test-only expiry confirmation sync failure");
        return instance.historyConfirmHolder.sync();
      };
      instance.fetch = async request => {
        if (new URL(request.url).pathname === "/history") {
          instance.historyConfirmRequests++;
          if (fault === "rollback" && instance.historyConfirmRequests === 2) Date.now = () => expiry - 1;
        }
        const response = await instance.historyConfirmHolder.fetch(request);
        if (fault === "revoked" && instance.historyConfirmRequests === 2) await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET disabled_at=? WHERE account_id=?", expiry - 1, actor.accountId));
        return response;
      };
    });
    await inOwner(instance => {
      instance.historyConfirmHolder = { fetch: instance.fetch.bind(instance) };
      instance.historyConfirmProofs = 0;
      instance.fetch = async request => {
        const before = Date.now;
        Date.now = () => expiry - 1;
        let response;
        try { response = await instance.historyConfirmHolder.fetch(request.clone()); }
        finally { Date.now = before; }
        if (new URL(request.url).pathname === "/game-authority" && (await request.json()).action === "authorize") {
          instance.historyConfirmProofs++;
          if (instance.historyConfirmProofs === 4) Date.now = () => expiry;
        }
        return response;
      };
    });
    const response = await historyGet(kind === "detail" ? `/api/history/${id}` : "/api/history", session);
    expect(response.status).toBe(fault === "revoked" ? 401 : 503);
    expect(Object.keys(await response.json())).toEqual(["error"]);
    expect(response.headers.has("Set-Cookie")).toBe(false);
    expect(await runInDurableObject(owner(id), instance => instance.historyConfirmRequests)).toBe(2);
    if (fault === "revoked") expect(await inOwner(instance => instance.historyConfirmProofs)).toBe(6);
  } finally {
    Date.now = originalNow;
    await inOwner(instance => { if (instance.historyConfirmHolder) { instance.fetch = instance.historyConfirmHolder.fetch; delete instance.historyConfirmHolder; } });
    await runInDurableObject(owner(id), (instance, state) => {
      if (instance.historyConfirmHolder) {
        instance.fetch = instance.historyConfirmHolder.fetch;
        instance.alarm = instance.historyConfirmHolder.alarm;
        state.storage.sync = instance.historyConfirmHolder.sync;
        delete instance.historyConfirmHolder;
      }
    });
  }
});

test("History cross-host and admin access revalidates live Accounts instead of cached metadata", async () => {
  const { id, actor } = await cancelledHistory();
  const peer = await enroll("OtherHistoryHost");
  const pending=await seedPending("RestrictedHistoryAccount");
  const redeemed=await post("/api/auth/enrollment/redeem",{enrollment_token:pending.token});
  expect(redeemed.status).toBe(200);
  for(const path of ["/api/history",`/api/history/${id}`])expect((await historyGet(path,cookie(redeemed))).status).toBe(401);
  for (const path of ["/api/history", `/api/history/${id}`]) {
    expect((await historyGet(path, peer.session)).status).toBe(200);
    expect((await historyGet(path)).status).toBe(401);
    expect((await historyGet(path, "__Host-brews_session=malformed")).status).toBe(401);
    expect((await historyGet(path, actor.session, { Origin: "https://other.invalid" })).status).toBe(403);
    expect((await historyGet(path, actor.session, { Authorization: "Bearer" })).status).toBe(403);
    expect((await historyGet(path, actor.session, { "Idempotency-Key": commandId() })).status).toBe(400);
    expect((await exports.default.fetch(`http://localhost:8787${path}`, { headers: { Cookie: peer.session } })).status).toBe(403);
  }
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", peer.accountId));
  expect((await historyGet(`/api/history/${id}`, peer.session)).status).toBe(200);
  for (const mutation of ["disabled_at", "credential_epoch", "status", "revoked_at", "expires_at", "deleted"]) {
    await inOwner((_instance, state) => {
      if (mutation === "disabled_at") state.storage.sql.exec("UPDATE accounts SET disabled_at=? WHERE account_id=?", Date.now(), peer.accountId);
      if (mutation === "credential_epoch") state.storage.sql.exec("UPDATE accounts SET credential_epoch=credential_epoch+1 WHERE account_id=?", peer.accountId);
      if (mutation === "status") state.storage.sql.exec("UPDATE accounts SET status='reset_required' WHERE account_id=?", peer.accountId);
      if (mutation === "revoked_at") state.storage.sql.exec("UPDATE account_sessions SET revoked_at=? WHERE account_id=?", Date.now(), peer.accountId);
      if (mutation === "expires_at") state.storage.sql.exec("UPDATE account_sessions SET issued_at=issued_at-86400000,expires_at=expires_at-86400000 WHERE account_id=?", peer.accountId);

    });
    if (mutation === "deleted") {
      await inOwner((_instance,state)=>state.storage.sql.exec("UPDATE accounts SET role='host' WHERE account_id=?",peer.accountId));
      const removed=await exports.default.fetch(`${origin}/_dev/commands`, {method:"POST",headers:{"Content-Type":"application/json",Authorization:["Bearer",env.DEV_CLI_KEY].join(" "),"Idempotency-Key":commandId()},body:JSON.stringify({operation:"delete_account",account_id:peer.accountId})});
      expect(removed.status).toBe(200);
      expect(await inOwner((_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM accounts WHERE account_id=?",peer.accountId).one().n)).toBe(0);
    }
    for (const path of ["/api/history", `/api/history/${id}`]) expect((await historyGet(path, peer.session)).status).toBe(401);
    if (mutation !== "deleted") await inOwner((_instance, state) => {
      state.storage.sql.exec("UPDATE accounts SET disabled_at=NULL,status='verified',credential_epoch=1 WHERE account_id=?", peer.accountId);
    });
    if (mutation !== "deleted") {
      const fresh = await post("/api/auth/login", { username: peer.username, password: peer.password }, { caller: "192.0.2.88" });
      expect(fresh.status).toBe(200);
      peer.session = cookie(fresh);
      expect((await historyGet(`/api/history/${id}`, peer.session)).status).toBe(200);
    }
  }
});
test("History rejects malformed authoritative owner replies without empty-success fallbacks", async () => {
  const { id, actor } = await cancelledHistory();
  const game = owner(id);
  for (const fault of ["wrong_id", "missing_nullable", "duplicate", "unknown", "sequence", "wrong_expiry", "winner", "not_found", "bad_cells", "exception"]) {
    await runInDurableObject(game, instance => {
      instance.historyFaultOriginal = instance.fetch.bind(instance);
      instance.fetch = async request => {
        if (new URL(request.url).pathname !== "/history") return instance.historyFaultOriginal(request);
        if (fault === "exception") throw new Error("test-only History owner unavailability");
        const response = await instance.historyFaultOriginal(request);
        const reply = await response.json();
        const h = reply.outcome.history ?? reply.outcome.summary;
        if (fault === "wrong_id") reply.game_id = commandId();
        if (fault === "missing_nullable") delete h.winner;
        if (fault === "unknown") h.presence = [];
        if (fault === "sequence") reply.outcome = ["detail", h];
        if (fault === "wrong_expiry") h.expires_at++;
        if (fault === "winner") h.winner = { player_id: commandId(), alias: "Unjustified" };
        if (fault === "not_found") reply.outcome = { result: "not_found" };
        if (fault === "bad_cells" && h.players) h.players[0].cells[0].position = [1,1];
        let encoded = JSON.stringify(reply);
        if (fault === "duplicate") encoded = encoded.replace('"winner":null', '"winner":null,"winner":null');
        return new Response(encoded, { headers: { "Content-Type": "application/json" } });
      };
    });
    try {
      const detail = await historyGet(`/api/history/${id}`, actor.session);
      expect(detail.status).toBe(fault === "not_found" ? 404 : 503);
      if (fault !== "bad_cells") expect((await historyGet("/api/history", actor.session)).status).toBe(503);
    } finally {
      await runInDurableObject(game, instance => { instance.fetch = instance.historyFaultOriginal; delete instance.historyFaultOriginal; });
    }
  }
  expect((await historyGet(`/api/history/${id}`, actor.session)).status).toBe(200);
});
test("History corruption and ignored durable clock writes fail closed in actual Workerd SQL", async () => {
  const { id, actor } = await cancelledHistory();
  await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("UPDATE game_history_board_cells SET is_matched=1 WHERE row=1 AND column=1"));
  for (const path of ["/api/history", `/api/history/${id}`]) expect((await historyGet(path, actor.session)).status).toBe(503);
  await runInDurableObject(owner(id), (_instance, state) => {
    state.storage.sql.exec("UPDATE game_history_board_cells SET is_matched=0");
    state.storage.sql.exec("CREATE TRIGGER ignored_history_clock BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END");
  });
  expect((await historyGet(`/api/history/${id}`, actor.session)).status).toBe(503);
  await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("DROP TRIGGER ignored_history_clock"));
  expect((await historyGet(`/api/history/${id}`, actor.session)).status).toBe(200);
});

test("History resolved winner, pagination and beyond-first-page expiry use original owners", async () => {
  const games=[];let actor;
  for(let i=0;i<3;i++) {
    const f=await startedGame(200,actor);actor=f.actor;
    if(i===2) {
      for(const value of ["1","2","3","4"]) {
        const host=await sync(f.id,actor.session,"account");
        expect((await post(`/api/games/${f.id}/calls/manual`,{value,expected_revision:host.view_revision},{session:actor.session})).status).toBe(200);
      }
      const host=await sync(f.id,actor.session,"account");
      expect((await post(`/api/games/${f.id}/winner`,{player_id:host.snapshot.players[0].player_id,expected_revision:host.view_revision},{session:actor.session})).status).toBe(200);
    } else expect((await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"in_progress"},{session:actor.session})).status).toBe(200);
    for(const c of f.connections)c.socket.close();
    const response=await historyGet(`/api/history/${f.id}`,actor.session);expect(response.status).toBe(200);games.push((await response.json()).history);
  }
  const expected=games.toSorted((a,b)=>b.ended_at-a.ended_at || b.game_id.localeCompare(a.game_id));
  const seen=[];let cursor;
  do {
    const response=await historyGet(`/api/history?limit=1${cursor ? `&cursor=${cursor}`:""}`,actor.session);expect(response.status).toBe(200);
    const page=await response.json();expect(Object.keys(page).sort()).toEqual(["games","next_cursor"]);seen.push(...page.games.map(g=>g.game_id));cursor=page.next_cursor;
  } while(cursor);
  expect(seen).toEqual(expected.map(g=>g.game_id));
  const resolved=await historyGet("/api/history?outcome=resolved&limit=50",actor.session);expect(resolved.status).toBe(200);const body=await resolved.json();expect(body.games).toHaveLength(1);expect(body.games[0].winner).toEqual(games[2].winner);expect(games[2].ordered_calls).toEqual(["1","2","3","4"]);
  const full=await historyGet("/api/history",actor.session);expect((await full.json()).games).toHaveLength(3);
  const first=await historyGet("/api/history?limit=1",actor.session);const next=(await first.json()).next_cursor;
  const originalNow=Date.now;
  try {
    Date.now=()=>games[0].expires_at-1;
    const login=await post("/api/auth/login",{username:actor.username,password:actor.password},{caller:"192.0.2.99"});expect(login.status).toBe(200);const fresh=cookie(login);
    expect((await historyGet(`/api/history/${games[0].game_id}`,fresh)).status).toBe(200);
    Date.now=()=>games[0].expires_at;
    expect((await historyGet(`/api/history/${games[0].game_id}`,fresh)).status).toBe(404);
    const second=await historyGet(`/api/history?limit=1&cursor=${next}`,fresh);expect(second.status).toBe(200);const page=await second.json();expect(page.games.map(g=>g.game_id)).toEqual([expected[1].game_id]);expect(page.next_cursor).toBeNull();
  } finally {Date.now=originalNow;}
},60000);



test("History rechecks Accounts after owner awaits and never returns a disabled account snapshot", async () => {
  const {id,actor}=await cancelledHistory();
  await runInDurableObject(owner(id),instance=>{
    instance.historyAwaitOriginal=instance.fetch.bind(instance);
    instance.fetch=async request=>{
      const response=await instance.historyAwaitOriginal(request);
      if(new URL(request.url).pathname==="/history") await inOwner((_instance,state)=>state.storage.sql.exec("UPDATE accounts SET disabled_at=? WHERE account_id=?",Date.now(),actor.accountId));
      return response;
    };
  });
  try {
    for(const path of [`/api/history/${id}`,"/api/history"]) {
      await inOwner((_instance,state)=>state.storage.sql.exec("UPDATE accounts SET disabled_at=NULL WHERE account_id=?",actor.accountId));
      expect((await historyGet(path,actor.session)).status).toBe(401);
    }
  }
  finally {await runInDurableObject(owner(id),instance=>{instance.fetch=instance.historyAwaitOriginal;delete instance.historyAwaitOriginal;});}
});
test("History remains readable while a genuine terminal Directory ACK is pending", async () => {
  const f=await startedGame();
  const directory=env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory,instance=>{
    instance.historyAckOriginal=instance.fetch.bind(instance);
    instance.fetch=async request=>{
      const response=await instance.historyAckOriginal(request.clone());
      if(new URL(request.url).pathname==="/games" && (await request.json()).action==="release") throw new Error("test-only lost genuine terminal ACK");
      return response;
    };
  });
  try {
    expect((await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"in_progress"},{session:f.actor.session})).status).toBe(202);
    expect(await runInDurableObject(owner(f.id),(_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM game_pending_work WHERE kind='release'").one().n)).toBe(1);
    expect((await historyGet(`/api/history/${f.id}`,f.actor.session)).status).toBe(200);
    const response=await historyGet("/api/history",f.actor.session);expect(response.status).toBe(200);expect((await response.json()).games.map(g=>g.game_id)).toEqual([f.id]);
    expect(await runInDurableObject(owner(f.id),(_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM game_pending_work WHERE kind='release'").one().n)).toBe(1);
  } finally {
    await runInDurableObject(directory,instance=>{instance.fetch=instance.historyAckOriginal;delete instance.historyAckOriginal;});
    for(const c of f.connections)c.socket.close();
  }
});
