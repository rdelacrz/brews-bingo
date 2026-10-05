import { beforeEach, expect, it } from "vitest";
import { runDurableObjectAlarm } from "cloudflare:test";
import { cookie, current, inOwner, post, resetStorage, seedPending, stub } from "./fixtures.js";

beforeEach(resetStorage);

it("anonymous access succeeds without deadlines or a spurious alarm", async () => {
  const response = await current();
  expect(response.status).toBe(200);
  expect((await response.json()).authenticated).toBe(false);
  expect(await inOwner((_instance, state) => state.storage.getAlarm())).toBe(null);
});

it("sets the earliest absolute expiry and cleans expired authority in a durable alarm", async () => {
  const fixture = await seedPending();
  const redemption = await post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token });
  expect(redemption.status).toBe(200);
  const session = cookie(redemption);
  const body = await redemption.json();
  const alarm = await inOwner((_instance, state) => state.storage.getAlarm());
  expect(alarm).toBe(body.session.expires_at);
  expect(alarm - Date.now()).toBeLessThanOrEqual(86400000);

  await inOwner((_instance, state) => {
    const now = Date.now();
    state.storage.sql.exec("UPDATE account_sessions SET issued_at=?,expires_at=?", now - 1000, now);
  });
  expect(await runDurableObjectAlarm(stub())).toBe(true);
  expect((await (await current(session)).json()).authenticated).toBe(false);
  const sessions = await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_sessions").one().n);
  expect(sessions).toBe(0);
});
