# Local saved-game operator Slice 5 review — 2026-10-12

## Result and scope

Authorized **local Slice 5 is complete** from published baseline `8fbf303c7f151230361f536b6803996aada3aad6`: C7 New-only configuration editing, C9 intentional resume, C10 nonterminal host transfer and bounded safe eligible-target discovery, with necessary DO-088–090 privileged capture. Final integrated gates passed against the frozen candidate. Independent bounded Core/runtime reviews and the final repair follow-up have no remaining blocking findings in their documented scopes.

This is local acceptance, not whole-backend or production certification. No staging, commit/push, frontend, deployment, infrastructure/account/secret provisioning, dependency upgrade or instruction-file change was performed. Older Slice 2 independent review and remaining Slice 8 existing-operation auditing/CLI reads remain pending. Further implementation and publication require separate authorization.

See [API choices](../plans/api-design.md#local-slice-5-operator-transport-and-coordination-choices), LLD Section 3.18 and [machine-readable evidence](operator-slice-2026-10-12-evidence.json).

## Delivered behavior

- **C7:** closed partial PATCH with required expected revision and UUID-v7 command header. Omission preserves values, null rejects, positions replace the complete list, and the effective configuration validates atomically in New. Equivalent no-ops retain a bounded receipt without revision/activity changes. Creator and creation identity are unchanged.
- **C9:** intentional open restores the existing permitted account projection and current operator capability. It creates no game, code, board, participant session or fixed-expiry extension. Only qualifying current designated-host pre-start activity renews the deadline. Passive, read-only, nondesignated Admin and duplicate attempts do not renew; terminal games cannot reopen.
- **C10:** target-specific confirmation and expected assignment revision; fresh current actor/recipient authority and Directory gates precede the Game CAS. The target is a distinct enabled Verified Host, not an Admin. Revisions, receipt and privileged success commit once. Outgoing actors may retrieve their original bounded receipt under current account authority but lose fresh mutation authority at commit.
- **Discovery:** account-ID/username-only, cursor-ordered, bounded limit+1 scan. Current host, ineligible accounts and assignment-gated targets are excluded. Filtering does not enlarge the scan; the page is advisory, not authority, and changes neither idle nor session deadlines.
- **Coordination/storage:** durable Game/Directory assignment intent, conservative removal guards, exact immutable proofs, capped retry backoff, reconstruction and completion acknowledgement. Global reservation and published code remain intact. Prepared work has no TTL, and unconfirmed settled evidence survives ordinary client-receipt expiry. Game counter 5 and Directory counter 3 migrations validate exact current/historical tables, DDL and indexes before writes; fixed historical templates are purpose-named and checked against the published baseline.
- **Audit:** actual authenticated Admin actor, Game target and typed purpose/outcome only. Mutation/receipt/success capture is atomic; inability to create required audit rolls back mutation or withholds a read. Capturable execution failures use fresh authority and separate Failed metadata without duplicating Success. Authenticated malformed/oversized operator transport captures minimal Rejected metadata; anonymous/bad-cookie attempts create no authenticated record. Fixed 90-day expiry and durable rollback fences are independent of game/History deletion. No submitted configuration, snapshots, answers, credentials or fingerprint payload enters audit rows.

These behaviors preserve the published audit and spectator release/terminal-ordering baseline. Cross-owner coordination is **not** claimed atomic.

## Findings repaired and independently rechecked

| Finding | Repair and verified boundary |
| --- | --- |
| CORE-01 | Lobby/code and transfer intents exclude each other at preparation, retry and commit. Exact earlier hidden creation/code association can reconcile safely; cancellation/timeout and later reservations cannot be stranded or replaced by delayed old work. Both intent orders and reconstructed genuine overlap are covered. |
| CORE-02 | Settled Directory evidence is not removed merely because 24-hour client retention passes. Game durably consumes the exact ACK before Directory completion starts bounded metadata retention. Game- and Directory-driven lost-ACK recovery, fresh successor transfer and late old ACK denial pass. |
| OPERATOR-TARGET-CUT-01 | Final target release couples a fresh Accounts actor/candidate cut with Directory's synchronous receiving gate check, then Game's current identity/assignment/original-cap check without another peer authorization await. Actual gate/disable/delete/reset/unverified transitions, gate after genuine candidate bytes, actor epoch revocation and expiry/rollback probes withhold protected targets. Permanent hooks now explicitly scope the second/final coupled call. |
| OPERATOR-AUDIT-RELEASE-01 | A genuine successful privileged read followed by lost owner-release output records a separate durably capturable Failed outcome. Ignore/abort/entropy capture failures remain unavailable and do not claim nonexistent audit; committed mutation effects are not replayed or erased. |
| OPERATOR-AUDIT-BODY-01 | All three mutation routes audit authenticated oversized Admin rejection using bounded metadata, with Host/anonymous zero-audit and required-write/entropy fail-closed controls. |
| CORE-FOLLOWUP-01 | A prepare never delivered to Directory can retire through its genuine current aborted proof after the durable admission floor. Directory records exact aborted evidence without changing assignment/reservation; delayed prepare still rejects after compaction. Future operations and missing committed evidence still reject. Required evidence IGNORE/ABORT produces Storage, not an ACK. The original failing assertion passes unchanged in both profiles; real Workerd recovers after 24 hours and eviction without a viewer credential, then permits a fresh transfer. |

The earlier parent OPERATOR-RELEASE-01 authentic SAME-ACTOR old-ACK disclosure is also repaired. Full request binding and a fresh per-invocation challenge reject genuine prior ACK substitution after an actual host transfer. Positive and negative decisions match their invocation; already observed original expiry is fenced before entropy is required. The original genuine-ACK control passes again on the final artifact.

## Final integrated gates

Pinned Rust **1.99.0**, locked Rust dependencies, existing provider/package lockfiles and scratch TMPDIR were used.

| Gate | Actual result |
| --- | --- |
| `cargo fmt --all --check`; `git diff --check` | Passed |
| `cargo nextest run --workspace --all-features --locked` | **882 passed, 0 failed, 0 skipped** |
| Same nextest with `--cargo-profile release-assertions` | **882 passed, 0 failed, 0 skipped** |
| `cargo test --workspace --all-features --locked --doc` | **12 passed** |
| Strict workspace/all-target native Clippy and Worker/Wasm Clippy | Passed with warnings denied |
| Fresh `npm run build` and complete `npm run test:worker` | **544 passed across 31 suites, no failures or pending tests** |
| Actual locally trusted gameplay HTTPS/WSS harness | **154 checks passed** |
| Actual locally trusted operator HTTPS/WSS harness | **54 checks passed**, including real account, Admin, player and spectator sockets |
| Combined actual local HTTPS/WSS | **208 checks passed**, no deployment |

The final corpus has **217 Rust/JavaScript/Python source/test hashes, one SQL fixture hash and four Worker artifact hashes**. The code graph matches all 217 current sources. Graph extraction retains 881 dangling-endpoint and 460 collapsed same-endpoint diagnostics; it is navigation evidence, not runtime correctness. The older documentation graph remains stale.

## Independent review and source reconciliation

The first independent Core/runtime reviews identified five blockers, not approvals inferred from subprocess exit status. After repair, Core follow-up reproduced CORE-FOLLOWUP-01; the runtime follow-up found no remaining runtime blocker, with an explicitly recorded interception-precision issue and invalid fixture. The final independent follow-up verified the narrow recovery repair and corrected tests.

Final personally executed review selections: **45 permanent native tests plus nine scratch probes in each profile**, all passing, and **40 actual Workerd cases** (30 permanent repairs, one unchanged genuine ACK probe and nine valid unchanged last-stage probes). Filtered/skipped cases are not counted as passes. Prior historical-schema review ran 180 real SQLite drift/partial-Delivery subcases; final follow-up verified their evidence and unchanged schema sources, without counting them as rerun tests.

All **129 other production Rust modules** match the preceding reviews; the sole later production change is the Directory aborted-evidence recovery branch. The native/Worker test deltas are recorded separately. Final review and integrated gates match the complete final source, SQL and artifact manifests. The Wasm intentionally differs from the preceding runtime review because of that repair; it was freshly built, independently exercised and reconciled, not asserted identical. No code/test changes followed the final review. Later documentation status updates are hashed separately in the evidence.

Secret-free review/report, inspected-source, original probe, excluded run and final log hashes are retained in the evidence JSON. Detailed raw logs and scratch probes are local scratch evidence, not a committed credential/reply archive.

## Failure provenance and limits

- The first integrated Worker run failed; it is preserved as failed evidence, not relabeled passing. The pre-floor-repair full runtime run was interrupted when the additional Core blocker appeared and is not acceptance evidence.
- A prior `actor_disable` scratch fixture tried to disable the designated Host; removal correctly conflicted. Its failed expected-denial assertion is retained as invalid/inconclusive and excluded, not presented as a successful mutation or stale disclosure. Successful nondesignated-Admin disable at that exact final stage was not independently exercised.
- Final reviewer initially reused wrong scratch-workspace native binaries. Those selections omitted the new regressions and are explicitly excluded. Accepted selections followed per-profile cleaning/rebuilding, enumerate the current tests and include both new regressions.
- Owner execution failure capture is verified where fresh authority and storage allow it. Separate Failed capture for transport-only Game-to-edge ACK uncertainty is not claimed; uncertainty cannot disclose data or justify a guessed owner outcome.
- Full local gates do not certify every deployed race, production Argon2/KDF and Unicode/runtime behavior, provider quotas/two-times headroom, load, backup/restore, deployment wiring or production readiness.

All Slice 5 checkboxes can close locally. Slice 2's older review and later slices remain unchanged except for explicitly noting Slice 5's partial audit contribution to Slice 8.
