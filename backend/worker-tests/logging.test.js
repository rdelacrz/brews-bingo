import { beforeEach, afterEach, expect, test, vi } from "vitest";
import { env, exports } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { commandId, enroll, inOwner, origin, resetStorage } from "./fixtures.js";

let records;
let spies;
let consoleCalls;
const directoryStub = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async () => {
  await resetStorage();
  await runInDurableObject(directoryStub(), async (_instance, state) => state.storage.deleteAll());
  records = [];
  consoleCalls = [];
  spies = ["log", "debug", "info", "warn", "error"].map(level => vi.spyOn(console, level).mockImplementation((...args) => {
    consoleCalls.push({ level, args });
    for (const arg of args) {
      if (typeof arg !== "string") continue;
      try {
        const record = JSON.parse(arg);
        if (record.target === "brews.security") records.push(record);
      } catch { /* Ignore non-application runtime diagnostics. */ }
    }
  }));
});
afterEach(async () => {
  await runInDurableObject(directoryStub(), instance => {
    if (instance.originalLoggingFetch) instance.fetch = instance.originalLoggingFetch;
    delete instance.originalLoggingFetch;
  });
  spies.forEach(spy => spy.mockRestore());
});

const event = (boundary, reason) => records.some(record =>
  record.fields.event === "boundary_failure" && record.fields.boundary === boundary && record.fields.reason === reason);
const secret = () => btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(24))));

function assertSafe(...canaries) {
  const text = JSON.stringify(consoleCalls, (_key, value) => value instanceof Error
    ? { name: value.name, message: value.message, stack: value.stack } : value);
  for (const { level, args } of consoleCalls) {
    expect(args.length === 1 && typeof args[0] === "string").toBe(true);
    const record = JSON.parse(args[0]);
    expect(record.target === "brews.security" && record.level.toLowerCase() === level).toBe(true);
  }
  for (const canary of [...canaries, env.DEV_CLI_KEY, env.RATE_LIMIT_KEY]) {
    expect(text.includes(canary)).toBe(false);
  }
  for (const record of records) {
    expect(Object.keys(record).sort()).toEqual(["fields", "level", "target"]);
    expect(JSON.stringify(record).length <= 1024).toBe(true);
    const allowed = {
      boundary_failure: ["boundary", "event", "reason"],
      management_delivery: ["command_id", "delivery", "event", "operation"],
      removal_state: ["action", "event", "operation_id"],
    };
    expect(Object.keys(record.fields).sort()).toEqual(allowed[record.fields.event]);
  }
}

test("auth credential and ingress failures log no sensitive input or request metadata", async () => {
  const suppliedSecret = secret();
  const response = await exports.default.fetch(`${origin}/api/auth/login`, {
    method: "POST",
    headers: { Origin: origin, "Content-Type": "application/json", "Idempotency-Key": commandId(), "CF-Connecting-IP": "192.0.2.99" },
    body: JSON.stringify({ username: "MissingAccountUser", password: suppliedSecret }),
  });
  expect(response.status).toBe(401);
  expect(event("accounts_auth", "invalid_credentials")).toBe(true);
  expect(records.filter(record => record.fields.event === "boundary_failure").length).toBe(1);
  const rejected = await exports.default.fetch(`${origin}/api/auth/login`, {
    method: "POST", headers: { Origin: "https://rejected.invalid", "Content-Type": "application/json" },
    body: JSON.stringify({ username: suppliedSecret, password: suppliedSecret }),
  });
  expect(rejected.status).toBe(403);
  expect(event("auth_ingress", "forbidden")).toBe(true);
  assertSafe(suppliedSecret, "MissingAccountUser", "192.0.2.99", "https://rejected.invalid");
});

test("unknown enrollment and reset completion credentials emit safe rejection diagnostics", async () => {
  const token = btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32))))
    .replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
  const suppliedPassword = secret();
  for (const path of ["enrollment", "password-reset"]) {
    const response = await exports.default.fetch(`${origin}/api/auth/${path}/complete`, {
      method: "POST",
      headers: { Origin: origin, "Content-Type": "application/json", "Idempotency-Key": commandId(), Cookie: `__Host-brews_session=${token}` },
      body: JSON.stringify({ new_password: suppliedPassword }),
    });
    expect(response.status).toBe(401);
    expect(response.headers.has("Set-Cookie")).toBe(false);
  }
  const rejections = records.filter(record => record.fields.boundary === "accounts_auth" && record.fields.reason === "forbidden");
  expect(rejections.length).toBe(2);
  expect(rejections.every(record => record.level === "WARN")).toBe(true);
  assertSafe(token, suppliedPassword);
});

test("wrong-scope completion credentials are diagnosed while session reads remain quiet", async () => {
  const fixture = await enroll();
  records.length = 0;
  consoleCalls.length = 0;
  const suppliedPassword = secret();
  for (const path of ["enrollment", "password-reset"]) {
    const response = await exports.default.fetch(`${origin}/api/auth/${path}/complete`, {
      method: "POST",
      headers: { Origin: origin, "Content-Type": "application/json", "Idempotency-Key": commandId(), Cookie: fixture.session },
      body: JSON.stringify({ new_password: suppliedPassword }),
    });
    expect(response.status).toBe(401);
    expect(response.headers.has("Set-Cookie")).toBe(false);
  }
  expect(records.filter(record => record.fields.boundary === "accounts_auth" && record.fields.reason === "forbidden").length).toBe(2);
  const before = consoleCalls.length;
  expect((await exports.default.fetch(`${origin}/api/session`, { headers: { Cookie: fixture.session } })).status).toBe(200);
  expect(consoleCalls.length).toBe(before);
  assertSafe(fixture.session, fixture.password, fixture.token, fixture.accountId, fixture.username, suppliedPassword);
});

test("ordinary session reads and unknown routes do not produce application logs", async () => {
  for (let i = 0; i < 3; i++) expect((await exports.default.fetch(`${origin}/api/session`)).status).toBe(200);
  expect((await exports.default.fetch(`${origin}/unknown`)).status).toBe(404);
  expect(records.length).toBe(0);
  expect(consoleCalls.length).toBe(0);
});

async function command(body, id = commandId()) {
  return exports.default.fetch(`${origin}/_dev/commands`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: ["Bearer", env.DEV_CLI_KEY].join(" "), "Idempotency-Key": id },
    body: JSON.stringify(body),
  });
}

test("management issuance and replay log only command metadata, never bearer links", async () => {
  const username = "PrivateLoggingHost";
  const id = commandId();
  const body = { operation: "create_account", username, role: "host" };
  const issued = await command(body, id);
  expect(issued.status).toBe(200);
  const result = await issued.json();
  expect(records.some(record => record.fields.event === "management_delivery" && record.fields.command_id === id && record.fields.delivery === "issued")).toBe(true);
  const replay = await command(body, id);
  expect(replay.status).toBe(200);
  expect((await replay.json()).result).toBe("committed");
  expect(records.filter(record => record.fields.event === "management_delivery").map(record => record.fields.delivery)).toEqual(["issued", "committed"]);
  assertSafe(username, result.url, new URL(result.url).hash.slice(1), result.receipt.account_id, result.receipt.link_id);
});

test("an audit-storage failure logs a safe error and never claims successful delivery", async () => {
  const canary = secret();
  // The genuine SQLite error can contain a private provider detail; logging must not.
  await inOwner((_instance, state) => state.storage.sql.exec(`CREATE TRIGGER reject_logging_audit BEFORE INSERT ON admin_audit BEGIN SELECT RAISE(ABORT,'${canary}'); END`));
  const response = await command({ operation: "create_account", username: "LoggingFailureHost", role: "host" });
  expect(response.status).toBe(503);
  expect(event("accounts_management", "storage")).toBe(true);
  expect(records.filter(record => record.fields.event === "management_delivery").length).toBe(0);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM accounts").one().n)).toBe(0);
  assertSafe(canary, "LoggingFailureHost");
});

test("removal audit failure emits a storage error before its pending retry response", async () => {
  const username = "LoggingRetryAuditHost";
  const created = await command({ operation: "create_account", username, role: "host" });
  const accountId = (await created.json()).receipt.account_id;
  const canary = secret();
  const id = commandId();
  records.length = 0;
  consoleCalls.length = 0;
  await inOwner((_instance, state) => state.storage.sql.exec(
    `CREATE TRIGGER reject_removal_log_audit BEFORE INSERT ON admin_audit WHEN NEW.operation='disable_account' AND NEW.outcome='succeeded' BEGIN SELECT RAISE(ABORT,'${canary}'); END`,
  ));
  const response = await command({ operation: "disable_account", account_id: accountId }, id);
  expect(response.status).toBe(202);
  const result = await response.json();
  expect(result.result).toBe("pending");
  const snapshot = await inOwner((_instance, state) => ({
    account: state.storage.sql.exec("SELECT disabled_at,credential_epoch FROM accounts WHERE account_id=?", accountId).one(),
    work: state.storage.sql.exec("SELECT phase,attempt_count FROM pending_account_removals WHERE operation_id=?", result.operation_id).one(),
    successfulAudits: state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit WHERE operation='disable_account' AND outcome='succeeded'").one().n,
  }));
  expect(snapshot.account).toEqual({ disabled_at: null, credential_epoch: 0 });
  expect(snapshot.work.phase).toBe("prepared");
  expect(snapshot.work.attempt_count).toBeGreaterThan(0);
  expect(snapshot.successfulAudits).toBe(0);
  expect(await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM directory_removal_pending WHERE account_id=?", accountId).one().n)).toBe(1);
  expect(records.some(record => record.level === "ERROR" && record.fields.boundary === "accounts_management" && record.fields.reason === "storage")).toBe(true);
  expect(records.some(record => record.fields.action === "retry_scheduled")).toBe(true);
  expect(records.some(record => record.fields.event === "management_delivery" && record.fields.command_id === id && record.fields.delivery === "pending")).toBe(true);
  expect(records.some(record => record.fields.action === "completed")).toBe(false);
  assertSafe(canary, username, accountId);
  await inOwner((_instance, state) => state.storage.sql.exec("DROP TRIGGER reject_removal_log_audit"));
});

test("removal proof mismatch logs a protocol failure and retains work for retry", async () => {
  const created = await command({ operation: "create_account", username: "LoggingProofHost", role: "host" });
  const accountId = (await created.json()).receipt.account_id;
  records.length = 0;
  await runInDurableObject(directoryStub(), instance => {
    instance.originalLoggingFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const response = await instance.originalLoggingFetch(request);
      const data = await response.json();
      // Corrupt the genuine grant to exercise rejection, not successful coordination.
      if (data.result === "acquired") data.account_id = commandId();
      return Response.json(data);
    };
  });
  const response = await command({ operation: "disable_account", account_id: accountId });
  expect(response.status).toBe(202);
  expect(event("directory_peer", "proof_mismatch")).toBe(true);
  expect(records.some(record => record.fields.event === "removal_state" && record.fields.action === "retry_scheduled")).toBe(true);
  expect(records.some(record => record.fields.action === "completed")).toBe(false);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT disabled_at FROM accounts WHERE account_id=?", accountId).one().disabled_at)).toBeNull();
  assertSafe(accountId, "LoggingProofHost");
});

test("private Directory malformed envelopes are diagnosed without logging raw content", async () => {
  const canary = secret();
  await directoryStub().fetch("https://directory.internal/removal", { method: "POST", body: JSON.stringify([canary]) });
  expect(event("directory_request", "invalid_input")).toBe(true);
  assertSafe(canary);
});

test("account and Directory alarm failures are classified without raw SQLite errors", async () => {
  const canary = secret();
  await command({ operation: "create_account", username: "LoggingAlarmHost", role: "host" });
  records.length = 0;
  const failed = await inOwner(async (instance, state) => {
    const expired = Date.now() - 1;
    state.storage.sql.exec("UPDATE admin_audit SET occurred_at=?,expires_at=?", expired - 90*86400000, expired);
    state.storage.sql.exec(`CREATE TRIGGER reject_log_cleanup BEFORE DELETE ON admin_audit BEGIN SELECT RAISE(ABORT,'${canary}'); END`);
    try { await instance.alarm(); return false; } catch { return true; }
  });
  expect(failed).toBe(true);
  expect(event("accounts_alarm", "storage")).toBe(true);
  const directoryFailed = await runInDurableObject(directoryStub(), async (instance, state) => {
    state.storage.sql.exec("CREATE TABLE unexpected_logging_table(value TEXT)");
    try { await instance.alarm(); return false; } catch { return true; }
  });
  expect(directoryFailed).toBe(true);
  expect(event("directory_alarm", "storage")).toBe(true);
  assertSafe(canary, "LoggingAlarmHost");
});

test("storage output-gate failure logs durability and withholds delivery diagnostics", async () => {
  const canary = secret();
  const result = await inOwner(async (instance, state) => {
    const original = state.storage.sync;
    state.storage.sync = async () => { throw new Error(canary); };
    try {
      const response = await instance.fetch(new Request("https://accounts.internal/management", {
        method: "POST",
        body: JSON.stringify({ command: { operation: "create_account", username: "LoggingGateHost", role: "host" }, command_id: commandId() }),
      }));
      return { status: response.status, cookie: response.headers.has("Set-Cookie") };
    } finally { state.storage.sync = original; }
  });
  expect(result.status).toBe(503);
  expect(result.cookie).toBe(false);
  expect(event("durability", "storage")).toBe(true);
  expect(records.some(record => record.fields.event === "management_delivery")).toBe(false);
  assertSafe(canary, "LoggingGateHost");
});

test("removal completion is not logged before the final owner output gate", async () => {
  const created = await command({ operation: "create_account", username: "LoggingFinalGateHost", role: "host" });
  const accountId = (await created.json()).receipt.account_id;
  records.length = 0;
  const result = await inOwner(async (instance, state) => {
    const original = state.storage.sync;
    let calls = 0;
    state.storage.sync = async function () {
      if (++calls === 3) throw new Error("test-only final logging output gate failure");
      return original.call(this);
    };
    try {
      const response = await instance.fetch(new Request("https://accounts.internal/management", {
        method: "POST",
        body: JSON.stringify({ command: { operation: "disable_account", account_id: accountId }, command_id: commandId() }),
      }));
      return { status: response.status, calls };
    } finally { state.storage.sync = original; }
  });
  expect(result.calls).toBe(3);
  expect(result.status).toBe(503);
  expect(event("durability", "storage")).toBe(true);
  expect(records.some(record => record.fields.action === "completed")).toBe(false);
  expect(records.some(record => record.fields.event === "management_delivery")).toBe(false);
  assertSafe(accountId, "LoggingFinalGateHost");
});

test("rejected CLI credentials emit one structured safe boundary event", async () => {
  const suppliedSecret = secret();
  const response = await exports.default.fetch(`${origin}/_dev/commands`, {
    method: "POST",
    headers: { Authorization: ["Bearer", suppliedSecret].join(" "), "Content-Type": "application/json" },
    body: JSON.stringify({ operation: "create_account", username: suppliedSecret, role: "host" }),
  });
  expect(response.status).toBe(403);
  expect(event("cli_ingress", "forbidden")).toBe(true);
  expect(records.length).toBe(1);
  expect(JSON.stringify(records).includes(suppliedSecret)).toBe(false);
  expect(JSON.stringify(records).includes(env.DEV_CLI_KEY)).toBe(false);
  expect(JSON.stringify(records).includes(env.RATE_LIMIT_KEY)).toBe(false);
  assertSafe(suppliedSecret);
});
