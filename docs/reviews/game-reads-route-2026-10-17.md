# Game read grouping and code route — 2026-10-17

Baseline: `69bc62d7cdcaefcfb880ddd52728a2018458ae99`. Local changes only; no commit, push or deployment.

## Changes

- Merge `backend/src/api/discovery.rs` into `backend/src/api/games/reads.rs`, including its command/request types, transport helpers and existing tests. Remove the old source module and unused parent re-exports. Existing API-root Rust exports and decoder names remain available.
- Apply the separately requested route revision: `GET /api/games/codes/{code}` replaces `/api/game-codes/{code}`. The old URL returns 404 without an alias or redirect. Update the Worker router, executable test callers and current API/LLD descriptions.
- Add native, Workerd and actual HTTPS regression coverage for old-route rejection. The Worker regression also verifies successful code lookup, malformed paths, wrong method, forbidden bearer credentials and query rejection.

Mechanical comparison against the baseline confirms the original decoder logic and existing tests are preserved apart from import paths, disambiguated test constants and the approved URL change. Cookie selection/zeroization, single code decoding, normalization, validation order, access rules and expiry behavior are unchanged. Private discovery runtime logic, domain services, storage, schema, cryptography, dependencies and `AGENTS.md` are untouched. Historical reviews retain their original paths/results.

## Verification

The fresh complete gate with two Worker shards exits **0**:

- **967 native tests per profile** (debug and release-assertions), **12 doctests**.
- **724 general Workerd cases** plus **20 separate current snapshot-contract cases**; no failed/skipped acceptance cases.
- **374 actual local HTTPS/WSS checks**, including the new legacy-code-route denial.
- **135 Python tooling tests**, strict native/Wasm Clippy, formatting and fresh Worker/CLI builds pass.
- All **298 release-input hashes**, runtime artifact hashes and five independent-review input hashes match the verified candidate. The refreshed code graph verifies **266 source hashes**, without the deleted API discovery file. Structural extraction limitations remain; the separate documentation graph is not refreshed.

The initial focused run passed 38 of 39 cases: the new regression expected `not found` instead of the existing `route not found` message. Only that assertion text was corrected; the targeted case and later full gate passed.

The first full gate failed in Worker shard 2; its cache identifies the unchanged `game_storage_audit.test.js` suite. The runner did not persist the original detailed failure payload, so its cause remains **unconfirmed**. All six storage-audit cases then passed in an isolated diagnostic. The fresh two-shard full gate passed without source, assertion or deadline changes; this is a separate execution, not a relabeled first attempt.

The unchanged superseded receiving-cut probe still returns exit 1 as historical evidence; mandatory current-policy acceptance passes. This does not certify provider deployment or close unrelated slice reviews/external production gates.

[Compact machine-readable evidence](game-reads-route-2026-10-17-evidence.json) binds source/artifact hashes, exact execution-report digests, command descriptors, both full attempts, the isolated diagnostic, mechanical comparison and supplied-source review. The independent review found no blocking defect within its supplied-source scope; it did not independently execute tests.
