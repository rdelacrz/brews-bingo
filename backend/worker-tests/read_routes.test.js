import { beforeEach, expect, test } from "vitest";
import publishedSavedSchema from "../tests/fixtures/game-schema-published-operator.json";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject, evictDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId, password, inOwner } from "./fixtures.js";

test("dedicated remaining-values legal one-value all-free New configuration is complete",async()=>{
 const f=await fixture({lobby:false});
 const positions=[{row:1,column:1},{row:1,column:2},{row:2,column:1},{row:2,column:2}];
 const patched=await exports.default.fetch(`${origin}/api/games/${f.id}/configuration`,{method:"PATCH",headers:{Origin:origin,Cookie:f.host.session,"Content-Type":"application/json","Idempotency-Key":commandId()},body:JSON.stringify({expected_revision:0,numeric_upper_bound:1,player_capacity:2,free_cells_enabled:true,free_cell_positions:positions})});
 expect(patched.status).toBe(200);await patched.text();
 expect(await read(f,"/remaining-values")).toMatchObject({remaining_values:["1"],remaining_count:1,exhausted:false});
});

test("dedicated remaining-values complete 75 pool rejects invalid configuration bounds",async()=>{
 const f=await fixture({lobby:false,upper:75});
 const before=await read(f,"/remaining-values");expect(before.remaining_values).toEqual(Array.from({length:75},(_,i)=>String(i+1)));
 for(const n of [0,1001]){
  const patched=await exports.default.fetch(`${origin}/api/games/${f.id}/configuration`,{method:"PATCH",headers:{Origin:origin,Cookie:f.host.session,"Content-Type":"application/json","Idempotency-Key":commandId()},body:JSON.stringify({expected_revision:0,numeric_upper_bound:n})});
  expect(patched.status).toBe(400);await patched.text();
 }
 expect(await read(f,"/remaining-values")).toEqual(before);
});

test("dedicated calls mixed actual manual random retry duplicate and sparse numeric remainder",async()=>{
 const f=await fixture();await start(f);
 try{
  const command=commandId(),view=await read(f,"");const input={expected_revision:view.game.game.view_revision,value:"2"};
  const manual=await post(`/api/games/${f.id}/calls/manual`,input,{session:f.host.session,id:command});expect(manual.status).toBe(200);const accepted=await manual.json();
  const current=await read(f,"");const random=await post(`/api/games/${f.id}/calls/random`,{expected_revision:current.game.game.view_revision},{session:f.host.session});expect(random.status).toBe(200);const drawn=await random.json();expect(drawn.call.value).not.toBe("2");
  const retry=await post(`/api/games/${f.id}/calls/manual`,input,{session:f.host.session,id:command});expect(retry.status).toBe(200);expect(await retry.json()).toEqual({result:"committed",receipt:accepted.receipt});
  const latest=await read(f,"");const duplicate=await post(`/api/games/${f.id}/calls/manual`,{expected_revision:latest.game.game.view_revision,value:"2"},{session:f.host.session});expect(duplicate.status).toBe(409);await duplicate.text();
  const calls=await read(f,"/calls",f.players[0].session);expect(calls.calls).toEqual([accepted.call,drawn.call]);expect(calls.latest).toEqual(drawn.call);
  const remainder=Array.from({length:12},(_,i)=>String(i+1)).filter(v=>v!=="2"&&v!==drawn.call.value);
  expect((await read(f,"/remaining-values")).remaining_values).toEqual(remainder);
 }finally{await close(f);}
});

test("dedicated passive expiry preserves existing alarm and coordinates prestart release",async()=>{
 const f=await fixture({lobby:false});
 const before=await runInDurableObject(owner(f.id),async(_,s)=>({due:s.storage.sql.exec("SELECT idle_due FROM game_record").one().idle_due,alarm:await s.storage.getAlarm()}));
 Date.now=()=>before.due;
 await read(f,"",f.host.session,"",401);
 await runInDurableObject(owner(f.id),async(i,s)=>{
  expect(s.storage.sql.exec("SELECT state FROM game_record").one().state).toBe("cancelled");
  expect(s.storage.sql.exec("SELECT count(*) AS n FROM game_history").one().n).toBe(0);
  expect(s.storage.sql.exec("SELECT kind FROM game_pending_work").one().kind).toBe("release");
  expect(await s.storage.getAlarm()).toBe(before.alarm);
  await i.alarm();
 });
 expect(await runInDurableObject(directory(),(_,s)=>s.storage.sql.exec("SELECT game_id FROM directory_global_reservation").one().game_id)).toBeNull();
 await read(f,"",f.host.session,"",401);
});

test("dedicated original idle cap is not extended by actual host resume",async()=>{
 const f=await fixture({lobby:false});
 const due=await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT idle_due FROM game_record").one().idle_due);
 Date.now=()=>due-1000;
 const login=await post("/api/auth/login",{username:f.host.username,password:f.host.password});
 expect(login.status).toBe(200);f.host.session=cookie(login);await login.text();
 let changed=false;
 await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{
  const real=await i.readOriginalFetch(req);
  if(new URL(req.url).pathname==="/read"&&!changed){
   changed=true;Date.now=()=>due-1;
   const resumed=await post(`/api/games/${f.id}/resume`,{},{session:f.host.session});expect(resumed.status).toBe(200);await resumed.text();
   Date.now=()=>due;
  }
  return real;
 };});
 try{await read(f,"",f.host.session,"",401);expect(changed).toBe(true);
  expect(await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT idle_due FROM game_record").one().idle_due)).toBeGreaterThan(due);
 }finally{await restore(f);}
});

test("dedicated held calls rejects actual accepted call before release",async()=>{
 const f=await fixture();await start(f);let changed=false;
 await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{
  const real=await i.readOriginalFetch(req);
  if(new URL(req.url).pathname==="/read"&&!changed){changed=true;await call(f,"2");}
  return real;
 };});
 try{await read(f,"/calls",f.players[0].session,"",503);expect(changed).toBe(true);await restore(f);
  expect((await read(f,"/calls",f.players[0].session)).calls.map(c=>c.value)).toEqual(["2"]);
 }finally{await restore(f);await close(f);}
});

test("dedicated held account cut rejects actual host assignment transfer",async()=>{
 const f=await fixture(),target=await enroll("ReadTransferTarget");let changed=false;
 await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{
  const real=await i.readOriginalFetch(req);
  if(new URL(req.url).pathname==="/read"&&!changed){changed=true;
   const transfer=await post(`/api/games/${f.id}/host-transfer`,{target_account_id:target.accountId,confirmed:true,expected_host_assignment_revision:0},{session:f.host.session});
   expect(transfer.status).toBe(200);await transfer.text();
  }
  return real;
 };});
 try{await read(f,"",f.host.session,"",401);expect(changed).toBe(true);
  expect(await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT host_id FROM game_record").one().host_id)).toBe(target.accountId);
  await restore(f);await read(f,"",f.host.session);await read(f,"",target.session);
 }finally{await restore(f);}
});

test("dedicated held spectator loses access during actual terminal notice and cleanup",async()=>{
 const f=await fixture();const s=await spectator(f);await start(f);let changed=false;
 await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{
  const real=await i.readOriginalFetch(req);
  if(new URL(req.url).pathname==="/read"&&!changed){changed=true;await terminal(f);}
  return real;
 };});
 try{await read(f,"/calls",s.session,"",401);expect(changed).toBe(true);
  expect(await runInDurableObject(owner(f.id),(_,st)=>st.storage.sql.exec("SELECT count(*) AS n FROM game_spectators").one().n)).toBe(0);
 }finally{await restore(f);await close(f);}
});

test("dedicated held original terminal grant is denied at History expiry equality",async()=>{
 const f=await fixture();await start(f);await terminal(f);let changed=false,expiry;
 await runInDurableObject(owner(f.id),(i,s)=>{expiry=s.storage.sql.exec("SELECT expires_at FROM game_history").one().expires_at;i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{
  const real=await i.readOriginalFetch(req);
  if(new URL(req.url).pathname==="/read"&&!changed){changed=true;Date.now=()=>expiry;}
  return real;
 };});
 try{await read(f,"/result",f.players[0].session,"",401);expect(changed).toBe(true);
  expect(await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms)).toBeGreaterThanOrEqual(expiry);
 }finally{await restore(f);await close(f);}
});
const clock=Date.now;
const owner=id=>env.GAMES.get(env.GAMES.idFromName(id));
const directory=()=>env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async()=>{Date.now=clock;await reset();await resetStorage();await runInDurableObject(directory(),async(_,s)=>s.storage.deleteAll());});
async function fixture({lobby=true,upper=12}={}){
 const host=await enroll("ReadRoutesHost");const made=await post("/api/games",{configuration:{numeric_upper_bound:upper,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],player_capacity:3,spectator_capacity:3}},{session:host.session});expect(made.status).toBe(200);const f={host,id:(await made.json()).game.game_id,players:[],sockets:[]};
 if(lobby){const opened=await post(`/api/games/${f.id}/lobby`,{expected_revision:0},{session:host.session});expect(opened.status).toBe(200);f.code=(await opened.json()).game_code;for(const alias of ["ReadOne","ReadTwo"]){const context=await post(`/api/games/${f.id}/admission-context`,{game_code:f.code},{id:null});const answer=password();const r=await post(`/api/games/${f.id}/players`,{game_code:f.code,alias,recovery_answer:answer},{session:cookie(context)});expect(r.status).toBe(200);f.players.push({session:cookie(r),answer,body:await r.json()});}}
 return f;
}
async function spectator(f){const c=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{id:null});expect(c.status).toBe(200);const r=await post(`/api/games/${f.id}/spectators`,{game_code:f.code},{session:cookie(c)});expect(r.status).toBe(200);return {session:cookie(r),body:await r.json()};}
const get=(f,path="",session=f.host.session,query="")=>exports.default.fetch(`${origin}/api/games/${f.id}${path}${query}`,{headers:session?{Cookie:session}:{}});
async function read(f,path,session,query="",status=200){const r=await get(f,path,session,query);expect(r.status,path+query).toBe(status);expect(r.headers.get("Set-Cookie")).toBeNull();expect(r.headers.get("Cache-Control")).toBe("no-store");return r.json();}
async function rows(f){return runInDurableObject(owner(f.id),(_,s)=>({record:s.storage.sql.exec("SELECT * FROM game_record").toArray(),grants:s.storage.sql.exec("SELECT * FROM game_connection_grants ORDER BY grant_id").toArray(),revisions:s.storage.sql.exec("SELECT * FROM game_view_revisions ORDER BY view_key").toArray(),sessions:s.storage.sql.exec("SELECT * FROM game_sessions ORDER BY session_id").toArray(),audits:s.storage.sql.exec("SELECT * FROM game_admin_audit ORDER BY audit_id").toArray()}));}
async function socket(f,session,view="player"){const r=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=${view}`,{headers:{Origin:origin,Cookie:session,Upgrade:"websocket",Connection:"Upgrade"}});expect(r.status).toBe(101);const ws=r.webSocket;const first=new Promise(resolve=>ws.addEventListener("message",e=>{const frame=JSON.parse(e.data);if(frame.kind==="snapshot"){ws.send(JSON.stringify({version:1,kind:"snapshot_ack",connection_id:frame.connection_id,delivery_id:frame.delivery_id,view_revision:frame.view_revision}));resolve(frame);}}));ws.accept();f.sockets.push(ws);await first;return ws;}
async function close(f){const original=Date.now;const floor=await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);Date.now=()=>Math.max(floor,clock());const waits=f.sockets.map(w=>w.readyState===3?Promise.resolve():new Promise(resolve=>w.addEventListener("close",resolve,{once:true})));for(const w of f.sockets)if(w.readyState!==3)w.close(1000);await Promise.all(waits);Date.now=original;}
async function start(f){for(const p of f.players)await socket(f,p.session);const v=await read(f,"",f.host.session);expect((await post(`/api/games/${f.id}/start`,{expected_revision:v.game.game.view_revision},{session:f.host.session})).status).toBe(200);}
async function terminal(f,resolved=false){if(resolved){for(let n=1;n<=f.upper;n++)await call(f,String(n));const q=await read(f,"/qualification");expect(q.qualified_players.length).toBeGreaterThan(0);const v=await read(f,"");expect((await post(`/api/games/${f.id}/winner`,{player_id:q.qualified_players[0].player_id,expected_revision:v.game.game.view_revision},{session:f.host.session})).status).toBe(200);}else expect((await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"in_progress"},{session:f.host.session})).status).toBe(200);}
async function call(f,value){const v=await read(f,"");const r=await post(`/api/games/${f.id}/calls/manual`,{expected_revision:v.game.game.view_revision,value},{session:f.host.session});expect(r.status).toBe(200);return r.json();}
async function recover(f){const p=f.players[0];const r=await post("/api/player-recovery",{game_code:f.code,alias:p.body.alias,recovery_answer:p.answer});expect(r.status).toBe(200);return cookie(r);}
async function admin(){const a=await enroll("ReadRoutesAdmin");await inOwner((_,s)=>s.storage.sql.exec("UPDATE accounts SET role=? WHERE account_id=?","admin",a.accountId));return a;}
async function restore(f){await runInDurableObject(owner(f.id),i=>{if(i.readOriginalFetch)i.fetch=i.readOriginalFetch;delete i.readOriginalFetch;});}
const routes=["","/membership","/players","/calls","/remaining-values","/membership/board","/boards","/qualification","/result"];
test("dedicated all nine routes reject methods command headers wrong Origin bearer and duplicate cookie",async()=>{
 const f=await fixture();
 for(const path of routes){
  const session=path==="/membership"||path==="/membership/board"?f.players[0].session:f.host.session;
  for(const [method,headers,status] of [
   ["PUT",{Cookie:session},405],
   ["GET",{Cookie:session,"Idempotency-Key":commandId()},400],
   ["GET",{Cookie:session,Origin:"https://invalid.example"},403],
   ["GET",{Cookie:session,Authorization:"Bearer invalid"},403],
   ["GET",{Cookie:`${session}; ${session}`},400],
  ]){
   const response=await exports.default.fetch(`${origin}/api/games/${f.id}${path}`,{method,headers});
   expect(response.status,path+method).toBe(status);expect(response.headers.get("Set-Cookie")).toBeNull();await response.text();
  }
 }
});
for(const path of routes){test(`dedicated read route ${path||"C5"} exists prestart with its own projection`,async()=>{const f=await fixture();const participant=path==="/membership"||path==="/membership/board";const data=await read(f,path,participant?f.players[0].session:f.host.session,"",path==="/result"?409:200);if(path==="/membership")expect(data.membership).toMatchObject({role:"player",player_id:f.players[0].body.player_id,recovery_enabled:true});if(path==="/membership/board")expect(data.board).toBeNull();if(path==="/calls")expect(data).toMatchObject({calls:[],latest:null});if(path==="/remaining-values")expect(data.remaining_values).toEqual(Array.from({length:12},(_,i)=>String(i+1)));expect(JSON.stringify(data)).not.toMatch(/token_verifier|session_epoch|recovery_answer|command_id|called_by/);});}
test("dedicated reads New cross-host all six permitted inspections create no grant revision timer session",async()=>{const f=await fixture({lobby:false});const b=await enroll("OtherReadHost");const before=await rows(f);for(const path of ["","/players","/calls","/remaining-values","/boards","/qualification"])await read(f,path,b.session);expect(await rows(f)).toEqual(before);expect(before.grants.length).toBe(1);const sync=await get(f,"/sync",b.session,"?view=account");expect(sync.status).toBe(200);expect((await rows(f)).grants.length).toBe(2);});
test("dedicated reads closed grammar ambiguous cookie contexts and explicit invalid never fallback",async()=>{const f=await fixture(),s=await spectator(f);const all=`${f.host.session}; ${f.players[0].session}; ${s.session}`;for(const p of ["","/calls","/qualification","/result","/membership"])await read(f,p,all,"",400);for(const p of ["/players","/remaining-values","/boards"])await read(f,p,all);await read(f,"/membership/board",all);await read(f,"/qualification",all,"?view=spectator",403);await read(f,"/membership",all,"?view=spectator");for(const q of ["?view=player&view=player","?view=%70layer","?known_revision=0","?role=host","?"])await read(f,"/calls",all,q,400);for(const q of ["?player_id=bad","?player_id="+commandId().toUpperCase(),"?player_id="+commandId()+"&x=1"])await read(f,"/boards",f.host.session,q,400);await read(f,"/boards",f.host.session,"?player_id="+commandId(),404);await read(f,"/membership/board",f.players[0].session,"?player_id="+f.players[1].body.player_id,400);await read(f,"",`${f.host.session}; __Secure-brews-player=invalid`,"?view=player",401);await read(f,"/membership/board",`${f.host.session}; __Secure-brews-player=invalid`,"",401);});
test("dedicated reads spectator own membership omits recovery and has no private qualification boards roster",async()=>{const f=await fixture(),s=await spectator(f);const v=await read(f,"/membership",`${f.host.session}; ${s.session}`);expect(v.membership.role).toBe("spectator");expect(v.membership.recovery_enabled).toBeUndefined();for(const path of ["/players","/remaining-values","/boards","/membership/board"])await read(f,path,s.session,"",401);await read(f,"/qualification",s.session,"",403);await read(f,"/calls",s.session);await read(f,"",s.session);});
test("dedicated reads actual presence ignores prepared slots and survives hibernation",async()=>{const f=await fixture();expect((await read(f,"/players")).players.every(p=>!p.connected)).toBe(true);await socket(f,f.players[0].session);try{expect((await read(f,"/players")).players.filter(p=>p.connected).map(p=>p.player_id)).toEqual([f.players[0].body.player_id]);await evictDurableObject(owner(f.id));expect((await read(f,"/players")).players.filter(p=>p.connected).map(p=>p.player_id)).toEqual([f.players[0].body.player_id]);}finally{await close(f);}});
test("dedicated reads gameplay complete calls numeric order selected boards qualifiers exhaustion and retained Leave",async()=>{const f=await fixture({upper:4});await start(f);try{const initial=await read(f,"/membership/board",f.players[0].session);for(let n=1;n<=4;n++)await call(f,String(n));const calls=await read(f,"/calls",f.players[0].session);expect(calls.latest).toEqual(calls.calls.at(-1));expect(calls.calls.map(c=>c.value)).toEqual(["1","2","3","4"]);expect(await read(f,"/remaining-values")).toMatchObject({remaining_values:[],remaining_count:0,exhausted:true});expect((await read(f,"")).game.game.state).toBe("in_progress");const after=await read(f,"/membership/board",f.players[0].session);expect(after.board.cells.map(c=>c.kind)).toEqual(initial.board.cells.map(c=>c.kind));expect(after.board.qualified).toBe(true);const q=await read(f,"/qualification");expect(q.qualified_players).toHaveLength(2);expect((await read(f,"/boards",f.host.session,"?player_id="+f.players[0].body.player_id)).boards).toHaveLength(1);expect((await post(`/api/games/${f.id}/membership/leave`,{expected_state:"in_progress"},{session:f.players[0].session})).status).toBe(200);const roster=await read(f,"/players");expect(roster.players.filter(p=>p.left_during_play)).toHaveLength(1);expect((await read(f,"/qualification")).qualified_players).toHaveLength(2);}finally{await close(f);}});
for(const resolved of [false,true])test(`dedicated reads terminal ${resolved?"Resolved":"Cancelled"} original grants and truthful roster`,async()=>{const f=await fixture({upper:4});f.upper=4;const outsider=await enroll("ReadFinalOutsider");await read(f,"",outsider.session);const s=await spectator(f);await start(f);try{await terminal(f,resolved);const result=await read(f,"/result",f.players[0].session);expect(result.result.role).toBe("final_player");expect(result.result.state).toBe(resolved?"resolved":"cancelled");const roster=await read(f,"/players");expect(roster.players.every(p=>p.left_during_play===null&&p.membership_retained&&p.connected)).toBe(true);expect((await read(f,"/membership",f.players[0].session)).membership).toMatchObject({access:"final_only",recovery_enabled:false});await read(f,"/remaining-values",f.host.session,"",409);for(const p of ["","/calls","/membership/board","/qualification","/result"])await read(f,p,f.players[0].session);await read(f,"/result",outsider.session,"",401);await read(f,"/result",s.session,"",401);expect((await post(`/api/games/${f.id}/exit`,{},{session:f.players[0].session})).status).toBe(200);await read(f,"/result",f.players[0].session,"",401);expect((await post(`/api/games/${f.id}/exit`,{},{session:f.host.session})).status).toBe(200);await read(f,"/result",f.host.session,"",401);expect((await exports.default.fetch(`${origin}/api/history/${f.id}`,{headers:{Cookie:f.host.session}})).status).toBe(200);const tables=await runInDurableObject(owner(f.id),(_,st)=>st.storage.sql.exec("SELECT name FROM sqlite_master WHERE name LIKE 'game_history_read_%'").toArray());expect(tables).toEqual([]);}finally{await close(f);}});
for(const mutation of ["recovery","delete-recovery","rename","leave","switch"]){test(`dedicated held membership rejects stale private payload after actual ${mutation}`,async()=>{const f=await fixture();let changed=false,status;
 await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const path=new URL(req.url).pathname;const real=await i.readOriginalFetch(req);if(path==="/read"&&!changed){changed=true;if(mutation==="recovery"){await recover(f);status=200;}else if(mutation==="rename"){const r=await exports.default.fetch(`${origin}/api/games/${f.id}/membership/alias`,{method:"PATCH",headers:{Origin:origin,Cookie:f.players[0].session,"Idempotency-Key":commandId(),"Content-Type":"application/json"},body:JSON.stringify({alias:"FreshReadAlias"})});status=r.status;await r.text();}else if(mutation==="delete-recovery"){const r=await exports.default.fetch(`${origin}/api/games/${f.id}/membership/recovery-answer`,{method:"DELETE",headers:{Origin:origin,Cookie:f.players[0].session,"Idempotency-Key":commandId(),"Content-Type":"application/json"},body:"{}"});status=r.status;await r.text();}else{const r=await post(`/api/games/${f.id}/membership/${mutation==="switch"?"spectator":"leave"}`,mutation==="switch"?{}:{expected_state:"awaiting_players"},{session:f.players[0].session});status=r.status;await r.text();}}return real;};});
 try{const r=await get(f,"/membership",f.players[0].session);const data=await r.json();expect(changed).toBe(true);expect(status).toBe(200);expect(r.status).toBe(mutation==="delete-recovery"||mutation==="rename"?503:401);expect(data.membership).toBeUndefined();expect(r.headers.get("Set-Cookie")).toBeNull();if(mutation==="delete-recovery")expect((await read(f,"/membership",f.players[0].session)).membership.recovery_enabled).toBe(false);}finally{await restore(f);}
});}
for(const mutation of ["disable","reset","delete","epoch","role"]){test(`dedicated final Accounts recut denies actual ${mutation}`,async()=>{const f=await fixture();let releases=0,changed=false;await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{if(new URL(req.url).pathname==="/read-release")releases++;return i.readOriginalFetch(req);};});await inOwner((i,s)=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const input=await req.clone().json();if(new URL(req.url).pathname==="/operator-accounts"&&input.action==="authorize"&&releases===2&&!changed){changed=true;if(mutation==="disable")s.storage.sql.exec("UPDATE accounts SET disabled_at=? WHERE account_id=?",Date.now(),f.host.accountId);else if(mutation==="reset")s.storage.sql.exec("UPDATE accounts SET status='reset_required' WHERE account_id=?",f.host.accountId);else if(mutation==="role")s.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?",f.host.accountId);else if(mutation==="epoch")s.storage.sql.exec("UPDATE accounts SET credential_epoch=credential_epoch+1 WHERE account_id=?",f.host.accountId);else{s.storage.sql.exec("DELETE FROM account_sessions WHERE account_id=?",f.host.accountId);s.storage.sql.exec("DELETE FROM accounts WHERE account_id=?",f.host.accountId);}}return i.readOriginalFetch(req);};});try{const r=await get(f);expect(changed).toBe(true);expect(r.status).toBe(401);expect((await r.json()).game).toBeUndefined();}finally{await restore(f);await inOwner(i=>{i.fetch=i.readOriginalFetch;delete i.readOriginalFetch;});}});}
for(const path of ["","/players","/calls","/remaining-values","/boards","/qualification","/result"]){test(`dedicated Admin ${path||"game"} typed audit success or rejection once`,async()=>{const f=await fixture(),a=await admin();await read(f,path,a.session,"",path==="/result"?409:200);const audits=(await rows(f)).audits;expect(audits).toHaveLength(1);const purpose=({"":"game","/remaining-values":"remaining_values"})[path]??path.slice(1);expect(audits[0]).toMatchObject({actor_account_id:a.accountId,target_game_id:f.id,operation:purpose+"_read",outcome:path==="/result"?"rejected":"succeeded"});expect(audits[0].expires_at-audits[0].occurred_at).toBe(7776000000);});}
test("dedicated audit entropy or ignored write cannot release an unaudited Admin read",async()=>{const f=await fixture(),a=await admin();await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("CREATE TRIGGER block_read_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END"));await read(f,"/boards",a.session,"",503);expect((await rows(f)).audits).toHaveLength(0);});
for(const route of ["/read","/read-release"]){test(`dedicated genuine old ${route} ACK cannot authorize later recovery invocation`,async()=>{const f=await fixture();let saved,substitute=false,mutated=false;await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const path=new URL(req.url).pathname;const response=await i.readOriginalFetch(req);if(path===route){if(!substitute)saved=await response.clone().text();else if(!mutated){mutated=true;await recover(f);return new Response(saved,{headers:{"Content-Type":"application/json"}});}}return response;};});try{await read(f,"/calls",f.players[0].session);expect(saved).toBeDefined();substitute=true;await read(f,"/calls",f.players[0].session,"",503);expect(mutated).toBe(true);}finally{await restore(f);}});}
test("dedicated negative ACK from different invocation is unavailable without false cookie clearing",async()=>{const f=await fixture();let saved;await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const real=await i.readOriginalFetch(req);if(new URL(req.url).pathname==="/read"){if(!saved)saved=await real.clone().text();else return new Response(saved,{headers:{"Content-Type":"application/json"}});}return real;};});try{await read(f,"/calls","__Secure-brews-player=invalid","",401);await read(f,"/calls","__Secure-brews-player=invalid","",503);}finally{await restore(f);}});
for(const path of ["","/calls","/qualification","/result","/boards","/remaining-values","/players"]){test(`dedicated malformed authenticated Admin ${path||"game"} typed rejection`,async()=>{const f=await fixture(),a=await admin();await read(f,path,a.session,"?unknown=1",400);const audits=(await rows(f)).audits;expect(audits).toHaveLength(1);expect(audits[0]).toMatchObject({outcome:"rejected",actor_account_id:a.accountId,target_game_id:f.id});});}
for(const mutation of ["Exit","expiry"]){test(`dedicated held final result denied after actual ${mutation}`,async()=>{const f=await fixture({upper:4});await start(f);try{await terminal(f);let changed=false,status;await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const real=await i.readOriginalFetch(req);if(new URL(req.url).pathname==="/read"&&!changed){changed=true;if(mutation==="Exit"){const r=await post(`/api/games/${f.id}/exit`,{},{session:f.players[0].session});status=r.status;await r.text();}else{const expiry=await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT history_expires_at FROM game_record").one().history_expires_at);Date.now=()=>expiry;status=200;}}return real;};});try{const r=await get(f,"/result",f.players[0].session);expect(changed).toBe(true);expect(status).toBe(200);expect(r.status).toBe(401);expect((await r.json()).result).toBeUndefined();if(mutation==="expiry"){const floor=await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);Date.now=clock;expect(floor).toBeGreaterThan(clock());await read(f,"/result",f.players[0].session,"",503);}}finally{await restore(f);Date.now=clock;}}finally{await close(f);}});}
for(const fault of ["wrong-challenge","wrong-digest","unknown","truncated","unsafe-clock","missing-code"]){test(`dedicated release ${fault} is unavailable`,async()=>{const f=await fixture();let touched=false;await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const real=await i.readOriginalFetch(req);if(new URL(req.url).pathname==="/read-release"){touched=true;const body=await real.json();if(fault==="wrong-challenge")body.challenge[0]^=1;else if(fault==="wrong-digest")body.request_digest[0]^=1;else if(fault==="unknown")body.untrusted=true;else if(fault==="unsafe-clock")body.checked_at=9007199254740992;else if(fault==="missing-code")delete body.code;return new Response(fault==="truncated"?"{":JSON.stringify(body),{headers:{"Content-Type":"application/json"}});}return real;};});try{const r=await get(f,"/calls",f.players[0].session);expect(touched).toBe(true);expect(r.status).toBe(503);expect((await r.json()).calls).toBeUndefined();}finally{await restore(f);}});}
test("dedicated edge deadline durably fenced before entropy failure and rollback denial",async()=>{const f=await fixture();let touched=false,observed;const random=crypto.getRandomValues;await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const real=await i.readOriginalFetch(req);if(new URL(req.url).pathname==="/read"&&!touched){touched=true;const reply=await real.clone().json();observed=reply.binding.valid_until;Date.now=()=>observed;crypto.getRandomValues=()=>{throw new Error("controlled entropy failure");};}return real;};});try{await read(f,"/calls",f.players[0].session,"",401);const floor=await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);expect(floor).toBe(observed);Date.now=clock;crypto.getRandomValues=random;await read(f,"/calls",f.players[0].session,"",503);}finally{Date.now=clock;crypto.getRandomValues=random;await restore(f);}});
test("dedicated same-clock actual challenges differ and entropy failure has no fallback",async()=>{const f=await fixture();const t=clock();Date.now=()=>t;const challenges=[];await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{if(new URL(req.url).pathname==="/read")challenges.push((await req.clone().json()).challenge);return i.readOriginalFetch(req);};});try{await read(f,"/calls",f.players[0].session);await read(f,"/calls",f.players[0].session);expect(challenges).toHaveLength(2);expect(challenges[0]).not.toEqual(challenges[1]);const saved=crypto.getRandomValues;crypto.getRandomValues=()=>{throw new Error("controlled entropy failure");};try{await read(f,"/calls",f.players[0].session,"",503);}finally{crypto.getRandomValues=saved;}}finally{Date.now=clock;await restore(f);}});
test("dedicated current shorter spectator grace cap enforced after receiving await",async()=>{
 const f=await fixture(),spectatorViewer=await spectator(f);await socket(f,spectatorViewer.session,"spectator");let changed=false,shorter,grace;
 await runInDurableObject(owner(f.id),(i,state)=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{
  const path=new URL(req.url).pathname;
  if(path==="/read-release"&&!changed){changed=true;const accepted=state.getWebSockets()[0];accepted.close(1000,"controlled current disconnect");await i.webSocketClose(accepted,1000,"controlled current disconnect",true);grace=state.storage.sql.exec("SELECT grace_expires_at FROM game_spectators").one().grace_expires_at;}
  const real=await i.readOriginalFetch(req);
  if(path==="/read-release"&&real.status===200){const ack=await real.clone().json();shorter=ack.valid_until;Date.now=()=>shorter;}
  return real;
 };});
 try{const r=await get(f,"/calls",spectatorViewer.session);expect(changed).toBe(true);expect(shorter).toBe(grace);expect(shorter-clock()).toBeLessThanOrEqual(300000);expect(shorter).toBeGreaterThan(clock());expect(r.status).toBe(401);expect((await r.json()).calls).toBeUndefined();const floor=await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata").one().last_observed_ms);expect(floor).toBe(shorter);}
 finally{Date.now=clock;await restore(f);await close(f);}
});
test("dedicated Admin socket registry failure is withheld and durably Failed",async()=>{const f=await fixture(),a=await admin();await runInDurableObject(owner(f.id),(i,s)=>{i.readSockets=s.getWebSockets.bind(s);s.getWebSockets=()=>{throw new Error("controlled socket registry failure");};});try{await read(f,"/boards",a.session,"",503);const audits=(await rows(f)).audits;expect(audits).toHaveLength(1);expect(audits[0]).toMatchObject({operation:"boards_read",outcome:"failed",actor_account_id:a.accountId});}finally{await runInDurableObject(owner(f.id),(i,s)=>{s.getWebSockets=i.readSockets;delete i.readSockets;});}});
for(const path of ["/read-release"]){test(`dedicated held ${path} after actual recovery cannot release old payload`,async()=>{const f=await fixture();let mutated=false;await runInDurableObject(owner(f.id),i=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const real=await i.readOriginalFetch(req);if(new URL(req.url).pathname===path&&!mutated){mutated=true;await recover(f);}return real;};});try{await read(f,"/membership/board",f.players[0].session,"",401);expect(mutated).toBe(true);}finally{await restore(f);}});}
test("dedicated maximum legal 20-member roster and full 1000 pool remain complete and bounded",async()=>{const f=await fixture({lobby:false,upper:1000});const p=await exports.default.fetch(`${origin}/api/games/${f.id}/configuration`,{method:"PATCH",headers:{Origin:origin,Cookie:f.host.session,"Content-Type":"application/json","Idempotency-Key":commandId()},body:JSON.stringify({expected_revision:0,player_capacity:20})});expect(p.status).toBe(200);const opened=await post(`/api/games/${f.id}/lobby`,{expected_revision:(await p.json()).receipt.outcome.view_revision},{session:f.host.session});expect(opened.status).toBe(200);f.code=(await opened.json()).game_code;for(let n=0;n<20;n++){const c=await post(`/api/games/${f.id}/admission-context`,{game_code:f.code},{id:null});const r=await post(`/api/games/${f.id}/players`,{game_code:f.code,alias:`ReadPlayer${String(n).padStart(2,"0")}01234567`},{session:cookie(c)});expect(r.status).toBe(200);}const roster=await read(f,"/players");expect(roster.players).toHaveLength(20);expect(roster.players.every(p=>!p.connected&&p.left_during_play===false)).toBe(true);const boards=await read(f,"/boards");expect(boards.boards).toHaveLength(20);expect(boards.boards.every(p=>p.board===null)).toBe(true);const pool=await read(f,"/remaining-values");expect(pool.remaining_values).toHaveLength(1000);expect(pool.remaining_values[1]).toBe("2");expect(pool.remaining_values[9]).toBe("10");expect(JSON.stringify(boards).length).toBeLessThan(256*1024);});
for(const fault of ["lost","wrong-binding","ignored-write"]){test(`dedicated observed expiry ${fault} ACK is unavailable without clear-cookie`,async()=>{const f=await fixture();let observed,touched=false;await runInDurableObject(owner(f.id),(i,s)=>{i.readOriginalFetch=i.fetch.bind(i);i.fetch=async req=>{const path=new URL(req.url).pathname;if(path==="/read-expiry-denial"){touched=true;if(fault==="ignored-write")s.storage.sql.exec("CREATE TRIGGER block_expiry_read BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END");}const real=await i.readOriginalFetch(req);if(path==="/read"){observed=(await real.clone().json()).binding.valid_until;Date.now=()=>observed;}if(path==="/read-expiry-denial"){if(fault==="lost")return new Response(null,{status:503});if(fault==="wrong-binding"){const ack=await real.json();ack.request_digest[0]^=1;return Response.json(ack);}}return real;};});try{await read(f,"/calls",f.players[0].session,"",503);expect(touched).toBe(true);}finally{Date.now=clock;await restore(f);}});}
for(const fault of ["none","unknown-shape","index-drift","ignored-marker"]){test(`dedicated frozen published counter5 Workerd migration ${fault}`,async()=>{const host=await enroll("ReadMigrationHost"),f={host,id:commandId()};let originalAudit;
 await runInDurableObject(owner(f.id),(_,s)=>{for(const sql of publishedSavedSchema)s.storage.sql.exec(sql);s.storage.sql.exec("INSERT INTO game_metadata VALUES(1,5,0,0)");const now=Date.now();s.storage.sql.exec("INSERT INTO game_admin_audit VALUES(?,?,?,?,?,?,?)",commandId(),host.accountId,f.id,"resume","succeeded",now,now+7776000000);originalAudit=s.storage.sql.exec("SELECT * FROM game_admin_audit").toArray();if(fault==="unknown-shape")s.storage.sql.exec("ALTER TABLE game_terminal_views ADD COLUMN unknown_shape TEXT");else if(fault==="index-drift")s.storage.sql.exec("DROP INDEX game_admin_audit_expiry");else if(fault==="ignored-marker")s.storage.sql.exec("CREATE TRIGGER ignore_read_migration_marker BEFORE UPDATE OF schema_version ON game_metadata BEGIN SELECT RAISE(IGNORE); END");});
 await read(f,"",host.session,"",fault==="none"?404:503);
 const after=await runInDurableObject(owner(f.id),(_,s)=>({counter:s.storage.sql.exec("SELECT schema_version FROM game_metadata").one().schema_version,audit:s.storage.sql.exec("SELECT * FROM game_admin_audit").toArray(),unexpected:s.storage.sql.exec("SELECT name FROM sqlite_master WHERE name LIKE 'game_history_read_%'").toArray(),ddl:s.storage.sql.exec("SELECT sql FROM sqlite_master WHERE type='table' AND name='game_admin_audit'").one().sql}));expect(after.counter).toBe(fault==="none"?6:5);expect(after.audit).toEqual(originalAudit);expect(after.unexpected).toEqual([]);expect(after.ddl.includes("remaining_values_read")).toBe(fault==="none");
});}
