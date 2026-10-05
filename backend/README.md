# Account auth backend

Local review slice: A1–A7 auth plus the restricted developer CLI account-management interface. No deployment, gameplay, UI or public admin-account routes. See the [repository README](../README.md) for CLI commands and local HTTPS setup.

## Read the code

- `src/worker_runtime/mod.rs`: Worker entrypoint and singleton AccountsObject.
- `src/worker_runtime/edge.rs`: Axum transport, private owner messages and cookies.
- `src/auth/`: account/session workflows, receipts, throttles and cleanup.
- `src/auth/management/`: shared management policy, audit, issuance and durable removal work.
- `src/worker_runtime/management.rs`: dedicated CLI authentication/transport.
- `src/worker_runtime/removals.rs`: Accounts-owned Directory reconciliation and recovery.
- `src/directory/`: minimal assignment gate and hosted-account guard; no gameplay ingress. Existing databases require the exact approved application-table inventory.
- `src/security/`: bounded Argon2id and bearer primitives.
- `src/storage/`: SQL port, preserved version-1 auth schema and additive version-2 management migration.
- `src/limits.rs`: backend timing, retention, throttling and transport limits.
- `../shared/domain/src/accounts.rs`: username/password length constraints.
- `../shared/domain`: typed IDs and pure account rules.
- `../shared/contracts`: safe public response types.
- `../shared/config/src/env`: env inventory, structs and one-time initialization.

Planning sources are in [`../docs/plans`](../docs/plans).

## Local verification

Prerequisites: pinned Rust toolchain, native Linux `mold`/`ld.mold`, Node/npm,
`cargo-nextest` and `worker-build 0.8.7`. Wasm keeps its own linker.

From the workspace root:

```sh
cargo nextest run --workspace --all-features
cargo test --workspace --all-features --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check -p brews-backend --target wasm32-unknown-unknown
cargo fmt --all --check
PUBLIC_API_PATH=/invalid cargo test -p brews-config --no-default-features --features frontend --test frontend_once_error
```

From `backend/`:

```sh
npm ci --ignore-scripts
npm test
```

`npm test` rebuilds Wasm before testing; `npm run test:worker` is the scoped test-only command.
Worker tests execute the built Rust Wasm under local workerd with SQLite Durable
Objects. Private fixtures seed ephemeral local account/link rows; there is no seed
route or deployed account. Test credentials and rate keys are not logged.

## Logging

`src/observability.rs` uses `tracing`, `tracing-subscriber` and `tracing-web` to emit one-line JSON to the Worker console. The subscriber initializes once per isolate, including direct Durable Object requests and alarms. It accepts only the `brews.security` target at INFO/WARN/ERROR, without span fields, ANSI output or application timestamps; Cloudflare supplies log timestamps. No performance/timing spans are enabled.

Logs cover credential/transport rejection, account-management storage failures, issuance versus receipt replay, private Directory protocol/proof failures, removal retries/abort intent and alarm/output-gate failures. Delivery and request completion diagnostics follow the final output gate. Removal retry/abort records describe local workflow decisions, not durable audit receipts. Ordinary session/account reads and route misses stay quiet.

Fields are closed event/boundary/reason/operation tags plus canonical command or removal-operation IDs where needed. Passwords, keys, bearer tokens/URLs, cookies, headers, request bodies, usernames, account/link IDs, IPs, SQL rows and raw error chains are excluded. Operational logs do not replace the transactional management audit.

View them during `wrangler dev`; production collection, retention and sampling remain account/deployment configuration, not provisioning done here. [Cloudflare's Rust crate guidance](https://developers.cloudflare.com/workers/languages/rust/crates/) documents the tracing/web-console integration. The logger omits timers because Worker time measurements have platform limitations.

## Configuration

The structs live in `../shared/config/src/env/{backend,frontend,cli}.rs`; common parsing helpers live in `env/utility/`. Their
constructors use envy and Serde fields: required values error when missing,
`Option` fields remain `None`, and `#[serde(default)]` supplies defaults.

Backend callers use `config::get_backend_config(&env)?` and read fields. Frontend callers use
`frontend::get_frontend_config()?`. Each accessor constructs its struct once and returns the
same reference afterward; initialization failures are cached. Cloudflare uses
runtime bindings; frontend `new()` captures only public build values. No OS-env
reads run in Wasm. Binding changes require a new deployment/isolate.

`wrangler.toml` contains only a local HTTPS origin. A local server also requires a
private `RATE_LIMIT_KEY` secret supplied through Wrangler's untracked `.dev.vars`
mechanism. Optional private `DEV_CLI_KEY` enables management and must differ from `RATE_LIMIT_KEY`; absence disables the CLI interface. No production secret is provisioned by this slice. Tests inject ephemeral keys in memory. Never add credentials to `[vars]`, docs, snapshots or source.

Auth bodies are bounded to 4 KiB; usernames and passwords follow the approved
10–50 ASCII policy. A1 and A3–A6 require canonical UUID-v7 `Idempotency-Key` headers.
Retries never replay credentials/cookies. Consumed links return generic invalid-link;
committed setup/reset with a lost response recovers through password login.

Production Argon2 capacity, quota forecasts and release gates remain unverified.
The default 19 MiB / 2 / 1 setting is a benchmark starting point, not deployment
approval. Socket subscriptions cannot be created by this slice. Close intents are
retained, but GameObject delivery, acknowledgements and retry/backoff are deferred.
Do not enable socket ingress until that sender is implemented; queuing an intent
does not establish socket closure.
