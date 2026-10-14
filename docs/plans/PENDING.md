# Pending backend, API and database implementation

## Status and use

Backlog assessed against the planning documents and implementation at commit `594adcb` (History APIs). Slice 1 is implemented, verified locally and published as `968747c`; see [LLD Section 3.14](lld.md#314-local-game-discovery-and-availability-implementation-slice). Slice 2 implementation/local gates pass, but final independent review remains pending; see [LLD Section 3.15](lld.md#315-local-player-recovery-and-private-answer-maintenance-checkpoint). Slice 3 is implemented, verified locally and independently reviewed; see [LLD Section 3.16](lld.md#316-local-player-alias-and-explicit-leave-checkpoint). Slice 4 is implemented, verified locally and independently reviewed within documented bounded scopes, with no remaining blocking findings; see [LLD Section 3.17](lld.md#317-local-spectators-and-pre-start-role-switching-checkpoint). Slice 5 was subsequently authorized and published as `7ffcd7e4cdb5d0dbaf0980635a8f76ea9f1c28b6`. Slice 6/7 implementation is local, with final slice acceptance still open; the shared response-policy dependency was separately approved and verified on 2026-10-14. Slice 7's unfinished checkpoint was authorized and published as `61b4e657af6c89491824a33b718c8f2fa38664b1`. Slice 8 Game-owned privileged auditing is implemented, independently verified locally and published as `8bbb334a689e85eea765d2f06307f94e2d624284`. Remote `main` matched after push/fetch/readback. Local Slice 9 readiness tooling/measurements are implemented and verified within their documented local scope; whole Slice 9/production acceptance remains open. See [LLD Section 3.21](lld.md#321-local-game-owned-privileged-auditing-slice-8). Slice 9 production database/runtime and release gates remain pending; deployment, provisioning and provider-state changes remain separately unauthorized. Slice grouping does not authorize later implementation, deployment or publication.

- `[ ]` means pending implementation or verification; `[x]` means completed and verified locally, not publication or production certification.
- Mark a pending item complete only after implementation and relevant native/Worker verification. Approval of a design is not implementation evidence.
- All DO-001–DO-107 design decisions are approved; no schema-policy approvals remain pending. Unfinished wire details and production measurements remain distinct from those approvals.
- Follow later approved decisions and the completion notes in [LLD Sections 3.9–3.24](lld.md), rather than treating every older generic “TBD” statement as a missing feature. The 2026-10-14 snapshot-policy approval and verification supersede earlier final-return blocker descriptions; unrelated final Slice 2/6/7 reviews remain open.
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

**Completed locally:** C7/C9/C10, bounded safe discovery, exact transfer/removal coordination, Game-owned Slice 5 audit capture/90-day cleanup and strict migrations. Final frozen gates pass: 882 native tests in each profile, 12 doctests, strict native/Wasm Clippy, 544 Workerd tests across 31 suites and 208 actual local HTTPS/WSS checks. Bounded independent Core/runtime reviews and final repair follow-up have no remaining blocking findings; final source/SQL/artifact hashes reconcile. See [review and evidence](../reviews/operator-slice-2026-10-12.md), [local transport choices](api-design.md#local-slice-5-operator-transport-and-coordination-choices) and LLD Section 3.18 for scope, failure provenance and limitations. Slice 2's older independent-review item remains unchanged; Slice 8 older-operation integration and CLI audit reads remain pending. At the local verification checkpoint, Slice 5 staging/publication had not been authorized or performed. The user subsequently authorized commit/push and it was published as `7ffcd7e4cdb5d0dbaf0980635a8f76ea9f1c28b6`; the original review/evidence snapshot remains immutable.

References: API C7/C9/C10; DO-018/039–044/076–090. The completed local slice adds configuration editing, intentional resume and host transfer to the published creation/passive-sync foundation. Production/release gates remain separate.

## Current local continuation — CI/CD and response snapshots (2026-10-14)

The user authorized local environment isolation, remote-target full-game testing and GitHub manual production approval support. The user separately chose [authorization-time HTTP snapshots](protected-response-policy.md): already-authorized responses may finish after revocation, while subsequent authoritative validations deny revoked access. This supersedes the unselected policy/receiving-cut acceptance dependency described in the historical Slice 6–9 checkpoints below; it does not extend expiry or accept an older failed assertion as a pass.

- [x] Record the explicit policy choice, named Accounts/Game/Directory cuts and unchanged expiry/replay safeguards.
- [x] Verify new response-policy acceptance: all 20 mandatory cases pass; the unchanged old probe retains its actual exit 1/failed assertion under the superseded rule.
- [x] Complete bounded independent reviews and integrated local verification of isolation, frozen-artifact tools, workflows, serving identity and remote-target harness, including cleanup. The runner exits 0; the final direct full-game run passes 96 checks against fresh local artifacts. See [review/evidence](../reviews/cicd-snapshot-2026-10-14.md) for exact cuts, preserved failures and limitations, including the unclassified earlier terminal-sync failure. This does not certify provider state or actual staging.
- [ ] Obtain separate publication authorization for this continuation; no commit/push is implied.

No actual staging/production deployment or infrastructure/secret provisioning has occurred. The eight external production checkboxes remain open, as does unrelated Slice 2 review. Current verification belongs in LLD 3.24 and the new continuation review, not rewritten historical reports.

## Slice 6 — Dedicated authorized read APIs

Goal: finish proposed read routes by reusing existing owner projections, not building a second gameplay engine.

**Local implementation present; acceptance blocked.** The nine dedicated routes, passive projections, required read auditing and audit-vocabulary migration are implemented. `RUNTIME-FINAL-01` reproduced five final-return races despite genuine current-invocation ACKs. A decision is required on explicitly specified authoritative-cut response semantics versus additional mutation/delivery coordination; neither has been approved. The E5 codec's duplicate/order defect has a tested local repair, with independent follow-up recorded separately. Full runtime acceptance is not established and all completion boxes remain unchecked. The user separately authorized the unfinished checkpoint commit/push; `7134e6ed9d468afaf261605433740753f3cc7c88` is now verified on `origin/main`. Publication does not close the finding or approve either contract choice. See [blocked review/evidence](../reviews/dedicated-read-slice-2026-10-12.md) and LLD Section 3.19.

- [ ] Finalize dedicated read contracts, role/session selection, query limits, pre-start unassigned results and terminal/Exit/expiry behavior; reconcile any proposed consolidation before treating a route as complete.
- [ ] Implement C5 game detail.
- [ ] Implement D1 own membership, including player-only recovery-enabled status, and D2 authorized player roster.
- [ ] Implement E1 ordered/latest calls and E2 remaining-value enumeration/count/exhaustion for authorized account viewers.
- [ ] Implement E3 own board, E4 account-authorized board inspection and E5 role-filtered qualification.
- [ ] Implement F1 final-result retrieval under eligible original grants, without creating new terminal access or duplicating History authority.
- [ ] Reuse existing sync/stream/History owner logic where applicable; test read-only cross-host inspection, own-board isolation, spectator restrictions, storage/peer failure and post-Exit/expiry denial.

References: API C5/D1/D2/E1–E5/F1; DO-056–058/063/067–075. Dedicated routes are present locally but not accepted. Full live remaining-value enumeration and own recovery-enabled status have projection coverage; terminal remaining-values intentionally returns 409 because approved History omits the pool bound. Spectator branches use the completed Slice 4 foundation. The final-return decision and full runtime acceptance remain open.

## Slice 7 — Authorized command-result lookup

Goal: resolve an interrupted response using the original command context without granting anonymous receipt access.

- [x] Finalize the local closed G4 committed/pending/unknown/expired/inadmissible response variants and actor proof, including lost-cookie and post-Exit/deletion limitations.
- [x] Implement G4 `GET /api/games/{game_id}/commands/{command_id}` using existing durable receipts/pending work and current original-actor authority.
- [x] Return only bounded secret-free outcomes; a command ID alone grants no access, and missing receipts must not imply that repeating effects is safe.
- [x] Test cross-actor denial, command-age floors, credential non-replay, pending coordination, retention boundaries and no duplicate effects.

References: API G4/G5; DO-076–084. Original-endpoint idempotent retries and receipt storage already exist; this slice adds the separate read-only lookup.

**Implemented and verified locally; final acceptance remains OPEN.** Final frozen integrated gates pass: **925 native tests in each profile**, **12 doctests**, strict native/Wasm Clippy and formatting, **656 positive Workerd cases across 34 suites**, and **314 actual local HTTPS/WSS checks**. The dedicated-read/G4 harness contains 106 checks, including 10 explicitly enumerated G4 checks. Independent bounded review and follow-ups close G4-AUDIT-01, G4-FACTS-01 and G4-AUDIT-ALARM-01, including required rejection/failure capture, semantic transfer/receipt consistency and audit-only owner retention alarms. No new blocking findings remain beyond the explicitly open shared contract dependency. All 234 source/test, 3 schema-fixture and 4 Worker artifact hashes reconcile. These positive results do not accept the failing last-hop assertion or certify production. See [review/evidence](../reviews/command-lookup-slice-2026-10-12.md), [API implementation choices](api-design.md#local-slice-7-command-result-lookup-implementation-choices) and [LLD Section 3.20](lld.md#320-local-command-result-lookup-checkpoint-slice-7).

- [x] Complete frozen source/schema/artifact verification, integrated positive gates and independent bounded review/finding follow-ups.
- [x] Select shared `RUNTIME-FINAL-01` response semantics: on 2026-10-14 the user explicitly approved authorization-time snapshots; revocation blocks subsequent authorization. The old held-200 observation is retained against the superseded receiving-cut rule. New-policy verification is tracked above; no extra RPC is called a last-hop repair.
- [ ] Final Slice 7 acceptance after the shared contract decision and its corresponding implementation/verification. The user authorized committing and pushing this unfinished checkpoint on 2026-10-13; publication does not accept its protected-response semantics or claim production readiness. The dated Slice 7 review/evidence preserve the pre-publication verification snapshot. Slice 6 completion boxes and Slice 2's older independent-review item remain unchecked, and the original Slice 6 probe/report/evidence remain unchanged.

## Slice 8 — Game-side privileged auditing (cross-cutting)

Goal: complete the game-owned portion of auditing alongside privileged game operations, not after the rest of the backend is considered complete.

- [x] Finalize the local game-specific privileged capture/failure contracts and closed operation/target vocabulary under the approved audit rules.
- [x] Complete Game-owned audit storage for authenticated privileged success, rejection and execution failure, preserving the actual acting admin rather than substituting the designated host.
- [x] Integrate capture with the older and newly added privileged Game operations; keep secrets, answers, submitted private content and History snapshots out of audit records.
- [x] Implement fixed 90-day expiry denial and bounded cleanup with local copy/index/restore enforcement; committed command replay does not duplicate successful action audit. Provider physical-copy/log/backup controls remain Slice 9 gates.
- [x] Extend restricted developer-CLI reads to exact authorized Game-owned audit records without adding an app audit endpoint or UI.
- [x] Add and run migration, attribution, replay, failure-path, retention/redaction and real SDK/HTTPS/WSS tests; independently verify the repaired frozen candidate within the recorded scope.

References: API H12; DO-088–090. Slice 5 introduced the partial Game audit foundation; Slice 8 now extends capture across the older mutations, account Sync/actual Stream, Game-backed discovery/History reads and restricted CLI, preserving prior actor/deadline/migration rules. Safe closed local wire/CLI choices are documented as implementation choices, not newly approved product policy.

**Implemented, verified locally and published as `8bbb334a689e85eea765d2f06307f94e2d624284`.** Final frozen integrated gates pass: **956 native tests per profile**, **12 doctests**, strict native/Wasm Clippy and formatting, **711 complete positive Workerd cases in 36 suites**, and **373 actual local HTTPS/WSS checks**. All 242 source/test, 4 schema/fixture and 4 Worker artifact hashes reconcile with the refreshed code-only graph. Independent Core/runtime and final narrow Stream follow-ups close the nine initial audit findings and S8-FOLLOWUP-01, with no new blocker in that bounded scope; failed prior attempts remain recorded. See [review/evidence](../reviews/game-audit-slice-2026-10-13.md), [local API choices](api-design.md#local-slice-8-game-owned-privileged-audit-implementation-choices) and [LLD Section 3.21](lld.md#321-local-game-owned-privileged-auditing-slice-8).

This does not close shared `RUNTIME-FINAL-01`, choose protected-response semantics, accept Slice 6/7, finish Slice 2's older review or certify provider copies/restore, full migration population, production resource costs or deployment. The unchanged final-return G4 assertion still fails on the final audit artifact. The user has now authorized committing/pushing this reviewed Slice 8 and working on Slice 9 locally. The dated review/evidence preserve the pre-publication verification snapshot. [Publication reconciliation](../reviews/game-audit-slice-publication-reconciliation.json) records the sole trailing-blank-line test cleanup, unchanged assertion/production/schema/artifact bytes, refreshed graph and 32 passing affected regressions. The authorized commit/push was then verified at `origin/main` as `8bbb334a689e85eea765d2f06307f94e2d624284`, with a clean worktree before Slice 9 began. Frontend, deployment/provisioning, secret delivery and provider-state changes remain unauthorized. This authorization does not choose the protected-response contract or close any production gate.

## Slice 9 — Production database/runtime and release gates

These are unfinished validation/operations gates, not evidence that the completed local features are absent. Perform provider-facing actions only after separate authorization.

**Local Slice 9 foundation implemented, verified locally and published as `b158f1375d9de0b3308de47da1a5bdd1e9647047`.** The user authorized local work after Slice 8 publication, not deployment/provisioning/secret delivery or provider-state changes. Final positive checks pass: **958 native tests per profile**, **12 doctests**, strict native/Wasm Clippy/formatting, **712 Workerd cases in 37 suites**, **373 actual HTTPS/WSS checks** and **18 Python runner tests**. The overall runner returns **2** because the retained final-return acceptance assertion still fails with HTTP 200, not because that failure became a passing diagnostic. Source/four-artifact maps and the refreshed **246-source** code graph match. Bounded independent follow-up accepts RG01–RG07 repairs; original failed attempts remain recorded. See [review/evidence](../reviews/runtime-readiness-slice-2026-10-13.md) and [LLD 3.22](lld.md#322-local-runtime-measurement-and-release-tooling-foundation-slice-9).

Verified **local-only** deliverables, separate from the production gates below:

- [x] Implement and exercise the provider-neutral [local release runner](../../backend/tools/README.md), including complete inventories, drift/fresh-build checks, bounded process cleanup and safe evidence; independently review its repaired boundaries. Current snapshot acceptance is mandatory; the old receiving-cut failure is retained separately as historical evidence.
- [x] Measure the exact production History query at fixed local metadata populations and sample current native KDF/selected real native/Workerd recovery behavior. First-page VM steps are **2,657 / 18,503 / 176,003** at **100 / 1,000 / 10,000** entries, with the existing temporary sort. These are not forecasts, maximum-game or provider CPU/row-work measurements.
- [x] Document an [expiry-preserving restore/backout worksheet](../operations/restore-and-backout.md) with quarantine/current-authority stop conditions; no provider restore mechanism or drill is claimed.
- [x] Refresh the [public provider-limit reference](../operations/provider-limit-reference-2026-10-13.md) read-only; account/plan/configured applicability and headroom remain unverified.

**Maximum-configured-game local continuation implemented, verified locally and published as `1f742c92b0c394864644890d8478f4563bcae5e4`.** Actual service admission, Start/calls/terminal operations create 20 boards/2,000 VALUE cells at the current 20-player/50-spectator/10×10/no-free-cell/1,000-value branch. Native accepts all 1,000 calls with 70 logical connections and zero sockets; Workerd admits all 70 participants and samples one call with three real upgraded socket pairs, not full socket/call load or maximum-game network TLS/WSS. SDK cursor work and page PRAGMAs remain explicitly unavailable; no zero counters or provider costs are invented. Frozen full checks pass 959 native tests per profile, 12 doctests, 713 Workerd cases/38 suites and 373 actual HTTPS/WSS checks, but the full runner still exits 2 for `RUNTIME-FINAL-01`. A later exact two-file parser/unit repair is separately covered by 27 Python tests, fresh native/Workerd probe03 and independent follow-up accepting GR-REVIEW-01/02 with no new blocker in that scope. All other 266 mapped sources and four artifacts remain identical; the historical full gate is not relabeled a current-parser rerun. See [review/evidence](../reviews/game-resource-continuation-2026-10-13.md), [measurement guide](../operations/game-resource-measurements.md) and [LLD 3.23](lld.md#323-local-maximum-configured-game-resource-measurement-continuation).

- [x] Implement, exercise and independently review the bounded local maximum-configured-game resource observations and safe probe; record the exact workload, unavailable counters and parser-only repair reconciliation.

No new numbered Slice 10 is inferred, no owner event/History forecast is fabricated, and local instrumented SQL/bytes/wall-time observations cannot certify provider capacity or all worst-case workloads. The existing eight production gates below stay unchecked until their exact acceptance criteria are met. The user authorized publication with “push”; exact remote `main` and `origin/main` readback matched `1f742c92b0c394864644890d8478f4563bcae5e4`, with a clean worktree. [Publication reconciliation](../reviews/game-resource-continuation-publication-reconciliation.json) records that result; the dated review/evidence retain their immutable prepublication cut. This request authorizes no new implementation or deployment.

**Historical foundation checkpoint (2026-10-13):** no CI provider or protected-response policy had been selected at that cut. [Publication reconciliation](../reviews/runtime-readiness-slice-publication-reconciliation.json) binds the frozen source/config/schema/probe commit blobs and records remote `main` equality at `b158f1375d9de0b3308de47da1a5bdd1e9647047`; the worktree was clean before continuation. Its eight provider gates and independent slice reviews were not closed by publication. Later approved work and the current checklist below govern present status.

**Current Slice 9 audit (2026-10-14):** the reviewed CI/CD and snapshot-policy continuation is implemented and verified locally; the user has separately authorized commit/push. [Review and machine-readable evidence](../reviews/cicd-snapshot-2026-10-14.md) record 959 native tests per profile, 12 doctests, 723 general Workerd cases plus 20 mandatory snapshot-contract cases, 373 existing HTTPS/WSS checks, 135 Python tests and 96 checks in the final loopback full game. The policy decision and current `RUNTIME-FINAL-01` acceptance pass; the unchanged older assertion remains a superseded historical failure. This does not complete unrelated Slice 2/6/7 sign-offs or certify a deployed target.

- [x] Implement and verify explicit staging/production configuration templates and fail-closed inventory/namespace/secret-reference checks. Actual provider resources and secret separation remain unverified.
- [x] Implement and independently review GitHub validation and manual-production workflows, exact candidate freezing, staging-proof continuity, post-approval readback and production protection checks; verify workflow syntax and pinned Wrangler packaging locally. Hosted execution and actual repository protections remain unverified.
- [x] Implement and exercise the remote-target full-game HTTPS/WSS harness against a real local Worker, including the independent board oracle, command replay, winner/History/release and exact own-account cleanup readback. Actual Cloudflare staging execution remains pending.
- [x] Implement and pass the mandatory authorization-time snapshot acceptance; preserve the original probe and its physical result without treating it as passing current-policy evidence.
- [x] Document operator, isolation, readiness, deployment verification and compatible-backout/forward-fix prerequisites in the [CI/CD guide](../operations/cicd.md) and restore worksheet. No provider drill is claimed.

**Remaining production acceptance, not absent local features:** all eight items below still lack their required external evidence. No actual deployment inventory, target origin, reviewed readiness or provider execution credentials are available in this session. Owner forecast and separately authorized provider setup/execution are prerequisites, not values or measurements to invent.

- [ ] Measure production Argon2 memory/CPU/latency and concurrent authentication/recovery impact; certify work factors, supported verifier bounds and resource/concurrency limits on the actual target.
- [ ] Validate pinned Unicode normalization/case-fold/whitespace behavior and clock/CSPRNG/SDK behavior across required deployed paths.
- [ ] Obtain the owner-provided peak event/History forecast and benchmark maximum legal games per Object: storage, rows read/written, requests, CPU/duration, alarms and WSS amplification.
- [ ] Reverify applicable provider quotas and demonstrate the approved two-times headroom; stop for architecture/cost review if the capacity gate fails.
- [ ] Measure deployed History list scan/sort cost and query plans, including the existing temporary ORDER BY B-tree; local query measurements are complete, but bounded page/fanout sizes alone do not establish bounded provider SQL work.
- [ ] Validate provider backup/copy/log retention and restore-time discard of expired History/audit data and stale indexes; the expiry-preserving restore/reconciliation worksheet is written, but provider controls and execution remain unverified.
- [ ] Complete actual authorized dev/staging/production namespace isolation, infrastructure/account setup, secrets delivery and operator/bootstrap execution without exposing private bindings to clients; local templates, validators and procedures are implemented.
- [ ] Execute and verify the implemented CI/release wiring on the actual repository/targets, deployed integration/load/security gates, operational observability, deployment readback and compatible-backout/forward-fix drills. Local workflows/harnesses/runbooks are complete, not hosted or provider acceptance.

| Gate | Verified local support | Evidence still required |
| --- | --- | --- |
| Argon2 target | Native/Workerd KDF and recovery observations; bounded verifier validation. | Target memory/CPU/latency, concurrent auth/recovery and reviewed production factors/caps. |
| Unicode/clock/CSPRNG/SDK | Pinned implementation and selected native/Workerd route regressions. | Required deployed-path behavior and target runtime validation. |
| Owner forecast / maximum game | Published maximum-configured-game native and sampled Workerd resource probe. | Owner peak event/History forecast and full forecast/max-game provider load, including row work, CPU, alarms and WSS amplification. |
| Quotas / two-times headroom | Dated public limits reference and safe resource summaries. | Applicable account/plan/configured quotas and measured headroom under the approved workload. |
| History query cost | Exact production query, local query plans and VM-step/sort observations at fixed populations. | Deployed provider query/row-work/CPU cost at the approved forecast. |
| Copies / retention / restore | Expiry fences, cleanup regressions and expiry-preserving quarantine/reconciliation worksheet. | Physical-copy/backup/log controls and an authorized isolated provider restore/expiry drill. |
| Isolation / secrets / bootstrap | Environment templates, inventory checks, restricted CLI and operator/setup guide. | Actual namespaces/accounts/origins, independently separated secret provisioning and operator execution. |
| Deployed release / operations | Reviewed workflows, frozen-candidate/protection/readback checks, local full-game harness and backout prerequisites. | Hosted CI, real protection/token settings, actual staging integration/security/load/observability and compatible provider backout/forward-fix drills. |

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
