# Agent guidance

## Project state and planning sources

The user has approved all planning proposals and authorized the initial Cloudflare backend workspace and account-auth API slice (A1–A7). The initial auth slice is implemented and committed. The reviewed Rust developer CLI, shared account-management logic, restricted Worker interface, minimal Directory removal coordination and selective logging are implemented. The Users APIs and DB cleanup were committed and pushed as `6005c8f9314e25f3f4015db6ea70b395f3a886d8`; this does not authorize publication of later work. The user has authorized the next local slice: game creation/open-lobby/Start, required owner coordination and timeout cleanup, and the minimum player admission/session/WebSocket presence/synchronization foundation needed for successful public Start. Creation/Start and participant-foundation wire contracts GAME-SLICE-01–05 are approved in docs/plans/api-design.md; answer recovery and C7 are deferred. GAME-SLICE-06 transport-only snapshot acknowledgements and durable queue budgeting are approved. The full authorized local creation/Start slice is implemented and independently reviewed; native/optimized, Worker/Wasm and local HTTPS/WSS gates pass. The user has separately authorized committing and pushing this reviewed slice. Do not implement frontend screens, number calls or winner awards. Production deployment, infrastructure/accounts and secret provisioning remain unauthorized. B1–B9 are implemented and independently reviewed locally, wired through the Worker to shared account-management rules. The user has authorized committing and pushing the reviewed Users APIs and DB layout cleanup. Ask only about genuine gaps/contradictions. Production deployment, infrastructure/accounts and secret provisioning remain unauthorized. The user has authorized committing and pushing the reviewed CLI/management/logging changes and documentation. Future implementation changes require separate review and commit/push authorization. Auth password maximum is 50 ASCII characters (minimum 10); login, link redemption and setup/reset completion require a UUID-v7 Idempotency-Key header with secret-free receipts/no cookie replay. Logout is independently idempotent.

Use these documents as the planning sources of truth:
- `docs/plans/requirements.md` — business requirements and scope.
- `docs/plans/hld.md` — architecture and high-level policies.
- `docs/plans/lld.md` — domain rules, workflows, and implementation planning.
- `docs/plans/api-design.md` — API operations and request/response contracts.
- `docs/plans/durable-object-design.md` — storage schemas and the sequential decision ledger.
- `docs/plans/research.md` — hosting/provider evidence and caveats.

The schema review proceeds in ledger order. Batch as many independent pending approvals as possible in one `clarify` call, including across categories; ask dependent decisions separately, track each response independently, and reconcile revisions before marking downstream items approved. All 107 DO ledger items are approved; no pending schema decisions remain. DO-021 username and DO-045 per-game alias case-sensitive comparisons are approved. DO-032–107 decisions and cross-document summaries have been reconciled; implementation-only items remain unverified (production KDF costs/caps, exact Unicode/runtime support, SDK/SQL/outbox/alarm wiring, quota measurements and actual release-gate test results). Do not claim these as completed or tested. If a future `clarify` call times out, is cancelled, skipped, or otherwise returns no answer for an item, do not infer or supply an answer; leave it pending and wait for the user to return. Account lifecycle is `PendingEnrollment`, `Verified`, `ResetRequired`; `disabled_at` alone represents disablement, with no redundant stored boolean.

## Graphify

- Load the `graphify` skill and run it whenever code changes; refresh after each cohesive edit batch.
- Prefer incremental extraction. Verify source hashes before claiming the graph is current.
- Use `graphify-out/code/` for the current structural code graph. The older documentation graph remains stale until separately refreshed.
- Exclude dependencies, build outputs and secrets. Graph artifacts do not authorize commits or pushes.

## Environment variables

- Keep backend/private, frontend/public and CLI structs in `shared/config/src/env/`; parsing helpers belong in `env/utility/`.
- Use `new` with envy/Serde: required values error, optional fields use `None` or explicit defaults.
- Initialize each singleton once; use `get_backend_config` and `get_frontend_config`, then read fields.
- No custom `from_iter`, raw mirror structs or separate initialization API. Keep Wasm input adapters thin and errors redacted.

## Testing

Prefer nextest for native tests; run doctests separately. From the workspace root:

```sh
cargo nextest run --workspace --all-features
cargo test --workspace --all-features --doc
```

Also run nextest with `--cargo-profile release-assertions` for optimized validation. Worker/Wasm tests remain separate.

## Rust code

- Keep the CLI entry point thin; commands belong in `cli/src/operations/` and client test suites under `cli/tests/`.
- Follow `docs/rust-best-practices.md`; keep modules private unless callers need them.
- Run `cargo fmt --all` after code writing is finished, then `cargo fmt --all --check`.
- Use specific names and component-local typed errors; do not expose raw error chains.
- Name schema helpers and unreleased migration tags by purpose, not app-release labels such as `v2`. Persisted schema counters are storage metadata, not application releases.
- Keep the backend SQL port and Directory database operations in `backend/src/db/`; schemas belong in `db/schema/`, with authentication DDL in `auth_schema.rs`.
- Import DB services and functions directly from their `db` modules; domain module roots must not re-export DB entities.
- Replace repeated policy literals with named constants at their owning boundary. Include units and share constraints across validation/storage; unrelated rules remain separate.
- Bind SQL domain/state values from Rust enum tags or constants; do not duplicate quoted policy values or interpolate untrusted values into SQL. Name SQL statement constants by operation and predicate/purpose; avoid numeric suffixes used only to distinguish names.
- Keep comments concise: explain non-obvious constraints, not obvious code.
- Embed unit tests in a bottom `#[cfg(test)]` module rather than separate `*_tests.rs` files. Keep integration/HTTPS suites under the owning crate’s `tests/`.
- Test boundaries and failure paths; verify native and Worker/Wasm behavior before reporting completion.

Never expose or persist passwords, bearer tokens, or other secrets in docs, graph labels, logs, or examples.
