import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject, evictDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, commandId, inOwner } from "./fixtures.js";
const clock=Date.now;
const owner=id=>env.GAMES.get(env.GAMES.idFromName(id));
const dir=()=>env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async()=>{Date.now=clock;await reset();await resetStorage();await runInDurableObject(dir(),async(_,s)=>s.storage.deleteAll());});
async function fixture(){const a=await enroll("RuntimeReviewHost"),b=await enroll("RuntimeReviewTarget");const r=await post("/api/games",{},{session:a.session});expect(r.status).toBe(200);return {a,b,id:(await r.json()).game.game_id};}
async function management(operation,accountId){return exports.default.fetch(`${origin}/_dev/commands`,{method:"POST",headers:{"Content-Type":"application/json",Authorization:["Bearer",env.DEV_CLI_KEY].join(" "),"Idempotency-Key":commandId()},body:JSON.stringify({operation,account_id:accountId})});}
async function transfer(f){return post(`/api/games/${f.id}/host-transfer`,{target_account_id:f.b.accountId,confirmed:true,expected_host_assignment_revision:0},{session:f.a.session});}
async function restore(stub){await runInDurableObject(stub(),i=>{if(i.reviewFetch)i.fetch=i.reviewFetch;delete i.reviewFetch;});}
for(const mutation of ["gate","disable","delete"]){
 test(`operator regression final owner target cut after ${mutation} during last Accounts await`,async()=>{
  const f=await fixture();let releasing=false,cuts=0,actualMutation;
  await runInDurableObject(owner(f.id),i=>{i.reviewFetch=i.fetch.bind(i);i.fetch=async req=>{if(new URL(req.url).pathname==="/operator-release")releasing=true;return i.reviewFetch(req);};});
  await inOwner(i=>{i.reviewFetch=i.fetch.bind(i);i.fetch=async req=>{const input=await req.clone().json();if(releasing&&new URL(req.url).pathname==="/operator-accounts"&&input.action==="authorize"&&++cuts===2){
   if(mutation==="gate")actualMutation=await (await dir().fetch("https://peer.internal/removal",{method:"POST",body:JSON.stringify({action:"acquire",operation_id:commandId(),account_id:f.b.accountId})})).json();
   else {const r=await management(mutation==="disable"?"disable_account":"delete_account",f.b.accountId);actualMutation={status:r.status,result:(await r.json()).result};}
  }return i.reviewFetch(req);};});
  try{const r=await exports.default.fetch(`${origin}/api/games/${f.id}/host-transfer-targets`,{headers:{Cookie:f.a.session}});const body=await r.json();const gated=await runInDurableObject(dir(),(_,s)=>s.storage.sql.exec("SELECT count(*) AS n FROM account_assignment_gates WHERE account_id=?",f.b.accountId).one().n);const state=await inOwner((_,s)=>s.storage.sql.exec("SELECT disabled_at FROM accounts WHERE account_id=?",f.b.accountId).toArray());
   console.log("REVIEW_FINAL_TARGET_CUT",JSON.stringify({mutation,cuts,actualMutation,receiving_status:r.status,disclosed_target:body.targets?.some(t=>t.account_id===f.b.accountId)??false,gated,account_exists:state.length===1,disabled:state[0]?.disabled_at!=null}));
   expect(cuts).toBe(2);if(mutation==="gate")expect(gated).toBe(1);else expect(actualMutation.status).toBe(200);expect(body.targets?.some(t=>t.account_id===f.b.accountId)??false).toBe(false);
  }finally{await runInDurableObject(owner(f.id),i=>{i.fetch=i.reviewFetch;delete i.reviewFetch;});await inOwner(i=>{i.fetch=i.reviewFetch;delete i.reviewFetch;});}
 });
}
test("operator regression authenticated admin target read release failure receives durable Failed audit",async()=>{
 const f=await fixture(),a=await enroll("ReviewPrivilegedAdmin");await inOwner((_,s)=>s.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?",a.accountId));let release=false,dropped=false;
 await runInDurableObject(owner(f.id),i=>{i.reviewFetch=i.fetch.bind(i);i.fetch=async req=>{if(new URL(req.url).pathname==="/operator-release")release=true;return i.reviewFetch(req);};});
 await runInDurableObject(dir(),i=>{i.reviewFetch=i.fetch.bind(i);i.fetch=async req=>{const input=await req.clone().json();const genuine=await i.reviewFetch(req);if(release&&new URL(req.url).pathname==="/host-transfer"&&input.action==="targets"){dropped=true;await genuine.text();return new Response(null,{status:503});}return genuine;};});
 try{const r=await exports.default.fetch(`${origin}/api/games/${f.id}/host-transfer-targets`,{headers:{Cookie:a.session}});const body=await r.json();const audits=await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT operation,outcome,actor_account_id FROM game_admin_audit ORDER BY occurred_at,audit_id").toArray());console.log("REVIEW_PRIVILEGED_FAILED_RELEASE",JSON.stringify({dropped,status:r.status,protected:!!body.targets,outcomes:audits.map(x=>x.outcome),actual_actor:audits.every(x=>x.actor_account_id===a.accountId)}));expect(dropped).toBe(true);expect(r.status).toBe(503);expect(body.targets).toBeUndefined();expect(audits.some(x=>x.outcome==="failed")).toBe(true);
 }finally{await runInDurableObject(owner(f.id),i=>{i.fetch=i.reviewFetch;delete i.reviewFetch;});await runInDurableObject(dir(),i=>{i.fetch=i.reviewFetch;delete i.reviewFetch;});}
});
test("operator regression oversized authenticated admin operator body rejected with durable audit",async()=>{
 const f=await fixture(),a=await enroll("ReviewBodyAdmin");await inOwner((_,s)=>s.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?",a.accountId));const r=await exports.default.fetch(`${origin}/api/games/${f.id}/configuration`,{method:"PATCH",headers:{Origin:origin,Cookie:a.session,"Idempotency-Key":commandId(),"Content-Type":"application/json"},body:JSON.stringify({expected_revision:0,unexpected:"x".repeat(70000)})});await r.text();const audits=await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT operation,outcome,actor_account_id FROM game_admin_audit").toArray());console.log("REVIEW_ADMIN_OVERSIZED_TRANSPORT",JSON.stringify({status:r.status,audit_count:audits.length}));expect(r.status).toBe(413);expect(audits).toHaveLength(1);expect(audits[0].actor_account_id).toBe(a.accountId);
});

for (const lastMutation of ["gate", "disable", "delete", "reset", "unverify"]) {
 test(`operator coupled last Accounts candidate cut rejects ${lastMutation}`, async () => {
  const f = await fixture(); let inCoupled = false, coupled = 0, count = 0, mutation;
  const prior = await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT idle_due,last_host_activity,revision FROM game_record").one());
  await runInDurableObject(dir(), i => { i.reviewFetch = i.fetch.bind(i); i.fetch = async req => { const input = await req.clone().json(); if (input.action !== "release_targets") return i.reviewFetch(req); coupled++; inCoupled = true; try { return await i.reviewFetch(req); } finally { inCoupled = false; } }; });
  await inOwner(i => { i.reviewFetch = i.fetch.bind(i); i.fetch = async req => {
   const input = await req.clone().json();
   if (inCoupled && coupled === 2 && input.action === "targets" && ++count === 1) {
    if (lastMutation === "gate") mutation = await (await dir().fetch("https://peer.internal/removal", { method: "POST", body: JSON.stringify({ action: "acquire", operation_id: commandId(), account_id: f.b.accountId }) })).json();
    else if (["disable", "delete"].includes(lastMutation)) { const r = await management(lastMutation === "disable" ? "disable_account" : "delete_account", f.b.accountId); mutation = { status: r.status, result: (await r.json()).result }; }
    else { await inOwner((_, s) => s.storage.sql.exec("UPDATE accounts SET status=?,credential_epoch=credential_epoch+1 WHERE account_id=?", lastMutation === "reset" ? "reset_required" : "pending_enrollment", f.b.accountId)); mutation = { status: 200 }; }
   }
   return i.reviewFetch(req);
  }; });
  try {
   const r = await exports.default.fetch(`${origin}/api/games/${f.id}/host-transfer-targets`, { headers: { Cookie: f.a.session } });
   const body = await r.json(); expect(coupled).toBe(2); expect(count).toBe(1); expect(mutation).toBeDefined(); expect(body.targets?.some(t => t.account_id === f.b.accountId) ?? false).toBe(false);
   if (lastMutation === "gate") expect(mutation.result).toBe("acquired"); else expect(mutation.status).toBe(200);
   const state = await inOwner((_, s) => s.storage.sql.exec("SELECT status,disabled_at FROM accounts WHERE account_id=?", f.b.accountId).toArray());
   if (lastMutation === "delete") expect(state).toHaveLength(0);
   if (lastMutation === "disable") expect(state[0].disabled_at).not.toBeNull();
   if (lastMutation === "reset") expect(state[0].status).toBe("reset_required");
   if (lastMutation === "unverify") expect(state[0].status).toBe("pending_enrollment");
   expect(await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT idle_due,last_host_activity,revision FROM game_record").one())).toEqual(prior);
  } finally { await restore(dir); await inOwner(i => { i.fetch = i.reviewFetch; delete i.reviewFetch; }); }
 });
}

for (const path of ["configuration", "resume", "host-transfer"]) {
 for (const identity of ["admin", "host", "anonymous"]) {
  test(`operator oversized ${path} captures only current ${identity} metadata`, async () => {
   const f = await fixture(), a = await enroll("OversizeAdmin"); await inOwner((_, s) => s.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", a.accountId));
   const headers = { Origin: origin, "Idempotency-Key": commandId(), "Content-Type": "application/json" };
   if (identity !== "anonymous") headers.Cookie = identity === "admin" ? a.session : f.a.session;
   const body = JSON.stringify({ unexpected: "SubmittedPrivateContent".repeat(4000) });
   const r = await exports.default.fetch(`${origin}/api/games/${f.id}/${path}`, { method: path === "configuration" ? "PATCH" : "POST", headers, body });
   expect(r.status).toBe(413); expect((await r.text()).includes("SubmittedPrivateContent")).toBe(false);
   const rows = await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT * FROM game_admin_audit").toArray());
   expect(rows).toHaveLength(identity === "admin" ? 1 : 0);
   if (identity === "admin") { expect(rows[0]).toMatchObject({ actor_account_id: a.accountId, target_game_id: f.id, operation: path.replaceAll("-", "_"), outcome: "rejected" }); expect(Object.keys(rows[0]).sort()).toEqual(["actor_account_id", "audit_id", "expires_at", "occurred_at", "operation", "outcome", "target_game_id"]); }
   expect(JSON.stringify(rows)).not.toContain("SubmittedPrivateContent");
  });
 }
}

for (const fault of ["ignore", "abort", "entropy"]) {
 test(`operator oversized privileged rejection fails closed on ${fault} capture`, async () => {
  const f = await fixture(), a = await enroll("CaptureFaultAdmin"); await inOwner((_, s) => s.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", a.accountId));
  const random = crypto.getRandomValues.bind(crypto);
  if (fault === "entropy") crypto.getRandomValues = () => { throw new Error("test-only entropy unavailable"); };
  else await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec(`CREATE TRIGGER capture_fault BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(${fault === "ignore" ? "IGNORE" : "ABORT,'test-only capture fault'"}); END`));
  try {
   const id = fault === "entropy" ? f.id : commandId();
   const r = await exports.default.fetch(`${origin}/api/games/${f.id}/configuration`, { method: "PATCH", headers: { Origin: origin, Cookie: a.session, "Content-Type": "application/json", "Idempotency-Key": id }, body: JSON.stringify({ unexpected: "PrivateCaptureBody".repeat(5000) }) });
   expect(r.status).toBe(503); expect(await r.text()).not.toContain("PrivateCaptureBody"); expect(await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT * FROM game_admin_audit").toArray())).toEqual([]);
  } finally { crypto.getRandomValues = random; }
 });
}

for (const first of ["transfer", "code"]) {
 test(`operator real held ${first} excludes competing intent and cancellation releases`, async () => {
  const f = await fixture(), a = await enroll("ConcurrentAdmin"); await inOwner((_, s) => s.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", a.accountId)); let collided, cancelled, count = 0;
  if (first === "transfer") await inOwner(i => { i.reviewFetch = i.fetch.bind(i); i.fetch = async req => {
   const input = await req.clone().json(); if (input.action === "recipient" && ++count === 1) {
    const collision = await post(`/api/games/${f.id}/lobby`, { expected_revision: 0 }, { session: f.a.session }); collided = { status: collision.status }; await collision.text();
    const cancellation = await post(`/api/games/${f.id}/cancel`, { confirmed: true, expected_state: "new" }, { session: a.session }); cancelled = { status: cancellation.status }; await cancellation.text();
   } return i.reviewFetch(req);
  }; });
  else await runInDurableObject(dir(), i => { i.reviewFetch = i.fetch.bind(i); i.fetch = async req => {
   const input = await req.clone().json(); if (input.action === "allocate_code" && ++count === 1) {
    // The real hidden allocation completes before the held response is delivered.
    const genuine = await i.reviewFetch(req);
    const collision = await transfer(f); collided = { status: collision.status }; await collision.text();
    const cancellation = await post(`/api/games/${f.id}/cancel`, { confirmed: true, expected_state: "new" }, { session: a.session }); cancelled = { status: cancellation.status }; await cancellation.text();
    return genuine;
   } return i.reviewFetch(req);
  }; });
  try {
   const r = first === "transfer" ? await transfer(f) : await post(`/api/games/${f.id}/lobby`, { expected_revision: 0 }, { session: f.a.session }); await r.text();
   expect(count).toBe(1); expect(collided.status).toBe(409); expect(cancelled.status).toBe(200);
   expect(await runInDurableObject(dir(), (_, s) => s.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id)).toBeNull();
   const created = await post("/api/games", {}, { session: f.a.session }); expect(created.status).toBe(200); const next = (await created.json()).game.game_id; expect(next).not.toBe(f.id);
   expect(await runInDurableObject(dir(), (_, s) => s.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id)).toBe(next);
  } finally { if (first === "transfer") await inOwner(i => { i.fetch = i.reviewFetch; delete i.reviewFetch; }); else await restore(dir); }
 });
}

for (const recovery of ["game", "directory", "undelivered"]) {
 test(`operator lost ${recovery === "undelivered" ? "undelivered abort" : "committed ACK"} past 24 hours survives cleanup and ${recovery} reconstruction`, async () => {
  const f = await fixture(), c = await enroll("LostAckSuccessor"); const sockets = [];
  const lobby = await post(`/api/games/${f.id}/lobby`, { expected_revision: 0 }, { session: f.a.session }); expect(lobby.status).toBe(200); const code = (await lobby.json()).game_code;
  try {
   for (const alias of ["LostAckPlayerA", "LostAckPlayerB"]) {
    const admission = await post(`/api/games/${f.id}/admission-context`, { game_code: code }, { id: null }); expect(admission.status).toBe(200);
    const joined = await post(`/api/games/${f.id}/players`, { game_code: code, alias }, { session: admission.headers.get("Set-Cookie").split(";", 1)[0] }); expect(joined.status).toBe(200);
    const upgraded = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`, { headers: { Origin: origin, Upgrade: "websocket", Connection: "Upgrade", Cookie: joined.headers.get("Set-Cookie").split(";", 1)[0] } }); expect(upgraded.status).toBe(101);
    const socket = upgraded.webSocket; const initial = new Promise(resolve => socket.addEventListener("message", event => { const frame = JSON.parse(event.data); if (frame.kind === "snapshot") { socket.send(JSON.stringify({ version: 1, kind: "snapshot_ack", connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision })); resolve(frame); } })); socket.accept(); sockets.push(socket); await initial;
   }
   const view = await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=account`, { headers: { Cookie: f.a.session } }); const version = (await view.json()).view_revision;
   expect((await post(`/api/games/${f.id}/start`, { expected_revision: version }, { session: f.a.session })).status).toBe(200);
   let lost = 0;
   const undelivered = recovery === "undelivered";
   await runInDurableObject(dir(), i => { i.reviewFetch = i.fetch.bind(i); i.fetch = async req => { const input = await req.clone().json(); if (undelivered && ["prepare", "settle"].includes(input.action)) { lost++; return new Response(null, { status: 503 }); } const genuine = await i.reviewFetch(req); if (input.action === "settle") { await genuine.text(); lost++; return new Response(null, { status: 503 }); } return genuine; }; });
   expect((await transfer(f)).status).toBe(undelivered ? 503 : 200); expect(lost).toBeGreaterThan(0); await restore(dir);
   const required = await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT * FROM game_host_transfers").one()); expect(required.phase).toBe(undelivered ? "aborted" : "committed");
   const stored = await runInDurableObject(dir(), (_, s) => s.storage.sql.exec("SELECT * FROM directory_host_transfers").toArray());
   expect(stored).toHaveLength(undelivered ? 0 : 1);
   if (!undelivered) { expect(stored[0].phase).toBe("committed"); expect(stored[0].completion_acknowledged_at).toBeNull(); }
   const retryAt = undelivered ? required.next_attempt_at : stored[0].next_attempt_at;
   Date.now = () => retryAt + 86400000;
   // Let the actual Directory alarm compact at the old receipt boundary, but
   // lose its delivery to Game so retention is independently observable.
   await runInDurableObject(owner(f.id), i => { i.reviewFetch = i.fetch.bind(i); i.fetch = async req => new URL(req.url).pathname === "/operator-transfer-ack" ? new Response(null, { status: 503 }) : i.reviewFetch(req); });
   await runInDurableObject(dir(), i => i.alarm());
   expect(await runInDurableObject(dir(), (_, s) => s.storage.sql.exec("SELECT completion_acknowledged_at FROM directory_host_transfers WHERE operation_id=?", required.operation_id).toArray())).toEqual(undelivered ? [] : [{ completion_acknowledged_at: null }]);
   expect(await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT count(*) AS n FROM game_host_transfers").one().n)).toBe(1);
   await runInDurableObject(owner(f.id), i => { i.fetch = i.reviewFetch; delete i.reviewFetch; });
   await evictDurableObject(owner(f.id)); await evictDurableObject(dir());
   Date.now = () => retryAt + 86410000;
   await runInDurableObject(recovery === "directory" ? dir() : owner(f.id), i => i.alarm());
   expect(await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT count(*) AS n FROM game_host_transfers").one().n)).toBe(0);
   expect(await runInDurableObject(dir(), (_, s) => s.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id)).toBe(f.id);
   const current = undelivered ? f.a : f.b;
   expect(await runInDurableObject(dir(), (_, s) => s.storage.sql.exec("SELECT designated_host_id FROM directory_hosted_nonterminal_games WHERE game_id=?", f.id).one().designated_host_id)).toBe(current.accountId);
   // Refresh only the current host; owner completion needs no viewer credential.
   await inOwner((_, s) => s.storage.sql.exec("UPDATE account_sessions SET issued_at=?,expires_at=? WHERE account_id=?", Date.now(), Date.now() + 86400000, current.accountId));
   const successor = await post(`/api/games/${f.id}/host-transfer`, { target_account_id: c.accountId, confirmed: true, expected_host_assignment_revision: undelivered ? 0 : 1 }, { session: current.session }); expect(successor.status).toBe(200);
   expect(await runInDurableObject(dir(), (_, s) => s.storage.sql.exec("SELECT designated_host_id FROM directory_hosted_nonterminal_games WHERE game_id=?", f.id).one().designated_host_id)).toBe(c.accountId);
  } finally {
   Date.now = clock;
   await runInDurableObject(owner(f.id), i => { if (i.reviewFetch) { i.fetch = i.reviewFetch; delete i.reviewFetch; } });
   const closed = sockets.map(socket => socket.readyState === 3 ? Promise.resolve() : new Promise(resolve => socket.addEventListener("close", resolve, { once: true }))); for (const socket of sockets) if (socket.readyState !== 3) socket.close(1000); await Promise.all(closed);
  }
 });
}

for (const fault of ["ignore", "abort", "entropy"]) {
 test(`operator release failure cannot claim captured audit on ${fault}`, async () => {
  const f = await fixture(), a = await enroll("ReleaseFaultAdmin"); await inOwner((_, s) => s.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", a.accountId)); let lost = 0;
  const random = crypto.getRandomValues.bind(crypto);
  await runInDurableObject(dir(), i => { i.reviewFetch = i.fetch.bind(i); i.fetch = async req => {
   const input = await req.clone().json(); const genuine = await i.reviewFetch(req);
   if (input.action === "release_targets" && ++lost === 2) {
    await genuine.text();
    if (fault === "entropy") crypto.getRandomValues = () => { throw new Error("test-only capture entropy unavailable"); };
    else await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec(`CREATE TRIGGER failed_capture_fault BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(${fault === "ignore" ? "IGNORE" : "ABORT,'test-only failed capture fault'"}); END`));
    return new Response(null, { status: 503 });
   } return genuine;
  }; });
  try {
   const r = await exports.default.fetch(`${origin}/api/games/${f.id}/host-transfer-targets`, { headers: { Cookie: a.session } }); expect(lost).toBe(2); expect(r.status).toBe(503); expect(await r.json()).not.toHaveProperty("targets");
   const rows = await runInDurableObject(owner(f.id), (_, s) => s.storage.sql.exec("SELECT operation,outcome,actor_account_id FROM game_admin_audit").toArray()); expect(rows).toEqual([{ operation: "host_transfer_targets", outcome: "succeeded", actor_account_id: a.accountId }]);
  } finally { crypto.getRandomValues = random; await restore(dir); }
 });
}
