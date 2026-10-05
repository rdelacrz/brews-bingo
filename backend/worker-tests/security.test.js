import { exports } from "cloudflare:workers";
import { beforeEach, expect, it } from "vitest";
import { commandId, cookie, current, enroll, inOwner, origin, password, post, resetStorage, seedPending } from "./fixtures.js";

beforeEach(resetStorage);

it("auth retries return secret-free receipts without reissuing cookies", async () => {
  const fixture = await enroll();
  const id = commandId();
  const body = { username: fixture.username, password: fixture.password };
  const first = await post("/api/auth/login", body, { id });
  expect(first.status).toBe(200);
  const retry = await post("/api/auth/login", body, { id });
  expect(retry.status).toBe(200);
  expect(retry.headers.has("Set-Cookie")).toBe(false);
  const receipt = await retry.json();
  expect(receipt.replayed).toBe(true);
  expect(receipt.account).toBe(undefined);
  expect(receipt.session).toBe(undefined);
  const count = await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_sessions").one().n);
  expect(count).toBe(2);
  const wrong = await post("/api/auth/login", { ...body, password: password() }, { id });
  expect(wrong.status).toBe(401);
  expect(wrong.headers.has("Set-Cookie")).toBe(false);
});

it("passwords allow exact ASCII controls but reject the 51-character input", async () => {
  const fixture = await seedPending();
  const redemption = await post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token });
  expect(redemption.status).toBe(200);
  const restricted = cookie(redemption);
  const secret = String.fromCharCode(...new Uint8Array(50));
  const tooLong = await post("/api/auth/enrollment/complete", { new_password: `${secret}x` }, { session: restricted });
  expect(tooLong.status).toBe(400);
  expect(tooLong.headers.has("Set-Cookie")).toBe(false);
  const complete = await post("/api/auth/enrollment/complete", { new_password: secret }, { session: restricted });
  expect(complete.status).toBe(200);
  const login = await post("/api/auth/login", { username: fixture.username, password: secret });
  expect(login.status).toBe(200);
  expect((await (await current(cookie(login))).json()).authenticated).toBe(true);
});

it("NUL and DEL survive username SQLite storage and exact case-sensitive lookup", async () => {
  const username = `User${String.fromCharCode(0)}name${String.fromCharCode(127)}X`;
  const fixture = await seedPending(username);
  const redemption = await post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token });
  expect(redemption.status).toBe(200);
  const secret = password();
  expect((await post("/api/auth/enrollment/complete", { new_password: secret }, { session: cookie(redemption) })).status).toBe(200);
  expect((await post("/api/auth/login", { username: `\v ${username} \v`, password: secret })).status).toBe(200);
  expect((await post("/api/auth/login", { username: username.toLowerCase(), password: secret })).status).toBe(401);
});

it("disabled accounts and changed epochs invalidate existing cookies immediately", async () => {
  const fixture = await enroll();
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET disabled_at=? WHERE account_id=?", Date.now(), fixture.accountId));
  expect((await (await current(fixture.session)).json()).authenticated).toBe(false);
  expect((await post("/api/auth/login", { username: fixture.username, password: fixture.password })).status).toBe(401);
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET disabled_at=NULL,credential_epoch=credential_epoch+1 WHERE account_id=?", fixture.accountId));
  expect((await (await current(fixture.session)).json()).authenticated).toBe(false);
});

it("link purpose failures are generic and do not consume the link", async () => {
  const fixture = await seedPending();
  const wrong = await post("/api/auth/password-reset/redeem", { reset_token: fixture.token });
  expect(wrong.status).toBe(400);
  expect((await wrong.json()).error.code).toBe("invalid_link");
  expect(wrong.headers.has("Set-Cookie")).toBe(false);
  const unconsumed = await inOwner((_instance, state) => state.storage.sql.exec("SELECT consumed_at FROM access_links").one().consumed_at);
  expect(unconsumed).toBe(null);
  expect((await post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token })).status).toBe(200);
});

it("failure-only throttles persist across requests and do not create sessions", async () => {
  const fixture = await enroll();
  for (let i = 0; i < 5; i++) {
    expect((await post("/api/auth/login", { username: fixture.username, password: password() })).status).toBe(401);
  }
  const blocked = await post("/api/auth/login", { username: fixture.username, password: fixture.password });
  expect(blocked.status).toBe(429);
  expect(blocked.headers.has("Set-Cookie")).toBe(false);
  const count = await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_sessions").one().n);
  expect(count).toBe(1);
});

it("an auth schema newer than this binary fails closed", async () => {
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE storage_metadata SET schema_version=2"));
  const response = await current();
  expect(response.status).toBe(503);
  expect(response.headers.has("Set-Cookie")).toBe(false);
  expect((await response.json()).error.code).toBe("service_unavailable");
});

it("body/query/method validation fails before owner credential work", async () => {
  const headers = { Origin: origin, "Content-Type": "application/json", "Idempotency-Key": commandId() };
  const tooLarge = await exports.default.fetch(`${origin}/api/auth/login`, { method: "POST", headers, body: " ".repeat(4097) });
  expect(tooLarge.status).toBe(413);
  const query = await exports.default.fetch(`${origin}/api/session?forbidden=value`);
  expect(query.status).toBe(400);
  const method = await exports.default.fetch(`${origin}/api/auth/login`);
  expect(method.status).toBe(405);
});
