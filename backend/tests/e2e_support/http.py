"""Origin-bound actors and exact-payload command retries."""
from dataclasses import dataclass
import http.cookiejar
from http.cookies import SimpleCookie
import json
import time
import urllib.error
import urllib.request
from urllib.parse import urlsplit
import uuid
from .safety import (COMMAND_TIMEOUT, HTTP_TIMEOUT, MAX_JSON_BYTES, E2EError,
                     identifier, json_object, require)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise E2EError('redirect_rejected')


class CookiePolicy(http.cookiejar.DefaultCookiePolicy):
    def set_ok(self, cookie, request):
        return (not cookie.domain_specified and cookie.secure and cookie.has_nonstandard_attr('HttpOnly')
                and super().set_ok(cookie, request))


@dataclass(repr=False)
class Reply:
    status: int
    value: dict
    set_cookie: bool


class Actor:
    def __init__(self, target):
        self.target = target
        self.jar = http.cookiejar.CookieJar(policy=CookiePolicy())
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect(),
            urllib.request.HTTPSHandler(context=target.tls()))

    def cookie_header(self, path):
        request = urllib.request.Request(self.target.url(path))
        self.jar.add_cookie_header(request)
        return request.get_header('Cookie', '')

    def request(self, path, *, method='GET', body=None, wire=None, command=None,
                expected=(200,), timeout=HTTP_TIMEOUT, max_bytes=MAX_JSON_BYTES):
        url = self.target.url(path)
        require(method in ('GET', 'POST'), 'http_method_rejected')
        headers = {'Origin': self.target.origin, 'Accept': 'application/json'}
        if command:
            headers['Idempotency-Key'] = identifier(command)
        if body is not None:
            require(wire is None, 'duplicate_request_body')
            wire = json.dumps(body, separators=(',', ':'), allow_nan=False).encode('utf8')
        if wire is not None:
            require(len(wire) <= MAX_JSON_BYTES, 'request_size_bound')
            headers['Content-Type'] = 'application/json'
        request = urllib.request.Request(url, method=method, headers=headers, data=wire)
        self.jar.add_cookie_header(request)
        try:
            try:
                response = self.opener.open(request, timeout=timeout)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                require(response.status not in (301, 302, 303, 307, 308), 'redirect_rejected')
                require(response.status in expected, 'http_status_mismatch')
                require(response.geturl() == url, 'response_origin_mismatch')
                require(response.headers.get('Cache-Control') == 'no-store' and
                        response.headers.get('Referrer-Policy') == 'no-referrer', 'response_privacy_headers')
                raw = response.read(max_bytes + 1)
                require(len(raw) <= max_bytes, 'response_size_bound')
                value = json_object(raw)
                cookies = response.headers.get_all('Set-Cookie', [])
                for raw_cookie in cookies:
                    self.validate_cookie(raw_cookie, path)
                self.jar.extract_cookies(response, request)
                return Reply(response.status, value, bool(cookies))
        except (OSError, urllib.error.URLError, ValueError):
            raise E2EError('http_transport_failed') from None

    @staticmethod
    def validate_cookie(raw, path):
        parsed = SimpleCookie()
        try:
            parsed.load(raw)
        except Exception:
            raise E2EError('cookie_rejected') from None
        require(len(parsed) == 1, 'cookie_rejected')
        name, morsel = next(iter(parsed.items()))
        require(bool(morsel['secure']) and bool(morsel['httponly']) and not morsel['domain'], 'cookie_rejected')
        if name == '__Host-brews_session':
            require(morsel['path'] == '/' and path.startswith('/api/auth/'), 'cookie_scope_rejected')
        else:
            require(name in ('__Secure-brews-player', '__Secure-brews-spectator',
                             '__Secure-brews-admission', '__Secure-brews-audience-admission'), 'cookie_rejected')
            parts = urlsplit(path).path.split('/')
            require(len(parts) >= 5 and parts[:3] == ['', 'api', 'games'], 'cookie_scope_rejected')
            game = identifier(parts[3])
            require(morsel['path'] == '/api/games/' + game, 'cookie_scope_rejected')


class Command:
    """The UUID and serialized body are immutable across every explicit retry."""
    def __init__(self, actor: Actor, path: str, body: dict, game_id=None):
        self.actor, self.path = actor, path
        self.command_id = str(uuid.uuid7())
        self.wire = json.dumps(body, separators=(',', ':'), allow_nan=False).encode('utf8')
        self.game_id = identifier(game_id) if game_id else None
        self.receipt = None
        self.pending_observed = False

    def submit(self, timeout=HTTP_TIMEOUT):
        return self.actor.request(self.path, method='POST', wire=self.wire,
                                  command=self.command_id, expected=(200, 202), timeout=timeout)

    def lookup(self, timeout=HTTP_TIMEOUT):
        require(self.game_id is not None, 'command_game_unknown')
        result = self.actor.request('/api/games/' + self.game_id + '/commands/' + self.command_id +
                                    '?view=account', timeout=timeout, max_bytes=4096)
        require(not result.set_cookie and set(result.value) == {
            'version', 'game_id', 'command_id', 'result_status'} and result.value['version'] == 1 and
            result.value['game_id'] == self.game_id and result.value['command_id'] == self.command_id,
            'command_lookup_mismatch')
        return result.value['result_status']

    def run(self, creation_observer=None):
        deadline = time.monotonic() + COMMAND_TIMEOUT
        for _ in range(60):
            remaining = deadline - time.monotonic()
            require(remaining > 0, 'command_timeout')
            reply = self.submit(min(HTTP_TIMEOUT, remaining))
            if reply.status == 200:
                value = reply.value
                receipt = value.get('receipt')
                require(type(receipt) is dict and receipt.get('command_id') == self.command_id,
                        'command_receipt_mismatch')
                if 'game_id' in receipt:
                    game_id = identifier(receipt['game_id'])
                    require(self.game_id in (None, game_id), 'command_receipt_game_mismatch')
                    self.game_id = game_id
                self.receipt = receipt
                if value.get('result') == 'committed':
                    require(not reply.set_cookie and set(value) == {'result', 'receipt'}, 'receipt_only_replay')
                return value
            require(not reply.set_cookie and set(reply.value) == {'result', 'operation_id'} and
                    reply.value['result'] == 'pending', 'pending_response_mismatch')
            identifier(reply.value['operation_id'])
            self.pending_observed = True
            if self.game_id is None and creation_observer:
                creation_observer(self)
            if self.game_id:
                status = self.lookup(min(HTTP_TIMEOUT, max(0.1, deadline - time.monotonic())))
                require(status.get('status') in ('pending', 'committed', 'unknown'), 'command_lookup_unusable')
                if status['status'] == 'committed':
                    require(type(status.get('follow_up_pending')) is bool, 'command_lookup_mismatch')
            time.sleep(min(0.25, max(0, deadline - time.monotonic())))
        raise E2EError('command_timeout')

    def effective(self, response: dict) -> dict:
        if response.get('result') != 'committed':
            return response
        outcome = self.receipt.get('outcome')
        require(type(outcome) is dict, 'command_outcome_missing')
        return outcome

    def replay(self):
        require(self.receipt is not None, 'command_receipt_missing')
        reply = self.submit()
        require(reply.status == 200 and not reply.set_cookie and
                reply.value == {'result': 'committed', 'receipt': self.receipt}, 'receipt_only_replay')
