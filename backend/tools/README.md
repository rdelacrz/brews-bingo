# Local backend release checks

`release_gate.py` is a provider-neutral runner for local checks, not a deployer or a production approval mechanism. It does not install tools, alter configuration/secrets, select a CI provider, provision resources or call a remote Cloudflare account.

## Run

Use the pinned Rust toolchain in `rust-toolchain.toml`, locked Cargo/npm dependencies already installed, Python 3.11+, and a private scratch `TMPDIR`. From the repository root:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s backend/tools/tests -v
PYTHONDONTWRITEBYTECODE=1 python3 backend/tools/release_gate.py \
  --output "$TMPDIR/brews-release-check-01"
```

The output directory **must not already exist** and must be outside the repository. Choose a new name for each run; do not delete or overwrite failed evidence. The directory is private, and reports have mode `0600`. The runner never writes raw child stdout/stderr or raw assertion payloads; command records contain exit codes, elapsed wall time and output hashes. Structured inventories exclude failure messages and credential fields; arbitrary case/check labels are persisted only as SHA-256 identifiers. Failed checks require a controlled local reproduction, not publication of raw credential-bearing logs.

`--lane native|worker|tls|contract` selects one lane. Default `all` runs them in order. Worker execution uses 1–3 complete shards (default 3), isolated caches and unchanged per-test deadlines. `--deadline SECONDS` bounds each child command (default 1800, allowed 30–3600); timeout kills its process group. No background server is intentionally left running by this runner.

| Lane | Checks |
| --- | --- |
| `native` | Exact pinned Cargo; formatting; complete workspace/all-feature/locked nextest in debug and optimized `release-assertions`; separate doctests; strict native and Worker/Wasm Clippy. No test skips are accepted. |
| `worker` | Fresh optimized Worker build; every general `backend/worker-tests/**/*.test.js` except the snapshot suite assigned to `contract`; exact physical file/case inventory and totals, rejecting duplicates, omissions, skips and runtime errors. Standalone `--lane worker` does not run those 20 policy cases; the full `all` gate does. |
| `tls` | Fresh native CLI plus the existing gameplay, operator, dedicated-read/G4 and privileged-audit real local HTTPS/WSS harnesses. Requires the complete Worker build to exist. |
| `contract` | Mandatory new `RUNTIME-FINAL-01` authorization-time snapshot acceptance, with exact complete case inventory. Executes the **unchanged** older receiving-cut G4 probe separately and preserves its physical assertion/exit as a superseded historical observation. Both require the complete Worker build. The new suite is excluded from the general Worker lane to avoid double counting, not from the full gate. |

Standalone TLS/contract lanes rebuild the Worker before exercising it; complete runtime artifact hashes are frozen after that build. Separate lanes' source and artifact maps must still be reconciled before combining results. Prefer `all` for one frozen cut. Do not edit source, test/probe inputs or Worker artifacts during a run. Drift is a failure, not a new passing checkpoint.

## Exit and evidence semantics

- `0`: all **requested local** checks passed. This is never production certification, protected-response policy approval or acceptance of unchecked slices.
- `1`: a command, evidence parser, inventory, input/artifact check or prerequisite failed. An unrelated probe setup failure is not relabeled a reproduced final-return finding.
- `2`: the mandatory **current** authorization-time snapshot acceptance failed. The original receiving-cut assertion's separately recorded failure remains genuine historical evidence, but no longer vetoes the user's explicitly approved policy. Unrelated historical setup/assertion failures still block with exit 1; the parser does not turn arbitrary failures into historical observations.

`report.json` always says `production_release: "blocked"` and enumerates outstanding independent-review/provider/forecast/provisioning gates. The user explicitly approved [authorization-time snapshots](../../docs/plans/protected-response-policy.md) on 2026-10-14; this tool verifies that chosen contract rather than choosing it. A successful `contract` result is `passed_authorization_time_snapshot`, binds policy/test/historical-probe input hashes, and nests the original physical outcome under `superseded_receiving_cut_probe` without relabeling it passed. No production-ready flag or bypass option exists.

The report binds source/schema/config/test/probe inputs to SHA-256, checks their before/after equality and records actual Worker artifact hashes. There is no inferred numeric capacity limit, forecast or provider quota. Original failed review/probe evidence remains unchanged. After the player-recovery route moved to `/api/games/player-recovery`, `historical_probe.config.mjs` adapts only that URL in the archived probe at test-transform time; the archived files, assertions and failure classification remain unchanged. This test-only adapter is part of the frozen source/config inventory, not a runtime alias.

## Local measurements

`backend/tests/local_readiness.rs` measures the exact Directory History query invoked by the production service, including SQLite VM-step/sort/full-scan counters and the corresponding bound query plan, at fixed metadata populations 100/1,000/10,000. This is a local index-population experiment, **not** an owner peak forecast, provider rowsRead/CPU charge or maximum-game capacity benchmark. A fixed page size does not imply fixed SQL work.

The native target retains functional recovery-owner checks with ephemeral OS randomness. `backend/worker-tests/local_readiness.test.js` exercises actual local Workerd recovery routes for the same selected normalization cases, current clock observations and session distinctness. Dedicated Argon2 hash/verify and recovery-hashing timings have been removed under the [approved default policy](../../docs/plans/argon2-default-policy.md); the backend already uses the pinned library's Argon2id v19 / 19 MiB / 2 / 1 defaults. Password/recovery correctness, salts and bounded PHC validation remain tested. No separate hashing-performance certification gate remains. These checks do not certify CSPRNG quality or deployed runtime behavior.

The next local [maximum-configured-game resource probe](../../docs/operations/game-resource-measurements.md) uses `game_resource_probe.py --output NEW_PRIVATE_DIRECTORY`, with actual native service operations and sampled local Workerd socket traffic. It preserves exact branch/sample/stage identities and independently validates state cardinalities, page-size equations and observed frame/ACK totals. Native full-pool observations and Workerd sampled-call observations are distinct; unavailable SDK row-work counters are explicitly pending, not reported as zero. The full native/Worker suites include these tests, but neither probe closes the owner-forecast/provider-capacity gate.

## Operations prerequisites

The [CI/CD guide](../../docs/operations/cicd.md) covers explicit staging/production isolation, frozen artifact promotion, exact serving-version readback, staged full-game evidence and required independent manual production approval. These are local definitions, not provisioned or deployed environments.

`backend/tests/full_game_e2e.py` targets an **existing** verified HTTPS staging or explicit loopback Worker. It creates only a least-privilege throwaway host and its own games, uses independent player/spectator cookie jars and continuously reading real WSS clients, checks actual randomly assigned boards with a test-only oracle, exercises Start/random/manual/winner/History/release and exact command replays, then removes its own host and verifies absence. It never starts a server, resets a database, cancels someone else's game or runs mutating gameplay in production. History remains until its contractual expiry. Credentials stay in environment; output is a new private directory with redacted labels/counts only.

```sh
python3 backend/tests/full_game_e2e.py --origin "$BREWS_API_ORIGIN" \
  --expected-origin "$BREWS_API_ORIGIN" --environment staging \
  --output "$TMPDIR/brews-full-game-01" --bootstrap-cli /path/to/verified/brews
```

The environment label/origin agreement is not namespace proof: deployment tooling must qualify the exact serving version and namespaces before and after a staging run. Local mode additionally accepts `--tls-ca-file TRUSTED_CA`, never disables TLS verification, and is not reported as deployed staging.

[Expiry-preserving restore/backout worksheet](../../docs/operations/restore-and-backout.md) specifies quarantine, original deadlines, clock/authority fences, schema compatibility and reconciliation stop conditions. It is not a tested provider restore script. Provider backup/log/copy controls, isolated environments, provisioning/secret delivery, deployed integration/load/security checks and two-times quota headroom still need separate authorization and actual target evidence. See [Slice 9 backlog](../../docs/plans/PENDING.md#slice-9--production-databaseruntime-and-release-gates).
