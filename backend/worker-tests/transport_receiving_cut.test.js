import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, origin, post, resetStorage, stub } from "./fixtures.js";

beforeEach(async () => { await reset(); await resetStorage(); });

for (const boundary of ["idle", "account"]) {
  test(`account Sync preserves initial ${boundary} expiry before another authorization await`, async () => {
    const actor = await enroll(`ReceivingCut${boundary}`);
    const created = await post("/api/games", {}, { session: actor.session });
    expect(created.status).toBe(200);
    const id = (await created.json()).game.game_id;
    const game = env.GAMES.get(env.GAMES.idFromName(id));
    const accountExpiry = await runInDurableObject(stub(), (_instance, state) =>
      state.storage.sql.exec("SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL").one().expires_at);
    const deadline = await runInDurableObject(game, (_instance, state) => {
      if (boundary === "account") {
        state.storage.sql.exec("UPDATE game_record SET idle_due=?", accountExpiry + 5000);
        return accountExpiry;
      }
      return state.storage.sql.exec("SELECT idle_due FROM game_record").one().idle_due;
    });
    const realNow = Date.now;
    let expiredReply = false;
    let postReplyAuthorizations = 0;
    await runInDurableObject(stub(), instance => {
      instance.receivingCutFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        const body = await request.clone().json();
        if (expiredReply && new URL(request.url).pathname === "/game-authority" && body.action === "authorize") {
          postReplyAuthorizations++;
          Date.now = () => deadline - 1;
        }
        return instance.receivingCutFetch(request);
      };
    });
    await runInDurableObject(game, instance => {
      instance.receivingCutFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        const body = await request.clone().json();
        const response = await instance.receivingCutFetch(request);
        if (body.operation === "sync" && !expiredReply) {
          expiredReply = true;
          Date.now = () => deadline;
        }
        return response;
      };
    });
    const sync = () => exports.default.fetch(`${origin}/api/games/${id}/sync?view=account`, {
      headers: { Cookie: actor.session },
    });
    try {
      const denied = await sync();
      expect(expiredReply).toBe(true);
      expect(denied.status).toBe(401);
      expect((await denied.json()).snapshot).toBeUndefined();
      expect(postReplyAuthorizations).toBe(0);
      const floor = await runInDurableObject(game, (_instance, state) =>
        state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);
      expect(floor).toBeGreaterThanOrEqual(deadline);
      if (boundary === "account") {
        const accountFloor = await runInDurableObject(stub(), (_instance, state) =>
          state.storage.sql.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms);
        expect(accountFloor).toBeGreaterThanOrEqual(deadline);
      }
      Date.now = () => deadline - 1;
      expect((await sync()).status).toBe(503);
    } finally {
      Date.now = realNow;
      for (const target of [game, stub()]) {
        await runInDurableObject(target, instance => {
          instance.fetch = instance.receivingCutFetch;
          delete instance.receivingCutFetch;
        });
      }
    }
  });
}
