"""One real remote game, through supported HTTP/WSS and restricted CLI APIs."""
import time
import uuid
from .bootstrap import Bootstrap
from .http import Actor, Command
from .oracle import BoardOracle, call_values, history_expiry
from .safety import E2EError, WAIT_TIMEOUT, identifier, require
from .stream import Stream, validate_view

CONFIGURATION = {'numeric_upper_bound': 75, 'board_side_length': 5, 'free_cells_enabled': True,
                 'free_cell_positions': [{'row': 3, 'column': 3}], 'player_capacity': 20,
                 'spectator_capacity': 50, 'winning_pattern': 'single_line'}


def state_of(snapshot):
    return snapshot['game']['state'] if snapshot.get('role') == 'host' else snapshot['state']


class Scenario:
    def __init__(self, target, cli_path, output, checks):
        self.target, self.output, self.checks = target, output, checks
        self.host = Actor(target)
        self.bootstrap = Bootstrap(target, cli_path, output)
        self.streams = []
        self.owned_games = set()
        self.creation_commands = []
        self.cleanup_status = 'not_run'
        self.cleanup_errors = []
        self.base = None
        self.players = []
        self.calls = []

    def check(self, condition, label):
        self.checks.check(condition, label)

    def sync(self, actor=None, role='account', base=None):
        reply = (actor or self.host).request((base or self.base) + '/sync?view=' + role)
        require(not reply.set_cookie and reply.value.get('up_to_date') is False and
                type(reply.value.get('snapshot')) is dict, 'full_sync_missing')
        return reply.value

    def readiness(self):
        deadline = time.monotonic() + 30
        observer = Actor(self.target)
        while True:
            try:
                reply = observer.request('/api/session', timeout=min(2, max(0.1, deadline - time.monotonic())))
            except E2EError as error:
                if str(error) != 'http_transport_failed':
                    raise
            else:
                self.check(reply.value.get('authenticated') is False and not reply.set_cookie and
                           not list(observer.jar), 'readiness_anonymous_before_bootstrap')
                return
            require(time.monotonic() < deadline, 'readiness_timeout')
            time.sleep(0.2)

    def observe_creation(self, command):
        current = self.host.request('/api/games/current').value
        if current.get('slot') != 'occupied':
            return
        summary = current.get('game', {})
        candidate = identifier(summary.get('game_id'))
        # Current slot discovery alone cannot establish that we created this game.
        probe = Command.__new__(Command)
        probe.actor, probe.game_id, probe.command_id = self.host, candidate, command.command_id
        try:
            status = probe.lookup()
        except E2EError:
            return
        receipt = status.get('receipt', {})
        if status.get('status') == 'committed' and receipt.get('game_id') == candidate and (
                receipt.get('outcome', {}).get('kind') == 'created'):
            command.game_id = candidate
            self.owned_games.add(candidate)
        elif status.get('status') == 'pending' and status.get('operation') == 'creation':
            command.game_id = candidate
            self.owned_games.add(candidate)

    def create(self):
        command = Command(self.host, '/api/games', {'configuration': CONFIGURATION})
        self.creation_commands.append(command)
        result = command.run(self.observe_creation)
        game_id = identifier(command.game_id)
        self.owned_games.add(game_id)
        base = '/api/games/' + game_id
        snapshot = self.sync(base=base)['snapshot']
        require(state_of(snapshot) == 'new' and snapshot['configuration'] == CONFIGURATION and
                snapshot['game']['designated_host_id'] == self.bootstrap.account_id,
                'created_configuration_mismatch')
        return game_id, base

    def mutation(self, suffix, body):
        command = Command(self.host, self.base + suffix, body, self.game_id)
        return command, command.run()

    def poll_presence(self):
        deadline = time.monotonic() + WAIT_TIMEOUT
        while True:
            current = self.sync()
            snapshot = current['snapshot']
            if snapshot.get('connected_player_count') == 3:
                self.check({p['player_id'] for p in snapshot['players']} == {
                    p['id'] for p in self.players} and all(p['connected'] for p in snapshot['players']) and
                    snapshot.get('connected_spectator_count') == 1, 'genuine_start_presence')
                return current
            require(time.monotonic() < deadline, 'start_presence_timeout')
            time.sleep(0.2)

    def wait_model(self, stream, state, calls):
        frame = stream.wait(lambda f: f.get('kind') == 'snapshot' and state_of(f['view']) == state and
                            call_values(f['view'].get('calls', [])) == calls)
        return frame['view']

    def assert_model(self, state, *, stage):
        current = self.sync()
        snapshot = current['snapshot']
        self.check(state_of(snapshot) == state and call_values(snapshot['calls']) == self.calls,
                   stage + '_host_calls')
        host_view = self.wait_model(self.host_stream, state, self.calls)
        self.check(host_view == snapshot, stage + '_host_wss')
        self.check({p['player_id'] for p in snapshot['players']} == {p['id'] for p in self.players},
                   stage + '_roster_identity')
        for index, player in enumerate(self.players):
            actual = next(p['board'] for p in snapshot['players'] if p['player_id'] == player['id'])
            player['oracle'].verify(actual, self.calls)
            own = self.sync(player['actor'], 'player')['snapshot']
            validate_view(own, self.game_id, 'player', player['id'])
            player['oracle'].verify(own['board'], self.calls)
            streamed = self.wait_model(player['stream'], state, self.calls)
            player['oracle'].verify(streamed['board'], self.calls)
            self.check(call_values(own['calls']) == self.calls and streamed == own and own['board'] == actual,
                       stage + '_player_' + str(index) + '_oracle_wss')
        if state == 'in_progress':
            public = self.wait_model(self.spectator_stream, state, self.calls)
            self.check(call_values(public['calls']) == self.calls and public['configuration'] == CONFIGURATION and
                       public['latest_call'] == (public['calls'][-1] if public['calls'] else None) and
                       public['exhausted'] is False, stage + '_spectator_public_wss')
        return current

    def verify_replay(self, command, stage):
        before = [self.sync()] + [self.sync(p['actor'], 'player') for p in self.players]
        if state_of(before[0]['snapshot']) == 'in_progress':
            before.append(self.sync(self.spectator, 'spectator'))
        command.replay()
        after = [self.sync()] + [self.sync(p['actor'], 'player') for p in self.players]
        if len(before) == 5:
            after.append(self.sync(self.spectator, 'spectator'))
        self.check(before == after, stage + '_receipt_replay_preserves_each_view')

    def run(self):
        self.readiness()
        self.account_id = self.bootstrap.enroll(self.host)
        self.check(self.host.request('/api/session').value.get('authenticated') is True, 'throwaway_host_enrolled')
        current = self.host.request('/api/games/current').value
        self.check(current == {'slot': 'free', 'game': None}, 'single_slot_initially_free')
        self.game_id, self.base = self.create()
        self.check(True, 'create_new_standard_board')
        revision = self.sync()['view_revision']
        self.mutation('/lobby', {'expected_revision': revision})
        lobby = self.sync()['snapshot']
        self.code = lobby['game_code']
        self.check(state_of(lobby) == 'awaiting_players' and isinstance(self.code, str), 'open_lobby')
        for index, alias in enumerate(('E2EPlayerA', 'E2EPlayerB', 'E2EPlayerC')):
            actor = Actor(self.target)
            context = actor.request(self.base + '/admission-context', method='POST', body={'game_code': self.code})
            require(context.set_cookie, 'player_context_cookie')
            command = Command(actor, self.base + '/players', {'game_code': self.code, 'alias': alias}, self.game_id)
            joined = command.run()
            require(joined.get('result') == 'player_joined' and
                    any(c.name == '__Secure-brews-player' for c in actor.jar), 'player_admission_result')
            player_id = identifier(joined['player_id'])
            stream = Stream(actor, self.game_id, 'player', player_id)
            self.streams.append(stream)
            stream.wait(lambda f: f.get('kind') == 'snapshot' and state_of(f['view']) == 'awaiting_players')
            self.players.append({'id': player_id, 'alias': alias, 'actor': actor, 'stream': stream})
            self.check(joined['alias'] == alias, 'player_' + str(index) + '_separate_admission_wss')
        self.spectator = Actor(self.target)
        require(self.spectator.request(self.base + '/spectator-admission-context', method='POST',
                body={'game_code': self.code}).set_cookie, 'spectator_context_cookie')
        spectator_command = Command(self.spectator, self.base + '/spectators', {'game_code': self.code}, self.game_id)
        spectator_command.run()
        require(any(c.name == '__Secure-brews-spectator' for c in self.spectator.jar), 'spectator_admission_cookie')
        spectator_view = self.sync(self.spectator, 'spectator')['snapshot']
        self.spectator_stream = Stream(self.spectator, self.game_id, 'spectator',
                                       identifier(spectator_view['spectator_id']))
        self.streams.append(self.spectator_stream)
        self.spectator_stream.wait(lambda f: f.get('kind') == 'snapshot')
        self.host_stream = Stream(self.host, self.game_id, 'account')
        self.streams.append(self.host_stream)
        self.host_stream.wait(lambda f: f.get('kind') == 'snapshot')
        connected = self.poll_presence()
        start, _ = self.mutation('/start', {'expected_revision': connected['view_revision']})
        running = self.sync()['snapshot']
        self.check(state_of(running) == 'in_progress', 'start_real_presence')
        for player in self.players:
            board = next(p['board'] for p in running['players'] if p['player_id'] == player['id'])
            player['oracle'] = BoardOracle(board)
        self.assert_model('in_progress', stage='start')
        self.verify_replay(start, 'start')
        current = self.sync()
        random, response = self.mutation('/calls/random', {'expected_revision': current['view_revision']})
        call = random.effective(response)['call']
        self.calls = call_values([call])
        self.assert_model('in_progress', stage='random_call')
        self.verify_replay(random, 'random_call')
        for index, value in enumerate(self.players[0]['oracle'].first_row()):
            if value in self.calls:
                continue
            current = self.sync()
            command, response = self.mutation('/calls/manual', {'value': value, 'expected_revision': current['view_revision']})
            accepted = command.effective(response)['call']
            self.check(accepted == {'sequence_no': len(self.calls) + 1, 'value': value}, 'manual_' + str(index) + '_accepted')
            self.calls.append(value)
            self.assert_model('in_progress', stage='manual_' + str(index))
            self.verify_replay(command, 'manual_' + str(index))
        qualified = self.sync()
        self.check(self.players[0]['oracle'].evaluate(self.calls)['qualified'] and
                   state_of(qualified['snapshot']) == 'in_progress', 'qualification_without_auto_award')
        winner_command, response = self.mutation('/winner', {'player_id': self.players[0]['id'],
                                                           'expected_revision': qualified['view_revision']})
        terminal = winner_command.effective(response)
        self.winner = {'player_id': self.players[0]['id'], 'alias': self.players[0]['alias']}
        self.check(terminal['state'] == 'resolved' and terminal['winner'] == self.winner and
                   terminal['history_available'] is True and
                   terminal['history_expires_at'] == history_expiry(terminal['ended_at']),
                   'winner_commits_terminal_history')
        self.assert_model('resolved', stage='terminal')
        for index, player in enumerate(self.players):
            final = self.sync(player['actor'], 'player')['snapshot']
            self.check(final['winner'] == self.winner and final['role'] == 'final_player',
                       'terminal_player_' + str(index) + '_own_board')
        self.verify_replay(winner_command, 'winner')
        denied = self.spectator.request(self.base + '/sync?view=spectator', expected=(401,))
        self.check(set(denied.value) == {'error'}, 'spectator_post_terminal_denied')
        history_path = '/api/history/' + self.game_id
        history = self.host.request(history_path).value['history']
        expected_keys = {'game_id', 'game_code', 'designated_host_id', 'outcome', 'started_at', 'ended_at',
                         'expires_at', 'winner', 'ordered_calls', 'players'}
        self.check(set(history) == expected_keys and history['game_id'] == self.game_id and
                   history['game_code'] == self.code and history['designated_host_id'] == self.account_id and
                   history['outcome'] == 'resolved' and history['winner'] == self.winner and
                   history['ended_at'] == terminal['ended_at'] and history['expires_at'] == terminal['history_expires_at']
                   and history['started_at'] == running['started_at'] and history['ordered_calls'] == self.calls,
                   'history_exact_outcome_calls_times')
        expected_players = [{'player_id': p['id'], 'alias': p['alias'], 'side_length': 5,
                             'cells': p['oracle'].evaluate(self.calls)['cells']} for p in self.players]
        self.check(history['players'] == sorted(expected_players, key=lambda p: p['player_id']), 'history_exact_boards')
        listing = self.host.request('/api/history?limit=50&outcome=resolved').value
        summary = {k: v for k, v in history.items() if k not in ('players', 'ordered_calls')}
        self.check(summary in listing['games'], 'history_list_contains_exact_summary')
        self.check(self.host.request('/api/games/current').value == {'slot': 'free', 'game': None}, 'terminal_reservation_released')
        self.host.request(self.base + '/exit?view=account', method='POST', body={}, command=str(uuid.uuid7()))
        self.check(self.host.request(history_path).value == {'history': history}, 'history_immutable_after_exit')
        self.host.request(self.base + '/sync?view=account', expected=(401,))
        self.check(True, 'account_final_grant_exit_denied')
        _, next_base = self.create()
        next_command = Command(self.host, next_base + '/cancel', {'confirmed': True, 'expected_state': 'new'},
                               next_base.split('/')[3])
        cancelled = next_command.effective(next_command.run())
        self.check(cancelled['state'] == 'cancelled' and cancelled['history_available'] is False,
                   'released_slot_recreate_confirmed_cancel')
        self.check(self.host.request(history_path).value == {'history': history}, 'history_immutable_after_successor')
        self.check(self.host.request('/api/games/current').value == {'slot': 'free', 'game': None}, 'successor_reservation_released')
        notice = self.spectator_stream.terminal_if_present()
        if notice is not None:
            self.check(notice['state'] == 'resolved' and notice['winner'] == self.winner and
                       call_values(notice['calls']) == self.calls, 'spectator_terminal_public_if_delivered')
        self.check(all(s.ack_count >= 1 for s in self.streams), 'all_streams_real_snapshot_ack')

    def cleanup(self):
        failed = False
        self.cleanup_errors = []
        # Keep sockets alive until own nonterminal games have been cancelled.
        for command in self.creation_commands:
            if command.game_id is None and self.bootstrap.account_id:
                try:
                    self.observe_creation(command)
                except Exception:
                    failed = True
                    self.cleanup_errors.append('creation_discovery_failed')
        for game_id in tuple(self.owned_games):
            base = '/api/games/' + game_id
            try:
                reply = self.host.request(base + '/sync?view=account', expected=(200, 401, 404))
                if reply.status == 200:
                    state = state_of(reply.value['snapshot'])
                    if state in ('new', 'awaiting_players', 'in_progress'):
                        Command(self.host, base + '/cancel', {'confirmed': True, 'expected_state': state}, game_id).run()
                elif reply.status == 401:
                    # Exited terminal grants cannot be read; require this game is not current.
                    current = self.host.request('/api/games/current').value
                    require(current.get('game', {}).get('game_id') != game_id if current.get('game') else True,
                            'cleanup_own_game_unresolved')
            except Exception:
                failed = True
                self.cleanup_errors.append('own_game_cleanup_failed')
        for stream in self.streams:
            try:
                stream.close()
            except Exception:
                failed = True
                self.cleanup_errors.append('stream_close_failed')
        if not failed:
            try:
                self.cleanup_status = self.bootstrap.remove()
                if self.cleanup_status == 'own_account_removed':
                    self.check(True, 'cleanup_own_account_absent')
            except Exception:
                failed = True
                self.cleanup_errors.append('own_account_remove_failed')
        if failed:
            self.cleanup_status = 'failed_own_resources_may_remain'
        return not failed
