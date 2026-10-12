# Local Slice 4 implementation and review checkpoint — 2026-10-12

## Verdict and scope

**Slice 4 is implemented, verified locally and independently reviewed within the documented bounded scopes, with no remaining blocking findings.** This covers spectators and pre-start role switching at baseline `50249d3fd7c7e52326c38dbe3faa4e828a3c2eef`. The candidate remains uncommitted and unpublished. No frontend, deployment, infrastructure/account or secret provisioning, dependency upgrade, later slice or instruction-file change is authorized or claimed.

The implementation follows the approved requirements/HLD/LLD/API/DO policies, notably DO-047/053–055/067–068/073/091–095. Concrete local wire choices are documented in [API notes](../plans/api-design.md#local-slice-4-spectator-and-role-switch-transport-choices); the checkpoint is [LLD 3.17](../plans/lld.md#317-local-spectators-and-pre-start-role-switching-checkpoint). Earlier Slice 2's independent-review checkbox remains open. This does not recertify the entire backend or extend the published baseline audit to unreviewed new code.

## Delivered behavior

- D6 spectator admission in Awaiting Players/In Progress: separate capacity, fixed sessions, digest-only admission contexts, actor-scoped secret-free retries and game-scoped HttpOnly cookies.
- Audience G1/G2/G3 Sync/WSS and H4/H5 presence: public lifecycle/configuration/objective/calls plus minimal own identity, no player roster/alias/board/qualification/recovery/final-fetch authority, and no contribution to the connected-player Start minimum.
- Five-minute accidental-disconnect grace, bounded never-connected occupancy, fixed-expiry reconnect, hibernation reconstruction, exact predecessor fencing and due-aware physical close retries.
- D12 spectator Leave with immediate authority/occupancy deletion and no grace.
- D8/D9 atomic pre-start role switching with fresh target identity, original absolute session expiry, capacity/alias/revalidation rollback, deleted old player recovery proof and optional fresh target-player proof.
- DO-073 terminal commit immediately denies audience authority, then attempts one bounded public notice before checked physical identity/session/context/revision deletion. No application ACK, spectator terminal reconnect/fetch grant or reservation-release dependency is introduced.
- Purpose-named forward migration, strict known historical/current table validation, optional Delivery compatibility and immutable-data preservation.

## Repaired findings

| Boundary | Verified repair |
| --- | --- |
| Terminal ordering | Terminal state/release intent and access denial commit before the genuine notice send; physical audience rows remain through the attempt and are removed afterward, including provider/storage/crash/no-ACK cases. |
| Browser re-entry / Core R1 / runtime R4 | Owner-rejected selected spectator proof on public preflight/admission clears only that game's spectator cookie. Natural retained HttpOnly context/session cookies cannot trap the browser; fresh code admission creates a new identity without account fallback or unrelated-cookie clearing. |
| Runtime R1 | Named output cuts intersect CURRENT session/grace/idle bounds with the ORIGINAL proof cap at each persistence/release await. Exclusive expiry denial durably records the exact observed time even when the clock rolls back during the denial await. |
| Runtime R2 and same-actor replay extension | Positive and negative private ACKs bind the entire typed request and a fresh Edge-generated 32-byte CSPRNG challenge per invocation. Genuine earlier same-actor/same-payload ACKs cannot release a later revoked actor's snapshot/receipt/cookie. Challenge/proof/buffers stay private, zeroized and non-Debug; clock/entropy faults fail closed. Original expiry is fenced before drawing entropy. |
| Runtime R3 | Exact predecessor close/delivery retirement is captured before supersession in the owning checked transaction. Old provider-close failures retain bounded due-aware work without blocking healthy successor 101/snapshot/Sync or terminal commit/Directory release; stale callbacks/ACKs cannot damage the successor or bypass retry due. |
| Core R2 / current-schema F1 | Unknown columns/DDL in recognized v1/v2/v3/current shapes reject before writes. All current Core and optional Delivery tables receive full DDL checks, not selected-column or name-count validation. Known materialized/quoted legacy forms, populated players/boards/History/final grants/close work and new spectator capability are preserved. |
| Schema-validation cost | One bounded catalog read replaces per-table database round trips; unique exact rows, full inventory and full DDL equality remain mandatory. Byte-identical DDL skips only unnecessary identifier normalization. No cached authority or validation across calls/awaits is introduced. |

## Final integrated execution

All final assertions are backed by [machine-readable evidence](spectator-slice-2026-10-12-evidence.json). Counts were parsed and cross-checked against enumerated results, not inferred from process exit or previous runs.

| Gate | Final result |
| --- | --- |
| `cargo nextest run --workspace --all-features --locked --no-fail-fast` | **850 passed**, 0 failed, 0 skipped |
| Same workspace, `--cargo-profile release-assertions` | **850 passed**, 0 failed, 0 skipped |
| `cargo test --workspace --all-features --locked --doc` | **12 passed** |
| Strict native workspace/all-targets and backend wasm32 Clippy | Passed with `--locked -- -D warnings` |
| Rustfmt and Git whitespace checks | Passed |
| Fresh pinned Worker build and complete real Workerd suite | **498 passed in 29 suites**, 0 failed, 0 skipped |
| Fresh CLI and actual local HTTPS/WSS harness | **154 named checks passed**, no cookie/board replay |
| Provenance | **204 Rust/JavaScript/Python source/test hashes**, **1 SQL fixture hash**, **4 Worker artifact hashes** verified |
| Code graph | Current structural hashes match the frozen source corpus; **809 dangling-endpoint and 399 collapsed directed-edge diagnostics** remain navigation limitations, not correctness evidence; documentation graph remains stale |

The final full Workerd run includes the unreduced stress cases: all 50 actual audience sockets survive reconstruction, 200 account anchors enforce the grant bound and permit ending, and all 1,000 sequential manual calls produce full History and a complete public winner notice.

## Independent review and reconciliation

The final Core follow-up personally reran 41 native tests per profile, including seven exact-source scratch probes; no blocking findings remained in that bounded range. The subsequent combined read-only follow-up independently inspected the catalog optimization and unchanged runtime repair boundaries, personally passed 30 focused native tests, 27 real Workerd cases and the unchanged original same-actor replay probe, and reported no remaining blocking findings. These executions are separate evidence, not summed into the integrated totals.

Source/artifact hashes bind the actual inspected bytes. The only test edits after the combined reviewer finished were bounded local stress-test timeout values: spectator tests from 60,000 to 180,000 ms and 240,000 to 600,000 ms; the account-anchor test received a 180,000 ms budget instead of the 30,000 ms default. Loops, requests, assertions, capacity, all production code and server deadlines are unchanged. Reverse-transform hashing verifies the inspected spectator fixture differs only by those budgets; the complete final suite re-executed all assertions. Production/build-support hashes and the four reviewed Worker artifacts match the final candidate. API/LLD checkpoint-status documentation was subsequently updated to record the completed gates and review; those documentation-only changes are separately hashed and do not change the approved policies or reviewed implementation.

Review findings, dispositions, inspected hashes and gate log hashes are preserved in the evidence file. Raw test credentials, private responses and challenges are not copied into this report or evidence.

## Failure provenance and limitations

- Early implementation and follow-up runs, failed fixture edits/builds, incomplete provider/fault-harness runs and stale artifact checks are not passing final gates. Their scratch logs/reports remain available.
- The first integrated Workerd run passed 495/498 cases and timed out on the three stress cases. Catalog batching retained all shape checks; an intermediate native pass initially rejected valid optional Delivery after v1 migration because its comparison used only Core inventory. Supplying the full recognized inventory corrected that regression, and preservation matrices were rerun. Later bounded stress attempts still exceeded narrow runner timers. Only the three stress-test budgets were enlarged; no assertion or production deadline was weakened.
- The first final runtime reviewer stopped with provider capacity and supplied no verdict. A fresh alternate-provider combined review supplies the bounded signoff above.
- Deliberately injected clock/socket/storage faults and resets can emit redacted transport-unavailable/reset diagnostics. Completed assertion reports, not claims of clean stderr, establish the passing gates.
- Actual local Workerd/TLS behavior does not certify deployed provider quotas, peak load/headroom, production KDF/Unicode/runtime costs/support, backups/restore controls or operational readiness. Those remain Slice 9 gates requiring separate authorization where provider-facing actions are involved.

No commit, push, deployment or other unauthorized provider action was performed. Later implementation and publication require separate authorization.
