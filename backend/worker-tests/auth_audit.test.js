import { afterEach, beforeEach, expect, test } from "vitest";
import { exports } from "cloudflare:workers";
import { commandId, cookie, enroll, inOwner, origin, password, post, resetStorage, seedLink, seedPending, stub } from "./fixtures.js";

const CLOCK_READ = "SELECT last_observed_ms FROM storage_metadata WHERE singleton=1";
const CLOCK_READBACK = "SELECT last_observed_ms,command_floor_ms FROM storage_metadata WHERE singleton=1";
const LOGIN_BLOCK_MS = 900_000;
const originalNow = Date.now;

beforeEach(async () => {
  await resetStorage();
  // Keep retired binding rows available; expiry correctness must precede alarm cleanup.
  await inOwner((instance, state) => {
    instance.authAuditHolder = {
      exec: state.storage.sql.exec.bind(state.storage.sql), alarm: instance.alarm.bind(instance), reads: 0,
    };
    instance.alarm = async () => new Response("test-only auth alarm held");
  });
});
afterEach(async () => {
  Date.now = originalNow;
  await inOwner((instance, state) => {
    if (instance.authAuditHolder) {
      state.storage.sql.exec = instance.authAuditHolder.exec;
      instance.alarm = instance.authAuditHolder.alarm;
      delete instance.authAuditHolder;
    }
    state.storage.sql.exec("DROP TRIGGER IF EXISTS auth_audit_clock_fault");
  });
});

async function auth(command, { id = commandId(), caller = "auth-audit-fixture" } = {}) {
  const response = await stub().fetch(`${origin}/auth`, {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command, command_id: id, caller_identity: caller }),
  });
  expect(response.status).toBe(200);
  return response.json();
}
const bearer = session => session.split("=", 2)[1];
const floor = () => inOwner((_instance, state) => state.storage.sql.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms);
async function gameAuth(token) {
  const response = await stub().fetch(`${origin}/game-authority`, {
    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ action: "authorize", token }),
  });
  expect(response.status).toBe(200);
  return response.json();
}
async function authorize(path, fixture) {
  if (path === "current") return auth({ type: "current", token: bearer(fixture.session) });
  if (path === "game") return gameAuth(bearer(fixture.session));
  const response = await exports.default.fetch(`${origin}/api/users?limit=1`, { headers: { Cookie: fixture.session } });
  return { status: response.status, body: await response.json() };
}
async function armClock(cutoff, readsBeforeCut, { fault, mutation } = {}) {
  Date.now = () => cutoff - 1;
  await inOwner((instance, state) => {
    const holder = instance.authAuditHolder;
    holder.reads = 0;
    if (fault === "ignore") holder.exec(`CREATE TRIGGER auth_audit_clock_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${cutoff} BEGIN SELECT RAISE(IGNORE); END`);
    if (fault === "abort") holder.exec(`CREATE TRIGGER auth_audit_clock_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${cutoff} BEGIN SELECT RAISE(ABORT,'fixture'); END`);
    if (fault === "clock_readback") holder.exec(`CREATE TRIGGER auth_audit_clock_fault AFTER UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${cutoff} BEGIN UPDATE storage_metadata SET last_observed_ms=OLD.last_observed_ms WHERE singleton=1; END`);
    if (fault === "floor_readback") holder.exec(`CREATE TRIGGER auth_audit_clock_fault AFTER UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${cutoff} BEGIN UPDATE storage_metadata SET command_floor_ms=OLD.command_floor_ms-1 WHERE singleton=1; END`);
    state.storage.sql.exec = (sql, ...params) => {
      if (sql === CLOCK_READ) {
        holder.reads += 1;
        if (mutation && holder.reads === readsBeforeCut + 1) holder.exec(mutation);
      }
      if (sql === CLOCK_READBACK && fault === "readback_failure"
          && holder.exec("SELECT last_observed_ms FROM storage_metadata").one().last_observed_ms >= cutoff) {
        throw new Error("test-only auth clock readback failure");
      }
      const result = holder.exec(sql, ...params);
      // Advance after the last pre-proof/pre-decision sample has already been taken.
      if (sql === CLOCK_READ && holder.reads === readsBeforeCut && !mutation) Date.now = () => cutoff;
      return result;
    };
  });
}
async function normalFixture(path) {
  const fixture = await enroll("AuthAuditNormal");
  const expiry = Date.now() + 60_000;
  await inOwner((_instance, state) => {
    state.storage.sql.exec("UPDATE account_sessions SET expires_at=? WHERE account_id=?", expiry, fixture.accountId);
    if (path === "admin") state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", fixture.accountId);
  });
  return { ...fixture, expiry };
}

test.each(["current", "game", "admin"])("%s auth durably fences the later expiry decision and denies one-millisecond rollback", async path => {
  const fixture = await normalFixture(path);
  await armClock(fixture.expiry, path === "game" ? 1 : 2);
  const denied = await authorize(path, fixture);
  if (path === "game") expect(denied.outcome).toMatchObject({ result: "rejected", code: "unauthorized" });
  else if (path === "current") expect(denied).toMatchObject({ status: 200, body: { authenticated: false } });
  else expect(denied.status).toBe(401);
  expect(await floor()).toBeGreaterThanOrEqual(fixture.expiry);
  Date.now = () => fixture.expiry - 1;
  const rollback = await authorize(path, fixture);
  if (path === "game") expect(rollback.outcome).toMatchObject({ result: "rejected", code: "unavailable" });
  else expect(rollback.status).toBe(503);
});

for (const path of ["current", "game", "admin"]) {
  test.each(["ignore", "abort", "clock_readback", "floor_readback", "readback_failure"])(`${path} auth propagates %s fence failures`, async fault => {
    const fixture = await normalFixture(path);
    await armClock(fixture.expiry, path === "game" ? 1 : 2, { fault });
    const response = await authorize(path, fixture);
    if (path === "game") expect(response.outcome).toMatchObject({ result: "rejected", code: "unavailable" });
    else expect(response.status).toBe(503);
    if (path !== "game") expect(Object.keys(response.body)).toEqual(["error"]);
    expect(await floor()).toBeLessThan(fixture.expiry);
  });
}

test("link redemption fences its actual expiry decision before rollback", async () => {
  const fixture = await seedPending("AuditLinkDecision");
  const expiry = await inOwner((_instance, state) => state.storage.sql.exec("SELECT expires_at FROM access_links WHERE account_id=?", fixture.accountId).one().expires_at);
  const id = commandId();
  const command = { type: "redeem", purpose: "Enrollment", token: fixture.token };
  await armClock(expiry, 2);
  const denied = await auth(command, { id });
  expect(denied.status).toBe(400);
  expect(denied.body.error.code).toBe("invalid_link");
  expect(denied.cookie.type).toBe("none");
  expect(await floor()).toBeGreaterThanOrEqual(expiry);
  Date.now = () => expiry - 1;
  expect((await auth(command, { id })).status).toBe(503);
});

test("login receipt expiry fences its actual decision before rollback", async () => {
  const fixture = await enroll("AuditLoginDecision");
  const id = commandId();
  const command = { type: "login", username: fixture.username, password: fixture.password };
  const issued = await auth(command, { id });
  expect(issued.status).toBe(200);
  const expiry = await inOwner((_instance, state) => state.storage.sql.exec("SELECT expires_at FROM command_receipts WHERE command_id=?", id).one().expires_at);
  await armClock(expiry, 4);
  const denied = await auth(command, { id });
  expect(denied.status).toBe(409);
  expect(denied.cookie.type).toBe("none");
  expect(await floor()).toBeGreaterThanOrEqual(expiry);
  Date.now = () => expiry - 1;
  expect((await auth(command, { id })).status).toBe(503);
});

async function completed(flow) {
  let fixture;
  let token;
  if (flow === "enrollment") {
    fixture = await seedPending("AuthAuditEnrollment");
    token = fixture.token;
  } else {
    fixture = await enroll("AuthAuditReset");
    await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET status='reset_required',credential_epoch=credential_epoch+1 WHERE account_id=?", fixture.accountId));
    token = await seedLink(fixture.accountId, "password_reset", 2);
  }
  const redeemed = await post(`/api/auth/${flow === "enrollment" ? "enrollment" : "password-reset"}/redeem`, { [flow === "enrollment" ? "enrollment_token" : "reset_token"]: token });
  expect(redeemed.status).toBe(200);
  const restricted = cookie(redeemed);
  const expiry = Date.now() + LOGIN_BLOCK_MS + 60_000;
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_sessions SET expires_at=? WHERE account_id=? AND revoked_at IS NULL", expiry, fixture.accountId));
  const secret = password();
  const id = commandId();
  const scope = flow === "enrollment" ? "EnrollmentOnly" : "PasswordResetOnly";
  const command = { type: "complete", scope, token: bearer(restricted), new_password: secret };
  const initial = await auth(command, { id });
  expect(initial.status).toBe(200);
  if (flow === "enrollment") expect(initial.cookie.expires_at).toBe(expiry);
  else expect(initial.cookie.type).toBe("clear");
  return { ...fixture, secret, restricted, expiry, id, command };
}

for (const flow of ["enrollment", "reset"]) {
  test(`${flow} retired completion cookie cannot return a receipt when expiry crosses password proof`, async () => {
    const fixture = await completed(flow);
    await armClock(fixture.expiry, 5);
    const response = await auth(fixture.command, { id: fixture.id });
    expect(response).toMatchObject({ status: 401, cookie: { type: "none" } });
    expect(Object.keys(response.body)).toEqual(["error"]);
    expect(await floor()).toBe(fixture.expiry);
    Date.now = () => fixture.expiry - 1;
    expect((await auth(fixture.command, { id: fixture.id })).status).toBe(503);
  });

  test.each(["ignore", "abort", "clock_readback", "floor_readback", "readback_failure"])(`${flow} retired completion retry propagates %s clock failure without receipt`, async fault => {
    const fixture = await completed(flow);
    await armClock(fixture.expiry, 5, { fault });
    const response = await auth(fixture.command, { id: fixture.id });
    expect(response).toMatchObject({ status: 503, cookie: { type: "none" } });
    expect(Object.keys(response.body)).toEqual(["error"]);
    expect(await floor()).toBeLessThan(fixture.expiry);
  });

  test.each([
    "UPDATE account_sessions SET expires_at=expires_at-1",
    "UPDATE account_sessions SET revoked_at=revoked_at+1",
    "UPDATE account_sessions SET credential_epoch=credential_epoch+1",
    "UPDATE accounts SET disabled_at=created_at",
    "UPDATE accounts SET credential_epoch=credential_epoch+1",
  ])(`${flow} retired retry rechecks post-proof authority: %s`, async mutation => {
    const fixture = await completed(flow);
    await armClock(fixture.expiry, 5, { mutation });
    const response = await auth(fixture.command, { id: fixture.id });
    expect(await inOwner(instance => instance.authAuditHolder.reads)).toBeGreaterThanOrEqual(6);
    expect(response).toMatchObject({ status: 401, cookie: { type: "none" } });
    expect(Object.keys(response.body)).toEqual(["error"]);
  });

  test(`${flow} retired wrong-password proofs block the same login caller and account buckets`, async () => {
    const fixture = await completed(flow);
    const wrong = password();
    for (let n = 0; n < 5; n++) expect((await auth({ ...fixture.command, new_password: wrong }, { id: fixture.id })).status).toBe(401);
    const buckets = await inOwner((_instance, state) => Array.from(state.storage.sql.exec("SELECT attempt_count,blocked_until FROM rate_limit_buckets WHERE scope='account_login'")));
    expect(buckets).toHaveLength(2);
    expect(buckets.every(bucket => bucket.attempt_count === 5 && bucket.blocked_until !== null)).toBe(true);
    expect((await auth(fixture.command, { id: fixture.id, caller: "other-audit-fixture" })).status).toBe(429);
    expect((await auth({ type: "login", username: fixture.username, password: fixture.secret }, { caller: "other-audit-fixture" })).status).toBe(429);
    expect((await auth({ type: "login", username: "OtherAccount1", password: wrong })).status).toBe(429);
    const blockEnd = Math.max(...buckets.map(bucket => bucket.blocked_until));
    Date.now = () => blockEnd;
    const replay = await auth(fixture.command, { id: fixture.id });
    expect(replay).toMatchObject({ status: 200, cookie: { type: "none" }, body: { replayed: true } });
    expect(JSON.stringify(replay.body).includes(fixture.secret)).toBe(false);
    expect(JSON.stringify(replay.body).includes(bearer(fixture.restricted))).toBe(false);
    expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM rate_limit_buckets WHERE scope='account_login'").one().n)).toBe(0);
  });

  test(`${flow} login failures block completion retries and rate write faults remain unavailable`, async () => {
    const fixture = await completed(flow);
    const wrong = password();
    await inOwner((_instance, state) => state.storage.sql.exec("CREATE TRIGGER auth_audit_rate_fault BEFORE INSERT ON rate_limit_buckets BEGIN SELECT RAISE(ABORT,'fixture'); END"));
    try {
      const response = await auth({ ...fixture.command, new_password: wrong }, { id: fixture.id });
      expect(response).toMatchObject({ status: 503, cookie: { type: "none" } });
    } finally {
      await inOwner((_instance, state) => state.storage.sql.exec("DROP TRIGGER auth_audit_rate_fault"));
    }
    for (let n = 0; n < 5; n++) expect((await auth({ type: "login", username: fixture.username, password: wrong })).status).toBe(401);
    expect((await auth(fixture.command, { id: fixture.id })).status).toBe(429);
  });
}
