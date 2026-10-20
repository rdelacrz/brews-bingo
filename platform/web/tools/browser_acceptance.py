"""Actual browser-to-local-Worker acceptance with ephemeral bootstrap only."""
from pathlib import Path
import argparse
import json
import os
import secrets
import subprocess
from local_worker import ROOT, WEB, local_worker


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if ROOT in output.parents or output == ROOT:
        raise SystemExit('Keep test artifacts outside the source tree')
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    with local_worker() as (origin, environment, scratch):
        username = 'WebAdmin' + secrets.token_hex(5)
        link = scratch / 'enrollment.url'
        result = subprocess.run([
            str(ROOT / 'target/debug/brews'), '--json', 'accounts', 'create', '--role', 'admin',
            '--username', username, '--link-output', str(link),
        ], cwd=ROOT, env=environment, capture_output=True, timeout=45)
        if result.returncode:
            raise SystemExit('Local test-only CLI bootstrap failed')
        enrollment = link.read_text().strip()
        link.unlink()
        if not enrollment.startswith(origin + '/enroll#'):
            raise SystemExit('Unexpected bootstrap link origin')
        payload = json.dumps({'origin': origin, 'username': username, 'enrollment': enrollment, 'output': str(output)})
        browser = subprocess.run(['node', str(WEB / 'tests/browser.mjs')], cwd=ROOT, input=payload, text=True, capture_output=True, timeout=420)
        # Browser output contains only step IDs and a controlled final JSON object.
        try:
            summary: dict[str, object] = json.loads(browser.stdout)
        except (json.JSONDecodeError, ValueError):
            summary = {'status': 'failed', 'reason': 'browser_runner_did_not_produce_safe_summary'}
        summary['process_exit_code'] = browser.returncode
        summary['deployment'] = 'none_local_workerd_only'
        summary['raw_process_output_persisted'] = False
        (output / 'report.json').write_text(json.dumps(summary, indent=2) + '\n')
        print(json.dumps(summary, indent=2))
        if browser.returncode or summary.get('status') != 'passed':
            raise SystemExit(1)


if __name__ == '__main__':
    main()
