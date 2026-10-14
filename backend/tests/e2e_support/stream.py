"""Bounded RFC6455 reader, masked ACK sender and continuous snapshot consumer."""
import base64
from collections import deque
import hashlib
import json
import re
import secrets
import socket
import struct
import threading
import time
from .oracle import call_values
from .safety import MAX_JSON_BYTES, WAIT_TIMEOUT, E2EError, identifier, json_object, require, safe_integer


class Transport:
    def __init__(self, sock):
        self.socket = sock
        self.send_lock = threading.Lock()
        self.stop = threading.Event()

    def read_exact(self, size, *, idle=False):
        result = bytearray()
        deadline = time.monotonic() + 10
        while len(result) < size:
            require(not self.stop.is_set(), 'wss_stopped')
            try:
                chunk = self.socket.recv(size - len(result))
            except socket.timeout:
                if idle and not result:
                    deadline = time.monotonic() + 10
                    continue
                require(time.monotonic() < deadline, 'wss_frame_timeout')
                continue
            require(bool(chunk), 'wss_transport_closed')
            result.extend(chunk)
            require(time.monotonic() < deadline, 'wss_frame_timeout')
        return bytes(result)

    def send(self, opcode, content):
        require(opcode in (1, 8, 10) and len(content) <= 512 and
                (opcode == 1 or len(content) <= 125), 'wss_client_frame_bound')
        mask = secrets.token_bytes(4)
        size = len(content)
        header = bytes((128 | opcode, 128 | size)) if size < 126 else (
            bytes((128 | opcode, 128 | 126)) + struct.pack('!H', size))
        data = bytes(byte ^ mask[index % 4] for index, byte in enumerate(content))
        with self.send_lock:
            self.socket.sendall(header + mask + data)

    def message(self):
        chunks = bytearray()
        fragments = 0
        active = False
        message_deadline = None
        controls = 0
        while True:
            first, second = self.read_exact(2, idle=not active)
            require(not first & 112 and not second & 128, 'wss_frame_flags')
            final, opcode = bool(first & 128), first & 15
            size = second & 127
            if size == 126:
                size = struct.unpack('!H', self.read_exact(2))[0]
                require(size >= 126, 'wss_noncanonical_size')
            elif size == 127:
                size = struct.unpack('!Q', self.read_exact(8))[0]
                require(65536 <= size <= MAX_JSON_BYTES, 'wss_frame_bound')
            require(size <= MAX_JSON_BYTES, 'wss_frame_bound')
            if opcode >= 8:
                require(final and size <= 125 and opcode in (8, 9, 10), 'wss_control_frame')
                controls += 1
                require(controls <= 64, 'wss_control_bound')
                content = self.read_exact(size)
                if opcode == 9:
                    self.send(10, content)
                elif opcode == 8:
                    require(len(content) != 1, 'wss_close_invalid')
                    code = None
                    if content:
                        code = struct.unpack('!H', content[:2])[0]
                        require(code in (1000, 1001, 1002, 1003, 1007, 1008, 1009, 1010, 1011,
                                         1012, 1013, 1014) or 3000 <= code <= 4999, 'wss_close_invalid')
                        try:
                            content[2:].decode('utf8')
                        except UnicodeError:
                            raise E2EError('wss_close_invalid') from None
                    self.send(8, content[:2])
                    return {'transport_close_code': code}
                continue
            require(opcode in (0, 1) and ((opcode == 0 and active) or (opcode == 1 and not active)),
                    'wss_fragment_order')
            if not active:
                message_deadline = time.monotonic() + 10
            active = True
            fragments += 1
            require(fragments <= 32 and len(chunks) + size <= MAX_JSON_BYTES, 'wss_message_bound')
            chunks.extend(self.read_exact(size))
            require(time.monotonic() <= message_deadline, 'wss_message_timeout')
            if final:
                return json_object(bytes(chunks))

    def close(self):
        self.stop.set()
        try:
            self.send(8, struct.pack('!H', 1000))
        except OSError:
            pass
        try:
            self.socket.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.socket.close()


def validate_view(view, game_id, role, actor_id):
    require(type(view) is dict, 'wss_view_shape')
    if role == 'account':
        require(view.get('role') in ('host', 'final_host'), 'wss_host_identity')
        require((view.get('game', {}).get('game_id') if view['role'] == 'host' else view.get('game_id'))
                == game_id, 'wss_game_identity')
    elif role == 'player':
        allowed = {'role', 'game_id', 'state', 'configuration', 'game_code', 'started_at', 'ended_at',
                   'view_revision', 'player_id', 'alias', 'calls', 'board', 'history_expires_at', 'winner'}
        require(set(view) <= allowed and view.get('role') in ('player', 'final_player') and
                view.get('player_id') == actor_id and view.get('game_id') == game_id, 'wss_player_privacy')
    else:
        allowed = {'role', 'game_id', 'state', 'configuration', 'game_code', 'started_at',
                   'view_revision', 'spectator_id', 'calls', 'latest_call', 'exhausted'}
        require(set(view) <= allowed and view.get('role') == 'spectator' and
                view.get('spectator_id') == actor_id and view.get('game_id') == game_id, 'wss_spectator_privacy')
    call_values(view.get('calls', []))


class Stream:
    def __init__(self, actor, game_id, role, actor_id=None):
        self.game_id, self.role, self.actor_id = identifier(game_id), role, actor_id
        path = '/api/games/' + game_id + '/stream?view=' + role
        target = actor.target
        raw = None
        wrapped = None
        try:
            raw = socket.create_connection((target.host, target.port), timeout=10)
            wrapped = target.tls().wrap_socket(raw, server_hostname=target.host)
            wrapped.settimeout(0.5)
            self.transport = Transport(wrapped)
            key = base64.b64encode(secrets.token_bytes(16)).decode('ascii')
            cookie = actor.cookie_header(path)
            request = (f'GET {path} HTTP/1.1\r\nHost: {target.origin[8:]}\r\n'
                       'Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\n'
                       f'Sec-WebSocket-Key: {key}\r\nOrigin: {target.origin}\r\nCookie: {cookie}\r\n\r\n')
            wrapped.sendall(request.encode('ascii'))
            header = bytearray()
            deadline = time.monotonic() + 10
            while not header.endswith(b'\r\n\r\n'):
                require(len(header) < 16384 and time.monotonic() < deadline, 'wss_handshake_bound')
                header.extend(self.transport.read_exact(1))
            try:
                lines = header.decode('ascii').split('\r\n')
                require(lines[0].split(' ')[1] == '101', 'wss_upgrade_rejected')
                fields = {}
                for line in lines[1:]:
                    if not line:
                        continue
                    name, value = line.split(':', 1)
                    name = name.lower()
                    require(name not in fields, 'wss_duplicate_header')
                    fields[name] = value.strip()
            except (ValueError, UnicodeError, IndexError):
                raise E2EError('wss_handshake_invalid') from None
            accept = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11')
                                                  .encode('ascii')).digest()).decode('ascii')
            require(fields.get('sec-websocket-accept') == accept and fields.get('upgrade', '').lower() ==
                    'websocket' and 'upgrade' in fields.get('connection', '').lower().split(', '), 'wss_handshake_invalid')
            self.condition = threading.Condition()
            self.frames = deque(maxlen=128)
            self.failure = None
            self.closed = False
            self.last_revision = -1
            self.last_view = None
            self.connection_id = None
            self.session_expires_at = None
            self.ack_count = 0
            self.thread = threading.Thread(target=self._read, name='brews-e2e-wss', daemon=True)
            self.thread.start()
        except BaseException:
            if wrapped:
                wrapped.close()
            elif raw:
                raw.close()
            raise

    def _accept(self, frame):
        if frame.get('kind') == 'terminal_notice':
            require(self.role == 'spectator' and set(frame) == {'version', 'kind', 'game_id', 'state',
                    'ended_at', 'objective', 'calls', 'winner'} and frame['version'] == 1 and
                    frame['game_id'] == self.game_id and frame['state'] in ('resolved', 'cancelled'),
                    'spectator_terminal_privacy')
            call_values(frame['calls'])
            winner = frame['winner']
            require(winner is None or (type(winner) is dict and set(winner) == {'player_id', 'alias'}),
                    'spectator_terminal_privacy')
            return
        require(set(frame) == {'version', 'kind', 'game_id', 'view_revision', 'connection_id',
                'session_expires_at', 'view', 'delivery_id'} and frame['version'] == 1 and
                frame['kind'] == 'snapshot' and list(frame)[-1] == 'delivery_id' and
                frame['game_id'] == self.game_id and safe_integer(frame['view_revision']) and
                safe_integer(frame['session_expires_at'], 1), 'wss_snapshot_envelope')
        connection = identifier(frame['connection_id'])
        require(self.connection_id in (None, connection), 'wss_connection_identity')
        self.connection_id = connection
        require(self.session_expires_at is None or frame['session_expires_at'] <= self.session_expires_at,
                'wss_session_expiry_extended')
        self.session_expires_at = frame['session_expires_at']
        require(type(frame['delivery_id']) is str and bool(re.fullmatch(r'[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]',
                frame['delivery_id'])), 'wss_delivery_identity')
        validate_view(frame['view'], self.game_id, self.role, self.actor_id)
        view_revision = (frame['view']['game']['view_revision'] if frame['view']['role'] == 'host'
                         else frame['view']['view_revision'])
        require(frame['view_revision'] == view_revision and frame['view_revision'] >= self.last_revision and
                (frame['view_revision'] != self.last_revision or frame['view'] == self.last_view), 'wss_revision_order')
        self.last_revision, self.last_view = frame['view_revision'], frame['view']
        ack = {'version': 1, 'kind': 'snapshot_ack', 'connection_id': connection,
               'delivery_id': frame['delivery_id'], 'view_revision': frame['view_revision']}
        self.transport.send(1, json.dumps(ack, separators=(',', ':')).encode('utf8'))
        self.ack_count += 1

    def _read(self):
        try:
            while not self.transport.stop.is_set():
                frame = self.transport.message()
                if 'transport_close_code' in frame:
                    with self.condition:
                        self.closed = True
                        self.condition.notify_all()
                    return
                self._accept(frame)
                with self.condition:
                    require(len(self.frames) < 128, 'wss_backlog_bound')
                    self.frames.append(frame)
                    self.condition.notify_all()
        except Exception as error:
            with self.condition:
                if not self.transport.stop.is_set():
                    self.failure = str(error) if isinstance(error, E2EError) else 'wss_reader_failed'
                self.condition.notify_all()

    def wait(self, predicate, timeout=WAIT_TIMEOUT):
        deadline = time.monotonic() + timeout
        with self.condition:
            while True:
                require(self.failure is None, self.failure or 'wss_reader_failed')
                for frame in self.frames:
                    if predicate(frame):
                        return frame
                require(not self.closed, 'wss_closed_before_condition')
                remaining = deadline - time.monotonic()
                require(remaining > 0, 'wss_condition_timeout')
                self.condition.wait(remaining)

    def terminal_if_present(self):
        # Terminal spectator delivery is best effort and never gates release.
        with self.condition:
            require(self.failure is None or self.failure == 'wss_transport_closed',
                    self.failure or 'wss_reader_failed')
            return next((f for f in self.frames if f.get('kind') == 'terminal_notice'), None)

    def close(self):
        self.transport.close()
        self.thread.join(timeout=2)
        require(not self.thread.is_alive(), 'wss_reader_cleanup_failed')
