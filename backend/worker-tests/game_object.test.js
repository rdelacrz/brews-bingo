import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId, inOwner } from "./fixtures.js";

beforeEach(async () => {
  // Reset every owner and native registry; account-only deletion leaves due Game alarms.
  await reset();
  await resetStorage();
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory, async (_instance, state) => state.storage.deleteAll());
});

test("an empty Game owner alarm neither panics nor invents a Game", async () => {
  const owner = env.GAMES.get(env.GAMES.idFromName(commandId()));
  await runInDurableObject(owner, async (instance, state) => {
    await instance.alarm();
    expect([...state.storage.sql.exec("SELECT game_id FROM game_record")]).toEqual([]);
    expect(await state.storage.getAlarm()).toBeNull();
  });
});

test("recovery probes actual absent known owner without initializing defaults", async () => {
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  const claimed = await directory.fetch("https://directory.internal/games", {
    method: "POST", body: JSON.stringify({ action: "claim", account_id: commandId(), command_id: commandId(), fingerprint: Array(32).fill(9) }),
  });
  expect(claimed.status).toBe(200);
  const { outcome: { work } } = await claimed.json();
  const request = { work, game_code: null };
  const owner = env.GAMES.get(env.GAMES.idFromName(work.game_id));
  const response = await owner.fetch("https://game.internal/recovery", { method: "POST", body: JSON.stringify(request) });
  expect(response.status).toBe(200);
  expect(await response.json()).toEqual({ request, outcome: { result: "absent" } });
  await runInDurableObject(owner, (_instance, state) => expect([...state.storage.sql.exec("SELECT game_id FROM game_record")]).toEqual([]));
});

test("account close proves actual absent target and echoes all identity fields", async () => {
  const target = { operation_id: commandId(), account_id: commandId(), session_id: commandId(), game_id: commandId(), connection_id: commandId(), epoch: 0, expires: Date.now() + 1000, created_at: Date.now() };
  const request = { caller: "accounts", action: "close_account", target };
  const owner = env.GAMES.get(env.GAMES.idFromName(target.game_id));
  const response = await owner.fetch("https://game.internal/close-account", { method: "POST", body: JSON.stringify(request) });
  expect(response.status).toBe(200);
  expect(await response.json()).toEqual({ result: "closed", request });
  await runInDurableObject(owner, (_instance, state) => {
    expect(state.getWebSockets()).toEqual([]);
    expect([...state.storage.sql.exec("SELECT * FROM game_current_connections")]).toEqual([]);
  });
});

async function newGame(configuration = {}) {
  const actor = await enroll();
  const created = await post("/api/games", configuration, { session: actor.session });
  expect(created.status).toBe(200);
  const game = await created.json();
  expect(game.result).toBe("created");
  return { actor, game };
}

test("Game owner opens and durably publishes a lobby", async () => {
  const { actor, game } = await newGame();
  const opened = await post(`/api/games/${game.game.game_id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(opened.status).toBe(200);
  const lobby = await opened.json();
  expect(lobby.result).toBe("lobby_opened");
  expect(lobby.state).toBe("awaiting_players");
  expect(lobby.game_code).toMatch(/^[A-Z0-9]{8}$/);
  const owner = env.GAMES.get(env.GAMES.idFromName(game.game.game_id));
  await runInDurableObject(owner, async (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT phase FROM game_pending_work")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT state,game_code FROM game_record")][0]).toMatchObject({ state: "awaiting_players", game_code: lobby.game_code });
    expect(await state.storage.getAlarm()).not.toBeNull();
  });
});

async function openLobby(configuration = {}) {
  const fixture = await newGame(configuration);
  const opened = await post(`/api/games/${fixture.game.game.game_id}/lobby`, { expected_revision: 0 }, { session: fixture.actor.session });
  expect(opened.status).toBe(200);
  return { ...fixture, lobby: await opened.json(), id: fixture.game.game.game_id };
}
async function join(id, code, alias) {
  const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
  expect(context.status).toBe(200);
  const admitted = await post(`/api/games/${id}/players`, { game_code: code, alias }, { session: cookie(context) });
  expect(admitted.status).toBe(200);
  return { session: cookie(admitted), body: await admitted.json() };
}
async function sync(id, session, view = "player", known) {
  const suffix = known === undefined ? "" : `&known_revision=${known}`;
  const response = await exports.default.fetch(`${origin}/api/games/${id}/sync?view=${view}${suffix}`, { headers: { Cookie: session } });
  expect(response.status).toBe(200);
  return response.json();
}
function messages(socket) {
  const queue = [];
  const readers = [];
  socket.addEventListener("message", event => {
    const frame = JSON.parse(event.data);
    if (readers.length) readers.shift()(frame); else queue.push(frame);
  });
  return () => queue.length ? Promise.resolve(queue.shift()) : new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("snapshot timeout")), 2000);
    readers.push(frame => { clearTimeout(timeout); resolve(frame); });
  });
}
async function stream(id, session, view = "player") {
  const response = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=${view}`, { headers: { Cookie: session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
  if (response.status !== 101) {
    const diagnostic = await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (_instance, state) => ({
      sockets: state.getWebSockets().map(ws => ws.readyState),
      current: [...state.storage.sql.exec("SELECT viewer_kind FROM game_current_connections")],
      deliveries: [...state.storage.sql.exec("SELECT view_revision,frame_bytes FROM game_delivery_pending")],
    }));
    throw new Error(`upgrade ${response.status}: ${await response.text()} ${JSON.stringify(diagnostic)}`);
  }
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  const next = messages(socket);
  socket.accept();
  const initial = await next();
  return { socket, next, initial };
}
function ack(connection, frame = connection.initial) {
  connection.socket.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision }));
}

test("two genuine hibernating player streams permit atomic public Start", async () => {
  const { actor, lobby, id } = await openLobby();
  const alice = await join(id, lobby.game_code, "Alice");
  const bob = await join(id, lobby.game_code, "Bob");
  const offline = await join(id, lobby.game_code, "Charlie");
  const before = await sync(id, alice.session);
  expect(before.snapshot.role).toBe("player");
  expect(before.snapshot.board).toBeUndefined();
  const first = await stream(id, alice.session);
  const second = await stream(id, bob.session);
  ack(first); ack(second);
  const host = await sync(id, actor.session, "account");
  expect(host.snapshot.connected_player_count).toBe(2);
  const startCommand = commandId();
  const started = await post(`/api/games/${id}/start`, { expected_revision: host.view_revision }, { session: actor.session, id: startCommand });
  expect(started.status).toBe(200);
  const startResult = await started.json();
  expect(startResult.result).toBe("started");
  const a = await first.next(); const b = await second.next();
  expect(a.view.state).toBe("in_progress");
  expect(a.view.player_id).toBe(alice.body.player_id);
  expect(b.view.player_id).toBe(bob.body.player_id);
  expect(a.view.board.cells).toHaveLength(25);
  expect(a.view.players).toBeUndefined();
  expect(JSON.stringify(a.view.board.cells)).not.toBe(JSON.stringify(b.view.board.cells));
  expect(a.delivery_id).toMatch(/^[A-Za-z0-9_-]{43}$/);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, async (_instance, state) => {
    expect(state.getWebSockets().filter(ws => ws.readyState === 1)).toHaveLength(2);
    const attachments = state.getWebSockets().map(ws => ws.deserializeAttachment());
    for (const attachment of attachments) {
      expect(typeof attachment).toBe("string");
      expect(new TextEncoder().encode(attachment).length).toBeLessThanOrEqual(512);
      expect(attachment).not.toContain(alice.session.split("=")[1]);
    }
    expect([...state.storage.sql.exec("SELECT player_id FROM game_boards")]).toHaveLength(3);
    expect([...state.storage.sql.exec("SELECT operation_id FROM game_pending_work")]).toEqual([]);
  });
  const boards = await runInDurableObject(owner, (_instance, state) => [...state.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")]);
  const repeated = await post(`/api/games/${id}/start`, { expected_revision: host.view_revision }, { session: actor.session, id: startCommand });
  expect(repeated.status).toBe(200);
  expect(await repeated.json()).toEqual({ result: "committed", receipt: startResult.receipt });
  expect(repeated.headers.has("Set-Cookie")).toBe(false);
  expect(await runInDurableObject(owner, (_instance, state) => [...state.storage.sql.exec("SELECT * FROM game_board_cells ORDER BY player_id,row,column")])).toEqual(boards);
  expect((await sync(id, offline.session)).snapshot.board.cells).toHaveLength(25);
  expect(Object.keys(a).at(-1)).toBe("delivery_id");
  ack(first, a); ack(second, b);
  first.socket.close(1000); second.socket.close(1000);
});

test("a repeated player stream supersedes its predecessor without counting two players", async () => {
  const { actor, lobby, id } = await openLobby();
  const alice = await join(id, lobby.game_code, "Alice");
  const original = await stream(id, alice.session);
  const closed = new Promise(resolve => original.socket.addEventListener("close", resolve, { once: true }));
  const replacement = await stream(id, alice.session);
  await closed;
  expect(replacement.initial.connection_id).not.toBe(original.initial.connection_id);
  const host = await sync(id, actor.session, "account");
  expect(host.snapshot.connected_player_count).toBe(1);
  const denied = await post(`/api/games/${id}/start`, { expected_revision: host.view_revision }, { session: actor.session });
  expect(denied.status).toBe(409);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT player_id FROM game_boards")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE viewer_kind='player'")]).toEqual([{ connection_id: replacement.initial.connection_id }]);
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_delivery_connections")]).toEqual([{ connection_id: replacement.initial.connection_id }]);
  });
  replacement.socket.close(1000);
});

test("account close holds uncertainty on failed SDK close and preserves the exact ledger", async () => {
  const { actor, id } = await openLobby();
  const host = await stream(id, actor.session, "account");
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  const hint = await runInDurableObject(owner, (_instance, state) => {
    const ws = state.getWebSockets()[0];
    const hint = JSON.parse(ws.deserializeAttachment());
    ws.originalClose = ws.close;
    ws.close = () => { throw new Error("test-only SDK close failure"); };
    return hint;
  });
  const request = { caller: "accounts", action: "close_account", target: { operation_id: commandId(), account_id: actor.accountId, session_id: hint.session_id, game_id: id, connection_id: hint.connection_id, epoch: hint.authentication_epoch, expires: hint.session_expires_at, created_at: Date.now() } };
  const close = () => owner.fetch("https://game.internal/close-account", { method: "POST", body: JSON.stringify(request) });
  const pending = await close();
  expect(pending.status).toBe(200);
  expect(await pending.json()).toEqual({ result: "unavailable", request });
  await runInDurableObject(owner, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE viewer_kind='account'")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toHaveLength(1);
    const ws = state.getWebSockets()[0];
    expect(ws.readyState).toBe(1);
    ws.close = ws.originalClose;
    delete ws.originalClose;
  });
  const closed = new Promise(resolve => host.socket.addEventListener("close", resolve, { once: true }));
  await close();
  await closed;
  const retry = await close();
  expect(await retry.json()).toEqual({ result: "closed", request });
  await runInDurableObject(owner, (_instance, state) => {
    expect(state.getWebSockets().filter(ws => ws.readyState !== 3)).toEqual([]);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toEqual([]);
  });
});

function closed(socket, timeoutMs = 2000) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("close timeout")), timeoutMs);
    socket.addEventListener("close", event => { clearTimeout(timeout); resolve(event); }, { once: true });
  });
}

test.each(["INSERT", "DELETE"])("delivery %s storage failure closes a genuine current stream as unavailable and retains credit", async operation => {
  const { lobby, id } = await openLobby();
  const alice = await join(id, lobby.game_code, "Alice");
  const connection = await stream(id, alice.session);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  const ended = closed(connection.socket);
  await runInDurableObject(owner, (_instance, state) => {
    state.storage.sql.exec(`CREATE TRIGGER delivery_fault BEFORE ${operation} ON game_delivery_pending BEGIN SELECT RAISE(ABORT,'test storage outage'); END`);
    if (operation === "INSERT") state.storage.sql.exec("CREATE TRIGGER retain_credit BEFORE DELETE ON game_delivery_pending BEGIN SELECT RAISE(ABORT,'test retirement outage'); END");
  });
  if (operation === "DELETE") ack(connection);
  else await runInDurableObject(owner, async (instance, state) => {
    state.storage.sql.exec("UPDATE game_view_revisions SET revision=revision+1");
    await instance.alarm().catch(() => {});
  });
  expect((await ended).code).toBe(1013);
  await runInDurableObject(owner, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toEqual([{ delivery_id: connection.initial.delivery_id }]);
    state.storage.sql.exec("DROP TRIGGER delivery_fault");
    if (operation === "INSERT") state.storage.sql.exec("DROP TRIGGER retain_credit");
  });
});

test("alarm removes expired orphan credit after actual close retirement failed without touching live credit", async () => {
  const { lobby, id } = await openLobby();
  const alice = await join(id, lobby.game_code, "Alice");
  const bob = await join(id, lobby.game_code, "Bob");
  const first = await stream(id, alice.session);
  const live = await stream(id, bob.session);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, (instance, state) => {
    instance.testOriginalClose = instance.webSocketClose.bind(instance);
    instance.webSocketClose = async (...args) => {
      try { await instance.testOriginalClose(...args); } catch { instance.testRetirementFailed = true; }
    };
    state.storage.sql.exec("CREATE TRIGGER failed_retirement BEFORE DELETE ON game_delivery_pending BEGIN SELECT RAISE(ABORT,'retirement unavailable'); END");
  });
  const ended = closed(first.socket);
  first.socket.close(1000);
  await ended;
  await expect.poll(() => runInDurableObject(owner, instance => instance.testRetirementFailed === true)).toBe(true);
  await runInDurableObject(owner, async (instance, state) => {
    expect(state.getWebSockets().some(ws => JSON.parse(ws.deserializeAttachment()).connection_id === first.initial.connection_id && ws.readyState !== 3)).toBe(false);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending WHERE connection_id=?", first.initial.connection_id)]).toHaveLength(1);
    state.storage.sql.exec("DROP TRIGGER failed_retirement");
    state.storage.sql.exec("UPDATE game_delivery_connections SET expires_at=? WHERE connection_id=?", Date.now() - 1, first.initial.connection_id);
    instance.webSocketClose = instance.testOriginalClose;
    await instance.alarm();
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_delivery_connections WHERE connection_id=?", first.initial.connection_id)]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending WHERE connection_id=?", first.initial.connection_id)]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending WHERE connection_id=?", live.initial.connection_id)]).toEqual([{ delivery_id: live.initial.delivery_id }]);
    expect(await state.storage.getAlarm()).toBeGreaterThan(Date.now() + 1000);
  });
  live.socket.close(1000);
});

async function holdConnectionProofs(skip = 0) {
  await inOwner(instance => {
    instance.testOriginalFetch = instance.fetch.bind(instance);
    instance.testHeld = [];
    instance.testProofSkip = skip;
    instance.testHolding = true;
    instance.fetch = async request => {
      const message = new URL(request.url).pathname === "/game-authority" ? await request.clone().json() : null;
      const response = await instance.testOriginalFetch(request);
      if (instance.testHolding && message?.action === "authorize_connection") {
        if (instance.testProofSkip > 0) instance.testProofSkip--;
        else await new Promise(resolve => instance.testHeld.push({ resolve, identity: message.identity }));
      }
      return response; // Original live Accounts proof; never a manufactured response.
    };
  });
}
async function releaseConnectionProofs() {
  await inOwner(instance => {
    instance.testHolding = false;
    for (const held of instance.testHeld) held.resolve();
    instance.fetch = instance.testOriginalFetch;
  });
}
function streamRequest(id, session, view = "account") {
  return exports.default.fetch(`${origin}/api/games/${id}/stream?view=${view}`, { headers: { Cookie: session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
}
async function nativeRegistry(id, count) {
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, (instance, state) => {
    instance.testMaximumSockets = 0;
    instance.testAccept = state.acceptWebSocket.bind(state);
    instance.testNativeFetch = instance.fetch.bind(instance);
    state.acceptWebSocket = ws => {
      instance.testAccept(ws);
      instance.testMaximumSockets = Math.max(instance.testMaximumSockets, state.getWebSockets().length);
    };
    instance.fetch = async request => {
      if (new URL(request.url).pathname !== "/test-native-registry") return instance.testNativeFetch(request);
      const pair = new WebSocketPair();
      // Invalid hints deliberately confer no authority; these are genuine upgraded endpoints.
      const player = commandId();
      pair[1].serializeAttachment(JSON.stringify({ version: 1, game_id: id, connection_id: commandId(), session_id: commandId(), viewer: { kind: "player", id: player }, authentication_epoch: 0, session_expires_at: Date.now() + 86_400_000, view: player, last_sent_revision: null }));
      state.acceptWebSocket(pair[1]);
      return new Response(null, { status: 101, webSocket: pair[0] });
    };
  });
  const clients = [];
  for (let i = 0; i < count; i++) {
    const response = await owner.fetch("https://game.internal/test-native-registry", { headers: { Upgrade: "websocket" } });
    expect(response.status).toBe(101);
    response.webSocket.accept();
    clients.push(response.webSocket);
  }
  await runInDurableObject(owner, (instance, state) => {
    instance.fetch = instance.testNativeFetch;
    expect(state.getWebSockets().filter(ws => ws.readyState === 1)).toHaveLength(count);
  });
  return clients;
}

test("two genuine concurrent account upgrades from 99 recheck capacity at native acceptance", async () => {
  const { actor, id } = await openLobby();
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET username='CapacityHost' WHERE account_id=?", actor.accountId));
  const admin = await enroll();
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", admin.accountId));
  const clients = await nativeRegistry(id, 99);
  await holdConnectionProofs();
  const first = streamRequest(id, actor.session);
  const second = streamRequest(id, admin.session);
  await expect.poll(() => inOwner(instance => instance.testHeld.length)).toBe(2);
  await releaseConnectionProofs();
  const responses = await Promise.all([first, second]);
  for (const response of responses) if (response.webSocket) { response.webSocket.accept(); response.webSocket.close(1000); }
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (instance, state) => {
    expect(instance.testMaximumSockets).toBeLessThanOrEqual(100);
    expect(state.getWebSockets().length).toBeLessThanOrEqual(100);
    state.acceptWebSocket = instance.testAccept;
  });
  for (const client of clients) if (client.readyState === 1) client.close(1000);
  expect(responses.some(response => response.status === 409)).toBe(true);
});

test("an oversized native registry remains enumerable for fail-closed alarm recovery", async () => {
  const { id } = await openLobby();
  await nativeRegistry(id, 101);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), async (instance, state) => {
    // A bounded first recovery pass may remain unavailable during native handshakes.
    await instance.alarm().catch(() => {});
    expect(await state.storage.getAlarm()).not.toBeNull();
  });
  await expect.poll(() => runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (_instance, state) => state.getWebSockets().filter(ws => ws.readyState === 1).length)).toBe(0);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), async (instance, state) => {
    await instance.alarm();
    expect(state.getWebSockets().filter(ws => ws.readyState === 1)).toHaveLength(0);
    state.acceptWebSocket = instance.testAccept;
  });
});

test.each(["IGNORE", "ABORT"])("prepared account close %s never ACKs an unfenced grant and delayed real proof cannot admit it", async fault => {
  const { actor, id } = await openLobby();
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, (instance, state) => {
    instance.testAcceptedAfterFence = 0;
    instance.testPreparedAccept = state.acceptWebSocket.bind(state);
    state.acceptWebSocket = ws => { instance.testAcceptedAfterFence++; instance.testPreparedAccept(ws); };
  });
  await holdConnectionProofs();
  const delayed = streamRequest(id, actor.session);
  await expect.poll(() => inOwner(instance => instance.testHeld.length)).toBe(1);
  const identity = await inOwner(instance => instance.testHeld[0].identity);
  const request = { caller: "accounts", action: "close_account", target: { operation_id: commandId(), ...identity, created_at: Date.now() } };
  await runInDurableObject(owner, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_connection_grants WHERE connection_id=?", identity.connection_id)]).toHaveLength(1);
    expect(state.getWebSockets()).toEqual([]);
    state.storage.sql.exec(`CREATE TRIGGER prepared_fence_fault BEFORE DELETE ON game_connection_grants WHEN OLD.connection_id='${identity.connection_id}' BEGIN SELECT RAISE(${fault}${fault === "ABORT" ? ",'prepared close unavailable'" : ""}); END`);
  });
  const close = () => owner.fetch("https://game.internal/close-account", { method: "POST", body: JSON.stringify(request) });
  expect(await (await close()).json()).toEqual({ result: "unavailable", request });
  await runInDurableObject(owner, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_connection_grants WHERE connection_id=?", identity.connection_id)]).toHaveLength(1);
    state.storage.sql.exec("DROP TRIGGER prepared_fence_fault");
  });
  expect(await (await close()).json()).toEqual({ result: "closed", request });
  await releaseConnectionProofs();
  const denied = await delayed;
  expect(denied.status).toBe(401);
  expect(denied.webSocket).toBeNull();
  await runInDurableObject(owner, (instance, state) => {
    expect(instance.testAcceptedAfterFence).toBe(0);
    expect(state.getWebSockets()).toEqual([]);
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_connection_grants WHERE connection_id=?", identity.connection_id)]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_current_connections")]).toEqual([]);
  });
});

test.each(["backlog", "bytes"])("real unacknowledged snapshots enforce %s credit with overflow close 1009", async limit => {
  const configuration = limit === "bytes" ? { configuration: { numeric_upper_bound: 1000, board_side_length: 10, player_capacity: 20 } } : {};
  const { actor, id, lobby } = await openLobby(configuration);
  const players = [];
  const seats = limit === "bytes" ? 20 : 1;
  for (let i = 0; i < seats; i++) players.push(await join(id, lobby.game_code, `Player${i}`));
  const connected = [];
  if (limit === "bytes") {
    connected.push(await stream(id, players[0].session), await stream(id, players[1].session));
    for (const connection of connected) ack(connection);
  }
  const connection = limit === "bytes" ? await stream(id, actor.session, "account") : await stream(id, players[0].session);
  const frames = [connection.initial];
  const ended = closed(connection.socket, 10_000);
  if (limit === "bytes") {
    const host = await sync(id, actor.session, "account");
    const response = await post(`/api/games/${id}/start`, { expected_revision: host.view_revision }, { session: actor.session });
    expect(response.status).toBe(200);
    frames.push(await connection.next());
    for (const player of connected) ack(player, await player.next());
  }
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  let before = null;
  let closeEvent = null;
  for (let i = 0; i < 64; i++) {
    before = await runInDurableObject(owner, (_instance, state) => [...state.storage.sql.exec("SELECT delivery_id,frame_bytes FROM game_delivery_pending WHERE connection_id=?", connection.initial.connection_id)]);
    const emitted = frames.map(frame => ({ delivery_id: frame.delivery_id, frame_bytes: new TextEncoder().encode(JSON.stringify(frame)).byteLength })).sort((a, b) => a.delivery_id.localeCompare(b.delivery_id));
    expect(before.sort((a, b) => a.delivery_id.localeCompare(b.delivery_id))).toEqual(emitted);
    expect(emitted.every(frame => frame.frame_bytes <= 256 * 1024)).toBe(true);
    await runInDurableObject(owner, async (instance, state) => {
      state.storage.sql.exec("UPDATE game_view_revisions SET revision=revision+1");
      await instance.alarm().catch(() => {});
    });
    const next = await Promise.race([connection.next().then(frame => ({ frame })), ended.then(event => ({ event }))]);
    if (next.event) { closeEvent = next.event; break; }
    frames.push(next.frame);
  }
  expect(closeEvent?.code).toBe(1009);
  if (limit === "backlog") expect(before).toHaveLength(64);
  else {
    const outstanding = before.reduce((sum, frame) => sum + frame.frame_bytes, 0);
    expect(before.length).toBeLessThan(64);
    expect(outstanding).toBeLessThanOrEqual(1024 * 1024);
    expect(outstanding + new TextEncoder().encode(JSON.stringify(frames.at(-1))).byteLength).toBeGreaterThan(1024 * 1024);
  }
  for (const player of connected) if (player.socket.readyState === 1) player.socket.close(1000);
});

test("terminal cleanup retains release work and credit while genuine SDK closure is uncertain", async () => {
  const { id, lobby } = await openLobby();
  const player = await join(id, lobby.game_code, "TerminalPlayer");
  const connection = await stream(id, player.session);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, async (instance, state) => {
    const ws = state.getWebSockets()[0];
    ws.testClose = ws.close;
    ws.close = () => { throw new Error("test-only terminal close failure"); };
    state.storage.sql.exec("UPDATE game_record SET idle_due=?", Date.now() - 1);
    await instance.alarm().catch(() => {});
    expect(state.storage.sql.exec("SELECT state,game_code,started_at FROM game_record").one()).toMatchObject({ state: "cancelled", started_at: null });
    expect(state.storage.sql.exec("SELECT kind,attempt_count FROM game_pending_work").one()).toMatchObject({ kind: "release", attempt_count: 1 });
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toEqual([{ delivery_id: connection.initial.delivery_id }]);
    expect(ws.readyState).toBe(1);
    expect(await state.storage.getAlarm()).not.toBeNull();
  });
  await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")), (_instance, state) => expect(state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id).toBe(id));
  const ended = closed(connection.socket);
  await runInDurableObject(owner, async (instance, state) => {
    const ws = state.getWebSockets()[0];
    ws.close = ws.testClose;
    state.storage.sql.exec("UPDATE game_pending_work SET next_attempt_at=?", Date.now());
    await instance.alarm().catch(() => {});
  });
  await ended;
  await runInDurableObject(owner, async (instance, state) => {
    state.storage.sql.exec("UPDATE game_pending_work SET next_attempt_at=?", Date.now());
    await instance.alarm();
    expect([...state.storage.sql.exec("SELECT operation_id FROM game_pending_work")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toEqual([]);
  });
  await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")), (_instance, state) => expect(state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id).toBeNull());
});

test.each(["unknown", "revision", "duplicate", "oversized", "binary"])("invalid original ACK %s fails closed with the owning policy class", async kind => {
  const { id, lobby } = await openLobby();
  const player = await join(id, lobby.game_code, "AckPlayer");
  const connection = await stream(id, player.session);
  const frame = connection.initial;
  const message = { version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision };
  if (kind === "unknown") message.delivery_id = frame.delivery_id === "A".repeat(43) ? `B${"A".repeat(42)}` : "A".repeat(43);
  if (kind === "revision") message.view_revision++;
  let bytes = JSON.stringify(message);
  if (kind === "duplicate") bytes = bytes.replace('"version":1', '"version":1,"version":1');
  if (kind === "oversized") bytes = ` ${bytes}${" ".repeat(512)}`;
  if (kind === "binary") bytes = new Uint8Array([1]);
  const ended = closed(connection.socket);
  connection.socket.send(bytes);
  expect((await ended).code).toBe(kind === "oversized" ? 1009 : 1008);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT player_id FROM game_players")]).toHaveLength(1);
    expect([...state.storage.sql.exec("SELECT player_id FROM game_boards")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_current_connections")]).toEqual([]);
  });
});

test("valid ACK releases exact encoded credit without renewal, gameplay or host keepalive", async () => {
  const { id, lobby } = await openLobby();
  const player = await join(id, lobby.game_code, "ValidAckPlayer");
  const connection = await stream(id, player.session);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  const authorityState = () => runInDurableObject(owner, (_instance, state) => ({
    game: state.storage.sql.exec("SELECT state,revision,last_host_activity,idle_due FROM game_record").one(),
    sessions: [...state.storage.sql.exec("SELECT session_id,session_epoch,issued_at,expires_at,revoked_at FROM game_sessions")],
    views: [...state.storage.sql.exec("SELECT view_key,revision FROM game_view_revisions ORDER BY view_key")],
    boards: [...state.storage.sql.exec("SELECT * FROM game_boards")],
  }));
  const before = await authorityState();
  await runInDurableObject(owner, (_instance, state) => {
    expect(state.storage.sql.exec("SELECT delivery_id,frame_bytes FROM game_delivery_pending").one()).toEqual({ delivery_id: connection.initial.delivery_id, frame_bytes: new TextEncoder().encode(JSON.stringify(connection.initial)).byteLength });
  });
  ack(connection);
  await expect.poll(() => runInDurableObject(owner, (_instance, state) => [...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")].length)).toBe(0);
  expect(await authorityState()).toEqual(before);
  expect(connection.socket.readyState).toBe(1);
  connection.socket.close(1000);
});

test("actual player session expiry closes native presence without deleting retained membership", async () => {
  const { actor, id, lobby } = await openLobby();
  const player = await join(id, lobby.game_code, "ExpiredPlayer");
  const connection = await stream(id, player.session);
  const ended = closed(connection.socket);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), async (instance, state) => {
    state.storage.sql.exec("UPDATE game_sessions SET expires_at=issued_at+1");
    await instance.alarm().catch(() => {});
  });
  expect((await ended).code).toBe(1008);
  expect((await sync(id, actor.session, "account")).snapshot.connected_player_count).toBe(0);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT player_id FROM game_players")]).toEqual([{ player_id: player.body.player_id }]);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT connection_id FROM game_current_connections")]).toEqual([]);
  });
});

test("admission expiry and unknown code never allocate membership or cookies", async () => {
  const { id, lobby } = await openLobby();
  const wrong = await post(`/api/games/${id}/admission-context`, { game_code: lobby.game_code === "ZZZZZZZZ" ? "YYYYYYYY" : "ZZZZZZZZ" }, { id: null });
  expect(wrong.status).toBe(409);
  expect(wrong.headers.has("Set-Cookie")).toBe(false);
  const context = await post(`/api/games/${id}/admission-context`, { game_code: lobby.game_code }, { id: null });
  expect(context.status).toBe(200);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, (_instance, state) => state.storage.sql.exec("UPDATE game_admission_contexts SET expires_at=issued_at+1"));
  const denied = await post(`/api/games/${id}/players`, { game_code: lobby.game_code, alias: "ExpiredContext" }, { session: cookie(context) });
  expect(denied.status).toBe(401);
  expect(denied.headers.has("Set-Cookie")).toBe(false);
  await runInDurableObject(owner, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT * FROM game_players")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT * FROM game_sessions")]).toEqual([]);
  });
});

test("lost player cookie retries are receipt-only and missing retry context cannot leak the result", async () => {
  const { id, lobby } = await openLobby();
  const context = await post(`/api/games/${id}/admission-context`, { game_code: lobby.game_code }, { id: null });
  const session = cookie(context);
  const command = commandId();
  const input = { game_code: lobby.game_code, alias: "LostCookie" };
  const fresh = await post(`/api/games/${id}/players`, input, { session, id: command });
  expect(fresh.status).toBe(200);
  const body = await fresh.json();
  const replay = await post(`/api/games/${id}/players`, input, { session, id: command });
  expect(replay.status).toBe(200);
  expect(await replay.json()).toEqual({ result: "committed", receipt: body.receipt });
  expect(replay.headers.has("Set-Cookie")).toBe(false);
  const absent = await post(`/api/games/${id}/players`, input, { id: command });
  expect(absent.status).toBe(401);
  expect(absent.headers.has("Set-Cookie")).toBe(false);
  expect(JSON.stringify(await absent.json())).not.toContain(body.player_id);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (_instance, state) => expect([...state.storage.sql.exec("SELECT session_id FROM game_sessions")]).toHaveLength(1));
});

test.each(["changed_view", "closed_authority", "overlapping_send"])("snapshot final-await %s resynchronizes without releasing captured output", async change => {
  const { actor, id } = await openLobby();
  const connection = await stream(id, actor.session, "account");
  ack(connection);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await expect.poll(() => runInDurableObject(owner, (_instance, state) => [...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")].length)).toBe(0);
  const emitted = [];
  connection.socket.addEventListener("message", event => emitted.push(event.data));
  const ended = closed(connection.socket);
  await holdConnectionProofs(1);
  await runInDurableObject(owner, (_instance, state) => state.storage.sql.exec("UPDATE game_view_revisions SET revision=revision+1"));
  const pending = runInDurableObject(owner, instance => instance.alarm().catch(() => {}));
  await expect.poll(() => inOwner(instance => instance.testHeld.length)).toBe(1);
  await runInDurableObject(owner, (_instance, state) => expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toHaveLength(1));
  if (change === "changed_view") {
    await runInDurableObject(owner, (_instance, state) => state.storage.sql.exec("UPDATE game_view_revisions SET revision=revision+1"));
  } else if (change === "overlapping_send") {
    await runInDurableObject(owner, instance => instance.alarm().catch(() => {}));
  } else {
    const identity = await inOwner(instance => instance.testHeld[0].identity);
    const request = { caller: "accounts", action: "close_account", target: { operation_id: commandId(), ...identity, created_at: Date.now() } };
    const reply = await owner.fetch("https://game.internal/close-account", { method: "POST", body: JSON.stringify(request) });
    expect((await reply.json()).request).toEqual(request);
  }
  await releaseConnectionProofs();
  await pending;
  expect((await ended).code).toBe(change === "closed_authority" ? 1008 : 1013);
  expect(emitted).toEqual([]);
});

test("genuine hibernation preserves outstanding credit and reconstructed native presence", async () => {
  const { actor, id, lobby } = await openLobby();
  const player = await join(id, lobby.game_code, "HibernatingPlayer");
  const connection = await stream(id, player.session);
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await evictDurableObject(owner);
  expect(connection.socket.readyState).toBe(1);
  expect((await sync(id, actor.session, "account")).snapshot.connected_player_count).toBe(1);
  await runInDurableObject(owner, (_instance, state) => {
    expect(state.getWebSockets().filter(ws => ws.readyState === 1)).toHaveLength(1);
    expect([...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")]).toEqual([{ delivery_id: connection.initial.delivery_id }]);
  });
  ack(connection);
  await expect.poll(() => runInDurableObject(owner, (_instance, state) => [...state.storage.sql.exec("SELECT delivery_id FROM game_delivery_pending")].length)).toBe(0);
  expect((await sync(id, player.session)).snapshot.player_id).toBe(player.body.player_id);
  connection.socket.close(1000);
});

test("unknown Game admission fails without initializing a configuration or default record", async () => {
  const id = commandId();
  const response = await post(`/api/games/${id}/admission-context`, { game_code: "UNKNOWN1" }, { id: null });
  expect(response.status).toBe(404);
  expect(response.headers.has("Set-Cookie")).toBe(false);
  await runInDurableObject(env.GAMES.get(env.GAMES.idFromName(id)), (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT * FROM game_record")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT * FROM game_configuration")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT * FROM game_players")]).toEqual([]);
  });
});

test.each(["admission", "player"])("fresh %s cookie is withheld when actual owner authority expires at its output await", async kind => {
  const { id, lobby } = await openLobby();
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  let session;
  if (kind === "player") session = cookie(await post(`/api/games/${id}/admission-context`, { game_code: lobby.game_code }, { id: null }));
  await runInDurableObject(owner, (instance, state) => {
    instance.testOriginalSync = state.storage.sync.bind(state.storage);
    instance.testCookieHold = true;
    state.storage.sync = async () => {
      await instance.testOriginalSync();
      const table = kind === "admission" ? "game_admission_contexts" : "game_sessions";
      if (instance.testCookieHold && [...state.storage.sql.exec(`SELECT * FROM ${table}`)].length) {
        instance.testCookieHold = false;
        await new Promise(resolve => { instance.testCookieGate = resolve; });
      }
    };
  });
  const pending = kind === "admission"
    ? post(`/api/games/${id}/admission-context`, { game_code: lobby.game_code }, { id: null })
    : post(`/api/games/${id}/players`, { game_code: lobby.game_code, alias: "OutputFence" }, { session });
  await expect.poll(() => runInDurableObject(owner, instance => typeof instance.testCookieGate === "function")).toBe(true);
  await runInDurableObject(owner, (instance, state) => {
    if (kind === "admission") state.storage.sql.exec("UPDATE game_record SET idle_due=?", Date.now() - 1);
    else state.storage.sql.exec("UPDATE game_sessions SET revoked_at=?", Date.now());
    state.storage.sync = instance.testOriginalSync;
    instance.testCookieGate();
  });
  const response = await pending;
  expect(response.status).toBe(kind === "admission" ? 404 : 401);
  expect(response.headers.has("Set-Cookie")).toBe(false);
});

test("Game owner alarm cancels and releases an expired prestart lobby", async () => {
  const { id } = await openLobby();
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  await runInDurableObject(owner, async (instance, state) => {
    state.storage.sql.exec("UPDATE game_record SET idle_due=?", Date.now() - 1);
    await instance.alarm();
    expect([...state.storage.sql.exec("SELECT state FROM game_record")][0].state).toBe("cancelled");
    expect([...state.storage.sql.exec("SELECT operation_id FROM game_pending_work")]).toEqual([]);
    expect([...state.storage.sql.exec("SELECT player_id FROM game_players")]).toEqual([]);
  });
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory, (_instance, state) => {
    expect([...state.storage.sql.exec("SELECT game_id FROM directory_global_reservation")][0].game_id).toBeNull();
  });
});
