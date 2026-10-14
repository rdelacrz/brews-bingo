"""Offline unit tests for the remote runner; these are not remote run evidence."""
import contextlib
from datetime import datetime, timezone
from copy import deepcopy
from email.message import Message
import importlib.util
import io
import json
import os
from pathlib import Path
import runpy
import socket
import struct
import subprocess
import tempfile
import threading
import unittest
from unittest.mock import patch
import uuid

SCRIPT = Path(__file__).resolve().parents[2] / 'tests/full_game_e2e.py'
SPEC = importlib.util.spec_from_file_location('full_game_e2e', SCRIPT)
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)
from e2e_support.bootstrap import Bootstrap, run_cli
from e2e_support.http import Actor, Command, NoRedirect, Reply
from e2e_support.oracle import BoardOracle, call_values, history_expiry
from e2e_support.safety import Checks, E2EError, Target, json_object, private_output
from e2e_support.scenario import Scenario
from e2e_support.stream import Stream, Transport, validate_view

GAME = '01890f3e-53b7-7d28-9b05-4f65092d5711'
PLAYER = '01890f3e-53b7-7d28-9b05-4f65092d5712'
ORIGIN = 'https://localhost:8443'
BASE = '/api/games/' + GAME


def board_fixture():
    cells = []
    for index in range(25):
        cells.append({'position': {'row': index // 5 + 1, 'column': index % 5 + 1},
                      'kind': {'kind': 'free'} if index == 12 else {'kind': 'value', 'value': str(index + 1)},
                      'is_matched': index == 12})
    return {'side_length': 5, 'cells': cells, 'qualified': False, 'qualifying_lines': []}


class OracleTests(unittest.TestCase):
    def setUp(self):
        self.board = board_fixture()
        self.oracle = BoardOracle(self.board)

    def test_initial_free_and_first_row(self):
        self.assertEqual(self.oracle.first_row(), ['1', '2', '3', '4', '5'])
        self.assertFalse(self.oracle.evaluate([])['qualified'])
        self.assertEqual(sum(c['is_matched'] for c in self.oracle.evaluate([])['cells']), 1)

    def test_every_row_golden(self):
        for row in range(5):
            values = [str(row * 5 + c + 1) for c in range(5) if row * 5 + c != 12]
            result = self.oracle.evaluate(values)
            self.assertEqual(result['qualifying_lines'], [{'kind': 'row', 'index': row + 1}])
            self.assertTrue(result['qualified'])

    def test_every_column_golden(self):
        for column in range(5):
            values = [str(r * 5 + column + 1) for r in range(5) if r * 5 + column != 12]
            self.assertEqual(self.oracle.evaluate(values)['qualifying_lines'], [{'kind': 'column', 'index': column + 1}])

    def test_diagonal_goldens_free_is_automatic(self):
        self.assertEqual(self.oracle.evaluate(['1', '7', '19', '25'])['qualifying_lines'], [{'kind': 'main_diagonal'}])
        self.assertEqual(self.oracle.evaluate(['5', '9', '17', '21'])['qualifying_lines'], [{'kind': 'anti_diagonal'}])

    def test_full_cells_all_twelve_lines(self):
        result = self.oracle.evaluate([str(n) for n in range(1, 76)])
        self.assertEqual(len(result['qualifying_lines']), 12)
        self.assertTrue(all(c['is_matched'] for c in result['cells']))

    def test_partial_near_line_is_not_qualified(self):
        self.assertFalse(self.oracle.evaluate(['1', '2', '3', '4'])['qualified'])

    def test_call_order_sequence_and_distinct_value(self):
        self.assertEqual(call_values([{'sequence_no': 1, 'value': '7'}, {'sequence_no': 2, 'value': '1'}]), ['7', '1'])
        for calls in ([{'sequence_no': 2, 'value': '1'}], [{'sequence_no': True, 'value': '1'}],
                      [{'sequence_no': 1, 'value': '01'}], [{'sequence_no': 1, 'value': '76'}],
                      [{'sequence_no': 1, 'value': '0'}], [{'sequence_no': 1, 'value': 1}],
                      [{'sequence_no': 1, 'value': '1'}, {'sequence_no': 2, 'value': '1'}],
                      [{'sequence_no': 1, 'value': '1', 'private': 'discard'}], {}):
            with self.subTest(), self.assertRaises(E2EError):
                call_values(calls)

    def test_oracle_reset_and_replay_do_not_accumulate_state(self):
        values = self.oracle.first_row()
        qualified = self.oracle.evaluate(values)
        self.assertEqual(qualified, self.oracle.evaluate(values))
        self.assertEqual(self.board, self.oracle.evaluate([]))
        self.assertEqual(self.oracle.initial, self.board)
        with self.assertRaises(E2EError):
            self.oracle.evaluate(['1', '1'])

    def test_malformed_board_rejected(self):
        changes = [lambda b: b.update(side_length=4), lambda b: b.update(side_length=True),
                   lambda b: b.update(cells=b['cells'][:-1]), lambda b: b.update(extra=1),
                   lambda b: b['cells'][0].update(position={'row': 0, 'column': 1}),
                   lambda b: b['cells'][0].update(position={'row': True, 'column': 1}),
                   lambda b: b['cells'][0].update(kind={'kind': 'free'}),
                   lambda b: b['cells'][12].update(kind={'kind': 'free', 'value': None}),
                   lambda b: b['cells'][12].update(kind={'kind': 'value', 'value': '13'}),
                   lambda b: b['cells'][1].update(kind={'kind': 'value', 'value': '1'}),
                   lambda b: b['cells'][0].update(kind={'kind': 'value', 'value': '76'}),
                   lambda b: b['cells'][0].update(is_matched=True),
                   lambda b: b['cells'][12].update(is_matched=False)]
        for change in changes:
            b = deepcopy(self.board)
            change(b)
            with self.subTest(), self.assertRaises(E2EError):
                BoardOracle(b)

    def test_history_three_calendar_months_clamps_and_preserves_milliseconds(self):
        def millis(year, month, day):
            return int(datetime(year, month, day, 12, 34, 56, tzinfo=timezone.utc).timestamp()) * 1000 + 789
        for before, after in (((2026, 1, 31), (2026, 4, 30)), ((2023, 11, 30), (2024, 2, 29)),
                              ((2024, 11, 30), (2025, 2, 28)), ((2026, 10, 14), (2027, 1, 14))):
            self.assertEqual(history_expiry(millis(*before)), millis(*after))
        for value in (0, True, -1, 2**53, '1'):
            with self.assertRaises(E2EError):
                history_expiry(value)

    def test_false_matching_qualification_or_line_is_detected(self):
        expected = self.oracle.evaluate(self.oracle.first_row())
        for change in (lambda b: b.update(qualified=False), lambda b: b.update(qualifying_lines=[]),
                       lambda b: b['qualifying_lines'][0].update(index=2),
                       lambda b: b['qualifying_lines'][0].update(index=True),
                       lambda b: b['cells'][0].update(is_matched=False),
                       lambda b: b.update(qualified=1), lambda b: b.update(side_length=5.0)):
            actual = deepcopy(expected)
            change(actual)
            with self.subTest(), self.assertRaises(E2EError):
                self.oracle.verify(actual, self.oracle.first_row())


class GuardTests(unittest.TestCase):
    def test_exact_https_local_and_staging_target(self):
        for origin in (ORIGIN, 'https://127.0.0.1', 'https://[::1]:8443'):
            self.assertEqual(Target(origin, origin, 'local').origin, origin)
        self.assertEqual(Target('https://staging.worker.workers.dev', 'https://staging.worker.workers.dev', 'staging').port, 443)

    def test_dangerous_origins_and_mismatch_rejected(self):
        urls = ('http://localhost', 'https://localhost/', 'https://localhost/api', 'https://localhost?x=1',
                'https://localhost?', 'https://localhost#x', 'https://user:secret@localhost',
                'https://@localhost', 'https://localhost:0', 'https://localhost:65536', 'https://localhost:',
                'https://localhost\\evil', ' https://localhost', 'https://localhost\n',
                'https://elsewhere.net', 'https://localhost.%00')
        for url in urls:
            with self.subTest(), self.assertRaises(E2EError):
                Target(url, url, 'local')
        with self.assertRaises(E2EError):
            Target(ORIGIN, ORIGIN + '/', 'local')

    def test_staging_reserved_local_and_production_rejected(self):
        urls = ('https://localhost', 'https://127.0.0.1', 'https://[::1]', 'https://10.0.0.1',
                'https://anything.invalid', 'https://anything.example', 'https://example.com',
                'https://example.org', 'https://example.net', 'https://anything.test',
                'https://production.site.net', 'https://prod.site.net', 'https://brews-prod.workers.dev')
        for url in urls:
            with self.subTest(), self.assertRaises(E2EError):
                Target(url, url, 'staging')
        with self.assertRaises(E2EError):
            Target(ORIGIN, ORIGIN, 'production')
        with self.assertRaises(E2EError):
            Target('https://staging.worker.dev', 'https://staging.worker.dev', 'staging', '/unused')

    def test_api_scope_guard(self):
        target = Target(ORIGIN, ORIGIN, 'local')
        self.assertEqual(target.url(BASE + '/sync?view=player'), ORIGIN + BASE + '/sync?view=player')
        for path in ('https://other.dev/api/session', '//other.dev/api/session', '/_dev/commands',
                     '/api/../_dev/commands', '/api/%2e%2e', '/api/a#fragment', '/api/a\r\nCookie:x'):
            with self.assertRaises(E2EError):
                target.url(path)

    def test_private_fresh_output_outside_repo(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp) / 'repo'
            repo.mkdir()
            private = private_output(Path(temp) / 'private', repo)
            self.assertEqual(private.stat().st_mode & 0o777, 0o700)
            for path in (private, repo / 'inside', Path('relative')):
                with self.assertRaises(E2EError):
                    private_output(path, repo)
            link = Path(temp) / 'link'
            link.symlink_to(repo, target_is_directory=True)
            with self.assertRaises(E2EError):
                private_output(link / 'inside', repo)
            destination = Path(temp) / 'symlink'
            destination.symlink_to(Path(temp) / 'absent')
            with self.assertRaises(E2EError):
                private_output(destination, repo)

    def test_json_duplicates_size_and_nonfinite_rejected(self):
        for raw in (b'{"x":1,"x":2}', b'[]', b'{"x":NaN}', b'{' * 5000, b'x' * (256 * 1024 + 1)):
            with self.assertRaises(E2EError):
                json_object(raw)

    def test_checks_unique_and_summary_secret_free(self):
        checks = Checks()
        checks.check(True, 'actual_condition')
        self.assertEqual(checks.summary('local', True, 'own_account_removed')['check_count'], 1)
        for label in ('actual_condition', 'https://secret', 'has-private-value'):
            with self.assertRaises(E2EError):
                checks.check(True, label)
        with self.assertRaises(E2EError):
            checks.check(False, 'failed_condition')
        self.assertEqual(len(checks.labels), 1)


class Response:
    def __init__(self, url, *, status=200, cookies=(), body=None):
        self.url, self.status = url, status
        self.headers = Message()
        self.headers['Cache-Control'] = 'no-store'
        self.headers['Referrer-Policy'] = 'no-referrer'
        for cookie in cookies:
            self.headers['Set-Cookie'] = cookie
        self.data = json.dumps(body or {'ok': True}).encode()

    def geturl(self):
        return self.url

    def info(self):
        return self.headers

    def read(self, size):
        return self.data[:size]

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return None


class ActorTests(unittest.TestCase):
    def setUp(self):
        self.actor = Actor(Target(ORIGIN, ORIGIN, 'local'))

    def test_redirect_rejected_without_followup(self):
        with self.assertRaises(E2EError):
            NoRedirect().redirect_request(None, None, 302, '', {}, 'https://secret.site/')
        with patch.object(self.actor.opener, 'open', return_value=Response(ORIGIN + '/api/session', status=302)) as opened:
            with self.assertRaises(E2EError):
                self.actor.request('/api/session')
            self.assertEqual(opened.call_count, 1)

    def test_separate_jars_secure_httponly_path_and_origin(self):
        raw = '__Secure-brews-player=unit-fixture; Secure; HttpOnly; Path=' + BASE + '; SameSite=Strict'
        with patch.object(self.actor.opener, 'open', return_value=Response(ORIGIN + BASE + '/players', cookies=[raw])):
            self.assertTrue(self.actor.request(BASE + '/players', method='POST', body={}).set_cookie)
        self.assertTrue(self.actor.cookie_header(BASE + '/stream?view=player'))
        self.assertFalse(self.actor.cookie_header('/api/session'))
        self.assertFalse(self.actor.cookie_header('/api/games/' + PLAYER + '/stream?view=player'))
        self.assertFalse(Actor(self.actor.target).cookie_header(BASE + '/stream'))
        with self.assertRaises(E2EError):
            self.actor.cookie_header('https://other.dev' + BASE + '/stream')

    def test_reject_cookie_domain_unprotected_unknown_and_wrong_path(self):
        cookies = ('__Secure-brews-player=x; Secure; HttpOnly; Path=/',
                   '__Secure-brews-player=x; Secure; Path=' + BASE,
                   '__Secure-brews-player=x; HttpOnly; Path=' + BASE,
                   '__Secure-brews-player=x; Secure; HttpOnly; Domain=localhost; Path=' + BASE,
                   'other=x; Secure; HttpOnly; Path=' + BASE,
                   '__Host-brews_session=x; Secure; HttpOnly; Path=/api')
        for cookie in cookies:
            with self.subTest(), self.assertRaises(E2EError):
                Actor.validate_cookie(cookie, BASE + '/players')

    def test_origin_and_command_headers_and_no_proxy(self):
        command = str(uuid.uuid7())
        def open_request(request, **kwargs):
            self.assertEqual(request.get_header('Origin'), ORIGIN)
            self.assertEqual(request.get_header('Idempotency-key'), command)
            self.assertEqual(request.data, b'{}')
            return Response(request.full_url)
        with patch.object(self.actor.opener, 'open', side_effect=open_request):
            self.actor.request(BASE + '/start', method='POST', body={}, command=command)

    def test_transport_error_redacts_error_chain(self):
        with patch.object(self.actor.opener, 'open', side_effect=OSError('private-value')):
            with self.assertRaises(E2EError) as caught:
                self.actor.request('/api/session')
            self.assertEqual(str(caught.exception), 'http_transport_failed')
            self.assertTrue(caught.exception.__suppress_context__)


class FakeActor:
    def __init__(self):
        self.requests = []
        self.responses = []

    def request(self, path, **kwargs):
        self.requests.append((path, kwargs))
        item = self.responses.pop(0)
        if isinstance(item, Exception):
            raise item
        return item


class CommandTests(unittest.TestCase):
    def command(self):
        actor = FakeActor()
        command = Command(actor, BASE + '/calls/manual', {'value': '1', 'expected_revision': 9}, GAME)
        receipt = {'command_id': command.command_id, 'game_id': GAME,
                   'outcome': {'kind': 'call_accepted', 'call': {'sequence_no': 1, 'value': '1'}}}
        return actor, command, receipt

    def test_fresh_and_exact_payload_receipt_replay(self):
        actor, command, receipt = self.command()
        actor.responses = [Reply(200, {'result': 'call_accepted', 'receipt': receipt}, False),
                           Reply(200, {'result': 'committed', 'receipt': receipt}, False)]
        command.run()
        command.replay()
        self.assertEqual(actor.requests[0], actor.requests[1])
        self.assertEqual(uuid.UUID(command.command_id).version, 7)

    def test_pending_lookup_then_retry_original_uuid_and_payload(self):
        actor, command, receipt = self.command()
        actor.responses = [Reply(202, {'result': 'pending', 'operation_id': PLAYER}, False),
            Reply(200, {'version': 1, 'game_id': GAME, 'command_id': command.command_id,
                        'result_status': {'status': 'committed', 'receipt': receipt, 'follow_up_pending': True}}, False),
            Reply(200, {'result': 'committed', 'receipt': receipt}, False)]
        with patch('e2e_support.http.time.sleep'):
            response = command.run()
        self.assertEqual(command.effective(response)['call'], {'sequence_no': 1, 'value': '1'})
        self.assertEqual(actor.requests[0], actor.requests[2])
        self.assertEqual(actor.requests[1][0], BASE + '/commands/' + command.command_id + '?view=account')
        self.assertTrue(command.pending_observed)

    def test_unknown_mutation_failure_does_not_generate_new_command(self):
        actor, command, _ = self.command()
        original = command.command_id
        actor.responses = [E2EError('http_transport_failed')]
        with self.assertRaises(E2EError):
            command.run()
        self.assertEqual(command.command_id, original)
        self.assertEqual(len(actor.requests), 1)

    def test_cookie_or_payload_in_replay_rejected(self):
        for cookie, additional in ((True, {}), (False, {'board': 'must-not-return'})):
            actor, command, receipt = self.command()
            command.receipt = receipt
            actor.responses = [Reply(200, {'result': 'committed', 'receipt': receipt, **additional}, cookie)]
            with self.assertRaises(E2EError):
                command.replay()

    def test_wrong_command_lookup_identity_rejected(self):
        actor, command, receipt = self.command()
        actor.responses = [Reply(200, {'version': 1, 'game_id': GAME, 'command_id': PLAYER,
                                      'result_status': {'status': 'committed', 'receipt': receipt}}, False)]
        with self.assertRaises(E2EError):
            command.lookup()

    def test_receipt_cannot_switch_game(self):
        actor, command, receipt = self.command()
        receipt['game_id'] = PLAYER
        actor.responses = [Reply(200, {'result': 'committed', 'receipt': receipt}, False)]
        with self.assertRaises(E2EError):
            command.run()
        self.assertEqual(command.game_id, GAME)

    def test_pending_deadline_bounded(self):
        actor, command, _ = self.command()
        with patch('e2e_support.http.time.monotonic', side_effect=[0, 46]):
            with self.assertRaises(E2EError):
                command.run()
        self.assertFalse(actor.requests)


class FakeSocket:
    def __init__(self, data=b''):
        self.data = bytearray(data)
        self.sent = bytearray()
        self.closed = False

    def recv(self, size):
        result = self.data[:size]
        del self.data[:size]
        return bytes(result)

    def sendall(self, data):
        self.sent.extend(data)

    def shutdown(self, how):
        pass

    def close(self):
        self.closed = True


def frame(opcode, payload, final=True):
    size = len(payload)
    prefix = bytes(((128 if final else 0) | opcode, size)) if size < 126 else (
        bytes(((128 if final else 0) | opcode, 126)) + struct.pack('!H', size))
    return prefix + payload


def snapshot_fixture():
    return {'version': 1, 'kind': 'snapshot', 'game_id': GAME, 'view_revision': 4,
            'connection_id': PLAYER, 'session_expires_at': 100,
            'view': {'role': 'player', 'game_id': GAME, 'state': 'in_progress',
                     'player_id': PLAYER, 'alias': 'Fixture', 'view_revision': 4,
                     'board': board_fixture(), 'calls': []}, 'delivery_id': 'A' * 43}


def receiver_fixture(role='player'):
    stream = Stream.__new__(Stream)
    stream.game_id, stream.role, stream.actor_id = GAME, role, PLAYER
    stream.connection_id = None
    stream.session_expires_at = None
    stream.last_revision, stream.last_view = -1, None
    stream.ack_count = 0
    stream.transport = Transport(FakeSocket())
    return stream


class StreamTests(unittest.TestCase):
    def test_fragmented_json_with_ping_and_masked_pong(self):
        wire = frame(1, b'{"a":', False) + frame(9, b'ping') + frame(0, b'1}')
        sock = FakeSocket(wire)
        self.assertEqual(Transport(sock).message(), {'a': 1})
        self.assertEqual(sock.sent[0], 138)
        self.assertTrue(sock.sent[1] & 128)

    def test_client_mask_and_payload_roundtrip(self):
        sock = FakeSocket()
        Transport(sock).send(1, b'{"fixture":true}')
        self.assertTrue(sock.sent[1] & 128)
        mask = sock.sent[2:6]
        decoded = bytes(b ^ mask[i % 4] for i, b in enumerate(sock.sent[6:]))
        self.assertEqual(decoded, b'{"fixture":true}')

    def test_snapshot_ack_exact_delivery_and_identity(self):
        stream = receiver_fixture()
        stream._accept(snapshot_fixture())
        raw = stream.transport.socket.sent
        size = raw[1] & 127
        offset = 2
        if size == 126:
            size = struct.unpack('!H', raw[2:4])[0]
            offset = 4
        mask = raw[offset:offset + 4]
        content = bytes(b ^ mask[i % 4] for i, b in enumerate(raw[offset + 4:]))
        ack = json.loads(content)
        self.assertEqual(ack, {'version': 1, 'kind': 'snapshot_ack', 'connection_id': PLAYER,
                              'delivery_id': 'A' * 43, 'view_revision': 4})
        self.assertEqual(stream.ack_count, 1)

    def test_wrong_identity_privacy_revision_and_delivery_rejected(self):
        changes = (lambda f: f['view'].update(player_id=GAME), lambda f: f['view'].update(players=[]),
                   lambda f: f.update(delivery_id='bad'), lambda f: f.update(connection_id='bad'),
                   lambda f: f['view'].update(view_revision=3), lambda f: f.update(game_id=PLAYER),
                   lambda f: f.update(private='discard'))
        for change in changes:
            f = snapshot_fixture()
            change(f)
            with self.subTest(), self.assertRaises(E2EError):
                receiver_fixture()._accept(f)

    def test_revision_is_per_viewer_not_compared_across_roles(self):
        stream = receiver_fixture()
        stream._accept(snapshot_fixture())
        changed = snapshot_fixture()
        changed['view']['alias'] = 'Changed'
        with self.assertRaises(E2EError):
            stream._accept(changed)
        old = snapshot_fixture()
        old['view_revision'] = old['view']['view_revision'] = 3
        with self.assertRaises(E2EError):
            stream._accept(old)

    def test_snapshot_cannot_extend_original_session_expiry(self):
        stream = receiver_fixture()
        stream._accept(snapshot_fixture())
        later = snapshot_fixture()
        later['session_expires_at'] = 101
        with self.assertRaises(E2EError):
            stream._accept(later)

    def test_public_spectator_projection_rejects_board_and_other_identity(self):
        view = {'role': 'spectator', 'game_id': GAME, 'spectator_id': PLAYER, 'calls': [], 'view_revision': 1}
        validate_view(view, GAME, 'spectator', PLAYER)
        for change in ({'board': {}}, {'players': []}, {'spectator_id': GAME}, {'alias': 'private'}):
            with self.assertRaises(E2EError):
                validate_view({**view, **change}, GAME, 'spectator', PLAYER)

    def test_terminal_notice_public_only(self):
        stream = receiver_fixture('spectator')
        public = {'version': 1, 'kind': 'terminal_notice', 'game_id': GAME, 'state': 'resolved',
                  'ended_at': 1, 'objective': 'single_line', 'calls': [], 'winner': {'player_id': PLAYER, 'alias': 'Fixture'}}
        stream._accept(public)
        self.assertEqual(stream.ack_count, 0)
        for extra in ({'board': {}}, {'history_expires_at': 1}, {'players': []}):
            with self.assertRaises(E2EError):
                stream._accept({**public, **extra})

    def test_malformed_frames_and_bounds(self):
        invalid = (b'\xc1\x02{}', b'\x81\x82xxxx{}', frame(0, b'{}'), frame(2, b'{}'),
                   frame(9, b'x', False), frame(8, b'x'), b'\x81\x7e\x00\x02{}',
                   b'\x81\x7f' + struct.pack('!Q', 256 * 1024 + 1),
                   frame(1, b'{', False) + frame(1, b'}'), frame(1, b'\xff'),
                   frame(9, b'x') * 65 + frame(1, b'{}'),
                   frame(1, b'{', False) + frame(0, b' ', False) * 32 + frame(0, b'}'))
        for wire in invalid:
            with self.subTest(), self.assertRaises(E2EError):
                Transport(FakeSocket(wire)).message()

    def test_close_validates_without_disclosing_reason(self):
        for code in (1000, 1001, 1008, 4001):
            self.assertEqual(Transport(FakeSocket(frame(8, struct.pack('!H', code) + b'private-reason'))).message(),
                             {'transport_close_code': code})
        for body in (struct.pack('!H', 1005), struct.pack('!H', 1000) + b'\xff'):
            with self.assertRaises(E2EError):
                Transport(FakeSocket(frame(8, body))).message()

    def test_eof_timeout_wait_and_client_caps(self):
        with self.assertRaises(E2EError):
            Transport(FakeSocket()).message()
        sock = FakeSocket()
        sock.recv = lambda size: (_ for _ in ()).throw(socket.timeout())
        with patch('e2e_support.stream.time.monotonic', side_effect=[0, 11]):
            with self.assertRaises(E2EError):
                Transport(sock).read_exact(1)
        for opcode, data in ((1, b'x' * 513), (10, b'x' * 126), (2, b'x')):
            with self.assertRaises(E2EError):
                Transport(FakeSocket()).send(opcode, data)
        stream = receiver_fixture()
        stream.condition, stream.frames, stream.failure, stream.closed = threading.Condition(), [], None, False
        with self.assertRaises(E2EError):
            stream.wait(lambda _: False, timeout=0)
        stream.closed = True
        with self.assertRaises(E2EError):
            stream.wait(lambda _: False)
        stream.transport.close()
        self.assertTrue(stream.transport.socket.closed)


class EntrypointTests(unittest.TestCase):
    def test_import_does_not_launch_process_or_network(self):
        with patch('socket.create_connection', side_effect=AssertionError('network forbidden')), \
             patch('urllib.request.OpenerDirector.open', side_effect=AssertionError('network forbidden')), \
             patch('subprocess.Popen', side_effect=AssertionError('process forbidden')):
            runpy.run_path(str(SCRIPT), run_name='import_fixture')

    def test_help_works_without_credentials(self):
        environment = {k: v for k, v in os.environ.items() if not k.startswith('BREWS_')}
        result = subprocess.run([os.sys.executable, str(SCRIPT), '--help'], env=environment, capture_output=True)
        self.assertEqual(result.returncode, 0)
        self.assertIn(b'--expected-origin', result.stdout)
        self.assertFalse(result.stderr)

    def test_invalid_target_no_network_bootstrap_or_output(self):
        with tempfile.TemporaryDirectory() as temp:
            destination = Path(temp) / 'new'
            arguments = ['--origin', 'https://private-value.invalid', '--expected-origin', 'https://private-value.invalid',
                         '--environment', 'staging', '--output', str(destination), '--bootstrap-cli', '/absent']
            output = io.StringIO()
            with patch('socket.create_connection', side_effect=AssertionError('network forbidden')), \
                 patch('subprocess.Popen', side_effect=AssertionError('process forbidden')), contextlib.redirect_stdout(output):
                self.assertEqual(runner.main(arguments), 1)
            self.assertFalse(destination.exists())
            self.assertNotIn('private-value', output.getvalue())
            self.assertEqual(json.loads(output.getvalue())['check_count'], 0)

    def test_missing_credentials_no_network_or_output(self):
        with tempfile.TemporaryDirectory() as temp:
            destination = Path(temp) / 'new'
            with patch.dict(os.environ, {}, clear=True), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(runner.main(['--origin', ORIGIN, '--expected-origin', ORIGIN, '--environment', 'local',
                                 '--output', str(destination), '--bootstrap-cli', '/absent']), 1)
            self.assertFalse(destination.exists())

    def test_cli_captured_failure_is_redacted(self):
        with self.assertRaises(E2EError) as caught:
            run_cli('/definitely-absent', [], {})
        self.assertEqual(str(caught.exception), 'cli_operation_failed')

    def test_cli_success_captures_only_bounded_json_metadata(self):
        class ProcessFixture:
            def __init__(self):
                self.stdout = io.BytesIO(b'{"result":"committed","receipt":{}}\n')
                self.stderr = io.BytesIO(b'discarded-private-fixture')
                self.returncode = 0
            def wait(self, **kwargs):
                return self.returncode
            def poll(self):
                return self.returncode
            def kill(self):
                self.returncode = -9
        captured = io.StringIO()
        with patch('e2e_support.bootstrap.subprocess.Popen', return_value=ProcessFixture()), contextlib.redirect_stdout(captured):
            result = run_cli('/fixture-cli', ['accounts', 'delete', GAME, '--yes'], {})
        self.assertEqual(result['result'], 'committed')
        self.assertEqual(captured.getvalue(), '')

    def test_cleanup_failure_still_closes_all_streams_and_retains_account(self):
        scenario = Scenario.__new__(Scenario)
        actor = FakeActor()
        actor.responses = [E2EError('http_transport_failed')]
        scenario.host, scenario.owned_games, scenario.creation_commands = actor, {GAME}, []
        closed = []
        scenario.streams = [type('StreamFixture', (), {'close': lambda _: closed.append(True)})() for _ in range(3)]
        scenario.bootstrap = type('BootstrapFixture', (), {'remove': lambda _: self.fail('unsafe account removal')})()
        self.assertFalse(scenario.cleanup())
        self.assertEqual(len(closed), 3)
        self.assertEqual(scenario.cleanup_status, 'failed_own_resources_may_remain')
        self.assertEqual(scenario.cleanup_errors, ['own_game_cleanup_failed'])

    def test_readiness_is_anonymous_and_bounded_before_bootstrap(self):
        actor = FakeActor()
        actor.jar = []
        actor.responses = [Reply(200, {'authenticated': False}, False)]
        scenario = Scenario.__new__(Scenario)
        scenario.target, scenario.checks = None, Checks()
        with patch('e2e_support.scenario.Actor', return_value=actor):
            scenario.readiness()
        self.assertEqual(actor.requests[0][0], '/api/session')
        self.assertEqual(len(scenario.checks.labels), 1)

    def test_existing_occupied_slot_fails_before_game_create(self):
        actor = FakeActor()
        actor.responses = [Reply(200, {'authenticated': True}, False),
                           Reply(200, {'slot': 'occupied', 'game': {'game_id': GAME}}, False)]
        scenario = Scenario.__new__(Scenario)
        scenario.host, scenario.checks = actor, Checks()
        scenario.readiness = lambda: None
        scenario.bootstrap = type('BootstrapFixture', (), {'enroll': lambda _, actor: PLAYER})()
        with self.assertRaises(E2EError) as caught:
            scenario.run()
        self.assertEqual(str(caught.exception), 'single_slot_initially_free')
        self.assertTrue(all(kwargs.get('method', 'GET') == 'GET' for _, kwargs in actor.requests))

    def test_uncertain_cli_creation_cleanup_reuses_original_command(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, {
                'BREWS_DEV_CLI_KEY': 'unit-fixture', 'BREWS_API_ORIGIN': ORIGIN}):
            bootstrap = Bootstrap(Target(ORIGIN, ORIGIN, 'local'), os.sys.executable, Path(temp))
            bootstrap.create_attempted = True
            def cli(path, args, environment, **options):
                if args[1] == 'create':
                    self.assertEqual(args[args.index('--role') + 1], 'host')
                    self.assertEqual(args[args.index('--command-id') + 1], bootstrap.create_id)
                    return {'result': 'committed', 'receipt': {'account_id': PLAYER,
                            'command_id': bootstrap.create_id, 'operation': 'create_account'}}
                if args[1] == 'get':
                    self.assertEqual(args, ['accounts', 'get', PLAYER])
                    self.assertEqual(options, {'expect_missing_account': True})
                    return {'error': {'code': 'http_rejected'}}
                self.assertEqual(args[:3], ['accounts', 'delete', PLAYER])
                return {'result': 'committed', 'receipt': {'account_id': PLAYER,
                        'command_id': bootstrap.delete_id, 'operation': 'delete_account'}}
            with patch('e2e_support.bootstrap.run_cli', side_effect=cli) as operation:
                self.assertEqual(bootstrap.remove(), 'own_account_removed')
                self.assertEqual(operation.call_count, 3)
            self.assertFalse((Path(temp) / 'cleanup-enrollment.url').exists())

    def test_cli_absence_readback_requires_exact_target_and_404(self):
        class ProcessFixture:
            def __init__(self, status):
                self.stdout = io.BytesIO()
                self.stderr = io.BytesIO(json.dumps({'error': {
                    'code': 'http_rejected',
                    'message': f'management endpoint rejected request (HTTP {status}); no automatic retry was made',
                }}).encode())
                self.returncode = 1
            def wait(self, **options):
                return self.returncode
            def poll(self):
                return self.returncode
        for status in (404, 401, 403, 503):
            with patch('e2e_support.bootstrap.subprocess.Popen', return_value=ProcessFixture(status)):
                if status == 404:
                    result = run_cli('/fixture-cli', ['accounts', 'get', PLAYER], {}, expect_missing_account=True)
                    self.assertEqual(result['error']['code'], 'http_rejected')
                else:
                    with self.assertRaises(E2EError):
                        run_cli('/fixture-cli', ['accounts', 'get', PLAYER], {}, expect_missing_account=True)
        with patch('e2e_support.bootstrap.subprocess.Popen', side_effect=AssertionError('process forbidden')):
            for args in (['accounts', 'delete', PLAYER], ['accounts', 'get', 'bad-id']):
                with self.assertRaises(E2EError):
                    run_cli('/fixture-cli', args, {}, expect_missing_account=True)

    def test_cli_cleanup_never_trusts_wrong_creation_receipt(self):
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, {
                'BREWS_DEV_CLI_KEY': 'unit-fixture', 'BREWS_API_ORIGIN': ORIGIN}):
            bootstrap = Bootstrap(Target(ORIGIN, ORIGIN, 'local'), os.sys.executable, Path(temp))
            bootstrap.create_attempted = True
            with patch('e2e_support.bootstrap.run_cli', return_value={'result': 'committed',
                    'receipt': {'account_id': PLAYER, 'command_id': GAME, 'operation': 'create_account'}}) as operation:
                with self.assertRaises(E2EError):
                    bootstrap.remove()
                self.assertEqual(operation.call_count, 1)
            self.assertIsNone(bootstrap.account_id)

    def test_bootstrap_origin_cannot_be_inherited_from_other_target(self):
        with patch.dict(os.environ, {'BREWS_DEV_CLI_KEY': 'unit-fixture', 'BREWS_API_ORIGIN': 'https://elsewhere.net'}):
            with self.assertRaises(E2EError):
                Bootstrap(Target(ORIGIN, ORIGIN, 'local'), os.sys.executable, Path('/unused'))

    def test_cleanup_cancels_only_proven_own_game_and_keeps_expected_state(self):
        actor = FakeActor()
        actor.responses = [Reply(200, {'up_to_date': False, 'snapshot': {'role': 'host', 'game': {'state': 'in_progress'}}}, False)]
        scenario = Scenario.__new__(Scenario)
        scenario.host, scenario.owned_games, scenario.creation_commands, scenario.streams = actor, {GAME}, [], []
        scenario.checks = Checks()
        scenario.bootstrap = type('BootstrapFixture', (), {'remove': lambda _: 'own_account_removed'})()
        with patch('e2e_support.scenario.Command') as command:
            command.return_value.run.return_value = {}
            self.assertTrue(scenario.cleanup())
            self.assertEqual(command.call_args.args, (actor, BASE + '/cancel',
                             {'confirmed': True, 'expected_state': 'in_progress'}, GAME))

    def test_occupied_slot_discovery_is_not_ownership(self):
        actor = FakeActor()
        actor.responses = [Reply(200, {'slot': 'occupied', 'game': {'game_id': GAME}}, False),
                          Reply(200, {'version': 1, 'game_id': GAME, 'command_id': PLAYER,
                                      'result_status': {'status': 'unknown'}}, False)]
        scenario = Scenario.__new__(Scenario)
        scenario.host, scenario.owned_games = actor, set()
        command = type('CommandFixture', (), {'command_id': PLAYER, 'game_id': None})()
        scenario.observe_creation(command)
        self.assertFalse(scenario.owned_games)
        self.assertIsNone(command.game_id)


if __name__ == '__main__':
    unittest.main()
