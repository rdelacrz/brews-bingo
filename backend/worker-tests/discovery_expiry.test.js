import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, inOwner } from "./fixtures.js";
const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const get = session => exports.default.fetch(`${origin}/api/games/current`, { headers: { Cookie: session } });
beforeEach(async () => {
  await reset(); await resetStorage();
  await runInDurableObject(directory(), async (_, state) => state.storage.deleteAll());
});

for (const [slot, cuts] of [["occupied", [1, 2, 3, 4, 5, 6]], ["free", [1, 2]]]) {
  for (const cut of cuts) {
    test(`${slot} genuine Accounts expiry after await at cut ${cut} fences rollback before denial arrives`, async () => {
      const actor = await enroll("DiscoveryExpiryHost");
      if (slot === "occupied") expect((await post("/api/games", {}, { session: actor.session })).status).toBe(200);
      const expires = Date.now() + 5000, originalNow = Date.now;
      await inOwner((instance, state) => {
        state.storage.sql.exec("UPDATE account_sessions SET expires_at=? WHERE account_id=?", expires, actor.accountId);
        instance.expiryOriginalFetch = instance.fetch.bind(instance); instance.expiryCount = 0;
        instance.fetch = async request => {
          const body = await request.clone().json();
          if (body.action === "fence_expiry") Date.now = () => expires - 1;
          const response = await instance.expiryOriginalFetch(request.clone());
          if (body.action === "authorize" && ++instance.expiryCount === cut) Date.now = () => expires;
          return response;
        };
      });
      try {
        const denied = await get(actor.session);
        expect(denied.status).toBe(401);
        expect(Object.keys(await denied.json())).toEqual(["error"]);
        Date.now = () => expires - 1;
        expect((await get(actor.session)).status).toBe(503);
        expect(await inOwner((_, state) => state.storage.sql.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms)).toBeGreaterThanOrEqual(expires);
      } finally {
        Date.now = originalNow;
        await inOwner(instance => { instance.fetch = instance.expiryOriginalFetch; });
      }
    });
  }
}

for (const [slot, cut] of [["free", 1], ["occupied", 1], ["occupied", 2]]) {
  test(`${slot} session expiry after discovery reply ${cut} fences original edge sample`, async () => {
    const actor = await enroll("DiscoveryReturnExpiry");
    let id;
    if (slot === "occupied") {
      const created = await post("/api/games", {}, { session: actor.session });
      expect(created.status).toBe(200); id = (await created.json()).game.game_id;
    }
    const expires = Date.now() + 5000, originalNow = Date.now;
    await inOwner((instance, state) => {
      state.storage.sql.exec("UPDATE account_sessions SET expires_at=? WHERE account_id=?", expires, actor.accountId);
      instance.returnAccountsFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        if ((await request.clone().json()).action === "fence_expiry") Date.now = () => expires - 1;
        return instance.returnAccountsFetch(request);
      };
    });
    const peer = slot === "free" ? directory() : env.GAMES.get(env.GAMES.idFromName(id));
    await runInDurableObject(peer, instance => {
      instance.returnPeerFetch = instance.fetch.bind(instance); instance.returnPeerCount = 0;
      instance.fetch = async request => {
        const response = await instance.returnPeerFetch(request);
        if (new URL(request.url).pathname === "/discovery" && ++instance.returnPeerCount === cut) Date.now = () => expires;
        return response;
      };
    });
    try {
      expect((await get(actor.session)).status).toBe(401);
      Date.now = () => expires - 1;
      expect((await get(actor.session)).status).toBe(503);
      expect(await inOwner((_, state) => state.storage.sql.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms)).toBeGreaterThanOrEqual(expires);
    } finally {
      Date.now = originalNow;
      await inOwner(instance => { instance.fetch = instance.returnAccountsFetch; });
      await runInDurableObject(peer, instance => { instance.fetch = instance.returnPeerFetch; });
    }
  });
}

for (const fault of ["ignore", "abort", "clock_readback", "readback_failure", "sync", "ack_sample", "ack_identity", "ack_rejected"]) {
  test(`required Accounts expiry fence ${fault} failure stays unavailable with no protected output`, async () => {
    const actor = await enroll("DiscoveryFenceFault");
    const expires = Date.now() + 5000, originalNow = Date.now;
    await inOwner((instance, state) => {
      state.storage.sql.exec("UPDATE account_sessions SET expires_at=? WHERE account_id=?", expires, actor.accountId);
      instance.faultAccountsFetch = instance.fetch.bind(instance);
      instance.expiryFaultHolder = {
        sync: state.storage.sync.bind(state.storage),
        exec: state.storage.sql.exec.bind(state.storage.sql),
        alarm: instance.alarm.bind(instance),
      };
      instance.alarm = async () => new Response("test-only alarm held");
      instance.faultArmed = false;
      instance.fetch = async request => {
        const body = await request.clone().json();
        if (body.action === "fence_expiry") {
          Date.now = () => expires - 1;
          instance.faultArmed = true;
          if (fault === "ignore") state.storage.sql.exec(`CREATE TRIGGER discovery_account_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${expires} BEGIN SELECT RAISE(IGNORE); END`);
          if (fault === "abort") state.storage.sql.exec(`CREATE TRIGGER discovery_account_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${expires} BEGIN SELECT RAISE(ABORT,'fixture'); END`);
          if (fault === "clock_readback") state.storage.sql.exec(`CREATE TRIGGER discovery_account_fault AFTER UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${expires} BEGIN UPDATE storage_metadata SET last_observed_ms=OLD.last_observed_ms WHERE singleton=1; END`);
          if (fault === "readback_failure") state.storage.sql.exec = (sql, ...params) => {
            if (sql === "SELECT last_observed_ms FROM storage_metadata WHERE singleton=1" && instance.expiryFaultHolder.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms >= expires) throw new Error("test-only expiry readback fault");
            return instance.expiryFaultHolder.exec(sql, ...params);
          };
          if (fault === "sync") state.storage.sync = async () => { throw new Error("test-only expiry sync fault"); };
        }
        const response = await instance.faultAccountsFetch(request);
        if (body.action === "authorize" && !instance.faultArmed) Date.now = () => expires;
        if (body.action === "fence_expiry" && fault.startsWith("ack_")) {
          const reply = await response.json();
          if (fault === "ack_sample") reply.outcome.observed_at -= 1;
          if (fault === "ack_identity") reply.outcome.authority.epoch += 1;
          if (fault === "ack_rejected") reply.outcome = { result: "rejected", code: "unauthorized" };
          return new Response(JSON.stringify(reply), { headers: { "Content-Type": "application/json" } });
        }
        return response;
      };
    });
    try {
      const response = await get(actor.session);
      expect(response.status).toBe(503);
      expect(Object.keys(await response.json())).toEqual(["error"]);
      expect(response.headers.has("Set-Cookie")).toBe(false);
      expect(await inOwner(instance => instance.faultArmed)).toBe(true);
      if (["ignore", "abort", "clock_readback", "readback_failure"].includes(fault)) {
        expect(await inOwner((instance, state) => instance.expiryFaultHolder.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms)).toBeLessThan(expires);
      }
    } finally {
      Date.now = originalNow;
      await inOwner((instance, state) => {
        instance.fetch = instance.faultAccountsFetch;
        instance.alarm = instance.expiryFaultHolder.alarm;
        state.storage.sync = instance.expiryFaultHolder.sync; state.storage.sql.exec = instance.expiryFaultHolder.exec;
        state.storage.sql.exec("DROP TRIGGER IF EXISTS discovery_account_fault");
      });
    }
  });
}

for (const malformed of ["action", "target", "epoch", "expires", "unknown", "canonical_id", "row_identity"]) {
  test(`malformed Accounts ${malformed} cannot forge a trusted expiry fence`, async () => {
    const actor = await enroll("DiscoveryMalformedFence");
    const expires = Date.now() + 5000, originalNow = Date.now;
    await inOwner((instance, state) => {
      state.storage.sql.exec("UPDATE account_sessions SET expires_at=? WHERE account_id=?", expires, actor.accountId);
      instance.malformedFetch = instance.fetch.bind(instance); instance.malformedFences = 0;
      instance.fetch = async request => {
        const body = await request.clone().json();
        if (body.action === "fence_expiry") { instance.malformedFences++; Date.now = () => expires - 1; }
        const response = await instance.malformedFetch(request);
        if (body.action !== "authorize") return response;
        const reply = await response.json(); Date.now = () => expires;
        if (malformed === "action") reply.action = "register";
        if (malformed === "target") reply.target = { game_id: actor.accountId, connection_id: actor.accountId };
        if (malformed === "epoch") reply.outcome.authority.epoch = -1;
        if (malformed === "expires") reply.outcome.authority.expires = 0;
        if (malformed === "unknown") reply.outcome.authority.browser_sample = expires;
        if (malformed === "canonical_id") reply.outcome.authority.session_id = "01900000-0000-4000-8000-000000000001";
        if (malformed === "row_identity") reply.outcome.authority.session_id = "01900000-0000-7000-8000-000000000001";
        return new Response(JSON.stringify(reply), { headers: { "Content-Type": "application/json" } });
      };
    });
    try {
      const response = await get(actor.session);
      expect(response.status).toBe(503);
      expect(Object.keys(await response.json())).toEqual(["error"]);
      expect(await inOwner((_, state) => state.storage.sql.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms)).toBeLessThan(expires);
      expect(await inOwner(instance => instance.malformedFences)).toBe(malformed === "row_identity" ? 1 : 0);
      Date.now = () => expires - 1;
      await inOwner(instance => { instance.fetch = instance.malformedFetch; });
      expect((await get(actor.session)).status).toBe(200);
    } finally {
      Date.now = originalNow;
      await inOwner(instance => { instance.fetch = instance.malformedFetch; });
    }
  });
}
