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
async function setup() {
  const actor = await enroll("RecoveryDeadlineHost");
  const created = await post("/api/games", {}, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const lobby = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(lobby.status).toBe(200);
  const code = (await lobby.json()).game_code;
  const answer = password();
  const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
  const joined = await post(`/api/games/${id}/players`, { game_code: code, alias: "DeadlinePlayer", recovery_answer: answer }, { session: cookie(context) });
  expect(joined.status).toBe(200);
  return { id, code, answer, session: cookie(joined) };
}
async function restore(id) {
  await runInDurableObject(owner(id), instance => {
    instance.fetch = instance.recoveryDeadlineHolder.fetch;
    delete instance.recoveryDeadlineHolder;
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
