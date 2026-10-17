# API module layout refactor — local verification, 2026-10-17

## Changes and scope

The user requested these source moves under `backend/src/api/`:

- `decode.rs` → `authentication.rs`, with `AuthenticationRequest`, `AuthenticationOperation`, `AuthenticationPayload` and `decode_authentication` replacing generic authentication names.
- `games.rs` → `games/writes.rs`.
- `game_reads.rs` → `games/reads.rs`.
- Add private `games/mod.rs` to organise exports; retain existing game API exports at the API root and keep the read/write implementation modules private.

All callers, cookie-helper references, nested-module imports and relevant visibility boundaries are updated. Mechanical comparisons against the unchanged pre-refactor source, using only the expected names/import paths and rustfmt, pass for all seven affected implementation files. HTTP routes, bodies, validation, authentication/authorization rules, cookies, expiry and game behavior are unchanged. Existing tests move with their modules; no tests are disabled or weakened.

The user subsequently authorized commit/push and separately requested deletion of `backend/src/directory/README.md`. That file is removed. No Rust source embeds it, and the deletion does not change the release runner's hashed inputs. Other baseline files, including the previously verified local Argon2-policy changes and their historical evidence, are preserved unchanged apart from the intentionally refreshed code graph. Publication includes those pending Argon2 changes under the user's separate commit/push authorization.

## Executed verification

The fresh complete local runner exits **0** with **297 source/config/policy inputs** and all four runtime artifacts stable through execution:

- **959 native tests in each profile**, with no skips/failures.
- **12 doctests**, formatting and strict native/Wasm Clippy pass.
- Fresh optimized Worker and native CLI builds pass.
- **723 general Workerd cases in 39 physical suites**, plus **20 mandatory snapshot-contract cases** in their separate suite, pass.
- **373 actual local HTTPS/WSS checks** pass across gameplay, operator, dedicated-read and audit harnesses.
- **135 Python tests** pass, including a repeat captured as a safe, unique case-hash inventory.
- Native all-target and Wasm `cargo check` pass.
- The refreshed structural graph verifies **265 current code-source hashes**, removes the old paths and includes the new module index. Recorded AST edge limitations remain; the documentation graph is not refreshed.

The unchanged old receiving-cut probe retains raw exit **1** as historical evidence under the superseded policy. Mandatory current snapshot acceptance passes; the historical result is not relabeled as a pass.

[Machine-readable evidence](api-layout-refactor-2026-10-17-evidence.json) binds the full runner report, command descriptors, source/artifact maps, mechanical comparison and safe Python inventory. A scratch evidence-recording attempt lacked `rustfmt` on its PATH after user steering; rerunning with the explicit pinned tool environment recorded the already-passing full gate. No application/test source was changed for that retry.

## Boundaries

This checkpoint does not close the remaining independent slice reviews or seven external production gates. No provider resources, secrets, deployment, actual staging test or frontend changes occurred. The previously published baseline is `2992962a20a512422ee5ec1796e9fdebc89f4a33`. This is the pre-commit verification record, not a production certificate; remote commit equality is checked separately after the authorized push.
