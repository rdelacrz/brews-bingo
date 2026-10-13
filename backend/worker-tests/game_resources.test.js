// Local SDK/WebSocket measurement, never deployed capacity certification.
import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import domainSource from "../../shared/domain/src/games.rs?raw";
import { enroll, post, resetStorage, origin, cookie } from "./fixtures.js";

const maximum = name => {
  const match = domainSource.match(new RegExp(`pub const ${name}: [^=]+ = ([0-9_]+);`));
  if (!match) throw new Error("maximum constant unavailable");
  return Number(match[1].replaceAll("_", ""));
};
const limits = { numeric: maximum("NUMERIC_UPPER_BOUND_MAX"), side: maximum("BOARD_SIDE_MAX"), players: maximum("PLAYER_CAPACITY_MAX"), spectators: maximum("SPECTATOR_CAPACITY_MAX") };
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const bytes = value => new TextEncoder().encode(typeof value === "string" ? value : JSON.stringify(value)).byteLength;
const pause = () => new Promise(resolve => setTimeout(resolve, 10));
beforeEach(async () => {
  await reset(); await resetStorage();
  await runInDurableObject(directory(), async (_, state) => state.storage.deleteAll());
});
async function jsonOK(response) {
  // Only status is included in failure output; protected bodies stay in memory.
  expect(response.status).toBe(200); return response.json();
}
async function sync(id, session, view) {
  return jsonOK(await exports.default.fetch(`${origin}/api/games/${id}/sync?view=${view}`, { headers: { Cookie: session } }));
}
async function storage(stub) {
  return runInDurableObject(stub, (_, state) => {
    const scalar = sql => [...state.storage.sql.exec(sql)][0];
    const sqlite_database_size_bytes = state.storage.sql.databaseSize;
    const tables = {};
    for (const { name } of state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")) {
      if (name.startsWith("_")) continue;
      if (!/^[a-zA-Z0-9_]+$/.test(name)) throw new Error("unsafe table identifier");
      tables[name] = scalar(`SELECT count(*) AS n FROM ${name}`).n;
    }
    return { sqlite_database_size_bytes, tables, page_pragmas_available:false };
  });
}
async function stream(id, session, view, sockets) {
  const response = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=${view}`, { headers: { Cookie: session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
  expect(response.status).toBe(101);
  const ws = response.webSocket;
  const stats = { frames: 0, frame_bytes: 0, snapshots: 0, terminal_notices: 0, ack_frames: 0, ack_bytes: 0, closed: false };
  let revision = -1;
  ws.addEventListener("message", event => {
    stats.frames++; stats.frame_bytes += bytes(event.data);
    const frame = JSON.parse(event.data);
    if (frame.kind === "snapshot") {
      stats.snapshots++; revision = frame.view_revision;
      const ack = JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision });
      ws.send(ack); stats.ack_frames++; stats.ack_bytes += bytes(ack);
    } else if (frame.kind === "terminal_notice") stats.terminal_notices++;
  });
  ws.addEventListener("close", () => { stats.closed = true; });
  const socket = { ws, stats, view, revision: () => revision };
  sockets.push(socket); ws.accept();
  for (let n = 0; n < 1000 && revision < 0; n++) await pause();
  expect(revision).toBeGreaterThanOrEqual(0);
  return socket;
}
async function settled(id) {
  for (let n = 0; n < 1000; n++) {
    const count = await runInDurableObject(owner(id), (instance, state) => [...state.storage.sql.exec("SELECT count(*) AS n FROM game_delivery_pending")][0].n);
    if (count === 0) return;
    await pause();
  }
  throw new Error("resource ACK settlement exceeded bound");
}
function finiteMetrics(value) {
  if (typeof value === "number") { expect(Number.isFinite(value)).toBe(true); expect(value).toBeGreaterThanOrEqual(0); }
  else if (value && typeof value === "object") for (const item of Object.values(value)) finiteMetrics(item);
}
test("maximum VALUE-cell game measures local SDK work and sampled native WebSocket traffic", async () => {
  const total = performance.now(), sockets = [], stages = [];
  const host = await enroll("ResourceHost");
  const configuration = { numeric_upper_bound: limits.numeric, board_side_length: limits.side, free_cells_enabled: false, free_cell_positions: [], player_capacity: limits.players, spectator_capacity: limits.spectators };
  const created = await jsonOK(await post("/api/games", { configuration }, { session: host.session }));
  const id = created.game.game_id;
  const opened = await jsonOK(await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: host.session }));
  const code = opened.game_code, players = [], spectators = [];
  const owners = { accounts: env.ACCOUNTS.get(env.ACCOUNTS.idFromName("accounts")), directory: directory(), game: owner(id) };
  const capture = async (name, start, payload_json_bytes = {}) => {
    const sql_sdk_exhausted_cursor_work = {}, storage_by_owner = {};
    // Direct storage observations are separate; SDK execution row work is unavailable.
    for (const [name, stub] of Object.entries(owners)) {
      sql_sdk_exhausted_cursor_work[name] = { measurement_available:false };
      storage_by_owner[name] = await storage(stub);
    }
    const result = { name, wall_ms: performance.now() - start, payload_json_bytes, sql_sdk_exhausted_cursor_work, storage: storage_by_owner };
    finiteMetrics(result); stages.push(result); return result;
  };
  // SDK cursor interception remains pending after bounded runtime traps.

  try {
    const lobbyStart = performance.now();
    for (let n = 0; n < limits.players; n++) {
      const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null }); expect(context.status).toBe(200);
      const joined = await post(`/api/games/${id}/players`, { game_code: code, alias: `Player${String(n).padStart(2, "0")}` }, { session: cookie(context) }); expect(joined.status).toBe(200);
      players.push(cookie(joined)); await joined.text();
    }
    for (let n = 0; n < limits.spectators; n++) {
      const context = await post(`/api/games/${id}/spectator-admission-context`, { game_code: code }, { id: null }); expect(context.status).toBe(200);
      const joined = await post(`/api/games/${id}/spectators`, { game_code: code }, { session: cookie(context) }); expect(joined.status).toBe(200);
      spectators.push(cookie(joined)); await joined.text();
    }
    // Exact sampled socket load: two players, one spectator. All 70 admissions are real.
    await stream(id, players[0], "player", sockets); await stream(id, players[1], "player", sockets); await stream(id, spectators[0], "spectator", sockets);
    await settled(id);
    const lobby = await capture("lobby", lobbyStart);
    expect(lobby.storage.game.tables.game_players).toBe(limits.players); expect(lobby.storage.game.tables.game_spectators).toBe(limits.spectators);
    expect(lobby.storage.game.tables.game_current_connections).toBe(sockets.length);
    const start = performance.now();
    const before = await sync(id, host.session, "account");
    await jsonOK(await post(`/api/games/${id}/start`, { expected_revision: before.view_revision }, { session: host.session }));
    const payloads = async () => {
      const account = await sync(id, host.session, "account"), player = await sync(id, players[0], "player"), spectator = await sync(id, spectators[0], "spectator");
      expect(account.snapshot.configuration.numeric_upper_bound).toBe(limits.numeric);
      expect(account.snapshot.players.length).toBe(limits.players);
      expect(account.snapshot.players.reduce((n, p) => n + p.board.cells.length, 0)).toBe(limits.players * limits.side ** 2);
      return { host_sync: bytes(account), player_sync: bytes(player), spectator_sync: bytes(spectator) };
    };
    const startedPayload = await payloads(); await settled(id);
    const started = await capture("started", start, startedPayload);
    expect(started.storage.game.tables.game_boards).toBe(limits.players); expect(started.storage.game.tables.game_board_cells).toBe(limits.players * limits.side ** 2);
    const calledAt = performance.now();
    const current = await sync(id, host.session, "account");
    const called = await jsonOK(await post(`/api/games/${id}/calls/manual`, { value: "1", expected_revision: current.view_revision }, { session: host.session }));
    expect(called.remaining_count).toBe(limits.numeric - 1); expect(called.exhausted).toBe(false);
    const calledPayload = await payloads(); await settled(id); await capture("called", calledAt, calledPayload);
    const endedAt = performance.now();
    const terminal = await jsonOK(await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: host.session }));
    expect(terminal.history_available).toBe(true); expect(terminal.winner === null).toBe(true);
    await settled(id);
    for (const socket of sockets) if (!socket.stats.closed) socket.ws.close(1000);
    for (let n = 0; n < 1000 && sockets.some(s => !s.stats.closed); n++) await pause();
    expect(sockets.every(s => s.stats.closed)).toBe(true);
    const ended = await capture("history_cleanup_release", endedAt);
    expect(ended.storage.game.tables.game_history_board_cells).toBe(limits.players * limits.side ** 2); expect(ended.storage.game.tables.game_history_calls).toBe(1);
    expect(ended.storage.game.tables.game_spectators).toBe(0); expect(ended.storage.game.tables.game_spectator_sessions).toBe(0);
    const occupied = await runInDurableObject(directory(), (_, state) => [...state.storage.sql.exec("SELECT count(*) AS n FROM directory_global_reservation WHERE game_id IS NOT NULL")][0].n);
    expect(occupied).toBe(0);
    const frames = sockets.map(s => ({ view: s.view, ...s.stats }));
    for (const s of frames) { expect(s.ack_frames).toBe(s.snapshots); expect(s.frames).toBe(s.snapshots + s.terminal_notices); }
    const totals = frames.reduce((out, s) => { for (const key of ["frames", "frame_bytes", "snapshots", "terminal_notices", "ack_frames", "ack_bytes"]) out[key] = (out[key] ?? 0) + s[key]; return out; }, {});
    const report = { target: "local_workerd_emulated_SDK", configuration: { branch: "maximum_numeric_VALUE_cells_no_free_cells", ...limits }, sample: { manual_calls: 1, admissions: players.length + spectators.length, socket_load: sockets.length, socket_scope: "native_Workerd_Upgrade_WebSocket_pairs_not_network_TLS_WSS", entropy: "Workerd_crypto_getRandomValues_not_native_TestRuntime", wall_ms: performance.now() - total, frames, totals }, stages, limits_pending: ["full_70_socket_load", "network_TLS_WSS", "full_1000_calls_Workerd", "SDK_cursor_row_work_runtime_observer_trap", "page_pragmas_SQLITE_AUTH", "other_configuration_branches", "worst_case_workload", "deployed_capacity", "owner_forecast", "traffic_concurrency", "provider_quota_headroom", "RUNTIME-FINAL-01"], metric_scope: "SDK databaseSize bytes and table cardinalities; SDK cursor row work unavailable after runtime observer trap, not provider billable rows" };
    finiteMetrics(report); console.log(`BREWS-GAME-RESOURCES ${JSON.stringify(report)}`);
  } finally {
    for (const socket of sockets) if (!socket.stats.closed) socket.ws.close(1000);

  }
}, 300000);
