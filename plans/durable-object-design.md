# Brews Bingo — Durable Object Design

## 1. Document status and navigation

- **Status:** Incremental item-by-item review of the schema proposal moved from the LLD (LLD-025). **DO-001 approves one separate SQLite-backed `AccountsObject` per application environment for host/admin accounts and credentials.** DO-002 approves final History placement in the original GameObject with a minimal Directory index. **All DO-001–DO-107 ledger items are approved; no pending schema approvals remain.** DO-073–DO-107 decisions are reconciled below and in companion planning summaries. Implementation-only work (runtime measurements, SDK/SQL integration, scheduler/outbox delivery and executed release-gate tests) remains unauthorized/unverified. Actual production Argon2 costs/caps, account-password runtime results, and access/session RNG target compatibility remain unmeasured. All ledger decisions are approved; the designs below still distinguish approved behavior from physical implementation details that remain TBD. The ownership/record inventory, Rust-style sketches, invariants and worksheets remain design material, not a finalized complete SQL schema or authorization to implement. See [the review ledger](#item-review).
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
| History and lookup/indexes | Self-contained final snapshots and minimal code/identity/expiry metadata; common three-month deadline. | **Approved placement — DO-002:** History stays in the original GameObject; Directory holds the minimal lookup/code/expiry index. DO-069 approves the logical `GameHistorySnapshot`/winner/player fields and immutability constraints; DO-071 approves the deadline rule. `GameIndexRecord` details, physical tables, materialization and deletion coordination (Sections 6.3/6.6) remain **TBD**. |
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
| DATA-07 | Participant sessions / exit authorization | Stable game/member or spectator binding, fixed expiry/revocation and permitted final-view exit state; resolve a player's current alias from PlayerRecord rather than duplicating it in the session binding. | DO-055 approves `ParticipantSessionRecord`; DO-056 approves `TerminalPlayerViewRecord`; DO-057 approves `AccountGameViewAccess`. DO-058 approves account final-view downgrade/Exit scope; physical storage/cleanup details remain pending. |
| DATA-08 | Private player recovery verifier | Stable member binding and protected verifier; optional, replaceable/deletable by valid-session owner; never public/History data. | DO-049 approves logical `PlayerRecoveryRecord` fields/types/privacy; DO-050 approves normalization/version behavior; verifier profile and transactions remain DO-051/052. |
| DATA-09 | Assigned player board | Immutable assigned layout/string values/free cells plus automatic matched state; no pre-start or recovery-generated replacement. | DO-059 approves `PlayerBoardRecord`/`BoardCell`; DO-060 approves child-row mapping and projection consistency; DO-061/062 approve exact uniqueness and bounded generation. Physical SQL/RNG integration remains **TBD**. |
| DATA-10 | Ordered calls / qualification / outcome | Accepted call order and unique values, authoritative eligibility, at most one winner and immutable terminal result. | DO-059 approves `CompletedLine`; DO-063 approves Single Line evaluation/signature; DO-064 approves `CallRecord` fields/keys; DO-065 approves sequencing/receipt association and DO-066 approves the atomic logical write set/order. Physical SQL/query details remain DO-096–098. |
| DATA-11 | Revisions / command outcomes | Durable consistency/retry identity and accepted result; bounded retention design without authorizing a replay archive. | DO-067/068 approve `ViewRevisionRecord`/`ViewKey` projection fields and ordering. DO-076–080 approve owner/actor-scoped CommandReceipt fields, server-computed canonical SHA-256 fingerprinting, 4-KiB typed secret-free outcomes, 24-hour ordinary receipt window (DO-029 credential-link exception 30 days), expired retry rejection and secret-safe credential issuance retries. Physical mapping remains implementation work; DO-095 sets WSS bounds and recovery policy. |
| DATA-12 | Final History and indexes | Final calls/aliases/winner/boards plus minimal metadata, shared expiry and code-collision lookup; no session/answer credentials. | DO-069/070 approve immutable History fields and ordered child rows; DO-071 approves the fixed UTC calendar-month deadline; DO-072 approves atomic terminal materialization/source cleanup; DO-073/074/075 approve spectator notice/deletion ordering, primary purge/code reuse, and copy/index/log/restore expiry enforcement. Physical scheduling/cleanup remains implementation work. |
| DATA-13 | Deadlines / coordination / audit | Minimum durable state for deadlines, typed cross-Object coordination, rate limiting and admin-action attribution. DO-076–090 approve receipts, pending work, rate-limit buckets, and audit policy; DO-094 approves durable deadline alarms. Per-operation physical payload/migration/runtime mappings remain implementation work. | Persisted deadlines in §6.3–§6.6; operational records in §6.7. |

<a id="schema-worksheet"></a>

## 4. Per-record/table schema worksheet — copy for each record

- **Record ID / purpose / HLD references:** TBD — link a Section 3 DATA ID to its Section 6 proposal.
- **Authoritative Object/store and binding:** DO-096/100 approve Directory/Accounts one per environment, Game per stable game_id, and private GAME_DIRECTORY/ACCOUNTS/GAMES bindings.
- **Table/record name and schema version:** DO-096 approves owner-local normalized snake_case tables/child rows; DO-099 approves versioned owner-local migration tracking. Exact DDL remains implementation work.
- **Lifecycle / readers / writers / privacy classification:** DO-096 approves owner/type/privacy mappings; DO-090 fixes audit read/retention boundaries; per-operation access is captured in the approved records/workflows.

| Column / property | SQL or record type | Nullable / default | Key / constraint / validation | Sensitive-data treatment | Meaning |
| --- | --- | --- | --- | --- | --- |
| TBD | TBD | TBD | TBD | TBD | TBD |

| Schema design item | Value to fill |
| --- | --- |
| Primary identifier and generation | UUID v7 for application-generated system IDs ([LLD Section 3.3](lld.md#identifier-policy)); **DO-005:** SQLite TEXT in canonical lowercase hyphenated form with validated v7 conversion. Concrete constructors, physical key/relationship definitions and target wiring remain **TBD**. |
| Relationships and ownership boundaries | DO-096 approves owner-local normalized record tables; DO-097 permits same-Object FKs where lifecycle allows and application validation for cross-Object references, no cross-Object cascade. |
| Unique/check constraints, canonical alias/code representation | DO-098 approves primary/unique keys for IDs, BINARY usernames/aliases, game codes, token verifiers, connections, calls, cells/History children, receipts/pending work and rate buckets; row-local CHECKs; cross-row rules stay transactional. |
| Indexes and query/access patterns | DO-096/098 approve owner-local indexes needed for approved lookup and uniqueness; exact index names, query plans and statements remain implementation work. |
| Foreign-key behavior or application-enforced cross-owner references | DO-097 approves local foreign keys only; cross-Object references/authority use trusted owner protocols, never SQL cascades. |
| Read/write statement shapes and parameterization | DO-098 requires fixed parameterized SQL for values; never interpolate caller values or dynamic identifiers. Exact statements/query plans remain TBD. |
| Timestamp-derived boolean state | Confirmed rule in Section 6.2: no persisted boolean duplicating a corresponding timestamp; document presence/deadline/reset semantics per record. Remaining record-specific details **TBD**. |
| Transaction boundary and invariants maintained | DO-066/072 approve owning-Game atomic write sets; DO-083 approves owner-local per-operation phases/fences/peer ACKs without cross-Object atomicity. Other invariant boundaries are defined per decision. |
| Concurrency/conflict detection and retry outcome | DO-076–084 approve actor-scoped receipts, fingerprint conflicts, bounded retention, old-command rejection, typed outbox phases, expected revisions/fences and capped retries; DO-093 approves participant socket fencing. |
| Retention trigger, persisted deadline, read denial and deletion | DO-071–075 approve fixed History expiry, denial, primary purge/reuse and copy/restore protection; DO-079/090 approve receipt/audit windows; DO-094 approves durable deadline alarms. |
| Copies/indexes/logs/backups and restore-time expiry enforcement | DO-075 prohibits expired History resurrection; DO-090 applies audit expiry. Exact provider copy/backup controls remain validation work. |
| Size, read/write amplification and quota assumptions | DO-103 approves max-game/owner-forecast validation with 2× quota headroom; forecast and measured amplification/capacity remain unverified. |
| DDL / initialization / migration / rollback | DO-096–099 approve normalized mapping, local-FK/constraint/query policy, and versioned forward-only fail-closed migrations. Exact DDL/runtime migration hooks remain implementation work. |
| Schema and constraint tests | DO-104 approves migration, constraint, transaction/race/security/load release gates. No application tests have been run. |

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
| `Timestamp` | **Approved — DO-003:** `i64` / SQLite `INTEGER`: UTC Unix epoch milliseconds since `1970-01-01T00:00:00Z`. Trusted backend record owners generate timestamps/deadlines; browser time and UUID v7 time components are not authoritative. Explicit sequence/revision fields govern ordering. History expiry’s UTC calendar-month arithmetic is approved in DO-071; clock rollback/skew handling and target integration remain **TBD**. |
| `Revision` | **Approved — DO-004:** nonnegative `i64` / SQLite `INTEGER`; new counters start at `0`. Increment when required by the accepted operation, within the owning Object transaction. Reads, rejections and retries do not themselves advance counters. Reject required advancement at the maximum instead of wrapping/resetting. Per-counter triggers remain **TBD** in their lifecycle/protocol; this is not time or cross-Object global ordering. |
| `GameCode` | Validated `String`: exactly eight uppercase ASCII letters/digits, preserving confirmed normalization and reuse rules. |
| `Digest` | `Vec<u8>`: digest bytes. **DO-027 approves `AccessLinkRecord.token_verifier`; DO-031 approves `AccountSessionRecord.token_verifier`** as 32 raw SHA-256 bytes stored in SQLite `BLOB` with a unique constraint/index, each purpose-specific. Other uses (including request fingerprints) remain **TBD**. |
| `EncodedVerifier` | `String`: encoded algorithm/version, parameters, salt and derived hash. Account passwords use **salted Argon2** (LLD-024); **DO-019 approves** a complete library-generated PHC string in `AccountRecord.verifier`, following [LLD Section 9.2](lld.md#password-hashing). Final Argon2 profile, physical constraints/mapping and cost remain **TBD**. Recovery-answer algorithm/format/cost remain independently **TBD**; this shared type does not impose the account-password scheme on answers. |
| `Option<T>` | **Approved mapping — DO-009:** optional scalar fields use SQL `NULL` for absence, not sentinel strings/IDs/numbers. Optional whole entities may use row absence. No implicit default; defaults require explicit per-record specification. |
| `Vec<T>` | **Approved mapping — DO-009:** use child rows rather than whole-collection JSON blobs, retaining explicit sequence/position when order matters. Empty collections have no child rows. Exact child tables, relationships, indexes and constraints remain **TBD**. |
| `SecretFreeJson` | Legacy/general safe JSON notation only where a separately approved typed boundary allows it; never raw credentials/verifiers/answers/History. DO-078 replaces CommandReceipt outcomes with a closed versioned tagged type family capped at 4 KiB UTF-8; DO-082 replaces pending-work generic JSON with typed `PendingPayload`. |

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

**Approved — DO-044:** Validate the complete resulting configuration on create/update and again before Start: numeric upper bound 1–1,000, square side 2–10, player capacity 2–20, spectator capacity 0–50, DO-042 free-cell constraints, and the sole first-release SingleLine pattern. Let `N = numeric_upper_bound` and `k = board_side_length² - configured_free_cell_count`. Reject when `N < k`. The number of cell-by-cell-distinct layouts is the falling factorial `P(N,k) = N × (N−1) × … × (N−k+1)`; `P(N,0) = 1`. Count only up to the retained start-time player membership count by saturating multiplication there, avoiding overflow. Reject Start if the feasible-layout count is less than the number of retained memberships, including disconnected players (HLD-034); the separate minimum of two currently connected players still applies. Invalid configuration patches apply nothing. DO-062 approves bounded CSPRNG partial-shuffle generation and fail-closed Start behavior; see the board-generation decision under `PlayerBoardRecord`. Physical SQL/indexes and Start transaction integration remain implementation design.



**TBD:** Physical table layouts under the DO-009 collection convention and target RNG integration/performance.

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

**DO-045 — Approved:** Store only the accepted trimmed `alias` in PlayerRecord, with display casing unchanged and exact case-sensitive uniqueness/recovery comparison. Preserve the other shown PlayerRecord fields/types. Alias input follows HLD-050/051. Per-game uniqueness comparison is serialized and atomic. Physical keys/indexes remain TBD. DO-046 approves stable-`player_id` session binding; DO-055 approves complete ParticipantSessionRecord fields and live/final access constraints.

**DO-046 — Approved:** In Awaiting Players, a valid player session may rename only its own membership to a valid, available alias using exact case-sensitive comparison. Atomically update the PlayerRecord alias and release the old alias while preserving `player_id`, seat, join time, role/capacity and recovery answer. Invalid/claimed aliases and late renames fail without mutation. Player sessions bind by stable `player_id`, not the alias; alias updates do not rewrite/rotate/revoke credentials or renew fixed expiry. Views and future recovery resolve the current alias from PlayerRecord. DO-055 approves the full ParticipantSessionRecord schema/access rules; response/retry contracts and physical SQL/indexes remain separate.

**DO-047 — Approved:** In Awaiting Players, role switch requires target-role capacity and commits atomically. Player→spectator releases player alias/seat, deletes PlayerRecoveryRecord, retires player-role sessions/sockets and creates spectator membership/session. Spectator→player requires a valid available alias/player slot, creates a fresh PlayerRecord/PlayerId and optional fresh answer, and retires spectator authority; it never restores deleted membership/answer. Replace the role-bound session at its original absolute expiry (no extension). Rejection preserves the original role/seat/credentials/verifier. Explicit pre-start player Leave removes membership/recovery/session and releases alias/seat; later join is fresh admission. In Progress player Leave retains membership/seat/board/matches/winner eligibility; a still-valid session may return until its fixed expiry, otherwise the established answer recovery applies. Spectator Leave immediately releases seat and revokes/deletes its session; later entry is fresh admission if capacity remains. Serialize against start/admission; no partial role/seat state. DO-048 approves retaining the latest explicit In Progress Leave timestamp after return; DO-055 approves the complete session schema/access; physical transaction/fencing integration remains implementation work.

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

**DO-049 — Approved:** Preserve `player_id: PlayerId`, `answer_verifier: EncodedVerifier`, `normalization_version: u32`, and `updated_at: Timestamp`. Absence means answer-based recovery is disabled; store no plaintext, and never expose the record in projections or History. Delete on accepted player-to-spectator switch, membership removal or terminal transition; answer replacement is atomic. DO-050 approves normalization/version behavior; DO-051 approves the Argon2id/PHC verifier profile and benchmark gate; DO-052 approves recovery/replacement transaction and session/socket-fencing policy.

**DO-050 — Approved:** Version 1 canonicalization applies Unicode NFC, trims outer Unicode whitespace, then applies Unicode case-folding; preserve internal whitespace and punctuation without collapsing/stripping them. Use the stored `normalization_version` to select the comparison profile for each verifier. A future profile change must not silently reinterpret an existing verifier: retain support for its old version until the player replaces the answer while authenticated, write the replacement with the current version, and never store plaintext or silently migrate a verifier.

**TBD:** Exact Unicode data/library pin and canonicalization implementation; production Argon2 costs/caps and target-runtime measurements; and rate-limit thresholds/durable implementation. DO-052 approves the logical recovery/replacement transaction, optimistic revalidation, session-epoch and socket-fencing policy; concrete physical integration remains implementation planning.

**DO-051 — Approved:** Use Argon2id v19 for player recovery answers, with a fresh independent 16-byte CSPRNG salt, 32-byte output and a complete PHC-encoded `EncodedVerifier`. Explicitly configure bounded memory/time/parallelism costs and reject malformed, unsupported or out-of-bounds verifier parameters before expensive work. Use 19 MiB / 2 iterations / parallelism 1 only as a benchmark starting point; production costs/caps remain pending target-runtime/resource measurements. Rate-limit thresholds/durable implementation remain open; DO-052 separately approves recovery transaction, retry and session/socket-fencing policy.

**DO-052 — Approved:** Perform Argon2 verification outside the write transaction, then under the owning GameObject’s serialized authority recheck eligible lifecycle, stable player membership/current alias, and unchanged recovery-verifier value/normalization version before committing recovery. On success atomically increment the PlayerRecord `session_epoch` once, invalidate predecessor sessions for that membership, and issue a fresh fixed one-day session at the new epoch. Old sockets fail authorization at their next frame after the epoch change; socket-close work is asynchronous and is not complete until closed/already-absent is acknowledged. Authenticated answer set/replace/delete is atomic, limited to Awaiting Players/In Progress, and does not rotate/extend the caller’s session. If a recovery’s verified record was changed/deleted before commit, fail closed without granting a session. A lost recovery response never replays/restores a session secret; the player must prove the answer again. Rate-limit thresholds and physical session/socket integration remain separate implementation planning; DO-055 approves the full ParticipantSessionRecord fields/access constraints.

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

**DO-053 — Approved:** Preserve the shown logical fields/types. `disconnected_at` and `grace_expires_at` are both absent while connected and both present during retained disconnect grace; the deadline is exactly five minutes after `disconnected_at`. The row continues to occupy spectator capacity during grace and is deleted on explicit Leave, grace expiry or terminal cleanup. `session_epoch` is a `Revision`; DO-054 approves current-session/epoch/connection matching for presence events, while DO-055 approves complete session binding/access.

**DO-054 — Approved:** Serialize spectator disconnect, reconnect and expiry decisions in the owning GameObject using trusted commit-time. Apply a disconnect only if spectator ID, current session/epoch and connection ID still match; stale closes are no-ops. The first matching unexpected disconnect sets both timestamps (`disconnected_at = now`, `grace_expires_at = now + 5 minutes`); duplicates do not restart grace. Reconnect requires the same eligible, unexpired spectator session and current identifiers, committed before the deadline; atomically clear both timestamps while retaining the same spectator ID/seat and original session expiry. At `now >= grace_expires_at`, expiry wins: re-read current presence/deadline and delete occupancy/session only if still disconnected. A reconnect committed first makes the stale expiry a no-op. After expiry, the old session cannot revive the seat; re-entry is fresh admission if lifecycle/capacity allow. Stale sockets fail current authorization; closure is tracked separately. DO-055 approves full session binding/access and multi-connection rules.

**TBD:** Hibernation/presence reconstruction, physical event/alarm payloads, projection/notification shape and close-retry integration; DO-056 approves terminal player-session policy and DO-058 approves account final-view/Exit policy.

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
```

**DO-046 approves the player-binding rule:** Bind by stable `player_id` only; do not persist a duplicate alias or derived key in the binding. Alias rename therefore changes the PlayerRecord but does not rewrite the session binding or extend its deadline. DO-055 approves the full logical session schema and live/final access policy; DO-056 covers the minimal terminal record and Exit/replay/deletion guards.

```rust
enum ParticipantAccess {
    Live,
    FinalViewOnly,
}
```

**DO-055 — Approved:** Preserve the shown logical `ParticipantSessionRecord` fields/types and `ParticipantBinding`/`ParticipantAccess` variants. Store only `token_verifier: Digest`, not a raw token; do not add stored expired/revoked booleans. Player bindings use stable `player_id`, never alias; spectator bindings use stable `spectator_id`. Each request/frame requires a non-revoked, unexpired session at the current participant epoch. `Live` permits role-appropriate access only for an eligible nonterminal membership. Only an existing eligible, unexpired, not-exited player session may receive terminal final-view access until Exit or its original one-day expiry, whichever comes first, without renewal. DO-056 defines the minimal terminal record that replaces the full live session row. Spectator session data is deleted at terminal; spectators cannot reconnect, and no new participant session/recovery is issued into a terminal game. Enforce one live WSS connection per participant session; a newly authorized connection supersedes its predecessor, and a stale close cannot alter current authority/seat. Role-switch replacement preserves original expiry; DO-052 recovery uses the approved new one-day session/epoch rotation. Token encoding, cookie wire details and physical session integration remain implementation planning.

```rust
struct TerminalPlayerViewRecord {
    session_id: SessionId,
    token_verifier: Digest,
    game_id: GameId,
    player_id: PlayerId,
    expires_at: Timestamp,
}
```

**DO-056 — Approved:** At a started game’s terminal commit, delete player recovery verifiers and spectator identity/session data. For each existing player session that is unrevoked, unexpired, at the current epoch, and not already exited, replace the full `ParticipantSessionRecord` with the minimal `TerminalPlayerViewRecord` above, preserving the original absolute expiry. It authorizes only that player’s final read-only view. Never issue or renew a terminal session; enforce expiry even if deletion is delayed. Exit atomically deletes every terminal grant for that player and closes their connections; retries cannot recreate access. Terminal requests cannot re-admit or recover a player. Pre-start cancellation still deletes all game/participant data. Physical close/retry integration remains open.

**DO-058 — Approved:** At terminal commit, downgrade only pre-existing eligible host/admin `LiveView` grants to `FinalViewOnly`; create no grant for a new terminal viewer. Preserve each grant only through the earliest of Exit, the bound account session’s original expiry, and History expiry; downgrade may shorten but never extend DO-057’s grant expiry. Exit deletes every grant for that account/game across all account sessions and closes those view connections. It does not log out the account or remove its History permission; another session cannot recreate terminal-game access. Delayed cleanup never extends authorization. Physical cleanup and socket-close integration remain open.

**TBD:** Physical schema/index and command-receipt details; token/cookie wire and socket-close integration.

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

**DO-057 — Approved:** Preserve the shown `AccountGameViewAccess` fields/types and `LiveView`/`FinalViewOnly` variants. Create a `LiveView` grant only when an enrolled host/admin with a current eligible account session enters a nonterminal game; bind it to that exact account session and do not let `expires_at` exceed that session’s original expiry. The grant is tracking metadata, not authority: each request revalidates account/session/game permission. Do not create a live-game grant from terminal access or History. DO-058 approves terminal downgrade, Exit scope across account sessions and minimal retention.

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

`Free` is valueless, not a pool value `"FREE"`, and is matched immediately. Ordinary cells match only through accepted calls. Empty `qualifying_lines` means no current qualification; `evaluated_through_call = 0` is initial evaluation before calls. One board per player exists only after Start. Exact complete layouts determine uniqueness; any optional digest optimization must not replace exact equality. Cells/matches/qualification are persisted projections of the immutable layout and committed calls, updated consistently with each call. DO-063 defines the pure Single Line evaluation boundary and deterministic completed-line ordering.

**DO-059 — Approved:** Preserve the shown `PlayerBoardRecord`, `BoardCell`, `BoardCellKind` and `CompletedLine` fields/types and variants. Free cells are valueless and initially matched; ordinary values match only through accepted calls. Empty `qualifying_lines` means no current qualification; `evaluated_through_call = 0` is the initial pre-call evaluation. Boards exist only after Start. Cell/match/qualification data are persisted projections kept consistent with committed calls. **DO-060 — Approved:** Store board scalars in one parent row keyed by `(game_id, player_id)`, cells as child rows keyed by `(game_id, player_id, row, column)`, and qualifying lines as ordered child rows, not JSON vectors. Persist cell/qualification projections consistently with the immutable layout, initially matched free cells and accepted-call history; advance `evaluated_through_call` consistently. **DO-061 — Approved:** Within a game, every complete player-board layout is unique by exact equality of `(CellPosition, BoardCellKind)` across all cells, including free-cell positions. Ignore mutable match/qualification projections; a digest may accelerate lookup but never replace exact comparison. Enforce before committing Start, including all retained memberships, disconnected or not. Cross-game uniqueness is not required.

**DO-062 — Approved:** After DO-044 feasibility passes, process retained start-time players in stable `player_id` order. For each candidate board, use unbiased partial Fisher–Yates over the implicit pool `1..=numeric_upper_bound`, with CSPRNG-driven unbiased bounded-index sampling; map sampled values to non-free positions in row-major order. Check each candidate against accepted layouts using DO-061 exact equality. Cap the whole Start attempt at 1,024 candidate boards, including duplicates. CSPRNG failure or budget exhaustion fails closed: no weaker PRNG fallback, duplicate boards, partial assignment or InProgress transition; preserve AwaitingPlayers, memberships and the global reservation. Command/receipt retry mechanics remain separate. The cap is a product guardrail, not a target-performance claim.

**DO-063 — Approved:** Implement Single Line behind its own `SingleLinePattern` Rust trait, not solely a generic pattern trait, with `evaluate(side_length: u8, cells: &[BoardCell]) -> Result<Vec<CompletedLine>, PatternEvaluationError>`. Evaluation is pure/provider-independent and validates board shape. `CompletedLine::Row(u8)` and `Column(u8)` use one-based indices; diagonals are the two full corner-to-corner lines. Return every completed line in deterministic order (rows ascending, columns ascending, main diagonal, anti-diagonal); nonempty means qualified. Re-evaluate durable cell state at initial assignment (free cells initially matched) and after every accepted call, including departed/offline players. Qualification never awards or resolves; future patterns get their own pattern-specific traits. No implementation or dependency is authorized.

**TBD:** concrete table names/SQL constraints/indexes, target RNG integration/performance, and physical call SQL/query details (DO-096–098).

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

**DO-064 — Approved:** Preserve the shown CallRecord fields/types and `CallMode::{Random, Manual}`. Key records by `(game_id, sequence_no)` and require `(game_id, value)` uniqueness so each value is accepted at most once per game. Derive the latest call from the highest sequence number. DO-065 settles sequence start/increment/overflow and the actor-scoped receipt association; DO-066 approves the atomic logical write set/order. Physical SQL/query details remain DO-096–098. Calls, affected projections, revisions and receipts still commit before delivery.

**DO-065 — Approved:** Start `sequence_no` at 1 and allocate the next number only in the accepted-call commit, increasing by exactly one. Rejections, no-ops and retries consume no sequence. Use checked arithmetic and fail closed without mutation at overflow; the approved 1,000-value pool makes `u32` overflow unreachable under normal game rules. Associate each accepted CallRecord's `command_id` with a secret-free receipt in the owning GameObject, scoped by the authenticated `ActorRef::Account(called_by_account_id)` and command ID within that game. Keep the operation and request fingerprint in the receipt; exact actor/ID/fingerprint retry returns the committed result, while reuse with a different fingerprint rejects. The association is logical, not a foreign key that prevents receipt expiry; retention/admissibility remain DO-076–079.

**DO-066 — Approved:** In the owning GameObject, process accepted random/manual calls in one SQLite transaction. Revalidate current mutation authority and InProgress state; serialize random/manual value selection or validation against the undrawn pool; resolve the DO-065 actor-scoped receipt before mutation (same actor/ID/fingerprint returns the committed result without new writes; different fingerprint rejects). For a new accepted call, insert the checked next-sequence CallRecord, update all retained boards’ matching-cell, qualification and `evaluated_through_call` projections, advance each visibly affected DO-068 view revision once, and associate the secret-free receipt in the same commit. Commit all-or-none and emit authorized WSS updates only afterward. Rejections, conflicts, no-ops and transaction failures consume no sequence or mutate board/revision state. Qualification alone does not award or resolve the game. Exact SQL/API syntax, schema constraints, random-sampling algorithm, general receipt schema/retention and delivery mechanics remain separate.

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

DO-067 approves these fields as projection revisions, not raw internal mutation counts. `Host` is the shared authorized host/admin projection, not authority; `Player(PlayerId)` is that stable player’s private projection; `Spectator(SpectatorId)` is that spectator’s public projection. Account/session grants and authentication are separate from ViewKey. A private change for one player does not advance another player’s revision.

**Approved — DO-067:** Preserve `ViewRevisionRecord { view: ViewKey, revision: Revision }` and `ViewKey::{Host, Player(PlayerId), Spectator(SpectatorId)}`. Revisions track visible projections, not internal mutations; account/session grants remain separate; a player-private change does not advance another player’s revision. **DO-068 approves** per-projection advancement, serialized snapshot/subscription ordering, gap recovery, independent authority revalidation and terminal counter lifetime. Physical delivery/hibernation/backpressure remain **TBD**.

**DO-068 — Approved:** Apply DO-004's `Revision` rules: initialize each projection counter at 0 and advance each affected authorized projection exactly once in the owning transaction when its visible data changes; internal-only changes, rejection, no-op and retry do not advance it. A connection/resync is one serialized cut that registers the attachment and captures the full authorized snapshot and revision; send that snapshot before later updates, whose revisions increase monotonically. Ignore duplicate/stale revisions; on a detected gap, request a fresh authorized snapshot rather than applying across the gap. Revision is freshness metadata, never authority; revalidate session/role/Exit/expiry for each operation and outgoing frame. At terminal transition, publish the final permitted projection/revision and retain only counters needed for still-eligible final-view access; delete them when that access ends and never copy revisions into History. Physical delivery/outbox, hibernation and backpressure remain later decisions.

<a id="history-records"></a>

### 6.6 Final History inside the original Game Object

**Approved placement (DO-002):** retain final History in the original Game Object, with only minimal searchable index/code/expiry metadata in the Directory and no separate History Object/datastore. DO-069 approves the logical immutable snapshot fields/constraints below, not retention of all live tables as History. DO-070 approves the logical child-row mapping/order below; **TBD — SQL constraints and terminal materialization/cleanup protocol.**

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

**DO-069 — Approved:** Preserve exactly the shown `GameHistorySnapshot`, `HistoryOutcome`, `WinnerSnapshot` and `HistoryPlayerSnapshot` fields/types. Create a snapshot only for a started terminal game. `winner` is present iff `outcome` is Resolved and identifies a player in `players` with the same final alias; Cancelled has no winner. `ordered_calls` contains immutable accepted values in call order; `players` contains each participating `player_id` once with its final board cell state. The snapshot contains no credentials, spectators, recovery material, sessions/presence, editable configuration or intermediate revisions. Preserve DO-071 expiry. `ordered_calls` and `players` are logical ordered collections; DO-009 selects child rows and DO-070 decides child-row mapping/order.

**DO-070 — Approved:** Represent each started terminal snapshot with one parent row per game containing the approved scalar snapshot fields and the winner ID/alias together iff Resolved. Store call values in child rows keyed by `(game_id, sequence_no)` and read in accepted-call order. Store player snapshot rows keyed by `(game_id, player_id)` and return players in stable `player_id` order. Store final board cells in child rows keyed by `(game_id, player_id, row, column)` and read one-based row-major; preserve each `BoardCell`'s kind/value/matched state. Use child rows, not JSON vectors, and retain no unapproved data. This logical mapping/order does not approve SQL constraints, materialization, cleanup or restore mechanics.

Exclude passwords, tokens, verifiers, spectator identities, live sessions/presence, recovery metadata, intermediate board revisions and unrelated editable configuration. Minimal existing final-view grants remain separate from History and never confer a new History entitlement.

At the first started-game terminal commit, the owning Game Object’s SQLite transaction must persist the approved DO-069 snapshot using DO-070’s row mapping with the fixed DO-071 deadline, together with the terminal GameRecord outcome. DO-072 approves this as one atomic transaction: it also removes obsolete mutable gameplay source rows whose content is fully represented by History, while retaining minimal terminal/History routing metadata and eligible DO-056/058 final-view grants within their original bounds. A rollback commits neither terminal transition nor History snapshot. DO-073 approves best-effort delivery to connected spectators after terminal commit, without required application ACK or indefinite wait, followed by deletion of server spectator identity/session data; disconnected/failed spectators cannot reconnect to recover the result. DO-074 approves expiry-denied access/code reuse at `now >= expires_at` and durable idempotent Game/Directory purge, clearing only the matching `game_id`. DO-075 requires secondary copies preserve the same fixed expiry, prohibits History in logs/unbounded exports/caches and blocks expired data/index resurrection on restore; provider retention controls and physical cleanup remain implementation gates. Cross-Object Directory updates are separately coordinated. Pre-start cancellation creates no snapshot and follows full-deletion rules.

Directory and Game must use the same persisted History deadline. **DO-071 — Approved:** Compute it once from the first terminal-commit timestamp using UTC calendar-month addition, preserve UTC time-of-day, and clamp to the target month’s final day when needed. Deny at `now >= expires_at`, even if purge is delayed; never use a fixed day count or extend on reads/retries. **Approved — DO-072:** The owning Game Object transaction atomically writes terminal outcome and complete History rows from that final state with DO-071 expiry, and removes obsolete mutable gameplay source rows represented by History. On failure, neither terminal state nor snapshot commits. Preserve minimal terminal/History routing metadata and bounded pre-existing final-view grants; Directory updates are separately coordinated. **Approved — DO-073–075:** Connected-spectator final delivery is best-effort after terminal commit; cleanup does not wait for acknowledgement and terminal reconnect is not recovery. Primary purge/code reuse follows the fixed DO-071 deadline with compare-by-game-ID Directory cleanup. Secondary copies/logs/exports may not extend access; restore enforces expiry before reads or collision results. Physical scheduling, provider retention and deletion integration remain implementation work.

<a id="operational-records"></a>

### 6.7 Shared operational records

These records belong to whichever Object owns the operation; they do not introduce additional Object classes. **Approved — DO-076–090:** CommandReceipt/ActorRef are owner-local and keyed by `(owner Object, authenticated actor, command_id)`; SHA-256 fingerprints use versioned canonical semantic requests; outcomes use bounded typed secret-free tagged variants ≤4 KiB. Ordinary receipt window is 24 hours, with DO-029 credential-issuance receipts retaining their approved 30 days; expired retries are rejected, never re-executed. Credential-bearing retries never replay secrets. PendingOperation is owner-local with DO-081 fields and DO-082 typed payload families; DO-083 applies operation-specific revisions/fences/ACKs; DO-084 retries exponentially from 1 second to a 5-minute cap without silent abandon. RateLimitBucket fields/owners are DO-085; DO-086 is 5 failures/15 minutes then a 15-minute block; DO-087 uses scoped HMAC keys, layered caller/game controls and short-lived buckets. AdminAuditRecord is owner-local; DO-089 captures authenticated admin success/reject/failure and CLI-path actor attribution; DO-090 retains 90 days and allows privileged CLI read only. Exact SQL/query statements and runtime implementations remain implementation work.

`CommandOutcome` is a closed versioned tagged family of operation-specific acknowledgements; persist only minimum nonsecret committed identifiers/revisions/deadlines, capped at 4 KiB serialized UTF-8. Per-operation variants follow the API contracts. `PendingPayload` is a closed tagged family for the DO-082 operation groups; variants carry bounded identifiers/revisions/deadlines only, never secrets or History snapshots.

#### `CommandReceipt`

```rust
struct CommandReceipt {
    command_id: CommandId,
    actor: ActorRef,
    operation_name: String,
    request_fingerprint: Digest,
    outcome: CommandOutcome, // closed versioned tagged safe result, ≤4 KiB UTF-8
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

DO-076 approves uniqueness within each owner Object on `(authenticated ActorRef, command_id)`; actor is server-derived. DO-077 approves a versioned canonical request encoding hashed with SHA-256; exclude bearer credentials, transport headers and server timestamps. Exact actor/ID/fingerprint retries replay only the receipt; mismatched reuse rejects. DO-078 approves a closed versioned typed outcome family, ≤4 KiB UTF-8, secret-free and without private snapshots. DO-079 approves 24-hour ordinary retention and rejects retries after expiry; DO-080 forbids bearer/cookie/URL replay and uses DO-029 successor reissue for links/new command IDs for fresh session issuance. DO-029's 30-day link-issuance receipt is the specific exception. `DeveloperCli`/`System` do not grant public authority. Canonical serialization implementation remains TBD.

#### `PendingOperation`

```rust
struct PendingOperation {
    operation_id: OperationId,
    operation_name: String,
    target: TargetRef,
    phase: CoordinationPhase,
    expected_revision: Option<Revision>,
    fence_generation: Option<Revision>,
    payload: PendingPayload, // closed operation-tagged, secret-free variant
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

DO-081 approves the owner-local fields/TargetRef/CoordinationPhase; `payload` is the closed typed DO-082 operation family for Directory publication, reservation, account removal/host transfer, socket close/ack, terminal cleanup/notice and History index/purge. Payloads contain bounded nonsecret IDs/revisions/deadlines only. DO-083 requires owner-local durable intent, per-operation phases/fences/revisions, receiver-side state validation and typed acknowledgements; do not claim peer completion before required ACK. DO-084 approves saturating attempt counts and deterministic exponential retry from 1 second to 5 minutes, with no silent abandon; unsafe orphans remain for reconciliation/alert. No generic remote SQL or cross-Object atomicity is implied. Exact variants’ physical encodings and reconciliation tooling remain implementation work.

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

DO-085 approves these fields/scopes and AccountsObject ownership for account login/access-link buckets; player-recovery buckets live in the owning GameObject. DO-086 approves failure-only accounting: 5 failures in a fixed 15-minute window blocks 15 minutes; success clears; blocked attempts do not extend the block. DO-087 approves domain-separated HMAC-SHA-256 subject keys using a server-managed key, trusted edge metadata for caller aggregation, layered account/game controls and deletion after window/block. Never store raw answer/username/token/IP or long-term attempt history. Key rotation and exact scope composition remain implementation work.

#### `AdminAuditRecord`

```rust
struct AdminAuditRecord {
    audit_id: AuditId,
    actor: AdminActorRef, // Account(AccountId) or DeveloperCli
    operation_name: String,
    target: TargetRef,
    outcome: AuditOutcome,
    occurred_at: Timestamp,
    expires_at: Timestamp,
}

enum AdminActorRef {
    Account(AccountId),
    DeveloperCli,
}

enum AuditOutcome {
    Succeeded,
    Rejected,
    Failed,
}
```

DO-088 approves owner-local account/game AdminAuditRecord fields and Succeeded/Rejected/Failed outcomes. DO-089 revises `actor_account_id` to `actor: AdminActorRef::{Account(AccountId), DeveloperCli}` and durably records authenticated privileged-action success/rejection/failure; CLI identifies the privileged path, not the individual human. No anonymous abuse attempts or secrets. DO-090 approves 90-day retention from `occurred_at`, expiry denial and copy deletion/restore protection; no client audit API/UI, privileged developer-CLI read only. Never retain audit as hidden History.

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

**Approved — DO-091–095:** Encode the shown attachment as compact version-1 JSON capped at 512 bytes; malformed/oversized/unsupported/inconsistent metadata closes fail-closed. Treat it only as a hint and reconstruct authority/presence from durable records on wake and before protected sends. For participant sessions, prepare/register a new authorized connection before atomically superseding the previous one; stale closes cannot change replacement presence. Each Object keeps one durable alarm for its earliest due deadline and re-reads/reschedules due work. Persist before WSS send; cap frame at 256 KiB and queued bytes at 1 MiB/socket, then close and require a fresh authorized snapshot; chunk any legal oversized snapshot only under a separately reviewed wire contract. Exact provider/Rust SDK behavior is unverified.

<a id="outstanding-decisions"></a>

### 6.9 Outstanding decisions — explicit TBD index

The field sketches provide logical record boundaries; all 107 ledger decisions are approved. Remaining TBD notes are implementation/measurement gates, not unreviewed decision proposals.

| Area | Approved decision summary / remaining implementation gate | Register |
| --- | --- | --- |
| Ownership / cardinality | **Approved — DO-001/002/100:** one Accounts and Directory owner per isolated environment, one GameObject per stable game_id, History in owning GameObject, private bindings GAME_DIRECTORY/ACCOUNTS/GAMES. DO-103 approves a measurement gate with 2× quota headroom; actual capacity remains unmeasured. | LLD-003, LLD-012, LLD-015, DO-001/002/100/103 |
| Types / identity / schema | **Approved — DO-003–009, 096–099:** UUIDv7 typed IDs, epoch-ms timestamps, revisions, canonical SQLite mappings/normalized owner tables, local FKs only, approved unique/check/parameterized-query policy, versioned forward-only fail-closed migrations. Exact DDL and query plans remain implementation work. | LLD-004, LLD-020, DO-096–099 |
| Directory coordination | **Approved — DO-013–018/039/074/082–084:** authoritative monotonic projections, one-slot reservation/gates/transfer fencing, typed owner-local operations, per-operation revisions/acks, bounded exponential retry, compare-by-game-ID History purge/code reuse. Physical transport/cadence remain implementation work. | LLD-005 |
| Credential lifecycle | Password minimum/ASCII policy and salted Argon2/library selection confirmed in [LLD Sections 9.1–9.2](lld.md#password-policy); DO-019–034 approve account/credential fields and transitions; DO-076–080 approve actor-scoped secret-safe receipts and retry policy; DO-085–090 approve abuse/audit records and policies; DO-105/106/107 settle self-removal, last-admin and no-role-editing scope. Production KDF costs/runtime and physical integration remain unverified. | LLD-009/023/024 |
| Player recovery | **Approved — DO-049–052/085–087:** versioned verifier, normalization, transaction/fencing and failure-only rate controls with scoped HMAC keys and bounded retention. Production KDF/Unicode/target details remain unverified. | LLD-010 |
| Immediate revocation | **Approved — DO-035–037, 082–084, 091–095:** authoritative registration/per-frame checks; durable close/ACK; expiry cleanup; typed owner-local work, fences/retries; attachment reconstruction, participant supersession, durable alarms and bounded close/resync. Physical delivery/scheduler integration remains implementation work. | LLD-005, LLD-007, LLD-009 |
| Game, board, call and History rules | **Approved — DO-038–075:** Game/configuration, board generation/qualification, calls/revisions, immutable History, expiry, terminal cleanup, account/session/receipt/rate-limit/audit/socket/outbox and retention rules are approved as detailed above. Physical schema/queries, runtime delivery and provider integration remain implementation work. | LLD-004, LLD-008, LLD-011/012, DO-038–075 |
| Attendance / final-view access | **Approved — DO-047/048/053–058/091–095:** role/leave/grace/session/final-view policies, hibernation-authoritative reconstruction, participant supersession, durable alarms and WSS close/resync bounds. Exact SDK handlers remain unverified. | LLD-007, LLD-011 |
| Synchronization | **Approved — DO-067/068/091–095:** per-view revision boundaries and snapshot ordering; compact versioned attachments; authoritative hibernation reconstruction; participant connection fencing; earliest-deadline alarms; bounded backpressure/close-resync. Physical delivery remains implementation work. | LLD-007 |
| Retry / pending work | **Approved — DO-076–084:** owner/actor-scoped receipts; canonical fingerprinting; bounded typed outcomes; 24h standard/30d link receipt; secret-safe retry; typed owner-local pending work; state-machine fences/ACKs; capped exponential retry and reconciliation, no silent abandon. Physical schemas/transport remain implementation work. | LLD-005 |
| History / deletion | **Approved — DO-069–075:** immutable snapshot, child rows/order, UTC expiry, atomic materialization, spectator cleanup, primary purge/code reuse, secondary-copy and restore expiry. Physical alarms/provider retention remain implementation validation. | LLD-011, LLD-012 |
| Abuse / audit | **Approved — DO-085–090:** bucket fields/owners; 5/15/15 failure throttles; scoped HMAC subject keys/aggregate controls; owner-local audit with CLI actor; 90-day retention and CLI-only read. Key rotation/physical mechanisms remain implementation work. | LLD-009, LLD-010, LLD-015 |
| Bindings / deployment / verification | **Approved — DO-096–104:** normalized owner schema/type/privacy mapping; same-Object FKs; unique/check/parameterized SQL policy; forward-only migrations; private bindings/internal calls; pinned-toolchain policy and fail-closed clock/RNG; 2× quota capacity gate; pre-deployment verification matrix. Measurements, exact pins, provider integration and test execution remain pending; no tests are claimed run. | LLD-004, LLD-013–015 |
| Open product interactions | **Approved — DO-105–107:** app admins cannot self-disable/delete; protect the last enabled Verified admin; no first-release role editing. Account creation/enable rules remain HLD-075/078. | LLD Section 12.2 |


<a id="item-review"></a>

## 7. Item-by-item review

**Review scope:** The DO-001–107 decision ledger is complete; all 107 items are approved. This section records decisions, source reconciliations and remaining implementation-only validation, not authorization to implement or proof that tests/capacity checks ran.

**Pause rule — user direction:** If a `clarify` question times out, is cancelled, skipped or otherwise unanswered, do not advance or infer any disposition. Leave the current item pending, stop the walkthrough for that turn and wait for the user to return. On resumption, present the same unresolved item rather than moving to the next one.

**Recording:** Approval outcomes are recorded in the ledger and reconciled in the Durable Object design plus affected HLD/LLD/API summaries. Untested target/runtime/quality claims remain unverified after design approval.

**Review cursor:** **All 107 Durable Object decisions approved; no pending items remain.** DO-021/045 case-sensitive username/alias decisions are approved. DO-073–075 approve terminal spectator cleanup, primary purge/code reuse and secondary-copy/restore enforcement. DO-076–090 approve receipts, retries, typed pending work, rate limits and audit policies. DO-091–095 approve attachments, hibernation, supersession, durable alarms and WSS bounds. DO-096–104 approve schema, constraints, migrations, namespaces, internal interfaces, runtime pinning, capacity and verification plans. DO-105–107 approve self-removal, last-admin and no-role-editing policies. Runtime KDF/Unicode/RNG, provider-specific mechanisms, exact SQL/API wire details and actual test/capacity outcomes remain unverified implementation work.

**Companion reconciliation:** DO-073–075 History notice/purge/copy/restore rules are summarized in HLD, LLD and API design. DO-076–090 receipt, abuse-control and audit decisions are reflected in operational schema/API trust boundaries. DO-091–095 attachment, hibernation, connection fencing, alarm and WSS bounds are reflected in HLD/LLD/API. DO-096–104 physical schema, interface, migration, capacity and test gates are summarized in LLD/API; exact SQL/runtime execution remains unverified. DO-105–107 privileged account edge cases are summarized in HLD/LLD.

**Companion reconciliation — DO-002/069–075:** History remains in the original GameObject with a minimal Directory index. DO-069/070 approve snapshot content/order; DO-071 expiry; DO-072 atomic materialization/source cleanup; DO-073 best-effort spectator notice then data deletion; DO-074 primary purge/code reuse; DO-075 copy/index/log/restore expiry. HLD/LLD/API summaries reflect these decisions; physical mechanisms remain implementation work.

**Companion reconciliation — DO-005:** The UUID-storage-format TBD wording in LLD Section 3.3 predates DO-005 and is superseded here for SQLite encoding only. API wire encoding, constructors and target wiring remain open; companion documents are unchanged.

**Cross-cutting revision — HLD-077 / DO-014–DO-018:** The user confirmed one nonterminal game at a time, excluding Resolved/Cancelled. DO-014–016 define its nullable-ID reservation and compare-by-ID lifecycle. DO-017 approves a presence-only account-assignment gate keyed by account ID; DO-018 approves acquiring it before the hosted-game check and holding it through disable/delete, with clear-and-reject when hosted games exist and retain-and-retry on interruption.



**Cross-cutting scope clarification — HLD-078 / DO-022:** Account-enable functionality and an API are required for disabled host/admin accounts. Only another fully enrolled admin account with valid authority, or the developer CLI, may execute it; the target account cannot enable itself. Ordinary hosts, players, spectators and unauthenticated callers are excluded. Enabling is distinct from password reset, does not grant a new role or recreate a deleted account, and must not revive revoked/expired credentials. **Approved enable outcome:** restore the pre-disable lifecycle state: Verified permits a fresh login with the existing password; PendingEnrollment still requires setup; ResetRequired still requires reset. Previously revoked/expired sessions and links stay invalid. Derive account disablement solely from populated `disabled_at`, preserving lifecycle status through disable/enable without a previous-status copy or redundant boolean. Populate the timestamp to disable and clear it to enable; authorized no-op retries do not change it. Fresh setup/reset links after enable use the existing flows. DO-022 also approves credential-epoch issuance, increment and match rules recorded in the AccountRecord subsection; DO-036 separately approves durable cross-Object socket-revocation work/acknowledgement and fail-closed frame checks, while physical transaction/outbox integration remains open. No implementation is authorized. The API proposal is B9/API-33; exact method/path/body details remain proposals.

**Cross-cutting timestamp revision during DO-022:** The user superseded the earlier stored-boolean choice: use `disabled_at` population for account disablement and do not persist equivalent booleans for any stored state already represented by a corresponding timestamp. Account enable/disable authority, preserved lifecycle/verifier and no-op retry semantics remain as recorded above. The user subsequently approved the credential-epoch rules, completing DO-022; DO-036 separately approves cross-Object socket-revocation policy, while transactional/outbox implementation details remain TBD.

**Cross-cutting account-status rename during DO-025:** Rename the account lifecycle status variant to `AccountStatus::Verified`; the lifecycle meaning is unchanged. Successful first setup/reset yields `Verified`, and an enabled `Verified` account may authenticate with its verifier; `disabled_at` independently controls disablement. Reconcile API/HLD/LLD and all lifecycle references; do not change unrelated uses of “active” for games or other state.

### 7.1 Approved decisions

- **DO-001 — Approved:** Use one separate SQLite-backed `AccountsObject` per application environment for all host/admin accounts, usernames, password verifiers, enrollment/reset links and account sessions. Keep anonymous player/spectator sessions in their owning `GameObject`. This approval covers placement/cardinality only, not record fields, Argon2 parameters, binding names, capacity or revocation/coordination mechanisms. No implementation/deployment is authorized.

- **DO-002 — Approved:** Store each started game's final History in its original GameObject; Directory holds only minimal searchable index/code/expiry metadata, not player boards or full snapshots. No new HistoryObject/datastore. Placement only. DO-069 approves snapshot fields/invariants, DO-070 approves logical child-row mapping/order, and DO-071 expiry; DO-072 approves atomic materialization/obsolete-live cleanup; DO-073–075 decide notice/deletion order, purge/code reuse, secondary copies/indexes and restore. No-History pre-start cancellation is unchanged.

- **DO-003 — Approved:** Timestamp is i64 / SQLite INTEGER containing UTC Unix epoch milliseconds since 1970-01-01T00:00:00Z. The trusted backend record owner supplies action/creation times and durable deadlines; browser time and UUID v7 time are not authoritative. Explicit sequences/revisions govern ordering. History expiry’s UTC calendar-month arithmetic is approved in DO-071; clock rollback/skew handling and target integration remain TBD.

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

- **DO-062 — Approved:** After feasibility passes, generate for retained start-time players in stable `player_id` order using unbiased partial Fisher–Yates over implicit `1..=numeric_upper_bound`, CSPRNG bounded sampling and row-major non-free-cell mapping. Enforce DO-061 exact uniqueness. Cap the whole Start attempt at 1,024 candidates, including duplicates. CSPRNG failure or exhaustion fails closed: no weaker fallback, duplicates, partial assignment or InProgress transition; preserve AwaitingPlayers, memberships and reservation. This is a product guardrail, not a performance claim; retry mechanics and target integration remain separate.

- **DO-063 — Approved:** Single Line uses its own `SingleLinePattern` Rust trait with `evaluate(side_length: u8, cells: &[BoardCell]) -> Result<Vec<CompletedLine>, PatternEvaluationError>`. Evaluation is pure/provider-independent and validates board shape. Row/column indices are one-based; both diagonals span the full board. Return every completed line deterministically (rows ascending, columns ascending, main diagonal, anti-diagonal); nonempty means qualified. Re-evaluate at assignment and after every accepted call for every retained player, including departed/offline members. Qualification never awards/resolves; future patterns get their own traits. No implementation/dependency is authorized.

- **DO-067 — Approved:** Preserve `ViewRevisionRecord { view: ViewKey, revision: Revision }` and `ViewKey::{Host, Player(PlayerId), Spectator(SpectatorId)}`. Host is the shared authorized host/admin projection, not authority; Player is that stable member’s private projection; Spectator is that spectator’s public projection. Account/session grants are separate. Private changes for one player do not advance another player’s revision; counters track visible projections, not internal mutations. DO-068 approves initialization at 0, advancement per changed authorized projection, snapshot/subscription ordering, gap recovery, independent authority revalidation and terminal counter lifetime.

- **DO-069 — Approved:** Preserve the shown `GameHistorySnapshot`, `HistoryOutcome`, `WinnerSnapshot` and `HistoryPlayerSnapshot` fields/types. Snapshot started terminal games only; winner iff Resolved and matches a player/alias in the snapshot; Cancelled has none. Calls are immutable and ordered; each participating player appears once with final board cells. Exclude credentials, spectators, recovery/session/presence, editable configuration and intermediate revisions. Preserve the DO-071 expiry. DO-070 approves child-row mapping/order; DO-072 materialization/cleanup remain separate.

### 7.2 Review queue

| ID | Area | Decision to review | Source | Status | Recorded decision |
| --- | --- | --- | --- | --- | --- |
| DO-001 | Ownership | Separate singleton AccountsObject placement/cardinality | [Detail](#object-boundaries) | Approved | One separate SQLite-backed AccountsObject per environment for host/admin accounts and credentials; participant sessions stay Game-owned. Placement/cardinality only; interfaces/protocols/capacity remain open. |
| DO-002 | Ownership | Final History placement in the original GameObject | [Detail](#object-boundaries) | Approved | Store each started game's final History in its original GameObject; Directory holds only minimal searchable index/code/expiry metadata, not player boards or full snapshots. No new HistoryObject/datastore. Placement only. DO-069 approves snapshot fields/invariants, DO-070 approves logical child-row mapping/order, and DO-071 approves expiry; DO-072 approves atomic materialization/obsolete-live cleanup; DO-073–075 decide notice/deletion order, purge/code reuse, secondary copies/indexes and restore. No-History pre-start cancellation is unchanged. |
| DO-003 | Shared representations | Timestamp representation and trusted clock policy | [Detail](#shared-types) | Approved | Timestamp is i64 / SQLite INTEGER containing UTC Unix epoch milliseconds since 1970-01-01T00:00:00Z. The trusted backend record owner supplies action/creation times and durable deadlines; browser time and UUID v7 time are not authoritative. Explicit sequences/revisions govern ordering. History expiry’s UTC calendar-month arithmetic is approved in DO-071; clock rollback/skew handling and target integration remain TBD. |
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
| DO-044 | Game and configuration | Configuration validation and start-time feasibility algorithm | [Detail](#game-records) | Approved | Validate full config on create/update and Start. For `N = numeric_upper_bound`, `k = side² − free_cells`, require `N ≥ k`; count possible cell-distinct layouts as `P(N,k)` (P(N,0)=1), saturating at retained start-time player membership count to avoid overflow. Reject Start if count is less than all retained memberships, connected or not; the independent two-connected-player gate also applies. Invalid patches apply nothing. DO-062 approves bounded CSPRNG sampling/failure; target RNG integration and physical Start transaction remain implementation work. |
| DO-045 | Players, spectators and access | PlayerRecord fields/types and case-sensitive alias uniqueness | [Detail](#game-records) | Approved (revised) | No stored comparison key; store the accepted outer-trimmed display alias only. Enforce exact case-sensitive uniqueness among retained PlayerRecords per game and exact current-alias spelling for recovery; `Alice` and `alice` are distinct. Cross-game reuse allowed. Preserve `player_id`, `joined_at`, `session_epoch`, nullable `last_explicit_leave_at`; no `is_connected` Boolean. DO-046 approves stable-player-ID session binding; DO-055 approves full session fields/access. Alias character/space rules remain HLD-050/051. |
| DO-046 | Players, spectators and access | Atomic alias rename and participant-session binding updates | [Detail](#game-records) | Approved | In Awaiting Players, valid player session may rename own membership to valid unclaimed exact-case alias. Atomically change PlayerRecord alias/release old while preserving player_id, seat, join time, role/capacity, answer verifier. Fail invalid/claimed/late rename without mutation. Session binds stable player_id, not alias; do not rewrite/rotate/revoke credentials or renew expiry. Current alias is resolved from PlayerRecord. DO-055 approves full schema/access; response/retry and physical SQL/index remain implementation work. |
| DO-047 | Players, spectators and access | Atomic role switching, leave and session updates | [Detail](#game-records) | Approved | Awaiting Players role changes atomically obey target capacity. Player→spectator releases alias/seat, deletes recovery record and retires old player session/sockets; spectator→player creates fresh PlayerRecord/PlayerId, eligible alias/seat and optional fresh answer, never restoring old state. Replace role-bound session without extending fixed absolute expiry. Rejection preserves old role/seat/session/proof. Pre-start player Leave removes membership/recovery/session and releases alias/seat; return is fresh admission. In Progress player Leave retains membership/board/award eligibility and a still-valid session can return until expiry. Spectator Leave immediately releases seat/revokes session; later entry is fresh. Serialize against start/admission. DO-048 separately approves retaining the latest explicit Leave timestamp after return; DO-055 approves full ParticipantSessionRecord/session-access rules. DO-056 terminal guards and physical transaction/fencing integration remain separate implementation work. |
| DO-048 | Players, spectators and access | Player leave-timestamp meaning after return | [Detail](#game-records) | Approved | Set `last_explicit_leave_at` on each explicit In Progress player Leave and keep it as the most recent Leave-event timestamp after return; do not clear on valid-session or answer-based return. It is historical event data, not current-presence state. Pre-start Leave deletes the membership/record. |
| DO-049 | Players, spectators and access | PlayerRecoveryRecord fields/types and version metadata | [Detail](#game-records) | Approved | Preserve `player_id: PlayerId`, `answer_verifier: EncodedVerifier`, `normalization_version: u32`, `updated_at: Timestamp`. Absence disables answer recovery; no plaintext or public/History exposure. Delete on accepted player→spectator switch, membership removal or terminal transition; replacement atomic. DO-050 normalization/version behavior and DO-051 Argon2id/PHC verifier profile are approved; DO-052 approves recovery transaction/session/socket-fencing policy. |
| DO-050 | Players, spectators and access | Recovery-answer normalization and version migration | [Detail](#game-records) | Approved | Version 1: Unicode NFC, trim outer Unicode whitespace, then Unicode case-fold; preserve internal whitespace/punctuation. Select verifier comparison by stored normalization_version. Future profile changes cannot reinterpret old verifier silently; retain old profile until authenticated answer replacement writes current version. No plaintext storage or silent verifier migration. Unicode data/library pin and implementation remain TBD. |
| DO-051 | Players, spectators and access | Recovery-answer verifier algorithm/format/work factors | [Detail](#game-records) | Approved | Argon2id v19; fresh independent 16-byte CSPRNG salt; 32-byte output; full PHC-encoded `EncodedVerifier`. Explicit bounded memory/time/parallelism costs; reject malformed/unsupported/out-of-bounds parameters before expensive work. 19 MiB / 2 iterations / parallelism 1 is only a benchmark starting point. Production costs/caps require target-runtime/resource measurements; DO-052 approves transaction/fencing policy while rate-limit thresholds and physical integration remain open. |
| DO-052 | Players, spectators and access | Recovery and answer-replacement transaction/session-socket fencing | [Detail](#game-records) | Approved | Verify Argon2 outside the write transaction; commit only after rechecking lifecycle, stable membership/current alias and unchanged verifier/version. Atomically advance `session_epoch`, invalidate predecessor sessions and issue a fresh fixed one-day session; old sockets fail per-frame authorization and asynchronous close completes only on acknowledgement. Answer set/replace/delete is atomic and does not rotate/extend the caller session. Stale proof fails closed; committed-but-lost recovery requires fresh proof, never secret replay. Rate-limit thresholds and physical integration remain open. |
| DO-053 | Players, spectators and access | SpectatorRecord fields/types and grace-deadline constraints | [Detail](#game-records) | Approved | Preserve the shown logical fields/types. While connected, both disconnect timestamps are absent; during retained grace, both are present and the deadline is exactly `disconnected_at + 5 minutes`. The record occupies capacity through grace and is deleted on explicit Leave, expiry or terminal cleanup. DO-054 approves current session/epoch/connection guards; DO-055 approves full session binding/access. |
| DO-054 | Players, spectators and access | Spectator disconnect/reconnect/grace races and stale-socket fencing | [Detail](#game-records) | Approved | Serialize with trusted commit-time. A disconnect mutates only the current spectator/session/epoch/connection; stale close is a no-op. First valid disconnect sets the paired five-minute timestamps; duplicate events do not restart grace. A valid reconnect before deadline clears them atomically without extending the session. `now >= grace_expires_at` expires first; the timer re-reads and deletes only if still disconnected, otherwise no-ops. Expired sessions cannot restore the old seat; re-entry is fresh admission. Stale sockets fail authorization and closure is tracked separately. Hibernation reconstruction and payload/scheduler/close integration remain open; DO-056 terminal rules are approved. |
| DO-055 | Players, spectators and access | ParticipantSessionRecord fields/types and live/final access constraints | [Detail](#game-records) | Approved | Preserve `session_id`, `token_verifier`, `game_id`, `game_code`, stable participant binding, `session_epoch`, `access`, `issued_at`, `expires_at`, and nullable `revoked_at`, with `Player { player_id }` / `Spectator { spectator_id }` bindings and `Live` / `FinalViewOnly` access. Never store raw tokens or redundant expiry/revocation booleans. Enforce current epoch, no revocation, and fixed one-day expiry; only eligible existing not-exited players retain FinalViewOnly until Exit/original expiry, while spectator sessions are deleted at terminal. No terminal admission/recovery; one live connection per participant session and stale close cannot alter seat/authority. Role switch preserves original expiry; DO-052 recovery issues the approved fresh one-day session. DO-056 terminal representation/Exit policy is approved; physical wire/storage integration remains open. |
| DO-056 | Players, spectators and access | Minimal terminal player-session record and Exit/replay/deletion guards | [Detail](#game-records) | Approved | At started-game terminal commit, delete recovery verifiers and spectator identity/session data; replace each eligible unrevoked, unexpired, current-epoch, not-exited player session with `TerminalPlayerViewRecord { session_id, token_verifier, game_id, player_id, expires_at }`, preserving original expiry. It grants only that player’s final read-only view; no new/renewed session. Expiry denies even if cleanup is delayed. Exit deletes all terminal grants for that player and closes their connections; retries cannot recreate access. Terminal requests cannot re-admit/recover; pre-start cancellation still deletes all game/participant data. Physical close/retry integration remains open. |
| DO-057 | Players, spectators and access | AccountGameViewAccess fields/types and eligible-viewer lifecycle | [Detail](#game-records) | Approved | Preserve `account_id`, `session_id`, `entered_at`, `expires_at`, `access: LiveView | FinalViewOnly`. Create LiveView only for an enrolled host/admin with an eligible current account session entering a nonterminal game; bind to that exact session and cap expiry at its original expiry. Grant is metadata, not authority; revalidate account/session/game permission each request. No terminal/History-derived live grant. DO-058 approves terminal downgrade, account/game-wide Exit across sessions, and expiry/History bounds. |
| DO-058 | Players, spectators and access | Account final-view downgrade, Exit scope across sessions and minimal retention | [Detail](#game-records) | Approved | At terminal commit, downgrade only pre-existing eligible host/admin LiveView grants to FinalViewOnly; create no new terminal grants. Keep each grant only until the earliest of explicit Exit, bound account-session original expiry, and History expiry, shortening but never extending DO-057 expiry. Exit deletes all grants for that account/game across sessions and closes view connections without logging out the account or removing History permission. Other sessions cannot recreate terminal-game access; delayed cleanup cannot extend authorization. Physical integration remains open. |
| DO-059 | Boards, calls and view revisions | PlayerBoardRecord, BoardCell and CompletedLine fields/types | [Detail](#game-records) | Approved | Preserve the shown logical fields/types and enum variants. Free cells are valueless and initially matched; ordinary values match only from accepted calls. Empty `qualifying_lines` means no current qualification; `evaluated_through_call = 0` is the initial pre-call evaluation. Boards exist only after Start. Cells/matches/qualification are persisted projections consistent with committed calls. DO-060 approves row mapping/projection checks; DO-061/062/063 approve exact uniqueness, bounded generation/failure and Single Line evaluation/trait contract. SQL constraints, target integration and implementation tests remain separate. |
| DO-060 | Boards, calls and view revisions | Board/cell physical row mapping and persisted-projection checks | [Detail](#game-records) | Approved | One board parent row per `(game_id, player_id)` for `side_length`, `assigned_at` and `evaluated_through_call`; cell child rows keyed by `(game_id, player_id, row, column)` for kind/value and `is_matched`; ordered child rows for `qualifying_lines`. Store vectors as child rows, not JSON. At Start and each accepted call, persist projections consistent with immutable layout, initially matched free cells and accepted-call history; advance `evaluated_through_call` consistently. DO-061–063 approve uniqueness/generation/evaluation contracts; SQL constraints and call statement sequence remain separate. |
| DO-061 | Boards, calls and view revisions | Exact full-board uniqueness enforcement | [Detail](#game-records) | Approved | Per game, compare exact `(CellPosition, BoardCellKind)` layout across all cells, including free positions; ignore match/qualification projections. A digest may accelerate but never replace exact equality. Check before Start commits and include every retained membership, connected or not. Cross-game duplicates allowed. DO-062 approves generation/collision/failure policy; physical SQL constraints remain separate. |
| DO-062 | Boards, calls and view revisions | Feasible random board generation and randomness failure handling | [Detail](#game-records) | Approved | After DO-044 feasibility, stable `player_id` order; unbiased partial Fisher–Yates over implicit `1..=numeric_upper_bound`, CSPRNG unbiased bounded sampling, map values to non-free cells row-major, exact DO-061 collision checks. Cap whole Start at 1,024 candidate boards including duplicates. RNG failure/budget exhaustion means no fallback, duplicate/partial assignments or InProgress transition; preserve AwaitingPlayers/memberships/reservation. Product guardrail, not performance claim. |
| DO-063 | Boards, calls and view revisions | Matching/qualification algorithms and pattern-specific Rust trait signatures | [Detail](#game-records) | Approved | Single Line uses `SingleLinePattern::evaluate(side_length: u8, cells: &[BoardCell]) -> Result<Vec<CompletedLine>, PatternEvaluationError>`, pure/provider-independent with board-shape validation. One-based row/column indices; both full diagonals; return every completed line deterministically (rows, columns, main, anti). Nonempty means qualified. Re-evaluate at assignment and every accepted call for all retained players, including departed/offline members; qualification never awards/resolves. Future patterns receive distinct traits; no implementation/dependency authorized. |
| DO-064 | Boards, calls and view revisions | CallRecord fields/types and keys | [Detail](#game-records) | Approved | Preserve `sequence_no: u32`, `value: String`, `mode: CallMode` (`Random`/`Manual`), `called_by_account_id: AccountId`, `called_at: Timestamp`, `command_id: CommandId`. Key by `(game_id, sequence_no)`; require unique `(game_id, value)`; latest is the highest sequence. DO-065 approves start-at-1 sequencing, checked fail-closed overflow and actor-scoped receipt association; DO-066 approves the atomic logical write set/order. Physical SQL/query details remain DO-096–098. |
| DO-065 | Boards, calls and view revisions | Call sequencing/overflow and actor-scoped receipt relationship | [Detail](#game-records) | Approved | Start at 1; increment by one only in the accepted-call commit; rejected/no-op operations and retries consume none. Checked overflow rejects without mutation (normally unreachable under the 1,000-value pool). Associate CallRecord.command_id with a secret-free GameObject receipt keyed in the game scope by authenticated Account actor + command ID; exact actor/ID/fingerprint retry returns the result, different fingerprint rejects. Logical association only; DO-076–079 govern receipt retention/admissibility. |
| DO-066 | Boards, calls and view revisions | Atomic accepted-call SQL transaction/statements | [Detail](#game-records) | Approved | One owning-GameObject SQLite transaction revalidates authority/InProgress, serializes value selection/validation and DO-065 receipt idempotency, then commits sequence, CallRecord, all board/qualification projections, affected view revisions and secret-free accepted-call receipt atomically; WSS follows commit. Reject/no-op/failure changes no sequence or board/revision state; qualification alone does not award/resolve. Exact SQL/schema, general receipt and delivery details remain separate. |
| DO-067 | Boards, calls and view revisions | ViewRevisionRecord/ViewKey fields/types and projection boundaries | [Detail](#game-records) | Approved | Preserve `ViewRevisionRecord { view: ViewKey, revision: Revision }` and `ViewKey::{Host, Player(PlayerId), Spectator(SpectatorId)}`. Host is shared authorized host/admin projection, not authority; Player is that stable member’s private projection; Spectator is that spectator’s public projection. Account/session grants are separate. A private change for one player does not advance another player’s counter; counters track visible projections, not internal mutations. Advancement/order/gap recovery is DO-068. |
| DO-068 | Boards, calls and view revisions | Authorized view-revision advancement, snapshot/subscription ordering and gap recovery | [Detail](#game-records) | Approved | Start each projection at revision 0; advance each affected projection once in the owning transaction only when visible data changes. Atomically serialize attachment registration with full authorized snapshot/revision capture; snapshot precedes later monotonically revised updates. Ignore duplicate/stale revisions; detected gaps require a fresh authorized snapshot. Revisions never grant authority; revalidate each operation/frame. Publish terminal projection/revision and retain counters only while eligible final-view access needs them; delete afterward, never copy into History. Physical delivery/outbox, hibernation and backpressure remain TBD. |
| DO-069 | History and retention | History snapshot/winner/player fields/types and immutable constraints | [Detail](#history-records) | Approved | Preserve the shown snapshot/winner/player fields/types. Started terminal games only; winner iff Resolved and matches a player/alias in the snapshot; Cancelled has none. Accepted calls remain ordered; each participating player appears once with final board cells. No credentials, spectators, recovery/session/presence, editable configuration or intermediate revisions. Preserve fixed DO-071 expiry. DO-070 approves child-row mapping/order; DO-072 approves atomic terminal materialization/obsolete-live-data cleanup; DO-073–075 remain separate. |
| DO-070 | History and retention | History child-table layout and ordered collections | [Detail](#history-records) | Approved | One parent per started terminal game with approved scalar fields and winner ID/alias together iff Resolved; calls keyed `(game_id, sequence_no)` in call order; players keyed `(game_id, player_id)` and returned in player_id order; cells keyed `(game_id, player_id, row, column)` in one-based row-major order, preserving kind/value/matched state. Child rows, not JSON vectors; no unapproved data. This logical mapping/order does not select SQL constraints or cleanup/materialization/restore. |
| DO-071 | History and retention | Three-calendar-month timezone/month-end expiry calculation | [Detail](#history-records) | Approved | Compute once from first terminal commit using UTC calendar-month addition, preserve time-of-day, clamp to the target month’s last day, and store the identical deadline in Game and Directory. Deny at `now >= expires_at` despite delayed purge; no fixed-day approximation or read/retry extension. DO-069 approves the snapshot fields/constraints; DO-072 approves atomic terminal materialization/obsolete-live cleanup; DO-074/075 cover purge/code-reuse/copy/restore mechanics. |
| DO-072 | History and retention | Terminal snapshot materialization and obsolete-live-data cleanup | [Detail](#history-records) | Approved | In the owning Game Object’s SQLite transaction, atomically commit terminal GameRecord outcome and complete DO-069/070 History parent/call/player/cell rows from the same final state with DO-071 expiry. Remove only obsolete mutable gameplay source rows fully represented by History in that transaction; preserve minimal terminal/History routing metadata and eligible pre-existing final-view grants within approved bounds. A rollback commits neither terminal transition nor snapshot; Directory updates remain separately coordinated. DO-073–075 decide notice/deletion ordering, purge/code reuse and secondary copies/indexes/restore. |
| DO-073 | History and retention | Terminal notice/revocation/deletion ordering without indefinite acknowledgement waits | [Detail](#history-records) | Approved | After terminal commit, attempt result delivery to connected spectators before deleting server-side spectator identity/session data; no required app ACK/indefinite wait; cleanup proceeds on disconnect/failure; no terminal reconnect recovery; preserve only bounded approved host/player grants. Transport/outbox remains implementation work. |
| DO-074 | History and retention | Scheduled purge and Directory/Game code-reservation reuse coordination | [Detail](#history-records) | Approved | Use DO-071 expiry; deny History and permit code reuse at `now >= expires_at`, even if purge is delayed. Active/unexpired History remains collision-protected. Durable idempotent purge removes Game History and clears Directory only if it still references the same game_id; no read/retry expiry extension. Cadence/SQL/retry implementation remains. |
| DO-075 | History and retention | History copies/indexes/log cleanup and restore-time expiry enforcement | [Detail](#history-records) | Approved | No History in logs/unbounded exports/caches. Any secondary copy retains the original expiry. Before restore exposure, discard expired copies/stale indexes; never resurrect expired History. Provider backup/log retention and physical deletion mechanisms remain validation work. |
| DO-076 | Command receipts and pending work | CommandReceipt/ActorRef fields/types and actor-scoped keys | [Detail](#operational-records) | Approved | Preserve shown CommandReceipt/ActorRef fields/types; owner-local unique scope `(owner Object, authenticated actor, command_id)`; actor is server-derived. outcome is closed typed family DO-078; fingerprint DO-077; retention DO-079; credential retries DO-080. |
| DO-077 | Command receipts and pending work | Request fingerprints and same-ID/different-request rejection | [Detail](#operational-records) | Approved | Server-compute versioned canonical semantic request fingerprint with SHA-256; exclude credentials, transport metadata and server timestamps. Same actor/ID/fingerprint retries receipt; changed fingerprint rejects without effects. Credential-bearing requests use DO-080 handling. |
| DO-078 | Command receipts and pending work | Typed secret-free command results and size bounds | [Detail](#operational-records) | Approved | Closed versioned tagged CommandOutcome family with only minimum nonsecret acknowledgement data, maximum 4 KiB UTF-8; no truncation, secrets, cookies/URLs or retained private snapshots. |
| DO-079 | Command receipts and pending work | Receipt retention and old-command admissibility/retry window | [Detail](#operational-records) | Approved | Ordinary completed receipts replay for 24h from completion; DO-029 credential-issuance receipts retain 30 days. Exact retries only during window; expired retries reject and never execute as new. |
| DO-080 | Command receipts and pending work | Credential-issuing command retry handling without stored raw secrets | [Detail](#operational-records) | Approved | Never store/replay bearer, cookie or URL. Same-command retry returns secret-free receipt only; lost link URL uses explicit DO-029 successor reissue; lost session-issuance response requires fresh auth/new command ID and new session under existing policy. |
| DO-081 | Command receipts and pending work | PendingOperation/TargetRef/CoordinationPhase fields/types | [Detail](#operational-records) | Approved | Preserve shown owner-local PendingOperation/TargetRef/CoordinationPhase fields; optional revision/fence and timestamps/attempt count; payload becomes typed DO-082 union; physical mapping remains. |
| DO-082 | Command receipts and pending work | Typed per-operation outbox payloads, owners and trust boundaries | [Detail](#operational-records) | Approved | Closed tagged per-operation payload family for Directory projection, reservation, account removal/host transfer, socket close/ack, terminal notice/cleanup and History index/purge. Owner-local durable work, typed peer routing/ack, receiver revalidation; only bounded nonsecret IDs/revisions/deadlines; no generic remote SQL or History/secrets. |
| DO-083 | Command receipts and pending work | Per-operation coordination state machines, fences and expected revisions | [Detail](#operational-records) | Approved | Owner-local intent precedes peer delivery; per-operation phases/fences/revisions; receiver validates its state and acts idempotently; completion only after required typed ACKs. No cross-Object atomicity, stale release or false completion. |
| DO-084 | Command receipts and pending work | Pending-work retry/backoff, completion and orphan reconciliation | [Detail](#operational-records) | Approved | Saturating attempt count and deterministic exponential retry from 1s to 5m cap. Never silently discard required work; retain to ACK or proven safe no-op. Unsafe orphan remains for reconciliation/alert. |
| DO-085 | Abuse controls and audit | RateLimitBucket fields/types and enforcement ownership | [Detail](#operational-records) | Approved | Approve shown RateLimitBucket fields/scopes; account login/link buckets in AccountsObject, recovery in owning GameObject. No raw subject data. |
| DO-086 | Abuse controls and audit | Rate-limit scopes, thresholds, windows and success/failure accounting | [Detail](#operational-records) | Approved | Failures only: 5 failed attempts in fixed 15-minute window block 15 minutes; success clears; blocked attempts do not extend. Applies to three approved scopes; caller/game aggregation is DO-087. |
| DO-087 | Abuse controls and audit | Rate-limit subject-key privacy, caller/game controls and bucket retention | [Detail](#operational-records) | Approved | Domain-separated HMAC-SHA-256 subject keys with server-managed key; trusted edge metadata for caller aggregation; layer account/game controls; delete after window/block, no long-term attempt history. Exact scopes/key rotation remain implementation work. |
| DO-088 | Abuse controls and audit | AdminAuditRecord fields/types, storage owner and event contents | [Detail](#operational-records) | Approved (revised) | Approved revised AdminAuditRecord actor field `AdminActorRef::{Account(AccountId), DeveloperCli}`; owner-local storage, shown target/operation/outcome/timestamps; account/game operations in their owner Objects; no separate Audit Object. |
| DO-089 | Abuse controls and audit | Durable audit capture of rejected/failed actions and developer attribution | [Detail](#operational-records) | Approved | Durably capture success, authorization/state rejection and execution failure for authenticated privileged admin operations. CLI attribution identifies privileged CLI path, not human; no anonymous abuse events or secrets. |
| DO-090 | Abuse controls and audit | Audit retention/deletion and privileged read policy | [Detail](#operational-records) | Approved | 90-day retention from occurred_at; deny expired reads and delete records/copies/indexes with restore protection. No client audit endpoint/UI; privileged developer-CLI read only. |
| DO-091 | Connections and scheduling | ConnectionAttachment/ViewerRef fields/types, encoding/version/size | [Detail](#socket-metadata) | Approved | Preserve shown attachment/viewer fields; compact version-1 JSON envelope <=512 encoded bytes; malformed, oversized, unsupported or inconsistent attachment closes fail-closed. Runtime-neutral cap, not provider-limit claim. |
| DO-092 | Connections and scheduling | Hibernation authority revalidation and presence reconstruction | [Detail](#socket-metadata) | Approved | Attachment is only lookup hint; on wake/per protected action reconstruct authority/presence from durable account/game/session/epoch/expiry/grant/current-connection/view revision state. Hibernation is not disconnect/grace. Close invalid before snapshot; uncertainty fails closed. |
| DO-093 | Connections and scheduling | Socket supersession and one-live-socket fencing | [Detail](#socket-metadata) | Approved | For a participant session, prepare/authenticate new connection before atomically setting current connection ID and superseding old; failed precommit preserves old. After commit old frames/actions fail; stale close cannot remove replacement; count session not sockets. Account subscription scope unchanged. |
| DO-094 | Connections and scheduling | Durable alarm scheduling across idle/grace/session/history deadlines | [Detail](#socket-metadata) | Approved | One durable earliest-deadline alarm per Object across persisted deadlines and pending-work next_attempt_at; bounded trusted-time processing, reread authoritative state, reschedule earliest. No memory-only timer or required fixed polling; runtime integration remains. |
| DO-095 | Connections and scheduling | Backpressure, message size bounds and delivery failure handling | [Detail](#socket-metadata) | Approved | Commit before WSS sends; max encoded frame 256 KiB and unsent queue 1 MiB/socket. On pressure/failure close rather than drop revisioned state; reconnect gets fresh authorized snapshot. Oversize legal snapshot requires reviewed chunk protocol. |
| DO-096 | Physical schema, deployment and verification | Per-record SQL/table/column mappings and privacy/read-write classifications | [Detail](#outstanding-decisions) | Approved | Three owner databases: Directory/Accounts per environment and Game per stable game_id. Approved owner-local normalized table groups, scalar columns/DO-009 child rows, UUID TEXT, timestamps/revisions INTEGER, bool 0/1, enums TEXT, digest BLOB, PHC TEXT, bounded typed JSON TEXT; attachments runtime-only; no raw secrets. |
| DO-097 | Physical schema, deployment and verification | Local foreign keys versus cross-Object reference validation | [Detail](#outstanding-decisions) | Approved | SQLite foreign keys only within same Object DB where lifecycle permits. Cross-Object references validated at trusted owner boundary/protocol; no cross-Object FK/cascade. |
| DO-098 | Physical schema, deployment and verification | Unique/check constraints and parameterized query specifications | [Detail](#outstanding-decisions) | Approved | Approved primary/unique keys for IDs, BINARY usernames/aliases, game codes, token verifiers, connection IDs, calls, cells/History child rows, receipts, pending work and rate buckets; row-local CHECKs for enums/bool/ranges/timestamps/null pairs; cross-row/cross-Object invariants in owner transactions. Exact board uniqueness uses DO-061 compare. Fixed parameterized SQL; never interpolate caller values/identifiers. |
| DO-099 | Physical schema, deployment and verification | Schema initialization/migration/backout and restore compatibility | [Detail](#outstanding-decisions) | Approved | Versioned owner-local migrations; contiguous transactional forward migrations before writes; fail closed on errors/unknown newer schema; expand/migrate/contract, no destructive down-migration. Backout only to backward-compatible worker or corrective forward migration; restore enforces DO-075. |
| DO-100 | Physical schema, deployment and verification | Durable Object namespace identities, binding names and environment isolation | [Detail](#outstanding-decisions) | Approved | Worker-only bindings `GAME_DIRECTORY`, `ACCOUNTS`, `GAMES`; one Directory/Accounts per isolated environment and one Game per stable game_id, never reusable code. Separate dev/staging/prod namespaces/storage, no browser exposure. |
| DO-101 | Physical schema, deployment and verification | Trusted internal interfaces and developer-CLI/backend integration | [Detail](#outstanding-decisions) | Approved | App ingress only through Worker HTTPS/WSS; private DO bindings; typed internal peer calls/ACKs, no direct peer DB access. Developer CLI uses restricted backend management/bootstrap interface and controlled credentials, not direct SQLite; first-admin HLD-075 rules remain. |
| DO-102 | Physical schema, deployment and verification | SDK/toolchain/dependency pins and target clock/randomness wiring | [Detail](#outstanding-decisions) | Approved | Pin stable Rust toolchain/Cargo.lock and compatible Worker SDK/build pair after compatibility validation; trusted Worker UTC clock, UUID-v7 time not authority; platform CSPRNG only, failure closed/no fallback. Exact crate pins/API validation remain a gate. |
| DO-103 | Physical schema, deployment and verification | Singleton capacity, storage sizes and read/write amplification validation plan | [Detail](#outstanding-decisions) | Approved | Preproduction benchmark max legal game plus owner-provided peak event/History forecast; measure per-owner storage, row reads/writes, requests, duration/CPU, alarms and WSS amplification. Reverify Cloudflare quotas; require <=50% applicable quotas (2x headroom), else stop for architecture/cost review. No capacity claim/results yet. |
| DO-104 | Physical schema, deployment and verification | Schema/constraint/transaction/race/security/load test acceptance criteria | [Detail](#outstanding-decisions) | Approved | Pre-deploy test gate covers migrations/failure/restore; SQL constraints/parameterization; fault-injected peer phases/acks/retries; lifecycle/expiry/transaction races; auth/privacy/HMAC/secrets/non-resurrection; hibernation/alarms/attachments/revisions/WSS bounds/close-resync; and DO-103 max-game/forecast load with 2x headroom. No tests claimed run; exact harness TBD. |
| DO-105 | Privileged product edge cases | Privileged self-disable/delete policy | [Detail](#account-records) | Approved | Admin API forbids authenticated admin self-disable/delete; no self-service UI/API. Developer CLI remains separate privileged path, subject to host-game and DO-106 guards. |
| DO-106 | Privileged product edge cases | Last-admin disable/delete protection | [Detail](#account-records) | Approved | Reject disable/delete if target is enabled Verified admin and operation would leave zero enabled Verified admins. Serialized in AccountsObject; applies API and CLI with no bypass. Disabled/PendingEnrollment/ResetRequired do not count; CLI bootstrap creation remains possible. |
| DO-107 | Privileged product edge cases | Role-editing feature scope confirmation | [Detail](#account-records) | Approved | No first-release role-editing UI/API/CLI. Role assigned at account creation; enable/reset/retry/host transfer do not change it. Future role conversion needs a separate decision and migration/safety review. |

## Sources

[1] https://docs.rs/strum/0.28.0/strum — strum - Rust
[2] https://docs.rs/strum/0.28.0/strum/additional_attributes/index.html — strum::additional_attributes - Rust
[3] https://docs.rs/strum/0.28.0/strum/derive.EnumString.html — EnumString in strum - Rust
