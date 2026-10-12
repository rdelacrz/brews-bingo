# Pending backend, API and database implementation

## Status and use

Backlog assessed against the planning documents and implementation at commit `594adcb` (History APIs). Slice 1 is implemented, verified locally and published as `968747c`; see [LLD Section 3.14](lld.md#314-local-game-discovery-and-availability-implementation-slice). Slice 2 implementation/local gates pass, but final independent review remains pending; see [LLD Section 3.15](lld.md#315-local-player-recovery-and-private-answer-maintenance-checkpoint). Slice 3 is implemented, verified locally and independently reviewed; see [LLD Section 3.16](lld.md#316-local-player-alias-and-explicit-leave-checkpoint). Slice 4 is implemented, verified locally and independently reviewed within documented bounded scopes, with no remaining blocking findings; see [LLD Section 3.17](lld.md#317-local-spectators-and-pre-start-role-switching-checkpoint). Later slice statuses are unchanged. Slice grouping does not authorize later implementation, deployment or publication.

- `[ ]` means pending implementation or verification; `[x]` means completed and verified locally, not publication or production certification.
- Mark a pending item complete only after implementation and relevant native/Worker verification. Approval of a design is not implementation evidence.
- All DO-001–DO-107 design decisions are approved; no schema-policy approvals remain pending. Unfinished wire details and production measurements remain distinct from those approvals.
- Follow later approved decisions and the completion notes in [LLD Sections 3.9–3.17](lld.md), rather than treating every older generic “TBD” statement as a missing feature.
- Frontend work is outside this backend checklist. New implementation scope and commit/push need separate authorization; deployment, infrastructure/account creation and secret provisioning remain unauthorized.

Sources: [requirements](requirements.md), [HLD](hld.md), [LLD](lld.md), [API operation catalog](api-design.md#operation-catalog), [Durable Object design](durable-object-design.md#outstanding-decisions), and [provider research](research.md).

## Slice 1 — Game discovery and dashboard APIs

Goal: locate the current game and support code-based entry without exposing private data or implying that discovery grants access.

- [x] Implement C1 `list_games` and finalize its filtering, ordering, pagination and safe summary contract.
- [x] Implement C2 `get_current_nonterminal_game`, including honest free-slot versus unpublished/pending-reservation results.
- [x] Implement C4 `resolve_game_code` using existing internal code lookup; unknown codes must never create games and terminal codes must not permit admission.
- [x] Implement D3 `get_game_availability` with public entry versus authorized account/participant projections; availability remains advisory until atomic admission.
- [x] Wire current Accounts authority, Directory routing and authoritative Game rechecks; never disclose unpublished codes, rosters, boards or coordination secrets.
- [x] Add the required owner-local queries/indexes and test stale projections, expiry, pending creation/release, revoked authority and peer/storage failure.

References: API C1/C2/C4/D3; DO-011–018. Existing History F3/F4 remains separate rather than being reimplemented as game listing.

**Complete locally.** Concrete wire choices are documented in [the API implementation notes](api-design.md#local-slice-1-discovery-implementation-choices). Existing tables/indexes suffice; no migration is required. Final integrated gates pass: 740 native tests in both profiles, 12 doctests, 226 fresh release Workerd tests and 81 actual HTTPS/WSS checks. Independent follow-up passes 79 isolated Workerd cases with no blocking findings, including the repaired Accounts/Game expiry-in-transit rollback cases. See [LLD Section 3.14](lld.md#314-local-game-discovery-and-availability-implementation-slice) for evidence and limitations. At the verification checkpoint no commit/push, frontend, deployment or provisioning had been performed. The user has separately authorized committing and pushing this reviewed Slice 1; frontend, deployment and provisioning remain unauthorized.

## Slice 2 — Player recovery and private answer maintenance

Goal: restore the same retained player after lost/expired session authority, without allocating another seat or board.

The user authorized local Slice 2 implementation after Slice 1 publication. Its implementation and integrated local gates are verified; final independent review remains pending. The user has now separately authorized committing and pushing Slice 2 with that review gate still outstanding. This does not authorize deployment, provisioning or later implementation.

- [x] Finalize D13/D14 request, response, error and retry contracts, preserving secret-free receipts and no credential replay.
- [x] Implement D13 `recover_player_session`: code, exact current alias and enrolled answer proof only; no terminal recovery or alternative ownership proof.
- [x] Verify outside the write transaction, then revalidate lifecycle, membership, current alias and unchanged verifier/normalization version at commit.
- [x] Advance the player session epoch, invalidate predecessor sessions, issue a fresh fixed-life session and durably fence/close predecessor sockets.
- [x] Implement D14 PUT/DELETE own recovery answer in Awaiting Players/In Progress without rotating or extending the current session.
- [x] Add Game-owned recovery rate-limit buckets with approved failure-only accounting, HMAC subject keys, trusted caller/game aggregation and expiry cleanup.
- [x] Add migrations and tests for proof failure, answer-change/recovery races, rollback, lost response, old-socket denial and secret redaction.
- [ ] Complete final independent review sign-off after the repaired response-release findings. The reviewer provider stopped before a final verdict; passing parent reruns do not substitute for that verdict.

Local contracts and gate evidence: [API notes](api-design.md#local-slice-2-player-recovery-implementation-choices), [LLD Section 3.15](lld.md#315-local-player-recovery-and-private-answer-maintenance-checkpoint). Final executed gates: 756 native tests in both profiles, 12 doctests, 247 fresh release Workerd cases and 95 actual HTTPS/WSS checks. At the verification checkpoint no Slice 2 commit or push had occurred. Commit/push is now separately authorized with the final-review gate still pending; no deployment or provisioning has occurred.

References: API D13/D14; DO-049–052/085–087. Optional answer normalization and verifier enrollment at initial player admission already exist; do not duplicate that foundation. Production KDF/Unicode certification remains in Slice 9.

## Slice 3 — Player alias and explicit Leave lifecycle

Goal: distinguish intentional membership changes from disconnection and terminal Exit.

The user authorized local Slice 3 implementation after publication of the runtime module-layout refactor (`423de87`). Implementation and integrated local gates pass; final independent review has no remaining blocking findings. The user has separately authorized committing and pushing this reviewed Slice 3. Later slices, frontend, deployment and provisioning remain unauthorized.

- [x] Finalize alias no-op/retry behavior and Leave lifecycle/version fencing so a delayed lobby Leave cannot silently acquire in-progress meaning after Start.
- [x] Implement D7 `rename_my_alias` atomically in Awaiting Players, preserving stable player ID, recovery verifier, seat and original session expiry.
- [x] Implement D10 pre-start player Leave: release seat/alias, delete membership/recovery/session authority and require fresh admission on return.
- [x] Implement D11 in-progress player Leave: retain membership/seat/board/matching/winner eligibility and fixed-expiry return authority; record the explicit Leave event separately from current presence.
- [x] Integrate receipts, view revisions, socket detachment/fencing and cleanup without changing terminal Exit behavior.
- [x] Test alias conflicts, Start/Leave races, concurrent requests, reconnect, failure rollback and post-terminal denial in native and Worker paths.
- [x] Complete final independent review sign-off against the frozen source hashes.

Local choices and checkpoint: [API notes](api-design.md#local-slice-3-transport-implementation-choices-d7d10d11), [LLD Section 3.16](lld.md#316-local-player-alias-and-explicit-leave-checkpoint). Executed integrated gates pass: 774 native tests in both profiles, 12 doctests, 284 fresh release Workerd cases and 125 actual HTTPS/WSS checks. Existing schemas suffice; no migration is required. The four reproduced output/close/receipt/prepared-socket findings are repaired and covered by permanent regressions. The first reviewer provider stopped before a final verdict; a fresh bounded read-only review then independently verified R1–R4 repairs, passed 18 focused native tests and signed off with no remaining blocking findings. All nine reviewed production hashes match the frozen source. At the verification checkpoint Slice 3 remained uncommitted and unpublished; commit/push is now separately authorized. Slice 2's earlier pending review is unchanged.

References: API D7/D10/D11; DO-045–048/055. Role switching depends on spectators and is grouped in Slice 4.

## Slice 4 — Spectators and pre-start role switching

Goal: support audience admission and delivery independently of player identity, private boards and Start eligibility.

The user authorized complete local Slice 4 at `50249d3`, with no commit/push or later scope. Implementation, repairs, final integrated gates and bounded independent follow-ups are complete locally with no remaining blocking findings. [API choices](api-design.md#local-slice-4-spectator-and-role-switch-transport-choices), [LLD 3.17](lld.md#317-local-spectators-and-pre-start-role-switching-checkpoint) and [the review/evidence report](../reviews/spectator-slice-2026-10-12.md) record the scope and limitations.

- [x] Finalize spectator admission/retry identity, cookie selection and audience projection contracts.
- [x] Implement D6 spectator admission in Awaiting Players/In Progress, enforcing separate capacity and rejecting terminal admission.
- [x] Add spectator records and spectator-capable participant session bindings, receipts, view revisions and owner-local migrations.
- [x] Extend G1/G2/G3 and H4 for audience-only sync/WSS, presence reconstruction, connection replacement and stale-close fencing; spectators must never count toward the connected-player Start minimum.
- [x] Implement five-minute accidental-disconnect grace and H5 expiry: retain occupancy during grace, reconnect without renewal, and release/delete on expiry.
- [x] Implement D12 spectator Leave with immediate occupancy/session removal and no disconnect grace.
- [x] Implement D8 player-to-spectator and D9 spectator-to-player switching atomically before Start, preserving original session expiry; capacity failure must preserve the original role/seat/proof.
- [x] Delete player recovery proof on switch to spectator; create fresh player identity and optional fresh proof on switch to player.
- [x] Implement best-effort terminal spectator notice followed by identity/session deletion without waiting for application ACK; no terminal spectator reconnect or result-fetch authority.
- [x] Test capacity, switch/Start/termination races, grace boundaries, hibernation, stale events, delivery failure and private-data isolation. Final gates pass: 850 native tests per profile, 12 doctests, 498 Workerd cases in 29 suites and 154 real HTTPS/WSS checks.

- [x] Complete independent review sign-off within the documented frozen-source scopes; no remaining blocking findings. Production/artifact hashes match, and post-review stress-fixture changes are timeout-only with full-suite verification.

References: API D6/D8/D9/D12/G1–G3/H4/H5/H9; DO-047/053–055/067–068/073/091–095. Account/player/spectator sync and stream now share the existing connection and delivery boundaries.

## Slice 5 — Saved-game setup, intentional resume and host transfer

Goal: complete nonterminal operator workflows without recreating games or weakening assignment/removal coordination.

- [x] Finalize C7 patch omission/null/replacement semantics, revision guards, receipts and error contract.
- [x] Implement C7 configuration editing only in New, validating and persisting the complete effective configuration atomically.
- [x] Finalize and implement C9 intentional open/resume, restoring existing state without replacement codes/boards; only qualifying current designated-host activity renews the pre-start idle deadline.
- [x] Finalize C10 target-confirmation and expected-host-assignment-revision transport, plus safe eligible-target discovery for ordinary hosts without exposing admin-only Users data.
- [x] Implement C10 nonterminal host transfer with current actor/target revalidation, target Directory gate fencing, once-only checked revision increments and actor-scoped receipts.
- [x] Coordinate assignment/index changes and retries without clearing the global reservation or allowing transfer to race past account removal; the new host inherits the authoritative resulting idle deadline.
- [x] Add relevant owner-local queries/migrations and native/Worker race, rollback, replay, timeout and revocation tests; integrate the necessary Slice 8 privileged auditing for these operators.

**Completed locally:** C7/C9/C10, bounded safe discovery, exact transfer/removal coordination, Game-owned Slice 5 audit capture/90-day cleanup and strict migrations. Final frozen gates pass: 882 native tests in each profile, 12 doctests, strict native/Wasm Clippy, 544 Workerd tests across 31 suites and 208 actual local HTTPS/WSS checks. Bounded independent Core/runtime reviews and final repair follow-up have no remaining blocking findings; final source/SQL/artifact hashes reconcile. See [review and evidence](../reviews/operator-slice-2026-10-12.md), [local transport choices](api-design.md#local-slice-5-operator-transport-and-coordination-choices) and LLD Section 3.18 for scope, failure provenance and limitations. Slice 2's older independent-review item remains unchanged; Slice 8 older-operation integration and CLI audit reads remain pending. No staging or publication is authorized or performed for Slice 5.

References: API C7/C9/C10; DO-018/039–044/076–090. The completed local slice adds configuration editing, intentional resume and host transfer to the published creation/passive-sync foundation. Production/release gates remain separate.

## Slice 6 — Dedicated authorized read APIs

Goal: finish proposed read routes by reusing existing owner projections, not building a second gameplay engine.

- [ ] Finalize dedicated read contracts, role/session selection, query limits, pre-start unassigned results and terminal/Exit/expiry behavior; reconcile any proposed consolidation before treating a route as complete.
- [ ] Implement C5 game detail.
- [ ] Implement D1 own membership, including player-only recovery-enabled status, and D2 authorized player roster.
- [ ] Implement E1 ordered/latest calls and E2 remaining-value enumeration/count/exhaustion for authorized account viewers.
- [ ] Implement E3 own board, E4 account-authorized board inspection and E5 role-filtered qualification.
- [ ] Implement F1 final-result retrieval under eligible original grants, without creating new terminal access or duplicating History authority.
- [ ] Reuse existing sync/stream/History owner logic where applicable; test read-only cross-host inspection, own-board isolation, spectator restrictions, storage/peer failure and post-Exit/expiry denial.

References: API C5/D1/D2/E1–E5/F1; DO-056–058/063/067–075. Much of the underlying data already appears in `/sync` and `/stream`; separate routes are absent. Full remaining-value enumeration and own recovery-enabled status require additional projection work. Spectator branches depend on Slice 4.

## Slice 7 — Authorized command-result lookup

Goal: resolve an interrupted response using the original command context without granting anonymous receipt access.

- [ ] Finalize G4 committed/pending/unknown/expired/inadmissible response variants and actor proof, including lost-cookie and post-Exit/deletion limitations.
- [ ] Implement G4 `GET /api/games/{game_id}/commands/{command_id}` using existing durable receipts/pending work and current original-actor authority.
- [ ] Return only bounded secret-free outcomes; a command ID alone grants no access, and missing receipts must not imply that repeating effects is safe.
- [ ] Test cross-actor denial, command-age floors, credential non-replay, pending coordination, retention boundaries and no duplicate effects.

References: API G4/G5; DO-076–084. Original-endpoint idempotent retries and receipt storage already exist; this slice adds the separate read-only lookup.

## Slice 8 — Game-side privileged auditing (cross-cutting)

Goal: complete the game-owned portion of auditing alongside privileged game operations, not after the rest of the backend is considered complete.

- [ ] Finalize game-specific privileged capture/failure contracts and operation/target vocabulary under the approved audit rules.
- [ ] Add Game-owned audit storage for authenticated privileged success, rejection and execution failure, preserving the actual acting admin rather than substituting the designated host.
- [ ] Integrate capture with existing and newly added privileged game operations; keep secrets, answers, submitted private content and History snapshots out of audit records.
- [ ] Implement fixed 90-day expiry denial and cleanup with copy/index/restore protection; committed command replay must not duplicate successful action audit.
- [ ] Extend restricted developer-CLI reads to authorized game-owned audit records without adding an app audit endpoint or UI.
- [ ] Add migration, attribution, replay, failure-path, retention and redaction tests.

References: API H12; DO-088–090. Account-management audit storage, retention and developer-only reads already exist. Slice 5 adds Game-owned storage, fixed 90-day cleanup and capture for configuration, resume, host transfer and target discovery only, with local migrations/tests. The unchecked items above track completion across the older game operations and restricted CLI reads; this partial integration does not close Slice 8.

## Slice 9 — Production database/runtime and release gates

These are unfinished validation/operations gates, not evidence that the completed local features are absent. Perform provider-facing actions only after separate authorization.

- [ ] Measure production Argon2 memory/CPU/latency and concurrent authentication/recovery impact; certify work factors, supported verifier bounds and resource/concurrency limits on the actual target.
- [ ] Validate pinned Unicode normalization/case-fold/whitespace behavior and clock/CSPRNG/SDK behavior across required deployed paths.
- [ ] Obtain the owner-provided peak event/History forecast and benchmark maximum legal games per Object: storage, rows read/written, requests, CPU/duration, alarms and WSS amplification.
- [ ] Reverify applicable provider quotas and demonstrate the approved two-times headroom; stop for architecture/cost review if the capacity gate fails.
- [ ] Measure History list scan/sort cost and query plans, including the existing temporary ORDER BY B-tree; bounded page/fanout sizes alone do not establish bounded SQL work.
- [ ] Validate provider backup/copy/log retention and restore-time discard of expired History/audit data and stale indexes; write expiry-preserving restore/reconciliation runbooks.
- [ ] Complete authorized dev/staging/production namespace isolation, infrastructure/account setup, secrets delivery and operator/bootstrap procedures without exposing private bindings to clients.
- [ ] Complete remaining CI/release wiring, deployed integration/load/security gates, operational observability, deployment verification and compatible-backout/forward-fix procedures.

References: LLD Sections 9.2, 10, 11 and the limitations in Sections 3.12–3.13; DO-075/090/099–104. Local native/Workerd results do not certify production quotas, backup controls or deployment readiness.

## Completed local baseline — not pending implementation

The checks below reflect recorded implementation/review checkpoints and the current source inventory, not tests rerun to create this document.

- [x] A1–A7 account authentication, setup/reset, current session and logout.
- [x] Restricted Rust developer CLI, shared account-management rules and selective redacted logging.
- [x] B1–B9 Users/admin account APIs, account auditing and database module/schema organization.
- [x] C6/C8/E6 game creation with configuration overrides, lobby publication and atomic Start/board generation.
- [x] D5 player admission with optional answer enrollment, fixed sessions, account/player presence and G1/G2/G3 sync/WSS foundation.
- [x] E7–E11 random/manual calls, automatic matching/Single Line qualification, one selected winner and confirmed cancellation/no-winner ending.
- [x] Account/player terminal grants and F2 Exit, terminal release/reconciliation, pre-start timeout cleanup and primary History expiry cleanup.
- [x] F3/F4 account-only cross-host History list/detail, bounded pagination/outcome filter, fixed retention and repaired expiry fencing.
- [x] Owner-local Accounts/Directory/Game schemas, migrations, constraints and receipt/pending-work foundations for the implemented slices.

Evidence: [LLD Sections 3.9–3.13](lld.md); [approved implementation contracts](api-design.md#game-creationstart-implementation-contract-decisions) and [History contracts](api-design.md#local-history-implementation-contract-decisions).

## Deferred or outside this implementation backlog

Do not count these as missing first-release backend features:

- Additional History date-range, host and search filters: explicitly deferred by HISTORY-02.
- History exports, additional datastores, public account registration, role editing, manual board marking, post-terminal gameplay and a second Close operation: outside approved first-release scope.
- Offline/venue-local gameplay authority or offline command queues: superseded by the approved online-only/reconnect design.
- Native Android/iOS delivery: later platform scope, not an initial web-backend blocker.
- Frontend screens/adapters and client connection behavior: separate implementation scope, not completed by backend APIs.
