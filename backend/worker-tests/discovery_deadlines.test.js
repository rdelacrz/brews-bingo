import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie } from "./fixtures.js";

const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
const get = (path, session) => exports.default.fetch(origin + path, { headers: session ? { Cookie: session } : {} });
beforeEach(async () => {
  await reset(); await resetStorage();
  await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll());
});

async function setup(view) {
  const actor = await enroll("DiscoveryDeadlineHost");
  const creation = await post("/api/games", {}, { session: actor.session });
  expect(creation.status).toBe(200);
  const id = (await creation.json()).game.game_id;
  const lobby = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(lobby.status).toBe(200);
  const code = (await lobby.json()).game_code;
  if (view === "entry") {
    const deadline = await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT idle_due FROM game_record").one().idle_due);
    return { id, deadline, path: `/api/game-codes/${code}`, session: undefined };
  }
  const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
  expect(context.status).toBe(200);
  const joined = await post(`/api/games/${id}/players`, { game_code: code, alias: "DeadlinePlayer" }, { session: cookie(context) });
  expect(joined.status).toBe(200);
  const deadline = await runInDurableObject(owner(id), (_instance, state) => {
    const expires = state.storage.sql.exec("SELECT expires_at FROM game_sessions").one().expires_at;
    state.storage.sql.exec("UPDATE game_record SET idle_due=?", expires + 5000);
    return expires;
  });
  return { id, deadline, path: `/api/games/${id}/availability?view=player`, session: cookie(joined) };
}

async function intercept(id, deadline, cut, fault) {
  await runInDurableObject(owner(id), (instance, state) => {
    instance.deadlineHolder = { fetch: instance.fetch.bind(instance), sync: state.storage.sync.bind(state.storage) };
    instance.deadlineReads = 0;
    instance.fetch = async request => {
      if (new URL(request.url).pathname !== "/discovery") return instance.deadlineHolder.fetch(request);
      const body = await request.clone().json();
      if (body.expiry) {
        Date.now = () => deadline - 1;
        if (fault === "ignore") state.storage.sql.exec("CREATE TRIGGER deny_expiry BEFORE UPDATE OF last_observed_ms ON game_metadata BEGIN SELECT RAISE(IGNORE); END");
        if (fault === "abort") state.storage.sql.exec("CREATE TRIGGER deny_expiry BEFORE UPDATE OF last_observed_ms ON game_metadata BEGIN SELECT RAISE(ABORT,'denial unavailable'); END");
        if (fault === "readback") state.storage.sql.exec("CREATE TRIGGER deny_expiry AFTER UPDATE OF last_observed_ms ON game_metadata BEGIN UPDATE game_metadata SET last_observed_ms=OLD.last_observed_ms; END");
        if (fault === "sync") state.storage.sync = async () => { throw new Error("denial durability unavailable"); };
        const response = await instance.deadlineHolder.fetch(request);
        if (fault === "wrong_ack" || fault === "wrong_sample") {
          const reply = await response.json();
          if (fault === "wrong_ack") reply.outcome = { result: "unauthorized" };
          else reply.outcome.expiry.observed_at -= 1;
          return Response.json(reply);
        }
        return response;
      }
      const response = await instance.deadlineHolder.fetch(request);
      if (++instance.deadlineReads === cut) Date.now = () => deadline;
      return response;
    };
  });
}
async function restore(id) {
  await runInDurableObject(owner(id), (instance, state) => {
    instance.fetch = instance.deadlineHolder.fetch;
    state.storage.sync = instance.deadlineHolder.sync;
    state.storage.sql.exec("DROP TRIGGER IF EXISTS deny_expiry");
    delete instance.deadlineHolder; delete instance.deadlineReads;
  });
}

for (const view of ["entry", "player"]) {
  for (const cut of [1, 2]) {
    test(`${view} expiry after owner reply ${cut} retains the edge sample despite rollback before denial`, async () => {
      const { id, deadline, path, session } = await setup(view);
      const originalNow = Date.now;
      await intercept(id, deadline, cut);
      try {
        const denied = await get(path, session);
        expect(denied.status).toBe(view === "entry" ? 404 : 503);
        expect(Object.keys(await denied.json())).toEqual(["error"]);
        expect(denied.headers.get("set-cookie")).toBeNull();
        const floor = await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);
        expect(floor).toBeGreaterThanOrEqual(deadline);
        Date.now = () => deadline - 1;
        expect((await get(path, session)).status).toBe(503);
      } finally { Date.now = originalNow; await restore(id); }
    });
  }
}

for (const fault of ["ignore", "abort", "readback", "sync", "wrong_ack", "wrong_sample"]) {
  test(`required Game expiry fence ${fault} failure cannot acknowledge or disclose a projection`, async () => {
    const { id, deadline, path } = await setup("entry");
    const originalNow = Date.now;
    await intercept(id, deadline, 2, fault);
    try {
      const denied = await get(path);
      expect(denied.status).toBe(503);
      expect(Object.keys(await denied.json())).toEqual(["error"]);
      expect(denied.headers.get("set-cookie")).toBeNull();
      if (["ignore", "abort", "readback"].includes(fault)) {
        expect(await runInDurableObject(owner(id), (_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBeLessThan(deadline);
      }
    } finally { Date.now = originalNow; await restore(id); }
  });
}
