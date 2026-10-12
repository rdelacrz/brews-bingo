CORE-READ-01 repair follow-up — 2026-10-12

**CORE-READ-01 is repaired. No new blocker was found in this bounded follow-up. RUNTIME-FINAL-01 remains open; this is not whole-Slice6 approval.** HEAD remains `7ffcd7e4cdb5d0dbaf0980635a8f76ea9f1c28b6`. No publication or repository/source/test/doc/graph/build/Git modification was made by this reviewer.

I read the [original Core report](/root/.hermes/cache/scratch/brews-read-slice-final-core-review.md), its inspected-hash evidence, unchanged reproducer and original failure logs. I personally reran the original reproducer and both new permanent tests against the repaired real contracts source, in default and release-assertions profiles. Parent test results are not counted as my evidence.

The private helper at [reads.rs:173](/home/orangepi/Workspace/brews-bingo/shared/contracts/src/games/reads.rs:173) enforces the existing 22-line maximum and one-based row/column indices 1–10. Strictly increasing `(kind, index)` keys reject duplicates and enforce rows ascending, columns ascending, main, anti. Both account and player branches call it. Existing account nonempty and player qualified/empty consistency checks remain. The real `map_enum` Deserialize path invokes validation, and `encode_json` decodes the serialized bytes; the execution therefore exercises the final codec, not a helper replica.

The original scratch reproducer is byte-for-byte unchanged (SHA-256 `c1a6bf7f4df16d5792cccf2c9a5af509e621cdbbde320fd53cbf302caee87044`). All eight original invalid-input cases remain: duplicate row, reversed rows, column before row and anti before main, each for account and player. The reproducer now passes its rejection assertion in both profiles. Its old conditional re-encode assertion remains; independent invalid-encode coverage comes from the permanent rejection test, which directly checks decode, encode and validate for 12 invalid sequences in both branches. The permanent positive test exercises the canonical subset and all 22 lines in both branches, empty unqualified player, empty account roster, and rejection of an empty qualification entry. These internal assertions are not counted as separate nextest case attempts.

Exact execution (scratch cwd `/root/.hermes/cache/scratch/brews-core-independent-src`, isolated target `/root/.hermes/cache/scratch/brews-core-independent-target`):

| Cargo profile | Actual result | Log |
|---|---|---|
| default | 3 executed, 3 passed, 0 failed, 82 filtered/skipped | [default](/root/.hermes/cache/scratch/brews-read-slice-core-repair-independent-default.log) |
| release-assertions | 3 executed, 3 passed, 0 failed, 82 filtered/skipped | [optimized](/root/.hermes/cache/scratch/brews-read-slice-core-repair-independent-release-assertions.log) |

**Six executed case attempts total, zero failures or retries**, below the ten-case ceiling. The optimized log confirms the `release-assertions` Cargo profile; nextest itself uses its default runner profile. Both invocations selected exactly:

- `core_qualification_requires_unique_canonical_line_order`
- `games::reads::tests::dedicated_read_qualification_rejects_duplicate_unordered_and_out_of_bounds_lines`
- `games::reads::tests::dedicated_read_qualification_accepts_canonical_subset_and_maximum_lines`

Actual commands:

```sh
cargo nextest run --locked -p brews-contracts --all-features --no-fail-fast -E 'test(=core_qualification_requires_unique_canonical_line_order) or test(=games::reads::tests::dedicated_read_qualification_rejects_duplicate_unordered_and_out_of_bounds_lines) or test(=games::reads::tests::dedicated_read_qualification_accepts_canonical_subset_and_maximum_lines)'
cargo nextest run --locked -p brews-contracts --all-features --no-fail-fast --cargo-profile release-assertions -E 'test(=core_qualification_requires_unique_canonical_line_order) or test(=games::reads::tests::dedicated_read_qualification_rejects_duplicate_unordered_and_out_of_bounds_lines) or test(=games::reads::tests::dedicated_read_qualification_accepts_canonical_subset_and_maximum_lines)'
```

Both used `PATH=/home/orangepi/.cargo/bin:/root/.cargo/bin:$PATH`, `CARGO_HOME=/home/orangepi/.cargo`, `RUSTUP_HOME=/home/orangepi/.rustup`, `RUSTUP_TOOLCHAIN=1.99.0`, `TMPDIR=/root/.hermes/cache/scratch` and the isolated `CARGO_TARGET_DIR` above. Exact argv, resolved environment, times, exit codes, log hashes and pass names are in the [evidence JSON](/root/.hermes/cache/scratch/brews-read-slice-core-repair-inspected-hashes.json). An initial scratch launcher stopped before invoking Cargo because the parent's default log name already existed; that log was preserved and independent log names were used. This executed zero cases.

Provenance and freeze:

- Original production `shared/contracts/src/games/reads.rs`: `d2fde9e555f2f4cfec5e52fb551203ccbef1def5ebfb50805578e41b6fe3a619`.
- Repaired production/current frozen/copied scratch source: `ff6cfea769c414a45994b9ef13fa3922dcc2f9b891254ca98c90ed06cbcee062`.
- Before and after execution, `python /root/.hermes/cache/scratch/verify_brews_read_slice.py verify` passed: all **226 source/test and two SQL/fixture hashes** match the current manifests, and the structural graph's source inventory matches. No original candidate hash was substituted for the repaired manifest. Hash agreement is not a claim to have manually inspected every source.
- Of **1002 recorded project inputs**, **999 retained identical hashes**, including all frozen code/tests, SQL fixtures and graph files. Three planning documents changed externally during this review: `docs/plans/PENDING.md`, `docs/plans/api-design.md`, and `docs/plans/lld.md`. Their distinct before/after hashes are recorded in the JSON; I did not modify them or certify their revisions. Consequently whole-input equality is explicitly false. The current source/SQL manifests, original report/evidence/logs and scratch tests are unchanged. HEAD and empty index remain unchanged; before/after working-tree status is retained separately, including external document drift. This does not invalidate the exact repaired-code execution evidence.
- Only the repaired production contracts file was copied into the original isolated workspace. All other frozen sources still match, except the previously appended `backend/tests/games_core/reads.rs` scratch probes, whose full hash is unchanged and whose prefix still equals production. The original standalone reproducer is also unchanged. Cargo manifests/lockfile and SQL fixtures match production. Both test logs identify the isolated crate path.
- Earlier failing logs remain untouched: [default original failure](/root/.hermes/cache/scratch/brews-core-native-default-followup.log) and [optimized original failure](/root/.hermes/cache/scratch/brews-core-native-release-qualification.log). Their hashes still equal the original review evidence. The before snapshot here was taken before copying and test execution, after initial source inspection.

Original unaffected Core scoped conclusions on authority, passive reads, retention, audit and migrations are carried forward explicitly from the original report; those tests were **not re-executed here**. This report closes only CORE-READ-01. No whole-workspace suite, Worker/Workerd build, runtime race/transport test or wider backend/provider/deployment readiness assessment was performed. RUNTIME-FINAL-01 remains open. No blanket sign-off or publication authorization is implied.
