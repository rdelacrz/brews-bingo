# Backend Rust audit — 2026-10-12

Baseline: `c5ebed6a561330fd1ac5447cd1a1365e98d31d78`.
Scope: local backend source, its shared contracts/domain dependencies, and native/Worker regression coverage. The user authorized auditing, simplifying and repairing existing behavior, not new feature slices, publication or deployment. This audit does not retroactively close the separate pending Slice 2 review.

## Status

Completed locally. All confirmed findings below are repaired, final integrated gates pass, and bounded independent follow-up reviews have no remaining blocking findings in their reviewed scope. The verified corpus contains 199 source/test files; applicable independent-review hashes match 45 inspected production files. Changes remain uncommitted and unpushed. No deployment was performed.

## Confirmed findings and repair scope

| Area | Reproduced defect | Implemented repair |
| --- | --- | --- |
| Worker SQLite bounds | The adapter's 512-row cap rejects valid 1,000-call games and terminal snapshots with up to 2,000 board cells. | Derive a bounded adapter cap from supported domain sizes; retain per-query/domain validation. |
| Accounts migration | Ignored or corrupted schema-marker writes can report successful initialization and commit partial/inconsistent migration state. | Verify stored markers inside the migration transaction and roll back on failure. |
| Account expiry | A later authorization sample can deny expiry without advancing the earlier transaction clock fence; rollback reauthorizes the same cookie. | Persist and verify the actual decision sample, including business denials. |
| Link/receipt decision samples (CORE-01) | Redemption and login receipt checks can sample expiry after the persisted transaction time, then accept a rollback retry. | Observe the exact decision sample for link/receipt/rate/cleanup cuts; final management reads stay write-free on success and fence denials atomically. |
| Completion retry throttling | Retired restricted-cookie password proof bypasses ordinary login failure buckets. | Use the existing caller/account login buckets, preserving receipt-only retry and no cookie replay. |
| Completion retry expiry | Password proof can finish after the original restricted session expires, yet still return its receipt. | Recheck original session identity, scope, epoch, revocation and fixed expiry after proof. |
| Auth output | Current-account output can outlive its session during owner durability or return transport. | Validate protected output after awaits; retain and fence the original observed expiry. |
| Users output | A committed account-creation reply can disclose an enrollment URL after the administrator expires in transit. | Bind private output to original administrator authority/deadline and deny expired release without replaying the mutation. |
| Game Sync output | A valid owner snapshot can arrive after player/account authority expires. | Carry and enforce the intersected session/idle/final-History deadline at the receiving cut. |
| Player Sync revocation (FINAL-RUNTIME-01) | A delayed player Sync reply can disclose its captured snapshot after Recovery or terminal Exit revokes access, before its original expiry. | Add an original-identity-bound private release check without replaying mutations or extending the deadline. |
| General Accounts peer | Receiving-time expiry denial discards its clock observation, allowing a rollback retry. | Reuse the exact-bound durable Accounts expiry-denial protocol. |
| Private auth grammar | Serde accepts positional array roots despite the private object-only protocol. | Reuse the object-root decoder; preserve duplicate/unknown-field rejection. |
| Gameplay final cut | Final-response expiry denial does not persist the Game clock observation. | Keep successful validation read-only; durably fence expiry denial with verified writes. |
| Viewer grant capacity | Accepted HTTP-only account anchors can exceed terminal pruning bounds and prevent winner/cancellation commit. | Align bounded admission and pruning; preserve accepted sessions and reject new overflow atomically. |
| Recovery predecessor closure | Recovery captures current connections but omits potentially accepted prepared predecessors. | Share bounded current/prepared close capture with Leave and preserve exact acknowledgement/successor fencing. |
| Close retry timing | Pending closure work is retried before `next_attempt_at`, defeating stored backoff. | Separate due dispatch from all-pending inspection and reject early/stale retry advancement. |
| Socket callback retry bypass (FINAL-RUNTIME-02) | Predecessor messages and provider errors reach generic retirement and retry physical close before the durable deadline. | Route known-fenced callback retirement through the exact due-aware dispatcher; retain early exact closed/absent acknowledgement and healthy successor delivery. |

The authentication, transport and game/storage findings were reproduced with native SQLite, exact-source control-flow probes, or actual local Workerd execution. Reproduction tests deliberately asserted faulty baseline behavior; they are distinct from the permanent regressions requiring corrected behavior. Trusted-clock rollback and provider/storage fault cases are explicit test preconditions, not claims of an unauthenticated account-takeover exploit.

## Organization and simplification

- Extract cohesive `db/game/{migration,gameplay,resolution}.rs` modules, with owning SQL and relevant bottom unit tests. Keep modules private and preserve existing service/migration entry points.
- The extraction's token comparison preserves all 139 original function bodies, 188 constants and seven structs; all 34 public function signatures are unchanged. Allowed differences are module/type paths, parent-only visibility and fixture-relative paths, plus a thin calendar-helper forwarding wrapper. Subsequent bug fixes are separate semantic changes.
- Replace production wildcard imports with explicit dependencies.
- Parse selected cookies directly through one bounded helper instead of masking/rebuilding headers and strings. Preserve selected-name duplicate rejection, case-sensitive selection, ignored unselected cookies and original header limits.
- Share management pagination constants across parsing/storage and remove redundant audit-tag conversion where touched.
- Bound private auth response reads before buffering the whole body.

## Final verification

All final gates ran against the same frozen source/test hashes and freshly built Worker artifact. The results below are complete runs, not sums of filtered test selections.

| Gate | Verified result |
| --- | --- |
| `cargo fmt --all --check`; `git diff --check` | Passed |
| `cargo nextest run --workspace --all-features --locked --no-fail-fast` | 803 passed, 0 skipped |
| Same nextest command with `--cargo-profile release-assertions` | 803 passed, 0 skipped |
| `cargo test --workspace --all-features --locked --doc` | 12 passed |
| Strict workspace/all-target native Clippy and backend `wasm32-unknown-unknown` Clippy, both with `-D warnings` | Passed |
| `worker-build --release`, followed by the complete Worker suite | 400 passed across 28 suites; no failed or pending tests |
| Fresh CLI build and `python3 backend/tests/gameplay_https_wss.py` | 125 actual local HTTPS/WSS checks passed; no board/cookie replay; no deployment |
| Source/test, applicable review and Worker artifact hashes | 199 source/test files, 45 inspected production files and 4 Worker artifacts verified |

Permanent regressions include migration marker faults; supported maximum calls/boards; actual decision-time expiry fencing, clock rollback and failed writes; retired completion throttling; owner/receiving authorization cuts; delayed player Sync after real Recovery/terminal Exit; terminal access pruning; bounded grant admission; accepted prepared predecessor closure; and valid/malformed/binary message plus error callbacks before close retry due time. Healthy successor delivery and healthy live/final Sync controls also pass.

### Independent review

The core follow-up independently executed 55 focused native tests/probes and found CORE-01 repaired with no remaining blocking core finding. The initial runtime review identified FINAL-RUNTIME-01/02 after the earlier fixes. Its final follow-up inspected both repairs, executed three focused native checks and reviewed the real Worker regression evidence, finding no remaining blocking issue in that bounded scope. Reviewed hashes were reconciled with the final corpus; these reviews do not claim exhaustive coverage of every line in the listed files or close the older separate Slice 2 gate.

### Evidence and intermediate failures

Machine-readable final counts, source/artifact hashes and reconciled review hashes are retained in [`backend-audit-2026-10-12-evidence.json`](backend-audit-2026-10-12-evidence.json). Raw execution/reviewer logs were recorded under the session scratch directory with the `brews-audit-` prefix; scratch retention is temporary.

Earlier baseline/focused passes were not treated as final sign-off. Intermediate failures included reproduced defects, concurrent-edit compilation failures, a background toolchain-selection error, a new regression expecting 401 instead of the existing `invalid_link` 400 contract, and a delayed-response test fixture crossing Workerd I/O contexts. The first version of that fixture caused a provider runtime crash and an incomplete JSON report despite its top-level success flag; it was not counted as passing evidence. The corrected fixture keeps captured bytes and real intervening Recovery/Exit inside the owner I/O context. The final complete Worker run exits successfully and all enumerated assertions pass.

## Limits

This is a bounded source review and local execution audit, not proof of bug absence or production readiness. A receiving release check establishes an owner authorization cut; it cannot guarantee that no concurrent revocation occurs afterward. This audit does not certify production KDF costs, provider quotas/load, backup/restore, deployed infrastructure or actual secret provisioning. Dependency upgrades, frontend, later feature slices and deployment are outside scope.

The code-only structural graph is refreshed and verified against 199 source/test hashes. Its extractor reports 775 dangling-endpoint edges and 368 collapsed directed same-endpoint edges; source currency does not certify graph completeness. These graph diagnostics are not Rust compile or runtime failures. The older documentation graph remains stale.
