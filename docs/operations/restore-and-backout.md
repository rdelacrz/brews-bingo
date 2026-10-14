# Expiry-preserving restore and compatible backout worksheet

**Status:** locally documented Slice 9 procedure, not provider-validated or authorized for execution. No restore/import/export/deployment/provisioning is performed by this document. A supported Cloudflare mechanism, operator ownership, isolated namespace and explicit authorization must be established before any provider action. Do not improvise provider commands from this worksheet.

Policies: [DO-075/090/099/103/104](../plans/durable-object-design.md), [LLD Sections 10–11](../plans/lld.md), [remaining gates](../plans/PENDING.md#slice-9--production-databaseruntime-and-release-gates). Provider physical deletion, backups, secondary copies and log retention are unverified. Local SQLite/Workerd tests do not establish these controls.

## 1. Quarantine and prerequisites

1. Obtain separate scope authorization for the exact source copy, destination environment, allowed operations and any credential/bootstrap changes. Keep the destination isolated from application traffic, account cookies, participant sockets and public routing. Never restore directly into a serving namespace.
2. Verify a supported provider restore mechanism and namespace/class/binding isolation with actual account-specific evidence. Stop if the provider cannot preserve required metadata or enforce retention on the physical copies themselves.
3. Record privately: provenance, snapshot cut, source/destination identity, schema counters, exact worker/toolchain/dependency/artifact versions, integrity checks, a **trusted current UTC cut** and independently retained authority/clock/command-floor evidence. Do not put database dumps, credentials, verifiers, answers, private snapshots, bearer URLs or secret-bearing logs into Git, graph labels, build/CI caches or this runbook.
4. A restored historical `last_observed_ms` is not proof of a current trusted clock or of the latest durable denial. If trusted time, the latest expiry fences, credential epochs, revocations or original immutable deadlines cannot be proved, remain quarantined. A simultaneous old backup + rolled-back trusted clock is not covered by the local rollback-fence tests.
5. Maintain a no-write/no-exposure hold while evaluating compatibility. Unknown/newer/mixed schema, drifted DDL/index inventory, broken foreign keys or failed/ignored migration writes must fail closed. No partial schema promotion or destructive down-migration.

## 2. Original retention, never renewed

| Data | Required restore treatment |
| --- | --- |
| Immutable started-game History | Preserve original `started_at`, `ended_at` and `expires_at`. Expiry is the approved UTC **three calendar months**, including time-of-day/month-end clamping; it is not 90 elapsed days or three months from restore. Deny at equality. |
| Game and Accounts privileged audit | Preserve original actor/target/operation/outcome/occurrence and expiry. Fixed **90 elapsed days from original occurrence**; never from migration, read, restore, account deletion or game deletion. Deny at equality. |
| Directory terminal index/routing copies | Preserve original Game identity/fingerprint/revision/deadline. Discard expired/stale copies without deleting a newer game/code/reservation/host assignment. An index is not a snapshot, live grant or proof of authority. |
| Credentials, grants and recovery | Preserve fixed original expiries and prove **current** epoch/revocation/Exit/session state. Never recreate deleted membership/recovery proofs, revive a superseded session, issue a credential from a receipt, extend a grant or make a restored expired spectator live. |
| Pending coordination and receipts | Preserve exact owner operation/phase/semantic binding and command floors. Pending transfer/release work has no invented TTL. Expired receipt absence does not prove that repeating effects is safe. |

Before exposure, discard expired History/audit material and stale derived indexes/copies using verified owner lifecycle/expiry logic under trusted time. Purge is not the read authorization mechanism: owner and receiving-edge expiry denial must already hold while cleanup is incomplete. Never copy History into audit/logs, renew its expiry or use it to issue live grants.

Local implementations to inspect and exercise on a separately authorized isolated destination:

- `backend/src/db/game/{migration,terminal,history,operator}.rs`: exact migration, original History/audit deadlines, read denial, expiry cleanup and durable rollback fences; audit-only owners can remain after domain deletion.
- `backend/src/db/directory_games.rs`: original minimal terminal routing/index metadata and guarded expiry removal.
- `backend/src/db/game_delivery.rs` and actual native socket handlers: logical attachment/current-connection authority, independent Delivery cleanup and fail-closed reconstruction.
- Accounts owner/session/management storage: current lifecycle, disablement, credential epoch and audit rules.

There is no general public restore endpoint or developer-CLI database restore command. Do not add one or substitute manual live SQL because this worksheet lists internal files. If bounded cleanup, alarms or cross-owner completion cannot be verified through the separately approved provider procedure, stop.

## 3. Authority and cross-owner reconciliation

A backup of Accounts can resurrect a password/session revoked **after** its snapshot, even if its original expiry has not elapsed. Do not silently activate historical authentication state. If current epochs/revocations cannot be reconciled, obtain a separately reviewed invalidation/re-authentication plan rather than inventing epoch increments or reissuing sessions during restore.

Reconcile Accounts, Directory and each Game against authoritative current evidence, not an old Directory projection alone. Check global reservation/creator assignment, terminal release, host-transfer prepared/committed/aborted outcomes, account removal gates, retained receipts/floors and pending exact compensation. Use the existing bounded private proof/ACK and alarm/outbox rules; no synthetic ACK, deleted pending intent, guessed timeout, fresh assignment/admission or duplicate original mutation.

Directory may refer to expired or missing History; denial must remain safe before eventual guarded index cleanup. A terminal code may have been reused: an old snapshot must not reclaim it or delete its new routing. Reconstruct sockets/current grants only under existing exact current authority; restoring attachment bytes alone grants no membership, final view or presence.

## 4. Isolated acceptance matrix

Record real commands, expected/actual results, target/runtime, observed cuts, exact source/artifact hashes and every failed attempt. Do not fabricate missing provider measurements.

- Expiry: before boundary, exactly at boundary, after boundary; empty pages; cleanup incomplete; restart/hibernation; delayed owner/edge replies; durable expiry denial followed by clock rollback.
- Copies: expired rows and children, stale Directory copies, audit-only owner after game deletion; original deadlines/actors retained; no secret disclosure or retention renewal.
- Authority: post-snapshot disable/reset/remove/recovery/Exit/replacement; old credentials/proofs/attachments denied; no fresh grant from History/index/receipt.
- Coordination: lost committed ACK, undelivered abort, retained pending work beyond ordinary receipt retention, terminal release while transfer/removal is pending; reconstruction cannot repeat effects or free another reservation.
- Migrations: exact supported historical/current templates, newer/unknown/drift rejection, transactional ignored/aborted required-write rollback and compatible worker/schema pairing at the restored population.
- Load: actual maximum legal game **plus the owner-provided peak event/History forecast**, per Object storage, row reads/writes, requests, wall duration/provider CPU, alarms and WSS amplification. Reverify applicable account/plan quotas and require measured use at or below half each relevant quota; otherwise stop for architecture/cost review. Local VM counters or one sample cannot stand in for these metrics.

The user separately approved [authorization-time snapshots](../plans/protected-response-policy.md), and current `RUNTIME-FINAL-01` acceptance passes in the [CI/CD continuation](../reviews/cicd-snapshot-2026-10-14.md). A restore cannot change those semantics, renew expiry or authorize subsequent revoked requests; the old receiving-cut failure remains historical evidence. The older Slice 2 review and final Slice 6/7 acceptance remain open until separately completed, as do actual provider restore and production gates.

## 5. Compatible backout / corrective forward fix

- Read the actual serving schema counters and validate exact inventories before selecting a worker. Storage counters are metadata, not application release names.
- Back out only to a worker actually verified to understand and safely write the **current** schemas. Restoring an older worker does not undo already migrated storage. The published audit schema at counter 8 must not be presented to a counter-7-only worker as if it were compatible.
- Prefer a reviewed corrective forward migration/worker when backward compatibility is not established. No destructive down-migration, secret/authority reset, deadline renewal or reinsertion of expired data.
- Preserve truthful committed outcomes/pending work. Do not repeat accepted mutations to compensate for a lost response. Keep incident evidence private and secret-free.
- Reopen traffic only after isolated restore, reconciliation, expiry/authority checks, applicable production capacity/security gates and explicit operator authorization are all complete. Verify the exact external target after any authorized write; a provider's success response is not acceptance evidence.

## Unfinished external prerequisites

Supported physical-copy/backup/log controls and deletion, target namespace/account isolation, trusted-time/authority evidence retention, real provider restore execution, owner peak forecast, capacity/quota/headroom measurement, deployed KDF/runtime validation, operator/bootstrap/secret procedures and deployment/backout drills remain pending. No current credentials or provider identifiers are invented here.
