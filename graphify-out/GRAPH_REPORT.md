# Graph Report - brews-bingo  (2026-10-03)

## Corpus Check
- 6 files · ~88,375 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 1 file(s) not represented in the graph (top: (none) 1)

## Summary
- 583 nodes · 1394 edges · 27 communities
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 35 edges (avg confidence: 0.84)
- Token cost: input/output token usage unavailable (graph labels refreshed without re-extraction)
- Source freshness: graph topology predates the latest planning edits. Four key DO-022/AccountRecord nodes were relabeled, but semantic relationships were not re-extracted; run `graphify /root/workspace/brews-bingo --update` before treating this as a current semantic index.

## Community Hubs (Navigation)
- Realtime Game Architecture
- Hosting Provider Research
- Durable Object Design Decisions
- Gameplay APIs and Player Workflows
- Game Object Data Models
- Game Lifecycle and Reservations
- Account Sessions and Credentials
- Client Synchronization and Views
- Directory and Game Indexes
- Authorization and Account Management
- Backend Components and Design Reviews
- Authentication and Password Security
- Cross-Object Coordination
- Commands and Revision Tracking
- API Contracts and Verification
- Business Requirements and Product Scope
- Board and Cell Data Models
- Operational Controls and Audit
- Game Configuration Rules
- Game History Snapshots
- Final Results and Views
- Draw Records and Sequencing
- WebSocket Connections and Attachments
- Enum Persistence and Serialization
- Gameplay Validation Rules
- Retention and Reliability
- Brand and Venue Identity

## God Nodes (most connected - your core abstractions)
1. `Durable Object Design — planning-only; DO-022 approved, DO-023 next` - 175 edges
2. `Hosting Research — September 30, 2026; recommendations only, no provider selected here` - 41 edges
3. `Business Requirements — approved scope, not implementation authorization` - 39 edges
4. `Research shortlist — compare Cloudflare stateful backend against Vercel Hobby + Neon/Supabase; no selection` - 33 edges
5. `GameObject — one per stable game_id; DO-002 approves original-object final History` - 31 edges
6. `Brews Bingo Low-Level Design Template` - 26 edges
7. `AWS — credible low-cost candidate, not verified forever-free complete stack` - 23 edges
8. `AccountRecord — DO-019 shape and DO-022 lifecycle/epoch policy approved` - 21 edges
9. `Timestamp — DO-003 approved i64 UTC epoch milliseconds, trusted backend time` - 21 edges
10. `Role and designated-host authorization` - 19 edges

## Surprising Connections (you probably didn't know these)
- `PendingOperation — proposed logical struct; not implemented` --semantically_similar_to--> `Vercel WebSockets — reported Beta on all plans; Function duration and cross-instance recovery constraints`  [INFERRED] [semantically similar]
  plans/durable-object-design.md → plans/research.md
- `ConnectionAttachment — proposed logical struct; not implemented` --semantically_similar_to--> `Durable Object WebSocket Hibernation — idle objects may sleep without disconnecting clients`  [INFERRED] [semantically similar]
  plans/durable-object-design.md → plans/research.md
- `Physical SQL/schema, bindings, migrations, transaction/race/load tests and target integration remain TBD and unexecuted` --semantically_similar_to--> `Research limits: no accounts/resources, deployment, benchmark, SLA test or legal review`  [INFERRED] [semantically similar]
  plans/durable-object-design.md → plans/research.md
- `CommandReceipt — proposed logical struct; not implemented` --semantically_similar_to--> `Offline multi-device authority/communication/claim-validation/reconnection remains separate HLD work`  [INFERRED] [semantically similar]
  plans/durable-object-design.md → plans/research.md
- `ViewRevisionRecord — proposed logical struct; not implemented` --semantically_similar_to--> `Offline multi-device authority/communication/claim-validation/reconnection remains separate HLD work`  [INFERRED] [semantically similar]
  plans/durable-object-design.md → plans/research.md

## Hyperedges (group relationships)
- **Cross-Object game creation and recovery** — plans_lld_game_directory_durable_object, plans_lld_game_durable_object, plans_api_design_create_game, plans_api_design_coordinate_global_game_reservation, plans_api_design_reconcile_pending_operation, plans_lld_directory_compare_by_id_coordination [EXTRACTED 1.00]
- **Restricted password setup and reset** — plans_api_design_redeem_enrollment_link, plans_api_design_complete_password_setup, plans_api_design_redeem_password_reset_link, plans_api_design_complete_password_reset, plans_lld_account_password_policy, plans_lld_salted_argon2_account_password_hashing, plans_lld_accountrecord_verifier_phc_string, plans_lld_fixed_credential_deadlines [EXTRACTED 1.00]
- **Terminal outcome retention and access split** — plans_api_design_award_game_winner, plans_api_design_cancel_unstarted_game, plans_api_design_end_game_without_winner, plans_api_design_purge_cancelled_unstarted_game, plans_api_design_finalize_game_retention, plans_api_design_purge_expired_history, plans_api_design_get_game_result, plans_api_design_exit_game_result, plans_lld_started_game_history, plans_lld_terminal_access_and_independent_exit [EXTRACTED 1.00]
- **Required single-owner accepted-call commit before delivery; implementation TBD** — plans_durable_object_design_gameobject, plans_durable_object_design_callrecord, plans_durable_object_design_playerboardrecord, plans_durable_object_design_viewrevisionrecord, plans_durable_object_design_commandreceipt [EXTRACTED 1.00]
- **Terminal freeze, shared History deadline and safe reservation release; detailed coordination TBD** — plans_durable_object_design_gamerecord, plans_durable_object_design_gamehistorysnapshot, plans_durable_object_design_gameindexrecord, plans_durable_object_design_globalreservation [EXTRACTED 1.00]
- **One durable call, board and qualification revision** — plans_hld_game_durable_object, plans_hld_automatic_call_matching, plans_hld_durable_player_boards, plans_hld_single_line, plans_hld_commit_before_broadcast_and_command_deduplication [EXTRACTED 1.00]
- **Private role-specific live delivery from one game owner** — plans_hld_game_durable_object, plans_hld_role_filtered_hibernating_wss, plans_hld_host, plans_hld_admin, plans_hld_player, plans_hld_audience_spectator, plans_hld_durable_player_boards [EXTRACTED 1.00]
- **Durable terminal commit before owner-safe global release** — plans_hld_game_directory_durable_object, plans_hld_game_durable_object, plans_hld_single_nonterminal_game_reservation, plans_hld_resolved, plans_hld_cancelled [EXTRACTED 1.00]
- **Research comparison only: integrated Cloudflare versus Vercel with two alternative free Postgres plans** — plans_research_cloudflare, plans_research_vercel_neon_stack, plans_research_vercel_supabase_stack [EXTRACTED 1.00]

## Communities (27 total, 0 thin omitted)

### Community 0 - "Realtime Game Architecture"
Cohesion: 0.05
Nodes (61): Admin, Admin-only Users view, App layer, Audience spectator, Awaiting Players, Backend account and session authority, Brews Bingo High-Level Design, Business requirements (+53 more)

### Community 1 - "Hosting Provider Research"
Cohesion: 0.06
Nodes (79): AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed, API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits, AWS — credible low-cost candidate, not verified forever-free complete stack, AWS Free account — reported six-month-or-credit-exhaustion lifetime, separate 12-month credit expiry, Google Cloud Run — conventional-container candidate with billable active WebSockets and 60-minute request limit, Cloudflare Workers/Pages + SQLite-backed Durable Objects — leading candidate, not selected, CloudFront flat-rate Free — reported ongoing $0 delivery on a Paid AWS account; not a backend bundle, Cloudflare D1 — optional candidate database, not required in the proposed research stack (+71 more)

### Community 2 - "Durable Object Design Decisions"
Cohesion: 0.08
Nodes (39): AccountRecord — DO-019 shape and DO-022 lifecycle/epoch policy approved, AccountRole — proposed logical enum; not implemented, AccountStatus — DO-022 lifecycle subdecision approved: PendingEnrollment, Active, ResetRequired, API Design — referenced safe projections and HTTP/WSS contracts, not private-record serialization, Salted Argon2 via RustCrypto argon2 — library choice confirmed, profile/runtime details TBD, DATA-01 — Provisioned account (logical coverage group, not selected table), DO-005 [Approved] — UUID storage encoding and boundary mapping, DO-006 [Approved] — Typed-ID constructors, creation ownership and target integration (+31 more)

### Community 3 - "Gameplay APIs and Player Workflows"
Cohesion: 0.12
Nodes (28): call_manual_value, call_random_value, expire_spectator_grace, get_game_availability, get_game_boards, get_game_qualification, get_my_board, get_remaining_values (+20 more)

### Community 4 - "Game Object Data Models"
Cohesion: 0.10
Nodes (28): AccountGameAccess — proposed logical enum; not implemented, AccountGameViewAccess — proposed logical struct; not implemented, DATA-06 — Membership / alias / occupancy (logical coverage group, not selected table), DATA-07 — Participant sessions / exit authorization (logical coverage group, not selected table), DATA-08 — Private player recovery verifier (logical coverage group, not selected table), DO-003 [Approved] — Timestamp representation and trusted clock policy, DO-045 [Pending] — PlayerRecord fields/types and alias-key constraints, DO-046 [Pending] — Atomic alias rename and participant-session binding updates (+20 more)

### Community 5 - "Game Lifecycle and Reservations"
Cohesion: 0.16
Nodes (21): award_game_winner, Backend-only coordination and maintenance, cancel_idle_unstarted_game, claim_game_code, coordinate_global_game_reservation, create_game, end_game_without_winner, finalize_game_retention (+13 more)

### Community 6 - "Account Sessions and Credentials"
Cohesion: 0.11
Nodes (22): AccessLinkPurpose — proposed logical enum; not implemented, AccessLinkRecord — proposed logical struct; not implemented, AccountSessionRecord — proposed logical struct; not implemented, AccountSessionScope — proposed logical enum; not implemented, AccountsObject — DO-001 approved separate SQLite-backed singleton per environment, AccountSocketSubscription — proposed logical struct; not implemented, DATA-02 — Access/enrollment/reset credential (logical coverage group, not selected table), DATA-03 — Account and restricted sessions (logical coverage group, not selected table) (+14 more)

### Community 7 - "Client Synchronization and Views"
Cohesion: 0.12
Nodes (18): connect_game_stream, execute_command_idempotently, GameView, get_game, get_game_calls, get_game_history, get_my_membership, HistoryView (+10 more)

### Community 8 - "Directory and Game Indexes"
Cohesion: 0.12
Nodes (21): AccountAssignmentGate — DO-017 approved account_id presence-only row, CancellationReason — proposed logical enum; not implemented, DATA-04 — Game directory and reservation (logical coverage group, not selected table), DO-011 [Approved] — GameIndexRecord fields/types and state-dependent nullability, DO-012 [Approved] — Game index keys, uniqueness, query indexes and state filters, DO-014 [Approved] — One GlobalReservation record: nullable game_id only (NULL = free; non-NULL = occupied), DO-017 [Approved] — AccountAssignmentGate fields/types; necessity, DO-038 [Pending] — GameRecord and TerminalOutcome fields/types, defaults and constraints (+13 more)

### Community 9 - "Authorization and Account Management"
Cohesion: 0.18
Nodes (17): create_admin_account, create_host_account, delete_account, disable_account, enable_account, find_hosted_nonterminal_games, get_user, initiate_password_reset (+9 more)

### Community 10 - "Backend Components and Design Reviews"
Cohesion: 0.17
Nodes (13): Durable Object namespace bindings, get_current_session, resume_game, Single API Worker, AccountsObject, Brews Bingo Low-Level Design Template, Business requirements, Cloudflare deployment and operations plan (+5 more)

### Community 11 - "Authentication and Password Security"
Cohesion: 0.21
Nodes (10): Authentication and sessions, complete_password_reset, complete_password_setup, expire_or_revoke_credentials, login_account, logout_session, redeem_enrollment_link, redeem_password_reset_link (+2 more)

### Community 12 - "Cross-Object Coordination"
Cohesion: 0.15
Nodes (12): CoordinationPhase — proposed logical enum; not implemented, DO-013 [Approved] — Projection publication, freshness and update delivery, DO-015 [Approved] — Single-reservation acquisition, lifecycle transitions, fencing and crash recovery, DO-016 [Approved] — Terminal release of the single reservation, retries and interrupted-operation reconciliation, DO-018 [Approved] — Account-removal gate acquisition, assignment/transfer races and recovery, DO-081 [Pending] — PendingOperation/TargetRef/CoordinationPhase fields/types, DO-082 [Pending] — Typed per-operation outbox payloads, owners and trust boundaries, DO-083 [Pending] — Per-operation coordination state machines, fences and expected revisions (+4 more)

### Community 13 - "Commands and Revision Tracking"
Cohesion: 0.15
Nodes (14): ActorRef — proposed logical enum; not implemented, CommandReceipt — proposed logical struct; not implemented, DATA-11 — Revisions / command outcomes (logical coverage group, not selected table), Digest — proposed digest bytes; algorithm/keying/length TBD by purpose, DO-066 [Pending] — Atomic accepted-call SQL transaction/statements, DO-067 [Pending] — ViewRevisionRecord/ViewKey fields/types and projection boundaries, DO-068 [Pending] — Authorized view-revision advancement, snapshot/subscription ordering and gap recovery, DO-076 [Pending] — CommandReceipt/ActorRef fields/types and actor-scoped keys (+6 more)

### Community 14 - "API Contracts and Verification"
Cohesion: 0.18
Nodes (10): API contract review worksheets, Auth-scope notation, get_command_result, reconcile_pending_operation, thiserror, thiserror documentation, Uuid::now_v7 documentation, uuid crate (+2 more)

### Community 15 - "Business Requirements and Product Scope"
Cohesion: 0.19
Nodes (12): BR-008 [Approved] — The bingo host shall control when each next value is drawn., BR-012 [Approved] — Clearing or replacing an active game's draw record shall require explicit confirmation to prevent accidental loss., BR-014 [Approved] — The current value and drawn-value record shall be readable by attendees on a shared venue display from typical seating positions., BR-016 [Approved] — An active game shall remain usable if the venue loses its internet connection., BR-019 [Approved] — The audience display shall show the winning pattern or objective for the current game., BR-020 [Approved] — The application shall offer an optional spoken announcement of each drawn value., BR-022 [Approved] — The application shall support access through web browsers on desktop and mobile devices., BR-023 [Approved] — The application shall support Android devices. (+4 more)

### Community 16 - "Board and Cell Data Models"
Cohesion: 0.19
Nodes (13): BoardCell — proposed logical struct; not implemented, BoardCellKind — proposed logical enum; not implemented, CellPosition — DO-008 approved one-based u8 row/column; physical layout TBD, CompletedLine — proposed logical enum; not implemented, DATA-09 — Assigned player board (logical coverage group, not selected table), DATA-10 — Ordered calls / qualification / outcome (logical coverage group, not selected table), DO-008 [Approved] — CellPosition representation and indexing validation, DO-059 [Pending] — PlayerBoardRecord, BoardCell and CompletedLine fields/types (+5 more)

### Community 17 - "Operational Controls and Audit"
Cohesion: 0.17
Nodes (11): AdminAuditRecord — proposed logical struct; not implemented, AuditOutcome — proposed logical enum; not implemented, DATA-13 — Deadlines / coordination / audit (logical coverage group, not selected table), DO-085 [Pending] — RateLimitBucket fields/types and enforcement ownership, DO-086 [Pending] — Rate-limit scopes, thresholds, windows and success/failure accounting, DO-087 [Pending] — Rate-limit subject-key privacy, caller/game controls and bucket retention, DO-088 [Pending] — AdminAuditRecord fields/types, storage owner and event contents, DO-089 [Pending] — Durable audit capture of rejected/failed actions and developer attribution (+3 more)

### Community 18 - "Game Configuration Rules"
Cohesion: 0.18
Nodes (12): DATA-05 — Game and configuration (logical coverage group, not selected table), DO-042 [Pending] — GameConfiguration fields/types and disabled free-cell representation, DO-043 [Pending] — Practical numeric pool/resource ceiling, DO-044 [Pending] — Configuration validation and start-time feasibility algorithm, GameConfiguration — proposed logical struct; not implemented, WinningPattern — proposed logical enum; not implemented, BR-006 [Approved] — Each game shall draw only from a configured pool of valid string values. The current iteration shall focus on generating and tracking values stored as strings; the pool may represent any agreed bingo format or combination of bingo values., BR-009 [Approved] — The application shall clearly indicate when no undrawn values remain. (+4 more)

### Community 19 - "Game History Snapshots"
Cohesion: 0.33
Nodes (9): DATA-12 — Final History and indexes (logical coverage group, not selected table), DO-002 [Approved] — Final History placement in the original GameObject, DO-069 [Pending] — History snapshot/winner/player fields/types and immutable constraints, DO-070 [Pending] — History child-table layout and ordered collections, DO-072 [Pending] — Terminal snapshot materialization and obsolete-live-data cleanup, GameHistorySnapshot — proposed logical struct; not implemented, HistoryOutcome — proposed logical enum; not implemented, HistoryPlayerSnapshot — proposed logical struct; not implemented (+1 more)

### Community 20 - "Final Results and Views"
Cohesion: 0.36
Nodes (7): Brews Bingo API Design, cancel_unstarted_game, exit_game_result, Final views and History, FinalResultView, get_game_result, purge_cancelled_unstarted_game

### Community 21 - "Draw Records and Sequencing"
Cohesion: 0.25
Nodes (8): CallMode — proposed logical enum; not implemented, CallRecord — proposed logical struct; not implemented, DO-064 [Pending] — CallRecord fields/types and keys, DO-065 [Pending] — Call sequencing/overflow and actor-scoped receipt relationship, BR-003 [Approved] — The application shall display the most recently drawn bingo value., BR-004 [Approved] — The application shall keep a record of the bingo values drawn during the current game., BR-005 [Approved] — The application shall let users see all bingo values drawn during the current game., BR-010 [Approved] — The host shall be able to review drawn values in the order they were called.

### Community 22 - "WebSocket Connections and Attachments"
Cohesion: 0.40
Nodes (6): ConnectionAttachment — proposed logical struct; not implemented, DO-091 [Pending] — ConnectionAttachment/ViewerRef fields/types, encoding/version/size, DO-092 [Pending] — Hibernation authority revalidation and presence reconstruction, DO-093 [Pending] — Socket supersession and one-live-socket fencing, DO-095 [Pending] — Backpressure, message size bounds and delivery failure handling, ViewerRef — proposed logical enum; not implemented

### Community 23 - "Enum Persistence and Serialization"
Cohesion: 0.33
Nodes (5): DO-007 [Approved] — Enum persistence labels and serialization/version compatibility, [1] strum - Rust, [2] strum::additional_attributes - Rust, [3] EnumString in strum - Rust, Rust strum — required generated enum parsing/formatting; dependency pin and Wasm build unverified

### Community 24 - "Gameplay Validation Rules"
Cohesion: 0.33
Nodes (5): BR-002 [Approved — revised wording] — The application shall support either randomly selecting a bingo value or allowing the bingo caller to enter a value manually. Every entered or selected value shall be represented as a string and validated against the valid string pool for the game; values outside the pool or already drawn in that game shall be rejected., BR-007 [Approved] — A bingo value shall not be drawn more than once within the same game., BR-018 [Approved] — The host shall have an operating view separate from the audience's display of the game., BR-027 [Approved via decision clarification] — The application shall validate winning claims against the active game's drawn values and configured winning pattern or objective. The detailed winning-pattern and validation rules remain to be defined., D-004 — Clarified: The app shall validate winning claims in the first release against the active game's drawn values and configured winning pattern or objective. Detailed winning-pattern and validation rules remain open for workflow planning.

### Community 25 - "Retention and Reliability"
Cohesion: 0.40
Nodes (5): BR-013 [Approved] — An active game's drawn-value record shall survive an accidental page refresh or application restart., BR-021 [Approved] — The host shall be able to review completed games' draw records after starting a new game., BR-026 [Approved via decision clarification] — The application shall support digital player cards for attendees. The detailed rules for card generation, assignment, marking, and winner handling remain to be defined., D-003 — Clarified: Digital player cards are in scope for the first release. Detailed card generation, assignment, marking, and winner-handling rules remain open for workflow planning., Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival

### Community 26 - "Brand and Venue Identity"
Cohesion: 0.50
Nodes (4): BR-001 [Approved] — The application shall be called Brews Bingo., BR-017 [Approved] — The application shall visibly identify Rockville Brews as the business hosting the bingo experience., Brews Bingo — bingo calling and visible draw tracking, Rockville Brews — business hosting the bingo experience

## Ambiguous Edges - Review These
- `AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed` → `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`  [AMBIGUOUS]
  plans/research.md · relation: conceptually_related_to
- `API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits` → `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`  [AMBIGUOUS]
  plans/research.md · relation: conceptually_related_to
- `Host-idle cancellation` → `Role and designated-host authorization`  [AMBIGUOUS]
  plans/lld.md · relation: conceptually_related_to
- `GameConfiguration — proposed logical struct; not implemented` → `BR-029 [Approved via decision clarification] — The host shall be able to configure bingo-board width and height, defaulting to 5 × 5; the application shall validate that the configured numeric value range contains enough values to fill the configured board.`  [AMBIGUOUS]
  plans/durable-object-design.md · relation: conceptually_related_to
- `BR-013 [Approved] — An active game's drawn-value record shall survive an accidental page refresh or application restart.` → `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `BR-021 [Approved] — The host shall be able to review completed games' draw records after starting a new game.` → `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to

## Knowledge Gaps
- **48 isolated node(s):** `Cloudflare Durable Object lifecycle documentation`, `Cloudflare Durable Object Namespace API`, `Cloudflare Durable Object State API`, `Cloudflare Durable Object WebSockets`, `Cloudflare SQLite-backed Durable Object Storage API` (+43 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 51 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed` and `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits` and `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `Host-idle cancellation` and `Role and designated-host authorization`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `GameConfiguration — proposed logical struct; not implemented` and `BR-029 [Approved via decision clarification] — The host shall be able to configure bingo-board width and height, defaulting to 5 × 5; the application shall validate that the configured numeric value range contains enough values to fill the configured board.`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-013 [Approved] — An active game's drawn-value record shall survive an accidental page refresh or application restart.` and `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-021 [Approved] — The host shall be able to review completed games' draw records after starting a new game.` and `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **Why does `Durable Object Design — planning-only; DO-022 approved, DO-023 next` connect `Durable Object Design Decisions` to `Hosting Provider Research`, `Game Object Data Models`, `Account Sessions and Credentials`, `Directory and Game Indexes`, `Cross-Object Coordination`, `Commands and Revision Tracking`, `Business Requirements and Product Scope`, `Board and Cell Data Models`, `Operational Controls and Audit`, `Game Configuration Rules`, `Game History Snapshots`, `Draw Records and Sequencing`, `WebSocket Connections and Attachments`, `Enum Persistence and Serialization`?**
  _High betweenness centrality (0.248) - this node is a cross-community bridge._