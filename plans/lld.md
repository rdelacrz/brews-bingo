# Brews Bingo — Low-Level Design Template

## 1. Document status and use

- **Status:** Template with confirmed single-API-Worker direction (LLD-016) and a proposed operation catalog captured at the user’s request (LLD-017; Section 6.4). Final API contracts, Durable Object structures and other implementation specifics remain `TBD`, to be filled in/reviewed by the user.
- **Business:** Rockville Brews.
- **Primary source:** [High-level design](hld.md), including decisions HLD-001–HLD-076.
- **Supporting sources:** [Business requirements](requirements.md) and [hosting research](research.md).
- **Scope:** Structure the detailed design for the initial desktop/mobile web release on Cloudflare. Native Android/iOS implementation remains deferred.
- **Approval boundary:** Creating this template does not approve the overall HLD, resolve open product questions, authorize implementation, or authorize infrastructure/account creation.
- **Reviewer / approval / revision:** TBD.

### 1.1 How to complete this template

**Confirmed input** means a constraint carried from the HLD or a subsequent explicit user direction recorded in the LLD decision register. **TBD** means the user must supply the detailed design. Inventory labels and document IDs below are planning references, not implemented resources. Section 6.4 separately records proposed Rust function names, HTTP methods/paths and auth scopes; recording them does not finalize signatures, wire contracts or storage structures.

For each view, stored record, API operation and event, complete its specification using the relevant reusable worksheet. Preserve the confirmed constraints while choosing implementation details. Any intentional change to an HLD rule needs an explicit decision rather than being hidden in a schema or payload.

Proposed function names and endpoint paths appear in Section 6.4 for review. No final API contract, concrete SQL DDL, HTTP status assignments, serialized enums, JSON schemas, Rust signatures, crate versions or Cloudflare binding names are selected here.

### 1.2 Source precedence and reconciliation

Use explicit later HLD decisions over older source wording:

- HLD-020 specifies online-only gameplay, superseding BR-016/D-005 offline continuity.
- HLD-037 makes **Resolved** winner-only and **Cancelled** the no-winner terminal state.
- HLD-038–HLD-058 settle configuration, attendance, capacity, alias and code rules that some older passages still describe as open.
- HLD-062 permits all enrolled hosts to inspect live boards read-only; HLD-074 extends those capabilities to admins. Earlier transfer wording must not imply exclusive board-read access.
- HLD-069 supersedes spectator terminal reconnect: only the already-delivered local result remains after spectator server data deletion. HLD-071 separately governs player final-view authorization.
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
| Durable ownership | Game Directory Durable Object for discovery/global coordination; one SQLite-backed Game Durable Object per game for gameplay/memberships/boards. Account-store placement remains TBD. | HLD Sections 3, 5, 6; HLD-005, HLD-020 |
| Global limits | At most one saved **New/Awaiting Players** game combined, independently of at most one **In Progress** game application-wide. Separate Objects are not a shared transaction. | HLD-012, HLD-039 |
| Exact lifecycle | **New → Awaiting Players → In Progress → Resolved** for a winner. **New**, **Awaiting Players** or **In Progress → Cancelled** for a no-winner ending. Neither terminal state reopens. | HLD-027, HLD-036–HLD-038 |
| Configuration and start | Configuration changes only in New; no Awaiting Players → New. Start needs at least two distinct currently connected players with valid sessions and no other In Progress game. Spectators never gate start. | HLD-038, HLD-040, HLD-048 |
| Capacities | Integer player limit 2–20, default 20; spectator limit 0–50, default 50. Zero spectators disables spectator admission. | HLD-018, HLD-046–HLD-048 |
| Values and boards | String values; initial pool `1` through configured upper bound, default `75`. Square side 2–10, default 5. Random boards have no repeated value within a board and no cell-for-cell duplicate boards within a game; placement is unrestricted. | HLD-025, HLD-033–HLD-035; BR-025, BR-028, BR-029 |
| Free cells and assignment | Optional valueless satisfied cells, enabled by default with one cell at one-based `(ceil(height / 2), ceil(width / 2))`. Locations configurable up to the whole board. Reject Start if enough distinct boards are impossible. Assign/persist boards at accepted start, never on lobby entry/recovery. | HLD-034, HLD-035 |
| Progression and winner | Backend-only automatic matching after accepted random/manual undrawn values; players never mark. Single Line means a complete row, column or either full corner-to-corner diagonal. Separate Rust traits per winning pattern; signatures TBD. Qualification is not an award; an authorized operator submits one qualifying winner after the real-life Bingo exchange. | HLD-023, HLD-024, HLD-026, HLD-042, HLD-074 |
| Durable commands | Persist mutation, automatic board/qualification effects, revision and command result before acknowledgement/broadcast. Lost responses do not justify a new random call; no offline command queue. | HLD Sections 6.2, 7.6, 8 |
| Game codes | Exactly eight generated uppercase ASCII letters A–Z/digits 0–9; case-insensitive input, trim outer ASCII spaces, reject internal spaces. Valid while Awaiting Players/In Progress. Reuse only when absent from live games and unexpired History; stable identities prevent old-credential transfer. | HLD-056–HLD-058 |
| Aliases | Nonempty, at most 20 ASCII characters after outer-space trim; printable letters/digits/punctuation, internal spaces preserved. Case-insensitive uniqueness per game. Rename only before start; recovery uses current alias. Old aliases release after rename/pre-start Leave/player-to-spectator switch. | HLD-049–HLD-055 |
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

Authorization implementation, role representation, permission helpers and audit format: **TBD**.

### 2.3 Attendance, exit and retention inputs

| Subject | Confirmed behavior | Mechanism |
| --- | --- | --- |
| Player before start | Explicit Leave releases seat/alias; fresh eligible rejoin allowed. Accidental disconnect retains membership/seat until start/cancellation; retained disconnected members receive start-time boards but do not satisfy the connected minimum. | TBD |
| Player during play | Leave/disconnect retains seat, board, matching and winner eligibility. Return uses valid session or approved answer proof; no replacement/new player admission. | TBD |
| Spectator departure | Explicit Leave releases seat immediately. Accidental disconnect has five-minute grace; no seat/session renewal by pretending to be another role. | TBD |
| Pre-start role switch | Both directions within target capacity; player target needs available alias. Failure preserves original role/seat. Player-to-spectator switch releases alias and deletes player recovery verifier; fresh optional enrollment on return. | TBD |
| Account/session credentials | Single-use enrollment/reset links last one day from issuance; sessions have fixed independent one-day expiry, not sliding expiry. Backend and open sockets enforce expiry/revocation. Account logins may coexist. | TBD |
| Player answer recovery | Optional private answer; code + current alias + matching enrolled answer replaces old player sessions/sockets, restores same membership in Awaiting Players/In Progress. No forgotten-alias lookup or alternative proof. Valid-session users may set/replace/delete their answer in those states. | TBD |
| Unstarted abandonment | After 24 hours without qualifying designated-host activity, commit Cancelled and delete game/participant data, no History. Host open/resume or accepted action resets the timer, not passive heartbeats/other viewers. Admin-override interaction is flagged in Section 12. | TBD |
| Pre-start cancellation | Commit Cancelled, then delete game/participant data without History. Notify joined participants; independent exit does not delay deletion. Intentional cancellation requires confirmation. | TBD |
| Started-game History | Final ordered calls, player aliases, winning alias if any and final board snapshots share **three calendar months from first terminal commit**, with minimal identity/code/expiry metadata. No credentials, intermediate replay archive or initial-release export. Deny expired reads even if deletion is delayed. | TBD |
| Terminal player access | Delete recovery verifier; retain only minimal existing-session/exit authorization until Exit or original session expiry. No new terminal recovery session. Pre-start deletion remains an exception with no reconnect. | TBD |
| Terminal spectator access | Deliver result to connected spectators, delete server identity/session data; local delivered result may remain until Exit. No terminal reconnect/refresh recovery. | TBD |
| Host/admin accounts | No inactivity expiry. Authorized disable/delete revokes access and is blocked while the account hosts a nonterminal game; transfer/cancel first. Do not delete unrelated/unexpired History. | TBD |

Sources: HLD-020–HLD-022, HLD-032, HLD-036–HLD-045, HLD-061, HLD-064–HLD-072, HLD-074.

## 3. Component and module design

| Logical component | Confirmed responsibility | Detailed design to fill |
| --- | --- | --- |
| Shared Rust domain | Pure rules, board feasibility/generation/matching and pattern-specific trait boundaries. | TBD — crates, types, traits, signatures, algorithms and errors. |
| Shared contracts | Shared safe app/backend contract types without provider/UI dependencies. | TBD — serialization, versioning and module boundaries. |
| Shared Dioxus UI / `platform/web` | Web views, nonauthoritative presentation, browser adapters and protected-cookie interactions. | TBD — routing, components, state management and adapters. |
| Rust API Worker | One initial deployable API service with multiple endpoint handlers; HTTP/WSS entry, authentication/authorization, request validation and routing to authoritative owners. | TBD — specific APIs, Rust modules/router, middleware, internal interfaces and failure mapping. |
| Game Directory Durable Object | Issued-code lookup and application-wide lifecycle coordination. | TBD — class/binding names, operations, schema and concurrency protocol. |
| Per-game Durable Object | Authoritative game/board/membership state, SQLite writes, role-filtered hibernating sockets. | TBD — class/binding names, schema, transactions, attachments and alarms. |
| Account/session authority | Strongly consistent credential lifecycle and server-owned account permissions. | TBD — physical Cloudflare store, ownership, interfaces and atomicity. |
| Developer CLI | Developer-restricted account operations through an authorized persistence/backend path. | TBD — language, packaging, commands, auth and private output delivery. |
| Later `platform/android` / `platform/ios` | Future Dioxus adapters/entrypoints without duplicating domain/contracts. | TBD — deferred; no native implementation in this release. |

Dependencies, compatible pinned toolchain/SDK versions and build commands: **TBD**. This inventory does not create a repository scaffold.

### 3.1 One API Worker, multiple endpoints

**Confirmed user direction (LLD-016):** Start with **one API Worker**, not one Worker per API endpoint. Incoming HTTP requests enter the Worker's fetch entrypoint and are dispatched by method/path to appropriate handlers. Organize handlers by responsibility in Rust modules; one deployment does not require one large function or one source file. Router choice, module names and signatures remain **TBD**.

The same API Worker covers the approved authentication, Users/account management, game operations, participant admission/recovery, History and WebSocket-upgrade responsibilities. These are responsibility groups, not a finalized endpoint list. Section 6.4 now records the requested proposed operation/function/path/scope catalog; the user will refine it and fill in detailed API contracts later.

This decision does not settle account-store placement or require every backend concern to live in a Game Object.

### 3.2 Worker, endpoint and Durable Object boundaries

| Concept | Responsibility in this design |
| --- | --- |
| API Worker | Deployable backend entry service that receives requests, enforces access checks and dispatches to the appropriate handler/owner. |
| API endpoint | An externally exposed operation identified by its eventual method/path; many endpoints share the API Worker. Exact endpoint definitions remain TBD. |
| Durable Object class / namespace | Stateful behavior and its collection of instances, accessed through configured backend namespace bindings. Concrete classes, binding names and interfaces remain TBD. |
| Durable Object instance | A stable state owner selected within a namespace. The existing HLD calls for one Game Object per game, not one per endpoint or browser. Concrete structures and schemas remain TBD. |

Conceptual game-request flow, not a selected API or internal protocol:

```text
Browser HTTPS request / WSS upgrade
    -> Single Brews Bingo API Worker
    -> Request validation, session/role checks and handler dispatch
    -> Directory lookup when needed: issued code -> stable game identity
    -> Configured namespace binding / handle for the owning Game Object
    -> Authoritative game checks, durable changes and authorized result/update
```

Namespace bindings provide backend resource access; object names/IDs select instances within that scope. They do not authenticate end users or prove a request came from the Dioxus frontend. The public Worker must enforce the session/role/action checks even when called outside the UI; a game code or Object ID is not an authorization credential. No browser receives Cloudflare credentials or direct SQLite access. Exact Worker-to-Object calls and trusted-context propagation remain **TBD**.

An Object's in-memory state is temporary; accepted game data must remain in its durable storage so activation after sleep/restart restores the same logical game. This does not introduce another store or change the existing persist-before-acknowledgement/broadcast rule.

## 4. Views and frontend contracts

### 4.1 View inventory

These are logical experiences, not a committed page/component count. Screen grouping, route paths, layout and state implementation are **TBD** for every row.

| ID | View / experience | Confirmed content and permission boundary | Detailed specification |
| --- | --- | --- | --- |
| VIEW-01 | Account sign-in | Host/admin password login, session-expired/access-denied feedback; no public registration. | TBD |
| VIEW-02 | Enrollment and password setup | One-day single-use link, restricted session, mandatory personal password setup; used/expired-link outcomes. | TBD |
| VIEW-03 | Account password reset | Privileged-issued single-use reset link and restricted new-password flow; not anonymous self-service recovery. | TBD |
| VIEW-04 | Game list / account home | Host/admin game discovery, create/resume and role-correct cross-game access; respect independent saved/live limits. | TBD |
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
| VIEW-15 | **Users** and account-management flow | Admin-only list of host/admin accounts; authorized creation/link/reissue/reset/disable/delete. Backend must also deny non-admin listing/operations; never list secrets. | TBD |
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

## 5. Database structure and schema — Cloudflare

### 5.1 Physical ownership worksheet

**Confirmed input:** Directory/game data uses SQLite-backed Durable Objects. There is one Game Object per game, not per board. Cross-Object calls are not a database transaction. Account-store placement, History discovery/indexing and exact record layout are undecided. Do not silently add D1, KV, R2 or another datastore; any new store needs a recorded decision. Eventually consistent KV is not the authoritative game/admission/active-slot registry.

**Deferred to the user:** Specific Durable Object structures, class/binding names, internal methods, stored fields and schemas will be supplied later. The ownership and record inventories below retain HLD responsibilities only; they do not select those structures or imply one Object per record group.

| Owner / concern | Logical data responsibility | Physical details |
| --- | --- | --- |
| Game Directory Object | Issued code → stable game lookup; application-wide saved/live coordination. Retained-History code checks must participate in issuance safety. | TBD — object identity, binding, schema, concurrency and reconciliation. |
| Game Object | Configuration, lifecycle, memberships/alias claims, boards/matches/qualifiers, calls, outcome, revisions and command results. | TBD — tables/records, indexes, transactions, deadlines and cleanup. |
| Account/session authority | Host/admin accounts, password/setup/reset state, link/session verifiers and authorization. | TBD — physical store, partition/owner, bindings and cross-owner access. |
| History and lookup/indexes | Self-contained final snapshots and minimal code/identity/expiry metadata; common three-month deadline. | TBD — physical placement, discovery, indexes and deletion coordination. |
| Operational metadata | Exit/access state, expiry/grace deadlines, reliable coordination and required nonsecret auditing. | TBD — minimum records, placement, retention and cleanup; not a hidden archive. |

### 5.2 Logical record inventory

These are conceptual groups, **not selected table names or a normalized schema**. Storage consolidation/splitting, identifiers, columns/types/constraints and relationships are TBD.

| ID | Record group | Required information / invariant from HLD | Schema |
| --- | --- | --- | --- |
| DATA-01 | Provisioned account | Stable identity, host/admin type, enrollment/password/reset/access state; Users-safe projection distinct from credentials. | TBD |
| DATA-02 | Access/enrollment/reset credential | Account/purpose binding, verifier, issue/expiry/consumed/revoked state; one-day single use. | TBD |
| DATA-03 | Account and restricted sessions | Account/scope binding, verifier, fixed expiry/revocation; restricted setup/reset is not normal account authority. | TBD |
| DATA-04 | Game directory and reservations | Code/stable-game mapping; separate saved-unstarted/live allowances, game-specific retry-safe release. | TBD |
| DATA-05 | Game and configuration | Designated host, exact lifecycle, fixed rules/pool/grid/free cells/capacities, relevant durable timestamps. | TBD |
| DATA-06 | Membership / alias / occupancy | Stable game/member identity, role, normalized alias claim, admission/Leave/presence/grace state; sockets are not seats. | TBD |
| DATA-07 | Participant sessions / exit authorization | Game/member or spectator binding, player current-alias association, fixed expiry/revocation and permitted final-view exit state. | TBD |
| DATA-08 | Private player recovery verifier | Stable member binding and protected verifier; optional, replaceable/deletable by valid-session owner; never public/History data. | TBD |
| DATA-09 | Assigned player board | Immutable assigned layout/string values/free cells plus automatic matched state; no pre-start or recovery-generated replacement. | TBD |
| DATA-10 | Ordered calls / qualification / outcome | Accepted call order and unique values, authoritative eligibility, at most one winner and immutable terminal result. | TBD |
| DATA-11 | Revisions / command outcomes | Durable consistency/retry identity and accepted result; bounded retention design without authorizing a replay archive. | TBD |
| DATA-12 | Final History and indexes | Final calls/aliases/winner/boards plus minimal metadata, shared expiry and code-collision lookup; no session/answer credentials. | TBD |
| DATA-13 | Deadlines / coordination / audit | Minimum durable state needed for cancellation/expiry/grace, cross-Object recovery and actual admin-action attribution. Exact content/retention remains open. | TBD |

### 5.3 Per-record/table schema worksheet — copy for each record

- **Record ID / purpose / HLD references:** TBD.
- **Authoritative Object/store and binding:** TBD.
- **Table/record name and schema version:** TBD.
- **Lifecycle / readers / writers / privacy classification:** TBD.

| Column / property | SQL or record type | Nullable / default | Key / constraint / validation | Sensitive-data treatment | Meaning |
| --- | --- | --- | --- | --- | --- |
| TBD | TBD | TBD | TBD | TBD | TBD |

| Schema design item | Value to fill |
| --- | --- |
| Primary identifier and generation | TBD |
| Relationships and ownership boundaries | TBD |
| Unique/check constraints, canonical alias/code representation | TBD |
| Indexes and query/access patterns | TBD |
| Foreign-key behavior or application-enforced cross-owner references | TBD |
| Read/write statement shapes and parameterization | TBD |
| Transaction boundary and invariants maintained | TBD |
| Concurrency/conflict detection and retry outcome | TBD |
| Retention trigger, persisted deadline, read denial and deletion | TBD |
| Copies/indexes/logs/backups and restore-time expiry enforcement | TBD |
| Size, read/write amplification and quota assumptions | TBD |
| DDL / initialization / migration / rollback | TBD |
| Schema and constraint tests | TBD |

**DDL and query definitions:** TBD — intentionally no executable SQL in this template.

### 5.4 Cross-owner consistency, expiry and migration

| Design topic | Required constraint | Detailed mechanism |
| --- | --- | --- |
| Create / publish / start | No overwritten saved game, exposed unready code or two live games; admit/start/assign consistently. | TBD |
| Terminal transition / release | Freeze durable result before game-specific slot release; no stale release clears a newer reservation. | TBD |
| Call transaction | Calls, affected boards, qualification, revision and command result commit together before push. | TBD |
| Credential redemption / reset | Atomic single-use consume + restricted session; reset/reissue invalidate predecessors under reviewed policy. | TBD |
| Code reuse / History expiry | Check live and unexpired History without permanent-code-ledger assumptions; old credentials never move to a reused code. | TBD |
| Retention scheduler | Enforce Section 2.3 deadlines without client visits or in-memory-only timers; preserve three **months**, not an assumed 90 days. | TBD |
| Notice then deletion | Deliver permitted final notices without retaining deleted participant credentials indefinitely or waiting for Exit. | TBD |
| Migration / restore | Preserve authority, committed boards, absolute deadlines and terminal immutability; never resurrect expired data. | TBD |

Migration inventory: **TBD** — version, affected Object/store, compatibility window, forward/backout actions, verification and failure recovery.

## 6. API requests and responses — Cloudflare Workers

### 6.1 Shared contract worksheet

| API concern | Confirmed input | Implementation specification |
| --- | --- | --- |
| Transport and routing | One API Worker serves multiple HTTPS endpoints and WSS upgrades, dispatching to handlers and authoritative backend owners (LLD-016). | Proposed methods/paths in Section 6.4; final contracts, base URL/versioning, Rust router/handler signatures and internal calls remain TBD. |
| Authentication | Backend-issued Secure/HttpOnly/SameSite cookies; fixed one-day expiry and restricted versus normal scopes. | TBD — cookie names/attributes, session lookup, CSRF/origin rules and error mapping. |
| Authorization | Server-owned role plus game assignment; admin overrides ownership only. Users/account-list data is admin-only. | TBD — middleware, checks and commit-time reauthorization. |
| Request validation | Preserve string values, alias/code normalization, lifecycle and capacity rules. | TBD — media types, encodings, schemas, unknown-field policy and limits. |
| Response contract | Return authorized state/results only; secret fields absent from normal views and logs. | TBD — payload envelopes, field names, status codes, headers and caching. |
| Retry safety | Actor/game-scoped command identity and durable outcome lookup; repeat accepted command without repeating effects. | TBD — idempotency transport, conflict behavior, result retention and retry policy. |
| Errors | Distinguish authentication, authorization, invalid input, state/capacity conflicts, expiry and unavailable/unknown outcome safely. | TBD — codes, response schemas, public messages and information-leak limits. |
| Compatibility / observability | No selected wire schema or API version; redact sensitive inputs and links. | TBD — version policy, correlation/audit fields, logs and traces. |

### 6.2 Logical operation inventory

These stable API-01–API-32 rows remain a high-level coverage checklist, not finalized endpoints or separate Worker deployments. The catalog-reference column maps them to the expanded A1–H12 entries in Section 6.4, including associated internal work. That section records proposed functions, methods/paths and auth scopes. Each eventual API still needs the worksheet in Section 6.3: **parameter definitions, request bodies, success/error responses and status codes remain TBD**. Internal entries do not acquire public routes through this mapping. Final endpoint grouping remains subject to user review within the single-API-Worker boundary (LLD-016).

| ID | Logical operation | Confirmed actor / guard / outcome | Proposed catalog entries (Section 6.4) | Request / responses |
| --- | --- | --- | --- | --- |
| API-01 | Account password login | Host/admin credentials after completed setup; issue fixed-life account session. | A1 | TBD |
| API-02 | Enrollment-link redemption / password setup | Valid unused link → restricted session → personal password; no early privileges or deadline extension. | A3, A4 | TBD |
| API-03 | Reset-link redemption / new password | Valid single-use reset proof; restricted reset authority, same account. | A5, A6 | TBD |
| API-04 | Session validation / logout | Resolve server-owned authority or revoke current access; game Exit is a separate concept. | A2, A7, H6 | TBD |
| API-05 | Users account listing | Enrolled admin only; list host/admin accounts without secrets. | B1, B2 | TBD |
| API-06 | Provision host/admin account | Developer CLI or existing admin; first admin via CLI. Persist pending account/link before returning private URL. | B3, B4 | TBD |
| API-07 | Reissue enrollment link | Privileged caller, same pending account; invalidate prior links/restricted sessions. | B5 | TBD |
| API-08 | Initiate account password reset | Privileged caller; issue one-day single-use link, revoke target sessions/sockets immediately and block old-password login while pending. | B6 | TBD |
| API-09 | Disable / delete account | Privileged caller; reject if target still hosts a nonterminal game, revoke access without deleting unrelated History. | B7, B8, H3 | TBD |
| API-10 | List / inspect games | Host/admin authorized cross-game view; read permission alone never grants host-role mutation. | C1, C2, C3, C5 | TBD |
| API-11 | Create New game | Host/admin creator becomes designated host; reject if saved-unstarted allowance occupied. | C6, H2 | TBD |
| API-12 | Read / update configuration | Reads authorized; changes require designated host/admin and New. | C5, C7 | TBD |
| API-13 | Publish Awaiting Players | Authorized operator; validate/fix configuration and durably issue code. | C8, H1 | TBD |
| API-14 | Open / resume selected game | Restore existing identity/configuration/code/state; role/lifecycle/exit restrictions apply. | C9 | TBD |
| API-15 | Start game | Designated host/admin, Awaiting Players, connected minimum/global slot/feasibility checks; persist start-time boards. | E6 | TBD |
| API-16 | Random / manual value call | Designated host/admin, playable In Progress; valid undrawn value → atomic calls/matches/qualification result. | E7, E8 | TBD |
| API-17 | Submit one qualified winner | Designated host/admin validates same-game qualification; commit Resolved once. | E9, H9 | TBD |
| API-18 | Cancel / manually end game | Designated host/admin with required confirmation; commit Cancelled, apply pre-start versus started retention. | E10, E11, H7, H8 | TBD |
| API-19 | Transfer designated host | Authorized operator confirms target; immediate committed transfer, no recipient acceptance or data reset. | C10, H12 | TBD |
| API-20 | Query command outcome | Authorized original command context; resolve lost acknowledgement without another effect. | G4, G5, H11 | TBD |
| API-21 | Resolve game code / entry eligibility | Known published code only; discovery is not identity or admission. | C4, D3 | TBD |
| API-22 | Join as player | Awaiting Players, valid available alias/free slot, optional answer; no board before start. | D4, D5 | TBD |
| API-23 | Join as spectator | Awaiting Players/In Progress and spectator capacity; no player privileges. | D6, H5 | TBD |
| API-24 | Rename own alias | Valid player session, Awaiting Players; available normalized name, stable membership and unchanged session expiry. | D7 | TBD |
| API-25 | Switch participant role | Awaiting Players and target eligibility; consistent seat/session/alias/verifier transition, no second admission. | D8, D9 | TBD |
| API-26 | Participant Leave | Valid owner; pre-start player versus In Progress player versus spectator policies differ. | D10, D11, D12 | TBD |
| API-27 | Recover player session | Code + current alias + enrolled answer; nonterminal restoration of same membership, replace old sessions/sockets. | D13 | TBD |
| API-28 | Set / replace / delete own recovery answer | Valid player session, Awaiting Players/In Progress; no alternate ownership proof. | D14 | TBD |
| API-29 | Final-view Exit / access cleanup | Independent exit, no outcome mutation or renewed game access; spectator local-only exit and pre-start deletion must not require a surviving server session. | F1, F2 | TBD |
| API-30 | History listing / detail | Host/admin, unexpired immutable final data only; no export or membership restoration. | F3, F4, H10 | TBD |
| API-31 | Authorized revision check / resynchronization | Repair freshness without treating heartbeat as state agreement; respect role/exit/expiry. | D1, D2, E1, E2, E3, E4, E5, G3 | TBD |
| API-32 | WSS connection / upgrade | Valid authorized game context; first application state is full role-filtered snapshot; Section 7 details TBD. | G1, G2, H4 | TBD |

No endpoints for manual board marking, public privileged registration, self-elevation, post-terminal gameplay, History export or alternate player recovery are implied. CLI-to-backend transport remains TBD; the catalog does not require the CLI to use public app routes.

### 6.3 Per-operation request/response worksheet — copy for each API operation

- **Operation ID / purpose / source decisions / consuming views:** TBD — link the relevant Section 6.2 API ID and Section 6.4 catalog ID(s).
- **Public Worker handler versus internal Object/CLI interface:** TBD.
- **Method / path / API version:** TBD — review the Section 6.4 proposal; internal helpers do not require an HTTP endpoint.
- **Authentication / allowed roles / lifecycle / ownership or admin override:** TBD.
- **Preconditions / validation / CSRF and origin checks:** TBD.

#### Request

| Location | Name | Type / format | Required / default | Validation / sensitivity |
| --- | --- | --- | --- | --- |
| Path / query / header / cookie / body: TBD | TBD | TBD | TBD | TBD |

**Body schema and sanitized example:** TBD. Recovery answers belong only in protected HTTPS request bodies, never query strings or WSS. Do not put real tokens, passwords or credential-bearing URLs in this document.

#### Success response

| Status | Header / cookie effects | Body field | Type / meaning | Visibility / cache policy |
| --- | --- | --- | --- | --- |
| TBD | TBD | TBD | TBD | TBD |

**Response schema and sanitized example:** TBD. Specify restricted-session versus normal-session effects and any durable-result/revision reference; do not expose HttpOnly session tokens in normal response bodies.

#### Error responses and side effects

| Failure condition | Status / error code | Public response schema | Durable effect / retry safety |
| --- | --- | --- | --- |
| TBD | TBD | TBD | TBD |

- **Backend owner / record reads / writes / transaction:** TBD.
- **Idempotency, concurrent requests and unknown-outcome recovery:** TBD.
- **Committed-result acknowledgement and WSS interaction:** TBD.
- **Revocation/expiry during execution, logging/redaction and audit:** TBD.
- **Contract, permission, validation and failure tests:** TBD.

### 6.4 Proposed operation and data-access catalog

**Status:** Captured at the user's request from the operation walkthrough. IDs A1–H12, Rust function names, methods/paths and auth scopes below are **proposals for review**, not implemented handlers or finalized contracts. The category/operation sections retain the requested plain-text format. Section 6.2 remains the stable high-level coverage index; its references point to this expanded catalog. Request/response schemas, parameter definitions, status/error codes, Rust signatures, router configuration and Durable Object structures remain **TBD**.

**Scope and access conventions:**
- **Mutating** includes changes to credentials, membership, connections or lifecycle, not just game data.
- **Non-mutating** means read-only, excluding ordinary request logging.
- **Admin:** authenticated, fully enrolled admin.
- **Host:** authenticated, fully enrolled host **or admin**. Game mutations additionally require designated-host ownership unless the caller is an admin.
- **Player:** authenticated player acting on their own membership. Host/admin privileges do not grant anonymous-player impersonation or recovery override.
- **Anyone:** no privileged account role required; token, admission, session and ownership checks still apply where stated. This includes spectators and unauthenticated entry/recovery flows; it never means unrestricted access to another user's data.
- **System:** backend-only operation, not independently callable by users. The user explicitly selected this label for internal operations; it is not a provisioned account role.
- Comma-separated scopes mean either role is permitted. Every operation still enforces the description's lifecycle, ownership, credential and privacy conditions.

**Mapping conventions:** Function names describe proposed handlers/helpers, not signatures. Paths include a suggested HTTP method; **— Internal** means no public URL. Repeated Leave/Cancel paths are server-selected role/state branches, not duplicate routes. Server push and retry helpers are not extra public endpoints. Reads may be combined into role-specific snapshots; dependent internal writes remain part of the command that owns them. Credentials and recovery answers are supplied through protected request bodies/cookies as appropriate, never interpolated into these paths. The developer CLI's transport remains separate/TBD.

#### A. Authentication and sessions

##### A1: `login_account` (Mutating)

Path: `POST /api/auth/login`

Auth scope: Anyone

Validate submitted account credentials and enrollment/reset/disabled state; create a fixed-one-day account session. Existing account sessions may remain valid.

##### A2: `get_current_session` (Non-mutating)

Path: `GET /api/session`

Auth scope: Anyone

Resolve the presented session and return its safe identity, scope, expiry and current permissions. Missing or invalid credentials do not disclose another identity or grant access. Never return credential verifiers.

##### A3: `redeem_enrollment_link` (Mutating)

Path: `POST /api/auth/enrollment/redeem`

Auth scope: Anyone

A valid enrollment token is required. Validate account binding, purpose, expiry and unused status; atomically consume the token and create a restricted password-setup session.

##### A4: `complete_password_setup` (Mutating)

Path: `POST /api/auth/enrollment/complete`

Auth scope: Anyone

A valid restricted enrollment session is required. Persist the chosen password verifier and setup completion; rotate into normal account authority without extending the restricted session’s original deadline.

##### A5: `redeem_password_reset_link` (Mutating)

Path: `POST /api/auth/password-reset/redeem`

Auth scope: Anyone

A valid reset token is required. Validate and consume the single-use token; establish restricted password-reset authority for the bound account.

##### A6: `complete_password_reset` (Mutating)

Path: `POST /api/auth/password-reset/complete`

Auth scope: Anyone

A valid restricted reset session is required. Replace the password verifier, complete reset state and retire restricted reset authority. Exact post-reset navigation/session behavior remains TBD.

##### A7: `logout_session` (Mutating)

Path: `POST /api/auth/logout`

Auth scope: Anyone

Revoke the caller’s current session, clear its cookie and stop associated authorized socket delivery. This cannot revoke another user’s session and is separate from leaving a game.

#### B. Users and privileged account management

The scopes below describe the app-facing endpoints. The developer CLI retains its separately authorized management and bootstrap capabilities.

##### B1: `list_users` (Non-mutating)

Path: `GET /api/users`

Auth scope: Admin

List safe metadata for provisioned host/admin accounts. Exclude anonymous memberships and secrets. Columns, filtering, sorting and pagination remain TBD.

##### B2: `get_user` (Non-mutating)

Path: `GET /api/users/{account_id}`

Auth scope: Admin

Read a target account’s safe metadata and lifecycle state for management. This does not expose password, token or recovery verifiers.

##### B3: `create_host_account` (Mutating)

Path: `POST /api/users/hosts`

Auth scope: Admin

Validate the proposed account identifier; create a pending host account and one-day enrollment link. Return the link only after durable creation. The developer CLI may also perform this through its privileged path.

##### B4: `create_admin_account` (Mutating)

Path: `POST /api/users/admins`

Auth scope: Admin

Create a pending admin account and enrollment link. Only an existing admin or the developer CLI may perform this; first-admin bootstrap uses CLI.

##### B5: `reissue_enrollment_link` (Mutating)

Path: `POST /api/users/{account_id}/enrollment-links`

Auth scope: Admin

For the same pending account, invalidate previous enrollment links/restricted sessions and issue a replacement one-day single-use link.

##### B6: `initiate_password_reset` (Mutating)

Path: `POST /api/users/{account_id}/password-reset-links`

Auth scope: Admin

Enter reset-pending state, immediately revoke target-account sessions/sockets, block old-password login and issue a one-day reset link.

##### B7: `disable_account` (Mutating)

Path: `POST /api/users/{account_id}/disable`

Auth scope: Admin

Reject if the account still hosts a nonterminal game. Otherwise disable access and revoke its credentials/connections as required.

##### B8: `delete_account` (Mutating)

Path: `DELETE /api/users/{account_id}`

Auth scope: Admin

Apply the hosted-game guard, then remove account/credential data without deleting unrelated or unexpired History. Preserve only permitted historical references.

#### C. Game discovery, configuration and ownership

##### C1: `list_games` (Non-mutating)

Path: `GET /api/games`

Auth scope: Host

List current-game summaries with lifecycle, designated host and appropriate access information. Detailed filtering remains TBD.

##### C2: `get_saved_game` (Non-mutating)

Path: `GET /api/games/saved`

Auth scope: Host

Find the game occupying the application-wide New/Awaiting Players allowance, if any. Supports resumption and creation-blocked feedback.

##### C3: `get_active_game` (Non-mutating)

Path: `GET /api/games/active`

Auth scope: Host

Find the game occupying the application-wide In Progress designation, independently of the saved-unstarted allowance.

##### C4: `resolve_game_code` (Non-mutating)

Path: `GET /api/game-codes/{code}`

Auth scope: Anyone

Normalize and resolve an issued code to an existing game and limited entry information. Unknown codes never create games; discovery grants no membership, private data or gameplay permissions.

##### C5: `get_game` (Non-mutating)

Path: `GET /api/games/{game_id}`

Auth scope: Anyone

Read only the caller-authorized game/configuration projection. Host/admin accounts and admitted player/spectator sessions receive their permitted fields. A game ID alone does not authorize private data or unrestricted game inspection.

##### C6: `create_game` (Mutating)

Path: `POST /api/games`

Auth scope: Host

Create a distinct New game, assign its creator as designated host and establish the host-idle deadline. Reject if the saved allowance is occupied.

##### C7: `update_game_configuration` (Mutating)

Path: `PATCH /api/games/{game_id}/configuration`

Auth scope: Host

The designated host or an authorized admin updates pool, board, free cells or capacities only in New. Validate the resulting configuration without partially applying invalid changes.

##### C8: `open_game_lobby` (Mutating)

Path: `POST /api/games/{game_id}/lobby`

Auth scope: Host

The designated host or an authorized admin freezes validated configuration, transitions to Awaiting Players and publishes its issued code consistently with game state.

##### C9: `resume_game` (Mutating)

Path: `POST /api/games/{game_id}/resume`

Auth scope: Host

Restore existing state without recreating codes/boards, subject to game permissions. A qualifying designated-host open/resume also updates the unstarted host-activity deadline. Passive reads must not silently refresh that deadline; the effect of non-designated admin activity remains TBD.

##### C10: `transfer_game_host` (Mutating)

Path: `POST /api/games/{game_id}/host-transfer`

Auth scope: Host

Validate designated-host or admin authority, target eligibility and confirmation; immediately replace designated-host assignment while preserving game data and reservations. No recipient acceptance is required.

#### D. Membership, admission, aliases and recovery

##### D1: `get_my_membership` (Non-mutating)

Path: `GET /api/games/{game_id}/membership`

Auth scope: Anyone

A valid participant session is required. Read its own membership’s role, current alias, access state and recovery-enabled status where applicable. Never return the answer/verifier or another participant’s membership.

##### D2: `list_game_players` (Non-mutating)

Path: `GET /api/games/{game_id}/players`

Auth scope: Host

Read player aliases and permitted membership/presence information. Distinguish retained disconnected players from currently connected players.

##### D3: `get_game_availability` (Non-mutating)

Path: `GET /api/games/{game_id}/availability`

Auth scope: Anyone

Read admission/start availability through a caller-appropriate projection. Unauthenticated entry flows receive only permitted entry information, not private roster data. Detailed occupancy and start eligibility depend on caller authority; spectators never satisfy the connected-player minimum.

##### D4: `is_alias_claimed` (Non-mutating)

Path: — Internal

Auth scope: System

Check a normalized alias claim within a game during admission, rename or role switching. No public alias-availability endpoint is implied.

##### D5: `join_game_as_player` (Mutating)

Path: `POST /api/games/{game_id}/players`

Auth scope: Anyone

In Awaiting Players, validate the code-based admission context, alias and capacity; claim alias/seat, create membership and optional answer verifier, and issue a player session. No board is generated yet.

##### D6: `join_game_as_spectator` (Mutating)

Path: `POST /api/games/{game_id}/spectators`

Auth scope: Anyone

Validate the code-based admission context and Awaiting Players/In Progress capacity; allocate spectator identity/seat and issue its session. No player or account privileges are granted.

##### D7: `rename_my_alias` (Mutating)

Path: `PATCH /api/games/{game_id}/membership/alias`

Auth scope: Player

Before start, claim the new alias and release the old one. Preserve membership, seat, answer verifier and original session expiry.

##### D8: `switch_to_spectator` (Mutating)

Path: `POST /api/games/{game_id}/membership/switch-to-spectator`

Auth scope: Player

Before start, acquire spectator admission, release player seat/alias and delete the player recovery verifier. Failure preserves the original role.

##### D9: `switch_to_player` (Mutating)

Path: `POST /api/games/{game_id}/membership/switch-to-player`

Auth scope: Anyone

A valid existing spectator session is required. Before start, claim an eligible player seat/alias, release spectator occupancy and optionally enroll a fresh answer. Do not restore deleted credentials or permit an unauthenticated membership takeover.

##### D10: `leave_player_lobby` (Mutating)

Path: `POST /api/games/{game_id}/membership/leave`

Auth scope: Player

Player Leave before start releases seat/alias and removes that membership from the start roster. Returning requires fresh eligible admission. Dispatch this branch using server-verified role and lifecycle.

##### D11: `leave_player_game` (Mutating)

Path: `POST /api/games/{game_id}/membership/leave`

Auth scope: Player

Player Leave during play retains seat, board, automatic matching and award eligibility. Preserve approved valid-session/answer-based return. Dispatch this branch using server-verified role and lifecycle.

##### D12: `leave_spectator_game` (Mutating)

Path: `POST /api/games/{game_id}/membership/leave`

Auth scope: Anyone

A valid spectator session is required. Immediately release that spectator’s occupancy and delete its identity/session data. Explicit Leave does not receive accidental-disconnect grace. Dispatch using server-verified role and lifecycle.

##### D13: `recover_player_session` (Mutating)

Path: `POST /api/player-recovery`

Auth scope: Anyone

Verify submitted code, current alias and enrolled answer; replace old sessions/sockets and restore the same nonterminal membership/board without another seat. No existing player session is required, but the approved recovery proof is mandatory.

##### D14: `manage_my_recovery_answer` (Mutating)

Path: `PUT /api/games/{game_id}/membership/recovery-answer` or `DELETE /api/games/{game_id}/membership/recovery-answer`

Auth scope: Player

Set/replace the caller’s answer using PUT or delete it using DELETE in Awaiting Players/In Progress. Deletion disables answer recovery. A lost-session caller cannot use this operation to establish ownership.

#### E. Live-game reads and commands

##### E1: `get_game_calls` (Non-mutating)

Path: `GET /api/games/{game_id}/calls`

Auth scope: Anyone

Return the most recent value and committed call sequence to authorized game viewers, including admitted spectators. A valid account or participant access context is required; knowing the game ID alone is insufficient.

##### E2: `get_remaining_values` (Non-mutating)

Path: `GET /api/games/{game_id}/remaining-values`

Auth scope: Host

Derive/read the remaining pool and exhaustion state from configuration and accepted calls for the host/admin operating or inspection view. Audience-facing exhaustion status can be included in the permitted game snapshot without exposing this full query.

##### E3: `get_my_board` (Non-mutating)

Path: `GET /api/games/{game_id}/membership/board`

Auth scope: Player

Read the caller’s assigned layout, free cells, automatic matches and qualification. No board exists before start.

##### E4: `get_game_boards` (Non-mutating)

Path: `GET /api/games/{game_id}/boards`

Auth scope: Host

Inspect all or selected player boards read-only. Selection/filter details remain TBD; designated-host ownership is not required for inspection.

##### E5: `get_game_qualification` (Non-mutating)

Path: `GET /api/games/{game_id}/qualification`

Auth scope: Host, Player

Hosts/admins receive qualifying players; players receive only their own qualification. Spectators receive no private qualifier details.

##### E6: `start_game` (Mutating)

Path: `POST /api/games/{game_id}/start`

Auth scope: Host

The designated host or an authorized admin validates the connected-player minimum, reservation availability and board feasibility; starts the game and persists boards/initial qualification for the retained start roster.

##### E7: `call_random_value` (Mutating)

Path: `POST /api/games/{game_id}/calls/random`

Auth scope: Host

The designated host or an authorized admin requests a random undrawn value. Persist the call, matching board changes, qualification, revision and command outcome consistently before delivery.

##### E8: `call_manual_value` (Mutating)

Path: `POST /api/games/{game_id}/calls/manual`

Auth scope: Host

The designated host or an authorized admin submits a string value. Validate pool membership/nonduplication and perform the same durable progression as a random call.

##### E9: `award_game_winner` (Mutating)

Path: `POST /api/games/{game_id}/winner`

Auth scope: Host

For the designated host or an authorized admin, revalidate the selected member’s qualification and commit one winner with Resolved. Retained disconnected/departed players remain eligible.

##### E10: `cancel_unstarted_game` (Mutating)

Path: `POST /api/games/{game_id}/cancel`

Auth scope: Host

With designated-host/admin confirmation, commit Cancelled from New/Awaiting Players, then apply notice and no-History deletion rules. Generate no boards.

##### E11: `end_game_without_winner` (Mutating)

Path: `POST /api/games/{game_id}/cancel`

Auth scope: Host

With designated-host/admin confirmation, commit Cancelled from In Progress, retain final History and safely release the live reservation. The current game state determines this branch rather than the pre-start cancellation branch.

#### F. Final views and History

##### F1: `get_game_result` (Non-mutating)

Path: `GET /api/games/{game_id}/result`

Auth scope: Host, Player

Return only a permitted final view. Players need existing unexpired, not-exited authorization; hosts/admins must satisfy their final-view access rules and use History after exit. Spectators cannot fetch/reconnect after terminal cleanup.

##### F2: `exit_game_result` (Mutating)

Path: `POST /api/games/{game_id}/exit`

Auth scope: Anyone

Requires the caller’s existing final-view authorization when a server-side exit record is needed. Clean up only that caller’s access without changing outcomes, logging accounts out or waiting for others. Spectator local-only Exit requires no request after server data deletion.

##### F3: `list_game_history` (Non-mutating)

Path: `GET /api/history`

Auth scope: Host

List unexpired started-game History across hosts. Exclude pre-start cancellations and expired records. Query options remain TBD.

##### F4: `get_game_history` (Non-mutating)

Path: `GET /api/history/{game_id}`

Auth scope: Host

Read final ordered calls, aliases, winner if any and final board snapshots. No replay archive, credentials, membership restoration or mutation.

#### G. Live synchronization and retries

##### G1: `connect_game_stream` (Mutating)

Path: `GET /api/games/{game_id}/stream` — WebSocket upgrade

Auth scope: Anyone

A valid authorized account or participant session is required, including for spectators. Register/supersede the appropriate connection and send a full role-specific snapshot. This is mutating because connection/presence state changes; existing membership is not duplicated.

##### G2: `send_game_update` (Non-mutating)

Path: Existing `/api/games/{game_id}/stream` socket

Auth scope: System

Push already-committed, role-filtered changes to currently authorized recipients. This is server delivery, not a user-invoked HTTP request or gameplay mutation.

##### G3: `synchronize_game_view` (Non-mutating)

Path: `GET /api/games/{game_id}/sync`

Auth scope: Anyone

Requires valid access to the requested game/view. Compare the client’s authorized-view revision and return fresh state when needed. Do not treat private updates as public gaps, renew credentials or bypass exit restrictions.

##### G4: `get_command_result` (Non-mutating)

Path: `GET /api/games/{game_id}/commands/{command_id}`

Auth scope: Anyone

Requires valid authority for the original actor/game-scoped command. Read its durable outcome after an interrupted response without exposing another actor’s results. Knowledge of a command ID is not authorization.

##### G5: `execute_command_idempotently` (Mutating)

Path: Original command’s method/path; no separate endpoint

Auth scope: System

Internal execution wrapper used after the original command’s own authentication and authorization checks. Return an existing committed result or safely complete the original command without duplicate calls, boards, awards or admissions. The initiating client retains the original endpoint’s scope.

#### H. Backend-only coordination and maintenance

##### H1: `claim_game_code` (Mutating)

Path: — Internal

Auth scope: System

Check a candidate against live mappings and unexpired History, then safely claim/publish it. Reused codes must not transfer old credentials.

##### H2: `coordinate_game_reservations` (Mutating)

Path: — Internal

Auth scope: System

Acquire/release the separate saved-unstarted and In Progress allowances. A stale operation must never release another game’s reservation.

##### H3: `find_hosted_nonterminal_games` (Non-mutating)

Path: — Internal

Auth scope: System

Find nonterminal games by designated-host account ID to enforce account disable/delete safeguards. This does not expose Users to ordinary hosts.

##### H4: `update_participant_presence` (Mutating)

Path: — Internal

Auth scope: System

Maintain connected-player eligibility and spectator-disconnect state. Superseded socket closure must not release the replacement connection’s seat.

##### H5: `expire_spectator_grace` (Mutating)

Path: — Internal

Auth scope: System

Recheck a due five-minute grace deadline and current presence; release expired occupancy and delete associated spectator identity/session data.

##### H6: `expire_or_revoke_credentials` (Mutating)

Path: — Internal

Auth scope: System

Enforce credential expiry/revocation, stop unauthorized socket delivery and clean up artifacts. Expiry alone does not delete retained player membership.

##### H7: `cancel_idle_unstarted_game` (Mutating)

Path: — Internal

Auth scope: System

After rechecking state/deadline, cancel an unstarted game following 24 hours of qualifying-host inactivity. Release only its saved allowance and trigger cleanup.

##### H8: `purge_cancelled_unstarted_game` (Mutating)

Path: — Internal

Auth scope: System

Delete committed pre-start-cancelled game/participant data and mappings without History or waiting for viewer acknowledgement. Preserve only justified retry protection.

##### H9: `finalize_game_retention` (Mutating)

Path: — Internal

Auth scope: System

Preserve started-game final History; delete recovery verifiers and spectator server data; retain only permitted minimal player final-view authorization.

##### H10: `purge_expired_history` (Mutating)

Path: — Internal

Auth scope: System

Enforce the three-calendar-month deadline and delete final records plus related indexes/copies. Prevent expired data from reappearing through restore.

##### H11: `reconcile_pending_operation` (Mutating)

Path: — Internal

Auth scope: System

Reconcile interrupted directory/game/account operations without duplicate effects, invalid publication, incorrect reservation release or resurrected data.

##### H12: `record_admin_action` (Mutating)

Path: — Internal

Auth scope: System

Record the actual admin actor, target, operation and result without secrets. Audit storage, retention and any administrative read interface remain TBD.

## 7. WSS messages, synchronization and client state

### 7.1 Message inventory

Message grouping/names are TBD; rows describe required purposes, not chosen event discriminators.

| Purpose | Confirmed content / constraint | Envelope, fields and encoding |
| --- | --- | --- |
| Initial / recovery snapshot | Full authorized state and revision; no lobby board; player-own versus host/admin board access versus audience-only projection. | TBD |
| Committed live change | Ordered revision-aware permitted updates for lifecycle, calls, boards and qualification; no secret recovery data. | TBD |
| Final result / cancellation notice | Resolved winner or Cancelled no-winner; spectator delivery/deletion and player final-view rules differ. | TBD |
| Synchronization repair | Detect stale/gapped/obsolete views and fetch fresh authorized state; private changes must not create false public gaps. | TBD |
| Connection liveness | Hibernation-compatible heartbeat/auto-response; does not renew session TTL or prove state freshness. | TBD |
| Expired / revoked / forbidden connection | Stop unauthorized delivery; closing/deauthorization and client feedback mechanics TBD. | TBD |

### 7.2 Per-message and transport worksheet

- **Purpose / trigger / originating owner / recipients:** TBD.
- **Event identifier, version, payload schema and sanitized example:** TBD.
- **Per-role field projection and authorized-view revision model:** TBD.
- **Snapshot capture/subscription ordering and missing-update prevention:** TBD.
- **Durable commit boundary versus emission and acknowledgement:** TBD.
- **Duplicate/out-of-order/gap handling and obsolete-socket rejection:** TBD.
- **Participant-session single-live-socket enforcement versus concurrent account logins:** TBD.
- **Durable Object hibernation handlers, socket attachments and restoration:** TBD.
- **Expiry/revocation/grace scheduling without in-memory-only timers:** TBD.
- **Payload/buffer limits, backpressure, heartbeat/check intervals and reconnect jitter:** TBD — preserve the HLD's 30-second reconnect cap.
- **Frontend state transitions, resync gating and private-cache clearing:** TBD.
- **Protocol compatibility, close behavior and tests:** TBD.

## 8. Detailed workflows and domain algorithms

For each row, fill in sequence, participants, records, preconditions, commit points, failure recovery and tests. Do not introduce extra user-visible lifecycle states or treat Object-to-Object work as one SQL transaction.

| Workflow / algorithm | Constraint to preserve | Detailed design |
| --- | --- | --- |
| Enrollment / reset / reissue | Restricted authority, one-day single-use links, fixed session expiry and predecessor invalidation. | TBD |
| Game create / publication | Separate saved/live allowances, fixed config/code, crash-safe directory/game agreement. | TBD |
| Admission / Leave / rename / switch | Capacity and alias ownership remain consistent; explicit Leave differs from loss; failed changes preserve prior membership. | TBD |
| Start and board generation | Connected minimum separate from retained start roster; feasibility before acceptance, unique position-sensitive boards, no reroll on retries. | TBD |
| Free-cell representation / evaluation | Valueless pre-satisfied cells and specified default coordinate; qualification may exist immediately, never automatic award. | TBD |
| Pattern-specific Rust traits | Separate trait per distinct pattern; first-release Single Line only; no signatures chosen. | TBD |
| Random/manual call → match → qualify | Valid undrawn value; persist all affected boards/qualification and command outcome before push. | TBD |
| Award / cancellation / slot release | Exactly one immutable terminal result; no viewer-dependent release or stale release of another game. | TBD |
| Host transfer / admin action | Confirmed immediate transfer versus non-owning admin action; actual actor recorded, authority rechecked on concurrent work. | TBD |
| Player recovery / answer maintenance | Same stable member/board, no new slot or terminal recovery, current-alias lookup and revoked old sessions. | TBD |
| Refresh / reconnect / interrupted command | Snapshot-first convergence; no offline queue, replacement boards, duplicate random draws or heartbeat-only freshness. | TBD |
| Idle cancellation / expiry / final cleanup | Durable deadlines and role-specific data/access deletion; retained History never becomes a credential or replay archive. | TBD |

## 9. Security specification worksheet

| Topic | Confirmed guardrail | Detailed policy / mechanism |
| --- | --- | --- |
| Passwords / secrets | Mandatory setup; password policy explicitly deferred by HLD-063. Store verifiers, never plaintext credentials. | TBD — length/characters, hashing, cost, storage and rotation. |
| Tokens / cookies | Opaque backend-issued one-day credentials; atomic single use and absolute session deadlines, Secure/HttpOnly/SameSite. | TBD — entropy/format, verifier, names/attributes, namespaces and rotation. |
| URL redemption | Enrollment/reset links are bearer credentials; no logging/analytics leakage or accidental consumption assumptions. | TBD — scanners/prefetch, referrer handling, handoff and lost-response recovery. |
| Privileged access | Users and account operations admin-only; first admin via CLI, later admins via CLI/existing admin; no self-elevation. | TBD — server enforcement, confirmation, account-management edge cases and audit. |
| Gameplay authorization | Admin ownership override does not override game rules; ordinary hosts remain ownership-bound. | TBD — action policy, cross-Object trust and commit-time checks. |
| Player recovery | Answers may be guessable; code/alias are not extra secret factors. No alternate proof or privileged anonymous-player takeover. | TBD — normalization, verifier protection, throttling and generic failure behavior. |
| Transport / browser requests | HTTPS/WSS; client inputs and browser cookies alone do not prove safe intent. | TBD — origin/CSRF/CORS policy, validation, response headers and rate limits. |
| Privacy and lifecycle | No secrets in Users/game snapshots/History; terminal access and deletion honor role-specific rules. | TBD — projection, cache/log redaction, expiry/cleanup and incident handling. |
| Infrastructure / CLI | Cloud credentials only in controlled developer/server environments; environment delivery alone is not authorization. | TBD — secret names/scopes, bindings, operator controls and deployment access. |

## 10. Verification plan

No application tests have been implemented or run by this template. Exact test files, tools, fixtures, acceptance thresholds and evidence are **TBD**.

| Test area | Scenarios to specify | Cases / expected result / evidence |
| --- | --- | --- |
| Domain and schema | Pool/string rules, every supported board size, free cells, distinct-board feasibility, Single Line, unique calls and data constraints. | TBD |
| Views and permissions | Host versus admin controls, Users frontend/backend denial, private-board projection, missing/expired sessions and restricted enrollment. | TBD |
| Worker routing and Object isolation | Multiple API handlers in the single Worker reach the correct authoritative owner; unknown codes do not create games, and Object IDs/bindings never replace user authorization. | TBD |
| Account lifecycle | Concurrent redemption, reissue/reset revocation, first-admin creation, non-owner overrides, removal blocked by hosted nonterminal game. | TBD |
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
| Single API Worker entrypoint/module routing, Object classes, bindings and deployment configuration | TBD — preserve LLD-016; exact names/configuration and Object structures remain user-supplied. |
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

Detailed specifications in LLD-001–LLD-015 remain pending; LLD-006 references the newly captured proposals. LLD-016 records the confirmed Worker organization, and LLD-017 records the requested catalog/format and System scope. Neither finalizes request/response schemas or Durable Object structures. Completing an entry records a design decision, not implementation evidence.

| ID | Decision / specification | Status / selected detail |
| --- | --- | --- |
| LLD-001 | Dioxus view grouping, routes, components, state management and accessibility | TBD |
| LLD-002 | Users fields/actions/filters and protected account-management UX | TBD |
| LLD-003 | Cloudflare account/session store placement and ownership | TBD |
| LLD-004 | SQLite tables/types/keys/indexes, record mappings and migrations | TBD |
| LLD-005 | Directory/game coordination and command idempotency/reconciliation | TBD |
| LLD-006 | Worker API methods/routes, request/response/error schemas and compatibility | Proposed functions, methods/paths and scopes recorded in Section 6.4 under LLD-017; final endpoint contracts, parameter/payload/error schemas, signatures and compatibility remain TBD. |
| LLD-007 | WSS payloads/revisions, privacy projections, hibernation and backpressure | TBD |
| LLD-008 | Board generation/feasibility, matching and pattern-specific trait signatures | TBD |
| LLD-009 | Password policy, token/cookie/verifier implementation and request security | TBD |
| LLD-010 | Answer normalization/protection, abuse controls and recovery transactions | TBD |
| LLD-011 | Presence/Leave/Exit mechanics, timer scheduling and terminal notice/deletion ordering | TBD |
| LLD-012 | History placement/indexing, calendar-month expiry, cleanup and backup/restore | TBD |
| LLD-013 | CLI interfaces, binding/credential scopes, secure link handoff and bootstrap | TBD |
| LLD-014 | Optional speech, browser matrix, quality/load targets and acceptance tests | TBD |
| LLD-015 | Build/deployment/observability and quota verification | TBD |
| LLD-016 | Initial API Worker organization and Durable Object boundary | Confirmed user direction — one API Worker serves multiple endpoints through organized Rust handlers and bindings to the existing directory/per-game state owners. No endpoint-per-Worker split. Specific APIs, Object structures, schemas, router/modules, bindings and internal protocols remain TBD for the user. See Sections 3.1–3.2, 5–6 and 11. |
| LLD-017 | Proposed operation catalog and auth-scope notation | Captured at user request — Section 6.4 retains category headers and per-operation ID/function/access-type heading, Path, Auth scope and description. Scope labels are Admin, Host, Player, Anyone and the user-selected System for internal work. Proposed names/paths remain reviewable; Anyone never bypasses required session/proof/ownership, Host includes admin with applicable game guards, and System is not an account role or public endpoint. Request/response schemas, function signatures and Object structures remain TBD. |

### 12.2 Product/source questions — do not silently decide in implementation

- **Admin actions and host-idle timer:** HLD-068 predates admin override. Clarify whether a non-designated admin's action/open/resume refreshes the saved game's host-idle deadline; do not select new behavior here. **TBD**.
- **Privileged account-management edge cases:** Self-removal, last-admin protection and any account type-editing feature are not defined by account creation permission. No role-editing feature is introduced by this template. Clarification if needed: **TBD**.
- **UI/speech/quality requirements:** Exact layouts, spoken-announcement behavior, accessibility/compatibility criteria and measurable quality targets still need decisions. **TBD**.
- **Source reconciliation and HLD approval:** Resolve stale requirements/HLD wording using explicit later decisions, then obtain required stage approval separately. **TBD**.

### 12.3 Traceability index

Coverage means a place to complete the design, not that every design choice or requirement is fulfilled. Ranges identify source decision rows; superseded rows are carried only as qualified in Section 1.2.

**Subsequent user direction:** LLD-016 records the single-Worker/multiple-endpoint approach and Worker/endpoint/Object distinction. LLD-017 captures the requested A1–H12 proposed operation catalog and scopes in Section 6.4, mapped from Section 6.2. These scoped LLD updates do not rewrite the HLD or complete detailed API/storage design.

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
- [ ] Cloudflare physical ownership and all schema/constraint/index/migration worksheets are filled in.
- [ ] Every logical API operation has explicit request, success, error, authorization and retry contracts.
- [ ] WSS schemas, role projections, snapshot ordering and expiry/reconnect mechanisms are specified.
- [ ] Domain algorithms and distinct pattern traits preserve confirmed rules without inventing features.
- [ ] Cross-Object transactions/retries and all retention/deletion paths have failure recovery designs.
- [ ] Host/admin/Users permissions, privileged creation and secret boundaries are reviewed end to end.
- [ ] Source conflicts and genuinely new product questions are resolved or explicitly deferred.
- [ ] Tests, quality targets, Cloudflare compatibility/budget checks and operational runbooks are defined.
- [ ] Detailed design approval is recorded; implementation receives its own explicit authorization.
