import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, password, commandId } from "./fixtures.js";
const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async () => {
  await reset(); await resetStorage();
  await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll());
});
async function setup(joinAt) {
  const actor = await enroll("RecoveryDeadlineHost");
  const created = await post("/api/games", {}, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const lobby = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(lobby.status).toBe(200);
  const code = (await lobby.json()).game_code;
  if (joinAt !== undefined) Date.now = () => joinAt;
  const answer = password();
  const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
  const joined = await post(`/api/games/${id}/players`, { game_code: code, alias: "DeadlinePlayer", recovery_answer: answer }, { session: cookie(context) });
  expect(joined.status).toBe(200);
  return { id, code, answer, session: cookie(joined), host: actor };
}
async function restore(id) {
  await runInDurableObject(owner(id), instance => {
    instance.fetch = instance.recoveryDeadlineHolder.fetch;
    delete instance.recoveryDeadlineHolder;
  });
}

for (const mode of ["recovery", "PUT", "DELETE"]) {
  for (const expired of [false, true]) {
    test(`original ${mode} output cap survives host resume ${expired ? "at" : "before"} equality`, async () => {
      const originalNow = Date.now;
      const base = originalNow();
      Date.now = () => base;
      let f;
      try {
        f = await setup(base + 10000);
        Date.now = () => base + 20000;
        await runInDurableObject(owner(f.id), instance => {
          instance.recoveryDeadlineHolder = { fetch: instance.fetch.bind(instance), first: null, second: null, calls: 0 };
          instance.fetch = async request => {
            const body = new URL(request.url).pathname === "/command" ? await request.clone().json() : {};
            const stored = instance.recoveryDeadlineHolder;
            const response = await stored.fetch(request);
            if (response.status === 200 && ["recover_player", "recovery_output", "put_recovery_answer", "delete_recovery_answer"].includes(body.operation)) {
              const reply = await response.clone().json();
              if (["recovered", "recovery_answer"].includes(reply.owner_result)) {
                if (++stored.calls === 1) {
                  stored.first = reply.valid_until;
                  Date.now = () => stored.first - 1000;
                  const resumed = await post(`/api/games/${f.id}/resume`, {}, { session: f.host.session });
                  expect(resumed.status).toBe(200);
                  await resumed.text();
                } else if (stored.calls === 2) {
                  stored.second = reply.valid_until;
                  Date.now = () => stored.first - (expired ? 0 : 1);
                }
              }
            }
            return response;
          };
        });
        const response = mode === "recovery"
          ? await post("/api/games/player-recovery", { game_code: f.code, alias: "DeadlinePlayer", recovery_answer: f.answer })
          : await exports.default.fetch(`${origin}/api/games/${f.id}/membership/recovery-answer`, {
              method: mode,
              headers: { Origin: origin, Cookie: f.session, "Content-Type": "application/json", "Idempotency-Key": commandId() },
              body: JSON.stringify(mode === "PUT" ? { recovery_answer: password() } : {}),
            });
        const observed = await runInDurableObject(owner(f.id), (instance, state) => ({
          first: instance.recoveryDeadlineHolder.first,
          second: instance.recoveryDeadlineHolder.second,
          calls: instance.recoveryDeadlineHolder.calls,
          floor: state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms,
        }));
        expect(observed.calls).toBe(2);
        expect(observed.second).toBeGreaterThan(observed.first);
        if (expired) {
          expect([401, 503]).toContain(response.status);
          expect(response.headers.get("set-cookie")).toBeNull();
          expect(Object.keys(await response.json())).toEqual(["error"]);
          expect(observed.floor).toBeGreaterThanOrEqual(observed.first);
          Date.now = () => observed.first - 1;
          const retried = await post("/api/games/player-recovery", { game_code: f.code, alias: "DeadlinePlayer", recovery_answer: f.answer });
          expect(retried.status).toBe(503);
          expect(retried.headers.get("set-cookie")).toBeNull();
        } else {
          expect(response.status).toBe(200);
          expect(response.headers.has("set-cookie")).toBe(mode === "recovery");
          await response.text();
        }
      } finally {
        Date.now = originalNow;
        if (f) await runInDurableObject(owner(f.id), instance => {
          if (instance.recoveryDeadlineHolder) {
            instance.fetch = instance.recoveryDeadlineHolder.fetch;
            delete instance.recoveryDeadlineHolder;
          }
        });
      }
    });
  }
}

for (const connected of [false, true]) {
  test(`recovery host conditional Sync ${connected ? "invalidates connected" : "preserves offline"} presence revision`, async () => {
    const f = await setup();
    let socket;
    try {
      if (connected) {
        const response = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`, {
          headers: { Cookie: f.session, Origin: origin, Upgrade: "websocket", Connection: "Upgrade" },
        });
        expect(response.status).toBe(101);
        socket = response.webSocket;
        const initial = new Promise((resolve, reject) => {
          const timeout = setTimeout(() => reject(new Error("recovery presence snapshot timeout")), 3000);
          socket.addEventListener("message", event => {
            const frame = JSON.parse(event.data);
            if (frame.kind === "snapshot") {
              socket.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision }));
              clearTimeout(timeout); resolve();
            }
          });
        });
        socket.accept(); await initial;
      }
      const hostSync = async known => {
        const response = await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=account${known === undefined ? "" : `&known_revision=${known}`}`, { headers: { Cookie: f.host.session } });
        expect(response.status).toBe(200);
        return response.json();
      };
      const before = await hostSync();
      expect(before.snapshot.connected_player_count).toBe(connected ? 1 : 0);
      const recovered = await post("/api/games/player-recovery", { game_code: f.code, alias: "DeadlinePlayer", recovery_answer: f.answer });
      expect(recovered.status).toBe(200);
      await recovered.text();
      const cached = await hostSync(before.view_revision);
      const full = await hostSync();
      expect(full.snapshot.connected_player_count).toBe(0);
      expect(cached.up_to_date).toBe(!connected);
      expect(cached.view_revision).toBe(before.view_revision + (connected ? 1 : 0));
      if (connected) expect(cached.snapshot.connected_player_count).toBe(0);
      else expect(cached.snapshot).toBeNull();
    } finally { socket?.close(1000); }
  });
}

for (const cut of ["recover_player", "recovery_output"]) {
  test(`recovery ${cut} deadline observed at edge survives rollback before the next owner await`, async () => {
    const { id, code, answer } = await setup();
    const originalNow = Date.now;
    await runInDurableObject(owner(id), instance => {
      instance.recoveryDeadlineHolder = { fetch: instance.fetch.bind(instance), deadline: null };
      instance.fetch = async request => {
        const body = await request.clone().json();
        const stored = instance.recoveryDeadlineHolder;
        if (stored.deadline !== null && (body.operation === "recovery_output" || body.expiry)) Date.now = () => stored.deadline - 1;
        const response = await stored.fetch(request.clone());
        if (body.operation === cut && response.status === 200) {
          const reply = await response.clone().json();
          expect(reply.owner_result).toBe("recovered");
          stored.deadline = reply.valid_until;
          expect(Number.isSafeInteger(stored.deadline)).toBe(true);
          Date.now = () => stored.deadline;
        }
        return response;
      };
    });
    try {
      const denied = await post("/api/games/player-recovery", { game_code: code, alias: "DeadlinePlayer", recovery_answer: answer });
      expect([401, 503]).toContain(denied.status);
      expect(denied.headers.get("set-cookie")).toBeNull();
      expect(Object.keys(await denied.json())).toEqual(["error"]);
      const observed = await runInDurableObject(owner(id), (instance, state) => ({
        deadline: instance.recoveryDeadlineHolder.deadline,
        floor: state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms,
      }));
      expect(observed.floor).toBeGreaterThanOrEqual(observed.deadline);
      Date.now = () => observed.deadline - 1;
      const retried = await post("/api/games/player-recovery", { game_code: code, alias: "DeadlinePlayer", recovery_answer: answer });
      expect(retried.status).toBe(503);
      expect(retried.headers.get("set-cookie")).toBeNull();
    } finally { Date.now = originalNow; await restore(id); }
  });
}
for (const fault of ["unavailable", "wrong_sample"]) {
  test(`recovery expiry fencing requires an exact durable ACK, not ${fault}`, async () => {
    const { id, code, answer } = await setup();
    const originalNow = Date.now;
    await runInDurableObject(owner(id), instance => {
      instance.recoveryDeadlineHolder = { fetch: instance.fetch.bind(instance), deadline: null };
      instance.fetch = async request => {
        const body = await request.clone().json();
        const stored = instance.recoveryDeadlineHolder;
        if (body.expiry) {
          Date.now = () => stored.deadline - 1;
          return Response.json({
            game_id: body.game_id, checked_at: Date.now(), valid_until: null,
            outcome: fault === "unavailable" ? { result: "unavailable" } : {
              result: "expiry_fenced", expiry: { ...body.expiry, observed_at: body.expiry.observed_at - 1 },
            },
          });
        }
        const response = await stored.fetch(request);
        if (body.operation === "recovery_output" && response.status === 200) {
          const reply = await response.clone().json();
          stored.deadline = reply.valid_until;
          Date.now = () => stored.deadline;
        }
        return response;
      };
    });
    try {
      const denied = await post("/api/games/player-recovery", { game_code: code, alias: "DeadlinePlayer", recovery_answer: answer });
      expect(denied.status).toBe(503);
      expect(denied.headers.get("set-cookie")).toBeNull();
      expect(Object.keys(await denied.json())).toEqual(["error"]);
    } finally { Date.now = originalNow; await restore(id); }
  });
}

for (const method of ["PUT", "DELETE"]) {
  for (const cut of [1, 2]) {
    test(`answer ${method} reply ${cut} fences observed expiry without releasing private data after rollback`, async () => {
      const { id, session } = await setup();
      const originalNow = Date.now;
      await runInDurableObject(owner(id), instance => {
        instance.recoveryDeadlineHolder = { fetch: instance.fetch.bind(instance), deadline: null, reads: 0 };
        instance.fetch = async request => {
          const body = await request.clone().json();
          const stored = instance.recoveryDeadlineHolder;
          if (stored.deadline !== null && (body.expiry || ["put_recovery_answer", "delete_recovery_answer"].includes(body.operation))) Date.now = () => stored.deadline - 1;
          const response = await stored.fetch(request.clone());
          if (["put_recovery_answer", "delete_recovery_answer"].includes(body.operation) && response.status === 200) {
            const reply = await response.clone().json();
            if (reply.owner_result === "recovery_answer" && ++stored.reads === cut) {
              stored.deadline = reply.valid_until;
              Date.now = () => stored.deadline;
            }
          }
          return response;
        };
      });
      try {
        const denied = await exports.default.fetch(`${origin}/api/games/${id}/membership/recovery-answer`, {
          method,
          headers: { Origin: origin, Cookie: session, "Content-Type": "application/json", "Idempotency-Key": commandId() },
          body: JSON.stringify(method === "PUT" ? { recovery_answer: password() } : {}),
        });
        expect(denied.status).toBe(401);
        expect(denied.headers.get("set-cookie")).toBeNull();
        expect(Object.keys(await denied.json())).toEqual(["error"]);
        const observed = await runInDurableObject(owner(id), (instance, state) => ({
          deadline: instance.recoveryDeadlineHolder.deadline,
          floor: state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms,
        }));
        expect(observed.floor).toBeGreaterThanOrEqual(observed.deadline);
      } finally { Date.now = originalNow; await restore(id); }
    });
  }
}
