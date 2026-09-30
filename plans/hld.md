# Brews Bingo — High-Level Design

## 1. Document status and purpose

- **Status:** Initial template — design decisions pending review.
- **Business:** Rockville Brews.
- **Requirements source:** [Business requirements](requirements.md).
- **Confirmed architectural constraint:** The system will include both a **backend server** and an **app layer**, as explicitly requested when starting this HLD.
- **Approval boundary:** Creating this template does not approve an architecture, technology stack, or implementation work.

This document will describe the system's major components, responsibilities, interactions, and trade-offs. Detailed screen designs, database schemas, API payloads, and implementation tasks will follow after the relevant high-level decisions are approved.

**How to use this template:** Replace `TBD` prompts as sections are reviewed. Record proposals separately from approved decisions in Section 13. Do not treat a listed design topic as a selected solution or a new business requirement.

## 2. Scope and design inputs

### 2.1 Confirmed inputs

- App access through desktop/mobile web, Android, and iOS; compatibility targets the latest generally available versions at launch (BR-022–BR-024, D-002).
- Separate host and audience views, plus digital player cards and app validation of winning claims (BR-018, BR-026, BR-027).
- Host-controlled random selection or manual entry, with valid-pool and duplicate checks (BR-002, BR-006–BR-008).
- Bingo values represented as strings. First-release numeric values run from `1` through a host-defined upper bound, defaulting to `75` (BR-025, BR-028).
- Configurable board width and height, defaulting to `5 × 5`, with sufficient-pool validation (BR-029).
- Active-game recovery, completed-game draw history, and continued operation through temporary internet outages (BR-013, BR-016, BR-021, D-005).
- Both a backend server and an app layer are required. Their technology, deployment location, and division of responsibilities are not yet selected.

### 2.2 Boundaries and unresolved scope

- Player registration, payments, prizes, and event administration are not currently requested.
- Digital-card generation, assignment, marking, winning patterns, and claim-handling rules require further definition.
- Native versus cross-platform mobile delivery, app distribution, hosting, and storage technology remain open.
- Event date is not an implementation constraint.

**Source-alignment note:** The requirements document still contains older prose saying a backend is undecided and that tracking does not imply recovery/history. This HLD uses the latest backend-server instruction and the approved recovery/history requirements above; those older statements should be reconciled during document review.

## 3. System context and architecture overview

**Status:** TBD — retain the two required layers without selecting their internal structure yet.

- **Actors and interactions:** TBD — describe how the host, attendees, and shared venue display interact with the system.
- **App layer boundary:** TBD — identify which experiences share an application and which need separate views or clients.
- **Backend server boundary:** TBD — identify server responsibilities and its relationship to app-side logic.
- **System context diagram:** TBD — show actors, app clients, backend server, and any approved external dependencies.
- **Component diagram:** TBD — show major components, persistence boundaries, communication paths, and trust boundaries.
- **State authority:** TBD — define who may accept game-changing actions and where authoritative state resides, both online and during outages.

Do not assume that separate layers require separate repositories, multiple backend services, or a cloud-only deployment.

## 4. App layer

### 4.1 Experiences and platform delivery

| Area | Design to complete | Requirements |
| --- | --- | --- |
| Host experience | TBD — game setup, draw/manual-entry controls, history, new-game confirmation, and claim review. | BR-002, BR-008, BR-010–BR-012, BR-021, BR-028, BR-029 |
| Audience experience | TBD — latest value, drawn-value record, winning objective, venue readability, and branding. | BR-001, BR-003, BR-005, BR-014, BR-017–BR-019 |
| Player experience | TBD — access to a game, digital-card assignment/display, marking, and submitting a winning claim. Joining and identity rules remain undecided. | BR-026, BR-027 |
| Platform delivery | TBD — desktop/mobile web and Android/iOS delivery approach, shared versus platform-specific code, and compatibility matrix. | BR-022–BR-024, D-002 |
| Announcements | TBD — optional speech controls, playback ownership, platform support, and behavior during outages. | BR-020 |

### 4.2 App-side responsibilities

- **Presentation and interaction state:** TBD.
- **Input feedback versus authoritative validation:** TBD.
- **Local persistence and refresh/restart recovery:** TBD.
- **Receiving updates and representing stale/disconnected state:** TBD.
- **Accessibility, responsive layouts, and venue-display usability:** TBD — define review criteria without assuming a visual design.

## 5. Backend server

**Status:** Required layer; allocation of responsibilities below remains TBD.

| Concern | Design questions to resolve |
| --- | --- |
| Game lifecycle and configuration | What does the server own when creating, running, ending, or replacing a game? |
| Draw processing | Where do random selection, manual-entry validation, duplicate prevention, and draw ordering occur? |
| Cards and claims | Where are cards generated/assigned and claims validated? How is card integrity established? |
| State distribution | How are host, audience, and player experiences kept consistent? |
| Persistence and history | Which records does the server retain, for how long, and how are they recovered? |
| Access control | How are host privileges and player/viewer permissions checked? |
| Retry and concurrency safety | How are repeated requests, concurrent actions, and reconnecting clients handled without duplicate changes? |

**Server structure, runtime, and scaling model:** TBD. These are planning concerns, not a commitment to separate services or specific frameworks.

## 6. Domain concepts and data ownership

**Status:** Candidate modeling topics only — not database tables or an approved schema.

| Concept | Definition and ownership to complete |
| --- | --- |
| Game configuration | TBD — value pool, numeric upper bound, board dimensions, and winning objective. |
| Active game state | TBD — lifecycle, current draw, remaining pool, and relationship to configuration. |
| Draw record | TBD — string value, ordering, and any metadata needed for consistency/recovery. |
| Digital player card | TBD — cell layout, values, assignment, marking state, and integrity rules. |
| Winning claim | TBD — referenced game/card, validation outcome, and claim lifecycle. |
| Completed-game record | TBD — retained draw history and any other explicitly approved retained data. |

**Cross-cutting decisions:** TBD — identifiers, string comparison/normalization, ownership, retention/deletion, and app/server copies. Preserve the approved string representation; do not silently coerce bingo values into integer identifiers.

## 7. Communication and key workflows

### 7.1 App–backend contract

- **Interaction and update mechanisms:** TBD — choose request/response and live-update approaches after reviewing connectivity needs.
- **Operation boundaries:** TBD — game configuration, draw/entry, state retrieval, card access, claims, and history.
- **Consistency contract:** TBD — ordering, acknowledgments, retry safety, conflict handling, and error categories.
- **Compatibility:** TBD — how app and server versions remain compatible.

Endpoint names, payload schemas, and transport technologies are intentionally not selected here.

### 7.2 Workflows to document

For each workflow, add its trigger, participating components, validation, state changes, success outcome, and failure/recovery behavior.

| Workflow | Design status |
| --- | --- |
| Configure and start a game, including pool/board validation | TBD |
| Connect host, audience display, and players to the intended game | TBD |
| Generate or assign a digital card and mark it during play | TBD — domain rules require review |
| Randomly draw or manually record a value, then update relevant views | TBD |
| Submit and validate a winning claim | TBD — patterns and claim rules require review |
| Exhaust the pool, complete a game, start a new game, and review history | TBD |
| Refresh/restart, lose connectivity, and reconnect | TBD — coordinate with Section 8 |

## 8. Offline operation, synchronization, and recovery

**Key design issue:** A backend is required, but active gameplay must continue through temporary internet outages. The design must explain how these requirements coexist rather than silently assuming constant server access or reducing approved functionality.

| Scenario | Behavior and recovery to define |
| --- | --- |
| Internet lost while the venue's local network remains available | TBD — backend reachability, draw authority, player/audience updates, and claim validation. |
| An individual device loses all connectivity | TBD — available actions, stale-state visibility, and limitations requiring review. |
| Backend unavailable | TBD — distinguish server failure from internet loss; define recovery expectations. |
| Host app refreshes or restarts | TBD — restoring the active game without losing or replaying draws. |
| Connectivity returns | TBD — reconciliation, ordering, duplicate prevention, and conflict resolution. |

**Local state, queued actions, and recovery ownership:** TBD. No local-first, venue-server, or cloud-only architecture is selected by this template.

## 9. Security and privacy

- **Host authorization:** TBD — protect draw controls and game-changing operations (BR-015).
- **Player/card access:** TBD — joining, assignment integrity, and protection against card or claim tampering; accounts are not assumed.
- **Trust and validation boundaries:** TBD — specify what the server and app may trust, including offline actions.
- **Data protection:** TBD — transport, storage, secrets, and minimum necessary participant data.
- **Abuse and operational safeguards:** TBD — assess risks and propose controls for review.

## 10. Quality attributes and verification approach

| Attribute | Target / evidence to define |
| --- | --- |
| Correctness | TBD — pool validity, string handling, duplicate prevention, ordering, board capacity, and claim-validation checks. |
| Recovery and continuity | TBD — refresh/restart, outage, reconnection, and history-retrieval scenarios. |
| Performance and capacity | TBD — expected concurrent games/players/displays and acceptable interaction/update delays. No targets assumed. |
| Usability and accessibility | TBD — host operation, mobile interaction, and shared-display readability criteria. |
| Compatibility | TBD — name the browser/OS test matrix using D-002's latest-generally-available-at-launch policy. |
| Security | TBD — authorization, card/claim integrity, and trust-boundary verification. |

## 11. Deployment and operations

- **Backend location and hosting:** TBD — assess against offline and multi-device needs.
- **App distribution:** TBD — web hosting and Android/iOS installation/distribution.
- **Environments and releases:** TBD — development/testing/production needs, deployment, and rollback approach.
- **Persistence operations:** TBD — backup, restoration, retention, and schema evolution if applicable.
- **Observability and support:** TBD — diagnostic events, health checks, error reporting, and privacy boundaries.
- **Operating costs and constraints:** TBD — record only after discussing expected usage and budget.

## 12. Requirements traceability

This is a planning index, **not evidence that a requirement has been designed or implemented**. Update it as sections are approved.

| Requirements | Design coverage to develop |
| --- | --- |
| BR-001, BR-017 | Section 4 — application identity and Rockville Brews branding. |
| BR-002, BR-006, BR-007, BR-008, BR-009, BR-025, BR-028, BR-029 | Sections 5–7 — configuration, string pools, draws, validation, and exhaustion. |
| BR-003, BR-004, BR-005, BR-010 | Sections 4–7 — latest value, draw records, visibility, and order. |
| BR-011, BR-012, BR-021 | Sections 4–7 — lifecycle, confirmation safeguards, and history. |
| BR-013, BR-016 | Sections 6–8 — persistence, continuity, and recovery. |
| BR-014, BR-018, BR-019, BR-020 | Section 4 — separate views, venue display, objective, and announcements. |
| BR-015 | Sections 5 and 9 — control permissions and enforcement. |
| BR-022, BR-023, BR-024 | Sections 4, 10, and 11 — platform delivery, compatibility, and distribution. |
| BR-026, BR-027 | Sections 4–9 — cards, claims, validation, synchronization, and integrity. |

## 13. Design decision register

HLD IDs track architectural choices separately from business requirements and the D-series business clarifications. `Confirmed constraint` records an explicit user direction; `Open` is not approval of a solution.

| ID | Decision topic | Status | Decision / rationale |
| --- | --- | --- | --- |
| HLD-001 | Backend server and app layer | Confirmed constraint | Both are required by the user's instruction to start this HLD; internal boundaries remain TBD. |
| HLD-002 | Online/offline state authority and backend location | Open | TBD — assess gameplay continuity and multi-device consistency together. |
| HLD-003 | App delivery strategy and technology choices | Open | TBD — web, Android, and iOS coverage without assuming native or cross-platform tooling. |
| HLD-004 | Backend structure and technology choices | Open | TBD. |
| HLD-005 | Persistence and retention approach | Open | TBD. |
| HLD-006 | Communication and synchronization approach | Open | TBD. |
| HLD-007 | Access control and participant/card identity | Open | TBD — do not assume player registration. |

For each reviewed decision, record options, trade-offs, the selected approach, affected BRs, and explicit approval. Keep unresolved game rules visible rather than resolving them implicitly through architecture.

## 14. Review checklist and next planning steps

- [ ] Review game/card/pattern rules needed to make architectural choices.
- [ ] Review system boundaries and app/backend responsibilities.
- [ ] Resolve backend availability, offline authority, and reconnection behavior.
- [ ] Review domain ownership, communication, security, and recovery.
- [ ] Agree on platform delivery, quality targets, and deployment approach.
- [ ] Verify every approved BR has design coverage and reconcile stale source-document prose.
- [ ] Record HLD approval before progressing to detailed technical design or implementation planning.
- [ ] Obtain separate authorization before implementing app or backend code.
