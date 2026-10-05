# Brews Bingo

Brews Bingo is a bingo app for Rockville Brews. This repository currently contains the Cloudflare authentication backend and a Rust developer CLI for account administration. Gameplay and the browser interface are not implemented yet.

## Repository map

- `backend/` — Worker routes, AccountsObject, shared account-management rules and minimal Directory coordination.
- `cli/src/operations/` — concrete developer CLI commands; `main.rs` is the entry point.
- `cli/tests/` — HTTPS client and binary integration test suites.
- `shared/domain/` — account rules and typed identifiers.
- `shared/contracts/` — transport DTOs; management contracts are explicitly feature-gated.
- `shared/config/src/env/` — typed environment settings; parsing helpers live in `utility/`.
- `docs/plans/` — requirements, architecture, API and storage plans.

The CLI calls the restricted Worker interface. It does **not** open SQLite directly. Future admin APIs reuse the backend's account-management service rather than duplicate its rules.

## Tools and builds

Use the Rust toolchain pinned in `rust-toolchain.toml`. Native Linux builds use mold via `.cargo/config.toml`; install mold and make `ld.mold` available. Worker builds also need Node/npm and `worker-build`.

```sh
rustup target add wasm32-unknown-unknown
cargo install cargo-nextest --locked
cargo install worker-build --version 0.8.7 --locked
npm --prefix backend ci

# Native crates and the CLI
cargo build --workspace
cargo build -p brews-cli --release

# Actual Cloudflare Worker/Wasm bundle
npm --prefix backend run build
```

## Developer CLI

Use `cargo run -p brews-cli --` below, or build once and use `target/release/brews`.

```sh
cargo run -p brews-cli -- --help
cargo run -p brews-cli -- accounts --help
```

### Configuration

The Worker requires `APP_ORIGIN` (canonical HTTPS origin) and `RATE_LIMIT_KEY` (a private 32-byte key encoded as unpadded base64url). Its optional `DEV_CLI_KEY` enables the CLI interface and must be a **different** private key. Without it, that interface is disabled. Do not commit secrets or pass keys on the command line.

The CLI reads:

| Variable | Use |
| --- | --- |
| `BREWS_API_ORIGIN` | Required canonical HTTPS origin for the Worker. |
| `BREWS_DEV_CLI_KEY` | Required private key matching that Worker's `DEV_CLI_KEY`. Supply through private environment configuration. |
| `BREWS_TLS_CA_FILE` | Optional PEM CA certificate for trusted local HTTPS. No TLS verification bypass. |

For an already configured endpoint:

```sh
export BREWS_API_ORIGIN=https://your-configured-worker.example
# Load BREWS_DEV_CLI_KEY through your private environment/secret tooling.
# Local HTTPS may additionally require BREWS_TLS_CA_FILE.
```

Local development uses ignored `backend/.dev.vars` for Worker secrets and a trusted localhost certificate:

```sh
cd backend
npx wrangler dev --local --local-protocol https \
  --https-key-path /private/path/localhost-key.pem \
  --https-cert-path /private/path/localhost-cert.pem
```

Use `https://localhost:8787` for the local CLI origin and the corresponding CA file. Set `APP_ORIGIN` to the intended enrollment/reset application origin; it need not equal the API origin. This is local execution only, not deployment or credential provisioning. Fresh mutation IDs must be within the 24-hour admission window. Existing link-issuance receipts remain replayable for 30 days without replaying their URL. Password KDF tuning and production capacity remain release gates.

### Account commands

```sh
# List accounts or inspect one using its canonical UUID-v7 ID
cargo run -p brews-cli -- accounts list --limit 50
cargo run -p brews-cli -- accounts get "$ACCOUNT_ID"

# Create the initial admin or a host; the recipient completes enrollment
cargo run -p brews-cli -- accounts create --role admin \
  --username RockvilleAdmin --link-output /private/path/admin-enrollment.url
cargo run -p brews-cli -- accounts create --role host \
  --username RockvilleHost --link-output /private/path/host-enrollment.url

# Replace an enrollment link, or initiate/reissue a password reset
cargo run -p brews-cli -- accounts reissue-enrollment "$ACCOUNT_ID" \
  --link-output /private/path/replacement-enrollment.url
cargo run -p brews-cli -- accounts reset-password "$ACCOUNT_ID" \
  --link-output /private/path/password-reset.url

# Destructive operations require explicit confirmation
cargo run -p brews-cli -- accounts disable "$ACCOUNT_ID" --yes
cargo run -p brews-cli -- accounts enable "$ACCOUNT_ID"
cargo run -p brews-cli -- accounts delete "$ACCOUNT_ID" --yes

# Developer-only privileged action audit
cargo run -p brews-cli -- audit list --limit 50
```

The CLI rejects non-object responses and malformed links before writing a file. Enrollment/reset links must use their purpose-specific HTTPS path and a canonical 32-byte token in the fragment, never the query.

Link files must be new and are created with owner-only permissions (0600). The CLI never prints bearer URLs. Treat these files as credentials: deliver them privately and remove them when no longer needed. A lost issuance response cannot recover the old URL; explicitly reissue to invalidate it and create a successor.

Mutations print their command ID before sending. For an uncertain response, repeat the **same** operation with `--command-id "$COMMAND_ID"`; do not invent a new ID to retry. Link commands must use a **new output path** on every retry: a failed attempt or receipt replay may leave an empty private reservation. Committed retries return a secret-free receipt, not another URL. The CLI does not automatically retry or follow redirects. Pending coordination is not completed deletion/revocation; unresolved work is retained for recovery.

Usernames are case-sensitive, 10–50 ASCII characters after boundary whitespace trimming, with no internal whitespace. Account roles are set at creation, not edited later. The CLI enforces the hosted-game and last-enabled-verified-admin removal guards too.

## Tests and checks

Run from the repository root:

```sh
cargo nextest run --workspace --all-features
cargo nextest run --workspace --all-features --cargo-profile release-assertions
cargo test --workspace --all-features --doc
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy -p brews-backend --target wasm32-unknown-unknown -- -D warnings

# Builds fresh Wasm, then tests in the actual local Worker runtime
npm --prefix backend test
```

Native tests cover shared rules, SQLite transactions, CLI parsing/output and HTTPS client behavior. Worker tests separately exercise the Wasm adapters, durable storage, alarms and restricted transport. No production deployment is required.
