import { afterEach, beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { commandId, cookie, enroll, inOwner, origin, password, post, resetStorage } from "./fixtures.js";

const directoryStub = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async () => {
  await resetStorage();
  await runInDurableObject(directoryStub(), async (_instance, state) => state.storage.deleteAll());
});

afterEach(async () => {
  await runInDurableObject(directoryStub(), instance => {
    if (instance.originalUsersFetch) instance.fetch = instance.originalUsersFetch;
    delete instance.originalUsersFetch;
  });
  await inOwner(instance => {
    if (instance.originalUsersResponseFetch) instance.fetch = instance.originalUsersResponseFetch;
    delete instance.originalUsersResponseFetch;
  });
});

async function admin() {
  const fixture = await enroll();
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role=? WHERE account_id=?", "admin", fixture.accountId));
  return fixture;
}
async function users(path = "", { method = "GET", session, body, id = method === "GET" ? undefined : commandId(), applicationOrigin = origin, headers = {} } = {}) {
  const requestHeaders = { ...headers };
  if (session) requestHeaders.Cookie = session;
  if (method !== "GET" && applicationOrigin !== undefined) requestHeaders.Origin = applicationOrigin;
  if (id) requestHeaders["Idempotency-Key"] = id;
  if (body !== undefined) requestHeaders["Content-Type"] = "application/json";
  return exports.default.fetch(`${origin}/api/users${path}`, { method, headers: requestHeaders, body: body === undefined ? undefined : JSON.stringify(body) });
}
function safeHeaders(response) {
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  expect(response.headers.get("Referrer-Policy")).toBe("no-referrer");
  expect(response.headers.get("X-Content-Type-Options")).toBe("nosniff");
  expect(response.headers.has("Set-Cookie")).toBe(false);
}

test.each(["account", "users account", "receipt", "error"])("Worker rejects a sequence-shaped nested %s peer response", async shape => {
  const actor = await admin();
  await inOwner(instance => {
    instance.originalUsersResponseFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const response = await instance.originalUsersResponseFetch(request);
      if (new URL(request.url).pathname !== "/users") return response;
      const value = await response.json();
      if (shape === "account") value.account = Object.values(value.account);
      if (shape === "users account") value.users[0] = Object.values(value.users[0]);
      if (shape === "receipt") value.receipt = Object.values(value.receipt);
      if (shape === "error") value.error = Object.values(value.error);
      return new Response(JSON.stringify(value), { status: response.status, headers: response.headers });
    };
  });
  const response = shape === "receipt"
    ? await users("/hosts", { method: "POST", session: actor.session, body: { username: "ApiPeerShapeHost" } })
    : await users(shape === "account" ? `/${actor.accountId}` : "", { session: shape === "error" ? undefined : actor.session });
  expect(response.status).toBe(503);
  safeHeaders(response);
  const body = await response.json();
  expect(Object.keys(body)).toEqual(["error"]);
  expect(body.error.code).toBe("service_unavailable");
});

test("an enrolled admin lists and reads safe Users metadata through the Worker", async () => {
  const actor = await admin();
  const response = await users("?limit=1", { session: actor.session });
  expect(response.status).toBe(200);
  safeHeaders(response);
  const page = await response.json();
  expect(page.result).toBe("users");
  expect(page.users.length).toBe(1);
  expect(page.next_cursor).toBeNull();
  expect(Object.keys(page.users[0]).sort()).toEqual(["account_id", "created_at", "disabled", "role", "status", "username"]);
  const detail = await users(`/${actor.accountId}`, { session: actor.session });
  expect(detail.status).toBe(200);
  expect((await detail.json()).account.account_id).toBe(actor.accountId);
  expect(JSON.stringify(page).includes(actor.password)).toBe(false);
  expect(JSON.stringify(page).includes(actor.session)).toBe(false);
  const audited = await inOwner((_instance, state) => state.storage.sql.exec("SELECT operation,actor FROM admin_audit ORDER BY audit_id").toArray());
  expect(audited.map(row => row.operation)).toEqual(["list_accounts", "get_account"]);
  expect(audited.every(row => row.actor === actor.accountId)).toBe(true);
});


async function createHost(actor, username = "ApiCreatedHost") {
  const response = await users("/hosts", { method: "POST", session: actor.session, body: { username } });
  expect(response.status).toBe(200);
  safeHeaders(response);
  return response.json();
}
async function completeCreated(created) {
  const bearer = new URL(created.enrollment_url).hash.slice(1);
  const redemption = await post("/api/auth/enrollment/redeem", { enrollment_token: bearer });
  expect(redemption.status).toBe(200);
  const secret = password();
  const response = await post("/api/auth/enrollment/complete", { new_password: secret }, { session: cookie(redemption) });
  expect(response.status).toBe(200);
  return { session: cookie(response), password: secret };
}

test("Users endpoints reject unauthenticated, host, restricted, expired and disabled actors", async () => {
  expect((await users()).status).toBe(401);
  const host = await enroll();
  const read = await users("", { session: host.session });
  expect(read.status).toBe(403);
  safeHeaders(read);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit WHERE actor=? AND outcome='rejected'", host.accountId).one().n)).toBe(1);
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role=? WHERE account_id=?", "admin", host.accountId));
  await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_sessions SET expires_at=? WHERE account_id=?", Date.now()-1, host.accountId));
  expect((await users("", { session: host.session })).status).toBe(401);
  const before = await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit").one().n);
  expect((await users("/admins", { method: "POST", body: { username: "NotPublicBootstrap" } })).status).toBe(401);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit").one().n)).toBe(before);
});

test("restricted, revoked, disabled and stale-epoch admin cookies never expose Users data", async () => {
  const actor = await admin();
  const before = await inOwner((_instance, state) => ({
    epoch: state.storage.sql.exec("SELECT credential_epoch FROM accounts WHERE account_id=?", actor.accountId).one().credential_epoch,
    audit: state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit").one().n,
  }));
  for (const mode of ["restricted", "revoked", "disabled", "stale_epoch"]) {
    await inOwner((_instance, state) => {
      state.storage.sql.exec("UPDATE account_sessions SET scope=?,revoked_at=NULL WHERE account_id=?", "normal", actor.accountId);
      state.storage.sql.exec("UPDATE accounts SET disabled_at=NULL,credential_epoch=? WHERE account_id=?", before.epoch, actor.accountId);
      if (mode === "restricted") state.storage.sql.exec("UPDATE account_sessions SET scope=? WHERE account_id=?", "enrollment_only", actor.accountId);
      if (mode === "revoked") state.storage.sql.exec("UPDATE account_sessions SET revoked_at=? WHERE account_id=?", Date.now(), actor.accountId);
      if (mode === "disabled") state.storage.sql.exec("UPDATE accounts SET disabled_at=? WHERE account_id=?", Date.now(), actor.accountId);
      if (mode === "stale_epoch") state.storage.sql.exec("UPDATE accounts SET credential_epoch=? WHERE account_id=?", before.epoch + 1, actor.accountId);
    });
    const response = await users("", { session: actor.session });
    expect(response.status).toBe(401);
    expect(Object.hasOwn(await response.json(), "users")).toBe(false);
  }
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit").one().n)).toBe(before.audit);
});

test("fixed-role creation follows planned fields and same-command replay never returns a URL", async () => {
  const actor = await admin();
  for (const [path, role, username] of [["/hosts", "Host", "ApiHostPerson"], ["/admins", "Admin", "ApiAdminPerson"]]) {
    const id = commandId();
    const response = await users(path, { method: "POST", session: actor.session, body: { username }, id });
    expect(response.status).toBe(200);
    safeHeaders(response);
    const created = await response.json();
    expect(created.result).toBe("created");
    expect(created.account.role).toBe(role);
    expect(created.account.status).toBe("PendingEnrollment");
    expect(created.receipt.command_id).toBe(id);
    expect(new URL(created.enrollment_url).origin).toBe(origin);
    expect(new URL(created.enrollment_url).pathname).toBe("/enroll");
    expect(new URL(created.enrollment_url).search).toBe("");
    const replay = await users(path, { method: "POST", session: actor.session, body: { username }, id });
    const replayBody = await replay.json();
    expect(replay.status).toBe(200);
    expect(replayBody.result).toBe("committed");
    expect(Object.keys(replayBody).sort()).toEqual(["receipt", "result"]);
    expect(JSON.stringify(replayBody).includes(new URL(created.enrollment_url).hash.slice(1))).toBe(false);
    expect((await users(path, { method: "POST", session: actor.session, body: { username: username + "Changed" }, id })).status).toBe(409);
  }
});

test("list pagination follows stable account IDs and rejects unsupported queries", async () => {
  const actor = await admin();
  await createHost(actor, "ApiPagedHostOne");
  await createHost(actor, "ApiPagedHostTwo");
  const seen = [];
  let cursor;
  do {
    const response = await users(`?limit=1${cursor ? `&cursor=${cursor}` : ""}`, { session: actor.session });
    expect(response.status).toBe(200);
    const page = await response.json();
    seen.push(...page.users.map(account => account.account_id));
    cursor = page.next_cursor;
  } while (cursor);
  expect(new Set(seen).size).toBe(3);
  expect(seen).toEqual([...seen].sort());
  for (const query of ["?role=host", "?status=Verified", "?limit=0", "?limit=101", "?limit=1&limit=1", "?cursor=bad", "?limit=%31", "?"]) {
    expect((await users(query, { session: actor.session })).status).toBe(400);
  }
});

test("creation rejects client role or authority and actions require empty bodies and exact Origin", async () => {
  const actor = await admin();
  const id = actor.accountId;
  for (const body of [{ username: "ApiEscalationHost", role: "admin" }, { username: "ApiEscalationHost", actor: "developer_cli" }, { username: "ApiEscalationHost", password: "not-an-input" }]) {
    expect((await users("/hosts", { method: "POST", session: actor.session, body })).status).toBe(400);
  }
  for (const method of ["PUT", "PATCH"]) expect((await users(`/${id}`, { method, session: actor.session, body: {} })).status).toBe(405);
  for (const originValue of [null, "https://wrong.invalid", "null", origin+"/"]) {
    const headers = originValue === null ? {} : { Origin: originValue };
    const response = await exports.default.fetch(`${origin}/api/users/hosts`, {
      method: "POST", headers: { ...headers, Cookie: actor.session, "Content-Type": "application/json", "Idempotency-Key": commandId() }, body: JSON.stringify({ username: "ApiOriginRejectHost" }),
    });
    expect(response.status).toBe(403);
  }
  expect((await users(`/${id}/enable`, { method: "POST", session: actor.session, body: {} })).status).toBe(400);
  expect((await users(`/${id}`, { method: "DELETE", session: actor.session, body: {} })).status).toBe(400);
  expect((await users("", { session: actor.session, headers: { Authorization: ["Bearer", env.DEV_CLI_KEY].join(" ") } })).status).toBe(403);
  expect((await users("/audit", { session: actor.session })).status).toBe(404);
});

test("enrollment reissue and reset reuse core invalidation and redact replay handoffs", async () => {
  const actor = await admin();
  const target = await createHost(actor);
  const id = target.account.account_id;
  const replaced = await users(`/${id}/enrollment-links`, { method: "POST", session: actor.session });
  expect(replaced.status).toBe(200);
  const next = await replaced.json();
  expect(next.result).toBe("enrollment_link");
  const oldRedemption = await post("/api/auth/enrollment/redeem", { enrollment_token: new URL(target.enrollment_url).hash.slice(1) });
  expect(oldRedemption.status).toBe(400);
  const enrolled = await completeCreated({ enrollment_url: next.enrollment_url });
  const key = commandId();
  const reset = await users(`/${id}/password-reset-links`, { method: "POST", session: actor.session, id: key });
  expect(reset.status).toBe(200);
  const result = await reset.json();
  expect(result.result).toBe("password_reset");
  expect(result.status).toBe("ResetRequired");
  expect(new URL(result.reset_url).pathname).toBe("/password-reset");
  const session = await exports.default.fetch(`${origin}/api/session`, { headers: { Cookie: enrolled.session } });
  expect((await session.json()).authenticated).toBe(false);
  const replay = await users(`/${id}/password-reset-links`, { method: "POST", session: actor.session, id: key });
  expect((await replay.json()).result).toBe("committed");
  expect((await users(`/${id}/enrollment-links`, { method: "POST", session: actor.session })).status).toBe(409);
});

test("disable, enable and delete use Directory coordination and receipt-only replay", async () => {
  const actor = await admin();
  const created = await createHost(actor);
  const target = created.account.account_id;
  const key = commandId();
  const disabled = await users(`/${target}/disable`, { method: "POST", session: actor.session, id: key });
  expect(disabled.status).toBe(200);
  const result = await disabled.json();
  expect(result.result).toBe("disabled");
  expect(result.disabled).toBe(true);
  expect(result.status).toBe("PendingEnrollment");
  expect((await users(`/${target}/disable`, { method: "POST", session: actor.session, id: key }).then(response => response.json())).result).toBe("committed");
  const enabled = await users(`/${target}/enable`, { method: "POST", session: actor.session });
  expect(enabled.status).toBe(200);
  expect((await enabled.json()).disabled).toBe(false);
  const deleteKey = commandId();
  const removed = await users(`/${target}`, { method: "DELETE", session: actor.session, id: deleteKey });
  expect(removed.status).toBe(200);
  expect((await removed.json()).deleted).toBe(true);
  expect((await users(`/${target}`, { session: actor.session })).status).toBe(404);
  expect((await users(`/${target}`, { method: "DELETE", session: actor.session, id: deleteKey }).then(response => response.json())).result).toBe("committed");
  expect(await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_assignment_gates").one().n)).toBe(0);
});

test("self-removal and hosted assignments remain guarded and audit failures roll back", async () => {
  const actor = await admin();
  expect((await users(`/${actor.accountId}/disable`, { method: "POST", session: actor.session })).status).toBe(403);
  expect((await users(`/${actor.accountId}`, { method: "DELETE", session: actor.session })).status).toBe(403);
  expect((await users(`/${actor.accountId}/enable`, { method: "POST", session: actor.session })).status).toBe(403);
  const target = await createHost(actor);
  const id = target.account.account_id;
  await directoryStub().fetch("https://directory.internal/removal", { method: "POST", body: JSON.stringify({ action: "reconcile", operation_id: commandId(), account_id: id }) });
  await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("INSERT INTO directory_hosted_nonterminal_games VALUES(?,?)", commandId(), id));
  expect((await users(`/${id}/disable`, { method: "POST", session: actor.session })).status).toBe(409);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM pending_account_removals").one().n)).toBe(0);
  await inOwner((_instance, state) => state.storage.sql.exec("CREATE TRIGGER reject_users_audit BEFORE INSERT ON admin_audit BEGIN SELECT RAISE(ABORT,'test-only audit rejection'); END"));
  expect((await users("", { session: actor.session })).status).toBe(503);
  expect((await users("/hosts", { method: "POST", session: actor.session, body: { username: "ApiUnauditedHost" } })).status).toBe(503);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM accounts WHERE username=?", "ApiUnauditedHost").one().n)).toBe(0);
});


test("role loss during Directory acquire aborts prepared work and releases its exact gate", async () => {
  const actor = await admin();
  const target = (await createHost(actor)).account.account_id;
  await runInDurableObject(directoryStub(), instance => {
    instance.originalUsersFetch = instance.fetch.bind(instance);
    let changed = false;
    instance.fetch = async request => {
      const message = await request.clone().json();
      const response = await instance.originalUsersFetch(request);
      if (message.action === "acquire" && !changed) {
        changed = true;
        await inOwner((_instance, state) => state.storage.sql.exec("UPDATE accounts SET role=? WHERE account_id=?", "host", actor.accountId));
      }
      return response;
    };
  });
  const response = await users(`/${target}/disable`, { method: "POST", session: actor.session });
  expect(response.status).toBe(403);
  safeHeaders(response);
  const state = await inOwner((_instance, state) => ({
    disabled: state.storage.sql.exec("SELECT disabled_at FROM accounts WHERE account_id=?", target).one().disabled_at,
    pending: state.storage.sql.exec("SELECT count(*) AS n FROM pending_account_removals WHERE target_account_id=?", target).one().n,
  }));
  expect(state).toEqual({ disabled: null, pending: 0 });
  expect(await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_assignment_gates WHERE account_id=?", target).one().n)).toBe(0);
});

test("session revocation during Directory reconciliation finishes cleanup but withholds the receipt", async () => {
  const actor = await admin();
  const target = (await createHost(actor)).account.account_id;
  await runInDurableObject(directoryStub(), instance => {
    instance.originalUsersFetch = instance.fetch.bind(instance);
    let changed = false;
    instance.fetch = async request => {
      const message = await request.clone().json();
      const response = await instance.originalUsersFetch(request);
      if (message.action === "reconcile" && !changed) {
        changed = true;
        await inOwner((_instance, state) => state.storage.sql.exec("UPDATE account_sessions SET revoked_at=? WHERE account_id=?", Date.now(), actor.accountId));
      }
      return response;
    };
  });
  const response = await users(`/${target}/disable`, { method: "POST", session: actor.session });
  expect(response.status).toBe(401);
  const result = await response.json();
  expect(Object.hasOwn(result, "receipt")).toBe(false);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT disabled_at FROM accounts WHERE account_id=?", target).one().disabled_at)).not.toBeNull();
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM pending_account_removals WHERE target_account_id=?", target).one().n)).toBe(0);
  expect(await runInDurableObject(directoryStub(), (_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_assignment_gates WHERE account_id=?", target).one().n)).toBe(0);
});

test("Users result is withheld if the issuer loses authority during the final storage sync", async () => {
  const actor = await admin();
  const secret = actor.session.split("=", 2)[1];
  const responseStatus = await inOwner(async (instance, state) => {
    const original = state.storage.sync;
    state.storage.sync = async function () {
      await original.call(this);
      state.storage.sql.exec("UPDATE accounts SET role=? WHERE account_id=?", "host", actor.accountId);
    };
    try {
      const response = await instance.fetch(new Request("https://accounts.internal/users", {
        method: "POST", body: JSON.stringify({ command: { operation: "list_accounts", after: null, limit: 50 }, command_id: null, session_token: secret }),
      }));
      const body = await response.json();
      return { status: response.status, hasUsers: Object.hasOwn(body, "users") };
    } finally { state.storage.sync = original; }
  });
  expect(responseStatus).toEqual({ status: 403, hasUsers: false });
});

test("self-reset commits credential revocation but cannot hand the URL to a revoked session", async () => {
  const actor = await admin();
  const id = commandId();
  const before = await inOwner((_instance, state) => state.storage.sql.exec("SELECT credential_epoch FROM accounts WHERE account_id=?", actor.accountId).one().credential_epoch);
  const response = await users(`/${actor.accountId}/password-reset-links`, { method: "POST", session: actor.session, id });
  expect(response.status).toBe(401);
  safeHeaders(response);
  const body = await response.json();
  expect(Object.hasOwn(body, "reset_url")).toBe(false);
  expect(Object.hasOwn(body, "receipt")).toBe(false);
  const snapshot = await inOwner((_instance, state) => ({
    account: state.storage.sql.exec("SELECT status,credential_epoch FROM accounts WHERE account_id=?", actor.accountId).one(),
    receipts: state.storage.sql.exec("SELECT count(*) AS n FROM management_receipts WHERE actor=? AND command_id=?", actor.accountId, id).one().n,
    links: state.storage.sql.exec("SELECT count(*) AS n FROM access_links WHERE account_id=? AND purpose=? AND revoked_at IS NULL", actor.accountId, "password_reset").one().n,
  }));
  expect(snapshot.account).toEqual({ status: "reset_required", credential_epoch: before + 1 });
  expect(snapshot.receipts).toBe(1);
  expect(snapshot.links).toBe(1);
  expect((await users(`/${actor.accountId}/password-reset-links`, { method: "POST", session: actor.session, id })).status).toBe(401);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT credential_epoch FROM accounts WHERE account_id=?", actor.accountId).one().credential_epoch)).toBe(before + 1);
});

test("lost Directory release acknowledgement returns Pending, then the same command completes once", async () => {
  const actor = await admin();
  const target = (await createHost(actor, "ApiAckRetryHost")).account.account_id;
  const id = commandId();
  await runInDurableObject(directoryStub(), instance => {
    instance.originalUsersFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const message = await request.clone().json();
      const response = await instance.originalUsersFetch(request);
      if (message.action === "reconcile") throw new Error("test-only lost Users release acknowledgement");
      return response;
    };
  });
  const pending = await users(`/${target}/disable`, { method: "POST", session: actor.session, id });
  expect(pending.status).toBe(202);
  safeHeaders(pending);
  const outcome = await pending.json();
  expect(outcome.result).toBe("pending");
  expect(Object.keys(outcome).sort()).toEqual(["operation_id", "result"]);
  const snapshot = await inOwner((_instance, state) => state.storage.sql.exec("SELECT phase,attempt_count,next_attempt_at FROM pending_account_removals WHERE operation_id=?", outcome.operation_id).one());
  expect(snapshot.phase).toBe("committed");
  expect(snapshot.attempt_count).toBeGreaterThan(0);
  expect(snapshot.next_attempt_at).toBeGreaterThan(Date.now()-500);
  await runInDurableObject(directoryStub(), instance => { instance.fetch = instance.originalUsersFetch; });
  const retry = await users(`/${target}/disable`, { method: "POST", session: actor.session, id });
  expect(retry.status).toBe(200);
  expect((await retry.json()).result).toBe("disabled");
  const replay = await users(`/${target}/disable`, { method: "POST", session: actor.session, id });
  expect((await replay.json()).result).toBe("committed");
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit WHERE actor=? AND operation=? AND outcome=?", actor.accountId, "disable_account", "succeeded").one().n)).toBe(1);
});

test("concurrent admin removal replies bind one receipt and one accepted mutation", async () => {
  const actor = await admin();
  const target = (await createHost(actor, "ApiConcurrentHost")).account.account_id;
  const id = commandId();
  await runInDurableObject(directoryStub(), instance => {
    instance.originalUsersFetch = instance.fetch.bind(instance);
    let acquired = 0;
    instance.fetch = async request => {
      const message = await request.clone().json();
      const ordinal = message.action === "acquire" ? ++acquired : 0;
      const response = await instance.originalUsersFetch(request);
      if (ordinal) await new Promise(resolve => setTimeout(resolve, ordinal === 1 ? 100 : 300));
      return response;
    };
  });
  const responses = await Promise.all([0, 1].map(() => users(`/${target}/disable`, { method: "POST", session: actor.session, id })));
  expect(responses.map(response => response.status)).toEqual([200, 200]);
  const results = await Promise.all(responses.map(response => response.json()));
  expect(results[0].receipt).toEqual(results[1].receipt);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit WHERE actor=? AND operation=? AND outcome=?", actor.accountId, "disable_account", "succeeded").one().n)).toBe(1);
});

test("outstanding socket acknowledgements prevent completed Users removal claims", async () => {
  const actor = await admin();
  const target = (await createHost(actor, "ApiSocketPendingHost")).account.account_id;
  await inOwner((_instance, state) => state.storage.sql.exec("INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) VALUES(?,?,?,?,0,?)", target, commandId(), commandId(), commandId(), Date.now()+86400000));
  const response = await users(`/${target}/disable`, { method: "POST", session: actor.session });
  expect(response.status).toBe(202);
  const result = await response.json();
  expect(result.result).toBe("pending");
  expect(Object.hasOwn(result, "receipt")).toBe(false);
  await inOwner(async (instance, state) => {
    state.storage.sql.exec("UPDATE pending_account_removals SET next_attempt_at=? WHERE operation_id=?", Date.now()-1, result.operation_id);
    await instance.alarm();
  });
  const retained = await inOwner((_instance, state) => state.storage.sql.exec("SELECT phase,attempt_count,next_attempt_at FROM pending_account_removals WHERE operation_id=?", result.operation_id).one());
  expect(retained.phase).toBe("committed");
  expect(retained.attempt_count).toBeGreaterThan(1);
  expect(retained.next_attempt_at).toBeGreaterThan(Date.now()-500);
});

test("Users handoffs and denials log only closed metadata, never credentials or URLs", async () => {
  const actor = await admin();
  const captured = [];
  const originals = ["log", "debug", "info", "warn", "error"].map(level => {
    const original = console[level];
    console[level] = (...args) => captured.push({ level, args });
    return { level, original };
  });
  try {
    const created = await createHost(actor, "ApiPrivateLogHost");
    const target = created.account.account_id;
    expect((await users(`/${target}/enable`, { method: "POST", session: actor.session })).status).toBe(200);
    const token = new URL(created.enrollment_url).hash.slice(1);
    const encoded = JSON.stringify(captured);
    for (const secret of [actor.session, actor.password, actor.token, created.enrollment_url, token, "ApiPrivateLogHost", target, created.receipt.link_id, env.DEV_CLI_KEY, env.RATE_LIMIT_KEY]) {
      expect(encoded.includes(secret)).toBe(false);
    }
    expect(captured.length).toBeGreaterThan(0);
    for (const { level, args } of captured) {
      expect(args.length === 1 && typeof args[0] === "string").toBe(true);
      const event = JSON.parse(args[0]);
      expect(event.target).toBe("brews.security");
      expect(event.level.toLowerCase()).toBe(level);
      expect(Object.keys(event.fields).sort()).toEqual(["command_id", "delivery", "event", "operation"]);
    }
  } finally {
    for (const { level, original } of originals) console[level] = original;
  }
});

test("Users commands share the actor's auth idempotency namespace", async () => {
  const actor = await admin();
  const id = commandId();
  const login = await post("/api/auth/login", { username: actor.username, password: actor.password }, { id });
  expect(login.status).toBe(200);
  expect((await users("/hosts", { method: "POST", session: cookie(login), id, body: { username: "ApiConflictedIdHost" } })).status).toBe(409);
  expect(await inOwner((_instance, state) => state.storage.sql.exec("SELECT count(*) AS n FROM accounts WHERE username=?", "ApiConflictedIdHost").one().n)).toBe(0);
});

test("strict public and private Users envelopes reject ambiguous authority claims", async () => {
  const actor = await admin();
  for (const body of [ '["ApiCreatedHost"]', '{"username":"ApiCreatedHost","username":"ApiCreatedHost"}', '{"username":"ApiCreatedHost","actor":"developer_cli"}' ]) {
    const response = await exports.default.fetch(`${origin}/api/users/hosts`, { method: "POST", headers: { Origin: origin, Cookie: actor.session, "Content-Type": "application/json", "Idempotency-Key": commandId() }, body });
    expect(response.status).toBe(400);
  }
  const badHeader = new Headers({ Origin: origin, Cookie: actor.session, "Idempotency-Key": commandId() });
  badHeader.append("Idempotency-Key", commandId());
  expect((await exports.default.fetch(`${origin}/api/users/${actor.accountId}/disable`, { method: "POST", headers: badHeader })).status).toBe(400);
  expect((await users("/hosts", { method: "POST", session: actor.session, id: commandId().toUpperCase(), body: { username: "ApiInvalidKeyHost" } })).status).toBe(400);
  expect((await users("/hosts", { method: "POST", session: actor.session, id: null, body: { username: "ApiMissingKeyHost" } })).status).toBe(400);
  expect((await users("", { session: actor.session, id: commandId() })).status).toBe(400);
  const token = actor.session.split("=", 2)[1];
  for (const body of [ '["list_accounts",null]', '{"command":{"operation":"list_accounts","after":null,"limit":50},"command_id":null,"session_token":"'+token+'","principal":"developer_cli"}', '{"command":{"operation":"list_accounts","after":null,"limit":50},"command_id":null,"command_id":null,"session_token":"'+token+'"}' ]) {
    const status = await inOwner(async instance => (await instance.fetch(new Request("https://accounts.internal/users", { method: "POST", body }))).status);
    expect(status).toBe(400);
  }
});
