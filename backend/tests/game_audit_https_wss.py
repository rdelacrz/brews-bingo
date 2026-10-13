"""Slice 8 real local HTTPS developer CLI and privileged Game auditing checks."""
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
with tempfile.TemporaryDirectory(prefix='brews-game-audit-implementation-https-', dir=os.environ['TMPDIR']) as directory:
    scratch=Path(directory)
    os.chmod(scratch,0o700)
    ca,ca_key,cert,key,csr=(scratch/name for name in ['ca.pem','ca-key.pem','localhost.pem','localhost-key.pem','localhost.csr'])
    commands=[
        ['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=Brews operator local CA','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign','-keyout',str(ca_key),'-out',str(ca)],
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
    def developer_read(label, game_id, *, after=None, limit=100):
        args=[str(ROOT/'target/debug/brews'),'--json','audit','game',game_id,'--limit',str(limit)]
        if after:args.extend(['--after',after])
        result=subprocess.run(args,env=environment,cwd=ROOT,capture_output=True,timeout=45)
        check(result.returncode==0,label+' actual developer CLI HTTP command')
        value=json.loads(result.stdout)
        check(value['result']=='game_audit' and value['game_id']==game_id,label+' typed exact Game target')
        check(len(value['events'])<=limit,label+' bounded page')
        for event in value['events']:
            check(set(event)=={'audit_id','actor','target_game_id','operation','outcome','occurred_at','expires_at'},label+' secret-free event fields')
            check(event['target_game_id']==game_id and event['expires_at']-event['occurred_at']==7776000000,label+' fixed expiry')
        completed.append(label)
        return value
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
            host_id,host=provision('OperatorHttpsHost','host')
            target_id,target=provision('OperatorHttpsTarget','host')
            admin_id,admin=provision('OperatorHttpsAdmin','admin')
            created,_=call('operator-create','/api/games',body={'configuration':{'numeric_upper_bound':4,'board_side_length':2,'free_cells_enabled':False,'free_cell_positions':[],'player_capacity':3,'spectator_capacity':1}},session=host)
            game=created['game']['game_id'];base='/api/games/'+game
            check(developer_read('audit-empty-exact-game',game)['events']==[],'host creation is not an admin event')
            call('operator-admin-invalid-patch-audited',base+'/configuration',method='PATCH',body={'expected_revision':0,'player_capacity':None},session=admin,expected=400)
            call('operator-admin-discovery-audited',base+'/host-transfer-targets',method='GET',session=admin)
            command=str(uuid.uuid7());noop,_=call('operator-patch-noop',base+'/configuration',method='PATCH',body={'expected_revision':0},session=host,id=command)
            retry,_=call('operator-patch-receipt',base+'/configuration',method='PATCH',body={'expected_revision':0},session=host,id=command);check(retry=={'result':'committed','receipt':noop['receipt']},'operator no-op receipt replay')
            call('operator-patch-null',base+'/configuration',method='PATCH',body={'expected_revision':0,'player_capacity':None},session=host,expected=400)
            configured,_=call('operator-patch-effective',base+'/configuration',method='PATCH',body={'expected_revision':0,'player_capacity':2},session=host)
            command=str(uuid.uuid7());resumed,_=call('operator-resume-host',base+'/resume',body={},session=host,id=command)
            again,_=call('operator-resume-receipt',base+'/resume',body={},session=host,id=command);check(again=={'result':'committed','receipt':resumed['receipt']},'resume replay regenerated view')
            passive,_=call('operator-resume-other-host',base+'/resume',body={},session=target);check(not passive['can_mutate'],'cross-host resume mutation capability')
            listing,_=call('operator-targets-safe',base+'/host-transfer-targets?limit=100',method='GET',session=host);check(listing['targets']==[{'account_id':target_id,'username':'OperatorHttpsTarget'}],'safe target-only projection')
            call('operator-targets-nonoperator',base+'/host-transfer-targets',method='GET',session=target,expected=403)
            call('operator-target-admin-rejected',base+'/host-transfer',body={'target_account_id':admin_id,'confirmed':True,'expected_host_assignment_revision':0},session=host,expected=409)
            command=str(uuid.uuid7());input={'target_account_id':target_id,'confirmed':True,'expected_host_assignment_revision':0}
            assigned,_=call('operator-transfer-New',base+'/host-transfer',body=input,session=host,id=command)
            duplicate,_=call('operator-outgoing-receipt',base+'/host-transfer',body=input,session=host,id=command);check(duplicate=={'result':'committed','receipt':assigned['receipt']},'outgoing assignment retry changed effect')
            call('operator-outgoing-mutation-denied',base+'/configuration',method='PATCH',body={'expected_revision':assigned['receipt']['outcome']['view_revision'],'player_capacity':3},session=host,expected=403)
            current,_=call('operator-newhost-sync',base+'/sync?view=account',method='GET',session=target)
            lobby,_=call('operator-lobby-after-saved-transfer',base+'/lobby',body={'expected_revision':current['view_revision']},session=target);code=lobby['game_code']
            account_streams=[]
            for label,session in [('outgoing-host',host),('designated-target',target),('admin',admin)]:
                stream=SocketClient('localhost',port,tls,base+'/stream?view=account',origin,session);sockets.append(stream);account_streams.append(stream)
                frame=stream.frame();check(frame['view']['game']['designated_host_id']==target_id,'operator current initial account WSS assignment');completed.append('operator-'+label+'-real-account-WSS')
            _,audience_context=call('operator-audience-context',base+'/spectator-admission-context',body={'game_code':code},retry=False,cookie_effect=True)
            _,audience=call('operator-audience-admission',base+'/spectators',body={'game_code':code},session=audience_context,cookie_effect=True)
            spectator_stream=SocketClient('localhost',port,tls,base+'/stream?view=spectator',origin,audience);sockets.append(spectator_stream)
            check(spectator_stream.frame()['view']['role']=='spectator','operator real audience WSS identity');completed.append('operator-real-spectator-WSS')
            players=[]
            for alias in ['OperatorHttpsA','OperatorHttpsB']:
                _,context=call(alias+'-context',base+'/admission-context',body={'game_code':code},cookie_effect=True,retry=False)
                joined,player=call(alias+'-join',base+'/players',body={'game_code':code,'alias':alias},session=context,cookie_effect=True);players.append(player)
                stream=SocketClient('localhost',port,tls,base+'/stream?view=player',origin,player);sockets.append(stream);check(stream.frame()['view']['player_id']==joined['player_id'],'operator WSS player identity')
                completed.append(alias+'-real-WSS-connected')
            state,_=call('operator-before-start',base+'/sync?view=account',method='GET',session=target)
            call('operator-start-existing-game',base+'/start',body={'expected_revision':state['view_revision']},session=target)
            state,_=call('operator-play-before-transfer',base+'/sync?view=account',method='GET',session=target)
            boards=[p['board'] for p in state['snapshot']['players']]
            # Admin initiates an immediate transfer to the original ordinary Host.
            assigned,_=call('operator-admin-transfer-InProgress',base+'/host-transfer',body={'target_account_id':host_id,'confirmed':True,'expected_host_assignment_revision':1},session=admin)
            for label,stream in zip(['outgoing-host','designated-target','admin'],account_streams):
                frame=stream.at_revision(assigned['receipt']['outcome']['view_revision'])
                check(frame['view']['game']['designated_host_id']==host_id,'operator account WSS retained stale designated assignment')
                check([p['board'] for p in frame['view']['players']]==boards and frame['view']['spectator_count']==1,'operator account WSS changed roster/board data')
                completed.append('operator-'+label+'-WSS-current-transfer')
            state,_=call('operator-inprogress-resume',base+'/resume',body={},session=host)
            check([p['board'] for p in state['game']['players']]==boards,'operator transfer changed boards');check(state['game']['game_code']==code,'operator transfer changed code')
            call('operator-oldtarget-call-denied',base+'/calls/manual',body={'expected_revision':state['game']['game']['view_revision'],'value':'1'},session=target,expected=403)
            call('operator-acting-admin-call',base+'/calls/manual',body={'expected_revision':state['game']['game']['view_revision'],'value':'1'},session=admin)
            audience_view,_=call('operator-audience-after-transfer',base+'/sync?view=spectator',method='GET',session=audience)
            frame=spectator_stream.at_revision(audience_view['view_revision'])
            check(all(key not in frame['view'] for key in ['players','board','alias','recovery_answer','qualifiers']) and frame['view']['calls'][-1]['value']=='1','operator transfer disclosed private audience data or lost live calls')
            completed.append('operator-spectator-WSS-transfer-privacy')
            ended,_=call('operator-acting-admin-terminal',base+'/cancel',body={'expected_state':'in_progress','confirmed':True},session=admin)
            captured=developer_read('audit-real-cli-admin-actions',game)
            check({(e['operation'],e['outcome']) for e in captured['events']} >= {('call_manual','succeeded'),('cancel','succeeded'),('account_stream','succeeded')},'real admin mutation and stream capture')
            check(all(e['actor']=={'account':admin_id} or e['actor']=='developer_cli' for e in captured['events']),'typed actual admin or DeveloperCli attribution')
            first_page=developer_read('audit-real-cli-page-one',game,limit=1)
            check(first_page['next_cursor']==first_page['events'][-1]['audit_id'],'strict audit next cursor')
            next_page=developer_read('audit-real-cli-page-two',game,after=first_page['next_cursor'],limit=1)
            check(next_page['events'][0]['audit_id']>first_page['next_cursor'],'audit cursor progression')
            invalid=subprocess.run([str(ROOT/'target/debug/brews'),'--json','audit','game',game,'--limit','101'],env=environment,cwd=ROOT,capture_output=True,timeout=45)
            check(invalid.returncode!=0,'CLI rejects excessive limit before HTTP');completed.append('audit-cli-excessive-limit-rejected')
            call('operator-terminal-resume-rejected',base+'/resume',body={},session=host,expected=409)
            audit_rows=None
            for database in (scratch/'state').rglob('*.sqlite'):
                with sqlite3.connect(database.as_uri()+'?mode=ro',uri=True) as owner_store:
                    if owner_store.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='game_admin_audit'").fetchone():
                        identity=owner_store.execute('SELECT game_id FROM game_operator_identity WHERE singleton=1').fetchone()
                        if identity and identity[0]==game:
                            audit_rows=owner_store.execute('SELECT actor_account_id,target_game_id,operation,outcome,occurred_at,expires_at FROM game_admin_audit').fetchall();break
            check(audit_rows is not None,'actual local Game audit store missing')
            check({(row[2],row[3]) for row in audit_rows} >= {('configuration','rejected'),('host_transfer_targets','succeeded'),('host_transfer','succeeded')},'actual HTTPS admin audit outcomes')
            check(all(row[0] in (admin_id,'developer_cli') and row[1]==game and row[5]-row[4]==90*24*60*60*1000 for row in audit_rows),'actual HTTPS admin attribution and fixed retention')
            completed.append('operator-real-HTTPS-game-audit-storage')
            check(len(completed)==len(set(completed)),'operator TLS check labels repeated')
            print(json.dumps({'actual_https_wss_game_audit_checks':'passed','check_count':len(completed),'checks':completed,'real_player_sockets':len(players),'real_account_sockets':len(account_streams),'real_spectator_sockets':1,'deployment':'none'}))
        finally:
            log.flush()
            retained_log=Path(os.environ['TMPDIR'])/'brews-game-audit-implementation-https-worker.log'
            shutil.copyfile(scratch/'worker.log',retained_log);os.chmod(retained_log,0o600)
            for stream in sockets:stream.close()
            process.terminate()
            try:process.wait(timeout=10)
            except subprocess.TimeoutExpired:process.kill();process.wait(timeout=10)
