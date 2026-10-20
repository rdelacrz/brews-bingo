# Local Dioxus web implementation — 2026-10-18

Status: implemented and verified locally, with no remaining blocking finding in the bounded reviews below. The final local gate exits 0. Work remains uncommitted; this document does not authorize publication or deployment.

## Scope

The user authorized the Dioxus web implementation against the existing plans. All VIEW-01–VIEW-17 surfaces are implemented in `shared/ui`, with a thin `platform/web` launcher, locked Dioxus/Tailwind tooling, same-origin typed HTTP/WebSocket clients and local Workers Static Assets packaging. See LLD §4.7 and `platform/web/README.md` for the boundaries and commands.

The implementation reuses shared domain/contracts/config and the existing public environment singleton. Backend roles, game state, revisions, qualification, winner choice, membership, fixed expiry and History remain authoritative. No public registration, exports, manual board marking, board regeneration, automatic award, offline mutation queue or implicit game cancellation was added.

## Runtime and safety repairs

- `/api/session` supplies standard Date for server/monotonic clock calibration. Fast device clocks do not become authority.
- Real keyed sibling boundaries remount private hook state on identity/context changes. Password/link material is cleared and not included in diagnostics.
- Account-cookie operations share bounded in-tab and origin-wide Web Locks. Password completion checks the expected restricted account/scope/deadline under that same lock. A cancelled or expired queued auth mutation cannot dispatch later; an undispatched queue failure does not start another blocked session recheck.
- Game commands preserve original ID/body uncertainty. Later rejection does not erase an earlier unknown outcome. Accepted call revisions gate subsequent commands until an authorized snapshot covers them.
- Full HTTP revalidation drains WebSocket events concurrently, prioritizes queued policy closes/terminal notices, and cannot roll back a newer snapshot. Covered queued frames are ACKed but not rendered. Recovery clears rejected opposite-role credentials through the approved fresh-code context, while live spectators still block player recovery.
- Users retains unresolved command state across panels and explains known hosted-game/last-admin guards with fixed safe messages.
- BoardGrid is shared by live/History views, with one named focusable scroll region, coordinate labels and non-color matches. Public Home and guarded Games remain separate. Results display the supplied UTC end time.

### Snapshot-send contention

A local Workerd reproducer held one account-connection authorization while a second unchanged broadcast ran. The old in-flight set treated overlap as Conflict and attempted to retire the healthy socket twice. This is consistent with pre-dispatch 503 followed by an unknown command and a later presence/revision conflict; it does not establish that every historical 503 had this cause.

The repair replaces that rejection guard with a per-Game async snapshot-send mutex. Ready-state, identity, authorization, expiry, revision, serialized-size, delivery-credit and final-send checks still run after acquisition. No API or schema change was introduced. The permanent `backend/worker-tests/snapshot_serialization.test.js` passed against the repair: no forced closure and both reads returned 200. A bounded independent review found no concrete deadlock/reentrancy/security blocker.

All temporary enum-only runtime diagnostics were restored from guarded backups. They are not part of the delivered source.

## Review closure

Independent, read-only supplied-source reviews covered core/auth, game/recovery/transport, Home/Users/History/assets and the supporting runtime change. Findings led to scoped repairs and regression tests. Core review04, game review05 and backend review06 reported no remaining blockers in their scopes. Dashboard review05's nonblocking guard-copy finding was repaired with typed errors and native/browser assertions. Core review07 closed the bounded auth-queue and guard-message changes. Backend review08 confirmed the final-await test correction preserves the stale-output security checks, with no blocker in that small scope.

The first final gate failed in `game_object.test.js`. Isolated execution identified the obsolete overlap case: it awaited a second alarm before releasing the first alarm's held final proof, then expected healthy-socket retirement. The other two focused cases passed. The corrected test starts the overlapping alarm without awaiting it, verifies it waits, then releases the proof and requires exactly one fresh revision on an OPEN socket. Changed-view/revocation close codes and zero-output assertions remain. All three focused cases pass. Only this regression file changed after the successful browser run; the source-difference record and asset hashes bind that unchanged application build. The failed gate and before/after focused results are retained, not replaced by the final rerun.

These are bounded source reviews, not a claim of exhaustive proof, independent human audit or deployed acceptance.

## Verification evidence

The [evidence manifest](evidence/frontend-local-2026-10-18/manifest.json) binds **362 release inputs**, **49 frontend assets**, the unchanged Worker artifacts and **319 current structural code sources**. Source/asset checks reconcile before and after execution. The final release-input digest is `e58340a76dc4ad314354d43ac82132340e15631a470ff768b518ba6426c0489a`; the original browser/native input set differs only in the corrected Worker regression, not application code.

| Check | Executed result |
| --- | --- |
| Workspace nextest, debug | 1,038 passed |
| Workspace nextest, release-assertions | 1,038 passed |
| Doctests | 12 passed |
| Complete general Workerd inventory | 741 passed across 41 suites; no failures/skips |
| Current authorization-time snapshot policy | 20 passed; no failures/skips |
| Actual local HTTPS/WSS | 375 checks passed |
| Chromium application flows | 35 checks passed; zero script errors |
| Backend tooling / frontend packaging | 136 / 2 tests passed |
| Formatting, strict native/backend-Wasm/frontend-Wasm Clippy, release web build | Passed |
| Text-token contrast | 15 pairs passed the 4.5:1 threshold |

Browser run42 is the final application run. It includes provisioning/login/reset, real-cookie admission, recovery, role changes, held-read replacement handling, host transfer/guards, calls/qualification/winner, History, final Exits, cancellation, a 10×10 mobile board, account deletion, bounded/cancelled cross-tab auth queues and exact result end-time rendering for account/player/spectator views. [Desktop](evidence/frontend-local-2026-10-18/home-desktop.png) and [mobile](evidence/frontend-local-2026-10-18/home-mobile.png) screenshots show public Home only. The prior complete run41 also passed.

The immutable historical receiving-cut probe still physically exits 1 on its superseded assertion (observed return 200). This is preserved separately and is consistent with the approved authorization-time snapshot policy; it is not counted as a passing test. The current-policy lane and overall local gate pass. Failed prior gate/browser evidence remains archived alongside the successful rerun.

Normal browser flows use actual local Worker responses and persisted backend state. Explicit fault fixtures are identified: a held membership read released as 503 during a real socket replacement, and a rejected test-only spectator credential. No plausible synthetic success payload substitutes for a failed API.

Only allowlisted check identities, statuses, booleans/counts and public Home screenshots are retained. Reports contain no credentials, private links, recovery proof, private boards or raw child-process output.

## Coverage and limits

- UI-AC-01–06, 09–10, 12, 14 and 16 have local browser/native coverage for the implemented flows; this does not assert every possible interleaving.
- UI-AC-07–08 and 11 combine browser role/board/presence checks with existing native/Worker lifecycle, generation, qualification and race tests.
- UI-AC-13 has original-command, identity/expiry, covered-frame, queue cancellation and real replacement tests. Unknown outcomes remain fail-closed rather than being retried under a new ID.
- UI-AC-15 uses the approved read-only History projections and fixed server expiry. Browser inspection is complemented by the existing owner/edge retention tests, not a months-long browser soak.
- UI-AC-17 has keyboard/modal focus, semantic labels, 320px large-board scrolling, non-color status and design-token contrast checks. Browser/assistive-device matrix, real audible speech and physical venue-distance readability are not certified.
- Structural code graph output is source-hashed, not runtime evidence. Its unresolved/collapsed AST references and the separately stale documentation graph are not hidden by a successful refresh.

No deployment, remote account/namespace/domain provisioning, secret provisioning, commit or push was performed. Frontend promotion integration and the seven existing external provider/security/load/retention/restore gates remain unverified and unauthorized by this implementation task. Historical failed attempts remain evidence; a retry pass is not a claim that every earlier failure's cause was proved.
