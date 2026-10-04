# Brews Bingo — Durable Object Design

## 1. Document status and navigation

- **Status:** Incremental item-by-item review of the schema proposal moved from the LLD (LLD-025). **DO-001 approves one separate SQLite-backed `AccountsObject` per application environment for host/admin accounts and credentials.** DO-002 approves final History placement in the original GameObject with a minimal Directory index. DO-022–DO-045 are approved, including the timer, configuration, pool-ceiling, feasibility and case-sensitive name-comparison decisions. DO-046 approves atomic pre-start alias rename semantics and stable-player-ID session treatment; DO-047 approves atomic role-switch/Leave behavior and role-bound session replacement without extending expiry; DO-048 approves retaining the latest explicit Leave timestamp after return; DO-049 approves the logical PlayerRecoveryRecord fields/types/privacy; DO-050 approves v1 answer normalization and versioned replacement policy; DO-051 approves the Argon2id/PHC recovery-verifier profile and benchmark gate. Current cursor: **DO-052 — Recovery and answer-replacement transaction/session-socket fencing**. DO-055's ParticipantSessionRecord fields and live/final access constraints remain pending. Actual production Argon2 costs/caps, account-password runtime results, and access/session RNG target compatibility remain unmeasured. Other unreviewed fields/protocols remain proposals. The ownership/record inventory, Rust-style sketches, invariants and worksheets remain design material, not a finalized complete SQL schema or authorization to implement. See [the review ledger](#item-review).
- **Ownership:** This document is the single home for Durable Object schemas and record design. [The LLD](lld.md) retains cross-cutting domain rules, UUID/error conventions, password policy/hashing, workflows, verification/deployment planning and the decision register. [API design](api-design.md) retains input/output bodies, safe projections and HTTP/WSS contracts; those are not direct serializations of these private records.
- **Source precedence:** Follow [LLD source precedence](lld.md#12-source-precedence-and-reconciliation), [the HLD](hld.md) and later explicit user decisions. Business requirements and hosting evidence remain in [requirements.md](requirements.md) and [research.md](research.md). Moving this proposal does not approve unselected placements, physical schemas or protocols.
- **Confirmed details carried forward:** Application-generated system IDs use UUID v7. Accounts use `username`: trim surrounding ASCII whitespace before validation and preserve remaining original casing in storage/display (DO-021). Username uniqueness/login uses approved exact case-sensitive comparison on the trimmed value, without case folding or a normalized comparison key. The account's `verifier` field retains the complete encoded password verifier, including hash, salt and parameters (AccountRecord fields/types/nullability and combined PHC representation approved in DO-019); salted Argon2/library selection follows [LLD Section 9.2](lld.md#password-hashing). DO-020 fixes username length/characters and DO-021 fixes trimming/comparison; control-character/SQL integration and remaining work-factor/runtime details stay **TBD**.
- **Timestamp-derived state:** Use existing timestamps as the sole stored representation of equivalent boolean states; `disabled_at` replaces the account disabled flag. See [the cross-record rule](#timestamp-state).
- **TBD:** Remaining detailed ownership/interfaces, field shapes, SQL types/DDL, keys/indexes/constraints, serialization, bindings, transaction/revocation protocols, migrations and executed tests. Accounts placement/cardinality is approved by DO-001; the Directory/per-game architecture and other confirmed domain constraints are not reopened.

Navigation:
- [Physical ownership worksheet](#ownership)
- [Logical record inventory — DATA-01–DATA-13](#record-inventory)
- [Per-record/table schema worksheet](#schema-worksheet)
- [Cross-owner consistency, expiry and migration](#consistency-migrations)
- [Proposed structures and fields](#schema-proposal)
  - [Object boundaries and cardinality](#object-boundaries)
  - [Shared types and enums](#shared-types)
  - [Directory records](#directory-records)
  - [Account records](#account-records)
  - [Game-owned live records](#game-records)
  - [Final History](#history-records)
  - [Operational records](#operational-records)
  - [Socket metadata](#socket-metadata)
  - [Outstanding decisions](#outstanding-decisions)

Section numbers are local to this document unless prefixed **LLD**. DATA IDs and LLD/HLD decision IDs keep their existing meanings. Rust blocks are logical schema sketches with field names and types, not executable migrations or compiled application code.

<a id="ownership"></a>

## 2. Physical ownership worksheet

**Confirmed input:** Directory/game data uses SQLite-backed Durable Objects. There is one Game Object per game, not per board. Cross-Object calls are not a database transaction. Section 6 records Accounts placement approved by DO-001 and Game-local History with a minimal Directory index approved by DO-002; named record fields/types, detailed interfaces/protocols and physical layout remain **TBD**. Do not silently add D1, KV, R2 or another datastore; any new store needs a recorded decision. Eventually consistent KV is not the authoritative game/admission/nonterminal-reservation registry.

**Proposed details now captured:** Section 6 records the requested Object boundaries, logical records, fields/types and invariants (LLD-018). Sections 2–3 remain coverage inventories and now point to those proposals; they do not imply one Object per record group. **DO-001 approves Accounts placement/cardinality. TBD — remaining placement/field approval, physical schema, bindings, internal methods and the mechanisms listed in Sections 4–5 and 6.9.**

| Owner / concern | Logical data responsibility | Physical details |
| --- | --- | --- |
| Game Directory Object | Issued code → stable game lookup; coordination of the single application-wide nonterminal-game reservation. Retained-History code checks participate in issuance safety. | Proposed `GameDirectoryObject`, `GameIndexRecord`, `GlobalReservation` and `AccountAssignmentGate` (Section 6.3); final identity, binding, schema and protocol **TBD**. |
| Game Object | Configuration, lifecycle, memberships/alias claims, boards/matches/qualifiers, calls, outcome, revisions and command results. | Proposed `GameObject` live records (Section 6.5); final tables, indexes, transactions, deadlines and cleanup **TBD**. |
| Account/session authority | Host/admin accounts, password/setup/reset state, link/session verifiers and authorization. | **Approved placement/cardinality (DO-001):** one separate SQLite-backed `AccountsObject` per application environment. Record fields (Section 6.4), bindings, cross-owner interfaces and protocols remain **TBD**. |
| History and lookup/indexes | Self-contained final snapshots and minimal code/identity/expiry metadata; common three-month deadline. | **Approved placement — DO-002:** History stays in the original GameObject; Directory holds the minimal lookup/code/expiry index. `GameHistorySnapshot` / `GameIndexRecord` fields, indexes and deletion coordination (Sections 6.3/6.6) remain **TBD**. |
| Operational metadata | Exit/access state, expiry/grace deadlines, reliable coordination and required nonsecret auditing. | Proposed grants, command/pending/rate-limit/audit records and connection attachments (Sections 6.5–6.8); exact placement, minimal contents, retention and cleanup **TBD**. |

<a id="record-inventory"></a>

## 3. Logical record inventory

These stable DATA-01–DATA-13 groups are **not selected table names or a normalized schema**. The last column maps them to proposed logical records in Section 6. Final consolidation/splitting, physical columns/types/constraints and relationships remain **TBD**; application-generated identifier generation now follows UUID v7 ([LLD Section 3.3](lld.md#identifier-policy)).

| ID | Record group | Required information / invariant from HLD | Proposed records (Section 6); final schema TBD |
| --- | --- | --- | --- |
| DATA-01 | Provisioned account | Stable identity, host/admin type, enrollment/password/reset/access state; Users-safe projection distinct from credentials. | `AccountRecord` (§6.4) fields/types/nullability approved in DO-019; physical schema/constraints remain **TBD**. |
| DATA-02 | Access/enrollment/reset credential | Account/purpose binding, verifier, issue/expiry/consumed/revoked state; one-day single use. | `AccessLinkRecord` (§6.4); **TBD** final schema. |
| DATA-03 | Account and restricted sessions | Account/scope binding, verifier, fixed expiry/revocation; restricted setup/reset is not normal account authority. | `AccountSessionRecord` and DO-035-approved logical `AccountSocketSubscription`; physical schema remains **TBD**. |
| DATA-04 | Game directory and reservation | Code/stable-game mapping; one global reservation across New/Awaiting Players/In Progress, with safe terminal release. | `GameIndexRecord`, `GlobalReservation`, `AccountAssignmentGate` (§6.3); **TBD** final schema. |
| DATA-05 | Game and configuration | Designated host, exact lifecycle, fixed rules/pool/grid/free cells/capacities, relevant durable timestamps. | `GameRecord`, `GameConfiguration` (§6.5); **TBD** final schema. |
| DATA-06 | Membership / alias / occupancy | Stable game/member identity, role, accepted trimmed alias, case-sensitive uniqueness, admission/Leave/presence/grace state; sockets are not seats. | `PlayerRecord`, `SpectatorRecord` (§6.5); `ConnectionAttachment` (§6.8); **TBD** final schema. |
| DATA-07 | Participant sessions / exit authorization | Stable game/member or spectator binding, fixed expiry/revocation and permitted final-view exit state; resolve a player's current alias from PlayerRecord rather than duplicating it in the session binding. | `ParticipantSessionRecord`, `AccountGameViewAccess` (§6.5); **TBD** final schema. |
| DATA-08 | Private player recovery verifier | Stable member binding and protected verifier; optional, replaceable/deletable by valid-session owner; never public/History data. | DO-049 approves logical `PlayerRecoveryRecord` fields/types/privacy; DO-050 approves normalization/version behavior; verifier profile and transactions remain DO-051/052. |
| DATA-09 | Assigned player board | Immutable assigned layout/string values/free cells plus automatic matched state; no pre-start or recovery-generated replacement. | `PlayerBoardRecord`, `BoardCell` (§6.5); **TBD** final schema. |
| DATA-10 | Ordered calls / qualification / outcome | Accepted call order and unique values, authoritative eligibility, at most one winner and immutable terminal result. | `CallRecord`, `TerminalOutcome`, `CompletedLine` (§6.5); **TBD** final schema. |
| DATA-11 | Revisions / command outcomes | Durable consistency/retry identity and accepted result; bounded retention design without authorizing a replay archive. | `ViewRevisionRecord` (§6.5); `CommandReceipt` (§6.7); **TBD** final schema. |
| DATA-12 | Final History and indexes | Final calls/aliases/winner/boards plus minimal metadata, shared expiry and code-collision lookup; no session/answer credentials. | `GameHistorySnapshot`, `WinnerSnapshot`, `HistoryPlayerSnapshot` (§6.6); `GameIndexRecord` (§6.3); **TBD** final schema. |
| DATA-13 | Deadlines / coordination / audit | Minimum durable state needed for cancellation/expiry/grace, cross-Object recovery and actual admin-action attribution. Exact content/retention remains open. | Persisted deadlines in §6.3–§6.6; `PendingOperation`, `RateLimitBucket`, `AdminAuditRecord` (§6.7); **TBD** final schema. |

<a id="schema-worksheet"></a>

## 4. Per-record/table schema worksheet — copy for each record

- **Record ID / purpose / HLD references:** TBD — link a Section 3 DATA ID to its Section 6 proposal.
- **Authoritative Object/store and binding:** TBD — review the proposed owner in Section 6; binding names remain unselected.
- **Table/record name and schema version:** TBD.
- **Lifecycle / readers / writers / privacy classification:** TBD.

| Column / property | SQL or record type | Nullable / default | Key / constraint / validation | Sensitive-data treatment | Meaning |
| --- | --- | --- | --- | --- | --- |
| TBD | TBD | TBD | TBD | TBD | TBD |

| Schema design item | Value to fill |
| --- | --- |
| Primary identifier and generation | UUID v7 for application-generated system IDs ([LLD Section 3.3](lld.md#identifier-policy)); **DO-005:** SQLite TEXT in canonical lowercase hyphenated form with validated v7 conversion. Concrete constructors, physical key/relationship definitions and target wiring remain **TBD**. |
| Relationships and ownership boundaries | TBD |
| Unique/check constraints, canonical alias/code representation | TBD |
| Indexes and query/access patterns | TBD |
| Foreign-key behavior or application-enforced cross-owner references | TBD |
| Read/write statement shapes and parameterization | TBD |
| Timestamp-derived boolean state | Confirmed rule in Section 6.2: no persisted boolean duplicating a corresponding timestamp; document presence/deadline/reset semantics per record. Remaining record-specific details **TBD**. |
| Transaction boundary and invariants maintained | TBD |
| Concurrency/conflict detection and retry outcome | TBD |
| Retention trigger, persisted deadline, read denial and deletion | TBD |
| Copies/indexes/logs/backups and restore-time expiry enforcement | TBD |
| Size, read/write amplification and quota assumptions | TBD |
| DDL / initialization / migration / rollback | TBD |
| Schema and constraint tests | TBD |

**DDL and query definitions:** TBD — Section 6 contains only logical field/type sketches, not executable SQL or a completed physical schema.

<a id="consistency-migrations"></a>

## 5. Cross-owner consistency, expiry and migration

| Design topic | Required constraint | Detailed mechanism |
| --- | --- | --- |
| Create / publish / start | No second game while one is nonterminal; no exposed unready code; commit admission/start/board assignment consistently. | TBD |
| Terminal transition / release | Freeze durable result before game-specific slot release; no stale release clears a newer reservation. | TBD |
| Call transaction | Calls, affected boards, qualification, revision and command result commit together before push. | TBD |
| Credential redemption / reset | Atomic single-use consume + restricted session; reset/reissue invalidate predecessors under reviewed policy. | TBD |
| Code reuse / History expiry | Check live and unexpired History without permanent-code-ledger assumptions; old credentials never move to a reused code. | TBD |
| Retention scheduler | Enforce [LLD Section 2.3](lld.md#23-attendance-exit-and-retention-inputs) deadlines without client visits or in-memory-only timers; preserve three **months**, not an assumed 90 days. | TBD |
| Notice then deletion | Deliver permitted final notices without retaining deleted participant credentials indefinitely or waiting for Exit. | TBD |
| Migration / restore | Preserve authority, committed boards, absolute deadlines and terminal immutability; never resurrect expired data. | TBD |

Migration inventory: **TBD** — version, affected Object/store, compatibility window, forward/backout actions, verification and failure recovery.

<a id="schema-proposal"></a>

## 6. Proposed Durable Object structures and fields

**Status — proposal captured at user request (LLD-018):** This section preserves the structure proposal moved from the LLD, including names and Rust-style types. It is not a finalized schema, an implemented service or approval to build. **DO-001 approves the separate singleton Accounts Object; DO-002 approves Game-local History with a minimal Directory index. TBD — field shapes, detailed ownership/interfaces and implementation choices.** The Directory/per-game ownership baseline remains confirmed; other unreviewed refinements below remain proposed. The user requested that undetermined details be explicitly marked **TBD**. Subsequent LLD-020 confirms UUID v7 for application-generated ID fields; that choice is no longer a proposal/TBD.

These are **logical persisted records**, suitable for tables in each Object's SQLite database—not one large Rust struct that must always be loaded into memory. Rust blocks are schema sketches, not compilable implementations. Records, projections, derived data and socket metadata are distinguished below. Final SQL types, defaults, normalization, serialization, constraints, migrations and SDK integration remain **TBD** unless carried forward as confirmed domain rules.

<a id="object-boundaries"></a>

### 6.1 Object boundaries and cardinality

| Proposed class | Instances | Responsibility | Status |
| --- | --- | --- | --- |
| `GameDirectoryObject` | One per application environment | Game discovery, code uniqueness, saved/in-progress reservations, History index and cross-game coordination. | Existing directory responsibility; exact class/instance identity and binding **TBD**. |
| `AccountsObject` | One per application environment | Host/admin accounts, usernames, password verifiers, enrollment/reset links, account sessions and account-management operations. | **Approved placement/cardinality — DO-001.** Separate SQLite-backed owner; field shapes, bindings, interfaces, protocols and measured capacity remain **TBD**. |
| `GameObject` | One per stable `game_id` | Configuration, lifecycle, players, spectators, participant sessions, boards, calls, qualification, final History and game-specific cleanup. | Existing per-game ownership; **final History placement here approved — DO-002**. Fields, physical tables, bindings and protocols remain **TBD**. |

Rationale: keep account credentials outside game storage; co-locate admission, boards, calls and qualification under one transactional owner; coordinate global exclusivity/code allocation in the Directory. Do not create an Object per player, board or session. No D1, KV or other storage service is introduced by this proposal. Final History stays in the original Game Object rather than a separate History Object (DO-002); the Directory retains its minimal index, not full boards or snapshots.

Use a stable Game Object identity derived from `game_id`, **never the reusable game code**. Application-generated `game_id` and other system IDs use UUID v7 ([LLD Section 3.3](lld.md#identifier-policy)). **TBD — target-specific generation wiring, namespace/instance naming, bindings, internal methods, deployment configuration and capacity validation for singleton owners.** A logical record group does not select one physical table or one additional Object.

<a id="shared-types"></a>

### 6.2 Shared type conventions

**Confirmed exception to the remaining proposals:** application-generated system IDs use UUID v7 via `uuid`, with distinct `uuid::Uuid`-backed newtypes ([LLD Section 3.3](lld.md#identifier-policy); LLD-020). DO-003 also approves epoch-millisecond Timestamp representation and trusted-backend time authority. Other unreviewed representations below remain proposed. DO-006 selects the shared typed-ID module/interface design; no code is implemented. **TBD — concrete constructors/module implementation, target clock/randomness integration, remaining storage/wire mapping, serialization and clock rollback/skew handling. DO-004 approves the common Revision initialization/advancement/overflow rules; per-counter triggers remain open.**

| Type | Representation / meaning and decision status |
| --- | --- |
| `GameId`, `AccountId`, `PlayerId`, `SpectatorId`, `SessionId`, `LinkId`, `CommandId`, `OperationId`, `ConnectionId`, `AuditId` | **Confirmed UUID v7** application-generated IDs using `uuid`; distinct `uuid::Uuid`-backed newtypes, not arbitrary strings or credentials. See [LLD Section 3.3](lld.md#identifier-policy) for scope/exception rules. **DO-005 approves SQLite TEXT in canonical lowercase hyphenated UUID form** with validated v7 storage conversion. **DO-006 approves** private-inner-value types in a shared domain ID module, validated `TryFrom<uuid::Uuid>` / `FromStr`, canonical formatting and adjacent `error.rs` / `thiserror` errors. Generate once in the authorized creation component; retain IDs on retries. Concrete implementation, target clock/randomness wiring and API wire encoding remain **TBD**. |
| `Timestamp` | **Approved — DO-003:** `i64` / SQLite `INTEGER`: UTC Unix epoch milliseconds since `1970-01-01T00:00:00Z`. Trusted backend record owners generate timestamps/deadlines; browser time and UUID v7 time components are not authoritative. Explicit sequence/revision fields govern ordering. Calendar-month arithmetic, clock rollback/skew handling and target integration remain **TBD**. |
| `Revision` | **Approved — DO-004:** nonnegative `i64` / SQLite `INTEGER`; new counters start at `0`. Increment when required by the accepted operation, within the owning Object transaction. Reads, rejections and retries do not themselves advance counters. Reject required advancement at the maximum instead of wrapping/resetting. Per-counter triggers remain **TBD** in their lifecycle/protocol; this is not time or cross-Object global ordering. |
| `GameCode` | Validated `String`: exactly eight uppercase ASCII letters/digits, preserving confirmed normalization and reuse rules. |
| `Digest` | `Vec<u8>`: digest bytes. **DO-027 approves `AccessLinkRecord.token_verifier`; DO-031 approves `AccountSessionRecord.token_verifier`** as 32 raw SHA-256 bytes stored in SQLite `BLOB` with a unique constraint/index, each purpose-specific. Other uses (including request fingerprints) remain **TBD**. |
| `EncodedVerifier` | `String`: encoded algorithm/version, parameters, salt and derived hash. Account passwords use **salted Argon2** (LLD-024); **DO-019 approves** a complete library-generated PHC string in `AccountRecord.verifier`, following [LLD Section 9.2](lld.md#password-hashing). Final Argon2 profile, physical constraints/mapping and cost remain **TBD**. Recovery-answer algorithm/format/cost remain independently **TBD**; this shared type does not impose the account-password scheme on answers. |
| `Option<T>` | **Approved mapping — DO-009:** optional scalar fields use SQL `NULL` for absence, not sentinel strings/IDs/numbers. Optional whole entities may use row absence. No implicit default; defaults require explicit per-record specification. |
| `Vec<T>` | **Approved mapping — DO-009:** use child rows rather than whole-collection JSON blobs, retaining explicit sequence/position when order matters. Empty collections have no child rows. Exact child tables, relationships, indexes and constraints remain **TBD**. |
| `SecretFreeJson` | Validated `serde_json::Value` containing no credentials, verifiers or recovery answers. Exact permitted typed payloads and size bounds: **TBD**. |

<a id="timestamp-state"></a>

**Timestamp-derived state — confirmed user direction during DO-022:** Across all stored records, when an existing timestamp represents the same state as a boolean, store the timestamp alone and derive the boolean; do not persist independently mutable duplicates. Nullable current-state/event timestamps such as `disabled_at`, `revoked_at` and `consumed_at` express their corresponding state through presence. `None`/SQL `NULL` means absent; never use zero as an absence sentinel. Derive deadline-based states using the deadline and trusted backend time instead of assuming a populated `expires_at` or `blocked_until` means already expired or currently blocked. A historical timestamp is not automatically evidence of a current state: retain the existing meaning and reset/clear rules, and leave unsettled semantics for review rather than inventing them.

API or in-memory boolean projections may be computed from the authoritative timestamp but are not additional database fields or client-supplied authority. Do not introduce timestamps just to eliminate unrelated booleans. Review of the current schema found only `AccountRecord.disabled` redundant with an existing timestamp, so it is removed in favor of `disabled_at`. `GameConfiguration.free_cells_enabled` and `BoardCell.is_matched` have no equivalent timestamp fields and remain unchanged; their other design questions remain pending.

Shared enum/value sketches (unreviewed field choices remain proposed):

**CellPosition approved — DO-008:** `row: u8` and `column: u8`, stored as SQLite `INTEGER`. Preserve one-based indexing and validate against the owning board side length; reject zero, negative or out-of-bounds values without clipping/wrapping. Board geometry/free-cell policies are unchanged; table/index layout remains **TBD**.

```rust
enum AccountRole {
    Host,
    Admin,
}

enum AccountStatus {
    PendingEnrollment,
    Verified,
    ResetRequired,
}

enum GameState {
    New,
    AwaitingPlayers,
    InProgress,
    Resolved,
    Cancelled,
}

enum WinningPattern {
    SingleLine,
}

enum CallMode {
    Random,
    Manual,
}

struct CellPosition {
    row: u8,       // One-based, matching the existing design.
    column: u8,    // One-based.
}

struct StorageMetadata {
    schema_version: u32,
}
```

**StorageMetadata approved — DO-010:** each Directory, Accounts and Game Object keeps one persistent local metadata row with `schema_version: u32` / SQLite `INTEGER`. Initialized schemas begin at version `1`; record a version change only with successful committed local initialization/migration. Unsupported newer versions refuse normal data operations instead of guessing or silently downgrading. Detailed SQL, rollout/compatibility/backout and executed migration tests remain **TBD**.

`GameState::InProgress` is a Rust-style spelling of the exact domain state **In Progress**, not a lifecycle rename.

**Approved enum-storage convention (DO-007; user condition — `strum` required):** Persist enum tags as stable snake_case SQLite `TEXT`, generated from their Rust enum definitions by **Rust `strum`**, not hand-written match/string tables, numeric variant positions or Debug formatting. Use the derive feature and generated string conversion/parsing; the library exposes derive macros through `strum` when `derive` is enabled.[1] The intended mapping uses `#[strum(serialize_all = "snake_case")]` and derived `Display` / `EnumString`; this attribute controls both serialization and deserialization case.[2] Examples: `AwaitingPlayers` → `awaiting_players`, `InProgress` → `in_progress`. Domain/UI lifecycle labels remain exactly New, Awaiting Players, In Progress, Resolved and Cancelled; keep UI presentation separate from persisted-tag formatting.

Require exact recognized stored tags, with no default/catch-all fallback; unknown labels are storage errors. Keep generated mappings stable: a Rust variant rename must preserve its existing stored label through an explicit attribute or receive a reviewed migration, never silently change database values. Payload-bearing variants require separately validated payload reconstruction; parsing their tag alone must not manufacture default IDs or fields, because `EnumString` documents defaulting additional variant data during deserialization.[3] **TBD — per-record tag/payload mappings, compatible pinned crate features/version, migration/version protocol and executed round-trip/unknown-tag/rename tests.** The documentation used here describes `strum` 0.28.0, not a selected dependency pin or verified Workers/Wasm build. API serialization remains separate.

<a id="directory-records"></a>

### 6.3 Directory-owned records

The Directory owns discovery and coordination, not boards, participant credentials or authoritative gameplay. **GameIndexRecord is proposed as a combined live-game and retained-History index**, not a historical-data-only table or the full History snapshot. Its ended/History-expiry metadata is populated only after the applicable game ending. **DO-011 approved after clarification:** this is a combined live-game and retained-History index, with `ended_at` as the ended timestamp field. Full History snapshots/boards remain in the original Game Object. **TBD — physical tables/index DDL and the complete cross-Object protocol.**

#### `GameIndexRecord` — searchable projection

**Fields/types and nullability approved — DO-011:** retain the fields below as a combined live-game/History index. `game_code` is absent before allocation; pending allocation is not publicly discoverable. `started_at` is present once started, `ended_at` only after ending, and `history_expires_at` only for a started terminal game. `PublicationState::{Pending, Published}` is internal publication metadata, not another game state. Keys/indexes, publication/freshness and coordination/cleanup protocols remain **TBD**.

```rust
struct GameIndexRecord {
    game_id: GameId,
    game_code: Option<GameCode>,
    designated_host_id: AccountId,

    state: GameState,
    source_revision: Revision,
    publication_state: PublicationState,

    created_at: Timestamp,
    started_at: Option<Timestamp>,
    ended_at: Option<Timestamp>,
    history_expires_at: Option<Timestamp>,
}

enum PublicationState {
    Pending,
    Published,
}
```

- `game_code` is absent for New before allocation; a pending allocation may reserve a code without publishing it for discovery. **DO-013 publication barrier:** keep initial lobby code publication Pending/hidden until configuration/code are committed in the GameObject and the Directory accepts the matching revision. After publication, ordinary projected state changes do not unpublish the code.
- `source_revision` carries the GameObject revision used for the projection. **DO-013:** accept an update only when its source revision is strictly newer than the stored projection; an exact duplicate is a no-op, older updates are ignored. The Game Object remains authoritative and validates actual lifecycle/access before admission or mutation.
- A terminal index entry reserves its code through the History deadline; it never permits terminal admission/recovery.
- Pre-start cancellation removes the index entry instead of retaining an empty History record.
- **Approved keys/indexes/filters — DO-012:** primary key `game_id`; unique allocated/non-null `game_code`, allowing uncoded entries to coexist; query indexes on `designated_host_id`, `state` and non-null `history_expires_at`. Public join-code discovery requires `Published` plus Awaiting Players/In Progress. History listing requires a started Resolved/Cancelled game with `history_expires_at` later than trusted backend time; exclude pre-start cancellations/expired entries even if cleanup is delayed. These filters do not replace role/session authorization or authoritative Game checks. Exact DDL/index names and query-plan validation remain **TBD**. Expired code reservations must be removed transactionally before reuse; old credentials stay bound to the old game identity.

**DO-011 approves fields/nullability; DO-012 approves keys/query indexes and filters; DO-013 approves initial publication readiness, monotonic projection freshness and authoritative revalidation. DO-015 approves claim only when `game_id` is NULL and release only when the stored ID matches. TBD — exact SQL/constraint/index names, query-plan validation, Directory-list staleness handling, caller acknowledgement of initial publication, cross-Object update delivery/retry/recovery, operation-level idempotency, start projection delivery, terminal release scheduling/reconciliation and crash-safe expiry/code reuse.**

#### `GlobalReservation` — one nonterminal game

```rust
struct GlobalReservation {
    game_id: Option<GameId>,
}
```

There is exactly **one** Directory-owned reservation row per application environment. `game_id = NULL` means the slot is free; a non-NULL `game_id` identifies the sole game allowed to be nonterminal across **New**, **Awaiting Players** and **In Progress**. Claim the slot when creating a New game and retain it through publication and Start. A transition to In Progress does not acquire another slot. Resolved/Cancelled are terminal and release the slot after the terminal result is durably committed. Retained History is outside the reservation.

**DO-014 approves this minimal record shape only:** no persisted operation ID, generation/fencing counter or reservation phase. DO-015 approves claiming only when `game_id` is NULL, clearing only when it matches the same never-reused game ID, and retrying a failed GameObject creation with the same reserved ID and any already-associated game code. DO-016 separately approves idempotent terminal release after commit: matching ID clears, NULL is already complete, and a different ID is stale/no-op. Delivery scheduling and command-receipt idempotency remain open; there is no cross-Object transaction.

#### `AccountAssignmentGate` — account-removal coordination

**DO-017 approved:** One Directory-side row keyed only by `account_id`; row presence blocks new-game/host assignment while account disable/delete is in progress. Operation ID, generation and timestamp are not persisted in this record. DO-018 approves acquiring the gate before the hosted-game check and holding it through disable/delete. DO-039 approves the transfer/removal order: gate acquired first rejects transfer; a transfer committed first must be visible to the hosted-game check and block disable/delete while the game is nonterminal. Durable coordination/recovery mechanics remain TBD; do not rely on a gate precheck alone.

```rust
struct AccountAssignmentGate {
    account_id: AccountId, // primary key; row presence means assignment is blocked
}
```

While this approved gate exists,

**DO-018 approved sequence:** Acquire the per-account gate before checking hosted nonterminal games; while held, reject new-game/host-transfer assignments to that account. If a hosted nonterminal game exists, clear the gate and reject disable/delete. If none exists, disable/delete the account and then clear the gate. On interruption, retain the gate and retry safely. **Still TBD:** serialization with assignments already in flight, command-receipt linkage, freshness proof and cross-Object failure recovery details.

<a id="account-records"></a>

### 6.4 Accounts-owned records

**Approved placement/cardinality (DO-001):** one separate SQLite-backed `AccountsObject` per application environment owns host/admin accounts, usernames, password verifiers, enrollment/reset links and account sessions. Anonymous player/spectator sessions stay in their owning Game Object. This co-locates username uniqueness, link consumption, restricted-session creation and account credential changes under one local transaction owner. **TBD — field/constraint acceptance, bindings/interfaces, capacity/security validation and local/cross-owner protocols.** Shared account-service contention is a trade-off, not measured capacity evidence; placing the data together does not implement transactions or the cross-Object socket-revocation delivery protocol approved in DO-036.

#### `AccountRecord`

**Approved — DO-019, amended during DO-022; verifier profile/encoding approved by DO-023:** keep the original fields/types/nullability with `verifier` as the account password field name. Determine disablement solely from `disabled_at`: populated means disabled; `None`/SQL `NULL` means enabled. Do not store a separate disabled boolean. `Disabled` remains excluded from AccountStatus; lifecycle status is preserved through disable/enable. Keep one complete library-generated Argon2id v19 PHC string in that field, using a fresh random 16-byte salt and 32-byte output; map the string to SQLite `TEXT`, with SQL `NULL` before first password setup. Do not add separate salt/hash columns or custom binary encoding. DO-022 approves lifecycle and credential-epoch policy. DO-024 approves explicit `m_cost`/`t_cost`/`p_cost`, an OWASP benchmark starting point, bounded fail-closed parameter handling and pre-production Worker/WASM validation. Exact production costs/caps and actual runtime/resource results remain TBD pending measurement; transaction/concurrency mechanics and other implementation choices remain open. Username rules are approved in DO-020/021.

```rust
struct AccountRecord {
    account_id: AccountId,

    username: String,

    role: AccountRole,
    status: AccountStatus,

    verifier: Option<EncodedVerifier>,
    credential_epoch: Revision,

    created_at: Timestamp,
    password_set_at: Option<Timestamp>,
    disabled_at: Option<Timestamp>,
}
```

- `username` is the account login name. **DO-021 approved:** trim leading/trailing ASCII whitespace before validation on entry and backend account-creation/login paths. Preserve the trimmed username's original casing in storage/display; do not store a separate normalized key. Usernames differing only in case are distinct; uniqueness and login use exact case-sensitive equality on the trimmed stored value, with no lowercase/case-folded comparison. Leave other permitted characters unchanged. Apply DO-020 validation to the trimmed result, not the untrimmed input. **DO-020 approved, processing order clarified by DO-021:** 10–50 decoded ASCII characters after trimming, excluding all remaining/internal ASCII whitespace (HT/tab, LF, VT, FF, CR: `0x09`–`0x0D`; ordinary space: `0x20`). Other ASCII control characters, including NUL and DEL, are allowed. Apply consistently to host/admin provisioning through the developer CLI and admin interface; do not silently narrow to printable characters. Exact SQL/constraint enforcement must preserve case-sensitive equality; control-character transport/UI/storage handling remains **TBD** and must be verified.
- `verifier` is the user-selected field name (DO-019). It is absent before first setup; after setup it holds the complete library-generated Argon2id v19 PHC string (work parameters, fresh 16-byte salt and 32-byte derived hash), **not only the raw salt or hash**. DO-023 approves storing this string as SQLite `TEXT` (SQL `NULL` before first setup) in this one field. This combined representation keeps salt and hash together in the **same account-owning Durable Object**, without separate salt/hash fields or custom binary encoding ([LLD Section 9.2](lld.md#password-hashing)). DO-024 approves explicit cost parameters, bounded fail-closed verification and target-runtime validation before production; actual supported numerical caps/profile require measurement. `credential_epoch` changes when reset/reissue/revocation must invalidate predecessors; concurrent ordinary logins do not increment it or revoke one another.
- **Disabled representation — latest confirmed user revision during DO-022:** `disabled_at: Option<Timestamp>` is the sole stored disablement state. Evaluate `disabled_at.is_some()` in Rust or `disabled_at IS NOT NULL` in SQL; timestamp population, not its numeric truthiness or comparison with the clock, means disabled. Set the timestamp when disabling; clear it on authorized enable. Keep the retained PendingEnrollment/Verified/ResetRequired status and password verifier unchanged. Do not persist a redundant boolean or previous-status field. Authorized no-op retries leave the timestamp unchanged. Concrete transaction/concurrency mechanisms remain **TBD**.
- Role is authoritative here, not in cached browser/socket data.
- Account deletion removes credentials without cascading into unrelated/unexpired History. Minimal noncredential references follow the applicable History lifetime.

Password length/character rules are confirmed in [LLD Section 9.1](lld.md#password-policy) (LLD-023); salted Argon2 via RustCrypto `argon2` is confirmed in [LLD Section 9.2](lld.md#password-hashing) (LLD-024). DO-019 approves AccountRecord fields/types/nullability and combined PHC storage; **DO-023 approves** Argon2id v19, fresh 16-byte salt, 32-byte output, and SQLite `TEXT` encoding in the nullable `verifier` field. **DO-024 approves** explicit work factors, the OWASP benchmark starting point, fail-closed parameter bounds and a target-runtime validation gate; actual costs/caps and runtime results remain TBD until measured. **DO-025 is next** for verifier upgrades and concurrent-reset protection. Account-operation transactions/races and privileged self-removal/last-admin rules remain separate TBDs. DO-020 approves username length/characters; DO-021 approves casing/trimming/comparison. DO-022 approves lifecycle/disablement and credential-epoch policy below; corresponding SQL/transaction/concurrency implementation remains open. Account-enable functionality/API is required (HLD-078), executable only by another admin account or developer CLI. Enable restores the pre-disable lifecycle state without reviving old sessions/links; disablement is represented solely by nullable `disabled_at`, independently of lifecycle status. No role-editing feature is selected.

##### Account lifecycle and disable/enable rules — approved DO-022 subdecision

- Create accounts in `PendingEnrollment` with `disabled_at = None` (SQL `NULL`). Successful first-password setup changes status to `Verified`.
- A privileged password-reset request on an enabled enrolled account sets `ResetRequired`; old-password login stays blocked until successful reset returns it to `Verified`. Expiry of a reset link does not restore access.
- Disable sets a previously absent `disabled_at` to the trusted backend timestamp without changing lifecycle status or the stored password verifier. Revoke existing sessions/links; reject login, enrollment (including reissue) and password-reset operations while disabled. The existing hosted-nonterminal-game guard and Directory assignment gate still apply to disable/delete.
- Enable is executable only by another enrolled admin account or developer CLI. Clear `disabled_at` to `None`/SQL `NULL`, preserving status/verifier, without reviving old sessions/links. `Verified` may authenticate afresh with its existing password; `PendingEnrollment` / `ResetRequired` require fresh links issued through the corresponding existing flows, not revival of old links or automatic session creation.
- Set `disabled_at` on the accepted enabled-to-disabled transition (`None` → timestamp) and clear it on disabled-to-enabled (`timestamp` → `None`). Already-disabled disable and already-enabled enable are no-ops after authorization checks; retries do not rewrite timestamps. Timestamp/lifecycle guard and credential-revocation updates must be consistent; SQL/transaction and command-retry mechanisms remain **TBD**.
- Deletion removes the account rather than adding a `Deleted` enum variant. Deleted accounts are not re-enabled; unrelated/unexpired History remains governed by its own retention.

**Credential-epoch rules — approved DO-022:** `credential_epoch` is an account-wide `Revision`, initialized to 0. Increment exactly once for replacement enrollment-link issuance; password-reset-link issuance or reissuance; successful password-setup/reset completion; and an accepted enabled-to-disabled transition. Do not increment for reads, ordinary login, link redemption alone, single-session logout, enabling, rejected operations or retries of an already-committed operation. Never roll the epoch back on enable. Each issued link/session stores the account's current epoch. Validate link/session token and require its stored epoch to match the current account epoch, as well as scope, expiry, individual revocation, lifecycle and `disabled_at`; links also require the correct purpose and unused state. Commit the increment with the related account/credential changes in AccountsObject; any credential issued by that transition carries the new epoch. Preserve concurrent ordinary logins. Reject overflow rather than wrapping. This policy does not extend fixed deadlines, determine reset-completion auto-login, or itself deliver cross-Object GameObject/WebSocket revocation. DO-036 separately approves durable close work, acknowledgements/retries and fail-closed per-frame authority checks. Exact SQL transactions, race/retry mechanics, credential schemas and DO-036 outbox/delivery implementation remain **TBD**; this approval is not implementation authorization.

#### `AccessLinkRecord`

**Approved — DO-026:** Use the listed required fields and nullable `consumed_at` / `revoked_at` timestamps, with one record per issued link under AccountsObject. `link_id` is the stable record identity; each link is bound to one account, purpose and issuance epoch. `issued_at` / `expires_at` are required UTC epoch-millisecond timestamps and preserve the existing fixed one-day lifetime. A raw reusable URL token is never stored. This approves the logical shape/core constraints only, not SQL DDL/indexes or row-retention policy.

```rust
struct AccessLinkRecord {
    link_id: LinkId,
    account_id: AccountId,
    purpose: AccessLinkPurpose,

    token_verifier: Digest,
    credential_epoch: Revision,

    issued_at: Timestamp,
    expires_at: Timestamp,
    consumed_at: Option<Timestamp>,
    revoked_at: Option<Timestamp>,
}

enum AccessLinkPurpose {
    Enrollment,
    PasswordReset,
}
```

**Approved — DO-027:** Generate 32 bytes from a cryptographically secure RNG (fail closed if unavailable), expose them as canonical unpadded Base64URL (43 characters), and store only SHA-256 of the raw 32 bytes as a unique 32-byte SQLite `BLOB` verifier. Validate canonical encoding and exact decoded length on input; retry generation if the unique verifier constraint detects a collision. Never persist/log the bearer token; `LinkId` is not a secret. This profile is for access links only, not other `Digest` fields. The target RNG compatibility remains subject to DO-024 validation. See the [OWASP Forgot Password Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html) for the supporting requirement to use CSPRNG-generated, sufficiently long, securely stored, user-linked, single-use, expiring tokens; OWASP does not mandate this specific 32-byte/SHA-256 profile.

**Approved — DO-028:** Build absolute links only from a configured canonical HTTPS origin, never the request `Host`; hand off the raw URL only to the authorized CLI/admin caller after durable issuance, with `Cache-Control: no-store` and token-redacted logs/traces. Put the bearer token in the URL fragment (not path/query); the browser removes it promptly and sends it only in the HTTPS redemption POST body. Redemption pages use `Referrer-Policy: no-referrer` and no third-party scripts/analytics. In one AccountsObject transaction, require matching purpose/account/epoch, unexpired and unused/unrevoked link, enabled account and the correct lifecycle (`PendingEnrollment` for enrollment, `ResetRequired` for password reset); conditionally consume once and create exactly one matching restricted session. At most one concurrent redemption succeeds; any failed precondition creates no session. Link redemption alone does not increment the credential epoch. The restricted session has its own fixed one-day deadline from issuance; password setup cannot extend it. Invalid-link cases return one safe generic result. Exact SQL/locking mechanics remain implementation TBD; DO-029 defines recovery after a committed-but-lost redemption response.

**Approved — DO-029:** Enrollment-link reissue is allowed only for an enabled `PendingEnrollment` account; password-reset-link reissue only for an enabled `ResetRequired` account. Disabled accounts reject both; reissue preserves lifecycle status. In one AccountsObject transaction, validate actor/account/state, increment `credential_epoch` exactly once, create one fresh link bound to the new epoch with a fixed 24-hour expiry, and mark predecessor links and account/restricted sessions revoked (`revoked_at`). The epoch check is the authoritative fence if a revocation write is delayed. Reset initiation/reissue keeps old-password login blocked; committing account-state changes does not itself prove cross-Object socket delivery/revocation complete.

For link-issuance operations, a repeated request with the same `command_id` does not mint another secret or replay a raw URL. Retain a secret-free issuance receipt for 30 days containing only account/link IDs, purpose, committed outcome and expiry. A caller that did not receive the URL explicitly issues a new reissue command; that creates a successor and invalidates predecessors. For a redemption committed before its response/cookie is lost, do not replay the consumed link or recover/replay the session secret. If the account remains `PendingEnrollment` or `ResetRequired`, an authorized admin/CLI may issue a fresh corresponding link using the epoch-bumping reissue flow. If setup/reset already completed and the account is `Verified`, use password login or the existing admin reset flow; do not roll back the password or issue an enrollment link. Replays return the approved generic invalid-link result.

Retain metadata-only link rows through the latest applicable expiry/consumption/revocation event plus 30 days, then allow asynchronous deletion. Raw tokens never enter receipts/logs. These rules approve business behavior and retention windows, not a physical receipt schema or SQL/locking/cleanup schedule. DO-036 separately approves durable cross-Object socket-revocation work and acknowledgements; its physical outbox/delivery implementation remains open.

#### `AccountSessionRecord`

```rust
struct AccountSessionRecord {
    session_id: SessionId,
    account_id: AccountId,

    token_verifier: Digest,
    scope: AccountSessionScope,
    credential_epoch: Revision,

    issued_at: Timestamp,
    expires_at: Timestamp,
    revoked_at: Option<Timestamp>,
}

enum AccountSessionScope {
    EnrollmentOnly,
    PasswordResetOnly,
    Normal,
}
```

**Approved — DO-030:** Keep exactly the shown logical fields/types. One record represents one issued session; `session_id` is nonsecret stable UUID-v7 metadata. Bind the record to one account, one scope and the account `credential_epoch` at issue. `issued_at` / `expires_at` are UTC Unix-epoch-millisecond timestamps with `expires_at > issued_at` and the fixed one-day absolute lifetime; activity/reconnect does not extend it. `revoked_at = NULL` means not individually revoked; otherwise it records revocation time. Do not add stored expired/revoked booleans. The record carries no role/game permission; resolve current authority separately. Multiple ordinary account sessions may coexist. DO-031 selects verifier bytes/algorithm; SQL/indices and DO-034 cleanup remain separate.

**Approved — DO-031:** Generate 32 fresh bytes from a CSPRNG for each session token, fail closed on RNG failure, encode as canonical unpadded Base64URL (43 characters), and store only the SHA-256 digest of the raw bytes as the unique 32-byte SQLite `BLOB` in `token_verifier`; retry a digest collision. Never persist/log the raw cookie secret. This is high-entropy token verification, not Argon2; target RNG support remains subject to DO-024 validation. Use one opaque `__Host-brews_session` cookie for restricted and normal session scopes, with `Secure; HttpOnly; Path=/; SameSite=Lax` and no `Domain`; HTTPS is required. Derive `Max-Age`/`Expires` from the fixed `expires_at`; do not slide it. Cookie contents carry no account, role, scope or epoch claims; those remain server-side. Browser-cookie-authenticated state-changing HTTPS requests and WebSocket upgrades require the exact configured application `Origin`, rejecting absent or mismatched values; `SameSite` is not the sole defense. Do not put bearer tokens in URLs or WSS application messages. Developer CLI/service authentication remains a separate privileged path.

`Normal` means account authentication, not permission for every game action. Current role and designated-host assignment still govern actions.

**Approved — DO-032 (enrollment completion only):** After password hashing, one AccountsObject transaction revalidates the enabled `PendingEnrollment` account, the unexpired/unrevoked `EnrollmentOnly` session and matching credential epoch; stores the verifier, sets status to `Verified` and `password_set_at`, increments the epoch exactly once, revokes the restricted session, and creates one `Normal` session bound to the new epoch. Use a distinct session bearer and the DO-031 cookie; preserve the restricted session's original absolute expiry (do not extend it), and issue the cookie only after commit. If the committed response/cookie is lost, do not replay a secret or roll back setup; sign in with the new password. A concurrent reset, disable or expiry that wins first causes setup to fail closed. Command-retry receipts remain outside this item.

**Approved — DO-033:** For password-reset completion, after hashing, one AccountsObject transaction revalidates the enabled `ResetRequired` account, the unexpired/unrevoked `PasswordResetOnly` session and matching epoch; stores the verifier, sets `Verified`/`password_set_at`, increments the epoch exactly once and retires the reset-only session. Do not issue or rotate into a `Normal` session. Clear the shared session cookie and direct the user to password login, which issues a new normal session. The epoch bump invalidates earlier sessions; a lost committed response is recovered by password login, never secret replay or rollback. Command-retry receipt mechanics remain outside this item.

**Approved — DO-034:** On each protected request, resolve the presented SHA-256 token verifier against authoritative `AccountSessionRecord` and `AccountRecord` state. Require the current credential epoch, valid scope/lifecycle, enabled account, `expires_at` in the future and `revoked_at = NULL`; `Normal` requires `Verified`, `EnrollmentOnly` requires `PendingEnrollment`, and `PasswordResetOnly` requires `ResetRequired`. No positive authorization cache may grant access across requests without revalidating current authority. Expired/revoked session rows may be deleted asynchronously once ineligible; cleanup is not part of the authorization decision. Do not alter DO-029 access-link metadata/receipt retention. No additional session-row retention period is selected. SQL/index/schedule details remain implementation work; socket-revocation delivery is DO-036.

#### `AccountSocketSubscription`

```rust
struct AccountSocketSubscription {
    account_id: AccountId,
    session_id: SessionId,
    game_id: GameId,
    connection_id: ConnectionId,

    credential_epoch: Revision,
    expires_at: Timestamp,
}
```

These records identify active game connections for revocation delivery and cleanup; they grant no authority. The metadata alone does not guarantee immediate revocation; DO-036 defines durable close work/acknowledgements and per-frame authoritative session checks, and DO-037 defines expiry-driven close work and stale-row cleanup.

**Approved — DO-035:** Keep the shown seven logical fields. Store this session-bound metadata in the per-environment `AccountsObject`; it grants no authority. `connection_id` (application-generated UUID-v7) is unique. Re-registering the identical tuple is a no-op; reusing that ID with a different account/session/game/epoch fails closed. Set `expires_at` to the linked session's fixed absolute expiry. Register only an enabled `Verified` account's `Normal` session. In one AccountsObject transaction, revalidate current session, epoch, expiry and revocation state and insert the row. Thus a revocation committed first makes registration fail; a registration committed first leaves its row available to DO-036. If registration fails after a WSS upgrade/game attachment is prepared, close the socket and send no authorized snapshot. Revocation delivery/acknowledgements follow DO-036; expiry-driven close work and post-ack stale-row deletion follow approved DO-037. Sweep cadence and physical cleanup details remain implementation work.

**Approved — DO-036:** In the same AccountsObject transaction that commits session/account invalidation or a credential-epoch change, persist durable revocation work for affected DO-035 subscriptions. Send idempotent close commands to each owning GameObject. Acknowledge only after the target `connection_id` is closed or already absent, then retire that subscription/work target; retry unacknowledged delivery. Do not claim all sockets are closed while any target lacks acknowledgement. Before each account-authorized outgoing WSS frame, the GameObject revalidates current account session/epoch; on stale/revoked authority or unavailable lookup, send no frame and close the connection. Cross-Object delivery is not an atomic network close: a frame authorized/in flight before the revocation commit may race with it. Outbox/target schema, delivery IDs, retry scheduling and physical transaction details remain implementation work.

**Approved — DO-037:** A subscription becomes ineligible at its linked account session's fixed `expires_at`; expiry never extends the deadline or increments the account epoch. Before every account-authorized outgoing WSS frame, the DO-034 authoritative check denies an expired session; send no frame and close the socket. A retryable expiry sweep may discover due subscriptions and atomically enqueue per-connection close work in AccountsObject. Use DO-036 idempotent GameObject close/acknowledgement, retaining the subscription/work target until the connection is closed or already absent; then delete its metadata. A delayed sweep cannot grant authority because per-frame checks enforce expiry. Work is bounded/paginated and idempotent; add no retention period after acknowledgement. Session-row cleanup is DO-034; access-link/receipt retention remains DO-029. Exact SQL, sweep cadence/implementation and physical outbox fields remain TBD.

<a id="game-records"></a>

### 6.5 Game-owned live records

All records here belong to the owning game's private storage. Proposed fields preserve the confirmed lifecycle, attendance and privacy rules; **TBD — physical mappings and complete local/cross-owner transaction design.**

#### `GameRecord`

```rust
struct GameRecord {
    game_id: GameId,
    game_code: Option<GameCode>,

    state: GameState,
    created_by_account_id: AccountId,
    designated_host_id: AccountId,
    host_assignment_revision: Revision,

    revision: Revision,

    created_at: Timestamp,
    lobby_opened_at: Option<Timestamp>,
    started_at: Option<Timestamp>,
    ended_at: Option<Timestamp>,

    last_qualifying_host_activity_at: Timestamp,
    idle_cancel_due_at: Option<Timestamp>,

    terminal_outcome: Option<TerminalOutcome>,
    history_expires_at: Option<Timestamp>,
}

enum TerminalOutcome {
    Resolved {
        winner_player_id: PlayerId,
        awarded_by_account_id: AccountId,
    },
    Cancelled {
        reason: CancellationReason,
        initiated_by: ActorRef,
    },
}

enum CancellationReason {
    OperatorCancelled,
    HostIdleTimeout,
}
```

`ActorRef` is defined in Section 6.7. `terminal_outcome` / `ended_at` are absent before termination; Resolved requires exactly one winner, Cancelled none. Host-idle cancellation applies only before start. `history_expires_at` exists only for started games after terminal commit. Terminal status, outcome and expiry cannot be renewed by retries/reads. Full live metadata need not survive into History; Section 6.6 defines proposed retained content.

**Approved — DO-038:** Preserve the shown logical fields/types and `GameState::{New, AwaitingPlayers, InProgress, Resolved, Cancelled}`. Creation starts in New with `revision = 0`, `host_assignment_revision = 0`, creator as designated host, `created_at = last_qualifying_host_activity_at = creation time`, and no code/lobby/start/end/outcome/history expiry; the initial pre-start idle deadline is last qualifying designated-host activity +24 hours. Allocate `game_code` and set `lobby_opened_at` on entry to AwaitingPlayers; set `started_at` at InProgress. Before terminal, `ended_at` and `terminal_outcome` are absent. Resolved pairs with exactly one winner and `TerminalOutcome::Resolved`; Cancelled has no winner and `TerminalOutcome::Cancelled`, with reasons OperatorCancelled or HostIdleTimeout and the initiating ActorRef. HostIdleTimeout applies only before start. Pre-start cancellation deletes game/participant data after commit and creates no History. `history_expires_at` exists only for started terminal games, three calendar months after the terminal time. Terminal status/outcome/expiry do not change on reads/retries.

**Approved — DO-039:** In New/AwaitingPlayers/InProgress only, revalidate current designated-host/admin actor and distinct enabled Verified Host target with no Directory assignment gate. Require `expected_host_assignment_revision`; stale/missing versions reject without mutation. The GameObject commit atomically changes `designated_host_id`, advances both `host_assignment_revision` and game `revision` once with checked arithmetic, and writes an actor-scoped `CommandReceipt` with the authenticated `ActorRef::Account`, operation, request fingerprint, safe outcome and committed revision. An exact actor/command/fingerprint retry within the applicable live-receipt window returns the receipt without another increment; command-ID reuse with a different fingerprint rejects. Expired-receipt replay remains governed by DO-076–DO-084. Preserve game data/reservation; no recipient acceptance. A removal gate acquired first rejects transfer; a committed transfer first must be visible to the removal hosted-game check, which then blocks disable/delete while the game remains nonterminal. Do not rely on a gate precheck alone; recover interrupted work without losing either operation.

**Approved — DO-040:** Only intentional open/resume or accepted, state-changing host action by the current designated host renews the 24-hour idle deadline by setting `last_qualifying_host_activity_at` to the trusted commit time and `idle_cancel_due_at` to that time +24 hours. A designated-host-initiated transfer qualifies once; a non-designated admin's override/open/resume, including an admin-initiated transfer, does not renew or re-anchor it. The successor inherits the existing deadline. Reads/lists/heartbeats/reconnects, rejected/no-op commands and retries do not renew it. DO-041 separately governs races at the deadline.

**Approved — DO-041:** At the serialized GameObject transaction point, compare trusted time against `idle_cancel_due_at`; the deadline is end-exclusive (`now >= due` is expired). Before the deadline, an accepted designated-host renewal may replace it or valid Start may commit InProgress and clear it while retaining the global reservation; stale alarms then no-op. At or after the deadline, expiry wins: the observing command/alarm rejects late Start/open/host actions, commits Cancelled with `HostIdleTimeout`, performs the approved pre-start deletion/no-History path, then compare-by-game-ID reservation release. An early/stale alarm rereads state/deadline and reschedules if still future; never cancel InProgress/terminal or release a newer reservation. Transaction linearization time, not request arrival, alarm delivery or browser time, decides; exactly-at-deadline loses. No grace period.

**TBD:** Physical SQL/index details, alarm scheduling/delivery, cross-Object terminal/deletion/release recovery and target timer implementation.

#### `GameConfiguration`

```rust
struct GameConfiguration {
    numeric_upper_bound: u32,
    board_side_length: u8,

    free_cells_enabled: bool,
    free_cell_positions: Vec<CellPosition>,

    player_capacity: u8,
    spectator_capacity: u8,

    winning_pattern: WinningPattern,
}
```

The initial numeric range produces **string** values such as `"1"` and `"75"`; no duplicate authoritative array of every pool value is proposed. Preserve the approved defaults/ranges in [LLD Section 2.1](lld.md#21-architecture-and-domain-constraints). Free positions must be unique and in bounds; configuration is immutable after lobby opening. Derive remaining values/exhaustion from configuration and accepted calls rather than a separate mutable `remaining_values` authority.

**Approved — DO-042:** Preserve all shown logical fields/types. `numeric_upper_bound: u32` defaults to 75 and defines string values `1..=numeric_upper_bound`; require a value in 1–1,000 inclusive (DO-043). DO-044 approves complete configuration validation and start-time feasibility. `board_side_length: u8` defaults to 5 and is square-only in 2–10. `free_cells_enabled: bool` defaults true; when false, `free_cell_positions` must be empty; when true, require one or more unique, one-based, in-bounds `CellPosition`s. Default placement is the center using the established ceil-half coordinate rule. `player_capacity: u8` defaults 20 (range 2–20); `spectator_capacity: u8` defaults 50 (range 0–50; zero disables admission). `winning_pattern` defaults to the sole first-release `SingleLine` variant. Configuration is mutable only in New and freezes at AwaitingPlayers. No stored remaining-values array or extra field. Physical child-table layout remains DO-009/implementation work.

**Approved — DO-043:** `numeric_upper_bound` must be between 1 and 1,000 inclusive. Default remains 75; numeric values are strings `1..=upper_bound`. At most 1,000 distinct accepted call records can exist per game; keep the pool implicit, without preallocating/storing a range array. This is a product guardrail, not a measured Cloudflare/SQLite capacity claim.

**Approved — DO-044:** Validate the complete resulting configuration on create/update and again before Start: numeric upper bound 1–1,000, square side 2–10, player capacity 2–20, spectator capacity 0–50, DO-042 free-cell constraints, and the sole first-release SingleLine pattern. Let `N = numeric_upper_bound` and `k = board_side_length² - configured_free_cell_count`. Reject when `N < k`. The number of cell-by-cell-distinct layouts is the falling factorial `P(N,k) = N × (N−1) × … × (N−k+1)`; `P(N,0) = 1`. Count only up to the retained start-time player membership count by saturating multiplication there, avoiding overflow. Reject Start if the feasible-layout count is less than the number of retained memberships, including disconnected players (HLD-034); the separate minimum of two currently connected players still applies. Invalid configuration patches apply nothing. Random sampling/generation, SQL/indexes and transaction mechanics remain implementation design.

**TBD:** Physical table layouts under the DO-009 collection convention.

#### `PlayerRecord`

```rust
struct PlayerRecord {
    player_id: PlayerId,
    alias: String,

    joined_at: Timestamp,
    session_epoch: Revision,
    last_explicit_leave_at: Option<Timestamp>,
}
```

`alias` preserves the accepted spelling/case after HLD-051 outer-space trimming. Store no duplicate comparison key or case-folded value. DO-045 approves exact case-sensitive comparison of trimmed aliases. Enforce uniqueness atomically among retained PlayerRecords in the same game; `Alice` and `alice` are distinct, and aliases may be reused across games. Pre-start Leave deletes the membership/releases the alias; In Progress Leave retains player/board/award eligibility. No active connection is not membership deletion. Do not persist an authoritative `is_connected` Boolean on this record; it becomes stale after connection loss/recovery. DO-048 approves `last_explicit_leave_at` as event history: update it for each explicit In Progress Leave and retain the most recent timestamp after return; it is not a current-away indicator.

**DO-045 — Approved:** Store only the accepted trimmed `alias` in PlayerRecord, with display casing unchanged and exact case-sensitive uniqueness/recovery comparison. Preserve the other shown PlayerRecord fields/types. Alias input follows HLD-050/051. Per-game uniqueness comparison is serialized and atomic. Physical keys/indexes remain TBD. DO-046 approves stable-`player_id` session binding; DO-055 still covers complete ParticipantSessionRecord fields and live/final access constraints.

**DO-046 — Approved:** In Awaiting Players, a valid player session may rename only its own membership to a valid, available alias using exact case-sensitive comparison. Atomically update the PlayerRecord alias and release the old alias while preserving `player_id`, seat, join time, role/capacity and recovery answer. Invalid/claimed aliases and late renames fail without mutation. Player sessions bind by stable `player_id`, not the alias; alias updates do not rewrite/rotate/revoke credentials or renew fixed expiry. Views and future recovery resolve the current alias from PlayerRecord. This does not approve the full ParticipantSessionRecord schema/access rules (DO-055), response/retry contracts, or physical SQL/indexes.

**DO-047 — Approved:** In Awaiting Players, role switch requires target-role capacity and commits atomically. Player→spectator releases player alias/seat, deletes PlayerRecoveryRecord, retires player-role sessions/sockets and creates spectator membership/session. Spectator→player requires a valid available alias/player slot, creates a fresh PlayerRecord/PlayerId and optional fresh answer, and retires spectator authority; it never restores deleted membership/answer. Replace the role-bound session at its original absolute expiry (no extension). Rejection preserves the original role/seat/credentials/verifier. Explicit pre-start player Leave removes membership/recovery/session and releases alias/seat; later join is fresh admission. In Progress player Leave retains membership/seat/board/matches/winner eligibility; a still-valid session may return until its fixed expiry, otherwise the established answer recovery applies. Spectator Leave immediately releases seat and revokes/deletes its session; later entry is fresh admission if capacity remains. Serialize against start/admission; no partial role/seat state. DO-048 approves retaining the latest explicit In Progress Leave timestamp after return; full session schema/access and physical transaction/fencing mechanics remain DO-052/055/implementation.

**TBD:** Physical keys/indexes and connection-derived presence mechanics.

#### `PlayerRecoveryRecord` — private material

```rust
struct PlayerRecoveryRecord {
    player_id: PlayerId,
    answer_verifier: EncodedVerifier,
    normalization_version: u32,
    updated_at: Timestamp,
}
```

No row means recovery disabled; no plaintext answer is stored. Delete on player-to-spectator switch, membership removal or terminal transition. Replacement is atomic. `normalization_version` prevents silent changes to existing answer-comparison rules. Never include this record in public projections or History.

**DO-049 — Approved:** Preserve `player_id: PlayerId`, `answer_verifier: EncodedVerifier`, `normalization_version: u32`, and `updated_at: Timestamp`. Absence means answer-based recovery is disabled; store no plaintext, and never expose the record in projections or History. Delete on accepted player-to-spectator switch, membership removal or terminal transition; answer replacement is atomic. DO-050 approves normalization/version behavior; DO-051 approves the Argon2id/PHC verifier profile and benchmark gate; DO-052 decides recovery/replacement transaction and session/socket fencing.

**DO-050 — Approved:** Version 1 canonicalization applies Unicode NFC, trims outer Unicode whitespace, then applies Unicode case-folding; preserve internal whitespace and punctuation without collapsing/stripping them. Use the stored `normalization_version` to select the comparison profile for each verifier. A future profile change must not silently reinterpret an existing verifier: retain support for its old version until the player replaces the answer while authenticated, write the replacement with the current version, and never store plaintext or silently migrate a verifier.

**TBD — exact Unicode data/library pin and canonicalization implementation; production Argon2 costs/caps and target-runtime measurements; throttling; and recovery/replacement transaction/session fencing (DO-052).**

**DO-051 — Approved:** Use Argon2id v19 for player recovery answers, with a fresh independent 16-byte CSPRNG salt, 32-byte output and a complete PHC-encoded `EncodedVerifier`. Explicitly configure bounded memory/time/parallelism costs and reject malformed, unsupported or out-of-bounds verifier parameters before expensive work. Use 19 MiB / 2 iterations / parallelism 1 only as a benchmark starting point; production costs/caps remain pending target-runtime/resource measurements. This profile does not settle DO-052 throttling, recovery transaction, retry or session/socket-fencing details.

#### `SpectatorRecord`

```rust
struct SpectatorRecord {
    spectator_id: SpectatorId,
    joined_at: Timestamp,
    session_epoch: Revision,
    disconnected_at: Option<Timestamp>,
    grace_expires_at: Option<Timestamp>,
}
```

No player alias, board or recovery-answer record. The row represents occupied spectator capacity, including the allowed disconnected grace period; delete on explicit Leave, grace expiry or terminal cleanup.

**TBD — reconnect/grace races, presence reconstruction, stale-socket fencing and notice/deletion ordering.**

#### `ParticipantSessionRecord`

```rust
struct ParticipantSessionRecord {
    session_id: SessionId,
    token_verifier: Digest,

    game_id: GameId,
    game_code: GameCode,
    participant: ParticipantBinding,

    session_epoch: Revision,
    access: ParticipantAccess,

    issued_at: Timestamp,
    expires_at: Timestamp,
    revoked_at: Option<Timestamp>,
}

enum ParticipantBinding {
    Player {
        player_id: PlayerId,
    },
    Spectator {
        spectator_id: SpectatorId,
    },
}

**DO-046 approves the player-binding rule:** Bind by stable `player_id` only; do not persist a duplicate alias or derived key in the binding. Alias rename therefore changes the PlayerRecord but does not rewrite the session binding or extend its deadline. DO-055 remains pending for the complete session-record fields/types and live/final access constraints.

enum ParticipantAccess {
    Live,
    FinalViewOnly,
}
```

Only eligible existing player sessions can become `FinalViewOnly`; spectator sessions are deleted at termination. Rename updates the binding without renewing expiry. Answer recovery increments the player's session epoch, replaces prior sessions/sockets and restores the same membership. Live access also requires membership; after terminal cleanup a reduced player session can reference the final snapshot instead. Exit removes final-view authority. History cannot mint new participant sessions; pre-start deletion still permits no reconnect.

**TBD — exact minimal terminal record, token/cookie binding, atomic recovery/rename/role switch, epoch fencing, exit/replay guards and deletion.**

#### `AccountGameViewAccess`

```rust
struct AccountGameViewAccess {
    account_id: AccountId,
    session_id: SessionId,
    entered_at: Timestamp,
    expires_at: Timestamp,
    access: AccountGameAccess,
}

enum AccountGameAccess {
    LiveView,
    FinalViewOnly,
}
```

Tracks account viewers independently of anonymous participant sessions, distinguishing an existing final-screen viewer from a later History reader. It does not replace current account/session authority. Exit removes the grant; terminal requests cannot recreate it as live-game admission. Grants cannot outlive their underlying credentials or renew them.

**TBD — exact account-view eligibility/grant lifecycle, multi-session Exit scope, final-view downgrade and minimal retention.**

#### `PlayerBoardRecord`

```rust
struct PlayerBoardRecord {
    player_id: PlayerId,
    side_length: u8,
    cells: Vec<BoardCell>,

    assigned_at: Timestamp,
    qualifying_lines: Vec<CompletedLine>,
    evaluated_through_call: u32,
}

struct BoardCell {
    position: CellPosition,
    kind: BoardCellKind,
    is_matched: bool,
}

enum BoardCellKind {
    Free,
    Value(String),
}

enum CompletedLine {
    Row(u8),
    Column(u8),
    MainDiagonal,
    AntiDiagonal,
}
```

`Free` is valueless, not a pool value `"FREE"`, and is matched immediately. Ordinary cells match only through accepted calls. Empty `qualifying_lines` means no current qualification; `evaluated_through_call = 0` is initial evaluation before calls. One board per player exists only after Start. Exact complete layouts determine uniqueness; any optional digest optimization must not replace exact equality. These data types do not replace the separately requested pattern-specific Rust traits. Cells/matches/qualification are persisted projections of the immutable layout and committed calls, updated consistently with each call.

**TBD — collection/row layout, uniqueness strategy, feasible generation/randomness, matching algorithms, consistency checks and trait signatures.**

#### `CallRecord`

```rust
struct CallRecord {
    sequence_no: u32,
    value: String,
    mode: CallMode,
    called_by_account_id: AccountId,
    called_at: Timestamp,
    command_id: CommandId,
}
```

Suggested uniqueness: `sequence_no` and `value` within the game. Calls, affected matches/qualification, revisions and command receipts commit together before delivery. Derive the most recent value from the highest sequence number rather than a second authoritative field.

**TBD — keys, sequencing/overflow rules, actor-scoped receipt relationship and SQL transaction/statements.**

#### `ViewRevisionRecord`

```rust
struct ViewRevisionRecord {
    view: ViewKey,
    revision: Revision,
}

enum ViewKey {
    Host,
    Player(PlayerId),
    Spectator(SpectatorId),
}
```

These revisions describe authorized projections, not raw internal mutation counts. One player's private recovery change must not create an unexplained gap in another player's view. `Host` is a shared view-key proposal, not a grant of designated-host/admin mutation rights.

**TBD — exact projection boundaries, account-specific authority changes, snapshot/subscription ordering, revision advancement, gap recovery and terminal cleanup.**

<a id="history-records"></a>

### 6.6 Final History inside the original Game Object

**Approved placement (DO-002):** retain final History in the original Game Object, with only minimal searchable index/code/expiry metadata in the Directory and no separate History Object/datastore. The separate immutable logical snapshot below remains a field/layout proposal rather than approval to keep all live tables as History. **TBD — snapshot fields, physical child tables and terminal materialization/cleanup protocol.**

```rust
struct GameHistorySnapshot {
    game_id: GameId,
    game_code: GameCode,
    designated_host_id: AccountId,

    started_at: Timestamp,
    ended_at: Timestamp,
    expires_at: Timestamp,

    outcome: HistoryOutcome,
    winner: Option<WinnerSnapshot>,
    ordered_calls: Vec<String>,
    players: Vec<HistoryPlayerSnapshot>,
}

enum HistoryOutcome {
    Resolved,
    Cancelled,
}

struct WinnerSnapshot {
    player_id: PlayerId,
    alias: String,
}

struct HistoryPlayerSnapshot {
    player_id: PlayerId,
    alias: String,
    side_length: u8,
    cells: Vec<BoardCell>,
}
```

`ordered_calls` and `players` are logical ordered collections; **DO-009 selects child rows**, preserving order rather than one large serialized blob. Exact table/key layouts remain **TBD**. Require winner presence exactly for Resolved, referring to the same game's final player snapshot. The snapshot retains final calls, aliases, winner if any and final board layout/values/matches, plus minimal identity/expiry metadata.

Exclude passwords, tokens, verifiers, spectator identities, live sessions/presence, recovery metadata, intermediate board revisions and unrelated editable configuration. Minimal existing final-view grants remain separate from History and never confer a new History entitlement.

Proposed terminal work: freeze final content and its fixed three-month deadline; delete recovery/spectator data; reduce eligible existing player/account access to final-view-only authority; remove obsolete live data when durable snapshot/coordination are safe. This list does not select delivery ordering: connected spectators still receive the permitted terminal notice before their server data is deleted, with no indefinite wait for acknowledgement. Pre-start cancellation creates **no** snapshot and follows full-deletion rules.

Directory and Game must use the same persisted History deadline. **TBD — calendar-month/timezone/month-end calculation, snapshot atomicity, notice/revocation/deletion ordering, scheduled purge, index/copy/log cleanup and restore-time expiry enforcement. Never substitute a fixed number of days or reset expiry on inspection.**

<a id="operational-records"></a>

### 6.7 Shared operational records

These records belong to whichever Object owns the operation; they do not introduce additional Object classes. **TBD — exact per-operation owner/key mapping, bounded contents and retention.**

#### `CommandReceipt`

```rust
struct CommandReceipt {
    command_id: CommandId,
    actor: ActorRef,
    operation_name: String,
    request_fingerprint: Digest,
    outcome: SecretFreeJson,
    committed_revision: Option<Revision>,
    completed_at: Timestamp,
    expires_at: Timestamp,
}

enum ActorRef {
    Account(AccountId),
    Player(PlayerId),
    Spectator(SpectatorId),
    DeveloperCli,
    System,
}
```

Use an actor-scoped command key; reject the same ID with a different fingerprint. Store only a bounded safe result, never tokens, password inputs, recovery answers or an indefinitely retained private snapshot. Once a receipt expires, reject inadmissibly old retries rather than executing them again as new commands. `DeveloperCli`/`System` do not imply public roles or bypass account/game guards.

**TBD — fingerprint construction, actor/scope key, typed outcome variants, original-actor authorization, retention/admissibility window and safe retry handling for credential-issuing operations.**

#### `PendingOperation`

```rust
struct PendingOperation {
    operation_id: OperationId,
    operation_name: String,
    target: TargetRef,
    phase: CoordinationPhase,
    expected_revision: Option<Revision>,
    fence_generation: Option<Revision>,
    payload: SecretFreeJson,
    created_at: Timestamp,
    next_attempt_at: Timestamp,
    attempt_count: u32,
}

enum TargetRef {
    Game(GameId),
    Account(AccountId),
    Session(SessionId),
}

enum CoordinationPhase {
    Prepared,
    LocallyCommitted,
    AwaitingPeerAcknowledgement,
}
```

Proposed durable outbox/coordination state covers publication, reservations, host transfer, account removal, revocation and index cleanup. Replace generic payloads with typed operation variants in the final design. Do not accumulate credentials or private History copies in these records. Fences and expected revisions require enforcement; fields alone do not create cross-Object atomicity.

**TBD — per-operation state machines/payloads, peer identity/trust, retry/backoff limits, completion/removal, reconciliation and orphan cleanup.**

#### `RateLimitBucket`

```rust
struct RateLimitBucket {
    scope: RateLimitScope,
    subject_key: Digest,
    window_started_at: Timestamp,
    attempt_count: u32,
    blocked_until: Option<Timestamp>,
    expires_at: Timestamp,
}

enum RateLimitScope {
    AccountLogin,
    AccessLinkRedemption,
    PlayerRecovery,
}
```

Account-related buckets belong in Accounts; recovery buckets belong in the owning Game alongside caller/game-wide controls. Avoid raw answers and unnecessary raw network identifiers. These enum values do not exhaust every abuse boundary or choose enforcement outside the Object.

**TBD — thresholds/windows, key construction, multi-scope caller/game limits, failed/successful-attempt accounting, retention and privacy.**

#### `AdminAuditRecord`

```rust
struct AdminAuditRecord {
    audit_id: AuditId,
    actor_account_id: AccountId,
    operation_name: String,
    target: TargetRef,
    outcome: AuditOutcome,
    occurred_at: Timestamp,
    expires_at: Timestamp,
}

enum AuditOutcome {
    Succeeded,
    Rejected,
    Failed,
}
```

Record the actual acting admin, not the designated host when different. Audit data must not become a hidden archive of deleted accounts, expired History or credentials. This record does not select a new audit-read API or UI.

**TBD — owner/storage, event contents, durable capture of rejects/failures, retention/deletion, privileged read policy and developer-CLI attribution if required.**

<a id="socket-metadata"></a>

### 6.8 Socket metadata versus persisted domain state

Proposed hibernation-compatible connection attachment:

```rust
struct ConnectionAttachment {
    connection_id: ConnectionId,
    session_id: SessionId,
    viewer: ViewerRef,
    authentication_epoch: Revision,
    session_expires_at: Timestamp,
    view: ViewKey,
}

enum ViewerRef {
    Account(AccountId),
    Player(PlayerId),
    Spectator(SpectatorId),
}
```

The attachment is connection metadata, not a bearer credential, durable membership or independent authority. Revalidate against authoritative account/session state; `authentication_epoch` corresponds to the applicable account or participant epoch. Count distinct currently connected valid-session players for Start, not sockets or all retained memberships. Preserve the confirmed one-live-socket-per-participant-session rule; stale socket closure cannot remove replacement presence. Store grace/expiry/idle deadlines durably, and drive cleanup from them rather than in-memory timers alone.

**TBD — attachment encoding/version/size, Rust SDK integration, hibernation revalidation, socket supersession, durable alarm scheduling, backpressure and DO-036 outbox/retry implementation. DO-036 approves per-frame authority revalidation and durable close acknowledgement policy; no provider or runtime behavior has been tested by these sketches.**

<a id="outstanding-decisions"></a>

### 6.9 Outstanding decisions — explicit TBD index

The field sketches provide proposed places to represent state; they do not finish the protocols below. The per-record TBD notes above remain authoritative detail prompts, not additional approved functionality.

| Area | Undetermined detail | Register |
| --- | --- | --- |
| Ownership / cardinality | **Approved — DO-001** Accounts placement/cardinality; **DO-002** History in the original GameObject with minimal Directory index. **TBD** — capacity/partition assessment, detailed ownership/interfaces and protocols. | LLD-003, LLD-012, LLD-015, LLD-018; DO-001, DO-002 |
| Types / identity / schema | UUID v7/typed IDs confirmed ([LLD Section 3.3](lld.md#identifier-policy)); **DO-003** epoch milliseconds, **DO-004** common revision rules and **DO-005** canonical UUID SQLite TEXT approved. **TBD** — constructor/target integration, API wire encoding, remaining physical mappings, table/column layout, constraints, keys/indexes, SQL and migrations. | LLD-004, LLD-020 |
| Directory coordination | **TBD** — publication, the single nonterminal reservation/fencing, assignment gates, transfer/removal races, authority freshness and recovery. | LLD-005 |
| Credential lifecycle | Password minimum/ASCII policy and salted Argon2/library selection confirmed in [LLD Sections 9.1–9.2](lld.md#password-policy); DO-019 approves AccountRecord fields/types/nullability and combined PHC storage in `verifier`. **DO-020** approves username length/characters. **DO-021** approves username casing/trimming/comparison. **DO-022** approves lifecycle/disablement and epoch transition/matching policy. **TBD** — username SQL/constraint and control-character integration, production Argon2 costs/caps and target-runtime/physical constraints, and remaining transaction/race/retry mechanics. DO-035/036 approve subscription registration and durable cross-Object revocation delivery/acknowledgement/per-frame revalidation; DO-037 approves expiry handling and cleanup policy, while sweep cadence, outbox/SQL details, self-removal and last-admin policy remain **TBD**. | LLD-009, LLD-023, LLD-024 |
| Player recovery | **Approved — DO-049/050:** logical `PlayerRecoveryRecord` fields/privacy and versioned Unicode NFC/outer-whitespace-trim/case-fold normalization. DO-051 approves Argon2id/PHC verifier profile and benchmark gate; production costs/caps remain measurement-gated. DO-052 throttling, atomic replacement/recovery and session/socket fencing remain pending. | LLD-010 |
| Immediate revocation | **Approved — DO-035/036/037:** session-bound subscriptions register transactionally against current authority; durable close work retries until per-connection acknowledgements; GameObjects revalidate authority before account-authorized WSS frames; expired subscriptions are swept into close work and removed after close/absence acknowledgement. A pre-commit-authorized/in-flight frame may race. **TBD** — outbox schema/IDs, sweep cadence, scheduling and physical integration. | LLD-005, LLD-007, LLD-009 |
| Game/configuration/boards | **Approved — DO-038:** GameRecord/TerminalOutcome fields, defaults and lifecycle invariants; **DO-042/043:** configuration and numeric-pool ceiling; **DO-044:** full configuration validation and start-time feasibility. **TBD** — random generation/sampling/matching, free-cell physical storage and distinct pattern-trait signatures. | LLD-004, LLD-008 |
| Attendance / final-view access | **Approved — DO-047/048:** role-switch/Leave state transitions, fixed-expiry session replacement, and historical explicit-Leave timestamp retention after return. **TBD** — presence reconstruction, DO-053/054 spectator record/grace behavior, DO-055 full participant-session schema/access, expiry scheduling, minimal terminal grants and Exit scope. | LLD-007, LLD-011 |
| Synchronization | **TBD** — role-specific projection/revision rules, snapshots/subscriptions, attachments, supersession and backpressure. | LLD-007 |
| Retry / pending work | **TBD** — typed command/outbox payloads, bounded receipt retention, old-command rejection, retry policy and reconciliation. | LLD-005 |
| History / deletion | **TBD** — month arithmetic, immutable materialization, terminal notice ordering, purge/index/copy cleanup and restore enforcement. | LLD-011, LLD-012 |
| Abuse / audit | **TBD** — rate-limit scopes/thresholds/key privacy, audit placement/content/read permissions and retention. | LLD-009, LLD-010, LLD-015 |
| Bindings / deployment / verification | **TBD** — names, trusted internal interfaces, SDK/toolchain integration, migrations, load/quota checks and executed tests. | LLD-004, LLD-013, LLD-014, LLD-015 |
| Open product interactions | **TBD** — remaining privileged account-management edge cases; DO-040 settles non-designated-admin host-idle renewal and DO-041 settles expiry/activity/start ordering. | [LLD Section 12.2](lld.md#122-productsource-questions--do-not-silently-decide-in-implementation) |


<a id="item-review"></a>

## 7. Item-by-item review

**Review scope:** Review the still-undetermined choices in this document one at a time. Existing confirmed HLD/LLD constraints are not reopened. The queue deduplicates the ownership worksheets, DATA inventory, record-level prompts and Section 6.9 index; generic worksheet blanks are coverage prompts, not extra decisions. Some primary items can need focused follow-up questions, which will be recorded without renumbering completed items. An accepted design or test plan is not evidence of executed code/tests, deployment, measured capacity or permission to implement.

**Pause rule — user direction:** If a `clarify` question times out, is cancelled, skipped or otherwise unanswered, do not advance or infer any disposition. Leave the current item pending, stop the walkthrough for that turn and wait for the user to return. On resumption, present the same unresolved item rather than moving to the next one.

**Recording:** Record each approval/revision/deferral/rejection here and reconcile affected wording in this document before continuing. Companion LLD/API/source reconciliation stays separate where outside this review's scope; reference any affected companion statuses explicitly rather than silently marking them complete. Untested target/runtime/quality claims remain unverified even after the design direction is approved.

**Review cursor:** Last completed approvals: **DO-041, DO-043–DO-051 — Approved** (DO-042 was approved independently). **DO-021's revised case-sensitive username comparison is also approved**, with its earlier trim/validation rules retained. Current item: **DO-052 — Recovery and answer-replacement transaction/session-socket fencing**. DO-046 approves atomic rename/current-alias behavior with stable-player-ID binding and unchanged session expiry; DO-047 approves role switch/Leave session lifecycle without expiry extension; DO-048 approves retaining the latest explicit In Progress Leave timestamp after return; DO-049 approves PlayerRecoveryRecord fields/privacy/deletion boundaries; DO-050 approves v1 normalization/versioned replacement; DO-051 approves Argon2id v19, independent 16-byte salt, 32-byte output and PHC verifier with 19 MiB/2/1 only as benchmark starting point. Production costs/caps remain pending target measurements. DO-022 approves account lifecycle/credential-epoch rules; `disabled_at` is the sole stored disablement state and the lifecycle variant is `Verified`. DO-023 approves Argon2id v19, fresh 16-byte salts, 32-byte output and one PHC `TEXT` verifier. DO-024 approves explicit costs, the 19 MiB / 2 / 1 benchmark starting point, bounded fail-closed parameter handling and pre-production Worker/WASM validation; production costs/caps and measurements remain pending. DO-025 approves conditional opportunistic rehash with current-verifier/epoch race checks and no session/epoch extension. DO-026 approves AccessLinkRecord logical fields, one-day fixed expiry and nullable consumed/revoked timestamps. DO-027 approves access-link token encoding/verifier; DO-028 approves safe canonical-origin delivery and atomic single-use redemption/session creation. DO-029 approves purpose-specific epoch-bumping reissue, predecessor invalidation, secret-free 30-day issuance receipts, lost-response recovery and post-event link metadata retention. DO-030 approves AccountSessionRecord fields and one-day fixed lifetime. DO-031 approves 32-byte CSPRNG session tokens, unique SHA-256 `BLOB` verifiers, `__Host-brews_session` cookie attributes and exact-Origin browser mutation/WebSocket checks; target RNG support remains unvalidated. DO-032 approves atomic enrollment completion and restricted-to-normal rotation without deadline extension. DO-033 approves no automatic post-reset login and password-login navigation. DO-034 approves current authoritative account/session checks without cross-request positive authorization caching and asynchronous session cleanup; DO-029 link/receipt retention is unchanged. DO-035 approves unique `connection_id`, fixed-expiry AccountSocketSubscription fields and transactionally revalidated Verified/Normal registration; DO-036 approves same-transaction durable revocation work, retryable close/acknowledgement and per-frame fail-closed authority checks. DO-037 approves expiry-driven durable close work, denial of expired sessions before outgoing frames, and deletion of subscription metadata after close/absence acknowledgement. DO-038 approves GameRecord/TerminalOutcome types/defaults, initial values, terminal consistency, and History/idle-deadline presence rules. DO-039 approves required expected-assignment revision, actor-scoped retry receipt, exactly-once checked counter updates and safe transfer/removal ordering. DO-040 approves designated-host-only renewal and excludes non-designated-admin activity. DO-041 approves end-exclusive deadline ordering, transaction-point trusted time, expiry-winning equality, stale-alarm rechecks and no grace period. DO-042 approves GameConfiguration fields/defaults, disabled free-cell representation and fixed-state rules. DO-043 approves the inclusive 1–1,000 numeric upper-bound ceiling (default 75) and implicit pool representation. DO-044 approves falling-factorial feasibility for all retained start-time memberships, with a saturating count and a separate connected-player start gate. DO-045 approves case-sensitive trimmed-alias comparison without a stored comparison key; ParticipantSessionRecord fields remain pending under DO-055.

**Companion reconciliation:** LLD Section 9.2 reflects DO-001's approved `AccountsObject` ownership. LLD Section 3.3 and the HLD/API contracts reflect DO-027/DO-028 token and redemption rules, plus DO-029 reissue/lost-response behavior, DO-030/DO-031 account-session/cookie decisions, DO-032 enrollment-completion rotation, DO-033 post-reset fresh-login behavior, DO-034 authoritative account/session validation and asynchronous session cleanup, DO-035 subscription ownership/registration, DO-036 durable retryable revocation and per-frame revalidation, DO-037 expiry-driven close work/subscription cleanup, DO-038 approved GameRecord/TerminalOutcome defaults/invariants, DO-039 host-transfer revision/actor/target-removal policies, DO-040 designated-host-only idle renewal, DO-041 exact-deadline/expiry race policy, DO-042/043 configuration defaults/free-cell constraints and numeric pool ceiling, DO-044 falling-factorial feasibility, DO-021/DO-045 approved case-sensitive username/alias comparisons, DO-046/047 approved rename, role-switch and Leave session semantics; DO-048 approved explicit Leave timestamp retention after return; DO-049/050 approved answer-record fields/privacy and versioned normalization; DO-051 approved Argon2id/PHC verifier profile with benchmark-gated costs. DO-052 recovery transaction/fencing details remain open. DO-029 sets link-metadata and secret-free receipt retention windows; outbox schema/scheduling, sweep cadence, physical receipt schema, DO-041 alarm/coordination implementation, random board sampling, and remaining implementation details stay open. ParticipantSessionRecord fields remain DO-055.

**Companion reconciliation — DO-002:** History-placement TBD wording in LLD-012 and related companion summaries predates DO-002 and is superseded here for placement only. Snapshot schemas, month arithmetic, coordination and restore mechanisms remain TBD. Companion documents are unchanged.

**Companion reconciliation — DO-005:** The UUID-storage-format TBD wording in LLD Section 3.3 predates DO-005 and is superseded here for SQLite encoding only. API wire encoding, constructors and target wiring remain open; companion documents are unchanged.

**Cross-cutting revision — HLD-077 / DO-014–DO-018:** The user confirmed one nonterminal game at a time, excluding Resolved/Cancelled. DO-014–016 define its nullable-ID reservation and compare-by-ID lifecycle. DO-017 approves a presence-only account-assignment gate keyed by account ID; DO-018 approves acquiring it before the hosted-game check and holding it through disable/delete, with clear-and-reject when hosted games exist and retain-and-retry on interruption.



**Cross-cutting scope clarification — HLD-078 / DO-022:** Account-enable functionality and an API are required for disabled host/admin accounts. Only another fully enrolled admin account with valid authority, or the developer CLI, may execute it; the target account cannot enable itself. Ordinary hosts, players, spectators and unauthenticated callers are excluded. Enabling is distinct from password reset, does not grant a new role or recreate a deleted account, and must not revive revoked/expired credentials. **Approved enable outcome:** restore the pre-disable lifecycle state: Verified permits a fresh login with the existing password; PendingEnrollment still requires setup; ResetRequired still requires reset. Previously revoked/expired sessions and links stay invalid. Derive account disablement solely from populated `disabled_at`, preserving lifecycle status through disable/enable without a previous-status copy or redundant boolean. Populate the timestamp to disable and clear it to enable; authorized no-op retries do not change it. Fresh setup/reset links after enable use the existing flows. DO-022 also approves credential-epoch issuance, increment and match rules recorded in the AccountRecord subsection; DO-036 separately approves durable cross-Object socket-revocation work/acknowledgement and fail-closed frame checks, while physical transaction/outbox integration remains open. No implementation is authorized. The API proposal is B9/API-33; exact method/path/body details remain proposals.

**Cross-cutting timestamp revision during DO-022:** The user superseded the earlier stored-boolean choice: use `disabled_at` population for account disablement and do not persist equivalent booleans for any stored state already represented by a corresponding timestamp. Account enable/disable authority, preserved lifecycle/verifier and no-op retry semantics remain as recorded above. The user subsequently approved the credential-epoch rules, completing DO-022; DO-036 separately approves cross-Object socket-revocation policy, while transactional/outbox implementation details remain TBD.

**Cross-cutting account-status rename during DO-025:** Rename the account lifecycle status variant to `AccountStatus::Verified`; the lifecycle meaning is unchanged. Successful first setup/reset yields `Verified`, and an enabled `Verified` account may authenticate with its verifier; `disabled_at` independently controls disablement. Reconcile API/HLD/LLD and all lifecycle references; do not change unrelated uses of “active” for games or other state.

### 7.1 Approved decisions

- **DO-001 — Approved:** Use one separate SQLite-backed `AccountsObject` per application environment for all host/admin accounts, usernames, password verifiers, enrollment/reset links and account sessions. Keep anonymous player/spectator sessions in their owning `GameObject`. This approval covers placement/cardinality only, not record fields, Argon2 parameters, binding names, capacity or revocation/coordination mechanisms. No implementation/deployment is authorized.

- **DO-002 — Approved:** Store each started game's final History in its original GameObject; Directory holds only minimal searchable index/code/expiry metadata, not player boards or full snapshots. No new HistoryObject/datastore. Placement only; fields, tables, calendar expiry and cleanup protocols remain open. Existing three-calendar-month retention and no-History pre-start cancellation are unchanged.

- **DO-003 — Approved:** Timestamp is i64 / SQLite INTEGER containing UTC Unix epoch milliseconds since 1970-01-01T00:00:00Z. The trusted backend record owner supplies action/creation times and durable deadlines; browser time and UUID v7 time are not authoritative. Explicit sequences/revisions govern ordering. Calendar-month arithmetic, clock rollback/skew handling and target integration remain TBD.

- **DO-004 — Approved:** Revision is a nonnegative i64 / SQLite INTEGER initialized at 0. Required advancement occurs in the owning Object transaction; reads, rejected requests and retries do not themselves increment it. Reject required advancement at the maximum rather than wrapping/resetting. Per-counter triggers remain for their lifecycle/protocol reviews; no timestamp or global ordering semantics.

- **DO-005 — Approved:** Application-generated UUID v7 IDs use SQLite TEXT in canonical lowercase hyphenated representation. Rust keeps distinct uuid::Uuid-backed ID types; storage conversion validates version 7, rejecting malformed/wrong-version IDs without repair. API wire encoding and concrete constructors/target wiring remain separately TBD.

- **DO-006 — Approved:** Use a shared domain ID module with distinct uuid::Uuid-backed types and private inner fields. TryFrom<uuid::Uuid> and FromStr validate version 7; formatting is canonical lowercase hyphenated UUID text. ID errors use adjacent error.rs / thiserror. Generate once in the authorized creation component and retain identity on retries. Concrete implementation and target clock/randomness wiring remain unverified/TBD; CommandId browser issuance and API wire encoding are not selected.

- **DO-007 — Approved:** Approve stable snake_case enum tags as SQLite TEXT, with mandatory Rust strum-generated conversion/parsing from enum definitions. Unknown tags error; no numeric/debug/default fallback. Preserve exact domain/UI lifecycle names. Changes to stored labels require explicit compatibility/migration; tag/payload mappings, pinned features/version and executed tests remain TBD.

- **DO-008 — Approved:** CellPosition uses row: u8 and column: u8 mapped to SQLite INTEGER, with existing one-based indexing. Validate against owning board side length and reject zero, negative/out-of-bounds values without clipping or wrapping. Board sizes/free-cell rules are unchanged; per-cell physical table/index layout remains TBD.

- **DO-009 — Approved:** Persist Vec<T> collections as child rows, with explicit sequence/position wherever order matters; an empty collection has no child rows. Optional scalar Option<T> fields use SQL NULL, not sentinel strings/IDs/numbers. Optional whole entities may use row absence. Defaults must be explicitly specified per record. Table names, relationships, indexes and field-specific constraints remain TBD.

- **DO-010 — Approved:** Each Directory, Accounts and Game Object has one persistent local StorageMetadata row with schema_version: u32 / SQLite INTEGER. An initialized schema starts at version 1; record the new version only on successful committed local initialization/migration. Refuse normal operations on unsupported newer versions; no guessing/silent downgrade. Rollout/backout, compatible migrations and SQL remain TBD.

**DO-011 rename reconciliation:** The requested `ended_at` name is applied to GameIndexRecord, GameRecord, GameHistorySnapshot, FinalResultView and the winner-result description. The subsequent clarification explicitly approves the combined live/History index; no historical-only premise remains pending. Lifecycle, retention, field types/nullability and implementation scope are unchanged.

- **DO-011 — Approved:** GameIndexRecord fields/types and nullability approved with ended_at, as a combined live-game and retained-History index. Pending/Published are publication metadata, not lifecycle states. End time is absent before ending; History expiry only for started terminal games. Retained terminal codes reserve through History expiry without admission/recovery; pre-start cancellation removes the entry. Indexes/freshness/coordination remain separate.

- **DO-012 — Approved:** Game index uses game_id as primary key and allocated game_code as unique; uncoded entries may coexist. Query indexes on designated_host_id, state and non-NULL history_expires_at. Public discovery requires Published and Awaiting Players/In Progress; History requires started Resolved/Cancelled with unexpired deadline. Role/session and Game authority checks remain required. SQL/index names, query plans and expiry/reuse coordination remain TBD.

- **DO-013 — Approved:** GameObject is authoritative. For each directory-visible committed change, project its Game revision. Initial Awaiting Players publication is hidden/Pending until configured state/code is committed in Game and Directory accepts the matching projection. Afterwards accept only a strictly newer source_revision; exact duplicate is a no-op and older revisions are ignored. Already-published codes remain Published during subsequent projected lifecycle changes. Discovery/listing is not authority: Game rechecks current state/code/permissions before admission or mutation. DO-015 separately approves compare-by-game_id reservation fencing; cross-Object delivery/retry/recovery and code reuse remain separate TBDs; list staleness and initial-publication completion reporting remain unresolved.
- **DO-014 — Approved:** Persist one Directory-owned `GlobalReservation` row per application environment with only `game_id: Option<GameId>`. `NULL` means free; a game ID means the single nonterminal-game slot is occupied from New through Awaiting Players/In Progress. Release after the GameObject durably commits Resolved/Cancelled. Do not persist operation ID, fencing generation or phase in this record. DO-015 defines the ID-based acquire/release guard; terminal retry scheduling remains DO-016. No cross-Object transaction is implied.

- **DO-015 — Approved:** Claim the Directory slot first with an atomic `NULL → game_id` compare-and-set, then initialize the GameObject under that same ID. If GameObject creation fails, keep the ID reserved; a later attempt checks the existing ID and recreates the same GameObject, preserving any code already associated with it. Start retains the same reservation and does not acquire another. Release only when the stored `game_id` equals the game being released; because game IDs are never reused, this prevents stale work from clearing a newer game without a generation counter. Command-receipt/idempotency details remain DO-076–DO-084; terminal retry behavior is DO-016. Directory and GameObject still have no shared transaction.
- **DO-016 — Approved:** Commit Resolved/Cancelled durably in the GameObject before releasing the Directory slot. Compare-and-clear only when the stored `game_id` equals that terminal game ID; if already NULL, treat release as complete; if a different ID is present, treat the request as stale and do not clear it. Retry safely after an uncertain acknowledgement. Exact dispatch/scheduling/backoff and command-receipt details remain open; no timeout alone frees the slot.
- **DO-017 — Approved:** Keep one Directory-side `AccountAssignmentGate` row keyed only by `account_id`; row presence blocks new-game/host assignment during account disable/delete. No operation ID, generation or timestamp is persisted in the gate record. Acquisition/release and recovery remain DO-018.
- **DO-018 — Approved:** Acquire the account’s Directory gate before checking for hosted nonterminal games. While held, reject new-game/host-transfer assignments to that account. If a hosted game exists, clear the gate and reject removal; otherwise disable/delete in AccountsObject and then clear the gate. On interruption, keep the gate and retry safely. DO-039 approves the high-level ordering against a concurrent transfer; durable coordinator, receipt ownership and physical recovery details remain **TBD**.

- **DO-019 — Approved:** Approve AccountRecord fields/types and nullability, renaming the account password field to `verifier: Option<EncodedVerifier>`. Keep one complete library-generated Argon2 PHC string (algorithm/version, work parameters, salt and derived hash), absent before password setup; no separate salt/hash fields. Other AccountRecord fields are unchanged. Username policy, exact account-state/epoch transitions, Argon2 variant/parameters, physical constraints and runtime/upgrade protocols remain separately TBD.

**Companion reconciliation — DO-019:** LLD schema-navigation and password-storage references now use `AccountRecord.verifier` and record the approved combined PHC representation. Cryptographic salt terminology is unchanged. API bodies still exclude private account credentials; no API contract change is required.

- **DO-020 — Approved:** Usernames are 10–50 ASCII characters, excluding all ASCII whitespace: HT/tab, LF, VT, FF, CR (0x09–0x0D) and ordinary space (0x20). Other ASCII control characters, including NUL and DEL, remain permitted; do not silently narrow to printable ASCII. Apply to host/admin provisioning via CLI and admin UI. Count decoded characters; no password/player-alias policy change. DO-021 approves trimming and exact case-sensitive username uniqueness/login. Transport/UI/storage handling of permitted controls must be verified.

- **DO-021 — Approved (revised):** Retain trimming, validation order, length and permitted-character rules: trim leading/trailing ASCII whitespace, then enforce DO-020's 10–50 ASCII characters with no internal ASCII whitespace; preserve trimmed original casing. Case-only variants are distinct usernames; use exact case-sensitive equality for uniqueness and login, with no case folding or separate normalized key. Password rules are unchanged. SQL/constraint and control-character integration remain TBD.

**DO-022 — Approved:** Account lifecycle, enable/disable behavior and the account-wide credential-epoch policy are recorded in [the AccountRecord section](#account-records). Epoch begins at zero; increments on replacement enrollment-link issuance, reset-link issuance/reissuance, successful setup/reset completion and actual disable transition. Issued links/sessions carry the current epoch and must match it during validation alongside purpose/scope, expiry, revocation, lifecycle and timestamp-derived disablement checks. Reads, ordinary login, link redemption alone, single-session logout, enable, rejection and committed retries do not increment it. Related local account/credential changes commit together; credentials issued by that transition use the new epoch; overflow is rejected. Fixed deadlines and concurrent ordinary logins are preserved. DO-033 separately approves no automatic post-reset session; DO-036/037 approve durable socket revocation and expiry cleanup policy while delivery/sweep implementation remains open.

**DO-032 — Approved:** At enrollment completion, after password hashing, revalidate the enabled PendingEnrollment account and unexpired/unrevoked EnrollmentOnly session against the current epoch in one AccountsObject transaction. Commit the verifier, Verified status, password_set_at, one epoch increment, restricted-session revocation and one new Normal session bound to the new epoch atomically. Use a distinct bearer and the DO-031 cookie, retaining the original absolute expiry rather than extending it. Issue the cookie only after commit; if the committed response is lost, require password login rather than replaying a secret or rolling back. Concurrent reset/disable/expiry wins cause fail-closed setup. Post-reset behavior remains DO-033; command-retry receipt mechanics are not decided here.

**DO-033 — Approved:** Password-reset completion revalidates the enabled ResetRequired account and live PasswordResetOnly session/current epoch in one AccountsObject transaction, persists the verifier, sets Verified/password_set_at, increments the epoch once and retires reset-only authority. It issues no Normal session: clear the shared cookie and direct the user to a fresh password login, which creates the new one-day normal session. The epoch invalidates earlier sessions; a lost response is recovered by login, not secret replay or rollback. Retry receipt details remain outside scope.

**DO-034 — Approved:** Every protected request validates the cookie's SHA-256 verifier, session row, current account epoch/status and enabled state directly against authoritative AccountsObject state, including scope/lifecycle, expiry and revocation. A Normal session requires Verified; restricted scopes require their matching pending lifecycle. Do not trust a positive authorization cache across requests. Delete expired/revoked session rows asynchronously after they become ineligible; authorization fails immediately regardless of cleanup. Preserve DO-029 access-link/receipt retention unchanged; no additional session-row retention period is selected. SQL/index/schedule details remain implementation work; socket delivery/expiry is DO-036/037.

**DO-037 — Approved:** AccountSocketSubscription becomes ineligible at the linked fixed session `expires_at`; natural expiry neither extends the deadline nor increments the account epoch. Per-frame DO-034 authorization denies an expired session before sending and closes its socket. A retryable, bounded/paginated expiry sweep may atomically enqueue per-connection close work in AccountsObject. Use DO-036 idempotent GameObject close/ack; retain the subscription/work target until close or already-absent is acknowledged, then delete subscription metadata. Sweeper delay cannot grant authority. No post-ack retention is added; DO-034 session cleanup and DO-029 link/receipt retention stay separate. SQL, cadence and outbox fields remain TBD.

**DO-038 — Approved:** Preserve the shown GameRecord, TerminalOutcome and CancellationReason logical fields/types. Start in New with both revisions at 0, creator/designated host equal to the creating account, creation/last-qualifying-host timestamps set to creation time and no code/lobby/start/end/outcome/history expiry. Initial idle deadline is last qualifying designated-host activity +24 hours. Set code/lobby timestamp on AwaitingPlayers entry and start timestamp on InProgress entry. Before terminal, `ended_at` and outcome are absent. Resolved iff there is exactly one winner and the outcome is Resolved; Cancelled has no winner and the outcome is Cancelled. HostIdleTimeout only applies before start. Pre-start Cancelled games are deleted after commit and create no History. Started terminal games receive a fixed three-calendar-month History expiry from terminal time. Reads/retries cannot renew terminal status/outcome/expiry. Host-assignment/revision transactions, non-designated-admin timer interaction, timer races and physical mappings remain DO-039/040/041 or implementation work.

### 7.2 Review queue

| ID | Area | Decision to review | Source | Status | Recorded decision |
| --- | --- | --- | --- | --- | --- |
| DO-001 | Ownership | Separate singleton AccountsObject placement/cardinality | [Detail](#object-boundaries) | Approved | One separate SQLite-backed AccountsObject per environment for host/admin accounts and credentials; participant sessions stay Game-owned. Placement/cardinality only; interfaces/protocols/capacity remain open. |
| DO-002 | Ownership | Final History placement in the original GameObject | [Detail](#object-boundaries) | Approved | Store each started game's final History in its original GameObject; Directory holds only minimal searchable index/code/expiry metadata, not player boards or full snapshots. No new HistoryObject/datastore. Placement only; fields, tables, calendar expiry and cleanup protocols remain open. Existing three-calendar-month retention and no-History pre-start cancellation are unchanged. |
| DO-003 | Shared representations | Timestamp representation and trusted clock policy | [Detail](#shared-types) | Approved | Timestamp is i64 / SQLite INTEGER containing UTC Unix epoch milliseconds since 1970-01-01T00:00:00Z. The trusted backend record owner supplies action/creation times and durable deadlines; browser time and UUID v7 time are not authoritative. Explicit sequences/revisions govern ordering. Calendar-month arithmetic, clock rollback/skew handling and target integration remain TBD. |
| DO-004 | Shared representations | Revision initialization, advancement and overflow behavior | [Detail](#shared-types) | Approved | Revision is a nonnegative i64 / SQLite INTEGER initialized at 0. Required advancement occurs in the owning Object transaction; reads, rejected requests and retries do not themselves increment it. Reject required advancement at the maximum rather than wrapping/resetting. Per-counter triggers remain for their lifecycle/protocol reviews; no timestamp or global ordering semantics. |
| DO-005 | Shared representations | UUID storage encoding and boundary mapping | [Detail](#shared-types) | Approved | Application-generated UUID v7 IDs use SQLite TEXT in canonical lowercase hyphenated representation. Rust keeps distinct uuid::Uuid-backed ID types; storage conversion validates version 7, rejecting malformed/wrong-version IDs without repair. API wire encoding and concrete constructors/target wiring remain separately TBD. |
| DO-006 | Shared representations | Typed-ID constructors, creation ownership and target integration | [Detail](#shared-types) | Approved | Use a shared domain ID module with distinct uuid::Uuid-backed types and private inner fields. TryFrom<uuid::Uuid> and FromStr validate version 7; formatting is canonical lowercase hyphenated UUID text. ID errors use adjacent error.rs / thiserror. Generate once in the authorized creation component and retain identity on retries. Concrete implementation and target clock/randomness wiring remain unverified/TBD; CommandId browser issuance and API wire encoding are not selected. |
| DO-007 | Shared representations | Enum persistence labels and serialization/version compatibility | [Detail](#shared-types) | Approved | Approve stable snake_case enum tags as SQLite TEXT, with mandatory Rust strum-generated conversion/parsing from enum definitions. Unknown tags error; no numeric/debug/default fallback. Preserve exact domain/UI lifecycle names. Changes to stored labels require explicit compatibility/migration; tag/payload mappings, pinned features/version and executed tests remain TBD. |
| DO-008 | Shared representations | CellPosition representation and indexing validation | [Detail](#shared-types) | Approved | CellPosition uses row: u8 and column: u8 mapped to SQLite INTEGER, with existing one-based indexing. Validate against owning board side length and reject zero, negative/out-of-bounds values without clipping or wrapping. Board sizes/free-cell rules are unchanged; per-cell physical table/index layout remains TBD. |
| DO-009 | Shared representations | Collection storage policy and nullable/default conventions | [Detail](#shared-types) | Approved | Persist Vec<T> collections as child rows, with explicit sequence/position wherever order matters; an empty collection has no child rows. Optional scalar Option<T> fields use SQL NULL, not sentinel strings/IDs/numbers. Optional whole entities may use row absence. Defaults must be explicitly specified per record. Table names, relationships, indexes and field-specific constraints remain TBD. |
| DO-010 | Shared representations | Object-local schema version metadata | [Detail](#shared-types) | Approved | Each Directory, Accounts and Game Object has one persistent local StorageMetadata row with schema_version: u32 / SQLite INTEGER. An initialized schema starts at version 1; record the new version only on successful committed local initialization/migration. Refuse normal operations on unsupported newer versions; no guessing/silent downgrade. Rollout/backout, compatible migrations and SQL remain TBD. |
| DO-011 | Directory | GameIndexRecord fields/types and state-dependent nullability | [Detail](#directory-records) | Approved | GameIndexRecord fields/types and nullability approved with ended_at, as a combined live-game and retained-History index. Pending/Published are publication metadata, not lifecycle states. End time is absent before ending; History expiry only for started terminal games. Retained terminal codes reserve through History expiry without admission/recovery; pre-start cancellation removes the entry. Indexes/freshness/coordination remain separate. |
| DO-012 | Directory | Game index keys, uniqueness, query indexes and state filters | [Detail](#directory-records) | Approved | Game index uses game_id as primary key and allocated game_code as unique; uncoded entries may coexist. Query indexes on designated_host_id, state and non-NULL history_expires_at. Public discovery requires Published and Awaiting Players/In Progress; History requires started Resolved/Cancelled with unexpired deadline. Role/session and Game authority checks remain required. SQL/index names, query plans and expiry/reuse coordination remain TBD. |
| DO-013 | Directory | Projection publication, freshness and update delivery | [Detail](#directory-records) | Approved | GameObject is authoritative. For each directory-visible committed change, project its Game revision. Initial Awaiting Players publication is hidden/Pending until configured state/code is committed in Game and Directory accepts the matching projection. Afterwards accept only a strictly newer source_revision; exact duplicate is a no-op and older revisions are ignored. Already-published codes remain Published during subsequent projected lifecycle changes. Discovery/listing is not authority: Game rechecks current state/code/permissions before admission or mutation. DO-015 separately approves compare-by-game_id reservation fencing; cross-Object delivery/retry/recovery and code reuse remain separate TBDs; list staleness and initial-publication completion reporting remain unresolved. |
| DO-014 | Directory | One GlobalReservation record: nullable `game_id` only (`NULL` = free; non-NULL = occupied) | [Detail](#directory-records) | Approved | Exactly one Directory row per environment. No persisted operation ID, generation/fencing counter or phase; protocol/recovery details remain DO-015/016. |
| DO-015 | Directory | Single-reservation acquisition, lifecycle transitions, fencing and crash recovery | [Detail](#directory-records) | Approved | Claim `game_id` atomically only if NULL before GameObject creation; on creation failure retain it and recreate the same object under the same ID/code association. Start keeps the same slot. Clear only if the stored ID equals the releasing game; IDs are never reused, so no generation counter is needed. Request idempotency remains DO-076–DO-084; terminal retry semantics are DO-016. |
| DO-016 | Directory | Terminal release of the single reservation, retries and interrupted-operation reconciliation | [Detail](#directory-records) | Approved | Commit terminal result first; compare-and-clear matching game ID, already-NULL is complete, another ID is stale/no-op; retry safely after uncertain acknowledgement. Scheduling/backoff and command receipts remain open. |
| DO-017 | Directory | AccountAssignmentGate fields/types; necessity | [Detail](#directory-records) | Approved | One presence-only Directory row keyed by `account_id` blocks assignments during disable/delete; no operation ID, generation or timestamp field. Protocol remains DO-018. |
| DO-018 | Directory | Account-removal gate acquisition, assignment/transfer races and recovery | [Detail](#directory-records) | Approved | Acquire gate before hosted-game check; block assignments to account; clear-and-reject if a nonterminal game exists, otherwise disable/delete then clear; retain gate and retry on interruption. DO-039 approves gate-first transfer rejection and transfer-first visibility to removal's hosted-game check; durable coordinator, receipt ownership and physical recovery remain open. |
| DO-019 | Accounts and credentials | AccountRecord fields/types and nullability | [Detail](#account-records) | Approved | Approve AccountRecord fields/types and nullability, renaming the account password field to `verifier: Option<EncodedVerifier>`. Keep one complete library-generated Argon2 PHC string (algorithm/version, work parameters, salt and derived hash), absent before password setup; no separate salt/hash fields. DO-020 settles username characters/length; DO-021 approves username trim/validation and revised case-sensitive comparison; DO-022 settles lifecycle/epoch policy; DO-023 settles Argon2id v19, salt/output lengths and SQLite TEXT mapping; DO-024 approves benchmark/bounds/runtime-validation policy; DO-025 settles safe opportunistic rehash race behavior. Production costs, hard caps, target measurements, crate pin and other physical schema constraints remain TBD. |
| DO-020 | Accounts and credentials | Username permitted characters and length bounds | [Detail](#account-records) | Approved | Usernames are 10–50 ASCII characters, excluding all ASCII whitespace: HT/tab, LF, VT, FF, CR (0x09–0x0D) and ordinary space (0x20). Other ASCII control characters, including NUL and DEL, remain permitted; do not silently narrow to printable ASCII. Apply to host/admin provisioning via CLI and admin UI. Count decoded characters; no password/player-alias policy change. DO-021 approves username casing/comparison after trimming. Transport/UI/storage handling of permitted controls must be verified. |
| DO-021 | Accounts and credentials | Username casing, boundary trimming and comparison semantics | [Detail](#account-records) | Approved (revised) | Retain approved trim/validation: trim leading/trailing ASCII whitespace, then apply DO-020 length/character validation (10–50 ASCII characters, no internal ASCII whitespace); preserve trimmed original casing. Case-only variants are distinct; uniqueness/login use exact case-sensitive equality, with no case-folding or separate normalized key. Password rules are unchanged. SQL/constraint and control-character integration remain TBD. |
| DO-022 | Accounts and credentials | Account status/credential-epoch transitions and disable/enable/reset interactions | [Detail](#account-records) | Approved | Enable/API by another admin or developer CLI only (HLD-078); restore pre-disable Verified/PendingEnrollment/ResetRequired lifecycle, never revive old sessions/links. `disabled_at` alone represents disablement; no duplicate boolean or prior-status field; authorized same-state retries leave the timestamp unchanged. Start epoch at 0; increment once on replacement enrollment-link issuance, reset-link issuance/reissuance, successful setup/reset completion and actual disable transition. Issued links/sessions store and validate against current epoch plus scope/purpose, expiry, revocation, lifecycle and disablement checks. No increment for reads, ordinary login, redemption alone, single logout, enable, rejection or committed retries. Commit related account/credential changes together in AccountsObject; assign the resulting epoch to credentials issued by that transition; reject overflow. Fixed deadlines and concurrent ordinary logins remain; reset auto-login remains separately approved in DO-033, and DO-036 approves cross-Object revocation work/acknowledgement and fail-closed frame checks. Transaction/race/retry/delivery implementation remains open/not authorized. |
| DO-023 | Accounts and credentials | Remaining password-verifier profile and physical encoding constraints | [Detail](#account-records) | Approved | Keep DO-019's one complete library-generated PHC string in `AccountRecord.verifier`. Use Argon2id v19, a fresh independent 16-byte random salt per setup/reset/rehash, and a 32-byte output. Map the Rust string to SQLite `TEXT`, nullable (`NULL`) before first setup; no separate salt/hash columns or custom binary encoding. Work factors and supported verifier/cost bounds remain TBD under DO-024; actual runtime/resource validation follows its approved gate and has not yet run. Crate version pin and target integration remain separate implementation-planning TBDs. |
| DO-024 | Accounts and credentials | Argon2 work factors, supported bounds and runtime validation plan | [Detail](#account-records) | Approved | Explicitly configure `m_cost`/`t_cost`/`p_cost`; use OWASP's 19 MiB / 2 iterations / parallelism 1 as the initial benchmark point, not a measured production profile. Parse PHC and reject malformed, unsupported, oversized or out-of-bounds algorithm/version/cost inputs before expensive verification; use an explicit bounded allowlist and never fall back to weaker hashing. Before production, validate Cloudflare Worker/WASM RNG, CPU/time, peak memory, latency and concurrency, then document production profile, caps, concurrency and rate limits. If the target cannot meet the security/performance bar, stop and review rather than silently lowering costs. No target benchmark/results are claimed. |
| DO-025 | Accounts and credentials | Password-verifier upgrades and concurrent-reset protection | [Detail](#account-records) | Approved | After successful verification, opportunistically rehash only an otherwise-supported old profile using a fresh salt/current explicit costs. Conditionally replace only if account still exists, is enabled/Verified, and exact verified PHC plus `credential_epoch` are unchanged. Hash-only rehash does not increment epoch or extend sessions. On CAS loss, discard candidate, reload and reverify current verifier; further conflicts fail closed/generic. If optional rehash fails, keep valid credential and allow login only after current status/epoch recheck; record redacted internal failure and retry later. Never overwrite reset/disable or issue a session based only on stale verification. Transaction/CAS/error plumbing remains implementation design; no implementation authorized. |
| DO-026 | Accounts and credentials | AccessLinkRecord fields/types and constraints | [Detail](#account-records) | Approved | One required logical record per issued link under AccountsObject: `link_id: LinkId` identity, `account_id: AccountId`, `purpose: AccessLinkPurpose` (`Enrollment`/`PasswordReset`), `token_verifier: Digest`, `credential_epoch: Revision`, required `issued_at`/`expires_at: Timestamp`, optional `consumed_at`/`revoked_at: Timestamp`. Bind to account, purpose and issuance epoch; retain fixed one-day expiry and one-time use. Never store raw reusable tokens. DO-027 approves token construction/verifier/encoding and unique digest index; DO-028 covers concurrent consumption; DO-029 reissue/predecessor/lost-response/retention. The remaining SQL schema details and transaction implementations are TBD. |
| DO-027 | Accounts and credentials | Random-token construction, verifier algorithm/length and encoding | [Detail](#account-records) | Approved | Access links only: generate 32 CSPRNG bytes and fail closed if unavailable; encode as canonical unpadded Base64URL (43 chars); store SHA-256 of raw bytes as unique 32-byte SQLite BLOB. Redemption input must be canonical and decode to exactly 32 bytes. Retry generation on unique digest collision. Never persist/log raw bearer token; LinkId is nonsecret. Do not apply to other `Digest` uses or password verifier; no HMAC/pepper/JWT selected. Target RNG compatibility remains subject to DO-024 and is unverified. OWASP supports CSPRNG, sufficient length, secure storage, user binding, single use and expiry, but not this specific profile. |
| DO-028 | Accounts and credentials | Access-link delivery/redemption safety and concurrent single-use consumption | [Detail](#account-records) | Approved | Canonical configured HTTPS origin, never request Host. One-time authorized CLI/admin handoff after durable issuance; Cache-Control no-store and token-redacted logs/traces. Token in URL fragment only; browser promptly removes it, then posts token in HTTPS body; no-referrer and no third-party scripts/analytics on redemption pages. Atomic AccountsObject transaction checks purpose/account/current epoch/expiry/use/revocation/account eligibility/enabled/lifecycle; conditional single consume + corresponding restricted session in same commit, at most one concurrent success. One-day restricted session deadline begins at issuance and setup does not extend it. Generic safe invalid-link response, no epoch increment on redemption. DO-029 defines recovery after committed-but-lost response; SQL/locking details remain implementation TBD. |
| DO-029 | Accounts and credentials | Enrollment reissue/predecessor cleanup and consumed-link lost-response handling | [Detail](#account-records) | Approved | Enrollment reissue only for enabled PendingEnrollment; reset-link reissue only for enabled ResetRequired; keep status, reject disabled. One AccountsObject transaction bumps credential_epoch exactly once, creates one fresh 24-hour link at the new epoch, and revokes predecessor links plus account/restricted sessions; epoch is authoritative fence. Reset remains blocked from old-password login; cross-Object socket completion is not claimed. Same command_id never mints/replays a secret; retain secret-free issuance receipt (account/link IDs, purpose, committed outcome, expiry) 30 days. Lost issuance URL requires explicit successor reissue; lost committed redemption response never replays link/session secret, and recovery uses a fresh purpose-appropriate link while pending/reset-required or normal login/admin reset after Verified. Replays use generic invalid-link response. Retain metadata-only link rows through latest expiry/consumption/revocation plus 30 days, then allow asynchronous deletion. Physical receipt schema, command-id-after-receipt-expiry behavior, cleanup schedule, SQL/locking and DO-036 outbox implementation remain open; DO-036 separately approves durable retryable cross-Object socket revocation. |
| DO-030 | Accounts and credentials | AccountSessionRecord fields/types and constraints | [Detail](#account-records) | Approved | Keep exactly `session_id: SessionId`, `account_id: AccountId`, `token_verifier: Digest`, `scope: AccountSessionScope` (`EnrollmentOnly`/`PasswordResetOnly`/`Normal`), `credential_epoch: Revision`, `issued_at`/`expires_at: Timestamp`, `revoked_at: Option<Timestamp>`. One row per session; UUID-v7 ID is nonsecret; bind account/scope/current epoch. `expires_at > issued_at`, fixed one-day absolute deadline with no activity/reconnect extension. NULL `revoked_at` means not individually revoked; no duplicate state booleans. Role/game permissions remain authoritative elsewhere; concurrent ordinary sessions allowed. Token digest profile DO-031; SQL/indexes and cleanup DO-034/implementation. |
| DO-031 | Accounts and credentials | Cookie/session format, binding and request protection | [Detail](#account-records) | Approved | Per session, generate 32 CSPRNG bytes (fail closed), canonical unpadded Base64URL 43-char token, store only SHA-256(raw bytes) as unique 32-byte SQLite BLOB in `token_verifier`; retry collisions; never persist/log raw secret; not Argon2; target RNG compatibility unverified under DO-024. One opaque `__Host-brews_session` cookie for all scopes with Secure, HttpOnly, Path=/, SameSite=Lax, no Domain, HTTPS only; Max-Age/Expires derive from fixed `expires_at`, never slide. No authority claims in cookie. Exact configured Origin required for browser-cookie-authenticated state-changing HTTPS requests and WSS upgrades; reject absent/mismatched Origin; SameSite alone insufficient. No tokens in URLs/WSS messages. CLI/service auth separate. |
| DO-032 | Accounts and credentials | Restricted-to-normal session rotation transaction | [Detail](#account-records) | Approved | For enrollment completion only, atomically revalidate enabled PendingEnrollment account + live EnrollmentOnly session/current epoch; store verifier, set Verified/password_set_at, increment epoch once, revoke the restricted session and create a Normal session at the new epoch. Use a distinct bearer/DO-031 cookie; preserve the original absolute expiry, set the cookie after commit, and require login rather than secret replay if the committed response is lost. Concurrent reset/disable/expiry fails closed. Post-reset behavior is DO-033; command-retry receipts remain outside scope. |
| DO-033 | Accounts and credentials | Post-reset session issuance and navigation behavior | [Detail](#account-records) | Approved | Reset completion atomically stores the verifier, sets Verified/password_set_at, bumps credential_epoch once and retires the live PasswordResetOnly session. Do not auto-issue a Normal session; clear the shared cookie and navigate to password login, which creates a new one-day session. Prior sessions fail the new epoch. Lost committed response recovers through login; no secret replay/rollback. Retry receipt details remain open. |
| DO-034 | Accounts and credentials | Account authority lookup/caching and expired/revoked artifact cleanup | [Detail](#account-records) | Approved | Each protected request resolves the cookie digest against authoritative session/account rows and checks current epoch, scope/lifecycle, enabled status, expiry and revocation. No positive authorization cache may grant across requests without revalidation. Asynchronously delete expired/revoked session rows when ineligible; cleanup does not gate denial. Preserve DO-029 link/receipt retention and add no session-row grace period. SQL/index/schedule details remain implementation work; socket delivery is DO-036. |
| DO-035 | Accounts and credentials | AccountSocketSubscription fields/types, uniqueness and registration races | [Detail](#account-records) | Approved | Keep the seven shown logical fields as AccountsObject revocation/cleanup metadata, never authority. Unique application UUID-v7 `connection_id`; identical registration is a no-op, conflicting reuse fails closed. Subscription expiry equals the linked session's fixed expiry. Register only enabled Verified/Normal session authority, revalidated with epoch/expiry/revocation and inserted in one AccountsObject transaction: revocation-first rejects; registration-first leaves a row for DO-036. Close a prepared/attached WSS socket and send no authorized snapshot if registration fails. DO-036 covers revocation delivery/acknowledgement; approved DO-037 covers expiry-driven close work and post-ack stale-row deletion. |
| DO-036 | Accounts and credentials | Immediate account revocation delivery, acknowledgements and fail-closed behavior | [Detail](#account-records) | Approved | Session/account invalidation or epoch change durably records affected subscription targets in the same AccountsObject transaction. Deliver idempotent GameObject close commands; acknowledge only when connection is closed/already absent, then retire target; retry unacknowledged work and never claim all sockets closed before all acks. GameObject revalidates session/epoch before every account-authorized outgoing WSS frame; stale/revoked or unavailable authority suppresses the frame and closes. A frame authorized/in flight before commit may race; no cross-Object atomic network close is claimed. Outbox schema, IDs, scheduling and physical transaction details remain implementation work. |
| DO-037 | Accounts and credentials | Stale account-socket subscription expiry and cleanup | [Detail](#account-records) | Approved | At linked fixed session `expires_at`, subscription becomes ineligible; no account-epoch bump/deadline extension. DO-034 per-frame check denies expired session, sends no frame and closes. A retryable bounded/paginated sweep atomically enqueues DO-036 per-connection close work; keep target until GameObject acknowledges closed/already absent, then delete subscription metadata. Delay cannot grant authority. No post-ack retention; DO-034 session cleanup and DO-029 link/receipt retention remain separate. SQL, cadence and outbox fields remain TBD. |
| DO-038 | Game and configuration | GameRecord and TerminalOutcome fields/types, defaults and constraints | [Detail](#game-records) | Approved | Preserve the shown logical fields/types. Start New with revisions 0, creator=designated host, creation/last-qualifying timestamps set, no code/lobby/start/end/outcome/history expiry and initial idle deadline +24h. Add code/lobby timestamp at AwaitingPlayers; start time at InProgress. Terminal fields absent preterminal; Resolved iff one winner/Resolved outcome; Cancelled has no winner/Cancelled outcome. HostIdleTimeout only pre-start; pre-start cancellation deletes after commit with no History. Started terminal expiry is three calendar months after terminal time. DO-038 approves GameRecord/TerminalOutcome logical fields, defaults and lifecycle invariants; DO-040/041 approve designated-host-only renewal and expiry/activity/start ordering; DO-042/043 approve configuration defaults/free-cell constraints and the numeric upper-bound ceiling; DO-044 approves full validation and capped falling-factorial feasibility. |
| DO-039 | Game and configuration | Host-assignment/revision transactions and actor attribution | [Detail](#game-records) | Approved | Nonterminal states only; revalidate designated-host/admin actor and distinct enabled Verified Host target, require target gate absent and expected assignment revision. Stale/missing version rejects. GameObject atomically updates designated_host_id, checked-increments host_assignment_revision and game revision once, and records authenticated actor/request fingerprint/safe result/committed revision in actor-scoped CommandReceipt. An exact actor/command/fingerprint retry within the applicable live-receipt window returns the receipt without another increment; command-ID reuse with a different fingerprint rejects. Expired-receipt replay remains governed by DO-076–DO-084. Preserve game data/reservation. Gate-first removal rejects transfer; transfer-first is visible to removal's hosted-game check, blocking target disable/delete while nonterminal. Physical cross-Object coordinator/receipt details remain TBD. |
| DO-040 | Game and configuration | Host-idle renewal policy for non-designated admin activity | [Detail](#game-records) | Approved | Only intentional open/resume or accepted state-changing action by current designated host renews. Designated-host transfer counts once; non-designated admin override/open/resume and admin transfer do not. New host inherits existing deadline. Passive reads/heartbeats/reconnects, rejected/no-op requests and retries never renew. DO-041 handles races. |
| DO-041 | Game and configuration | Idle cancellation versus activity/start timer races | [Detail](#game-records) | Approved | Compare trusted time at serialized GameObject transaction point; end-exclusive `now >= due`. Before due, accepted host renewal or Start may win; Start commits InProgress/clears deadline, stale alarm no-ops. At/after due, expiry wins: reject late host actions/Start, commit Cancelled/HostIdleTimeout, pre-start deletion/no History, compare-by-game-ID reservation release. Early/stale alarm rereads/reschedules if deadline future; never affects InProgress/terminal or newer reservation. Equality expires; no grace. Physical alarm and cross-Object recovery remain TBD. |
| DO-042 | Game and configuration | GameConfiguration fields/types and disabled free-cell representation | [Detail](#game-records) | Approved | `numeric_upper_bound: u32` positive, default 75, range 1..=1,000; values are strings with no stored pool array. `board_side_length: u8` default 5, square range 2–10. `free_cells_enabled: bool` default true; disabled implies empty positions, enabled requires ≥1 unique one-based in-bounds `CellPosition`; default is center under ceil-half rule. `player_capacity: u8` default 20, range 2–20. `spectator_capacity: u8` default 50, range 0–50 (zero disables). `winning_pattern` defaults to sole first-release SingleLine. Mutable only in New; frozen at AwaitingPlayers. DO-044 defines complete validation/feasibility; physical layout remains open. |
| DO-043 | Game and configuration | Practical numeric pool/resource ceiling | [Detail](#game-records) | Approved | `numeric_upper_bound` is 1–1,000 inclusive; default 75. At most 1,000 distinct accepted call records/game; pool remains implicit. Product guardrail, not measured Cloudflare/SQLite capacity claim. DO-044 defines its feasibility use. |
| DO-044 | Game and configuration | Configuration validation and start-time feasibility algorithm | [Detail](#game-records) | Approved | Validate full config on create/update and Start. For `N = numeric_upper_bound`, `k = side² − free_cells`, require `N ≥ k`; count possible cell-distinct layouts as `P(N,k)` (P(N,0)=1), saturating at retained start-time player membership count to avoid overflow. Reject Start if count is less than all retained memberships, connected or not; the independent two-connected-player gate also applies. Invalid patches apply nothing. Sampling/physical details remain implementation work. |
| DO-045 | Players, spectators and access | PlayerRecord fields/types and case-sensitive alias uniqueness | [Detail](#game-records) | Approved (revised) | No stored comparison key; store the accepted outer-trimmed display alias only. Enforce exact case-sensitive uniqueness among retained PlayerRecords per game and exact current-alias spelling for recovery; `Alice` and `alice` are distinct. Cross-game reuse allowed. Preserve `player_id`, `joined_at`, `session_epoch`, nullable `last_explicit_leave_at`; no `is_connected` Boolean. DO-046 approves stable-player-ID session binding; DO-055 separately reviews full session fields/access. Alias character/space rules remain HLD-050/051. |
| DO-046 | Players, spectators and access | Atomic alias rename and participant-session binding updates | [Detail](#game-records) | Approved | In Awaiting Players, valid player session may rename own membership to valid unclaimed exact-case alias. Atomically change PlayerRecord alias/release old while preserving player_id, seat, join time, role/capacity, answer verifier. Fail invalid/claimed/late rename without mutation. Session binds stable player_id, not alias; do not rewrite/rotate/revoke credentials or renew expiry. Current alias is resolved from PlayerRecord. Full ParticipantSessionRecord schema/access, response/retry, physical SQL/index remain DO-055/implementation. |
| DO-047 | Players, spectators and access | Atomic role switching, leave and session updates | [Detail](#game-records) | Approved | Awaiting Players role changes atomically obey target capacity. Player→spectator releases alias/seat, deletes recovery record and retires old player session/sockets; spectator→player creates fresh PlayerRecord/PlayerId, eligible alias/seat and optional fresh answer, never restoring old state. Replace role-bound session without extending fixed absolute expiry. Rejection preserves old role/seat/session/proof. Pre-start player Leave removes membership/recovery/session and releases alias/seat; return is fresh admission. In Progress player Leave retains membership/board/award eligibility and a still-valid session can return until expiry. Spectator Leave immediately releases seat/revokes session; later entry is fresh. Serialize against start/admission. DO-048 separately approves retaining the latest explicit Leave timestamp after return; DO-052/055 physical transaction/session details remain separate. |
| DO-048 | Players, spectators and access | Player leave-timestamp meaning after return | [Detail](#game-records) | Approved | Set `last_explicit_leave_at` on each explicit In Progress player Leave and keep it as the most recent Leave-event timestamp after return; do not clear on valid-session or answer-based return. It is historical event data, not current-presence state. Pre-start Leave deletes the membership/record. |
| DO-049 | Players, spectators and access | PlayerRecoveryRecord fields/types and version metadata | [Detail](#game-records) | Approved | Preserve `player_id: PlayerId`, `answer_verifier: EncodedVerifier`, `normalization_version: u32`, `updated_at: Timestamp`. Absence disables answer recovery; no plaintext or public/History exposure. Delete on accepted player→spectator switch, membership removal or terminal transition; replacement atomic. DO-050 normalization/version behavior and DO-051 Argon2id/PHC verifier profile are approved; DO-052 transaction/session/socket fencing remains separate. |
| DO-050 | Players, spectators and access | Recovery-answer normalization and version migration | [Detail](#game-records) | Approved | Version 1: Unicode NFC, trim outer Unicode whitespace, then Unicode case-fold; preserve internal whitespace/punctuation. Select verifier comparison by stored normalization_version. Future profile changes cannot reinterpret old verifier silently; retain old profile until authenticated answer replacement writes current version. No plaintext storage or silent verifier migration. Unicode data/library pin and implementation remain TBD. |
| DO-051 | Players, spectators and access | Recovery-answer verifier algorithm/format/work factors | [Detail](#game-records) | Approved | Argon2id v19; fresh independent 16-byte CSPRNG salt; 32-byte output; full PHC-encoded `EncodedVerifier`. Explicit bounded memory/time/parallelism costs; reject malformed/unsupported/out-of-bounds parameters before expensive work. 19 MiB / 2 iterations / parallelism 1 is only a benchmark starting point. Production costs/caps require target-runtime/resource measurements; DO-052 abuse controls, recovery/replacement transactions and fencing remain separate. |
| DO-052 | Players, spectators and access | Recovery and answer-replacement transaction/session-socket fencing | [Detail](#game-records) | Pending | — |
| DO-053 | Players, spectators and access | SpectatorRecord fields/types and grace-deadline constraints | [Detail](#game-records) | Pending | — |
| DO-054 | Players, spectators and access | Spectator disconnect/reconnect/grace races and stale-socket fencing | [Detail](#game-records) | Pending | — |
| DO-055 | Players, spectators and access | ParticipantSessionRecord fields/types and live/final access constraints | [Detail](#game-records) | Pending | — |
| DO-056 | Players, spectators and access | Minimal terminal player-session record and Exit/replay/deletion guards | [Detail](#game-records) | Pending | — |
| DO-057 | Players, spectators and access | AccountGameViewAccess fields/types and eligible-viewer lifecycle | [Detail](#game-records) | Pending | — |
| DO-058 | Players, spectators and access | Account final-view downgrade, Exit scope across sessions and minimal retention | [Detail](#game-records) | Pending | — |
| DO-059 | Boards, calls and view revisions | PlayerBoardRecord, BoardCell and CompletedLine fields/types | [Detail](#game-records) | Pending | — |
| DO-060 | Boards, calls and view revisions | Board/cell physical row mapping and persisted-projection checks | [Detail](#game-records) | Pending | — |
| DO-061 | Boards, calls and view revisions | Exact full-board uniqueness enforcement | [Detail](#game-records) | Pending | — |
| DO-062 | Boards, calls and view revisions | Feasible random board generation and randomness failure handling | [Detail](#game-records) | Pending | — |
| DO-063 | Boards, calls and view revisions | Matching/qualification algorithms and pattern-specific Rust trait signatures | [Detail](#game-records) | Pending | — |
| DO-064 | Boards, calls and view revisions | CallRecord fields/types and keys | [Detail](#game-records) | Pending | — |
| DO-065 | Boards, calls and view revisions | Call sequencing/overflow and actor-scoped receipt relationship | [Detail](#game-records) | Pending | — |
| DO-066 | Boards, calls and view revisions | Atomic accepted-call SQL transaction/statements | [Detail](#game-records) | Pending | — |
| DO-067 | Boards, calls and view revisions | ViewRevisionRecord/ViewKey fields/types and projection boundaries | [Detail](#game-records) | Pending | — |
| DO-068 | Boards, calls and view revisions | Authorized view-revision advancement, snapshot/subscription ordering and gap recovery | [Detail](#game-records) | Pending | — |
| DO-069 | History and retention | History snapshot/winner/player fields/types and immutable constraints | [Detail](#history-records) | Pending | — |
| DO-070 | History and retention | History child-table layout and ordered collections | [Detail](#history-records) | Pending | — |
| DO-071 | History and retention | Three-calendar-month timezone/month-end expiry calculation | [Detail](#history-records) | Pending | — |
| DO-072 | History and retention | Terminal snapshot materialization and obsolete-live-data cleanup | [Detail](#history-records) | Pending | — |
| DO-073 | History and retention | Terminal notice/revocation/deletion ordering without indefinite acknowledgement waits | [Detail](#history-records) | Pending | — |
| DO-074 | History and retention | Scheduled purge and Directory/Game code-reservation reuse coordination | [Detail](#history-records) | Pending | — |
| DO-075 | History and retention | History copies/indexes/log cleanup and restore-time expiry enforcement | [Detail](#history-records) | Pending | — |
| DO-076 | Command receipts and pending work | CommandReceipt/ActorRef fields/types and actor-scoped keys | [Detail](#operational-records) | Pending | — |
| DO-077 | Command receipts and pending work | Request fingerprints and same-ID/different-request rejection | [Detail](#operational-records) | Pending | — |
| DO-078 | Command receipts and pending work | Typed secret-free command results and size bounds | [Detail](#operational-records) | Pending | — |
| DO-079 | Command receipts and pending work | Receipt retention and old-command admissibility/retry window | [Detail](#operational-records) | Pending | — |
| DO-080 | Command receipts and pending work | Credential-issuing command retry handling without stored raw secrets | [Detail](#operational-records) | Pending | — |
| DO-081 | Command receipts and pending work | PendingOperation/TargetRef/CoordinationPhase fields/types | [Detail](#operational-records) | Pending | — |
| DO-082 | Command receipts and pending work | Typed per-operation outbox payloads, owners and trust boundaries | [Detail](#operational-records) | Pending | — |
| DO-083 | Command receipts and pending work | Per-operation coordination state machines, fences and expected revisions | [Detail](#operational-records) | Pending | — |
| DO-084 | Command receipts and pending work | Pending-work retry/backoff, completion and orphan reconciliation | [Detail](#operational-records) | Pending | — |
| DO-085 | Abuse controls and audit | RateLimitBucket fields/types and enforcement ownership | [Detail](#operational-records) | Pending | — |
| DO-086 | Abuse controls and audit | Rate-limit scopes, thresholds, windows and success/failure accounting | [Detail](#operational-records) | Pending | — |
| DO-087 | Abuse controls and audit | Rate-limit subject-key privacy, caller/game controls and bucket retention | [Detail](#operational-records) | Pending | — |
| DO-088 | Abuse controls and audit | AdminAuditRecord fields/types, storage owner and event contents | [Detail](#operational-records) | Pending | — |
| DO-089 | Abuse controls and audit | Durable audit capture of rejected/failed actions and developer attribution | [Detail](#operational-records) | Pending | — |
| DO-090 | Abuse controls and audit | Audit retention/deletion and privileged read policy | [Detail](#operational-records) | Pending | — |
| DO-091 | Connections and scheduling | ConnectionAttachment/ViewerRef fields/types, encoding/version/size | [Detail](#socket-metadata) | Pending | — |
| DO-092 | Connections and scheduling | Hibernation authority revalidation and presence reconstruction | [Detail](#socket-metadata) | Pending | — |
| DO-093 | Connections and scheduling | Socket supersession and one-live-socket fencing | [Detail](#socket-metadata) | Pending | — |
| DO-094 | Connections and scheduling | Durable alarm scheduling across idle/grace/session/history deadlines | [Detail](#socket-metadata) | Pending | — |
| DO-095 | Connections and scheduling | Backpressure, message size bounds and delivery failure handling | [Detail](#socket-metadata) | Pending | — |
| DO-096 | Physical schema, deployment and verification | Per-record SQL/table/column mappings and privacy/read-write classifications | [Detail](#outstanding-decisions) | Pending | — |
| DO-097 | Physical schema, deployment and verification | Local foreign keys versus cross-Object reference validation | [Detail](#outstanding-decisions) | Pending | — |
| DO-098 | Physical schema, deployment and verification | Unique/check constraints and parameterized query specifications | [Detail](#outstanding-decisions) | Pending | — |
| DO-099 | Physical schema, deployment and verification | Schema initialization/migration/backout and restore compatibility | [Detail](#outstanding-decisions) | Pending | — |
| DO-100 | Physical schema, deployment and verification | Durable Object namespace identities, binding names and environment isolation | [Detail](#outstanding-decisions) | Pending | — |
| DO-101 | Physical schema, deployment and verification | Trusted internal interfaces and developer-CLI/backend integration | [Detail](#outstanding-decisions) | Pending | — |
| DO-102 | Physical schema, deployment and verification | SDK/toolchain/dependency pins and target clock/randomness wiring | [Detail](#outstanding-decisions) | Pending | — |
| DO-103 | Physical schema, deployment and verification | Singleton capacity, storage sizes and read/write amplification validation plan | [Detail](#outstanding-decisions) | Pending | — |
| DO-104 | Physical schema, deployment and verification | Schema/constraint/transaction/race/security/load test acceptance criteria | [Detail](#outstanding-decisions) | Pending | — |
| DO-105 | Privileged product edge cases | Privileged self-disable/delete policy | [Detail](#account-records) | Pending | — |
| DO-106 | Privileged product edge cases | Last-admin disable/delete protection | [Detail](#account-records) | Pending | — |
| DO-107 | Privileged product edge cases | Role-editing feature scope confirmation | [Detail](#account-records) | Pending | Enable feature/actor scope settled by HLD-078 during DO-022; review only whether role editing is in scope. No role-editing feature is implicitly approved. |

## Sources

[1] https://docs.rs/strum/0.28.0/strum — strum - Rust
[2] https://docs.rs/strum/0.28.0/strum/additional_attributes/index.html — strum::additional_attributes - Rust
[3] https://docs.rs/strum/0.28.0/strum/derive.EnumString.html — EnumString in strum - Rust
