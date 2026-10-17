import { beforeEach, expect, test } from 'vitest';
import { env, exports } from 'cloudflare:workers';
import { reset, runInDurableObject, evictDurableObject } from 'cloudflare:test';
import { enroll, post, resetStorage, origin, cookie, commandId, password } from './fixtures.js';
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async () => {
  await reset(); await resetStorage();
  await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName('directory')), async (_i, s) => s.storage.deleteAll());
});
async function fixture() {
  const host = await enroll('LifecycleHost');
  const made = await post('/api/games', {configuration:{numeric_upper_bound:4,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],player_capacity:3,spectator_capacity:0}}, {session:host.session});
  expect(made.status).toBe(200); const id = (await made.json()).game.game_id;
  const opened = await post(`/api/games/${id}/lobby`, {expected_revision:0}, {session:host.session});
  expect(opened.status).toBe(200); const code = (await opened.json()).game_code;
  const players = [];
  for (const alias of ['First','Second']) players.push(await join({id,code},alias));
  return {host,id,code,players};
}
async function join(f,alias) {
  const context = await post(`/api/games/${f.id}/admission-context`,{game_code:f.code},{id:null});
  expect(context.status).toBe(200); const answer=password();
  const result = await post(`/api/games/${f.id}/players`,{game_code:f.code,alias,recovery_answer:answer},{session:cookie(context)});
  expect(result.status).toBe(200); return {session:cookie(result),answer,body:await result.json()};
}
const sync = (f,session,view='player') => exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=${view}`,{headers:{Cookie:session}});
const rename = (f,alias,{id=commandId(),session=f.players[0].session}={}) => exports.default.fetch(`${origin}/api/games/${f.id}/membership/alias`,{method:'PATCH',headers:{Origin:origin,Cookie:session,'Content-Type':'application/json','Idempotency-Key':id},body:JSON.stringify({alias})});
const leave = (f,state,{id=commandId(),session=f.players[0].session}={}) => post(`/api/games/${f.id}/membership/leave`,{expected_state:state},{id,session});
async function stream(f,player) {
  const result = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=player`,{headers:{Origin:origin,Cookie:player.session,Upgrade:'websocket',Connection:'Upgrade'}});
  expect(result.status).toBe(101); const ws=result.webSocket;
  const queue=[]; const readers=[];
  ws.addEventListener('message',e=>{
    const frame=JSON.parse(e.data);
    ws.send(JSON.stringify({version:1,kind:'snapshot_ack',connection_id:frame.connection_id,delivery_id:frame.delivery_id,view_revision:frame.view_revision}));
    if(readers.length) readers.shift()(frame); else queue.push(frame);
  });
  const next=()=>queue.length?Promise.resolve(queue.shift()):new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(new Error('lifecycle frame timeout')),3000);readers.push(frame=>{clearTimeout(timeout);resolve(frame);});});
  ws.accept(); const initial=await next();
  return {ws,initial,next};
}
async function start(f) {
  const sockets=[await stream(f,f.players[0]),await stream(f,f.players[1])];
  const host=await sync(f,f.host.session,'account'); expect(host.status).toBe(200);
  const current=await host.json(); expect(current.snapshot.connected_player_count).toBe(2);
  const result=await post(`/api/games/${f.id}/start`,{expected_revision:current.view_revision},{session:f.host.session}); expect(result.status).toBe(200);
  return sockets;
}
async function counts(f) {
  return runInDurableObject(owner(f.id),(_i,s)=>({
    players:[...s.storage.sql.exec('SELECT count(*) AS n FROM game_players')][0].n,
    sessions:[...s.storage.sql.exec('SELECT count(*) AS n FROM game_sessions')][0].n,
    proofs:[...s.storage.sql.exec('SELECT count(*) AS n FROM game_recovery')][0].n,
    current:[...s.storage.sql.exec('SELECT count(*) AS n FROM game_current_connections WHERE viewer_kind=?','player')][0].n,
    prepared:[...s.storage.sql.exec('SELECT count(*) AS n FROM game_connection_grants WHERE viewer_kind=?','player')][0].n,
  }));
}
async function restore(f) { await runInDurableObject(owner(f.id),i=>{if(i.lifecycleOriginal){i.fetch=i.lifecycleOriginal;delete i.lifecycleOriginal;} delete i.lifecycleCut;}); }

test('rename preserves stable proof/session/expiry, trims ASCII and releases old exact alias',async()=>{
  const f=await fixture();
  await runInDurableObject(owner(f.id),(i,s)=>{
    // Capture private rows inside the owner; export only equality booleans.
    i.lifecycleInvariant=JSON.stringify([...s.storage.sql.exec('SELECT * FROM game_sessions')]);
    i.lifecycleProof=JSON.stringify([...s.storage.sql.exec('SELECT * FROM game_recovery')]);
    i.lifecyclePlayer=JSON.stringify([...s.storage.sql.exec('SELECT player_id,joined_at,session_epoch,last_explicit_leave_at FROM game_players')]);
  });
  const other=await (await sync(f,f.players[1].session)).json();
  const command=commandId(); const response=await rename(f,' first!@#$%^&*()_+-= ',{id:command,session:`${f.host.session}; ${f.players[0].session}`});
  expect(response.status).toBe(200); expect(response.headers.has('set-cookie')).toBe(false);
  const accepted=await response.json(); expect(accepted.result).toBe('alias_renamed'); expect(accepted.player_id).toBe(f.players[0].body.player_id);
  expect(accepted.alias).toBe('first!@#$%^&*()_+-='); expect(accepted.session_expires_at).toBe(f.players[0].body.session_expires_at);
  expect((await (await sync(f,f.players[1].session)).json()).view_revision).toBe(other.view_revision);
  const unchanged=await runInDurableObject(owner(f.id),(i,s)=>({sessions:i.lifecycleInvariant===JSON.stringify([...s.storage.sql.exec('SELECT * FROM game_sessions')]),proof:i.lifecycleProof===JSON.stringify([...s.storage.sql.exec('SELECT * FROM game_recovery')]),player:i.lifecyclePlayer===JSON.stringify([...s.storage.sql.exec('SELECT player_id,joined_at,session_epoch,last_explicit_leave_at FROM game_players')])}));
  expect(unchanged).toEqual({sessions:true,proof:true,player:true});
  const retry=await rename(f,accepted.alias,{id:command}); expect(retry.status).toBe(200); expect(await retry.json()).toEqual({result:'committed',receipt:accepted.receipt}); expect(retry.headers.has('set-cookie')).toBe(false);
  expect((await rename(f,'Different',{id:command})).status).toBe(409);
  const noop=await rename(f,accepted.alias); expect(noop.status).toBe(200); expect((await noop.json()).view_revision).toBe(accepted.view_revision);
  await join(f,'First');
  const old=await post('/api/games/player-recovery',{game_code:f.code,alias:'First',recovery_answer:f.players[0].answer}); expect(old.status).toBe(401);
  const recovered=await post('/api/games/player-recovery',{game_code:f.code,alias:accepted.alias,recovery_answer:f.players[0].answer}); expect(recovered.status).toBe(200); expect((await recovered.json()).membership.player_id).toBe(accepted.player_id);
});
test('lobby Leave deletes membership and authority and requires new ID/proof with safe lost-response 401',async()=>{
  const f=await fixture(); const socket=await stream(f,f.players[0]); const command=commandId();
  const result=await leave(f,'awaiting_players',{id:command}); expect(result.status).toBe(200);
  const body=await result.json(); expect(body).toMatchObject({result:'player_left',left:true,expected_state:'awaiting_players',membership_retained:false,fresh_admission_required:true,session_expires_at:null,player_id:f.players[0].body.player_id});
  const clear=result.headers.get('set-cookie'); expect(clear).toContain(`__Secure-brews-player=;`);expect(clear).toContain(`Path=/api/games/${f.id}`);expect(clear).toContain('Secure; HttpOnly;');expect(clear).toContain('SameSite=Strict; Max-Age=0');expect(clear).not.toContain('__Host-brews');
  expect(await counts(f)).toEqual({players:1,sessions:1,proofs:1,current:0,prepared:0});
  expect((await leave(f,'awaiting_players',{id:command})).status).toBe(401);
  expect((await sync(f,f.players[0].session)).status).toBe(401);
  const recovery=await post('/api/games/player-recovery',{game_code:f.code,alias:'First',recovery_answer:f.players[0].answer});expect(recovery.status).toBe(401);
  const fresh=await join(f,'First');expect(fresh.body.player_id).not.toBe(f.players[0].body.player_id);
  socket.ws.close(1000);
});
test('In Progress Leave retains board/proof/return session, excludes delayed lobby branch, retry preserves successor',async()=>{
  const f=await fixture(); const sockets=await start(f);
  const original=await (await sync(f,f.players[0].session)).json();
  expect((await leave(f,'awaiting_players')).status).toBe(409);expect((await rename(f,'Late')).status).toBe(409);
  const command=commandId(); const result=await leave(f,'in_progress',{id:command});expect(result.status).toBe(200);expect(result.headers.has('set-cookie')).toBe(false);
  const body=await result.json();expect(body).toMatchObject({membership_retained:true,fresh_admission_required:false,player_id:f.players[0].body.player_id,session_expires_at:f.players[0].body.session_expires_at});
  const returned=await (await sync(f,f.players[0].session)).json();expect(returned.snapshot.board).toEqual(original.snapshot.board);
  const successor=await stream(f,f.players[0]);
  const stamp=await runInDurableObject(owner(f.id),(_i,s)=>[...s.storage.sql.exec('SELECT last_explicit_leave_at,session_epoch FROM game_players WHERE player_id=?',f.players[0].body.player_id)][0]);
  const retry=await leave(f,'in_progress',{id:command});expect(retry.status).toBe(200);expect(await retry.json()).toEqual({result:'committed',receipt:body.receipt});
  const current=await runInDurableObject(owner(f.id),(_i,s)=>({stamp:[...s.storage.sql.exec('SELECT last_explicit_leave_at,session_epoch FROM game_players WHERE player_id=?',f.players[0].body.player_id)][0],connection:[...s.storage.sql.exec('SELECT connection_id FROM game_current_connections WHERE principal_id=?',f.players[0].body.player_id)][0]?.connection_id}));
  expect(current.stamp).toEqual(stamp);expect(current.connection).toBe(successor.initial.connection_id);
  await evictDurableObject(owner(f.id));
  expect((await sync(f,f.players[0].session)).status).toBe(200);
  for(let value=1;value<=4;value++) {
    const host=await (await sync(f,f.host.session,'account')).json();
    const called=await post(`/api/games/${f.id}/calls/manual`,{value:String(value),expected_revision:host.view_revision},{session:f.host.session});expect(called.status).toBe(200);
  }
  const matched=await (await sync(f,f.players[0].session)).json();expect(matched.snapshot.board.qualified).toBe(true);
  const fresh=await leave(f,'in_progress');expect(fresh.status).toBe(200);
  const later=await runInDurableObject(owner(f.id),(_i,s)=>[...s.storage.sql.exec('SELECT last_explicit_leave_at,session_epoch FROM game_players WHERE player_id=?',f.players[0].body.player_id)][0]);expect(later.last_explicit_leave_at).toBeGreaterThan(stamp.last_explicit_leave_at);expect(later.session_epoch).toBe(stamp.session_epoch);
  const host=await (await sync(f,f.host.session,'account')).json();
  const winner=await post(`/api/games/${f.id}/winner`,{player_id:f.players[0].body.player_id,expected_revision:host.view_revision},{session:f.host.session});expect(winner.status).toBe(200);
  expect((await leave(f,'in_progress')).status).toBe(401);expect((await rename(f,'Terminal')).status).toBe(401);
  expect((await sync(f,f.players[0].session)).status).toBe(200);
  const exit=await post(`/api/games/${f.id}/exit?view=player`,{},{session:f.players[0].session});expect(exit.status).toBe(200);
  for(const s of [...sockets,successor]) s.ws.close(1000);
});
for(const mode of ['IGNORE',"ABORT, 'test-only lifecycle failure'"]) {
  test(`required ${mode==='IGNORE'?'ignored':'aborted'} Leave writes roll back membership and closure`,async()=>{
    const f=await fixture();const ws=await stream(f,f.players[0]);
    const before=await counts(f);
    await runInDurableObject(owner(f.id),(_i,s)=>s.storage.sql.exec(`CREATE TRIGGER lifecycle_fault BEFORE DELETE ON game_sessions BEGIN SELECT RAISE(${mode}); END`));
    const result=await leave(f,'awaiting_players');expect(result.status).toBe(503);expect(result.headers.has('set-cookie')).toBe(false);expect(await counts(f)).toEqual(before);
    const closes=await runInDurableObject(owner(f.id),(_i,s)=>[...s.storage.sql.exec('SELECT count(*) AS n FROM game_socket_closures')][0].n);expect(closes).toBe(0);
    expect((await sync(f,f.players[0].session)).status).toBe(200);ws.ws.close(1000);
  });
}
for(const cut of [1,2]) {
  test(`membership receiving cut ${cut} fences original expiry through rollback during denial`,async()=>{
    const f=await fixture();const now=Date.now;
    await runInDurableObject(owner(f.id),i=>{
      i.lifecycleOriginal=i.fetch.bind(i); i.fetch=async request=>{
        const path=new URL(request.url).pathname; const input=path==='/command'?await request.clone().json():{};
        if(path==='/discovery' && i.lifecycleCut) Date.now=()=>i.lifecycleCut-1;
        const result=await i.lifecycleOriginal(request);
        if(input.operation===(cut===1?'rename_alias':'rename_alias_output') && result.status===200) {
          const body=await result.clone().json();if(body.owner_result==='membership'){i.lifecycleCut=body.valid_until;Date.now=()=>i.lifecycleCut;}
        }
        return result;
      };
    });
    try {
      const result=await rename(f,'Renamed');expect(result.status).not.toBe(200);expect(result.headers.has('set-cookie')).toBe(false);
      const fenced=await runInDurableObject(owner(f.id),(i,s)=>[...s.storage.sql.exec('SELECT last_observed_ms FROM game_metadata')][0].last_observed_ms>=i.lifecycleCut);expect(fenced).toBe(true);
    } finally {Date.now=now;await restore(f);}
  });
}
test('wrong expiry-denial peer acknowledgement never releases own alias bytes',async()=>{
  const f=await fixture();const now=Date.now;
  await runInDurableObject(owner(f.id),i=>{i.lifecycleOriginal=i.fetch.bind(i);i.fetch=async request=>{
    const path=new URL(request.url).pathname;const input=path==='/command'?await request.clone().json():{};const result=await i.lifecycleOriginal(request);
    if(input.operation==='rename_alias' && result.status===200){const body=await result.clone().json();if(body.owner_result==='membership')Date.now=()=>body.valid_until;}
    if(path==='/discovery'){const body=await result.json();body.game_id=commandId();return new Response(JSON.stringify(body),{status:200,headers:{'Content-Type':'application/json'}});}
    return result;
  };});
  try {const result=await rename(f,'Renamed');expect(result.status).toBe(503);expect((await result.text()).includes('Renamed')).toBe(false);expect(result.headers.has('set-cookie')).toBe(false);}finally{Date.now=now;await restore(f);}
});
for(const action of ['epoch','alias','state']) {
  test(`second membership owner cut denies ${action} supersession after authorized result`,async()=>{
    const f=await fixture();
    await runInDurableObject(owner(f.id),(i,s)=>{i.lifecycleOriginal=i.fetch.bind(i);i.fetch=async request=>{
      const input=new URL(request.url).pathname==='/command'?await request.clone().json():{};const result=await i.lifecycleOriginal(request);
      if(input.operation==='rename_alias' && result.status===200){
        if(action==='epoch')s.storage.sql.exec('UPDATE game_players SET session_epoch=session_epoch+1 WHERE player_id=?',f.players[0].body.player_id);
        if(action==='alias')s.storage.sql.exec('UPDATE game_players SET alias=? WHERE player_id=?','Successor',f.players[0].body.player_id);
        if(action==='state')s.storage.sql.exec('UPDATE game_record SET state=?,started_at=?,idle_due=NULL WHERE singleton=1','in_progress',Date.now());
      }return result;
    };});
    try{const result=await rename(f,'Renamed');expect(result.status).not.toBe(200);expect((await result.text()).includes('Renamed')).toBe(false);expect(result.headers.has('set-cookie')).toBe(false);}finally{await restore(f);}
  });
}

for (const ownerCut of ['rename_alias','rename_alias_output']) {
  test(`owner ${ownerCut} storage-await expiry denies protected alias and fences rollback`, async()=>{
    const f=await fixture(); const now=Date.now;
    await runInDurableObject(owner(f.id),(i,s)=>{
      i.lifecycleHolder={fetch:i.fetch.bind(i),sync:s.storage.sync,active:false,expired:false,deadline:[...s.storage.sql.exec('SELECT idle_due FROM game_record')][0].idle_due};
      s.storage.sync=async function(){await i.lifecycleHolder.sync.call(this);if(i.lifecycleHolder.active && !i.lifecycleHolder.expired){i.lifecycleHolder.expired=true;Date.now=()=>i.lifecycleHolder.deadline;}};
      i.fetch=async request=>{const input=new URL(request.url).pathname==='/command'?await request.clone().json():{};i.lifecycleHolder.active=input.operation===ownerCut;return i.lifecycleHolder.fetch(request.clone());};
    });
    try {
      const result=await rename(f,'OwnerExpiryProtected');expect(result.status).not.toBe(200);expect(result.headers.has('set-cookie')).toBe(false);expect((await result.text()).includes('OwnerExpiryProtected')).toBe(false);
      const observed=await runInDurableObject(owner(f.id),(i,s)=>({fenced:[...s.storage.sql.exec('SELECT last_observed_ms FROM game_metadata')][0].last_observed_ms>=i.lifecycleHolder.deadline,expired:i.lifecycleHolder.expired,deadline:i.lifecycleHolder.deadline}));
      expect(observed.fenced).toBe(true);expect(observed.expired).toBe(true);
      Date.now=()=>observed.deadline-1;expect((await rename(f,'OwnerExpiryProtected')).status).toBe(503);
    } finally {
      Date.now=now;await runInDurableObject(owner(f.id),(i,s)=>{i.fetch=i.lifecycleHolder.fetch;s.storage.sync=i.lifecycleHolder.sync;delete i.lifecycleHolder;});
    }
  });
}

test('receipt-only rename retry after a later rename preserves later spelling and revisions',async()=>{
  const f=await fixture();const command=commandId();
  const first=await rename(f,'OldAccepted',{id:command});expect(first.status).toBe(200);const accepted=await first.json();
  const later=await rename(f,'LaterAccepted');expect(later.status).toBe(200);const current=await later.json();
  const retry=await rename(f,' OldAccepted ',{id:command});expect(retry.status).toBe(200);expect(await retry.json()).toEqual({result:'committed',receipt:accepted.receipt});expect(retry.headers.has('set-cookie')).toBe(false);
  const actual=await (await sync(f,f.players[0].session)).json();expect(actual.snapshot.alias).toBe('LaterAccepted');expect(actual.view_revision).toBe(current.view_revision);
});
