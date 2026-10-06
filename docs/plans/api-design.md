# Brews Bingo — API Design

## 1. Document status and navigation

- **Status:** API design companion to [the LLD](lld.md). Moved from the LLD at the user's request (LLD-022), preserving the proposed API-01–API-32 coverage index and A1–H12 operation catalog, including Request/Response subsections. Request/Response body fields now use **Field / Type / Required / Description** tables; other request/response information appears in preceding paragraphs. Moving or reformatting the content does not approve unfinished contracts or authorize implementation.
- **Ownership:** This document is the single home for Worker/API organization, methods/paths, auth-scope notation, DTO/request/response proposals, contract worksheets and WSS protocol design. The [Durable Object design](durable-object-design.md) retains storage ownership, records/fields/types and schema worksheets. The LLD retains cross-cutting domain rules, implementation conventions, verification/deployment planning and the decision register.
- **Source precedence:** Follow [LLD source precedence](lld.md#12-source-precedence-and-reconciliation), [HLD](hld.md) and later explicit decisions recorded in the [LLD register](lld.md#121-detailed-decisions-for-the-user-to-fill). Requirements/research remain in [requirements.md](requirements.md) and [research.md](research.md); stale source wording is not silently rewritten.
- **Confirmed conventions:** Application-generated system IDs use UUID v7 by default ([LLD Section 3.3](lld.md#identifier-policy), LLD-020). Component-local Rust errors use `thiserror` in adjacent `error.rs` files ([LLD Section 3.4](lld.md#error-conventions), LLD-021). Account passwords must be at least 10 characters using any combination of ASCII characters, with no required character-class mix ([LLD Section 9.1](lld.md#password-policy), LLD-023). Salted Argon2 via RustCrypto `argon2` is confirmed by [LLD Section 9.2](lld.md#password-hashing), LLD-024; final profile and runtime details remain **TBD**. These confirmed constraints do not approve every proposed wire field.
- **TBD:** Final contract approval, requiredness/defaults, encodings, response/error/status schemas, signatures, routing, compatibility, security mechanisms and executed contract tests. Scope/role/lifecycle rules already confirmed by the HLD are not reopened by these placeholders.

Navigation:
- [Worker organization and boundaries](#worker-boundaries)
- [Shared contract worksheet](#shared-contracts)
- [Logical operation inventory](#operation-index)
- [Per-operation worksheet](#operation-worksheet)
- [Operation catalog and Request/Response proposals](#operation-catalog)
- [WSS messages and synchronization](#wss-design)

Section numbers below are local to this document. References prefixed **LLD** refer to `lld.md`; `LLD-*`, `HLD-*`, `BR-*`, `API-*` and A1–H12 IDs retain their existing meanings. Logical internal operations remain internal; this move introduces no public routes.

<a id="worker-boundaries"></a>

## 2. Worker organization and state-owner boundaries

### 2.1 One API Worker, multiple endpoints

**Confirmed user direction (LLD-016):** Start with **one API Worker**, not one Worker per API endpoint. Incoming HTTP requests enter the Worker's fetch entrypoint and are dispatched by method/path to appropriate handlers. Organize handlers by responsibility in Rust modules; one deployment does not require one large function or one source file. Router choice, module names and signatures remain **TBD**.

The same API Worker covers the approved authentication, Users/account management, game operations, participant admission/recovery, History and WebSocket-upgrade responsibilities. These are responsibility groups, not a finalized endpoint list. Section 6 records the proposed operation/function/path/scope catalog and potential Request/Response fields; final schemas and detailed API contracts remain **TBD** for user review.

The single-Worker decision does not settle account-store placement or require every backend concern to live in a Game Object. [Durable Object design Section 6](durable-object-design.md#schema-proposal) separately captures a proposed Accounts Object; acceptance remains **TBD**.

### 2.2 Worker, endpoint and Durable Object boundaries

| Concept | Responsibility in this design |
| --- | --- |
| API Worker | Deployable backend entry service that receives requests, enforces access checks and dispatches to the appropriate handler/owner. |
| API endpoint | An externally exposed operation identified by its eventual method/path; many endpoints share the API Worker. Exact endpoint definitions remain TBD. |
| Durable Object class / namespace | Stateful behavior and its collection of instances, accessed through configured backend namespace bindings. Proposed class names appear in [Durable Object design Section 6.1](durable-object-design.md#object-boundaries); final classes, binding names and interfaces remain **TBD**. |
| Durable Object instance | A stable state owner selected within a namespace. The existing HLD calls for one Game Object per game, not one per endpoint or browser. Proposed logical structures appear in [Durable Object design Section 6](durable-object-design.md#schema-proposal); final structures and physical schemas remain **TBD**. |

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

<a id="shared-contracts"></a>

## 3. Shared contract worksheet

| API concern | Confirmed input | Implementation specification |
| --- | --- | --- |
| Transport and routing | One API Worker serves multiple HTTPS endpoints and WSS upgrades, dispatching to handlers and authoritative backend owners (LLD-016). | Proposed methods/paths in Section 6; final contracts, base URL/versioning, Rust router/handler signatures and internal calls remain TBD. |
| Authentication | Backend-issued `__Host-brews_session` cookie with `Secure; HttpOnly; Path=/; SameSite=Lax`, no `Domain`, HTTPS-only; fixed one-day expiry and restricted versus normal scopes (DO-031). Account links/sessions must match current `credential_epoch` (DO-022) plus scope/purpose, expiry, revocation, lifecycle and `disabled_at`. DO-028 approves atomic link consumption/session creation; DO-030 approves session-row binding; DO-032/033 approve enrollment rotation and post-reset fresh login. DO-034 requires current authoritative account/session validation for each protected request and disallows an unrevalidated positive authorization cache across requests. DO-035 requires WSS subscription registration against current enabled Verified/Normal session authority in one AccountsObject transaction, with registration-first/revocation-first ordering and fail-closed socket handling. DO-036 requires durable same-transaction revocation work and per-outgoing-frame authority revalidation; DO-037 requires expiry sweeps to enqueue idempotent closes and retain subscription targets through close/absence acknowledgement before deletion. Do not report all sockets closed before target acknowledgements. | SQL/outbox fields, sweep cadence/scheduling, indexes, exact-Origin implementation/error mapping and remaining wire details. |
| Authorization | Server-owned role plus game assignment; admin overrides ownership only. Users/account-list data is admin-only. | TBD — middleware, checks and commit-time reauthorization. |
| Request validation | Preserve string values, alias outer-space trimming/case-sensitive comparison, code normalization, lifecycle and capacity rules. | Potential operation inputs in Section 6; final media types, encodings, requiredness, validation/unknown-field policy and limits **TBD**. |
| Response contract | Return authorized state/results only; secret fields absent from normal views and logs; privileged link issuance uses a separate protected handoff. DO-028 requires canonical-origin links, no-store handoff and token-redacted logs/traces. | Potential safe projections/outputs in Section 6; final envelopes, fields/encodings, status codes and remaining cache/transport details **TBD**. |
| Retry safety | Actor/game-scoped command identity and durable outcome lookup; repeat accepted command without repeating effects. | TBD — idempotency transport, conflict behavior, result retention and retry policy. |
| Errors | Component-owned errors use `thiserror` in local `error.rs` files per [LLD Section 3.4](lld.md#error-conventions). Deliberately map them to safe transport errors; never expose raw source chains. | **TBD** — public codes/statuses, response schemas, messages, WSS close behavior, retry classification and mapping tests. |
| Compatibility / observability | No selected wire schema or API version; DO-028/031 forbid raw access-link/session tokens in logs/traces. | TBD — version policy, correlation/audit fields and remaining logs/traces. |
| System identifier contract | Application-generated `*Id` types listed in [Durable Object design Section 6.2](durable-object-design.md#shared-types) use UUID v7 under LLD-020. IDs are not bearer credentials; validate their version without changing identity or granting authority. | **TBD** — canonical wire spelling/encoding, parse-error mapping and exact generator/issuer ownership for retry IDs. Reuse an original command ID on retries. |

<a id="operation-index"></a>

## 4. Logical operation inventory

These stable API-01–API-33 rows remain a high-level coverage checklist, not finalized endpoints or separate Worker deployments. The catalog-reference column maps them to the expanded A1–H12 entries in Section 6, including associated internal work. That section records proposed functions, methods/paths, auth scopes and per-operation Request/Response subsections with potential fields/types and cookie effects. Each eventual API still needs finalization through Section 5: **requiredness/defaults, exact encoding, complete success/error schemas, status codes and protocol details remain TBD**. Internal entries do not acquire public routes through this mapping. Final endpoint grouping remains subject to user review within the single-API-Worker boundary (LLD-016).

| ID | Logical operation | Confirmed actor / guard / outcome | Proposed catalog entries (Section 6) | Request / responses |
| --- | --- | --- | --- | --- |
| API-01 | Account password login | Host/admin credentials after completed setup; issue fixed-life account session. | A1 | Proposed Request/Response: A1 in Section 6; final contract **TBD**. |
| API-02 | Enrollment-link redemption / password setup | Valid unused link → restricted session → personal password; no early privileges or deadline extension. | A3, A4 | Proposed Request/Response: A3, A4 in Section 6; final contract **TBD**. |
| API-03 | Reset-link redemption / new password | Valid single-use reset proof; restricted reset authority, same account. | A5, A6 | Proposed Request/Response: A5, A6 in Section 6; final contract **TBD**. |
| API-04 | Session validation / logout | Resolve server-owned authority or revoke current access; game Exit is a separate concept. | A2, A7, H6 | Proposed Request/Response: A2, A7, H6 in Section 6; final contract **TBD**. |
| API-05 | Users account listing | Enrolled admin only; list host/admin accounts without secrets. | B1, B2 | Proposed Request/Response: B1, B2 in Section 6; final contract **TBD**. |
| API-06 | Provision host/admin account | Developer CLI or existing admin; first admin via CLI. Persist pending account/link before returning private URL. | B3, B4 | Proposed Request/Response: B3, B4 in Section 6; final contract **TBD**. |
| API-07 | Reissue enrollment link | Privileged caller, same pending account; invalidate prior links/restricted sessions. | B5 | Proposed Request/Response: B5 in Section 6; final contract **TBD**. |
| API-08 | Initiate account password reset | Privileged caller; issue one-day single-use link, revoke target sessions/sockets immediately and block old-password login while pending. | B6 | Proposed Request/Response: B6 in Section 6; final contract **TBD**. |
| API-09 | Disable / delete account | Privileged caller; acquire Directory gate before host-game check; reject if target hosts a nonterminal game, otherwise remove access without deleting unrelated History. | B7, B8, H3 | Proposed Request/Response: B7, B8, H3 in Section 6; final contract **TBD**. |
| API-10 | List / inspect games | Host/admin authorized cross-game view; read permission never grants host-role mutation. C2 is the single current nonterminal-game lookup. | C1, C2, C5 | Proposed Request/Response: C1, C2, C5 in Section 6; final contract **TBD**. |
| API-11 | Create New game | Host/admin creator becomes designated host; reject while any other nonterminal game holds the single global slot (HLD-077). | C6, H2 | Proposed Request/Response: C6, H2 in Section 6; final contract **TBD**. |
| API-12 | Read / update configuration | Reads authorized; changes require designated host/admin and New. | C5, C7 | Proposed Request/Response: C5, C7 in Section 6; final contract **TBD**. |
| API-13 | Publish Awaiting Players | Authorized operator; validate/fix configuration and durably issue code. | C8, H1 | Proposed Request/Response: C8, H1 in Section 6; final contract **TBD**. |
| API-14 | Open / resume selected game | Restore existing identity/configuration/code/state; role/lifecycle/exit restrictions apply. | C9 | Proposed Request/Response: C9 in Section 6; final contract **TBD**. |
| API-15 | Start game | Designated host/admin, Awaiting Players, connected minimum, this game still owns the same global reservation, feasibility checks; persist start-time boards. | E6 | Proposed Request/Response: E6 in Section 6; final contract **TBD**. |
| API-16 | Random / manual value call | Designated host/admin, playable In Progress; valid undrawn value → atomic calls/matches/qualification result. | E7, E8 | Proposed Request/Response: E7, E8 in Section 6; final contract **TBD**. |
| API-17 | Submit one qualified winner | Designated host/admin validates same-game qualification; commit Resolved once. | E9, H9 | Proposed Request/Response: E9, H9 in Section 6; final contract **TBD**. |
| API-18 | Cancel / manually end game | Designated host/admin with required confirmation; commit Cancelled, apply pre-start versus started retention. | E10, E11, H7, H8 | Proposed Request/Response: E10, E11, H7, H8 in Section 6; final contract **TBD**. |
| API-19 | Transfer designated host | Current designated host or admin confirms a distinct eligible Host target; require expected host-assignment revision, immediate committed transfer, no recipient acceptance or data reset. | C10, H12 | Proposed Request/Response: C10, H12 in Section 6; final contract **TBD**. |
| API-20 | Query command outcome | Authorized original command context; resolve lost acknowledgement without another effect. | G4, G5, H11 | Proposed Request/Response: G4, G5, H11 in Section 6; final contract **TBD**. |
| API-21 | Resolve game code / entry eligibility | Known published code only; discovery is not identity or admission. | C4, D3 | Proposed Request/Response: C4, D3 in Section 6; final contract **TBD**. |
| API-22 | Join as player | Awaiting Players, valid available alias/free slot, optional answer; no board before start. | D4, D5 | Proposed Request/Response: D4, D5 in Section 6; final contract **TBD**. |
| API-23 | Join as spectator | Awaiting Players/In Progress and spectator capacity; no player privileges. | D6, H5 | Proposed Request/Response: D6, H5 in Section 6; final contract **TBD**. |
| API-24 | Rename own alias | Valid player session, Awaiting Players; valid available case-sensitive alias after outer-space trimming, stable membership and unchanged session expiry. | D7 | Proposed Request/Response: D7 in Section 6; final contract **TBD**. |
| API-25 | Switch participant role | Awaiting Players and target eligibility; consistent seat/session/alias/verifier transition, no second admission. | D8, D9 | Proposed Request/Response: D8, D9 in Section 6; final contract **TBD**. |
| API-26 | Participant Leave | Valid owner; pre-start player versus In Progress player versus spectator policies differ. | D10, D11, D12 | Proposed Request/Response: D10, D11, D12 in Section 6; final contract **TBD**. |
| API-27 | Recover player session | Code + current alias + enrolled answer; nonterminal restoration of same membership, replace old sessions/sockets. | D13 | Proposed Request/Response: D13 in Section 6; final contract **TBD**. |
| API-28 | Set / replace / delete own recovery answer | Valid player session, Awaiting Players/In Progress; no alternate ownership proof. | D14 | Proposed Request/Response: D14 in Section 6; final contract **TBD**. |
| API-29 | Final-view Exit / access cleanup | Independent exit, no outcome mutation or renewed game access; spectator local-only exit and pre-start deletion must not require a surviving server session. | F1, F2 | Proposed Request/Response: F1, F2 in Section 6; final contract **TBD**. |
| API-30 | History listing / detail | Host/admin, unexpired immutable final data only; no export or membership restoration. | F3, F4, H10 | Proposed Request/Response: F3, F4, H10 in Section 6; final contract **TBD**. |
| API-31 | Authorized revision check / resynchronization | Repair freshness without treating heartbeat as state agreement; respect role/exit/expiry. | D1, D2, E1, E2, E3, E4, E5, G3 | Proposed Request/Response: D1, D2, E1, E2, E3, E4, E5, G3 in Section 6; final contract **TBD**. |
| API-32 | WSS connection / upgrade | Valid authorized game context; first application state is full role-filtered snapshot; Section 7 details TBD. | G1, G2, H4 | Proposed Request/Response: G1, G2, H4 in Section 6; final contract **TBD**. |
| API-33 | Enable disabled account | Another enrolled admin account or developer CLI only; no self-enable or role escalation. Clear `disabled_at`, preserving lifecycle status and prior credential revocations; DO-022 epoch policy approved, while transaction/concurrency/delivery mechanisms remain TBD (HLD-078). | B9 | Proposed Request/Response: B9 in Section 6; final contract **TBD**. |

No endpoints for manual board marking, public privileged registration, self-elevation, post-terminal gameplay, History export or alternate player recovery are implied. CLI-to-backend transport remains TBD; the catalog does not require the CLI to use public app routes.

<a id="operation-worksheet"></a>

## 5. Per-operation request/response worksheet

- **Operation ID / purpose / source decisions / consuming views:** TBD — link the relevant Section 4 API ID and Section 6 catalog ID(s).
- **Public Worker handler versus internal Object/CLI interface:** TBD — preserve each Section 6 entry’s boundary; internal Request/Response proposals do not create public routes.
- **Method / path / API version:** TBD — review the Section 6 proposal; internal helpers do not require an HTTP endpoint.
- **Authentication / allowed roles / lifecycle / ownership or admin override:** TBD.
- **Preconditions / validation / CSRF and origin checks:** TBD.

### Request

Describe path/query parameters, authentication cookies/headers, origin checks, preconditions, validation and retry metadata here, **before** the body table. Include only request-body fields in the table. State “No request body” when applicable; do not invent fields to fill an empty table. Field definitions and sanitized examples remain **TBD**. Recovery answers belong only in protected HTTPS bodies, never query strings or WSS; never include real passwords, tokens or credential-bearing URLs in examples.

Use `Y` or `N` for each proposed field's requiredness, following the Section 6 conventions. The empty table is a worksheet to populate, not a selected empty payload.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

### Response

Describe success status, headers/cookie effects, visibility/cache policy, durable-result/revision semantics and any body/no-body alternatives here, **before** the body table. Specify restricted versus normal session effects; never expose HttpOnly session bearer values in the body. Refine the Section 6 safe projections; schema, sanitized examples and exact status remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

### Error responses and side effects

Failure conditions, status/error-code mapping, durable effects and retry safety remain **TBD**. The body fields below are shared proposals, not finalized error contracts; optional nonsecret correlation data has no selected field name yet. Describe operation-specific errors and cookie/header effects in paragraphs before its response table rather than mixing transport details into body-field rows. Never return raw internal errors or credential material.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `code` | `String` | Y | Proposed safe public error code; vocabulary and envelope TBD. |
| `message` | `String` | Y | Proposed safe client-facing message; disclosure and mapping TBD. |

- **Backend owner / record reads / writes / transaction:** TBD.
- **Idempotency, concurrent requests and unknown-outcome recovery:** TBD.
- **Committed-result acknowledgement and WSS interaction:** TBD.
- **Revocation/expiry during execution, logging/redaction and audit:** TBD.
- **Contract, permission, validation and failure tests:** TBD.

<a id="operation-catalog"></a>

## 6. Proposed operation and data-access catalog

**Status:** Captured at the user's request. IDs A1–H12, Rust function names, methods/paths and auth scopes remain **proposals for review** (LLD-017), not implemented handlers. Every operation now includes proposed **Request** and **Response** inputs/outputs (LLD-019), including internal and socket-specific semantics. Section 4 remains the stable coverage index. Final requiredness, wire schemas, status/error codes, Rust signatures and routing remain **TBD**; Object structures are separately proposed in [Durable Object design Section 6](durable-object-design.md#schema-proposal) and are not finalized here.

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

**Request/Response proposal conventions (LLD-019):** Every catalog entry below now has **Request** and **Response** subheadings. These are potential inputs/outputs, not approved wire contracts. Public HTTP operations place body fields in **Field / Type / Required / Description** tables and path/query/cookie/header inputs, authorization, failures and side effects in paragraphs before those tables. For System entries, “Request” means trusted internal inputs and “Response” means an internal result or outbound socket message; it does **not** introduce a public endpoint. G1 describes an upgrade plus frames, G2 server push, and G5 a wrapper around the original command.

- **Table notation:** `Field` is the body field name, `Type` is its proposed data type, and `Required` contains only `Y` or `N`. `Y` means required in the described proposed payload; `N` means optional or conditional, with the condition stated in `Description` or the preceding paragraphs. Conditional requirements still apply when their branch is selected; `N` does not permit ignoring a selected branch's required data. These are proposed requiredness flags, not final contract approval. A header-only table has no specified fields: preceding prose distinguishes no body from a schema still TBD. `$` denotes the entire body/result when no wrapping property was proposed; it is not a literal JSON key. Dotted paths denote nested fields rather than new top-level properties. Tables for internal operations describe trusted argument/result payloads; WSS tables explicitly distinguish application messages from the bodyless HTTP upgrade.
- Field names/DTO shapes below remain proposals; application-generated system ID types now follow the confirmed UUID v7 convention (LLD-020). Shared ID, timestamp, enum and configuration notation follows [Durable Object design Section 6](durable-object-design.md#schema-proposal); it does not select Rust serialization or JSON encodings. `object` means a described, not-yet-finalized projection. `Option<T>` indicates conditional/nullable data, not whether a JSON key must be omitted or present as null; that distinction is **TBD**.
- **TBD for every entry unless separately approved:** final input requiredness/defaults, validation limits, field encodings, media/API versions, request/response envelopes, success/error statuses, remaining header names, exact error variants, general caching policy, rate limits and contract tests. DO-028 approves no-store and safe generic invalid-link handling; DO-031 approves the account session cookie and exact-Origin requirement. Local notes highlight additional operation-specific TBDs. Existing confirmed guards still apply; a tentative field must not change a product rule.
- The cookie is browser-supplied credential transport, not a body field or proof that the frontend is trusted. Derive actor, role, stable membership, assignment and expiry server-side; do not accept client-provided authority claims. For browser-cookie-authenticated state-changing HTTPS requests and WSS upgrades, require the exact configured application `Origin` and reject absent/mismatched values (DO-031); `SameSite` alone is insufficient. For an account WSS upgrade, register the unique `connection_id` against the current enabled Verified/Normal session in AccountsObject before sending an authorized snapshot (DO-035); close the socket if registration fails. Under DO-036, revalidate session/epoch before each account-authorized outgoing frame and suppress/close on failed or unavailable authority; close-command completion requires target acknowledgement. A frame already authorized/in flight at revocation commit may race. Other CSRF/CORS mechanics and error mapping remain **TBD**.
- Proposed `command_id: CommandId` metadata supports retry-sensitive mutations. DO-076–080 approve owner/actor-scoped receipts, canonical SHA-256 fingerprinting, typed secret-free outcomes ≤4 KiB, 24-hour ordinary receipt retention (DO-029 link receipts retain 30 days), old-command rejection and credential-secret non-replay. Accepted calls use DO-065 sequence/receipt policy. Endpoint-specific command-ID requiredness/placement, wire envelope and error mapping remain **TBD**. Commit-time authority checks remain mandatory; client-visible revisions belong only to authorized projections.
- Passwords, bearer-link redemption tokens and recovery answers appear only as conceptual input fields, never sample values. Under DO-028, the access-link token is delivered only in a canonical HTTPS URL fragment, cleared with `history.replaceState`, and then sent only in a protected HTTPS body—never a request path/query, cookie, log or WSS frame. Do not expose session bearer values, stored verifiers or private recovery material in response bodies. Session issuance/rotation occurs through protected HttpOnly cookies.
- **Sensitive-output exception:** authorized account/link creation, reissue and reset may deliver the newly issued enrollment/reset URL to the authorized administrator/developer. Those URLs are bearer secrets, not ordinary account metadata, list/detail fields, logs or secret-free receipt payloads. No real URL/token appears here. DO-028 approves canonical-origin fragment links, `Cache-Control: no-store`, and redacted logs/traces. Under DO-029, a repeated link-issuance request with the same `command_id` returns only its secret-free 30-day receipt, never the URL/token; an undelivered URL requires an explicit new reissue command, which invalidates predecessors. A committed-but-lost redemption response requires a fresh purpose-appropriate link while setup/reset remains pending; completed setup/reset uses login or the admin reset flow, not credential rollback.
- Failure candidates are design prompts, not promises to disclose every cause. Use safe authentication/recovery errors and do not reveal another actor's identity, answer-enrollment status or private records. DO-028 requires one safe generic invalid-link result across access-link validation failures. Potential shared error fields are `code: String`, safe `message: String` and optional nonsecret correlation reference; final envelope/status/retry hints for other errors are **TBD**. An interrupted response is not proof the command failed or is safe to resubmit with a new ID.
- Response proposals describe committed success unless explicitly labeled pending. Do not report deletion, reservation release or immediate revocation complete merely because cross-Object work was queued. Exact pending/unknown-outcome contracts remain **TBD**. Neither success bodies nor cached receipts may outlive the caller's access or the data-retention boundary.

**Derived state is not stored twice:** Boolean response properties that correspond to authoritative timestamps are computed projections, not extra database columns. In particular, account `disabled` is derived only from `disabled_at` population; enable/disable writes that timestamp, not a client-provided flag. Derive expiry/blocking from deadline comparisons rather than timestamp presence where appropriate. See [the cross-record storage rule](durable-object-design.md#timestamp-state). This does not add response fields or finalize their visibility.

### Game creation/Start implementation contract decisions

These decisions finalize only this local implementation slice; unrelated catalog contracts remain proposals/TBD. The 107 approved Durable Object rules remain unchanged. This slice is implemented and independently reviewed locally; its execution gates pass. The user has subsequently authorized commit/push; production deployment remains unauthorized.

**Local verification:** 563 workspace tests pass in each native profile, 12 doctests pass, and all 127 fresh-built Worker tests pass, including 42 game cases. A separate local HTTPS/WSS check passes 30 assertions with two real connected players and three retained boards. Native/Wasm strict Clippy and `cargo fmt --all`/`--check` pass. Independent runtime review also executes all 127 Worker tests and three additional ingress probes successfully. Source hashes match the reviewed code. Production KDF/capacity/provider measurements remain release gates; this is not production approval.

| Decision | Status | Approved contract / scope |
| --- | --- | --- |
| GAME-SLICE-01 | Approved | C6 accepts an optional configuration object. Omitted settings use approved defaults; supplied settings undergo approved validation. C7 configuration updates are deferred from this slice. |
| GAME-SLICE-02 | Approved | C6/C8/E6 use their proposed POST paths, the existing account cookie, exact Origin and UUID-v7 `Idempotency-Key`. C8/E6 require `expected_revision` in the JSON body. Fresh success is HTTP 200 with documented safe fields plus a secret-free receipt; exact retries return only the receipt and never regenerate boards. Unfinished coordination is HTTP 202 with an operation ID and no unpublished code. Reuse existing no-store JSON/error envelopes; lifecycle/reservation/revision/insufficient-player conflicts are 409 and unavailable dependencies are 503. |
| GAME-SLICE-03 | Approved | Implement C6/C8/E6 plus the minimum D5/G1/G2/G3 admission/session/presence/synchronization foundation so Start can succeed through public APIs. Include required Directory/Game/Accounts coordination and pre-start timeout cleanup. Exclude frontend screens, number calls, winner awards, deployment and another Git push. |
| GAME-SLICE-04 | Approved | A code-validated `POST /api/games/{game_id}/admission-context` preflight establishes a 15-minute game-scoped HttpOnly admission cookie without taking a seat. D5 requires that cookie, exact Origin, UUID-v7 `Idempotency-Key` and JSON `game_code`, `alias`, optional `recovery_answer`. Bind commands to the server-reserved player identity. Fresh 200 returns player ID/alias, fixed one-day expiry, view revision and secret-free receipt, and sets a separate Secure/HttpOnly/SameSite=Strict game-scoped player cookie. Exact retries return only the receipt, never a cookie. Existing valid players reconnect without another seat. Enroll approved normalized Argon2 answer verifiers, but defer the recovery endpoint; lost player-cookie delivery cannot be restored from a receipt. |
| GAME-SLICE-05 | Approved | G1 uses its proposed upgrade path, valid cookies and exact Origin. Version-1 JSON snapshot frames carry `kind`, `game_id`, `view_revision`, `connection_id`, `session_expires_at` and role-filtered `view`; committed updates use full replacement snapshots. Player views expose only the caller's board, host/admin views follow DO-067. When account and player cookies coexist, require nonsecret `view=account\|player` and validate the selected authority. G3 accepts that selector and optional `known_revision`, returning `up_to_date`, `view_revision`, `snapshot` (null only when unchanged). No URL credentials, credential renewal, gameplay-command frames or chunking. Keep approved frame/queue caps and close/resync behavior; close codes are 1008 denied authority, 1009 oversized frame and 1013 unavailable/overloaded service. |
| GAME-SLICE-06 | Approved | Actual local Workerd exposes no `bufferedAmount` queued-byte counter. Extend each version-1 snapshot with an unpredictable server-generated `delivery_id` at the end; accept transport-only `snapshot_ack` with connection ID, delivery ID and view revision. Validate current session/connection and known outstanding delivery before freeing the durable byte budget. Outstanding-delivery metadata survives hibernation; exceeding 1 MiB or a bounded backlog closes/resyncs. ACKs never authorize gameplay, renew credentials or determine whether Start committed. The user approved continuing the full authorized creation/Start slice. |

**Reusable proposed safe response shapes:** These are DTO/projection descriptions, **not direct serialization of the storage records in [Durable Object design Section 6](durable-object-design.md#schema-proposal)**. Final nested schemas, field visibility and optionality remain **TBD**.

| Shape | Potential fields / visibility |
| --- | --- |
| `SafeAccount` | `account_id: AccountId`, `username: String`, `role: AccountRole`, `status: AccountStatus`, `disabled: bool` (computed from `disabled_at.is_some()`, not stored), `created_at: Timestamp`; optional safe lifecycle timestamps only where authorized. Self/setup flows receive only their required subset; admin Users flows may receive the management subset. No verifiers, epochs, tokens, links or participant records. |
| `SessionView` | `kind: String`, `scope: String`, `expires_at: Timestamp`; identity variant containing only the authenticated account's safe ID/role or the caller's game/member binding, with player alias where applicable. Exact discriminators **TBD**. No bearer, verifier or arbitrary other identity. |
| `GameSummary` | `game_id: GameId`, `game_code: Option<GameCode>`, `state: GameState`, permitted designated-host ID, relevant timestamps and server-derived caller capabilities. No private roster/board/credential data. Public code entry uses a narrower projection rather than this account summary. |
| `MembershipView` | `game_id: GameId`, participant role, own `player_id: PlayerId` or `spectator_id: SpectatorId`, player alias when applicable, permitted access state, `session_expires_at: Timestamp`, player-only `recovery_enabled: bool`. No recovery answer/verifier; a spectator does not acquire player fields. |
| `CallView` | `sequence_no: u32`, `value: String`; optionally a public-safe `called_at: Timestamp` (**TBD**). Do not implicitly expose raw call actor IDs, command IDs or storage metadata. |
| `BoardView` | Own or otherwise authorized `player_id: PlayerId`/alias, `side_length: u8`, `cells: Vec<BoardCell>` (position, Free/Value, automatic matched state), `qualified: bool`, `qualifying_lines: Vec<CompletedLine>`. No client mark input or pre-start assigned board. |
| `GameView` | Authorized `GameSummary` subset, permitted `configuration: GameConfiguration` fields, `calls: Vec<CallView>`, latest call/exhaustion, `view_revision: Revision`; own `MembershipView`/board for a player, permitted roster/boards for hosts/admins, audience-only state for spectators. Fields absent by role/state remain absent, not blank private copies. DO-067 scopes revisions to Host, Player(PlayerId) or Spectator(SpectatorId); DO-068 defines advancement, snapshot/update ordering and gap resync. Account/session grants remain separate, and one player’s private change does not advance another player’s revision. Exact projection fields remain TBD. | |
| `FinalResultView` | `game_id: GameId`, terminal outcome, `winner: Option<WinnerSnapshot>`, `ended_at: Timestamp`, final call/board content only as authorized, original expiry metadata where relevant, and applicable view revision. Player gets only own board; account final-view grants follow their permissions; a delivered spectator result has no private boards or renewed retrieval authority. Pre-start deletion may permit only a transient cancellation notice, not a fetchable History snapshot. |
| `HistoryView` | Safe projection of the DO-069 immutable started-terminal snapshot: game/code/host identity, started/ended/expiry, outcome/winner, calls ordered by `sequence_no`, players ordered by `player_id`, and each participating player’s final alias/board cells in one-based row-major order. Host/admin-only; never credentials, recovery/session/presence data, spectators or intermediate revisions. DO-070’s child-row layout is private storage, not direct DTO serialization. |

### A. Authentication and sessions

#### A1: `login_account` (Mutating)

Path: `POST /api/auth/login`

Auth scope: Anyone

Validate submitted account credentials and enrollment/reset/disabled state; create a fixed-one-day account session. Existing account sessions may remain valid.

##### Request

No existing account session is required; request-origin/CSRF and abuse controls still apply. The password is sensitive input and must be sent in the HTTPS body only.

Username rules follow [DO-020/021](durable-object-design.md#account-records): trim leading/trailing ASCII whitespace (`0x09`–`0x0D` and `0x20`) before validating 10–50 decoded ASCII characters with no internal whitespace. Preserve trimmed original casing in storage/display. DO-021's approved user revision uses exact case-sensitive equality for uniqueness and login; usernames differing only by case are distinct. No lowercase/case-folded comparison or normalized key is used. Other permitted ASCII controls, including NUL and DEL, remain unchanged. Apply consistently to CLI/admin account creation and login. SQL/constraint enforcement must preserve case-sensitive uniqueness; control-character transport/UI handling and validation/error mechanics remain **TBD**. Password upper bounds remain separately **TBD**. Password hashing is selected as salted Argon2 via RustCrypto `argon2` ([LLD-024](lld.md#password-hashing)); variant, work factors, version, features and runtime details remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `username` | `String` | Y | Submitted account username. |
| `password` | `String` | Y | Submitted account password: at least 10 characters, ASCII-only, any combination with no character-class quotas ([LLD Section 9.1](lld.md#password-policy)). Preserve the exact decoded input for verification. |

##### Response

The returned session has normal scope and a fixed expiry. Issue the DO-031 `__Host-brews_session` cookie (`Secure; HttpOnly; Path=/; SameSite=Lax`, no `Domain`); never return its bearer value in JSON. Other account logins remain valid. Do not return account salt, hash or verifier to clients.

Failure candidates are invalid credentials, enrollment/reset required, a disabled account or throttling; public distinctions and status/error mapping remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account` | `SafeAccount` | Y | Safe account metadata. |
| `session` | `SessionView` | Y | Normal-scope session view with fixed `expires_at: Timestamp`. |

#### A2: `get_current_session` (Non-mutating)

Path: `GET /api/session`

Auth scope: Anyone

Resolve the presented session against authoritative AccountsObject session/account state on each protected request (DO-034); verify current epoch, expiry, revocation, account enabled/lifecycle and scope before returning its safe identity, scope, expiry and permissions. Do not grant from an unrevalidated positive cache. Missing or invalid credentials do not disclose another identity or grant access. Never return credential verifiers.

##### Request

Cookie input is the current account, restricted or participant session, if present. There is no body or arbitrary target identifier.

How the request selects a session context when the browser holds multiple account/game cookies remains **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Return only the presented valid session’s permitted identity, scope and original expiry. Restricted sessions expose setup/reset authority only.

Absent/invalid-session handling—an anonymous result versus an authentication error—remains **TBD**. Never renew expiry or expose credential material. The vocabulary for current permitted actions remains **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `authenticated` | `bool` | Y | Whether the presented session is authenticated in the proposed result; the absent/invalid-session result-versus-error choice remains TBD. |
| `session` | `Option<SessionView>` | N | The presented valid session’s permitted identity, scope and original expiry, when available; nullable versus absent remains TBD. |
| `capabilities` | `Vec<String>` | N | Possible representation of current permitted actions; vocabulary TBD. Restricted sessions expose setup/reset authority only. |

#### A3: `redeem_enrollment_link` (Mutating)

Path: `POST /api/auth/enrollment/redeem`

Auth scope: Anyone

A valid enrollment token is required. Resolve its DO-027 verifier and, in one AccountsObject transaction, validate account binding, purpose, expiry, unused/unrevoked state, current epoch, enabled state and `PendingEnrollment`; atomically consume it and create the restricted password-setup session. At most one concurrent redemption succeeds.

##### Request

The enrollment token is sensitive bearer proof and must be sent in the HTTPS body only. It appears only in the fragment of its canonical configured HTTPS link; the client reads it from the fragment, immediately removes the fragment with `history.replaceState`, then posts the token. Do not put it in the URL path/query, logs, cookies or client-selected account/role/expiry fields.

No existing normal session is required; token binding, expiry and single-use state are server-validated.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `enrollment_token` | `String` | Y | Enrollment token providing the required bearer proof. |

##### Response

The session is restricted to enrollment completion. Atomically consume the link and issue a fixed-life restricted HttpOnly session; never echo the token. Redemption responses use `Cache-Control: no-store`; the redemption page uses `Referrer-Policy: no-referrer` and no third-party scripts/analytics.

Return one safe generic invalid-link result for invalid, expired, consumed, revoked, wrong-purpose, stale-epoch or ineligible-account cases, and create no session. After a committed redemption whose response/cookie is lost, follow DO-029’s fresh-link recovery policy; do not replay the token or session secret.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `setup_required` | `bool` | Y | Whether password setup is required. |
| `session` | `SessionView` | Y | Session view restricted to enrollment completion. |

#### A4: `complete_password_setup` (Mutating)

Path: `POST /api/auth/enrollment/complete`

Auth scope: Anyone

A valid restricted enrollment session is required. After password hashing, follow DO-032: in one AccountsObject transaction, revalidate enabled `PendingEnrollment` state, the unexpired/unrevoked `EnrollmentOnly` session and matching epoch; persist the verifier, set `Verified`/`password_set_at`, increment the epoch once, revoke the restricted session and create one `Normal` session at the new epoch. The new session preserves the restricted session's original absolute deadline. Post-reset session behavior is separate under DO-033.

##### Request

Cookie input is a valid enrollment-only session. The new password is required sensitive input. Account identity and intended role come from the restricted session, never the body.

Enforce the password policy before storing a verifier. Password hashing is selected as salted Argon2 via RustCrypto `argon2` ([LLD-024](lld.md#password-hashing)); variant, work factors, version, features and runtime details remain **TBD**. Maximum length, transport/UI details and any confirmation field remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `new_password` | `String` | Y | Chosen password: at least 10 characters, any combination of ASCII characters, with no required character-class mix, per [LLD Section 9.1](lld.md#password-policy). Preserve the exact decoded input. |

##### Response

Successful setup returns the DO-032 normal `SessionView` and, only after commit, a distinct bearer in the DO-031 `__Host-brews_session` cookie with the original absolute expiry preserved. Retire the restricted session in the same transaction; if the committed response/cookie is lost, use password login rather than replaying the new secret or rolling back setup. Do not echo the password or return account salt, hash or verifier.

Failure candidates are an invalid, expired or replaced restricted session, already-completed setup or password-policy rejection; the exact contract remains **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account` | `SafeAccount` | Y | Safe account metadata after successful setup. |
| `session` | `SessionView` | Y | Session view with normal authority after successful setup, preserving the restricted session’s original absolute expiry. |

#### A5: `redeem_password_reset_link` (Mutating)

Path: `POST /api/auth/password-reset/redeem`

Auth scope: Anyone

A valid reset token is required. Resolve its DO-027 verifier and, in one AccountsObject transaction, require the bound account to be enabled and `ResetRequired`, with matching purpose/current epoch and an unexpired unused/unrevoked link; atomically consume it and create restricted password-reset authority. At most one concurrent redemption succeeds.

##### Request

The reset token is required sensitive bearer proof and must be sent in the HTTPS body only. It appears only in the fragment of its canonical configured HTTPS link; the client reads it from the fragment, immediately removes the fragment with `history.replaceState`, then posts the token. Account and purpose are resolved server-side; do not place the token in the URL path/query, logs or cookies.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `reset_token` | `String` | Y | Password-reset token providing the required bearer proof. |

##### Response

Consume the link once and establish restricted HttpOnly reset authority; no normal privileges are granted yet. Redemption responses use `Cache-Control: no-store`; the redemption page uses `Referrer-Policy: no-referrer` and no third-party scripts/analytics.

Return one safe generic invalid-link result for invalid, expired, consumed, revoked, wrong-purpose, stale-epoch or ineligible-account cases, and create no session. After a committed redemption whose response/cookie is lost, follow DO-029’s fresh-link recovery policy; do not replay the token or session secret.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `reset_required` | `bool` | Y | Whether password reset is required. |
| `session` | `SessionView` | Y | Session view restricted to password reset. |

#### A6: `complete_password_reset` (Mutating)

Path: `POST /api/auth/password-reset/complete`

Auth scope: Anyone

A valid restricted reset session is required. After password hashing, follow DO-033: in one AccountsObject transaction revalidate the enabled `ResetRequired` account, unexpired/unrevoked `PasswordResetOnly` session and matching epoch; store the verifier, set `Verified`/`password_set_at`, increment the epoch exactly once and retire reset-only authority. Do not issue a `Normal` session automatically; clear the shared cookie and direct the user to password login.

##### Request

Cookie input is a valid reset-only session. The new password is required sensitive input. Do not accept a caller-supplied target account, role or session-expiry override.

Apply the same password validation and exact-input preservation rule as setup, enforcing the policy before storing a verifier. Password hashing is selected as salted Argon2 via RustCrypto `argon2` ([LLD-024](lld.md#password-hashing)); variant, work factors, version, features and runtime details remain **TBD**. Remaining maximum-length/transport/UI details remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `new_password` | `String` | Y | Chosen replacement password: at least 10 characters, any combination of ASCII characters, with no required character-class mix, per [LLD Section 9.1](lld.md#password-policy). Preserve the exact decoded input. |

##### Response

On successful reset, retire reset-only authority, clear the `__Host-brews_session` cookie and direct the user to password login; do not issue a normal session automatically (DO-033). The new login creates a normal session with its own fixed one-day expiry. If the committed reset response is lost, use the new password to log in; do not replay secrets or roll back the reset.

Failure candidates are replaced or expired reset authority, invalid account state or password rejection. Do not return the password or account salt, hash or verifier.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `reset_completed` | `bool` | Y | Whether password reset completed. |
| `next_action` | `String` | N | If included, indicates sign-in after reset (DO-033); exact wire value/schema remains TBD. |

#### A7: `logout_session` (Mutating)

Path: `POST /api/auth/logout`

Auth scope: Anyone

Revoke the caller’s current session, clear its cookie and stop associated authorized socket delivery. This cannot revoke another user’s session and is separate from leaving a game.

##### Request

Cookie input is the caller’s current session. There is no target user/session field and no business body.

Context selection and repeat-logout behavior when the cookie is already absent or invalid remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The response may contain the proposed acknowledgment or be an empty success response; the choice remains **TBD**. Clear the matching cookie and stop its authorized socket delivery.

No other account session is revoked; no game Leave, global account logout or game outcome change is implied.

Partial revocation/failure reporting and exact response status remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `logged_out` | `bool` | N | Proposed logout acknowledgment if a response body is selected; an empty success response is also under consideration (TBD). |

### B. Users and privileged account management

The scopes below describe the app-facing endpoints. The developer CLI retains its separately authorized management and bootstrap capabilities.

**Current B1–B9 implementation contract:** These admin APIs are now authorized for local implementation through the Worker and AccountsObject, reusing the CLI's management/audit/receipt rules. All require a live enabled Verified admin with a Normal session; the HTTP transport uses only the session cookie, never the CLI key or a client actor/role claim. POST/DELETE require the exact configured Origin and one canonical UUID-v7 `Idempotency-Key`; reads reject retry headers and any supplied mismatched Origin. Account IDs and cursors preserve canonical UUID-v7 spelling. All responses are JSON with `no-store`, `no-referrer` and `nosniff`, without `Set-Cookie`.

B1 accepts only `cursor` and `limit`: ascending account ID, default 50, maximum 100, `limit` in 1–100; unsupported filters and duplicate/unknown query fields are rejected. B2 has no query/body. B3/B4 accept only the strict object `{username}` and derive the role from the endpoint. B5–B9 have no body; no general username/role edit or admin audit route is added.

The selected response uses a `result` discriminator. First successes retain the fields in the operation's table below and add a secret-free `receipt`: `created`, `enrollment_link`, `password_reset`, `disabled`, `deleted` or `enabled`. B1 uses `users` and B2 `account`. Committed retries return only `{result: "committed", receipt}`, never a URL; unfinished removal returns `{result: "pending", operation_id}` with HTTP 202. Other successes are HTTP 200. Errors use the shared redacted `{error: {code, message}}` contract with HTTP 400/401/403/404/409/503. Existing SafeAccount enum serialization is preserved. The detailed proposal notes below remain historical where this explicit contract resolves a TBD.

Mutations and safe first-result projections remain inside the owning serialized transaction, with fail-closed audit storage. A raw bearer handoff is emitted only after the final storage output gate. Disable/delete reprove issuer authority after the Directory await; loss of authority cannot commit a prepared removal. Unfinished coordination is retained for alarm recovery, without persisting issuer credentials. No gameplay/socket sender or production deployment is implied. Protected result release revalidates current admin authority after the final scheduling/storage awaits. An admin self-reset commits its reset and revocation but then fails this release guard with HTTP 401: no URL/receipt is handed to the revoked session. Another authorized admin or the CLI must explicitly reissue the reset link; retries never mint or replay the lost secret.

#### B1: `list_users` (Non-mutating)

Path: `GET /api/users`

Auth scope: Admin

List safe metadata for provisioned host/admin accounts. Exclude anonymous memberships and secrets. Columns, filtering, sorting and pagination remain TBD.

##### Request

Cookie input is an enrolled admin session. There is no body.

Possible query inputs are `role: Option<AccountRole>`, `status: Option<AccountStatus>`, `cursor: Option<String>` and `limit: Option<u32>`; filters/sort/paging support, defaults and limits remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Do not include anonymous memberships, passwords, verifiers, access-link URLs or session tokens. Selected columns and total-count support remain **TBD**.

Failure candidates are an unauthenticated or non-admin caller, or an invalid query; status/error schemas remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `users` | `Vec<SafeAccount>` | Y | Safe metadata for provisioned host/admin accounts. |
| `next_cursor` | `Option<String>` | N | Possible continuation cursor if pagination is selected; nullable versus absent remains TBD. |

#### B2: `get_user` (Non-mutating)

Path: `GET /api/users/{account_id}`

Auth scope: Admin

Read a target account’s safe metadata and lifecycle state for management. This does not expose password, token or recovery verifiers.

##### Request

Path input is `account_id: AccountId`. Cookie input is an enrolled admin session. There is no body.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Potential management availability is derived server-side; its exact fields remain **TBD**.

Failure candidates are a missing/deleted target or denied admin access. Do not disclose credentials, links or verifiers.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account` | `SafeAccount` | Y | Target account’s safe metadata and lifecycle state for management. |

#### B3: `create_host_account` (Mutating)

Path: `POST /api/users/hosts`

Auth scope: Admin

Validate the proposed username; create a pending host account and one-day enrollment link. Return the link only after durable creation. The developer CLI may also perform this through its privileged path.

##### Request

Cookie input is an enrolled admin session. The target role is fixed to Host by this operation and is not user-editable.

Command metadata `command_id: CommandId` is proposed for retry-safe creation; transport and requiredness remain **TBD**. CLI input transport is separate.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `username` | `String` | Y | Username for the new host account. |

##### Response

Return these outputs only after durable creation. The enrollment link is a sensitive handoff to the authorized creator, not ordinary account metadata; build it from the configured canonical HTTPS origin, keep the token in its fragment, use `Cache-Control: no-store`, and redact it from logs/traces (DO-028). DO-029's same-command retry returns only a secret-free receipt; an undelivered URL is recovered only through an explicit successor reissue, which invalidates predecessors.

Failure candidates are a username conflict, invalid username format or denied creation. Do not return raw token verifiers or an initial password.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account` | `SafeAccount` | Y | Safe account metadata in pending enrollment. |
| `enrollment_url` | `String` | Y | Enrollment link for the newly created account. |
| `link_expires_at` | `Timestamp` | Y | Enrollment-link expiry. |

#### B4: `create_admin_account` (Mutating)

Path: `POST /api/users/admins`

Auth scope: Admin

Create a pending admin account and enrollment link. Only an existing admin or the developer CLI may perform this; first-admin bootstrap uses CLI.

##### Request

Cookie input is an existing enrolled admin session. The target role is fixed to Admin.

Command metadata `command_id: CommandId` is proposed; transport and requiredness remain **TBD**. First-admin bootstrap has no anonymous HTTP bypass and uses the developer CLI.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `username` | `String` | Y | Username for the new admin account. |

##### Response

The enrollment URL is a sensitive handoff to the authorized creator; use the configured canonical HTTPS origin, fragment-only token, `Cache-Control: no-store` and token-redacted logs/traces (DO-028). DO-029's same-command retry returns only a secret-free receipt; an undelivered URL is recovered only through an explicit successor reissue, which invalidates predecessors.

Failure candidates are an unauthorized caller, username conflict or invalid username; no self-elevation or public registration is allowed.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account` | `SafeAccount` | Y | Safe account metadata in pending enrollment. |
| `enrollment_url` | `String` | Y | Enrollment link for the newly created admin account. |
| `link_expires_at` | `Timestamp` | Y | Enrollment-link expiry. |

#### B5: `reissue_enrollment_link` (Mutating)

Path: `POST /api/users/{account_id}/enrollment-links`

Auth scope: Admin

For an enabled `PendingEnrollment` account, atomically increment `credential_epoch` once, revoke predecessor links and account/restricted sessions, and issue exactly one fresh single-use link for 24 hours bound to the new epoch. Keep account status unchanged. Disabled or otherwise ineligible accounts are rejected. A same-`command_id` retry returns a secret-free receipt only; to recover an undelivered URL, explicitly issue a new reissue command, which creates a new successor link.

##### Request

Path input is `account_id: AccountId`. Cookie input is an enrolled admin session. There is no replacement username/role in the body; no business-body fields are proposed here.

Command metadata `command_id: CommandId` is proposed; requiredness/transport remain **TBD**. DO-029 fixes same-ID behavior for link issuance: never mint another link or replay its raw URL; retain only the approved secret-free receipt for 30 days.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The enrollment URL is sensitive. Predecessors are invalidated as part of the accepted reissue. The canonical-origin fragment-only link uses `Cache-Control: no-store` and token-redacted logs/traces (DO-028). DO-029 retains metadata-only link rows through the latest expiry/consumption/revocation event plus 30 days, then permits asynchronous deletion.

Failure candidates are an absent account, an already-enrolled account or an account not eligible for enrollment reissue. Enrolled accounts use reset instead.

Predecessor links and restricted/account sessions are revoked in the accepted reissue transaction; epoch matching remains the authoritative fence. A committed-but-lost issuance response never replays the raw link: use an explicit new reissue command. A committed-but-lost redemption response never replays the consumed token or session secret; if still pending/reset-required, an authorized admin/CLI issues a fresh link, otherwise use password login or the admin reset flow. The API returns the approved generic invalid-link response on replay. Delivery follows the DO-028 protected handoff rules.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account_id` | `AccountId` | Y | Account whose enrollment link was reissued. |
| `enrollment_url` | `String` | Y | Replacement enrollment link. |
| `link_expires_at` | `Timestamp` | Y | Replacement enrollment-link expiry. |

On a repeated request with the same `command_id`, return only a secret-free receipt with account ID, link ID, `Enrollment` purpose, committed outcome and expiry; never include/recover the URL. Exact receipt wire envelope/status/field encoding remains **TBD**.

#### B6: `initiate_password_reset` (Mutating)

Path: `POST /api/users/{account_id}/password-reset-links`

Auth scope: Admin

For an enabled enrolled account, enter `ResetRequired` and issue a one-day reset link; for an already enabled `ResetRequired` account, atomically reissue a fresh link while keeping old-password login blocked. Each issuance/reissue increments `credential_epoch` exactly once and revokes predecessor links/account sessions. Reset initiation also records durable DO-036 socket-revocation work for affected subscriptions; do not claim close completion until the owning GameObjects acknowledge targets. Reject reset operations while `disabled_at` is populated. Reset-link expiry does not restore old-password access (DO-022/DO-029).

##### Request

Path input is `account_id: AccountId`. Cookie input is an enrolled admin session. No new password is supplied by the admin; no business-body fields are proposed here.

Command metadata `command_id: CommandId` is proposed; requiredness/transport remain **TBD**. DO-029 fixes same-ID link-issuance behavior: no duplicate mint and no raw-URL replay; retain only the secret-free 30-day receipt. No body placement is assigned to this metadata.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The reset URL is sensitive. Success implies the reset gate, predecessor invalidation and durable DO-036 revocation-work commit; it does not imply that every GameObject has acknowledged socket closure. Do not report all sockets closed while any target remains pending. Use the canonical HTTPS origin, fragment-only token, `Cache-Control: no-store` and token-redacted logs/traces (DO-028).

A repeated request with the same `command_id` returns only the secret-free 30-day receipt (account/link IDs, purpose, committed outcome, expiry); it never mints another token or replays the URL. New issuance after a lost URL requires an explicit successor reissue, which invalidates predecessors. A committed-but-lost redemption response is recovered via a fresh link only while status remains `ResetRequired`; after successful reset use password login. Exact outbox/result envelopes, retry status and response encoding remain **TBD**; the DO-036 delivery and acknowledgment rules apply. Old-password login must remain blocked while reset is pending.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account_id` | `AccountId` | Y | Account placed into password-reset-pending state. |
| `status` | `AccountStatus` | Y | Account status `ResetRequired`. |
| `reset_url` | `String` | Y | Password-reset link for the account. |
| `link_expires_at` | `Timestamp` | Y | Reset-link expiry. |

On a repeated request with the same `command_id`, return only a secret-free receipt with account ID, link ID, `PasswordReset` purpose, committed outcome and expiry; never include/recover the URL. Exact receipt wire envelope/status/field encoding remains **TBD**.

#### B7: `disable_account` (Mutating)

Path: `POST /api/users/{account_id}/disable`

Auth scope: Admin

Acquire the Directory gate first; while held, reject new-game/host-transfer assignments to this account. If a nonterminal hosted game exists, clear the gate and reject. Otherwise run a serialized AccountsObject mutation that revalidates actor/target, rejects app-admin self-disable (DO-105), and atomically rejects disabling the last enabled Verified admin with the disable/revocation (DO-106). On success disable and revoke credentials/connections, then clear the gate. If interrupted, retain the gate and retry safely.

##### Request

Path input is `account_id: AccountId`. Cookie input is an enrolled admin session. No business-body fields are proposed here.

Command metadata `command_id: CommandId` remains proposed; DO-105 forbids app-admin self-disable/delete and DO-106 prevents leaving zero enabled Verified admins. Removal confirmation, endpoint requiredness/transport and error mapping remain **TBD**; no body placement is assigned here.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The response may contain the proposed account/status body or be an empty success response; the choice remains **TBD**. Success requires access revocation, not merely a requested status change.

A failure candidate is a target hosting a nonterminal game. The optional error-detail field below is restricted to the authorized admin; its error envelope and exact disclosure remain **TBD**.

There is no automatic cancellation/transfer; preserve unrelated games and History. An already-disabled account is an authorized no-op without timestamp changes; revoked credentials stay revoked. Missing-target handling, operation-retry races and partial revocation outcomes remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account_id` | `AccountId` | N | Account disabled, if the proposed account/status response body is selected; an empty success response is also under consideration (TBD). |
| `status` | `AccountStatus` | N | Preserved lifecycle state (PendingEnrollment, Verified or ResetRequired), if the proposed body is selected; an empty success response is also under consideration (TBD). |
| `disabled` | `bool` | N | Computed as `disabled_at.is_some()`; `true` after successful disable if the proposed body is selected. Not a database column or client-writeable flag; lifecycle status remains independent. |
| `blocking_game_ids` | `Vec<GameId>` | N | Optional safe detail in the nonterminal-hosting failure response, not a success-body field. Only for the authorized admin; exact error detail/envelope TBD. |

#### B8: `delete_account` (Mutating)

Path: `DELETE /api/users/{account_id}`

Auth scope: Admin

Acquire the Directory gate first; while held, reject new-game/host-transfer assignments to this account. If a nonterminal hosted game exists, clear the gate and reject. Otherwise run a serialized AccountsObject mutation that revalidates actor/target, rejects app-admin self-delete (DO-105), and atomically rejects deleting the last enabled Verified admin with the deletion/revocation (DO-106). On success delete account/credential data without deleting unrelated or unexpired History, then clear the gate. If interrupted, retain the gate and retry safely.

##### Request

Path input is `account_id: AccountId`. Cookie input is an enrolled admin session. Do not assume a DELETE body.

Command metadata `command_id: CommandId` remains proposed. DO-105/106 govern self-removal/last-admin; confirmation/header placement, repeat-delete response and command requiredness remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The response may contain the proposed account/deletion acknowledgment or be an empty success response; the choice remains **TBD**. No deleted credential record is returned.

Failure candidates are a hosted nonterminal game, denied removal or an unknown target; safe blocker details remain **TBD**.

Preserve unexpired History/minimal references; do not label deletion complete while required account/credential cleanup is still pending.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account_id` | `AccountId` | N | Account deleted, if the proposed acknowledgment body is selected; an empty success response is also under consideration (TBD). |
| `deleted` | `bool` | N | Proposed deletion acknowledgment if a response body is selected; an empty success response is also under consideration (TBD). |

#### B9: `enable_account` (Mutating)

Path: `POST /api/users/{account_id}/enable` (proposed)

Auth scope: Admin

**Confirmed capability and actor restriction (HLD-078; DO-022):** Enable an existing disabled host/admin account only when the caller is a different fully enrolled admin account with valid authority, or the developer using the separately privileged CLI path. Enforce caller-account ID != target-account ID for app-admin calls; ordinary hosts, participants, unauthenticated callers and the disabled target itself cannot enable it. This is not role editing, deleted-account restoration or automatic revival of revoked/expired credentials. **Approved outcome:** clear the authoritative `disabled_at` timestamp while preserving the existing lifecycle `status`. No disabled boolean is stored. Previously Verified accounts may log in afresh with their existing password; PendingEnrollment still needs setup, and ResetRequired still needs reset. Do not revive old sessions/links. Pending setup/reset requires a newly issued link through the existing flow. Clear `disabled_at` on the accepted enable transition; an authorized already-enabled retry is a no-op without timestamp changes. DO-022 approves the credential-epoch increments and match checks; DO-036 approves durable cross-Object socket-revocation work/acknowledgement and fail-closed frame checks. Exact transaction, concurrency and physical outbox integration remain **TBD**.

##### Request

Path input is `account_id: AccountId`. The app route uses an enrolled admin session cookie; the CLI uses its privileged server-validated interface, not browser cloud credentials. No request body is proposed. Confirmation, CSRF/origin checks, command metadata, retry behavior and exact route are **TBD**. Authorization and target-state checks must hold at commit.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The following safe acknowledgment body is proposed; final success/error encoding and status codes remain **TBD**. Do not issue an account-session cookie for the target or return its verifier, token, or other credentials in this acknowledgment. A future reviewed link-delivery flow is separate. Denied authority/self-enable, missing/deleted target, non-disabled target and conflicting account operations require explicit error/retry handling; an already-enabled target is a no-op after authorization checks. Exact concurrent-operation/idempotency mechanics remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account_id` | `AccountId` | Y | Target account after successful enable. |
| `status` | `AccountStatus` | Y | Preserved lifecycle state (Verified, PendingEnrollment or ResetRequired). |
| `disabled` | `bool` | Y | Computed as `disabled_at.is_some()`; `false` after successful enable clears the timestamp. Not a database column or client-writeable flag; lifecycle status remains independent. |

### C. Game discovery, configuration and ownership

#### C1: `list_games` (Non-mutating)

Path: `GET /api/games`

Auth scope: Host

List current-game summaries with lifecycle, designated host and appropriate access information. Detailed filtering remains TBD.

##### Request

**Potential inputs (proposal):** An enrolled host/admin session is supplied by cookie. No request body.

Possible query parameters: `state: Option<GameState>`, `cursor: Option<String>`, `limit: Option<u32>`; supported filters, ordering and paging remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

**Potential outputs (proposal):** Each summary distinguishes the designated host and caller permissions; discovery does not grant mutation rights.

Failure candidates: invalid/expired account session or unsupported query; exact fields/statuses remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `games` | `Vec<GameSummary>` | Y | Summaries for permitted current games. |
| `next_cursor` | `Option<String>` | N | Possible continuation cursor if paging is chosen; paging remains TBD. |

#### C2: `get_current_nonterminal_game` (Non-mutating)

Path: `GET /api/games/current`

Auth scope: Host

Find the sole application-wide nonterminal game in New, Awaiting Players or In Progress. Supports resume/access decisions and creation-blocked feedback; authorization still governs what the caller may see/do (HLD-077).

##### Request

**Potential inputs (proposal):** An enrolled host/admin session is supplied by cookie. No request body or target-host filter; this lookup concerns the one global nonterminal-game slot.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

**Potential outputs (proposal):** An empty slot is not an error by itself. Only return a safe summary the caller is authorized to view; do not expose another host’s private game data.

Prepared/unpublished reservation visibility and absent-result encoding remain **TBD**. Do not expose uncommitted or credential-bearing coordination records.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game` | `Option<GameSummary>` | N | The sole nonterminal game occupying the global slot (New, Awaiting Players or In Progress), if any and caller-authorized; absent-result encoding remains TBD. |

#### C3: Retired duplicate lookup

The former `get_active_game` proposal is removed: C2 is the single proposed lookup for the one application-wide nonterminal game across New/Awaiting Players/In Progress. C3 is not an endpoint or handler. Its identifier is retained only to avoid silently reusing a prior catalog ID.

#### C4: `resolve_game_code` (Non-mutating)

Path: `GET /api/game-codes/{code}`

Auth scope: Anyone

Normalize and resolve an issued code to an existing game and limited entry information. Unknown codes never create games; discovery grants no membership, private data or gameplay permissions.

##### Request

**Potential inputs (proposal):** Path parameter: `code: String`, normalized/validated to `GameCode` by the backend. No request body or privileged session required.

Apply entry lookup abuse controls; a code is a discovery key, not identity proof.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

**Potential outputs (proposal):** The body is a limited eligible-game entry projection. No roster, private boards, account metadata or credentials. A terminal reserved code must not expose a joinable game.

Unknown, terminal, unavailable and malformed-code error distinctions, plus any admission-context handoff, remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Resolved game identifier in the limited eligible-game entry projection. |
| `game_code` | `GameCode` | Y | Resolved issued game code. |
| `state` | `GameState` | Y | Resolved game's lifecycle state. |
| `can_join_player` | `bool` | Y | Whether player entry is available in this limited projection. |
| `can_join_spectator` | `bool` | Y | Whether spectator entry is available in this limited projection. |

#### C5: `get_game` (Non-mutating)

Path: `GET /api/games/{game_id}`

Auth scope: Anyone

Read only the caller-authorized game/configuration projection. Host/admin accounts and admitted player/spectator sessions receive their permitted fields. A game ID alone does not authorize private data or unrestricted game inspection.

##### Request

**Potential inputs (proposal):** Path parameter: `game_id: GameId`. A valid authorized account or participant session is supplied by cookie. No request body.

The server derives the allowed view; no client-selected role or arbitrary player identity grants access.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

**Potential outputs (proposal):** No pre-start board, spectator private boards or recovery material. Terminal readers must obey final-view/History rules, not bypass them through this route.

Unknown/forbidden/expired game handling and exact role projections remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game` | `GameView` | Y | Caller-authorized view containing only caller-visible configuration, lifecycle, calls, permitted membership/board information and `view_revision: Revision`. |

#### C6: `create_game` (Mutating)

Path: `POST /api/games`

Auth scope: Host

Create a distinct New game, assign its creator as designated host and establish the host-idle deadline. Reject if the creator account has an active AccountAssignmentGate or any other nonterminal game occupies the single global reservation. Claim the slot before GameObject creation; on failure, retries use the same game ID/code association (HLD-077, DO-015, DO-017/018).

##### Request

**Approved GAME-SLICE-01/02:** An enabled Verified host/admin Normal session is supplied by the existing account cookie. Require exact application Origin and a canonical UUID-v7 `Idempotency-Key`. The optional creation-time configuration object supplies overrides; omitted settings use approved defaults. Validate the complete effective configuration before claiming the global slot. No caller-selected designated host or game ID.

The `command_id: CommandId` comes only from `Idempotency-Key`, not the JSON body. The authenticated creator determines the designated host.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `configuration` | `GameConfiguration` overrides | N | Optional object. Omitted settings use approved defaults; supplied fields must satisfy DO-042–044. C7 updates are deferred from this slice. |

##### Response

**Potential outputs (proposal):** Returned for the operating caller. No published code or board yet.

Failure candidates: occupied global nonterminal-game reservation, invalid configuration or unavailable coordination.

**Approved GAME-SLICE-02:** Fresh committed success is HTTP 200 with the documented safe fields and a secret-free receipt. Exact authorized actor/command/fingerprint retries return only the receipt. Unfinished coordination is HTTP 202 with an operation ID and no unpublished code. If Directory has already claimed a `game_id` but GameObject initialization failed, retry under that same ID with the same configuration fingerprint; preserve any existing code association and never allocate a replacement game. Use the existing no-store JSON/error envelope; reservation conflicts are 409 and unavailable coordination is 503.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game` | `GameSummary` | Y | Created game's summary in New. |
| `configuration` | `GameConfiguration` | Y | Created game's configuration. |
| `idle_cancel_due_at` | `Timestamp` | Y | Host-idle cancellation deadline. |

#### C7: `update_game_configuration` (Mutating)

Path: `PATCH /api/games/{game_id}/configuration`

Auth scope: Host

The designated host or an authorized admin updates pool, board, free cells or capacities only in New. Validate the resulting configuration without partially applying invalid changes.

##### Request

**Potential inputs (proposal):** Path parameter: `game_id: GameId`. A designated host/admin session is supplied by cookie.

The body is a configuration patch with proposed optional fields. Omitted-field versus null/replace semantics remain **TBD**.

Command metadata: `command_id: CommandId`; possible `expected_revision: Revision` conflict guard. Transport/requiredness remain **TBD**; these metadata fields are not assigned here to the body.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `numeric_upper_bound` | `u32` | N | Optional numeric upper-bound patch; require 1–1,000 inclusive (DO-043), default 75; string pool is `1..=numeric_upper_bound`. Omitted-field versus null/replace semantics remain TBD. |
| `board_side_length` | `u8` | N | Proposed optional board side-length patch field; omitted-field versus null/replace semantics remain TBD. |
| `free_cells_enabled` | `bool` | N | Proposed optional free-cell enablement patch field; omitted-field versus null/replace semantics remain TBD. |
| `free_cell_positions` | `Vec<CellPosition>` | N | Proposed optional free-cell positions patch field; omitted-field versus null/replace semantics remain TBD. |
| `player_capacity` | `u8` | N | Proposed optional player-capacity patch field; omitted-field versus null/replace semantics remain TBD. |
| `spectator_capacity` | `u8` | N | Proposed optional spectator-capacity patch field; omitted-field versus null/replace semantics remain TBD. |
| `winning_pattern` | `WinningPattern` | N | Proposed optional winning-pattern patch field; omitted-field versus null/replace semantics remain TBD. |

##### Response

**Potential outputs (proposal):** All accepted fields apply together.

Failure candidates: non-New game, non-owner host, invalid bounds/free positions/capacities or revision conflict. No partial patch on rejection.

Validation/error fields and concurrency protocol remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `configuration` | `GameConfiguration` | Y | Resulting configuration. |
| `view_revision` | `Revision` | Y | Resulting view revision. |

#### C8: `open_game_lobby` (Mutating)

Path: `POST /api/games/{game_id}/lobby`

Auth scope: Host

The designated host or an authorized admin freezes validated configuration, transitions to Awaiting Players and publishes its issued code consistently with game state.

##### Request

**Potential inputs (proposal):** Path parameter: `game_id: GameId`. A designated host/admin session is supplied by cookie.

**Approved GAME-SLICE-02:** Require the existing account cookie, exact application Origin and canonical UUID-v7 `Idempotency-Key`. The JSON body requires `expected_revision` for the currently authorized host projection. No client-supplied code or replacement configuration; command ID belongs only in the header.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `expected_revision` | `Revision` | Y | Expected current host-projection revision, checked before a fresh lobby transition. Exact receipt retries do not repeat the transition. |

##### Response

**Potential outputs (proposal):** Only return successful publication when Directory/Game state is consistent; no boards assigned yet.

Fresh matching publication success is HTTP 200 with the documented safe fields plus a secret-free receipt; exact authorized retries return only the receipt. Unfinished publication is HTTP 202 with an operation ID and no unpublished code. Lifecycle/reservation/revision conflicts are 409; unavailable dependencies are 503. Reuse the existing no-store JSON/error envelope.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Game identifier. |
| `state` | `GameState` | Y | Awaiting Players. |
| `game_code` | `GameCode` | Y | Published issued game code. |
| `configuration` | `GameConfiguration` | Y | Frozen configuration. |
| `view_revision` | `Revision` | Y | Resulting view revision. |

#### C9: `resume_game` (Mutating)

Path: `POST /api/games/{game_id}/resume`

Auth scope: Host

Restore existing state without recreating codes/boards, subject to game permissions. A qualifying designated-host open/resume also updates the unstarted host-activity deadline. Passive reads must not silently refresh that deadline; the effect of non-designated admin activity remains TBD.

##### Request

**Potential inputs (proposal):** Path parameter: `game_id: GameId`. An authorized host/admin session is supplied by cookie. No input-body fields are specified in this proposal.

Potential command metadata: `command_id: CommandId`; identify an intentional open/resume, not a background heartbeat. Format remains **TBD**; metadata is not assigned here to the body.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

**Potential outputs (proposal):** Return existing code/boards; do not recreate them. Passive/other-host views do not renew the host-idle timer. Under DO-040, non-designated admin open/resume does not renew it either; the current designated host's accepted qualifying activity is required.

Terminal/exited-view routing and denied/expired outcomes remain **TBD**; do not reopen terminal access.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game` | `GameView` | Y | Restored existing game view. |
| `idle_cancel_due_at` | `Option<Timestamp>` | N | The designated operator may also receive the host-idle cancellation deadline where applicable. |

#### C10: `transfer_game_host` (Mutating)

Path: `POST /api/games/{game_id}/host-transfer`

Auth scope: Designated Host / Admin

Transfer only in New/AwaitingPlayers/InProgress. Revalidate current designated-host/admin authority and the distinct eligible target at commit; reject if the target's Directory assignment gate is present. Require the expected host-assignment revision and atomically compare-and-set `designated_host_id` in the GameObject, incrementing `host_assignment_revision` and game `revision` once each and writing the authenticated actor-scoped command receipt. An exact actor/command/fingerprint retry within the applicable live-receipt window returns the receipt without another increment; command-ID reuse with a different fingerprint rejects. Expired-receipt replay remains governed by DO-076–DO-084. Preserve game data and the same global reservation; no recipient acceptance. The removal/transfer race follows DO-039's gate-first versus commit-first order; a gate precheck alone is insufficient. The transfer's actor-scoped CommandReceipt is distinct from the broader AdminAuditRecord/audit-retention policy (DO-088–DO-090).

##### Request

**Potential inputs (proposal):** Path parameter: `game_id: GameId`. A designated host/admin session is supplied by cookie.

Command metadata (required): `command_id: CommandId`, `expected_host_assignment_revision: Revision`. Metadata is not assigned here to the body. The authenticated actor comes from the session cookie; actor identity is never caller-supplied.

Eligible-target discovery for ordinary hosts, confirmation binding and wire representation of the required expected revision remain **TBD**; no incoming acceptance token.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `target_account_id` | `AccountId` | Y | Selected distinct, enabled, Verified account whose role is Host and whose Directory assignment gate is absent. |
| `confirmed` | `bool` | Y | Proposed confirmation field, required to be true for the selected target; confirmation binding remains TBD. |

##### Response

**Potential outputs (proposal):** Preserve game contents and reservations. Failure candidates: terminal game, invalid/disabled/unverified/non-Host target, target assignment gate, missing/stale host-assignment revision, denied authority, or same-command ID with a different request fingerprint.

Exact eligibility/error/confirmation contract remains **TBD**; transfer success is a committed assignment, not a pending invitation.

The response also includes the caller's refreshed capabilities/view revision. The field names, types and shape of these additional details remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Game whose designated-host assignment was transferred. |
| `designated_host_id` | `AccountId` | Y | Committed designated-host account identifier. |
| `host_assignment_revision` | `Revision` | Y | Committed host-assignment revision. |

### D. Membership, admission, aliases and recovery

#### D1: `get_my_membership` (Non-mutating)

Path: `GET /api/games/{game_id}/membership`

Auth scope: Anyone

A valid participant session is required. Read its own membership’s role, current alias, access state and recovery-enabled status where applicable. Never return the answer/verifier or another participant’s membership.

##### Request

Path: `game_id: GameId`. Cookie: valid participant session. There is no target player/spectator parameter and no request body.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Return only the caller's membership; never return an answer/verifier or expose another participant's membership.

No-membership, invalid-session and terminal-only-session handling **TBD**. This route cannot recreate deleted membership or renew access.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `membership` | `MembershipView` | Y | The caller's membership, including role/own identity, player alias where applicable, access state and original session expiry. |
| `recovery_enabled` | `bool` | N | Player-only recovery-enabled status; not applicable to other participant roles. |

#### D2: `list_game_players` (Non-mutating)

Path: `GET /api/games/{game_id}/players`

Auth scope: Host

Read player aliases and permitted membership/presence information. Distinguish retained disconnected players from currently connected players.

##### Request

Path: `game_id: GameId`. Cookie: enrolled host/admin session. No request body.

Possible pagination/selection parameters **TBD**; no anonymous roster query.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Distinguish retained disconnected players from currently connected players, and retained start-roster membership from connected valid-session start eligibility. Do not expose recovery-enabled status, credentials or private answer metadata to hosts.

Exact roster revision/projection, expired-game behavior and error schemas **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `players` | `Vec<object>` | Y | Player roster with proposed nested fields `player_id: PlayerId`, `alias: String`, `connected: bool`, `retained: bool` and `left_during_play: bool`. Field names and precise presence semantics **TBD**. |

#### D3: `get_game_availability` (Non-mutating)

Path: `GET /api/games/{game_id}/availability`

Auth scope: Anyone

Read admission/start availability through a caller-appropriate projection. Unauthenticated entry flows receive only permitted entry information, not private roster data. Detailed occupancy and start eligibility depend on caller authority; spectators never satisfy the connected-player minimum.

##### Request

Path: `game_id: GameId`; code-based entry context where required, format **TBD**. An optional authorized account/participant cookie determines extra permitted detail.

No request body and no client-authoritative desired role/occupancy counts.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Admission/start availability uses a caller-appropriate projection. Entry availability is advisory until atomic admission. Unauthenticated entry flows receive only permitted entry information, not private roster/start details; detailed occupancy and start eligibility depend on caller authority. Spectators never satisfy the connected-player minimum.

Exact audience matrix, code-context enforcement, reason codes and current/global-slot freshness **TBD**.

The authorized host/admin projection may also include capacities; their field names/types and the exact projection shape remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `state` | `GameState` | Y | Lifecycle state in the caller-appropriate entry projection. |
| `can_join_player` | `bool` | Y | Whether player entry is available; advisory until atomic admission. |
| `can_join_spectator` | `bool` | Y | Whether spectator entry is available; advisory until atomic admission. |
| `player_count` | `u32` | N | Conditional authorized host/admin detail only; exact audience matrix and requiredness within that projection TBD. |
| `spectator_count` | `u32` | N | Conditional authorized host/admin detail only; not automatically public entry information. |
| `connected_player_count` | `u32` | N | Conditional authorized host/admin count used for connected-player start eligibility; spectators do not count. |
| `can_start` | `bool` | N | Conditional authorized host/admin start eligibility; no start authority is granted by this field. |

#### D4: `is_alias_claimed` (Non-mutating)

Path: — Internal

Auth scope: System

Check an exact case-sensitive trimmed alias claim within a game during admission, rename or role switching. No public alias-availability endpoint is implied.

##### Request

These are trusted internal arguments, not an HTTP request body or a public alias-availability endpoint.

The transaction context must belong to admission, rename or role switching. The exact Rust signature is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Owning game supplied to the internal operation. |
| `alias` | `String` | Y | Candidate alias after HLD-051 outer-space trimming; compare exact case-sensitive spelling against each retained PlayerRecord alias in the owning game. This is the alias itself, not a duplicate normalized key. |
| `exclude_player_id` | `PlayerId` | N | Optional exclusion for a same-membership rename, supplied by the trusted caller. |

##### Response

This is a trusted internal result; no public identity/availability endpoint is implied.

An advisory check alone does not claim the alias; the enclosing transaction enforces uniqueness. Error/result type and self-rename semantics **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `claimed` | `bool` | Y | Whether the exact case-sensitive trimmed alias is already claimed in the game, subject to the unresolved self-rename semantics. |

#### D5: `join_game_as_player` (Mutating)

Path: `POST /api/games/{game_id}/players`

Auth scope: Anyone

In Awaiting Players, validate the code-based admission context, alias and capacity; claim alias/seat, create membership and optional answer verifier, and issue a player session. No board is generated yet.

##### Request

**Approved GAME-SLICE-04:** Before D5, a code-validated `POST /api/games/{game_id}/admission-context` establishes a 15-minute game-scoped HttpOnly retry-context cookie, reserving a stable server-chosen player identity but no seat. D5 requires that cookie plus exact application Origin, UUID-v7 `Idempotency-Key`, `game_code`, `alias` and optional `recovery_answer` in JSON. No caller-supplied player ID. A valid existing player session reconnects rather than allocating another membership.

Canonicalize/enroll an optional answer under DO-050/051; blank/omitted means no verifier, never an empty credential. The recovery endpoint itself remains deferred from this slice.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_code` | `GameCode` | N | Required code-based admission proof when this body-field alternative is used; an equivalent verified code-entry context may be used instead (**TBD**). Conditional, not permission to omit admission proof. |
| `alias` | `String` | Y | Requested player alias. |
| `recovery_answer` | `String` | N | Optional sensitive recovery answer, submitted in the HTTPS body only. Blank/omitted means no enrolled proof, not an empty credential; exact input handling **TBD**. |

##### Response

Cookie effect: issue a fixed-one-day HttpOnly player session bound to the stable game/member and current alias. No token or answer in JSON. No board is generated yet.

Fresh HTTP 200 sets the fixed-one-day Secure/HttpOnly/SameSite=Strict game-scoped player cookie and returns only player ID, accepted alias, expiry, view revision and secret-free receipt. Exact context-bound command/fingerprint retries return only the receipt, never a cookie or full snapshot. Admission conflicts/invalid authority do not partially allocate a seat/alias/verifier. A lost player-cookie response cannot be recovered from the command receipt; answer recovery is required and its endpoint remains deferred from this slice.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `membership` | `MembershipView` | Y | Membership for the admitted player. |
| `game` | `GameView` | Y | Game view in the lobby. |
| `board` | `null` | N | Null or omitted (**TBD**); no board is generated before start. |

#### D6: `join_game_as_spectator` (Mutating)

Path: `POST /api/games/{game_id}/spectators`

Auth scope: Anyone

Validate the code-based admission context and Awaiting Players/In Progress capacity; allocate spectator identity/seat and issue its session. No player or account privileges are granted.

##### Request

Path: `game_id: GameId`. Code-based admission requires `game_code` or verified code-entry context (**TBD**). No player alias or recovery answer is submitted. No privileged account is required.

Proposed command metadata: `command_id: CommandId`; transport/placement remains **TBD**, not assigned to the body here. Duplicate admission/retry identity **TBD**.

The exact no-body/admission-context alternative is **TBD**; a missing body does not remove the code-context requirement.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_code` | `GameCode` | N | Required when code-based admission uses this body-field alternative; verified code-entry context may be used instead (**TBD**). Conditional, not permission to omit admission proof. |

##### Response

Cookie effect: issue an HttpOnly spectator session. No board, private qualifiers or account privileges are granted.

Failure candidates: disabled/full spectators, invalid code context or terminal game. Exact no-body/admission-context alternative and lost-response behavior **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `membership` | `MembershipView` | Y | Spectator membership. |
| `game` | `GameView` | Y | Audience-only game view. |
| `session_expires_at` | `Timestamp` | Y | Fixed session expiry. |

#### D7: `rename_my_alias` (Mutating)

Path: `PATCH /api/games/{game_id}/membership/alias`

Auth scope: Player

In Awaiting Players only, validate the caller's stable player binding and candidate alias, then atomically claim the new exact-case alias and release the old one while preserving membership, seat, answer verifier and original session expiry. The player session binds by stable `player_id`, not alias; rename does not rewrite/rotate credentials or renew expiry. Current alias is read from PlayerRecord. Self-rename/no-op response behavior, retry metadata and response/error types remain **TBD**.

##### Request

Path: `game_id: GameId`. Cookie: caller's valid player session. No target player ID.

Command metadata: `command_id: CommandId`; optional `expected_revision: Revision`. Exact transport/requiredness **TBD**; neither is newly assigned to the request body.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `alias` | `String` | Y | New display alias requested before start. |

##### Response

The old alias is released; stable membership, recovery verifier and seat remain. The player session stays bound by stable `player_id`; do not rewrite/rotate credentials or extend the original session expiry. Response fields are the current PlayerRecord identity/alias and unchanged expiry.

Failure candidates: started/terminal game, invalid/claimed alias or stale/invalid session. Idempotency metadata, response/error variants and self-rename/no-op response remain **TBD**; they must not change the approved atomic alias/session-binding behavior.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `player_id` | `PlayerId` | Y | Stable player identity. |
| `alias` | `String` | Y | Accepted alias. |
| `session_expires_at` | `Timestamp` | Y | Unchanged original session expiry. |
| `view_revision` | `Revision` | Y | View revision. |

#### D8: `switch_to_spectator` (Mutating)

Path: `POST /api/games/{game_id}/membership/switch-to-spectator`

Auth scope: Player

DO-047: before start, atomically acquire spectator admission, release player seat/alias and delete the PlayerRecoveryRecord. Retire player-role session/socket authority and replace it with spectator authority at the same absolute session expiry, never extending it. Failure preserves original role, seat and recovery proof.

##### Request

Path: `game_id: GameId`. Cookie: valid existing player session. No target identity or recovery answer.

Command metadata: `command_id: CommandId`; transport/placement is **TBD**, not assigned to the body here. No body fields are specified; exact response/confirmation representation remains **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The player alias/seat are released and the recovery verifier is deleted.

Cookie effect: replace/deauthorize player authority for the new spectator binding at the existing session's fixed absolute expiry; do not extend its lifetime. DO-055 approves the stable participant binding/access model; physical fencing, token/cookie transport and retry mechanics remain TBD, with DO-056 covering terminal record/Exit rules.

Failure candidates: non-lobby state or no spectator capacity. Rejection preserves the original player membership/session and verifier.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `membership` | `MembershipView` | Y | New spectator membership. |
| `game` | `GameView` | Y | Audience-only game view. |

#### D9: `switch_to_player` (Mutating)

Path: `POST /api/games/{game_id}/membership/switch-to-player`

Auth scope: Anyone

DO-047: a valid existing spectator session is required. Before start, claim an eligible player seat/alias, create a fresh PlayerRecord/PlayerId, release spectator occupancy and optionally enroll a fresh answer. Do not restore deleted credentials or permit an unauthenticated membership takeover.

##### Request

Path: `game_id: GameId`. Cookie: valid existing spectator session. The switch is before start; it does not permit unauthenticated membership takeover.

Proposed command metadata: `command_id: CommandId`; transport/placement remains **TBD**, not assigned to the body here. No old/deleted player session or answer is restored. Replace role-bound session authority at the original fixed absolute expiry without extension (DO-047).

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `alias` | `String` | Y | Requested player alias. |
| `recovery_answer` | `String` | N | Optional fresh sensitive recovery answer in the HTTPS body; this enrolls fresh proof rather than restoring a deleted answer. |

##### Response

No board before start.

Cookie effect: bind authority to the fresh player membership and retire spectator authority/occupancy at the original fixed expiry without extension (DO-047).

Failure candidates: no player slot, invalid/claimed alias or non-lobby state. Preserve spectator role/seat/session on rejection. DO-055 approves the participant-session fields/access rules; physical fencing/retry details remain TBD, and DO-056 covers terminal record/Exit rules.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `membership` | `MembershipView` | Y | New player membership. |
| `game` | `GameView` | Y | Lobby game view. |

#### D10: `leave_player_lobby` (Mutating)

Path: `POST /api/games/{game_id}/membership/leave`

Auth scope: Player

DO-047: player Leave before start removes membership, recovery proof and session authority, releases seat/alias, and removes that membership from the start roster. Returning requires fresh eligible admission. Dispatch this branch using server-verified role and lifecycle.

##### Request

Path: `game_id: GameId`. Cookie: caller's valid player session. Proposed command metadata: `command_id: CommandId`; transport/placement remains **TBD**, not assigned to the body here. No arbitrary member ID; no body fields are specified.

The server verifies the pre-start branch using role and lifecycle. Optional lifecycle/version binding to prevent a delayed lobby Leave changing meaning after Start: **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The proposed acknowledgment body below or an empty success may be returned; the choice is **TBD**.

Cookie effect: clear/deauthorize removed membership credentials; release seat/alias, delete recovery material and remove the membership from the start roster. Returning requires fresh eligible admission (DO-047).

Repeat/lost-response handling and Start/Leave conflict contract **TBD**; no board or new session is returned.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `left` | `bool` | N | Proposed Leave acknowledgment; conditional on the acknowledgment-body alternative rather than empty success (**TBD**). |
| `membership_retained` | `bool` | N | False for lobby Leave; conditional on the acknowledgment-body alternative rather than empty success (**TBD**). |
| `fresh_admission_required` | `bool` | N | True; conditional on the acknowledgment-body alternative rather than empty success (**TBD**). |

#### D11: `leave_player_game` (Mutating)

Path: `POST /api/games/{game_id}/membership/leave`

Auth scope: Player

DO-047: player Leave during play retains membership, seat, board, automatic matching and award eligibility. A still-valid session may return until its original fixed expiry; after expiry, approved answer-based recovery may restore the same membership. Do not extend session expiry. DO-048: `last_explicit_leave_at` records the latest explicit Leave event and remains set after return; it does not indicate current presence. Dispatch this branch using server-verified role and lifecycle.

##### Request

Path: `game_id: GameId`. Cookie: caller's valid player session. Proposed command metadata: `command_id: CommandId`; transport/placement remains **TBD**, not assigned to the body here. No target participant field; no body fields are specified.

The server selects the In Progress branch using verified role and lifecycle; the client cannot request pre-start deletion of a running-game member.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The fields below describe a potential response body; exact body/status **TBD**.

Retain membership, board, alias/seat, automatic matching and award eligibility. Preserve return through the still-valid fixed-expiry session or enrolled-answer proof; this is not logout or terminal Exit (DO-047).

Socket detachment/fencing, repeat Leave and terminal race mechanics **TBD**; approved policy retains session eligibility only through its original expiry (DO-047).

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `left` | `bool` | N | Proposed Leave acknowledgment in the potential body; exact body/status and field inclusion **TBD**. |
| `membership_retained` | `bool` | N | True for Leave during play; proposed potential-body field, exact body/status and field inclusion **TBD**. |
| `player_id` | `PlayerId` | N | Retained player identity; proposed potential-body field, exact body/status and field inclusion **TBD**. |
| `session_expires_at` | `Timestamp` | N | Unchanged session expiry; proposed potential-body field, exact body/status and field inclusion **TBD**. |

#### D12: `leave_spectator_game` (Mutating)

Path: `POST /api/games/{game_id}/membership/leave`

Auth scope: Anyone

DO-047: a valid spectator session is required. Immediately release that spectator’s occupancy and revoke/delete its session data. Explicit Leave does not receive accidental-disconnect grace; later entry requires fresh admission. Dispatch using server-verified role and lifecycle.

##### Request

Path: `game_id: GameId`. Cookie: valid spectator session. Proposed command metadata: `command_id: CommandId`; transport/placement remains **TBD**, not assigned to the body here. No caller-selected spectator ID; no body fields are specified.

The server selects the spectator branch using verified role and lifecycle. No explicit-Leave grace request: explicit Leave does not receive accidental-disconnect grace.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

The proposed acknowledgment body below or an empty success may be returned; the choice is **TBD**.

Cookie effect: clear/deauthorize spectator credentials; delete spectator identity/session data and immediately release occupancy. No retained History identity.

Idempotent repeat handling after credential deletion and lost-response policy **TBD**; do not require a surviving session to perform local UI exit.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `left` | `bool` | N | Proposed Leave acknowledgment; conditional on the acknowledgment-body alternative rather than empty success (**TBD**). |
| `membership_retained` | `bool` | N | False for spectator Leave; conditional on the acknowledgment-body alternative rather than empty success (**TBD**). |

#### D13: `recover_player_session` (Mutating)

Path: `POST /api/player-recovery`

Auth scope: Anyone

Verify submitted code, current alias and enrolled answer; replace old sessions/sockets and restore the same nonterminal membership/board without another seat. No existing player session is required, but the approved recovery proof is mandatory.

##### Request

No existing session is required, but approved recovery proof is mandatory. Normalize the game code by HLD-056; trim the submitted alias under HLD-051 and require exact case-sensitive spelling under DO-045. DO-050 approves answer normalization version 1: Unicode NFC, trim outer Unicode whitespace, then Unicode case-fold; preserve internal whitespace and punctuation. Select normalization by the stored record version. DO-051 approves Argon2id v19, fresh independent 16-byte CSPRNG salt, 32-byte output and PHC-encoded verifier; 19 MiB / 2 iterations / parallelism 1 is only a benchmark starting point, with production costs/caps pending target measurements. DO-052 approves verification outside the write transaction followed by commit-time revalidation of lifecycle, membership/current alias and unchanged verifier/version; success advances `session_epoch`, invalidates predecessor sessions, and issues a fresh fixed one-day session. Old sockets fail authorization at the next frame; close completion requires acknowledgement. A committed-but-lost response requires fresh answer proof; session secrets are never replayed. Rate-limit thresholds/durable implementation and exact Unicode library details remain **TBD**.

No arbitrary player/account/board replacement input.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_code` | `String` | Y | Game code submitted for recovery; normalize by confirmed rules. |
| `alias` | `String` | Y | Current player alias; outer-space trim under HLD-051, then exact case-sensitive comparison with the current retained membership (DO-045 approved). |
| `recovery_answer` | `String` | Y | Required sensitive proof: the enrolled recovery answer. Apply normalization profile selected by the stored `normalization_version`; version 1 is Unicode NFC → outer Unicode-whitespace trim → case-fold, preserving internal whitespace/punctuation (DO-050). |

##### Response

Return the body only after successful proof, restoring the same nonterminal membership and any existing board.

Cookie effect: after atomic recovery commit, issue a fresh fixed one-day player session at the incremented `session_epoch`; predecessor sessions become invalid atomically, with socket close work asynchronous and tracked to closed/already-absent acknowledgement. No new membership/seat/board.

Invalid code/alias/answer, absent enrollment and terminal game produce safe failure without answer/account enumeration; committed-but-lost response recovery requires fresh answer proof and never replays a session secret. Exact errors remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `membership` | `MembershipView` | Y | Existing player membership restored after successful proof. |
| `game` | `GameView` | Y | Role-filtered game view. |
| `board` | `Option<BoardView>` | N | The same board, only if already assigned; no new board is created. Nullable versus absent representation **TBD**. |

#### D14: `manage_my_recovery_answer` (Mutating)

Path: `PUT /api/games/{game_id}/membership/recovery-answer` or `DELETE /api/games/{game_id}/membership/recovery-answer`

Auth scope: Player

Set/replace the caller’s answer using PUT or delete it using DELETE in Awaiting Players/In Progress. Deletion disables answer recovery. A lost-session caller cannot use this operation to establish ownership. DO-052 approves atomic serialization with concurrent recovery: revalidate the verifier/version before recovery commit; an answer update that wins first makes the stale proof fail closed. Answer management does not rotate or extend the caller’s current session.

##### Request

Path: `game_id: GameId`. Cookie: valid owner-player session. PUT sets/replaces the answer; DELETE has no answer body. This operation applies in Awaiting Players/In Progress.

Proposed command metadata: `command_id: CommandId`; method-specific validation/transport **TBD**, not assigned to the body here. The existing valid session establishes ownership, not a submitted old answer; a lost-session caller cannot use this operation to establish ownership.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `recovery_answer` | `String` | N | Sensitive new/replacement answer required for PUT; apply the existing record's normalization profile when replacing and version 1 for a new record (DO-050: Unicode NFC → outer Unicode-whitespace trim → case-fold, preserving internal whitespace/punctuation). Not supplied for DELETE. Conditional across methods, not optional when setting/replacing the answer. |

##### Response

The proposed body below has an empty-success alternative (**TBD**).

Never return the plaintext answer, normalized answer or verifier. Replacement invalidates the previous proof; deletion disables answer recovery without deleting membership.

Failure candidates: invalid session, unsupported answer or terminal game. Do not disclose this private setting in host/spectator updates.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `recovery_enabled` | `bool` | N | True after accepted PUT, false after DELETE; conditional on returning the proposed body rather than empty success (**TBD**). |
| `view_revision` | `Revision` | N | Possibly included in the proposed response body; inclusion and the empty-success alternative are **TBD**. |

### E. Live-game reads and commands

#### E1: `get_game_calls` (Non-mutating)

Path: `GET /api/games/{game_id}/calls`

Auth scope: Anyone

Return the most recent value and committed call sequence to authorized game viewers, including admitted spectators. A valid account or participant access context is required; knowing the game ID alone is insufficient.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying a valid authorized account or participant session. No request body. Knowing the game ID alone is insufficient.

Call-range/pagination filters, if needed, are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed response for authorized game viewers, including admitted spectators. No raw `CallRecord` with internal actor/command metadata is implied. Terminal readers still obey the final-view/History boundary.

Exact query limits, authorized projection and errors are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `calls` | `Vec<CallView>` | Y | Committed calls in sequence order; empty before the first call. |
| `latest` | `Option<CallView>` | N | Most recent committed call, if any; empty before the first call. Nullability versus absence remains TBD. |
| `view_revision` | `Revision` | Y | Revision of the returned call view. |

#### E2: `get_remaining_values` (Non-mutating)

Path: `GET /api/games/{game_id}/remaining-values`

Auth scope: Host

Derive/read the remaining pool and exhaustion state from configuration and accepted calls for the host/admin operating or inspection view. Audience-facing exhaustion status can be included in the permitted game snapshot without exposing this full query.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying an enrolled host/admin session. No request body.

Optional page/cursor controls for large pools are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed host/admin operating or inspection response derived/read from configuration and accepted calls. Values remain strings; enumeration versus paged results and practical limits are **TBD**. Exhaustion does not terminate the game.

Participants cannot access this full operating query; audience-facing exhaustion status can appear in the permitted game snapshot without exposing the full query. Failures include invalid authority or unavailable game.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `remaining_values` | `Vec<String>` | Y | Remaining pool values as strings; enumeration versus paging and practical limits TBD. |
| `remaining_count` | `u32` | Y | Count of remaining pool values. |
| `exhausted` | `bool` | Y | Whether the remaining pool is exhausted; exhaustion does not terminate the game. |
| `view_revision` | `Revision` | Y | Revision of the returned remaining-pool view. |

#### E3: `get_my_board` (Non-mutating)

Path: `GET /api/games/{game_id}/membership/board`

Auth scope: Player

Read the caller’s assigned layout, free cells, automatic matches and qualification. No board exists before start.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying a valid owner-player session. No player ID selector or request body.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed response is restricted to the caller’s layout, free cells, automatic matches and qualification. No assigned board exists before accepted Start, and this read does not generate a fresh replacement.

Pre-start null/absent response versus explicit unassigned status is **TBD**. Final-view-only sessions use their retained access rules; invalid/exited/expired-session failures are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `board` | `Option<BoardView>` | N | Caller’s assigned layout, free cells, automatic matches and qualification, when assigned. No assigned board before Start; not a freshly generated replacement. Pre-start null/absence versus explicit unassigned status TBD. |
| `view_revision` | `Revision` | Y | Revision of the returned board view. |

#### E4: `get_game_boards` (Non-mutating)

Path: `GET /api/games/{game_id}/boards`

Auth scope: Host

Inspect all or selected player boards read-only. Selection/filter details remain TBD; designated-host ownership is not required for inspection.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying an enrolled host/admin session. No request body.

Possible query `player_ids: Vec<PlayerId>` for selection, plus optional paging; all-versus-selected/default and encoding are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed read-only inspection response. No board creation for pre-start members. Unknown selection and pagination/unassigned result format are **TBD**.

Read-only access does not require designated-host ownership, but grants no mutation or anonymous-player recovery privileges.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `boards` | `Vec<BoardView>` | Y | All or selected player boards; permitted player identity/alias accompanies each board. Selection/default, pagination and unassigned result format TBD. |
| `view_revision` | `Revision` | Y | Revision of the returned board inspection view. |

#### E5: `get_game_qualification` (Non-mutating)

Path: `GET /api/games/{game_id}/qualification`

Auth scope: Host, Player

Hosts/admins receive qualifying players; players receive only their own qualification. Spectators receive no private qualifier details.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying an authorized host/admin or owner-player session. No request body.

The server derives the projection from authority; no client role flag or other-player selector for a player.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed authority-dependent bodies: hosts/admins receive qualifying players; players receive only their own qualification. Branch-only fields below are conditional, not freely omittable within their applicable proposed branch. Both projections may include `view_revision: Revision`; the final envelope/discriminator is **TBD**.

No spectator private qualifier details, no automatic award and no exclusion merely because a qualifying player is disconnected/departed.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `qualifying_players` | `Vec<object>` | N | Required in the proposed host/admin branch only. Entries contain `player_id: PlayerId`, `alias: String` and `qualifying_lines: Vec<CompletedLine>`. Disconnected/departed qualifying players are not excluded merely for absence. |
| `qualified` | `bool` | N | Required in the proposed player branch only; whether the caller qualifies. |
| `qualifying_lines` | `Vec<CompletedLine>` | N | Required in the proposed player branch only; the caller’s own qualifying lines, not another player’s details. |
| `view_revision` | `Revision` | N | Optional revision of the returned view; either authority-dependent projection may include it. |

#### E6: `start_game` (Mutating)

Path: `POST /api/games/{game_id}/start`

Auth scope: Host

The designated host or an authorized admin validates the connected-player minimum, that this game still owns the single global nonterminal-game reservation, and DO-044's exact distinct-layout feasibility against every retained start-time player membership; it transitions this same game to In Progress and persists boards/initial qualification for the retained start roster.

##### Request

**Approved GAME-SLICE-02:** Path `game_id: GameId`; existing account cookie carrying a current enabled Verified Normal designated-host/admin session, exact application Origin and canonical UUID-v7 `Idempotency-Key`. The JSON body requires `expected_revision` for the authorized host projection. The command ID belongs only in the header. No other request-body fields.

No caller-supplied roster, boards, random seed or claimed connection count. The backend chooses the retained start roster and checks currently connected eligible players, this game’s ownership of the same global reservation and board feasibility.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `expected_revision` | `Revision` | Y | Expected current host-projection revision, checked for a fresh Start. Exact receipt retries do not regenerate boards or repeat Start. |

##### Response

**Approved GAME-SLICE-02:** Fresh accepted Start is HTTP 200 with the documented game ID/state/start time/view revision and a secret-free receipt; optional full game view is omitted in this slice because authorized WSS/sync snapshots deliver boards. Exact authorized retries return only the receipt, without board regeneration. Lifecycle/reservation/revision/insufficient-player conflicts are 409 and unavailable dependencies are 503. Reuse the existing no-store JSON/error envelope.

Committed retries return the original start/assignments. No boards before accepted Start, and no automatic winner for free-cell qualification.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Identifier of the started game. |
| `state` | `GameState` | Y | In Progress after accepted Start. |
| `started_at` | `Timestamp` | Y | Accepted game-start timestamp. |
| `view_revision` | `Revision` | Y | Revision of the committed started-game view. |
| `game` | `GameView` | N | Optional host game view with assigned boards/initial qualification for the retained start roster. |

#### E7: `call_random_value` (Mutating)

Path: `POST /api/games/{game_id}/calls/random`

Auth scope: Host

The designated host or an authorized admin requests a random undrawn value. Persist the call, matching board changes, qualification, revision and command outcome consistently before delivery.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying a designated-host/admin session. Proposed metadata `command_id: CommandId` and optional `expected_revision: Revision`; transport/placement and precise requiredness are **TBD**, not assigned to the body here. No named request-body fields are specified.

No selected value, board changes or qualification supplied by the client.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed response may additionally contain optional host-visible qualification/board changes; grouping is **TBD**, with no field names specified.

DO-066 approves this call, matching board changes, qualification, affected view revisions and accepted-call receipt in one owning-Game-Object transaction; authorized WSS follows commit. Retry uses the same committed value/result without new writes, never another random draw.

Failure candidates: exhausted pool, non-In-Progress state, stale/conflicting command or denied authority. Exhaustion does not automatically end the game.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `call` | `CallView` | Y | Committed random call of an undrawn value; retries return the same committed value. |
| `remaining_count` | `u32` | Y | Remaining pool count after the committed call. |
| `exhausted` | `bool` | Y | Whether the pool is exhausted after the committed call; does not imply automatic game termination. |
| `view_revision` | `Revision` | Y | Revision of the committed progression view. |

#### E8: `call_manual_value` (Mutating)

Path: `POST /api/games/{game_id}/calls/manual`

Auth scope: Host

The designated host or an authorized admin submits a string value. Validate pool membership/nonduplication and perform the same durable progression as a random call under DO-066’s atomic owning-Game-Object write set; authorized WSS follows commit.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying a designated-host/admin session. Proposed metadata `command_id: CommandId` and optional `expected_revision: Revision`; transport/placement and precise requiredness are **TBD**, not assigned to the body here.

No client-authored board matches or qualification. The backend validates pool membership/nonduplication and performs the same durable progression as a random call.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `value` | `String` | Y | Required manual string value; must belong to the pool and not already have been drawn. Exact string validation TBD. |

##### Response

Proposed response may additionally contain optional authorized progression changes as for E7; grouping/field names are **TBD**.

Failure candidates: value outside the pool/already drawn, wrong lifecycle, stale command or denied authority. No partial matching on rejected calls.

Exact string validation/error fields and durable-result envelope are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `call` | `CallView` | Y | Committed manual call; uses the same durable progression as a random call. |
| `remaining_count` | `u32` | Y | Remaining pool count after the committed call. |
| `exhausted` | `bool` | Y | Whether the pool is exhausted after the committed call. |
| `view_revision` | `Revision` | Y | Revision of the committed progression view. |

#### E9: `award_game_winner` (Mutating)

Path: `POST /api/games/{game_id}/winner`

Auth scope: Host

For the designated host or an authorized admin, revalidate the selected member’s qualification and commit one winner with Resolved. Retained disconnected/departed players remain eligible.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying a designated-host/admin session. Proposed metadata `command_id: CommandId` and optional `expected_revision: Revision`; transport/placement and precise requiredness are **TBD**, not assigned to the body here.

The backend revalidates qualification; no client-asserted winner proof, board or timing priority. Retained disconnected/departed players remain eligible.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `player_id` | `PlayerId` | Y | Selected winner’s member identifier; backend revalidates qualification before committing one winner with Resolved. |

##### Response

Proposed winner-award response. Failure candidates: unknown/nonqualifying member, already terminal state, command conflict or denied authority. A retained departed member is not rejected merely for absence.

Terminal receipt/release coordination and exact error/body schema are **TBD**; retries cannot award another winner. Terminal and History-expiry timestamps accompany the final result; their exact nesting remains **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `result` | `FinalResultView` | Y | Final result with Resolved and one `winner`, fixed `ended_at: Timestamp` and `history_expires_at: Timestamp`; exact body schema/nesting TBD. |
| `view_revision` | `Revision` | Y | Revision of the committed final-result view. |

#### E10: `cancel_unstarted_game` (Mutating)

Path: `POST /api/games/{game_id}/cancel`

Auth scope: Host

With designated-host/admin confirmation, commit Cancelled from New/Awaiting Players, then apply notice and no-History deletion rules. Generate no boards.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying a designated-host/admin session. Proposed `command_id: CommandId`, `expected_state: GameState` (New/Awaiting Players) or equivalent revision/confirmation binding; precise contract is **TBD**. Their body/header transport remains **TBD**, so they are not listed as body fields.

Confirmation must bind to the displayed game and pre-start lifecycle. A delayed pre-start confirmation must not silently select the In Progress cancellation branch after Start.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `confirmed` | `bool` | N | Proposed body confirmation (`true`) bound to the displayed game and New/Awaiting Players lifecycle. Exact confirmation contract/field requiredness TBD; the confirmation obligation is not optional. |

##### Response

Proposed potential cancellation body. Commit Cancelled from New/Awaiting Players, then apply notice and no-History deletion rules. Generate no boards and provide no History or renewed participant access. Notice and cleanup do not wait for every viewer’s Exit.

State/confirmation conflicts, deletion completion versus acceptance and post-delete retry response are **TBD**; do not fabricate a retained final snapshot.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Identifier of the cancelled game in the proposed cancellation body; exact acceptance/completion and retry response TBD. |
| `state` | `GameState` | Y | Cancelled in the proposed cancellation body. |
| `history_available` | `bool` | Y | False: pre-start cancellation creates no History. |
| `deletion_pending` | `bool` | N | Conditional field if cleanup is asynchronous; indicates pending deletion. Deletion completion versus acceptance TBD. |

#### E11: `end_game_without_winner` (Mutating)

Path: `POST /api/games/{game_id}/cancel`

Auth scope: Host

With designated-host/admin confirmation, commit Cancelled from In Progress, retain final History and safely release the live reservation. The current game state determines this branch rather than the pre-start cancellation branch.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying a designated-host/admin session. Proposed `command_id: CommandId`, `expected_state: GameState` (InProgress) or equivalent version binding; encoding is **TBD**. Their body/header transport remains **TBD**, so they are not listed as body fields.

No winner input. The current game state determines the In Progress cancellation branch rather than the pre-start cancellation branch; confirmation is bound to the In Progress game.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `confirmed` | `bool` | N | Proposed body confirmation bound to the In Progress game. Exact encoding/field requiredness TBD; the confirmation obligation is not optional. |

##### Response

Proposed response for ending a started game without a winner. Final started-game History is retained; release only this game’s live reservation. Existing viewer Exit does not control completion.

Failure candidates: confirmation/state/authority conflict or terminal replay mismatch. Exact response/coordination details are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `result` | `FinalResultView` | Y | Final result with Cancelled and `winner: null`, with fixed terminal/History expiry timestamps. Exact response schema TBD. |
| `view_revision` | `Revision` | Y | Revision of the committed final-result view. |

### F. Final views and History

#### F1: `get_game_result` (Non-mutating)

Path: `GET /api/games/{game_id}/result`

Auth scope: Host, Player

Return only a permitted final view. Players use the existing `TerminalPlayerViewRecord` grant (DO-056), with original expiry and no post-Exit recreation; hosts/admins use existing eligible account-view authorization under DO-058’s approved downgrade and account/game-wide Exit across sessions, bounded by account-session/History expiry. Spectators cannot fetch/reconnect after terminal cleanup.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying eligible existing, unexpired and not-exited player/account final-view authority.

No request body, new admission proof or spectator reconnect request. An account login alone must not recreate an exited live-game view. Hosts/admins must satisfy final-view access rules and use History after exit; spectators cannot fetch/reconnect after terminal cleanup.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed permitted final-view response. No new session, verifier or retention extension. Spectators retain only the result already delivered locally.

Pre-start deletion, exited/expired access and History redirection for eligible accounts: exact response/status are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `result` | `FinalResultView` | Y | Permitted final view: a player receives only their own final board, not other private player boards; a host/admin receives only permitted final-view content. |

#### F2: `exit_game_result` (Mutating)

Path: `POST /api/games/{game_id}/exit`

Auth scope: Anyone

Requires existing final-view authorization when server-side cleanup is needed. For players, atomically delete all terminal grants for that stable player ID and close their connections, without changing outcomes or other users. Account-view Exit follows DO-058: delete all grants for that account/game across sessions without logging out or removing History permission. Spectator local-only Exit requires no request after server data deletion.

##### Request

Proposed request when server cleanup is needed: path `game_id: GameId`; cookie carrying existing final-view authority. Proposed `command_id: CommandId` metadata; transport/placement and requiredness are **TBD**, not assigned to the body here. No named request-body fields are specified.

No target player/account ID. Spectator local-only Exit after deletion and pre-start-deleted views need no surviving server session/request.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed response may contain `exited: bool` or be an empty success; the choice is **TBD**. Player Exit removes all terminal grants for that player; account-view cleanup follows DO-058 across sessions. Cookie effects and response shape remain TBD.

Account authentication remains valid; game result/History and other viewers remain unchanged. Exit does not wait for other viewers, and the caller cannot recreate final-view authority through resume.

Player Exit retries cannot recreate a deleted grant. DO-058 approves account/game-wide Exit across sessions; response and no-session local behavior remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `exited` | `bool` | N | Proposed Exit acknowledgment if a response body is chosen; empty success is an alternative and the exact response remains TBD. |

#### F3: `list_game_history` (Non-mutating)

Path: `GET /api/history`

Auth scope: Host

List unexpired started-game History across hosts. Exclude pre-start cancellations and expired records. Query options remain TBD.

##### Request

Proposed request: cookie carrying an enrolled host/admin session. No request body.

Possible query `cursor: Option<String>`, `limit: Option<u32>`, `outcome: Option<HistoryOutcome>` and terminal-date range; filter/date encoding/order/defaults are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed list of unexpired started-game History across hosts. Exclude pre-start cancellations and expired History even before physical cleanup. Exact summary columns and whether to use paging are **TBD**.

No exports, account credentials, spectator identities or renewed membership. Access/query errors are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
|| `games` | `Vec<object>` | Y | History summaries with proposed `game_id: GameId`, `game_code: GameCode`, terminal outcome/time, `expires_at: Timestamp` and optional winning alias; exact summary columns TBD. Deadline follows DO-071’s UTC three-calendar-month rule. Includes only unexpired started-game History across hosts. |
| `next_cursor` | `Option<String>` | N | Possible continuation cursor if paging is chosen. Paging and nullability versus absence TBD. |

#### F4: `get_game_history` (Non-mutating)

Path: `GET /api/history/{game_id}`

Auth scope: Host

Read final ordered calls, aliases, winner if any and final board snapshots. No replay archive, credentials, membership restoration or mutation.

##### Request

Proposed request: path `game_id: GameId`; cookie carrying an enrolled host/admin session. No request body. A game code/anonymous participant proof is not History authority.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed read-only final History response. No intermediate replay/archive, sessions, account credentials, recovery verifiers, spectator identities, membership restoration, renewed game access or mutation.

Absent/expired History and denied-access error distinctions are **TBD**; reads never extend the DO-071 UTC calendar deadline, which denies at `now >= expires_at`.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
|| `history` | `HistoryView` | Y | Final History follows the DO-069 snapshot fields/invariants and fixed DO-071 expiry; the DTO remains a safe projection, not direct storage serialization. |

### G. Live synchronization and retries

#### G1: `connect_game_stream` (Mutating)

Path: `GET /api/games/{game_id}/stream` — WebSocket upgrade

Auth scope: Anyone

A valid authorized account or participant session is required, including for spectators. Register/supersede the appropriate connection and send a full role-specific snapshot. DO-068 makes attachment registration and snapshot/revision capture one serialized cut; the snapshot precedes later updates. This is mutating because connection/presence state changes; existing membership is not duplicated.

##### Request

Proposed WebSocket handshake inputs: path `game_id: GameId`; cookie containing a valid authorized account/participant session; WebSocket upgrade headers and browser Origin. There is no normal JSON request body.

A possible reconnect revision hint may use query/subprotocol; transport is **TBD**. No bearer token or recovery answer may appear in the URL/subprotocol.

Viewer/role and applicable session expiry are server-derived; existing credentials must satisfy lifecycle/Exit restrictions.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Proposed transport: successful WebSocket upgrade followed by a full authorized `GameView`/eligible `FinalResultView` snapshot. The table describes application-message payloads after the upgrade, not an ordinary HTTP JSON success body or handshake JSON. Frame/envelope names are **TBD**; no wrapping property is specified.

Supersede the prior participant-session socket without allocating another seat.

Handshake rejection/close-code mapping and SDK delivery remain TBD. DO-091 approves a versioned compact JSON connection attachment capped at 512 encoded bytes; reject malformed/oversized/unsupported state fail-closed. DO-092 makes attachments lookup hints only and requires durable authority/presence/session/epoch/expiry/grant/current-connection/view-revision reconstruction on wake and protected actions; invalid sockets close before snapshot. DO-093 approves prepare-then-fence socket replacement. DO-094 approves one durable earliest-deadline alarm per Object. DO-095 caps encoded frames at 256 KiB and unsent queue at 1 MiB/socket; close on pressure/failure rather than drop revisioned state, then reconnect with authorized snapshot. No spectator terminal reconnect.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `$` | `GameView / FinalResultView` | Y | Full authorized root snapshot: `GameView` or an eligible `FinalResultView`; role-specific application-message payload, with exact frame/envelope shape **TBD**. |
| `view_revision` | `Revision` | Y | Authorized-view revision accompanying the snapshot; frame/envelope placement remains **TBD**. |
| `connection_id` | `ConnectionId` | N | Potential connection metadata. Proposed; requiredness/placement TBD. |
| `session_expires_at` | `Timestamp` | N | Potential connection metadata for the server-derived session expiry. Proposed; requiredness/placement TBD. |

#### G2: `send_game_update` (Non-mutating)

Path: Existing `/api/games/{game_id}/stream` socket

Auth scope: System

Push already-committed, role-filtered changes to currently authorized recipients. This is server delivery, not a user-invoked HTTP request or gameplay mutation.

##### Request

These are proposed trusted internal delivery inputs, not a browser HTTP request or a new public endpoint. Current recipients must be authorized; the server derives projections and revalidates authority. Payload typing and fan-out signature are **TBD**.

Trusted internal delivery inputs include the owning game, committed state/change reference and current authorized recipients with their view revisions. The exact typed signature and envelope remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Owning game for delivery of already-committed changes. |
| `connection_ids` | `Vec<ConnectionId>` | N | Possible internal connection selection. Proposed; requiredness/placement TBD. |

##### Response

The table describes proposed outbound socket frames on the existing stream, not an HTTP response body or handshake JSON. This is server delivery of already-committed, role-filtered changes.

There is no gameplay write and no client acknowledgement required to establish the committed result. A queued send is not proof that the recipient received it.

DO-068 requires monotonically increasing authorized-view revisions; clients ignore duplicate/stale updates and request a fresh authorized snapshot on a detected gap. DO-095 requires commit-before-send and close/resync on pressure or delivery failure; 256 KiB encoded frame and 1 MiB unsent queue/socket caps apply. Exact frame types, close-code mapping and provider delivery guarantees remain implementation work. No credential/recovery fields, private-board leaks or new public endpoint.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `kind` | `String` | Y | Proposed outbound frame kind; exact frame types are **TBD**. |
| `game_id` | `GameId` | Y | Game associated with the committed update. |
| `view_revision` | `Revision` | Y | Revision of the recipient's authorized view. |
| `payload` | `object` | Y | Role-filtered committed call/board/state/result changes, without credential/recovery material or private-board leaks. |

#### G3: `synchronize_game_view` (Non-mutating)

Path: `GET /api/games/{game_id}/sync`

Auth scope: Anyone

Requires valid access to the requested game/view. Compare the client’s authorized-view revision and return fresh state when needed. Do not treat private updates as public gaps, renew credentials or bypass exit restrictions.

##### Request

Proposed inputs: path `game_id: GameId`; cookie providing valid caller-specific view authority; possible query `known_revision: Option<Revision>`. There is no request body.

The view is derived from the session, not a caller-chosen role/player ID. Revision hint encoding is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Only role-specific revisions are compared; an unchanged result does not renew credentials or host activity. A detected gap requires fresh authorized snapshot state under DO-068; the unchanged response representation and any non-gap optimization remain **TBD**.

Expired/exited/forbidden view restrictions and no terminal spectator retrieval remain enforced.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `up_to_date` | `bool` | Y | Proposed indication of whether the client's authorized view is current. |
| `view_revision` | `Revision` | Y | Revision of the authorized role-specific view. |
| `snapshot` | `Option<GameView>` | N | Fresh authorized snapshot when fresh state is needed, including detected revision gaps; permitted final-view alternatives, unchanged-response representation and nullability versus absence remain **TBD**. |

#### G4: `get_command_result` (Non-mutating)

Path: `GET /api/games/{game_id}/commands/{command_id}`

Auth scope: Anyone

Requires valid authority for the original actor/game-scoped command. Read its durable outcome after an interrupted response without exposing another actor’s results. Knowledge of a command ID is not authorization.

##### Request

Proposed inputs: path `game_id: GameId` and `command_id: CommandId`; cookie providing current valid authority for the original actor/game-scoped command. There is no request body.

Command ID knowledge alone grants no lookup. Post-admission/lost-cookie and post-exit/deletion recovery are **TBD**, not an anonymous receipt bypass.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

Return only a bounded authorized original result. No raw bearer credentials, answer/verifier, another actor's outcome or expired private snapshot. Receipt lookup does not prolong data/access retention.

Unknown, pending, expired/inadmissible and failed-command response variants/statuses are **TBD**. Absence of a receipt is not evidence that it is safe to repeat effects.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `outcome` | `object` | N | Bounded authorized outcome for an available original result; required in that result branch. Other response variants/statuses are **TBD**. |
| `committed_revision` | `Revision` | N | Possible metadata for an available original result. Proposed; requiredness/placement TBD. |
| `completed_at` | `Timestamp` | N | Possible completion metadata for an available original result. Proposed; requiredness/placement TBD. |

#### G5: `execute_command_idempotently` (Mutating)

Path: Original command’s method/path; no separate endpoint

Auth scope: System

Internal execution wrapper used after the original command’s own authentication and authorization checks. Return an existing committed result or safely complete the original command without duplicate calls, boards, awards or admissions. The initiating client retains the original endpoint’s scope.

##### Request

These are proposed trusted internal arguments to the original-command execution wrapper, after the original command's authentication and authorization checks. There is no separate endpoint or invented retry body/envelope; the initiating client retains the original endpoint's scope.

Derive the request fingerprint server-side. The wrapper inherits the original operation's privilege and lifecycle checks; the exact typed signature is **TBD**.

Trusted internal inputs include the authenticated/authorized actor, owning game/account context, command ID, operation kind and validated arguments. Expected revision/fence and retry-admissibility context are optional. Exact typed signature is **TBD**; no additional field names or envelope are specified.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `actor` | `ActorRef` | Y | Authenticated/authorized actor supplied within trusted internal execution, not caller-selected authority in a public request body. |
| `command_id` | `CommandId` | Y | Original command's ID in the trusted internal signature; this does not select a client transport or introduce a separate retry request. |

##### Response

This is a proposed trusted internal result, not an independent HTTP response/route or an extra side effect. Exact result type is **TBD**.

Secret-bearing issuance operations need a separate safe handoff/retry policy (**TBD**). Never store or expose a raw token/link through a secret-free receipt.

Internal result: original committed safe outcome on a matching replay, newly committed outcome on first execution, or conflict/inadmissible/authorization failure; exact result type is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `replayed` | `bool` | N | Possible indication of replay. Proposed; requiredness/placement TBD. |
| `committed_revision` | `Option<Revision>` | N | Possible committed revision. Proposed; requiredness/placement TBD. Nullability versus absence remains **TBD**. |

### H. Backend-only coordination and maintenance

#### H1: `claim_game_code` (Mutating)

Path: — Internal

Auth scope: System

Check a candidate against live mappings and unexpired History, then safely claim/publish it. Reused codes must not transfer old credentials.

##### Request

These are proposed trusted internal inputs, not a public request body. Authoritative issuance context accompanies the invocation.

Candidate generation, collision retry policy and prepared-publication state are **TBD**. No client-chosen Object creation from an unknown code.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Owning game for the candidate code claim/publication. |
| `game_code` | `GameCode` | Y | Candidate game code. |
| `operation_id` | `OperationId` | Y | Operation coordinating the claim/publication. |

##### Response

This is a proposed trusted internal result. Public discoverability is allowed only after consistent publication; terminal History reservation and old-game credential isolation remain enforced.

Exact result enum, expiry cleanup ordering and retained index data are **TBD**.

Internal result: claimed code and owning game/operation, or collision/retry/conflict. The unnamed result components do not introduce new field names; exact result enum is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `publication_state` | `PublicationState` | N | Possible publication state. Proposed; requiredness/placement TBD. |

#### H2: `coordinate_global_game_reservation` (Mutating)

Path: — Internal

Auth scope: System

Coordinate the one global nonterminal-game reservation. Claim it on New creation, retain it through Awaiting Players and In Progress, and release it only after committed Resolved/Cancelled. A stale operation must never release a newer game’s reservation.

##### Request

These are proposed trusted internal protocol arguments, not persisted reservation-record fields. DO-014 stores only nullable `game_id`; DO-015 approves claim only when NULL and release only when the stored ID matches. DO-016 approves idempotent terminal outcomes: matching ID clears, NULL is complete, and a different ID is stale/no-op. Command idempotency, exact action enum and delivery scheduling remain **TBD**. There is no public reservation API.

Internal protocol inputs may include the fields below and a desired acquire/commit/release action. No operation/fencing metadata is stored in the DO-014 record; exact signature, command idempotency and action enum remain **TBD** under DO-016.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

| `game_id` | `GameId` | Y | Game whose reservation is being coordinated. |
| `operation_id` | `OperationId` | N | Optional proposed protocol/idempotency input; not persisted in the DO-014 reservation row. Requiredness remains **TBD** under DO-016. |


##### Response

This is a proposed trusted internal result, not a public reservation API. DO-016 approves compare-and-clear only when the stored `game_id` equals the releasing game ID; already-NULL is complete and a different ID is stale/no-op. Delivery scheduling/backoff and command-receipt idempotency remain **TBD**.

Transition/result variants and nullability are **TBD**.

Internal result proposal: whether the single slot is free or occupied, and the owning game where authorized; conflict/retry/reconciliation outcomes, any protocol metadata and exact variants remain **TBD** under DO-015/016.

| Field | Type | Required | Description |
| --- | --- | --- | --- |


#### H3: `find_hosted_nonterminal_games` (Non-mutating)

Path: — Internal

Auth scope: System

While the target account’s Directory gate is held, find nonterminal games by designated-host account ID for disable/delete. A stale empty index is not sufficient to remove the account; assignment freshness/serialization remain under review. This does not expose Users to ordinary hosts.

##### Request

These are proposed trusted internal inputs, accompanied by trusted removal/assignment-check context, not public request fields or a Users listing for ordinary hosts.

Exact lookup signature, pending assignment inclusion and gate/consistency snapshot are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `account_id` | `AccountId` | Y | Designated-host account ID used to find hosted nonterminal games. |
| `operation_id` | `OperationId` | N | Optional operation/receipt context; not stored in the presence-only AccountAssignmentGate. Exact role in retries remains **TBD**. |

##### Response

This is a proposed trusted internal result, with potential pending-assignment blockers; their inclusion and exact shape remain **TBD**.

An empty stale index does not authorize removal: recheck under the assignment-gate protocol before commit.

Freshness/proof/result types are **TBD**. Ordinary hosts gain no Users listing from this helper.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `games` | `Vec<object>` | Y | Nonterminal-game entries containing `game_id: GameId`, nonterminal state and assignment revision. State and assignment-revision fields remain unnamed with their types **TBD**; no new DTO or top-level copies are introduced. |

#### H4: `update_participant_presence` (Mutating)

Path: — Internal

Auth scope: System

Maintain connected-player eligibility and spectator-disconnect state. Superseded socket closure must not release the replacement connection’s seat.

##### Request

These are proposed trusted internal presence-event arguments, not a public request body. Current epoch/connection generation and session validity must be rechecked; event enum/signature are **TBD**.

Internal inputs include the IDs below, participant binding, connect/disconnect/supersede event and trusted observation time. **Approved — DO-054:** serialize spectator presence updates in the owning GameObject using trusted commit-time. Apply disconnect only if spectator ID, current session/epoch and connection ID still match; the first matching unexpected disconnect sets the five-minute deadline, duplicates do not restart it, and stale events no-op. A valid reconnect before the deadline rechecks identity/session and atomically clears the paired timestamps without extending the session; `now >= grace_expires_at` expires first. The exact event enum/signature and hibernation reconstruction remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Game whose participant presence is updated. |
| `session_id` | `SessionId` | Y | Participant session whose validity must be rechecked. |
| `connection_id` | `ConnectionId` | Y | Connection associated with the presence event; current epoch/connection generation must be rechecked. |

##### Response

This is a proposed trusted internal presence result. Count distinct valid-session players, not sockets. A stale closure cannot clear replacement presence or release its seat.

Projection revision remains governed by DO-067/068; DO-092 requires reconstruction from authoritative durable state after hibernation and close-before-snapshot for invalid connections. DO-094 requires earliest persisted-deadline alarm processing. Exact runtime handlers/results remain implementation work.

Internal result: updated connected eligibility/presence projection or stale-event no-op. Exact projection/result shape remains **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `grace_expires_at` | `Option<Timestamp>` | N | Spectator grace deadline in the applicable presence-update branch, not a player deadline or a required stale-event result. DO-053 approves the nullable stored field; internal result serialization (null versus omitted) remains **TBD**. |

#### H5: `expire_spectator_grace` (Mutating)

Path: — Internal

Auth scope: System

At `now >= grace_expires_at`, re-read the current spectator, session/epoch, presence and expected deadline; release occupancy and delete associated spectator/session data only if it is still disconnected and the scheduled deadline still matches. If reconnect committed first, the stale expiry is a no-op. DO-054 approves expiry-winning equality; exact scheduler payload/implementation remains TBD.

##### Request

These are proposed trusted internal scheduled inputs, not a public request body. Read current presence/deadline before deleting; expire only if `now >=` the expected deadline and the spectator is still disconnected. A stale/reconnected deadline does not release capacity. Alarm payload/signature are **TBD**.

DO-094 approves one durable earliest-deadline alarm per Object across persisted domain deadlines and pending-work `next_attempt_at`; on wake, process bounded due work using trusted time, reread authority and schedule the next earliest deadline. Exact platform payload/signature and SDK handler wiring remain TBD; unnamed arguments do not acquire invented fields.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Game containing the spectator occupancy. |
| `spectator_id` | `SpectatorId` | Y | Spectator whose grace deadline and current presence must be checked. |
| `grace_expires_at` | `Timestamp` | Y | Expected five-minute grace deadline; current deadline and presence must be read before deletion. |

##### Response

This is a proposed trusted internal result, with no board/History entry or public response. Cleanup completes only for the matching still-disconnected spectator; reconnect-first makes the scheduled event a no-op. DO-054 approves this ordering, not the physical close/scheduler integration.

Exact outcome enum, credential/socket cleanup and scheduling errors are **TBD**.

Internal result: expired seat/identity/session removed, not-yet-due reschedule, or stale/reconnected no-op; exact outcome enum is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `next_due_at` | `Option<Timestamp>` | N | Possible next scheduled deadline. Proposed; requiredness/placement TBD. Nullability versus absence remains **TBD**. |

#### H6: `expire_or_revoke_credentials` (Mutating)

Path: — Internal

Auth scope: System

Enforce credential expiry/revocation, stop unauthorized socket delivery and clean up artifacts. Expiry alone does not delete retained player membership.

##### Request

These are proposed trusted internal arguments, not public request fields. Use typed references rather than raw passwords/tokens. DO-036 fixes the per-subscription revocation target and epoch-fencing requirements; batch limits and invocation form remain **TBD**.

Internal inputs: target account/session/link/member scope, expiry or revocation reason, trusted time and expected epoch/deadline where needed. For account revocation, DO-036 atomically persists work for each affected DO-035 subscription and uses the current epoch. Expected epoch/deadline is conditional; batch limits, exact signature, field names, reference types and invocation form remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

##### Response

This is a proposed trusted internal result. Pending propagation must remain distinguishable from completed work; exact bounded status is **TBD**.

No retained player-membership deletion solely from token expiry and no secret material in results/logs. DO-036's durable close/acknowledgement protocol applies to account sockets; do not report a connection closed until its GameObject acknowledges it closed or already absent.

The internal result distinguishes credential invalidation committed from per-connection close propagation. Report each affected target as pending or acknowledged closed/already absent; only acknowledgements certify socket closure. Keep retries durable and do not expose secrets. Exact bounded status/result shape remains **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `completed` | `bool` | N | Potential completion status. Proposed; requiredness/placement TBD. |
| `remaining_work` | `bool` | N | Potential indication of remaining work, including incomplete propagation. Proposed; requiredness/placement TBD. |
| `next_due_at` | `Option<Timestamp>` | N | Potential next scheduled deadline. Proposed; requiredness/placement TBD. Nullability versus absence remains **TBD**. |

#### H7: `cancel_idle_unstarted_game` (Mutating)

Path: — Internal

Auth scope: System

After rechecking state/deadline, cancel an unstarted game following 24 hours of qualifying designated-host inactivity under DO-040/041. At the serialized transaction point, trusted `now >= idle_cancel_due_at` means expiry wins, including equality; commit Cancelled/HostIdleTimeout, delete pre-start game/participant data without History, then compare-by-game-ID release the same global reservation. An early/stale alarm rereads/reschedules; it cannot cancel In Progress or release a newer reservation. Alarm scheduling and cross-Object recovery remain TBD.

##### Request

These are proposed trusted internal scheduled inputs, not a public request body. Under DO-041, compare trusted transaction-point time end-exclusively to the current `idle_cancel_due_at`; re-read state/activity and reschedule early/stale alarms. Expiry wins at or after due; DO-040 excludes non-designated-admin override actions from timer renewal. Physical schedule/delivery remains TBD.

Internal scheduled inputs include the fields below, trusted time and activity/lifecycle revision. The latter arguments remain unnamed with types **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Unstarted game whose state and qualifying designated-host activity must be rechecked. |
| `idle_cancel_due_at` | `Timestamp` | Y | Expected inactivity-cancellation deadline, subject to state/activity recheck. |

##### Response

This is a proposed trusted internal result. Release the matching global nonterminal-game reservation after pre-start Cancelled/deletion is durably coordinated; never cancel In Progress or create History.

Outcome enum, race handling, scheduling and cleanup completion are **TBD**.

Internal result: committed pre-start Cancelled with cleanup/release work, not due, already started/terminal, or superseded-deadline no-op. Exact outcome enum is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

#### H8: `purge_cancelled_unstarted_game` (Mutating)

Path: — Internal

Auth scope: System

Delete committed pre-start-cancelled game/participant data and mappings without History or waiting for viewer acknowledgement. Preserve only justified retry protection.

##### Request

These are proposed trusted internal purge arguments, not a public request body. Validate that no accepted Start occurred; source-of-proof and purge sequencing are **TBD**.

Internal inputs include the IDs below, durable pre-start cancellation proof and reservation/publication fences. Proof and fence argument names/types remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Game whose committed pre-start cancellation must be proved before purge. |
| `operation_id` | `OperationId` | Y | Operation coordinating the safe purge. |

##### Response

This is a proposed trusted internal result. No History/board snapshot, no waiting for viewer acknowledgement and no credential-bearing tombstone. Retain only justified bounded retry protection.

Exact receipt, purge progress/errors and anti-resurrection mechanism are **TBD**.

Internal result: game/participant data and applicable mappings removed, already-cleaned no-op, or pending safe reconciliation; exact receipt/result shape is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

#### H9: `finalize_game_retention` (Mutating)

Path: — Internal

Auth scope: System

Preserve started-game final History; delete recovery verifiers and spectator server data; retain only permitted minimal player final-view authorization.

##### Request

These are proposed trusted internal retention arguments, not a public request body. The original terminal timestamp determines retention; the caller cannot supply a fresh terminal date to extend expiry.

Snapshot and notice inputs/signature are **TBD**.

Internal inputs include the game ID below, committed terminal outcome/revision and started-game final state reference. Exact signature and unnamed argument types remain **TBD**; retention uses the original terminal timestamp.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Started game whose committed terminal outcome and final state are being retained. |

##### Response

This is a proposed trusted internal result, not a public History export. Delete recovery/spectator material and retain only eligible minimal existing access; connected spectator terminal notice/deletion sequencing still applies.

Materialization/coordination completion, replay no-op and cleanup failure variants are **TBD**.

Internal result: immutable `GameHistorySnapshot` reference and fixed History expiry, plus cleanup/final-view-grant progress. The reference and progress components remain unnamed, not an embedded public snapshot; exact result variants are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
|| `history_expires_at` | `Timestamp` | Y | Fixed expiry computed once under DO-071 from the first terminal timestamp by UTC calendar-month addition with month-end clamping; never extended by replay or a caller-supplied date. Exact replay no-op/cleanup failure variants are **TBD**. |

#### H10: `purge_expired_history` (Mutating)

Path: — Internal

Auth scope: System

Enforce DO-071’s fixed UTC calendar-month deadline at `now >= expires_at`; deny History/code reuse eligibility at expiry even if purge is delayed. DO-074 requires durable idempotent primary purge and Directory compare-by-game_id clearing; DO-075 prevents secondary-copy/index/log retention or restore resurrection. Never renew expiry.

##### Request

These are proposed trusted internal scheduled inputs for the three-calendar-month retention deadline, not a public request body. Enumerate associated indexes/copies through trusted owners; no caller-selected retention extension.

DO-074 requires stable identity/deadline checks and Directory compare-by-game_id cleanup so stale purge cannot erase a reused code; DO-075 requires expiry checks before restore exposure. Batch selection mechanics remain implementation work.

Internal scheduled inputs include the fields below, trusted time and original History identity/version. Unnamed argument types and exact signature remain **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `game_id` | `GameId` | Y | Game whose expired History and related indexes/copies are subject to purge. |
|| `history_expires_at` | `Timestamp` | Y | Expected DO-071 UTC calendar expiry deadline; not a caller-selected retention extension. |

##### Response

This is a proposed trusted internal result. Never return deleted calls/aliases/boards in purge results. Deny expired reads even while physical cleanup is incomplete.

DO-074/075 approve primary and secondary expiry semantics; physical cross-owner cleanup and restore-verification mechanisms remain implementation work.

Internal result distinguishes expired purge complete, not-yet-due/stale safe no-op, or pending retry. DO-074/075 require no false completion and no expired-data resurrection; bounded deletion-count/progress field names remain TBD.

| Field | Type | Required | Description |
| --- | --- | --- | --- |

#### H11: `reconcile_pending_operation` (Mutating)

Path: — Internal

Auth scope: System

Reconcile interrupted directory/game/account operations without duplicate effects, invalid publication, incorrect reservation release or resurrected data.

##### Request

These are proposed trusted internal reconciliation arguments. Read trusted persisted work rather than accepting arbitrary caller payload/phase; the signature is **TBD**.

Internal inputs include the operation ID below, owning store/context, persisted `PendingOperation` reference and current peer revisions/fences. The reference is trusted persisted work, not an arbitrary caller-supplied record; unnamed components retain their descriptions and signature is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `operation_id` | `OperationId` | Y | Interrupted operation whose trusted persisted work is reconciled. |

##### Response

This is a proposed trusted internal result. No duplicate domain effects, stale reservation release, invalid publication or resurrected records. Exact recovery action/result variants are **TBD**.

Outcomes remain bounded and secret-free; no new user endpoint.

Internal result: completed, safely resumed, conflicted or retry-needed operation; exact recovery action/result variants are **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `phase` | `CoordinationPhase` | N | Possible coordination phase. Proposed; requiredness/placement TBD. |
| `next_attempt_at` | `Option<Timestamp>` | N | Possible next retry time. Proposed; requiredness/placement TBD. Nullability versus absence remains **TBD**. |

#### H12: `record_admin_action` (Mutating)

Path: — Internal

Auth scope: System

Record actual admin actor, target, operation and verified outcome without secrets. DO-088–090 approve owner-local audit records, account/game ownership, `AdminActorRef::{Account, DeveloperCli}`, authenticated admin success/reject/failure capture, 90-day retention and developer-CLI-only reads; no client audit UI/API.

##### Request

These are proposed trusted internal audit arguments, not a public request body. Potential correlation/command reference and deduplication key are **TBD**.

Never substitute the designated host for the acting admin or pass credentials/answers.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
|| `actor` | `AdminActorRef` | Y | `Account(AccountId)` or `DeveloperCli`; actual acting privileged path, never substitute designated host. |
| `operation_name` | `String` | Y | Admin operation being recorded. |
| `target` | `TargetRef` | Y | Target of the actual admin operation. |
| `outcome` | `AuditOutcome` | Y | Verified outcome of the admin operation, without secrets. |
| `occurred_at` | `Timestamp` | Y | Trusted occurrence time. |

##### Response

This is a proposed trusted internal result. Exact result/failure policy is **TBD**.

DO-090 fixes retention at 90 days from `occurred_at`; server denies after expiry, deletes copies/indexes and prevents restore resurrection. No client audit-read UI/API or hidden History archive; privileged developer-CLI read only.

Owner/storage, allowed metadata, reliable rejected/failed-action capture and purge behavior are **TBD**.

Internal result: audit ID and recorded/duplicate outcome, or recording failure. No field name is invented for the outcome; exact result/failure policy is **TBD**.

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `audit_id` | `AuditId` | N | Audit identifier in the recorded/duplicate result branch, required in that branch; recording failure is an alternative with exact policy **TBD**. |

<a id="wss-design"></a>

## 7. WSS messages, synchronization and client state

### 7.1 Message inventory

Final message grouping/names are **TBD**; rows describe required purposes, not chosen event discriminators. Section 6 G1–G3 contains potential upgrade, outbound-update and synchronization inputs/outputs, not a finalized WSS protocol.

| Purpose | Confirmed content / constraint | Envelope, fields and encoding |
| --- | --- | --- |
| Initial / recovery snapshot | Full authorized state and revision; no lobby board; player-own versus host/admin board access versus audience-only projection. | TBD |
| Committed live change | Ordered revision-aware permitted updates for lifecycle, calls, boards and qualification; no secret recovery data. | TBD |
| Final result / cancellation notice | Resolved winner or Cancelled no-winner; spectator delivery/deletion and player final-view rules differ. | TBD |
| Synchronization repair | Detect stale/gapped/obsolete views and fetch fresh authorized state; private changes must not create false public gaps. | TBD |
| Connection liveness | Hibernation-compatible heartbeat/auto-response; does not renew session TTL or prove state freshness. | TBD |
| Expired / revoked / forbidden connection | DO-092 requires close before snapshot when reconstructed authority is invalid/uncertain; DO-036 requires fail-closed frame authorization. Close-code and client feedback mapping remain implementation details. | TBD |

### 7.2 Per-message and transport worksheet

- **Purpose / trigger / originating owner / recipients:** TBD.
- **Event identifier, version, payload schema and sanitized example:** TBD.
- **Per-role field projection and authorized-view revision model:** DO-067/068 approve Host/Player/Spectator projection boundaries, freshness revisions, snapshot/subscription ordering and gap recovery; exact payload fields, envelopes and transport remain TBD.
- **Snapshot capture/subscription ordering and missing-update prevention:** DO-068 requires one serialized attachment/snapshot/revision cut and snapshot-before-later-update order. DO-091/092 cap/version/check attachments and reconstruct authority after hibernation; DO-095 requires commit-before-send and close/resync on pressure/failure. Provider delivery guarantees remain TBD.
- **Durable commit boundary versus emission and acknowledgement:** DO-066/095 require accepted authoritative state to commit before WSS emission; DO-073 does not require an application ACK for terminal spectator delivery before identity/session cleanup. Exact peer delivery/outbox and close-work acknowledgements remain implementation details under DO-036/083.
- **Duplicate/out-of-order/gap handling and obsolete-socket rejection:** DO-068 requires duplicate/stale suppression and full authorized resync on a detected gap. DO-093 fences the superseded participant connection at commit and makes stale close events unable to clear its replacement; DO-095 closes on delivery pressure/failure for reconnect/resync. Runtime socket control/API remains implementation work.
- **Participant-session single-live-socket enforcement versus concurrent account logins:** DO-093 approves prepare/authenticate new socket before atomically replacing the current connection ID; failed precommit preserves old socket, after commit old actions/frames fail, stale close cannot remove replacement. Count participant sessions, not sockets; host account concurrent login policy remains separate.
- **Durable Object hibernation handlers, socket attachments and restoration:** Logical rules are approved by DO-091/092: <=512-byte versioned attachment is a lookup hint; rebuild authority from SQLite on wake/protected action, fail closed, preserve healthy connections as presence. Runtime handler/API/SDK integration remains TBD.
- **Expiry/revocation/grace scheduling without in-memory-only timers:** DO-094 approves one durable earliest-deadline alarm per Object across persisted domain deadlines and pending-work retries, bounded trusted-time processing, authoritative reread and earliest reschedule. Platform alarm-handler integration remains TBD.
- **Payload/buffer limits and backpressure:** DO-095 approves 256 KiB encoded frame and 1 MiB unsent queue per socket; commit before send, close rather than drop revisioned state, reconnect via authorized snapshot. Oversize legal snapshot requires separately reviewed chunking. Preserve HLD 30-second reconnect cap; heartbeat/check intervals remain TBD.
- **Frontend state transitions, resync gating and private-cache clearing:** TBD.
- **Protocol compatibility, close behavior and tests:** DO-091–095 approve fail-closed attachment limits, hibernation authority reconstruction, socket fencing, durable deadline scheduling and close/resync on bounded-delivery failure. Exact close-code/error mapping, version negotiation, provider compatibility and executed tests remain TBD.
