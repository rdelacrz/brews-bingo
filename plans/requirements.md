# Brews Bingo — Business Requirements

## Document status

**Stage:** High-level business requirements only. No application implementation is authorized by this document.

The business requirements below have been reviewed individually through the Hermes web app. They are consolidated into one numbered set so that all approved scope uses stable BR-IDs.

## Business context and objective

- **Business:** Rockville Brews.
- **Application name:** Brews Bingo.
- **Occasion:** An upcoming bingo night.
- **Objective:** Help Rockville Brews run bingo by randomly drawing bingo values, displaying the current draw, and keeping a visible record of values already drawn in the current game.

### Original stakeholder request

> “We have a bingo night coming up we need like an app that randomly shows the number and keeps track and shows all of the numbers drawn. You think you can do that?”

Source: The Rockville Brews owner's request, supplied by the user. The application name and planning-first approach were supplied separately by the user.

## Consolidated business requirements

Every row is part of the same approved business-requirements set. The previously separate suggested requirements have been renumbered as BR-006 through BR-025; no requirement was removed by the consolidation.

| ID | Business requirement | Source | Approval status |
| --- | --- | --- | --- |
| BR-001 | The application shall be called **Brews Bingo**. | User's instruction. | Approved |
| BR-002 | The application shall support either randomly selecting a bingo value or allowing the bingo caller to enter a value manually. Every entered or selected value shall be represented as a string and validated against the valid string pool for the game; values outside the pool or already drawn in that game shall be rejected. | Owner: “randomly shows the number”; user's revision and current-iteration scope. | Approved — revised wording |
| BR-003 | The application shall display the most recently drawn bingo value. | Owner: “shows the number.” | Approved |
| BR-004 | The application shall keep a record of the bingo values drawn during the current game. | Owner: “keeps track.” | Approved |
| BR-005 | The application shall let users see all bingo values drawn during the current game. | Owner: “shows all of the numbers drawn.” | Approved |
| BR-006 | Each game shall draw only from a configured pool of valid string values. The current iteration shall focus on generating and tracking values stored as strings; the pool may represent any agreed bingo format or combination of bingo values. | User's current-iteration scope; previously approved suggested requirement. | Approved |
| BR-007 | A bingo value shall not be drawn more than once within the same game. | Previously approved suggested requirement. | Approved |
| BR-008 | The bingo host shall control when each next value is drawn. | Previously approved suggested requirement. | Approved |
| BR-009 | The application shall clearly indicate when no undrawn values remain. | Previously approved suggested requirement. | Approved |
| BR-010 | The host shall be able to review drawn values in the order they were called. | Previously approved suggested requirement. | Approved |
| BR-011 | The host shall be able to run multiple games during an event sequentially. At most one game may be nonterminal at a time (New, Awaiting Players or In Progress); another game may be created only after the current game becomes Resolved or Cancelled. Terminal games do not occupy the limit. | Previously approved suggested requirement; user decision HLD-077. | Approved — revised wording |
| BR-012 | Clearing or replacing an active game's draw record shall require explicit confirmation to prevent accidental loss. | Previously approved suggested requirement. | Approved |
| BR-013 | An active game's drawn-value record shall survive an accidental page refresh or application restart. | Previously approved suggested requirement. | Approved |
| BR-014 | The current value and drawn-value record shall be readable by attendees on a shared venue display from typical seating positions. | Previously approved suggested requirement. | Approved |
| BR-015 | Only the designated host shall be able to change the active game or draw values. How this is enforced remains undecided. | Previously approved suggested requirement. | Approved |
| BR-016 | An active game shall remain usable if the venue loses its internet connection. | Previously approved suggested requirement. | Approved |
| BR-017 | The application shall visibly identify Rockville Brews as the business hosting the bingo experience. | Previously approved suggested requirement. | Approved |
| BR-018 | The host shall have an operating view separate from the audience's display of the game. | Previously approved suggested requirement. | Approved |
| BR-019 | The audience display shall show the winning pattern or objective for the current game. | Previously approved suggested requirement. | Approved |
| BR-020 | The application shall offer an optional spoken announcement of each drawn value. | Previously approved suggested requirement. | Approved |
| BR-021 | The host shall be able to review completed games' draw records after starting a new game. | Previously approved suggested requirement. | Approved |
| BR-022 | The application shall support access through web browsers on desktop and mobile devices. | Previously approved suggested requirement. | Approved |
| BR-023 | The application shall support Android devices. | Previously approved suggested requirement. | Approved |
| BR-024 | The application shall support iOS devices. | Previously approved suggested requirement. | Approved |
| BR-025 | The application shall treat bingo values as strings in the current iteration, including values that may contain letters, digits, separators, or other agreed text combinations. | User's current-iteration scope; previously approved suggested requirement. | Approved |
| BR-026 | The application shall support digital player cards for attendees. The detailed rules for card generation, assignment, marking, and winner handling remain to be defined. | D-003 clarification. | Approved via decision clarification |
| BR-027 | The application shall validate winning claims against the active game's drawn values and configured winning pattern or objective. The detailed winning-pattern and validation rules remain to be defined. | D-004 clarification. | Approved via decision clarification |
| BR-028 | For the first release, the host shall be able to configure a numeric string-value range beginning at `1` and ending at a host-defined upper bound, with the default upper bound set to `75`. | D-001 clarification. | Approved via decision clarification |
| BR-029 | The host shall be able to configure bingo-board width and height, defaulting to `5 × 5`; the application shall validate that the configured numeric value range contains enough values to fill the configured board. | D-001 clarification. | Approved via decision clarification |

**Interpretation to confirm later:** “Keeps track” is interpreted here as tracking draws within the active game. It does not yet imply saving past games or surviving a closed application. The value-pool model is defined as configurable strings, while the exact first-release pool examples, draw timing, screen layout, and host workflow remain to be detailed.

### How to review these requirements

Refer to stable BR-IDs rather than document line numbers. Each requirement can be marked **Approved**, **Needs revision**, **Deferred**, or **Rejected**. A deferred or rejected requirement is not part of the agreed release scope.

Example: `Approve BR-006 and BR-007. Revise BR-013 to cover refresh only. Defer BR-020.`

When a requirement is revised, keep its ID and update its wording and status. All requirements currently listed in this document have been approved; future changes should use the same individual review process.

## Business decisions and clarifications

The original open decisions have been reviewed one by one. Their high-level direction is recorded below; detailed rules and acceptance criteria will be developed in the game-workflow and implementation-planning stages.

| ID | Decision needed | Why it matters | Status / clarification |
| --- | --- | --- | --- |
| D-001 | What configured string-value pool and format combinations should be supported in the first release? | Defines the initial pool configuration and examples while preserving the ability to represent values beyond integers. | **Clarified:** The first release shall support numeric string values from `1` through a host-defined upper bound, defaulting to `75`. The application shall validate that the configured range provides enough values to fill the bingo board. Board width and height shall be configurable, defaulting to `5 × 5`. |
| D-002 | Which desktop and mobile browsers, Android versions/devices, and iOS versions/devices are required for launch? | Defines the supported platform matrix and compatibility testing scope. | **Clarified:** Support the latest generally available desktop and mobile browsers, Android versions/devices, and iOS versions/devices at launch. |
| D-003 | Will attendees use physical bingo cards, or is managing player cards part of the desired app? | Distinguishes a caller/display tool from a player-facing bingo platform. | **Clarified:** Digital player cards are in scope for the first release. Detailed card generation, assignment, marking, and winner-handling rules remain open for workflow planning. |
| D-004 | Will a person verify winning claims, or is the app expected to help validate them? | Winner validation is additional scope, not implied by number tracking. | **Clarified:** The app shall validate winning claims in the first release against the active game's drawn values and configured winning pattern or objective. Detailed winning-pattern and validation rules remain open for workflow planning. |
| D-005 | What internet access can be relied on at the venue? | Helps evaluate offline operation and recovery needs. | **Clarified:** Internet may be intermittent. The app shall continue operating through temporary internet outages. |

## Scope boundaries at this stage

- This document defines business needs, not screen designs, technical architecture, or a technology stack.
- The application is intended to support web access on desktop and mobile, Android devices, and iOS devices. The exact browser and operating-system support matrix remains to be defined.
- The current iteration focuses on bingo value generation and tracking. Values are modeled as strings rather than integers so that future pools can include arbitrary agreed combinations such as letters, digits, and separators.
- Player registration, payments, prizes, and event administration are **not currently requested**. Digital player cards and first-release winner validation are in scope, but card generation, assignment, marking, winning-pattern, and validation details remain to be defined.
- No decision has been made about installation, hosting, accounts, storage technology, or whether a server-side backend is needed.
- Tracking a game does not yet imply keeping player identities or other personal data.
- No code, infrastructure, or dependencies should be introduced as part of this requirements stage.

## Proposed planning stages

Each stage should be reviewed before progressing to the next; this is not approval to implement.

1. **Business requirements — current stage:** Review, revise, defer, or reject business requirements individually.
2. **Game rules and workflows:** Resolve relevant open decisions and define how a host starts, runs, finishes, and recovers a game within the approved scope.
3. **Front-end planning:** Agree on views, information hierarchy, controls, accessibility, and expected device/display use before selecting visual details.
4. **Application logic and backend planning:** Define draw rules, game state, data lifetime, recovery, and any synchronization or access-control needs. Decide which responsibilities belong on the device and whether any require a backend service.
5. **Implementation planning:** Choose technologies, define testable acceptance criteria, and divide the approved scope into small implementation milestones.

Implementation begins only after explicit approval to move beyond planning. Before that, success means having an agreed scope with unresolved questions clearly identified—not having an app scaffold.
