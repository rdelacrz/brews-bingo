# Player recovery route — 2026-10-17

Baseline: `69bc62d7cdcaefcfb880ddd52728a2018458ae99`. This checkpoint includes the previously verified game-read grouping/code route and the separately requested recovery URL change. The user authorized committing/pushing these changes, followed by a separate backend logic scan. No deployment is authorized.

## Changes and review

- Move recovery to `POST /api/games/player-recovery`; the submitted `game_code` still selects the existing game. The former `/api/player-recovery` returns 404 without an alias or redirect.
- Use the existing `/api/games/{*path}` Worker dispatch and remove the obsolete standalone router entry. Outside tests, the recovery decoder changes only its exact URL literal. Recovery proof, rate limits, stable identity/board, session rotation, authority and expiry policies are unchanged; no domain, database, schema or cryptographic code changes.
- Update executable callers and current API documentation. Add native/Workerd namespace and method/path regressions plus an actual HTTPS legacy-route rejection.
- Keep the archived receiving-cut probe and config byte-identical. A test-only transform adapts precisely its one renamed recovery URL, without changing assertions or line numbers. The release runner uses this config; a small self-check verifies transform scope, exact replacement, unchanged archived bytes and rejection of missing/duplicate matches. The adapter is a frozen release input, not a runtime alias.

The prior discovery/code-route review is recorded in [its checkpoint](game-reads-route-2026-10-17.md). Mechanical checks for this revision confirm that the production recovery decoder differs only by the URL and eleven affected existing runtime suites differ only by that replacement. Additional tests are separate from existing policy assertions. These are bounded static/mechanical checks, not independent whole-backend acceptance.

## Executed verification

The final full local gate (`brews-player-recovery-route-gates-03`) exits **0**:

| Check | Result |
| --- | --- |
| Native nextest, debug and release-assertions | 969 passed each |
| Doctests | 12 passed |
| General Workerd | 725 passed across 39 physical suites |
| Current authorization-time snapshot contract | 20 passed |
| Actual local HTTPS/WSS | 372 checks passed |
| Python tooling | 136 tests passed |
| Formatting, strict native/Wasm Clippy, fresh Worker/CLI builds | Passed |

All **299 release-input hashes**, Worker artifact hashes and **267 code graph source hashes** match the final candidate. The graph includes the new `.mjs` adapter; it has 1,103 dangling-endpoint and 541 collapsed directed edges, so it is navigation support rather than a runtime proof. The separate documentation graph is stale.

Earlier attempts remain separate: attempt 01 stopped at the pinned-toolchain prerequisite before tests; attempt 02 failed a native shared fixture assertion because proof-based recovery intentionally does not use that fixture's cookie-validation policy. The correction removed recovery from the cookie-bound fixture and kept its own namespace/method regression; runtime policy was not weakened. Focused native tests then passed, and the fresh complete attempt 03 passed all current lanes without skips.

The adapted historical probe still physically exits 1 at the exact superseded receiving-cut assertion; this is archived history, not a passing current case or a current-policy blocker. Local test totals do not include it as passed. Variable manual-call checks explain why the actual HTTPS/WSS total is not a fixed suite-size promise.

[Machine-readable evidence](player-recovery-route-2026-10-17-evidence.json) preserves the complete final gate, safe command descriptors, failed attempts, focused checks, frozen inputs/artifacts and graph hashes. No raw process logs, cookies, recovery answers or credentials are archived.

No provider deployment, provisioning, secret changes or production acceptance occurred. Unrelated final slice reviews and external production gates remain open. New findings from the subsequent requested scan are not part of this publication.
