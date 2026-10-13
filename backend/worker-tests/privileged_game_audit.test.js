import {beforeEach,expect,test} from 'vitest';
import {env,exports} from 'cloudflare:workers';
import {reset,runInDurableObject,evictDurableObject} from 'cloudflare:test';
import {origin,enroll,post,resetStorage,inOwner,commandId} from './fixtures.js';
import publishedCommandSchema from '../tests/fixtures/game-schema-published-command.json';
const realNow=Date.now;
const owner=id=>env.GAMES.get(env.GAMES.idFromName(id));
const directory=()=>env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName('directory'));
beforeEach(async()=>{Date.now=realNow;await reset();await resetStorage();await runInDurableObject(directory(),(_,s)=>s.storage.deleteAll());});
async function actor(name,admin=false){const actor=await enroll(name);if(admin)await inOwner((_,s)=>s.storage.sql.exec('UPDATE accounts SET role=? WHERE account_id=?','admin',actor.accountId));return actor;}
async function fixture(suffix=''){const host=await actor('GameAuditHost'+suffix),admin=await actor('GameAuditAdmin'+suffix,true);const response=await post('/api/games',{}, {session:host.session});expect(response.status).toBe(200);const id=(await response.json()).game.game_id;return {host,admin,id};}
async function rows(f){return runInDurableObject(owner(f.id),(_,s)=>({audits:s.storage.sql.exec('SELECT * FROM game_admin_audit ORDER BY audit_id').toArray(),record:s.storage.sql.exec('SELECT * FROM game_record').toArray(),receipts:s.storage.sql.exec('SELECT * FROM game_receipts ORDER BY actor,command_id').toArray(),grants:s.storage.sql.exec('SELECT * FROM game_connection_grants ORDER BY session_id').toArray()}));}
async function cli(id,extra={},headers={}){return exports.default.fetch(`${origin}/_dev/commands`,{method:'POST',headers:{Authorization:`Bearer ${env.DEV_CLI_KEY}`,'Content-Type':'application/json',...headers},body:JSON.stringify({operation:'list_game_audit',game_id:id,after:null,limit:100,...extra})});}
async function adminRequest(f,path,body,id=commandId()){return post(`/api/games/${f.id}/${path}`,body,{session:f.admin.session,id});}

test('Game audit CLI exact owner paging typed DeveloperCli bounded private records and no activity/grant renewal',async()=>{
 const f=await fixture();const command=commandId();
 let response=await adminRequest(f,'cancel',{expected_state:'new',confirmed:true},command);expect(response.status).toBe(200);
 response=await adminRequest(f,'cancel',{expected_state:'new',confirmed:true},command);expect(response.status).toBe(200);
 let audits=(await rows(f)).audits;expect(audits.filter(a=>a.operation==='cancel'&&a.outcome==='succeeded')).toHaveLength(1);expect(audits[0].actor_account_id).toBe(f.admin.accountId);
 const before=await rows(f);response=await cli(f.id);expect(response.status).toBe(200);expect(response.headers.get('Set-Cookie')).toBeNull();const result=await response.json();expect(result.result).toBe('game_audit');expect(result.game_id).toBe(f.id);expect(result.events).toHaveLength(1);expect(result.events[0].actor).toEqual({account:f.admin.accountId});expect(result.events[0].expires_at-result.events[0].occurred_at).toBe(7776000000);
 expect(Object.keys(result.events[0]).sort()).toEqual(['actor','audit_id','expires_at','occurred_at','operation','outcome','target_game_id']);
 const after=await rows(f);for(const field of ['record','receipts','grants'])expect(after[field]).toEqual(before[field]);expect(after.audits.find(a=>a.operation==='list_game_audit').actor_account_id).toBe('developer_cli');
 const limited=await cli(f.id,{limit:1});expect(limited.status).toBe(200);const page=await limited.json();expect(page.events).toHaveLength(1);expect(page.next_cursor).toBe(page.events[0].audit_id);const next=await (await cli(f.id,{after:page.next_cursor,limit:1})).json();expect(next.events[0].audit_id>page.next_cursor).toBe(true);
 for(const extra of [{limit:0},{limit:101},{after:commandId().toUpperCase()},{game_id:commandId().toUpperCase()},{limit:'1'},{actor:'developer_cli'}])expect((await cli(f.id,extra)).status).toBe(400);
 for(const headers of [{Cookie:f.admin.session},{Origin:origin},{Authorization:'Bearer invalid'}])expect((await cli(f.id,{},headers)).status).toBe(403);
 expect((await exports.default.fetch(`${origin}/api/games/${f.id}/audit`,{headers:{Cookie:f.admin.session}})).status).toBe(404);
 expect((await exports.default.fetch(`${origin}/_dev/commands`,{method:'POST',headers:{Authorization:`Bearer ${env.DEV_CLI_KEY}`,'Content-Type':'application/json'},body:JSON.stringify({operation:'list_audit',after:null,limit:100})})).status).toBe(200);
});

test('older malformed authenticated operations capture minimal rejection; anonymous host and selected player traffic excluded',async()=>{
 const f=await fixture();
 for(const [path,body] of [['lobby',{expected_revision:null}],['start',{expected_revision:null}],['calls/random',{expected_revision:null}],['calls/manual',{value:'submitted-private',expected_revision:null}],['winner',{player_id:'submitted-private',expected_revision:0}],['cancel',{expected_state:'new',confirmed:'submitted-private'}]]){
  expect((await adminRequest(f,path,body)).status).toBe(400);
  expect((await post(`/api/games/${f.id}/${path}`,body,{session:f.host.session})).status).toBe(400);
  expect((await post(`/api/games/${f.id}/${path}`,body)).status).toBe(400);
 }
 expect((await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=player&unknown=1`,{headers:{Cookie:f.admin.session}})).status).toBe(400);
 expect((await exports.default.fetch(`${origin}/api/games/${f.id}/start`,{method:'DELETE',headers:{Cookie:f.admin.session,Origin:origin}})).status).toBe(405);
 expect((await post(`/api/games/${f.id}/cancel`,{expected_state:'new',confirmed:false},{session:'__Host-brews_session=invalid'})).status).toBe(400);
 const audits=(await rows(f)).audits;expect(audits).toHaveLength(7);expect(audits.every(a=>a.outcome==='rejected'&&a.actor_account_id===f.admin.accountId&&a.target_game_id===f.id)).toBe(true);expect(JSON.stringify(audits)).not.toContain('submitted-private');
});

test('fresh older mutation audit IGNORE or failure rolls back domain receipt and success; read capture fails closed',async()=>{
 for(const denial of ['IGNORE',"ABORT,'controlled capture fault'"]){const f=await fixture(denial==='IGNORE'?'Ignored':'Failed'),before=await rows(f);
 await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec(`CREATE TRIGGER deny_capture BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(${denial}); END`));
 expect((await adminRequest(f,'cancel',{expected_state:'new',confirmed:true})).status).toBe(503);
 expect((await cli(f.id)).status).toBe(503);expect((await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=account`,{headers:{Cookie:f.admin.session}})).status).toBe(503);
 const after=await rows(f);for(const field of ['record','receipts','grants','audits'])expect(after[field]).toEqual(before[field]);
 await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec('DROP TRIGGER deny_capture'));
 expect((await post(`/api/games/${f.id}/cancel`,{expected_state:'new',confirmed:true},{session:f.host.session})).status).toBe(200);
 }
});

test('Game-owned audit authenticated create replay capture stays single and cross-admin lobby attribution is actual admin',async()=>{
 const host=await actor('GameAuditCreateAdmin',true),command=commandId();let response=await post('/api/games',{}, {session:host.session,id:command});expect(response.status).toBe(200);const made=await response.json();const f={host,admin:await actor('GameAuditOtherAdmin',true),id:made.game.game_id};
 expect((await post('/api/games',{}, {session:host.session,id:command})).status).toBe(200);expect((await rows(f)).audits.filter(a=>a.operation==='creation'&&a.outcome==='succeeded')).toHaveLength(1);
 response=await adminRequest(f,'lobby',{expected_revision:0});expect(response.status).toBe(200);const audit=(await rows(f)).audits.find(a=>a.operation==='lobby'&&a.outcome==='succeeded');expect(audit.actor_account_id).toBe(f.admin.accountId);expect(audit.actor_account_id).not.toBe(host.accountId);
});

for(const fault of ['none','IGNORE',"ABORT,'controlled counter failure'"]){test(`strict Game audit pinned previous DDL migration ${fault} preserves original record expiry`,async()=>{
 const f=await fixture(),audit=commandId();await runInDurableObject(owner(f.id),(_,s)=>{s.storage.sql.exec('DROP INDEX game_admin_audit_expiry');s.storage.sql.exec('DROP TABLE game_admin_audit');s.storage.sql.exec(publishedCommandSchema.find(s=>s.startsWith('CREATE TABLE game_admin_audit(')));s.storage.sql.exec('CREATE INDEX game_admin_audit_expiry ON game_admin_audit(expires_at,audit_id)');s.storage.sql.exec('UPDATE game_metadata SET schema_version=7');const now=Date.now();s.storage.sql.exec('INSERT INTO game_admin_audit VALUES(?,?,?,?,?,?,?)',audit,f.admin.accountId,f.id,'command_lookup','succeeded',now,now+7776000000);if(fault!=='none')s.storage.sql.exec(`CREATE TRIGGER deny_upgrade BEFORE UPDATE OF schema_version ON game_metadata BEGIN SELECT RAISE(${fault}); END`);});
 const before=(await rows(f)).audits;const response=await cli(f.id);expect(response.status).toBe(fault==='none'?200:503);const after=await rows(f);expect(after.audits.find(a=>a.audit_id===audit)).toEqual(before[0]);expect(await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec('SELECT schema_version FROM game_metadata').one().schema_version)).toBe(fault==='none'?8:7);
});}

test('fixed audit expiry equality denial survives audit-only owner reconstruction and clock rollback',async()=>{
 const f=await fixture();expect((await adminRequest(f,'cancel',{expected_state:'new',confirmed:true})).status).toBe(200);const original=(await rows(f)).audits;const expires=original[0].expires_at;
 Date.now=()=>expires;
 try {await evictDurableObject(owner(f.id));const response=await cli(f.id);expect(response.status).toBe(200);expect((await response.json()).events).toEqual([]);Date.now=()=>expires-1;expect((await cli(f.id)).status).toBe(503);Date.now=()=>expires;expect((await cli(f.id)).status).toBe(200);}finally{Date.now=realNow;}
});

test('durably capturable Directory reservation failure uses refreshed actual admin and no submitted data',async()=>{
 const f=await fixture();const before=await rows(f);
 await runInDurableObject(directory(),instance=>{instance.gameAuditOriginalFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const body=await request.clone().json();if(body.action==='confirm_reservation')throw new Error('controlled reservation transport failure');return instance.gameAuditOriginalFetch(request);};});
 try{const response=await adminRequest(f,'start',{expected_revision:0});expect(response.status).toBe(503);const after=await rows(f);expect(after.record).toEqual(before.record);expect(after.receipts).toEqual(before.receipts);expect(after.audits).toHaveLength(1);expect(after.audits[0]).toMatchObject({actor_account_id:f.admin.accountId,target_game_id:f.id,operation:'start',outcome:'failed'});}finally{await runInDurableObject(directory(),instance=>{instance.fetch=instance.gameAuditOriginalFetch;delete instance.gameAuditOriginalFetch;});}
});

test('malformed authenticated developer read captures DeveloperCli rejection with canonical target only',async()=>{
 const f=await fixture();const response=await cli(f.id,{submitted_private:'must-never-be-retained'});expect(response.status).toBe(400);const audits=(await rows(f)).audits;expect(audits).toHaveLength(1);expect(audits[0]).toMatchObject({actor_account_id:'developer_cli',target_game_id:f.id,operation:'list_game_audit',outcome:'rejected'});expect(JSON.stringify(audits)).not.toContain('must-never-be-retained');
});

test('account context read captures once before disclosure; final projection does not repeat success',async()=>{
 const f=await fixture();const response=await exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=account`,{headers:{Cookie:f.admin.session}});expect(response.status).toBe(200);const audits=(await rows(f)).audits;expect(audits.filter(a=>a.operation==='account_context'&&a.outcome==='succeeded')).toHaveLength(1);expect(audits[0].actor_account_id).toBe(f.admin.accountId);
});

for(const fault of ['target','operation','actor','expiry','status','oversize'])test(`developer audit owner boundary rejects ${fault} and captures DeveloperCli failure`,async()=>{
 const f=await fixture();expect((await adminRequest(f,'cancel',{expected_state:'new',confirmed:true})).status).toBe(200);
 await runInDurableObject(owner(f.id),instance=>{instance.auditOriginalFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const reply=await instance.auditOriginalFetch(request);if(new URL(request.url).pathname!=='/developer-audit')return reply;const value=await reply.json();if(fault==='target')value.game_id=commandId();if(fault==='operation')value.events[0].operation='unapproved';if(fault==='actor')value.events[0].actor={human:'unapproved'};if(fault==='expiry')value.events[0].expires_at++;if(fault==='oversize')value.padding='x'.repeat(300000);return Response.json(value,{status:fault==='status'?202:200});};});
 try{expect((await cli(f.id)).status).toBe(503);expect((await rows(f)).audits.some(a=>a.actor_account_id==='developer_cli'&&a.operation==='list_game_audit'&&a.outcome==='failed')).toBe(true);}finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
});

test('developer audit response expiry after owner await durably fences rollback before disclosure',async()=>{
 const f=await fixture();expect((await adminRequest(f,'cancel',{expected_state:'new',confirmed:true})).status).toBe(200);const expiry=(await rows(f)).audits[0].expires_at;
 await runInDurableObject(owner(f.id),instance=>{instance.auditOriginalFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const reply=await instance.auditOriginalFetch(request);if(new URL(request.url).pathname==='/developer-audit')Date.now=()=>expiry;return reply;};});
 try{expect((await cli(f.id)).status).toBe(503);await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});Date.now=()=>expiry-1;await evictDurableObject(owner(f.id));expect((await cli(f.id)).status).toBe(503);}finally{Date.now=realNow;}
});

for(const fault of ['column','index'])test(`pinned previous audit schema ${fault} drift is rejected before writes`,async()=>{
 const f=await fixture();await runInDurableObject(owner(f.id),(_,s)=>{s.storage.sql.exec('DROP INDEX game_admin_audit_expiry');s.storage.sql.exec('DROP TABLE game_admin_audit');s.storage.sql.exec(publishedCommandSchema.find(s=>s.startsWith('CREATE TABLE game_admin_audit(')));s.storage.sql.exec('CREATE INDEX game_admin_audit_expiry ON game_admin_audit(expires_at,audit_id)');s.storage.sql.exec('UPDATE game_metadata SET schema_version=7');if(fault==='column')s.storage.sql.exec('ALTER TABLE game_terminal_views ADD COLUMN unapproved TEXT');else s.storage.sql.exec('DROP INDEX game_admin_audit_expiry');});const before=await rows(f);expect((await cli(f.id)).status).toBe(503);const after=await rows(f);expect(after).toEqual(before);expect(await runInDurableObject(owner(f.id),(_,s)=>s.storage.sql.exec('SELECT schema_version FROM game_metadata').one().schema_version)).toBe(7);
});

for(const kind of ['history','discovery'])test(`authenticated admin ${kind} invalid owner binding captures refreshed actual actor failure`,async()=>{
 const f=await fixture();await runInDurableObject(owner(f.id),instance=>{instance.auditOriginalFetch=instance.fetch.bind(instance);instance.fetch=async request=>{const reply=await instance.auditOriginalFetch(request);if(new URL(request.url).pathname!==`/${kind}`)return reply;const value=await reply.json();value.game_id=commandId();return Response.json(value);};});
 try{const path=kind==='history'?`/api/history/${f.id}`:`/api/games/${f.id}/availability?view=account`;expect((await exports.default.fetch(`${origin}${path}`,{headers:{Cookie:f.admin.session}})).status).toBe(503);expect((await rows(f)).audits.some(a=>a.actor_account_id===f.admin.accountId&&a.operation===`${kind}_read`&&a.outcome==='failed')).toBe(true);}finally{await runInDurableObject(owner(f.id),instance=>{instance.fetch=instance.auditOriginalFetch;delete instance.auditOriginalFetch;});}
});

test('developer read transient owner durability failure is withheld and captures DeveloperCli execution failure',async()=>{
 const f=await fixture();await runInDurableObject(owner(f.id),(instance,s)=>{instance.auditOriginalSync=s.storage.sync.bind(s.storage);let failed=false;s.storage.sync=async()=>{if(!failed){failed=true;throw new Error('controlled developer audit durability failure');}await instance.auditOriginalSync();};});
 try{const response=await cli(f.id);expect(response.status).toBe(503);expect((await response.json()).error.code).toBe('unavailable');const audits=(await rows(f)).audits;expect(audits.filter(a=>a.operation==='list_game_audit'&&a.outcome==='succeeded')).toHaveLength(1);expect(audits.filter(a=>a.operation==='list_game_audit'&&a.outcome==='failed')).toHaveLength(1);expect(audits.every(a=>a.actor_account_id==='developer_cli')).toBe(true);}finally{await runInDurableObject(owner(f.id),(instance,s)=>{s.storage.sync=instance.auditOriginalSync;delete instance.auditOriginalSync;});}
});
