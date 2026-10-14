#!/usr/bin/env python3
"""Run one full game against an existing, explicitly pinned HTTPS target.

This runner creates only its own throwaway host and games through supported APIs.
It never starts a Worker, changes provider configuration or wipes storage. History
is retained until the backend's contractual expiry. Staging namespace/deployment
identity must be verified independently by the deployment tooling before invoking
this runner; --environment staging and matching origins do not establish identity.
Only redacted checks/counts are written to the fresh private output directory.
"""
import argparse
import json
import os
from pathlib import Path
import sys

# Importing this module is inert; in particular, never import gameplay_https_wss.
SUPPORT_ROOT = Path(__file__).resolve().parent
if str(SUPPORT_ROOT) not in sys.path:
    sys.path.insert(0, str(SUPPORT_ROOT))
from e2e_support.safety import Checks, E2EError, Target, private_output, require
from e2e_support.scenario import Scenario

ROOT = SUPPORT_ROOT.parents[1]


class Parser(argparse.ArgumentParser):
    def error(self, message):
        self.exit(2, 'full_game_e2e: invalid command arguments; use --help\n')


def parser():
    result = Parser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    result.add_argument('--origin', required=True, help='Existing HTTPS origin, with no path or trailing slash')
    result.add_argument('--expected-origin', required=True, help='Explicit exact expected HTTPS origin')
    result.add_argument('--environment', required=True, choices=('staging', 'local'))
    result.add_argument('--output', required=True, help='NEW absolute private directory outside the repository')
    result.add_argument('--bootstrap-cli', required=True, help='Existing restricted brews CLI executable')
    result.add_argument('--tls-ca-file', help='Local mode only: trusted loopback CA file; verification remains enabled')
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    checks = Checks()
    scenario = None
    output = None
    passed = False
    failure = None
    cleanup = 'not_started'
    try:
        target = Target(args.origin, args.expected_origin, args.environment, args.tls_ca_file)
        require(bool(os.environ.get('BREWS_DEV_CLI_KEY')), 'cli_credentials_missing')
        require(os.environ.get('BREWS_API_ORIGIN') == target.origin, 'cli_origin_mismatch')
        binary = Path(args.bootstrap_cli).resolve(strict=True)
        require(binary.is_file() and os.access(binary, os.X_OK), 'bootstrap_cli_invalid')
        output = private_output(args.output, ROOT)
        scenario = Scenario(target, binary, output, checks)
        scenario.run()
        passed = True
    except KeyboardInterrupt:
        failure = 'interrupted'
    except E2EError as error:
        failure = str(error)
    except Exception:
        failure = 'runner_failed'
    finally:
        if scenario is not None:
            if not scenario.cleanup():
                passed = False
                failure = failure or 'cleanup_failed'
            cleanup = scenario.cleanup_status
    summary = checks.summary(args.environment, passed, cleanup)
    if failure:
        summary['failure_code'] = failure
    if scenario and scenario.cleanup_errors:
        summary['cleanup_errors'] = sorted(set(scenario.cleanup_errors))
    if passed:
        summary['accepted_call_count'] = len(scenario.calls)
        summary['player_count'] = len(scenario.players)
        summary['snapshot_ack_count'] = sum(s.ack_count for s in scenario.streams)
    if output:
        try:
            destination = output / 'summary.json'
            descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, 'w') as handle:
                json.dump(summary, handle, sort_keys=True)
                handle.write('\n')
        except OSError:
            summary['full_game_e2e'] = 'failed'
            summary['failure_code'] = 'summary_write_failed'
            passed = False
    print(json.dumps(summary, sort_keys=True))
    return 0 if passed else 1


if __name__ == '__main__':
    sys.exit(main())
