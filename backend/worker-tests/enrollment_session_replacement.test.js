import { beforeEach, expect, it } from "vitest";
import { cookie, current, enroll, post, resetStorage, seedPending } from "./fixtures.js";

beforeEach(resetStorage);

it("valid enrollment redemption retires the current browser session, but an invalid link does not", async () => {
  const existing = await enroll("ExistingSessionHost");
  const pending = await seedPending("EnrollmentLinkHost");

  const invalid = await post(
    "/api/auth/enrollment/redeem",
    { enrollment_token: "not-a-valid-link-token" },
    { session: existing.session },
  );
  expect(invalid.status).toBe(400);
  expect(invalid.headers.has("Set-Cookie")).toBe(false);
  expect((await (await current(existing.session)).json()).authenticated).toBe(true);

  const redeemed = await post(
    "/api/auth/enrollment/redeem",
    { enrollment_token: pending.token },
    { session: existing.session },
  );
  expect(redeemed.status).toBe(200);
  expect(redeemed.headers.has("Set-Cookie")).toBe(true);

  const retired = await (await current(existing.session)).json();
  expect(retired).toMatchObject({ authenticated: false, session: null });
  const restricted = await (await current(cookie(redeemed))).json();
  expect(restricted).toMatchObject({
    authenticated: true,
    account: { account_id: pending.accountId, username: pending.username, status: "PendingEnrollment" },
    session: { account_id: pending.accountId, scope: "EnrollmentOnly" },
  });
});