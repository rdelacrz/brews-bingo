# Brews Bingo web

Dioxus 0.7.10 client-rendered Rust, with a small web launcher in this directory and reusable application modules in `shared/ui`. The backend remains the authority for roles, game state, membership, revisions and expiry. No cloud credentials or backend config are compiled into the browser.

## Local build

Use the workspace Rust toolchain, Dioxus CLI **0.7.10**, and Node/npm. The checked-in Cargo/npm lockfiles bind dependencies.

```sh
python3 platform/web/tools/build.py
# Or pass --dx /path/to/dx for an isolated CLI installation.
```

This builds Tailwind from the shared UI and produces `target/dx/brews-web/release/web/public`. It writes restrictive static-asset headers including a hash for the generated loader, same-origin connections, no referrer and no-store responses. No CDN fonts, third-party scripts or service worker are used.

`PUBLIC_APP_NAME` and `PUBLIC_API_PATH` use the existing `brews_config::env::frontend::get_frontend_config()` singleton. They are **build inputs**, not runtime secrets. `/api` is the only accepted API root; the browser uses same-origin credentials and never reads HttpOnly cookies. Rebuild when public inputs change. Private Worker/CLI configuration remains in its existing scopes.

## Local Cloudflare integration

Build the existing API Worker and CLI first using the backend instructions, then:

```sh
python3 platform/web/tools/local_worker.py
```

The helper creates a loopback-only HTTPS Wrangler instance with an ephemeral SQLite database, test-only keys and a temporary certificate. It uses the existing API bindings/migrations plus Workers Static Assets. `/api`, `/api/*` and `/_dev/*` execute Worker code before asset fallback; frontend paths use SPA fallback. Stopping the process removes the temporary state. A local certificate warning is expected.

No existing account, remote namespace, secret, domain or deployment is modified. `configuration()` generates the combined configuration for local verification; it is **not** an approved production deployment configuration. The existing API promotion pipeline does not yet package/approve browser assets: frontend staging/promotion acceptance remains a separate release gate.

## Checks

```sh
cargo nextest run --workspace --all-features --locked
cargo nextest run --workspace --all-features --locked --cargo-profile release-assertions
cargo test --workspace --all-features --doc --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo clippy -p brews-web --target wasm32-unknown-unknown --locked -- -D warnings
python3 -m unittest discover -s platform/web/tools -p 'test_*.py'
python3 platform/web/tools/browser_acceptance.py --output "$TMPDIR/brews-web-browser-new-run"
# Against a running local HTTPS preview; screenshots contain public Home only:
node platform/web/tests/theme.mjs https://localhost:PORT "$TMPDIR/brews-theme-new-run"
```

Browser acceptance uses a local Worker, real secure cookies and backend state, with a temporary admin created by the local-only CLI. Normal flows do not substitute mocked API data. One explicitly named fault-injection case holds a membership read while a real replacement socket closes the old connection, then releases the read with 503; another seeds a rejected, test-only spectator credential to verify cleanup. Only controlled check names, counts, non-sensitive transport statuses/booleans and public Home screenshots are retained; no private boards, credentials or enrollment links are written into reports. The output directory must be new and outside the repository.

## Organization and visual system

- `shared/ui/src/client`: bounded same-origin requests, redacted errors and original-command retry identity.
- `shared/ui/src/session.rs`: account-only session context, expiry and identity-change clearing.
- `shared/ui/src/features`: Home/dashboard, account auth, Game lifecycle/role variants, recovery, Users and History.
- `shared/ui/src/components`: labelled forms/status, native confirmation dialogs, read-only boards/call lists.
- `shared/ui/assets/theme.css`: matches the official [Rockville Brews site](https://www.rockvillebrews.com/) palette: espresso `#210C03`, cream `#F6F4EF`, gold `#EDAD38`, with dark teal venue views. Cormorant Garamond headings and DM Sans body text are self-hosted Latin-subset variable WOFF2 fonts; system fallbacks cover other scripts. Gold actions use dark text, while links use the darker gold-ink color for contrast. Font licenses and provenance live in `public/assets/fonts/`. Tailwind scans shared Rust views.
- `platform/web/src/main.rs`: launch only; no business rules or privileged configuration.

Player and spectator sessions remain separate from host/admin account sessions. Frontend routing never confers backend authority. Anonymous entry uses an eight-character code, then explicit Play/Spectate admission. The browser never generates boards, marks cells, awards winners, renews sessions or queues offline gameplay.
