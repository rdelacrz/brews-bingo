# Backend `/ponytail` scan — 2026-10-17

Scanned published commit **`f667ad36dbcfc5f7e7d40244d8d1a8fdb53aa19f`**, after its successful push and exact remote `main` readback. The commit groups game discovery reads and moves game-code/player-recovery URLs under `/api/games`. This subsequent scan is read-only: findings and evidence are local, uncommitted, and not fixes or release acceptance.

## Confirmed findings

### 1. PT-HISTORY-EXPIRY — account-expiry denial does not fence the final local History sample (high)

**Locations:** `backend/src/db/game/history.rs:35–67`, `backend/src/worker_runtime/history/owner.rs:42–59`.

When the final Accounts proof is still valid at its receiving check but the immediately following local History cut observes account expiry, `authorize` returns `Unauthorized` before the History-expiry fencing branch. The owner returns that denial without persisting the account-expiry sample. Neither owner need have durably observed that final timestamp.

**Real Workerd reproduction:** obtain both genuine private Accounts proofs just before the original account expiry, cross expiry only at the local final History cut, receive the real `unauthorized` outcome, then roll clocks back by one millisecond. Both durable floors remain at expiry minus one. A fresh **public History detail request returns HTTP 200** with the same session. The initial denial itself disclosed no History; the defect is subsequent revival after an already observed expiry. This is not the superseded receiving-cut revocation policy.

**Small correction direction:** durably retain the exact observed account-expiry denial, using the existing owner/Accounts fencing mechanisms and required sync before denial completes. Success must not acquire an extra unvalidated await. Add the boundary/rollback regression.

The first diagnostic expired the proof before its receiving check and correctly failed closed through the existing Accounts fence. The refined probe isolates the later local cut; the first successful guard is preserved as a separate result, not hidden or presented as this bug.

### 2. PT-READ-EXPIRY-SAMPLE — dedicated release fences the wrong clock sample (medium; native service scope)

**Location:** `backend/src/db/game/reads.rs:519–539`.

`release_read` samples the clock outside the transaction, samples again inside, and on an authorization/expiry denial fences the **outer**, earlier sample. If expiry is crossed between the samples, the actual denial observation is lost.

**Native reproduction:** outer sample is expiry minus one; inner sample is expiry. The real method denies the release, but stores expiry minus one. After rollback to that value, a fresh owner-service read succeeds. The invariant test fails with the actual SQLite adapter and unmodified production implementation in an isolated source copy.

**Small correction direction:** carry the inner sampled timestamp out with the denial, or fence that timestamp in its transaction. Preserve rollback/readback checks and the existing non-extending binding cap.

**Boundary:** this proves the service-level defect, not a complete public HTTP exploit. Worker adapters have additional denial recuts/fences that can mask some interleavings; that distinction must remain in any follow-up.

### 3. PT-RECOVERY-CAP — recovery output can replace its original cap after host resume (medium)

**Locations:** `backend/src/worker_runtime/games.rs:492–535` and `:537–595`; current deadlines are recomputed by `backend/src/db/game/player_recovery.rs:487–493`.

Recovery-answer output shadows the initial `valid_until` with the second reply's value. Recovered-session output checks the original cap before its final owner await, then checks only the newly returned cap afterwards. Neither preserves the initial minimum across those replies.

**Real Workerd reproductions:** after the first genuine reply, perform a legitimate host `resume` before the original lobby idle deadline, then release the second reply exactly at the original cap. Both requests return **HTTP 200**; recovery also emits its player cookie. The later deadline is greater than the original cap. No expired session lifetime was extended—the violated constraint is the original response's non-extending idle cap.

**Small correction direction:** keep the first deadline and use the minimum at every later receiving cut, with the existing exact-sample expiry fence. Do not remove proof revalidation or change renewal policy for genuinely new requests.

### 4. PT-RECOVERY-PRESENCE — recovery removes presence without advancing the host revision (medium)

**Locations:** `backend/src/db/game/player_recovery.rs:285–365`, `backend/src/db/game/player_lifecycle.rs:73–137`, `backend/src/db/game.rs:2160–2176`.

Recovery deletes the player's current connection and advances its private revision, but does not advance `HOST_VIEW` for that presence change. Later close reconciliation cannot recover the revision change from a connection row already deleted.

**Real Workerd reproduction:** admit a real upgraded player socket, read the host snapshot, recover the player, then compare full versus conditional host Sync. Connected count changes **1 → 0** while all host revisions remain **4**. Conditional Sync returns **`up_to_date: true` and no snapshot**. A host UI can retain stale presence until another revision-changing event.

**Small correction direction:** advance the host revision when recovery actually removes visible player presence; retain epoch rotation, durable socket fencing and private revision behavior. Cover connected and already-disconnected recovery without unnecessary revision churn.

## Worthwhile small cleanup candidates

These are secondary to the correctness findings. None was applied.

- **Reuse an existing helper:** `worker_runtime/game_accounts.rs:208–215` reconstructs the same proof already handled by `AccountAuthorityWire::proof` at `:127–135`. Call `wire.proof()?`; leave reply/connection binding and expiry checks in place.
- **Remove an exact repeated predicate:** `db/directory_host_transfer.rs:93–95` repeats the immutable projection validation already executed at `:75–77`, before any replay/tombstone branching. Keep the first validation, prepared-binding check, projection application and durable readback.
- **Process expired final grants once:** `db/game/terminal.rs:659–691` deletes expired grants in one loop and processes/deletes those same captured rows again in the next. Consolidate only after preserving anchor/eligibility semantics, corruption handling and subsequent current-connection pruning.
- **Avoid redundant work:** `db/directory_host_transfer.rs:181` parses the same readback twice; `db/game/gameplay.rs:636–644` round-trips an already canonical, bounded digit string through `to_string`. These are local reductions, not reasons for a generic framework.

Repeated security checks, post-await authorization, invocation binding, expiry/rollback fencing, SQL readbacks, selective auditing and retention coordination are not cleanup targets merely because they repeat.

## Verification and limits

- Three bounded supplied-source static reviews covered production prefixes in **64 files** across authentication/management, gameplay/membership, read/delivery and Directory/History coordination. They did not execute tools/tests or establish whole-slice acceptance. Parent verification inspected selected Worker/API paths, traced graph relationships, mechanically checked duplicate bodies, and exercised the findings.
- The combined fresh local Workerd diagnostic runs **four regressions: four fail, zero runtime errors, zero skips**. They represent three findings because the cap bug is exercised by both recovery and answer management. The isolated native diagnostic adds **one failing regression** for the fourth finding. These are genuine newly uncovered failures, not passing acceptance evidence.
- Production source/release-input hashes and Worker artifact hashes stayed identical to the published candidate. The native probe modifies only an extra test module in a private `git archive` copy, with the production `release_read` implementation prefix verified byte-identical. Existing repository tests were not weakened or changed.
- [Machine-readable evidence](backend-ponytail-scan-2026-10-17-evidence.json) preserves exact probe sources/config, safe execution descriptors, observations, earlier diagnostic cuts, static reports, source hashes and artifact bindings. Raw child output and runtime credentials are not persisted.
- The earlier all-lane gate passed with its then-existing coverage; this scan adds boundary cases it did not cover. Neither that result nor final slice sign-offs are retroactively reclassified as passing these new regressions.
- This is not an exhaustive migration/schema audit, provider test, deployment, load/restore certification or formal proof. The dedicated-read finding remains native service-level. The separate documentation graph remains stale.

No fixes, later commit/push, deployment or provider-state change was performed after the scan began. New implementation and publication need separate authorization.
