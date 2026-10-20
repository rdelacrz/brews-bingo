"""Local static-assets/Worker integration. No deployment or remote account access."""
from contextlib import contextmanager
from pathlib import Path
import json
import os
import secrets
import socket
import ssl
import subprocess
import tempfile
import time
import tomllib
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
WEB = ROOT / 'platform/web'
PUBLIC = ROOT / 'target/dx/brews-web/release/web/public'


def configuration(assets: Path) -> dict:
    """Use the existing API's bindings/migrations; never copy private env values."""
    base = tomllib.loads((ROOT / 'backend/wrangler.toml').read_text())
    for key in ('env', 'build', 'vars'):
        base.pop(key, None)
    base['main'] = str(ROOT / 'backend/build/worker/shim.mjs')
    base['assets'] = {
        'directory': str(assets.resolve()),
        'not_found_handling': 'single-page-application',
        'run_worker_first': ['/api', '/api/*', '/_dev/*'],
    }
    base['workers_dev'] = False
    base['preview_urls'] = False
    base['routes'] = []
    return base


@contextmanager
def local_worker(assets: Path = PUBLIC):
    if not (assets / 'index.html').is_file():
        raise RuntimeError('Build the Dioxus web artifact first')
    if not (ROOT / 'backend/build/worker/shim.mjs').is_file():
        raise RuntimeError('Build the API Worker first')
    scratch_root = os.environ.get('TMPDIR', '/root/.hermes/cache/scratch')
    with tempfile.TemporaryDirectory(prefix='brews-web-local-', dir=scratch_root) as directory:
        scratch = Path(directory)
        scratch.chmod(0o700)
        key, cert = scratch / 'key.pem', scratch / 'cert.pem'
        ca, ca_key, csr = scratch / 'ca.pem', scratch / 'ca-key.pem', scratch / 'leaf.csr'
        commands = [
            ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-subj', '/CN=Brews browser test CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign', '-keyout', str(ca_key), '-out', str(ca)],
            ['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost,IP:127.0.0.1', '-addext', 'basicConstraints=critical,CA:FALSE', '-addext', 'keyUsage=critical,digitalSignature,keyEncipherment', '-addext', 'extendedKeyUsage=serverAuth', '-keyout', str(key), '-out', str(csr)],
            ['openssl', 'x509', '-req', '-in', str(csr), '-CA', str(ca), '-CAkey', str(ca_key), '-CAcreateserial', '-days', '1', '-copy_extensions', 'copy', '-out', str(cert)],
        ]
        for command in commands:
            if subprocess.run(command, capture_output=True).returncode:
                raise RuntimeError('Local certificate generation failed')
        key.chmod(0o600)
        ca_key.chmod(0o600)
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        origin = f'https://localhost:{port}'
        cli_key = secrets.token_urlsafe(32)
        config = scratch / 'worker.json'
        config.write_text(json.dumps(configuration(assets)))
        variables = scratch / 'worker.env'
        variables.write_text(f'APP_ORIGIN={origin}\nRATE_LIMIT_KEY={secrets.token_urlsafe(32)}\nDEV_CLI_KEY={cli_key}\n')
        variables.chmod(0o600)
        environment = {**os.environ, 'WRANGLER_SEND_METRICS': 'false', 'WRANGLER_LOG_PATH': str(scratch / 'wrangler.log'), 'BREWS_API_ORIGIN': origin, 'BREWS_DEV_CLI_KEY': cli_key, 'BREWS_TLS_CA_FILE': str(ca)}
        process = subprocess.Popen([
            str(ROOT / 'backend/node_modules/.bin/wrangler'), 'dev', '--config', str(config), '--local',
            '--local-protocol', 'https', '--https-key-path', str(key), '--https-cert-path', str(cert),
            '--env-file', str(variables), '--ip', '127.0.0.1', '--port', str(port),
            '--persist-to', str(scratch / 'state'), '--log-level', 'error',
        ], cwd=ROOT / 'backend', env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            tls = ssl.create_default_context(cafile=str(ca))
            for _ in range(200):
                if process.poll() is not None:
                    raise RuntimeError('Local Worker stopped before readiness')
                try:
                    with urllib.request.urlopen(origin + '/api/session', context=tls, timeout=1) as response:
                        if response.status == 200:
                            with urllib.request.urlopen(origin + '/', context=tls, timeout=2) as asset:
                                if asset.status == 200 and asset.headers.get_content_type() == 'text/html':
                                    asset.read(256)
                                    break
                except (OSError, urllib.error.URLError):
                    time.sleep(.2)
            else:
                raise RuntimeError('Local Worker readiness deadline exceeded')
            yield origin, environment, scratch
        finally:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--assets', type=Path, default=PUBLIC)
    args = parser.parse_args()
    with local_worker(args.assets) as (origin, _, __):
        print(f'Local-only web UI: {origin} (ephemeral database; Ctrl-C stops)', flush=True)
        try:
            while True:
                time.sleep(3600)
        except KeyboardInterrupt:
            pass
