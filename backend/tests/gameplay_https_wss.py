"""Local gameplay HTTPS/WSS checks; build the Worker and CLI before running."""
from pathlib import Path
import base64
import hashlib
import json
import os
import secrets
import socket
import ssl
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

    def close(self):
        try:
            self.send(8, struct.pack('!H', 1000))
        except OSError:
            pass
        self.socket.close()


with tempfile.TemporaryDirectory(prefix='brews-spectator-followup-https-', dir=os.environ['TMPDIR']) as directory:
    scratch = Path(directory)
    os.chmod(scratch, 0o700)
    cert, key = scratch / 'localhost.pem', scratch / 'localhost-key.pem'
    ca, ca_key, csr = scratch / 'ca.pem', scratch / 'ca-key.pem', scratch / 'localhost.csr'
    commands = [
        ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-subj', '/CN=Brews local test CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign', '-keyout', str(ca_key), '-out', str(ca)],
        ['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost,IP:127.0.0.1', '-addext', 'basicConstraints=critical,CA:FALSE', '-addext', 'keyUsage=critical,digitalSignature,keyEncipherment', '-addext', 'extendedKeyUsage=serverAuth', '-keyout', str(key), '-out', str(csr)],
        ['openssl', 'x509', '-req', '-in', str(csr), '-CA', str(ca), '-CAkey', str(ca_key), '-CAcreateserial', '-days', '1', '-copy_extensions', 'copy', '-out', str(cert)],
    ]
    for command in commands:
        check(subprocess.run(command, capture_output=True).returncode == 0, 'local certificate creation failed')
    for private in [key, ca_key]:
        os.chmod(private, 0o600)
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        port = probe.getsockname()[1]
    origin = f'https://localhost:{port}'
    cli_key = secrets.token_urlsafe(32)
    rate_key = secrets.token_urlsafe(32)
    variables = scratch / 'worker.env'
    with variables.open('x') as handle:
        os.chmod(variables, 0o600)
        handle.write(f'APP_ORIGIN={origin}\nRATE_LIMIT_KEY={rate_key}\nDEV_CLI_KEY={cli_key}\n')
    # Use the already-built artifact; Wrangler's custom build exceeds a readiness probe.
    config_source = (ROOT / 'backend/wrangler.toml').read_text()
    build_section = '\n[build]\ncommand = "worker-build --release"\n'
    check(config_source.count(build_section) == 1, 'unexpected Worker custom-build configuration')
    config_source = config_source.replace(build_section, '\n').replace(
        'main = "build/worker/shim.mjs"',
        'main = ' + json.dumps(str(ROOT / 'backend/build/worker/shim.mjs')),
    )
    local_config = scratch / 'worker.toml'
    local_config.write_text(config_source)
    environment = {**os.environ, 'BREWS_API_ORIGIN': origin, 'BREWS_DEV_CLI_KEY': cli_key, 'BREWS_TLS_CA_FILE': str(ca), 'WRANGLER_SEND_METRICS': 'false', 'WRANGLER_LOG_PATH': str(scratch / 'wrangler-logs')}
    tls = ssl.create_default_context(cafile=str(ca))
    completed = []
    sockets = []

    def call(label, path, *, method='POST', body=None, session=None, command=None, retry=True, expected=200, cookie_effect=False, application_origin=origin):
        headers = {}
        if session:
            headers['Cookie'] = session
        if method != 'GET':
            headers['Origin'] = application_origin
            if retry:
                headers['Idempotency-Key'] = command or str(uuid.uuid7())
        if body is not None:
            headers['Content-Type'] = 'application/json'
        request = urllib.request.Request(origin + path, method=method, headers=headers, data=None if body is None else json.dumps(body).encode())
        try:
            response = urllib.request.urlopen(request, context=tls, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            check(response.status == expected, label + ' status mismatch: ' + str(response.status))
            value = json.loads(response.read(256 * 1024))
            check(response.headers.get('Cache-Control') == 'no-store', label + ' no-store policy missing')
            check(response.headers.get('Referrer-Policy') == 'no-referrer', label + ' referrer policy missing')
            raw_cookie = response.headers.get('Set-Cookie')
            check(bool(raw_cookie) == cookie_effect, label + ' cookie effect mismatch')
            if cookie_effect:
                check('Secure' in raw_cookie and 'HttpOnly' in raw_cookie, label + ' cookie protection missing')
        completed.append(label)
        return value, raw_cookie.split(';', 1)[0] if raw_cookie else None

    log_path = scratch / 'worker.log'
    with log_path.open('x') as log:
        os.chmod(log_path, 0o600)
        process = subprocess.Popen([
            str(ROOT / 'backend/node_modules/.bin/wrangler'), 'dev', '--config', str(local_config), '--local', '--local-protocol', 'https',
            '--https-key-path', str(key), '--https-cert-path', str(cert), '--env-file', str(variables), '--ip', '127.0.0.1',
            '--port', str(port), '--persist-to', str(scratch / 'state'), '--log-level', 'warn',
        ], cwd=ROOT / 'backend', env=environment, stdout=log, stderr=subprocess.STDOUT)
        try:
            ready = False
            readiness_error = None
            for _ in range(150):
                check(process.poll() is None, 'local Worker exited before readiness')
                try:
                    with urllib.request.urlopen(origin + '/api/session', context=tls, timeout=1) as response:
                        ready = response.status == 200
                    if ready:
                        break
                except (OSError, urllib.error.URLError) as error:
                    readiness_error = type(getattr(error, 'reason', error)).__name__
                    time.sleep(0.2)
            if not ready:
                startup = log_path.read_text(errors='replace')
                print(json.dumps({'readiness_failed': True, 'last_error_class': readiness_error, 'custom_build_started': 'Compiling' in startup or 'Compiling to Wasm' in startup, 'custom_build_finished': 'Your wasm pkg is ready' in startup, 'worker_ready_logged': 'Ready on' in startup, 'configuration_error_logged': 'configuration unavailable' in startup}), flush=True)
            check(ready, 'local Worker readiness timed out')
            output_file = scratch / 'host.url'
            result = subprocess.run([
                str(ROOT / 'target/debug/brews'), '--json', 'accounts', 'create', '--role', 'admin', '--username', 'GameHttpsAdmin', '--link-output', str(output_file),
            ], env=environment, cwd=ROOT, capture_output=True, timeout=45)
            check(result.returncode == 0, 'CLI game-host provisioning failed')
            check((output_file.stat().st_mode & 0o777) == 0o600, 'enrollment link permissions invalid')
            enrollment_url = output_file.read_text().strip()
            check(enrollment_url.startswith(origin + '/enroll#'), 'enrollment URL origin mismatch')
            _, restricted = call('redeem-host-enrollment', '/api/auth/enrollment/redeem', body={'enrollment_token': enrollment_url.split('#', 1)[1]}, cookie_effect=True)
            host_password = secrets.token_urlsafe(24)
            _, host = call('complete-host-enrollment', '/api/auth/enrollment/complete', body={'new_password': host_password}, session=restricted, cookie_effect=True)
            call('discovery-account-required', '/api/games/current', method='GET', retry=False, expected=401)
            free, _ = call('discovery-free-slot-over-TLS', '/api/games/current', method='GET', session=host, retry=False)
            check(free == {'slot': 'free', 'game': None}, 'free discovery slot mismatch')
            call('discovery-unknown-code-over-TLS', '/api/game-codes/ZZZZZZZZ', method='GET', retry=False, expected=404)
            call('unauthenticated-create-denied', '/api/games', body={}, expected=401)
            call('wrong-Origin-denied', '/api/games', body={}, session=host, application_origin='https://different.example.test', expected=403)
            call('invalid-configuration-before-reservation', '/api/games', body={'configuration': {'numeric_upper_bound': 1}}, session=host, expected=400)
            creation = str(uuid.uuid7())
            created, _ = call('create-New', '/api/games', body={}, session=host, command=creation)
            check(created['result'] == 'created' and created['game']['state'] == 'new', 'creation lifecycle mismatch')
            game_id = created['game']['game_id']
            discovery, _ = call('discovery-New-list-over-TLS', '/api/games?state=new', method='GET', session=host, retry=False)
            check(discovery['slot'] == 'occupied' and len(discovery['games']) == 1 and discovery['next_cursor'] is None, 'New discovery listing mismatch')
            check(discovery['games'][0]['game_id'] == game_id and discovery['games'][0]['game_code'] is None and discovery['games'][0]['may_operate'], 'New summary authority or code mismatch')
            replay, _ = call('create-receipt-only', '/api/games', body={}, session=host, command=creation)
            check(set(replay) == {'result', 'receipt'} and replay['result'] == 'committed', 'creation retry exposed snapshot')
            call('global-reservation-conflict', '/api/games', body={}, session=host, expected=409)
            base = f'/api/games/{game_id}'
            call('lobby-revision-conflict', base + '/lobby', body={'expected_revision': 1}, session=host, expected=409)
            lobby, _ = call('open-lobby', base + '/lobby', body={'expected_revision': 0}, session=host)
            check(lobby['result'] == 'lobby_opened' and lobby['state'] == 'awaiting_players', 'lobby lifecycle mismatch')
            code = lobby['game_code']
            entry, _ = call('discovery-normalized-code-over-TLS', '/api/game-codes/%20' + code.lower() + '%20', method='GET', retry=False)
            check(entry == {'game_id': game_id, 'game_code': code, 'state': 'awaiting_players', 'can_join_player': True, 'can_join_spectator': True}, 'limited normalized code projection mismatch')
            availability, _ = call('discovery-public-availability-over-TLS', base + '/availability?view=entry&game_code=' + code, method='GET', retry=False)
            check(availability == {'view': 'entry', 'state': 'awaiting_players', 'can_join_player': True, 'can_join_spectator': True}, 'public availability leaked detail')
            spectator_context, audience_context_cookie = call('spectator-context-over-TLS', base + '/spectator-admission-context', body={'game_code': code}, retry=False, cookie_effect=True)
            audience_command = str(uuid.uuid7())
            joined_audience, audience_cookie = call('spectator-admission-over-TLS', base + '/spectators', body={'game_code': code}, session=audience_context_cookie, command=audience_command, cookie_effect=True)
            audience_expiry = joined_audience['receipt']['outcome']['session_expires_at']
            for index, selected in enumerate([audience_cookie, audience_context_cookie + '; ' + audience_cookie, audience_cookie + '; ' + audience_context_cookie]):
                reused, _ = call('spectator-current-preflight-natural-cookies-' + str(index), base + '/spectator-admission-context', body={'game_code': code}, session=selected, retry=False)
                check(reused == {'result': 'admission_context', 'expires_at': audience_expiry}, 'current spectator preflight changed absolute expiry')
            value_start = audience_cookie.index('=') + 1
            bad_audience = audience_cookie[:value_start] + ('B' if audience_cookie[value_start] == 'A' else 'A') + audience_cookie[value_start + 1:]
            for index, selected in enumerate([bad_audience, bad_audience + '; ' + audience_context_cookie]):
                denied, cleared = call('spectator-invalid-selected-preflight-proof-' + str(index), base + '/spectator-admission-context', body={'game_code': code}, session=selected, retry=False, expected=401, cookie_effect=True)
                check(set(denied) == {'error'} and cleared == '__Secure-brews-spectator=', 'invalid spectator proof granted authority or cleared another cookie')
            repeated_audience, _ = call('spectator-retry-no-cookie-over-TLS', base + '/spectators', body={'game_code': code}, session=audience_context_cookie, command=audience_command)
            check(repeated_audience == {'result': 'committed', 'receipt': joined_audience['receipt']}, 'audience receipt replay mismatch')
            call('spectator-anonymous-retry-denied-over-TLS', base + '/spectators', body={'game_code': code}, command=audience_command, expected=401)
            call('spectator-mixed-cookie-selector-required-over-TLS', base + '/sync', method='GET', session=host + '; ' + audience_cookie, retry=False, expected=400)
            own_audience, _ = call('spectator-explicit-selector-over-TLS', base + '/sync?view=spectator', method='GET', session=host + '; ' + audience_cookie, retry=False)
            check(own_audience['snapshot']['role'] == 'spectator' and all(key not in own_audience['snapshot'] for key in ['board', 'players', 'alias', 'player_id']), 'audience private isolation failed')
            promoted, promoted_cookie = call('spectator-to-player-over-TLS', base + '/membership/player', body={'game_code': code, 'alias': 'SwitchTemporary', 'recovery_answer': secrets.token_urlsafe(18)}, session=audience_cookie, cookie_effect=True)
            check(promoted['receipt']['outcome']['session_expires_at'] == audience_expiry, 'spectator to player renewed absolute expiry')
            call('switched-old-spectator-cookie-denied-over-TLS', base + '/sync?view=spectator', method='GET', session=audience_cookie, retry=False, expected=401)
            demoted, audience_cookie = call('player-to-spectator-over-TLS', base + '/membership/spectator', body={}, session=promoted_cookie, cookie_effect=True)
            check(demoted['receipt']['outcome']['session_expires_at'] == audience_expiry, 'player to spectator renewed absolute expiry')
            call('switched-old-player-cookie-denied-over-TLS', base + '/sync?view=player', method='GET', session=promoted_cookie, retry=False, expected=401)
            audience_socket = SocketClient('localhost', port, tls, base + '/stream?view=spectator', origin, audience_cookie)
            audience_first = audience_socket.frame()
            check(audience_first['view']['role'] == 'spectator' and audience_first['view']['calls'] == [], 'audience WSS initial projection mismatch')
            completed.append('spectator-real-WSS-initial-projection')
            call('Start-needs-real-presence', base + '/start', body={'expected_revision': lobby['view_revision']}, session=host, expected=409)

            def join(alias, answer=None, *, label=None):
                label = alias if label is None else label
                _, context = call('admission-context-' + label, base + '/admission-context', body={'game_code': code}, retry=False, cookie_effect=True)
                body = {'game_code': code, 'alias': alias}
                if answer is not None:
                    body['recovery_answer'] = answer
                join_id = str(uuid.uuid7())
                player, player_cookie = call('join-' + label, base + '/players', body=body, session=context, command=join_id, cookie_effect=True)
                replay, _ = call('join-receipt-only-' + label, base + '/players', body=body, session=context, command=join_id)
                check(set(replay) == {'result', 'receipt'} and replay['result'] == 'committed', 'join retry replayed private result')
                return player, player_cookie

            alice_answer = secrets.token_urlsafe(24)
            alice, alice_cookie = join('Alice', alice_answer)
            bob, bob_cookie = join('Bob')
            retained, retained_cookie = join('alice')
            alias_path = base + '/membership/alias'
            leave_path = base + '/membership/leave'
            call('alias-account-cookie-is-not-player-over-TLS', alias_path, method='PATCH', body={'alias': 'Alice Updated!'}, session=host, expected=401)
            rename_command = str(uuid.uuid7())
            renamed, _ = call('rename-own-alias-over-TLS', alias_path, method='PATCH', body={'alias': ' Alice Updated! '}, session=alice_cookie, command=rename_command)
            check(renamed['player_id'] == alice['player_id'] and renamed['alias'] == 'Alice Updated!' and renamed['session_expires_at'] == alice['session_expires_at'], 'rename changed identity or original expiry')
            replayed_alias, _ = call('rename-canonical-receipt-only-over-TLS', alias_path, method='PATCH', body={'alias': 'Alice Updated!'}, session=alice_cookie, command=rename_command)
            check(replayed_alias == {'result': 'committed', 'receipt': renamed['receipt']}, 'rename retry replayed data or changed receipt')
            call('rename-conflicting-command-over-TLS', alias_path, method='PATCH', body={'alias': 'Different'}, session=alice_cookie, command=rename_command, expected=409)
            call('rename-claimed-alias-over-TLS', alias_path, method='PATCH', body={'alias': 'Bob'}, session=alice_cookie, expected=409)
            renamed_view, _ = call('rename-same-cookie-current-alias-over-TLS', base + '/sync?view=player', method='GET', session=alice_cookie, retry=False)
            check(renamed_view['snapshot']['player_id'] == alice['player_id'] and renamed_view['snapshot']['alias'] == renamed['alias'], 'existing cookie did not follow stable renamed membership')
            call('rename-previous-alias-recovery-denied-over-TLS', '/api/player-recovery', body={'game_code': code, 'alias': 'Alice', 'recovery_answer': alice_answer}, expected=401)
            reset_alias, _ = call('rename-restore-alias-over-TLS', alias_path, method='PATCH', body={'alias': 'Alice'}, session=alice_cookie)
            check(reset_alias['session_expires_at'] == alice['session_expires_at'], 'rename restoration renewed session')
            same_alias, _ = call('rename-self-no-op-over-TLS', alias_path, method='PATCH', body={'alias': ' Alice '}, session=alice_cookie)
            check(same_alias['view_revision'] == reset_alias['view_revision'], 'self rename bumped own revision')
            departing_answer = secrets.token_urlsafe(24)
            departing, departing_cookie = join('Departing', departing_answer)
            departing_socket = SocketClient('localhost', port, tls, base + '/stream?view=player', origin, departing_cookie)
            try:
                departing_socket.frame()
                lobby_leave_command = str(uuid.uuid7())
                left_lobby, cleared = call('player-lobby-Leave-over-TLS', leave_path, body={'expected_state': 'awaiting_players'}, session=departing_cookie, command=lobby_leave_command, cookie_effect=True)
                check(not left_lobby['membership_retained'] and left_lobby['fresh_admission_required'] and cleared == '__Secure-brews-player=', 'lobby Leave did not delete authority and clear only player cookie')
                call('lobby-Leave-old-cookie-denied-over-TLS', base + '/sync?view=player', method='GET', session=departing_cookie, retry=False, expected=401)
                call('lobby-Leave-old-answer-denied-over-TLS', '/api/player-recovery', body={'game_code': code, 'alias': 'Departing', 'recovery_answer': departing_answer}, expected=401)
                call('lobby-Leave-deleted-authority-retry-denied-over-TLS', leave_path, body={'expected_state': 'awaiting_players'}, session=departing_cookie, command=lobby_leave_command, expected=401)
                for _ in range(20):
                    departing_frame = departing_socket.frame(allow_close=True)
                    if 'transport_close_code' in departing_frame:
                        check(departing_frame['transport_close_code'] in [1008, 1013], 'lobby Leave predecessor was not fenced')
                        break
                else:
                    raise RuntimeError('lobby Leave predecessor close exceeded bounded backlog')
                completed.append('lobby-Leave-predecessor-real-WSS-closed')
            finally:
                departing_socket.close()
            fresh_departing, fresh_departing_cookie = join('Departing', label='Departing-again')
            check(fresh_departing['player_id'] != departing['player_id'], 'fresh admission restored deleted identity')
            call('fresh-admitted-player-lobby-Leave-over-TLS', leave_path, body={'expected_state': 'awaiting_players'}, session=fresh_departing_cookie, cookie_effect=True)
            own, _ = call('player-before-Start-no-board', base + '/sync?view=player', method='GET', session=alice_cookie, retry=False)
            check(own['snapshot']['role'] == 'player' and 'board' not in own['snapshot'], 'prestart player board leaked')
            answer_path = base + '/membership/recovery-answer'
            replacement_answer = secrets.token_urlsafe(24)
            call('recovery-answer-account-is-not-owner-over-TLS', answer_path, method='PUT', body={'recovery_answer': replacement_answer}, session=host, expected=401)
            answer_command = str(uuid.uuid7())
            changed, _ = call('recovery-answer-replace-over-TLS', answer_path, method='PUT', body={'recovery_answer': replacement_answer}, session=alice_cookie, command=answer_command)
            check(changed['recovery_enabled'], 'replacement did not enable recovery')
            repeated, _ = call('recovery-answer-receipt-only-over-TLS', answer_path, method='PUT', body={'recovery_answer': replacement_answer}, session=alice_cookie, command=answer_command)
            check(repeated == {'result': 'committed', 'receipt': changed['receipt']}, 'answer receipt replay leaked data')
            call('recovery-old-answer-denied-over-TLS', '/api/player-recovery', body={'game_code': code, 'alias': 'Alice', 'recovery_answer': alice_answer}, expected=401)
            removed, _ = call('recovery-answer-delete-over-TLS', answer_path, method='DELETE', body={}, session=alice_cookie)
            check(not removed['recovery_enabled'], 'answer deletion kept recovery enabled')
            call('recovery-deleted-answer-denied-over-TLS', '/api/player-recovery', body={'game_code': code, 'alias': 'Alice', 'recovery_answer': replacement_answer}, expected=401)
            call('recovery-answer-restore-over-TLS', answer_path, method='PUT', body={'recovery_answer': alice_answer}, session=alice_cookie)
            for session in [alice_cookie, bob_cookie]:
                stream = SocketClient('localhost', port, tls, base + '/stream?view=player', origin, session)
                sockets.append(stream)
                initial = stream.frame()
                check(initial['view']['state'] == 'awaiting_players' and 'board' not in initial['view'], 'initial stream board leaked')
            completed.append('two-real-TLS-player-streams')
            current, _ = call('host-reads-connected-presence', base + '/sync?view=account', method='GET', session=host, retry=False)
            check(current['snapshot']['connected_player_count'] == 2, 'Start presence is not two real players')
            availability, _ = call('discovery-connected-Start-availability-over-TLS', base + '/availability?view=account', method='GET', session=host, retry=False)
            check(availability['can_start'] and availability['connected_player_count'] == 2 and availability['player_count'] == 3, 'discovery Start eligibility does not reflect real sockets/retained members')
            own_availability, _ = call('discovery-player-availability-over-TLS', base + '/availability?view=player', method='GET', session=alice_cookie, retry=False)
            check(own_availability == {'view': 'player', 'state': 'awaiting_players', 'can_join_player': True, 'can_join_spectator': True}, 'player availability leaked account details')
            start_id = str(uuid.uuid7())
            started, _ = call('Start-InProgress', base + '/start', body={'expected_revision': current['view_revision']}, session=host, command=start_id)
            check(started['result'] == 'started' and started['state'] == 'in_progress', 'Start did not commit')
            discovery, _ = call('discovery-filtered-list-preserves-occupied-slot', '/api/games?state=new', method='GET', session=host, retry=False)
            check(discovery == {'slot': 'occupied', 'games': [], 'next_cursor': None}, 'filtered empty list falsely released the slot')
            entry, _ = call('discovery-InProgress-code-over-TLS', '/api/game-codes/' + code, method='GET', retry=False)
            check(entry['state'] == 'in_progress' and not entry['can_join_player'], 'InProgress discovery admitted a new player')
            received = []
            for stream in sockets:
                for _ in range(20):
                    frame = stream.frame()
                    if frame['view']['state'] == 'in_progress':
                        received.append(frame)
                        break
                else:
                    raise RuntimeError('Start snapshot not delivered within bounded stream backlog')
            check(received[0]['view']['player_id'] == alice['player_id'] and received[1]['view']['player_id'] == bob['player_id'], 'stream delivered another player identity')
            check(all(len(frame['view']['board']['cells']) == 25 and 'players' not in frame['view'] for frame in received), 'private snapshot boundary failed')
            completed.append('own-board-WSS-delivery-and-ACK')
            final, _ = call('host-retained-roster-boards', base + '/sync?view=account', method='GET', session=host, retry=False)
            boards_before = {player['player_id']: player['board'] for player in final['snapshot']['players']}
            check(set(boards_before) == {alice['player_id'], bob['player_id'], retained['player_id']}, 'disconnected retained player lost Start board')
            running_context, running_context_cookie = call('running-spectator-context-over-TLS', base + '/spectator-admission-context', body={'game_code': code}, retry=False, cookie_effect=True)
            running_audience, running_cookie = call('running-spectator-admission-over-TLS', base + '/spectators', body={'game_code': code}, session=running_context_cookie, cookie_effect=True)
            call('running-spectator-switch-denied-over-TLS', base + '/membership/player', body={'game_code': code, 'alias': 'LateSwitch'}, session=running_cookie, expected=409)
            call('spectator-delayed-lobby-Leave-conflict-over-TLS', base + '/membership/leave', body={'expected_state': 'awaiting_players'}, session=running_cookie, expected=409)
            left_audience, cleared_audience = call('spectator-running-explicit-Leave-over-TLS', base + '/membership/leave', body={'expected_state': 'in_progress'}, session=running_cookie, cookie_effect=True)
            check(left_audience['receipt']['outcome']['change'] == 'left' and cleared_audience == '__Secure-brews-spectator=', 'spectator Leave cleanup cookie mismatch')
            call('spectator-Leave-old-cookie-denied-over-TLS', base + '/sync?view=spectator', method='GET', session=running_cookie, retry=False, expected=401)
            # Model a lost Leave response: the browser still carries both old cookies.
            if not isinstance(host, str) or not isinstance(running_context_cookie, str) or not isinstance(running_cookie, str):
                raise RuntimeError('spectator reentry cookie fixture missing')
            denied_entry, stale_cookie_clear = call('spectator-stale-browser-entry-denial-clears-cookie-over-TLS', base + '/spectator-admission-context', body={'game_code': code}, session=host + '; ' + running_context_cookie + '; ' + running_cookie, retry=False, expected=401, cookie_effect=True)
            check(set(denied_entry) == {'error'} and stale_cookie_clear == '__Secure-brews-spectator=', 'stale cookie denial issued authority or cleared another cookie')
            _, fresh_context_cookie = call('spectator-browser-fresh-context-after-clearing-over-TLS', base + '/spectator-admission-context', body={'game_code': code}, session=host + '; ' + running_context_cookie, retry=False, cookie_effect=True)
            if not isinstance(fresh_context_cookie, str):
                raise RuntimeError('fresh spectator context cookie missing')
            fresh_audience, fresh_audience_cookie = call('spectator-browser-fresh-admission-after-clearing-over-TLS', base + '/spectators', body={'game_code': code}, session=host + '; ' + fresh_context_cookie, cookie_effect=True)
            if not isinstance(fresh_audience_cookie, str):
                raise RuntimeError('fresh spectator session cookie missing')
            check(fresh_audience['receipt']['outcome']['identity']['spectator_id'] != running_audience['receipt']['outcome']['identity']['spectator_id'], 'stale cookie reentry resurrected deleted spectator identity')
            fresh_view, _ = call('spectator-browser-fresh-role-private-isolation-over-TLS', base + '/sync?view=spectator', method='GET', session=host + '; ' + fresh_audience_cookie, retry=False)
            check(fresh_view['snapshot']['role'] == 'spectator' and all(key not in fresh_view['snapshot'] for key in ['players', 'board', 'alias', 'recovery_answer', 'qualifiers']), 'fresh spectator entry disclosed private data')
            call('spectator-browser-fresh-role-Leave-over-TLS', base + '/membership/leave', body={'expected_state': 'in_progress'}, session=fresh_audience_cookie, cookie_effect=True)
            replay, _ = call('Start-receipt-only', base + '/start', body={'expected_revision': current['view_revision']}, session=host, command=start_id)
            check(set(replay) == {'result', 'receipt'} and replay['result'] == 'committed', 'Start retry exposed assignments')
            final_again, _ = call('Start-retry-does-not-regenerate', base + '/sync?view=account', method='GET', session=host, retry=False)
            check(boards_before == {player['player_id']: player['board'] for player in final_again['snapshot']['players']}, 'Start retry regenerated boards')
            own, _ = call('own-view-unchanged-sync', base + '/sync?view=player&known_revision=' + str(received[0]['view_revision']), method='GET', session=alice_cookie, retry=False)
            check(own['up_to_date'] and own['snapshot'] is None, 'private revision unchanged contract failed')
            recovery_command = str(uuid.uuid7())
            recovery_input = {'game_code': ' ' + code.lower() + ' ', 'alias': ' Alice ', 'recovery_answer': alice_answer}
            restored, recovered_cookie = call('recover-InProgress-player-over-TLS', '/api/player-recovery', body=recovery_input, command=recovery_command, cookie_effect=True)
            check(restored['receipt']['game_id'] == game_id, 'recovery receipt game binding mismatch')
            call('recovered-predecessor-session-denied-over-TLS', base + '/sync?view=player', method='GET', session=alice_cookie, retry=False, expected=401)
            recovered_view, _ = call('recovered-same-player-and-board-over-TLS', base + '/sync?view=player', method='GET', session=recovered_cookie, retry=False)
            check(recovered_view['snapshot']['player_id'] == alice['player_id'] and recovered_view['snapshot']['board'] == boards_before[alice['player_id']], 'recovery replaced player or board')
            repeated, _ = call('recovery-secret-free-receipt-only-over-TLS', '/api/player-recovery', body=recovery_input, command=recovery_command)
            check(repeated == {'receipt': restored['receipt']}, 'recovery replay exposed cookie/snapshot')
            call('recovery-retry-still-needs-answer-over-TLS', '/api/player-recovery', body={**recovery_input, 'recovery_answer': secrets.token_urlsafe(24)}, command=recovery_command, expected=401)
            old_stream = sockets[0]
            for _ in range(20):
                frame = old_stream.frame(allow_close=True)
                if 'transport_close_code' in frame:
                    check(frame['transport_close_code'] in [1008, 1013], 'recovery did not fence predecessor transport')
                    break
                check(frame['view_revision'] <= received[0]['view_revision'], 'predecessor received a post-recovery snapshot')
            else:
                raise RuntimeError('recovery predecessor close was not acknowledged within bounded backlog')
            old_stream.close()
            completed.append('recovery-predecessor-WSS-close-acknowledged')
            alice_cookie = recovered_cookie
            sockets[0] = SocketClient('localhost', port, tls, base + '/stream?view=player', origin, alice_cookie)
            successor = sockets[0].frame()
            check(successor['view']['player_id'] == alice['player_id'] and successor['view']['board'] == boards_before[alice['player_id']], 'recovery successor stream lost board')
            completed.append('recovery-successor-real-WSS-same-board')
            call('rename-after-Start-denied-over-TLS', alias_path, method='PATCH', body={'alias': 'TooLate'}, session=alice_cookie, expected=409)
            call('delayed-lobby-Leave-after-Start-conflicts-over-TLS', leave_path, body={'expected_state': 'awaiting_players'}, session=bob_cookie, expected=409)
            bob_leave_command = str(uuid.uuid7())
            left_game, _ = call('player-InProgress-Leave-over-TLS', leave_path, body={'expected_state': 'in_progress'}, session=bob_cookie, command=bob_leave_command)
            check(left_game['membership_retained'] and not left_game['fresh_admission_required'], 'in-progress Leave did not retain return authority')
            for _ in range(20):
                departing_frame = sockets[1].frame(allow_close=True)
                if 'transport_close_code' in departing_frame:
                    check(departing_frame['transport_close_code'] in [1008, 1013], 'in-progress Leave did not fence predecessor socket')
                    break
            else:
                raise RuntimeError('in-progress Leave predecessor close exceeded bounded backlog')
            sockets[1].close()
            completed.append('InProgress-Leave-predecessor-real-WSS-closed')
            away_host, _ = call('InProgress-Leave-host-presence-over-TLS', base + '/sync?view=account', method='GET', session=host, retry=False)
            away_bob = next(player for player in away_host['snapshot']['players'] if player['player_id'] == bob['player_id'])
            check(not away_bob['connected'] and away_bob['board'] == boards_before[bob['player_id']], 'in-progress Leave lost board or kept detached presence')
            sockets[1] = SocketClient('localhost', port, tls, base + '/stream?view=player', origin, bob_cookie)
            bob_return = sockets[1].frame()
            check(bob_return['session_expires_at'] == bob['session_expires_at'] and bob_return['view']['board'] == boards_before[bob['player_id']], 'same-session return renewed expiry or regenerated board')
            completed.append('InProgress-Leave-same-session-real-WSS-return')
            repeated_leave, _ = call('InProgress-Leave-receipt-only-over-TLS', leave_path, body={'expected_state': 'in_progress'}, session=bob_cookie, command=bob_leave_command)
            check(repeated_leave == {'result': 'committed', 'receipt': left_game['receipt']}, 'Leave retry repeated effects or leaked live state')
            returned_host, _ = call('InProgress-Leave-retry-preserves-returned-presence-over-TLS', base + '/sync?view=account', method='GET', session=host, retry=False)
            check(next(player for player in returned_host['snapshot']['players'] if player['player_id'] == bob['player_id'])['connected'], 'same-command retry detached successor')
            retained_leave, _ = call('retained-offline-explicit-Leave-over-TLS', leave_path, body={'expected_state': 'in_progress'}, session=retained_cookie)
            check(retained_leave['membership_retained'], 'offline explicit Leave removed winner eligibility')
            call('new-player-after-Start-denied', base + '/admission-context', body={'game_code': code}, retry=False, expected=409)
            host_view, _ = call('gameplay-current-host-revision', base + '/sync?view=account', method='GET', session=host, retry=False)
            random_id = str(uuid.uuid7())
            random_input = {'expected_revision': host_view['view_revision']}
            drawn, _ = call('random-call', base + '/calls/random', body=random_input, session=host, command=random_id)
            check(drawn['result'] == 'call_accepted' and drawn['call']['sequence_no'] == 1, 'random call not committed')
            repeat, _ = call('random-receipt-only', base + '/calls/random', body=random_input, session=host, command=random_id)
            check(repeat == {'result': 'committed', 'receipt': drawn['receipt']}, 'random retry drew again')
            call('stale-random-call-denied', base + '/calls/random', body=random_input, session=host, expected=409)
            # Qualify the retained offline player using their actual assigned first row.
            selected = boards_before[retained['player_id']]
            row_values = [cell['kind']['value'] for cell in selected['cells'] if cell['position']['row'] == 1 and cell['kind']['kind'] == 'value']
            called = {drawn['call']['value']}
            for value in row_values:
                if value in called:
                    continue
                host_view, _ = call('manual-revision-' + value, base + '/sync?view=account', method='GET', session=host, retry=False)
                manual_id = str(uuid.uuid7())
                manual_input = {'value': value, 'expected_revision': host_view['view_revision']}
                accepted, _ = call('manual-call-' + value, base + '/calls/manual', body=manual_input, session=host, command=manual_id)
                check(accepted['call']['value'] == value, 'manual call normalized or replaced value')
                called.add(value)
                repeat, _ = call('manual-receipt-only-' + value, base + '/calls/manual', body=manual_input, session=host, command=manual_id)
                check(repeat == {'result': 'committed', 'receipt': accepted['receipt']}, 'manual retry advanced game')
            qualified, _ = call('retained-offline-qualification', base + '/sync?view=player', method='GET', session=retained_cookie, retry=False)
            check(qualified['snapshot']['board']['qualified'], 'offline board was not evaluated')
            host_view, _ = call('winner-current-host-revision', base + '/sync?view=account', method='GET', session=host, retry=False)
            winner_id = str(uuid.uuid7())
            winner_input = {'player_id': retained['player_id'], 'expected_revision': host_view['view_revision']}
            resolved, _ = call('winner-commits-Resolved', base + '/winner', body=winner_input, session=host, command=winner_id)
            check(resolved['state'] == 'resolved' and resolved['winner'] == {'player_id': retained['player_id'], 'alias': 'alice'}, 'selected winner mismatch')
            check(resolved['history_available'] and resolved['history_expires_at'] > resolved['ended_at'], 'terminal History deadline missing')
            for _ in range(30):
                public_frame = audience_socket.frame(allow_terminal=True)
                if public_frame.get('kind') == 'terminal_notice':
                    check(public_frame['state'] == 'resolved' and public_frame['winner'] == resolved['winner'] and {item['value'] for item in public_frame['calls']} == called, 'audience terminal public outcome mismatch')
                    break
            else:
                raise RuntimeError('spectator terminal notice did not arrive within bounded backlog')
            completed.append('spectator-terminal-notice-over-real-WSS')
            audience_socket.close()
            call('spectator-terminal-sync-denied-over-TLS', base + '/sync?view=spectator', method='GET', session=audience_cookie, retry=False, expected=401)
            call('discovery-terminal-code-denied-over-TLS', '/api/game-codes/' + code, method='GET', retry=False, expected=404)
            free, _ = call('discovery-terminal-release-over-TLS', '/api/games/current', method='GET', session=host, retry=False)
            check(free == {'slot': 'free', 'game': None}, 'terminal discovery reservation not released')
            repeat, _ = call('winner-receipt-only', base + '/winner', body=winner_input, session=host, command=winner_id)
            check(repeat == {'result': 'committed', 'receipt': resolved['receipt']}, 'winner retry changed result')
            for index, stream in enumerate(sockets):
                for _ in range(20):
                    frame = stream.frame()
                    if frame['view']['state'] == 'resolved':
                        check(frame['view']['player_id'] == [alice, bob][index]['player_id'], 'terminal private identity mismatch')
                        check('players' not in frame['view'] and frame['view']['winner'] == resolved['winner'], 'terminal private boundary failed')
                        break
                else:
                    raise RuntimeError('final snapshot not delivered within bounded stream backlog')
            completed.append('final-private-WSS-result-delivery')
            call('terminal-call-denied', base + '/calls/random', body={'expected_revision': resolved['view_revision']}, session=host, expected=409)
            offline_final, _ = call('offline-final-read', base + '/sync?view=player', method='GET', session=retained_cookie, retry=False)
            check(offline_final['snapshot']['state'] == 'resolved', 'existing offline terminal access lost')
            history_path = '/api/history/' + game_id
            detail, _ = call('History-detail-over-real-TLS', history_path, method='GET', session=host, retry=False)
            history = detail['history']
            check(set(history) == {'game_id','game_code','designated_host_id','outcome','started_at','ended_at','expires_at','winner','ordered_calls','players'}, 'History detail projection leaked final-view fields')
            check(history['winner'] == resolved['winner'] and len(history['players']) == 3, 'History snapshot lost retained boards or winner')
            listing, _ = call('History-list-over-real-TLS', '/api/history?limit=1&outcome=resolved', method='GET', session=host, retry=False)
            check(len(listing['games']) == 1 and listing['games'][0]['game_id'] == game_id and listing['next_cursor'] is None, 'History list projection mismatch')
            call('History-player-cookie-denied', history_path, method='GET', session=alice_cookie, retry=False, expected=401)
            call('History-account-Exit', base + '/exit?view=account', body={}, session=host)
            _, fresh_host = call('History-fresh-login-after-Exit', '/api/auth/login', body={'username':'GameHttpsAdmin','password':host_password}, cookie_effect=True)
            after_exit, _ = call('History-independent-of-final-grants', history_path, method='GET', session=fresh_host, retry=False)
            check(after_exit == detail, 'History changed or final grants were required after Exit')
            for state in ['new', 'awaiting_players']:
                created_next, _ = call('create-after-terminal-' + state, '/api/games', body={}, session=host)
                next_base = '/api/games/' + created_next['game']['game_id']
                if state == 'awaiting_players':
                    call('open-next-lobby', next_base + '/lobby', body={'expected_revision': 0}, session=host)
                cancel_id = str(uuid.uuid7())
                cancel_input = {'confirmed': True, 'expected_state': state}
                cancelled, _ = call('confirmed-prestart-cancel-' + state, next_base + '/cancel', body=cancel_input, session=host, command=cancel_id)
                check(cancelled['state'] == 'cancelled' and not cancelled['history_available'] and cancelled['history_expires_at'] is None, 'prestart cancellation retained History')
                repeat, _ = call('cancel-receipt-only-' + state, next_base + '/cancel', body=cancel_input, session=host, command=cancel_id)
                check(repeat == {'result': 'committed', 'receipt': cancelled['receipt']}, 'cancel retry did not preserve result')
            check(len(completed) == len(set(completed)), 'acceptance labels are duplicated')
            print(json.dumps({'actual_https_wss_game_checks': 'passed', 'checks': completed, 'check_count': len(completed), 'genuine_connected_players': 2, 'retained_boards': len(boards_before), 'board_replay': False, 'cookie_replay': False, 'deployment': 'none'}))
        finally:
            if 'audience_socket' in locals():
                audience_socket.close()
            for stream in sockets:
                stream.close()
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
