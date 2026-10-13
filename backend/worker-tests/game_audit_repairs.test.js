import {beforeEach,expect,test} from 'vitest';
import {env,exports} from 'cloudflare:workers';
import {reset,runInDurableObject,evictDurableObject} from 'cloudflare:test';
import {origin,enroll,post,resetStorage,inOwner,commandId,stub} from './fixtures.js';
const realNow=Date.now;
const owner=id=>env.GAMES.get(env.GAMES.idFromName(id));
const directory=()=>env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName('directory'));
beforeEach(async()=>{Date.now=realNow;await reset();await resetStorage();await runInDurableObject(directory(),(_,s)=>s.storage.deleteAll());});
async function fixture(){const host=await enroll('IndependentHost'),admin=await enroll('IndependentAdmin');await inOwner((_,s)=>s.storage.sql.exec('UPDATE accounts SET role=? WHERE account_id=?','admin',admin.accountId));const response=await post('/api/games',{}, {session:host.session});expect(response.status).toBe(200);return {host,admin,id:(await response.json()).game.game_id};}
async function rows(f){return runInDurableObject(owner(f.id),(_,s)=>({audits:s.storage.sql.exec('SELECT * FROM game_admin_audit ORDER BY audit_id').toArray(),record:s.storage.sql.exec('SELECT * FROM game_record').toArray(),receipts:s.storage.sql.exec('SELECT * FROM game_receipts').toArray(),pending:s.storage.sql.exec('SELECT * FROM game_pending_work').toArray()}));}
async function cli(f,body){return exports.default.fetch(`${origin}/_dev/commands`,{method:'POST',headers:{Authorization:`Bearer ${env.DEV_CLI_KEY}`,'Content-Type':'application/json'},body:body??JSON.stringify({operation:'list_game_audit',game_id:f.id,after:null,limit:100})});}
async function cancel(f,id=commandId()){return post(`/api/games/${f.id}/cancel`,{expected_state:'new',confirmed:true},{session:f.admin.session,id});}
async function intercept(target,fn){await runInDurableObject(target,instance=>{instance.independentOriginal=instance.fetch.bind(instance);instance.fetch=request=>fn(request,instance.independentOriginal);});}
async function restore(target){await runInDurableObject(target,instance=>{instance.fetch=instance.independentOriginal;delete instance.independentOriginal;});}

for(const fault of ['missing-game','native-registry','storage-sync','register-authority'])test(`independent actual GET account stream ${fault} durably captures outcome`,async()=>{
 const f=await fixture();if(fault==='missing-game')f.id=commandId();
 if(fault==='native-registry')await runInDurableObject(owner(f.id),(instance,s)=>{instance.independentRegistry=s.getWebSockets.bind(s);s.getWebSockets=()=>{throw new Error('independent stream registry fault');};});
 if(fault==='storage-sync')await runInDurableObject(owner(f.id),(instance,s)=>{instance.independentSync=s.storage.sync.bind(s.storage);let once=false;s.storage.sync=async()=>{if(!once){once=true;throw new Error('independent stream durability fault');}await instance.independentSync();};});
 if(fault==='register-authority'){let once=false;await intercept(stub(),async(request,original)=>{const body=await request.clone().json();if(new URL(request.url).pathname==='/game-authority'&&body.action==='register'&&!once){once=true;throw new Error('independent stream registration transport fault');}return original(request);});}
 try{const response=await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=account`,{headers:{Cookie:f.admin.session,Origin:origin,Upgrade:'websocket',Connection:'Upgrade'}});expect(response.status).toBe(fault==='missing-game'?404:503);const audits=(await rows(f)).audits;expect(audits.some(a=>a.actor_account_id===f.admin.accountId&&a.operation==='account_stream'&&a.outcome===(fault==='missing-game'?'rejected':'failed'))).toBe(true);}finally{
 if(fault==='native-registry')await runInDurableObject(owner(f.id),(instance,s)=>{s.getWebSockets=instance.independentRegistry;delete instance.independentRegistry;});
 if(fault==='storage-sync')await runInDurableObject(owner(f.id),(instance,s)=>{s.storage.sync=instance.independentSync;delete instance.independentSync;});
 if(fault==='register-authority')await restore(stub());
 }
});

for(const kind of ['history','discovery'])test(`independent already-identified Admin ${kind} receiving-edge Accounts recut failure retains Failed audit`,async()=>{
 const f=await fixture();let hydrated=false,failed=false;
 await intercept(owner(f.id),async(request,original)=>{const response=await original(request);if(new URL(request.url).pathname===`/${kind}`)hydrated=true;return response;});
 await intercept(stub(),async(request,original)=>{if(new URL(request.url).pathname==='/game-authority'&&hydrated&&!failed){failed=true;throw new Error('independent receiving edge authority transport fault');}return original(request);});
 try{const path=kind==='history'?`/api/history/${f.id}`:`/api/games/${f.id}/availability?view=account`;expect((await exports.default.fetch(`${origin}${path}`,{headers:{Cookie:f.admin.session}})).status).toBe(503);expect(failed).toBe(true);const audits=(await rows(f)).audits;expect(audits.some(a=>a.actor_account_id===f.admin.accountId&&a.operation===`${kind}_read`&&a.outcome==='failed')).toBe(true);}finally{await restore(stub());await restore(owner(f.id));}
});

for(const operation of ['configuration','resume','game-read'])test(`independent existing owner ${operation} transient Accounts failure gets freshly captured`,async()=>{
 const f=await fixture();let failed=false;await intercept(stub(),async(request,original)=>{if(new URL(request.url).pathname==='/operator-accounts'&&!failed){failed=true;throw new Error('independent existing owner authority fault');}return original(request);});
 try{const path=operation==='game-read'?`/api/games/${f.id}?view=account`:`/api/games/${f.id}/${operation}`;const response=operation==='game-read'?await exports.default.fetch(`${origin}${path}`,{headers:{Cookie:f.admin.session}}):await exports.default.fetch(`${origin}${path}`,{method:operation==='configuration'?'PATCH':'POST',headers:{Cookie:f.admin.session,Origin:origin,'Content-Type':'application/json','Idempotency-Key':commandId()},body:operation==='configuration'?'{"expected_revision":0}':'{}'});expect(response.status).toBe(503);expect(failed).toBe(true);expect((await rows(f)).audits.some(a=>a.actor_account_id===f.admin.accountId&&a.operation===(operation==='game-read'?'game_read':operation)&&a.outcome==='failed')).toBe(true);}finally{await restore(stub());}
});

for(const kind of ['history','discovery'])test(`independent pre-owner transient Accounts failure for targeted ${kind} is freshly capturable`,async()=>{
 const f=await fixture();let failed=false;
 await intercept(stub(),async(request,original)=>{if(new URL(request.url).pathname==='/game-authority'&&!failed){failed=true;throw new Error('independent transient authority transport fault');}return original(request);});
 try{const path=kind==='history'?`/api/history/${f.id}`:`/api/games/${f.id}/availability?view=account`;expect((await exports.default.fetch(`${origin}${path}`,{headers:{Cookie:f.admin.session}})).status).toBe(503);expect(failed).toBe(true);const audits=(await rows(f)).audits;expect(audits).toHaveLength(1);expect(audits[0]).toMatchObject({actor_account_id:f.admin.accountId,operation:`${kind}_read`,outcome:'failed'});}finally{await restore(stub());}
});


for(const fault of ['future-occurrence','negative-clock','nan-clock','unsafe-clock'])test(`independent edge audit observed clock rejects ${fault}`,async()=>{
 const f=await fixture();expect((await cancel(f)).status).toBe(200);
 await intercept(owner(f.id),async(request,original)=>{const reply=await original(request);if(new URL(request.url).pathname!=='/developer-audit')return reply;const value=await reply.json();if(fault==='future-occurrence'){value.events[0].occurred_at=realNow()+60000;value.events[0].expires_at=value.events[0].occurred_at+7776000000;}else Date.now=()=>fault==='negative-clock'?-1:fault==='nan-clock'?NaN:9007199254740992;return Response.json(value);});
 try{const response=await cli(f);expect(response.status).toBe(503);}finally{Date.now=realNow;await restore(owner(f.id));}
});


for (const fault of ['negative-clock', 'nan-clock', 'unsafe-clock']) test(`empty audit page rejects ${fault} at receiving edge`, async () => {
 const f = await fixture();
 await intercept(owner(f.id), async (request, original) => {
  const reply = await original(request);
  if (new URL(request.url).pathname !== '/developer-audit') return reply;
  const value = await reply.json(); expect(value.events).toEqual([]);
  Date.now = () => fault === 'negative-clock' ? -1 : fault === 'nan-clock' ? NaN : 9007199254740992;
  return Response.json(value);
 });
 try { expect((await cli(f)).status).toBe(503); }
 finally { Date.now = realNow; await restore(owner(f.id)); }
});

test('two private discovery invocations retain exactly one logical Admin Success', async () => {
 const f = await fixture(); let phases = [];
 await intercept(owner(f.id), async (request, original) => {
  if (new URL(request.url).pathname === '/discovery') phases.push((await request.clone().json()).phase);
  return original(request);
 });
 try {
  const response = await exports.default.fetch(`${origin}/api/games/current`, {headers: {Cookie: f.admin.session}});
  expect(response.status).toBe(200); expect(phases).toEqual(['primary', 'recut']);
  const audits = (await rows(f)).audits;
  expect(audits).toHaveLength(1);
  expect(audits[0]).toMatchObject({actor_account_id: f.admin.accountId, target_game_id: f.id, operation: 'discovery_read', outcome: 'succeeded'});
 } finally { await restore(owner(f.id)); }
 const rejected = await exports.default.fetch(`${origin}/api/games/current?phase=recut`, {headers: {Cookie: f.admin.session}});
 expect(rejected.status).toBe(400);
});

for (const fault of ['IGNORE', 'ABORT', 'sdk', 'sdk-after-accept']) test(`account successor ${fault} failure fences its native socket and retains predecessor`, async () => {
 const f = await fixture();
 const stream = () => exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=account`, {headers: {Cookie: f.admin.session, Origin: origin, Upgrade: 'websocket', Connection: 'Upgrade'}});
 const first = await stream(); expect(first.status).toBe(101);
 const client = first.webSocket; client.accept();
 const before = await runInDurableObject(owner(f.id), (_, state) => state.storage.sql.exec('SELECT * FROM game_current_connections').toArray());
 if (fault.startsWith('sdk')) await runInDurableObject(owner(f.id), (instance, state) => {
  instance.repairAccept = state.acceptWebSocket.bind(state); let once = true;
  state.acceptWebSocket = socket => { if (once) { once = false; if (fault === 'sdk-after-accept') instance.repairAccept(socket); throw new Error('controlled one-shot native accept failure'); } return instance.repairAccept(socket); };
 });
 else await runInDurableObject(owner(f.id), (_, state) => state.storage.sql.exec(`CREATE TRIGGER deny_stream_success BEFORE INSERT ON game_admin_audit WHEN NEW.operation='account_stream' AND NEW.outcome='succeeded' BEGIN SELECT RAISE(${fault === 'ABORT' ? "ABORT, 'controlled stream audit fault'" : fault}); END`));
 try {
  const response = await stream(); expect(response.status).toBe(503);
  const after = await runInDurableObject(owner(f.id), (_, state) => ({current: state.storage.sql.exec('SELECT * FROM game_current_connections').toArray(), native: state.getWebSockets().map(socket => ({connection: socket.deserializeAttachment().connection_id, state: socket.readyState}))}));
  expect(after.current).toEqual(before);
  expect(after.native.filter(socket => socket.state === 1)).toHaveLength(1);
  expect(client.readyState).toBe(1);
  const audits = (await rows(f)).audits.filter(event => event.operation === 'account_stream');
  expect(audits.filter(event => event.outcome === 'succeeded')).toHaveLength(1);
  expect(audits.filter(event => event.outcome === 'failed')).toHaveLength(1);
  expect(audits.every(event => event.actor_account_id === f.admin.accountId && event.target_game_id === f.id && event.expires_at === event.occurred_at + 7776000000)).toBe(true);
 } finally {
  await runInDurableObject(owner(f.id), (instance, state) => {
   if (fault.startsWith('sdk')) { state.acceptWebSocket = instance.repairAccept; delete instance.repairAccept; }
   else state.storage.sql.exec('DROP TRIGGER deny_stream_success');
  });
  client.close(1000);
 }
});

test('account Stream one-shot registry error records one fresh Failed and no Success', async () => {
 const f = await fixture();
 await runInDurableObject(owner(f.id), (instance, state) => {
  instance.repairRegistry = state.getWebSockets.bind(state); let once = true;
  state.getWebSockets = () => { if (once) { once = false; throw new Error('controlled one-shot registry fault'); } return instance.repairRegistry(); };
 });
 try {
  const response = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=account`, {headers: {Cookie: f.admin.session, Origin: origin, Upgrade: 'websocket', Connection: 'Upgrade'}});
  expect(response.status).toBe(503);
  const audits = (await rows(f)).audits;
  expect(audits).toHaveLength(1);
  expect(audits[0]).toMatchObject({actor_account_id: f.admin.accountId, target_game_id: f.id, operation: 'account_stream', outcome: 'failed'});
 } finally { await runInDurableObject(owner(f.id), (instance, state) => { state.getWebSockets = instance.repairRegistry; delete instance.repairRegistry; }); }
});

for (const kind of ['history', 'discovery']) test(`targeted ${kind} transient authority fault never recreates a disabled Admin`, async () => {
 const f = await fixture(); let once = true;
 await intercept(stub(), async (request, original) => {
  if (new URL(request.url).pathname === '/game-authority' && once) {
   once = false;
   await inOwner((_, state) => state.storage.sql.exec('UPDATE accounts SET disabled_at=? WHERE account_id=?', Date.now(), f.admin.accountId));
   throw new Error('controlled authority fault after disable');
  }
  return original(request);
 });
 try {
  const path = kind === 'history' ? `/api/history/${f.id}` : `/api/games/${f.id}/availability?view=account`;
  const response = await exports.default.fetch(`${origin}${path}`, {headers: {Cookie: f.admin.session}});
  expect(response.status).toBe(503); expect(once).toBe(false);
  expect((await rows(f)).audits).toEqual([]);
 } finally { await restore(stub()); }
});

test('discovery continuation preserves independent authenticated denials with one verified Success', async () => {
 const f = await fixture();
 const response = await exports.default.fetch(`${origin}/api/games/${f.id}/availability?view=account`, {headers: {Cookie: f.admin.session}});
 expect(response.status).toBe(404);
 const audits = (await rows(f)).audits.filter(event => event.operation === 'discovery_read');
 expect(audits.filter(event => event.outcome === 'succeeded')).toHaveLength(1);
 expect(audits.filter(event => event.outcome === 'rejected')).toHaveLength(2);
 expect(audits.filter(event => event.outcome === 'failed')).toHaveLength(0);
 expect(audits.every(event => event.actor_account_id === f.admin.accountId && event.target_game_id === f.id)).toBe(true);
});

test('missing History Admin rejection is durable and scheduled through audit-only reconstruction', async () => {
 const f = await fixture(); const missing = commandId();
 const response = await exports.default.fetch(`${origin}/api/history/${missing}`, {headers: {Cookie: f.admin.session}});
 expect(response.status).toBe(404);
 const retained = await runInDurableObject(owner(missing), async (_, state) => ({audit: state.storage.sql.exec('SELECT * FROM game_admin_audit').one(), alarm: await state.storage.getAlarm(), games: state.storage.sql.exec('SELECT count(*) AS count FROM game_record').one().count}));
 expect(retained.games).toBe(0);
 expect(retained.audit).toMatchObject({actor_account_id: f.admin.accountId, target_game_id: missing, operation: 'history_read', outcome: 'rejected'});
 expect(retained.alarm).toBe(retained.audit.expires_at);
 Date.now = () => retained.audit.expires_at;
 try {
  await evictDurableObject(owner(missing));
  await runInDurableObject(owner(missing), instance => instance.alarm());
  expect(await runInDurableObject(owner(missing), (_, state) => state.storage.sql.exec('SELECT count(*) AS count FROM game_admin_audit').one().count)).toBe(0);
 } finally { Date.now = realNow; }
});

for (const role of ['host', 'admin']) for (const acknowledge of [false, true]) {
 test(`sequential ${role} account replacement preserves successor and frame delivery ACK=${acknowledge}`, async () => {
  const f = await fixture(); const clients = []; const queues = []; const readers = [];
  const session = f[role].session;
  const next = index => queues[index].length ? Promise.resolve(queues[index].shift()) : new Promise((resolve, reject) => {
   const timer = setTimeout(() => reject(new Error('replacement snapshot timeout')), 3000);
   readers[index].push(frame => { clearTimeout(timer); resolve(frame); });
  });
  try {
   for (let index = 0; index < 2; index++) {
    const response = await exports.default.fetch(`${origin}/api/games/${f.id}/stream?view=account`, {
     headers: {Cookie: session, Origin: origin, Upgrade: 'websocket', Connection: 'Upgrade'},
    });
    expect(response.status).toBe(101);
    const client = response.webSocket; clients.push(client); queues.push([]); readers.push([]);
    client.addEventListener('message', event => {
     const frame = JSON.parse(event.data);
     if (acknowledge) client.send(JSON.stringify({version: 1, kind: 'snapshot_ack', connection_id: frame.connection_id, delivery_id: frame.delivery_id, view_revision: frame.view_revision}));
     if (readers[index].length) readers[index].shift()(frame); else queues[index].push(frame);
    });
    client.accept(); const frame = await next(index);
    if (acknowledge) await expect.poll(() => runInDurableObject(owner(f.id), (_, state) => state.storage.sql.exec('SELECT count(*) AS count FROM game_delivery_pending WHERE connection_id=?', frame.connection_id).one().count), {timeout: 5000}).toBe(0);
    expect(await runInDurableObject(owner(f.id), (_, state) => state.storage.sql.exec('SELECT count(*) AS count FROM game_current_connections').one().count)).toBe(1);
   }
   for (let revision = 0; revision < 3; revision++) {
    const response = await exports.default.fetch(`${origin}/api/games/${f.id}/configuration`, {
     method: 'PATCH', headers: {Cookie: f.host.session, Origin: origin, 'Content-Type': 'application/json', 'Idempotency-Key': commandId()},
     body: JSON.stringify({expected_revision: revision, player_capacity: 10 + revision}),
    });
    expect(response.status).toBe(200);
    expect((await next(1)).view_revision).toBe(revision + 1);
   }
   const observed = await runInDurableObject(owner(f.id), (_, state) => ({
    current: state.storage.sql.exec('SELECT count(*) AS count FROM game_current_connections').one().count,
    active: state.getWebSockets().filter(socket => socket.readyState === 1).length,
    audit: state.storage.sql.exec('SELECT actor_account_id,operation,outcome FROM game_admin_audit').toArray(),
   }));
   expect(observed.current).toBe(1); expect(observed.active).toBe(1);
   expect(observed.audit).toHaveLength(role === 'admin' ? 2 : 0);
   expect(observed.audit.every(event => event.actor_account_id === f.admin.accountId && event.operation === 'account_stream' && event.outcome === 'succeeded')).toBe(true);
  } finally { clients.forEach(client => client.close(1000)); }
 });
}
