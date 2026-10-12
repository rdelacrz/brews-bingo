# Dedicated authorized read slice — blocked local checkpoint, 2026-10-12

## Status and publication boundary

**Slice 6 is implemented locally but is not accepted, committed or pushed. `RUNTIME-FINAL-01` remains open and requires a contract/coordination decision.** Passing tests below are bounded execution evidence, not a final runtime sign-off or production certification.

The preceding reviewed Slice 5 was committed and pushed as `7ffcd7e4cdb5d0dbaf0980635a8f76ea9f1c28b6` (`feat(backend): implement saved-game operators and host transfer`). A fresh `git fetch origin main` confirmed HEAD and `origin/main` at that exact commit. The index remains empty. That publication authorized continuation into local Slice 6, not automatic publication of this unfinished candidate.

[Machine-readable evidence](dedicated-read-slice-2026-10-12-evidence.json) records current source/DDL/artifact hashes, actual result inventories, preserved review hashes and the open failing reproductions. Planning status is reconciled in [PENDING](../plans/PENDING.md), [LLD 3.19](../plans/lld.md#319-local-dedicated-read-api-implementation-checkpoint) and [API choices](../plans/api-design.md#local-slice-6-dedicated-read-transport-choices). All Slice 6 completion boxes remain unchecked. Older Slice 2 review and later Slice 7/remaining Slice 8/production Slice 9 work remain open.

## Implemented scope

Nine dedicated GET routes under `/api/games/{game_id}` reuse existing owner projections:

| Operation | Suffix | Scope |
|---|---|---|
| C5 | none | Selected authorized game detail |
| D1 | `/membership` | Own player/spectator membership; player-only current recovery status |
| D2 | `/players` | Account-authorized roster and actual current presence |
| E1 | `/calls` | Complete ordered safe call ledger and nullable latest call |
| E2 | `/remaining-values` | Complete numerically ordered live pool, count and exhaustion |
| E3 | `/membership/board` | Own player board; explicit null before Start |
| E4 | `/boards` | Account-authorized all/one canonical stable-player-ID board selection |
| E5 | `/qualification` | Account qualifiers or own player qualification |
| F1 | `/result` | Existing original final account/player grants only |

Role/query/header/body selection is closed and explicit invalid proof never falls back. Passive reads do not intentionally enter, allocate grants/sessions, renew timers or advance gameplay/view revisions. Necessary expiry cleanup, durable clock fences and required Admin read audit remain separate permitted writes. Sync/Stream intentional entry is preserved.

The initial implementer's unapproved pool/Leave History retention was removed. Terminal E2 is explicitly 409 after eligible original account-grant authorization, because approved fixed History does not retain the pool bound. Terminal roster Leave is null/unknown; retained denotes the frozen start roster, and connected intersects actual OPEN sockets with current original final bindings. No new History fields/tables or retention extension is introduced. Counter 6 changes only the read-audit purpose vocabulary, with frozen published counter-5 DDL/index templates and fail-closed legacy/optional-Delivery validation.

## Open release finding: RUNTIME-FINAL-01

The actual owner validates current identity/content and returns a fresh, fully bound current-invocation ACK. If a competing mutation commits after that last authoritative cut but before the edge receives the ACK, the edge can still release the originally captured HTTP 200 payload. Fresh nonce binding correctly rejects an older invocation; it does not invalidate the genuine reply for the pending invocation.

The read-only reviewer reproduced five manifestations using genuine response bytes and checked competing mutation/readback:

| Competing change during final return | Actual protected HTTP 200 |
|---|---|
| Player Recovery | Membership delivered under the old selected player session |
| Own recovery-answer DELETE | Old `recovery_enabled:true` after fresh readback is false |
| Own alias PATCH | Old alias after fresh readback shows the new alias |
| Persisted account credential-epoch increment | Protected calls delivered to the old account credential |
| Account Exit from another eligible original session | Final result delivered after all original account/Game grants were removed |

Recovery, DELETE, PATCH and Exit use real public APIs with checked success/readback. Epoch increment is an actual authoritative SQL revocation fault, not a fabricated account-mutation API success. The valid-cookie coexistence control does not authorize fallback. Separate account History remains accessible after final Exit as designed.

After the E5 repair and a fresh Worker build, the parent reran both the original independent probe and its preserved portable version: **six cases, five failing rejection assertions and one passing genuine older-invocation replay control**. These are failures confirming the open blocker, not passing acceptance tests. The preserved probe is byte-identical to the original except for two portable import paths; the evidence generator verifies that reverse transformation. No assertions were weakened.

### Decision needed before acceptance

The [independent architectural check](evidence/dedicated-read-release-remediation.md) inspected the actual interfaces and found no minimal repair for the stronger receipt-time requirement within the currently approved independent Accounts/Game/edge boundaries. The edge has no synchronous access to remote authority/content after its final await. Another fixed RPC, renamed endpoint, refreshed payload, tighter positive TTL or unordered notification merely moves that final gap. Game-only coordination would also leave independent Accounts revocation unordered.

Neither alternative below is approved by this checkpoint:

1. **Explicit authoritative-cut semantics:** allow already-authorized in-flight responses to arrive after a competing mutation, specifying Game snapshot and Accounts-proof cuts separately. Keep current proof, original grants, fresh invocation binding, no cross-request positive cache and receiving-time expiry fences. This must not be described as atomic joint Accounts/Game authorization or receipt-time invalidation.
2. **Retain stronger final-release invalidation:** authorize new coordination ordering relevant mutation commits against pending response release, with an explicit authority owner, delivery/acknowledgement boundary, revocation ordering and timeout/crash/recovery policy. A read lease, mutation lock or account guard cannot be silently introduced as an approved implementation detail.

The architecture report also identifies the same source-level last-hop caveat in published helpers. It does not assert an executed exploit of those helpers, authorize changes to earlier slices, revoke their historical bounded evidence or certify the whole backend. DO-036's already-in-flight WSS-frame caveat is precedent, not approval to change the HTTP contract.

## Repaired Core finding: CORE-READ-01

The original E5 codec accepted duplicate/out-of-order completed lines in account and player responses. The local repair enforces existing line/index bounds and strictly increasing rows, columns, main diagonal, anti-diagonal ordering. Permanent tests reject duplicate, unordered, out-of-range and oversized lists through decode, encode and direct validation; canonical subsets/full maximum and valid empty/unqualified branches remain accepted.

[Independent follow-up](evidence/dedicated-read-core-repair.md) reran the unchanged eight-input reproducer and both new permanent tests against the repaired real source: **three passing cases in each native profile**, 82 explicitly filtered cases per run. `CORE-READ-01` is closed within that follow-up; this does not close the runtime finding.

## Verified current execution

These commands used pinned Rust 1.99.0, locked dependencies and the existing local test environment. No production provisioning/deployment occurred.

| Gate | Current actual result |
|---|---|
| Full workspace nextest, default | **908 passed, zero failed/skipped** |
| Full workspace nextest, `release-assertions` | **908 passed, zero failed/skipped** |
| Separate workspace doctests | **12 passed** |
| Strict native all-targets and Worker/Wasm Clippy | Passed without warnings |
| Formatting and Git whitespace | Passed |
| Fresh Worker build | Passed; four actual artifacts hash-verified |
| Dedicated Workerd suite | **75 passed**, one suite, zero failed/skipped |
| Dedicated actual HTTPS/WSS harness | **96 unique checks**, two real player sockets and one spectator socket; `deployment:none` |
| Separate preserved release-race probe | **Five failed, one passed**; open-blocker evidence |
| Current structural graph/source freeze | **226 code/test hashes and two SQL/DDL inputs match** |

Native commands: `cargo nextest run --workspace --all-features --locked --no-fail-fast`, the same with `--cargo-profile release-assertions`, `cargo test --workspace --all-features --locked --doc`, `cargo clippy --workspace --all-features --all-targets --locked -- -D warnings`, and `cargo clippy -p brews-backend --all-features --locked --target wasm32-unknown-unknown -- -D warnings`.

The first full Workerd/runtime pipeline was deliberately stopped after the blocking reviews, before source repair. Its 13 saved log/freeze inputs remain under the scratch `brews-read-slice-initial-integrated-*` prefix. No completed full-suite JSON exists and **no full integrated runtime pass is claimed**. Earlier writer TLS controls (154 gameplay and 54 operator checks) are historical supporting controls, not rerun/current combined totals. Earlier compile/fixture/Workerd harness failures are preserved in the implementation report and original independent reports; none are substituted for current passes.

Original [Core review](evidence/dedicated-read-core-review.md) records 69 native case executions including two product reproductions and one corrected reviewer-fixture failure. Original [runtime review](evidence/dedicated-read-runtime-review.md) records 70 unique selected cases, 65 passing and five failing, with eleven permanent cases filtered; those review counts are not added to parent totals or represented as whole-suite sign-off. The architectural check ran no new cases.

## Reproduction

From `backend/`, using the existing locally built Worker:

```sh
npm run test:worker -- worker-tests/read_routes.test.js
python3 tests/dedicated_reads_https_wss.py
npm run test:worker -- --config ../docs/reviews/evidence/dedicated-read-release-races.config.mjs
```

The third command is intentionally separate from the passing suite and currently exits 1 with the five open-contract failures. It runs real Workerd, mutations/readback and captured genuine ACKs. It must remain failing under the existing stronger contract until a reviewed implementation or explicit contract change resolves the issue.

## Provenance and limits

The machine evidence includes all 226 frozen code/test hashes, two SQL/DDL hashes, four fresh Worker hashes, actual case inventories and support-artifact hashes. Between original independent review and this repaired candidate, only `shared/contracts/src/games/reads.rs` changed in the code/test corpus. Planning/evidence documents were separately updated by the parent; the Core follow-up explicitly records that external document drift rather than claiming whole-input equality.

The graph is current only for its explicit structural code corpus, not the separate review probes/documentation graph. Its **939 dangling-endpoint and 472 directed collapsed-edge warnings** remain extraction/navigation limitations, not compiler or runtime proof. The documentation graph remains stale.

No frontend, later slice, dependency/configuration upgrade, AGENTS modification, deployment, infrastructure/account setup or production secret provisioning occurred. Local tests do not certify production KDF/Unicode/clock/CSPRNG behavior, provider quotas/load/headroom, backups/restores or release readiness. The implemented artifact remains local and blocked; publication and completion are not inferred from passing tests or reviewer process exit codes.
