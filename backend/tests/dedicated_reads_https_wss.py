"""Local dedicated-read HTTPS/WSS checks; build the Worker and CLI before running."""
from pathlib import Path
import base64
import hashlib
import json
import os
import secrets
import shutil
import socket
import ssl
import sqlite3
import struct
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]


def check(condition, label):
    if not condition:
        raise RuntimeError(label)


class SocketClient:
    def __init__(self, host, port, tls, path, origin, session):
        self.socket = tls.wrap_socket(socket.create_connection((host, port), timeout=10), server_hostname=host)
        self.socket.settimeout(10)
        key = base64.b64encode(secrets.token_bytes(16)).decode('ascii')
        handshake = (f'GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n'
                     f'Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n'
                     f'Origin: {origin}\r\nCookie: {session}\r\n\r\n')
        self.socket.sendall(handshake.encode('ascii'))
        header = bytearray()
        while not header.endswith(b'\r\n\r\n'):
            check(len(header) < 16384, 'WSS handshake headers exceed bound')
            chunk = self.socket.recv(1)
            check(bool(chunk), 'WSS handshake closed')
            header.extend(chunk)
        lines = header.decode('ascii').split('\r\n')
        check(lines[0].split(' ')[1] == '101', 'WSS upgrade rejected')
        fields = dict(line.split(': ', 1) for line in lines[1:] if ': ' in line)
        expected = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode('ascii')).digest()).decode('ascii')
        check(next((value for name, value in fields.items() if name.lower() == 'sec-websocket-accept'), None) == expected, 'WSS accept key mismatch')

    def read_exact(self, size):
        buffer = bytearray()
        while len(buffer) < size:
            chunk = self.socket.recv(size - len(buffer))
            check(bool(chunk), 'WSS connection closed before frame')
            buffer.extend(chunk)
        return bytes(buffer)

    def send(self, opcode, content):
        check(len(content) <= 512, 'client transport frame exceeds bound')
        mask = secrets.token_bytes(4)
        size = len(content)
        prefix = bytes([128 | opcode, 128 | size]) if size < 126 else bytes([128 | opcode, 128 | 126]) + struct.pack('!H', size)
        masked = bytes(byte ^ mask[index % 4] for index, byte in enumerate(content))
        self.socket.sendall(prefix + mask + masked)

    def frame(self, *, allow_close=False, allow_terminal=False) -> dict:
        while True:
            first, second = self.read_exact(2)
            check(first & 128 and not first & 112, 'unexpected fragmented/compressed WSS frame')
            check(not second & 128, 'server frame unexpectedly masked')
            size = second & 127
            if size == 126:
                size = struct.unpack('!H', self.read_exact(2))[0]
            elif size == 127:
                size = struct.unpack('!Q', self.read_exact(8))[0]
            check(size <= 256 * 1024, 'server frame exceeds approved cap')
            data = self.read_exact(size)
            opcode = first & 15
            if opcode == 9:
                self.send(10, data)
                continue
            if opcode == 8:
                check(allow_close and len(data) >= 2, 'unexpected WSS close frame')
                self.send(8, data)
                return {'transport_close_code': struct.unpack('!H', data[:2])[0]}
            check(opcode == 1, 'expected JSON snapshot frame')
            value = json.loads(data)
            if allow_terminal and value.get('kind') == 'terminal_notice':
                check(value.get('version') == 1 and 'board' not in value and 'players' not in value and 'history_expires_at' not in value, 'terminal audience notice leaked protected state')
                return value
            check(value.get('version') == 1 and value.get('kind') == 'snapshot', 'invalid snapshot envelope')
            check(list(value)[-1] == 'delivery_id', 'delivery ID is not final serialized field')
            self.send(1, json.dumps({'version': 1, 'kind': 'snapshot_ack', 'connection_id': value['connection_id'], 'delivery_id': value['delivery_id'], 'view_revision': value['view_revision']}, separators=(',', ':')).encode('utf8'))
            return value

    def at_revision(self,revision):
        for _ in range(20):
            frame=self.frame()
            if frame['view_revision']>=revision:return frame
        raise RuntimeError('operator WSS revision progression exceeded bound')

    def close(self):
        try:
            self.send(8, struct.pack('!H', 1000))
        except OSError:
            pass
        self.socket.close()


# This focused harness uses the same real TLS handshake/frame checks as gameplay.
with tempfile.TemporaryDirectory(prefix='brews-read-slice-followup-https-', dir=os.environ['TMPDIR']) as directory:
    scratch=Path(directory)
    os.chmod(scratch,0o700)
    ca,ca_key,cert,key,csr=(scratch/name for name in ['ca.pem','ca-key.pem','localhost.pem','localhost-key.pem','localhost.csr'])
    commands=[
        ['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=Brews reads local CA','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign','-keyout',str(ca_key),'-out',str(ca)],
        ['openssl','req','-new','-newkey','rsa:2048','-nodes','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost,IP:127.0.0.1','-keyout',str(key),'-out',str(csr)],
        ['openssl','x509','-req','-in',str(csr),'-CA',str(ca),'-CAkey',str(ca_key),'-CAcreateserial','-days','1','-copy_extensions','copy','-out',str(cert)],
    ]
    for command in commands:
        check(subprocess.run(command,capture_output=True).returncode==0,'local operator certificate generation')
    with socket.socket() as probe:
        probe.bind(('127.0.0.1',0));port=probe.getsockname()[1]
    origin=f'https://localhost:{port}'
    cli_key=secrets.token_urlsafe(32)
    variables=scratch/'local-fixtures.env'
    variables.write_text(f'APP_ORIGIN={origin}\nRATE_LIMIT_KEY={secrets.token_urlsafe(32)}\nDEV_CLI_KEY={cli_key}\n')
    os.chmod(variables,0o600)
    config=(ROOT/'backend/wrangler.toml').read_text()
    config=config.replace('command = "worker-build --release"','command = "true"').replace('main = "build/worker/shim.mjs"','main = '+json.dumps(str(ROOT/'backend/build/worker/shim.mjs')))
    local_config=scratch/'worker.toml';local_config.write_text(config)
    environment={**os.environ,'BREWS_API_ORIGIN':origin,'BREWS_DEV_CLI_KEY':cli_key,'BREWS_TLS_CA_FILE':str(ca),'WRANGLER_SEND_METRICS':'false','WRANGLER_LOG_PATH':str(scratch/'wrangler-logs')}
    tls=ssl.create_default_context(cafile=str(ca));completed=[];sockets=[]
    def call(label,path,*,method='POST',body=None,session=None,id=None,expected=200,retry=True,cookie_effect=False):
        headers={'Origin':origin,'CF-Connecting-IP':'192.0.2.44'}
        if session:headers['Cookie']=session
        if method!='GET' and retry:headers['Idempotency-Key']=id or str(uuid.uuid7())
        if body is not None:headers['Content-Type']='application/json'
        request=urllib.request.Request(origin+path,method=method,headers=headers,data=None if body is None else json.dumps(body).encode())
        try:response=urllib.request.urlopen(request,context=tls,timeout=30)
        except urllib.error.HTTPError as error:response=error
        with response:
            check(response.status==expected,label+' HTTP status '+str(response.status))
            value=json.loads(response.read(256*1024));cookie=response.headers.get('Set-Cookie')
            check(bool(cookie)==cookie_effect,label+' cookie effect')
            check(response.headers.get('Cache-Control')=='no-store',label+' no-store')
        completed.append(label)
        return value,cookie.split(';',1)[0] if cookie else None
    def provision(name,role):
        output=scratch/(name+'.url')
        result=subprocess.run([str(ROOT/'target/debug/brews'),'--json','accounts','create','--role',role,'--username',name,'--link-output',str(output)],env=environment,cwd=ROOT,capture_output=True,timeout=45)
        check(result.returncode==0,'local fixture CLI provisioning');check(output.stat().st_mode&0o777==0o600,'local link file permissions')
        url=output.read_text().strip()
        _,restricted=call(name+'-redeem','/api/auth/enrollment/redeem',body={'enrollment_token':url.split('#',1)[1]},cookie_effect=True)
        _,session=call(name+'-complete','/api/auth/enrollment/complete',body={'new_password':secrets.token_urlsafe(24)},session=restricted,cookie_effect=True)
        current,_=call(name+'-identity','/api/session',method='GET',session=session)
        return current['account']['account_id'],session
    with (scratch/'worker.log').open('x') as log:
        os.chmod(scratch/'worker.log',0o600)
        process=subprocess.Popen([str(ROOT/'backend/node_modules/.bin/wrangler'),'dev','--config',str(local_config),'--local','--local-protocol','https','--https-key-path',str(key),'--https-cert-path',str(cert),'--env-file',str(variables),'--ip','127.0.0.1','--port',str(port),'--persist-to',str(scratch/'state'),'--log-level','warn'],cwd=ROOT/'backend',env=environment,stdout=log,stderr=subprocess.STDOUT)
        try:
            ready=False
            for _ in range(150):
                check(process.poll() is None,'local operator Worker exited')
                try:
                    with urllib.request.urlopen(origin+'/api/session',context=tls,timeout=1) as response:ready=response.status==200
                    if ready:break
                except (OSError,urllib.error.URLError):time.sleep(0.2)
            check(ready,'local operator Worker readiness')
            host_id,host=provision('ReadHttpsHost','host')
            other_id,other=provision('ReadHttpsOther','host')
            admin_id,admin=provision('ReadHttpsAdmin','admin')
            config={'numeric_upper_bound':12,'board_side_length':2,'free_cells_enabled':False,'free_cell_positions':[],'player_capacity':3,'spectator_capacity':3}
            made,_=call('reads-create-New','/api/games',body={'configuration':config},session=host)
            game=made['game']['game_id'];base='/api/games/'+game
            def read(label,path='',session=host,expected=200):return call(label,base+path,method='GET',session=session,expected=expected)[0]
            for name,path in [('C5',''),('D2','/players'),('E1','/calls'),('E2','/remaining-values'),('E4','/boards'),('E5','/qualification')]:read('reads-New-'+name,path,other)
            read('reads-New-F1-conflict','/result',host,409)
            lobby,_=call('reads-open-lobby',base+'/lobby',body={'expected_revision':0},session=host)
            code=lobby['game_code'];players=[]
            for n,alias in enumerate(['ReadHttpsOne','ReadHttpsTwo']):
                context,admission=call('reads-admission-'+str(n),base+'/admission-context',body={'game_code':code},retry=False,cookie_effect=True)
                answer=secrets.token_urlsafe(24)
                joined,session=call('reads-join-'+str(n),base+'/players',body={'game_code':code,'alias':alias,'recovery_answer':answer},session=admission,cookie_effect=True)
                players.append({'session':session,'answer':answer,'body':joined})
            p=players[0]
            membership=read('reads-D1-own-current-recovery','/membership',p['session']);check(membership['membership']['player_id']==p['body']['player_id'] and membership['membership']['recovery_enabled'],'own real membership projection')
            board=read('reads-E3-unassigned-null','/membership/board',p['session']);check(board['board'] is None,'unassigned own board must be explicit null')
            q=read('reads-E5-prestart-false','/qualification',p['session']);check(not q['qualified'] and q['qualifying_lines']==[],'prestart own qualification')
            calls=read('reads-E1-no-latest','/calls',p['session']);check(calls['calls']==[] and calls['latest'] is None,'empty calls latest')
            remaining=read('reads-E2-complete-numeric-order','/remaining-values');check(remaining['remaining_values']==[str(n) for n in range(1,13)],'remaining values numeric string order')
            selected=read('reads-E4-selected-stable-ID','/boards?player_id='+p['body']['player_id'],other);check(len(selected['boards'])==1 and selected['boards'][0]['board'] is None,'prestart selected board')
            read('reads-E4-no-own-cookie-privilege','/boards',p['session'],401)
            read('reads-selector-ambiguous','',host+'; '+p['session'],400)
            read('reads-explicit-invalid-no-fallback','?view=player',host+'; __Secure-brews-player=invalid',401)
            call('reads-recovery-delete',base+'/membership/recovery-answer',method='DELETE',body={},session=p['session'])
            check(not read('reads-D1-deleted-recovery','/membership',p['session'])['membership']['recovery_enabled'],'current deleted recovery state')
            call('reads-recovery-restore',base+'/membership/recovery-answer',method='PUT',body={'recovery_answer':p['answer']},session=p['session'])
            for n,player in enumerate(players):
                stream=SocketClient('localhost',port,tls,base+'/stream?view=player',origin,player['session']);sockets.append(stream)
                frame=stream.frame();check(frame['view']['role']=='player','real reads player WSS');completed.append('reads-player-'+str(n)+'-actual-WSS')
            roster=read('reads-D2-actual-connected-roster','/players');check({player['player_id'] for player in roster['players'] if player['connected']}=={player['body']['player_id'] for player in players},'current physical WSS identities')
            c,context=call('reads-spectator-context',base+'/spectator-admission-context',body={'game_code':code},retry=False,cookie_effect=True)
            s,spec=call('reads-spectator-join',base+'/spectators',body={'game_code':code},session=context,cookie_effect=True)
            membership=read('reads-D1-spectator-omits-recovery','/membership',spec);check('recovery_enabled' not in membership['membership'],'spectator recovery privacy')
            read('reads-spectator-qualification-denied','/qualification',spec,403)
            read('reads-spectator-calls-permitted','/calls',spec)
            spec_stream=SocketClient('localhost',port,tls,base+'/stream?view=spectator',origin,spec);sockets.append(spec_stream);check(spec_stream.frame()['view']['role']=='spectator','real reads spectator WSS');completed.append('reads-spectator-actual-WSS')
            revision=read('reads-C5-before-Start')['game']['game']['view_revision']
            call('reads-Start',base+'/start',body={'expected_revision':revision},session=host)
            own=read('reads-E3-assigned-board','/membership/board',p['session']);check(len(own['board']['cells'])==4,'actual assigned board')
            for value in range(1,13):
                revision=read('reads-C5-call-revision-'+str(value))['game']['game']['view_revision']
                call('reads-call-'+str(value),base+'/calls/manual',body={'value':str(value),'expected_revision':revision},session=host)
            calls=read('reads-E1-full-accepted-ledger','/calls',p['session']);check(len(calls['calls'])==12 and calls['latest']==calls['calls'][-1],'full accepted call ledger')
            remaining=read('reads-E2-full-exhaustion','/remaining-values');check(remaining['exhausted'] and remaining['remaining_values']==[] and remaining['remaining_count']==0,'actual pool exhaustion')
            check(read('reads-C5-exhaustion-does-not-end')['game']['game']['state']=='in_progress','exhaustion lifecycle')
            q=read('reads-E5-own-automatic-qualification','/qualification',p['session']);check(q['qualified'],'automatic own qualification')
            q=read('reads-E5-account-stable-qualifiers','/qualification',other);check(len(q['qualified_players'])==2,'account qualifier list')
            boards=read('reads-E4-live-cross-host-all-boards','/boards',other);check(len(boards['boards'])==2,'cross-host board inspection')
            admin_read=read('reads-Admin-C5-actual-audit','',admin)
            call('reads-confirm-no-winner',base+'/cancel',body={'confirmed':True,'expected_state':'in_progress'},session=host)
            for name,path in [('C5',''),('D1','/membership'),('E1','/calls'),('E3','/membership/board'),('E5','/qualification'),('F1','/result')]:read('reads-final-player-'+name,path,p['session'])
            read('reads-final-host-F1','/result',host)
            roster=read('reads-final-D2-current-WSS-null-Leave','/players');check({player['player_id'] for player in roster['players']}=={player['body']['player_id'] for player in players} and all(player['left_during_play'] is None and player['membership_retained'] and player['connected'] for player in roster['players']),'truthful final roster identities, presence and unknown Leave')
            read('reads-final-E4-original-host-grant','/boards',host)
            read('reads-final-E2-unavailable-409','/remaining-values',host,409)
            read('reads-final-spectator-denied','/result',spec,401)
            read('reads-final-passive-other-no-grant','/result',other,401)
            call('reads-player-Exit',base+'/exit',body={},session=p['session'])
            read('reads-final-exited-player-denied','/result',p['session'],401)
            call('reads-account-Exit',base+'/exit',body={},session=host)
            read('reads-final-exited-account-denied','/result',host,401)
            call('reads-History-separate-after-Exit','/api/history/'+game,method='GET',session=host)
            audit_rows=None
            for database in (scratch/'state').rglob('*.sqlite'):
                with sqlite3.connect(database.as_uri()+'?mode=ro',uri=True) as store:
                    if store.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='game_admin_audit'").fetchone():
                        identity=store.execute('SELECT game_id FROM game_operator_identity WHERE singleton=1').fetchone()
                        if identity and identity[0]==game:
                            check(not store.execute("SELECT name FROM sqlite_master WHERE name LIKE 'game_history_read_%'").fetchall(),'unapproved terminal metadata exists')
                            audit_rows=store.execute('SELECT actor_account_id,target_game_id,operation,outcome,occurred_at,expires_at FROM game_admin_audit').fetchall();break
            check(audit_rows is not None and any(row[0]==admin_id and row[1]==game and row[2]=='game_read' and row[3]=='succeeded' and row[5]-row[4]==7776000000 for row in audit_rows),'actual local read Admin audit')
            completed.append('reads-actual-SQL-audit-and-fixed-History-schema')
            check(len(completed)==len(set(completed)),'reads TLS check labels repeated')
            print(json.dumps({'actual_https_wss_read_checks':'passed','check_count':len(completed),'checks':completed,'real_player_sockets':2,'real_spectator_sockets':1,'deployment':'none'}))
        finally:
            log.flush()
            retained_log=Path(os.environ['TMPDIR'])/'brews-read-slice-followup-https-worker.log'
            shutil.copyfile(scratch/'worker.log',retained_log);os.chmod(retained_log,0o600)
            for stream in sockets:stream.close()
            process.terminate()
            try:process.wait(timeout=10)
            except subprocess.TimeoutExpired:process.kill();process.wait(timeout=10)
