# Maximum-configured-game resource continuation — 2026-10-13

## Status and scope

**Implemented and verified locally; independent parser/probe follow-up accepts both review repairs with no new blocking finding in that scope. Uncommitted and unpublished.** This is an independent local continuation inside Slice 9/DO-103, not a new Slice 10 or production capacity certificate. The separately authorized readiness foundation was published as `b158f1375d9de0b3308de47da1a5bdd1e9647047`; remote `main` was read back equal after this work. The original [readiness review](runtime-readiness-slice-2026-10-13.md) stays immutable; its [publication reconciliation](runtime-readiness-slice-publication-reconciliation.json) is separate.

Deliverables: real native `game_resources` and local Workerd `game_resources.test.js` workloads; a reproducible, safe [resource probe/interpretation guide](../operations/game-resource-measurements.md); parser regressions and the singular-nextest summary fix in the existing release runner. No production implementation, schema, dependency/configuration, protected-response probe, frontend, provider state, provisioning, secrets, deployment or CI-provider selection changed. This continuation requires separate publication authorization.

## Actual observations

The measured branch uses current maxima: **20 players, 50 spectators, 10×10 boards with no free cells, numeric pool 1,000**. Actual admission, Start and calls create **20 boards/2,000 VALUE cells**; no direct fabricated board/call inserts substitute for service operations. Both workloads end without a winner through the approved confirmed cancellation path, preserve the corresponding terminal History, clean up spectators and release the Directory reservation.

- **Native:** all **1,000 calls** are accepted with current revisions and fresh UUID-v7 commands. There are **70 logical accepted connections, zero sockets**. Deterministic TestRuntime entropy and a fixed command-window clock are fixture choices, not OS/deployed entropy or concurrent traffic certification. Native also retrieves authorized History.
- **Local Workerd:** all **70 admissions**, Start and **one** call go through actual Worker routes. Two players and one spectator have **three real Upgrade/WebSocket pairs**, not network TLS/WSS or a full 70-socket load. The final fresh sample receives **12 frames / 63,006 bytes** (11 snapshots, one terminal notice) and sends **11 ACK frames / 1,848 bytes**. Actual delivery credit settles and all three sockets close. Random-board samples may differ in serialized bytes; this is the final probe's observation, not a stable universal value.

| Game-owner storage cut | Native allocated SQLite page-image bytes | Local Workerd SDK databaseSize bytes |
| --- | ---: | ---: |
| Lobby | 454,656 | 479,232 |
| Started | 679,936 | 704,512 |
| Called | 1,548,288 after 1,000 calls | 704,512 after one call |
| Terminal History/cleanup/release | 3,821,568 | 839,680 |

Native terminal allocation includes **559 freelist pages** within **933 pages at 4,096 bytes**. Allocation is not live payload, process RSS, peak memory or provider charged storage. The native called-stage Game port records **507,430 query calls**, **8,445,292 materialized rows**, **155,876 execute calls** and **170,300 successful-statement changed rows**. Changes use actual SQLite total_changes deltas, including trigger effects/attempted successful statements, not net committed effects. Direct observation SQL bypasses that measured port. These counters are not provider billable rows/requests; wall time is not provider active CPU.

Original attempts to wrap real SDK cursor observations trapped, and page PRAGMAs returned SQLite authorization denials. The final Worker producer installs no SQL observer, emits **explicitly unavailable** cursor work for every owner/stage, and uses actual SDK databaseSize/table cardinalities instead. It does not fabricate zero counters, native-equivalent page measurements or complete SDK operation costs. Full socket/call load, worst-case/error/replay/configuration branches and deployed/provider measurements remain pending.

## Verification and exact repair cuts

| Gate | Actual result |
| --- | --- |
| Frozen full native workspace, debug / release-assertions | **959 / 959 passed**, no skipped cases |
| Separate doctests | **12 passed** |
| Strict native and Wasm Clippy; formatting/whitespace | Passed |
| Fresh full local Workerd inventory | **713 passed across 38 suites**, zero failed/skipped |
| Existing actual local HTTPS/WSS harnesses | **373 checks passed**; not maximum-game network-WSS measurements |
| Retained protected-response acceptance | **Failed**, one assertion, held HTTP **200**, `RUNTIME-FINAL-01`; full runner exit **2** |
| Post-review current Python runner/parser suite | **27 passed** |
| Fresh current-parser resource probe03 | **One native test per profile + one actual Workerd test passed**, probe exit **0** |

The complete full gate is an immutable **pre-parser-repair** cut, not falsely relabeled as a current-parser full run. After it finished, exactly **two of 268 mapped sources** changed: `backend/tools/game_resource_probe.py` and its parser-unit file. All **266 other mapped sources**, including the native/Worker producers, production code, schemas, dependency/settings inputs and release runner, stay identical. All **four Worker artifacts** are identical across full gates01 and final probe03. The repaired parser is separately covered by the current 27-test suite, entirely fresh resource probe03 and independent final follow-up; the whole full gate was not repeated after this parser-only delta. [Machine-readable evidence](game-resource-continuation-2026-10-13-evidence.json) records both cuts, hashes and exact reconciliation.

Initial independent review found GR-REVIEW-01 (empty/truthy/inconsistent socket inventory accepted by the parser) and GR-REVIEW-02 (explicit unavailable SDK observations could be dropped). The successor requires the exact player/player/spectator inventory and lobby cardinality, integer frame/ACK counts and aggregates, positive initial snapshot/byte observations, boolean closure identity, all stage/owner unavailable shapes and the retained observer-trap label. Independent follow-up passed **27 Python tests**, rejected **130 additional bounded in-memory negative probes**, reconciled actual final observations and accepted both repairs with no new reproduced blocker. These additional negatives are unit data, not extra service/runtime checks. Paired **326-file** review inventory is unchanged over that audit; no reviewer-owned children remain.

Original implementation failures/traps, probe01 and the initial review remain preserved. Post-repair probe02 still records **exit 1/GateError** despite all five commands exiting zero: the parent initially expected `pending` instead of the actual producer's `limits_pending`. The field expectation/fixture were corrected without changing the producer, and fresh probe03 passed. That failed report is not rewritten or counted as a passing runtime run. The final reviewer also preserves its own scratch metadata-harness bytes/text mismatch and corrected rerun; neither constitutes a product/capacity result.

Independent [initial review](evidence/game-resource-continuation-2026-10-13/initial-review.md), [accepted final follow-up](evidence/game-resource-continuation-2026-10-13/final-review.md), paired inventories/execution records and the failed probe02 report have [exact-byte durable copies](evidence/game-resource-continuation-2026-10-13/manifest.json). Original scratch files and all their hashes stay unchanged; this local evidence archive is not a provider backup/restore mechanism.

Current code graph verifies **250 source hashes** (4,800 nodes, 14,520 edges, 214 communities). Its **990 dangling-endpoint / 521 collapsed directed edges** remain structural extractor/navigation limitations; the documentation graph remains stale. Existing published review/evidence/probe files stay byte-identical.

## Acceptance still open

All eight original Slice 9 production checkboxes stay unchecked. Missing items include the owner peak event/History forecast, actual target CPU/memory/concurrency and deployed runtime behavior, maximum-game-plus-forecast/provider quotas and two-times headroom, physical copies/logs/expiry-preserving restore, isolated environments/provisioning/secrets and deployed load/security/release gates. No production parameters or weaker KDF policy are selected.

`RUNTIME-FINAL-01`, Slice 6/7 acceptance and Slice 2's older independent review remain open. Positive observations do not approve authoritative-cut semantics or stronger mutation/delivery coordination. Frontend, provider mutations, deployment/provisioning and secret delivery remain unauthorized. No continuation commit/push occurred.
