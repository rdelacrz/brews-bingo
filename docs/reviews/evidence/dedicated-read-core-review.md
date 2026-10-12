Independent FINAL Slice 6 Core review — 2026-10-12

**One remaining blocking acceptance finding: CORE-READ-01 (P2 correctness).** This is a bounded source/native/SQLite review, not whole-backend approval. The project was not edited. Published HEAD remains `7ffcd7e4cdb5d0dbaf0980635a8f76ea9f1c28b6`; the index is empty.

**CORE-READ-01 — E5's typed codec accepts duplicate and noncanonical qualifying-line sequences.**

Location: [QualificationRead::validate](/home/orangepi/Workspace/brews-bingo/shared/contracts/src/games/reads.rs:142), specifically the account loop at line 151 and player branch at line 162. The reused [completed_lines decoder](/home/orangepi/Workspace/brews-bingo/shared/contracts/src/games.rs:1304) checks length and index range, but not uniqueness or ordering. The account branch only requires a nonempty list; the player branch only checks that `qualified` agrees with emptiness. Consequently both `decode_json` and the supposedly validating `encode_json` accept sequences that the approved deterministic SingleLine evaluator cannot return.

Actual native reproduction, in **both default and release-assertions**:

```json
{"role":"player","qualified":true,"qualifying_lines":[{"kind":"row","index":1},{"kind":"row","index":1}],"view_revision":0}
```

The independent probe tested four invalid sequences in each of the two branches: duplicate row, rows 2 then 1, column before row, and anti-diagonal before main-diagonal. **All eight decoded and re-encoded successfully.** The probe asserts rejection and fails with the exact eight accepted case names. This is executed Rust codec evidence, not a hypothetical SQL failure. No assertion claims that current SQL projections emit these malformed sequences or that this bypasses authorization. Existing board-backed validation does compare against the evaluator; the standalone E5 DTO loses that guarantee.

This violates the requested strict DTO/order validation, finite acceptance A08/H10/T11–T12, and DO-063's deterministic rows/columns/main/anti order. Repair the standalone qualification validation to enforce unique ascending rows, unique ascending columns, then main and anti diagonals, in both branches; preserve current bounds and qualified/empty consistency. Add permanent rejection tests for decode and encode, plus a valid maximum-line positive case. No new policy is needed. This reviewer made no repair to the frozen project.

Probe source: [core_qualification_review.rs](/root/.hermes/cache/scratch/brews-core-independent-src/shared/contracts/tests/core_qualification_review.rs:1). Evidence: [default follow-up log](/root/.hermes/cache/scratch/brews-core-native-default-followup.log), [optimized codec log](/root/.hermes/cache/scratch/brews-core-native-release-qualification.log).

**Freeze and provenance**

I ran `python /root/.hermes/cache/scratch/verify_brews_read_slice.py verify` before review and again after execution. All **226 source/test hashes and two SQL/DDL-fixture hashes** match the supplied final manifests and the current structural graph's source inventory. HEAD/index/protected-file checks passed. I recorded before hashes before substantive source inspection; after hashes match all **237 recorded project inputs**, including planning/guidance. No unexpected frozen production changes were observed. Hash verification of 226 files is not a claim to have manually reviewed every line in all 226.

[core-inspected-hashes.json](/root/.hermes/cache/scratch/core-inspected-hashes.json) contains explicit path→SHA-256 maps for **41 personally inspected files**, before/after maps and scope, frozen-match booleans, all review-input and test-log hashes, exact PASS names, run counts, scratch provenance, and excluded claims. Inspection includes changed source diffs and relevant transitive sections, not whole-file certification for every listed file.

The scratch workspace initially matched every frozen source/test file. Its production sources remain exact copies. Only `backend/tests/games_core/reads.rs` has appended review tests, and the standalone qualification integration test is new. Cargo used a newly created isolated target directory; actual selected probe names appear in logs. There were no stale-binary or wrong-workspace runs counted. No project source/tests/docs/graph/build artifacts, staging, commit, push, deployment, provisioning, secrets or frontend were changed.

Direct comparison to `git show 7ffcd7e:backend/src/db/schema/game_schema.rs` shows the saved counter-5 template differs only by its provenance comment and redirect to the frozen operator vocabulary. The saved operator template replaces only the mutable purpose enumeration with its four published literal tags. Other referenced policy/tag definitions are unchanged at this frozen cut. The prior-v1 golden SQL remains byte-identical to HEAD. [schema provenance](/root/.hermes/cache/scratch/brews-core-schema-provenance.json) records published and template hashes. The executed permanent fixture test matches the frozen published DDL/index array; no previous inventory was inferred by filtering today's table list.

**Bounded review conclusions beyond the finding**

- A01–A09 / R01–R03: all nine dedicated route names and DTO mappings are wired. Closed selectors separate account-only, own-player, participant and general reads. Read projection code directly uses authorized projections; it does not call `account_view`, enter, reconcile presence or allocate a grant. Existing Sync/Stream entry code remains intact. Real SQLite snapshots confirm first cross-host reads preserve all owner rows at a fixed clock; subsequent terminal access still denies that outsider. Necessary clock, expired cleanup and Admin audit work are separate from gameplay activity.
- D1 / H02–H04 / T05,T13,T14,T17: actual PUT then DELETE with the same session produces true then false, preserves original expiry, advances only that player's private revision, and leaves the other player and shared Host revisions unchanged. Terminal D1 is false; final reads require original grants. Original account Exit denies E4 while separate History access remains. Permanent tests cover recovery epoch invalidation and History purge denial.
- E1–E5 / H09–H10 / T06–T12: existing safe call/board machinery is reused; stable-ID ordering and canonical selectors are bounded. The executed maximum cases use the published **20 players, 100 cells/board, 1,000 values/calls and 20-byte alias limit**, not 50 players or a new alias policy. A real complete 1,000 accepted-call run stays In Progress and exhausted. E2 terminal is explicitly unavailable after grant authorization; no original pool is guessed or retained. D2 terminal Leave is null and retained refers to History's frozen roster. Current final-connection rows, not History presence, feed terminal presence. The E5 codec exception is CORE-READ-01 above.
- H05–H08 / T15–T23: native release tests exercise identity/epoch/role/assignment/deletion, same-identity payload change, unchanged original cap, equality/rollback fencing and ignored fence writes. I inspected Worker route/release wiring for integration context, but **did not execute Worker/Wasm, held asynchronous races, WSS, or HTTPS**. Those remain the separate runtime reviewer/parent's evidence; this report does not substitute native tests for them.
- H11 / T24–T25: typed read-purpose tags differ from target discovery, the owner audit includes only actor/target/purpose/outcome/times, and required capture precedes release. Personally executed tests/probes cover Admin success/state rejection, ignored insert, insertion failure, audit readback failure, entropy failure, immutable target identity, fixed 90-day equality cleanup and rollback fencing. Failed required capture withholds the read and rolls back. No assertion claims knowledge of an unobserved edge delivery outcome or completion of older-operation/CLI auditing.
- H12 / T26: published counter-5 migration changes the audit CHECK vocabulary only. Personally executed permanent tests reject extra columns/tables/indexes, missing indexes, altered CHECK/FK/STRICT and partial Delivery shapes before writes, and roll back ignored/aborted markers. Additional scratch probes construct fresh databases from the **published DDL fixture**, then insert rows obtained from actual executed game/recovery/terminal operations. Live players/sessions/boards/recovery/closure work, final History/boards/original grants/pending work and complete optional Delivery survive migration with only the schema marker changed. A real prepared Game/Directory transfer remains intact. Foreign-key checks pass. A fault after dropping the old audit table but before rename rolls back the entire catalog and data. Known-v1 populated migration and the existing v2–v4/optional-Delivery shape matrix pass in both profiles. No new History pool/Leave/session metadata is present; production resolution/terminal/History modules are unchanged from publication.

**Execution accounting — 69 native case executions total, including failures and retries**

All commands used scratch cwd `/root/.hermes/cache/scratch/brews-core-independent-src`, `CARGO_TARGET_DIR=/root/.hermes/cache/scratch/brews-core-independent-target`, `CARGO_HOME=/home/orangepi/.cargo`, `RUSTUP_HOME=/home/orangepi/.rustup`, `RUSTUP_TOOLCHAIN=1.99.0`, `TMPDIR=/root/.hermes/cache/scratch`, and the requested Cargo PATH entries. No whole-workspace gates ran.

Common selection F:

```text
test(dedicated_read) or test(dedicated_release) or test(core_probe) or test(operator_audit) or test(operator_identity_is_immutable)
```

Historical selection H:

```text
test(forward_schema_migration_preserves_existing_game_configuration_and_board) or test(historical_gameplay_recovery_spectator_inventories)
```

| Actual invocation | Executed result | Log |
|---|---|---|
| `cargo nextest run -p brews-backend -p brews-contracts --all-features --no-fail-fast -E F` | 31 run: 30 passed, 1 scratch-helper failure, 778 skipped | `brews-core-native-default.log` |
| Same, `--cargo-profile release-assertions`, filter `F or H` | 33 passed, 0 failed, 776 skipped | `brews-core-native-release.log` |
| Default, filter `test(core_probe_populated_published) or H or test(core_qualification_requires)` | 4 run: 3 passed, 1 reproduced contract failure, 806 skipped | `brews-core-native-default-followup.log` |
| `cargo nextest run -p brews-contracts --all-features --cargo-profile release-assertions -E 'test(core_qualification_requires)'` | 1 run: 0 passed, 1 reproduced contract failure, 82 skipped | `brews-core-native-release-qualification.log` |

F/H above are notation for the literal expanded filter expressions, not shell variables that were executed. Exact selected test names and SHA-256s are in the evidence JSON. Across these runs, the 29 selected permanent tests and four successful scratch probes pass in both profiles; the fifth scratch test reproduces CORE-READ-01 in both profiles. Total actual executions: **66 passes, two reproductions of the product contract failure, one corrected reviewer-fixture failure = 69**, below the requested ceiling. Skips are explicit filtering, not unexecuted tests reported as passes.

The first scratch migration failure was **not a product finding**: its generic snapshot used `ORDER BY 1,2` for an optional Delivery table with only one column. I changed only the scratch helper to `ORDER BY 1`, kept all preservation assertions, and reran the affected test successfully in both profiles. That initial failed log remains preserved. The isolated build/list log is preparation, not an extra executed test gate.

Standalone reproduction of CORE-READ-01 in the scratch workspace is the final table command, with `--cargo-profile release-assertions` omitted for default. It runs the real codec and intentionally fails its rejection assertion; no source patch or positive authentication fixture is needed.

**Limits and disposition**

No remaining Core blocker was found in the inspected authority, passive-storage, terminal-retention, audit or migration paths, but **the slice is not signed off while CORE-READ-01 remains**. Broad parent test totals cannot close this concrete missing validation. The original finite acceptance analysis is baseline analysis, not approval. Runtime race/transport evidence belongs to the parent/runtime reviewer. No whole-backend, production KDF/Unicode/clock/provider/load/headroom/backup/deployment certification or publication permission is implied. Older Slice 2 review, Slice 7, remaining Slice 8 and production Slice 9 work remain open.
