# Independent protected snapshot and full-game harness review

Review started 2026-10-14 04:09:59 UTC and completed within the requested ten-minute bound. Repository review was read-only. Only review copies and safe reports were written under scratch. No repo edits, graph refresh, cargo/Worker build, SDK/Workerd/TLS/server/full-suite execution, provider calls, deployment, provisioning, secret changes, commit or push occurred.

## Disposition

No independently reproduced blocking defect was found in the approved protected HTTP snapshot acceptance, exact 20-case contract parser, or full-game scenario within this review. This is a bounded source/offline-test review, not final integrated gate approval. One known deployment integration blocker was independently reproduced, and documentation remains pending with the parent. The parent's final run must cover the current deletion-readback change and the cohesive final source cut.

The normative adopted policy is `docs/plans/protected-response-policy.md`: **“It may finish because it was authorized before revocation. Revocation blocks subsequent requests.”** Approval is already given. No weaker or alternative policy is proposed. Sequential Accounts/Game/Directory cuts are explicitly not an atomic remote-state cut; starting a request before revocation alone does not authorize its completion. Original expiry/caps, durability, invocation/projection binding and receiving-side rollback fencing remain mandatory.

## Findings and parent-owned follow-up

1. **Known integration blocker, reproduced offline; deployment reviewer owns repair.** `backend/tools/release_gate.py:86–101` hashes `AGENTS.md`, `docs/rust-best-practices.md` and `docs/operations/cicd.md`. `backend/tools/deployment.py:136–139` excludes those paths; `local_gate` requires every runner input to be present with the same hash at line 169 in the latest inspected cut. An honest all-lane report therefore cannot freeze successfully. An explicitly synthetic selector/local_gate reproduction using the prior frozen 21-file input set omitted `AGENTS.md` and `docs/operations/cicd.md` and raised exactly `Local gate source drift`. The Rust guide omission follows directly from the same selectors; it was supplemental static inspection, not part of that 21-file reproduction. Evidence: `/root/.hermes/cache/scratch/brews-snapshot-review-final-frozen-20261014/selector-reproduction.json`. Latest deployment changes preserve the selector mismatch. Scoped repair: reconcile deployment's approved source inventory with the runner inventory and verify an honest report at the final cut. Do not bypass the hash check. No deployment code was changed here.
2. **Documentation gap, parent already owns verification.** `docs/operations/cicd.md:3` still says the retained RUNTIME-FINAL-01 assertion remains blocked, and lines 33/46 describe readiness with the old probe framing. The file does not explain standalone `--lane worker` omission. `release_gate.py:452–466` deliberately excludes the new suite; `run` lines 505/519–524 executes the mandatory contract lane only for `--lane all` or `--lane contract`. CLI help merely lists lane choices. Document that standalone worker checks omit these 20 cases, all-lane release checks include them once through contract, and the historical physical failure remains preserved under the superseded requirement. This is an accurate documentation correction, not a policy change.

## Protected-response source review

`protected_response_snapshot.test.js:76–108` intercepts only the second genuine `/read-release` of the current invocation. It checks the distinct challenge and original cap. The before-cut variant commits its action before calling the real owner fetch. The after-cut variant first obtains the real successful owner ACK, checks its challenge/cap, commits the real mutation, and returns the unchanged cloned bytes/status/headers. The harness does not fabricate a success proof or mutate its protected reply. Interception is transport scheduling around the real owner operation, not a mocked authorization result.

The exact inventory is 20 cases:

| Coverage | Cases | Source |
| --- | ---: | --- |
| G4 command recovery before/after final cut | 2 | 164–179 |
| Dedicated membership, assigned board, private player detail recovery before/after | 6 | 181–215 |
| Recovery-answer DELETE and alias rename before/after | 4 | 217–234 |
| Authoritative SQL account credential-epoch invalidation before/after | 2 | 236–253 |
| Genuine cross-session account final Exit before/after | 2 | 255–283 |
| Original cap equality; cap denial with rollback during durable fencing | 2 | 285–314 |
| Earlier invocation `/read` and final `/read-release` ACK substitution | 2 | 316–351 |

Recovery uses the actual recovery API and checks stable identity/current state. SQL epoch invalidation updates only the exact Accounts row, synchronizes storage and reads back the new epoch; its label/comment accurately limits it to authoritative storage, not public disable/reset/remove endpoint coverage. Board/private-detail/Exit fixtures obtain actual WebSocket snapshots and send exact snapshot ACKs, then Start. Final Exit uses a second actual account login, registers it through sync, reaches terminal state through confirmed In Progress cancellation, and invokes the real Exit API. Private-player detail excludes the other player's alias and ID. Subsequent revoked requests deny; after content mutations fresh authorized reads reflect the new content. Receiving-cap equality still denies, the durable floor reaches the cap even during rollback, and a later rollback read is denied. Saved prior invocation ACKs cannot authorize a new identical read.

`release_gate.py:305–385` requires exactly the 20 distinct names in the one expected physical file; skipped/pending/todo/runtime/unhandled errors, wrong files/names, duplicates, contradictory counters/status/exit codes and boolean counters fail closed. Missing policy/suite/historical inputs block. Current contract input hashes must equal evidence hashes; current inventory/status/policy and historical result are checked again. `run` snapshots source hashes before/after and requires unchanged Worker artifacts. The policy marker check is part of a reviewed source cut, not a cryptographic independent approval signature; no report-only bypass was found.

The unchanged historical test/config SHA-256 pins match exactly. Its known failure remains `failed_acceptance_assertion`, raw exit 1, failed 1, returned HTTP 200, `superseded: true`. Strict stack/assertion classification rejects unrelated or infrastructure failure. It is not relabeled as a passing old requirement. New failed acceptance remains a failed contract/nonzero runner exit. General Worker inventory and execution both exclude the new suite; mandatory all-lane contract executes it once. The historical parser itself remains the original parser according to the supplied runtime report; this reviewer verified its exact pinned input hashes and classification behavior offline.

## Full-game and identity source review

- Import and help are inert; neither launches a Worker, CLI management operation or network. Explicit invocation requires exact origin pinning and credentials. HTTPS TLS verification stays enabled; local custom CA is limited to loopback; redirects/proxies are rejected. HTTP actors use separate host-only Secure/HttpOnly jars and scope-check cookies. Staging namespace/serving identity is explicitly an independently checked deployment-tool responsibility, not inferred from an environment label.
- Bootstrap creates only a random throwaway `host`, not an administrator; CLI secret travels in the child environment. Native CLI outputs bounded metadata; child streams are captured in memory and failures become fixed diagnostic labels. Enrollment and uncertain-create handoff links are private and promptly unlinked even on failure; cleanup reuses the original create command/payload and proves exact account ID before delete.
- One actual random call and actual manual calls are checked against the independent 5×5 oracle for all three players, host HTTP/WSS, and public spectator WSS. The oracle checks all cells, lines and qualification; free center and calls do not depend on production imports. Replay reuses immutable UUID and serialized bytes, requires receipt-only/no-cookie replay, and checks every viewer's unchanged full sync. 202 is explicitly pending and does not masquerade as completed 200; lookups/retries keep the original command identity.
- Continuous WSS readers validate game/role/private identity, exact envelope, connection/delivery identity, per-viewer revisions and nonextending original session expiry, then send masked exact ACKs. Parser bounds cover message/frame bytes, fragmentation, control frames, backlog and waits; malformed JSON/frames fail with fixed labels. The chosen 75-value/5×5/free-center profile is a fixed supported test profile, not a maximum-capacity test. No measured provider headroom is claimed.
- Creation ownership comes from this actor's actual create receipt or exact-command creation lookup, never current-slot discovery alone. An occupied existing slot fails before game creation. Cleanup cancels only proven own nonterminal game IDs using exact observed expected state, leaves previous/unowned games alone, closes every opened stream, then removes only its own account. Unresolved own-game cleanup preserves the account and reports failure. History intentionally persists until contractual expiry.
- Parent's latest readback repair is included in the latest freeze: `bootstrap.py:45–63` retains bounded stderr only for exact account absence readback, requires CLI exit 1, exactly one error object and the precise HTTP 404 message; `remove` lines 143–157 targets the same proven account. `cli/src/main.rs` confirms CLI errors are written to stderr. Extra/malformed/error/status 401/403/500 output cannot certify absence. Normal child stderr is discarded; no raw chain is persisted. Concurrent retained stdout/stderr can fail closed on unexpected output, not silently succeed.
- `deployment_identity.rs` returns only protocol version and canonical non-nil immutable UUID from `CF_VERSION_METADATA`; missing/malformed metadata fails unavailable. HTTPS/GET/no query/no body checks precede output. Router wiring exists. Identity tests cover privacy/no cookie, HTTPS, disallowed methods, query grammar and fetch-level GET-body rejection. Vitest uses random ephemeral test-only keys and a fixed local metadata fixture. No identity Worker tests were executed by this reviewer; these fixtures do not establish a deployed serving identity.

## Exact independent checks performed

Frozen code copies avoid races and disable bytecode writes. The first tested freeze contains a scratch `.git` metadata pointer to the existing repo so unit tests can read HEAD; no Git mutation or publication occurs. All process-executing gate methods are mocked by these unit tests except isolated Python child timeout/cancellation/output tests. E2E socket/HTTP/CLI operations are fixture-driven, except the inert help subprocess.

1. In `/root/.hermes/cache/scratch/brews-snapshot-review-final-frozen-20261014`:
   `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s backend/tools/tests -p test_release_gate.py -v`
   **27 run, 27 passed, 0 failed/skipped, exit 0**, tool wall time 1.189505331 s.
2. In that same freeze:
   `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s backend/tools/tests -p test_full_game_e2e.py -v`
   **56 run, 56 passed, 0 failed/skipped, exit 0**, tool wall time 0.194777408 s.
3. After the parent's stderr-readback correction, in `/root/.hermes/cache/scratch/brews-snapshot-review-readback-frozen-20261014`:
   same `test_full_game_e2e.py` command, **56 run, 56 passed, 0 failed/skipped, exit 0**, tool wall time 0.260261370 s. This rerun was justified by the changed readback implementation and fixture.
4. Offline synthetic source-selector/local_gate rejection reproduction described above; not runtime evidence.
5. Read-only `validate_snapshot_contract` of the parent's actual `/root/.hermes/cache/scratch/brews-runtime-policy-final/contract-safe-summary.json` against current release_gate/input hashes: passed, new 20 passed and historical 1 failed preserved.
6. Source/historical hash checks and latest live-versus-frozen comparison: 26 inputs checked, zero differences at the last comparison. Earlier concurrent parent changes are preserved in separate freezes rather than attributed to tested older hashes.

Safe summaries are stored as `independent-test-summary.json` in the tested freezes. No raw child logs, private boards, cookies, passwords, tokens or assertion response bodies were written into this report or review evidence.

## Existing actual evidence inspected, not rerun here

The runtime handoff `/root/.hermes/cache/scratch/brews-cicd-runtime-policy-report.md` and safe execution metadata record actual focused Workerd execution: **20 new passes, exit 0; original physical 1 failure, exit 1**, both under unchanged source/artifact inputs. The matching safe summary passed the independent validator. This reviewer did not reproduce those Workerd executions.

Parent-owned `/root/.hermes/cache/scratch/brews-cicd-e2e-loopback-04/execution.json` and `e2e/summary.json` record actual loopback HTTPS/WSS **exit 0, 95 checks, host + 3 players + 1 spectator, 6 accepted calls, 44 ACKs, cleanup own_account_removed**. It is local loopback, not provider/staging execution. Its 95 labels do not include the new `cleanup_own_account_absent` readback label, so it cannot be presented as execution of the later readback cut. Earlier loopback01–03 failures remain earlier failures; the supplied task explains the correct last-admin guard and least-privilege host correction. Only loopback01's safe failure summary was separately opened here, and it remains `cleanup_failed`, not an expected pass.

Final integrated SDK/TLS/full-game checks and doc/graph verification belong to the parent after cohesive edits. No production/staging tests, external gate closure, capacity/headroom certification, atomic remote cut guarantee or final full-gates signoff is asserted.

## Latest inspected source SHA-256

Latest frozen root: `/root/.hermes/cache/scratch/brews-snapshot-review-readback-frozen-20261014`. Earlier 27-test and first 56-test cut: `/root/.hermes/cache/scratch/brews-snapshot-review-final-frozen-20261014/inspected-hashes.json`. Release-gate/test/snapshot/policy/historical hashes are unchanged between these cuts. Initial pre-readback inspection is preserved separately in `/root/.hermes/cache/scratch/brews-snapshot-review-frozen-20261014`.

| Path | SHA-256 |
| --- | --- |
| `AGENTS.md` | `f99c90f3d787ee32f62cd0496d927f3e678dbd6c1837c80c6307054ad4c9f248` |
| `backend/src/worker_runtime/deployment_identity.rs` | `438bdf21aec944ae45110ce646ffb880fdcea90113a31d86d4066f8412a65794` |
| `backend/src/worker_runtime/mod.rs` | `1335643ba1f148aebad13065588c341f467f5f8a0d539c18292bf76b56291150` |
| `backend/tests/e2e_support/__init__.py` | `7c85136a0f752e0487a121ddf6f4cc84cb556a663197b15fd2e08c5490850c44` |
| `backend/tests/e2e_support/bootstrap.py` | `97cc1cbd437046513b6e7e9fc70b3f8073b6430d66608ae01a4a1faee117e270` |
| `backend/tests/e2e_support/http.py` | `ad5ff008a7feccb20f19dc6b6c124a5c0eb79a5e35e85133be49a1b8564175f8` |
| `backend/tests/e2e_support/oracle.py` | `b7a6fceea7c5c52773959593635a43b9e1ade84da9332af7c064cc8c4b4097b7` |
| `backend/tests/e2e_support/safety.py` | `880fccb464f71fe4c8ad432f4005338f9738b6146e895c614f3b5a4911e57aee` |
| `backend/tests/e2e_support/scenario.py` | `49908981836d38a35063f01153b3d1e702e7a44675f223b14ed4f706d1fede24` |
| `backend/tests/e2e_support/stream.py` | `1c4be1950dedf3101bffd533fd1f16ce625b128718c1cc4f770ee6831976ec26` |
| `backend/tests/full_game_e2e.py` | `32b97b898e87c64f9093199ce379052e4f40217e2021c0967af43c74f41bb3e5` |
| `backend/tools/deployment.py` | `0e2eb5a7690b7bc851e3e806d59ef43fa109ecad87a47d8317136e9d098d9d29` |
| `backend/tools/release_gate.py` | `601aa2c5d2f394ac3f67b212916e381b75c91284393dd8063d5ac77ffc8b7d97` |
| `backend/tools/tests/test_full_game_e2e.py` | `11a2559e8fd992d50cf4667b7f21349cb169ff8582785800502139fd472df16c` |
| `backend/tools/tests/test_release_gate.py` | `a238c23f5bb106fbe5031466ff4ce0f8941d06353b6be3f0b8e7782418faa8c0` |
| `backend/vitest.config.js` | `7301e8f6aa68beb6f2af5b4fbdc5633a5420c626665330c67d7f77861b2c7c4e` |
| `backend/worker-tests/deployment_identity.test.js` | `4172bbdd9e583dac72523cc403cc361f002ea8d7a975be876d4ec867ca73940d` |
| `backend/worker-tests/protected_response_snapshot.test.js` | `5b0a339b10d4723177bcdf79163126b43fe6b2ebb98e5335b5dada7854703d9d` |
| `cli/src/error.rs` | `7ef0b5c901045a8e3574ad44acdafed69f3605a7edfb36b062a78c3eb85721a1` |
| `cli/src/main.rs` | `57fb066a0210c2aa2a757b6c4ca95338a70fcad671ace798087dc0c41793474f` |
| `cli/src/output.rs` | `37de78ea1015f97e9aa16e0f89c84c089a6a494e575cd431aed3e91a39ffa076` |
| `docs/operations/cicd.md` | `c7a4cee6b265fcdd112754d343992d89d654b134dd6be93bd755d4ee04d7544f` |
| `docs/plans/protected-response-policy.md` | `fa1aa9d4e1602f36d2eb841e1ad590fc1ccbd344ea5c02a1dcae8964d2f2db1d` |
| `docs/reviews/evidence/command-lookup-final-return.config.mjs` | `fa946f6c1fcec6da2e54f1c207a1ebddd9d730489121adface64bb0ea99d3355` |
| `docs/reviews/evidence/command-lookup-final-return.test.js` | `28cec08b858f5ca8ab0335007d8a1afcf3cbd63612ace70c5f21261f86875a11` |
| `docs/rust-best-practices.md` | `1d7d2fd0d5ea2f94d81124e16598557b0cd8e5a32aee951b8cc2a2c3adc4bab5` |
