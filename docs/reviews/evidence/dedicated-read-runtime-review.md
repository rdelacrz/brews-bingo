READONLY independent final LOCAL Slice 6 runtime review — blocking finding remains

Published HEAD: `7ffcd7e4cdb5d0dbaf0980635a8f76ea9f1c28b6`. Reviewed frozen local candidate, not published Slice 6. **RUNTIME-FINAL-01 / P0 is open. This review does not approve the candidate. Parent repair, fresh artifact/freeze reconciliation, independent re-review and the final integrated gate remain required.**

Only scratch test/config/evidence files were written. No project source/test/docs/graph/config/build writes, staging, commit, push, frontend, deployment, provisioning or secret delivery. No Worker build or global/native rebuild was performed. Parent full Worker/native/TLS pipelines were concurrent and are separate evidence. No subagents were launched.

RUNTIME-FINAL-01 — final receiving await releases data after current authority/content changed

The receiving edge authenticates Accounts before each of two release calls, awaits the owner `/read-release`, validates the returned nonce/digest/timestamps, then after the second call serializes the original projection. The post-await checks concern time and the returned ACK; they do not observe the authority/content changes which completed during that last await. A genuine, exactly matching successful ACK for this invocation consequently acts as cached positive authority at the final edge cut.

Exact source locations: [two-cut loop](/home/orangepi/Workspace/brews-bingo/backend/src/worker_runtime/game_reads.rs:307), [pre-await account authorization](/home/orangepi/Workspace/brews-bingo/backend/src/worker_runtime/game_reads.rs:323), [final release await](/home/orangepi/Workspace/brews-bingo/backend/src/worker_runtime/game_reads.rs:373), [post-await ACK/time checks](/home/orangepi/Workspace/brews-bingo/backend/src/worker_runtime/game_reads.rs:383), [captured HTTP 200 projection](/home/orangepi/Workspace/brews-bingo/backend/src/worker_runtime/game_reads.rs:414). Owner `release_read` does compare current identity/assignment and encoded projection, but those checks precede the held final return: [owner content comparison](/home/orangepi/Workspace/brews-bingo/backend/src/db/game/reads.rs:546). Adding another fixed number of the same release calls would only move the last unguarded return; the repair must address the actual receiving cut under the stated contract.

Reproducer: use the scratch config below against the already built, hash-verified Worker. Hook the actual GameObject fetch, call its original provider implementation, and at the second `/read-release` materialize `real.clone().arrayBuffer()` in that request context. Commit the competing mutation, verify its HTTP result and authoritative readback, then return those genuine captured bytes with the original status/headers. No positive ACK, challenge, digest, account proof or service success was fabricated. Response bytes stay in memory and credentials are never printed.

| Personally reproduced schedule | Mutation/readback before captured ACK return | Actual protected reply | Required disposition |
| --- | --- | --- | --- |
| D1 selected old player; valid account cookie coexists; PlayerRecovery | Recovery HTTP 200; successor D1 HTTP 200 with the same stable player ID | Old session D1 HTTP 200 with membership | Deny old selected authority; no fallback |
| D1 same-session recovery DELETE | DELETE HTTP 200; fresh D1 `recovery_enabled:false` | Captured D1 HTTP 200 with `recovery_enabled:true` | Recut or withhold stale private content |
| D1 same-session alias PATCH | PATCH HTTP 200; fresh D1 alias `ReviewFreshAlias` | Captured D1 HTTP 200 alias `ReadOne` | Recut or withhold stale private content |
| E1 account credential epoch revocation | Authoritative Accounts epoch increments from 1 to 2 | Old account credential gets HTTP 200, `calls:[]` | Current account/session authority required even for an empty protected projection |
| F1 account Exit from another eligible original session | Real second login intentionally entered through Sync before terminal; Exit HTTP 200; both original grants removed (2→0); separate History HTTP 200 | Exited first session gets HTTP 200 `result.role:final_host` | No original final grant, so deny F1; History remains separate |

All five captured public replies had no Set-Cookie. That correct cookie behavior does not cure protected data release. Nested readback requests in the first three schedules produced two extra release calls (logged total 4); the mutation hook fires only at the outer response's second release. The epoch and account Exit schedules logged exactly two releases. Epoch alteration is an actual persisted revocation fault, not a fabricated mutation API success; the other four mutations use real public APIs with checked status/readback.

Policies violated: primary implementation override requires owner and receiving edge cuts after every relevant await and current content recut or authoritative rejection; finite T16/T17/T19/T20/T23, A02/A04/A09 and H05–H08. DO-055/056 require current participant session/epoch and no post-recovery reuse; DO-057 requires current account/session authority rather than grant metadata; DO-058 removes all account/game final grants across sessions on Exit. D1's explicit local current-alias/current-recovery contract disallows the shown stale successful replies. This is one common final-return defect with five demonstrated manifestations; it is not five independent root causes.

A separate independent probe captured an earlier genuine initial `/read` ACK for the SAME actor and identical `/calls` request, performed real PlayerRecovery on a later invocation and substituted the old bytes. It passed: HTTP 503, no calls and no cookies; successor readback preserved the same player ID. Fresh invocation challenge binding correctly rejects that older invocation. It does not prevent the matching-current-invocation schedules above.

Personally executed bounded evidence

Permanent command (backend working directory; no build):

```sh
npm run test:worker -- worker-tests/read_routes.test.js -t '^(?!dedicated malformed authenticated Admin)(?!dedicated Admin /(players|calls|qualification|remaining-values) typed audit).*' --reporter=json --outputFile=/root/.hermes/cache/scratch/brews-read-slice-runtime-permanent-results.json
```

75 registered assertions, **64 passed, 0 failed, 11 explicitly filtered/skipped**, one actual Workerd suite. This covers all nine pre-start routes, role/query isolation, passive cross-host read snapshots and intentional Sync entry, actual OPEN/hibernation presence, gameplay/qualification/Leave, both terminal outcomes, original grants/Exit, held initial membership mutations, final Accounts recuts, Admin capture/failure, genuine old ACK replay, negative ACK mismatch, bounded ACK corruption, original idle/History/session expiry fencing, entropy failure, actual shorter spectator grace, maximum 20 admissions/1,000 pool and frozen schema migration controls. It includes the intentionally induced `read denial unavailable` Worker diagnostic from ignored denial storage; stderr is not clean. There is no unhandled-error footer in the completed log, and case counts come from the actual assertion inventory, not a partial harness summary.

Independent command (same backend working directory; existing fresh artifact only):

```sh
npm run test:worker -- --config /root/.hermes/cache/scratch/brews-read-slice-runtime-independent.config.mjs --reporter=default --reporter=json --outputFile=/root/.hermes/cache/scratch/brews-read-slice-runtime-independent-verified-results.json
```

**6 actual cases: 5 failed as the confirmed reproductions above, 1 passed genuine old-ACK replay.** Total unique personally executed cases = **70**, 65 passing and 5 failing. The six independent cases were executed twice: first run also had an invalid expected starting epoch of 0; the authoritative rerun captures the actual starting epoch and verifies +1. All five now fail specifically because protected HTTP 200 was released. Total case executions including that first independent run = 76. An earlier npm invocation from the workspace root exited 254 before any case; its log is preserved and is not execution evidence.

Explicit permanent skips:

- `dedicated Admin /players typed audit success or rejection once`
- `dedicated Admin /calls typed audit success or rejection once`
- `dedicated Admin /remaining-values typed audit success or rejection once`
- `dedicated Admin /qualification typed audit success or rejection once`
- `dedicated malformed authenticated Admin game typed rejection`
- `dedicated malformed authenticated Admin /calls typed rejection`
- `dedicated malformed authenticated Admin /qualification typed rejection`
- `dedicated malformed authenticated Admin /result typed rejection`
- `dedicated malformed authenticated Admin /boards typed rejection`
- `dedicated malformed authenticated Admin /remaining-values typed rejection`
- `dedicated malformed authenticated Admin /players typed rejection`

All seven malformed-Admin-query audit cases and four repeated typed audit-purpose successes were filtered to meet the bounded unique-case budget. Executed Admin game/boards/result cases and failure cases are not a substitute for those eleven skipped assertions. No personal native tests, cargo check, full suite or HTTPS/WSS harness ran. Parent/writer reports were read as supporting provenance, never counted as this review's execution.

Review and coverage limits

Read AGENTS and Rust standards, original request plus primary follow-up overrides, finite R01–03/A01–09/H/T acceptance matrix, implementation report and latest relevant requirements/HLD/LLD/API/DO/research sections. Personally inspected the API decoder, closed read DTOs, passive owner projection/release logic, private read transport, peer/challenge/bounded encoder and actual socket observation helpers, permanent runtime suite and relevant native maximum fixtures. R01 passive projection separation, R02 terminal E2 eligible-original-grant 409, and R03 terminal nullable Leave/current original OPEN presence are represented in the implementation and exercised in the selected permanent cases. No new History pool/Leave metadata is accepted. Approved player maximum is 20; alias maximum is 20 serialized bytes with the existing domain validator, not approval of new Unicode behavior.

Private request/reply boundaries use one projection plus binding rather than two full DTO copies; `ReadRequest` has selection/token/challenge and `ReleaseRequest` adds one projection/binding/fresh challenge/time. Public DTO encoding is bounded at 262,144 bytes; private transport at 278,528 bytes and digest serialization separately at 557,056. This review inspected those bounds and the existing maximum fixtures, but personally exercised the maximum roster only before assignment and the 1,000-value pool, not the combined 20×100 fully qualified boards with 1,000 accepted calls through actual Workerd. That compound maximum, all distinct wrong-game/actor/action/selection/player/receipt/proof-width substitutions, exhaustive role-change branches, every ±1 deadline and every owner sync/Accounts/Directory await schedule were not independently executed here. Existing selected cases cover a subset; no broad matrix completion is claimed. The final-return finding already prevents sign-off.

No whole-backend certification, production KDF/Unicode/SDK/clock/CSPRNG/quota/load/headroom, backup/restore guarantees, provider/account provisioning or release readiness is claimed. Older Slice 2 final review remains pending; Slice 7 command lookup and remaining older-operation/CLI Slice 8 remain pending. Existing published Slice 5 audit 90-day retention and DO-073 terminal commit→bounded notice→physical deletion rules were not reauthorized or changed by this review.

Frozen provenance

Personal before and after hashing matches **all 226 frozen code/test inputs**, both SQL/DDL fixture hashes and **all four fresh Worker artifacts**. The current structural code graph manifest contains exactly 226 source hashes, equal to the frozen source map. Documentation graph remains stale. HEAD is unchanged and index empty. Fresh parent build log was inspected before runtime execution; parent fresh `brews-read-slice-final-artifact-hashes.json` was checked against all four actual files before execution and again after. The writer artifact manifest was supporting historical handoff evidence, not substituted for that fresh parent manifest.

`runtime-inspected-hashes.json` contains clear absolute support-reference/log maps and separate full frozen before/after source/SQL/artifact path→SHA-256 maps. Hashing all 226 inputs is not a claim to have manually read every unrelated source file. Requested final report/evidence live in scratch; no project file was written.

Actual Worker SHA-256:

- `backend/build/index.js`: `1078cf91d1ef5c8ce713eb8ccdd53830fa96d1e16469117a323245f3e64583e6`
- `backend/build/index_bg.wasm`: `436adecd2e1772f463897aab6e21db42639cd0d7655f80192a34e4df31301b8a`
- `backend/build/package.json`: `9eb6bc2af50fe42e73ea9271984927cf9779d6b88c96b38f1326daf3192c8a6e`
- `backend/build/worker/shim.mjs`: `ac99177861405a6046923cbacabbd6adfa1853a56d0c679b9f5e09097c2f4728`

Personally executed evidence SHA-256:

- `/root/.hermes/cache/scratch/brews-read-slice-runtime-permanent.log`: `1a7961cb2b2a8e209d047816f5432f1e3d8b543415d924c6e60f5fdebb9f7ae5`
- `/root/.hermes/cache/scratch/brews-read-slice-runtime-permanent-results.json`: `73156df9fd4b9ea9f5f6fe18c26caa60923d7f2426f793d009ab26a530512770`
- `/root/.hermes/cache/scratch/brews-read-slice-runtime-independent.log`: `767750d04306c04346bd0311472a447f0fdb140a67d838af651c366906dac42f`
- `/root/.hermes/cache/scratch/brews-read-slice-runtime-independent-executed.log`: `adb10082402231e33d2b31df38ce257c64392576e29faf35c034cefab0b0d653`
- `/root/.hermes/cache/scratch/brews-read-slice-runtime-independent-results.json`: `7f07aa4fbd404e3bc260441c1eef39f4b67c6d819a7a1bb8e82a7a5c6848ec03`
- `/root/.hermes/cache/scratch/brews-read-slice-runtime-independent-verified.log`: `efe7662197d53abff592acf4eeda9c58f8b40d137a64056568feb73568ae85a1`
- `/root/.hermes/cache/scratch/brews-read-slice-runtime-independent-verified-results.json`: `a2e8b7384f6e7a1330daa84934b6eb48fcf39cb0d074b7826ba2a58d399c75d4`

Machine-readable evidence SHA-256: `25e54d4794349a918640a9648eb4ebbdf02108e156387500a815e6f7ffb27cbd`.

Status: **blocking final-return finding open; repair and re-review required before final integrated acceptance.**
