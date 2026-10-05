import { beforeEach, expect, it } from "vitest";
import { cookie, inOwner, password, post, resetStorage, seedPending } from "./fixtures.js";

beforeEach(resetStorage);

it("owner transaction rolls back enrollment changes when session insertion fails", async () => {
  const fixture = await seedPending();
  const redemption = await post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token });
  expect(redemption.status).toBe(200);
  await inOwner((_instance, state) => state.storage.sql.exec("CREATE TRIGGER reject_fixture_session BEFORE INSERT ON account_sessions BEGIN SELECT RAISE(ABORT, 'fixture storage failure'); END"));
  const completion = await post("/api/auth/enrollment/complete", { new_password: password() }, { session: cookie(redemption) });
  expect(completion.status).toBe(503);
  expect(completion.headers.has("Set-Cookie")).toBe(false);
  const account = await inOwner((_instance, state) => state.storage.sql.exec("SELECT status,credential_epoch,verifier IS NULL AS no_password FROM accounts").one());
  expect(account).toMatchObject({ status: "pending_enrollment", credential_epoch: 0, no_password: 1 });
  const session = await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n, max(revoked_at) AS revoked FROM account_sessions").one());
  expect(session).toMatchObject({ n: 1, revoked: null });
});

it("owner transaction commits failure metadata despite a rejected credential command", async () => {
  const missing = "MissingAccount1";
  const response = await post("/api/auth/login", { username: missing, password: password() });
  expect(response.status).toBe(401);
  expect(response.headers.has("Set-Cookie")).toBe(false);
  const buckets = await inOwner((_instance, state) => Array.from(state.storage.sql.exec("SELECT attempt_count FROM rate_limit_buckets")));
  expect(buckets.length).toBe(2);
  expect(buckets.every(row => row.attempt_count === 1)).toBe(true);
});
