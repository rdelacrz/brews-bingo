# Brews Bingo — Durable Object Design

## 1. Document status and navigation

- **Status:** Incremental item-by-item review of the schema proposal moved from the LLD (LLD-025). **DO-001 approves one separate SQLite-backed `AccountsObject` per application environment for host/admin accounts and credentials.** DO-002 approves final History placement in the original GameObject with a minimal Directory index. **DO-022 is approved**, including account lifecycle, timestamp-derived disablement and credential-epoch invalidation policy; DO-023 is next in the review queue. Other unreviewed fields/protocols remain proposals. The ownership/record inventory, Rust-style sketches, invariants and worksheets remain design material, not a finalized SQL schema or authorization to implement. See [the review ledger](#item-review).
- **Ownership:** This document is the single home for Durable Object schemas and record design. [The LLD](lld.md) retains cross-cutting domain rules, UUID/error conventions, password policy/hashing, workflows, verification/deployment planning and the decision register. [API design](api-design.md) retains input/output bodies, safe projections and HTTP/WSS contracts; those are not direct serializations of these private records.
- **Source precedence:** Follow [LLD source precedence](lld.md#12-source-precedence-and-reconciliation), [the HLD](hld.md) and later explicit user decisions. Business requirements and hosting evidence remain in [requirements.md](requirements.md) and [research.md](research.md). Moving this proposal does not approve unselected placements, physical schemas or protocols.
- **Confirmed details carried forward:** Application-generated system IDs use UUID v7. Accounts use `username`: trim surrounding ASCII whitespace before validation, preserve remaining original casing in storage/display, and compute temporary ASCII-lowercase values for uniqueness/login comparisons; no separate normalized key (DO-021). The account's `verifier` field retains the complete encoded password verifier, including hash, salt and parameters (AccountRecord fields/types/nullability and combined PHC representation approved in DO-019); salted Argon2/library selection follows [LLD Section 9.2](lld.md#password-hashing). DO-020/021 fix username length/characters, trimming, casing and comparison; control-character/SQL integration and remaining hashing profile/runtime details stay **TBD**.
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
| DATA-03 | Account and restricted sessions | Account/scope binding, verifier, fixed expiry/revocation; restricted setup/reset is not normal account authority. | `AccountSessionRecord`, `AccountSocketSubscription` (§6.4); **TBD** final schema. |
| DATA-04 | Game directory and reservation | Code/stable-game mapping; one global reservation across New/Awaiting Players/In Progress, with safe terminal release. | `GameIndexRecord`, `GlobalReservation`, `AccountAssignmentGate` (§6.3); **TBD** final schema. |
| DATA-05 | Game and configuration | Designated host, exact lifecycle, fixed rules/pool/grid/free cells/capacities, relevant durable timestamps. | `GameRecord`, `GameConfiguration` (§6.5); **TBD** final schema. |
| DATA-06 | Membership / alias / occupancy | Stable game/member identity, role, normalized alias claim, admission/Leave/presence/grace state; sockets are not seats. | `PlayerRecord`, `SpectatorRecord` (§6.5); `ConnectionAttachment` (§6.8); **TBD** final schema. |
| DATA-07 | Participant sessions / exit authorization | Game/member or spectator binding, player current-alias association, fixed expiry/revocation and permitted final-view exit state. | `ParticipantSessionRecord`, `AccountGameViewAccess` (§6.5); **TBD** final schema. |
| DATA-08 | Private player recovery verifier | Stable member binding and protected verifier; optional, replaceable/deletable by valid-session owner; never public/History data. | `PlayerRecoveryRecord` (§6.5); **TBD** final schema. |
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
| `Digest` | `Vec<u8>`: digest bytes. Algorithm, keyed/unkeyed use and exact length: **TBD** per purpose. |
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
    Active,
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

**DO-017 approved:** One Directory-side row keyed only by `account_id`; row presence blocks new-game/host assignment while account disable/delete is in progress. Operation ID, generation and timestamp are not persisted in this record. DO-018 now approves acquiring the gate before the hosted-game check and holding it through disable/delete; exact handling of assignments already in flight, retries/receipts and cross-Object consistency remains open.

```rust
struct AccountAssignmentGate {
    account_id: AccountId, // primary key; row presence means assignment is blocked
}
```

While this approved gate exists,

**DO-018 approved sequence:** Acquire the per-account gate before checking hosted nonterminal games; while held, reject new-game/host-transfer assignments to that account. If a hosted nonterminal game exists, clear the gate and reject disable/delete. If none exists, disable/delete the account and then clear the gate. On interruption, retain the gate and retry safely. **Still TBD:** serialization with assignments already in flight, command-receipt linkage, freshness proof and cross-Object failure recovery details.

<a id="account-records"></a>

### 6.4 Accounts-owned records

**Approved placement/cardinality (DO-001):** one separate SQLite-backed `AccountsObject` per application environment owns host/admin accounts, usernames, password verifiers, enrollment/reset links and account sessions. Anonymous player/spectator sessions stay in their owning Game Object. This co-locates username uniqueness, link consumption, restricted-session creation and account credential changes under one local transaction owner. **TBD — field/constraint acceptance, bindings/interfaces, capacity/security validation and local/cross-owner protocols.** Shared account-service contention is a trade-off, not measured capacity evidence; placing the data together does not implement transactions or immediate game-socket revocation.

#### `AccountRecord`

**Approved — DO-019, amended during DO-022:** keep the original fields/types/nullability with `verifier` as the account password field name. Determine disablement solely from `disabled_at`: populated means disabled; `None`/SQL `NULL` means enabled. Do not store a separate disabled boolean. `Disabled` remains excluded from AccountStatus; lifecycle status is preserved through disable/enable. Keep the complete Argon2 PHC string in that one field, not separate salt/hash fields. DO-022 approves lifecycle and credential-epoch policy; transaction/concurrency mechanics, hashing-profile details and implementation choices remain open. Username rules are subsequently approved in DO-020/021.

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

- `username` is the account login name. **DO-021 approved:** trim leading/trailing ASCII whitespace before validation on entry and backend account-creation/login paths. Preserve the trimmed username's original casing in storage/display. Compute temporary ASCII-lowercase values from supplied/stored usernames for case-insensitive uniqueness and login matching; never persist a separate normalized key or overwrite the original casing. Leave other permitted characters unchanged. Apply DO-020 validation to the trimmed result, not the untrimmed input. **DO-020 approved, processing order clarified by DO-021:** 10–50 decoded ASCII characters after trimming, excluding all remaining/internal ASCII whitespace (HT/tab, LF, VT, FF, CR: `0x09`–`0x0D`; ordinary space: `0x20`). Other ASCII control characters, including NUL and DEL, are allowed. Apply consistently to host/admin provisioning through the developer CLI and admin interface; do not silently narrow to printable characters. Casing-only variations are the same username for uniqueness/login. Exact SQL/constraint enforcement and control-character transport/UI/storage handling remain **TBD** and must be verified; do not assume a database collation handles every permitted control character correctly.
- `verifier` is the user-selected field name (DO-019). It is absent before setup; after setup it holds the complete library-generated Argon2 PHC string (algorithm/version, work parameters, salt and derived hash), **not only the raw salt or hash**. This approved combined representation keeps salt and hash together in the **same account-owning Durable Object**, without separate salt/hash fields ([LLD Section 9.2](lld.md#password-hashing)). `credential_epoch` changes when reset/reissue/revocation must invalidate predecessors; concurrent ordinary logins do not increment it or revoke one another.
- **Disabled representation — latest confirmed user revision during DO-022:** `disabled_at: Option<Timestamp>` is the sole stored disablement state. Evaluate `disabled_at.is_some()` in Rust or `disabled_at IS NOT NULL` in SQL; timestamp population, not its numeric truthiness or comparison with the clock, means disabled. Set the timestamp when disabling; clear it on authorized enable. Keep the retained PendingEnrollment/Active/ResetRequired status and password verifier unchanged. Do not persist a redundant boolean or previous-status field. Authorized no-op retries leave the timestamp unchanged. Concrete transaction/concurrency mechanisms remain **TBD**.
- Role is authoritative here, not in cached browser/socket data.
- Account deletion removes credentials without cascading into unrelated/unexpired History. Minimal noncredential references follow the applicable History lifetime.

Password length/character rules are confirmed in [LLD Section 9.1](lld.md#password-policy) (LLD-023); salted Argon2 via RustCrypto `argon2` is confirmed in [LLD Section 9.2](lld.md#password-hashing) (LLD-024). DO-019 approves AccountRecord fields/types/nullability and its combined PHC verifier representation. DO-020 approves username length/characters. DO-021 approves username casing/trimming/comparison. TBD — username SQL/constraint and control-character integration, final Argon2 profile/runtime/physical constraints, account-operation transaction/race handling, and privileged self-removal/last-admin rules. DO-022 approves lifecycle/disablement and credential-epoch policy below; corresponding SQL/transaction/concurrency implementation remains open. Account-enable functionality/API is required (HLD-078), executable only by another admin account or developer CLI. Enable restores the pre-disable lifecycle state without reviving old sessions/links; disablement is represented solely by nullable `disabled_at`, independently of lifecycle status. No role-editing feature is selected.

##### Account lifecycle and disable/enable rules — approved DO-022 subdecision

- Create accounts in `PendingEnrollment` with `disabled_at = None` (SQL `NULL`). Successful first-password setup changes status to `Active`.
- A privileged password-reset request on an enabled enrolled account sets `ResetRequired`; old-password login stays blocked until successful reset returns it to `Active`. Expiry of a reset link does not restore access.
- Disable sets a previously absent `disabled_at` to the trusted backend timestamp without changing lifecycle status or the stored password verifier. Revoke existing sessions/links; reject login, enrollment (including reissue) and password-reset operations while disabled. The existing hosted-nonterminal-game guard and Directory assignment gate still apply to disable/delete.
- Enable is executable only by another enrolled admin account or developer CLI. Clear `disabled_at` to `None`/SQL `NULL`, preserving status/verifier, without reviving old sessions/links. `Active` may authenticate afresh with its existing password; `PendingEnrollment` / `ResetRequired` require fresh links issued through the corresponding existing flows, not revival of old links or automatic session creation.
- Set `disabled_at` on the accepted enabled-to-disabled transition (`None` → timestamp) and clear it on disabled-to-enabled (`timestamp` → `None`). Already-disabled disable and already-enabled enable are no-ops after authorization checks; retries do not rewrite timestamps. Timestamp/lifecycle guard and credential-revocation updates must be consistent; SQL/transaction and command-retry mechanisms remain **TBD**.
- Deletion removes the account rather than adding a `Deleted` enum variant. Deleted accounts are not re-enabled; unrelated/unexpired History remains governed by its own retention.

**Credential-epoch rules — approved DO-022:** `credential_epoch` is an account-wide `Revision`, initialized to 0. Increment exactly once for replacement enrollment-link issuance; password-reset-link issuance or reissuance; successful password-setup/reset completion; and an accepted enabled-to-disabled transition. Do not increment for reads, ordinary login, link redemption alone, single-session logout, enabling, rejected operations or retries of an already-committed operation. Never roll the epoch back on enable. Each issued link/session stores the account's current epoch. Validate link/session token and require its stored epoch to match the current account epoch, as well as scope, expiry, individual revocation, lifecycle and `disabled_at`; links also require the correct purpose and unused state. Commit the increment with the related account/credential changes in AccountsObject; any credential issued by that transition carries the new epoch. Preserve concurrent ordinary logins. Reject overflow rather than wrapping. This policy does not extend fixed deadlines, determine reset-completion auto-login, or itself implement immediate cross-Object GameObject/WebSocket revocation. Exact SQL transactions, race/retry mechanics, credential schemas and cross-Object delivery remain **TBD**; this approval is not implementation authorization.

#### `AccessLinkRecord`

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

Store a verifier, never the reusable raw token. Link consumption and restricted-session creation commit atomically; the epoch must match the account. Expiry stays fixed from issuance. Enrollment reissue invalidates prior links/restricted sessions. Losing a redemption response does not make a consumed link reusable.

**TBD — token construction/verifier, safe delivery/redemption, concurrent consume constraints, predecessor cleanup and lost-response handling.**

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

`Normal` means account authentication, not permission for every game action. Current role and designated-host assignment still govern actions. On enrollment completion, invalidate the restricted credential and issue a rotated normal credential without extending the original absolute deadline.

**TBD — cookie/session format, rotation transaction, post-reset session/navigation behavior, authority lookup/caching and artifact cleanup.**

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

These records identify active game connections for revocation delivery and cleanup; they grant no authority. An epoch/subscription table alone does not guarantee immediate revocation. Fresh authority checks and reliable delivery are still required.

**TBD — subscription uniqueness/registration races, revocation delivery and acknowledgement, fail-closed behavior, expiry and stale-subscription cleanup.**

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

**TBD — constraints/defaults, host-assignment/revision transactions, timer races and interaction of non-designated admin activity with host-idle renewal ([LLD Section 12.2](lld.md#122-productsource-questions--do-not-silently-decide-in-implementation)).**

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

**TBD — practical pool-size/resource limit (`u32` is not a product limit approval), exact validation/feasibility algorithm, disabled free-cell representation and exact child-table layouts under the DO-009 collection convention.**

#### `PlayerRecord`

```rust
struct PlayerRecord {
    player_id: PlayerId,
    alias: String,
    alias_key: String,

    joined_at: Timestamp,
    session_epoch: Revision,
    last_explicit_leave_at: Option<Timestamp>,
}
```

`alias` preserves display spelling; `alias_key` applies confirmed ASCII trimming/case-insensitive comparison and is unique among retained players in this game. Pre-start Leave deletes the membership/releases the alias; In Progress Leave retains player/board/award eligibility. No active connection is not membership deletion. Do not persist an authoritative `is_connected` Boolean on this record; it becomes stale after connection loss/recovery.

**TBD — keys/checks, atomic rename/role-switch/Leave/session updates, interpretation of the leave timestamp after return and connection-derived presence mechanics.**

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

**TBD — normalization/version migration, verifier algorithm/cost, throttling and recovery/replacement transactions.**

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
        alias_key: String,
    },
    Spectator {
        spectator_id: SpectatorId,
    },
}

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

**TBD — attachment encoding/version/size, Rust SDK integration, hibernation revalidation, socket supersession, durable alarm scheduling, backpressure and reliable immediate revocation. No provider or runtime behavior has been tested by these sketches.**

<a id="outstanding-decisions"></a>

### 6.9 Outstanding decisions — explicit TBD index

The field sketches provide proposed places to represent state; they do not finish the protocols below. The per-record TBD notes above remain authoritative detail prompts, not additional approved functionality.

| Area | Undetermined detail | Register |
| --- | --- | --- |
| Ownership / cardinality | **Approved — DO-001** Accounts placement/cardinality; **DO-002** History in the original GameObject with minimal Directory index. **TBD** — capacity/partition assessment, detailed ownership/interfaces and protocols. | LLD-003, LLD-012, LLD-015, LLD-018; DO-001, DO-002 |
| Types / identity / schema | UUID v7/typed IDs confirmed ([LLD Section 3.3](lld.md#identifier-policy)); **DO-003** epoch milliseconds, **DO-004** common revision rules and **DO-005** canonical UUID SQLite TEXT approved. **TBD** — constructor/target integration, API wire encoding, remaining physical mappings, table/column layout, constraints, keys/indexes, SQL and migrations. | LLD-004, LLD-020 |
| Directory coordination | **TBD** — publication, the single nonterminal reservation/fencing, assignment gates, transfer/removal races, authority freshness and recovery. | LLD-005 |
| Credential lifecycle | Password minimum/ASCII policy and salted Argon2/library selection confirmed in [LLD Sections 9.1–9.2](lld.md#password-policy); DO-019 approves AccountRecord fields/types/nullability and combined PHC storage in `verifier`. **DO-020** approves username length/characters. **DO-021** approves username casing/trimming/comparison. **DO-022** approves lifecycle/disablement and epoch transition/matching policy. **TBD** — username SQL/constraint and control-character integration, final Argon2 profile/runtime/physical constraints, token/cookie construction, transaction/race/retry mechanics, socket revocation delivery, self-removal and last-admin policy. | LLD-009, LLD-023, LLD-024 |
| Player recovery | **TBD** — answer normalization/protection, throttling and atomic replacement of old credentials/sockets. | LLD-010 |
| Immediate revocation | **TBD** — subscription races, fresh authorization, cross-Object delivery, acknowledgement and failure policy; fields alone are insufficient. | LLD-005, LLD-007, LLD-009 |
| Game/configuration/boards | **TBD** — practical pool limit, generation/feasibility/matching, free-cell storage and distinct pattern-trait signatures. | LLD-004, LLD-008 |
| Attendance / final-view access | **TBD** — presence reconstruction, role switching, grace/idle/expiry scheduling, minimal terminal grants and Exit scope. | LLD-007, LLD-011 |
| Synchronization | **TBD** — role-specific projection/revision rules, snapshots/subscriptions, attachments, supersession and backpressure. | LLD-007 |
| Retry / pending work | **TBD** — typed command/outbox payloads, bounded receipt retention, old-command rejection, retry policy and reconciliation. | LLD-005 |
| History / deletion | **TBD** — month arithmetic, immutable materialization, terminal notice ordering, purge/index/copy cleanup and restore enforcement. | LLD-011, LLD-012 |
| Abuse / audit | **TBD** — rate-limit scopes/thresholds/key privacy, audit placement/content/read permissions and retention. | LLD-009, LLD-010, LLD-015 |
| Bindings / deployment / verification | **TBD** — names, trusted internal interfaces, SDK/toolchain integration, migrations, load/quota checks and executed tests. | LLD-004, LLD-013, LLD-014, LLD-015 |
| Open product interactions | **TBD** — non-designated admin host-idle renewal and privileged account-management edge cases; no implicit policy from a field/enum. | [LLD Section 12.2](lld.md#122-productsource-questions--do-not-silently-decide-in-implementation) |


<a id="item-review"></a>

## 7. Item-by-item review

**Review scope:** Review the still-undetermined choices in this document one at a time. Existing confirmed HLD/LLD constraints are not reopened. The queue deduplicates the ownership worksheets, DATA inventory, record-level prompts and Section 6.9 index; generic worksheet blanks are coverage prompts, not extra decisions. Some primary items can need focused follow-up questions, which will be recorded without renumbering completed items. An accepted design or test plan is not evidence of executed code/tests, deployment, measured capacity or permission to implement.

**Pause rule — user direction:** If a `clarify` question times out, is cancelled, skipped or otherwise unanswered, do not advance or infer any disposition. Leave the current item pending, stop the walkthrough for that turn and wait for the user to return. On resumption, present the same unresolved item rather than moving to the next one.

**Recording:** Record each approval/revision/deferral/rejection here and reconcile affected wording in this document before continuing. Companion LLD/API/source reconciliation stays separate where outside this review's scope; reference any affected companion statuses explicitly rather than silently marking them complete. Untested target/runtime/quality claims remain unverified even after the design direction is approved.

**Review cursor:** Last completed: **DO-022 — Approved**. Current item: **DO-023 — remaining password-verifier profile and physical encoding constraints**. DO-022 approves the lifecycle/timestamp rules and the credential-epoch increment/matching policy; `disabled_at` is the sole stored disablement state. The general no-redundant-boolean rule is recorded in Section 6.2. Continue with DO-023; detailed transactions, races and immediate cross-Object revocation remain separate open design work.

**Companion reconciliation:** The account-placement/cardinality TBD wording in LLD-003 and related LLD summaries predates DO-001 and is superseded for this review by the decision recorded here. Ownership interfaces, security/capacity validation and protocols remain open; the companion LLD/API documents are not edited by this approval.

**Companion reconciliation — DO-002:** History-placement TBD wording in LLD-012 and related companion summaries predates DO-002 and is superseded here for placement only. Snapshot schemas, month arithmetic, coordination and restore mechanisms remain TBD. Companion documents are unchanged.

**Companion reconciliation — DO-005:** The UUID-storage-format TBD wording in LLD Section 3.3 predates DO-005 and is superseded here for SQLite encoding only. API wire encoding, constructors and target wiring remain open; companion documents are unchanged.

**Cross-cutting revision — HLD-077 / DO-014–DO-018:** The user confirmed one nonterminal game at a time, excluding Resolved/Cancelled. DO-014–016 define its nullable-ID reservation and compare-by-ID lifecycle. DO-017 approves a presence-only account-assignment gate keyed by account ID; DO-018 approves acquiring it before the hosted-game check and holding it through disable/delete, with clear-and-reject when hosted games exist and retain-and-retry on interruption.



**Cross-cutting scope clarification — HLD-078 / DO-022:** Account-enable functionality and an API are required for disabled host/admin accounts. Only another fully enrolled admin account with valid authority, or the developer CLI, may execute it; the target account cannot enable itself. Ordinary hosts, players, spectators and unauthenticated callers are excluded. Enabling is distinct from password reset, does not grant a new role or recreate a deleted account, and must not revive revoked/expired credentials. **Approved enable outcome:** restore the pre-disable lifecycle state: Active permits a fresh login with the existing password; PendingEnrollment still requires setup; ResetRequired still requires reset. Previously revoked/expired sessions and links stay invalid. Derive account disablement solely from populated `disabled_at`, preserving lifecycle status through disable/enable without a previous-status copy or redundant boolean. Populate the timestamp to disable and clear it to enable; authorized no-op retries do not change it. Fresh setup/reset links after enable use the existing flows. DO-022 also approves credential-epoch issuance, increment and match rules recorded in the AccountRecord subsection; transaction/concurrency implementation and immediate cross-Object socket revocation remain open. No implementation is authorized. The API proposal is B9/API-33; exact method/path/body details remain proposals.

**Cross-cutting timestamp revision during DO-022:** The user superseded the earlier stored-boolean choice: use `disabled_at` population for account disablement and do not persist equivalent booleans for any stored state already represented by a corresponding timestamp. Account enable/disable authority, preserved lifecycle/verifier and no-op retry semantics remain as recorded above. The user subsequently approved the credential-epoch rules, completing DO-022; cross-Object delivery and transaction/concurrency mechanisms remain TBD.

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
- **DO-018 — Approved:** Acquire the account’s Directory gate before checking for hosted nonterminal games. While held, reject new-game/host-transfer assignments to that account. If a hosted game exists, clear the gate and reject removal; otherwise disable/delete in AccountsObject and then clear the gate. On interruption, keep the gate and retry safely. Serialization with in-flight assignments, receipt ownership and cross-Object recovery details remain **TBD**.

- **DO-019 — Approved:** Approve AccountRecord fields/types and nullability, renaming the account password field to `verifier: Option<EncodedVerifier>`. Keep one complete library-generated Argon2 PHC string (algorithm/version, work parameters, salt and derived hash), absent before password setup; no separate salt/hash fields. Other AccountRecord fields are unchanged. Username policy, exact account-state/epoch transitions, Argon2 variant/parameters, physical constraints and runtime/upgrade protocols remain separately TBD.

**Companion reconciliation — DO-019:** LLD schema-navigation and password-storage references now use `AccountRecord.verifier` and record the approved combined PHC representation. Cryptographic salt terminology is unchanged. API bodies still exclude private account credentials; no API contract change is required.

- **DO-020 — Approved:** Usernames are 10–50 ASCII characters, excluding all ASCII whitespace: HT/tab, LF, VT, FF, CR (0x09–0x0D) and ordinary space (0x20). Other ASCII control characters, including NUL and DEL, remain permitted; do not silently narrow to printable ASCII. Apply to host/admin provisioning via CLI and admin UI. Count decoded characters; no password/player-alias policy change. Case/normalization remains DO-021; transport/UI/storage handling of permitted controls must be verified.

- **DO-021 — Approved:** Trim leading/trailing ASCII whitespace on username entry and backend validation for account creation/login. Apply DO-020 length/character validation to the trimmed result (10–50 ASCII characters, no internal ASCII whitespace). Store/display trimmed original casing; derive temporary ASCII-lowercase comparison values for case-insensitive uniqueness/login, without storing a normalized key or changing other permitted characters. Password/player-alias rules are unchanged. SQL/constraint and control-character integration remain TBD.

**DO-022 — Approved:** Account lifecycle, enable/disable behavior and the account-wide credential-epoch policy are recorded in [the AccountRecord section](#account-records). Epoch begins at zero; increments on replacement enrollment-link issuance, reset-link issuance/reissuance, successful setup/reset completion and actual disable transition. Issued links/sessions carry the current epoch and must match it during validation alongside purpose/scope, expiry, revocation, lifecycle and timestamp-derived disablement checks. Reads, ordinary login, link redemption alone, single-session logout, enable, rejection and committed retries do not increment it. Related local account/credential changes commit together; credentials issued by that transition use the new epoch; overflow is rejected. Fixed deadlines and concurrent ordinary logins are preserved. No automatic post-reset login or immediate cross-Object socket revocation is implied; detailed transactions/retries and delivery remain TBD.

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
| DO-018 | Directory | Account-removal gate acquisition, assignment/transfer races and recovery | [Detail](#directory-records) | Approved | Acquire gate before hosted-game check; block assignments to account; clear-and-reject if a nonterminal game exists, otherwise disable/delete then clear; retain gate and retry on interruption. In-flight-operation serialization and recovery details remain open. |
| DO-019 | Accounts and credentials | AccountRecord fields/types and nullability | [Detail](#account-records) | Approved | Approve AccountRecord fields/types and nullability, renaming the account password field to `verifier: Option<EncodedVerifier>`. Keep one complete library-generated Argon2 PHC string (algorithm/version, work parameters, salt and derived hash), absent before password setup; no separate salt/hash fields. Other AccountRecord fields are unchanged. Username policy is DO-020/021; lifecycle/epoch policy is DO-022. Argon2 variant/parameters, physical constraints and runtime/upgrade protocols remain separately TBD. |
| DO-020 | Accounts and credentials | Username permitted characters and length bounds | [Detail](#account-records) | Approved | Usernames are 10–50 ASCII characters, excluding all ASCII whitespace: HT/tab, LF, VT, FF, CR (0x09–0x0D) and ordinary space (0x20). Other ASCII control characters, including NUL and DEL, remain permitted; do not silently narrow to printable ASCII. Apply to host/admin provisioning via CLI and admin UI. Count decoded characters; no password/player-alias policy change. Case/normalization remains DO-021; transport/UI/storage handling of permitted controls must be verified. |
| DO-021 | Accounts and credentials | Username casing, boundary trimming and comparison-time normalization | [Detail](#account-records) | Approved | Trim leading/trailing ASCII whitespace on username entry and backend validation for account creation/login. Apply DO-020 length/character validation to the trimmed result (10–50 ASCII characters, no internal ASCII whitespace). Store/display trimmed original casing; derive temporary ASCII-lowercase comparison values for case-insensitive uniqueness/login, without storing a normalized key or changing other permitted characters. Password/player-alias rules are unchanged. SQL/constraint and control-character integration remain TBD. |
| DO-022 | Accounts and credentials | Account status/credential-epoch transitions and disable/enable/reset interactions | [Detail](#account-records) | Approved | Enable/API by another admin or developer CLI only (HLD-078); restore pre-disable Active/PendingEnrollment/ResetRequired lifecycle, never revive old sessions/links. `disabled_at` alone represents disablement; no duplicate boolean or prior-status field; authorized same-state retries leave the timestamp unchanged. Start epoch at 0; increment once on replacement enrollment-link issuance, reset-link issuance/reissuance, successful setup/reset completion and actual disable transition. Issued links/sessions store and validate against current epoch plus scope/purpose, expiry, revocation, lifecycle and disablement checks. No increment for reads, ordinary login, redemption alone, single logout, enable, rejection or committed retries. Commit related account/credential changes together in AccountsObject; assign the resulting epoch to credentials issued by that transition; reject overflow. Fixed deadlines and concurrent ordinary logins remain; reset auto-login and immediate GameObject/WebSocket revocation are not determined. Transaction/race/retry/delivery mechanics and implementation remain open/not authorized. |
| DO-023 | Accounts and credentials | Remaining password-verifier profile and physical encoding constraints | [Detail](#account-records) | Pending | Combined PHC string in `AccountRecord.verifier` settled by DO-019; salt/hash co-location already confirmed. Review remaining variant/version, salt/output lengths and physical constraints without reopening the accepted field name/combined representation. |
| DO-024 | Accounts and credentials | Argon2 work factors, supported bounds and runtime validation plan | [Detail](#account-records) | Pending | — |
| DO-025 | Accounts and credentials | Password-verifier upgrades and concurrent-reset protection | [Detail](#account-records) | Pending | — |
| DO-026 | Accounts and credentials | AccessLinkRecord fields/types and constraints | [Detail](#account-records) | Pending | — |
| DO-027 | Accounts and credentials | Random-token construction, verifier algorithm/length and encoding | [Detail](#account-records) | Pending | — |
| DO-028 | Accounts and credentials | Access-link delivery/redemption safety and concurrent single-use consumption | [Detail](#account-records) | Pending | — |
| DO-029 | Accounts and credentials | Enrollment reissue/predecessor cleanup and consumed-link lost-response handling | [Detail](#account-records) | Pending | — |
| DO-030 | Accounts and credentials | AccountSessionRecord fields/types and constraints | [Detail](#account-records) | Pending | — |
| DO-031 | Accounts and credentials | Cookie/session format, binding and request protection | [Detail](#account-records) | Pending | — |
| DO-032 | Accounts and credentials | Restricted-to-normal session rotation transaction | [Detail](#account-records) | Pending | — |
| DO-033 | Accounts and credentials | Post-reset session issuance and navigation behavior | [Detail](#account-records) | Pending | — |
| DO-034 | Accounts and credentials | Account authority lookup/caching and expired/revoked artifact cleanup | [Detail](#account-records) | Pending | — |
| DO-035 | Accounts and credentials | AccountSocketSubscription fields/types, uniqueness and registration races | [Detail](#account-records) | Pending | — |
| DO-036 | Accounts and credentials | Immediate account revocation delivery, acknowledgements and fail-closed behavior | [Detail](#account-records) | Pending | — |
| DO-037 | Accounts and credentials | Stale account-socket subscription expiry and cleanup | [Detail](#account-records) | Pending | — |
| DO-038 | Game and configuration | GameRecord and TerminalOutcome fields/types, defaults and constraints | [Detail](#game-records) | Pending | — |
| DO-039 | Game and configuration | Host-assignment/revision transactions and actor attribution | [Detail](#game-records) | Pending | — |
| DO-040 | Game and configuration | Host-idle renewal policy for non-designated admin activity | [Detail](#game-records) | Pending | — |
| DO-041 | Game and configuration | Idle cancellation versus activity/start timer races | [Detail](#game-records) | Pending | — |
| DO-042 | Game and configuration | GameConfiguration fields/types and disabled free-cell representation | [Detail](#game-records) | Pending | — |
| DO-043 | Game and configuration | Practical numeric pool/resource ceiling | [Detail](#game-records) | Pending | — |
| DO-044 | Game and configuration | Configuration validation and start-time feasibility algorithm | [Detail](#game-records) | Pending | — |
| DO-045 | Players, spectators and access | PlayerRecord fields/types and alias-key constraints | [Detail](#game-records) | Pending | — |
| DO-046 | Players, spectators and access | Atomic alias rename and participant-session binding updates | [Detail](#game-records) | Pending | — |
| DO-047 | Players, spectators and access | Atomic role switching, leave and session updates | [Detail](#game-records) | Pending | — |
| DO-048 | Players, spectators and access | Player leave-timestamp meaning after return | [Detail](#game-records) | Pending | — |
| DO-049 | Players, spectators and access | PlayerRecoveryRecord fields/types and version metadata | [Detail](#game-records) | Pending | — |
| DO-050 | Players, spectators and access | Recovery-answer normalization and version migration | [Detail](#game-records) | Pending | — |
| DO-051 | Players, spectators and access | Recovery-answer verifier algorithm/format/work factors | [Detail](#game-records) | Pending | — |
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
