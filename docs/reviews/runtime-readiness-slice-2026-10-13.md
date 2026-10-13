# Local runtime readiness foundation — Slice 9, 2026-10-13

## Status and publication

**Local measurement/release-tooling foundation is implemented, exercised and independently reviewed within the stated bounded scope. Whole Slice 9 and production acceptance remain OPEN.** No production source/schema, dependencies/configuration/secrets, frontend, deployment or provisioning changed. Current Slice 9 work is local/unpublished and needs separate commit/push authorization.

The authorized reviewed Slice 8 was committed/pushed as **`8bbb334a689e85eea765d2f06307f94e2d624284`**, `feat(backend): complete game-owned privileged auditing`. Push/fetch/exact remote `main` readback matched; the worktree was clean before Slice 9 began. The original dated [Slice 8 review](game-audit-slice-2026-10-13.md) and evidence remain immutable pre-publication snapshots. Its [publication reconciliation](game-audit-slice-publication-reconciliation.json) documents the sole trailing-blank-line test cleanup, unchanged assertion/production/schema/artifact content, refreshed graph and 32 passing affected regressions.

References: [Slice 9 backlog](../plans/PENDING.md#slice-9--production-databaseruntime-and-release-gates), [LLD 3.22](../plans/lld.md#322-local-runtime-measurement-and-release-tooling-foundation-slice-9), [runner usage](../../backend/tools/README.md), [restore/backout worksheet](../operations/restore-and-backout.md), [public provider reference](../operations/provider-limit-reference-2026-10-13.md), [machine-readable evidence](runtime-readiness-slice-2026-10-13-evidence.json).

## Working artifacts

- `backend/tools/release_gate.py`: provider-neutral local native, Worker, actual HTTPS/WSS and retained contract lanes. Full execution builds the Worker fresh; standalone TLS/contract lanes also rebuild. Exact inputs and four artifacts are fenced against drift. Missing/duplicate/inconsistent results, ignored/filtered/skipped tests, wrong-suite/incomplete/abnormal probe results and timeouts fail closed. Native test identities match across profiles; Worker physical files/cases and TLS check totals reconcile. Dynamic labels are persisted only as SHA-256 identities; raw child output, error payloads and environment/credential values are not automatically logged. Bounded process groups and synchronized cancellation are exercised.
- `backend/tests/local_readiness.rs`: a normal, nonignored exact-source measurement test using production Directory migration/query/service and actual security/recovery-owner logic, plus the included exact-source normalization golden test. Test outputs are safe JSON stdout, not machine-specific file writes.
- `backend/worker-tests/local_readiness.test.js`: real local Workerd enrollment, player recovery and same-player sync for four selected normalization proof sets; clock-window/session-distinctness observations, not mocks or quality certification.
- Restore/backout documentation: quarantine, trusted current time/latest authority evidence, original History/calendar and audit/fixed deadlines, guarded stale-index removal, pending exact coordination and schema-compatible backout/forward fix. It explicitly stops when an old Accounts snapshot could resurrect post-snapshot revocations. No provider restore endpoint, manual live SQL or invented epoch/grant/ACK is added.
- Read-only public Cloudflare documentation refresh: current published limits and the actual SQL cursor `rowsRead`/`rowsWritten` distinction. It is not account-scoped quota/headroom evidence.

No CI provider/workflow, remote infrastructure, hashing-placement/profile/bounds/concurrency change, History query/index optimization or protected-response semantics was selected.

## Actual final integrated execution

Pinned Rust/Cargo **1.99.0**, locked dependencies. Final `release_gate.py` execution is recorded at `/root/.hermes/cache/scratch/brews-slice9-final-run-02`.

| Check | Actual final result |
| --- | --- |
| Complete workspace nextest, debug/all features | **958 passed**, 0 skipped; unique physical identities reconciled |
| Same, optimized `release-assertions` | **958 passed**, 0 skipped; identical native identity inventory |
| Separate all-feature workspace doctests | **12 passed**, no ignored/filtered/failed cases |
| Formatting and strict native/Worker-Wasm Clippy | Passed |
| Fresh optimized Worker and native CLI builds | Passed |
| Complete positive Workerd suite | **712 passed**, **37 physical suites**, no failed/skipped/runtime errors; three complete isolated-cache shards reconciled |
| Actual local gameplay/participants/History HTTPS/WSS | **154 checks passed** |
| Actual operator/transfer HTTPS/WSS | **54 checks passed** |
| Actual dedicated-read/G4 HTTPS/WSS | **106 checks passed** |
| Actual privileged-audit/CLI HTTPS/WSS | **59 checks passed** |
| Combined actual HTTPS/WSS | **373 checks passed** |
| Python runner fault suite | **18 tests passed**; actual child timeout/cancellation/output handling plus clearly synthetic parser/mocked orchestration data |
| Unchanged retained final-return acceptance probe | **1 failed**, 0 passed/skipped: HTTP 200 at the original line-85 denial assertion |

The overall runner exited **2**, correctly preserving **failed acceptance** and `production_release: "blocked"`. That exit is not a positive full release gate or an “expected passing” diagnostic. `RUNTIME-FINAL-01`, Slice 2's older independent review and Slice 6/7 acceptance remain open. Passing owner-local checks do not select authorization-cut semantics or stronger response/mutation coordination.

Source-before/after and all four runtime artifact maps match. Code-only Graphify was incrementally refreshed: **246 source/test hashes** verify against the current corpus and final runner inputs. Structural limitations remain **965 dangling-endpoint edges and 515 collapsed directed edges**; these are extraction/navigation limitations, not runtime findings. The documentation graph remains stale.

## Measurements — observations, not capacity certification

The actual production History SELECT is intercepted from `DirectoryService.history_indexes`; no equivalent copied benchmark SQL is substituted. EXPLAIN binds the same parameters. At each fixed population, unfiltered/resolved/cancelled first/cursor/tail/single pages check an independent exact-ID/order/expiry/lookahead model, including timestamp ties. Fixture mix: nonstarted cancelled, expired before now, exactly at expiry and live terminal metadata, across hosts and pending/published projections. There are **36 samples per native profile**.

| Metadata population | First unfiltered returned rows | Actual SQLite VM steps | Sort operations |
| ---: | ---: | ---: | ---: |
| 100 | 51 | 2,657 | 1 |
| 1,000 | 51 | 18,503 | 1 |
| 10,000 | 51 | 176,003 | 1 |

SQLite **3.50.2** reports the state index and **`USE TEMP B-TREE FOR ORDER BY`** for these plans. FullscanStep is zero, but that does not mean no index traversal. Fixed LIMIT/page/fanout is not fixed SQL work. VM/Sort/Fullscan counters are not provider rowsRead/rowsWritten, billable requests or CPU. Query wall time covers stepping/materialization after preparation; service wall time includes measurement/EXPLAIN overhead and owner transaction/query work, not uninstrumented production latency.

Current explicit Argon2 sample uses `m_cost=19456 KiB`, `t_cost=2`, `p_cost=1`, without changing policy. Final optimized local account sample: **42.245 ms hash**, **42.845 ms matching verification**, **39.893 ms mismatching verification** (rounded wall time). These are single sequential observations, not admitted concurrency, percentiles, peak memory, provider CPU or approval of production work factors.

Native and real local Workerd cover NFC/EM SPACE/NBSP outer whitespace/sharp-s, Greek sigma, non-Turkic I and preserved internal-whitespace distinctions with ephemeral randomized proofs. Native owner routes deny predecessor sessions and accept fresh recovery; Workerd sync retains the same player. Native emitted normalization versions: NFC/Rust whitespace **17.0.0**, case-fold **9.0.0**. Selected-case local passes do not prove exhaustive/deployed Unicode behavior. Distinct issued session samples and current request-window clocks do not certify CSPRNG quality or deployed trusted-time behavior.

Maximum-legal-game resource fixtures and owner peak-event/History forecasts are still absent. No synthetic forecast, extrapolated quota comparison, resource/concurrency cap or provider-measured maximum population is fabricated.

## Independent findings and scope

Initial runner review requested changes on **RG01–RG07**. The failed review and its original 14-test evidence remain unchanged. Repairs distinguish deadline/abnormal exits, require exact contract suite/counts, hash arbitrary labels, rebuild standalone runtime artifacts, compare unique native inventories, reject incomplete doctests and cancel registered Worker command groups on main-thread interruption.

Final read-only follow-up found **no new reproduced blocker within its narrow scope** and accepted those seven repairs. It ran **18 Python tests** and **37 additional bounded parser/process/orchestration checks**, with zero unexpected acceptances. It checked native/Worker measurement code and restore guidance but deliberately did not perform competing service builds/full suites. Parent actual final execution above supplies the fresh integrated service results. Seven inspected-file before/after hash pairs match; protected original reports/probes remained unchanged.

Actual timeout, direct main-thread SIGINT and parallel main-map interrupt/error probes reaped registered children; no reviewer process remains. This is not proof against arbitrary independently detached descendants. Measurement redaction uses closed/static producer fields and forbidden-field/content checks, not a general arbitrary-secret detector. Synthetic unit/mocked report replay is never counted as real native/Worker/TLS service execution.

## Preserved failures and unfinished operations

- Development measurement compile/type adapters, a small filtered cursor-index failure and wrong clock-table read were corrected; original failed attempts/results remain in the measurement report/inventory. Parent's only later native-test change removes host-specific output-file writes; the actual integrated run exercised the final stdout-only source.
- The initial integrated runner invocation stopped at its pinned-Cargo prerequisite **before** formatting/build/test execution. The background Cargo output hash differed from the explicit pinned foreground check. It was rerun with the prescribed existing environment; no tool installation or configuration change occurred. Both attempts retain distinct safe metadata/output locations.
- The original and final independent reviews each preserve command-scoped Git safe-directory lookup failures and corrected readbacks, not global Git configuration changes.
- The final shared-contract assertion still fails on the actual final artifact. Its source/config and earlier Slice 6/7 reports/evidence were not weakened or rewritten.
- Public provider pages were fetched HTTP 200, but actual plan/configured limits and account-level quotas were not read. No headroom or production readiness is inferred.

The original eight Slice 9 production checkboxes stay unchecked: actual deployed KDF memory/CPU/concurrency and runtime behavior; owner forecast/max-game metrics; applicable quotas/two-times headroom; provider physical copies/backups/logs and isolated restore; authorized environment/provisioning/secret/bootstrap procedures; selected CI/release/deployed load/security/observability/backout gates. Restore guidance is a documented worksheet, not a provider-run drill. Current Slice 9 changes remain uncommitted and unpushed.
