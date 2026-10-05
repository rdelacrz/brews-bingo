import { beforeEach, expect, it } from "vitest";
import { cookie, current, enroll, inOwner, password, post, resetStorage, seedLink, seedPending } from "./fixtures.js";

beforeEach(resetStorage);

it("enrolls through A3/A4, rotates at the original deadline, then logs in through A1", async () => {
  const fixture = await seedPending();
  const redeemed = await post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token });
  expect(redeemed.status).toBe(200);
  const restricted = cookie(redeemed);
  const redemption = await redeemed.json();
  expect(redemption.setup_required).toBe(true);
  expect(redemption.session.scope).toBe("EnrollmentOnly");
  const view = await (await current(restricted)).json();
  expect(view.authenticated).toBe(true);
  expect(view.session.scope).toBe("EnrollmentOnly");

  const secret = password();
  const completed = await post("/api/auth/enrollment/complete", { new_password: secret }, { session: restricted });
  expect(completed.status).toBe(200);
  expect(cookie(completed) === restricted).toBe(false);
  const completion = await completed.json();
  expect(completion.account.status).toBe("Verified");
  expect(completion.session.scope).toBe("Normal");
  expect(completion.session.expires_at).toBe(redemption.session.expires_at);
  expect((await (await current(restricted)).json()).authenticated).toBe(false);

  const login = await post("/api/auth/login", { username: ` ${fixture.username} `, password: secret });
  expect(login.status).toBe(200);
  expect((await login.json()).session.scope).toBe("Normal");
  const persisted = await inOwner((_instance, state) => state.storage.sql.exec("SELECT credential_epoch, typeof(verifier) AS storage_type FROM accounts").one());
  expect(persisted.credential_epoch).toBe(1);
  expect(persisted.storage_type).toBe("text");
});

it("resets through A5/A6 without auto-login and invalidates prior Normal sessions", async () => {
  const fixture = await enroll();
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET status='reset_required',credential_epoch=credential_epoch+1 WHERE account_id=?", fixture.accountId));
  const token = await seedLink(fixture.accountId, "password_reset", 2);
  const redeemed = await post("/api/auth/password-reset/redeem", { reset_token: token });
  expect(redeemed.status).toBe(200);
  expect((await redeemed.json()).session.scope).toBe("PasswordResetOnly");
  const secret = password();
  const completed = await post("/api/auth/password-reset/complete", { new_password: secret }, { session: cookie(redeemed) });
  expect(completed.status).toBe(200);
  expect(completed.headers.get("Set-Cookie").includes("Max-Age=0")).toBe(true);
  expect(await completed.json()).toMatchObject({ reset_completed: true, next_action: "login" });
  expect((await (await current(fixture.session)).json()).authenticated).toBe(false);
  const rejected = await post("/api/auth/login", { username: fixture.username, password: fixture.password });
  expect(rejected.status).toBe(401);
  const login = await post("/api/auth/login", { username: fixture.username, password: secret });
  expect(login.status).toBe(200);
});

it("logout A7 revokes only the presented session and leaves other logins valid", async () => {
  const fixture = await enroll();
  const login = await post("/api/auth/login", { username: fixture.username, password: fixture.password });
  expect(login.status).toBe(200);
  const second = cookie(login);
  const logout = await post("/api/auth/logout", undefined, { session: fixture.session, id: null });
  expect(logout.status).toBe(200);
  expect((await (await current(fixture.session)).json()).authenticated).toBe(false);
  expect((await (await current(second)).json()).authenticated).toBe(true);
});

it("simultaneous redemption is single-use and all losers receive generic invalid-link", async () => {
  const fixture = await seedPending();
  const responses = await Promise.all(Array.from({ length: 3 }, () => post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token })));
  expect(responses.filter(r => r.status === 200).length).toBe(1);
  for (const response of responses.filter(r => r.status !== 200)) {
    expect(response.status).toBe(400);
    expect((await response.json()).error.code).toBe("invalid_link");
    expect(response.headers.has("Set-Cookie")).toBe(false);
  }
  const count = await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_sessions").one().n);
  expect(count).toBe(1);
});
