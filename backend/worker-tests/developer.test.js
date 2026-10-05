import { beforeEach, expect, test } from "vitest";
import { runInDurableObject } from "cloudflare:test";
import { env, exports } from "cloudflare:workers";
import { commandId, enroll, inOwner, origin, resetStorage } from "./fixtures.js";

const directoryStub = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async () => {
  await resetStorage();
  await runInDurableObject(directoryStub(), async (_instance,state) => state.storage.deleteAll());
});

async function initializeDirectory() {
  const operationId=commandId(), accountId=commandId();
  for (const action of ["acquire","release"]) {
    const response=await directoryStub().fetch("https://directory.internal/removal", {
      method:"POST",body:JSON.stringify({action,operation_id:operationId,account_id:accountId}),
    });
    expect((await response.json()).result).toBe(action==="acquire"?"acquired":"released");
  }
}

test("the restricted interface rejects browser headers, bad credentials and non-object payloads", async () => {
  const body=JSON.stringify({operation:"list_accounts",limit:50,after:null});
  const credential=["Bearer",env.DEV_CLI_KEY].join(" ");
  let caseIndex=0;
  for (const [headers,payload,url,expected] of [
    [{"Content-Type":"application/json"},body,origin,403],
    [{"Content-Type":"application/json",Authorization:["Bearer",env.RATE_LIMIT_KEY].join(" ")},body,origin,403],
    [{"Content-Type":"application/json",Authorization:credential,Origin:origin},body,origin,403],
    [{"Content-Type":"application/json",Authorization:credential,Cookie:"__Host-brews_session=invalid"},body,origin,403],
    [{"Content-Type":"application/json",Authorization:credential},body,origin.replace("https:","http:"),403],
    [{"Content-Type":"application/json",Authorization:credential},'["list_accounts",null,50]',origin,400],
    [{"Content-Type":"application/json",Authorization:credential},'{"operation":"list_accounts","limit":50,"limit":50}',origin,400],
    [{"Content-Type":"application/json",Authorization:credential},'{"operation":"list_accounts","limit":50,"actor":"developer_cli"}',origin,400],
  ]) {
    const response=await exports.default.fetch(`${url}/_dev/commands`,{method:"POST",headers,body:payload});
    expect(response.status,`rejection case ${caseIndex++}`).toBe(expected);
    expect(response.headers.has("Set-Cookie")).toBe(false);
  }
  const duplicates=new Headers({"Content-Type":"application/json",Authorization:credential});
  duplicates.append("Authorization",credential);
  expect((await exports.default.fetch(`${origin}/_dev/commands`,{method:"POST",headers:duplicates,body})).status).toBe(403);
  expect(await inOwner((_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit").one().n)).toBe(0);
});

test("the CLI transport requires an exact canonical mutation command ID", async () => {
  const body=JSON.stringify({operation:"create_account",username:"MutationHeaderHost",role:"host"});
  for (const id of [undefined,commandId().toUpperCase(),commandId().replaceAll("-",""),"01890f3e-53b7-4d28-9b05-4f65092d5711"]) {
    const headers={"Content-Type":"application/json",Authorization:["Bearer",env.DEV_CLI_KEY].join(" ")};
    if (id) headers["Idempotency-Key"]=id;
    expect((await exports.default.fetch(`${origin}/_dev/commands`,{method:"POST",headers,body})).status).toBe(400);
  }
  const headers=new Headers({"Content-Type":"application/json",Authorization:["Bearer",env.DEV_CLI_KEY].join(" "),"Idempotency-Key":commandId()});
  headers.append("Idempotency-Key",commandId());
  expect((await exports.default.fetch(`${origin}/_dev/commands`,{method:"POST",headers,body})).status).toBe(400);
  expect(await inOwner((_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM accounts").one().n)).toBe(0);
});

test("Directory cancellation seals late acquires and preserves other operations' gates", async () => {
  await initializeDirectory();
  const accountId=commandId(), canceled=commandId(), active=commandId();
  const peer=async(action,id)=>{
    const response=await directoryStub().fetch("https://directory.internal/removal",{method:"POST",body:JSON.stringify({action,operation_id:id,account_id:accountId})});
    return response.json();
  };
  expect((await peer("acquire",active)).result).toBe("acquired");
  expect((await peer("reconcile",canceled)).result).toBe("released");
  const late=await peer("acquire",canceled);
  expect(late.result).toBe("rejected");
  expect(late.code).toBe("completed");
  expect(await runInDurableObject(directoryStub(),(_instance,state)=>state.storage.sql.exec("SELECT operation_id FROM directory_removal_pending WHERE account_id=?",accountId).one().operation_id)).toBe(active);
  expect((await peer("release",active)).result).toBe("released");
});

test("Directory receipt cleanup is alarm-driven rather than waiting for a peer visit", async () => {
  await initializeDirectory();
  const scheduled=await runInDurableObject(directoryStub(),(_instance,state)=>state.storage.getAlarm());
  expect(scheduled).not.toBeNull();
  await runInDurableObject(directoryStub(),async(instance,state)=>{
    const expired=Date.now()-1, completed=expired-86400000;
    const sample=commandId();
    const timestamp=BigInt(completed).toString(16).padStart(12,"0");
    const oldId=timestamp.slice(0,8)+"-"+timestamp.slice(8)+sample.slice(13);
    state.storage.sql.exec("UPDATE directory_removal_receipts SET operation_id=?,completed_at=?",oldId,completed);
    await instance.alarm();
    expect(state.storage.sql.exec("SELECT count(*) AS n FROM directory_removal_receipts").one().n).toBe(0);
    expect(await state.storage.getAlarm()).toBeNull();
  });
});

test("hosted-game rejection reconciles the gate and account intent before responding", async () => {
  const created=await command({operation:"create_account",username:"ActiveGameHost",role:"host"});
  expect(created.status).toBe(200);
  const accountId=(await created.json()).receipt.account_id;
  await initializeDirectory();
  await runInDurableObject(directoryStub(), (_instance,state) => {
    state.storage.sql.exec("INSERT INTO directory_hosted_nonterminal_games(game_id,designated_host_id) VALUES(?,?)",commandId(),accountId);
  });
  const response=await command({operation:"disable_account",account_id:accountId});
  expect(response.status).toBe(409);
  expect((await response.json()).error.code).toBe("hosted_game");
  expect(await inOwner((_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM pending_account_removals").one().n)).toBe(0);
  expect(await runInDurableObject(directoryStub(), (_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM account_assignment_gates").one().n)).toBe(0);
});

test("an audit-write failure rolls back actual Worker creation and withholds results", async () => {
  await inOwner((_instance,state)=>state.storage.sql.exec("CREATE TRIGGER reject_management_audit BEFORE INSERT ON admin_audit BEGIN SELECT RAISE(ABORT,'blocked'); END"));
  const response=await command({operation:"create_account",username:"AuditFailureHost",role:"host"});
  expect(response.status).toBe(503);
  expect(response.headers.has("Set-Cookie")).toBe(false);
  expect(await inOwner((_instance,state)=>state.storage.sql.exec("SELECT count(*) AS n FROM accounts").one().n)).toBe(0);
  const read=await exports.default.fetch(`${origin}/_dev/commands`,{
    method:"POST",headers:{"Content-Type":"application/json",Authorization:["Bearer",env.DEV_CLI_KEY].join(" ")},
    body:JSON.stringify({operation:"list_accounts",limit:50,after:null}),
  });
  expect(read.status).toBe(503);
  expect((await read.json()).error.code).toBe("unavailable");
});

test("dedicated CLI credential allows safe account listing without cookies", async () => {
  const response = await exports.default.fetch(`${origin}/_dev/commands`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: ["Bearer", env.DEV_CLI_KEY].join(" ") },
    body: JSON.stringify({ operation: "list_accounts", after: null, limit: 50 }),
  });
  expect(response.status).toBe(200);
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  expect(response.headers.has("Set-Cookie")).toBe(false);
  const result = await response.json();
  expect(result.result).toBe("accounts");
  expect(result.accounts.length).toBe(0);
});

test("CLI reads schedule audit retention and alarms purge expired events", async () => {
  const response = await exports.default.fetch(`${origin}/_dev/commands`, {
    method: "POST", headers: { "Content-Type": "application/json", Authorization: ["Bearer", env.DEV_CLI_KEY].join(" ") },
    body: JSON.stringify({ operation: "list_accounts", after: null, limit: 50 }),
  });
  expect(response.status).toBe(200);
  const scheduled = await inOwner(async (_instance, state) => state.storage.getAlarm());
  expect(scheduled).not.toBeNull();
  await inOwner(async (instance, state) => {
    const expired = Date.now() - 1;
    state.storage.sql.exec("UPDATE admin_audit SET occurred_at=?,expires_at=?", expired - 90*86400000, expired);
    await instance.alarm();
    expect(state.storage.sql.exec("SELECT count(*) AS n FROM admin_audit").one().n).toBe(0);
  });
});

async function command(body, id = commandId()) {
  return exports.default.fetch(`${origin}/_dev/commands`, {
    method: "POST", headers: { "Content-Type": "application/json", Authorization: ["Bearer", env.DEV_CLI_KEY].join(" "), "Idempotency-Key": id },
    body: JSON.stringify(body),
  });
}

test("persistent local runtime metadata does not masquerade as an unknown Directory schema", async () => {
  await runInDurableObject(directoryStub(), (_instance,state) => {
    state.storage.sql.exec("CREATE TABLE IF NOT EXISTS __miniflare_do_name (id INTEGER PRIMARY KEY, name TEXT)");
    state.storage.sql.exec("INSERT OR REPLACE INTO __miniflare_do_name(id,name) VALUES(1,'directory')");
  });
  const created = await command({ operation: "create_account", username: "LocalRuntimeHost", role: "host" });
  expect(created.status).toBe(200);
  const accountId = (await created.json()).receipt.account_id;
  const response = await command({ operation: "disable_account", account_id: accountId });
  expect(response.status).toBe(200);
});

test("socket-close pending removal uses durable backoff instead of spinning its alarm", async () => {
  const fixture=await enroll();
  await inOwner((_instance,state)=>{
    const session=state.storage.sql.exec("SELECT session_id,credential_epoch,expires_at FROM account_sessions WHERE account_id=? AND scope='normal' ORDER BY issued_at DESC LIMIT 1",fixture.accountId).one();
    state.storage.sql.exec("INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) VALUES(?,?,?,?,?,?)",fixture.accountId,session.session_id,commandId(),commandId(),session.credential_epoch,session.expires_at);
  });
  const response=await command({operation:"disable_account",account_id:fixture.accountId});
  expect(response.status).toBe(202);
  const work=await inOwner((_instance,state)=>state.storage.sql.exec("SELECT phase,attempt_count,next_attempt_at FROM pending_account_removals").one());
  expect(work.phase).toBe("committed");
  expect(work.attempt_count).toBeGreaterThan(0);
  expect(work.next_attempt_at).toBeGreaterThan(Date.now()-500);
});

test("CLI disable of an enrolled account finishes without socket work", async () => {
  const fixture = await enroll();
  const response = await command({ operation: "disable_account", account_id: fixture.accountId });
  const closeCount = await inOwner((_instance,state) => state.storage.sql.exec("SELECT count(*) AS n FROM account_socket_close_work").one().n);
  expect(closeCount).toBe(0);
  expect(response.status).toBe(200);
});

test("CLI disable completes only after Directory release and returns a secret-free receipt", async () => {
  const created = await command({ operation: "create_account", username: "HostedAccount", role: "host" });
  expect(created.status).toBe(200);
  const accountId = (await created.json()).receipt.account_id;
  const response = await command({ operation: "disable_account", account_id: accountId });
  expect(response.status).toBe(200);
  const result = await response.json();
  expect(result.result).toBe("committed");
  expect(result.receipt.operation).toBe("disable_account");
  expect(result.receipt.account_id).toBe(accountId);
  expect(Object.hasOwn(result,"url")).toBe(false);
});
