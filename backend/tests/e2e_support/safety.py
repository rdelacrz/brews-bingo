"""Target, filesystem and diagnostic boundaries; no network at import."""
import ipaddress
import json
import os
from pathlib import Path
import re
import ssl
from urllib.parse import urlsplit
import uuid

MAX_JSON_BYTES = 256 * 1024
HTTP_TIMEOUT = 10
COMMAND_TIMEOUT = 45
WAIT_TIMEOUT = 25


class E2EError(Exception):
    """Contains only a fixed diagnostic code, never a submitted value."""


def require(condition: bool, code: str) -> None:
    if not condition:
        raise E2EError(code)


def safe_integer(value, minimum=0, maximum=2**53 - 1) -> bool:
    return type(value) is int and minimum <= value <= maximum


def identifier(value) -> str:
    require(isinstance(value, str) and bool(re.fullmatch(
        r'[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}', value)),
        'invalid_identifier')
    return value


def json_object(raw: bytes) -> dict:
    def pairs(items):
        result = {}
        for name, value in items:
            require(name not in result, 'duplicate_json_field')
            result[name] = value
        return result
    try:
        require(len(raw) <= MAX_JSON_BYTES, 'json_size_bound')
        value = json.loads(raw, object_pairs_hook=pairs,
                           parse_constant=lambda _: (_ for _ in ()).throw(E2EError('invalid_json')))
        require(type(value) is dict, 'json_object_required')
        return value
    except (ValueError, UnicodeError, RecursionError):
        raise E2EError('invalid_json') from None


class Target:
    def __init__(self, origin: str, expected: str, environment: str, ca_file=None):
        require(environment in ('local', 'staging'), 'environment_rejected')
        require(origin == expected, 'origin_mismatch')
        require(isinstance(origin, str) and origin.isascii() and not any(
            c.isspace() or ord(c) < 33 for c in origin), 'origin_rejected')
        try:
            parsed = urlsplit(origin)
            host, port = parsed.hostname, parsed.port if parsed.port is not None else 443
        except ValueError:
            raise E2EError('origin_rejected') from None
        require(parsed.scheme == 'https' and bool(host) and parsed.netloc and
                '@' not in parsed.netloc and not parsed.username and not parsed.password and not parsed.path and
                not parsed.query and not parsed.fragment and '?' not in origin and '#' not in origin and
                '\\' not in origin and '%' not in origin and 1 <= port <= 65535,
                'origin_rejected')
        require(host == host.lower() and not host.endswith('.'), 'origin_rejected')
        require(not parsed.netloc.endswith(':') and not parsed.netloc.endswith('.'), 'origin_rejected')
        local = host in ('localhost', '127.0.0.1', '::1')
        if environment == 'local':
            require(local, 'local_target_required')
        else:
            require(not local and ca_file is None, 'staging_target_rejected')
            try:
                ipaddress.ip_address(host)
            except ValueError:
                pass
            else:
                raise E2EError('staging_target_rejected')
            labels = host.split('.')
            require(len(labels) >= 2 and all(re.fullmatch(r'[a-z0-9](?:[a-z0-9-]*[a-z0-9])?', s)
                    for s in labels), 'staging_target_rejected')
            require(not any(s in ('localhost', 'invalid', 'example', 'test', 'local', 'internal',
                                  'prod', 'production') for s in labels) and
                    host not in ('example.com', 'example.org', 'example.net') and
                    not any(re.search(r'(^|-)prod(?:uction)?($|-)', s) for s in labels),
                    'staging_target_rejected')
        self.origin, self.environment, self.host, self.port = origin, environment, host, port
        self.ca_file = str(Path(ca_file).resolve(strict=True)) if ca_file else None

    def tls(self):
        return ssl.create_default_context(cafile=self.ca_file)

    def url(self, path: str) -> str:
        require(isinstance(path, str) and path.startswith('/api/') and path.isascii() and
                not any(c.isspace() or ord(c) < 33 for c in path) and
                not any(s in path for s in ('\\', '#', '://', '//', '..', '%')), 'api_path_rejected')
        return self.origin + path


def private_output(path, repository: Path) -> Path:
    requested = Path(path)
    require(requested.is_absolute(), 'output_absolute_required')
    # Resolve ancestors before creation; never accept a symlink/existing destination.
    resolved = requested.parent.resolve(strict=True) / requested.name
    require(not resolved.is_relative_to(repository.resolve()) and
            not requested.exists() and not requested.is_symlink(), 'output_fresh_outside_repository_required')
    try:
        resolved.mkdir(mode=0o700)
        require(resolved.stat().st_mode & 0o777 == 0o700, 'output_permissions')
    except OSError:
        raise E2EError('output_creation_failed') from None
    return resolved


class Checks:
    def __init__(self):
        self.labels = set()

    def check(self, condition: bool, label: str):
        require(bool(re.fullmatch(r'[a-z][a-z0-9_]{0,79}', label)), 'unsafe_check_label')
        require(condition, label)
        require(label not in self.labels, 'duplicate_check_label')
        self.labels.add(label)

    def summary(self, environment, passed, cleanup):
        return {'full_game_e2e': 'passed' if passed else 'failed', 'environment': environment,
                'checks': sorted(self.labels), 'check_count': len(self.labels),
                'cleanup': cleanup, 'history_policy': 'retained_until_contract_expiry'}
