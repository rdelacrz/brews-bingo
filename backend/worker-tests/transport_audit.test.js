import {beforeEach,expect,test} from 'vitest';
import {env,exports} from 'cloudflare:workers';
import {reset,runInDurableObject} from 'cloudflare:test';
import {resetStorage,enroll,current,post,cookie,origin,stub,commandId,password} from './fixtures.js';
beforeEach(async()=>{ await reset(); await resetStorage(); });
test('Current suppresses protected output after expiry during owner sync',async()=>{
 const actor=await enroll('TransportAuditHost');
 let deadline;
 const realNow=Date.now;
 await runInDurableObject(stub(),(instance,state)=>{
  deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
  instance.auditOriginalSync=state.storage.sync.bind(state.storage);
  instance.auditOriginalFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{instance.auditActive=new URL(request.url).pathname==='/auth'; try{return await instance.auditOriginalFetch(request);}finally{instance.auditActive=false;}};
  state.storage.sync=async()=>{await instance.auditOriginalSync(); if(instance.auditActive) Date.now=()=>deadline;};
 });
 try{
  const response=await current(actor.session); const body=await response.json();
  expect(response.status).toBe(200); expect(body.authenticated).toBe(false);
  expect(body.session).toBeNull();
  const floor=await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms);
  expect(floor).toBeGreaterThanOrEqual(deadline);
  Date.now=()=>deadline-1; expect((await current(actor.session)).status).toBe(503);
 }finally{
  Date.now=realNow;
  await runInDurableObject(stub(),(instance,state)=>{state.storage.sync=instance.auditOriginalSync;instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalSync;delete instance.auditOriginalFetch;});
 }
});
test('private auth rejects a positional request root',async()=>{
 const response=await stub().fetch('https://accounts.internal/auth',{method:'POST',body:JSON.stringify([{type:'current',token:null},null,'audit'])});
 const body=await response.json(); expect(body.status).toBe(400);expect(body.body.authenticated).toBeUndefined();
});
test('Game Sync withholds player snapshot after edge expiry',async()=>{
 const actor=await enroll('TransportAuditHost');
 const created=await post('/api/games',{}, {session:actor.session});expect(created.status).toBe(200);
 const id=(await created.json()).game.game_id;
 const lobby=await post(`/api/games/${id}/lobby`,{expected_revision:0},{session:actor.session}); expect(lobby.status).toBe(200);
 const code=(await lobby.json()).game_code;
 const context=await post(`/api/games/${id}/admission-context`,{game_code:code},{id:null});expect(context.status).toBe(200);
 const joined=await post(`/api/games/${id}/players`,{game_code:code,alias:'TransportAuditPlayer'},{session:cookie(context)});expect(joined.status).toBe(200);
 const owner=env.GAMES.get(env.GAMES.idFromName(id));const realNow=Date.now;
 await runInDurableObject(owner,(instance,state)=>{
  const deadline=state.storage.sql.exec('SELECT expires_at FROM game_sessions').one().expires_at;
  state.storage.sql.exec('UPDATE game_record SET idle_due=?',deadline+5000);
  instance.auditOriginalFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{
   const parsed=await request.clone().json();const response=await instance.auditOriginalFetch(request);
   if(parsed.operation==='sync') Date.now=()=>deadline;
   return response;
  };
 });
 try{
  const response=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`,{headers:{Cookie:cookie(joined)}});
  const body=await response.json();expect(response.status).toBe(401);expect(body.snapshot).toBeUndefined();
  const floor=await runInDurableObject(owner,(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM game_metadata').one().last_observed_ms);
  expect(floor).toBeGreaterThanOrEqual(Date.now());
  Date.now=()=>floor-1; const denied=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`,{headers:{Cookie:cookie(joined)}});expect(denied.status).toBe(503);
 }finally{Date.now=realNow;await runInDurableObject(owner,instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
});

test('Users withholds newly issued enrollment secret after caller expiry at edge',async()=>{
 const actor=await enroll('TransportAuditAdmin');const realNow=Date.now;
 await runInDurableObject(stub(),(instance,state)=>{
  state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?",actor.accountId);
  const deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
  instance.auditOriginalFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{const response=await instance.auditOriginalFetch(request);if(new URL(request.url).pathname==='/users')Date.now=()=>deadline;return response;};
 });
 try{
  const response=await post('/api/users/hosts',{username:'TransportCreatedHost'},{session:actor.session});
  const body=await response.json();expect(response.status).toBe(503);expect(body.enrollment_url).toBeUndefined();
  const floor=await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms);
  expect(floor).toBeGreaterThanOrEqual(Date.now());
  Date.now=()=>floor-1;expect((await current(actor.session)).status).toBe(503);
 }finally{Date.now=realNow;await runInDurableObject(stub(),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
});
test('History initial account expiry denial durably fences rollback',async()=>{
 const actor=await enroll('TransportExpiryHost');const realNow=Date.now;let deadline;
 await runInDurableObject(stub(),(instance,state)=>{
  deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
  instance.auditOriginalFetch=instance.fetch.bind(instance);instance.auditOnce=true;
  instance.fetch=async request=>{
   const response=await instance.auditOriginalFetch(request);
   if(new URL(request.url).pathname==='/game-authority'&&instance.auditOnce){instance.auditOnce=false;Date.now=()=>deadline;}
   return response;
  };
 });
 try{
  const get=()=>exports.default.fetch(`${origin}/api/history`,{headers:{Cookie:actor.session}});
  const denied=await get();expect(denied.status).toBe(401);
  const floor=await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms);
  expect(floor).toBeGreaterThanOrEqual(deadline);
  Date.now=()=>deadline-1;const revived=await get();expect(revived.status).toBe(503);
 }finally{Date.now=realNow;await runInDurableObject(stub(),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;delete instance.auditOnce;});}
});

for (const fault of ['rollback', 'missing_ack', 'wrong_sample', 'wrong_identity', 'failed_write', 'failed_sync']) {
 test(`History initial expiry fence preserves receiving sample and fails closed: ${fault}`,async()=>{
  const actor=await enroll(`HistoryFence${fault}`); const realNow=Date.now; let deadline;
  await runInDurableObject(stub(),(instance,state)=>{
   deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
   instance.auditOriginalFetch=instance.fetch.bind(instance);
   instance.auditOriginalSync=state.storage.sync.bind(state.storage);
   instance.fetch=async request=>{
    const path=new URL(request.url).pathname;
    const parsed=await request.clone().json();
    if(path==='/game-authority'&&parsed.action==='fence_expiry') {
     instance.auditFenceSample=parsed.observed_at;
     Date.now=()=>deadline-1;
     if(fault==='failed_write') state.storage.sql.exec(`CREATE TRIGGER audit_fence BEFORE UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${deadline} BEGIN SELECT RAISE(IGNORE); END`);
     if(fault==='failed_sync') state.storage.sync=async()=>{throw new Error('audit durability failure');};
     if(fault==='missing_ack') return new Response('{}',{headers:{'Content-Type':'application/json'}});
     let response;
     try { response=await instance.auditOriginalFetch(request); } finally {state.storage.sync=instance.auditOriginalSync;}
     if(fault==='wrong_sample'||fault==='wrong_identity') {
      const body=await response.json();
      if(fault==='wrong_sample') body.outcome.observed_at--;
      else body.outcome.authority.session_id=crypto.randomUUID();
      return Response.json(body);
     }
     return response;
    }
    const response=await instance.auditOriginalFetch(request);
    if(path==='/game-authority'&&parsed.action==='authorize') Date.now=()=>deadline;
    return response;
   };
  });
  try {
   const denied=await exports.default.fetch(`${origin}/api/history`,{headers:{Cookie:actor.session}});
   expect(denied.status).toBe(fault==='rollback'?401:503);
   expect((await denied.json()).games).toBeUndefined();
   const sample=await runInDurableObject(stub(),instance=>instance.auditFenceSample);
   expect(sample).toBe(deadline);
   if(fault==='rollback') {
    const floor=await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms);
    expect(floor).toBeGreaterThanOrEqual(deadline);
    expect((await current(actor.session)).status).toBe(503);
   }
  } finally {
   Date.now=realNow;
   await runInDurableObject(stub(),(instance,state)=>{instance.fetch=instance.auditOriginalFetch;state.storage.sync=instance.auditOriginalSync;delete instance.auditOriginalFetch;delete instance.auditOriginalSync;});
  }
 });
}

for (const fault of ['rollback','missing_ack','wrong_deadline','failed_write','failed_sync']) {
 test(`auth edge output expiry never releases protected body/cookie: ${fault}`,async()=>{
  const actor=await enroll(`AuthFence${fault}`);const realNow=Date.now;let deadline;
  await runInDurableObject(stub(),(instance,state)=>{
   deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
   instance.auditOriginalFetch=instance.fetch.bind(instance);instance.auditOriginalSync=state.storage.sync.bind(state.storage);
   instance.fetch=async request=>{
    const path=new URL(request.url).pathname;const parsed=await request.clone().json();
    if(path==='/auth-output'&&parsed.observed_at!==null) {
     instance.auditFenceSample=parsed.observed_at;Date.now=()=>deadline-1;
     if(fault==='failed_write') state.storage.sql.exec(`CREATE TRIGGER audit_fence BEFORE UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${deadline} BEGIN SELECT RAISE(IGNORE); END`);
     if(fault==='failed_sync')state.storage.sync=async()=>{throw new Error('audit durability failure');};
     if(fault==='missing_ack')return Response.json({});
     let response;try {response=await instance.auditOriginalFetch(request);}finally{state.storage.sync=instance.auditOriginalSync;}
     if(fault==='wrong_deadline'){const body=await response.json();body.authority.expires_at++;return Response.json(body);}
     return response;
    }
    const response=await instance.auditOriginalFetch(request);if(path==='/auth')Date.now=()=>deadline;return response;
   };
  });
  try {
   const denied=await current(actor.session);expect(denied.status).toBe(503);
   const body=await denied.json();expect(body.account).toBeUndefined();expect(body.session).toBeUndefined();expect(denied.headers.get('Set-Cookie')).toBeNull();
   expect(await runInDurableObject(stub(),instance=>instance.auditFenceSample)).toBe(deadline);
   if(fault==='rollback') {
    expect(await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms)).toBeGreaterThanOrEqual(deadline);
    expect((await current(actor.session)).status).toBe(503);
   }
  }finally{Date.now=realNow;await runInDurableObject(stub(),(instance,state)=>{instance.fetch=instance.auditOriginalFetch;state.storage.sync=instance.auditOriginalSync;delete instance.auditOriginalFetch;delete instance.auditOriginalSync;});}
 });
}

for (const change of ['revoke','disable','epoch','lifecycle']) {
 test(`auth owner sync rechecks live eligibility: ${change}`,async()=>{
  const actor=await enroll(`OwnerChange${change}`);
  await runInDurableObject(stub(),(instance,state)=>{
   instance.auditOriginalFetch=instance.fetch.bind(instance);instance.auditOriginalSync=state.storage.sync.bind(state.storage);
   instance.fetch=async request=>{instance.auditActive=new URL(request.url).pathname==='/auth';try{return await instance.auditOriginalFetch(request);}finally{instance.auditActive=false;}};
   state.storage.sync=async()=>{
    await instance.auditOriginalSync();if(!instance.auditActive)return;
    if(change==='revoke')state.storage.sql.exec('UPDATE account_sessions SET revoked_at=? WHERE account_id=?',Date.now(),actor.accountId);
    if(change==='disable')state.storage.sql.exec('UPDATE accounts SET disabled_at=? WHERE account_id=?',Date.now(),actor.accountId);
    if(change==='epoch')state.storage.sql.exec('UPDATE accounts SET credential_epoch=credential_epoch+1 WHERE account_id=?',actor.accountId);
    if(change==='lifecycle')state.storage.sql.exec("UPDATE accounts SET status='reset_required' WHERE account_id=?",actor.accountId);
   };
  });
  try{const denied=await current(actor.session);expect(denied.status).toBe(200);const body=await denied.json();expect(body.account).toBeUndefined();expect(body.authenticated).toBe(false);expect(denied.headers.get('Set-Cookie')).toContain('Max-Age=0');}
  finally{await runInDurableObject(stub(),(instance,state)=>{instance.fetch=instance.auditOriginalFetch;state.storage.sync=instance.auditOriginalSync;delete instance.auditOriginalFetch;delete instance.auditOriginalSync;});}
 });
}

async function lobbyPlayer() {
 const actor=await enroll('SyncAuditHost');
 const created=await post('/api/games',{}, {session:actor.session});expect(created.status).toBe(200);
 const id=(await created.json()).game.game_id;
 const lobby=await post(`/api/games/${id}/lobby`,{expected_revision:0},{session:actor.session});expect(lobby.status).toBe(200);
 const code=(await lobby.json()).game_code;
 const context=await post(`/api/games/${id}/admission-context`,{game_code:code},{id:null});expect(context.status).toBe(200);
 const joined=await post(`/api/games/${id}/players`,{game_code:code,alias:'SyncAuditPlayer'},{session:cookie(context)});expect(joined.status).toBe(200);
 return {id, session:cookie(joined), owner:env.GAMES.get(env.GAMES.idFromName(id))};
}
for(const fault of ['rollback','missing_ack','wrong_sample','wrong_deadline','wrong_game','failed_write','failed_sync']) {
 test(`Sync transit expiry fences the Game before denial: ${fault}`,async()=>{
  const {id,session,owner}=await lobbyPlayer();const realNow=Date.now;let deadline;
  // Exercise the up-to-date response as well: revision equality must not bypass expiry.
  const live=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`,{headers:{Cookie:session}});
  const revision=(await live.json()).view_revision;
  await runInDurableObject(owner,(instance,state)=>{
   deadline=state.storage.sql.exec('SELECT expires_at FROM game_sessions').one().expires_at;
   state.storage.sql.exec('UPDATE game_record SET idle_due=?',deadline+5000);
   instance.auditOriginalFetch=instance.fetch.bind(instance);instance.auditOriginalSync=state.storage.sync.bind(state.storage);
   instance.fetch=async request=>{
    const parsed=await request.clone().json();
    if(parsed.expiry) {
     instance.auditFenceSample=parsed.expiry.observed_at;Date.now=()=>deadline-1;
     if(fault==='failed_write')state.storage.sql.exec(`CREATE TRIGGER audit_fence BEFORE UPDATE OF last_observed_ms ON game_metadata WHEN NEW.last_observed_ms>=${deadline} BEGIN SELECT RAISE(IGNORE); END`);
     if(fault==='failed_sync')state.storage.sync=async()=>{throw new Error('audit durability failure');};
     if(fault==='missing_ack')return Response.json({});
     let response;try{response=await instance.auditOriginalFetch(request);}finally{state.storage.sync=instance.auditOriginalSync;}
     if(fault==='wrong_sample'||fault==='wrong_deadline'||fault==='wrong_game') {
      const body=await response.json();
      if(fault==='wrong_sample')body.outcome.expiry.observed_at--;
      else if(fault==='wrong_deadline')body.outcome.expiry.deadline++;
      else body.game_id=crypto.randomUUID();
      return Response.json(body);
     }
     return response;
    }
    const response=await instance.auditOriginalFetch(request);if(parsed.operation==='sync')Date.now=()=>deadline;return response;
   };
  });
  try{
   const denied=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player&known_revision=${revision}`,{headers:{Cookie:session}});
   expect(denied.status).toBe(fault==='rollback'?401:503);
   const body=await denied.json();expect(body.snapshot).toBeUndefined();expect(body.up_to_date).toBeUndefined();
   expect(await runInDurableObject(owner,instance=>instance.auditFenceSample)).toBe(deadline);
   if(fault==='rollback') {
    expect(await runInDurableObject(owner,(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM game_metadata').one().last_observed_ms)).toBeGreaterThanOrEqual(deadline);
    const again=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`,{headers:{Cookie:session}});expect(again.status).toBe(503);
   }
  }finally{Date.now=realNow;await runInDurableObject(owner,(instance,state)=>{instance.fetch=instance.auditOriginalFetch;state.storage.sync=instance.auditOriginalSync;delete instance.auditOriginalFetch;delete instance.auditOriginalSync;});}
 });
}

test('Sync owner output gate rechecks the player session and durably denies expiry',async()=>{
 const {id,session,owner}=await lobbyPlayer();const realNow=Date.now;let deadline;
 await runInDurableObject(owner,(instance,state)=>{
  deadline=state.storage.sql.exec('SELECT expires_at FROM game_sessions').one().expires_at;
  state.storage.sql.exec('UPDATE game_record SET idle_due=?',deadline+5000);
  instance.auditOriginalFetch=instance.fetch.bind(instance);instance.auditOriginalSync=state.storage.sync.bind(state.storage);
  instance.fetch=async request=>{const parsed=await request.clone().json();instance.auditActive=parsed.operation==='sync';try{return await instance.auditOriginalFetch(request);}finally{instance.auditActive=false;}};
  state.storage.sync=async()=>{await instance.auditOriginalSync();if(instance.auditActive)Date.now=()=>deadline;};
 });
 try{
  const denied=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`,{headers:{Cookie:session}});expect(denied.status).toBe(401);expect((await denied.json()).snapshot).toBeUndefined();
  expect(await runInDurableObject(owner,(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM game_metadata').one().last_observed_ms)).toBeGreaterThanOrEqual(deadline);
  Date.now=()=>deadline-1;
  const again=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`,{headers:{Cookie:session}});expect(again.status).toBe(503);
 }finally{Date.now=realNow;await runInDurableObject(owner,(instance,state)=>{instance.fetch=instance.auditOriginalFetch;state.storage.sync=instance.auditOriginalSync;delete instance.auditOriginalFetch;delete instance.auditOriginalSync;});}
});

for(const shape of ['response_array','body_array','error_array','cookie_array','cookie_unknown','nested_session_array','nested_account_array']) {
 test(`private auth response rejects positional/nonobject structures: ${shape}`,async()=>{
  await runInDurableObject(stub(),instance=>{
   instance.auditOriginalFetch=instance.fetch.bind(instance);
   instance.fetch=async request=>{
    const response=await instance.auditOriginalFetch(request);
    if(new URL(request.url).pathname!=='/auth')return response;
    let body=await response.json();
    if(shape==='response_array')body=[body.status,body.body,body.cookie,body.authority];
    if(shape==='body_array')body.body=[false,null];
    if(shape==='error_array')body.body={error:['unauthorized','authentication required']};
    if(shape==='cookie_array')body.cookie=['none'];
    if(shape==='cookie_unknown')body.cookie={type:'none',extra:true};
    if(shape==='nested_session_array')body.body={authenticated:true,session:['Account','normal',1,crypto.randomUUID(),'host']};
    if(shape==='nested_account_array')body.body={account:[crypto.randomUUID(),'Fixture','host','verified',false,1]};
    return Response.json(body);
   };
  });
  try{const response=await current();expect(response.status).toBe(503);expect(response.headers.get('Set-Cookie')).toBeNull();}
  finally{await runInDurableObject(stub(),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
 });
}

for(const fault of ['rollback','missing_ack','wrong_identity','wrong_deadline','failed_write']) {
 test(`Users issuance transit expiry requires exact durable ACK: ${fault}`,async()=>{
  const actor=await enroll(`UsersFence${fault}`);const realNow=Date.now;let deadline;const id=commandId();
  await runInDurableObject(stub(),(instance,state)=>{
   state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?",actor.accountId);
   deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
   instance.auditOriginalFetch=instance.fetch.bind(instance);
   instance.fetch=async request=>{
    const path=new URL(request.url).pathname;const parsed=await request.clone().json();
    if(path==='/game-authority'&&parsed.action==='fence_expiry') {
     instance.auditFenceSample=parsed.observed_at;Date.now=()=>deadline-1;
     if(fault==='failed_write')state.storage.sql.exec(`CREATE TRIGGER audit_fence BEFORE UPDATE OF last_observed_ms ON storage_metadata WHEN NEW.last_observed_ms>=${deadline} BEGIN SELECT RAISE(IGNORE); END`);
     if(fault==='missing_ack')return Response.json({});
     const response=await instance.auditOriginalFetch(request);
     if(fault==='wrong_identity'||fault==='wrong_deadline') {
      const body=await response.json();
      if(fault==='wrong_identity')body.outcome.authority.account_id=crypto.randomUUID();
      else body.outcome.authority.expires++;
      return Response.json(body);
     }
     return response;
    }
    const response=await instance.auditOriginalFetch(request);if(path==='/users')Date.now=()=>deadline;return response;
   };
  });
  try{
   const denied=await post('/api/users/hosts',{username:'UsersIssuedHost'},{session:actor.session,id});expect(denied.status).toBe(503);
   const body=await denied.json();expect(body.enrollment_url).toBeUndefined();expect(body.account).toBeUndefined();expect(denied.headers.get('Set-Cookie')).toBeNull();
   expect(await runInDurableObject(stub(),instance=>instance.auditFenceSample)).toBe(deadline);
   if(fault==='rollback') {
    expect(await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms)).toBeGreaterThanOrEqual(deadline);
    expect((await current(actor.session)).status).toBe(503);
   }
  }finally{Date.now=realNow;await runInDurableObject(stub(),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
 });
}

test('Users owner sync expiry suppresses enrollment issuance and durably fences rollback',async()=>{
 const actor=await enroll('UsersOwnerExpiry');const realNow=Date.now;let deadline;
 await runInDurableObject(stub(),(instance,state)=>{
  state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?",actor.accountId);
  deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
  instance.auditOriginalFetch=instance.fetch.bind(instance);instance.auditOriginalSync=state.storage.sync.bind(state.storage);
  instance.fetch=async request=>{instance.auditActive=new URL(request.url).pathname==='/users';try{return await instance.auditOriginalFetch(request);}finally{instance.auditActive=false;}};
  state.storage.sync=async()=>{await instance.auditOriginalSync();if(instance.auditActive)Date.now=()=>deadline;};
 });
 try{
  const denied=await post('/api/users/hosts',{username:'UsersOwnerCreated'},{session:actor.session});expect(denied.status).toBe(401);expect((await denied.json()).enrollment_url).toBeUndefined();
  expect(await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms)).toBeGreaterThanOrEqual(deadline);
  Date.now=()=>deadline-1;expect((await current(actor.session)).status).toBe(503);
 }finally{Date.now=realNow;await runInDurableObject(stub(),(instance,state)=>{instance.fetch=instance.auditOriginalFetch;state.storage.sync=instance.auditOriginalSync;delete instance.auditOriginalFetch;delete instance.auditOriginalSync;});}
});

test('Sync with a missing transport deadline cannot release a snapshot',async()=>{
 const {id,session,owner}=await lobbyPlayer();
 await runInDurableObject(owner,instance=>{
  instance.auditOriginalFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{
   const parsed=await request.clone().json();const response=await instance.auditOriginalFetch(request);
   if(parsed.operation!=='sync')return response;
   const body=await response.json();delete body.valid_until;return Response.json(body);
  };
 });
 try{const denied=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`,{headers:{Cookie:session}});expect(denied.status).toBe(503);expect((await denied.json()).snapshot).toBeUndefined();}
 finally{await runInDurableObject(owner,instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
});

test('private auth cannot extend Set-Cookie expiry or replay the withheld cookie',async()=>{
 const actor=await enroll('AuthCookieBinding');const id=commandId();
 await runInDurableObject(stub(),instance=>{
  instance.auditOriginalFetch=instance.fetch.bind(instance);
  instance.fetch=async request=>{
   const response=await instance.auditOriginalFetch(request);if(new URL(request.url).pathname!=='/auth')return response;
   const body=await response.json();if(body.cookie.type==='set')body.cookie.expires_at++;return Response.json(body);
  };
 });
 try{
  const denied=await post('/api/auth/login',{username:actor.username,password:actor.password},{id});expect(denied.status).toBe(503);expect(denied.headers.get('Set-Cookie')).toBeNull();
  const replay=await post('/api/auth/login',{username:actor.username,password:actor.password},{id});expect(replay.status).toBe(200);expect(replay.headers.get('Set-Cookie')).toBeNull();expect((await replay.json()).replayed).toBe(true);
 }finally{await runInDurableObject(stub(),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
});

test('captured player closures remain fenced through activity without early physical retries',async()=>{
 const host=await enroll('TransportRetryHost');const answer=password();
 const created=await post('/api/games',{configuration:{numeric_upper_bound:4,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],player_capacity:2,spectator_capacity:0}},{session:host.session});expect(created.status).toBe(200);
 const id=(await created.json()).game.game_id;
 const lobby=await post(`/api/games/${id}/lobby`,{expected_revision:0},{session:host.session});expect(lobby.status).toBe(200);const code=(await lobby.json()).game_code;
 const context=await post(`/api/games/${id}/admission-context`,{game_code:code},{id:null});
 const joined=await post(`/api/games/${id}/players`,{game_code:code,alias:'RetryPlayer',recovery_answer:answer},{session:cookie(context)});expect(joined.status).toBe(200);
 const owner=env.GAMES.get(env.GAMES.idFromName(id));const realNow=Date.now;let first;let successor;
 async function stream(session) {
  const response=await exports.default.fetch(`${origin}/api/games/${id}/stream?view=player`,{headers:{Cookie:session,Origin:origin,Upgrade:'websocket',Connection:'Upgrade'}});expect(response.status).toBe(101);
  const socket=response.webSocket;
  const initial=new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(new Error('audit snapshot timeout')),3000);socket.addEventListener('message',event=>{clearTimeout(timeout);resolve(JSON.parse(event.data));},{once:true});});
  socket.addEventListener('message',event=>{const frame=JSON.parse(event.data);socket.send(JSON.stringify({version:1,kind:'snapshot_ack',connection_id:frame.connection_id,delivery_id:frame.delivery_id,view_revision:frame.view_revision}));});
  socket.accept();return {socket,initial:await initial};
 }
 try {
  first=await stream(cookie(joined));
  await runInDurableObject(owner,(instance,state)=>{
   instance.auditCloseCalls=0;
   for(const ws of state.getWebSockets())if(JSON.parse(ws.deserializeAttachment()).connection_id===first.initial.connection_id){ws.auditOriginalClose=ws.close;ws.close=()=>{instance.auditCloseCalls++;throw new Error('audit provider close failure');};}
  });
  const recovered=await post('/api/games/player-recovery',{game_code:code,alias:'RetryPlayer',recovery_answer:answer});expect(recovered.status).toBe(200);
  const before=await runInDurableObject(owner,(instance,state)=>({calls:instance.auditCloseCalls,row:state.storage.sql.exec('SELECT * FROM game_socket_closures WHERE connection_id=?',first.initial.connection_id).one()}));
  expect(before.row.attempt_count).toBe(1);expect(before.calls).toBe(1);
  Date.now=()=>before.row.next_attempt_at-1;
  successor=await stream(cookie(recovered));
  await runInDurableObject(owner,instance=>instance.alarm());
  const early=await runInDurableObject(owner,(instance,state)=>({calls:instance.auditCloseCalls,row:state.storage.sql.exec('SELECT * FROM game_socket_closures WHERE connection_id=?',first.initial.connection_id).one()}));
  expect(early).toEqual(before);
  Date.now=()=>before.row.next_attempt_at;
  await runInDurableObject(owner,instance=>instance.alarm());
  const due=await runInDurableObject(owner,(instance,state)=>({calls:instance.auditCloseCalls,row:state.storage.sql.exec('SELECT * FROM game_socket_closures WHERE connection_id=?',first.initial.connection_id).one()}));
  expect(due.calls).toBe(before.calls+1);expect(due.row.attempt_count).toBe(2);expect(due.row.next_attempt_at).toBe(before.row.next_attempt_at+4000);
 } finally {
  Date.now=realNow;
  await runInDurableObject(owner,(_instance,state)=>{for(const ws of state.getWebSockets())if(ws.auditOriginalClose){ws.close=ws.auditOriginalClose;delete ws.auditOriginalClose;}});
  first?.socket.close(1000);successor?.socket.close(1000);
 }
});

for(const fault of ['rollback','wrong_ack']) {
 test(`account Sync preserves the first receiving expiry across denial awaits: ${fault}`,async()=>{
  const host=await enroll(`SyncAccount${fault}`);const created=await post('/api/games',{}, {session:host.session});expect(created.status).toBe(200);const id=(await created.json()).game.game_id;
  const owner=env.GAMES.get(env.GAMES.idFromName(id));const realNow=Date.now;let deadline;
  await runInDurableObject(stub(),(instance,state)=>{
   deadline=state.storage.sql.exec('SELECT expires_at FROM account_sessions WHERE revoked_at IS NULL').one().expires_at;
   instance.auditOriginalFetch=instance.fetch.bind(instance);
   instance.fetch=async request=>{
    const parsed=await request.clone().json();
    if(new URL(request.url).pathname==='/game-authority'&&parsed.action==='fence_expiry') {
     instance.auditFenceSample=parsed.observed_at;
     if(fault==='wrong_ack')return Response.json({});
    }
    return instance.auditOriginalFetch(request);
   };
  });
  await runInDurableObject(owner,(instance,state)=>{
   state.storage.sql.exec('UPDATE game_record SET idle_due=?',deadline+5000);
   instance.auditOriginalFetch=instance.fetch.bind(instance);
   instance.fetch=async request=>{
    const parsed=await request.clone().json();
    if(parsed.expiry)Date.now=()=>deadline-1;
    const response=await instance.auditOriginalFetch(request);if(parsed.operation==='sync')Date.now=()=>deadline;return response;
   };
  });
  try{
   const denied=await exports.default.fetch(`${origin}/api/games/${id}/sync?view=account`,{headers:{Cookie:host.session}});expect(denied.status).toBe(fault==='rollback'?401:503);expect((await denied.json()).snapshot).toBeUndefined();
   expect(await runInDurableObject(stub(),instance=>instance.auditFenceSample)).toBe(deadline);
   expect(await runInDurableObject(owner,(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM game_metadata').one().last_observed_ms)).toBeGreaterThanOrEqual(deadline);
   if(fault==='rollback') {
    expect(await runInDurableObject(stub(),(_instance,state)=>state.storage.sql.exec('SELECT last_observed_ms FROM storage_metadata').one().last_observed_ms)).toBeGreaterThanOrEqual(deadline);
    expect((await current(host.session)).status).toBe(503);
   }
  }finally{Date.now=realNow;await runInDurableObject(owner,instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});await runInDurableObject(stub(),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
 });
}
