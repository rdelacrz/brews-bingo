// RUNTIME-FINAL-01: genuine sequential owner cuts authorize a bounded HTTP snapshot.
import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId, password, inOwner } from "./fixtures.js";

const clock = Date.now;
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async () => {
  Date.now = clock;
  await reset();
  await resetStorage();
  await runInDurableObject(directory(), (_, state) => state.storage.deleteAll());
});

async function fixture() {
  const host = await enroll("SnapshotHost");
  const made = await post("/api/games", { configuration: {
    numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: 3, spectator_capacity: 0,
  } }, { session: host.session });
  expect(made.status).toBe(200);
  const f = { host, id: (await made.json()).game.game_id, players: [], sockets: [] };
  const opened = await post(`/api/games/${f.id}/lobby`, { expected_revision: 0 }, { session: host.session });
  expect(opened.status).toBe(200);
  f.code = (await opened.json()).game_code;
  for (const alias of ["SnapshotOne", "SnapshotTwo"]) {
    const context = await post(`/api/games/${f.id}/admission-context`, { game_code: f.code }, { id: null });
    expect(context.status).toBe(200);
    const answer = password(), command = commandId();
    const joined = await post(`/api/games/${f.id}/players`, {
      game_code: f.code, alias, recovery_answer: answer,
    }, { session: cookie(context), id: command });
    expect(joined.status).toBe(200);
    f.players.push({ alias, answer, command, session: cookie(joined), body: await joined.json() });
  }
  return f;
}
const get = (f, path, session = f.players[0].session) => exports.default.fetch(
  `${origin}/api/games/${f.id}${path}`, { headers: { Cookie: session } });
async function read(f, path, session) {
  const r = await get(f, path, session);
  expect(r.status).toBe(200);
  expect(r.headers.get("Set-Cookie")).toBeNull();
  expect(r.headers.get("Cache-Control")).toBe("no-store");
  return r.json();
}
async function denied(response, status = 401) {
  expect(response.status).toBe(status);
  expect(response.headers.get("Set-Cookie")).toBeNull();
  expect(Object.keys(await response.json())).toEqual(["error"]);
}
async function recover(f) {
  const p = f.players[0];
  const r = await post("/api/player-recovery", { game_code: f.code, alias: p.alias, recovery_answer: p.answer });
  expect(r.status).toBe(200);
  const current = cookie(r);
  expect((await r.json()).membership.player_id).toBe(p.body.player_id);
  return current;
}
async function mutateContent(f, mutation) {
  const rename = mutation === "rename";
  const r = await exports.default.fetch(`${origin}/api/games/${f.id}/membership/${rename ? "alias" : "recovery-answer"}`, {
    method: rename ? "PATCH" : "DELETE",
    headers: { Origin: origin, Cookie: f.players[0].session, "Idempotency-Key": commandId(), "Content-Type": "application/json" },
    body: JSON.stringify(rename ? { alias: "SnapshotRenamed" } : {}),
  });
  expect(r.status).toBe(200);
  const result = await r.json();
  expect(rename ? result.alias === "SnapshotRenamed" : result.recovery_enabled === false).toBe(true);
}

// Hold only the second genuine /read-release of this invocation. The action commits
// before that cut, or after its genuine successful reply; no authorization is fabricated.
async function holdFinal(f, timing, action) {
  const observed = { releases: 0, changed: false, authorized: false };
  await runInDurableObject(owner(f.id), instance => {
    instance.snapshotOriginalFetch = instance.fetch.bind(instance);
    let firstChallenge;
    instance.fetch = async request => {
      if (new URL(request.url).pathname !== "/read-release") return instance.snapshotOriginalFetch(request);
      const input = await request.clone().json();
      observed.releases++;
      if (observed.releases === 1) firstChallenge = input.challenge;
      const final = observed.releases === 2;
      if (final) {
        expect(input.challenge).not.toEqual(firstChallenge);
        observed.cap = input.binding.valid_until;
        expect(Date.now()).toBeLessThan(observed.cap);
      }
      if (final && timing === "before") { observed.changed = true; await action(); }
      const actual = await instance.snapshotOriginalFetch(request);
      if (!final) return actual;
      const bytes = await actual.clone().arrayBuffer();
      const ack = JSON.parse(new TextDecoder().decode(bytes));
      observed.authorized = actual.status === 200 && ack.code === null;
      if (timing === "after") {
        expect(observed.authorized).toBe(true);
        expect(ack.challenge).toEqual(input.challenge);
        expect(ack.valid_until).toBeLessThanOrEqual(observed.cap);
        observed.changed = true;
        await action();
      }
      return new Response(bytes, { status: actual.status, headers: actual.headers });
    };
  });
  return observed;
}
async function restore(f) {
  await runInDurableObject(owner(f.id), instance => {
    if (instance.snapshotOriginalFetch) instance.fetch = instance.snapshotOriginalFetch;
    delete instance.snapshotOriginalFetch;
  });
}
function assertCut(observed, timing) {
  expect(observed.changed).toBe(true);
  expect(observed.releases).toBe(2);
  expect(observed.authorized).toBe(timing === "after");
}

async function realSocket(f, session) {
  const r = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`, {
    headers: { Origin: origin, Cookie: session, Upgrade: "websocket", Connection: "Upgrade" },
  });
  expect(r.status).toBe(101);
  const ws = r.webSocket;
  f.sockets.push(ws);
  const first = new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("snapshot frame timeout")), 3000);
    ws.addEventListener("message", event => {
      const frame = JSON.parse(event.data);
      if (frame.kind !== "snapshot") return;
      ws.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id,
        delivery_id: frame.delivery_id, view_revision: frame.view_revision }));
      clearTimeout(timeout);
      resolve();
    });
  });
  ws.accept();
  await first;
}
async function actualStart(f) {
  for (const p of f.players) await realSocket(f, p.session);
  const host = await read(f, "", f.host.session);
  const r = await post(`/api/games/${f.id}/start`, { expected_revision: host.game.game.view_revision }, { session: f.host.session });
  expect(r.status).toBe(200);
  await r.text();
  expect((await read(f, "/membership/board")).board).not.toBeNull();
}
async function cleanup(f) {
  await restore(f);
  Date.now = clock;
  for (const ws of f.sockets) {
    if (ws.readyState === 3) continue;
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error("snapshot socket close timeout")), 3000);
      ws.addEventListener("close", () => { clearTimeout(timeout); resolve(); }, { once: true });
      ws.close(1000);
    });
  }
}

for (const timing of ["before", "after"]) {
  test(`RUNTIME-FINAL-01 G4 recovery ${timing} final cut`, async () => {
    const f = await fixture(), path = `/commands/${f.players[0].command}`;
    const baseline = await read(f, path);
    expect(baseline.result_status.status).toBe("committed");
    let successor;
    const observed = await holdFinal(f, timing, async () => { successor = await recover(f); });
    try {
      if (timing === "after") expect(await read(f, path)).toEqual(baseline);
      else await denied(await get(f, path));
      assertCut(observed, timing);
      await restore(f);
      await denied(await get(f, path));
      expect(await read(f, path, successor)).toEqual(baseline);
    } finally { await cleanup(f); }
  });

  for (const [label, path, started] of [["membership", "/membership", false], ["board", "/membership/board", true], ["privacy", "", true]]) {
    test(`RUNTIME-FINAL-01 dedicated ${label} recovery ${timing} final cut`, async () => {
      const f = await fixture();
      try {
        if (started) await actualStart(f);
        const baseline = await read(f, path);
        if (label === "privacy") {
          expect(baseline.game.role).toBe("player");
          expect(baseline.game.player_id).toBe(f.players[0].body.player_id);
          expect(baseline.game.alias).toBe(f.players[0].alias);
          expect(baseline.game.board).not.toBeNull();
          expect(JSON.stringify(baseline).includes(f.players[1].alias)).toBe(false);
          expect(JSON.stringify(baseline).includes(f.players[1].body.player_id)).toBe(false);
        }
        let successor;
        const observed = await holdFinal(f, timing, async () => { successor = await recover(f); });
        if (timing === "after") expect(await read(f, path)).toEqual(baseline);
        else await denied(await get(f, path));
        assertCut(observed, timing);
        await restore(f);
        await denied(await get(f, path));
        const current = await read(f, path, successor);
        // Recovery advances the view revision, while retaining the same private identity/board.
        if (label === "membership") expect(current.membership.player_id).toBe(baseline.membership.player_id);
        else if (label === "board") expect(current.board).toEqual(baseline.board);
        else {
          expect(current.game.role).toBe("player");
          expect(current.game.player_id).toBe(baseline.game.player_id);
          expect(current.game.alias).toBe(baseline.game.alias);
          expect(current.game.board).toEqual(baseline.game.board);
          expect(current.game.view_revision).toBeGreaterThan(baseline.game.view_revision);
        }
      } finally { await cleanup(f); }
    });
  }

  for (const mutation of ["delete-answer", "rename"]) {
    test(`RUNTIME-FINAL-01 ${mutation} ${timing} final cut content`, async () => {
      const f = await fixture(), baseline = await read(f, "/membership");
      const observed = await holdFinal(f, timing, () => mutateContent(f, mutation));
      try {
        if (timing === "after") expect(await read(f, "/membership")).toEqual(baseline);
        else await denied(await get(f, "/membership"), 503);
        assertCut(observed, timing);
        await restore(f);
        const current = await read(f, "/membership");
        expect(current.membership.player_id).toBe(baseline.membership.player_id);
        expect(current.membership.session_expires_at).toBe(baseline.membership.session_expires_at);
        expect(current.membership.alias).toBe(mutation === "rename" ? "SnapshotRenamed" : baseline.membership.alias);
        expect(current.membership.recovery_enabled).toBe(mutation !== "delete-answer");
        expect(current).not.toEqual(baseline);
      } finally { await cleanup(f); }
    });
  }

  test(`RUNTIME-FINAL-01 SQL authoritative account epoch invalidation ${timing} final cut`, async () => {
    const f = await fixture(), baseline = await read(f, "/players", f.host.session);
    const observed = await holdFinal(f, timing, async () => {
      // Scoped authoritative storage test; this is not a public disable API test.
      await inOwner(async (_, state) => {
        state.storage.sql.exec("UPDATE accounts SET credential_epoch=credential_epoch+1 WHERE account_id=?", f.host.accountId);
        await state.storage.sync();
        expect(state.storage.sql.exec("SELECT credential_epoch FROM accounts WHERE account_id=?", f.host.accountId).one().credential_epoch).toBe(2);
      });
    });
    try {
      if (timing === "after") expect(await read(f, "/players", f.host.session)).toEqual(baseline);
      else await denied(await get(f, "/players", f.host.session));
      assertCut(observed, timing);
      await restore(f);
      await denied(await get(f, "/players", f.host.session));
    } finally { await cleanup(f); }
  });

  test(`RUNTIME-FINAL-01 cross-session final Exit ${timing} final cut`, async () => {
    const f = await fixture();
    try {
      await actualStart(f);
      const login = await post("/api/auth/login", { username: f.host.username, password: f.host.password });
      expect(login.status).toBe(200);
      const second = cookie(login);
      await login.text();
      const sync = await get(f, "/sync?view=account", second);
      expect(sync.status).toBe(200);
      await sync.text();
      const cancelled = await post(`/api/games/${f.id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: f.host.session });
      expect(cancelled.status).toBe(200);
      await cancelled.text();
      const baseline = await read(f, "/result", f.host.session);
      const observed = await holdFinal(f, timing, async () => {
        const exit = await post(`/api/games/${f.id}/exit?view=account`, {}, { session: second });
        expect(exit.status).toBe(200);
        expect((await exit.json()).result).toBe("exited");
      });
      if (timing === "after") expect(await read(f, "/result", f.host.session)).toEqual(baseline);
      else await denied(await get(f, "/result", f.host.session));
      assertCut(observed, timing);
      await restore(f);
      for (const session of [f.host.session, second]) await denied(await get(f, "/result", session));
      expect((await read(f, "/result")).result.state).toBe("cancelled");
    } finally { await cleanup(f); }
  });
}

for (const rollback of [false, true]) {
  test(`RUNTIME-FINAL-01 original cap expiry after genuine final reply${rollback ? " with durable denial rollback" : ""}`, async () => {
    const f = await fixture();
    let cap, fenced = false;
    const observed = await holdFinal(f, "after", async () => { cap = observed.cap; Date.now = () => cap; });
    if (rollback) await runInDurableObject(owner(f.id), instance => {
      const held = instance.fetch.bind(instance);
      instance.fetch = async request => {
        if (new URL(request.url).pathname === "/read-expiry-denial") {
          const input = await request.clone().json();
          expect(input.observed_at).toBe(cap);
          Date.now = () => cap - 1;
          fenced = true;
        }
        return held(request);
      };
    });
    try {
      await denied(await get(f, "/membership"));
      assertCut(observed, "after");
      expect(Date.now()).toBe(rollback ? cap - 1 : cap);
      if (rollback) expect(fenced).toBe(true);
      const floor = await runInDurableObject(owner(f.id), (_, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);
      expect(floor).toBeGreaterThanOrEqual(cap);
      await restore(f);
      Date.now = () => cap - 1;
      await denied(await get(f, "/membership"), 503);
    } finally { await cleanup(f); }
  });
}

for (const path of ["/read", "/read-release"]) {
  test(`RUNTIME-FINAL-01 stale ${path} challenge substitution is 503 and fresh revoked request is 401`, async () => {
    const f = await fixture();
    let saved, replay = false, changed = false, releases = 0;
    await runInDurableObject(owner(f.id), instance => {
      instance.snapshotOriginalFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        const route = new URL(request.url).pathname;
        if (route === "/read") releases = 0;
        if (route === "/read-release") releases++;
        const actual = await instance.snapshotOriginalFetch(request);
        if (route === path && (path === "/read" || releases === 2)) {
          const bytes = await actual.clone().arrayBuffer();
          if (!replay) saved = { bytes, status: actual.status, headers: actual.headers };
          else if (!changed) {
            changed = true;
            if (path === "/read-release") expect((await actual.clone().json()).code).toBeNull();
            await recover(f);
            return new Response(saved.bytes, { status: saved.status, headers: saved.headers });
          }
          return new Response(bytes, { status: actual.status, headers: actual.headers });
        }
        return actual;
      };
    });
    try {
      await read(f, "/membership");
      expect(saved).toBeDefined();
      replay = true;
      await denied(await get(f, "/membership"), 503);
      expect(changed).toBe(true);
      await restore(f);
      await denied(await get(f, "/membership"));
    } finally { await cleanup(f); }
  });
}
