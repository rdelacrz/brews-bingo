"""Restricted CLI handoff; credentials only in environment, links promptly unlinked."""
import os
from pathlib import Path
import secrets
import subprocess
import threading
import time
from urllib.parse import urlsplit
import uuid
from .safety import E2EError, identifier, json_object, require

CLI_TIMEOUT = 45
CLI_OUTPUT_BYTES = 64 * 1024


def run_cli(path, args, environment, *, expect_missing_account=False):
    if expect_missing_account:
        require(len(args) == 3 and args[:2] == ['accounts', 'get'], 'cli_delete_readback_invalid')
        identifier(args[2])
    process = None
    readers = []
    output = bytearray()
    overflow = threading.Event()

    def drain(pipe, retain):
        total = 0
        try:
            while chunk := pipe.read(4096):
                total += len(chunk)
                if total > CLI_OUTPUT_BYTES:
                    overflow.set()
                    try:
                        process.kill()
                    except OSError:
                        pass
                    break
                if retain:
                    output.extend(chunk)
        except (OSError, ValueError):
            overflow.set()
        finally:
            pipe.close()

    try:
        process = subprocess.Popen([str(path), '--json', *args], env=environment,
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for pipe, retain in ((process.stdout, True), (process.stderr, expect_missing_account)):
            reader = threading.Thread(target=drain, args=(pipe, retain), daemon=True)
            readers.append(reader)
            reader.start()
        process.wait(timeout=CLI_TIMEOUT)
        for reader in readers:
            reader.join(timeout=2)
        require(not overflow.is_set() and all(not r.is_alive() for r in readers), 'cli_output_bound')
        require(process.returncode == (1 if expect_missing_account else 0), 'cli_operation_failed')
        lines = output.splitlines()
        require(1 <= len(lines) <= 3, 'cli_metadata_invalid')
        values = [json_object(line) for line in lines]
        if expect_missing_account:
            require(len(values) == 1 and values[0] == {'error': {
                'code': 'http_rejected',
                'message': 'management endpoint rejected request (HTTP 404); no automatic retry was made',
            }}, 'cli_delete_readback_failed')
            return values[0]
        results = [v for v in values if 'result' in v]
        require(len(results) == 1, 'cli_metadata_invalid')
        return results[0]
    except (OSError, subprocess.TimeoutExpired):
        raise E2EError('cli_operation_failed') from None
    finally:
        if process and process.poll() is None:
            process.kill()
            process.wait(timeout=5)
        for reader in readers:
            reader.join(timeout=2)


class Bootstrap:
    def __init__(self, target, cli_path, output):
        self.target, self.path, self.output = target, Path(cli_path).resolve(strict=True), output
        require(self.path.is_file() and os.access(self.path, os.X_OK), 'bootstrap_cli_invalid')
        require(bool(os.environ.get('BREWS_DEV_CLI_KEY')), 'cli_credentials_missing')
        require(os.environ.get('BREWS_API_ORIGIN') == target.origin, 'cli_origin_mismatch')
        self.environment = dict(os.environ)
        self.environment.pop('BREWS_TLS_CA_FILE', None)
        if target.ca_file:
            self.environment['BREWS_TLS_CA_FILE'] = target.ca_file
        # The CLI transport itself rejects redirects and uses its configured HTTPS origin.
        for name in ('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'http_proxy', 'https_proxy', 'all_proxy'):
            self.environment.pop(name, None)
        self.account_id = None
        self.create_attempted = False
        self.create_id = str(uuid.uuid7())
        self.username = 'E2E' + secrets.token_hex(8)
        self.delete_id = str(uuid.uuid7())

    def enroll(self, actor):
        link = self.output / 'enrollment.url'
        try:
            self.create_attempted = True
            result = run_cli(self.path, ['accounts', 'create', '--role', 'host', '--username', self.username,
                                        '--command-id', self.create_id, '--link-output', str(link)], self.environment)
            receipt = result.get('receipt', {})
            require(result.get('result') == 'issued' and result.get('link_written') is True and
                    receipt.get('command_id') == self.create_id and receipt.get('operation') == 'create_account',
                    'cli_create_mismatch')
            self.account_id = identifier(receipt.get('account_id'))
            require(link.is_file() and not link.is_symlink() and link.stat().st_mode & 0o777 == 0o600 and
                    link.stat().st_size <= 4096, 'enrollment_file_invalid')
            url = link.read_text().strip()
            parsed = urlsplit(url)
            require(parsed.scheme + '://' + parsed.netloc == self.target.origin and parsed.path == '/enroll'
                    and not parsed.query and bool(parsed.fragment), 'enrollment_origin_mismatch')
            token = parsed.fragment
        finally:
            link.unlink(missing_ok=True)
        redemption = actor.request('/api/auth/enrollment/redeem', method='POST',
                    body={'enrollment_token': token}, command=str(uuid.uuid7()))
        require(redemption.set_cookie, 'enrollment_cookie_missing')
        password = secrets.token_urlsafe(24)
        completion = actor.request('/api/auth/enrollment/complete', method='POST',
                    body={'new_password': password}, command=str(uuid.uuid7()))
        require(completion.set_cookie, 'account_cookie_missing')
        return self.account_id

    def recover_creation_for_cleanup(self):
        # A lost CLI reply cannot justify another command ID or deleting by username.
        # The new handoff filename is local transport only; the original API payload
        # and UUID remain exactly the same. Never redeem a cleanup-only link.
        link = self.output / 'cleanup-enrollment.url'
        try:
            result = run_cli(self.path, ['accounts', 'create', '--role', 'host', '--username', self.username,
                                        '--command-id', self.create_id, '--link-output', str(link)], self.environment)
            receipt = result.get('receipt', {})
            require(result.get('result') in ('issued', 'committed') and
                    receipt.get('command_id') == self.create_id and receipt.get('operation') == 'create_account',
                    'cli_create_cleanup_mismatch')
            self.account_id = identifier(receipt.get('account_id'))
        finally:
            link.unlink(missing_ok=True)

    def remove(self):
        if self.account_id is None and self.create_attempted:
            self.recover_creation_for_cleanup()
        if self.account_id is None:
            return 'no_account_created'
        deadline = time.monotonic() + 60
        for _ in range(10):
            result = run_cli(self.path, ['accounts', 'delete', self.account_id, '--yes',
                                        '--command-id', self.delete_id], self.environment)
            if result.get('result') == 'committed':
                receipt = result.get('receipt', {})
                require(receipt.get('account_id') == self.account_id and receipt.get('command_id') == self.delete_id
                        and receipt.get('operation') == 'delete_account', 'cli_delete_mismatch')
                run_cli(self.path, ['accounts', 'get', self.account_id], self.environment,
                        expect_missing_account=True)
                return 'own_account_removed'
            require(result.get('result') == 'pending', 'cli_delete_mismatch')
            require(time.monotonic() < deadline, 'cli_delete_timeout')
            time.sleep(0.25)
        raise E2EError('cli_delete_timeout')
