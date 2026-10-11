import { beforeEach, expect, test } from 'vitest';
import { env, exports } from 'cloudflare:workers';
import { reset, runInDurableObject } from 'cloudflare:test';
import { enroll, post, resetStorage, origin, cookie, commandId, password } from './fixtures.js';
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async()=>{await reset(); await resetStorage(); await runInDurableObject(env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName('directory')),async(_i,s)=>s.storage.deleteAll());});
async function fixture(){
 const host=await enroll('ReviewHost');
 const made=await post('/api/games',{configuration:{numeric_upper_bound:4,board_side_length:2,free_cells_enabled:false,free_cell_positions:[],player_capacity:3,spectator_capacity:0}},{session:host.session});
 expect(made.status).toBe(200); const id=(await made.json()).game.game_id;
 const opened=await post(`/api/games/${id}/lobby`,{expected_revision:0},{session:host.session}); expect(opened.status).toBe(200); const code=(await opened.json()).game_code;
 const ctx=await post(`/api/games/${id}/admission-context`,{game_code:code},{id:null}); expect(ctx.status).toBe(200);
 const answer=password(); const joined=await post(`/api/games/${id}/players`,{game_code:code,alias:'ReviewPlayer',recovery_answer:answer},{session:cookie(ctx)}); expect(joined.status).toBe(200);
 return {id,code,answer,player:(await joined.json()).player_id,session:cookie(joined)};
}
const recover=f=>post('/api/player-recovery',{game_code:f.code,alias:'ReviewPlayer',recovery_answer:f.answer});
const sync=(f,session)=>exports.default.fetch(`${origin}/api/games/${f.id}/sync?view=player`,{headers:{Cookie:session}});
async function restore(f){await runInDurableObject(owner(f.id),i=>{if(i.reviewOriginal){i.fetch=i.reviewOriginal;delete i.reviewOriginal;} delete i.reviewCut;});}
test('cut1 expiry observed before output request survives rollback',async()=>{
 const f=await fixture(); const now=Date.now;
 await runInDurableObject(owner(f.id),i=>{i.reviewOriginal=i.fetch.bind(i); i.fetch=async req=>{
  const input=new URL(req.url).pathname==='/command'?await req.clone().json():{};
  if(input.operation==='recovery_output' && i.reviewCut){Date.now=()=>i.reviewCut-1;}
  const result=await i.reviewOriginal(req);
  if(input.operation==='recover_player' && result.status===200){const body=await result.clone().json(); i.reviewCut=body.valid_until; Date.now=()=>i.reviewCut;}
  return result;
 };});
 try{const result=await recover(f);console.log('CUT1',JSON.stringify({status:result.status,cookie:result.headers.has('set-cookie')})); expect(result.status).not.toBe(200);expect(result.headers.has('set-cookie')).toBe(false);}
 finally{Date.now=now;await restore(f);}
});
test('cut2 denial fences original observed sample when subsequent clock sample rolls back',async()=>{
 const f=await fixture(); const now=Date.now;
 await runInDurableObject(owner(f.id),i=>{i.reviewOriginal=i.fetch.bind(i); i.fetch=async req=>{
  const input=new URL(req.url).pathname==='/command'?await req.clone().json():{};
  const result=await i.reviewOriginal(req);
  if(input.operation==='recovery_output' && result.status===200){const body=await result.clone().json();i.reviewCut=body.valid_until;let n=0; Date.now=()=> n++===0?i.reviewCut:i.reviewCut-1;}
  return result;
 };});
 try{const result=await recover(f);const meta=await runInDurableObject(owner(f.id),(i,s)=>({cut:i.reviewCut,floor:[...s.storage.sql.exec('SELECT last_observed_ms FROM game_metadata')][0].last_observed_ms}));console.log('CUT2',JSON.stringify({status:result.status,...meta}));expect(meta.floor).toBeGreaterThanOrEqual(meta.cut);}
 finally{Date.now=now;await restore(f);}
});
test('D14 private response is denied if owning authority revoked after owner cut',async()=>{
 const f=await fixture();
 await runInDurableObject(owner(f.id),(i,s)=>{i.reviewOriginal=i.fetch.bind(i);i.fetch=async req=>{
  const input=new URL(req.url).pathname==='/command'?await req.clone().json():{};
  const result=await i.reviewOriginal(req);
  if(input.operation==='put_recovery_answer' && result.status===200){s.storage.sql.exec('UPDATE game_players SET session_epoch=session_epoch+1 WHERE player_id=?',f.player);}
  return result;
 };});
 try{const result=await exports.default.fetch(`${origin}/api/games/${f.id}/membership/recovery-answer`,{method:'PUT',headers:{Origin:origin,Cookie:f.session,'Idempotency-Key':commandId(),'Content-Type':'application/json'},body:JSON.stringify({recovery_answer:password()})});console.log('D14',JSON.stringify({status:result.status,oldAuthoritySync:(await sync(f,f.session)).status}));expect(result.status).not.toBe(200);}
 finally{await restore(f);}
});
test('second recovery cut cannot substitute a different syntactically valid cookie token',async()=>{
 const f=await fixture();
 await runInDurableObject(owner(f.id),i=>{i.reviewOriginal=i.fetch.bind(i);i.fetch=async req=>{
  const input=new URL(req.url).pathname==='/command'?await req.clone().json():{};
  const result=await i.reviewOriginal(req);
  if(input.operation==='recovery_output' && result.status===200){const body=await result.json();const raw=crypto.getRandomValues(new Uint8Array(32));body.cookie.token=btoa(String.fromCharCode(...raw)).replaceAll('+','-').replaceAll('/','_').replace(/=+$/,'');return new Response(JSON.stringify(body),{status:200,headers:{'Content-Type':'application/json'}});}
  return result;
 };});
 try{const result=await recover(f); const accepted=result.status===200?cookie(result):null;console.log('TOKEN_SUBSTITUTION',JSON.stringify({status:result.status,issuedSync:accepted?(await sync(f,accepted)).status:null}));expect(result.status).not.toBe(200);}
 finally{await restore(f);}
});
for(const mode of ['IGNORE', "ABORT, 'review-only fault'"]){
 test(`real SQLite failure bucket write ${mode.startsWith('IGNORE')?'IGNORE':'ABORT'} rolls back all scope buckets`,async()=>{
  const f=await fixture();
  await runInDurableObject(owner(f.id),(_i,s)=>s.storage.sql.exec(`CREATE TRIGGER review_bucket_fault BEFORE INSERT ON game_recovery_buckets BEGIN SELECT RAISE(${mode}); END`));
  const result=await post('/api/player-recovery',{game_code:f.code,alias:'ReviewPlayer',recovery_answer:password()});
  const buckets=await runInDurableObject(owner(f.id),(_i,s)=>[...s.storage.sql.exec('SELECT * FROM game_recovery_buckets')]);
  console.log('BUCKET_FAULT',JSON.stringify({mode:mode.startsWith('IGNORE')?'IGNORE':'ABORT',status:result.status,count:buckets.length}));expect(result.status).toBe(503);expect(buckets).toEqual([]);
 });
}
