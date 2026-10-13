# Slice 7 — local command-result lookup verification

Date: 2026-10-12. Baseline: `7134e6ed9d468afaf261605433740753f3cc7c88`.

**G4 is implemented and verified locally. Final acceptance remains blocked by `RUNTIME-FINAL-01`; this report does not select response-cut semantics, certify production, or authorize publication of Slice 7.** The user-authorized unfinished Slice 6 checkpoint was pushed and read back as `origin/main` at the baseline above. The new Slice 7 source, tests, documentation and graph remain unstaged, uncommitted and unpushed.

## Implemented scope

- `GET /api/games/{game_id}/commands/{command_id}` with exact canonical UUID-v7 identifiers, closed optional role selection, empty body, supplied-Origin validation, and rejection of supplied Idempotency-Key/private or duplicate query parameters.
- Current server-derived original-account or same-game participant authority before receipt/age lookup. Admin cannot override actor ownership. Invalid selected proof cannot fall back; command knowledge, admission context, recovery answer and a lost cookie are not lookup authority. Recovery preserves stable player ownership but invalidates the predecessor session. Role switching, alias reuse, deleted membership, terminal grant expiry and Exit do not inherit or create access.
- Closed version-1 committed/pending/unknown/expired/inadmissible results, bounded to **4096 encoded bytes**, without Set-Cookie, credentials, configuration/boards, internal operation IDs, fingerprints, nonces or hidden pending targets. Unknown does not establish safe mutation retry.
- Committed local receipts retain their original 24-hour completion-based deadline. Matching required work is represented by `follow_up_pending`; prepared work is not committed. Required work older than 24 hours may remain pending without resurrecting an expired receipt. Transfer-abort settlement and System terminal release stay distinct from successful local command effects.
- Exact current original-account Directory creation intent before Game initialization, without a Game record, broad Directory probing or command/effect retry. Ordinary GET performs only permitted clock, expiry and audit maintenance: no sender drain, attempt advancement, ACK clearing, board/call change, presence reconciliation, session/grant renewal or viewer entry.
- Actual Admin attribution with typed `command_lookup` purpose, atomic required capture/readback, fixed 90-day retention and audit-only owner alarm cleanup. Initial capture does not repeat Success during internal recuts.
- Strict purpose-vocabulary migration from the frozen published read schema to storage counter **7**. All known table/index shapes and optional Delivery inventory are validated; unknown drift and ignored/aborted counter writes fail without partial promotion. Earlier golden fixtures remain covered. This counter is storage metadata, not an app release.

Approved contracts remain the API G4/G5 and DO-076–084/088–090 policies. Wire choices are local implementation choices, not new independent approvals. See API design's Local Slice 7 notes and LLD Section 3.20.

## Finding dispositions

| Finding | Disposition and actual evidence |
| --- | --- |
| G4-AUDIT-01 | **Repaired.** An authenticated Admin's uninitialized-owner lookup now captures absent-intent rejection and Directory execution failure using refreshed exact authority and the existing required audit boundary. Original independent probes now observe one correctly attributed rejected/failed row, with 403/503 and no cookie. IGNORE/ABORT regressions still fail closed. |
| G4-FACTS-01 | **Repaired.** Retained transfer target/expected-assignment inputs must reproduce their canonical fingerprint. A present own receipt must agree with Committed phase, destination and checked assignment increment before status/expiry interpretation. Actual committed-transfer corruption now returns redacted 503. Tests also alter both fingerprint copies, proving byte equality alone is insufficient. Original outgoing-host lookup remains permitted without mutation privilege. |
| G4-AUDIT-ALARM-01 | **Repaired.** The parent's actual audit-only owner probe initially read a durable row but `alarm:null`. Initial Admin creation capture now arms the existing earliest owner cleanup deadline without changing a prior earlier alarm. A truly empty-domain owner with no routing/work/transfers/sockets completes normal audit/Delivery cleanup without broadcasting a nonexistent view. Actual equality alarm physically purges rows and clears the alarm; rollback throws the existing clock denial while rows and the persisted observed-time fence remain unchanged. |
| RUNTIME-FINAL-01 | **OPEN.** Actual public recovery revokes the old session during return of a genuine current-invocation final ACK. A fresh old-session request is 401, but the held G4 request returns 200 with a committed receipt and no cookie. Fresh challenge/full-request/result/cap binding rejects older ACK substitution; it does not establish receipt-time revocation safety. |

The unchanged stronger-contract acceptance assertion is retained in [the portable probe](evidence/command-lookup-final-return.test.js), separately from the passing regression suite. From `backend/`:

```sh
npx vitest run --config ../docs/reviews/evidence/command-lookup-final-return.config.mjs
```

Its required-denial assertion **fails**, as independently reproduced on the final artifact. Neither authoritative-cut semantics nor stronger mutation/delivery coordination has been selected. No lease, release lock or new account guard is silently introduced. The original Slice 6 probe/report/evidence remain unchanged; earlier independently executed five failures and one passing older-ACK control are not relabeled as acceptance.

## Final integrated positive gates

All final gates ran on the frozen candidate with pinned Rust 1.99.0 and locked dependencies. Counts are parsed from actual execution, not inferred from declarations or selected test lists.

| Gate | Actual final result |
| --- | --- |
| `cargo fmt --all --check`; `git diff --check` | Passed |
| Workspace/all-features nextest, debug | **925 passed, 0 failed, 0 skipped** |
| Workspace/all-features nextest, release-assertions | **925 passed, 0 failed, 0 skipped** |
| Workspace/all-features doctests | **12 passed** |
| Strict native workspace/all-targets Clippy | Passed |
| Strict backend Wasm Clippy | Passed |
| Fresh optimized local release Worker build | Passed |
| Complete Workerd positive regression suite | **656 passed, 0 failed, 0 skipped, 34 suites** |
| Actual local gameplay HTTPS/WSS | **154 checks passed** |
| Actual local operator HTTPS/WSS | **54 checks passed** |
| Actual local dedicated-read/G4 HTTPS/WSS | **106 checks passed**, including 10 specifically enumerated G4 checks, two player sockets and one spectator socket |
| Combined actual HTTPS/WSS | **314 checks passed; no deployment** |
| Source/schema/artifact freeze | **234 source/test SHA-256s, 3 schema-fixture SHA-256s and 4 Worker artifact SHA-256s verified** |

The stronger-contract failure is deliberately **not** part of a claimed zero-failure acceptance result. Passing positive gates do not close that finding.

## Independent review and execution limits

The original independent review reproduced the audit and contradictory-transfer findings against an immutable candidate. Its actual selected native cases passed (27 per profile); its extra runtime failures were the finding manifestations and shared races, not sign-off. A bounded repair follow-up independently passed both original reproductions, the 36 permanent lookup cases and 19 selected native cases per profile while retaining the shared failing assertion.

The final audit-alarm follow-up inspected the changed runtime sources and unchanged transfer-validation core. Before/after verification matched every **234 source / 3 fixture / 4 artifact** input, with no source/artifact changes. It personally executed **49 Workerd case executions: 44 passes, one shared-contract failure, and four preserved scratch setup/control failures; zero skipped**. This includes all 37 permanent lookup/repair cases, the four original unchanged probes, the original alarm probe, and the unchanged portable final-return assertion. These are extra bounded executions, not additional unique cases in the complete 656-case suite.

Its optional scratch schedule proves actual pending-creation lookup, one initial Success per GET despite recuts, preservation of an earlier native alarm, unchanged owner/Directory state except permitted clock/audit maintenance, and 503/no result/no cookie when actual scheduler invocation is faulted. A subsequent manual rearm in that fault fixture is expressly **not** evidence of automatic scheduling success; automatic scheduling is separately proved by the permanent/original alarm cases. Actual equality purge and rollback clock readback pass.

The four scratch failures were a consumed-request clone and genuine concurrent Directory creation recovery confounding the control snapshots. All failed runs were preserved. The isolated rerun removed only the scratch Directory alarm; durable intent/retry metadata and the full snapshot assertions remained unchanged. They are not new product findings, passes, or evidence of GET retrying effects.

The final bounded conclusion is: **G4-AUDIT-01, G4-FACTS-01 and G4-AUDIT-ALARM-01 repaired; no new blocking findings beyond the explicitly open shared contract dependency.** This is not exhaustive review of all awaits, corruption combinations, socket/work combinations, provider capabilities or production quotas.

## Failure provenance and retained evidence

- The first full native run was **923/924**, not a pass. Implementation had accidentally replaced the unrelated expected recovery-bucket count **6** with the schema-version constant. The original count predicate was restored; all subsequent full final native gates pass. No schema assertion was weakened.
- Full runtime attempts interrupted for newly identified repairs were preserved and are not reported as acceptance passes. The fresh final run completed all 656 positive Worker cases and all three real TLS harnesses.
- [Machine-readable evidence](command-lookup-slice-2026-10-12-evidence.json) contains full frozen source/schema/artifact hashes, complete positive Worker case inventory, final review execution inventory/hash checks, portable failure-probe hashes, protected baseline-input hashes and scratch log/report hashes.
- Scratch reports are `brews-command-lookup-{review,repair-review,final-followup}.md` and their hash receipts. Final logs use `brews-command-lookup-final-*`. Exact run/filter/results and raw retained failures are hash-linked in the evidence; the portable failing probe and permanent tests remain in the repository.

## Graph and remaining scope

`graphify-out/code/` was incrementally refreshed after cohesive source/test changes and all **234** source hashes match. Structural diagnostics retain **893 dangling-endpoint** and **473 collapsed directed same-endpoint** edges; navigation may be incomplete. These are extractor limitations, not runtime test failures. No semantic documentation extraction was refreshed.

Slice 7 remains local/unpublished and final acceptance remains open. Slice 6 stays blocked despite its separately authorized checkpoint publication. Slice 2's older independent-final-review checkbox remains open. Remaining Slice 8 audit/CLI integration, Slice 9 production release/runtime/quota/KDF/Unicode certification, frontend, exports, deployment, infrastructure/accounts and secret provisioning are not completed or authorized by this work.
