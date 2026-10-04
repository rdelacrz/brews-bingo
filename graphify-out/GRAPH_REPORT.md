# Graph Report - brews-bingo  (2026-10-04)

## Corpus Check
- 14 indexed source files (12 re-extracted) · ~93,143 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 647 nodes · 1078 edges · 63 communities (34 shown, 29 thin omitted)
- Extraction: 98% EXTRACTED · 1% INFERRED · 1% AMBIGUOUS · INFERRED: 9 edges (avg confidence: 0.78)
- Extractor token usage: unavailable from delegated extractors. Graphify benchmark: ~3.8× fewer tokens per query (32,350-word benchmark corpus; ~43,133 naive tokens vs. ~11,371 average query tokens).
- Source freshness: refreshed `AGENTS.md` and the current API, Durable Object, HLD and LLD planning documents; retained unchanged requirements/research sources and seven dated saved query traces. The graph is a navigation aid, not an authority—verify current planning sources for decisions.

## Community Hubs (Navigation)
- Hosting Provider Research
- Realtime Game Architecture
- Realtime Game Architecture
- Authentication and Password Security
- Authentication and Password Security
- Business Requirements and Product Scope
- Authorization and Account Management
- Game Lifecycle and Reservations
- Game Object Data Models
- Gameplay APIs and Player Workflows
- Gameplay APIs and Player Workflows
- Authorization and Account Management
- Client Synchronization and Views
- Board and Cell Data Models
- Final Results and Views
- Account Sessions and Credentials
- Commands and Revision Tracking
- Durable Object Design Decisions
- Realtime Game Architecture
- Planning Documents and Sources
- Directory and Game Indexes
- Durable Object Design Decisions
- Authorization and Account Management
- Directory and Game Indexes
- Client Synchronization and Views
- Operational Controls and Audit
- WebSocket Connections and Attachments
- Decision Review Workflow
- Graphify Usage Guidance
- Historical Graphify Traces
- Authentication and Password Security
- Durable Object Design Decisions
- Account Sessions and Credentials
- Account Lifecycle and Disablement
- Requirements and Persistence
- Game Lifecycle and Reservations
- Security and Privacy
- Host Authorization and Idle Cancellation
- Game Configuration Rules
- Hosting Provider Research
- Hosting Provider Research
- Identifier Conventions
- Retention and Reliability
- Deployment and Verification
- Account Lifecycle States
- Account Lifecycle States
- Account Lifecycle States
- Backend Components and Design Reviews
- Realtime Game Architecture
- Backend Components and Design Reviews
- Platform Architecture
- Game Lifecycle and Reservations
- Authentication and Password Security
- History and Retention
- Authorization and Account Management
- Account Sessions and Credentials

## God Nodes (most connected - your core abstractions)
1. `Brews Bingo — Durable Object Design` - 79 edges
2. `Hosting Research — September 30, 2026; recommendations only, no provider selected here` - 40 edges
3. `Business Requirements — approved scope, not implementation authorization` - 38 edges
4. `Research shortlist — compare Cloudflare stateful backend against Vercel Hobby + Neon/Supabase; no selection` - 33 edges
5. `Brews Bingo API Design` - 28 edges
6. `Low-Level Design — planning template, detailed acceptance pending` - 27 edges
7. `AWS — credible low-cost candidate, not verified forever-free complete stack` - 23 edges
8. `Game Durable Object` - 17 edges
9. `Membership, admission, aliases and recovery` - 15 edges
10. `Players, spectators and access` - 14 edges

## Surprising Connections (you probably didn't know these)
- `BR-021 approved: host can review completed games’ draw records after starting a new game` --conceptually_related_to--> `Unreconciled note: tracking does not imply past-game persistence or closed-app survival`  [AMBIGUOUS]
  graphify-out/memory/query_20261003_165132_f1875f64_what_is_the_exact_relationship_between_br_021_and.md → graphify-out/memory/query_20261003_165132_e0d2b564_what_is_the_exact_relationship_between_br_013_and.md
- `Persistence and realtime are separate decisions; free Postgres alone is not reliable cross-instance broadcast` --references--> `BR-002 [Approved — revised wording] — The application shall support either randomly selecting a bingo value or allowing the bingo caller to enter a value manually. Every entered or selected value shall be represented as a string and validated against the valid string pool for the game; values outside the pool or already drawn in that game shall be rejected.`  [EXTRACTED]
  plans/research.md → plans/requirements.md
- `Persistence and realtime are separate decisions; free Postgres alone is not reliable cross-instance broadcast` --references--> `BR-007 [Approved] — A bingo value shall not be drawn more than once within the same game.`  [EXTRACTED]
  plans/research.md → plans/requirements.md
- `Persistence and realtime are separate decisions; free Postgres alone is not reliable cross-instance broadcast` --references--> `BR-018 [Approved] — The host shall have an operating view separate from the audience's display of the game.`  [EXTRACTED]
  plans/research.md → plans/requirements.md
- `Hosting Research — September 30, 2026; recommendations only, no provider selected here` --references--> `Business Requirements — approved scope, not implementation authorization`  [EXTRACTED]
  plans/research.md → plans/requirements.md

## Hyperedges (group relationships)
- **Cross-Object game creation and recovery** — plans_api_design_create_game, plans_api_design_coordinate_global_game_reservation, plans_api_design_reconcile_pending_operation [EXTRACTED 1.00]
- **Terminal outcome retention and access split** — plans_api_design_award_game_winner, plans_api_design_cancel_unstarted_game, plans_api_design_end_game_without_winner, plans_api_design_purge_cancelled_unstarted_game, plans_api_design_finalize_game_retention, plans_api_design_purge_expired_history, plans_api_design_get_game_result, plans_api_design_exit_game_result [EXTRACTED 1.00]
- **Account Lifecycle States** — agents_account_lifecycle, agents_pendingenrollment, agents_verified, agents_resetrequired [EXTRACTED 1.00]
- **Documents Form the Planning Sources of Truth** — agents_planning_sources_of_truth, agents_plans_requirements_md, agents_plans_hld_md, agents_plans_lld_md, agents_plans_api_design_md, agents_plans_durable_object_design_md, agents_plans_research_md [EXTRACTED 1.00]
- **Item-by-item review queue (DO-029–DO-107)** — plans_durable_object_design_do_029, plans_durable_object_design_do_030, plans_durable_object_design_do_031, plans_durable_object_design_do_032, plans_durable_object_design_do_033, plans_durable_object_design_do_034, plans_durable_object_design_do_035, plans_durable_object_design_do_036, plans_durable_object_design_do_037, plans_durable_object_design_do_038, plans_durable_object_design_do_039, plans_durable_object_design_do_040, plans_durable_object_design_do_041, plans_durable_object_design_do_042, plans_durable_object_design_do_043, plans_durable_object_design_do_044, plans_durable_object_design_do_045, plans_durable_object_design_do_046, plans_durable_object_design_do_047, plans_durable_object_design_do_048, plans_durable_object_design_do_049, plans_durable_object_design_do_050, plans_durable_object_design_do_051, plans_durable_object_design_do_052, plans_durable_object_design_do_053, plans_durable_object_design_do_054, plans_durable_object_design_do_055, plans_durable_object_design_do_056, plans_durable_object_design_do_057, plans_durable_object_design_do_058, plans_durable_object_design_do_059, plans_durable_object_design_do_060, plans_durable_object_design_do_061, plans_durable_object_design_do_062, plans_durable_object_design_do_063, plans_durable_object_design_do_064, plans_durable_object_design_do_065, plans_durable_object_design_do_066, plans_durable_object_design_do_067, plans_durable_object_design_do_068, plans_durable_object_design_do_069, plans_durable_object_design_do_070, plans_durable_object_design_do_071, plans_durable_object_design_do_072, plans_durable_object_design_do_073, plans_durable_object_design_do_074, plans_durable_object_design_do_075, plans_durable_object_design_do_076, plans_durable_object_design_do_077, plans_durable_object_design_do_078, plans_durable_object_design_do_079, plans_durable_object_design_do_080, plans_durable_object_design_do_081, plans_durable_object_design_do_082, plans_durable_object_design_do_083, plans_durable_object_design_do_084, plans_durable_object_design_do_085, plans_durable_object_design_do_086, plans_durable_object_design_do_087, plans_durable_object_design_do_088, plans_durable_object_design_do_089, plans_durable_object_design_do_090, plans_durable_object_design_do_091, plans_durable_object_design_do_092, plans_durable_object_design_do_093, plans_durable_object_design_do_094, plans_durable_object_design_do_095, plans_durable_object_design_do_096, plans_durable_object_design_do_097, plans_durable_object_design_do_098, plans_durable_object_design_do_099, plans_durable_object_design_do_100, plans_durable_object_design_do_101, plans_durable_object_design_do_102, plans_durable_object_design_do_103, plans_durable_object_design_do_104, plans_durable_object_design_do_105, plans_durable_object_design_do_106, plans_durable_object_design_do_107 [EXTRACTED 1.00]
- **Research comparison only: integrated Cloudflare versus Vercel with two alternative free Postgres plans** — plans_research_cloudflare, plans_research_vercel_neon_stack, plans_research_vercel_supabase_stack [EXTRACTED 1.00]

## Communities (63 total, 29 thin omitted)

### Community 0 - "Hosting Provider Research"
Cohesion: 0.05
Nodes (84): BR-002 [Approved — revised wording] — The application shall support either randomly selecting a bingo value or allowing the bingo caller to enter a value manually. Every entered or selected value shall be represented as a string and validated against the valid string pool for the game; values outside the pool or already drawn in that game shall be rejected., BR-007 [Approved] — A bingo value shall not be drawn more than once within the same game., BR-016 [Approved] — An active game shall remain usable if the venue loses its internet connection., BR-018 [Approved] — The host shall have an operating view separate from the audience's display of the game., D-005 — Clarified: Internet may be intermittent. The app shall continue operating through temporary internet outages., AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed, API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits, AWS — credible low-cost candidate, not verified forever-free complete stack (+76 more)

### Community 1 - "Realtime Game Architecture"
Cohesion: 0.07
Nodes (51): Admin, Admin-only Users view, App layer, Audience spectator, Awaiting Players, Backend account and session authority, Brews Bingo High-Level Design, Business requirements (+43 more)

### Community 2 - "Realtime Game Architecture"
Cohesion: 0.06
Nodes (60): HLD-002 Online-only state authority and backend location, HLD-005 Persistence and retention approach, HLD-006 Communication and synchronization approach, HLD-007 Authentication, permissions and participant identity, HLD-008 Cloudflare hosting and cost boundary, HLD-010 Access-link enrollment and first-password setup, HLD-011 Host/host game-management capabilities, HLD-012 Start prerequisite and application-wide game limit (+52 more)

### Community 3 - "Authentication and Password Security"
Cohesion: 0.06
Nodes (32): API contract review worksheets, API-01–API-33 coverage index, Auth-scope notation, Authentication and sessions, Role-specific view revisions and resynchronization, Brews Bingo API Design, complete_password_reset, complete_password_setup (+24 more)

### Community 4 - "Authentication and Password Security"
Cohesion: 0.06
Nodes (33): AccountRecord complete PHC verifier field — DO-019 approved, Self-removal, last-admin protection, and account-operation race details — unresolved, API/WSS proposals are centralized in api-design.md; final contracts remain TBD, Explicit Argon2 costs, OWASP benchmark point, bounded fail-closed policy, runtime gate — DO-024 approved; production tuning pending, Conditional opportunistic rehash policy — DO-025 approved, Argon2id v19, 16-byte fresh salt, 32-byte output, SQLite TEXT — DO-023 approved, Detailed design approval and implementation authorization — pending, Low-Level Design — planning template, detailed acceptance pending (+25 more)

### Community 5 - "Business Requirements and Product Scope"
Cohesion: 0.07
Nodes (33): BR-001 [Approved] — The application shall be called Brews Bingo., BR-003 [Approved] — The application shall display the most recently drawn bingo value., BR-004 [Approved] — The application shall keep a record of the bingo values drawn during the current game., BR-005 [Approved] — The application shall let users see all bingo values drawn during the current game., BR-006 [Approved] — Each game shall draw only from a configured pool of valid string values. The current iteration shall focus on generating and tracking values stored as strings; the pool may represent any agreed bingo format or combination of bingo values., BR-008 [Approved] — The bingo host shall control when each next value is drawn., BR-009 [Approved] — The application shall clearly indicate when no undrawn values remain., BR-010 [Approved] — The host shall be able to review drawn values in the order they were called. (+25 more)

### Community 6 - "Authorization and Account Management"
Cohesion: 0.13
Nodes (11): Users and privileged account operations, Account lifecycle states, Cookie-backed sessions and credential epochs, DO-022 — credential epoch and enable, DO-027 — link verifier, DO-028 — protected access-link handoff, DO-029 — approved secret-free link-issuance receipt, DO-030 — session-row binding (+3 more)

### Community 7 - "Game Lifecycle and Reservations"
Cohesion: 0.14
Nodes (17): Backend-only coordination and maintenance, cancel_idle_unstarted_game, claim_game_code, coordinate_global_game_reservation, create_game, expire_spectator_grace, Game discovery, configuration and ownership, get_current_nonterminal_game (+9 more)

### Community 8 - "Game Object Data Models"
Cohesion: 0.23
Nodes (16): Brews Bingo — Durable Object Design, DO-045 — PlayerRecord fields/types and alias-key constraints [Pending], DO-046 — Atomic alias rename and participant-session binding updates [Pending], DO-047 — Atomic role switching, leave and session updates [Pending], DO-048 — Player leave-timestamp meaning after return [Pending], DO-049 — PlayerRecoveryRecord fields/types and version metadata [Pending], DO-050 — Recovery-answer normalization and version migration [Pending], DO-051 — Recovery-answer verifier algorithm/format/work factors [Pending] (+8 more)

### Community 9 - "Gameplay APIs and Player Workflows"
Cohesion: 0.13
Nodes (15): get_game_availability, get_my_membership, is_alias_claimed, join_game_as_player, leave_player_game, leave_player_lobby, leave_spectator_game, list_game_players (+7 more)

### Community 10 - "Gameplay APIs and Player Workflows"
Cohesion: 0.17
Nodes (12): award_game_winner, call_manual_value, cancel_unstarted_game, end_game_without_winner, finalize_game_retention, get_game_boards, get_game_qualification, get_my_board (+4 more)

### Community 11 - "Authorization and Account Management"
Cohesion: 0.24
Nodes (11): create_admin_account, create_host_account, delete_account, disable_account, find_hosted_nonterminal_games, get_user, initiate_password_reset, list_users (+3 more)

### Community 12 - "Client Synchronization and Views"
Cohesion: 0.24
Nodes (8): connect_game_stream, expire_or_revoke_credentials, GameView, get_game, get_game_calls, logout_session, synchronize_game_view, update_participant_presence

### Community 13 - "Board and Cell Data Models"
Cohesion: 0.18
Nodes (11): Boards, calls and view revisions, DO-059 — PlayerBoardRecord, BoardCell and CompletedLine fields/types [Pending], DO-060 — Board/cell physical row mapping and persisted-projection checks [Pending], DO-061 — Exact full-board uniqueness enforcement [Pending], DO-062 — Feasible random board generation and randomness failure handling [Pending], DO-063 — Matching/qualification algorithms and pattern-specific Rust trait signatures [Pending], DO-064 — CallRecord fields/types and keys [Pending], DO-065 — Call sequencing/overflow and actor-scoped receipt relationship [Pending] (+3 more)

### Community 14 - "Final Results and Views"
Cohesion: 0.22
Nodes (8): exit_game_result, Final views and History, FinalResultView, get_game_history, get_game_result, HistoryView, list_game_history, purge_expired_history

### Community 15 - "Account Sessions and Credentials"
Cohesion: 0.20
Nodes (10): Accounts and credentials, DO-029 — Enrollment reissue/predecessor cleanup and consumed-link lost-response handling [Approved]; epoch-bumping reissue, predecessor invalidation, secret-free 30-day receipts, and no secret replay, DO-030 — AccountSessionRecord fields/types and constraints [Approved]; fixed one-day absolute lifetime and timestamp-derived revocation state, DO-031 — Cookie/session format, binding and request protection [Approved]; 32-byte CSPRNG/SHA-256 verifier, __Host cookie, and exact-Origin protections, DO-032 — Restricted-to-normal session rotation transaction [Pending], DO-033 — Post-reset session issuance and navigation behavior [Pending], DO-034 — Account authority lookup/caching and expired/revoked artifact cleanup [Pending], DO-035 — AccountSocketSubscription fields/types, uniqueness and registration races [Pending] (+2 more)

### Community 16 - "Commands and Revision Tracking"
Cohesion: 0.20
Nodes (10): Command receipts and pending work, DO-076 — CommandReceipt/ActorRef fields/types and actor-scoped keys [Pending], DO-077 — Request fingerprints and same-ID/different-request rejection [Pending], DO-078 — Typed secret-free command results and size bounds [Pending], DO-079 — Receipt retention and old-command admissibility/retry window [Pending], DO-080 — Credential-issuing command retry handling without stored raw secrets [Pending], DO-081 — PendingOperation/TargetRef/CoordinationPhase fields/types [Pending], DO-082 — Typed per-operation outbox payloads, owners and trust boundaries [Pending] (+2 more)

### Community 17 - "Durable Object Design Decisions"
Cohesion: 0.20
Nodes (10): DO-096 — Per-record SQL/table/column mappings and privacy/read-write classifications [Pending], DO-097 — Local foreign keys versus cross-Object reference validation [Pending], DO-098 — Unique/check constraints and parameterized query specifications [Pending], DO-099 — Schema initialization/migration/backout and restore compatibility [Pending], DO-100 — Durable Object namespace identities, binding names and environment isolation [Pending], DO-101 — Trusted internal interfaces and developer-CLI/backend integration [Pending], DO-102 — SDK/toolchain/dependency pins and target clock/randomness wiring [Pending], DO-103 — Singleton capacity, storage sizes and read/write amplification validation plan [Pending] (+2 more)

### Community 18 - "Realtime Game Architecture"
Cohesion: 0.20
Nodes (10): Cloudflare Workers Rust language support, Dioxus, Dioxus 0.7 documentation, Dioxus bundling documentation, Dioxus deployment documentation, Dioxus latest stable release page, Rust, Rust Durable Object State documentation (+2 more)

### Community 19 - "Planning Documents and Sources"
Cohesion: 0.25
Nodes (7): Agent Guidance, plans/api-design.md, plans/durable-object-design.md, plans/hld.md, plans/lld.md, plans/requirements.md, plans/research.md

### Community 20 - "Directory and Game Indexes"
Cohesion: 0.25
Nodes (8): DO-038 — GameRecord and TerminalOutcome fields/types, defaults and constraints [Pending], DO-039 — Host-assignment/revision transactions and actor attribution [Pending], DO-040 — Host-idle renewal policy for non-designated admin activity [Pending], DO-041 — Idle cancellation versus activity/start timer races [Pending], DO-042 — GameConfiguration fields/types and disabled free-cell representation [Pending], DO-043 — Practical numeric pool/resource ceiling [Pending], DO-044 — Configuration validation and start-time feasibility algorithm [Pending], Game and configuration

### Community 21 - "Durable Object Design Decisions"
Cohesion: 0.25
Nodes (8): DO-069 — History snapshot/winner/player fields/types and immutable constraints [Pending], DO-070 — History child-table layout and ordered collections [Pending], DO-071 — Three-calendar-month timezone/month-end expiry calculation [Pending], DO-072 — Terminal snapshot materialization and obsolete-live-data cleanup [Pending], DO-073 — Terminal notice/revocation/deletion ordering without indefinite acknowledgement waits [Pending], DO-074 — Scheduled purge and Directory/Game code-reservation reuse coordination [Pending], DO-075 — History copies/indexes/log cleanup and restore-time expiry enforcement [Pending], History and retention

### Community 22 - "Authorization and Account Management"
Cohesion: 0.43
Nodes (8): HLD-009 Developer-only CLI and privileged app provisioning, HLD-064 Privileged account forgotten-password reset, HLD-065 Privileged account enrollment-link reissue, HLD-070 Account retention and safe privileged removal, HLD-073 Rename ordinary hosting account type to host, HLD-074 New admin role: account management and cross-game actions, HLD-075 Admin-account creation restricted to CLI/existing admins, HLD-076 Admin-only Users view for provisioned accounts

### Community 23 - "Directory and Game Indexes"
Cohesion: 0.29
Nodes (5): Directory account-assignment gate, DO-014 — reservation record, DO-015 — reservation compare-and-set, DO-016 — idempotent terminal reservation result, HLD-077 — global nonterminal-game slot

### Community 24 - "Client Synchronization and Views"
Cohesion: 0.38
Nodes (6): call_random_value, execute_command_idempotently, get_command_result, Live synchronization and retries, reconcile_pending_operation, send_game_update

### Community 25 - "Operational Controls and Audit"
Cohesion: 0.29
Nodes (7): Abuse controls and audit, DO-085 — RateLimitBucket fields/types and enforcement ownership [Pending], DO-086 — Rate-limit scopes, thresholds, windows and success/failure accounting [Pending], DO-087 — Rate-limit subject-key privacy, caller/game controls and bucket retention [Pending], DO-088 — AdminAuditRecord fields/types, storage owner and event contents [Pending], DO-089 — Durable audit capture of rejected/failed actions and developer attribution [Pending], DO-090 — Audit retention/deletion and privileged read policy [Pending]

### Community 26 - "WebSocket Connections and Attachments"
Cohesion: 0.33
Nodes (6): Connections and scheduling, DO-091 — ConnectionAttachment/ViewerRef fields/types, encoding/version/size [Pending], DO-092 — Hibernation authority revalidation and presence reconstruction [Pending], DO-093 — Socket supersession and one-live-socket fencing [Pending], DO-094 — Durable alarm scheduling across idle/grace/session/history deadlines [Pending], DO-095 — Backpressure, message size bounds and delivery failure handling [Pending]

### Community 27 - "Decision Review Workflow"
Cohesion: 0.40
Nodes (3): Dependent Decisions, DO-032 Is Next Pending After DO-029–DO-031 Approvals, Independent Approvals in the Same Category

### Community 29 - "Historical Graphify Traces"
Cohesion: 0.40
Nodes (5): Account-session design, reached in the cited graph through AccountsObject, AccountsObject: separate SQLite-backed singleton per environment (as described by the query snapshot), Business Requirements: approved scope is not implementation authorization, Historical Graphify snapshot: Durable Object Design — planning-only; DO-022 approved, DO-023 next, Hosting Research: recommendations only; no provider selected in the cited snapshot

### Community 30 - "Authentication and Password Security"
Cohesion: 0.50
Nodes (3): LLD-023 — password policy, LLD-024 — salted Argon2, Salted Argon2 password hashing

### Community 31 - "Durable Object Design Decisions"
Cohesion: 0.50
Nodes (4): DO-105 — Privileged self-disable/delete policy [Pending], DO-106 — Last-admin disable/delete protection [Pending], DO-107 — Role-editing feature scope confirmation [Pending], Privileged product edge cases

### Community 32 - "Account Sessions and Credentials"
Cohesion: 0.50
Nodes (4): Fragment-only link delivery and atomic redemption — DO-028 approved, Link reissue and secret-free lost-response receipts — DO-029 approved, AccountSessionRecord, bearer secret, digest, cookie profile — DO-030/DO-031 approved, Session rotation transaction — DO-032 pending

### Community 35 - "Requirements and Persistence"
Cohesion: 0.67
Nodes (3): BR-013 approved: active-game drawn-value record survives accidental page refresh or application restart, Unreconciled note: tracking does not imply past-game persistence or closed-app survival, BR-021 approved: host can review completed games’ draw records after starting a new game

### Community 37 - "Game Lifecycle and Reservations"
Cohesion: 0.67
Nodes (3): Admin action effect on host-idle timer — unresolved product question, Single global nonterminal game slot and immutable terminal lifecycle, Detailed workflow commit/recovery/test specifications remain TBD

## Ambiguous Edges - Review These
- `AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed` → `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`  [AMBIGUOUS]
  plans/research.md · relation: conceptually_related_to
- `API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits` → `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`  [AMBIGUOUS]
  plans/research.md · relation: conceptually_related_to
- `BR-013 [Approved] — An active game's drawn-value record shall survive an accidental page refresh or application restart.` → `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `BR-021 [Approved] — The host shall be able to review completed games' draw records after starting a new game.` → `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `Host-idle cancellation; admin refresh/open/resume effect is unresolved` → `Role and designated-host authorization`  [AMBIGUOUS]
  graphify-out/memory/query_20261003_165131_5dc344fd_what_is_the_exact_relationship_between_role_and_de.md · relation: conceptually_related_to
- `BR-013 approved: active-game drawn-value record survives accidental page refresh or application restart` → `Unreconciled note: tracking does not imply past-game persistence or closed-app survival`  [AMBIGUOUS]
  graphify-out/memory/query_20261003_165132_e0d2b564_what_is_the_exact_relationship_between_br_013_and.md · relation: conceptually_related_to
- `Unreconciled note: tracking does not imply past-game persistence or closed-app survival` → `BR-021 approved: host can review completed games’ draw records after starting a new game`  [AMBIGUOUS]
  graphify-out/memory/query_20261003_165132_f1875f64_what_is_the_exact_relationship_between_br_021_and.md · relation: conceptually_related_to
- `BR-029 approved via clarification: configurable board dimensions default to 5×5 and numeric range must fill the board` → `GameConfiguration is a proposed logical struct, not implemented or fully approved`  [AMBIGUOUS]
  graphify-out/memory/query_20261003_165132_1cc58b04_what_is_the_exact_relationship_between_br_029_and.md · relation: conceptually_related_to
- `API Gateway WebSockets: metered messages and connection-minutes, with two-hour connection and ten-minute idle limits` → `Unconfirmed AWS new-account API Gateway/Amplify allowances`  [AMBIGUOUS]
  graphify-out/memory/query_20261003_165132_a1184f88_what_is_the_exact_relationship_between_api_gateway.md · relation: conceptually_related_to
- `AWS Amplify Hosting: documented 12-month build/CDN allowance; applicability to new accounts unconfirmed` → `Unconfirmed AWS new-account API Gateway/Amplify allowances; credit catalog differs from older 12-month pricing language`  [AMBIGUOUS]
  graphify-out/memory/query_20261003_165132_50b53b32_what_is_the_exact_relationship_between_aws_amplify.md · relation: conceptually_related_to

## Knowledge Gaps
- **172 isolated node(s):** `[10] Introducing Netlify's Free plan`, `[2] Fair Use Guidelines - Vercel`, `[23] Firebase pricing plans - Google`, `[27] Limits · Cloudflare Pages docs`, `[30] Oracle Cloud Free Tier` (+167 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 198 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **29 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed` and `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits` and `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-013 [Approved] — An active game's drawn-value record shall survive an accidental page refresh or application restart.` and `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-021 [Approved] — The host shall be able to review completed games' draw records after starting a new game.` and `Unreconciled interpretation note: tracking does not yet imply past-game persistence or closed-app survival`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `Host-idle cancellation; admin refresh/open/resume effect is unresolved` and `Role and designated-host authorization`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-013 approved: active-game drawn-value record survives accidental page refresh or application restart` and `Unreconciled note: tracking does not imply past-game persistence or closed-app survival`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `Unreconciled note: tracking does not imply past-game persistence or closed-app survival` and `BR-021 approved: host can review completed games’ draw records after starting a new game`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._