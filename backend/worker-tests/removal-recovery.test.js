import { afterEach, beforeEach, expect, test } from "vitest";
import { runInDurableObject } from "cloudflare:test";
import { env, exports } from "cloudflare:workers";
import { commandId, inOwner, origin, resetStorage, stub } from "./fixtures.js";

const directoryStub = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async () => {
  await resetStorage();
  await runInDurableObject(directoryStub(), async (_instance, state) => state.storage.deleteAll());
});
afterEach(async () => {
  await runInDurableObject(directoryStub(), instance => {
    if (instance.originalRecoveryFetch) instance.fetch = instance.originalRecoveryFetch;
    delete instance.originalRecoveryFetch;
    delete instance.recoveryAcquireCount;
  });
});
async function command(body, id = commandId()) {
  return exports.default.fetch(`${origin}/_dev/commands`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: ["Bearer", env.DEV_CLI_KEY].join(" "), "Idempotency-Key": id },
    body: JSON.stringify(body),
  });
}
async function seedHost(username = "RecoveryHost") {
  const accountId = commandId();
  await inOwner((_instance, state) => state.storage.sql.exec(
    "INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?,'host','pending_enrollment',NULL,0,?,NULL,NULL)",
    accountId, username, Date.now(),
  ));
  return accountId;
}
async function delayDirectory(mode) {
  await runInDurableObject(directoryStub(), instance => {
    instance.originalRecoveryFetch = instance.fetch.bind(instance);
    instance.recoveryAcquireCount = 0;
    instance.fetch = async request => {
      if (mode === "before") {
        await new Promise(resolve => setTimeout(resolve, 400));
        return instance.originalRecoveryFetch(request);
      }
      const body = await request.clone().json();
      const ordinal = body.action === "acquire" ? ++instance.recoveryAcquireCount : 0;
      const response = await instance.originalRecoveryFetch(request);
      if (ordinal) await new Promise(resolve => setTimeout(resolve, ordinal === 1 ? 100 : 400));
      return response;
    };
  });
}

test("prepared removal has a durable recovery alarm before its first Directory await", async () => {
  const accountId = await seedHost();
  expect(await inOwner((_instance, state) => state.storage.getAlarm())).toBeNull();
  await delayDirectory("before");
  const pending = command({ operation: "disable_account", account_id: accountId });
  let observed;
  for (let n = 0; n < 30; n++) {
    observed = await inOwner(async (_instance, state) => {
      const rows = state.storage.sql.exec("SELECT phase FROM pending_account_removals WHERE target_account_id=?", accountId).toArray();
      return { phase: rows[0]?.phase ?? null, alarm: await state.storage.getAlarm() };
    });
    if (observed.phase === "prepared") break;
    await new Promise(resolve => setTimeout(resolve, 10));
  }
  const response = await pending;
  expect(response.status).toBe(200);
  expect(observed.phase).toBe("prepared");
  expect(observed.alarm).not.toBeNull();
});

function idAt(ms) {
  const timestamp = BigInt(ms).toString(16).padStart(12, "0");
  return `${timestamp.slice(0, 8)}-${timestamp.slice(8)}${commandId().slice(13)}`;
}
async function seedPrepared(accountId, createdAt = Date.now()) {
  const operationId = idAt(createdAt), id = idAt(createdAt);
  const body = { operation: "disable_account", account_id: accountId };
  const fingerprint = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(["brews-management-v1", body])));
  await inOwner((_instance, state) => state.storage.sql.exec(
    "INSERT INTO pending_account_removals(operation_id,actor,command_id,target_account_id,operation,request_fingerprint,phase,created_at,next_attempt_at,attempt_count) VALUES(?,'developer_cli',?,?,'disable_account',?,'prepared',?,?,0)",
    operationId, id, accountId, fingerprint, createdAt, Date.now() - 1,
  ));
  return { operationId, id, body };
}
async function accountState(accountId) {
  return inOwner((_instance, state) => ({
    account: state.storage.sql.exec("SELECT disabled_at,credential_epoch FROM accounts WHERE account_id=?", accountId).one(),
    pending: state.storage.sql.exec("SELECT phase,attempt_count,next_attempt_at FROM pending_account_removals WHERE target_account_id=?", accountId).toArray(),
    succeeded: state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit WHERE target_account_id=? AND operation='disable_account' AND outcome='succeeded'", accountId).one().n,
  }));
}

test("stale ungranted prepared intent is permanently rejected and unlocked after reconciliation", async () => {
  const accountId = await seedHost();
  const { body, id } = await seedPrepared(accountId, Date.now() - 86400001);
  const response = await command(body, id);
  expect(response.status).toBe(409);
  expect((await response.json()).error.code).toBe("stale_command");
  const state = await accountState(accountId);
  expect(state.pending).toEqual([]);
  expect(state.account.disabled_at).toBeNull();
  expect(state.account.credential_epoch).toBe(0);
  expect(state.succeeded).toBe(0);
  expect((await command({ operation: "reissue_enrollment", account_id: accountId })).status).toBe(200);
});

test("late genuine duplicate acquire replies return the matching committed receipt", async () => {
  const accountId = await seedHost();
  await delayDirectory("after");
  const id = commandId(), body = { operation: "disable_account", account_id: accountId };
  const responses = await Promise.all([command(body, id), command(body, id)]);
  expect(responses.map(response => response.status)).toEqual([200, 200]);
  const results = await Promise.all(responses.map(response => response.json()));
  expect(results[0].result).toBe("committed");
  expect(results[1]).toEqual(results[0]);
  const replay = await command(body, id);
  expect(replay.status).toBe(200);
  expect(await replay.json()).toEqual(results[0]);
  const state = await accountState(accountId);
  expect(state.pending).toEqual([]);
  expect(state.account.credential_epoch).toBe(1);
  expect(state.succeeded).toBe(1);
});

test("private management envelopes reject positional arrays without reading or mutating accounts", async () => {
  const response = await stub().fetch("https://accounts.internal/management", {
    method: "POST", body: JSON.stringify([{ operation: "list_accounts", limit: 50, after: null }, null]),
  });
  expect(response.status).toBe(400);
  expect((await response.json()).error.code).toBe("invalid_input");
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit").one().n)).toBe(0);
});

async function peer(action, operationId = commandId(), accountId = commandId()) {
  const response = await directoryStub().fetch("https://directory.internal/removal", {
    method: "POST", body: JSON.stringify({ action, operation_id: operationId, account_id: accountId }),
  });
  return response.json();
}
async function recoverDue() {
  await inOwner(async (instance, state) => {
    state.storage.sql.exec("UPDATE pending_account_removals SET next_attempt_at=?", Date.now() - 1);
    await instance.alarm();
  });
}

test("legacy prepared intent without an alarm recovers through genuine Directory calls", async () => {
  const accountId = await seedHost();
  const { body, id } = await seedPrepared(accountId);
  await inOwner((_instance, state) => state.storage.deleteAlarm());
  expect(await inOwner((_instance, state) => state.storage.getAlarm())).toBeNull();
  await recoverDue();
  expect((await command(body, id)).status).toBe(200);
  const state = await accountState(accountId);
  expect(state.pending).toEqual([]);
  expect(state.account.credential_epoch).toBe(1);
  expect(state.succeeded).toBe(1);
});

test("alarm recovery unlocks stale ungranted work instead of retrying forever", async () => {
  const accountId = await seedHost();
  await seedPrepared(accountId, Date.now() - 86400001);
  await recoverDue();
  const state = await accountState(accountId);
  expect(state.pending).toEqual([]);
  expect(state.account.disabled_at).toBeNull();
  expect(state.account.credential_epoch).toBe(0);
  expect(state.succeeded).toBe(0);
  expect((await command({ operation: "reissue_enrollment", account_id: accountId })).status).toBe(200);
});

test("old known Directory pending grant remains eligible for legitimate completion", async () => {
  const accountId = await seedHost();
  const createdAt = Date.now() - 86400001;
  const { operationId, id, body } = await seedPrepared(accountId, createdAt);
  expect((await peer("reconcile")).result).toBe("released");
  expect(await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name<>'__miniflare_do_name' ORDER BY name").toArray().map(row => row.name))).toEqual(["_cf_METADATA", "account_assignment_gates", "directory_hosted_nonterminal_games", "directory_metadata", "directory_removal_pending", "directory_removal_receipts", "directory_removal_rejections"]);
  await runInDurableObject(directoryStub(), (_instance, state) => {
    state.storage.sql.exec("INSERT INTO account_assignment_gates(account_id) VALUES(?)", accountId);
    state.storage.sql.exec("INSERT INTO directory_removal_pending(operation_id,account_id,created_at) VALUES(?,?,?)", operationId, accountId, createdAt);
  });
  await recoverDue();
  const response = await command(body, id);
  expect(response.status).toBe(200);
  expect((await response.json()).result).toBe("committed");
  expect((await accountState(accountId)).pending).toEqual([]);
});

test("genuine completed acquire rejection cannot authorize a prepared mutation", async () => {
  const accountId = await seedHost();
  const { operationId, id, body } = await seedPrepared(accountId);
  expect((await peer("reconcile", operationId, accountId)).result).toBe("released");
  const response = await command(body, id);
  expect(response.status).toBe(409);
  expect((await response.json()).error.code).toBe("conflict");
  const state = await accountState(accountId);
  expect(state.pending).toEqual([]);
  expect(state.account.disabled_at).toBeNull();
  expect(state.succeeded).toBe(0);
});

test("late genuine grant after another handler aborts is permanently rejected without mutation", async () => {
  const accountId = await seedHost();
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET credential_epoch=? WHERE account_id=?", Number.MAX_SAFE_INTEGER, accountId));
  await delayDirectory("after");
  const id = commandId(), body = { operation: "disable_account", account_id: accountId };
  const responses = await Promise.all([command(body, id), command(body, id)]);
  expect(responses.map(response => response.status)).toEqual([409, 409]);
  for (const response of responses) expect((await response.json()).error.code).toBe("conflict");
  const state = await accountState(accountId);
  expect(state.pending).toEqual([]);
  expect(state.account.disabled_at).toBeNull();
  expect(state.succeeded).toBe(0);
});

test("undelayed parallel matching removals never return negative results or duplicate successful audit", async () => {
  for (let n = 0; n < 12; n++) {
    const accountId = await seedHost(`ParallelRecovery${n}`);
    const id = commandId(), body = { operation: "disable_account", account_id: accountId };
    const responses = await Promise.all(Array.from({ length: 6 }, () => command(body, id)));
    const results = await Promise.all(responses.map(async response => {
      expect([200, 202]).toContain(response.status);
      const result = await response.json();
      expect(response.status === 200 ? result.result === "committed" : result.result === "pending").toBe(true);
      return result;
    }));
    const replay = await command(body, id);
    expect(replay.status).toBe(200);
    const result = await replay.json();
    for (const observed of results.filter(value => value.result === "committed")) expect(observed).toEqual(result);
    const state = await accountState(accountId);
    expect(state.pending).toEqual([]);
    expect(state.account.credential_epoch).toBe(1);
    expect(state.succeeded).toBe(1);
  }
});

test.each(["stale", "committed"])("%s recovery withholds completion until genuine Directory reconciliation succeeds", async mode => {
  const accountId = await seedHost();
  const { body, id } = await seedPrepared(accountId, mode === "stale" ? Date.now() - 86400001 : Date.now());
  expect((await peer("reconcile")).result).toBe("released");
  if (mode === "committed") {
    await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("CREATE TRIGGER reject_recovery_ack BEFORE INSERT ON directory_removal_receipts BEGIN SELECT RAISE(ABORT,'blocked'); END"));
  } else {
    await runInDurableObject(directoryStub(), instance => {
      instance.originalRecoveryFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        const body = await request.clone().json();
        const response = await instance.originalRecoveryFetch(request);
        if (body.action === "reconcile") throw new Error("test-only lost Directory acknowledgement");
        return response;
      };
    });
  }
  const response = await command(body, id);
  expect(response.status).toBe(202);
  expect((await response.json()).result).toBe("pending");
  let state = await accountState(accountId);
  expect(state.pending[0].phase).toBe(mode === "stale" ? "aborting" : "committed");
  expect(state.pending[0].attempt_count).toBeGreaterThan(0);
  expect(state.pending[0].next_attempt_at).toBeGreaterThan(Date.now() - 500);
  expect(await inOwner((_instance, state) => state.storage.getAlarm())).not.toBeNull();
  expect(state.account.disabled_at === null).toBe(mode === "stale");
  if (mode === "committed") {
    await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("DROP TRIGGER reject_recovery_ack"));
  } else {
    await runInDurableObject(directoryStub(), instance => {
      instance.fetch = instance.originalRecoveryFetch;
      delete instance.originalRecoveryFetch;
    });
  }
  await recoverDue();
  state = await accountState(accountId);
  expect(state.pending).toEqual([]);
  expect(state.succeeded).toBe(mode === "stale" ? 0 : 1);
});

test("committed admin-session recovery backs off while socket acknowledgements remain outstanding", async () => {
  const issuer = await seedHost("RecoveryAdminIssuer");
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", issuer));
  const accountId = await seedHost("RecoverySocketHost");
  await inOwner((_instance, state) => state.storage.sql.exec(
    "INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) VALUES(?,?,?,?,0,?)",
    accountId, commandId(), commandId(), commandId(), Date.now() + 86400000,
  ));
  const id = commandId();
  const response = await command({ operation: "disable_account", account_id: accountId }, id);
  expect(response.status).toBe(202);
  // Model the already-committed shared admin surface's persisted attribution only.
  // Recovery must not need, persist, or invent the issuer's expired bearer session.
  await inOwner((_instance, state) => {
    state.storage.sql.exec("UPDATE pending_account_removals SET actor=?,next_attempt_at=? WHERE command_id=?", issuer, Date.now()-1, id);
    state.storage.sql.exec("UPDATE management_receipts SET actor=? WHERE command_id=?", issuer, id);
  });
  const before = (await accountState(accountId)).pending[0].attempt_count;
  await recoverDue();
  const state = await accountState(accountId);
  expect(state.pending[0].phase).toBe("committed");
  expect(state.pending[0].attempt_count).toBeGreaterThan(before);
  expect(state.pending[0].next_attempt_at).toBeGreaterThan(Date.now()-500);
  expect(state.succeeded).toBe(1);
});

test("private inputs preserve duplicate-field rejection and require object envelopes", async () => {
  const accountId = await seedHost(), operationId = commandId();
  for (const body of ["[]", "null", "true", JSON.stringify([{ operation: "disable_account", account_id: accountId }, commandId()]), '{"command":{"operation":"list_accounts","limit":50},"command_id":null,"command_id":null}', '{"command":{"operation":"list_accounts","limit":50},"principal":"developer_cli"}']) {
    expect((await stub().fetch("https://accounts.internal/management", { method: "POST", body })).status).toBe(400);
  }
  for (const body of ["[]", "null", JSON.stringify(["acquire", operationId, accountId]), `{"action":"acquire","operation_id":"${operationId}","account_id":"${accountId}","account_id":"${accountId}"}`, JSON.stringify({ action: "acquire", operation_id: operationId, account_id: accountId, principal: "developer_cli" })]) {
    const response = await directoryStub().fetch("https://directory.internal/removal", { method: "POST", body });
    expect((await response.json()).result).toBe("rejected");
  }
  expect((await accountState(accountId)).pending).toEqual([]);
  expect(await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM directory_removal_pending").one().n)).toBe(0);
});

test.each(["array", "duplicate", "unknown"])("malformed %s private Directory reply cannot become a mutation grant", async mode => {
  const accountId = await seedHost();
  await runInDurableObject(directoryStub(), instance => {
    instance.originalRecoveryFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const response = await instance.originalRecoveryFetch(request);
      const result = await response.json();
      const original = JSON.stringify(result);
      const body = mode === "array" ? JSON.stringify([result]) : mode === "duplicate" ? original.replace('"result":', '"result":"acquired","result":') : original.replace(/}$/, ',"untrusted":true}');
      return new Response(body, { headers: { "Content-Type": "application/json" } });
    };
  });
  const response = await command({ operation: "disable_account", account_id: accountId });
  expect(response.status).toBe(202);
  const state = await accountState(accountId);
  expect(state.pending[0].phase).toBe("prepared");
  expect(state.account.disabled_at).toBeNull();
  expect(state.succeeded).toBe(0);
  await runInDurableObject(directoryStub(), instance => {
    instance.fetch = instance.originalRecoveryFetch;
    delete instance.originalRecoveryFetch;
  });
  await recoverDue();
  expect((await accountState(accountId)).pending).toEqual([]);
});
