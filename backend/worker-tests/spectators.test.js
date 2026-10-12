import {beforeEach,expect,test} from "vitest";
import {env,exports} from "cloudflare:workers";
import {reset,runInDurableObject,evictDurableObject} from "cloudflare:test";
import {enroll,post,resetStorage,origin,cookie,commandId,password} from "./fixtures.js";
const nativeNow=Date.now;
const nativeRandom=crypto.getRandomValues;
const owner=id=>env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async()=>{await reset();await resetStorage();await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),async(_,state)=>state.storage.deleteAll());});
async function fixture(capacity=3){
 const host=await enroll("SpectatorHost");
 const created=await post("/api/games",{configuration:{numeric_upper_bound:4,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],player_capacity:3,spectator_capacity:capacity}},{session:host.session});expect(created.status).toBe(200);
 const id=(await created.json()).game.game_id;const opened=await post(`/api/games/${id}/lobby`,{expected_revision:0},{session:host.session});expect(opened.status).toBe(200);return {host,id,code:(await opened.json()).game_code};
}
async function spectator(f){const context=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{id:null});expect(context.status).toBe(200);const contextCookie=cookie(context);const input={game_code:f.code};const command=commandId();const joined=await post(`/api/games/${f.id}/spectators`,input,{session:contextCookie,id:command});expect(joined.status).toBe(200);return {session:cookie(joined),context:contextCookie,command,input,body:await joined.json()};}
async function player(f,alias){const context=await post(`/api/games/${f.id}/admission-context`,{game_code:f.code},{id:null});expect(context.status).toBe(200);const answer=password();const joined=await post(`/api/games/${f.id}/players`,{game_code:f.code,alias,recovery_answer:answer},{session:cookie(context)});expect(joined.status).toBe(200);return{session:cookie(joined),answer,body:await joined.json()};}
const get=(f,session,query="view=spectator")=>exports.default.fetch(`${origin}/api/games/${f.id}/sync${query?`?${query}`:""}`,{headers:session?{Cookie:session}:{}});
async function view(f,session,role="spectator"){const r=await get(f,session,`view=${role}`);expect(r.status).toBe(200);return r.json();}
async function rows(f){return runInDurableObject(owner(f.id),(_,state)=>({spectators:[...state.storage.sql.exec("SELECT * FROM game_spectators")],sessions:[...state.storage.sql.exec("SELECT session_id,spectator_id,session_epoch,issued_at,expires_at,length(token_verifier) AS digest_bytes FROM game_spectator_sessions")],players:[...state.storage.sql.exec("SELECT * FROM game_players")],proofs:[...state.storage.sql.exec("SELECT player_id FROM game_recovery")],connections:[...state.storage.sql.exec("SELECT * FROM game_current_connections")],revisions:[...state.storage.sql.exec("SELECT * FROM game_view_revisions ORDER BY view_key")],notices:[...state.storage.sql.exec("SELECT connection_id,expires_at FROM game_spectator_notices")]}));}
async function stream(f,session,role="spectator",ack=true){const r=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=${role}`,{headers:{Cookie:session,Origin:origin,Upgrade:"websocket",Connection:"Upgrade"}});expect(r.status).toBe(101);const ws=r.webSocket;const queue=[],readers=[];ws.addEventListener("message",event=>{const frame=JSON.parse(event.data);if(ack&&frame.kind==="snapshot")ws.send(JSON.stringify({version:1,kind:"snapshot_ack",connection_id:frame.connection_id,delivery_id:frame.delivery_id,view_revision:frame.view_revision}));if(readers.length)readers.shift()(frame);else queue.push(frame);});const next=()=>queue.length?Promise.resolve(queue.shift()):new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(new Error("spectator frame timeout")),3000);readers.push(frame=>{clearTimeout(timeout);resolve(frame);});});ws.accept();return{ws,next,initial:await next()};}
async function started(f){const a=await player(f,"PrivateOne"),b=await player(f,"PrivateTwo");const sockets=[await stream(f,a.session,"player"),await stream(f,b.session,"player")];const current=await view(f,f.host.session,"account");const r=await post(`/api/games/${f.id}/start`,{expected_revision:current.view_revision},{session:f.host.session});expect(r.status).toBe(200);return{players:[a,b],sockets};}
async function clock(f,time){await runInDurableObject(owner(f.id),instance=>{instance.spectatorRealNow??=Date.now;Date.now=()=>time;});}
async function restoreClock(f){Date.now=nativeNow;await runInDurableObject(owner(f.id),instance=>{Date.now=nativeNow;delete instance.spectatorRealNow;});}
function expectClearedSpectator(response,f){expect(response.headers.get("Set-Cookie")).toBe(`__Secure-brews-spectator=; Secure; HttpOnly; Path=/api/games/${f.id}; SameSite=Strict; Max-Age=0`);}

for(const capacity of [0,1,50])test(`spectator capacity ${capacity} is enforced separately with no double seat`,async()=>{const f=await fixture(capacity);for(let n=0;n<capacity;n++)await spectator(f);const context=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{id:null});const full=await post(`/api/games/${f.id}/spectators`,{game_code:f.code},{session:cookie(context)});expect(full.status).toBe(409);const state=await rows(f);expect(state.spectators).toHaveLength(capacity);expect(state.players).toHaveLength(0);});
test("spectator admission retries require actor context and never replay cookies",async()=>{const f=await fixture();const s=await spectator(f);for(const selected of [s.context,`${s.context}; ${s.session}`]){const retry=await post(`/api/games/${f.id}/spectators`,s.input,{session:selected,id:s.command});expect(retry.status).toBe(200);expect(retry.headers.has("Set-Cookie")).toBe(false);expect(await retry.json()).toEqual({result:"committed",receipt:s.body.receipt});}expect((await post(`/api/games/${f.id}/spectators`,s.input,{session:f.host.session,id:s.command})).status).toBe(401);expect((await rows(f)).spectators).toHaveLength(1);const response=await get(f,s.session);expect(response.status).toBe(200);expect(response.headers.has("Set-Cookie")).toBe(false);expect((await rows(f)).sessions[0].digest_bytes).toBe(32);});
test("spectator code context mismatches and terminal admission fail closed",async()=>{const f=await fixture();const context=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{id:null});expect((await post(`/api/games/${f.id}/spectators`,{game_code:"ZZZZZZZZ"},{session:cookie(context)})).status).toBe(409);await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"awaiting_players"},{session:f.host.session});expect((await post(`/api/games/${f.id}/spectators`,{game_code:f.code},{session:cookie(context)})).status).toBe(404);expect((await rows(f)).spectators).toHaveLength(0);});
test("spectator audience projection has no roster boards proofs or privileged commands",async()=>{const f=await fixture();const p=await player(f,"PRIVATE_ALIAS");const s=await spectator(f);const before=await view(f,s.session);expect(before.snapshot.role).toBe("spectator");for(const key of ["players","board","player_id","alias","qualifiers","recovery_answer"])expect(before.snapshot[key]).toBeUndefined();expect(JSON.stringify(before)).not.toContain("PRIVATE_ALIAS");const rename=await exports.default.fetch(`${origin}/api/games/${f.id}/membership/alias`,{method:"PATCH",headers:{Origin:origin,Cookie:p.session,"Idempotency-Key":commandId(),"Content-Type":"application/json"},body:JSON.stringify({alias:"PRIVATE_RENAMED"})});expect(rename.status).toBe(200);expect((await view(f,s.session)).view_revision).toBe(before.view_revision);const unchanged=await get(f,s.session,`view=spectator&known_revision=${before.view_revision}`);expect(await unchanged.json()).toEqual({up_to_date:true,view_revision:before.view_revision,snapshot:null});expect((await post(`/api/games/${f.id}/start`,{expected_revision:before.view_revision},{session:s.session})).status).toBe(401);});
test("coexisting cookies require explicit sync selector and Leave selects only participant",async()=>{const f=await fixture();const s=await spectator(f),p=await player(f,"CookiePlayer");const mixed=`${f.host.session}; ${s.session}; ${p.session}`;expect((await get(f,mixed,"")).status).toBe(400);for(const role of ["account","player","spectator"])expect((await get(f,mixed,`view=${role}`)).status).toBe(200);expect((await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:mixed})).status).toBe(400);const left=await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:`${f.host.session}; ${s.session}`});expect(left.status).toBe(200);expect(left.headers.get("Set-Cookie")).toContain("__Secure-brews-spectator=;");expect(left.headers.get("Set-Cookie")).not.toContain("__Host-brews_session");expect((await get(f,s.session)).status).toBe(401);});
test("spectators alone never satisfy the Start connected player minimum",async()=>{const f=await fixture();const s=await spectator(f);const socket=await stream(f,s.session);const host=await view(f,f.host.session,"account");expect(host.snapshot.connected_player_count).toBe(0);expect(host.snapshot.connected_spectator_count).toBe(1);expect((await post(`/api/games/${f.id}/start`,{expected_revision:host.view_revision},{session:f.host.session})).status).toBe(409);socket.ws.close(1000);});
test("running spectator admission sees ordered public calls and latest exhaustion",async()=>{const f=await fixture();const running=await started(f);const s=await spectator(f);const socket=await stream(f,s.session);expect(socket.initial.view.state).toBe("in_progress");for(let n=1;n<=4;n++){const host=await view(f,f.host.session,"account");expect((await post(`/api/games/${f.id}/calls/manual`,{value:String(n),expected_revision:host.view_revision},{session:f.host.session})).status).toBe(200);const current=await view(f,s.session);expect(current.view_revision).toBe(socket.initial.view_revision+n);expect(current.snapshot.latest_call).toEqual({sequence_no:n,value:String(n)});expect(current.snapshot.exhausted).toBe(n===4);expect(current.snapshot.board).toBeUndefined();}expect((await post(`/api/games/${f.id}/membership/player`,{game_code:f.code,alias:"TooLate"},{session:s.session})).status).toBe(409);socket.ws.close(1000);for(const c of running.sockets)c.ws.close(1000);});
test("role switches retire sockets delete proof preserve absolute expiry and release exact alias",async()=>{const f=await fixture();const p=await player(f,"SwitchAlias");const old=await stream(f,p.session,"player");const r=await post(`/api/games/${f.id}/membership/spectator`,{},{session:p.session});expect(r.status).toBe(200);const switched=await r.json();expect(switched.receipt.outcome.change).toBe("switched");expect(switched.receipt.outcome.session_expires_at).toBe(p.body.session_expires_at);expect(r.headers.get("Set-Cookie")).toContain("__Secure-brews-player=;");const s=cookie(r);expect((await get(f,p.session,"view=player")).status).toBe(401);expect((await rows(f)).proofs).toHaveLength(0);const audience=await stream(f,s);const promoted=await post(`/api/games/${f.id}/membership/player`,{game_code:f.code,alias:"SwitchAlias",recovery_answer:password()},{session:s});expect(promoted.status).toBe(200);const outcome=(await promoted.json()).receipt.outcome;expect(outcome.identity.player_id).not.toBe(p.body.player_id);expect(outcome.session_expires_at).toBe(p.body.session_expires_at);expect((await rows(f)).proofs).toHaveLength(1);expect((await get(f,s)).status).toBe(401);expect((await view(f,cookie(promoted),"player")).snapshot.board).toBeUndefined();old.ws.close(1000);audience.ws.close(1000);});
for(const target of ["spectator","player"])test(`target ${target} capacity/alias rejection preserves original role`,async()=>{const f=await fixture(1);const p=await player(f,"Taken"),s=await spectator(f);const before=await rows(f);const response=await post(`/api/games/${f.id}/membership/${target}`,target==="player"?{game_code:f.code,alias:"Taken"}:{},{session:target==="player"?s.session:p.session});expect(response.status).toBe(409);expect(await rows(f)).toEqual(before);});
for(const fault of ["insert","delete","receipt"])test(`role switch controlled SQLite ${fault} fault rolls back`,async()=>{const f=await fixture();const p=await player(f,"FaultPlayer");const before=await rows(f);const statement={insert:"BEFORE INSERT ON game_spectators",delete:"BEFORE DELETE ON game_sessions",receipt:"BEFORE INSERT ON game_receipts"}[fault];await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec(`CREATE TRIGGER spectator_fault ${statement} BEGIN SELECT RAISE(IGNORE); END`));const response=await post(`/api/games/${f.id}/membership/spectator`,{},{session:p.session});expect(response.status).toBe(503);expect(response.headers.has("Set-Cookie")).toBe(false);expect(await rows(f)).toEqual(before);expect((await get(f,p.session,"view=player")).status).toBe(200);});
for(const offset of [-1,0,1])test(`never-connected spectator grace exclusive boundary ${offset}`,async()=>{const f=await fixture();const s=await spectator(f);const deadline=(await rows(f)).spectators[0].grace_expires_at;try{await clock(f,deadline+offset);const response=await get(f,s.session);expect(response.status).toBe(offset<0?200:401);expect((await rows(f)).spectators).toHaveLength(offset<0?1:0);if(offset>=0){await clock(f,deadline-1);expect((await get(f,s.session)).status).toBe(503);expect((await rows(f)).sessions).toHaveLength(0);}}finally{await restoreClock(f);}});
test("spectator replacement survives hibernation and stale predecessor callback",async()=>{const f=await fixture();const s=await spectator(f);const first=await stream(f,s.session);const replacement=await stream(f,s.session);await evictDurableObject(owner(f.id));expect((await view(f,s.session)).snapshot.spectator_id).toBe(replacement.initial.view.spectator_id);const current=await rows(f);expect(current.connections.filter(c=>c.viewer_kind==="spectator")[0].connection_id).toBe(replacement.initial.connection_id);expect(current.spectators[0].disconnected_at).toBeNull();first.ws.close(1000);replacement.ws.close(1000);});
for(const startedGame of [false,true])test(`terminal spectator notice is best effort and deletes all authority: started ${startedGame}`,async()=>{const f=await fixture();const running=startedGame?await started(f):null;const s=await spectator(f);const audience=await stream(f,s.session,"spectator",false);const command=commandId();const ended=await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:startedGame?"in_progress":"awaiting_players"},{session:f.host.session,id:command});expect(ended.status).toBe(200);let notice;for(let n=0;n<5;n++){const frame=await audience.next();if(frame.kind==="terminal_notice"){notice=frame;break;}}expect(notice.state).toBe("cancelled");expect(notice.objective).toBe("single_line");expect(notice.board).toBeUndefined();expect(notice.players).toBeUndefined();expect(notice.history_expires_at).toBeUndefined();const state=await rows(f);expect(state.spectators).toHaveLength(0);expect(state.sessions).toHaveLength(0);expect(state.notices).toHaveLength(0);expect((await get(f,s.session)).status).toBe(startedGame?401:404);expect((await post(`/api/games/${f.id}/spectators`,s.input,{session:s.context,id:s.command})).status).toBe(startedGame?409:404);expect(await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),(_,state)=>[...state.storage.sql.exec("SELECT game_id FROM directory_global_reservation WHERE game_id IS NOT NULL")])).toHaveLength(0);if(running)for(const c of running.sockets)c.ws.close(1000);audience.ws.close(1000);});
test("closed switch/admission JSON mandatory command UUID and account cannot prove anonymous membership",async()=>{const f=await fixture();const s=await spectator(f);for(const [path,body]of [["spectators",{game_code:f.code,spectator_id:commandId()}],["membership/player",{game_code:f.code,alias:"Okay",role:"player"}],["membership/spectator",{capabilities:[]}],["membership/leave",{expected_state:"resolved"}]]){const r=await post(`/api/games/${f.id}/${path}`,body,{session:s.session});expect(r.status).toBe(400);}expect((await post(`/api/games/${f.id}/membership/player`,{game_code:f.code,alias:"Okay"},{session:s.session,id:null})).status).toBe(400);expect((await get(f,f.host.session,"view=spectator")).status).toBe(401);});

test("valid current spectator admission without context is a secret-free reconnect",async()=>{
 const f=await fixture(1),s=await spectator(f);const before=await rows(f);
 const context=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{session:`${s.session}; ${s.context}`,id:null});expect(context.status).toBe(200);expect(context.headers.has("Set-Cookie")).toBe(false);
 const command=commandId();for(let n=0;n<2;n++){const r=await post(`/api/games/${f.id}/spectators`,s.input,{session:s.session,id:command});expect(r.status).toBe(200);expect(r.headers.has("Set-Cookie")).toBe(false);const body=await r.json();expect(body.result).toBe("committed");expect(body.receipt.outcome.change).toBe("reconnected");expect(body.receipt.outcome.identity).toEqual(s.body.receipt.outcome.identity);expect(body.receipt.outcome.session_expires_at).toBe(s.body.receipt.outcome.session_expires_at);}
 const after=await rows(f);expect(after.spectators).toEqual(before.spectators);expect(after.sessions).toEqual(before.sessions);expect(after.revisions).toEqual(before.revisions);
});
for(const offset of [-1,0,1])test(`actual disconnected spectator WSS reconnect exclusive grace ${offset}`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session);const original=(await rows(f)).sessions[0];
 await runInDurableObject(owner(f.id),(instance,state)=>{instance.spectatorClosedWs=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===socket.initial.connection_id);});
 await new Promise(resolve=>{socket.ws.addEventListener("close",resolve,{once:true});socket.ws.close(1000);});
 await view(f,f.host.session,"account");
 const disconnected=(await rows(f)).spectators[0];expect(disconnected.grace_expires_at-disconnected.disconnected_at).toBe(300000);
 await runInDurableObject(owner(f.id),async instance=>{await instance.webSocketError(instance.spectatorClosedWs,new Error("test-only duplicate callback"));});
 expect((await rows(f)).spectators[0]).toEqual(disconnected);
 try{await clock(f,disconnected.grace_expires_at+offset);
 const r=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=spectator`,{headers:{Cookie:s.session,Origin:origin,Upgrade:"websocket",Connection:"Upgrade"}});
 expect(r.status).toBe(offset<0?101:401);
 if(offset<0){r.webSocket.accept();const after=await rows(f);expect(after.sessions[0]).toEqual(original);expect(after.spectators[0].grace_expires_at).toBeNull();await runInDurableObject(owner(f.id),instance=>instance.alarm());expect((await rows(f)).spectators).toHaveLength(1);r.webSocket.close(1000);}
 else{expect((await rows(f)).sessions).toHaveLength(0);await clock(f,disconnected.grace_expires_at-1);expect((await get(f,s.session)).status).toBe(503);}
 }finally{await restoreClock(f);socket.ws.close(1000);}
});
test("all fifty audience sockets reconstruct actual presence after hibernation",async()=>{
 const f=await fixture(50),sockets=[];
 try{for(let n=0;n<50;n++)sockets.push(await stream(f,(await spectator(f)).session));await evictDurableObject(owner(f.id));const host=await view(f,f.host.session,"account");expect(host.snapshot.spectator_count).toBe(50);expect(host.snapshot.connected_spectator_count).toBe(50);expect(host.snapshot.connected_player_count).toBe(0);const state=await rows(f);expect(state.connections.filter(c=>c.viewer_kind==="spectator")).toHaveLength(50);expect(state.spectators.every(s=>s.grace_expires_at===null&&s.disconnected_at===null)).toBe(true);}
 finally{for(const socket of sockets)socket.ws.close(1000);}
},180000);
for(const cut of ["sync","release"])test(`spectator ${cut} receiving deadline denial durably fences rollback`,async()=>{
 const f=await fixture(),s=await spectator(f);let deadline;
 await runInDurableObject(owner(f.id),instance=>{instance.spectatorFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const path=new URL(request.url).pathname,input=await request.clone().json();if(input.expiry)Date.now=()=>deadline-1;const r=await instance.spectatorFetch(request);if(path==="/command"&&input.operation==="sync"&&input.view==="spectator"){const b=await r.clone().json();if(b.owner_result==="audience_synced"){deadline=b.authority.valid_until;if(cut==="sync")Date.now=()=>deadline;}}else if(path==="/audience-release"&&cut==="release")Date.now=()=>deadline;return r;};});
 try{const r=await get(f,s.session);expect([401,503]).toContain(r.status);expect(r.headers.has("Set-Cookie")).toBe(false);const floor=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata")][0].last_observed_ms);expect(floor).toBeGreaterThanOrEqual(deadline);expect((await rows(f)).spectators).toHaveLength(0);}
 finally{await restoreClock(f);await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.spectatorFetch;delete instance.spectatorFetch;});}
});
for(const change of ["leave","switch","terminal"])test(`delayed spectator sync cannot disclose original output after ${change}`,async()=>{
 const f=await fixture(),s=await spectator(f);await runInDurableObject(owner(f.id),instance=>{instance.spectatorFetch=instance.fetch.bind(instance);let changed=false;instance.fetch=async request=>{const input=await request.clone().json(),r=await instance.spectatorFetch(request);if(!changed&&input.operation==="sync"&&input.view==="spectator"){changed=true;const body=change==="terminal"?{confirmed:true,expected_state:"awaiting_players"}:change==="leave"?{expected_state:"awaiting_players"}:{game_code:f.code,alias:"NewPlayer"};const path=change==="terminal"?"cancel":change==="leave"?"membership/leave":"membership/player";const mutation=await post(`/api/games/${f.id}/${path}`,body,{session:change==="terminal"?f.host.session:s.session});expect(mutation.status).toBe(200);await mutation.text();}return r;};});
 try{const r=await get(f,s.session);expect([401,404,503]).toContain(r.status);expect(Object.keys(await r.json())).toEqual(["error"]);expect(r.headers.has("Set-Cookie")).toBe(false);}
 finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.spectatorFetch;delete instance.spectatorFetch;});}
});
for(const fault of ["send","close","both"])test(`terminal spectator ${fault} provider failure cannot retain seat or hold reservation`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session);
 await runInDurableObject(owner(f.id),(_,state)=>{const ws=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===socket.initial.connection_id);if(fault!=="close"){ws.spectatorSend=ws.send;ws.send=()=>{throw new Error("test-only spectator send fault");};}if(fault!=="send"){ws.spectatorClose=ws.close;ws.close=()=>{throw new Error("test-only spectator close fault");};}});
 try{const ended=await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"awaiting_players"},{session:f.host.session});expect(ended.status).toBe(200);const after=await rows(f);expect(after.spectators).toHaveLength(0);expect(after.sessions).toHaveLength(0);expect(after.notices).toHaveLength(0);expect(after.connections.filter(c=>c.viewer_kind==="spectator")).toHaveLength(0);expect((await exports.default.fetch(`${origin}/api/games/current`,{headers:{Cookie:f.host.session}})).status).toBe(200);expect(await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),(_,state)=>[...state.storage.sql.exec("SELECT game_id FROM directory_global_reservation WHERE game_id IS NOT NULL")])).toHaveLength(0);}
 finally{await runInDurableObject(owner(f.id),(_,state)=>{for(const ws of state.getWebSockets()){if(ws.spectatorSend)ws.send=ws.spectatorSend;if(ws.spectatorClose)ws.close=ws.spectatorClose;}});socket.ws.close(1000);}
});
test("deleted switch and Leave source credentials cannot replay receipts",async()=>{
 const f=await fixture(),p=await player(f,"RetrySource"),cmd=commandId();const switched=await post(`/api/games/${f.id}/membership/spectator`,{},{session:p.session,id:cmd});expect(switched.status).toBe(200);const s=cookie(switched);const lost=await post(`/api/games/${f.id}/membership/spectator`,{},{session:p.session,id:cmd});expect(lost.status).toBe(401);expect(lost.headers.has("Set-Cookie")).toBe(false);const leave=commandId();expect((await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:s,id:leave})).status).toBe(200);const retry=await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:s,id:leave});expect(retry.status).toBe(401);expect(retry.headers.has("Set-Cookie")).toBe(false);expect((await rows(f)).spectators).toHaveLength(0);await spectator(f);
});
for(const fault of ["marker","ddl"])test(`actual SQLite spectator migration ${fault} corruption fails closed without publishing marker`,async()=>{
 const f=await fixture(),p=await player(f,"MigrationSurvivor");await runInDurableObject(owner(f.id),(_,state)=>{for(const table of ["game_admin_audit","game_host_transfers","game_operator_identity","game_spectator_terminal_work","game_spectator_notices","game_spectator_sessions","game_spectator_contexts","game_spectators"])state.storage.sql.exec(`DROP TABLE ${table}`);state.storage.sql.exec("UPDATE game_metadata SET schema_version=3");if(fault==="marker")state.storage.sql.exec("CREATE TRIGGER spectator_marker BEFORE UPDATE OF schema_version ON game_metadata BEGIN SELECT RAISE(IGNORE); END");else state.storage.sql.exec("ALTER TABLE game_players ADD COLUMN unexpected TEXT");});
 await evictDurableObject(owner(f.id));expect((await get(f,p.session,"view=player")).status).toBe(503);const meta=await runInDurableObject(owner(f.id),(_,state)=>({version:[...state.storage.sql.exec("SELECT schema_version FROM game_metadata")][0].schema_version,players:[...state.storage.sql.exec("SELECT alias FROM game_players")],spectator:[...state.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'game_spectator%'")]}));expect(meta.version).toBe(3);expect(meta.players).toEqual([{alias:"MigrationSurvivor"}]);expect(meta.spectator).toHaveLength(0);
});

test("concurrent Start and player role switch serialize without deleting a started board",async()=>{
 const f=await fixture(),a=await player(f,"RaceOne"),b=await player(f,"RaceTwo"),s=await spectator(f);const sockets=[await stream(f,a.session,"player"),await stream(f,b.session,"player"),await stream(f,s.session)];
 try{const host=await view(f,f.host.session,"account");const [start,change]=await Promise.all([post(`/api/games/${f.id}/start`,{expected_revision:host.view_revision},{session:f.host.session}),post(`/api/games/${f.id}/membership/spectator`,{},{session:a.session})]);const state=await view(f,f.host.session,"account");
 if(state.snapshot.state==="in_progress"){expect(start.status).toBe(200);expect(change.status).toBe(409);expect(state.snapshot.players).toHaveLength(2);expect(state.snapshot.players.every(p=>p.board)).toBe(true);expect((await get(f,a.session,"view=player")).status).toBe(200);}
 else{expect(start.status).toBe(409);expect(change.status).toBe(200);expect(state.snapshot.players).toHaveLength(1);expect((await rows(f)).spectators).toHaveLength(2);}
 }finally{for(const socket of sockets)socket.ws.close(1000);}
});
test("spectator availability releases expired grace occupancy and fences rollback",async()=>{
 const f=await fixture(1),s=await spectator(f),deadline=(await rows(f)).spectators[0].grace_expires_at;
 try{await clock(f,deadline);const entry=await exports.default.fetch(`${origin}/api/game-codes/${f.code}`);expect(entry.status).toBe(200);expect((await entry.json()).can_join_spectator).toBe(true);expect((await rows(f)).spectators).toHaveLength(0);await clock(f,deadline-1);expect((await get(f,s.session)).status).toBe(503);}
 finally{await restoreClock(f);}
});
for(const event of ["ack","frame","attachment"])test(`stale spectator ${event} cannot reauthorize a switched socket`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session,"spectator",false);
 await runInDurableObject(owner(f.id),(instance,state)=>{const ws=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===socket.initial.connection_id);instance.spectatorStaleSocket=ws;instance.spectatorOldAttachment=ws.deserializeAttachment();ws.spectatorClose=ws.close;ws.close=()=>{throw new Error("test-only retained predecessor");};});
 const change=await post(`/api/games/${f.id}/membership/player`,{game_code:f.code,alias:"AfterAudience"},{session:s.session});expect(change.status).toBe(200);const changedPlayerId=(await change.clone().json()).receipt.outcome.identity.player_id;
 try{await runInDurableObject(owner(f.id),async(instance,state)=>{const ws=instance.spectatorStaleSocket;if(event==="attachment"){const hint=JSON.parse(ws.deserializeAttachment());hint.viewer={kind:"player",id:changedPlayerId};ws.serializeAttachment(JSON.stringify(hint));}const message=event==="frame"?JSON.stringify({version:1,kind:"arbitrary_mutation",value:"1"}):JSON.stringify({version:1,kind:"snapshot_ack",connection_id:socket.initial.connection_id,delivery_id:socket.initial.delivery_id,view_revision:socket.initial.view_revision});try{await instance.webSocketMessage(ws,message);}catch(error){expect(String(error)).toContain("game transport unavailable");}expect([...state.storage.sql.exec("SELECT * FROM game_current_connections WHERE viewer_kind='spectator'")]).toHaveLength(0);});if(event==="attachment"){const uncertain=await get(f,s.session);expect(uncertain.status).toBe(503);expect(uncertain.headers.has("Set-Cookie")).toBe(false);expect(Object.keys(await uncertain.json())).toEqual(["error"]);await runInDurableObject(owner(f.id),instance=>{const ws=instance.spectatorStaleSocket;ws.serializeAttachment(instance.spectatorOldAttachment);ws.close=ws.spectatorClose;ws.close(1000);});}expect((await get(f,s.session)).status).toBe(401);expect((await get(f,cookie(change),"view=player")).status).toBe(200);}
 finally{await runInDurableObject(owner(f.id),instance=>{const ws=instance.spectatorStaleSocket;ws.close=ws.spectatorClose;ws.close(1000);delete instance.spectatorStaleSocket;delete instance.spectatorOldAttachment;});socket.ws.close(1000);}
});

test("failed spectator replacement acceptance preserves predecessor and seat",async()=>{
 const f=await fixture(),s=await spectator(f),old=await stream(f,s.session);
 await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec("CREATE TRIGGER audience_accept BEFORE INSERT ON game_current_connections WHEN NEW.viewer_kind='spectator' BEGIN SELECT RAISE(IGNORE); END"));
 const failed=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=spectator`,{headers:{Cookie:s.session,Origin:origin,Upgrade:"websocket",Connection:"Upgrade"}});expect(failed.status).toBe(503);
 const current=await rows(f);expect(current.connections.filter(c=>c.viewer_kind==="spectator")[0].connection_id).toBe(old.initial.connection_id);expect(current.spectators[0].disconnected_at).toBeNull();expect((await get(f,s.session)).status).toBe(200);
 await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec("DROP TRIGGER audience_accept"));const fresh=await stream(f,s.session);expect(fresh.initial.view.spectator_id).toBe(old.initial.view.spectator_id);old.ws.close(1000);fresh.ws.close(1000);
});
test("original absolute spectator session expiry removes connected authority despite WSS presence",async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session);const expiry=s.body.receipt.outcome.session_expires_at;
 try{await clock(f,expiry);const r=await get(f,s.session);expect([401,404]).toContain(r.status);expect((await rows(f)).sessions).toHaveLength(0);expect((await rows(f)).spectators).toHaveLength(0);await clock(f,expiry-1);expect((await get(f,s.session)).status).toBe(503);}
 finally{await restoreClock(f);socket.ws.close(1000);}
});

// These callbacks intercept the provider's actual first terminal send, before cleanup.
async function observeTerminalSend(f, socket, fault=false) {
 await runInDurableObject(owner(f.id),(instance,state)=>{
  instance.spectatorTerminalObservation=null;
  const ws=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===socket.initial.connection_id);
  ws.spectatorSend=ws.send;
  ws.send=message=>{
   const frame=JSON.parse(message);
   if(frame.kind==="terminal_notice") {
    instance.spectatorTerminalObservation={
     state:[...state.storage.sql.exec("SELECT state FROM game_record")][0].state,
     identities:[...state.storage.sql.exec("SELECT spectator_id FROM game_spectators")].length,
     sessions:[...state.storage.sql.exec("SELECT session_id FROM game_spectator_sessions")].length,
     revisions:[...state.storage.sql.exec("SELECT view_key FROM game_view_revisions WHERE view_key=?",socket.initial.view.spectator_id)].length,
     contexts:[...state.storage.sql.exec("SELECT spectator_id FROM game_spectator_contexts")].length,
     current:[...state.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE viewer_kind='spectator'")].length,
     attempt:[...state.storage.sql.exec("SELECT attempted FROM game_spectator_terminal_work")][0].attempted,
     calls:frame.calls.length,
     release:[...state.storage.sql.exec("SELECT kind FROM game_pending_work WHERE kind='release'")].length,
    };
    if(fault)throw new Error("test-only terminal send fault after observation");
   }
   return ws.spectatorSend.call(ws,message);
  };
 });
}
async function assertPurged(f,s){
 const after=await rows(f);expect(after.spectators).toHaveLength(0);expect(after.sessions).toHaveLength(0);expect(after.notices).toHaveLength(0);
 expect(after.revisions.some(r=>r.view_key===s.body.receipt.outcome.identity.spectator_id)).toBe(false);
 const more=await runInDurableObject(owner(f.id),(_,state)=>({contexts:[...state.storage.sql.exec("SELECT spectator_id FROM game_spectator_contexts")],work:[...state.storage.sql.exec("SELECT * FROM game_spectator_terminal_work")]}));expect(more.contexts).toHaveLength(0);expect(more.work).toHaveLength(0);
 expect([401,404]).toContain((await get(f,s.session)).status);
 expect([401,404]).toContain((await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=spectator`,{headers:{Cookie:s.session,Origin:origin,Upgrade:"websocket",Connection:"Upgrade"}})).status);
 expect(await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),(_,state)=>[...state.storage.sql.exec("SELECT game_id FROM directory_global_reservation WHERE game_id IS NOT NULL")])).toHaveLength(0);
}
for(const ending of ["operator","idle","cancel","winner"])test(`actual terminal send precedes physical spectator deletion: ${ending}`,async()=>{
 const f=await fixture(),running=["cancel","winner"].includes(ending)?await started(f):null;
 if(ending==="winner")for(let n=1;n<=4;n++){const host=await view(f,f.host.session,"account");expect((await post(`/api/games/${f.id}/calls/manual`,{value:String(n),expected_revision:host.view_revision},{session:f.host.session})).status).toBe(200);}
 const s=await spectator(f),socket=await stream(f,s.session,"spectator",false);await observeTerminalSend(f,socket);
 try{
  if(ending==="idle"){
   // A genuine idle deadline near now keeps the audience's fixed session live.
   await runInDurableObject(owner(f.id),async(instance,state)=>{const due=Date.now()+1000;state.storage.sql.exec("UPDATE game_record SET last_host_activity=?,idle_due=?",due-86400000,due);Date.now=()=>due;await instance.alarm();});
  }else if(ending==="winner"){
   const host=await view(f,f.host.session,"account");const r=await post(`/api/games/${f.id}/winner`,{player_id:running.players[0].body.player_id,expected_revision:host.view_revision},{session:f.host.session});expect(r.status).toBe(200);
  }else{expect((await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:ending==="cancel"?"in_progress":"awaiting_players"},{session:f.host.session})).status).toBe(200);}
  const observed=await runInDurableObject(owner(f.id),instance=>instance.spectatorTerminalObservation);
  expect(observed).toEqual({state:ending==="winner"?"resolved":"cancelled",identities:1,sessions:1,revisions:1,contexts:1,current:0,attempt:1,calls:ending==="winner"?4:0,release:1});
  await assertPurged(f,s);
  if(running){const final=await view(f,running.players[0].session,"player");expect(final.snapshot.role).toBe("final_player");}
 }finally{await restoreClock(f);socket.ws.close(1000);if(running)for(const c of running.sockets)c.ws.close(1000);}
});

for (const state of ["awaiting_players", "in_progress"]) {
 for (const entry of ["context", "join"]) {
  test(`stale spectator browser cookie clears on ${entry} denial and permits fresh ${state} admission`, async () => {
   const f = await fixture(1), running = state === "in_progress" ? await started(f) : null;
   const s = await spectator(f), deadline = (await rows(f)).spectators[0].grace_expires_at;
   expect(s.body.receipt.outcome.session_expires_at).toBeGreaterThan(deadline);
   const jar = new Map([f.host.session, s.context, s.session].map(pair => [pair.split("=", 1)[0], pair]));
   const selected = () => [...jar.values()].join("; ");
   const apply = response => {
    const raw = response.headers.get("Set-Cookie");
    if (!raw) return;
    const pair = raw.split(";", 1)[0], name = pair.split("=", 1)[0];
    if (raw.includes("Max-Age=0")) jar.delete(name); else jar.set(name, pair);
   };
   try {
    await clock(f, deadline);
    const denied = await post(`/api/games/${f.id}/${entry === "context" ? "spectator-admission-context" : "spectators"}`, {game_code: f.code}, {session: selected(), id: entry === "context" ? null : commandId()});
    expect(denied.status).toBe(401); expectClearedSpectator(denied, f);
    expect(Object.keys(await denied.json())).toEqual(["error"]);
    expect((await rows(f)).spectators).toHaveLength(0); apply(denied);
    expect(jar.get(f.host.session.split("=", 1)[0])).toBe(f.host.session);
    const context = await post(`/api/games/${f.id}/spectator-admission-context`, {game_code: f.code}, {session: selected(), id: null});
    expect(context.status).toBe(200); expect(context.headers.get("Set-Cookie")).toMatch(/^__Secure-brews-audience-admission=/); apply(context);
    const joined = await post(`/api/games/${f.id}/spectators`, {game_code: f.code}, {session: selected()});
    expect(joined.status).toBe(200); apply(joined);
    const result = await joined.json();
    expect(result.receipt.outcome.identity.spectator_id).not.toBe(s.body.receipt.outcome.identity.spectator_id);
    expect((await rows(f)).spectators).toHaveLength(1);
    expect((await view(f, selected())).snapshot.role).toBe("spectator");
    const old = await get(f, `${f.host.session}; ${s.session}`);
    expect(old.status).toBe(401); expect(old.headers.has("Set-Cookie")).toBe(false);
   } finally {
    if (running) for (const socket of running.sockets) socket.ws.close(1000);
    await restoreClock(f);
   }
  });
 }
}

test("spectator denial cookie clearing is limited to selected entry proof and exact game path", async () => {
 const f = await fixture(), s = await spectator(f), base = `/api/games/${f.id}`;
 const offset = s.session.indexOf("=") + 1;
 const bad = s.session.slice(0, offset) + (s.session[offset] === "A" ? "B" : "A") + s.session.slice(offset + 1);
 const invalid = await exports.default.fetch(`${origin}${base}/spectator-admission-context`, {method: "POST", headers: {Origin: "https://other.example.test", Cookie: `${f.host.session}; ${bad}; ${s.context}`, "Content-Type": "application/json"}, body: JSON.stringify({game_code: f.code})});
 expect(invalid.status).toBe(403); expect(invalid.headers.has("Set-Cookie")).toBe(false);
 const malformed = await post(`${base}/spectator-admission-context`, {game_code: f.code, spectator_id: commandId()}, {session: bad, id: null});
 expect(malformed.status).toBe(400); expect(malformed.headers.has("Set-Cookie")).toBe(false);
 const wrongCode = (f.code[0] === "Z" ? "Y" : "Z") + f.code.slice(1);
 const conflict = await post(`${base}/spectator-admission-context`, {game_code: wrongCode}, {session: bad, id: null});
 expect(conflict.status).toBe(409); expect(conflict.headers.has("Set-Cookie")).toBe(false);
 const absent = await post(`${base}/spectators`, {game_code: f.code}, {session: f.host.session});
 expect(absent.status).toBe(401); expect(absent.headers.has("Set-Cookie")).toBe(false);
 const privateRead = await get(f, `${f.host.session}; ${bad}`);
 expect(privateRead.status).toBe(401); expect(privateRead.headers.has("Set-Cookie")).toBe(false);
 expect((await rows(f)).spectators).toHaveLength(1);
});

test("actual spectator preflight reuses current identity with both natural cookies at owner and edge cuts",async()=>{
 const f=await fixture(1),s=await spectator(f),before=await rows(f);
 for(const selected of [s.session,`${s.context}; ${s.session}`,`${s.session}; ${s.context}`]){
  const r=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{session:selected,id:null});expect(r.status).toBe(200);expect(r.headers.has("Set-Cookie")).toBe(false);expect(await r.json()).toEqual({result:"admission_context",expires_at:s.body.receipt.outcome.session_expires_at});
 }
 expect(await rows(f)).toEqual(before);
 const valueStart=s.session.indexOf("=")+1;const bad=s.session.slice(0,valueStart)+(s.session[valueStart]==="A"?"B":"A")+s.session.slice(valueStart+1);
 for(const selected of [bad,`${bad}; ${s.context}`]){const r=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{session:selected,id:null});expect(r.status).toBe(401);expectClearedSpectator(r,f);}
 const badJoin=await post(`/api/games/${f.id}/spectators`,s.input,{session:`${bad}; ${s.context}`});expect(badJoin.status).toBe(401);expectClearedSpectator(badJoin,f);
 expect(await rows(f)).toEqual(before);
});

for(const fault of ["enumeration","delete"])test(`terminal ${fault} failure purges audience or retains only checked cleanup work and releases`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session,"spectator",false);
 await runInDurableObject(owner(f.id),(instance,state)=>{
  if(fault==="delete")state.storage.sql.exec("CREATE TRIGGER spectator_terminal_cleanup_fault BEFORE DELETE ON game_spectator_sessions BEGIN SELECT RAISE(IGNORE); END");
  else{state.spectatorGetWebSockets=state.getWebSockets.bind(state);state.getWebSockets=()=>{if([...state.storage.sql.exec("SELECT state FROM game_record")][0].state==="cancelled")throw new Error("test-only terminal enumeration fault before send");return state.spectatorGetWebSockets();};}
 });
 try{
  const r=await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"awaiting_players"},{session:f.host.session});expect(r.status).toBe(fault==="enumeration"?503:200);
  expect(await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),(_,state)=>[...state.storage.sql.exec("SELECT game_id FROM directory_global_reservation WHERE game_id IS NOT NULL")])).toHaveLength(0);
  if(fault==="delete"){
   const before=await rows(f);expect(before.sessions).toHaveLength(1);const denied=await get(f,s.session);expect(denied.status).toBe(503);expect(Object.keys(await denied.json())).toEqual(["error"]);expect(denied.headers.has("Set-Cookie")).toBe(false);
   const pending=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT attempted,next_attempt_at FROM game_spectator_terminal_work")]);expect(pending).toHaveLength(1);expect(pending[0].attempted).toBe(1);
   await runInDurableObject(owner(f.id),(_,state)=>state.storage.sql.exec("DROP TRIGGER spectator_terminal_cleanup_fault"));await evictDurableObject(owner(f.id));await runInDurableObject(owner(f.id),instance=>instance.alarm());
  }else await runInDurableObject(owner(f.id),(_,state)=>{state.getWebSockets=state.spectatorGetWebSockets;delete state.spectatorGetWebSockets;});
  await assertPurged(f,s);
 }finally{await runInDurableObject(owner(f.id),(_,state)=>{if(state.spectatorGetWebSockets)state.getWebSockets=state.spectatorGetWebSockets;state.storage.sql.exec("DROP TRIGGER IF EXISTS spectator_terminal_cleanup_fault");});socket.ws.close(1000);}
});

test("real Workerd maximum legal call pool commits winner and sends complete public terminal notice",async()=>{
 const host=await enroll("MaxPoolSpectatorHost");const created=await post("/api/games",{configuration:{numeric_upper_bound:1000,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],player_capacity:2,spectator_capacity:1}},{session:host.session});expect(created.status).toBe(200);const id=(await created.json()).game.game_id;const opened=await post(`/api/games/${id}/lobby`,{expected_revision:0},{session:host.session});expect(opened.status).toBe(200);const f={host,id,code:(await opened.json()).game_code},running=await started(f);
 try{
  let revision=(await view(f,host.session,"account")).view_revision;
  for(let n=1;n<=1000;n++){const r=await post(`/api/games/${id}/calls/manual`,{value:String(n),expected_revision:revision},{session:host.session});expect(r.status).toBe(200);revision=(await r.json()).view_revision;}
  const s=await spectator(f),socket=await stream(f,s.session,"spectator",false);await observeTerminalSend(f,socket);
  try{
   revision=(await view(f,host.session,"account")).view_revision;
   const r=await post(`/api/games/${id}/winner`,{player_id:running.players[0].body.player_id,expected_revision:revision},{session:host.session});expect(r.status).toBe(200);
   const terminal=await socket.next();expect(terminal.kind).toBe("terminal_notice");expect(terminal.calls).toHaveLength(1000);expect(terminal.calls[999]).toEqual({sequence_no:1000,value:"1000"});expect(new TextEncoder().encode(JSON.stringify(terminal)).length).toBeGreaterThan(32768);
   const observed=await runInDurableObject(owner(id),instance=>instance.spectatorTerminalObservation);expect(observed.identities).toBe(1);expect(observed.sessions).toBe(1);expect(observed.calls).toBe(1000);expect(observed.state).toBe("resolved");await assertPurged(f,s);
   const history=await runInDurableObject(owner(id),(_,state)=>[...state.storage.sql.exec("SELECT count(*) AS calls FROM game_history_calls")][0].calls);expect(history).toBe(1000);
  }finally{socket.ws.close(1000);}
 }finally{for(const c of running.sockets)c.ws.close(1000);}
},600000);

for(const capacity of [0,1])for(const state of ["awaiting_players","in_progress"])test(`spectator discovery safe owner/edge capacity ${capacity} ${state}`,async()=>{
 const f=await fixture(capacity),running=state==="in_progress"?await started(f):null;
 try{
  for(const full of capacity?[false,true]:[false]){
   let s;if(full)s=await spectator(f);
   const flag=capacity>0&&!full;
   const entry=await exports.default.fetch(`${origin}/api/game-codes/${f.code}`);expect(entry.status).toBe(200);const publicCode=await entry.json();expect(publicCode.can_join_spectator).toBe(flag);expect(publicCode.can_join_player).toBe(state==="awaiting_players");
   for(const [selected,session] of [["entry",null],["account",f.host.session],...(running?[["player",running.players[0].session]]:[]),...(s?[["spectator",s.session]]:[])]){
    const r=await exports.default.fetch(`${origin}/api/games/${f.id}/availability?view=${selected}${selected==="entry"?`&game_code=${f.code}`:""}`,{headers:session?{Cookie:session}:{}});expect(r.status).toBe(200);expect(r.headers.has("Set-Cookie")).toBe(false);const body=await r.json();expect(body.state).toBe(state);expect(body.can_join_spectator).toBe(flag);
    if(selected==="account"){expect(body.spectator_count).toBe(full?1:0);expect(body.spectator_capacity).toBe(capacity);}else for(const field of ["players","board","player_count","spectator_count","spectator_capacity"])expect(body[field]).toBeUndefined();
   }
  }
 }finally{if(running)for(const socket of running.sockets)socket.ws.close(1000);}
});

for(const offset of [-1,0])test(`crash before notice consumption reconstructs bounded terminal attempt ${offset}`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session,"spectator",false);
 await runInDurableObject(owner(f.id),(_,state)=>{
  const storage=state.storage;storage.spectatorSync=storage.sync.bind(storage);
  storage.sync=()=>{const work=[...storage.sql.exec("SELECT attempted FROM game_spectator_terminal_work")];if(work.length&&work[0].attempted===0)throw new Error("test-only crash after terminal commit before notice consumption");return storage.spectatorSync();};
 });
 try{
  const r=await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"awaiting_players"},{session:f.host.session});expect(r.status).toBe(503);expect(r.headers.has("Set-Cookie")).toBe(false);
  const captured=await runInDurableObject(owner(f.id),async(_,state)=>({alarm:await state.storage.getAlarm(),work:[...state.storage.sql.exec("SELECT * FROM game_spectator_terminal_work")][0],state:[...state.storage.sql.exec("SELECT state FROM game_record")][0].state,sessions:[...state.storage.sql.exec("SELECT session_id FROM game_spectator_sessions")].length,current:[...state.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE viewer_kind='spectator'")].length}));expect(captured.state).toBe("cancelled");expect(captured.sessions).toBe(1);expect(captured.current).toBe(0);expect(captured.work.attempted).toBe(0);expect(captured.alarm).toBeLessThanOrEqual(captured.work.attempt_deadline);
  await runInDurableObject(owner(f.id),(_,state)=>{state.storage.sync=state.storage.spectatorSync;delete state.storage.spectatorSync;});
  await evictDurableObject(owner(f.id));await clock(f,captured.work.attempt_deadline+offset);await runInDurableObject(owner(f.id),instance=>instance.alarm());
  if(offset<0){const frame=await socket.next();expect(frame.kind).toBe("terminal_notice");expect(frame.state).toBe("cancelled");}
  await assertPurged(f,s);await runInDurableObject(owner(f.id),instance=>instance.alarm());expect((await rows(f)).notices).toHaveLength(0);
 }finally{await runInDurableObject(owner(f.id),(_,state)=>{if(state.storage.spectatorSync)state.storage.sync=state.storage.spectatorSync;});await restoreClock(f);socket.ws.close(1000);}
});

for(const field of ["viewer","session_id","connection_id","authentication_epoch","session_expires_at"])test(`terminal notice does not trust mismatched attachment ${field}`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session,"spectator",false);
 await runInDurableObject(owner(f.id),(instance,state)=>{
  const storage=state.storage,ws=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===socket.initial.connection_id);
  instance.spectatorTerminalSent=0;instance.spectatorOriginalHint=ws.deserializeAttachment();
  ws.spectatorSend=ws.send;ws.send=message=>{if(JSON.parse(message).kind==="terminal_notice")instance.spectatorTerminalSent++;return ws.spectatorSend.call(ws,message);};
  storage.spectatorSync=storage.sync.bind(storage);storage.sync=()=>{const work=[...storage.sql.exec("SELECT attempted FROM game_spectator_terminal_work")];if(work.length&&work[0].attempted===0){const hint=JSON.parse(instance.spectatorOriginalHint);hint[field]=field==="viewer"?{kind:"account",id:commandId()}:field.endsWith("_id")?commandId():hint[field]+1;ws.serializeAttachment(JSON.stringify(hint));}return storage.spectatorSync();};
 });
 try{
  const r=await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"awaiting_players"},{session:f.host.session});expect(r.status).toBe(200);
  expect(await runInDurableObject(owner(f.id),instance=>instance.spectatorTerminalSent)).toBe(0);await assertPurged(f,s);
 }finally{await runInDurableObject(owner(f.id),(instance,state)=>{if(state.storage.spectatorSync)state.storage.sync=state.storage.spectatorSync;for(const ws of state.getWebSockets()){if(ws.spectatorSend){ws.send=ws.spectatorSend;ws.serializeAttachment(instance.spectatorOriginalHint);}}});socket.ws.close(1000);}
});

test("runtime repair genuine other spectator ACK cannot release deleted spectator projection",async()=>{
 const f=await fixture(3),a=await spectator(f),b=await spectator(f);
 await runInDurableObject(owner(f.id),instance=>{
  instance.runtimeOriginalFetch=instance.fetch.bind(instance);instance.runtimeFault=false;instance.runtimeAck=null;instance.runtimeSawRejected=false;
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   const response=await instance.runtimeOriginalFetch(request);
   if(path==="/audience-release"&&input.request?.release==="sync"){
    if(!instance.runtimeFault)instance.runtimeAck=await response.clone().text();
    else {instance.runtimeSawRejected=(await response.clone().json()).release_result==="rejected";return new Response(instance.runtimeAck,{status:200,headers:{"Content-Type":"application/json"}});}
   }
   if(instance.runtimeFault&&path==="/command"&&input.operation==="sync"&&input.view==="spectator"){
    const r=await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:b.session});expect(r.status).toBe(200);await r.text();
   }
   return response;
  };
 });
 try{
  expect((await get(f,a.session)).status).toBe(200);
  await runInDurableObject(owner(f.id),instance=>{expect(JSON.parse(instance.runtimeAck).release_result).toBe("released");instance.runtimeFault=true;});
  const released=await get(f,b.session);const body=await released.json();expect(released.status).toBe(503);expect(Object.keys(body)).toEqual(["error"]);expect(released.headers.has("Set-Cookie")).toBe(false);
  const observed=await runInDurableObject(owner(f.id),instance=>({real_selected_release_denied:instance.runtimeSawRejected}));expect(observed.real_selected_release_denied).toBe(true);
  expect((await rows(f)).spectators).toHaveLength(1);
  await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.runtimeOriginalFetch;});
  expect((await get(f,b.session)).status).toBe(401);expect((await get(f,a.session)).status).toBe(200);
 }finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.runtimeOriginalFetch;delete instance.runtimeOriginalFetch;delete instance.runtimeFault;delete instance.runtimeAck;delete instance.runtimeSawRejected;});}
});

test("runtime repair replacement predecessor close fault allows successor and terminal release",async()=>{
 const f=await fixture(),s=await spectator(f),old=await stream(f,s.session);
 await runInDurableObject(owner(f.id),(instance,state)=>{
  instance.runtimeOld=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===old.initial.connection_id);
  instance.runtimeOldClose=instance.runtimeOld.close;instance.runtimeOld.close=()=>{throw new Error("test-only actual predecessor close fault");};
 });
 try{
  const replacement=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=spectator`,{headers:{Cookie:s.session,Origin:origin,Upgrade:"websocket",Connection:"Upgrade"}});expect(replacement.status).toBe(101);
  const frames=[];let receive;replacement.webSocket.addEventListener("message",event=>{const frame=JSON.parse(event.data);if(receive)receive(frame);else frames.push(frame);});replacement.webSocket.accept();
  const initial=await (frames.length?Promise.resolve(frames.shift()):new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error("replacement initial timeout")),3000);receive=frame=>{clearTimeout(timer);resolve(frame);};}));
  expect(initial.kind).toBe("snapshot");expect(initial.view).toEqual((await view(f,s.session)).snapshot);
  expect(initial.session_expires_at).toBe(old.initial.session_expires_at);
  const after=await rows(f);expect(after.connections.filter(c=>c.viewer_kind==="spectator")).toHaveLength(1);expect(after.connections.filter(c=>c.viewer_kind==="spectator")[0].connection_id).not.toBe(old.initial.connection_id);
  const ledger=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT connection_id FROM game_socket_closures WHERE connection_id=?",old.initial.connection_id)]);expect(ledger).toHaveLength(1);
  for(let attempt=0;attempt<2;attempt++){
   const before=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT next_attempt_at,attempt_count FROM game_socket_closures WHERE connection_id=?",old.initial.connection_id)][0]);
   await clock(f,before.next_attempt_at);
   expect((await get(f,s.session)).status).toBe(200);
   const after=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT next_attempt_at,attempt_count FROM game_socket_closures WHERE connection_id=?",old.initial.connection_id)][0]);
   expect(after.attempt_count).toBe(before.attempt_count+1);expect(after.next_attempt_at).toBeGreaterThan(before.next_attempt_at);
  }
  const retry=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?",old.initial.connection_id)]);
  const delivery=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT connection_id FROM game_delivery_connections WHERE connection_id=?",old.initial.connection_id)]);expect(delivery).toHaveLength(0);
  await runInDurableObject(owner(f.id),async instance=>{
   await instance.webSocketError(instance.runtimeOld,new Error("test-only stale callback"));
   await instance.webSocketClose(instance.runtimeOld,1000,"stale predecessor",true);
   await instance.webSocketMessage(instance.runtimeOld,JSON.stringify({version:1,kind:"snapshot_ack",connection_id:old.initial.connection_id,delivery_id:old.initial.delivery_id,view_revision:old.initial.view_revision}));
  });
  expect(await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?",old.initial.connection_id)])).toEqual(retry);
  const denied=await get(f,s.session);expect(denied.status).toBe(200);
  const ending=await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"awaiting_players"},{session:f.host.session});expect(ending.status).toBe(200);
  const blockedState=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT state FROM game_record")][0].state);expect(blockedState).toBe("cancelled");
  expect((await rows(f)).sessions).toHaveLength(0);
  expect(await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),(_,state)=>[...state.storage.sql.exec("SELECT game_id FROM directory_global_reservation WHERE game_id IS NOT NULL")])).toHaveLength(0);
  expect(await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT * FROM game_socket_closures WHERE connection_id=?",old.initial.connection_id)])).toEqual(retry);
  await runInDurableObject(owner(f.id),instance=>{instance.runtimeOld.close=instance.runtimeOldClose;});
  expect([401,404]).toContain((await get(f,s.session)).status);
 }finally{await runInDurableObject(owner(f.id),instance=>{instance.runtimeOld.close=instance.runtimeOldClose;delete instance.runtimeOld;delete instance.runtimeOldClose;});old.ws.close(1000);await restoreClock(f);}
});

test("runtime repair spectator accepted prepared orphan cannot gain presence and survives role switch as fenced close work",async()=>{
 const f=await fixture(),s=await spectator(f);
 await runInDurableObject(owner(f.id),(instance,state)=>{
  instance.runtimeAccept=state.acceptWebSocket;
  state.acceptWebSocket=function(ws,tags){instance.runtimeAccept.call(this,ws,tags);ws.runtimeOrphanClose=ws.close;ws.close=()=>{throw new Error("test-only orphan physical close fault");};};
  state.storage.sql.exec("CREATE TRIGGER runtime_orphan_owner_accept BEFORE INSERT ON game_current_connections WHEN NEW.viewer_kind='spectator' BEGIN SELECT RAISE(IGNORE); END");
 });
 let healthy;
 try{
  const failed=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=spectator`,{headers:{Cookie:s.session,Origin:origin,Upgrade:"websocket",Connection:"Upgrade"}});expect(failed.status).toBe(503);
  const prepared=await runInDurableObject(owner(f.id),(instance,state)=>{
   state.acceptWebSocket=instance.runtimeAccept;state.storage.sql.exec("DROP TRIGGER runtime_orphan_owner_accept");
   return {grants:[...state.storage.sql.exec("SELECT connection_id FROM game_connection_grants WHERE viewer_kind='spectator'")],current:[...state.storage.sql.exec("SELECT connection_id FROM game_current_connections WHERE viewer_kind='spectator'")],physical:state.getWebSockets().filter(ws=>ws.runtimeOrphanClose).map(ws=>JSON.parse(ws.deserializeAttachment()).connection_id)};
  });expect(prepared.current).toHaveLength(0);expect(prepared.grants).toHaveLength(1);expect(prepared.physical).toEqual([prepared.grants[0].connection_id]);
  const change=await post(`/api/games/${f.id}/membership/player`,{game_code:f.code,alias:"OrphanSuccessor"},{session:s.session});expect(change.status).toBe(200);
  const captured=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT connection_id FROM game_socket_closures WHERE connection_id=?",prepared.physical[0])]);expect(captured).toHaveLength(1);
  healthy=await stream(f,cookie(change),"player");expect(healthy.initial.view.alias).toBe("OrphanSuccessor");expect((await get(f,cookie(change),"view=player")).status).toBe(200);
  const again=await runInDurableObject(owner(f.id),(instance,state)=>({before:[...state.storage.sql.exec("SELECT next_attempt_at,attempt_count FROM game_socket_closures WHERE connection_id=?",prepared.physical[0])],ws:state.getWebSockets().filter(ws=>ws.runtimeOrphanClose).length}));expect(again.ws).toBe(1);
  await runInDurableObject(owner(f.id),async(instance,state)=>{const ws=state.getWebSockets().find(ws=>ws.runtimeOrphanClose);await instance.webSocketError(ws,new Error("test-only duplicate orphan callback"));});
  const ledger=await runInDurableObject(owner(f.id),(_,state)=>[...state.storage.sql.exec("SELECT next_attempt_at,attempt_count FROM game_socket_closures WHERE connection_id=?",prepared.physical[0])]);expect(ledger).toEqual(again.before);
  expect((await get(f,cookie(change),"view=player")).status).toBe(200);
 }finally{await runInDurableObject(owner(f.id),(instance,state)=>{state.acceptWebSocket=instance.runtimeAccept;state.storage.sql.exec("DROP TRIGGER IF EXISTS runtime_orphan_owner_accept");for(const ws of state.getWebSockets())if(ws.runtimeOrphanClose){ws.close=ws.runtimeOrphanClose;delete ws.runtimeOrphanClose;}delete instance.runtimeAccept;});healthy?.ws.close(1000);}
});

for(const offset of [-1,0,1])test(`runtime repair current shorter grace release exclusive boundary ${offset}`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session);
 await runInDurableObject(owner(f.id),(instance,state)=>{
  instance.runtimeOriginalFetch=instance.fetch.bind(instance);instance.runtimeDisconnectDone=false;instance.runtimeGrace=null;instance.runtimeRelease=null;
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   if(input.expiry&&instance.runtimeGrace!==null)Date.now=()=>instance.runtimeGrace-1;
   const response=await instance.runtimeOriginalFetch(request);
   if(path==="/command"&&input.operation==="sync"&&input.view==="spectator"&&!instance.runtimeDisconnectDone){
    instance.runtimeDisconnectDone=true;
    const ws=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===socket.initial.connection_id);expect(ws).toBeDefined();
    await instance.webSocketError(ws,new Error("test-only genuine current endpoint disconnect"));
    instance.runtimeGrace=[...state.storage.sql.exec("SELECT grace_expires_at FROM game_spectators")][0].grace_expires_at;
   }
   if(path==="/audience-release"&&input.request?.release==="sync"){
    instance.runtimeRelease=await response.clone().json();Date.now=()=>instance.runtimeGrace+offset;
   }
   return response;
  };
 });
 try{
  const response=await get(f,s.session),body=await response.json();if(offset<0)expect(response.status).toBe(200);else expect([401,503]).toContain(response.status);
  if(offset<0)expect(body.snapshot.spectator_id).toBe(s.body.receipt.outcome.identity.spectator_id);
  else {expect(Object.keys(body)).toEqual(["error"]);expect(response.headers.has("Set-Cookie")).toBe(false);}
  const observation=await runInDurableObject(owner(f.id),(instance,state)=>({grace:instance.runtimeGrace,reply:instance.runtimeRelease,clock:Date.now(),floor:[...state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata")][0].last_observed_ms}));
  expect(observation.reply.release_result).toBe("released");expect(observation.reply.valid_until).toBe(observation.grace);expect(observation.clock).toBe(observation.grace-1);
  if(offset>=0)expect(observation.floor).toBe(observation.grace+offset);
  await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.runtimeOriginalFetch;});
  if(offset>=0){
   await clock(f,observation.grace-1);expect((await get(f,s.session)).status).toBe(503);expect((await rows(f)).sessions).toHaveLength(0);
  }else expect((await get(f,s.session)).status).toBe(200);
 }finally{await restoreClock(f);await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.runtimeOriginalFetch;delete instance.runtimeOriginalFetch;delete instance.runtimeDisconnectDone;delete instance.runtimeGrace;delete instance.runtimeRelease;});socket.ws.close(1000);}
});

for(const variant of ["game","action","receipt","context"])test(`runtime repair genuine ACK replay binds ${variant}`,async()=>{
 let f=await fixture();
 const install=async target=>runInDurableObject(owner(target.id),instance=>{
  instance.runtimeBindingFetch=instance.fetch.bind(instance);instance.runtimeBindingFault=false;instance.runtimeBindingAck=null;instance.runtimeBindingDenied=false;
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   const response=await instance.runtimeBindingFetch(request);
   if(path==="/audience-release"){
    const reply=await response.clone().json();
    if(!instance.runtimeBindingFault&&reply.release_result==="released")instance.runtimeBindingAck=await response.clone().text();
    else if(instance.runtimeBindingFault&&input.request?.release===(variant==="receipt"?"receipt":variant==="context"?"context":"sync")){
     instance.runtimeBindingDenied=reply.release_result==="rejected";
     return new Response(instance.runtimeBindingAck,{status:200,headers:{"Content-Type":"application/json"}});
    }
   }
   if(instance.runtimeBindingFault&&path==="/command"){
    const output=await response.clone().json();
    if(variant==="receipt"&&input.operation==="join_spectator"&&output.owner_result==="audience"){
     expect(output.cookie.kind).toBe("spectator");
     const left=await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:`__Secure-brews-spectator=${output.cookie.token}`});expect(left.status).toBe(200);await left.text();
    }else if(variant==="context"&&input.operation==="spectator_context"&&output.owner_result==="audience"){
     expect(output.cookie.kind).toBe("spectator_admission");
     const joined=await post(`/api/games/${f.id}/spectators`,{game_code:f.code},{session:`__Secure-brews-audience-admission=${output.cookie.token}`});expect(joined.status).toBe(200);await joined.text();
    }
   }
   return response;
  };
 });
 await install(f);
 let a;
 try{
  a=await spectator(f);
  if(variant==="game"){
   expect((await get(f,a.session)).status).toBe(200);
   const ack=await runInDurableObject(owner(f.id),instance=>instance.runtimeBindingAck);
   const old=f;await post(`/api/games/${f.id}/cancel`,{confirmed:true,expected_state:"awaiting_players"},{session:f.host.session});
   await runInDurableObject(owner(old.id),instance=>{instance.fetch=instance.runtimeBindingFetch;});
   const created=await post("/api/games",{configuration:{numeric_upper_bound:4,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],player_capacity:3,spectator_capacity:3}},{session:f.host.session});expect(created.status).toBe(200);
   const id=(await created.json()).game.game_id;const opened=await post(`/api/games/${id}/lobby`,{expected_revision:0},{session:f.host.session});expect(opened.status).toBe(200);f={host:f.host,id,code:(await opened.json()).game_code};
   a=await spectator(f);await install(f);await runInDurableObject(owner(f.id),instance=>{instance.runtimeBindingAck=ack;});
  }else if(variant==="action"||variant==="context"){
   // Current-session context ACK has the same grace cap as Sync, so the
   // action control cannot pass merely through the existing no-extension guard.
   const context=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{session:variant==="action"?a.session:undefined,id:null});expect(context.status).toBe(200);
  }
  await runInDurableObject(owner(f.id),instance=>{expect(JSON.parse(instance.runtimeBindingAck).release_result).toBe("released");instance.runtimeBindingFault=true;});
  let response;
  if(variant==="receipt"){
   // Obtain a genuine new admission context while preserving A's real receipt ACK.
   await runInDurableObject(owner(f.id),instance=>{instance.runtimeBindingFault=false;instance.runtimeSavedAck=instance.runtimeBindingAck;});
   const context=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{id:null});expect(context.status).toBe(200);
   await runInDurableObject(owner(f.id),instance=>{instance.runtimeBindingAck=instance.runtimeSavedAck;instance.runtimeBindingFault=true;});
   response=await post(`/api/games/${f.id}/spectators`,{game_code:f.code},{session:cookie(context)});
  }else if(variant==="context")response=await post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{id:null});
  else response=await get(f,a.session);
  expect(response.status).toBe(503);expect(Object.keys(await response.json())).toEqual(["error"]);expect(response.headers.has("Set-Cookie")).toBe(false);
  if(variant==="receipt"||variant==="context")expect(await runInDurableObject(owner(f.id),instance=>instance.runtimeBindingDenied)).toBe(true);
  await runInDurableObject(owner(f.id),instance=>{instance.runtimeBindingFault=false;});
  expect((await get(f,a.session)).status).toBe(200);
 }finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.runtimeBindingFetch;delete instance.runtimeBindingFetch;delete instance.runtimeBindingFault;delete instance.runtimeBindingAck;delete instance.runtimeBindingDenied;delete instance.runtimeSavedAck;});}
});

for(const cut of ["sync","release"])for(const offset of [-1,0,1])test(`runtime repair new grace during ${cut} owner persistence cut ${offset}`,async()=>{
 const f=await fixture(),s=await spectator(f),socket=await stream(f,s.session,"spectator",false);
 await runInDurableObject(owner(f.id),(instance,state)=>{
  instance.runtimeCutFetch=instance.fetch.bind(instance);instance.runtimeCutSync=state.storage.sync.bind(state.storage);instance.runtimeCutArmed=false;instance.runtimeCutDone=false;instance.runtimeCutGrace=null;instance.runtimeCutCount=0;
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   if(!instance.runtimeCutDone&&((cut==="sync"&&path==="/command"&&input.operation==="sync"&&input.view==="spectator")||(cut==="release"&&path==="/audience-release"&&input.request?.release==="sync")))instance.runtimeCutArmed=true;
   const result=await instance.runtimeCutFetch(request);
   if(path==="/command"&&input.operation==="sync"&&input.view==="spectator")instance.runtimeCutOwnerReply=await result.clone().json();
   if(path==="/audience-release"&&input.request?.release==="sync")instance.runtimeCutReply=await result.clone().json();
   return result;
  };
  state.storage.sync=async()=>{
   await instance.runtimeCutSync();
   // Sync has three unchanged-snapshot broadcast flushes before its own output flush.
   if(instance.runtimeCutArmed&&!instance.runtimeCutDone&&++instance.runtimeCutCount===(cut==="sync"?4:1)){
    instance.runtimeCutDone=true;instance.runtimeCutArmed=false;
    const ws=state.getWebSockets().find(ws=>JSON.parse(ws.deserializeAttachment()).connection_id===socket.initial.connection_id);expect(ws).toBeDefined();
    await instance.webSocketError(ws,new Error("test-only actual current disconnect during persistence"));
    instance.runtimeCutGrace=[...state.storage.sql.exec("SELECT grace_expires_at FROM game_spectators")][0].grace_expires_at;
    Date.now=()=>instance.runtimeCutGrace+offset;
   }
  };
 });
 try{
  const response=await get(f,s.session);expect(response.status).toBe(offset<0?200:401);
  if(offset<0){expect((await response.json()).snapshot.spectator_id).toBe(s.body.receipt.outcome.identity.spectator_id);await runInDurableObject(owner(f.id),instance=>{expect(instance.runtimeCutReply.valid_until).toBe(instance.runtimeCutGrace);if(cut==="sync")expect(instance.runtimeCutOwnerReply.authority.valid_until).toBe(instance.runtimeCutGrace);});}
  else {expect(Object.keys(await response.json())).toEqual(["error"]);expect(response.headers.has("Set-Cookie")).toBe(false);const floor=await runInDurableObject(owner(f.id),(instance,state)=>({observed:instance.runtimeCutGrace+offset,floor:[...state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata")][0].last_observed_ms}));expect(floor.floor).toBeGreaterThanOrEqual(floor.observed);await clock(f,floor.observed-1);expect((await get(f,s.session)).status).toBe(503);}
 }finally{await restoreClock(f);await runInDurableObject(owner(f.id),(instance,state)=>{instance.fetch=instance.runtimeCutFetch;state.storage.sync=instance.runtimeCutSync;delete instance.runtimeCutFetch;delete instance.runtimeCutSync;delete instance.runtimeCutArmed;delete instance.runtimeCutDone;delete instance.runtimeCutGrace;delete instance.runtimeCutReply;delete instance.runtimeCutCount;delete instance.runtimeCutOwnerReply;});socket.ws.close(1000);}
});

for(const kind of ["sync","context","receipt"])test(`attempt repair same actor genuine stale ${kind} ACK after real Leave fails closed`,async()=>{
 const f=await fixture(),s=await spectator(f);
 const invoke=()=>kind==="sync"?get(f,s.session):kind==="context"?post(`/api/games/${f.id}/spectator-admission-context`,{game_code:f.code},{session:s.session,id:null}):post(`/api/games/${f.id}/spectators`,s.input,{session:`${s.context}; ${s.session}`,id:s.command});
 await runInDurableObject(owner(f.id),instance=>{
  instance.attemptFetch=instance.fetch.bind(instance);instance.attemptReplay=false;instance.attemptAck=null;instance.attemptInputs=[];
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   const response=await instance.attemptFetch(request);
   if(path==="/audience-release"&&input.request?.release===kind){
    instance.attemptInputs.push(input);
    const reply=await response.clone().json();
    if(!instance.attemptReplay){expect(reply.release_result).toBe("released");instance.attemptAck=await response.clone().text();}
    else {instance.attemptDenied=reply.release_result==="rejected";return new Response(instance.attemptAck,{status:200,headers:{"Content-Type":"application/json"}});}
   }
   if(instance.attemptReplay&&path==="/command"&&input.operation===(kind==="sync"?"sync":kind==="context"?"spectator_context":"join_spectator")){
    const output=await response.clone().json();expect(output.owner_result).toBe(kind==="sync"?"audience_synced":"audience");
    const left=await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:s.session});expect(left.status).toBe(200);await left.text();
   }
   return response;
  };
 });
 try{
  for(let n=0;n<2;n++){const healthy=await invoke();expect(healthy.status).toBe(200);expect(healthy.headers.has("Set-Cookie")).toBe(false);const body=await healthy.json();expect(body.binding).toBeUndefined();expect(body.challenge).toBeUndefined();}
  await runInDurableObject(owner(f.id),instance=>{instance.attemptReplay=true;});
  const replay=await invoke();expect(replay.status).toBe(503);expect(Object.keys(await replay.json())).toEqual(["error"]);expect(replay.headers.has("Set-Cookie")).toBe(false);
  const observed=await runInDurableObject(owner(f.id),instance=>({denied:instance.attemptDenied,count:instance.attemptInputs.length,identical:instance.attemptInputs.every(input=>JSON.stringify(input.request)===JSON.stringify(instance.attemptInputs[0].request)),unique:new Set(instance.attemptInputs.map(input=>JSON.stringify(input.challenge))).size}));
  expect(observed).toEqual({denied:true,count:3,identical:true,unique:3});
  expect((await rows(f)).spectators).toHaveLength(0);
  await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.attemptFetch;});expect((await get(f,s.session)).status).toBe(401);
 }finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.attemptFetch;for(const key of ["attemptFetch","attemptReplay","attemptAck","attemptInputs","attemptDenied"])delete instance[key];});}
});

for(const result of ["released","rejected"])test(`attempt repair genuine ${result} ACK for wrong challenge fails closed`,async()=>{
 const f=await fixture(),s=await spectator(f);
 await runInDurableObject(owner(f.id),instance=>{
  instance.attemptFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   if(path==="/audience-release"){
    const original=await instance.attemptFetch(request);const real=await original.clone().json();expect(real.release_result).toBe(result);
    input.challenge[0]^=1;
    const changed=await instance.attemptFetch(new Request(request,{body:JSON.stringify(input)}));const other=await changed.clone().json();expect(other.release_result).toBe(result);
    expect(other.binding).not.toEqual(real.binding);expect(Object.keys(other).sort()).toEqual(result==="released"?["binding","checked_at","release_result","valid_until"]:["binding","code","release_result"]);
    instance.attemptCompared=true;return changed;
   }
   const response=await instance.attemptFetch(request);
   if(result==="rejected"&&path==="/command"&&input.operation==="sync"){
    const left=await post(`/api/games/${f.id}/membership/leave`,{expected_state:"awaiting_players"},{session:s.session});expect(left.status).toBe(200);await left.text();
   }
   return response;
  };
 });
 try{const response=await get(f,s.session);expect(response.status).toBe(503);expect(Object.keys(await response.json())).toEqual(["error"]);expect(response.headers.has("Set-Cookie")).toBe(false);expect(await runInDurableObject(owner(f.id),instance=>instance.attemptCompared)).toBe(true);}
 finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.attemptFetch;delete instance.attemptFetch;delete instance.attemptCompared;});}
});

for(const fault of ["missing challenge","short challenge","missing binding","future timestamp"])test(`attempt repair ${fault} cannot release protected projection`,async()=>{
 const f=await fixture(),s=await spectator(f);
 await runInDurableObject(owner(f.id),instance=>{
  instance.attemptFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{
   if(new URL(request.url).pathname!=="/audience-release")return instance.attemptFetch(request);
   instance.attemptIntercepted=true;
   if(fault.endsWith("challenge")){
    const input=await request.clone().json();if(fault==="missing challenge")delete input.challenge;else input.challenge.pop();
    const malformed=await instance.attemptFetch(new Request(request,{body:JSON.stringify(input)}));expect(malformed.status).toBe(400);return malformed;
   }
   const real=await instance.attemptFetch(request),reply=await real.json();expect(reply.release_result).toBe("released");
   if(fault==="missing binding")delete reply.binding;else {expect(reply.valid_until).toBeGreaterThan(Date.now()+10000);reply.checked_at=Date.now()+10000;}
   return new Response(JSON.stringify(reply),{status:200,headers:{"Content-Type":"application/json"}});
  };
 });
 try{const response=await get(f,s.session);expect(response.status).toBe(503);expect(Object.keys(await response.json())).toEqual(["error"]);expect(response.headers.has("Set-Cookie")).toBe(false);expect(await runInDurableObject(owner(f.id),instance=>instance.attemptIntercepted)).toBe(true);
  await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.attemptFetch;});expect((await get(f,s.session)).status).toBe(200);
 }finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.attemptFetch;delete instance.attemptFetch;delete instance.attemptIntercepted;});}
});

for(const fault of ["rng","clock"])test(`attempt repair trusted ${fault} failure sends no release request`,async()=>{
 const f=await fixture(),s=await spectator(f);
 await runInDurableObject(owner(f.id),instance=>{
  instance.attemptFetch=instance.fetch.bind(instance);instance.attemptReleaseCount=0;instance.attemptDraws=0;
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   if(path==="/audience-release")instance.attemptReleaseCount++;
   const response=await instance.attemptFetch(request);
   if(path==="/command"&&input.operation==="sync"&&input.view==="spectator"){
    expect((await response.clone().json()).owner_result).toBe("audience_synced");
    if(fault==="rng")crypto.getRandomValues=function(){crypto.getRandomValues=nativeRandom;instance.attemptDraws++;throw new Error("injected entropy failure");};else Date.now=()=>{Date.now=nativeNow;return NaN;};
   }
   return response;
  };
 });
 try{const response=await get(f,s.session);expect(response.status,"fault response").toBe(503);expect(Object.keys(await response.json())).toEqual(["error"]);expect(response.headers.has("Set-Cookie")).toBe(false);
  const observed=await runInDurableObject(owner(f.id),instance=>{crypto.getRandomValues=nativeRandom;Date.now=nativeNow;instance.fetch=instance.attemptFetch;let rngOK=true;try{crypto.getRandomValues(new Uint8Array(32));}catch{rngOK=false;}return{requests:instance.attemptReleaseCount,draws:instance.attemptDraws,clockOK:Number.isFinite(Date.now()),rngOK};});expect(observed).toEqual({requests:0,draws:fault==="rng"?1:0,clockOK:true,rngOK:true});expect((await get(f,s.session)).status,"healthy after fault restore").toBe(200);
 }finally{await runInDurableObject(owner(f.id),instance=>{crypto.getRandomValues=nativeRandom;Date.now=nativeNow;instance.fetch=instance.attemptFetch;for(const key of ["attemptFetch","attemptReleaseCount","attemptDraws"])delete instance[key];});}
});

test("attempt repair trusted challenges ignore client headers on healthy retry",async()=>{
 const f=await fixture(),s=await spectator(f),chosen=JSON.stringify(Array(32).fill(7));
 await runInDurableObject(owner(f.id),instance=>{
  instance.attemptFetch=instance.fetch.bind(instance);instance.attemptChallenges=[];
  instance.fetch=async request=>{if(new URL(request.url).pathname==="/audience-release"){const input=await request.clone().json();instance.attemptChallenges.push(JSON.stringify(input.challenge));}return instance.attemptFetch(request);};
 });
 try{
  for(let n=0;n<2;n++){const r=await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=spectator`,{headers:{Cookie:s.session,"X-Brews-Audience-Challenge":chosen}});expect(r.status).toBe(200);expect(r.headers.has("Set-Cookie")).toBe(false);const body=await r.json();expect(body.challenge).toBeUndefined();expect(body.binding).toBeUndefined();}
  expect(await runInDurableObject(owner(f.id),instance=>({count:instance.attemptChallenges.length,unique:new Set(instance.attemptChallenges).size,chosen:instance.attemptChallenges.includes(chosen)}))).toEqual({count:2,unique:2,chosen:false});
 }finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.attemptFetch;delete instance.attemptFetch;delete instance.attemptChallenges;});}
});

test("attempt repair original expiry fences rollback before any entropy draw",async()=>{
 const f=await fixture(),s=await spectator(f);
 await runInDurableObject(owner(f.id),instance=>{
  instance.attemptFetch=instance.fetch.bind(instance);instance.attemptDraws=0;instance.attemptReleaseCount=0;instance.attemptFence=false;
  instance.fetch=async request=>{
   const path=new URL(request.url).pathname,input=await request.clone().json();
   if(path==="/audience-release")instance.attemptReleaseCount++;
   if(input.expiry){instance.attemptFence=true;Date.now=()=>instance.attemptDeadline-1;}
   const response=await instance.attemptFetch(request);
   if(path==="/command"&&input.operation==="sync"&&input.view==="spectator"){
    const reply=await response.clone().json();expect(reply.owner_result).toBe("audience_synced");instance.attemptDeadline=reply.authority.valid_until;
    Date.now=()=>instance.attemptDeadline;
    crypto.getRandomValues=()=>{instance.attemptDraws++;throw new Error("injected entropy failure");};
   }
   return response;
  };
 });
 try{
  const response=await get(f,s.session);expect(response.status).toBe(401);expect(Object.keys(await response.json())).toEqual(["error"]);expect(response.headers.has("Set-Cookie")).toBe(false);
  const observed=await runInDurableObject(owner(f.id),(instance,state)=>{Date.now=nativeNow;crypto.getRandomValues=nativeRandom;instance.fetch=instance.attemptFetch;return{deadline:instance.attemptDeadline,floor:[...state.storage.sql.exec("SELECT last_observed_ms FROM game_metadata")][0].last_observed_ms,draws:instance.attemptDraws,releases:instance.attemptReleaseCount,fenced:instance.attemptFence};});
  expect(observed.draws).toBe(0);expect(observed.releases).toBe(0);expect(observed.fenced).toBe(true);expect(observed.floor).toBeGreaterThanOrEqual(observed.deadline);
  expect((await rows(f)).sessions).toHaveLength(0);await clock(f,observed.deadline-1);expect((await get(f,s.session)).status).toBe(503);
 }finally{await restoreClock(f);await runInDurableObject(owner(f.id),instance=>{Date.now=nativeNow;crypto.getRandomValues=nativeRandom;instance.fetch=instance.attemptFetch;for(const key of ["attemptFetch","attemptDraws","attemptReleaseCount","attemptFence","attemptDeadline"])delete instance[key];});}
});
