import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { cookie, enroll, origin, password, post, resetStorage } from "./fixtures.js";

const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async () => {
  await reset();
  await resetStorage();
  await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),
    async (_instance, state) => state.storage.deleteAll());
});
const sync = (f, known, session = f.session, view = "player") => exports.default.fetch(
  `${origin}/api/games/${f.id}/sync?view=${view}${known === undefined ? "" : `&known_revision=${known}`}`,
  { headers: { Cookie: session } });
async function setup(terminal = false) {
  const actor = await enroll("SyncReleaseHost");
  const created = await post("/api/games", {}, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const lobby = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(lobby.status).toBe(200);
  const code = (await lobby.json()).game_code;
  const answer = password();
  const sockets = [];
  let session;
  for (const alias of terminal ? ["ReleasePlayer", "SecondPlayer"] : ["ReleasePlayer"]) {
    const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
    const joined = await post(`/api/games/${id}/players`, { game_code: code, alias, recovery_answer: answer }, { session: cookie(context) });
    expect(joined.status).toBe(200);
    if (!session) session = cookie(joined);
    if (terminal) {
      const stream = await exports.default.fetch(`${origin}/api/games/${id}/stream?view=player`,
        { headers: { Cookie: cookie(joined), Origin: origin, Upgrade: "websocket", Connection: "Upgrade" } });
      expect(stream.status).toBe(101);
      stream.webSocket.accept();
      sockets.push(stream.webSocket);
    }
  }
  const f = { id, code, answer, session, actor, sockets };
  if (terminal) {
    const host = await sync(f, undefined, actor.session, "account");
    expect(host.status).toBe(200);
    const revision = (await host.json()).view_revision;
    expect((await post(`/api/games/${id}/start`, { expected_revision: revision }, { session: actor.session })).status).toBe(200);
    expect((await post(`/api/games/${id}/cancel`, { confirmed: true, expected_state: "in_progress" }, { session: actor.session })).status).toBe(200);
  }
  return f;
}
async function holdSync(f, beforeDelivery, bodyRead = false) {
  await runInDurableObject(owner(f.id), (instance, state) => {
    instance.syncReleaseOriginalFetch = instance.fetch.bind(instance);
    let intercepted = false;
    instance.fetch = async request => {
      const input = await request.clone().json();
      const response = await instance.syncReleaseOriginalFetch(request);
      if (intercepted || input.operation !== "sync" || input.view !== "player") return response;
      intercepted = true;
      const text = await response.text();
      const captured = JSON.parse(text);
      if (bodyRead) {
        return new Response(new ReadableStream({ async start(controller) {
          await beforeDelivery(captured, instance, state);
          controller.enqueue(new TextEncoder().encode(text));
          controller.close();
        } }), { status: response.status, headers: response.headers });
      }
      await beforeDelivery(captured, instance, state);
      return new Response(text, { status: response.status, headers: response.headers });
    };
  });
  return () => runInDurableObject(owner(f.id), instance => {
    if (instance.syncReleaseOriginalFetch) {
      instance.fetch = instance.syncReleaseOriginalFetch;
      delete instance.syncReleaseOriginalFetch;
    }
  });
}
async function assertDenied(response) {
  expect([401, 503]).toContain(response.status);
  expect(response.headers.get("set-cookie")).toBeNull();
  expect(Object.keys(await response.json())).toEqual(["error"]);
}
for (const terminal of [false, true]) {
  test(`healthy ${terminal ? "terminal final-access" : "live"} player snapshot and up_to_date release`, async () => {
    const f = await setup(terminal);
    try {
      const snapshot = await sync(f);
      expect(snapshot.status).toBe(200);
      expect(snapshot.headers.get("set-cookie")).toBeNull();
      const first = await snapshot.json();
      expect(first.snapshot.player_id).toBeDefined();
      if (terminal) expect(first.snapshot.state).toBe("cancelled");
      const current = await sync(f, first.view_revision);
      expect(current.status).toBe(200);
      expect(await current.json()).toEqual({ up_to_date: true, view_revision: first.view_revision, snapshot: null });
    } finally { for (const socket of f.sockets) socket.close(1000); }
  });
  for (const upToDate of [false, true]) {
    test(`delayed ${terminal ? "terminal Exit" : "Recovery"} ${upToDate ? "up_to_date" : "snapshot"} cannot release original Sync`, async () => {
      const f = await setup(terminal);
      const first = await sync(f);
      expect(first.status).toBe(200);
      const revision = (await first.json()).view_revision;
      let successor;
      let changedBeforeDelivery = false;
      const restore = await holdSync(f, async proof => {
        expect(proof.owner_result).toBe("synced");
        expect(proof.response.up_to_date).toBe(upToDate);
        expect(Date.now()).toBeLessThan(proof.valid_until);
        const changed = terminal
          ? await post(`/api/games/${f.id}/exit?view=player`, {}, { session: f.session })
          : await post("/api/games/player-recovery", { game_code: f.code, alias: "ReleasePlayer", recovery_answer: f.answer });
        expect(changed.status).toBe(200);
        successor = terminal ? null : cookie(changed);
        await changed.text();
        changedBeforeDelivery = true;
        expect(Date.now()).toBeLessThan(proof.valid_until);
      }, upToDate);
      try {
        await assertDenied(await sync(f, upToDate ? revision : undefined));
        expect(changedBeforeDelivery).toBe(true);
        await restore();
        if (successor) expect((await sync(f, undefined, successor)).status).toBe(200);
        else expect((await sync(f, undefined, f.actor.session, "account")).status).toBe(200);
      } finally {
        await restore();
        for (const socket of f.sockets) socket.close(1000);
      }
    });
  }
}
for (const fault of ["missing", "identity", "deadline"]) {
  test(`player Sync fails closed on ${fault} release proof`, async () => {
    const f = await setup();
    await runInDurableObject(owner(f.id), instance => {
      instance.syncReleaseOriginalFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        const response = await instance.syncReleaseOriginalFetch(request.clone());
        if (new URL(request.url).pathname !== "/sync-player-release") return response;
        const reply = await response.json();
        expect(reply.release_result).toBe("released");
        if (fault === "missing") delete reply.authority;
        else if (fault === "identity") reply.authority.epoch++;
        else reply.authority.valid_until++;
        return Response.json(reply);
      };
    });
    try { await assertDenied(await sync(f)); }
    finally { await runInDurableObject(owner(f.id), instance => {
      instance.fetch = instance.syncReleaseOriginalFetch; delete instance.syncReleaseOriginalFetch;
    }); }
  });
}
for (const cut of ["sync", "release"]) {
  test(`player ${cut} receive expiry stays denied through a rollback during fencing`, async () => {
    const f = await setup();
    const realNow = Date.now;
    let deadline, releases = 0;
    await runInDurableObject(owner(f.id), instance => {
      instance.syncReleaseOriginalFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        const input = await request.clone().json();
        const path = new URL(request.url).pathname;
        if (input.expiry) Date.now = () => deadline - 1;
        if (path === "/sync-player-release") releases++;
        const response = await instance.syncReleaseOriginalFetch(request);
        if (input.operation === "sync") {
          const reply = await response.clone().json();
          if (reply.owner_result === "synced") {
            deadline = reply.valid_until;
            if (cut === "sync") Date.now = () => deadline;
          }
        } else if (cut === "release" && path === "/sync-player-release") Date.now = () => deadline;
        return response;
      };
    });
    try {
      await assertDenied(await sync(f));
      expect(releases).toBe(cut === "sync" ? 0 : 1);
      const floor = await runInDurableObject(owner(f.id), (_instance, state) =>
        state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);
      expect(floor).toBeGreaterThanOrEqual(deadline);
      Date.now = () => deadline - 1;
      await assertDenied(await sync(f));
    } finally {
      Date.now = realNow;
      await runInDurableObject(owner(f.id), instance => {
        instance.fetch = instance.syncReleaseOriginalFetch; delete instance.syncReleaseOriginalFetch;
      });
    }
  });
}
test("terminal access pruned at the original expiry cannot release a held snapshot after rollback", async () => {
  const f = await setup(true);
  const realNow = Date.now;
  let pruned = false;
  const restore = await holdSync(f, async (reply, instance, state) => {
    const proof = reply.player_authority;
    Date.now = () => proof.valid_until;
    await instance.alarm();
    expect(state.storage.sql.exec("SELECT COUNT(*) AS count FROM game_terminal_views WHERE session_id=?", proof.session_id).one().count).toBe(0);
    pruned = true;
    Date.now = () => proof.valid_until - 1;
  }, true);
  try {
    await assertDenied(await sync(f));
    expect(pruned).toBe(true);
  } finally {
    Date.now = realNow; await restore();
    for (const socket of f.sockets) socket.close(1000);
  }
});
