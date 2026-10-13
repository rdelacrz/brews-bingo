# Local Game-owned privileged audit slice review — 2026-10-13

## Status and scope

**Slice 8 is implemented and verified locally, with no remaining blocking finding in the bounded independent audit follow-ups.** This is not acceptance of the shared protected-response contract, full backend signoff, provider certification or publication authorization.

The previous user-authorized unfinished Slice 7 checkpoint was committed and pushed as `61b4e657af6c89491824a33b718c8f2fa38664b1` (`wip(backend): checkpoint command lookup with open release-contract finding`); HEAD and `origin/main` were independently read back equal. Current Slice 8 changes are local, unstaged, uncommitted and unpushed. Frontend, infrastructure/accounts/secrets, production deployment and Slice 9 remain outside this authorization.

Planning references: [API local choices](../plans/api-design.md#local-slice-8-game-owned-privileged-audit-implementation-choices), [LLD Section 3.21](../plans/lld.md#321-local-game-owned-privileged-auditing-slice-8), [DO mapping](../plans/durable-object-design.md), [pending ledger](../plans/PENDING.md). Actual gate/source/schema/artifact maps, full integrated Worker case inventory and combined review provenance are in [machine-readable evidence](game-audit-slice-2026-10-13-evidence.json).

## Implemented behavior

- Extends existing Game-owned privileged storage/capture across older game creation, lobby/Start, random/manual calls, winner/cancellation/no-winner ending and Exit, account Sync/actual Stream establishment, and Game-backed discovery/History reads. Attribution is the actual verified Admin, not the designated host. Closed metadata excludes submitted content, credentials, answers, snapshots and fingerprints.
- Fresh accepted owner effects, actor receipts and required Success are atomic. Committed command replay does not manufacture another successful action. Authenticated semantic rejection is distinguished from capturable execution failure, including randomness/generation exhaustion. Unauthenticated, ordinary Host, selected player/spectator/entry and System/frame traffic do not become Admin actions. Initial, owner and receiving-edge failure cuts refresh attribution without retrying original effects or inventing a missing Game target.
- Stream preparation is not Success: capture is tied to actual native acceptance and durable current binding, with exact compensation for SDK-before/after acceptance and ignored/aborted successor writes. Frame/ACK traffic does not append action records. Verified owner operation Success is not a claim that protected content reached a browser or that remote authority remained valid after the final hop.
- Adds a typed, seven-field, exact-game developer audit page behind the existing restricted bearer-only HTTPS management interface. Browser cookies/claims grant no audit authority. The CLI uses `brews [--json] audit game GAME_ID [--after AUDIT_ID] [--limit N]`; limit and cursor are validated locally, responses are closed/secret-free, and raw private error chains are not exposed. The page precedes its own DeveloperCli read record.
- Fixed expiry is the original occurrence plus 90 days, independent of Game, History, account deletion and purge progress. Receiving-clock validation and durable sampled-expiry fences prevent disclosure at equality/rollback, including empty pages and audit-only reconstruction. Reads/restore do not renew activity, grants or expiry.
- Purpose-named current unpublished storage counter 8 migration preserves exact published counter-7 templates, legitimate prior actor values, outcomes and deadlines. Whole owner/optional Delivery DDL/index inventories are validated before atomic promotion; drift, unknown indexes, malformed actors and ignored/aborted required writes roll back. Indexed target/audit-ID seeks use a fixed 256-candidate plus sentinel budget, bounded cleanup and exact lookahead; uncertain completion returns unavailable, not a fabricated final page. This is bounded page/cleanup work, not a claim that the full migration copy is bounded.

## Final frozen integrated gates

Pinned Rust `1.99.0` and locked dependencies; no provider/deployment mutation.

| Gate | Actual final result |
| --- | --- |
| `cargo nextest run --workspace --all-features --locked` | 956 passed, 0 skipped |
| Same with `--cargo-profile release-assertions` | 956 passed, 0 skipped |
| Workspace all-feature doctests | 12 passed |
| Strict native and Worker/Wasm Clippy | Passed with `-D warnings` |
| `cargo fmt --all --check`; whitespace check | Passed |
| Fresh optimized Worker build | Passed; 4 actual artifact hashes verified |
| Complete positive Workerd suite | 711 passed, 36 suites, 0 failed/skipped; all expected files/cases reconciled |
| Real local HTTPS/WSS gameplay/participants/History | 154 checks passed |
| Real local HTTPS/WSS operators/transfer | 54 checks passed |
| Real local HTTPS/WSS dedicated reads/G4 | 106 checks passed |
| Real local HTTPS/WSS privileged auditing/CLI | 59 checks passed |
| Combined actual HTTPS/WSS | 373 checks passed |
| Frozen source/schema and code graph | 242 source/test hashes, 4 schema/fixture hashes match |

The complete Workerd suite ran in three full, independently cached shards; the merger rejects omissions, duplicate files/cases, skipped/pending cases, errors and mismatched counts. This changes scheduling only, not assertions. Parent native and runtime runners both exited 0. Individual case names and source/artifact SHA-256s are retained in the evidence JSON. Actual fresh Wasm loaded by independent review is `3dcb5b64076e96f82cd5f3a929b04512278dbc42736026aa1cc29d8318805359`.

Code-only Graphify was incrementally refreshed after cohesive source edits and independently checked before/after: 4,642 nodes, 14,176 edges, 198 communities. Extractor diagnostics remain: 935 dangling-endpoint edges and 508 collapsed directed same-endpoint edges. These are structural-extraction limitations, not Rust test failures. The older documentation graph is stale.

## Independent findings and final dispositions

Initial independent Core/runtime reviews requested changes. Their nine finding groups were reproduced and repaired; the subsequent full bounded follow-up verified those groups but discovered a new no-fault public Stream replacement failure. It correctly withheld signoff despite passing parent positive gates. A final narrow independent review closes that new defect on the new frozen artifact; it does not pretend to have rerun the entire earlier scope.

| Finding | Final scoped disposition |
| --- | --- |
| CORE-S8-01 | Required creation-identity lookup remains captured inside the transaction; explicit known targets avoid only optional eager lookup. Genuine original SELECT failure yields one actual Admin Failed, no effects. |
| CORE-S8-02 | Transfer fingerprint/typed confirmation/revision rejection occurs inside capture; independent malformed authenticated attempts retain separate Rejected events. |
| CORE-S8-03 | Generation exhaustion remains a typed execution failure with Failed capture and no boards/call effects, not a semantic Rejected outcome. |
| CORE-S8-04 | No Success at Stream preparation; actual acceptance/current commit and rollback/SDK compensation are exercised. Normal replacement is separately closed by S8-FOLLOWUP-01. |
| CORE-S8-05 | Actual private primary/recut protocol produces one logical Admin Success; public phase selection and malformed private phases are denied. Two independent native primary calls are not silently deduplicated. |
| CORE-S8-06 | Exact indexed candidate work is independent of tested population size; strict order-index recognition, bounded exhaustion/cleanup and migration rollback pass. Full migration population/provider quotas remain unverified. |
| S8-RUNTIME-01 | Receiving-edge future/negative/NaN/unsafe time and empty pages fail closed; sampled expiry durably fences rollback/reconstruction before disclosure. |
| S8-RUNTIME-02 | Genuine GET Stream missing-owner/registry/sync/register/SDK faults capture fresh actual Admin Failed without false Success or stale attribution. |
| S8-RUNTIME-03 | Initial targeted, operator/read-owner and post-hydration receiving-edge faults use freshly verified actor/target and do not repeat original effects. Ordinary Host and expiry paths preserve typed fail-closed behavior. |
| S8-FOLLOWUP-01 | Normal same-session account replacement no longer races its predecessor's redundant close broadcast against initial successor output. Exact retirement still runs; current-account/participant callbacks and the original concurrent-output/authority/equality guards are unchanged. |

The narrow repair compares all six durable account binding fields synchronously; the comparison is not authorization and discloses no data. Only after successful exact Game/Delivery/Accounts predecessor retirement is a noncurrent/absent account callback's redundant broadcast skipped. Current callbacks, participant presence and real domain updates retain their paths. Permanent Host/Admin × actual client ACK/no-ACK controls each verify two public upgrades, three later real Host update snapshots, one current/native connection and no frame-audit flood. All 32 permanent repair cases pass. Independent original public no-ACK and actual live client ACK controls pass unchanged, with no held close callback or replacement snapshot queue/lease.

The final narrow reviewer also executed separate account sessions, replayed stale callback after newer revisions, actual current account close, malformed/unknown native attachment, exact SDK/prepared fences and the unchanged protected snapshot race guards. Physical executions: 71 passed, 1 failed observer setup attempt, 122 outside-focus skipped. The failed observer used the wrong serialized action tag; its original source/log/result and corrected one-case passing rerun are both retained. Successful process exit is not used as signoff. All current source/schema/artifact/graph/evidence/HEAD maps remain exact; concurrent authorized parent LLD/DO summary edits are explicitly recorded as policy/status differences, not hidden as unchanged.

## Preserved failures and provenance

The evidence composes historical blocked review and subsequent scoped repair; it does not overwrite a failed review with a passing label.

- Initial integrated runtime actually exited 1, **not killed**: 679 Worker cases in 35 suites, 670 passed/9 failed, no skipped cases. TLS was not reached. Discovery read-only assertions, ordinary-Host expiry status, History authority counts and five durable-denial variants required repairs. No forced-expiry/fencing/ACK assertion was weakened. Fourteen original files remain in the hashed pre-repair checkpoint.
- The first repaired full follow-up retained native 316 passes/2 obsolete two-primary assertion failures, selected Worker 168 passes/9 failures plus 200 skipped, and actual CLI 46/46. It found S8-FOLLOWUP-01 rather than accepting passing gates. Wrong typed-error expectations, direct-callback ACK-context diagnostics and an old discovery all-table-state assertion remain individually classified and failed where appropriate. Actual private primary/recut and narrowly allowed audit bookkeeping have separate passing evidence; these old probe failures are not reported as repaired test executions.
- Before the narrow Stream edit, the positive 956/profile, 707/36 Worker and 373 actual TLS gates and blocked full follow-up were preserved in a 42-file SHA-256 checkpoint. They are superseded for final integrated verification, not relabeled the post-edit gates. All four new permanent replacement controls first failed against that prior artifact, then passed on the rebuilt artifact; the old assertion bytes are unchanged.
- The retained direct-callback ACK exception and held-close scheduling diagnostic are not normal-path live client acceptance evidence. Final actual ACK evidence uses `client.send`. A parent shell PATH failure exited 127 before compilation and was resolved by restoring the prescribed existing toolchain environment, not an install/configuration change.
- The unchanged portable G4 final-return acceptance probe was rerun on the actual final audit artifact and still **fails**: 1 execution, 0 passed, 1 failed, 0 skipped. Its assertion receives HTTP 200 after actual Recovery where it requires 401/503. It remains `RUNTIME-FINAL-01`, not part of the positive Workerd suite and not closed by audit work.

The evidence records report/inventory/source/result hashes and exact scratch paths. Earlier frozen reports/probes remain unchanged. Scratch retention is not provider backup/restore certification.

## Limits and remaining authorization

- `RUNTIME-FINAL-01` remains OPEN. Neither explicit authorization-cut semantics nor stronger pending-response invalidation/timeout/crash coordination has been selected. Fresh challenges, repeated RPCs and owner-local audit success do not repair the final return boundary. Slice 6/7 final acceptance and Slice 2's older review remain open.
- Audit failure capture requires a currently provable privileged actor, an actual supported target and storage that accepts a durable record. Pre-target/global/index-only ingress does not invent a Game or fan out records. Current account/session/Origin/privacy authority is not relaxed to guarantee a log.
- Local migration, indexed-work, copy/restore/expiry and Workerd/native/TLS evidence do not prove all corruption shapes, a bounded full migration, provider physical copies/backups/log retention, deployed Unicode/KDF/SDK/CSPRNG behavior, production quotas/headroom/costs or delivery under crashes/load.
- Safe closed local API/CLI choices are documented as implementation choices, not newly approved product policy. No frontend/app audit UI/endpoint, configuration/dependency upgrade, AGENTS edit, deployment or infrastructure/secret provisioning was performed.
- This reviewed Slice 8 candidate remains local. A later commit/push needs separate publication authorization; Slice 9 provider-facing work also remains separate.
