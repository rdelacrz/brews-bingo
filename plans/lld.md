# Brews Bingo — Low-Level Design Template

## 1. Document status and use

- **Status:** Planning decision ledger DO-001–107 is complete and approved. HLD-078/DO-022 account enable rules, DO-073–090 terminal/history/receipt/rate/audit policies, DO-091–095 connection/hibernation/alarm/backpressure policies and DO-096–107 schema/deployment/verification/account-edge policies are reconciled in the companion designs. Physical SQL/runtime integration, measured Argon2/Unicode/Cloudflare compatibility, capacity forecasts and actual release-gate tests remain unverified; no implementation or tests are authorized.
- **Business:** Rockville Brews.
- **Primary source:** [High-level design](hld.md), including decisions HLD-001–HLD-078, and the approved DO-001–DO-107 ledger for subsequent detailed policies.
- **Supporting sources:** [Business requirements](requirements.md) and [hosting research](research.md).
- **Companion designs:** [API design](api-design.md) is the single home for API organization, operations, requests/responses and WSS contracts. [Durable Object design](durable-object-design.md) is the single home for storage ownership, schema proposals, records/fields/types and schema worksheets. This LLD links to both rather than duplicating those specifications.
- **Scope:** Structure the detailed design for the initial desktop/mobile web release on Cloudflare. Section 3.5 / LLD-027 selects Dioxus Router, Axum inside the single Rust Worker, selective API-first asset routing and native Durable Object state/socket handlers. Section 4 retains the requested screen/component/navigation proposals (LLD-026). Native Android/iOS implementation remains deferred.
- **Approval boundary:** Capturing this LLD and its UI proposals does not approve the overall HLD, finalize new UI/contract choices, authorize implementation, or authorize infrastructure/account creation.
- **Reviewer / approval / revision:** TBD.

### 1.1 How to complete this template

**Confirmed input** means a constraint carried from the HLD or a subsequent explicit user direction recorded in the LLD decision register. **Proposed** means a suggested design recorded for review, not final adoption. **TBD** explicitly identifies an undetermined choice or unfinished specification. Inventory labels and document IDs are planning references, not implemented resources. [API design](api-design.md#operation-catalog) records proposed Rust functions, methods/paths, scopes and Request/Response inputs/outputs; [Durable Object design Section 6](durable-object-design.md#schema-proposal) records proposed Object owners, fields, types and supporting enums. UUID v7 and the local `thiserror` convention are confirmed in Sections 3.3–3.4; account-password policy and salted Argon2/library selection are confirmed in Sections 9.1–9.2; other proposed details do not become finalized APIs, physical schemas or protocols.

For each view, stored record, API operation and event, complete its specification using the relevant reusable worksheet. Preserve the confirmed constraints while choosing implementation details. Any intentional change to an HLD rule needs an explicit decision rather than being hidden in a schema or payload.

Proposed API names/paths and DTOs remain proposals in [API design](api-design.md#operation-catalog); storage decisions and logical schemas are in [Durable Object design](durable-object-design.md). DO-100 approves GAME_DIRECTORY/ACCOUNTS/GAMES bindings and environment isolation; DO-101 approves Worker-only ingress, typed internal Object calls and restricted backend CLI operations. DO-102 approves stable Rust/lockfile and compatible Worker SDK/build pinning after validation, trusted Worker time and fail-closed CSPRNG. Exact versions, SQL DDL, wire contracts and function signatures remain implementation validation.

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
- LLD-027 selects the routing libraries and integration boundaries in Section 3.5, superseding older router-choice deferrals. API paths/DTOs/error contracts remain in the API companion; that document is not rewritten by this scoped LLD update.

Source-document wording reconciliation remains separate; this template does not edit or claim full consistency of those documents. Any ambiguity not explicitly settled remains `TBD` in Section 12.

## 2. Confirmed design baseline

### 2.1 Architecture and domain constraints

| Area | Confirmed input | Source |
| --- | --- | --- |
| Frontend | Rust/Dioxus, client-rendered web first, with Dioxus Router for typed navigation (LLD-027) and Dioxus-integrated Tailwind CSS styling (LLD-028). All approved app features on desktop/mobile browsers; compatible versions remain to be pinned/validated. | HLD Sections 4.1–4.3; HLD-003, HLD-019; LLD-027/028 |
| Backend | One Rust Cloudflare API Worker using `workers-rs` and Axum as its HTTP request service; responsibility-based handlers call authoritative state owners. No Worker per endpoint or conventional listening server. | HLD Sections 3, 5; LLD-016/027 |
| Delivery | Workers Static Assets with SPA navigation fallback; same-origin `/api` and `/api/*` run Worker-first, never fall through to app HTML. HTTPS commands/authentication and native GameObject WSS; SSR/fullstack server functions are not selected. | HLD Sections 4.2, 7.1; LLD-027 |
| Durable ownership | Approved per-environment Directory and AccountsObject plus one SQLite-backed GameObject per stable game_id; DO-100 private bindings are GAME_DIRECTORY/ACCOUNTS/GAMES. Axum dispatches to these owners; it does not replace them or hold authoritative global state. | DO-001/096/100/101; LLD-027; [Durable Object design](durable-object-design.md#object-boundaries) |
| Global limits | Exactly one nonterminal New/Awaiting Players/In Progress game application-wide. Claim the sole Directory reservation at New creation, keep it through start, and release after Resolved/Cancelled; terminal History is excluded. Separate Objects are not a shared transaction. | HLD-012, HLD-039 (superseded), HLD-077 |
| Exact lifecycle | **New → Awaiting Players → In Progress → Resolved** for a winner. **New**, **Awaiting Players** or **In Progress → Cancelled** for a no-winner ending. Neither terminal state reopens. | HLD-027, HLD-036–HLD-038 |
| Configuration and start | Configuration changes only in New; no Awaiting Players → New. Start needs at least two distinct currently connected players with valid sessions and the same global reservation still held by this game. Spectators never gate start. | HLD-038, HLD-040, HLD-048, HLD-077 |
| Capacities | Integer player limit 2–20, default 20; spectator limit 0–50, default 50. Zero spectators disables spectator admission. | HLD-018, HLD-046–HLD-048 |
| Values and boards | String values; initial pool `1..=numeric_upper_bound`, with positive upper bound 1–1,000 inclusive and default 75 (DO-042/043). At most 1,000 distinct accepted call records/game; no preallocated pool array. DO-044: for `N` values and `k = side² − free cells`, require `N ≥ k` and at least as many layouts `P(N,k)` as all retained start-time memberships (count capped at roster size); disconnected retained players receive boards. The separate two-connected-player Start gate remains. Square side 2–10, default 5; generated boards contain no repeated value and are cell-for-cell distinct. | HLD-025, HLD-033–HLD-035; BR-025, BR-028, BR-029 |
| Free cells and assignment | Optional valueless satisfied cells, enabled by default with one cell at one-based `(ceil(height / 2), ceil(width / 2))`. Locations configurable up to the whole board. DO-044 counts only non-free cells in `k`; reject Start if enough distinct boards are impossible. Assign/persist boards at accepted start, never on lobby entry/recovery. | HLD-034, HLD-035; DO-042 approves the GameConfiguration representation/defaults and disabled-empty-position invariant; DO-044 approves the exact feasibility criterion. |
| Progression and winner | Backend-only automatic matching after accepted random/manual undrawn values; players never mark. Single Line means a complete row, column or either full corner-to-corner diagonal. Separate Rust traits per winning pattern; DO-063 approves `SingleLinePattern::evaluate(side_length: u8, cells: &[BoardCell]) -> Result<Vec<CompletedLine>, PatternEvaluationError>`, with board-shape validation and deterministic all-line ordering. Qualification is not an award; an authorized operator submits one qualifying winner after the real-life Bingo exchange. | HLD-023, HLD-024, HLD-026, HLD-042, HLD-074 |
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
| Player during play | DO-047 approves Leave/disconnect retains seat, board, matching and winner eligibility. Return uses a still-valid session until its fixed expiry or approved answer proof; no replacement/new player admission. DO-048 keeps the latest explicit-Leave timestamp after return as historical event data, not current presence. DO-052 approves recovery’s serialized optimistic revalidation, session-epoch replacement and socket fencing; DO-055 approves the session schema/access. | DO-047/048/052/055 policy approved; DO-056 terminal player-grant/Exit policy and DO-058 account-view downgrade/cross-session Exit are approved; physical integration remains TBD. |
| Spectator departure | DO-047 approves explicit Leave releases seat and revokes/deletes session immediately. Accidental disconnect has five-minute grace; no seat/session renewal by pretending to be another role. DO-053 approves the spectator record and paired five-minute grace timestamps. DO-054 approves trusted-time disconnect/reconnect/expiry ordering, stale-event guards and fresh admission after expiry. DO-055 approves spectator-session binding/access. | DO-047/053/054/055 policy/schema approved; hibernation reconstruction and physical integration remain TBD; DO-056 terminal cleanup and DO-058 account-view policies are approved. |
| Pre-start role switch | DO-047 approves both directions within target capacity. Player→spectator releases alias/seat and deletes recovery proof; spectator→player requires an available alias/seat and creates fresh membership/optional answer. Failure preserves original role/seat/proof. Replace role-bound session at its original fixed expiry without extension. DO-055 approves the participant-session binding/access model. | DO-047/055 policy approved; DO-056 terminal player-session policy and DO-058 account-view downgrade/cross-session Exit are approved; physical transaction/fencing remains TBD. |
| Account/session credentials | Single-use enrollment/reset links last one day from issuance; sessions have fixed independent one-day expiry, not sliding expiry. Backend and open sockets enforce expiry/revocation. Account logins may coexist. | TBD |
| Player answer recovery | Optional private answer; code + current alias + matching enrolled answer replaces old player sessions/sockets, restores same membership in Awaiting Players/In Progress. No forgotten-alias lookup or alternative proof. Valid-session users may set/replace/delete their answer in those states. DO-049 approves private versioned verifier-record fields/privacy; DO-050 approves v1 Unicode NFC → outer Unicode-whitespace trim → case-fold, preserving internal whitespace/punctuation; future version changes require authenticated replacement and no silent verifier migration. | DO-049–052 policy and verifier profile approved; production costs, exact Unicode implementation, throttling thresholds/durable mechanism and physical integration remain TBD. |
| Unstarted abandonment | After 24 hours without qualifying designated-host activity, commit Cancelled and delete game/participant data, no History. DO-041: serialized trusted-time comparison, end-exclusive deadline; exact-deadline expiry wins over Start/open/host action, no grace. Earlier renewal/Start may commit first; stale alarms reread/reschedule and cannot cancel In Progress or release a newer reservation. Physical alarm and cross-Object recovery remain TBD. DO-040: only designated-host qualifying activity renews; admin override does not. | HLD-068; DO-040/041 |
| Pre-start cancellation | Commit Cancelled, then delete game/participant data without History. Notify joined participants; independent exit does not delay deletion. Intentional cancellation requires confirmation. | TBD |
| Started-game History | DO-069 fixes immutable `GameHistorySnapshot`/winner/player fields and constraints; DO-070 fixes parent/call/player/cell child-row mapping/order; DO-071 fixes UTC three-calendar-month expiry/time-of-day/month-end clamp/end-exclusive denial; DO-072 approves atomic terminal materialization/obsolete-source cleanup. DO-073 approves best-effort connected-spectator delivery then server identity/session deletion without ACK wait; DO-074 approves primary purge/code reuse; DO-075 approves secondary-copy/index/restore expiry enforcement. No credentials, recovery/session/presence data, spectators, editable config or intermediate revisions enter History. | DO-069–075 policies approved; provider schedule/cleanup/restore integration remains implementation work. |
| Terminal player access | Delete recovery verifier; retain a minimal existing-session grant until Exit or original session expiry. No new terminal recovery session; pre-start deletion remains an exception with no reconnect. | DO-056 approves the minimal terminal record and participant Exit/replay/deletion guards; DO-058 approves account-view downgrade/cross-session Exit. Physical cleanup integration is TBD. |
| Terminal spectator access | Best-effort result delivery after terminal commit; do not wait indefinitely or require app ACK. Delete server identity/session data even if disconnected/delivery fails. Local delivered result may remain until Exit; no terminal reconnect/refresh recovery. | DO-073 policy approved; transport/cleanup integration remains TBD. |
| Host/admin accounts | No inactivity expiry. Authorized disable/delete revokes access and is blocked while the account hosts a nonterminal game; transfer/cancel first. DO-105 bars app self-disable/delete; DO-106 rejects disable/delete that would leave zero enabled Verified admins; DO-107 excludes role editing in first release. Preserve unrelated/unexpired History. | DO-105–107 policy approved; physical serialization/error mapping remains implementation work. |

Sources: HLD-020–HLD-022, HLD-032, HLD-036–HLD-045, HLD-061, HLD-064–HLD-072, HLD-074.

## 3. Component and module design

| Logical component | Confirmed responsibility | Detailed design to fill |
| --- | --- | --- |
| Shared Rust domain | Pure rules, board feasibility/generation/matching and pattern-specific trait boundaries. | UUID-v7 typed IDs and local `thiserror` errors follow Sections 3.3–3.4; other crates, types, traits, signatures, algorithms and concrete error variants **TBD**. |
| Shared contracts | Shared safe app/backend contract types without provider/UI dependencies. | TBD — serialization, versioning and module boundaries. |
| Shared Dioxus UI / `platform/web` | Web views, nonauthoritative presentation, browser adapters and protected-cookie interactions. | LLD-027 selects Dioxus Router and LLD-028 selects Dioxus-integrated Tailwind styling. Section 4 proposes screen routes/components and UI state boundaries; exact modules/signatures, adapters, path spellings and visual tokens remain review/implementation work. |
| Rust API Worker | One initial deployable API service; LLD-027 selects Axum HTTP dispatch through the Worker fetch entrypoint. | Section 3.5 defines router/asset/native-upgrade integration boundaries. Endpoint/DTO/error contracts remain in [api-design.md](api-design.md); exact SDK wiring and middleware remain unverified. |
| Game Directory Durable Object | Issued-code lookup and application-wide lifecycle coordination. | Approved Directory ownership and private GAME_DIRECTORY binding; [record/protocol decisions](durable-object-design.md#directory-records) remain authoritative. Concrete SDK dispatch, SQL and concurrency integration remain implementation work. |
| Per-game Durable Object | Authoritative game/board/membership state, SQLite writes and role-filtered hibernating sockets. | DO-096/100 approve GameObject per stable game_id, normalized local schema and private binding; DO-091–095 approve logical attachment/reconstruction/fencing/alarm/backpressure policies. Exact DDL, SDK handlers and deployment config remain implementation work. |
| Account/session authority | Strongly consistent credential lifecycle and server-owned account permissions. | DO-001/100 approve AccountsObject per environment and private ACCOUNTS binding. Axum session resolution uses this authority; no positive cross-request authorization cache. Exact [account protocol](durable-object-design.md#account-records) and SDK integration remain implementation work. |
| Developer CLI | Developer-restricted account operations through an authorized persistence/backend path. | TBD — language, packaging, commands, auth and private output delivery. |
| Later `platform/android` / `platform/ios` | Future Dioxus adapters/entrypoints without duplicating domain/contracts. | TBD — deferred; no native implementation in this release. |

The `uuid` and `thiserror` libraries are selected in Sections 3.3–3.4. Compatible pinned dependency/toolchain/SDK versions, target configuration and build commands remain **TBD**. This inventory does not create a repository scaffold.

### 3.1 API Worker organization — moved

The single-Worker design and endpoint organization live in [API design Section 2.1](api-design.md#21-one-api-worker-multiple-endpoints). LLD-016 remains confirmed. LLD-027 / [Section 3.5](#routing-design) now selects Axum HTTP routing and its integration boundaries; the API catalog/contracts remain in the companion, whose older router-choice TBD is superseded for that choice only.

### 3.2 API/state-owner boundary — moved

Worker/endpoint/Object distinctions and the namespace-versus-user authorization boundary live in [API design Section 2.2](api-design.md#22-worker-endpoint-and-durable-object-boundaries). LLD-027 preserves private state ownership and specifies Axum-to-owner/native WebSocket dispatch in [Section 3.5](#routing-design). Persisted records remain in [Durable Object design](durable-object-design.md#schema-proposal); LLD Section 5 is its navigation reference.

<a id="identifier-policy"></a>

### 3.3 Application-generated identifiers — UUID v7

**Confirmed user direction (LLD-020):** Use **UUID v7** for system identifiers whose generation is controlled by Brews Bingo application/developer code, including the developer CLI. This is the default for `GameId`, `AccountId`, `PlayerId`, `SpectatorId`, `SessionId`, `LinkId`, `CommandId`, `OperationId`, `ConnectionId` and `AuditId`, and for future comparable application-generated entity/operation identifiers. No compelling reason to use another UUID version has been identified for these IDs. Any exception requires an explicit, documented compelling reason and review; do not silently substitute another scheme.

Use the Rust **`uuid` crate** with its `v7` feature for generation; the crate documents UUID v7 support and a `Uuid` value representation.[1] `Uuid::now_v7()` is the standard current-time generation entry point where the chosen target supports it; it requires `std` and `v7`.[2] DO-102 approves trusted Worker UTC time for security decisions, UUID-v7 time as non-authoritative, and platform CSPRNG only with fail-closed error/no fallback. The crate’s compatible version/features and actual Worker/Dioxus/CLI clock/randomness integration remain **unverified** until target builds/tests.

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

<a id="routing-design"></a>
### 3.5 Routing design — Dioxus Router, Axum and native Durable Objects

**Selected direction (LLD-027, user-requested incorporation of the routing recommendation):** Use **Dioxus Router for browser navigation**, **Axum for HTTP routing inside the single Rust API Worker**, and **native Cloudflare Durable Object handlers/bindings for authoritative state, alarms and hibernating WebSockets**. This extends LLD-016 without introducing a Worker per endpoint, SSR, a conventional listening server or a new datastore. Library/boundary selection is settled here; exact crate versions, feature combinations, middleware ordering, deployment configuration and executed compatibility checks remain unverified.

**Scope and provenance:** Official routing/SDK documentation was consulted on 2026-10-04.[8][9][10] API operation IDs, methods/paths, DTOs and WSS envelopes remain owned by [API design](api-design.md#operation-catalog); this section records the library integration and dispatch boundaries, not a second endpoint catalog. The companion's older “router choice TBD” wording is superseded for library choice by LLD-027; editing that companion is outside this LLD-only update. Section 4's individual screen paths/layouts remain proposals, not automatically finalized by choosing a router.

#### 3.5.1 Routing layers and request flow

| Layer | Selected responsibility | Boundary |
| --- | --- | --- |
| Cloudflare ingress and Static Assets | Serve the static Dioxus build; send `/api` and `/api/*` to Worker code before asset handling. Use SPA fallback for browser screen navigation. | Serving HTML/Wasm is not an account or game authorization decision. No protected data embedded in the public static shell. |
| Dioxus Router in the browser | Typed `Routable` routes, navigation and layout selection for Home, Login, Games, Users, setup/lobby/Play and History. | Route guards improve UX but cannot authorize backend data or mutations. |
| Axum in the API Worker | Dispatch HTTP method/path; extract typed input; apply shared request processing; call the appropriate service/state owner; map safe responses/errors. | Router state is not the authoritative store for sessions, games, command receipts or membership. |
| Durable Object handlers and bindings | Reach `GAME_DIRECTORY`, `ACCOUNTS` and the correct `GAMES` instance using the DO-100 identities; revalidate domain rules and commit through the owning Object. | Private bindings and stable IDs are not end-user proof. No public Object access or cross-Object SQL transaction is added. |
| Native GameObject WebSocket and alarm handlers | Accept authorized hibernating connections, reconstruct authority, enforce per-frame checks and process persisted deadlines. | Axum HTTP routing does not replace DO-091–095 hibernation, fencing, alarms or bounded delivery. |

Dioxus documents a typed `Routable` enum and `Router` component for URL parsing, navigation and rendering.[8] Axum provides method/path routing, typed extractors and Tower middleware; `workers-rs` documents serving an Axum router through its fetch event using the `http` feature.[9][10]

```text
Browser navigation: GET /users
  -> Cloudflare Static Assets / SPA shell
  -> Dioxus Router selects Users
  -> validated session/capability check for presentation
  -> GET /api/users to load protected data

API request: GET /api/users
  -> Rust Worker fetch entrypoint
  -> Axum method/path dispatch and authoritative admin check
  -> AccountsObject -> safe response (never credential records)

Game command: /api/games/{game_id}/...
  -> Axum handler -> owning GameObject through private binding
  -> current authority/lifecycle/revision checks -> durable commit
  -> safe HTTP result and permitted post-commit updates

Game stream upgrade: GET /api/games/{game_id}/stream
  -> Worker upgrade dispatch and session/Origin validation
  -> owning GameObject native WebSocket acceptance/handlers
  -> authorized initial snapshot, hibernation and subsequent updates
```

A host can load the same static application shell as an admin, but must still be denied `/api/users`. Similarly, changing a screen URL or player ID cannot expose another player's board. Unknown codes must not instantiate games; Directory discovery and owner selection retain the approved stable-ID and lifecycle rules.

#### 3.5.2 Static assets, SPA fallback and same-origin API routing

Use a same-origin browser/API arrangement for the initial web deployment, preserving the host-only account cookie and exact-Origin checks. Reserve `/api` and its descendants for backend responses, including the existing proposed game-stream upgrade path. The selected asset configuration shape is:

```toml
# Illustrative configuration shape only; not an existing deployment file.
[assets]
directory = "<Dioxus web build output>"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/api", "/api/*"]
```

`ASSETS` is the static asset collection binding, not another Durable Object namespace or persistence store. Resolve the actual build directory, generated Rust Worker entrypoint and compatibility date when creating a separately authorized deployment configuration. Cloudflare supports SPA fallback and selective Worker-first path patterns; its documentation specifically recommends this split for SPA/API applications.[12][13]

- A direct navigation or refresh of a screen path such as `/games/{game_id}/play` must load the app shell, then let Dioxus resolve the route and revalidate session/game state. A client-side unknown screen renders a Not Found experience, not a fabricated game.
- API requests must reach backend dispatch even when typed into the address bar as navigation requests or when their path could match a static file. Include the exact `/api` root as well as `/api/*`; do not exempt privileged API paths with negative asset patterns.
- Unknown API paths return an API not-found response; unsupported methods return an appropriate method error under the final contract. **Never send an API failure to SPA fallback**, redirect it silently to the login HTML page or return `200 index.html` as a JSON/API result. Authentication failures remain API responses interpreted by the frontend.
- Do not blanket-enable Worker-first processing for every static asset merely to authenticate the public shell. Cloudflare distinguishes direct asset serving from Worker invocations, and SPA navigation behavior depends on compatibility settings; pin and test the chosen configuration rather than relying on an implicit default.[12][13]
- If Worker code explicitly serves non-API assets through `ASSETS`, guard that fallback by request class/path so API and upgrade requests cannot enter it. Missing asset handling and MIME types must not deliver HTML where JavaScript/Wasm is expected.
- The public shell must contain no personalized bootstrap data or secrets. Protected API responses retain their access/cache rules; link pages retain DO-028 fragment clearing, no-store, no-referrer and no-third-party constraints. Loading a SPA page does not weaken API authorization.

#### 3.5.3 Axum handler organization and middleware

Use a thin `#[event(fetch)]` entrypoint that passes ordinary HTTP requests into the Axum router/service. The documented Workers integration calls the router with the request and returns its response; **do not introduce `TcpListener`, `axum::serve` or a conventional `#[tokio::main]` server loop** for this architecture.[9] This does not prohibit compatible runtime utilities; any timer/task middleware still needs Worker-target validation.

Group routes by responsibility, with module names below illustrative rather than a selected repository scaffold:

| Router/handler group | Responsibility | Authorization placement |
| --- | --- | --- |
| Authentication and sessions | Login, current session, enrollment/reset and logout. | Public or restricted-purpose entry with applicable Origin, proof, expiry and abuse checks; not an unconditional normal-session middleware gate. |
| Users/account management | Listing, provisioning and guarded account actions. | Enabled Verified admin; preserve self-removal, last-admin and hosted-game guards at the owning mutation. |
| Game discovery/configuration | Code lookup, current/list/read, create, configure, publish and transfer. | Public discovery returns only its limited projection; account reads and designated-host/admin mutations are distinct policies. |
| Participants and recovery | Join, restore, rename, switch, Leave and answer maintenance. | Participant or recovery-proof checks, not account-only authorization; no arbitrary client role/identity trust. |
| Gameplay | Authorized reads, Start, calls, winner and no-winner ending. | Role-specific projections and current designated-host/admin permission; owner rechecks state/qualification at commit. |
| Results and History | Bounded final-view access/Exit and account-only unexpired History. | Final-view grants and History permission remain different; no new terminal admission. |
| Game stream upgrade | Recognize and forward the approved upgrade to the correct GameObject. | Session, exact-Origin, registration and native upgrade boundary in Section 3.5.4. |

Use Axum extraction for typed path/query/JSON inputs, but validate domain meaning explicitly: parsing a UUID does not check UUID v7, authorize an actor or prove that a game exists. Keep transport DTOs separate from credential-bearing storage structs. Use component-local `thiserror` types and deliberate safe HTTP mappings rather than exposing raw SDK errors.

Shared request processing covers approved Origin checks, bounded request bodies, authoritative session resolution, route-specific capability checks and safe error/correlation handling. Axum's Tower integration supplies middleware composition; exact compatible layers, order, numeric limits and public error envelopes remain implementation/contract decisions.[10] Apply the correct checks to login/link/participant routes instead of locking all `/api/*` behind an account session. Do not use permissive credentialed CORS or browser route guards as a substitute for backend authorization.

Keep handlers thin: parse/validate transport, derive the authenticated actor, call the responsible service/owner and format the result. The owner still validates enabled/lifecycle/session epoch, assignment, expected revision, command identity, game state, capacity and winner eligibility at its commit boundary. Middleware success cannot authorize a later conflicting mutation. `AppState` may carry request-safe configuration and binding/service handles, not mutable global gameplay or a cross-request positive authorization cache.

**Choice rationale:** Axum's extractors, composed routers and middleware fit the planned multi-area API better than a large manual dispatch function. The built-in `worker::Router` remains a documented method/path router with shared data, but is not the selected top-level API router; no second parallel HTTP router is required just for ordinary endpoints.[10][15] Axum does not select storage, idempotency, RPC encoding or stateful scheduling.

#### 3.5.4 Native Durable Object WebSocket boundary

Keep the existing proposed `GET /api/games/{game_id}/stream` endpoint rather than inventing a separate public socket namespace. Worker dispatch validates the request/Origin/session and forwards through the private binding; the owning GameObject completes authoritative registration and accepts the server socket through Cloudflare's hibernation-aware API. Cloudflare recommends that API and documents forwarding the upgrade from a Worker to a Durable Object.[14]

Do not substitute Axum's conventional WebSocket task loop for the GameObject's native acceptance and wake/message/close handlers. Preserve DO-035/036 registration and per-frame account checks, DO-055/093 participant-session fencing, DO-068 snapshot-before-update ordering, DO-091/092 attachment reconstruction and DO-094/095 alarm/delivery rules. Account-session and participant-session validation remain distinct even though the URL is shared.

The upgrade path must preserve the provider's WebSocket-bearing response and successful handshake status across Rust/HTTP conversions. Whether dispatch forwards this branch before ordinary Axum body handling or uses a validated Axum-compatible adapter remains an integration choice. Never wrap a successful upgrade in JSON, drain it through body middleware or treat a failed upgrade as an asset request. Test that conversion and close behavior explicitly; platform documentation alone does not establish that the selected Rust feature combination works.

#### 3.5.5 Worker/Wasm integration and remaining validation

- Enable the selected `workers-rs` standard-HTTP integration and a compatible Axum feature set. The official example uses `worker` features `http`/`axum` and disables Axum's default features while selecting the needed extractors; it is a reference, not an approved dependency pin or permission to copy native-server defaults.[9][11]
- Axum handlers/state impose `Send` requirements that can conflict with JavaScript-backed SDK values or futures. `workers-rs` documents `SendFuture`, `SendWrapper` and `#[worker::send]` helpers for its environment.[9] Use them only where required and valid for the pinned SDK/runtime; do not add ad hoc unsafe `Send` implementations or assume native thread safety.
- Pin and validate Dioxus, Axum, Tower, `http`/body types, Worker SDK, build tooling and compatibility date together. Check feature compatibility, bundle size, startup and request overhead on the actual target; no cost/performance claim follows merely from adopting Axum.
- Preserve multiple `Set-Cookie` headers, status codes, content types, safe error bodies, binding access and credential redaction through adapters. Verify timeout/cancellation semantics: an HTTP timeout may follow a committed mutation, so the original command/receipt recovery rules still apply.
- Concrete API schemas/requiredness, middleware ordering, per-route request limits, module signatures, native upgrade conversion and final frontend path spellings remain **TBD**. The single-Worker topology, chosen routing libraries and native state-owner boundaries are **selected**, not tested. No application code, dependency, configuration file, resource or deployment is created by this section.

#### 3.5.6 Routing acceptance scenarios — not executed

| ID | Future verification |
| --- | --- |
| ROUTE-AC-01 | Open/refresh Home, Users and a nested game screen: static shell plus Dioxus route resolution works; screen loading itself reveals no protected data. |
| ROUTE-AC-02 | Request `/api`, an unknown `/api/*` path and a wrong method, including navigation headers and a colliding asset path: backend dispatch/error response wins; never SPA HTML. |
| ROUTE-AC-03 | Exercise anonymous, restricted, host, admin, player and spectator contexts against public/restricted/protected routes; direct API calls cannot bypass Origin, authority or owner commit-time rules. |
| ROUTE-AC-04 | Verify typed parsing/domain validation, bounded body rejection, component-error redaction, preserved cookie headers and same-origin session behavior through the Worker/Axum adapter. |
| ROUTE-AC-05 | Upgrade the game stream through the Worker/DO boundary: preserve the native WebSocket response, registration/snapshot order, fail-closed rejection, wake reconstruction, expiry and replacement fencing. |
| ROUTE-AC-06 | Simulate timeout/lost response after a committed command; recover the same result without another draw/create/award or replayed credential secret. |
| ROUTE-AC-07 | Check genuine static asset delivery, missing assets, frontend Not Found, and SPA/API prefix separation under the pinned compatibility date; no HTML masquerading as Wasm/JS/JSON. |
| ROUTE-AC-08 | Build and exercise the pinned Worker/Wasm feature combination and selected middleware under the approved DO-103/104 compatibility/capacity gates. Record results; none are claimed here. |

<a id="styling-design"></a>
### 3.6 Dioxus-integrated Tailwind styling

**Confirmed user direction (LLD-028):** Style the initial desktop/mobile web UI with **Tailwind CSS in Dioxus components**, using the Dioxus build/asset workflow. This selects the styling approach; it does not select a component library, a color palette, design tokens, theme modes or exact package versions, and does not authorize application implementation.

- Write Tailwind utility classes on Dioxus RSX elements and encapsulate repeated visual patterns in the shared components from Section 4.2. Maintain accessible semantics, focus/disabled/error states, board match/qualification distinctions, responsive layouts and venue readability independently of styling classes.
- Compile Tailwind during local development and the production build; deliver the generated stylesheet with the Dioxus static assets. Do not generate CSS in the API Worker or use a browser/CDN Tailwind compiler in production. Axum and Durable Objects keep their backend responsibilities unchanged.
- Dioxus's current tutorial documents automatic Tailwind CLI execution during serve/build when a `tailwind.css` input is present at the app root, with output in the assets folder.[16] Its styling guide shows RSX source scanning and linking generated CSS through `document::Stylesheet` / `asset!`.[17] Prefer the built-in Dioxus workflow; the guide's separate manual watcher is an alternative, not a second required compiler. Pin and validate one build path with the selected Dioxus CLI/Tailwind versions before implementation.
- Ensure source discovery covers every shared Rust/RSX component used by the web build, not just the web entrypoint. Choose complete class strings for conditional variants, or an explicit inclusion mechanism supported by the pinned Tailwind version; production builds must retain every reachable state style. Exact workspace paths and CSS input/output filenames remain implementation details.
- Palette, typography, spacing, breakpoints and any theme tokens remain design work under LLD-001/026. Tailwind is the selected styling foundation, not a replacement for component behavior, backend authorization or the UI acceptance scenarios. No additional UI kit or custom CSS policy is selected here.
- Before release, verify production CSS loading on direct/nested navigation, responsive and venue views, keyboard focus/contrast/non-color status cues, and all conditional board/validation/connection states. These are future checks, not completed browser or build tests.

## 4. Views and frontend contracts

**Design status:** Concrete UI design proposals captured at the user's request (LLD-026), not implemented screens or a new approval of the whole LLD. The requested public **Home**, privileged **Games**, admin-only **Users**, **Create game**, and active **Play** experiences are included, with enrollment, admission, lobby, recovery, results and History completing their flows. Approved domain/security rules remain mandatory. Suggested routes, component names, layout choices and interaction details below are reviewable UI proposals; they do not finalize API payloads, add backend permissions or authorize implementation.

**Source boundary:** Read this section with [HLD app experiences](hld.md#41-experiences-and-platform-delivery), [API operations](api-design.md#operation-catalog) and the [approved DO ledger](durable-object-design.md#item-review). Later approved decisions govern over stale companion summaries. This section does not rewrite the business requirements or add public self-registration, role editing, player impersonation, export, offline play or new winning patterns.

### 4.1 Screen inventory and navigation

Preserve VIEW-01–VIEW-16 as stable experience IDs; add **VIEW-17** for the public Home page. Multiple IDs may share a page shell, and some are dialogs or nested panels rather than separate pages. Paths below are **frontend routes**, not new HTTP API endpoints. `{game_id}` and `{account_id}` are stable nonsecret IDs; a route, code or client-selected tab never establishes authority.

| ID | Screen / experience | Proposed entry | Access and destination |
| --- | --- | --- | --- |
| VIEW-17 | [Home](#view-17) | `/` | Public default entry: game-code field, Join, and host/admin login. No public game roster or History list. |
| VIEW-01 | [Account login](#view-01) | `/login` | Shared host/admin sign-in; successful normal session opens Games. |
| VIEW-02 | [Enrollment and password setup](#view-02) | `/enroll` via private link | Valid enrollment link/restricted session only; completed setup opens Games. |
| VIEW-03 | [Password reset](#view-03) | `/reset-password` via private link | Valid reset link/restricted session only; completion returns to Login, not automatic login. |
| VIEW-04 | [Games dashboard](#view-04) | `/games` | Enabled, Verified hosts/admins; current game plus historical games and Create game entry. |
| VIEW-05 | [Create game / New game setup](#view-05) | `/games/new`; `/games/{game_id}/setup` | Hosts/admins create; only designated host/admin edits an existing New game. Other hosts inspect read-only. |
| VIEW-06 | [Host/admin lobby](#view-06) | `/games/{game_id}/lobby` | Account-authorized Awaiting Players view; only designated host/admin controls Start, Cancel and Transfer. |
| VIEW-07 | [Host/admin Play](#view-07) | `/games/{game_id}/play` | Account-authorized In Progress view; operator controls or read-only host inspection. |
| VIEW-08 | [Join and role selection](#view-08) | `/join/{code}` after Home lookup | Limited discovery, then explicit player/spectator admission or verified restoration. |
| VIEW-09 | [Player lobby](#view-09) | `/games/{game_id}/lobby` | Existing player session in Awaiting Players; no board yet. |
| VIEW-10 | [Player Play](#view-10) | `/games/{game_id}/play` | Own persisted board, automatic matches and own qualification only. |
| VIEW-11 | [Recovery / answer settings](#view-11) | `/recover`; nested player settings panel | Public proof-entry form versus authenticated own-answer maintenance are separate modes. |
| VIEW-12 | [Spectator / venue display](#view-12) | Authorized lobby/play shell; in-page Display mode | Spectator admission within capacity, audience projection only. Display mode grants no extra access. |
| VIEW-13 | [Result / cancellation notice](#view-13) | `/games/{game_id}/result`; transient notice | Existing eligible final-view grants only; spectator result is already-delivered local state. |
| VIEW-14 | [History list / detail](#view-14) | History section on Games; `/history/{game_id}` | Hosts/admins only, unexpired started-game snapshots; never resume gameplay. |
| VIEW-15 | [Users / account management](#view-15) | `/users`, `/users/new`, `/users/{account_id}` | Verified admin only; safe account metadata, provisioning and guarded management. |
| VIEW-16 | [Connection / access / error boundary](#view-16) | Shared across routes | Loading, syncing, reconnecting, denied, expired, unavailable and command-outcome states. |

**Primary journeys**

- Attendee: **Home → Join → Player lobby or Spectator view → Play → Result → Exit to Home**. In Progress closes new player entry; existing-player recovery is separate from a new join.
- Returning player: **Home/Join → verified existing membership**, or **Recover → same membership** using game code, exact current alias and previously enrolled answer. Neither path regenerates a board.
- Host/admin: **Home → Login → Games → Create game → New setup → Open lobby → Start → Play → Result → Exit to Games**. Creation, lobby publication and Start are distinct actions/states.
- Historical inspection: **Games → History detail → Games**. No transition from History into active play.
- Admin: **Games → Users → account detail / Create account → private link handoff**. The recipient follows the separate enrollment/reset flow. First-admin bootstrap stays outside the app in the developer CLI.
- Opening a saved game route resolves current backend lifecycle and authorization before choosing its screen. Route a New game to setup, Awaiting Players to lobby, In Progress to Play, and a terminal game only to a permitted result/History destination. Never create a replacement game or follow a reused code into a new membership automatically.

### 4.2 Shared layout, components and state rules

#### Application shell and role-aware navigation

**Styling foundation:** All proposed screens and shared presentation components use the Dioxus-integrated Tailwind approach in [Section 3.6](#styling-design). Visual tokens/layout details remain reviewable; the styling framework is selected, not TBD.

- **`AppShell` / `BrandHeader`:** visible **Brews Bingo** title and **Rockville Brews** identity, one clear page heading, Home navigation, and a compact session/connection area. Public Home remains accessible rather than automatically replacing `/` with Games.
- **`AccountNavigation`:** Games for normal host/admin sessions; Users only for normal admin sessions; account username, role badge and **Log out**. Before login show **Host / admin login**. This includes the requested admin login without removing the approved host login path. Restricted enrollment/reset sessions do not get privileged navigation.
- **`GameHeader`:** lifecycle label, published code if any, caller's role/view mode, and permitted designated-host reference. Use a safe account-display projection when provided; ordinary hosts must not query admin-only Users just to render a host name. New games have no code yet; do not introduce a persisted game-title field merely for a heading.
- **`CapabilityGate`:** render controls from current server-derived permissions plus lifecycle, not route names or a client role selector. Read-only host/admin viewing is not a participant admission and does not consume player/spectator capacity. The board and membership panels must use the explicitly authorized account or participant context; never combine projections accidentally when both sessions exist.
- On narrow screens use a single-column layout, labelled navigation menu and full-width primary actions. On wider screens use a main content column plus a secondary rules/roster panel. Play can use a wider control/board split; venue mode dedicates most space to calls. Breakpoints are content-driven implementation details, not new platform scope.

#### Reusable presentation components

| Proposed component | Responsibility and limits |
| --- | --- |
| `GameCodeForm` / `GameCodeBadge` | Accessible code input and validation; large selectable/copyable published code. Copy is user-initiated and failure leaves selectable text. A code is not an authentication token. |
| `LifecycleBadge` / `PermissionBanner` | Exact New, Awaiting Players, In Progress, Resolved, Cancelled labels; distinguish designated operator, admin override and read-only viewer. |
| `RulesSummary` / `CapacitySummary` | Pool bounds, square size, free-cell configuration, Single Line objective and permitted player/spectator counts. State which counts are connected versus occupying seats. |
| `BoardGrid` | Read-only row/column-labelled cells; distinct FREE, matched and unmatched treatments; completed-line highlighting only from authorized backend evaluation. Setup uses a separate interactive free-cell editor, not this player board. |
| `CurrentCall` / `OrderedCallList` | Latest committed value prominently; complete ordered record with sequence labels and an explicit no-calls state. Preserve string values and call order. |
| `PlayerRoster` / `BoardInspector` / `QualificationList` | Account-only roster/presence and private boards/qualifiers; player screens use own-board/own-qualification components, spectators receive neither. |
| `CommandFeedback` / `ConfirmationDialog` | In-flight, committed, rejected, unknown-outcome and pending-coordination feedback; target-specific confirmations and retry discipline. |
| `ConnectionBanner` / `AccessBoundary` | Connecting/Synchronizing/Live/Reconnecting, freshness/expiry warnings and safe exits; never equate socket liveness with current authority. |
| `EmptyState` / `InlineError` / `LoadMore` | Distinguish legitimate empty data from failed/forbidden loads; explicit retry, no fabricated totals or client-made records. Paging controls require an agreed server contract. |
| `PrivateLinkHandoff` | Newly issued enrollment/reset link and expiry for the authorized issuer only; never part of lists, history, notifications, analytics or retry receipts. |

#### Form, command and navigation behavior

1. Client validation explains the same rules as the backend; backend checks remain decisive. Keep rejected nonsecret edits available for correction, focus the error summary/field, and never partially present a rejected mutation as saved.
2. Show action-specific pending labels and suppress duplicate clicks. Accept only committed responses/snapshots as game truth; do not optimistically draw, mark, award, admit, switch roles or announce success.
3. An interrupted response is **unknown outcome**, not failure. Resolve the original actor/command context through the approved retry/result mechanism (G4 where available); do not create another random call or another game with a fresh command ID. Reject expired-command replay rather than executing it anew. Credential issuance has separate no-secret-replay recovery (DO-029/080).
4. Refresh snapshots after stale revisions, concurrent Start/transfer/terminal changes or capacity conflicts. Remove invalid controls immediately; close an obsolete winner/transfer dialog. Do not report socket closure, purge or reservation release complete merely because work was queued.
5. **Back, Leave, Exit and Log out are different.** Leaving an unsaved creation form discards only the local draft after a proposed discard confirmation; it does not create or cancel a game. Navigating away from a saved New/lobby/live game does not implicitly cancel it. Explicit participant Leave follows role/state semantics; terminal Exit retires the applicable grant; Log out revokes the current account session, not every login and not the game outcome.
6. Do not treat refresh, tab close, browser Back, socket loss or hibernation as a successfully committed Leave/Exit. App-provided terminal navigation should run Exit first and handle an unknown result honestly. Browser/history navigation always revalidates access. Exact unload/multi-tab wiring remains an implementation detail; it must not relax grant expiry or post-Exit denial.
7. Passwords, recovery answers and raw access links stay out of route/query state, persistent browser storage, logs and analytics. Keep sensitive form values only in the active form, clear them on success, dismissal, expiry or navigation, and never rehydrate them from a receipt. Protected game/account/History data is not an offline archive; clear unauthorized views on logout, expiry, revocation or role/context change.

#### Accessibility, mobile and venue design targets

**Proposed acceptance targets, not a claim of compliance:** semantic landmarks/headings, persistent field labels, keyboard-operable controls, visible focus, screen-reader error/status announcements, sufficient contrast and touch-sized actions; use WCAG 2.2 AA as the implementation review target. Test actual browser/assistive-technology support later.

- Do not encode role, connection, matches or qualification by color alone. Give each board cell a readable row/column, value or FREE label, and match state. Player cells are not mark buttons.
- In the setup free-cell editor, use keyboard-operable selected/unselected cells with coordinate labels and a text summary. Board previews must not announce random or assigned player values.
- Confirmation dialogs state the target and consequence, focus safely, allow dismissal before submission and restore focus. Avoid generic “OK” for destructive actions; use **Cancel game**, **End without winner**, **Delete account**, or **Transfer host**.
- Announce newly committed calls/qualification changes concisely rather than re-reading an entire board on each frame; do not move focus on passive updates. A user navigating older calls is not forcibly scrolled away.
- Large boards/call histories remain inspectable on mobile without shrinking text into unreadability; allow an explicit zoom/scroll region with equivalent accessible cell descriptions. Venue mode emphasizes large high-contrast current value, recent calls and objective, with an accessible full ordered list still available.
- Optional spoken announcements are required by BR-020. The suggested device-local controls and failure behavior in VIEW-12 remain a proposal for LLD-014; no global audio controller or automatic multi-device coordination is implied.

### 4.3 Screen specifications

<a id="view-17"></a>
#### VIEW-17 — Home / public entry

**Purpose/access:** Default page for everyone, including attendees without accounts. Sources: user-requested Home, BR-001/017, HLD-017/056–058. Contract touchpoints: A2, C4 and D3; lookup is not admission.

**Components/layout**
- Branded header, concise “Join a game at Rockville Brews” heading, and a central `GameCodeForm` with **Game code** label, text field, helper text and primary **Join** button. Submit by button or Enter.
- Header **Host / admin login** button; for an already validated normal account show **Games**, plus **Users** only for admins. Keep attendee entry usable in either case.
- Secondary **Recover player access** link to VIEW-11. An optional **Continue your game** card may appear only after validating the caller's existing session/membership; a remembered code alone cannot produce a resume grant.

**Behavior/states:** Trim outer ASCII spaces, accept case-insensitive code input and display its uppercase form; require exactly eight A–Z/0–9 characters and reject internal spaces. Do not silently truncate pasted values. Show inline format errors, a checking state, and a safe unavailable/not-joinable response for invalid/closed codes under the final error contract. Successful lookup opens VIEW-08 or restores verified existing membership; it never creates a game or seat itself. Network failure leaves the code editable with Retry. Do not list public live-game participants or historical games.

<a id="view-01"></a>
#### VIEW-01 — Host / admin login

**Purpose/access:** One account login form for both roles; no role picker, self-registration or public admin bootstrap. Sources: HLD-061/063/074–075, DO-020/021/030–034. Contracts: A1, A2, A7.

**Components:** Username, password, a proposed show/hide-password control, primary **Log in**, **Back to Home**, and help text directing users needing setup/reset to an authorized admin/developer for a link. Use password-manager-compatible fields; show current-session status without reading HttpOnly cookies.

**Validation/navigation:** Trim boundary ASCII whitespace from username, preserve its remaining case, apply 10–50 ASCII characters/no internal ASCII whitespace, and compare case-sensitively. Preserve the password exactly; do not trim, case-fold or silently truncate it. Password policy is Section 9.1; control-character browser/transport behavior and maximum size remain unresolved, not silently narrowed by a field widget. Successful normal login opens Games (or a revalidated same-app protected destination); do not replay a previously failed mutation after login.

**States:** Pending submission, safe generic login failure, throttled/retry-later, expired session and unavailable service. Public messages must not enumerate account existence, disabled status or reset state unless the final safe contract explicitly permits that distinction. Restricted sessions go only to their authorized setup/reset flow. Logging out clears account-private UI state without ending the game or inventing an all-sessions logout feature.

<a id="view-02"></a>
#### VIEW-02 — Enrollment / first password setup

**Purpose/access:** Complete a provisioned account's mandatory first password before Games/Users access. Sources: HLD-009/010, DO-028/029/032. Contracts: A3, A4, A2.

**Components:** Link-validation progress, minimal authorized account identity/purpose, new-password input and proposed confirmation input, policy help, show/hide control, **Set password**, and a safe return/help action. No role editor or admin-created initial password.

**Behavior/states:** Read the private link fragment, promptly clear it with `history.replaceState`, and redeem via protected POST; keep tokens out of queries/logs/analytics and load no third-party scripts on this page. A valid restricted session shows the form; match confirmation locally and submit only the actual password. Generic used/expired/revoked/invalid-link feedback directs the recipient to obtain a fresh authorized link. Do not turn loading, refresh or retries into multiple redemptions.

On committed completion, the account becomes Verified and its distinct normal session retains the restricted session's original absolute expiry; navigate to Games. If completion committed but its response/cookie was lost, offer normal password login, not replay of a bearer or an extra setup transaction. If still pending and restricted authority was lost, use authorized link reissue. Preserve the no-store/referrer restrictions.

<a id="view-03"></a>
#### VIEW-03 — Password reset

**Purpose/access:** Privileged-issued reset link, not email-based self-service or anonymous password recovery. Sources: HLD-064/065, DO-028/029/033. Contracts: A5, A6, A2.

**Components:** Same secure link boundary and password/confirmation components as enrollment, purpose-specific copy, **Set new password**, and login/help navigation.

**Behavior/states:** Require a valid reset-only session; preserve exact password input and all account policy. Explain that the previous password is unavailable while reset is pending, and link expiry does not restore it. On successful completion retire reset authority, clear the restricted cookie and navigate to Login with a nonsecret success message; **do not auto-login**. A lost committed response is recovered through password login; if reset is still required, an authorized issuer supplies a successor link. Generic invalid-link and throttling states mirror enrollment without exposing account secrets.

<a id="view-04"></a>
#### VIEW-04 — Games dashboard

**Purpose/access:** Landing page after host/admin login, containing both the current game and historical games as requested. Sources: HLD-028/029/032/077, DO-014–016/057/058/071. Contracts: C1, C2, C9 and F3/F4.

**Components/layout**
- Page heading **Games**, primary **Create game** navigation button, shared account navigation and refresh/load feedback.
- **Current game** card, at most one: state, issued code or “Not published”, safe designated-host reference, relevant timestamps and server-derived access mode. Render **Resume setup**, **Open lobby**, or **Open play** for the selected lifecycle; non-designated hosts get **View read-only**, while admins see an explicit cross-game operator banner.
- **Historical games** section beneath it: outcome, game code, ended time, winner alias only when Resolved, expiry and **View history**. Empty copy distinguishes “No current game” from “No unexpired historical games”. See VIEW-14 for detail.

**Behavior/states:** The Create game button opens the form; it does not create on navigation. When a nonterminal slot is occupied, disable creation with a visible reason and link to the existing authorized view. Include a preparing/reconciling state if creation/release is pending; a missing summary is not proof the slot is free. Backend creation still resolves races. Historical rows remain available while a new game is running. Do not count terminal History toward the active-game limit, expose deleted pre-start games as History, or label a terminal game “Resume”.

Propose newest-ended-first History and a **Load more** control when the API supports it; pagination/sort semantics remain a contract task. Do not fetch all History to fake paging, expose an unverified total, or treat failed loading as an empty list. Account-session expiry replaces protected content with Login guidance, not an attendee join.

<a id="view-05"></a>
#### VIEW-05 — Create game and saved New-game setup

**Purpose/access:** Configure before inviting participants. Sources: HLD-025/033–040/046–048/068/077; DO-038–044/062. Contracts: C2, C5–C8, C10 and E10.

**Components/layout:** A labelled configuration form with a side-by-side rules/free-cell preview on wide screens and stacked preview on mobile. Before creation, label it **Create game**; after a successful New record exists, label it **Game setup — New**, show save state and the server idle deadline, and provide **Back to Games** without cancelling.

| Control | Default and input behavior |
| --- | --- |
| Numeric pool upper bound | Integer 1–1,000, default 75; fixed lower bound 1 displayed as non-editable help. Values remain strings in game data. |
| Board size | One square side-length selector, integer 2–10, default 5; show the resulting square dimensions rather than independent width/height controls. |
| Free cells | Enabled by default, one center cell at one-based `(ceil(side / 2), ceil(side / 2))`; checkbox/toggle plus coordinate-labelled selection grid. Disabled sends an empty position set; enabled requires at least one unique in-bounds cell. |
| Player capacity | Integer 2–20, default 20. Explain that two currently connected eligible players are required to Start, independently of occupied seats. |
| Spectator capacity | Integer 0–50, default 50; zero visibly means spectator entry disabled. |
| Winning pattern | Read-only **Single Line** explanation and sample row/column/diagonal highlighting; no unapproved selectable patterns. |

**Actions/validation:** Before persistence, **Create game** explicitly claims the global slot and creates New, with the authenticated creator as designated host. Afterward use **Save changes**, primary **Open lobby**, secondary **Transfer host** where eligible, and a separated **Cancel game** action with confirmation. Open lobby first ensures configuration is durably saved, then makes it fixed and publishes the code; show the consequence before confirmation. No board values are generated for preview or assigned until Start.

Show pool-size versus non-free-cell validation, invalid integer/coordinate errors and feasibility warnings; the authoritative Start check must cover every retained member, not only connected players. In particular, an all-free layout cannot supply distinct boards for multiple players; never silently weaken uniqueness or silently change selected free cells. When shrinking a board would invalidate selected positions, propose an explicit reset-to-default choice or require correction instead of silently discarding positions.

**Persistence/contract seam:** C6 creation-with-configuration versus default-only creation remains unresolved in the API. If the final contract uses C6 followed by C7, make partial success visible: the New game exists and occupies the slot even if saving the user's configuration fails. Resume/retry against that same game; do not claim combined atomicity or create another game. Draft state is not an extra server lifecycle state.

**States:** Invalid form, unsaved changes, creating/saving/publishing, revision conflict, another game claimed the slot, unexpected lifecycle transition, idle expiry and unknown command outcome. On successful publication navigate to the host/admin lobby and remove edit controls. There is no return from Awaiting Players to editable New. Passive viewing/reconnect does not renew the idle deadline; admin actions do not renew another designated host's deadline.

<a id="view-06"></a>
#### VIEW-06 — Host/admin Awaiting Players lobby

**Purpose/access:** Invite and inspect attendance before Start. Sources: HLD-038–040/043/048/059–062/068/074; DO-039–044/053–055. Contracts: C5, C9/C10, D2/D3, E6/E10, G1/G3.

**Components:** Large published code with Copy, fixed `RulesSummary`, player occupied/max and connected-eligible counts shown separately, spectator occupied/max count including retained grace seats, account-only roster with aliases and present/disconnected indicators, and permitted host assignment/status. No player boards yet; show a “Boards are assigned when the game starts” explanation, not placeholder random cards.

**Operator actions:** Primary **Start game** enabled only while synchronized and current prerequisites appear satisfied; **Cancel game** with deletion/no-History confirmation; **Transfer host** with eligible target selection and confirmation; **Back to Games**. State the unmet Start reason, including fewer than two connected eligible players, validation/feasibility failure or pending authority. Backend rechecks all conditions and the global reservation at commit.

**Transfer panel:** Show only eligible enabled Verified Host targets, selected target identity and “takes effect immediately; no acceptance step”. Use the expected assignment revision and no same-target transfer. Target discovery for an ordinary host needs a dedicated safe projection in the API design; do not reuse admin Users or offer admin roles as targets. After commit remove the outgoing ordinary host's mutations; admin override remains separate.

**States:** Empty roster, last-slot joins, stale presence, Start/transfer/cancel races and failed generation. A failed Start leaves Awaiting Players without partial boards. A committed Start replaces the lobby with the authorized Play view; cancellation shows the transient notice. Non-designated hosts see the same authorized read-only information but no mutation controls, accompanied by an explicit read-only banner. No kick-player or manual presence-edit feature is added.

<a id="view-08"></a>
#### VIEW-08 — Join / choose how to participate

**Purpose/access:** Public code lookup plus explicit admission; no attendee account required. Sources: HLD-017/018/049–058; DO-045/047/052–055. Contracts: C4, D1/D3, D5/D6, D13.

**Components:** Code and permitted lifecycle/entry summary, role cards **Play** and **Spectate**, player-only **Alias** input and optional private **What do you like most about Brews?** recovery-answer field, primary role-specific Join button, **Recover existing player** and **Use another code** links. Expose only fields in the limited entry projection, not the account GameSummary, roster, other aliases or boards.

**Rules/help:** Alias is nonempty, maximum 20 ASCII characters after trimming outer ordinary spaces; printable letters/digits/punctuation and internal spaces are permitted, with exact case-sensitive uniqueness. Explain that the current alias must be remembered. The optional answer is private authentication material, not survey feedback; blank means no recovery enrollment. State that code + exact current alias + enrolled answer is the only lost-player-session recovery, and that no host can look it up or override it.

**Behavior/states:** Awaiting Players offers both roles within capacity. In Progress offers new spectator entry and separate existing-player recovery, never a new-player Join or role promotion. Terminal/unissued/unavailable codes offer no join. Full/disabled roles show a reason; rejection never silently admits the visitor under another role. Alias conflicts are corrected within the form, not checked through the backend-only D4 helper as a new public endpoint. On Start racing a player join, show updated role choices. On committed admission, consume one seat and navigate directly to the authorized lobby/play snapshot; a valid existing session restores instead of adding another membership. Failed requests must not display a claimed alias/seat as accepted.

<a id="view-09"></a>
#### VIEW-09 — Player lobby

**Purpose/access:** Already-admitted player waiting for Start. Sources: HLD-034/041/043/045/052–055/066–067; DO-046–052/055. Contracts: D1, D7/D8/D10/D14, C5, G1/G3.

**Components:** Own alias, game code/status, rules/objective and permitted occupancy summary; prominent **Waiting for the host to start** panel; **Rename alias**, **Recovery settings**, **Switch to spectator**, and **Leave game**. No board, other private aliases/roster or host controls are required for this projection.

**Behavior:** Rename preserves stable membership/seat/answer and session deadline; announce the new current alias needed for recovery. Proposed confirmation before player-to-spectator switch explains loss of player seat, alias and recovery answer. Proposed pre-start Leave confirmation explains the same removal and that rejoining is a fresh capacity/alias decision. Failed switch/rename/Leave preserves current displayed committed state. Role switch goes straight to the admitted spectator view with no second Join.

**States:** Rename conflict, target capacity full, session replaced/expired, or Start racing an edit. On accepted Start, fetch the already-assigned own board and transition to Play; never generate it client-side. On pre-start cancellation, show a transient cancellation notice; no board/History retrieval follows. A transient disconnect retains membership but does not count as a connected player for Start.

<a id="view-07"></a>
#### VIEW-07 — Play: host/admin console

**Purpose/access:** Operate or inspect an In Progress game. Sources: BR-002–010/018/026–027, HLD-023/024/026/042/060/062/074; DO-061–068. Contracts: C5/C10, E1/E2/E4–E9/E11, G1/G3/G4.

**Layout/components**
- Top: game code/state, designation/view-mode banner and connection status.
- Main panel: large latest committed call, Single Line objective, ordered call history and remaining-value/exhaustion status. Before any call show **No values called yet**, never a fabricated initial value.
- Operator control area: prominent **Draw random value**; separate manual-value text field and **Call value** button. Validate against the actual remaining string pool; no arbitrary numeric coercion or silent repair of invalid/duplicate values.
- Private inspection panel: player roster/board selector, read-only board cells and completed lines, plus a **Qualifying players** list. Show all eligible retained players, including disconnected or intentionally departed ones; lack of presence is not disqualification.
- Lifecycle action area: **Submit winner**, **Transfer host**, **End without winner**, and **Back to Games**, visually separated from draw controls.

**Winner/end interaction:** Selecting a qualifier highlights its board and current alias/stable identity; proposed final confirmation names the selected winner and explains that submission immediately ends the game as Resolved. It is the operator's choice after the real-life Bingo exchange, not automatic ranking or a digital player claim. Backend revalidates qualification at commit. **End without winner** requires confirmation and produces Cancelled with started-game History. Neither terminal action needs a second close command. Dismissing confirmation changes nothing.

**Controls/states:** Random/manual actions are unavailable while unsynchronized, pending an unresolved call, exhausted or terminal. Exhaustion does not auto-end the game: authorized winner/no-winner actions remain. Show committed call and matches together; never draw or advance speech from a speculative response. A lost acknowledgement triggers original-command resolution. Non-designated ordinary hosts can inspect boards/qualifiers read-only but have no call, transfer, award or end controls. Admin action on another host's game does not transfer assignment. Terminal commit switches to Result; remove all live mutations.

<a id="view-10"></a>
#### VIEW-10 — Play: player board

**Purpose/access:** The caller's assigned board only. Sources: HLD-024/026/034/042/062, DO-047/052/055/059–068. Contracts: D1/D11/D14, E1/E3/E5, G1/G3.

**Components:** Own alias and game identity, large read-only `BoardGrid`, latest/ordered calls, Single Line objective, own qualification status, connection banner, **Recovery settings** and **Leave game**. FREE cells are visibly valueless/pre-satisfied; matching and completed lines come from the backend. Do not include mark/unmark, board regeneration, new-player Join, post-start alias/role editing, or an in-app **Bingo** claim button.

**Qualification:** Show **Your board qualifies — say Bingo and your alias to the host** when the backend flags it, including before the first call if free cells form a line. Qualification does not mean the player has won; only a committed operator award changes the result.

**Leave/recovery states:** Proposed Leave confirmation explains that the seat, same board and winner eligibility remain until game end; it does not free a slot for a replacement. Return requires a still-valid session or the approved answer proof, never alias alone. Reconnection displays the same assigned board and missed committed matches. A replaced session shows a safe “This session was replaced” state when the authorized contract supports it; no recovery answer or other board is exposed. Expiry clears private content and offers eligible recovery, not indefinite stale access. End transitions to the bounded own-result view.

<a id="view-11"></a>
#### VIEW-11 — Player recovery and private answer settings

**Recovery mode (`/recover`):** Game-code field, exact current-alias field, private answer input, **Recover access**, **Back to Home/Join**, and a concise no-alternative-recovery explanation. Prefill a nonsecret code only; do not enumerate remembered aliases or expose whether another player enrolled an answer. Contracts: D13 with C4/D3 as needed; DO-045/049–052/080/085–087.

Send proof to the backend without treating client comparison as authority. Approved answer comparison is versioned Unicode NFC → trim outer Unicode whitespace → case-fold, preserving internal whitespace/punctuation; this does not change case-sensitive alias rules. Invalid proof uses safe generic feedback, and throttling follows server-provided retry availability rather than a client promise. Clear proof from form state after the attempt flow. A successful recovery replaces prior player sessions/sockets and opens the existing lobby or same assigned board; it consumes no new seat. A lost issuance response requires fresh proof rather than replaying a cookie. Closed games, forgotten aliases and missing/forgotten answers have no fallback, host override or terminal recovery.

**Settings mode (nested panel):** Available only with valid own player authority in Awaiting Players/In Progress. Show only **Recovery enabled/not enabled**, an empty new-answer field, **Set/Replace answer**, and **Remove recovery answer** with a proposed warning that future lost-session recovery will be unavailable. Never reveal, prefill or echo the saved answer/verifier. Contract: D14. Accepted maintenance does not rotate/extend the current session. Closing settings returns to the same lobby/Play without joining again; role switch/removal/terminal transition deletes the recovery record under the approved policy.

<a id="view-12"></a>
#### VIEW-12 — Spectator and venue display

**Purpose/access:** Read-only audience information, distinct from the operator console. Sources: BR-014/018–020, HLD-016/018/023/044/045/054/069; DO-047/053–055/073/095. Contracts: D1/D6/D9/D12, C5/E1, G1/G3.

**Lobby components:** Code/state, permitted rules/capacity summary, **Waiting for the game to start**, **Switch to player** while Awaiting Players and target eligibility allows, and **Leave**. Switching opens alias/optional-answer fields and commits a new player membership only after validation; failure preserves the spectator seat.

**Live/venue components:** Large current value, visible recent calls, complete ordered-call view, Single Line objective, status/connection banner, **Display mode** and **Leave**. Display mode proposes a simplified high-contrast layout and user-initiated fullscreen where supported, with an obvious exit-fullscreen control. It is a presentation toggle on an authorized audience view, not a capacity bypass, separate public data feed or operator-page screen share. An attendee-facing device must not render hidden account/private-board controls behind a CSS toggle.

**Boundaries/states:** No player boards, qualifiers, Users, winner-selection or host mutation controls. New spectators can enter during In Progress within capacity; they cannot become players then. Explicit Leave releases the seat immediately. Accidental disconnect preserves the seat only within the server's five-minute grace and original session lifetime; a later return requires fresh admission. At terminal completion render only an actually delivered winner/no-winner result; after server spectator cleanup, refresh/reconnect cannot fetch it again.

**Speech proposal (LLD-014):** Include an **Announce new calls on this device** toggle, initially off, enabled by an explicit user gesture; recommend one venue device to avoid duplicate audible announcements. Speak only newly committed live call sequences while enabled, never resync backlog, rejected calls or repeated frames. Pause on stale/terminal/unauthorized state and provide a visible unsupported/blocked/muted status without blocking gameplay. Voice/language/rate choices, replay controls and cross-device ownership remain review items, not new authoritative game state.

<a id="view-13"></a>
#### VIEW-13 — Results and cancellation notice

**Purpose/access:** Communicate the terminal outcome without reopening the game. Sources: HLD-027/036/037/069–072, DO-056/058/069–075. Contracts: F1/F2 and the last authorized WSS result.

**Components:** **Resolved — winner: [approved alias projection]** or **Cancelled — no winner**, ended time when available, authorized final calls/board panel, and prominent **Exit**. Show a pre-start cancellation explanation separately: no boards or History were created and the game is unavailable for re-entry. Never require all participants to Exit to finalize or release the game.

| Viewer | Final content and Exit behavior |
| --- | --- |
| Existing eligible player | Own final board, permitted calls/result, no other private boards. Access lasts only until Exit or original session expiry. Exit removes all terminal grants for that player; no new recovery or post-Exit return. |
| Existing eligible host/admin account viewer | Authorized final content under its pre-existing grant, capped by session/History expiry. **Exit to Games** deletes grants for that account/game across sessions but does not log the account out or remove separate History rights. A later account login cannot recreate this terminal grant. |
| Connected spectator that received the result | Audience result already in memory; **Exit to Home** clears it locally. No server grant is retained; refresh/reconnect is not a result-recovery route. |
| Pre-start cancellation recipient | Transient cancellation notice and return navigation; cleanup does not wait for receipt/Exit and there is no History link. |
| New, expired or already-exited viewer | Safe unavailable/access-ended page; no final-game admission. An independently authenticated host/admin may open unexpired History instead. |

No Replay game, restart, further call, second winner, edit outcome, or export button. A host/admin **View History** action uses the separate History route, with app-controlled departure from the result following Exit semantics. If terminal Exit completion is unknown, keep it marked pending and reconcile; do not claim server access was revoked solely because the component disappeared. Backend expiry remains effective even if cleanup is delayed.

<a id="view-14"></a>
#### VIEW-14 — Historical games and History detail

**Purpose/access:** Cross-host read-only inspection for normal host/admin accounts, not former participant entitlement. Sources: BR-021, HLD-029/032/072, DO-069–075. Contracts: F3/F4.

**List:** Lives in the Games dashboard, satisfying the requested historical-games display. Show code, terminal outcome, ended time, winner only for Resolved, expiry and **View history**. Do not mix pre-start cancellations or expired entries into the list. Suggested outcome filtering/newest-ended-first sorting and cursor paging need final API agreement; no fabricated total or extra datastore is implied.

**Detail components:** Read-only outcome/winner header, original game code, permitted host identity and started/ended/expiry times; complete final ordered calls; participating-player aliases and a selectable/expandable final-board inspector. Preserve API ordering (calls by sequence, players by stable ID, cells row-major). Show final cells/matches, not intermediate board playback or unsupported live-presence data. Include **Back to Games**, no gameplay controls.

**Privacy/expiry:** No recovery material, session metadata, spectators, account credentials or export/download action. Do not reconstruct removed editable configuration or resurrect live memberships from a snapshot. Display the server's fixed UTC three-calendar-month expiry with a timezone-labelled human date; the client must not recompute a rolling retention period. At expiry/unavailability replace cached protected content with an unavailable message and remove stale list entries; viewing, revisiting or another game never extends the deadline. Loading failure is not proof a history record does not exist, and History permission never grants terminal-game re-entry.

<a id="view-15"></a>
#### VIEW-15 — Users, provisioning and account detail

**Purpose/access:** Admin-only account management; hosts, players, spectators and unauthenticated callers must fail both route and API guards. Sources: HLD-074–078, DO-020–022/028/029/036/088–090/105–107. Contracts: B1–B9. No developer console, audit browser or infrastructure credentials in the app.

**Users list components:** **Users** heading, **Create account**, optional role/lifecycle filters, refresh/load feedback, and a responsive table/card list. Propose columns **Username**, **Role (Host/Admin)**, **Lifecycle (PendingEnrollment/Verified/ResetRequired)**, **Access (Enabled/Disabled)**, **Created**, and **Manage**. Enabled/Disabled is derived from `disabled_at`, separate from lifecycle. Account detail can expose its safe ID and authorized lifecycle timestamps. Do not display epochs, hashes, salts, tokens, previous links or anonymous game memberships. Render permitted username control characters safely/visibly without changing the exact underlying comparison value.

**Create account:** Username field with the same trim/case/ASCII validation as Login; explicit **Host / Admin** role selection at creation only, with additional privilege explanation for Admin; **Create account** and Cancel. Dispatch the corresponding B3/B4 operation rather than an unrestricted role-edit endpoint. Do not request an initial password or invite an anonymous user to bootstrap an admin.

**Account detail/action panel:** Refresh the target and server-derived action eligibility before confirmation/commit. Select action labels from lifecycle/access, not a single ambiguous “status”.

| Action | Availability, confirmation and visible effect |
| --- | --- |
| Reissue enrollment link | Enabled PendingEnrollment only; explain that predecessor links/restricted sessions become invalid. Existing verified accounts use reset instead. |
| Issue / reissue reset link | Enabled enrolled or ResetRequired target according to B6; explain immediate old-password/session invalidation and that expiry of the link does not restore the old password. No admin-supplied replacement password. |
| Disable account | Not self; reject while target hosts a nonterminal game or if it would leave no enabled Verified admin. Confirmation identifies target and loss of access. Do not auto-cancel/transfer the target's game. |
| Enable account | Disabled target only, another eligible admin or separate CLI path; preserve lifecycle and revoked credentials. Explain whether password setup/reset is still needed; no self-enable or role change. |
| Delete account | Not self, no hosted nonterminal game, not the last enabled Verified admin. Irreversible target-specific confirmation; unrelated/unexpired History is retained. |

Role is immutable after creation; no Promote/Demote, edit-username, bulk removal or audit-export feature is proposed. When a guard blocks management, show its safe authorized reason and an appropriate existing-game/admin-setup next step, not a client override. Creation/management has pending/rejected/unknown-result states; disabling access is not proof every socket has acknowledged close. Show pending close work separately only when the contract supplies it.

**Private link handoff:** After committed creation/issuance show purpose, target, fixed expiry, selectable newly issued URL and a user-initiated Copy button in a dedicated dismissible panel. Warn the admin to deliver it privately. Do not embed a sample secret in docs, turn the URL into account metadata, persist it in browser storage, auto-open it as the issuer, or put it in a generic success toast. Clearing the panel/navigation clears the in-memory secret. Same-command retries cannot recover it; offer explicit successor reissue after a lost/dismissed URL and explain predecessor invalidation. Self-target reset handoff, where issuer authentication is invalidated, requires final safe response/redaction UX review rather than an invented permission exception.

**List states:** Loading, empty, filters-with-no-results, failed page, forbidden/expired session, target deleted concurrently and action blocked by a newer lifecycle/game assignment. Role/status filters and pagination are proposed API seams; do not silently omit unloaded accounts or claim a complete count. No username-search endpoint is introduced by this layout.

<a id="view-16"></a>
#### VIEW-16 — Shared connection, permission and failure states

**Sources:** HLD-020–022, DO-034/036/052/055–058/067/068/079/080/091–095. Contracts: A2, D1, G1/G3/G4 and each owning operation. These are UI states, not extra persisted game lifecycle variants.

| UI state | What to show and allow |
| --- | --- |
| Loading / Connecting | Label the requested screen, show a progress placeholder without fabricated counts or an assigned board, and keep game mutations disabled. |
| Synchronizing | Existing connection is not enough; wait for the authorized full snapshot and revision alignment before enabling controls. |
| Live | Show committed state and currently permitted actions; keep periodic/foreground freshness checks separate from heartbeat liveness. |
| Reconnecting / stale | Visibly stale last-authorized view only while access remains eligible; no mutations/offline queue. Retry with approved jittered backoff capped at 30 seconds. Do not replay old speech or generate a replacement card. |
| Command outcome unknown | Preserve nonsecret command context and check the original result; show why another draw/create/award cannot yet be issued safely. Credentials follow their separate fresh-auth/reissue rules. |
| Coordination pending | Explain only the supplied pending work; no false “all sockets closed”, “deleted”, or “new game ready” claim. |
| Expired / revoked / replaced | Stop protected updates and clear private content. Offer Login for accounts or eligible nonterminal recovery for players; spectators need eligible restoration or fresh admission. Do not extend credentials. |
| Forbidden | Render a safe access-denied page with Home/authorized navigation; hide Users/private boards and never treat an empty hidden control as backend enforcement. |
| Unavailable / ended / History expired | Explain that the requested access is unavailable without leaking protected data. Offer Home or permitted Games/History, never auto-join a newly reused code. |
| Throttled / service failure | Safe inline retry guidance; respect server timing, do not spin on failed auth or infer account existence. A temporary network failure does not mean an empty roster/list. |

A second socket for one participant session may supersede the first; present a safe reconnect/replaced-session state, not two counted seats. Account logins may coexist, but terminal account Exit spans that account/game's grants. On visibility resume, restore authorization and snapshot/revision state before controls; when a gap or bounded-delivery failure occurs, replace from an authorized snapshot rather than merging speculative local gameplay.

### 4.4 UI state ownership and API handoff

LLD-027 selects Dioxus Router for these screens and Axum for their backend HTTP requests; [Section 3.5](#routing-design) defines the separation. The proposed component names above are responsibilities, not Rust files or selected signatures. Keep three separate state classes: **ephemeral UI** (open panel, unsaved input, board selection), **validated session/capabilities** (never bearer contents), and **server snapshots plus command results/revisions**. Only the last two govern protected rendering; none becomes a local authoritative copy. Account and participant contexts need separate permission checks even when using the same game route.

| Experience group | Existing proposed operations to use | Remaining contract work, not a new endpoint here |
| --- | --- | --- |
| Home / Join / Recovery | A2, C4, D1/D3, D5/D6/D13 | Limited entry projection, session-context resolution, safe errors and admission/restore response shapes. D4 remains internal. |
| Login / Enrollment / Reset | A1–A7 | Safe auth/link errors, exact cookie/redirect behavior and original-deadline handling; passwords/confirmation UI do not change request fields. |
| Games / Setup / Lobby | C1/C2/C5–C10, D2/D3, E6/E10 | Creation/defaults/configuration save seam, capability/display-name projections, safe eligible-transfer-target discovery for ordinary hosts, revision/confirmation transport. |
| Account / player / audience Play | C5/C10, E1–E5/E7–E9/E11, D11/D12/D14 | Role-specific snapshots, safe public call fields and command outcomes; no direct storage-record serialization. |
| Player lobby / role switch / settings | D1/D7–D10/D14 | Own membership fields, accepted-switch routing and conditional errors without a second admission. |
| Result / History | F1–F4 | Exact Exit pending/retry result, permitted final-view fields, History list columns/sort/paging. No result grant from History. |
| Users | B1–B9 | Safe columns, capability/guard reasons, role/lifecycle filters/paging and secure new-link handoff; no broad Users exposure for host transfer. |
| Synchronization | G1/G3/G4 with approved DO view revisions | Snapshot/update envelope, close/error mapping, pending-command resolution, hibernation and framework wiring. System coordination helpers remain backend-only. |

For each implementation slice, extend the existing per-operation contract in `api-design.md` rather than silently adding a UI-specific endpoint or copying storage structs. Frontend routes above do not finalize the API catalog. Literal copy, styling tokens, icons and breakpoint values can be refined without changing authority, persistence, lifecycle, retention or input policies.

### 4.5 UI acceptance checklist — specification, not executed tests

| ID | Scenario and required result |
| --- | --- |
| UI-AC-01 | Directly open `/` on desktop/mobile: branded Home has labelled game-code input, Join/Enter submission and host/admin login; no public Games/Users/History data leak. |
| UI-AC-02 | Validate code trimming/case/length/internal spaces, case-sensitive usernames and aliases, and exact password preservation. Invalid submissions do not create a game, seat or session. |
| UI-AC-03 | Test anonymous, restricted, host, admin, player and spectator route/direct-API denial; switching a client route/tab/role must not grant Users, other boards or game mutation access. |
| UI-AC-04 | Login/setup/reset reach the correct next screen: setup retains original expiry, reset requires login; used/expired links and lost secret responses do not replay credentials. |
| UI-AC-05 | Games shows at most one nonterminal card, blocks competing creation, and still displays unexpired History; incomplete coordination is not shown as a free slot. |
| UI-AC-06 | Create/Save/Open lobby validate defaults/bounds/free coordinates, preserve partial creation truth and stop editing after publication. Leaving the screen does not silently cancel the saved game. |
| UI-AC-07 | Lobby distinguishes occupied and connected counts; disconnected retained players get boards but do not satisfy the connected minimum. Start failure leaves no partial boards; no board exists before committed Start. |
| UI-AC-08 | Join/rename/switch/Leave races with Start/capacity preserve correct membership on rejection, use exact current aliases and never auto-switch roles or admit twice. |
| UI-AC-09 | Operator Play supports random/manual calls, complete ordered history, exhaustion, qualified-winner selection and confirmed no-winner end. Duplicate clicks/lost ACKs cannot produce extra calls or outcomes. |
| UI-AC-10 | Other hosts inspect read-only; admins act without silently becoming designated host; transfer removes outgoing ordinary-host mutations at commit and uses no admin-only Users access for target lookup. |
| UI-AC-11 | Player Play has only own board/qualification, no marks/claim button; spectators/venue display never receive private boards or qualifiers. FREE/initial qualification and departed-player eligibility remain correct. |
| UI-AC-12 | Recovery/settings never reveal saved proof, permit alias lookup or bypass terminal rules. Recovered play uses the same board, and replaced sockets do not create extra seats. |
| UI-AC-13 | Disconnect/gap/hibernation/replacement/expiry transitions visibly gate actions; authorized snapshots restore state without offline mutations, renewed TTLs or replayed audio. |
| UI-AC-14 | Results honor each role's grant and Exit scope; spectator refresh cannot recover final data; pre-start cancellation has no History; browser Back does not bypass completed Exit. |
| UI-AC-15 | History remains read-only across a new game and disappears at original expiry; no export, live replay, extra private fields or inherited authority from a reused code. |
| UI-AC-16 | Users is admin-only; creation roles are explicit/immutable, self/last-admin/hosted-game removal guards hold, enable preserves lifecycle, and links are one-time private handoffs with honest pending-close feedback. |
| UI-AC-17 | Keyboard, focus, labels, contrast, non-color statuses, large-board mobile inspection and venue-distance readability are exercised. Optional speech handles unsupported/blocked devices without blocking gameplay or repeating resync backlog. |

### 4.6 Remaining UI review and implementation gates

- Review proposed screen grouping/routes, action copy, visual tokens and responsive behavior (LLD-001/026); LLD-028 selects Dioxus-integrated Tailwind. The above fills the design inventory, not Dioxus modules or a prototype.
- Finalize Users filters/paging/action-capability projections and private-link handoff edge cases (LLD-002), without changing DO-105–107 protections or adding role editing.
- Finalize the API seams in Section 4.4, including ordinary-host transfer-target discovery; a UI dependency is not approval of an unreviewed public account-list API.
- Settle optional speech ownership/controls, browser/assistive-technology matrix and measurable venue/accessibility targets (LLD-014). The proposed local opt-in behavior is not yet a separately approved audio policy.
- Resolve browser input/transport of all permitted ASCII controls and still-unselected password/answer size caps without silent narrowing, truncation or new normalization.
- Confirm frontend handling of navigation/unload, account/participant coexistence, unknown command outcomes and terminal Exit races against the final contracts. Do not add implicit game cancellation or broaden session grants.
- Implement and execute UI-AC-01–UI-AC-17 only after separate implementation authorization. All 107 approved DO decisions remain unchanged by these UI proposals.

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

The message inventory and transport/synchronization worksheet moved to [api-design.md Section 7](api-design.md#wss-design). DO-067/068 approve projection/revisions, atomic attachment+snapshot ordering and gap resync. DO-091–095 approve attachment schema/cap, authoritative hibernation reconstruction, socket fencing, earliest-deadline alarm, frame/queue limits and close/resync on delivery pressure/failure. Worker SDK/close handlers and executed transport tests remain unverified.

Persisted view/session/attachment proposals now live in [Durable Object design Section 6](durable-object-design.md#schema-proposal). This reference preserves existing section numbering without duplicating the WSS specification.

## 8. Detailed workflows and domain algorithms

For each row, fill in sequence, participants, records, preconditions, commit points, failure recovery and tests. Do not introduce extra user-visible lifecycle states or treat Object-to-Object work as one SQL transaction.

| Workflow / algorithm | Constraint to preserve | Detailed design |
| --- | --- | --- |
| Enrollment / reset / reissue | Restricted authority, one-day single-use links, fixed session expiry and predecessor invalidation. DO-029 requires enabled-state/purpose eligibility, one epoch bump per accepted link issuance, predecessor-link/session revocation, secret-free 30-day same-command receipts, and explicit successor reissue rather than bearer-secret replay after a lost response. DO-032 approves atomic enrollment completion and restricted-to-normal rotation without extending the original session deadline. DO-033 approves password login after reset, with no automatic normal-session issuance. DO-034 requires authoritative request-time account/session checks and asynchronous cleanup of ineligible session rows; DO-035/036 approve session-bound subscription registration, durable retryable close work and per-frame checks; DO-037 approves expiry sweeps and metadata deletion after close/absence acknowledgement. Retain link metadata through latest expiry/consumption/revocation + 30 days. | Physical receipt schema, SQL/indexes, session/outbox storage, cleanup schedule, sweep cadence and remaining transaction details stay TBD. |
| Game create / publication | Claim one global slot at New creation and retain it through Awaiting Players/In Progress; fixed config/code, crash-safe Directory/Game agreement. | TBD |
| Admission / Leave / rename / switch | Capacity and alias ownership remain consistent; explicit Leave differs from loss; failed changes preserve prior membership. | TBD |
| Start and board generation | Connected minimum is separate from retained start roster. DO-044 validates the config and uses capped `P(N,k)` feasibility against all retained memberships before acceptance. DO-062 approves stable-player-order CSPRNG partial Fisher–Yates generation, exact duplicate checks, the 1,024-candidate whole-Start cap and fail-closed behavior; command retries must not reassign committed boards. | DO-044/062 approved; target RNG integration and start transaction details TBD. |
| Free-cell representation / evaluation | Valueless pre-satisfied cells and specified default coordinate; qualification may exist immediately, never automatic award. | TBD |
| Pattern-specific Rust traits | Separate trait per distinct pattern; first-release Single Line only. DO-063 approves the Single Line trait signature, shape validation, deterministic completed-line ordering and qualification semantics. | DO-063 approved; implementation/integration tests remain pending. |
| Random/manual call → match → qualify | Valid undrawn value; DO-065 starts call sequences at 1 and associates accepted calls with actor-scoped receipts; sequence is consumed only on acceptance. Persist all affected boards/qualification, revision and command outcome before push. | DO-066 approves one atomic logical transaction for the call, affected board/qualification projections, revisions and accepted-call receipt, with post-commit push; physical SQL/query details and general receipt schema/retention remain TBD. |
| Award / cancellation / slot release | Exactly one immutable terminal result; no viewer-dependent release or stale release of another game. | TBD |
| Host transfer / admin action | HLD-060 authorizes confirmed immediate nonterminal transfer without changing account role; DO-039 requires current actor/target checks, expected-assignment-revision compare-and-set, once-only checked revision increments and actor-scoped command receipts, with target-removal ordering. DO-040/041 approve designated-host-only timer renewal and expiry/activity/start ordering. Separate AdminAuditRecord/retention policy remains DO-088–090. | Physical cross-Object coordination/recovery, confirmation binding, alarm delivery and remaining API details stay TBD. |
| Player recovery / answer maintenance | Same stable member/board, no new slot or terminal recovery, current-alias lookup and revoked old sessions. | TBD |
| Refresh / reconnect / interrupted command | Snapshot-first convergence; no offline queue, replacement boards, duplicate random draws or heartbeat-only freshness. DO-068 approves atomic attachment/snapshot revision cut, ordering, stale/duplicate suppression and full authorized resync. DO-091–095 approve compact attachments, durable authority reconstruction, socket fencing, earliest-deadline alarm and bounded frame/queue close-resync policy. | Worker SDK/WSS delivery, physical outbox integration and tests remain unverified. |
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
- **TBD —** maximum password length and request-size limits; “at least 10” selects a minimum, not an exact length or a chosen upper bound. Section 4 proposes a client-side password confirmation field for setup/reset; it does not add a credential field to the API. Final UI approval and exact API validation/error representations remain **TBD** in [API design](api-design.md#shared-contracts).

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
| Views and permissions | Home entry, host/admin controls, Users frontend/backend denial, private-board projection, missing/expired sessions and restricted enrollment. | UI-AC-01–UI-AC-17 in Section 4.5 specify screen/role/lifecycle scenarios; no UI implementation or application tests have been run. |
| Worker routing and Object isolation | Dioxus/SPA paths are separate from Axum API dispatch and native DO upgrade/handler paths; unknown codes do not create games, and bindings/IDs never replace authorization. | ROUTE-AC-01–ROUTE-AC-08 in Section 3.5 specify future compatibility, fallback, auth/cookie, upgrade and retry checks; none have been executed. |
| Account lifecycle | Concurrent redemption, reissue/reset revocation, first-admin creation, non-owner overrides, removal blocked by hosted nonterminal game. | TBD |
| Password policy | Reject fewer than 10 characters and non-ASCII input; accept policy-valid ASCII passwords without character-class quotas; preserve whitespace/case and decoded input through setup/reset/login. | **TBD** — UI/transport edge cases and tests; hashing scenarios are listed below. |
| Password hashing | Correct/wrong password verification; independent salts for the same password; PHC round-trip and embedded-parameter use; malformed/unsupported/out-of-bounds verifier rejection; RNG failure; no secrets in projections/logs; setup/reset/login and proposed rehash races; WASM resource/concurrency measurements. | **TBD** — tests/benchmarks not implemented or executed; account and recovery-answer production costs/caps remain untuned pending target measurements. Algorithm/format decisions are partially approved in DO-023/024 and DO-051. |
| Lifecycle/concurrency | Competing create/start/call/winner/cancel/transfer/admission; separate global slots and immutable terminals. | TBD |
| Participant identity | Alias case/space/rename/reuse, role switch, fresh rejoin versus recovery, answer deletion and no inherited credentials. | TBD |
| Durable recovery | Restart/hibernation, commit with lost ACK/broadcast, snapshot race, revision gap, stale socket and original-command retry. | DO-091–095 approve logical fail-closed attachment reconstruction, socket fencing, durable deadline alarm and close/resync bounds; DO-076–084 approve receipts/outbox phases/retries. Release-gate tests approved by DO-104 but not run. |
| Expiry/retention | One-day credentials, five-minute spectator grace, 24-hour host-idle cancellation, and DO-071 UTC three-calendar-month History deadline with month-end clamp/end-exclusive denial; no resurrection/export. | DO-071 policy approved; schedulers, purge and restore integration remain TBD. |
| Terminal access | Player final-view window, spectator local-only result, pre-start deletion, independent Exit and post-exit denial. | TBD |
| Browser/accessibility | Desktop/mobile matrix, venue readability, keyboard/screen readers and optional spoken-announcement behavior. | Section 4.2 proposes accessibility/layout targets; VIEW-12 proposes local speech controls; UI-AC-17 covers future validation. Device matrix, audio policy review and results remain pending. |
| Capacity/resilience | DO-103 requires max legal game plus owner peak-event/History forecast measurement for each Object’s storage, row reads/writes, requests, duration/CPU, alarms and WSS fan-out; reverify quotas and require ≤50% of applicable quota, else stop for architecture/cost review. | Gate approved; forecast and measurement results remain TBD. |

Latency, concurrency/load targets, measurable accessibility criteria and failure/cleanup budgets: **TBD**, not inferred from provider documentation.

## 11. Cloudflare deployment and operations worksheet

| Item | Detailed specification |
| --- | --- |
| Rust/Dioxus/Workers SDK/toolchain compatible pinned versions | DO-102 approves pinning stable Rust/Cargo.lock and a compatible Worker SDK/build pair after compatibility validation. LLD-028 requires a compatible pinned Dioxus CLI/Tailwind build path, generated CSS and shared-RSX source coverage. Exact pins and target/build validation remain TBD. |
| UUID/error libraries | `uuid` with UUID v7 and `thiserror` selected. DO-096 approves canonical lowercase-hyphenated UUID TEXT; DO-102 approves trusted Worker UTC time and platform CSPRNG with fail-closed error/no fallback. Compatible crate pins, target integration and validation remain TBD (Sections 3.3–3.4). |
| Password-hashing library/runtime | Salted Argon2 via RustCrypto `argon2` selected for account passwords; DO-051 separately approves Argon2id v19/PHC for recovery answers. **TBD** — pinned compatible version/features, secure WASM randomness, hashing placement, production costs/caps, CPU/memory/concurrency benchmarks and failure handling. |
| Single API Worker entrypoint/module routing, Object classes, bindings and deployment configuration | LLD-027 selects Axum via the workers-rs HTTP fetch service and native DO socket/alarm handlers; preserve LLD-016 and DO-100 private bindings. Concrete entrypoint build output, SDK/feature pins, middleware and deployment configuration remain unverified. |
| Static asset build, routing, caching and private-route separation | LLD-027 selects Dioxus Router, same-origin Static Assets SPA fallback and Worker-first `/api` plus `/api/*`; reserve API errors/upgrades from HTML fallback. ASSETS is an asset binding, not a state store. Actual build directory, compatibility date and response/cache header wiring remain validation work. |
| Local / test / production environments and isolated resources | TBD |
| SQLite schema/Object migrations and version compatibility | DO-099 approves owner-local versioned forward-only transactional migration with fail-closed startup and compatible-backout/forward-fix policy; DO-075 restore expiry enforcement. Exact DDL/runtime hooks remain TBD. |
| Secrets, developer CLI setup and initial-admin bootstrap procedure | HLD-075/DO-101 approve CLI-only first-admin bootstrap and restricted backend management interface; credentials stay in controlled developer environment. Exact secret names/scopes/operations remain TBD. |
| Durable deadlines/alarms or equivalent expiry/cleanup scheduling | TBD |
| Logs, metrics, tracing, redaction and admin-action auditing | DO-088/089 approve owner-local audit records and authenticated privileged success/reject/failure capture with CLI path attribution; DO-090 approves 90-day retention and privileged CLI-only read. Secrets remain redacted; exact instrumentation is TBD. |
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
| LLD-001 | Dioxus view grouping, routes, components, state management and accessibility | Concrete UI proposals remain in Section 4 / LLD-026. LLD-027 selects Dioxus Router and LLD-028 selects Tailwind through the Dioxus build/asset workflow. Exact screen paths/layouts, visual tokens, component signatures and executed accessibility/UI checks remain pending. |
| LLD-002 | Users fields/actions/filters and protected account-management UX | VIEW-15 proposes safe columns, creation/detail panels, guarded actions and one-time private-link handoff. Role/access/lifecycle constraints remain approved; filter/paging/capability contracts and handoff edge-case UX still need review. |
| LLD-003 | Cloudflare account/session store placement and ownership | DO-001 approves one AccountsObject per environment; DO-100 approves GAME_DIRECTORY/ACCOUNTS/GAMES private bindings and environment isolation. Ownership is approved; exact deployment config/internal methods remain implementation work. |
| LLD-004 | SQLite tables/types/keys/indexes, record mappings and migrations | DO-096–099 approve owner-local normalized mappings/type/privacy, local FKs/cross-owner validation, unique/check/parameterized-query rules and versioned forward-only migrations. Exact DDL/statements/index query plans and runtime migration hooks remain TBD. |
| LLD-005 | Directory/game coordination and command idempotency/reconciliation | DO-014/015 approve one nullable `game_id` reservation row, claim-first creation, same-ID/same-code GameObject recreation after failure, and compare-by-ID acquire/release without a generation counter in [Durable Object design Section 6.3](durable-object-design.md#directory-records). DO-016 approves idempotent terminal compare-and-clear (matching ID clears; NULL is complete; different ID is stale/no-op). DO-017 approves a presence-only account-assignment gate keyed by account_id; DO-018 approves acquire-before-check, assignment blocking, clear-and-reject for hosted games, and retain-and-retry on interruption. DO-076–084 approve owner/actor-scoped receipts, fingerprints, bounded typed results, retention, secret-safe issuance retry, typed pending payloads, per-operation fences/ACKs and capped retry/reconciliation. Physical transport/recovery remains implementation work. |
| LLD-006 | Worker API methods/routes, request/response/error schemas and compatibility | LLD-027 selects Axum in the single Rust Worker, responsibility-based handler composition and strict API/SPA separation. Existing proposed functions/paths/DTOs remain in [API design Section 6](api-design.md#operation-catalog); final requiredness, errors/statuses, wire schemas, middleware ordering and native upgrade adapters remain TBD. |
| LLD-007 | WSS payloads/revisions, privacy projections, hibernation and backpressure | DO-067/068 approve per-view projection boundaries/revisions, single-cut attachment+snapshot ordering, monotonic updates, gap-triggered full resync, independent authority checks and terminal counter lifetime in [Durable Object design Section 6.5](durable-object-design.md#game-records); persisted session/attachment records remain in [Sections 6.4/6.8](durable-object-design.md#account-records). WSS contracts live in [API design Section 7](api-design.md#wss-design). DO-091–095 approve v1 attachment cap, authoritative hibernation reconstruction, participant replacement fencing, earliest-deadline alarm and frame/queue close-resync limits. Rust SDK delivery integration remains implementation work. |
| LLD-008 | Board generation/feasibility, matching and pattern-specific trait signatures | DO-044/061/062/063 approve feasibility, uniqueness, bounded generation/failure and the Single Line evaluator contract; target RNG/SQL/transaction integration and implementation tests remain TBD |
| LLD-009 | Password policy, token/cookie/verifier implementation and request security | Minimum 10 characters and any combination of ASCII characters confirmed by LLD-023 (Section 9.1); salted Argon2 via RustCrypto `argon2` confirmed by LLD-024 (Section 9.2). DO-019 approves AccountRecord fields/types/nullability and combined PHC storage in `verifier`; DO-022 approves lifecycle/disablement and credential-epoch rules in [Durable Object design Section 6.4](durable-object-design.md#account-records). DO-076–090 approve receipts, secret-safe retries, rate-limit and audit policies; DO-101 approves trusted Worker/Object/CLI boundary. Production Argon2 costs/caps, exact Unicode runtime, cookie/wire encoding, physical SQL and provider integration remain **TBD**. |
| LLD-010 | Answer normalization/protection, abuse controls and recovery transactions | DO-049 approves the logical private `PlayerRecoveryRecord` fields/types/privacy; DO-050 approves version-1 Unicode NFC, outer Unicode whitespace trim and case-folding, preserving internal whitespace/punctuation, with stored-version verification and authenticated replacement on future version changes; DO-051 approves Argon2id v19, fresh independent 16-byte salt, 32-byte output and PHC encoding; 19 MiB/2/1 is only a benchmark starting point and production costs/caps require target measurements. DO-052 approves optimistic verifier revalidation, atomic answer change, session-epoch replacement and socket-fencing policy. DO-085–087 approve rate bucket fields/owners, 5 failures/15m then 15m block, and scoped HMAC/privacy policy. Exact Unicode implementation, key rotation and physical integration remain **TBD**. | DO-048–053 |
| LLD-011 | Presence/Leave/Exit mechanics, timer scheduling and terminal notice/deletion ordering | DO-053 approves spectator record/grace fields; DO-054 approves disconnect/reconnect/expiry ordering and stale-event guards; DO-055 approves participant-session binding/access in [Durable Object design Section 6.5](durable-object-design.md#game-records). DO-056 approves terminal player grant/Exit/replay/deletion; DO-057/058 approve account LiveView/final-view and cross-session Exit policies; DO-059 approves logical board fields/types; DO-060 approves board row mapping/projection checks. DO-073/056/058 policies are approved for terminal notice and final-view cleanup; DO-091–095 approve attachment, reconstruction, fencing, alarms and backpressure policies. Rust SDK/socket-close integration remains **TBD**. |
| LLD-012 | History placement/indexing, calendar-month expiry, cleanup and backup/restore | Final History remains in the original Game Object with a minimal Directory index. DO-069 approves immutable snapshot/winner/player fields and constraints; DO-070 approves parent/call/player/cell row mapping and ordering; DO-071 approves UTC calendar-month expiry/month-end clamping/end-exclusive denial. DO-072 approves atomic materialization/cleanup; DO-073 approves best-effort spectator result delivery followed by server identity/session deletion without ACK wait; DO-074 approves primary scheduled purge/code reuse; DO-075 approves copy/index/log expiry and restore non-resurrection. Provider scheduling, backup/log controls and physical cleanup remain **TBD**. |
| LLD-013 | CLI interfaces, binding/credential scopes, secure link handoff and bootstrap | DO-101 approves the restricted developer CLI invoking authorized Worker management/bootstrap operations with controlled credentials, no direct SQLite and no browser exposure. Exact credential scopes, transport, commands and packaging remain TBD. |
| LLD-014 | Optional speech, browser matrix, quality/load targets and acceptance tests | DO-103 approves max-game plus owner-provided peak forecast capacity validation with 2× quota headroom; DO-104 approves the schema/transaction/race/security/load release-gate plan. VIEW-12 proposes device-local opt-in speech with no resync replay; Section 4.2 proposes responsive/accessibility targets and UI-AC-17 covers future validation. Audio behavior approval, browser/assistive-technology matrix and measurable venue targets remain pending; no measurements/tests have been run. |
| LLD-015 | Build/deployment/observability and quota verification | DO-100–104 approve isolated bindings, pinned-toolchain/clock/RNG policy, forward migrations, 2× quota capacity gate and test/verification release gate. Exact pins, provider runtime integration, forecast and actual results remain unverified. |
| LLD-016 | Initial API Worker organization and Durable Object boundary | One API Worker serves the app. DO-100/101 preserve private GAME_DIRECTORY/ACCOUNTS/GAMES bindings, typed trusted peer operations and restricted CLI. LLD-027 selects Dioxus Router, Axum HTTP dispatch and native DO state/socket handlers. No endpoint-per-Worker split or conventional listening server; exact SDK/transport/module wiring remains unverified. |
| LLD-017 | Proposed operation catalog and auth-scope notation | Captured at user request — [API design Section 6](api-design.md#operation-catalog) retains category headers and per-operation ID/function/access-type heading, Path, Auth scope and description. Scope labels are Admin, Host, Player, Anyone and the user-selected System for internal work. Proposed names/paths remain reviewable; Anyone never bypasses required session/proof/ownership, Host includes admin with applicable game guards, and System is not an account role or public endpoint. Potential Request/Response fields are recorded under LLD-019; final schemas and function signatures remain **TBD**. Object structures are separately proposed under LLD-018, not finalized by this catalog. |
| LLD-018 | Durable Object records and types | The DO-001–107 logical design decisions are approved and logged in [Durable Object design Section 6](durable-object-design.md#schema-proposal), with physical schema and implementation gates kept distinct. All approved logical decisions are recorded; exact DDL/runtime mechanisms and verification remain implementation work. |
| LLD-019 | Proposed per-operation Request and Response inputs/outputs | Captured at user request — every A1–H12 operation in [API design Section 6](api-design.md#operation-catalog) has Request/Response subheadings with potential typed fields, session/cookie effects and failure candidates. Shared safe DTO descriptions do not expose storage records. Internal helpers use internal inputs/results; WSS entries distinguish upgrade/frames/push. Final schemas, requiredness, encodings, status/error contracts, retry/security mechanisms and tests remain **TBD**; no new endpoint or implementation is approved. |
| LLD-020 | Application-generated system ID convention | **Confirmed user direction** — UUID v7 by default for application/developer-controlled identifiers, including the listed `*Id` types; use the Rust `uuid` crate and distinct typed IDs. DO-096 approves canonical lowercase-hyphenated UUID TEXT mapping; DO-102 approves trusted Worker clock/randomness policy. Tokens, short codes and provider-owned IDs remain separate. Compatible target integration and concrete constructors remain unverified. See Section 3.3 and [Durable Object design Section 6.2](durable-object-design.md#shared-types). |
| LLD-021 | Rust error library and component-local ownership | **Confirmed user direction** — use `thiserror` for application-owned typed errors in adjacent component `error.rs` files; auth errors belong in `auth/error.rs`. Concrete variants/conversions/versions/tests **TBD**; transport mappings remain in the API design. See Section 3.4. |
| LLD-022 | API design document extraction | **Confirmed user direction** — `plans/api-design.md` is the sole home for API organization/contracts/inventories/catalog/Request–Response/WSS design. LLD Sections 3.1–3.2 and 6–7 link to it; stable operation IDs/content are preserved. Unresolved contract details remain **TBD**. |
| LLD-023 | Account password length and character policy | **Confirmed user direction** — at least 10 characters, any combination of ASCII characters, no required mix of character classes. Applies to host/admin account password setup/reset and consistent later verification. **Earlier hashing deferral superseded by LLD-024**; maximum length and UI/transport details remain TBD. See Section 9.1 and the API password inputs. |
| LLD-024 | Salted Argon2 account-password hashing | **Confirmed user direction** — use Argon2 with salt and RustCrypto `argon2` for host/admin passwords, superseding LLD-023's hashing deferral. DO-019 approves `verifier` as one complete library-generated PHC string rather than raw salt alone or separate salt/hash fields. **DO-023 approves** Argon2id v19, fresh independent 16-byte salts, 32-byte output and SQLite `TEXT` mapping. **DO-024 approves** explicit work factors, the OWASP benchmark starting point, bounded fail-closed verification and a pre-production runtime-validation gate; production tuning/caps and target measurements remain unverified. Compatible crate pin and upgrade mechanics remain open. |
| LLD-025 | Durable Object design document extraction | **Confirmed user direction** — move the existing schema proposal into `plans/durable-object-design.md`. It is the single home for Object ownership, DATA inventory, schema/consistency/migration worksheets, records/fields/types, invariants and storage TBDs. LLD Section 5 and API storage references link to it. Field names/types and confirmed constraints are preserved; the move does not approve pending proposals or authorize implementation. |
| LLD-026 | Requested UI screen/component designs | **Requested design proposals — not implemented:** Section 4 preserves VIEW-01–VIEW-16 and adds VIEW-17 Home with game-code input, Join and shared host/admin login. Defines Games with History, Create/setup, account/player/spectator lobby/Play, Users, enrollment/reset, recovery, result/History, shared state/privacy/accessibility rules, existing API touchpoints and UI-AC-01–UI-AC-17. User-requested screen coverage is recorded; new routes/layout/audio details remain proposals and API contracts remain in the companion document. |
| LLD-027 | Rust routing libraries and Cloudflare dispatch boundaries | **Selected direction at user request:** Dioxus Router for browser routes; Axum as the HTTP service inside the single Rust API Worker; Static Assets SPA fallback with Worker-first `/api` and `/api/*`; native Durable Object state, alarms and hibernating WebSockets. Section 3.5 records module/middleware responsibility, authoritative checks, no API-to-HTML fallback, Worker/Wasm Send/feature caveats and ROUTE-AC-01–08. Exact pins, wire contracts, adapters and measured/tested compatibility remain pending; no implementation authorized. |
| LLD-028 | Dioxus-integrated Tailwind styling | **Confirmed user direction:** Use Tailwind CSS in Dioxus RSX/shared UI components, compile through the Dioxus-compatible build workflow and serve generated CSS as a static asset. Section 3.6 records source coverage, conditional styles and future production/accessibility checks. Exact toolchain pins, visual tokens and component-library choices remain open; no implementation or tests performed. |

### 12.2 Product/source questions — do not silently decide in implementation

- **Designated-host idle timer (DO-040/041):** Only intentional open/resume or an accepted, state-changing action by the current designated host renews the 24-hour pre-start deadline; a designated-host-initiated transfer qualifies once. Non-designated admin override/open/resume, including admin transfer, does not renew; the successor inherits the existing deadline. DO-041 approves end-exclusive trusted-time comparison at serialized transaction point; expiry wins at equality, no grace, stale alarms recheck/reschedule and never affect In Progress/newer reservation. Physical alarm scheduling and cross-Object recovery remain TBD.
- **Privileged account-management edge cases (DO-105–107):** App-admin self-disable/delete is prohibited; CLI remains separately privileged subject to hosted-game and last-admin guards. Every disable/delete preserves at least one enabled Verified admin; Disabled/PendingEnrollment/ResetRequired do not count. No first-release role-editing operation is approved; role is fixed at account creation. DO-022/HLD-078 enable authority and `disabled_at` lifecycle rules remain unchanged. Physical serialization and API error mapping remain implementation work.
- **UI/speech/quality requirements:** Section 4 / LLD-026 now proposes screen layouts, navigation, controls, shared failure states and UI acceptance scenarios. Styling/routes and contract seams remain reviewable; device-local opt-in speech is proposed, not separately approved. Exact compatibility and measurable venue/accessibility/quality targets remain TBD (Section 4.6, LLD-014).
- **Source reconciliation and HLD approval:** Resolve stale requirements/HLD wording using explicit later decisions, then obtain required stage approval separately. **TBD**.

### 12.3 Traceability index

Coverage means a place to complete the design, not that every design choice or requirement is fulfilled. LLD Section 5 references [Durable Object design](durable-object-design.md), and Sections 6–7 reference API/WSS content in [api-design.md](api-design.md), so coverage ranges containing those sections include their companion documents. Ranges identify source decision rows; superseded rows are carried only as qualified in Section 1.2.

**Subsequent user direction:** LLD-016 records the single-Worker/multiple-endpoint approach and Worker/endpoint/Object distinction. LLD-017 captures the requested A1–H12 proposed operation catalog and scopes in [API design Section 6](api-design.md#operation-catalog), mapped from [API design Section 4](api-design.md#operation-index). LLD-018 captures the structure/field proposal in [Durable Object design Section 6](durable-object-design.md#schema-proposal), mapped from DATA-01–DATA-13 in [Durable Object design Section 3](durable-object-design.md#record-inventory), with explicit outstanding TBDs. LLD-019 adds potential Request/Response inputs/outputs to all [API design Section 6](api-design.md#operation-catalog) operations and links the [API design Section 4](api-design.md#operation-index) coverage rows. LLD-020/LLD-021 add confirmed ID/error conventions; LLD-022 moves detailed API material to the linked companion. LLD-023 settles password minimum length and ASCII/composition rules; LLD-024 subsequently confirms salted Argon2 via RustCrypto `argon2` while leaving profile/runtime details TBD. LLD-025 moves the existing storage/schema proposal and worksheets into the linked Durable Object design document without changing fields/types. These scoped updates do not rewrite the HLD or complete detailed API/storage design. LLD-026 captures the requested concrete UI proposals in Section 4 without changing approved DO decisions or authorizing application work. LLD-027 selects the routing library/integration split in Section 3.5 without finalizing individual UI paths, API contracts or runtime compatibility. LLD-028 records the user-selected Dioxus-integrated Tailwind styling foundation; visual design tokens and build validation remain separate.

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

- [ ] Review/finalize the Section 4 screen specifications, route/contract seams and UI-AC acceptance scenarios; proposed coverage is recorded, not implemented or tested.
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
- [ ] Exercise ROUTE-AC-01–ROUTE-AC-08 on the pinned Worker/Wasm stack; verify API/SPA separation, typed/auth/cookie handling, native upgrades and committed-command recovery.
- [ ] Detailed design approval is recorded; implementation receives its own explicit authorization.

## Sources

[1] https://docs.rs/uuid/latest/uuid — uuid — version features and UUID representation
[2] https://docs.rs/uuid/latest/uuid/struct.Uuid.html — Uuid::now_v7 — UUID v7 generation
[3] https://docs.rs/thiserror/latest/thiserror — thiserror — typed Rust error derivation
[4] https://docs.rs/argon2/latest/argon2 — RustCrypto argon2 — password hashing and verification
[5] https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html — OWASP — Password Storage Cheat Sheet
[6] https://docs.rs/argon2/latest/argon2/constant.RECOMMENDED_SALT_LEN.html — argon2 — recommended salt length
[7] https://docs.rs/argon2/latest/argon2/struct.Params.html — argon2 — memory, iteration, parallelism and output parameters
[8] https://dioxuslabs.com/learn/0.7/essentials/router — Dioxus typed routing
[9] https://docs.rs/worker/0.8.7/worker — workers-rs HTTP integration and Send helpers
[10] https://docs.rs/axum/latest/axum — Axum routing, extractors and Tower middleware
[11] https://raw.githubusercontent.com/cloudflare/workers-rs/main/examples/axum/Cargo.toml — Official Axum-on-Workers dependency example
[12] https://developers.cloudflare.com/workers/static-assets/routing/single-page-application — Workers Static Assets SPA routing
[13] https://developers.cloudflare.com/workers/static-assets/binding — Static Assets binding and selective Worker-first paths
[14] https://developers.cloudflare.com/durable-objects/best-practices/websockets — Durable Object hibernating WebSockets
[15] https://docs.rs/worker/latest/worker/struct.Router.html — Built-in worker::Router alternative
[16] https://dioxuslabs.com/learn/0.7/tutorial/new_app — Dioxus built-in Tailwind build support
[17] https://dioxuslabs.com/learn/0.7/guides/utilities/tailwind — Tailwind styling in Dioxus components
