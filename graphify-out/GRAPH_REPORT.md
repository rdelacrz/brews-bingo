# Graph Report - brews-bingo  (2026-10-04)

## Corpus Check
- 7 files · ~117,272 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 798 nodes · 2630 edges · 26 communities (22 shown, 4 thin omitted)
- Extraction: 99% EXTRACTED · 0% INFERRED · 1% AMBIGUOUS · INFERRED: 2 edges (avg confidence: 0.85)
- Token usage/cost: **unknown** — host semantic subagent usage is not exposed. No external model API was used; this is not a zero-cost claim.

## Community Hubs (Navigation)
- Approved Durable Ownership Policies
- UI Proposals and API Views
- Preserved Hosting Research
- Participant Sessions and Recovery
- Cross-Document Design Navigation
- Game Lifecycle and Capacity
- Requirements and Technology Baseline
- High-Level Design Navigation
- Selected Rust Routing Boundaries
- Account Login and Enrollment
- Unexecuted UI and Release Gates
- Roles and Platform Boundaries
- History and Directory Maintenance
- Board Generation and Qualification
- Argon2 Verifier and Validation
- Account Lifecycle and Enablement
- Case-Sensitive Participant Identity
- Canonical Planning Sources
- Tailwind and Accessible Components
- Privileged Link Handoff
- Authoritative Sessions and Revocation
- Offline Wording Supersession
- Privileged Account Removal Guards
- Unfinalized Contract Planning
- Single Nonterminal Game Reservation

## God Nodes (most connected - your core abstractions)
1. `DO ledger complete; logical policy approved` - 139 edges
2. `7.2 Review queue` - 110 edges
3. `13. Design decision register` - 80 edges
4. `Brews Bingo — Low-Level Design Template` - 49 edges
5. `Hosting Research — September 30, 2026; recommendations only, no provider selected here` - 46 edges
6. `Online-only authoritative gameplay` - 43 edges
7. `Confirmed domain baseline` - 41 edges
8. `Brews Bingo — API Design` - 40 edges
9. `Brews Bingo — High-Level Design` - 39 edges
10. `Brews Bingo — Durable Object Design` - 37 edges

## Surprising Connections (you probably didn't know these)
- `Purge API wording conflicts with expiry-based code reuse` --conceptually_related_to--> `DO-074 — Scheduled purge and Directory/Game code-reservation reuse coordination [Approved]`  [AMBIGUOUS]
  plans/api-design.md → plans/durable-object-design.md
- `Lifecycle-aware authorized frontend navigation` --semantically_similar_to--> `Server-verified role and lifecycle dispatch`  [INFERRED] [semantically similar]
  plans/lld.md → plans/api-design.md
- `Six canonical planning documents` --references--> `Hosting Research — September 30, 2026; recommendations only, no provider selected here`  [EXTRACTED]
  AGENTS.md → plans/research.md
- `Six canonical planning documents` --references--> `Brews Bingo — High-Level Design`  [EXTRACTED]
  AGENTS.md → plans/hld.md
- `Six canonical planning documents` --references--> `Brews Bingo — Low-Level Design Template`  [EXTRACTED]
  AGENTS.md → plans/lld.md

## Hyperedges (group relationships)
- **Research comparison only: integrated Cloudflare versus Vercel with two alternative free Postgres plans** — plans_research_cloudflare, plans_research_vercel_neon_stack, plans_research_vercel_supabase_stack [EXTRACTED 1.00]
- **Separate frontend, Worker and Object routing authority** — plans_lld_lifecycle_navigation, plans_api_design_single_api_worker, plans_durable_object_design_worker_only_ingress [INFERRED 0.85]
- **Three approved durable ownership boundaries** — plans_durable_object_design_accountsobject, plans_durable_object_design_gamedirectoryobject, plans_durable_object_design_gameobject [EXTRACTED 1.00]
- **LLD-027 selected routing libraries and native owner boundaries** — plans_lld_dioxus_router, plans_lld_axum, plans_lld_native_do_routing, plans_lld_static_assets_api_split [EXTRACTED 1.00]

## Communities (26 total, 4 thin omitted)

### Community 0 - "Approved Durable Ownership Policies"
Cohesion: 0.05
Nodes (100): API-11 — Create New game, cancel_idle_unstarted_game, coordinate_global_game_reservation, create_game, purge_expired_history, record_admin_action, AccountsObject, ActorRef (+92 more)

### Community 1 - "UI Proposals and API Views"
Cohesion: 0.06
Nodes (95): API-04 — Session validation / logout, API-07 — Reissue enrollment link, API-08 — Initiate account password reset, API-10 — List / inspect games, API-12 — Read / update configuration, API-14 — Open / resume selected game, API-15 — Start game, API-16 — Random / manual value call (+87 more)

### Community 2 - "Preserved Hosting Research"
Cohesion: 0.06
Nodes (79): AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed, API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits, AWS — credible low-cost candidate, not verified forever-free complete stack, AWS Free account — reported six-month-or-credit-exhaustion lifetime, separate 12-month credit expiry, Google Cloud Run — conventional-container candidate with billable active WebSockets and 60-minute request limit, Cloudflare Workers/Pages + SQLite-backed Durable Objects — leading candidate, not selected, CloudFront flat-rate Free — reported ongoing $0 delivery on a Paid AWS account; not a backend bundle, Cloudflare D1 — optional candidate database, not required in the proposed research stack (+71 more)

### Community 3 - "Participant Sessions and Recovery"
Cohesion: 0.08
Nodes (51): API-23 — Join as spectator, API-29 — Final-view Exit / access cleanup, exit_game_result, expire_spectator_grace, get_game_result, update_participant_presence, AccountGameAccess, AccountGameViewAccess (+43 more)

### Community 4 - "Cross-Document Design Navigation"
Cohesion: 0.09
Nodes (3): Brews Bingo — API Design, Brews Bingo — Durable Object Design, Brews Bingo — Low-Level Design Template

### Community 5 - "Game Lifecycle and Capacity"
Cohesion: 0.14
Nodes (31): HLD-001 — Backend server, app, and developer CLI layers, HLD-005 — Persistence and retention approach, HLD-011 — Host/host game-management capabilities, HLD-013 — Resume saved unstarted games, HLD-014 — Explicit confirmation before manual ending, HLD-018 — Configurable player and spectator limits with hard maxima, HLD-024 — One host-selected winner and final result, HLD-027 — Final Resolved outcome, independent exits and History-only return (+23 more)

### Community 6 - "Requirements and Technology Baseline"
Cohesion: 0.10
Nodes (28): DO-043 — Practical numeric pool/resource ceiling [Approved], HLD-003 — Rust/Dioxus frontend and web-first platform delivery, HLD-004 — Backend structure and technology choices, Rust, BR-001 — The application shall be called Brews Bingo., BR-002 — The application shall support either randomly selecting a bingo value or allowing the bingo caller to enter a value manually. Every entered or selected value shall be represented as a string and validated against the valid string pool for the game; values outside the pool or already drawn in that game shall be rejected., BR-003 — The application shall display the most recently drawn bingo value., BR-004 — The application shall keep a record of the bingo values drawn during the current game. (+20 more)

### Community 8 - "Selected Rust Routing Boundaries"
Cohesion: 0.15
Nodes (12): LLD-001 — Dioxus view grouping, routes, components, state management and accessibility, LLD-006 — Worker API methods/routes, request/response/error schemas and compatibility, LLD-016 — Initial API Worker organization and Durable Object boundary, LLD-027 — Rust routing libraries and Cloudflare dispatch boundaries, ROUTE-AC-01 — Open/refresh Home, Users and a nested game screen: static shell plus Dioxus route resolution works; screen loading itself reveals no protected data., ROUTE-AC-02 — Request /api, an unknown /api/ path and a wrong method, including navigation headers and a colliding asset path: backend dispatch/error response wins; never SPA HTML., ROUTE-AC-03 — Exercise anonymous, restricted, host, admin, player and spectator contexts against public/restricted/protected routes; direct API calls cannot bypass Origin, authority or owner commit-time rules., ROUTE-AC-04 — Verify typed parsing/domain validation, bounded body rejection, component-error redaction, preserved cookie headers and same-origin session behavior through the Worker/Axum adapter. (+4 more)

### Community 9 - "Account Login and Enrollment"
Cohesion: 0.14
Nodes (16): Agent guidance, API-02 — Enrollment-link redemption / password setup, API-03 — Reset-link redemption / new password, complete_password_reset, complete_password_setup, login_account, redeem_enrollment_link, redeem_password_reset_link (+8 more)

### Community 10 - "Unexecuted UI and Release Gates"
Cohesion: 0.10
Nodes (20): DO-103 — Singleton capacity, storage sizes and read/write amplification validation plan [Approved], DO-104 — Schema/constraint/transaction/race/security/load test acceptance criteria [Approved], LLD-014 — Optional speech, browser matrix, quality/load targets and acceptance tests, UI-AC-01 — Directly open / on desktop/mobile: branded Home has labelled game-code input, Join/Enter submission and host/admin login; no public Games/Users/History data leak. [UI proposal], UI-AC-02 — Validate code trimming/case/length/internal spaces, case-sensitive usernames and aliases, and exact password preservation. Invalid submissions do not create a game, seat or session. [UI proposal], UI-AC-03 — Test anonymous, restricted, host, admin, player and spectator route/direct-API denial; switching a client route/tab/role must not grant Users, other boards or game mutation access. [UI proposal], UI-AC-04 — Login/setup/reset reach the correct next screen: setup retains original expiry, reset requires login; used/expired links and lost secret responses do not replay credentials. [UI proposal], UI-AC-05 — Games shows at most one nonterminal card, blocks competing creation, and still displays unexpired History; incomplete coordination is not shown as a free slot. [UI proposal] (+12 more)

### Community 11 - "Roles and Platform Boundaries"
Cohesion: 0.17
Nodes (9): Dioxus, HLD-009 — Developer-only CLI and privileged app provisioning, HLD-019 — Extensible platform folder boundaries, HLD-064 — Privileged account forgotten-password reset, HLD-065 — Privileged account enrollment-link reissue, HLD-073 — Rename ordinary hosting account type to host, HLD-074 — New admin role: account management and cross-game actions, HLD-075 — Admin-account creation restricted to CLI/existing admins (+1 more)

### Community 12 - "History and Directory Maintenance"
Cohesion: 0.13
Nodes (14): API-13 — Publish Awaiting Players, API-17 — Submit one qualified winner, claim_game_code, finalize_game_retention, find_hosted_nonterminal_games, purge_cancelled_unstarted_game, reconcile_pending_operation, AccountAssignmentGate (+6 more)

### Community 13 - "Board Generation and Qualification"
Cohesion: 0.39
Nodes (12): DO-044 — Configuration validation and start-time feasibility algorithm [Approved], DO-062 — Feasible random board generation and randomness failure handling [Approved], DO-063 — Matching/qualification algorithms and pattern-specific Rust trait signatures [Approved], HLD-023 — Pattern-specific Rust traits and Single Line qualification, HLD-025 — Square board sizes and default, HLD-026 — Automatic matching and durable per-player boards, HLD-033 — Random board generation and position-sensitive uniqueness, HLD-034 — Board generation and assignment immediately after host start (+4 more)

### Community 14 - "Argon2 Verifier and Validation"
Cohesion: 0.32
Nodes (9): AccountRecord, DATA-01 — Provisioned account, DO-019 — AccountRecord fields/types and nullability [Approved], DO-023 — Remaining password-verifier profile and physical encoding constraints [Approved], DO-024 — Argon2 work factors, supported bounds and runtime validation plan [Approved], DO-025 — Password-verifier upgrades and concurrent-reset protection [Approved], LLD-009 — Password policy, token/cookie/verifier implementation and request security, LLD-023 — Account password length and character policy (+1 more)

### Community 15 - "Account Lifecycle and Enablement"
Cohesion: 0.18
Nodes (12): API-05 — Users account listing, API-33 — Enable disabled account, enable_account, get_user, list_users, SafeAccount, AccountRole, AccountStatus (+4 more)

### Community 16 - "Case-Sensitive Participant Identity"
Cohesion: 0.40
Nodes (15): DO-045 — PlayerRecord fields/types and case-sensitive alias uniqueness [Approved (revised)], HLD-007 — Authentication, permissions and participant identity, HLD-010 — Access-link enrollment and first-password setup, HLD-017 — Role choice, aliases and state-based admission, HLD-021 — Single-use access URLs and one-day role-bound sessions, HLD-022 — Optional Brews-answer player-session recovery, HLD-030 — Player alias length, character categories and reuse, HLD-031 — No alternate player recovery; replacement invalidates old session (+7 more)

### Community 19 - "Privileged Link Handoff"
Cohesion: 0.27
Nodes (11): API-06 — Provision host/admin account, create_admin_account, create_host_account, initiate_password_reset, reissue_enrollment_link, AccessLinkRecord, DATA-02 — Access/enrollment/reset credential, DO-026 — AccessLinkRecord fields/types and constraints [Approved] (+3 more)

### Community 20 - "Authoritative Sessions and Revocation"
Cohesion: 0.26
Nodes (11): expire_or_revoke_credentials, AccessLinkPurpose, AccountSessionRecord, AccountSessionScope, AccountSocketSubscription, DATA-03 — Account and restricted sessions, DO-030 — AccountSessionRecord fields/types and constraints [Approved], DO-034 — Account authority lookup/caching and expired/revoked artifact cleanup [Approved] (+3 more)

### Community 21 - "Offline Wording Supersession"
Cohesion: 0.20
Nodes (9): HLD-002 — Online-only state authority and backend location, HLD-006 — Communication and synchronization approach, HLD-008 — Cloudflare hosting and cost boundary, HLD-020 — Durable WSS recovery and online-only gameplay, BR-016 — An active game shall remain usable if the venue loses its internet connection., D-002 — Which desktop and mobile browsers, Android versions/devices, and iOS versions/devices are required for launch?, D-003 — Will attendees use physical bingo cards, or is managing player cards part of the desired app?, D-004 — Will a person verify winning claims, or is the app expected to help validate them? (+1 more)

### Community 22 - "Privileged Account Removal Guards"
Cohesion: 0.38
Nodes (7): API-09 — Disable / delete account, delete_account, disable_account, DO-105 — Privileged self-disable/delete policy [Approved], DO-106 — Last-admin disable/delete protection [Approved], DO-107 — Role-editing feature scope confirmation [Approved], HLD-070 — Account retention and safe privileged removal

### Community 23 - "Unfinalized Contract Planning"
Cohesion: 0.29
Nodes (6): API-01 — Account password login, LLD-002 — Users fields/actions/filters and protected account-management UX, LLD-008 — Board generation/feasibility, matching and pattern-specific trait signatures, LLD-021 — Rust error library and component-local ownership, LLD-022 — API design document extraction, LLD-025 — Durable Object design document extraction

### Community 24 - "Single Nonterminal Game Reservation"
Cohesion: 0.36
Nodes (7): HLD-012 — Start prerequisite and application-wide game limit, HLD-015 — Awaiting Players state and game-code joining, HLD-016 — Spectator entry before and during a game, HLD-039 — Former independent saved-unstarted and In-Progress limits, HLD-056 — Game-code alphabet and input handling, HLD-077 — One application-wide nonterminal game at a time, BR-011 — The host shall be able to run multiple games during an event sequentially. At most one game may be nonterminal at a time (New, Awaiting Players or In Progress); another game may be created only after the current game becomes Resolved or Cancelled. Terminal games do not occupy the limit.

## Ambiguous Edges - Review These
- `AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed` → `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`  [AMBIGUOUS]
  plans/research.md · relation: conceptually_related_to
- `API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits` → `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`  [AMBIGUOUS]
  plans/research.md · relation: conceptually_related_to
- `BR-013 — An active game's drawn-value record shall survive an accidental page refresh or application restart.` → `Historical tracking interpretation conflicts with approved persistence`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `BR-015 — Only the designated host shall be able to change the active game or draw values. How this is enforced remains undecided.` → `Older designated-host-only requirement qualified by admin override`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `BR-016 — An active game shall remain usable if the venue loses its internet connection.` → `Unreconciled offline requirement versus online-only direction`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `BR-021 — The host shall be able to review completed games' draw records after starting a new game.` → `Historical tracking interpretation conflicts with approved persistence`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `D-005 — What internet access can be relied on at the venue?` → `Unreconciled offline requirement versus online-only direction`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `HLD-020 — Durable WSS recovery and online-only gameplay` → `Unreconciled offline requirement versus online-only direction`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `HLD-032 — Final-board History and common three-month TTL` → `Historical tracking interpretation conflicts with approved persistence`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `HLD-074 — New admin role: account management and cross-game actions` → `Older designated-host-only requirement qualified by admin override`  [AMBIGUOUS]
  plans/requirements.md · relation: conceptually_related_to
- `LLD-027 — Rust routing libraries and Cloudflare dispatch boundaries` → `API router-TBD summary superseded by scoped LLD-027 selection`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-001 — Separate singleton AccountsObject placement/cardinality [Approved]` → `Stale AccountsObject acceptance TBD`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-001 — Separate singleton AccountsObject placement/cardinality [Approved]` → `Stale account-placement TBD versus approved ownership`  [AMBIGUOUS]
  plans/lld.md · relation: conceptually_related_to
- `DO-023 — Remaining password-verifier profile and physical encoding constraints [Approved]` → `Stale password-variant TBD`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-023 — Remaining password-verifier profile and physical encoding constraints [Approved]` → `Stale EncodedVerifier profile deferral`  [AMBIGUOUS]
  plans/durable-object-design.md · relation: conceptually_related_to
- `DO-024 — Argon2 work factors, supported bounds and runtime validation plan [Approved]` → `Stale password-variant TBD`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-025 — Password-verifier upgrades and concurrent-reset protection [Approved]` → `Historical DO-025 next cursor is stale`  [AMBIGUOUS]
  plans/durable-object-design.md · relation: conceptually_related_to
- `DO-040 — Host-idle renewal policy for non-designated admin activity [Approved]` → `Stale non-designated-admin idle effect TBD`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-041 — Idle cancellation versus activity/start timer races [Approved]` → `Stale non-designated-admin idle effect TBD`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-051 — Recovery-answer verifier algorithm/format/work factors [Approved]` → `Stale EncodedVerifier profile deferral`  [AMBIGUOUS]
  plans/durable-object-design.md · relation: conceptually_related_to
- `DO-052 — Recovery and answer-replacement transaction/session-socket fencing [Approved]` → `Stale DO-052/055 pending wording`  [AMBIGUOUS]
  plans/lld.md · relation: conceptually_related_to
- `DO-055 — ParticipantSessionRecord fields/types and live/final access constraints [Approved]` → `Stale DO-052/055 pending wording`  [AMBIGUOUS]
  plans/lld.md · relation: conceptually_related_to
- `DO-056 — Minimal terminal player-session record and Exit/replay/deletion guards [Approved]` → `Terminal spectator cleanup timing needs precedence`  [AMBIGUOUS]
  plans/durable-object-design.md · relation: conceptually_related_to
- `DO-073 — Terminal notice/revocation/deletion ordering without indefinite acknowledgement waits [Approved]` → `Terminal spectator cleanup timing needs precedence`  [AMBIGUOUS]
  plans/durable-object-design.md · relation: conceptually_related_to
- `DO-074 — Scheduled purge and Directory/Game code-reservation reuse coordination [Approved]` → `Purge API wording conflicts with expiry-based code reuse`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-100 — Durable Object namespace identities, binding names and environment isolation [Approved]` → `Stale AccountsObject acceptance TBD`  [AMBIGUOUS]
  plans/api-design.md · relation: conceptually_related_to
- `DO-100 — Durable Object namespace identities, binding names and environment isolation [Approved]` → `Stale account-placement TBD versus approved ownership`  [AMBIGUOUS]
  plans/lld.md · relation: conceptually_related_to

## Knowledge Gaps
- **49 isolated node(s):** `[10] Introducing Netlify's Free plan`, `[2] Fair Use Guidelines - Vercel`, `[23] Firebase pricing plans - Google`, `[27] Limits · Cloudflare Pages docs`, `[30] Oracle Cloud Free Tier` (+44 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 104 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **4 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `AWS Amplify Hosting — documented 12-month allowance, applicability to new accounts unconfirmed` and `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `API Gateway WebSockets — metered messages/connection-minutes; two-hour connection and ten-minute idle limits` and `Unconfirmed AWS new-account API Gateway/Amplify allowances: credit catalogs differ from older 12-month pricing language`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-013 — An active game's drawn-value record shall survive an accidental page refresh or application restart.` and `Historical tracking interpretation conflicts with approved persistence`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-015 — Only the designated host shall be able to change the active game or draw values. How this is enforced remains undecided.` and `Older designated-host-only requirement qualified by admin override`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-016 — An active game shall remain usable if the venue loses its internet connection.` and `Unreconciled offline requirement versus online-only direction`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `BR-021 — The host shall be able to review completed games' draw records after starting a new game.` and `Historical tracking interpretation conflicts with approved persistence`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **What is the exact relationship between `D-005 — What internet access can be relied on at the venue?` and `Unreconciled offline requirement versus online-only direction`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
## Canonical incremental refresh verification

- Canonical corpus: 7 files; 6 changed sources semantically re-extracted; unchanged research extraction preserved.
- Generated fragments, historical graph snapshots and saved query memories are excluded from the semantic corpus.
- Current DO status rows: **107 total, 104 Approved, 3 Approved (revised), 0 pending**. Logical approval is not implementation or test evidence.
- UI coverage: **17 VIEW IDs and UI-AC-01–UI-AC-17**, all proposals / unexecuted acceptance scenarios.
- **LLD-027 selected direction:** Dioxus Router browser navigation, Axum HTTP service inside one Worker, same-origin Worker-first `/api` and `/api/*` SPA split, and native DO state/alarms/hibernating WebSockets. Earlier API router-TBD wording is superseded only for that choice; pins/contracts remain unfinalized. ROUTE-AC-01–08 are future, unexecuted checks.
- **LLD-028 confirmed direction:** Dioxus-integrated Tailwind styling in RSX/shared components, compiled into static assets. No production browser/CDN compiler or API Worker CSS generation. Exact pins, visual tokens and build/browser validation remain open; History policy is unchanged.
- 13 explicit source-conflict/ambiguity nodes; current ledger rows outrank stale approval cursors and summary prose. No source documents were reconciled by this refresh.
- Edge-collapse caveat: 2733 raw evidence edges → 2630 undirected simple-graph edges (103 endpoint-pair collapses). Repeated/reciprocal evidence and all relations are retained in `all_evidence` / `all_relations` on collapsed edges; topology is not a multigraph.
- No dangling endpoints, missing endpoints or self-loops; seven-source coverage and hashes verified. JSON, report and embedded HTML node/edge/community counts match.
- Pre-existing research node attributes/edges remain intact except regenerated community/display metadata; source content unchanged.
- See `refresh-verification.json` for absolute paths, before/after hashes, exact counts, diagnostics and authorized mid-refresh source revisions.

### Semantic communities (all nodes)

| Community | Label | Nodes | Cohesion |
| --- | --- | ---: | ---: |
| 0 | Approved Durable Ownership Policies | 126 | 0.052825396825396824 |
| 1 | UI Proposals and API Views | 114 | 0.056202453035242975 |
| 2 | Preserved Hosting Research | 88 | 0.0577324973876698 |
| 3 | Participant Sessions and Recovery | 59 | 0.0771478667445938 |
| 4 | Cross-Document Design Navigation | 40 | 0.08974358974358974 |
| 5 | Game Lifecycle and Capacity | 34 | 0.1354723707664884 |
| 6 | Requirements and Technology Baseline | 32 | 0.1028225806451613 |
| 7 | High-Level Design Navigation | 31 | 0.06451612903225806 |
| 8 | Selected Rust Routing Boundaries | 27 | 0.150997150997151 |
| 9 | Account Login and Enrollment | 23 | 0.1422924901185771 |
| 10 | Unexecuted UI and Release Gates | 23 | 0.10276679841897234 |
| 11 | Roles and Platform Boundaries | 21 | 0.17142857142857143 |
| 12 | History and Directory Maintenance | 18 | 0.13071895424836602 |
| 13 | Board Generation and Qualification | 17 | 0.3897058823529412 |
| 14 | Argon2 Verifier and Validation | 16 | 0.31666666666666665 |
| 15 | Account Lifecycle and Enablement | 16 | 0.18333333333333332 |
| 16 | Case-Sensitive Participant Identity | 16 | 0.4 |
| 17 | Canonical Planning Sources | 15 | 0.13333333333333333 |
| 18 | Tailwind and Accessible Components | 15 | 0.13333333333333333 |
| 19 | Privileged Link Handoff | 14 | 0.27472527472527475 |
| 20 | Authoritative Sessions and Revocation | 14 | 0.26373626373626374 |
| 21 | Offline Wording Supersession | 11 | 0.2 |
| 22 | Privileged Account Removal Guards | 10 | 0.37777777777777777 |
| 23 | Unfinalized Contract Planning | 8 | 0.2857142857142857 |
| 24 | Single Nonterminal Game Reservation | 8 | 0.35714285714285715 |
| 25 | Lifecycle-Aware Frontend Navigation | 2 | 1.0 |

### Source ambiguities

- `plans/lld.md:L47` — LLD baseline still calls placement TBD; current DO-001 and DO-100 approve the singleton and bindings.
- `plans/lld.md:L688` — Historical navigation prose says pending, but both authoritative ledger status rows are Approved.
- `plans/api-design.md:L31-L40` — API boundary summary leaves placement/binding acceptance TBD despite approved DO-001/100; router and exact SDK interfaces remain TBD.
- `plans/api-design.md:L218` — API login note leaves variant TBD although DO-023 approves Argon2id v19; actual production costs/caps remain unmeasured.
- `plans/api-design.md:L904` — DO-040 settles no renewal by non-designated admin; physical race/alarm wiring remains implementation work.
- `plans/api-design.md:L2292` — H10 says deny code reuse eligibility at expiry; DO-074 explicitly permits reuse at expiry with compare-by-game-ID cleanup. Do not silently adopt the API phrase.
- `plans/durable-object-design.md:L314` — DO-025 is already Approved in the authoritative ledger, despite retained historical next-item wording.
- `plans/durable-object-design.md:L152` — Shared-type summary leaves recovery algorithm/format TBD; approved DO-023/051 settle profiles while production measurements remain open.
- `plans/durable-object-design.md:L625-L627` — DO-056 summary says deletion at terminal commit; later DO-073 requires best-effort connected delivery after terminal commit then deletion, without ACK wait.
- `plans/requirements.md:L43` — Original BR-016/D-005 offline wording persists; later HLD-020 explicitly supersedes offline gameplay.
- `plans/requirements.md:L42` — Later HLD-074 allows valid admin cross-game actions without changing designated host.
- `plans/requirements.md:L58` — Interpretation denies implied persistence/history despite explicit requirements and later final-History policies.
- `plans/api-design.md:L27` — Later LLD-027 explicitly selects Dioxus Router, Axum HTTP service and native DO boundaries. API prose remains unchanged; contracts/pins and compatibility tests remain pending.
