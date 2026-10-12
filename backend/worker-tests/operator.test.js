import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject, evictDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, commandId, inOwner } from "./fixtures.js";
const originalOperatorDateNow=Date.now;
const owner=id=>env.GAMES.get(env.GAMES.idFromName(id));
const directory=()=>env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async()=>{Date.now=originalOperatorDateNow;await reset();await resetStorage();await runInDurableObject(directory(),async(_,state)=>state.storage.deleteAll());});
async function fixture(suffix="Person"){
 const host=await enroll("OperatorHost"+suffix);
 const created=await post("/api/games",{}, {session:host.session});expect(created.status).toBe(200);
 return {host,id:(await created.json()).game.game_id};
}
async function admin(){const result=await enroll("OperatorAdminPerson");await inOwner((_,state)=>state.storage.sql.exec("UPDATE accounts SET role=? WHERE account_id=?","admin",result.accountId));return result;}
async function patch(f,body,options={}){return exports.default.fetch(`${origin}/api/games/${f.id}/configuration`,{method:"PATCH",headers:{Origin:origin,"Content-Type":"application/json","Idempotency-Key":options.id??commandId(),Cookie:options.session??f.host.session},body:typeof body==="string"?body:JSON.stringify(body)});}
async function sync(f,session=f.host.session){const response=await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=account`,{headers:{Cookie:session}});expect(response.status).toBe(200);return response.json();}
async function rows(f){return runInDurableObject(owner(f.id),(_,state)=>({game:state.storage.sql.exec("SELECT host_id,host_assignment_revision,revision,idle_due,last_host_activity,state,game_code FROM game_record").toArray(),audits:state.storage.sql.exec("SELECT * FROM game_admin_audit ORDER BY occurred_at,audit_id").toArray(),transfers:state.storage.sql.exec("SELECT * FROM game_host_transfers").toArray(),receipts:state.storage.sql.exec("SELECT count(*) AS count FROM game_receipts").one().count}));}
async function targets(f,session=f.host.session,query=""){return exports.default.fetch(`${origin}/api/games/${f.id}/host-transfer-targets${query}`,{headers:{Cookie:session}});}
async function transfer(f,target,revision=0,options={}){return post(`/api/games/${f.id}/host-transfer`,{target_account_id:target.accountId,confirmed:true,expected_host_assignment_revision:revision},{session:options.session??f.host.session,id:options.id??commandId()});}
test("operator C7 closed patch all fields omission null duplicate unknown and atomic failure",async()=>{
 const f=await fixture();
 for(const body of ["[]","null",'{"expected_revision":0,"numeric_upper_bound":null}','{"expected_revision":0,"free_cell_positions":null}','{"expected_revision":0,"player_capacity":3,"player_capacity":4}','{"expected_revision":0,"unknown":1}','{}','{"expected_revision":9007199254740992}'])expect((await patch(f,body)).status,body).toBe(400);
 for(const body of [{expected_revision:0,board_side_length:2},{expected_revision:0,free_cells_enabled:false},{expected_revision:0,numeric_upper_bound:1},{expected_revision:0,free_cell_positions:[{row:1,column:1},{row:1,column:1}]}])expect((await patch(f,body)).status).toBe(400);
 expect((await rows(f)).game[0].revision).toBe(0);
 const accepted=await patch(f,{expected_revision:0,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],numeric_upper_bound:4,player_capacity:3,spectator_capacity:0,winning_pattern:"single_line"});expect(accepted.status).toBe(200);const result=await accepted.json();expect(result.result).toBe("configuration_updated");expect(result.configuration.free_cell_positions).toEqual([]);expect(result.configuration.numeric_upper_bound).toBe(4);expect(result.receipt.outcome.changed).toBe(true);expect(accepted.headers.get("Set-Cookie")).toBeNull();
 expect((await patch(f,{expected_revision:0,player_capacity:4})).status).toBe(409);
 expect((await patch(f,{expected_revision:1,player_capacity:4,winning_pattern:"full_house"})).status).toBe(400);
 const lobby=await post(`/api/games/${f.id}/lobby`,{expected_revision:1},{session:f.host.session});expect(lobby.status).toBe(200);expect((await lobby.json()).configuration).toEqual(result.configuration);
 expect((await patch(f,{expected_revision:2,player_capacity:4})).status).toBe(409);
});
test("operator C7 current credentials exact Origin mandatory header and no no-op renewal",async()=>{
 const f=await fixture();let before=(await rows(f)).game[0];const id=commandId();
 const response=await patch(f,{expected_revision:0},{id});expect(response.status).toBe(200);const result=await response.json();expect(result.receipt.outcome.changed).toBe(false);expect((await rows(f)).game[0]).toEqual(before);
 const retry=await patch(f,{expected_revision:0},{id});expect(retry.status).toBe(200);expect(await retry.json()).toEqual({result:"committed",receipt:result.receipt});expect((await rows(f)).game[0]).toEqual(before);
 for(const [name,headers,status] of [["missing key",{Origin:origin,Cookie:f.host.session},400],["wrong origin",{Origin:"https://wrong.example",Cookie:f.host.session,"Idempotency-Key":commandId()},403],["bad cookie",{Origin:origin,Cookie:"__Host-brews_session=invalid","Idempotency-Key":commandId()},401]]){const response=await exports.default.fetch(`${origin}/api/games/${f.id}/configuration`,{method:"PATCH",headers:{...headers,"Content-Type":"application/json"},body:'{"expected_revision":0}'});expect(response.status,name).toBe(status);}
});
test("operator C9 intentional existing resume renews once read-only host/admin passive and retry do not",async()=>{
 const f=await fixture(),other=await enroll("OperatorOtherHost"),a=await admin();const before=(await rows(f)).game[0];
 const passive=await sync(f);expect(passive.snapshot.game.game_id).toBe(f.id);expect((await rows(f)).game[0]).toEqual(before);
 for(const actor of [other,a]){const response=await post(`/api/games/${f.id}/resume`,{}, {session:actor.session});expect(response.status).toBe(200);const result=await response.json();expect(result.game.game.game_id).toBe(f.id);expect(result.can_mutate).toBe(actor===a);expect((await rows(f)).game[0]).toEqual(before);}
 const id=commandId(),response=await post(`/api/games/${f.id}/resume`,{}, {session:f.host.session,id});expect(response.status).toBe(200);const result=await response.json();expect(result.result).toBe("resumed");expect(result.game.game_code).toBeNull();expect(result.can_mutate).toBe(true);const updated=(await rows(f)).game[0];expect(updated.idle_due).toBeGreaterThan(before.idle_due);
 const retry=await post(`/api/games/${f.id}/resume`,{}, {session:f.host.session,id});expect(retry.status).toBe(200);expect(await retry.json()).toEqual({result:"committed",receipt:result.receipt});expect((await rows(f)).game[0]).toEqual(updated);
 expect((await post(`/api/games/${f.id}/resume`,{expected_state:"new"},{session:f.host.session})).status).toBe(400);
});
test("operator C10 safe discovery and transfer preserves reservation newhost authority outgoing exact retry",async()=>{
 const f=await fixture(),b=await enroll("OperatorTargetB"),c=await enroll("OperatorTargetC"),a=await admin();
 expect((await targets(f,b.session)).status).toBe(403);
 const listing=await targets(f);expect(listing.status).toBe(200);const list=await listing.json();expect(list.targets.map(t=>t.account_id).sort()).toEqual([b.accountId,c.accountId].sort());for(const entry of list.targets)expect(Object.keys(entry).sort()).toEqual(["account_id","username"]);expect(list.host_assignment_revision).toBe(0);
 for(const query of ["?limit=0","?limit=101","?limit=1&limit=2","?role=host","?cursor=bad","?limit=01"])expect((await targets(f,f.host.session,query)).status,query).toBe(400);
 expect((await transfer(f,a)).status).toBe(409);expect((await transfer(f,f.host)).status).toBe(409);expect((await transfer(f,b,1)).status).toBe(409);
 const id=commandId(),response=await transfer(f,b,0,{id});expect(response.status).toBe(200);const result=await response.json();expect(result.receipt.outcome.designated_host_id).toBe(b.accountId);expect(result.receipt.outcome.host_assignment_revision).toBe(1);
 expect((await patch(f,{expected_revision:1,player_capacity:3})).status).toBe(403);
 const retry=await transfer(f,b,0,{id});expect(retry.status).toBe(200);expect(await retry.json()).toEqual({result:"committed",receipt:result.receipt});
 expect((await transfer(f,c,1,{id})).status).toBe(409);expect((await targets(f)).status).toBe(403);
 const mutate=await patch(f,{expected_revision:(await sync(f,b.session)).view_revision,player_capacity:3},{session:b.session});expect(mutate.status).toBe(200);
 const transferred=await transfer(f,c,1,{session:b.session});expect(transferred.status).toBe(200);
 const indexed=await runInDurableObject(directory(),(_,state)=>({index:state.storage.sql.exec("SELECT designated_host_id FROM directory_game_index WHERE game_id=?",f.id).one(),reserved:state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one()}));expect(indexed.index.designated_host_id).toBe(c.accountId);expect(indexed.reserved.game_id).toBe(f.id);
 const lobby=await post(`/api/games/${f.id}/lobby`,{expected_revision:(await sync(f,c.session)).view_revision},{session:c.session});expect(lobby.status).toBe(200);
 expect((await rows(f)).game[0].host_assignment_revision).toBe(2);
});
test("operator recipient disabled pending reset-required admin gate invalid and stale assignment",async()=>{
 for(const condition of ["disabled","pending_enrollment","reset_required","admin","gate"]){
  const f=await fixture(condition),b=await enroll("OperatorTarget"+condition);
  if(condition==="gate")await runInDurableObject(directory(),(_,state)=>{const now=Date.now();const op=commandId();state.storage.sql.exec("INSERT INTO account_assignment_gates VALUES(?)",b.accountId);state.storage.sql.exec("INSERT INTO directory_removal_pending VALUES(?,?,?)",op,b.accountId,now);});
  else await inOwner((_,state)=>{if(condition==="disabled")state.storage.sql.exec("UPDATE accounts SET disabled_at=? WHERE account_id=?",Date.now(),b.accountId);else if(condition==="admin")state.storage.sql.exec("UPDATE accounts SET role=? WHERE account_id=?","admin",b.accountId);else if(condition==="pending_enrollment")state.storage.sql.exec("UPDATE accounts SET status=?,verifier=NULL,password_set_at=NULL WHERE account_id=?",condition,b.accountId);else state.storage.sql.exec("UPDATE accounts SET status=? WHERE account_id=?",condition,b.accountId);});
  expect((await transfer(f,b)).status,condition).toBe(409);expect((await rows(f)).game[0].host_id).toBe(f.host.accountId);
  expect((await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"new"},{session:f.host.session})).status).toBe(200);
 }
});
test("operator privileged audit actual actor once success failure rollback and fixed retention",async()=>{
 const f=await fixture(),a=await admin();const id=commandId();
 const response=await patch(f,{expected_revision:0,player_capacity:3},{id,session:a.session});expect(response.status).toBe(200);
 expect((await patch(f,{expected_revision:0,player_capacity:3},{id,session:a.session})).status).toBe(200);
 expect((await patch(f,{expected_revision:0,player_capacity:4},{session:a.session})).status).toBe(409);
 const discovery=await targets(f,a.session);expect(discovery.status).toBe(200);
 let audits=(await rows(f)).audits;expect(audits).toHaveLength(3);expect(audits.filter(a=>a.operation==="configuration"&&a.outcome==="succeeded")).toHaveLength(1);for(const event of audits){expect(event.actor_account_id).toBe(a.accountId);expect(event.target_game_id).toBe(f.id);expect(event.expires_at-event.occurred_at).toBe(7776000000);expect(Object.keys(event).sort()).toEqual(["actor_account_id","audit_id","expires_at","occurred_at","operation","outcome","target_game_id"]);}
 await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec("CREATE TRIGGER block_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END"));
 expect((await patch(f,{expected_revision:1,player_capacity:4},{session:a.session})).status).toBe(503);expect((await sync(f,a.session)).snapshot.configuration.player_capacity).toBe(3);
 await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec("DROP TRIGGER block_audit"));
 const cancelled=await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"new"},{session:f.host.session});expect(cancelled.status).toBe(200);expect((await rows(f)).audits).toHaveLength(3);
 expect((await post(`/api/games/${f.id}/resume`,{}, {session:a.session})).status).toBe(404);
});
test("operator durable Directory removal guards both hosts until matching commit proof settles",async()=>{
 const f=await fixture(),b=await enroll("OperatorTransferTarget");let captured;const denials=[];
 await runInDurableObject(directory(),instance=>{
  instance.operatorFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{
   const copy=await request.clone().json();const response=await instance.operatorFetch(request);
   if(new URL(request.url).pathname==="/host-transfer" && copy.action==="prepare" && (await response.clone().json()).result==="prepared") {
    captured=copy;
    // Inspect actual removals after durable preparation and before returning the
    // genuine grant. No cross-isolate promise is held by the runtime callback.
    for(const accountId of [f.host.accountId,b.accountId]){
     const denial=await instance.operatorFetch(new Request("https://peer.internal/removal",{method:"POST",body:JSON.stringify({action:"acquire",operation_id:commandId(),account_id:accountId})}));denials.push(await denial.json());
    }
   }
   return response;
  };
 });
 try{
  const response=await transfer(f,b);expect(response.status).toBe(200);expect(denials.map(x=>x.code)).toEqual(["hosted_game","hosted_game"]);expect((await rows(f)).game[0].host_id).toBe(b.accountId);
  const stale=await runInDurableObject(directory(),instance=>instance.operatorFetch(new Request("https://peer.internal/host-transfer",{method:"POST",body:JSON.stringify(captured)})));expect((await stale.json()).result).toBe("rejected");
 }finally{await runInDurableObject(directory(),instance=>{instance.fetch=instance.operatorFetch;delete instance.operatorFetch;});}
});

test("operator resume rejects terminal without recreating grants and transfer preserves in-progress boards sockets",async()=>{
 const f=await fixture(),b=await enroll("OperatorPlayingTarget");
 const lobby=await post(`/api/games/${f.id}/lobby`,{expected_revision:0},{session:f.host.session});expect(lobby.status).toBe(200);const code=(await lobby.json()).game_code;
 const sockets=[];
 try{
  for(const alias of ["OperatorPlayerA","OperatorPlayerB"]){
   const admission=await post(`/api/games/${f.id}/admission-context`,{game_code:code},{id:null});expect(admission.status).toBe(200);
   const joined=await post(`/api/games/${f.id}/players`,{game_code:code,alias},{session:admission.headers.get("Set-Cookie").split(";",1)[0]});expect(joined.status).toBe(200);const session=joined.headers.get("Set-Cookie").split(";",1)[0];
   const upgraded=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`,{headers:{Origin:origin,Upgrade:"websocket",Connection:"Upgrade",Cookie:session}});expect(upgraded.status).toBe(101);const socket=upgraded.webSocket;
   const initial=new Promise(resolve=>socket.addEventListener("message",event=>{const frame=JSON.parse(event.data);if(frame.kind==="snapshot"){socket.send(JSON.stringify({version:1,kind:"snapshot_ack",connection_id:frame.connection_id,delivery_id:frame.delivery_id,view_revision:frame.view_revision}));resolve(frame);}}));
   socket.accept();sockets.push(socket);await initial;
  }
  const beforeStart=await sync(f);expect((await post(`/api/games/${f.id}/start`,{expected_revision:beforeStart.view_revision},{session:f.host.session})).status).toBe(200);
  const before=await sync(f);expect(before.snapshot.players.every(p=>p.board)).toBe(true);
  expect((await transfer(f,b)).status).toBe(200);
  const resumed=await post(`/api/games/${f.id}/resume`,{}, {session:b.session});expect(resumed.status).toBe(200);const after=await resumed.json();expect(after.game.players.map(p=>p.board)).toEqual(before.snapshot.players.map(p=>p.board));expect(after.game.game_code).toBe(code);
  expect((await post(`/api/games/${f.id}/calls/manual`,{expected_revision:after.game.game.view_revision,value:"1"},{session:f.host.session})).status).toBe(403);
  expect((await post(`/api/games/${f.id}/calls/manual`,{expected_revision:after.game.game.view_revision,value:"1"},{session:b.session})).status).toBe(200);
  expect((await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"in_progress"},{session:b.session})).status).toBe(200);
  expect((await post(`/api/games/${f.id}/resume`,{}, {session:b.session})).status).toBe(409);
 }finally{const closed=sockets.map(socket=>socket.readyState===3?Promise.resolve():new Promise(resolve=>socket.addEventListener("close",resolve,{once:true})));for(const socket of sockets)if(socket.readyState!==3)socket.close(1000);await Promise.all(closed);}
});

for(const operation of ["configuration","resume","host-transfer"]){
 test(`operator receiving ${operation} expiry is durably denied before rollback reauthorization`,async()=>{
  const f=await fixture(),b=await enroll("OperatorCutTarget");const realNow=Date.now;
  const deadline=await inOwner((_,state)=>state.storage.sql.exec("SELECT expires_at FROM account_sessions WHERE account_id=? AND revoked_at IS NULL",f.host.accountId).one().expires_at);
  await runInDurableObject(owner(f.id),(instance,state)=>{
   instance.operatorCutFetch=instance.fetch.bind(instance);
   instance.fetch=async request=>{const input=await request.clone().json();const response=await instance.operatorCutFetch(request);if(new URL(request.url).pathname==="/command"&&input.operation.replaceAll("_","-")===operation){Date.now=()=>deadline;}return response;};
   state.storage.sql.exec("UPDATE game_record SET idle_due=?",deadline+10000);
  });
  let cut=false;
  await inOwner(instance=>{instance.operatorCutFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const input=await request.clone().json();if(new URL(request.url).pathname==="/operator-accounts" && input.action==="authorize" && Date.now()===deadline){cut=true;Date.now=()=>deadline-1;}return instance.operatorCutFetch(request);};});
  try{
   const response=operation==="configuration"?await patch(f,{expected_revision:0,player_capacity:3}):operation==="resume"?await post(`/api/games/${f.id}/resume`,{},{session:f.host.session}):await transfer(f,b);
   expect([401,503]).toContain(response.status);expect(await response.json()).not.toHaveProperty("receipt");expect(cut).toBe(false);
   const floor=await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);expect(floor).toBeGreaterThanOrEqual(deadline);
   Date.now=()=>deadline-1;expect((await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=account`,{headers:{Cookie:f.host.session}})).status).toBe(503);
  }finally{Date.now=realNow;await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.operatorCutFetch;delete instance.operatorCutFetch;});await inOwner(instance=>{instance.fetch=instance.operatorCutFetch;delete instance.operatorCutFetch;});}
 });
}

test("operator genuine same-actor earlier release ACK cannot disclose targets after actual host transfer", async () => {
 const a = await enroll("HeldOperatorHost"), b = await enroll("HeldOperatorTarget");
 const created = await post("/api/games", {}, {session:a.session}); expect(created.status).toBe(200); const id=(await created.json()).game.game_id;
 const transferId = commandId(), fixed = Date.now()+1000, realOperatorNow=Date.now;
 const read = () => exports.default.fetch(`${origin}/api/games/${id}/host-transfer-targets`, {headers:{Cookie:a.session}});
 await runInDurableObject(owner(id), instance => {
  instance.heldOriginalFetch=instance.fetch.bind(instance); instance.heldOriginalNow=Date.now; Date.now=()=>fixed;
  instance.heldReplay=false; instance.heldAck=null; instance.heldOwnerDenied=false;
  instance.fetch=async request => {
   const path=new URL(request.url).pathname; const input=await request.clone().json();
   const response=await instance.heldOriginalFetch(request);
   if(path==="/operator-release" && Array.isArray(input.response?.response?.targets)) {
    if(!instance.heldReplay) instance.heldAck=await response.clone().text();
    else { instance.heldOwnerDenied=(await response.clone().json()).rejection!==null; return new Response(instance.heldAck,{status:200,headers:{"Content-Type":"application/json"}}); }
   }
   if(instance.heldReplay && path==="/command" && input.operation==="host_transfer_targets") {
    const transferred=await post(`/api/games/${id}/host-transfer`,{target_account_id:b.accountId,confirmed:true,expected_host_assignment_revision:0},{session:a.session,id:transferId}); expect(transferred.status).toBe(200); await transferred.text();
   }
   return response;
  };
 });
 try {
  const healthy=await read(); expect(healthy.status).toBe(200); await healthy.text();
  await runInDurableObject(owner(id), instance=>{expect(JSON.parse(instance.heldAck).rejection).toBeNull(); instance.heldReplay=true;});
  const stale=await read(), body=await stale.json(); const denied=await runInDurableObject(owner(id),instance=>instance.heldOwnerDenied);

  expect(denied).toBe(true); expect(stale.status).toBe(503); expect(body.targets).toBeUndefined(); expect(stale.headers.has("Set-Cookie")).toBe(false);
 } finally {
  await runInDurableObject(owner(id),instance=>{instance.fetch=instance.heldOriginalFetch; delete instance.heldOriginalFetch; delete instance.heldOriginalNow; delete instance.heldReplay; delete instance.heldAck; delete instance.heldOwnerDenied;});
  Date.now=realOperatorNow;
 }
});

test("operator authenticated admin invalid transport is audited once without submitted content or anonymous attempts",async()=>{
 const f=await fixture(),a=await admin();
 expect((await patch(f,{expected_revision:0,unexpected:"OperatorPrivateContent"},{session:a.session})).status).toBe(400);
 expect((await post(`/api/games/${f.id}/resume`,{unexpected:"OperatorPrivateContent"},{session:a.session})).status).toBe(400);
 expect((await post(`/api/games/${f.id}/host-transfer`,{target_account_id:f.host.accountId,confirmed:false,expected_host_assignment_revision:0},{session:a.session})).status).toBe(400);
 expect((await targets(f,a.session,"?limit=0")).status).toBe(400);
 const audits=(await rows(f)).audits;expect(audits).toHaveLength(4);expect(new Set(audits.map(row=>row.operation))).toEqual(new Set(["configuration","resume","host_transfer","host_transfer_targets"]));
 expect(audits.every(row=>row.actor_account_id===a.accountId&&row.target_game_id===f.id&&row.outcome==="rejected")).toBe(true);expect(JSON.stringify(audits)).not.toContain("OperatorPrivateContent");
 expect((await patch(f,{expected_revision:0,unexpected:"OperatorPrivateContent"},{session:"__Host-brews_session=invalid"})).status).toBe(400);
 expect((await rows(f)).audits).toHaveLength(4);
});

test("operator genuine earlier Accounts reply is rejected at an identical command and clock",async()=>{
 const f=await fixture(),realNow=Date.now,fixed=realNow()+1,id=commandId();let captured;
 await inOwner(instance=>{instance.operatorReplayFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const input=await request.clone().json();const response=await instance.operatorReplayFetch(request);if(new URL(request.url).pathname==="/operator-accounts"&&input.action==="authorize"){
  if(captured)return new Response(captured,{status:200,headers:{"Content-Type":"application/json"}});captured=await response.clone().text();
 }return response;};});
 try{
  Date.now=()=>fixed;
  const denied=await post(`/api/games/${f.id}/resume`,{},{session:f.host.session,id});expect(denied.status).toBe(503);expect(await denied.json()).not.toHaveProperty("receipt");
 }finally{Date.now=realNow;await inOwner(instance=>{instance.fetch=instance.operatorReplayFetch;delete instance.operatorReplayFetch;});}
 // The accepted timer command remains exactly once; a healthy retry returns its
 // old receipt under fresh authorization rather than creating another snapshot.
 const before=(await rows(f)).game[0],healthy=await post(`/api/games/${f.id}/resume`,{},{session:f.host.session,id});expect(healthy.status).toBe(200);expect((await healthy.json()).result).toBe("committed");expect((await rows(f)).game[0]).toEqual(before);
});

for(const loss of ["before_settle","after_settle"]){
 test(`operator committed assignment and actor receipt survive ${loss} crash and exact recovery`,async()=>{
  const f=await fixture(),b=await enroll("OperatorCrashTarget"),id=commandId();
  await runInDurableObject(directory(),instance=>{instance.operatorCrashFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const input=await request.clone().json();if(new URL(request.url).pathname==="/host-transfer"&&input.action==="settle"){
   if(loss==="after_settle")await instance.operatorCrashFetch(request);return new Response(null,{status:503});
  }return instance.operatorCrashFetch(request);};});
  try{
   const accepted=await transfer(f,b,0,{id});expect(accepted.status).toBe(200);const receipt=(await accepted.json()).receipt;
   const pending=await rows(f);expect(pending.game[0].host_id).toBe(b.accountId);expect(pending.transfers).toHaveLength(1);expect(pending.transfers[0].phase).toBe("committed");
   await evictDurableObject(owner(f.id));
   const repeated=await transfer(f,b,0,{id});expect(repeated.status).toBe(200);expect(await repeated.json()).toEqual({result:"committed",receipt});expect((await rows(f)).game[0]).toEqual(pending.game[0]);
  }finally{await runInDurableObject(directory(),instance=>{instance.fetch=instance.operatorCrashFetch;delete instance.operatorCrashFetch;});}
  const realNow=Date.now,due=(await rows(f)).transfers[0].next_attempt_at;Date.now=()=>Math.max(realNow(),due);
  try{await evictDurableObject(directory());await runInDurableObject(owner(f.id),instance=>instance.alarm());
  expect((await rows(f)).transfers).toHaveLength(0);const directoryRows=await runInDurableObject(directory(),(_,state)=>({host:state.storage.sql.exec("SELECT designated_host_id FROM directory_hosted_nonterminal_games WHERE game_id=?",f.id).one().designated_host_id,reserved:state.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id}));expect(directoryRows).toEqual({host:b.accountId,reserved:f.id});}finally{Date.now=realNow;}
 });
}
