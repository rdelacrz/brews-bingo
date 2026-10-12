import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { commandId, cookie, current, enroll, inOwner, origin, post, resetStorage } from "./fixtures.js";

beforeEach(async () => {
  await reset();
  await resetStorage();
});

function owner(id) { return env.GAMES.get(env.GAMES.idFromName(id)); }

async function sync(fixture, session = fixture.actor.session, view = "account") {
  return exports.default.fetch(`${origin}/api/games/${fixture.id}/sync?view=${view}`, {
    headers: { Cookie: session },
  });
}

async function connect(id, session) {
  const response = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=player`, {
    headers: { Cookie: session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" },
  });
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  const initial = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("initial snapshot timeout")), 3000);
    socket.addEventListener("message", event => {
      const frame = JSON.parse(event.data);
      socket.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id,
        delivery_id: frame.delivery_id, view_revision: frame.view_revision }));
      clearTimeout(timeout);
      resolve();
    });
  });
  socket.accept();
  await initial;
  return socket;
}

async function startedGame({ players, side, upper }) {
  const actor = await enroll();
  const created = await post("/api/games", { configuration: {
    numeric_upper_bound: upper, board_side_length: side, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: players, spectator_capacity: 0,
  } }, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const opened = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(opened.status).toBe(200);
  const code = (await opened.json()).game_code;
  const sessions = [];
  for (let index = 0; index < players; index++) {
    const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
    expect(context.status).toBe(200);
    const joined = await post(`/api/games/${id}/players`, { game_code: code, alias: `Player${index}` }, { session: cookie(context) });
    expect(joined.status).toBe(200);
    sessions.push(cookie(joined));
  }
  const sockets = [];
  try {
    for (const session of sessions.slice(0, 2)) sockets.push(await connect(id, session));
    const fixture = { id, actor, sessions };
    const view = await sync(fixture);
    expect(view.status).toBe(200);
    const started = await post(`/api/games/${id}/start`, {
      expected_revision: (await view.json()).view_revision,
    }, { session: actor.session });
    expect(started.status).toBe(200);
    return fixture;
  } finally {
    for (const socket of sockets) socket.close(1000);
  }
}

async function seedCalls(fixture, count) {
  const commands = Array.from({ length: count }, () => commandId());
  await runInDurableObject(owner(fixture.id), (_instance, state) => {
    const sql = state.storage.sql;
    const record = sql.exec("SELECT game_id,host_id,started_at FROM game_record").one();
    state.storage.transactionSync(() => {
      for (let index = 0; index < count; index++) {
        sql.exec("INSERT INTO game_calls VALUES(?,?,?,?,?,?,?)", record.game_id, index + 1,
          String(index + 1), "manual", record.host_id, commands[index], record.started_at);
      }
      sql.exec("UPDATE game_boards SET evaluated_through_call=?", count);
      sql.exec("UPDATE game_board_cells SET is_matched=CASE WHEN CAST(value AS INTEGER)<=? THEN 1 ELSE 0 END", count);
      sql.exec("DELETE FROM game_completed_lines");
      for (const board of sql.exec("SELECT player_id,side_length FROM game_boards")) {
        const cells = [...sql.exec("SELECT row,column,is_matched FROM game_board_cells WHERE player_id=? ORDER BY row,column", board.player_id)];
        const lines = [];
        const complete = predicate => cells.filter(predicate).every(cell => cell.is_matched === 1);
        for (let row = 1; row <= board.side_length; row++) if (complete(cell => cell.row === row)) lines.push(["row", row]);
        for (let column = 1; column <= board.side_length; column++) if (complete(cell => cell.column === column)) lines.push(["column", column]);
        if (complete(cell => cell.row === cell.column)) lines.push(["main_diagonal", null]);
        if (complete(cell => cell.row + cell.column === board.side_length + 1)) lines.push(["anti_diagonal", null]);
        lines.forEach(([kind, index], sequence) => sql.exec("INSERT INTO game_completed_lines VALUES(?,?,?,?)", board.player_id, sequence, kind, index));
      }
    });
  });
}

async function cancelAndReadHistory(fixture) {
  const cancelled = await post(`/api/games/${fixture.id}/cancel`, {
    confirmed: true, expected_state: "in_progress",
  }, { session: fixture.actor.session });
  expect(cancelled.status).toBe(200);
  const detail = await exports.default.fetch(`${origin}/api/history/${fixture.id}`, {
    headers: { Cookie: fixture.actor.session },
  });
  expect(detail.status).toBe(200);
  return detail.json();
}

test("the full 1000-value pool remains callable, synchronizable and archivable in Workerd", async () => {
  const fixture = await startedGame({ players: 2, side: 2, upper: 1000 });
  // Seed valid prior progression; the last call and all reads use the actual Worker.
  await seedCalls(fixture, 999);
  const host = await sync(fixture);
  expect(host.status).toBe(200);
  const before = await host.json();
  expect(before.snapshot.calls).toHaveLength(999);
  const last = await post(`/api/games/${fixture.id}/calls/manual`, {
    value: "1000", expected_revision: before.view_revision,
  }, { session: fixture.actor.session });
  expect(last.status).toBe(200);
  expect(await last.json()).toMatchObject({ result: "call_accepted", remaining_count: 0, exhausted: true });
  const player = await sync(fixture, fixture.sessions[0], "player");
  expect(player.status).toBe(200);
  expect((await player.json()).snapshot.calls).toHaveLength(1000);
  const history = await cancelAndReadHistory(fixture);
  expect(history.history.ordered_calls).toHaveLength(1000);
});

test("terminal archival supports all 20 retained players with 100 cells each", async () => {
  const fixture = await startedGame({ players: 20, side: 10, upper: 1000 });
  const history = await cancelAndReadHistory(fixture);
  expect(history.history.players).toHaveLength(20);
  expect(history.history.players.every(player => player.cells.length === 100)).toBe(true);
  await runInDurableObject(owner(fixture.id), (_instance, state) => {
    expect(state.storage.sql.exec("SELECT count(*) AS n FROM game_history_board_cells").one().n).toBe(2000);
    expect(state.storage.sql.exec("SELECT count(*) AS n FROM game_players").one().n).toBe(0);
  });
});

test.each(["ignored", "corrupted"])("account migration rejects a %s schema marker and rolls back", async fault => {
  await inOwner((_instance, state) => {
    const sql = state.storage.sql;
    for (const table of ["management_receipts", "admin_audit", "pending_account_removals"]) sql.exec(`DROP TABLE ${table}`);
    sql.exec("UPDATE storage_metadata SET schema_version=1,last_observed_ms=123,command_floor_ms=100");
    sql.exec(fault === "ignored"
      ? "CREATE TRIGGER marker_fault BEFORE UPDATE OF schema_version ON storage_metadata BEGIN SELECT RAISE(IGNORE); END"
      : "CREATE TRIGGER marker_fault AFTER UPDATE OF schema_version ON storage_metadata BEGIN UPDATE storage_metadata SET schema_version=3; END");
  });
  const response = await current();
  expect(response.status).toBe(503);
  expect(response.headers.has("Set-Cookie")).toBe(false);
  await inOwner((_instance, state) => {
    expect(state.storage.sql.exec("SELECT schema_version,last_observed_ms,command_floor_ms FROM storage_metadata").one())
      .toEqual({ schema_version: 1, last_observed_ms: 123, command_floor_ms: 100 });
    expect([...state.storage.sql.exec("SELECT name FROM sqlite_master WHERE name IN ('management_receipts','admin_audit','pending_account_removals')")]).toEqual([]);
  });
});
