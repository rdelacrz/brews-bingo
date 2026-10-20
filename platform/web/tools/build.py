"""Build browser assets locally; no Wrangler publish/upload invocation."""
from pathlib import Path
import argparse
import base64
import hashlib
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[3]
WEB = ROOT / 'platform/web'
PUBLIC = ROOT / 'target/dx/brews-web/release/web/public'


def protect_assets(directory: Path):
    index = directory / 'index.html'
    html = index.read_text()
    scripts = re.findall(r'<script\b[^>]*>(.*?)</script>', html, re.DOTALL | re.IGNORECASE)
    hashes = ["'sha256-" + base64.b64encode(hashlib.sha256(body.encode()).digest()).decode() + "'" for body in scripts if body.strip()]
    csp = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval' " + ' '.join(hashes) + "; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; font-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
    (directory / '_headers').write_text('/*\n  Cache-Control: no-store\n  Referrer-Policy: no-referrer\n  X-Content-Type-Options: nosniff\n  X-Frame-Options: DENY\n  Permissions-Policy: camera=(), microphone=(), geolocation=()\n  Content-Security-Policy: ' + csp + '\n')
    # Precompressed HTML no longer represents the final header inventory after packaging.
    for extension in ('.br', '.gz'):
        (directory / ('_headers' + extension)).unlink(missing_ok=True)
    return hashlib.sha256(index.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dx', default='dx')
    args = parser.parse_args()
    if shutil.which(args.dx) is None:
        raise SystemExit('Install the pinned Dioxus CLI 0.7.10 or pass --dx /path/to/dx')
    version = subprocess.check_output([args.dx, '--version'], text=True)
    if not version.startswith('dioxus 0.7.10 '):
        raise SystemExit('Dioxus CLI 0.7.10 is required')
    subprocess.run(['npm', 'ci', '--ignore-scripts'], cwd=WEB, check=True)
    subprocess.run(['npm', 'run', 'css'], cwd=WEB, check=True)
    subprocess.run([args.dx, 'build', '--web', '--release', '--package', 'brews-web', '--locked'], cwd=WEB, check=True)
    if not (PUBLIC / 'index.html').is_file():
        raise SystemExit('Expected Dioxus static artifact was not produced')
    print('Web artifact: ' + str(PUBLIC.relative_to(ROOT)))
    print('HTML SHA-256: ' + protect_assets(PUBLIC))


if __name__ == '__main__':
    main()
