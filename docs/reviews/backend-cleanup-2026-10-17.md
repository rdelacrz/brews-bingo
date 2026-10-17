# Backend transport cleanup — 2026-10-17

Baseline: `7d97c3ba3bd76c04a61a5ec277503477a8161867`.
Scope: local behavior-preserving cleanup under the user's `/ponytail ultra` request; no publication or deployment.

## Changes

- `backend/src/api/utility.rs` owns shared exact-Origin, strict JSON media-type, canonical command-ID, bounded original-byte JSON-object and cookie checks. Authentication now uses the existing domain `CommandId` parser instead of duplicating UUID validation. All affected API callers and the private recovery wire use these helpers; obsolete recovery-decoder re-exports are removed.
- Endpoint policy stays at the call site: authentication's broader media grammar, optional versus required Origin, bodyless operations, retry-header requiredness, validation ordering and error categories remain unchanged. Cookie selection, duplicate rejection and byte bounds are preserved.
- `backend/src/worker_runtime/response.rs` shares private JSON/security headers and byte-response construction. Game, account, History, discovery, dedicated-read and deployment-identity adapters reuse it. History/discovery retain `application/json`; other affected responses retain `application/json; charset=utf-8`. Discovery keeps its bounded serializer. Deployment identity no longer depends on the game response module.
- Remove the duplicate status assignment in game rejection output and the temporary allocation in History cursor splitting, preserving exact component count and validation.

Net reduction: **79 non-test source lines** across the affected implementation files, including the new helpers. Four new test functions cover original-byte/size/header validation and response construction; the two API tests also run in the existing exact-source private-wire harness, yielding six additional native test instances. Existing embedded test bodies and integration/Worker test sources are unchanged.

No dependency, schema, storage migration, core gameplay/authentication/authorization, cryptographic, expiry, replay, auditing, WebSocket lifecycle or planning-policy changes. Repeated authority/expiry checks were not treated as removable duplication. `AGENTS.md` is unchanged.

## Executed verification

The full `release_gate.py` run completed with exit **0**, using the pinned Rust 1.99.0 environment and all lanes:

| Check | Result |
| --- | --- |
| Native nextest, debug | 965 passed |
| Native nextest, release-assertions | 965 passed |
| Doctests | 12 passed |
| Native and Wasm strict Clippy; formatting | Passed |
| Worker build | Passed |
| General Workerd suites | 723 passed, no failed/skipped cases |
| Current authorization-time snapshot contract | 20 passed |
| Local HTTPS/WSS | 370 checks passed |
| Python tooling tests | 135 passed |

The HTTPS/WSS total is the actual execution count: gameplay 151, operator 54, dedicated reads 106 and game audit 59. Hash-set comparison with the previous layout run confirms every fixed check is identical; only the value-dependent `manual-revision-`, `manual-call-` and `manual-receipt-only-` checks differ. No test was removed to account for the previous run's 373 total.

The unchanged historical receiving-cut probe returned exit 1 under the superseded requirement. The runner validated its exact classification; it is retained evidence, not counted as passing current acceptance. The separate remote/full-game promotion harness was not rerun by this cleanup gate.

An initial archive-only unit-test replay failed its success assertion; repeating the archive verification with the explicit pinned environment passed all 135 tests without source changes. The integrated gate itself passed on its first run.

## Review and evidence

[Machine-readable evidence](backend-cleanup-2026-10-17-evidence.json) contains the executed gate report, redacted command descriptors, source/artifact hashes and independent static review. The review found no blocking defect in the supplied exact diff/source bundle. Direct delegated filesystem access failed at sandbox startup, so this is a bounded supplied-source review, not independent test execution or a whole-backend acceptance audit.

All 299 release inputs and Worker artifacts matched the completed gate; all 25 review-input hashes matched the reviewed candidate. The refreshed code graph verifies 267 source hashes. Its structural extractor still reports 1,109 dangling-endpoint and 541 collapsed directed edges; it is navigation evidence, not a complete dependency or runtime proof. The separate documentation graph remains stale.

The seven external production gates and unrelated final slice reviews remain open. No frontend, provider provisioning, secrets, deployment, commit or push was performed.
