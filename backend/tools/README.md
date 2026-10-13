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
| `worker` | Fresh optimized Worker build; every `backend/worker-tests/**/*.test.js`; exact physical file/case inventory and totals, rejecting duplicates, omissions, skips and runtime errors. |
| `tls` | Fresh native CLI plus the existing gameplay, operator, dedicated-read/G4 and privileged-audit real local HTTPS/WSS harnesses. Requires the complete Worker build to exist. |
| `contract` | The **unchanged** retained `RUNTIME-FINAL-01` G4 final-return acceptance probe. Requires the complete Worker build to exist. Runs separately from positive Worker tests. |

Standalone TLS/contract lanes rebuild the Worker before exercising it; complete runtime artifact hashes are frozen after that build. Separate lanes' source and artifact maps must still be reconciled before combining results. Prefer `all` for one frozen cut. Do not edit source, test/probe inputs or Worker artifacts during a run. Drift is a failure, not a new passing checkpoint.

## Exit and evidence semantics

- `0`: all **requested local** checks passed. This is never production certification, protected-response policy approval or acceptance of unchecked slices.
- `1`: a command, evidence parser, inventory, input/artifact check or prerequisite failed. An unrelated probe setup failure is not relabeled a reproduced final-return finding.
- `2`: the retained final-return acceptance assertion executed and failed at its known HTTP-200 denial boundary. This is a real failed acceptance gate, **not** an expected passing test or permission to deploy.

`report.json` always says `production_release: "blocked"` and enumerates outstanding acceptance/provider/forecast/provisioning gates. Even a future passing probe is only an observation; this tool cannot choose a protected-response contract or accept it on the user's behalf. No production-ready flag or bypass option exists.

The report binds source/schema/config/test/probe inputs to SHA-256, checks their before/after equality and records actual Worker artifact hashes. There is no inferred numeric capacity limit, forecast or provider quota. Original failed review/probe evidence remains unchanged.

## Local measurements

`backend/tests/local_readiness.rs` measures the exact Directory History query invoked by the production service, including SQLite VM-step/sort/full-scan counters and the corresponding bound query plan, at fixed metadata populations 100/1,000/10,000. This is a local index-population experiment, **not** an owner peak forecast, provider rowsRead/CPU charge or maximum-game capacity benchmark. A fixed page size does not imply fixed SQL work.

The native target also samples current explicit Argon2 hash/verify and real recovery-owner paths with ephemeral OS randomness. `backend/worker-tests/local_readiness.test.js` exercises actual local Workerd recovery routes for the same selected normalization cases, current clock observations and session distinctness. The samples are single sequential wall-time observations, not percentiles, concurrent throughput, peak memory, CSPRNG-quality certification or deployed runtime validation. The normal native and Worker suites execute these cases; no ignored benchmark is substituted for a gate.

## Operations prerequisites

[Expiry-preserving restore/backout worksheet](../../docs/operations/restore-and-backout.md) specifies quarantine, original deadlines, clock/authority fences, schema compatibility and reconciliation stop conditions. It is not a tested provider restore script. Provider backup/log/copy controls, isolated environments, provisioning/secret delivery, deployed integration/load/security checks and two-times quota headroom still need separate authorization and actual target evidence. See [Slice 9 backlog](../../docs/plans/PENDING.md#slice-9--production-databaseruntime-and-release-gates).
