# Brews Bingo — Low-Level Design Template

## 1. Document status and use

- **Status:** Incremental LLD with confirmed single-API-Worker direction (LLD-016), UUID v7 application-generated IDs (LLD-020), component-local `thiserror` errors (LLD-021), the account-password length/ASCII policy (LLD-023), and salted Argon2 account-password hashing via RustCrypto `argon2` (LLD-024). Proposed Durable Object fields/types now live in [durable-object-design.md](durable-object-design.md) (LLD-018, LLD-025). All detailed API/Request/Response/WSS information now lives in [api-design.md](api-design.md) (LLD-017, LLD-019, LLD-022). Final acceptance of other proposals, physical schemas, wire contracts and unselected implementation details remains **TBD**.
- **Business:** Rockville Brews.
- **Primary source:** [High-level design](hld.md), including decisions HLD-001–HLD-076.
- **Supporting sources:** [Business requirements](requirements.md) and [hosting research](research.md).
- **Companion designs:** [API design](api-design.md) is the single home for API organization, operations, requests/responses and WSS contracts. [Durable Object design](durable-object-design.md) is the single home for storage ownership, schema proposals, records/fields/types and schema worksheets. This LLD links to both rather than duplicating those specifications.
- **Scope:** Structure the detailed design for the initial desktop/mobile web release on Cloudflare. Native Android/iOS implementation remains deferred.
- **Approval boundary:** Creating this template does not approve the overall HLD, resolve open product questions, authorize implementation, or authorize infrastructure/account creation.
- **Reviewer / approval / revision:** TBD.

### 1.1 How to complete this template

**Confirmed input** means a constraint carried from the HLD or a subsequent explicit user direction recorded in the LLD decision register. **Proposed** means a suggested design recorded for review, not final adoption. **TBD** explicitly identifies an undetermined choice or unfinished specification. Inventory labels and document IDs are planning references, not implemented resources. [API design](api-design.md#operation-catalog) records proposed Rust functions, methods/paths, scopes and Request/Response inputs/outputs; [Durable Object design Section 6](durable-object-design.md#schema-proposal) records proposed Object owners, fields, types and supporting enums. UUID v7 and the local `thiserror` convention are confirmed in Sections 3.3–3.4; account-password policy and salted Argon2/library selection are confirmed in Sections 9.1–9.2; other proposed details do not become finalized APIs, physical schemas or protocols.

For each view, stored record, API operation and event, complete its specification using the relevant reusable worksheet. Preserve the confirmed constraints while choosing implementation details. Any intentional change to an HLD rule needs an explicit decision rather than being hidden in a schema or payload.

Proposed function names/paths and input/output fields appear in [API design](api-design.md#operation-catalog), and Rust-style record/type sketches in [Durable Object design Section 6](durable-object-design.md#schema-proposal). They are non-executable design proposals. UUID version/generation library, error library/local placement and salted Argon2/password-hashing library are selected; exact dependency versions, final API contracts, SQL DDL, HTTP statuses, serialized encodings, function signatures and Cloudflare bindings remain **TBD**.

### 1.2 Source precedence and reconciliation

Use explicit later HLD decisions over older source wording:

- HLD-020 specifies online-only gameplay, superseding BR-016/D-005 offline continuity.
- HLD-037 makes **Resolved** winner-only and **Cancelled** the no-winner terminal state.
- HLD-038–HLD-058 settle configuration, attendance, capacity, alias and code rules that some older passages still describe as open.
- HLD-062 permits all enrolled hosts to inspect live boards read-only; HLD-074 extends those capabilities to admins. Earlier transfer wording must not imply exclusive board-read access.
- HLD-069 supersedes spectator terminal reconnect: only the already-delivered local result remains after spectator server data deletion. HLD-071 separately governs player final-view authorization.
- HLD-063 deferred password policy to LLD; LLD-023 settles the minimum length and ASCII/composition rules. LLD-024 now selects salted Argon2 via RustCrypto `argon2`, superseding the hashing deferral. Older hashing-TBD wording in the HLD is superseded for that selection; [API design](api-design.md) follows the selected algorithm/library. Remaining profile/runtime details stay **TBD** in Section 9.2.
- HLD-073–HLD-076 establish **host** and **admin**, admin cross-game controls, privileged provisioning and the admin-only **Users** view. Older host-only provisioning/account descriptions are not an additional restriction on admins.
- BR-015's designated-host-only rule is qualified by HLD-074's admin override. Board, History and platform scope also have later HLD refinements.

Source-document wording reconciliation remains separate; this template does not edit or claim full consistency of those documents. Any ambiguity not explicitly settled remains `TBD` in Section 12.

## 2. Confirmed design baseline

### 2.1 Architecture and domain constraints

| Area | Confirmed input | Source |
| --- | --- | --- |
| Frontend | Rust/Dioxus, client-rendered web first; all approved app features on desktop/mobile browsers. Latest stable versions are rechecked and pinned later, not chosen here. | HLD Sections 4.1–4.3; HLD-003, HLD-019 |
| Backend | One Rust Cloudflare API Worker initially, using `workers-rs`, serving multiple endpoints through organized handlers. Managed server-side authority, not one Worker per endpoint or a conventional native server process. | HLD Sections 3, 5; LLD-016 |
| Delivery | Workers Static Assets; HTTPS commands/authentication and authenticated WSS snapshots/updates. SSR/fullstack server functions are not selected. | HLD Sections 4.2, 7.1 |
| Durable ownership | Game Directory Durable Object for discovery/global coordination; one SQLite-backed Game Durable Object per game for gameplay/memberships/boards. Account-store placement remains **TBD**; [Durable Object design Section 6](durable-object-design.md#schema-proposal) proposes a separate singleton Accounts Object without adopting it as confirmed HLD input. | HLD Sections 3, 5, 6; HLD-005, HLD-020 |
| Global limits | Exactly one nonterminal New/Awaiting Players/In Progress game application-wide. Claim the sole Directory reservation at New creation, keep it through start, and release after Resolved/Cancelled; terminal History is excluded. Separate Objects are not a shared transaction. | HLD-012, HLD-039 (superseded), HLD-077 |
| Exact lifecycle | **New → Awaiting Players → In Progress → Resolved** for a winner. **New**, **Awaiting Players** or **In Progress → Cancelled** for a no-winner ending. Neither terminal state reopens. | HLD-027, HLD-036–HLD-038 |
| Configuration and start | Configuration changes only in New; no Awaiting Players → New. Start needs at least two distinct currently connected players with valid sessions and the same global reservation still held by this game. Spectators never gate start. | HLD-038, HLD-040, HLD-048, HLD-077 |
| Capacities | Integer player limit 2–20, default 20; spectator limit 0–50, default 50. Zero spectators disables spectator admission. | HLD-018, HLD-046–HLD-048 |
| Values and boards | String values; initial pool `1..=numeric_upper_bound`, with positive upper bound 1–1,000 inclusive and default 75 (DO-042/043). At most 1,000 distinct accepted call records/game; no preallocated pool array. DO-044: for `N` values and `k = side² − free cells`, require `N ≥ k` and at least as many layouts `P(N,k)` as all retained start-time memberships (count capped at roster size); disconnected retained players receive boards. The separate two-connected-player Start gate remains. Square side 2–10, default 5; generated boards contain no repeated value and are cell-for-cell distinct. | HLD-025, HLD-033–HLD-035; BR-025, BR-028, BR-029 |
| Free cells and assignment | Optional valueless satisfied cells, enabled by default with one cell at one-based `(ceil(height / 2), ceil(width / 2))`. Locations configurable up to the whole board. DO-044 counts only non-free cells in `k`; reject Start if enough distinct boards are impossible. Assign/persist boards at accepted start, never on lobby entry/recovery. | HLD-034, HLD-035; DO-042 approves the GameConfiguration representation/defaults and disabled-empty-position invariant; DO-044 approves the exact feasibility criterion. |
| Progression and winner | Backend-only automatic matching after accepted random/manual undrawn values; players never mark. Single Line means a complete row, column or either full corner-to-corner diagonal. Separate Rust traits per winning pattern; signatures TBD. Qualification is not an award; an authorized operator submits one qualifying winner after the real-life Bingo exchange. | HLD-023, HLD-024, HLD-026, HLD-042, HLD-074 |
| Durable commands | Persist mutation, automatic board/qualification effects, revision and command result before acknowledgement/broadcast. Lost responses do not justify a new random call; no offline command queue. | HLD Sections 6.2, 7.6, 8 |
| Game codes | Exactly eight generated uppercase ASCII letters A–Z/digits 0–9; case-insensitive input, trim outer ASCII spaces, reject internal spaces. Valid while Awaiting Players/In Progress. Reuse only when absent from live games and unexpired History; stable identities prevent old-credential transfer. | HLD-056–HLD-058 |
| Aliases | Nonempty, at most 20 ASCII characters after outer-space trim; printable letters/digits/punctuation, internal spaces preserved. Per-game uniqueness and recovery are exact case-sensitive matches on the trimmed alias (`Alice` differs from `alice`), approved per latest user revision. Rename only before start; recovery uses the exact current alias. Old aliases release after rename/pre-start Leave/player-to-spectator switch. | HLD-049–HLD-055; DO-045 |
| Recovery and transport | Online-only, snapshot-first, role-filtered revision-aware WSS, hibernation-safe expiry; reconnect backoff capped at 30 seconds. No new board or slot on restoration. | HLD-020, HLD-022; HLD Sections 7.6, 8 |
| Hosting budget | Cloudflare ongoing free allowances are the baseline, not evidence of tested capacity or unlimited hosting. Quotas/tool compatibility must be checked during detailed design and verification. | HLD Sections 2, 10, 11 |

### 2.2 Roles and authorization inputs

An account's **type** is distinct from the **designated-host assignment** on a game. Admin override changes authorization, not ownership or game validity rules.

| Actor / state | Confirmed access | Boundary |
| --- | --- | --- |
| Developer CLI operator | Privileged account creation/link/reset/management; bootstrap the first admin. | Developer-only credential-bearing operational surface; no cloud credentials in app clients. |
| Restricted enrollment/reset session | Complete its specific password-setup/reset flow. | No normal host/admin permissions before completion. |
| Host account | Create games; mutate its designated games; inspect other games, live boards/qualifiers and unexpired History read-only. | No Users/account management or mutations in other hosts' games. |
| Admin account | All host capabilities, account/link/reset management, Users, and otherwise-valid game actions without being designated host. | Same lifecycle, value, qualification, capacity, confirmation, terminal and retention guards; no anonymous-player recovery bypass. |
| Player membership | Own read-only board/qualification, permitted lobby identity/role changes, Leave and approved recovery. | No manual marks, new player admission after start, other private boards or History entitlement. |
| Spectator membership | Audience state/calls/objective/result under lifecycle and capacity rules. | No private boards, qualification details, gameplay controls, Users or History entitlement. |

Only the developer CLI or already-existing fully enrolled admins may create admin accounts. Users lists provisioned **admin/host accounts**, not anonymous game memberships, and excludes credential/verifier material (HLD-073–HLD-076).

Authorization implementation, role representation, permission helpers and audit format: **TBD**. Transport/auth-scope contracts are maintained in [API design](api-design.md#shared-contracts); the rules above are cross-cutting domain constraints, not a duplicate API specification.

### 2.3 Attendance, exit and retention inputs

| Subject | Confirmed behavior | Mechanism |
| --- | --- | --- |
| Player before start | DO-047 approves explicit Leave releases seat/alias, deletes recovery data/session, and makes return a fresh eligible admission. Accidental disconnect retains membership/seat until start/cancellation; retained disconnected members receive start-time boards but do not satisfy the connected minimum. | DO-047 policy approved; physical transaction and stale-command fencing remain TBD. |
| Player during play | DO-047 approves Leave/disconnect retains seat, board, matching and winner eligibility. Return uses a still-valid session until its fixed expiry or approved answer proof; no replacement/new player admission. DO-048 keeps the latest explicit-Leave timestamp after return as historical event data, not current presence. DO-052 approves recovery’s serialized optimistic revalidation, session-epoch replacement and socket fencing; physical integration and DO-055 session fields remain pending. | DO-047/048/052 policy approved; concrete delivery and storage integration remain TBD. |
| Spectator departure | DO-047 approves explicit Leave releases seat and revokes/deletes session immediately. Accidental disconnect has five-minute grace; no seat/session renewal by pretending to be another role. DO-053 approves the spectator record and paired five-minute grace timestamps. DO-054 approves trusted-time disconnect/reconnect/expiry ordering, stale-event guards and fresh admission after expiry. | DO-047/053/054 policy/schema approved; hibernation reconstruction, DO-055 session binding/access and physical integration remain TBD. |
| Pre-start role switch | DO-047 approves both directions within target capacity. Player→spectator releases alias/seat and deletes recovery proof; spectator→player requires an available alias/seat and creates fresh membership/optional answer. Failure preserves original role/seat/proof. Replace role-bound session at its original fixed expiry without extension. | DO-047 policy approved; complete session schema/access and physical transaction/fencing remain TBD. |
| Account/session credentials | Single-use enrollment/reset links last one day from issuance; sessions have fixed independent one-day expiry, not sliding expiry. Backend and open sockets enforce expiry/revocation. Account logins may coexist. | TBD |
| Player answer recovery | Optional private answer; code + current alias + matching enrolled answer replaces old player sessions/sockets, restores same membership in Awaiting Players/In Progress. No forgotten-alias lookup or alternative proof. Valid-session users may set/replace/delete their answer in those states. DO-049 approves private versioned verifier-record fields/privacy; DO-050 approves v1 Unicode NFC → outer Unicode-whitespace trim → case-fold, preserving internal whitespace/punctuation; future version changes require authenticated replacement and no silent verifier migration. | DO-049–052 policy and verifier profile approved; production costs, exact Unicode implementation, throttling thresholds/durable mechanism and physical integration remain TBD. |
| Unstarted abandonment | After 24 hours without qualifying designated-host activity, commit Cancelled and delete game/participant data, no History. DO-041: serialized trusted-time comparison, end-exclusive deadline; exact-deadline expiry wins over Start/open/host action, no grace. Earlier renewal/Start may commit first; stale alarms reread/reschedule and cannot cancel In Progress or release a newer reservation. Physical alarm and cross-Object recovery remain TBD. DO-040: only designated-host qualifying activity renews; admin override does not. | HLD-068; DO-040/041 |
| Pre-start cancellation | Commit Cancelled, then delete game/participant data without History. Notify joined participants; independent exit does not delay deletion. Intentional cancellation requires confirmation. | TBD |
| Started-game History | Final ordered calls, player aliases, winning alias if any and final board snapshots share **three calendar months from first terminal commit**, with minimal identity/code/expiry metadata. No credentials, intermediate replay archive or initial-release export. Deny expired reads even if deletion is delayed. | TBD |
| Terminal player access | Delete recovery verifier; retain only minimal existing-session/exit authorization until Exit or original session expiry. No new terminal recovery session. Pre-start deletion remains an exception with no reconnect. | TBD |
| Terminal spectator access | Deliver result to connected spectators, delete server identity/session data; local delivered result may remain until Exit. No terminal reconnect/refresh recovery. | TBD |
| Host/admin accounts | No inactivity expiry. Authorized disable/delete revokes access and is blocked while the account hosts a nonterminal game; transfer/cancel first. Do not delete unrelated/unexpired History. | TBD |

Sources: HLD-020–HLD-022, HLD-032, HLD-036–HLD-045, HLD-061, HLD-064–HLD-072, HLD-074.

## 3. Component and module design

| Logical component | Confirmed responsibility | Detailed design to fill |
| --- | --- | --- |
| Shared Rust domain | Pure rules, board feasibility/generation/matching and pattern-specific trait boundaries. | UUID-v7 typed IDs and local `thiserror` errors follow Sections 3.3–3.4; other crates, types, traits, signatures, algorithms and concrete error variants **TBD**. |
| Shared contracts | Shared safe app/backend contract types without provider/UI dependencies. | TBD — serialization, versioning and module boundaries. |
| Shared Dioxus UI / `platform/web` | Web views, nonauthoritative presentation, browser adapters and protected-cookie interactions. | TBD — routing, components, state management and adapters. |
| Rust API Worker | One initial deployable API service; see the authoritative [API design](api-design.md#worker-boundaries). | Contract/routing/interface/error mapping details are maintained in [api-design.md](api-design.md); finalization **TBD**. |
| Game Directory Durable Object | Issued-code lookup and application-wide lifecycle coordination. | Proposed `GameDirectoryObject` records in [Durable Object design Section 6.3](durable-object-design.md#directory-records); final class/binding, schema and concurrency protocol **TBD**. |
| Per-game Durable Object | Authoritative game/board/membership state, SQLite writes, role-filtered hibernating sockets. | Proposed `GameObject` records/History/attachments in [Durable Object design Section 6.5](durable-object-design.md#game-records)–[Durable Object design Section 6.8](durable-object-design.md#socket-metadata); final schema, transactions, bindings and alarms **TBD**. |
| Account/session authority | Strongly consistent credential lifecycle and server-owned account permissions. | Proposed singleton `AccountsObject` in [Durable Object design Section 6.1](durable-object-design.md#object-boundaries) / [Durable Object design Section 6.4](durable-object-design.md#account-records); placement acceptance, interfaces and atomicity **TBD**. |
| Developer CLI | Developer-restricted account operations through an authorized persistence/backend path. | TBD — language, packaging, commands, auth and private output delivery. |
| Later `platform/android` / `platform/ios` | Future Dioxus adapters/entrypoints without duplicating domain/contracts. | TBD — deferred; no native implementation in this release. |

The `uuid` and `thiserror` libraries are selected in Sections 3.3–3.4. Compatible pinned dependency/toolchain/SDK versions, target configuration and build commands remain **TBD**. This inventory does not create a repository scaffold.

### 3.1 API Worker organization — moved

The single-Worker design and endpoint organization now live in [API design Section 2.1](api-design.md#21-one-api-worker-multiple-endpoints). LLD-016 remains confirmed; this is a document move, not a deployment change.

### 3.2 API/state-owner boundary — moved

Worker/endpoint/Object distinctions, the request-flow diagram and namespace-versus-user authorization boundary now live in [API design Section 2.2](api-design.md#22-worker-endpoint-and-durable-object-boundaries). Persisted Object records now live in [Durable Object design](durable-object-design.md#schema-proposal); LLD Section 5 is its navigation reference.

<a id="identifier-policy"></a>

### 3.3 Application-generated identifiers — UUID v7

**Confirmed user direction (LLD-020):** Use **UUID v7** for system identifiers whose generation is controlled by Brews Bingo application/developer code, including the developer CLI. This is the default for `GameId`, `AccountId`, `PlayerId`, `SpectatorId`, `SessionId`, `LinkId`, `CommandId`, `OperationId`, `ConnectionId` and `AuditId`, and for future comparable application-generated entity/operation identifiers. No compelling reason to use another UUID version has been identified for these IDs. Any exception requires an explicit, documented compelling reason and review; do not silently substitute another scheme.

Use the Rust **`uuid` crate** with its `v7` feature for generation; the crate documents UUID v7 support and a `Uuid` value representation.[1] `Uuid::now_v7()` is the standard current-time generation entry point where the chosen target supports it; it requires `std` and `v7`.[2] The crate also documents WebAssembly-specific configuration; exact compatible version/features and clock/randomness integration for Cloudflare Workers, Dioxus web and the developer CLI remain **TBD** until target builds/tests verify them.[1] No dependency is installed or pinned by this document.

- Preserve distinct typed IDs rather than interchangeable strings: for example, the intended shape is `AccountId(uuid::Uuid)`, with analogous newtypes for the other ID names. Generation/parse constructors must enforce UUID v7; raw `Uuid` alone accepts other versions. Exact module, derives and constructor signatures remain **TBD**.
- Generate a new identity once in the component authorized to create it. Account/game/member creation remains server/CLI controlled. A client-generated retry `CommandId`, if selected by the eventual protocol, is also UUID v7 and remains untrusted input. Reuse the same ID for the same retry; never regenerate it merely because a response was lost. Exact command-ID issuer/transport remains **TBD**.
- UUID validity, uniqueness and ordering are not authentication, ownership, command-admissibility or lifecycle proof. Keep authorization checks, unique constraints, explicit timestamps, call sequences, revisions and fencing generations. Do not infer global commit order, expiry or freshness from ID ordering across clients/Objects.
- UUID v7 contains a time component.[1][2] Treat IDs as nonsecret identifiers; they may reveal approximate generation time and must never substitute for bearer credentials or private recovery proof.
- Validate version/format at input boundaries without coercing malformed or other-version values into a different identity. UUID storage representation (SQLite text versus binary), canonical external encoding, collision handling and any future legacy-data migration remain **TBD**. Wire representation belongs in [API design](api-design.md#shared-contracts).

**Different value categories, not alternative defaults for system IDs:**

| Value | Treatment / reason |
| --- | --- |
| `SessionId` and `LinkId` versus bearer secrets | `SessionId` / `LinkId` remain nonsecret UUID-v7 metadata IDs, never bearer credentials. DO-027 approves the access-link token profile; DO-030 approves AccountSessionRecord's logical fields; DO-031 approves a separate 32-byte CSPRNG session bearer, canonical unpadded Base64URL and SHA-256 digest plus the host-only `__Host-brews_session` cookie. DO-032 approves atomic enrollment-completion rotation without extending the original deadline; DO-033 requires fresh password login after reset, without automatic normal-session issuance; DO-034 requires authoritative per-request validation; DO-035 makes `connection_id` a unique nonsecret subscription key, not a bearer. Link delivery/redemption follows DO-028/029. |
| `GameCode` | Keep the confirmed eight-character A–Z/0–9 discovery code; it is a short human-facing lookup value, not the stable `GameId` or an authorization credential. |
| Usernames and player aliases | Retain their domain-specific outer-whitespace trimming/validation rules; per latest user revision, exact case-sensitive equality with no case-folding governs username uniqueness/login and per-game alias comparison (DO-021/DO-045). They are user-provided names, not system-generated record IDs. |
| Cloudflare Object IDs and externally assigned identifiers | Preserve provider-defined representations; the app does not control their generator. UUID-v7 `GameId` remains separate from the namespace/object handle. Exact mapping remains **TBD**. |
| Sequence numbers, revisions, epochs, fences and digests | Keep their counter/hash semantics; they are not newly generated entity identities and must not be replaced with UUIDs. |

<a id="error-conventions"></a>

### 3.4 Component-local Rust errors — `thiserror`

**Confirmed user direction (LLD-021):** Define application-owned typed Rust errors using **`thiserror`**, in an **`error.rs` file adjacent to the subcomponent that owns and uses those errors**. An `auth` folder must define its auth errors in **`auth/error.rs`**, not in a central catch-all file or scattered handler files. Apply the same convention to other components/subcomponents as their module boundaries are selected.

Use `#[derive(Debug, thiserror::Error)]` on the component-owned error enum/struct, with explicit `#[error(...)]` messages. `thiserror` derives the standard Rust error implementation and supports source-error chaining and conversions via `#[source]`/`#[from]`.[3] Concrete variants/messages, wrapping policy, visibility, crate version and feature configuration remain **TBD**; the choice of library and local file convention are **confirmed**, not TBD.

Illustrative placement, **not a selected repository scaffold**:

| Owning subcomponent | Required local placement if that folder exists | Example type name (name TBD) |
| --- | --- | --- |
| `auth/` | `auth/error.rs` — all auth-owned errors | `AuthError` |
| `accounts/` | `accounts/error.rs` — account-management-owned errors | `AccountsError` |
| `games/` | `games/error.rs` — game-owned errors | `GameError` |
| Nested `games/boards/` | `games/boards/error.rs` — errors owned by that nested subcomponent | `BoardError` |

- Keep definitions with their owning component. Sibling/parent consumers import or wrap that typed error rather than duplicating its variants. Genuine cross-component types belong to the component that owns the shared concern; no global all-purpose error enum is selected.
- Component functions return typed `Result<T, ComponentError>` (or a local alias) at their boundaries. Local module declarations/re-exports and exact signatures remain **TBD**. Preserve meaningful domain errors rather than flattening them into strings.
- Wrap lower-level/provider errors at the appropriate component boundary and preserve source chains where safe; apply `#[from]` only where automatic conversion has the intended meaning. The library choice does not decide retryability or recovery behavior.
- Keep passwords, tokens, recovery answers, credential-bearing URLs and other sensitive inputs out of error fields, `Display`/`Debug` output and logs. A source chain can contain private implementation details; it is not automatically safe for external disclosure.
- Internal Rust errors are not public response schemas. The transport boundary maps them deliberately to safe HTTP/WSS errors; that contract, public codes/statuses and close behavior are specified separately in [API design](api-design.md#shared-contracts) and remain **TBD**. Do not serialize raw component errors or expose provider diagnostics by default.

**TBD —** final module tree, concrete error types/variants/messages, conversion boundaries, redaction tests, retry classification and runtime/toolchain compatibility. This records implementation conventions only; no `error.rs`, Cargo dependency or application code is created now.

## 4. Views and frontend contracts

### 4.1 View inventory

These are logical experiences, not a committed page/component count. Screen grouping, route paths, layout and state implementation are **TBD** for every row.

| ID | View / experience | Confirmed content and permission boundary | Detailed specification |
| --- | --- | --- | --- |
| VIEW-01 | Account sign-in | Host/admin password login, session-expired/access-denied feedback; no public registration. | TBD |
| VIEW-02 | Enrollment and password setup | One-day single-use link, restricted session, mandatory personal password setup; used/expired-link outcomes. | TBD |
| VIEW-03 | Account password reset | Privileged-issued single-use reset link and restricted new-password flow; not anonymous self-service recovery. | TBD |
| VIEW-04 | Game list / account home | Host/admin game discovery, create/resume and role-correct cross-game access; respect the single global nonterminal-game limit. | TBD |
| VIEW-05 | New game configuration | Pool, square board, free cells and capacities; only authorized operator in New may edit. | TBD |
| VIEW-06 | Host/admin lobby | Fixed rules/code, player roster/presence/capacity, eligible Start, confirmed cancel/transfer; read-only for other hosts. | TBD |
| VIEW-07 | Host/admin live game | Current/ordered calls, pool exhaustion, boards/qualifiers, random/manual calls, winner submission, confirmed end/transfer. Admin may act across games; other hosts inspect read-only. | TBD |
| VIEW-08 | Game-code entry / join | Normalize code, explain role availability, collect player alias/optional answer; admission failures do not silently switch roles. | TBD |
| VIEW-09 | Player lobby | No board before start; own-alias rename, answer maintenance, permitted role switch and explicit Leave. | TBD |
| VIEW-10 | Player board | Own persisted board, automatic matches and qualification; no manual marking or digital Bingo-claim control. | TBD |
| VIEW-11 | Player recovery / answer settings | Current-alias answer recovery or authenticated answer maintenance; never display stored answer/verifier or offer alternate recovery. | TBD |
| VIEW-12 | Audience / venue display | Current/previous calls, objective and result; venue readability and optional speech details TBD; no private boards. | TBD |
| VIEW-13 | Result / cancellation notice | Role-specific final view, independent Exit; local-only spectator final result versus bounded existing player authorization. Pre-start cancellation does not create History. | TBD |
| VIEW-14 | History list / detail | Host/admin read-only final calls/aliases/boards until common expiry; no export, reopening or membership restoration. | TBD |
| VIEW-15 | **Users** and account-management flow | Admin-only list of host/admin accounts; authorized creation/link/reissue/reset/disable/delete and enable. Enable requires another enrolled admin account (no self-enable), or the separate developer CLI path (HLD-078, API B9). Backend must also deny non-admin listing/operations; never list secrets. | TBD |
| VIEW-16 | Shared connection / error states | Connecting, Synchronizing, Live, Reconnecting, expired/unavailable and denied states; stale view is read-only, no offline mutation queue. | TBD |

### 4.2 Per-view worksheet — copy for each view

| Specification item | Value to fill |
| --- | --- |
| View ID, title, HLD/BR references | TBD |
| Route/navigation entry, exit and back behavior | TBD |
| Role/state permissions and backend enforcement references | TBD |
| Components, layout, responsive breakpoints and venue treatment | TBD |
| Displayed fields, source API/events and privacy projection | TBD |
| Form fields, validation messages and accessible labels | TBD |
| Actions, confirmations, pending state and duplicate-submit handling | TBD |
| Loading, empty, error, forbidden, stale and expired states | TBD |
| Live subscriptions, resynchronization and state replacement | TBD |
| Session expiry/logout, multi-tab behavior and private-cache clearing | TBD |
| Keyboard/screen-reader support, focus and visual contrast | TBD |
| Optional speech ownership/settings/error behavior where applicable | TBD |
| Test scenarios and acceptance evidence | TBD |

## 5. Durable Object design — external reference

**Confirmed document organization (LLD-025):** The existing Durable Object schema proposal now lives in [durable-object-design.md](durable-object-design.md) in this folder. That document is the single editing location for storage ownership, the DATA-01–DATA-13 inventory, schema worksheets, logical records/fields/types/enums, record invariants, coordination/retention prompts and remaining TBDs.

- [Physical ownership](durable-object-design.md#ownership) and [Object boundaries/cardinality](durable-object-design.md#object-boundaries)
- [Logical record inventory](durable-object-design.md#record-inventory) and [per-record schema worksheet](durable-object-design.md#schema-worksheet)
- [Shared types](durable-object-design.md#shared-types)
- [Directory records](durable-object-design.md#directory-records), [account records](durable-object-design.md#account-records) and [game-owned live records](durable-object-design.md#game-records)
- [Final History](durable-object-design.md#history-records), [operational records](durable-object-design.md#operational-records) and [socket metadata](durable-object-design.md#socket-metadata)
- [Consistency, expiry and migrations](durable-object-design.md#consistency-migrations) and [outstanding decisions](durable-object-design.md#outstanding-decisions)

Existing field names/types and privacy/lifecycle constraints are preserved, including `username` without a separate normalized key and the selected `verifier` field name (DO-019). Final physical schemas, placement acceptance, bindings and protocols remain **TBD**. DO-038 approves the logical `GameRecord`/`TerminalOutcome` fields, defaults and lifecycle invariants; DO-039 approves host-transfer actor/target checks, expected-revision compare-and-set, receipt/revision commit and target-removal ordering; DO-040/041 approve designated-host-only deadline renewal and expiry race ordering; DO-042/043 approve the configuration representation/defaults and 1,000 upper-bound ceiling; DO-044 approves full-configuration validation and capped falling-factorial feasibility. DO-021 username comparison and DO-045 alias comparison are approved per latest user revision. DO-046 approves atomic pre-start alias rename with stable-player-ID session binding/no TTL renewal; DO-047 approves atomic role switch/Leave and fixed-expiry session replacement; DO-048 approves retaining the latest explicit Leave timestamp after return; DO-049 approves the logical PlayerRecoveryRecord fields/privacy; DO-050 approves versioned Unicode NFC/outer-whitespace-trim/case-fold answer comparison; DO-051 approves the Argon2id/PHC recovery-verifier profile and benchmark gate. DO-052 recovery transaction/fencing and DO-055 ParticipantSessionRecord/access constraints remain pending. UUID/error conventions and password policy/hashing remain in LLD Sections 3.3–3.4 and 9.1–9.2; API bodies/projections remain in [api-design.md](api-design.md). This move creates no code, database, migration or infrastructure.

**Confirmed cross-record storage rule (user revision during DO-022):** Do not persist a boolean when the corresponding timestamp already determines that same state. For accounts, `disabled_at IS NOT NULL` means disabled; NULL means enabled. Populate it on disable, clear it on enable, and leave it unchanged on authorized no-op retries. Lifecycle status and the verifier remain independent. API booleans may be derived projections, not stored duplicates. Deadline-derived state still requires comparison with trusted backend time. See [Durable Object timestamp-derived state](durable-object-design.md#timestamp-state).

**Approved credential-epoch policy (DO-022):** Each account's `credential_epoch` is a `Revision` starting at 0. Increment exactly once on replacement enrollment-link issuance, reset-link issuance/reissuance, successful password setup/reset completion, or an accepted enabled-to-disabled transition. Do not increment for reads, ordinary login, link redemption alone, single-session logout, enable, rejected operations or retries of already-committed operations; never roll back on enable. New links/sessions store the current epoch and validation requires equality with the account's current epoch plus purpose/scope, expiry, revocation, lifecycle and `disabled_at` checks. Commit epoch changes with the related account/credential changes in AccountsObject; credentials issued by the transition use its new epoch. Reject overflow, preserve fixed deadlines and permit concurrent ordinary logins. DO-036/037 approve durable cross-Object close/expiry policies, acknowledgement and fail-closed frame checks. Physical transaction, retry/outbox and sweep implementation remains TBD. See [DO-022](durable-object-design.md#account-records).

## 6. API design — external reference

**Confirmed document organization (LLD-022):** All detailed API information has moved to [api-design.md](api-design.md) in this folder. That file is the single editing location for Worker/API organization, shared contracts, the API-01–API-32 coverage index, per-operation worksheets, A1–H12 operations with Request/Response proposals, safe DTOs, and WSS protocol design.

- [Worker organization and boundaries](api-design.md#worker-boundaries)
- [Shared contracts and transport error mapping](api-design.md#shared-contracts)
- [Logical operation inventory](api-design.md#operation-index)
- [Per-operation worksheet](api-design.md#operation-worksheet)
- [Operation catalog and Request/Response proposals](api-design.md#operation-catalog)

Operation IDs, proposed names/routes/scopes, descriptions and inputs/outputs are preserved. Final contracts remain **TBD** in the companion document; the extraction does not approve proposals or implement endpoints. Cross-cutting domain rules and UUID/error conventions stay in this LLD; storage records and schema details live in [Durable Object design](durable-object-design.md).

## 7. WSS design — external reference

The message inventory and transport/synchronization worksheet moved with the API design to [api-design.md Section 7](api-design.md#wss-design). Define message grouping, payloads, view revisions, snapshot ordering, hibernation, expiry/revocation, close behavior and transport tests there; final protocol choices remain **TBD**.

Persisted view/session/attachment proposals now live in [Durable Object design Section 6](durable-object-design.md#schema-proposal). This reference preserves existing section numbering without duplicating the WSS specification.

## 8. Detailed workflows and domain algorithms

For each row, fill in sequence, participants, records, preconditions, commit points, failure recovery and tests. Do not introduce extra user-visible lifecycle states or treat Object-to-Object work as one SQL transaction.

| Workflow / algorithm | Constraint to preserve | Detailed design |
| --- | --- | --- |
| Enrollment / reset / reissue | Restricted authority, one-day single-use links, fixed session expiry and predecessor invalidation. DO-029 requires enabled-state/purpose eligibility, one epoch bump per accepted link issuance, predecessor-link/session revocation, secret-free 30-day same-command receipts, and explicit successor reissue rather than bearer-secret replay after a lost response. DO-032 approves atomic enrollment completion and restricted-to-normal rotation without extending the original session deadline. DO-033 approves password login after reset, with no automatic normal-session issuance. DO-034 requires authoritative request-time account/session checks and asynchronous cleanup of ineligible session rows; DO-035/036 approve session-bound subscription registration, durable retryable close work and per-frame checks; DO-037 approves expiry sweeps and metadata deletion after close/absence acknowledgement. Retain link metadata through latest expiry/consumption/revocation + 30 days. | Physical receipt schema, SQL/indexes, session/outbox storage, cleanup schedule, sweep cadence and remaining transaction details stay TBD. |
| Game create / publication | Claim one global slot at New creation and retain it through Awaiting Players/In Progress; fixed config/code, crash-safe Directory/Game agreement. | TBD |
| Admission / Leave / rename / switch | Capacity and alias ownership remain consistent; explicit Leave differs from loss; failed changes preserve prior membership. | TBD |
| Start and board generation | Connected minimum is separate from retained start roster. DO-044 validates the config and uses capped `P(N,k)` feasibility against all retained memberships before acceptance. Generation produces valid, unique position-sensitive boards; no reroll on retries. | DO-044 approved; random sampling and start transaction details TBD. |
| Free-cell representation / evaluation | Valueless pre-satisfied cells and specified default coordinate; qualification may exist immediately, never automatic award. | TBD |
| Pattern-specific Rust traits | Separate trait per distinct pattern; first-release Single Line only; no signatures chosen. | TBD |
| Random/manual call → match → qualify | Valid undrawn value; persist all affected boards/qualification and command outcome before push. | TBD |
| Award / cancellation / slot release | Exactly one immutable terminal result; no viewer-dependent release or stale release of another game. | TBD |
| Host transfer / admin action | HLD-060 authorizes confirmed immediate nonterminal transfer without changing account role; DO-039 requires current actor/target checks, expected-assignment-revision compare-and-set, once-only checked revision increments and actor-scoped command receipts, with target-removal ordering. DO-040/041 approve designated-host-only timer renewal and expiry/activity/start ordering. Separate AdminAuditRecord/retention policy remains DO-088–090. | Physical cross-Object coordination/recovery, confirmation binding, alarm delivery and remaining API details stay TBD. |
| Player recovery / answer maintenance | Same stable member/board, no new slot or terminal recovery, current-alias lookup and revoked old sessions. | TBD |
| Refresh / reconnect / interrupted command | Snapshot-first convergence; no offline queue, replacement boards, duplicate random draws or heartbeat-only freshness. | TBD |
| Idle cancellation / expiry / final cleanup | DO-041 approves exact-deadline/end-exclusive cancellation ordering, stale-alarm recheck and compare-by-game-ID reservation release. Durable alarm scheduling, role-specific cleanup delivery and physical recovery remain TBD; retained History never becomes a credential or replay archive. | HLD-068; DO-041 |

## 9. Security specification worksheet

### Cross-cutting security decisions

| Topic | Confirmed guardrail | Detailed policy / mechanism |
| --- | --- | --- |
| Passwords / secrets | Mandatory setup; **at least 10 characters, any combination of ASCII characters, no required character-class mix** (LLD-023; Section 9.1). Store verifiers, never plaintext credentials. | **Salted Argon2 via RustCrypto `argon2` confirmed** (LLD-024; Section 9.2). DO-019 approves one complete PHC string; DO-023 approves Argon2id v19, fresh independent 16-byte salt, 32-byte output, and SQLite `TEXT` mapping in nullable `AccountRecord.verifier`. DO-024 approves explicit `m_cost`/`t_cost`/`p_cost`, OWASP benchmark start, bounded fail-closed verification and a pre-production Worker/WASM validation gate; production costs/caps and actual target measurements remain pending. Crate pin and upgrade mechanics also remain open. Maximum password length is separately TBD. |
| Account sessions | DO-030 approves the account-session fields, account/scope/epoch binding and fixed one-day absolute deadline; DO-031 approves the separate 32-byte CSPRNG/Base64URL bearer, SHA-256 verifier and `__Host-brews_session` cookie. DO-032 approves atomic enrollment-completion rotation to a new-epoch Normal session while preserving the restricted session's absolute expiry. DO-033 requires fresh password login after reset. DO-034 requires authoritative current-state checks on each protected request and async cleanup after a session becomes ineligible. DO-035 approves unique AccountSocketSubscription IDs and transactional current-session registration before an authorized snapshot. DO-037 approves expiry-triggered close work and subscription deletion only after close/absence acknowledgement. Outbox schema/IDs/sweep cadence/scheduling and physical integration remain TBD. No positive authorization cache may grant across requests without revalidation. Target RNG compatibility remains unverified under DO-024. |
| URL redemption | DO-028 approves fragment-only canonical-origin delivery, no-referrer/no-third-party handling and atomic one-time consume/session creation; DO-029 approves reissue, secret-free receipts and lost-response recovery. | Remaining scanner/prefetch behavior and physical cleanup/transaction details TBD. |
| Privileged access | Users and account operations admin-only; first admin via CLI, later admins via CLI/existing admin; no self-elevation. | TBD — server enforcement, confirmation, account-management edge cases and audit. |
| Gameplay authorization | Admin ownership override does not override game rules; ordinary hosts remain ownership-bound. | TBD — action policy, cross-Object trust and commit-time checks. |
| Player recovery | Answers may be guessable; code/alias are not extra secret factors. No alternate proof or privileged anonymous-player takeover. | DO-050 approves version-1 normalization (Unicode NFC, trim outer Unicode whitespace, case-fold; preserve internal whitespace/punctuation). DO-051 approves Argon2id v19/PHC, independent 16-byte salt, 32-byte output and benchmark gate; production costs/caps remain pending measurements. DO-052 approves transaction/session-fencing policy. Existing HLD abuse rules require bounded attempts/backoff per membership plus caller/game-wide controls and generic failures; exact thresholds and durable enforcement remain TBD. |
| Transport / browser requests | HTTPS/WSS; DO-031 requires the exact configured Origin for browser-cookie-authenticated state-changing HTTPS requests and WSS upgrades; SameSite is not the sole defense. | Other CSRF/CORS mechanics, validation, response headers and rate limits remain TBD. |
| Privacy and lifecycle | No secrets in Users/game snapshots/History; terminal access and deletion honor role-specific rules. | TBD — projection, cache/log redaction, expiry/cleanup and incident handling. |
| Infrastructure / CLI | Cloud credentials only in controlled developer/server environments; environment delivery alone is not authorization. | TBD — secret names/scopes, bindings, operator controls and deployment access. |
| IDs and error disclosure | UUID-v7 IDs are nonsecret and distinct from credential secrets (Section 3.3); typed component errors/source chains are not automatically public-safe (Section 3.4). | **TBD** — version/parse validation tests, redaction checks and safe transport mapping in [API design](api-design.md#shared-contracts). |

<a id="password-policy"></a>

### 9.1 Account password policy

**Confirmed user direction (LLD-023):** Host/admin account passwords must contain **at least 10 characters** and may use **any combination of ASCII characters**. This settles the length/accepted-character/composition portion previously deferred by HLD-063.

- Require ASCII-only input; no mandatory uppercase, lowercase, digit or symbol quota, and no requirement to combine character classes.
- Apply the same rule to first-password setup and password reset, and preserve the entered password exactly for later verification. Do not trim whitespace, case-fold or silently truncate it. Count the decoded password characters, not transport escape notation.
- Do not silently narrow “ASCII” to alphanumeric or printable-only characters. **TBD —** exact UI/input and transport handling for ASCII control characters; any proposed character exclusion requires an explicit policy revision rather than an implementation assumption.
- **Password hashing: salted Argon2 via RustCrypto `argon2`, confirmed by subsequent LLD-024 (Section 9.2).** The earlier hashing deferral is superseded. DO-019 approves combined PHC storage in `AccountRecord.verifier`; DO-023 approves Argon2id v19, fresh 16-byte salts, 32-byte output and SQLite `TEXT` mapping. Production work factors, accepted verifier/cost bounds, target integration, resource validation and upgrade mechanics remain **TBD**; store protected verifiers, never plaintext.
- **TBD —** maximum password length and request-size limits; “at least 10” selects a minimum, not an exact length or a chosen upper bound. UI confirmation fields and exact API validation/error representations remain **TBD** in [API design](api-design.md#shared-contracts).

This policy is for provisioned account passwords, not anonymous-player recovery answers, game aliases, bearer tokens or UUID identifiers. It does not change enrollment/reset authorization or session deadlines.

<a id="password-hashing"></a>

### 9.2 Salted Argon2 account-password hashing

**Confirmed user direction (LLD-024):** Use **Argon2 with a salt** for host/admin account-password hashing, using the RustCrypto **`argon2`** Rust crate. This replaces the earlier hashing deferral in LLD-023. Store the complete password verifier in `AccountRecord` owned by the **DO-001-approved `AccountsObject`**; its PHC string contains the parameters, salt and derived output. Do not treat those as separate stored hash/salt fields. DO-051 approves Argon2id v19 for player recovery-answer verifiers, including PHC encoding, fresh independent 16-byte salt and 32-byte output; this selection does not set production costs/caps, which remain benchmark-gated, or determine access-link/session token digest handling. The crate implements Argon2d, Argon2i and Argon2id and exposes password-hashing/verification APIs that use PHC strings.[4]

**Implementation profile — partially approved:** DO-019 approves AccountRecord fields/types/nullability and the complete PHC string in `verifier`. **DO-023 approves** Argon2id v19, a fresh independent 16-byte salt, 32-byte output, and SQLite `TEXT` storage (NULL before first password setup). DO-024 retains work-factor, supported bounds and runtime/resource validation decisions; no dependency has been installed or target compatibility/performance demonstrated.

| Item | Detail / decision status |
| --- | --- |
| Algorithm and library | **Confirmed:** salted Argon2 via RustCrypto `argon2`. Pin the compatible crate version/features during implementation planning; no Cargo version is selected here. |
| Variant / algorithm version | **Approved — DO-023:** Argon2id, version 19 (`v=19` in PHC). This is the crate's documented default; OWASP recommends Argon2id for password storage.[4][5] |
| Salt | **Approved — DO-023:** generate a fresh, independent 16-byte salt from a cryptographically secure random source for each setup, reset or future rehash, matching the crate's recommended salt length.[6] Target RNG integration/runtime validation remains subject to DO-024. |
| Output | **Approved — DO-023:** 32 hash-output bytes, matching the documented default output length; this is not a password-length limit or the PHC string's length.[7] |
| Work factors | **Approved — DO-024:** explicitly configure `m_cost` (1 KiB memory blocks), `t_cost` (iterations/passes), and `p_cost` (degree of parallelism).[7] Do not rely on mutable library defaults. Exact production values remain pending target validation. |
| Benchmark starting point | **Approved — DO-024 as the initial benchmark point only, not a measured/approved production profile:** `m_cost=19456` (19 MiB), `t_cost=2`, `p_cost=1`, following OWASP's summary minimum configuration for Argon2id.[5] Benchmark stronger settings where feasible; do not silently reduce security settings to meet a quota. |
| Stored representation | **Approved — DO-019/023:** the full library-generated Argon2id v19 PHC string in `AccountRecord.verifier: Option<EncodedVerifier>`, mapped to SQLite `TEXT` and absent (`NULL`) before first password setup. Keep algorithm/version, work parameters, salt and derived hash together; no separate salt/hash fields or custom binary encoding. The password-hashing API produces a PHC string suitable for authentication.[4] Supported PHC/cost bounds and runtime validation remain for DO-024. |

#### 9.2.1 Salt and verifier handling

- Use the salt parameter supplied/generated through the library's password-hashing API, not a homemade salt/password concatenation or fast-hash substitute. Never use a shared fixed salt or derive it from an account ID, UUID, username, timestamp or password.
- The salt is not a secret. Persist it in the **same Durable Object as its corresponding password hash**: the approved Argon2id v19 PHC string in SQLite `TEXT` field `AccountRecord.verifier` holds algorithm/version, costs, encoded salt and output together. Write/replace the complete value atomically under that owner so a reset or rehash cannot leave a mismatched salt/hash pair. No separate salt Object, KV entry or secret store is needed, and no redundant independently mutable salt field or custom binary encoding is proposed. Transaction/schema mechanics remain TBD.
- Illustrative format only, **not a credential, generated verifier or fixed parameter configuration**:

```text
$argon2id$v=19$m=<m>,t=<t>,p=<p>$<salt-base64>$<hash-base64>
```

- Keep the complete verifier private to backend authentication/storage. Never include it in Users listings, game snapshots, History, API responses, audit/retry payloads or logs; do not persist plaintext or reversibly encrypted account passwords.
- A pepper is different from the required per-password salt. No pepper, secret-management dependency or pepper-rotation scheme is selected by this decision.

#### 9.2.2 Setup, reset and login integration

1. **Setup/reset:** require the already-defined restricted-session authority and validate Section 9.1 before hashing. Hash the exact decoded password bytes without trimming, case conversion, Unicode normalization or truncation. Apply this on the trusted backend, not solely in browser validation or client-side hashing.
2. Generate a fresh secure salt and use the crate's high-level password-hashing API with the explicit accepted profile. Store only the complete encoded verifier, retaining the existing `None`-before-setup semantics. Treat randomness/configuration/hashing failure as failure, never as permission to store an unsalted/weak fallback or complete password setup.
3. **Enrollment completion — DO-032 approved:** in one AccountsObject transaction, revalidate enabled PendingEnrollment state, the live EnrollmentOnly session and current epoch; persist the verifier, set Verified/password_set_at, increment epoch once, revoke the restricted session and create one Normal session at the new epoch. Preserve the original absolute session expiry, set the new cookie only after commit, and require login rather than secret replay if the committed response is lost. **Reset completion — DO-033 approved:** revalidate enabled ResetRequired state, live PasswordResetOnly session and current epoch; persist the verifier, set Verified/password_set_at, increment epoch once and retire reset-only authority. Issue no Normal session; clear the cookie and require fresh password login. DO-034 requires authoritative current-state lookup on each protected request and asynchronous deletion of expired/revoked session rows; no positive cached grant across requests. Uncovered retry/compute/CAS/SQL scheduling details remain TBD.
4. **Login:** load the private stored verifier, parse it through the library's PHC support, validate the supported algorithm/version/parameter bounds, and call its `PasswordVerifier` API against the exact supplied password. Verification uses parameters from the parsed stored hash rather than the current hasher's configured defaults.[4] Do not create a new random salt and compare encoded strings on login, or accept client-supplied verifier/cost parameters.
5. Grant a session only after successful verification and the current account-state/credential-epoch checks (DO-034); do not trust a positive cross-request authorization cache without revalidation. Password mismatch must not reveal account existence, verifier contents or internal errors. Map hashing/parsing/RNG failures into component-owned `thiserror` errors in `auth/error.rs`; distinguish operational failures internally, redact sources, and leave concrete variants and external response mappings **TBD** in the API design.

Rust API guidance: use the crate's `Argon2`/`Params` types and high-level `PasswordHasher`/`PasswordVerifier` traits rather than implementing Argon2 or comparisons. The retrieved crate documentation's hashing example generates a random salt with the `getrandom` feature; concrete imports, feature flags and random-source wiring must match the pinned release and Cloudflare WASM target.[4][7] This is design guidance, not a tested code sample.

#### 9.2.3 Runtime, upgrades and remaining TBDs

- **DO-024 — approved validation gate; measurements pending:** before production use, confirm WASM build/features, cryptographically secure randomness, peak memory, CPU time, latency and concurrent authentication impact in the actual Worker/Durable Object deployment. Choose where trusted hashing runs without changing the one-API-Worker architecture or assuming native threads/OS RNG support. A successful local/native build would not establish deployed feasibility.
- **DO-024 — approved fail-closed bounds policy; numeric limits pending target measurement:** select production work factors, rate limits, admitted hashing concurrency, password/request/verifier size caps, and allowed PHC algorithm/version/cost bounds before expensive verification. Reject malformed/unsupported/oversized/out-of-bounds verifiers rather than falling back. These mechanisms do not impose an unapproved password maximum or alter the ASCII/minimum-length policy.
- **Approved — DO-025:** After successful verification, opportunistically rehash only an otherwise-supported outdated profile using a fresh salt/current explicit costs. Conditionally replace only if the exact verifier verified and `credential_epoch` are unchanged and the account still exists, is enabled and `Verified`. A hash-only rehash does not increment credential_epoch or extend sessions. On race, discard the candidate and re-read/re-verify the current verifier; further conflicts fail closed with a generic authentication response. If rehash fails but the current credential remains valid, allow login after a fresh status/epoch check, record a redacted internal failure and retry later. Exact transaction/CAS/error plumbing remains TBD; never allow stale rehash to overwrite reset/disable.
- **Approved — DO-023:** Argon2id v19, fresh independent 16-byte salts, 32-byte output and complete PHC string in SQLite `TEXT` `AccountRecord.verifier`. **DO-024 approved policy:** explicit work factors, OWASP benchmark starting point, fail-closed bounded parameter handling and pre-production runtime/resource validation. Production tuning/caps and measured target results remain pending. Compatible pinned crate version/features, target integration and tests also remain unselected/unverified. **Salted Argon2 and the RustCrypto `argon2` crate are settled**, not deferred.

## 10. Verification plan

No application tests have been implemented or run by this template. Exact test files, tools, fixtures, acceptance thresholds and evidence are **TBD**.

| Test area | Scenarios to specify | Cases / expected result / evidence |
| --- | --- | --- |
| Domain and schema | Pool/string rules, every supported board size, free cells, distinct-board feasibility, Single Line, unique calls and data constraints. | TBD |
| Identifier convention | UUID v7 generation/parse validation across selected targets, distinct newtypes, stable retry IDs and no ID-as-credential assumptions. | **TBD** — no target build/runtime evidence yet. |
| Component errors | `thiserror` derives, local `error.rs` ownership, typed conversion/source chains, secret redaction and deliberate transport mappings. | **TBD** — concrete error/contract tests not implemented. |
| Views and permissions | Host versus admin controls, Users frontend/backend denial, private-board projection, missing/expired sessions and restricted enrollment. | TBD |
| Worker routing and Object isolation | Multiple API handlers in the single Worker reach the correct authoritative owner; unknown codes do not create games, and Object IDs/bindings never replace user authorization. | TBD |
| Account lifecycle | Concurrent redemption, reissue/reset revocation, first-admin creation, non-owner overrides, removal blocked by hosted nonterminal game. | TBD |
| Password policy | Reject fewer than 10 characters and non-ASCII input; accept policy-valid ASCII passwords without character-class quotas; preserve whitespace/case and decoded input through setup/reset/login. | **TBD** — UI/transport edge cases and tests; hashing scenarios are listed below. |
| Password hashing | Correct/wrong password verification; independent salts for the same password; PHC round-trip and embedded-parameter use; malformed/unsupported/out-of-bounds verifier rejection; RNG failure; no secrets in projections/logs; setup/reset/login and proposed rehash races; WASM resource/concurrency measurements. | **TBD** — tests/benchmarks not implemented or executed; account and recovery-answer production costs/caps remain untuned pending target measurements. Algorithm/format decisions are partially approved in DO-023/024 and DO-051. |
| Lifecycle/concurrency | Competing create/start/call/winner/cancel/transfer/admission; separate global slots and immutable terminals. | TBD |
| Participant identity | Alias case/space/rename/reuse, role switch, fresh rejoin versus recovery, answer deletion and no inherited credentials. | TBD |
| Durable recovery | Restart/hibernation, commit with lost ACK/broadcast, snapshot race, revision gap, stale socket and original-command retry. | TBD |
| Expiry/retention | One-day credentials, five-minute spectator grace, 24-hour host-idle cancellation, month-based History deletion and no resurrection/export. | TBD |
| Terminal access | Player final-view window, spectator local-only result, pre-start deletion, independent Exit and post-exit denial. | TBD |
| Browser/accessibility | Desktop/mobile matrix, venue readability, keyboard/screen readers and optional spoken-announcement behavior. | TBD |
| Capacity/resilience | Cloudflare budget, storage/request/CPU impact, role-filtered fan-out, connection storms, backend outage and cleanup backlog. | TBD |

Latency, concurrency/load targets, measurable accessibility criteria and failure/cleanup budgets: **TBD**, not inferred from provider documentation.

## 11. Cloudflare deployment and operations worksheet

| Item | Detailed specification |
| --- | --- |
| Rust/Dioxus/Workers SDK/toolchain compatible pinned versions | TBD |
| UUID/error libraries | `uuid` with UUID v7 and `thiserror` selected; compatible versions, target features/clock/RNG configuration and validation **TBD** (Sections 3.3–3.4). |
| Password-hashing library/runtime | Salted Argon2 via RustCrypto `argon2` selected for account passwords; DO-051 separately approves Argon2id v19/PHC for recovery answers. **TBD** — pinned compatible version/features, secure WASM randomness, hashing placement, production costs/caps, CPU/memory/concurrency benchmarks and failure handling. |
| Single API Worker entrypoint/module routing, Object classes, bindings and deployment configuration | **TBD** — preserve LLD-016; proposed Object names/structures in [Durable Object design Section 6](durable-object-design.md#schema-proposal) do not finalize bindings, deployment configuration or SDK interfaces. |
| Static asset build, routing, caching and private-route separation | TBD |
| Local / test / production environments and isolated resources | TBD |
| SQLite schema/Object migrations and version compatibility | TBD |
| Secrets, developer CLI setup and initial-admin bootstrap procedure | TBD |
| Durable deadlines/alarms or equivalent expiry/cleanup scheduling | TBD |
| Logs, metrics, tracing, redaction and admin-action auditing | TBD |
| Cloudflare free-allowance verification, usage model and quota failure handling | TBD |
| Backup/restore, expiry preservation, rollback and reconciliation runbooks | TBD |
| CI checks, deployment verification and release rollback | TBD |
| Deferred native adapters/build/signing | TBD — later phase only |

No provider resources, credentials, accounts, databases or dependencies are created by this document.

## 12. Open design register and review gates

### 12.1 Detailed decisions for the user to fill

Detailed specifications in LLD-001–LLD-015 remain **TBD** where indicated; affected entries reference the captured proposals. LLD-016 records confirmed Worker organization; LLD-017/LLD-019 retain API catalog/input-output provenance now in [api-design.md](api-design.md); LLD-018 covers storage proposals, now in [durable-object-design.md](durable-object-design.md) under LLD-025. LLD-020 confirms UUID v7 IDs, LLD-021 confirms local `thiserror` errors, LLD-022 confirms the API document split, LLD-023 confirms the password minimum/ASCII rule, and LLD-024 supersedes its hashing deferral with salted Argon2 via RustCrypto `argon2`. Capturing a proposal does not finalize request/response schemas, physical storage or protocols. Completing an entry records design, not implementation evidence.

| ID | Decision / specification | Status / selected detail |
| --- | --- | --- |
| LLD-001 | Dioxus view grouping, routes, components, state management and accessibility | TBD |
| LLD-002 | Users fields/actions/filters and protected account-management UX | TBD |
| LLD-003 | Cloudflare account/session store placement and ownership | Proposed singleton `AccountsObject` in [Durable Object design Section 6.1](durable-object-design.md#object-boundaries) / [Durable Object design Section 6.4](durable-object-design.md#account-records); final placement/cardinality acceptance and ownership interfaces **TBD**. |
| LLD-004 | SQLite tables/types/keys/indexes, record mappings and migrations | Proposed records/indexes in [Durable Object design Section 6](durable-object-design.md#schema-proposal); application-generated IDs now UUID v7 per LLD-020. Final physical schema, keys/constraints, mappings, SQL and migrations **TBD**. |
| LLD-005 | Directory/game coordination and command idempotency/reconciliation | DO-014/015 approve one nullable `game_id` reservation row, claim-first creation, same-ID/same-code GameObject recreation after failure, and compare-by-ID acquire/release without a generation counter in [Durable Object design Section 6.3](durable-object-design.md#directory-records). DO-016 approves idempotent terminal compare-and-clear (matching ID clears; NULL is complete; different ID is stale/no-op). DO-017 approves a presence-only account-assignment gate keyed by account_id; DO-018 approves acquire-before-check, assignment blocking, clear-and-reject for hosted games, and retain-and-retry on interruption. In-flight-operation serialization, operation receipts/idempotency, retry scheduling/backoff and cross-Object recovery details remain **TBD** in Sections 6.3/6.7. |
| LLD-006 | Worker API methods/routes, request/response/error schemas and compatibility | Proposed functions/methods/paths/scopes (LLD-017) and per-operation Request/Response fields/types/effects (LLD-019) in [API design Section 6](api-design.md#operation-catalog); final requiredness, encodings, complete schemas/statuses, signatures and compatibility **TBD**. |
| LLD-007 | WSS payloads/revisions, privacy projections, hibernation and backpressure | Persisted revision/subscription/attachment proposals remain in [Durable Object design Section 6.4](durable-object-design.md#account-records) / [Durable Object design Section 6.5](durable-object-design.md#game-records) / [Durable Object design Section 6.8](durable-object-design.md#socket-metadata); WSS contracts live in [API design Section 7](api-design.md#wss-design). Final protocol mechanisms **TBD**. |
| LLD-008 | Board generation/feasibility, matching and pattern-specific trait signatures | TBD |
| LLD-009 | Password policy, token/cookie/verifier implementation and request security | Minimum 10 characters and any combination of ASCII characters confirmed by LLD-023 (Section 9.1); salted Argon2 via RustCrypto `argon2` confirmed by LLD-024 (Section 9.2). DO-019 approves AccountRecord fields/types/nullability and combined PHC storage in `verifier`; DO-022 approves lifecycle/disablement and credential-epoch rules in [Durable Object design Section 6.4](durable-object-design.md#account-records). Other records remain under review. Maximum length, final Argon2 profile/runtime/physical constraints, token/cookie design, transaction/race/retry mechanics, cross-Object revocation delivery and request security remain **TBD**. |
| LLD-010 | Answer normalization/protection, abuse controls and recovery transactions | DO-049 approves the logical private `PlayerRecoveryRecord` fields/types/privacy; DO-050 approves version-1 Unicode NFC, outer Unicode whitespace trim and case-folding, preserving internal whitespace/punctuation, with stored-version verification and authenticated replacement on future version changes; DO-051 approves Argon2id v19, fresh independent 16-byte salt, 32-byte output and PHC encoding; 19 MiB/2/1 is only a benchmark starting point and production costs/caps require target measurements. DO-052 approves optimistic verifier revalidation, atomic answer change, session-epoch replacement and socket-fencing policy. Exact Unicode implementation, throttling thresholds/durable enforcement and physical integration remain **TBD**. | DO-048–053 |
| LLD-011 | Presence/Leave/Exit mechanics, timer scheduling and terminal notice/deletion ordering | DO-053 approves spectator record/grace fields and DO-054 approves disconnect/reconnect/expiry race ordering and stale-event guards in [Durable Object design Section 6.5](durable-object-design.md#game-records). Hibernation/presence reconstruction, scheduler and close-work integration, DO-055 session binding/access, Exit scope and terminal notice/deletion mechanics remain **TBD**. |
| LLD-012 | History placement/indexing, calendar-month expiry, cleanup and backup/restore | Proposed final snapshot in the original Game Object with Directory index ([Durable Object design Section 6.3](durable-object-design.md#directory-records) / [Durable Object design Section 6.6](durable-object-design.md#history-records)); placement acceptance, month arithmetic, cleanup and restore mechanisms **TBD**. |
| LLD-013 | CLI interfaces, binding/credential scopes, secure link handoff and bootstrap | TBD |
| LLD-014 | Optional speech, browser matrix, quality/load targets and acceptance tests | TBD |
| LLD-015 | Build/deployment/observability and quota verification | TBD |
| LLD-016 | Initial API Worker organization and Durable Object boundary | Confirmed user direction — one API Worker serves multiple endpoints through organized Rust handlers and bindings to the existing directory/per-game state owners. No endpoint-per-Worker split. Final API contracts, Object schemas, router/modules, bindings and internal protocols remain **TBD**; subsequent proposals are captured under LLD-017/LLD-018. See [API design Section 2](api-design.md#worker-boundaries), and LLD Sections 5 and 11. |
| LLD-017 | Proposed operation catalog and auth-scope notation | Captured at user request — [API design Section 6](api-design.md#operation-catalog) retains category headers and per-operation ID/function/access-type heading, Path, Auth scope and description. Scope labels are Admin, Host, Player, Anyone and the user-selected System for internal work. Proposed names/paths remain reviewable; Anyone never bypasses required session/proof/ownership, Host includes admin with applicable game guards, and System is not an account role or public endpoint. Potential Request/Response fields are recorded under LLD-019; final schemas and function signatures remain **TBD**. Object structures are separately proposed under LLD-018, not finalized by this catalog. |
| LLD-018 | Proposed Durable Object owners, records, field names and types | Captured at user request — [Durable Object design Section 6](durable-object-design.md#schema-proposal) records Directory/Accounts/Game boundaries, Rust-style fields/types, History, operational records and connection metadata; [Durable Object design Section 3](durable-object-design.md#record-inventory) maps DATA IDs. New placements and other field shapes remain proposals; the system-ID convention is subsequently confirmed by LLD-020. **TBD** items are explicit beside each group and in [Durable Object design Section 6.9](durable-object-design.md#outstanding-decisions); remaining adoption, physical schemas, transactions/revocation, security, retention mechanisms, bindings and implementation remain pending. |
| LLD-019 | Proposed per-operation Request and Response inputs/outputs | Captured at user request — every A1–H12 operation in [API design Section 6](api-design.md#operation-catalog) has Request/Response subheadings with potential typed fields, session/cookie effects and failure candidates. Shared safe DTO descriptions do not expose storage records. Internal helpers use internal inputs/results; WSS entries distinguish upgrade/frames/push. Final schemas, requiredness, encodings, status/error contracts, retry/security mechanisms and tests remain **TBD**; no new endpoint or implementation is approved. |
| LLD-020 | Application-generated system ID convention | **Confirmed user direction** — UUID v7 by default for application/developer-controlled identifiers, including the listed `*Id` types; use the Rust `uuid` crate and distinct typed IDs. A different scheme needs a documented compelling reason/review. Tokens, short codes and provider-owned IDs remain separate categories. Target configuration, encoding and concrete constructors **TBD**. See Section 3.3 and [Durable Object design Section 6.2](durable-object-design.md#shared-types). |
| LLD-021 | Rust error library and component-local ownership | **Confirmed user direction** — use `thiserror` for application-owned typed errors in adjacent component `error.rs` files; auth errors belong in `auth/error.rs`. Concrete variants/conversions/versions/tests **TBD**; transport mappings remain in the API design. See Section 3.4. |
| LLD-022 | API design document extraction | **Confirmed user direction** — `plans/api-design.md` is the sole home for API organization/contracts/inventories/catalog/Request–Response/WSS design. LLD Sections 3.1–3.2 and 6–7 link to it; stable operation IDs/content are preserved. Unresolved contract details remain **TBD**. |
| LLD-023 | Account password length and character policy | **Confirmed user direction** — at least 10 characters, any combination of ASCII characters, no required mix of character classes. Applies to host/admin account password setup/reset and consistent later verification. **Earlier hashing deferral superseded by LLD-024**; maximum length and UI/transport details remain TBD. See Section 9.1 and the API password inputs. |
| LLD-024 | Salted Argon2 account-password hashing | **Confirmed user direction** — use Argon2 with salt and RustCrypto `argon2` for host/admin passwords, superseding LLD-023's hashing deferral. DO-019 approves `verifier` as one complete library-generated PHC string rather than raw salt alone or separate salt/hash fields. **DO-023 approves** Argon2id v19, fresh independent 16-byte salts, 32-byte output and SQLite `TEXT` mapping. **DO-024 approves** explicit work factors, the OWASP benchmark starting point, bounded fail-closed verification and a pre-production runtime-validation gate; production tuning/caps and target measurements remain unverified. Compatible crate pin and upgrade mechanics remain open. |
| LLD-025 | Durable Object design document extraction | **Confirmed user direction** — move the existing schema proposal into `plans/durable-object-design.md`. It is the single home for Object ownership, DATA inventory, schema/consistency/migration worksheets, records/fields/types, invariants and storage TBDs. LLD Section 5 and API storage references link to it. Field names/types and confirmed constraints are preserved; the move does not approve pending proposals or authorize implementation. |

### 12.2 Product/source questions — do not silently decide in implementation

- **Designated-host idle timer (DO-040/041):** Only intentional open/resume or an accepted, state-changing action by the current designated host renews the 24-hour pre-start deadline; a designated-host-initiated transfer qualifies once. Non-designated admin override/open/resume, including admin transfer, does not renew; the successor inherits the existing deadline. DO-041 approves end-exclusive trusted-time comparison at serialized transaction point; expiry wins at equality, no grace, stale alarms recheck/reschedule and never affect In Progress/newer reservation. Physical alarm scheduling and cross-Object recovery remain TBD.
- **Privileged account-management edge cases:** Self-removal, last-admin protection and any account type-editing feature are not defined by account creation permission. HLD-078 explicitly requires an enable feature/API for disabled accounts, restricted to another enrolled admin or developer CLI; restore the pre-disable lifecycle state without reviving old sessions/links. Disablement is derived solely from populated `disabled_at`, preserving lifecycle status and storing no redundant boolean; DO-022 approves lifecycle, no-op timestamp and credential-epoch rules, plus fresh setup/reset links after enable. Account-operation races/transactions, DO-036 outbox/delivery integration, self-removal and last-admin protection remain open. No role-editing feature is introduced. Remaining clarification: **TBD**.
- **UI/speech/quality requirements:** Exact layouts, spoken-announcement behavior, accessibility/compatibility criteria and measurable quality targets still need decisions. **TBD**.
- **Source reconciliation and HLD approval:** Resolve stale requirements/HLD wording using explicit later decisions, then obtain required stage approval separately. **TBD**.

### 12.3 Traceability index

Coverage means a place to complete the design, not that every design choice or requirement is fulfilled. LLD Section 5 references [Durable Object design](durable-object-design.md), and Sections 6–7 reference API/WSS content in [api-design.md](api-design.md), so coverage ranges containing those sections include their companion documents. Ranges identify source decision rows; superseded rows are carried only as qualified in Section 1.2.

**Subsequent user direction:** LLD-016 records the single-Worker/multiple-endpoint approach and Worker/endpoint/Object distinction. LLD-017 captures the requested A1–H12 proposed operation catalog and scopes in [API design Section 6](api-design.md#operation-catalog), mapped from [API design Section 4](api-design.md#operation-index). LLD-018 captures the structure/field proposal in [Durable Object design Section 6](durable-object-design.md#schema-proposal), mapped from DATA-01–DATA-13 in [Durable Object design Section 3](durable-object-design.md#record-inventory), with explicit outstanding TBDs. LLD-019 adds potential Request/Response inputs/outputs to all [API design Section 6](api-design.md#operation-catalog) operations and links the [API design Section 4](api-design.md#operation-index) coverage rows. LLD-020/LLD-021 add confirmed ID/error conventions; LLD-022 moves detailed API material to the linked companion. LLD-023 settles password minimum length and ASCII/composition rules; LLD-024 subsequently confirms salted Argon2 via RustCrypto `argon2` while leaving profile/runtime details TBD. LLD-025 moves the existing storage/schema proposal and worksheets into the linked Durable Object design document without changing fields/types. These scoped updates do not rewrite the HLD or complete detailed API/storage design.

| HLD decisions | Template coverage |
| --- | --- |
| HLD-001–HLD-008 | Sections 1–3, 5–7, 9–11: baseline layers, hosting, persistence and contracts |
| HLD-009–HLD-010 | Sections 2.2, 3–6, 9, 11: CLI/account enrollment |
| HLD-011–HLD-018 | Sections 2, 4–8, 10: game workflow, admission and limits |
| HLD-019–HLD-022 | Sections 2–3, 5–7, 9–11: platform boundaries, recovery and sessions |
| HLD-023–HLD-035 | Sections 2, 4–8, 10: boards, patterns, terminal state and History |
| HLD-036–HLD-048 | Sections 2.3, 4–8, 10: cancellation, attendance, role switching and capacity |
| HLD-049–HLD-058 | Sections 2.1, 4–6, 8–10: aliases, codes, reuse and stable identity |
| HLD-059–HLD-062 | Sections 2.2, 4, 6–10: host assignment/transfer, sessions and board visibility |
| HLD-063–HLD-067 | Sections 4–6, 8–10: password/reset/reissue and recovery-answer maintenance |
| HLD-068–HLD-072 | Sections 2.3, 5–11: abandonment, deletion, final access and no export |
| HLD-073–HLD-076 | Sections 2.2, 4–6, 8–12: host/admin split, override, creation and Users |

| Requirement references | Template coverage / qualification |
| --- | --- |
| BR-001–BR-010 | Sections 2, 4, 6, 8, 10: naming, display, ordered unique values and draw controls |
| BR-011–BR-013 | Sections 2, 5–8, 10: fresh games, terminal/cancellation safeguards and durable recovery |
| BR-014–BR-020 | Sections 1.2, 2.2, 4, 6–7, 9–10: venue/role views, branding, objective and speech; admin exception and online-only supersession explicit |
| BR-021–BR-024 | Sections 2–5, 10–11: History and web coverage; native delivery remains later |
| BR-025–BR-029 | Sections 2, 5–8, 10: strings, digital boards and winning validation with HLD refinements |

### 12.4 Completion checklist

- [ ] Each view has a completed worksheet, permissions, states and acceptance criteria.
- [ ] [Durable Object design Section 6](durable-object-design.md#schema-proposal) proposals are accepted/revised, their TBDs resolved, and all physical ownership/schema/constraint/index/migration worksheets completed.
- [ ] [API design Section 6](api-design.md#operation-catalog) Request/Response proposals are reviewed and every API has finalized requiredness, success/error schemas/statuses, authorization and retry contracts.
- [ ] [API design WSS schemas/protocol](api-design.md#wss-design) and storage interactions are specified, without duplicating transport contracts here.
- [ ] UUID v7 generation/validation and component-local `thiserror` conventions are exercised on selected Rust targets; any ID exception has a reviewed compelling reason.
- [ ] Salted Argon2 profile, PHC mapping, secure randomness, resource/abuse bounds and reset/rehash concurrency are reviewed and exercised on the selected backend target; Section 9.2 recommendations are explicitly accepted or revised.
- [ ] Domain algorithms and distinct pattern traits preserve confirmed rules without inventing features.
- [ ] Cross-Object transactions/retries and all retention/deletion paths have failure recovery designs.
- [ ] Host/admin/Users permissions, privileged creation and secret boundaries are reviewed end to end.
- [ ] Source conflicts and genuinely new product questions are resolved or explicitly deferred.
- [ ] Tests, quality targets, Cloudflare compatibility/budget checks and operational runbooks are defined.
- [ ] Detailed design approval is recorded; implementation receives its own explicit authorization.

## Sources

[1] https://docs.rs/uuid/latest/uuid — uuid — version features and UUID representation
[2] https://docs.rs/uuid/latest/uuid/struct.Uuid.html — Uuid::now_v7 — UUID v7 generation
[3] https://docs.rs/thiserror/latest/thiserror — thiserror — typed Rust error derivation
[4] https://docs.rs/argon2/latest/argon2 — RustCrypto argon2 — password hashing and verification
[5] https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html — OWASP — Password Storage Cheat Sheet
[6] https://docs.rs/argon2/latest/argon2/constant.RECOMMENDED_SALT_LEN.html — argon2 — recommended salt length
[7] https://docs.rs/argon2/latest/argon2/struct.Params.html — argon2 — memory, iteration, parallelism and output parameters
