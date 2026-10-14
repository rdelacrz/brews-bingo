# RUNTIME-FINAL-01 focused local handoff

Approved policy: `authorization_time_snapshot`. Normative source: `docs/plans/protected-response-policy.md`, explicit user choice “It may finish because it was authorized before revocation. Revocation blocks subsequent requests.”

Only `backend/tools/release_gate.py`, `backend/tools/tests/test_release_gate.py`, and the new `backend/worker-tests/protected_response_snapshot.test.js` were edited by this task. No shared runtime implementation change was necessary. Parent-owned deployment identity, docs/reviews, graph, deployment/e2e files and old evidence were not edited. No builds, provider actions, deployment, credentials, commit or push were performed.

## Final actual focused execution

- New permanent Workerd suite: 1 physical file, 20 passed, 0 failed, 0 skipped, actual process exit 0. The exact 20-case inventory and reporter counters passed the strict acceptance parser; no runtime or unhandled errors were reported.
- Original unchanged historical Workerd probe: 1 physical file, 0 passed, 1 failed, 0 skipped, actual process exit 1. Original `parse_contract` result: `failed_acceptance_assertion`, finding `RUNTIME-FINAL-01`, observed returned HTTP status 200. Its known assertion remains failed and is recorded with `superseded: true`; it is never relabeled as passed.
- Runner unit/fault suite: 27 tests run, 27 passed, 0 failed, 0 skipped, actual process exit 0.
- Combined direct `Runner.contract()` result: `passed_authorization_time_snapshot`, policy `authorization_time_snapshot`, passed 20, failed 0, skipped 0. Both actual Workerd executions were separate. `Runner.run()` all-lane execution was not performed; exit behavior was unit-tested: new acceptance failure returns nonzero, malformed/stale evidence blocks, correctly preserved historical assertion failure does not veto the approved policy.
- The existing Worker artifact was used directly. All four artifact hashes remained unchanged before/after the two Workerd executions.
- `parse_contract` AST is identical to the pre-edit tracked version. The unchanged historical input hashes match the user-provided pins. JavaScript syntax and focused `git diff --check` passed.

Final safe execution metadata: `/root/.hermes/cache/scratch/brews-runtime-policy-final/`. Actual child stdout/stderr was retained only in memory; persisted metadata contains hashes, command/exit/timing information, safe case identifiers and outcome counts, never protected bodies, cookies, passwords, tokens or raw assertion payloads. Metadata and summary files are mode 0600; the directory is mode 0700.

## Coverage

The 20 mandatory tests cover before-final-cut denial and after-final-cut allowed completion for G4 recovery, dedicated membership, assigned board, private player game detail, recovery-answer DELETE, alias rename, scoped authoritative SQL credential-epoch invalidation (explicitly not public disable), and real cross-session account final Exit. Board/private-detail and Exit fixtures use actual WebSocket attachment/snapshot ACK, Start and confirmed In Progress cancellation. The held reply is the second genuine `/read-release` for the current invocation; each authorization reply is produced by the actual owner and its cloned bytes remain unchanged while the real mutation commits. Subsequent revoked-session requests return 401; subsequent current-session content reflects DELETE/rename and recovery. Expiry equality after the genuine final reply still denies, including rollback during the real durable expiry denial and a persistent floor followed by rollback denial. Genuine earlier `/read` and final `/read-release` ACK substitution returns 503, with subsequent fresh revoked-session requests returning 401.

## Runner and parent integration

The contract lane independently executes new acceptance and the untouched original probe. It validates exact names/counts, unique reporters/inventories, no skipped/todo/pending/error cases, approved policy and source hashes, strict known historical assertion classification, and current summary policy/status. The general Worker lane explicitly excludes the new suite and its expected inventory excludes it as well, so release-gate worker/contract counts do not double count acceptance. Native and TLS inventories are unchanged. Production blockers retain Slice 2 and Slice 6/7 final independent review plus unrelated deployed/provider gates; the pending policy decision was removed. Source hashes include the approved policy, new suite, planning/operations/Rust/agent policies and relevant `.github` workflows/policies.

Parent handoff: wire `deployment.local_gate` to require policy `authorization_time_snapshot`, status `passed_authorization_time_snapshot`, exact new acceptance inventory and hashes, and nested `superseded_receiving_cut_probe` preserving the real historical outcome. Builds/full suite, graph refresh, and docs/reviews remain parent-owned. These focused results are local verification, not production certification.

## SHA-256 source inputs

| Path | SHA-256 |
| --- | --- |
| `backend/tools/release_gate.py` | `601aa2c5d2f394ac3f67b212916e381b75c91284393dd8063d5ac77ffc8b7d97` |
| `backend/tools/tests/test_release_gate.py` | `a238c23f5bb106fbe5031466ff4ce0f8941d06353b6be3f0b8e7782418faa8c0` |
| `backend/worker-tests/protected_response_snapshot.test.js` | `5b0a339b10d4723177bcdf79163126b43fe6b2ebb98e5335b5dada7854703d9d` |
| `docs/plans/protected-response-policy.md` | `fa1aa9d4e1602f36d2eb841e1ad590fc1ccbd344ea5c02a1dcae8964d2f2db1d` |
| `docs/reviews/evidence/command-lookup-final-return.test.js` | `28cec08b858f5ca8ab0335007d8a1afcf3cbd63612ace70c5f21261f86875a11` |
| `docs/reviews/evidence/command-lookup-final-return.config.mjs` | `fa946f6c1fcec6da2e54f1c207a1ebddd9d730489121adface64bb0ea99d3355` |

## SHA-256 Worker artifacts

| Path | SHA-256 |
| --- | --- |
| `backend/build/index.js` | `40a1af52a746a9b13c6af4939ad39ce281f2ffe42dfce83c213cc9ea2360a281` |
| `backend/build/index_bg.wasm` | `0970f27cd948b73b05318c9f9eaef1b370cd5b23dde8f0b5d349c3cac646e9e2` |
| `backend/build/worker/shim.mjs` | `ac99177861405a6046923cbacabbd6adfa1853a56d0c679b9f5e09097c2f4728` |
| `target/wasm32-unknown-unknown/release/brews_backend.wasm` | `7d72f185c25b57654a3e54bed3d96abd817096fbf62c32126d678ca3ff7b5a69` |

## Safe execution metadata

```json
{
  "protected-response-snapshot-acceptance.json": {
    "command": [
      "/home/orangepi/Workspace/brews-bingo/backend/node_modules/.bin/vitest",
      "run",
      "worker-tests/protected_response_snapshot.test.js",
      "--reporter=json"
    ],
    "elapsed_wall_seconds": 44.61541179197957,
    "exit_code": 0,
    "raw_output_persisted": false,
    "stderr_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "stdout_sha256": "c5cddfbb51fd17ce933d594e0e01aa8d89bdf36b35756c7417f863835a90152a",
    "timed_out": false
  },
  "runner-unit-execution.json": {
    "command": [
      "python3",
      "-m",
      "unittest",
      "discover",
      "-s",
      "backend/tools/tests",
      "-p",
      "test_release_gate.py"
    ],
    "elapsed_wall_seconds": 1.2904265860270243,
    "exit_code": 0,
    "raw_output_persisted": false,
    "stderr_sha256": "f69c5274c870237059e175e32e557f80c2d5cf32ddc4f2bd7bcaaf93df84d722",
    "stdout_sha256": "453129de56bc6633243fcf75fdd88b5559d60066832d40cf5c0602317faaa921",
    "timed_out": false
  },
  "superseded-receiving-cut-probe.json": {
    "command": [
      "/home/orangepi/Workspace/brews-bingo/backend/node_modules/.bin/vitest",
      "run",
      "--config",
      "/home/orangepi/Workspace/brews-bingo/docs/reviews/evidence/command-lookup-final-return.config.mjs",
      "--reporter=json"
    ],
    "elapsed_wall_seconds": 6.089030653994996,
    "exit_code": 1,
    "raw_output_persisted": false,
    "stderr_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "stdout_sha256": "7e1f76c97fb436b5e5e727e5b2f73c56f9e2d66a1a638930b43e0f63b43a00b7",
    "timed_out": false
  }
}
```

## Exact accepted case inventory

- `RUNTIME-FINAL-01 G4 recovery after final cut`
- `RUNTIME-FINAL-01 G4 recovery before final cut`
- `RUNTIME-FINAL-01 SQL authoritative account epoch invalidation after final cut`
- `RUNTIME-FINAL-01 SQL authoritative account epoch invalidation before final cut`
- `RUNTIME-FINAL-01 cross-session final Exit after final cut`
- `RUNTIME-FINAL-01 cross-session final Exit before final cut`
- `RUNTIME-FINAL-01 dedicated board recovery after final cut`
- `RUNTIME-FINAL-01 dedicated board recovery before final cut`
- `RUNTIME-FINAL-01 dedicated membership recovery after final cut`
- `RUNTIME-FINAL-01 dedicated membership recovery before final cut`
- `RUNTIME-FINAL-01 dedicated privacy recovery after final cut`
- `RUNTIME-FINAL-01 dedicated privacy recovery before final cut`
- `RUNTIME-FINAL-01 delete-answer after final cut content`
- `RUNTIME-FINAL-01 delete-answer before final cut content`
- `RUNTIME-FINAL-01 original cap expiry after genuine final reply`
- `RUNTIME-FINAL-01 original cap expiry after genuine final reply with durable denial rollback`
- `RUNTIME-FINAL-01 rename after final cut content`
- `RUNTIME-FINAL-01 rename before final cut content`
- `RUNTIME-FINAL-01 stale /read challenge substitution is 503 and fresh revoked request is 401`
- `RUNTIME-FINAL-01 stale /read-release challenge substitution is 503 and fresh revoked request is 401`

Earlier diagnostic runs are not final acceptance evidence: the initial new suite ran 18 passed / 2 failed because its recovery successor comparison incorrectly required the old view revision; those expectations were corrected. An intermediate acceptance ran 20 passed / 0 failed, but its combined contract validation correctly rejected changed input hashes during editing. The final separate execution above ran against unchanged inputs and passed strict validation.
