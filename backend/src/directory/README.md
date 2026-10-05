# Minimal Directory account-removal gate

This slice exposes only private removal coordination, not game creation,
discovery, lifecycle, host-management or browser APIs. Bind its database to the
isolated environment's `GAME_DIRECTORY` object named `directory`; do not run the
Accounts migration against this database.

Directory errors and coordination types live in `mod.rs`. Import the service and
migration entry point directly from `crate::db::directory`; their implementation
lives in `../db/directory.rs`.

## Integration

```rust,ignore
use crate::db::directory::{DirectoryService, migrate_directory};

migrate_directory(db)?;
let directory = DirectoryService::new(db, runtime)?;
let grant = directory.acquire_removal(operation_id, account_id)?;
// Accounts commits the previously authorized disable/delete intent locally.
let ack = directory.release_removal(grant.operation_id, grant.account_id)?;
```

`Runtime` and `Database` are the existing backend ports. Acquiring returns
`RemovalGrant`; releasing or explicitly reconciling returns `RemovalReleaseAck`.
Both carry the exact typed
`OperationId` and `AccountId`. They are coordination acknowledgements, not
independent proof of actor authority. Accounts must durably persist and authorize
its removal intent **before** peer delivery, revalidate before its local account
mutation, and retain release work until the matching acknowledgement. Recovery
scheduling (1-second initial delay, 5-minute cap) belongs to Accounts, not this
receiver. The Worker-only adapter owns trusted routing and missing-binding errors.

### Explicit abort/lost-ACK reconciliation

```rust,ignore
// Trusted Accounts only: persist Aborting or Committed before sending this call.
let ack = directory.reconcile_removal(operation_id, account_id)?;
// Accounts consumes the exact reply only for abort_removal or finish_removal.
```

`reconcile_removal(operation_id: OperationId, account_id: AccountId)` returns
`Result<RemovalReleaseAck, DirectoryError>`. It is a separate cancellation/status
fence, **not** a relaxed acquire/release endpoint. Accounts must persist `Aborting`
before cancellation (including a hosted-game rejection or uncertain acquire
result), or have already persisted `Committed` before lost-release-ACK recovery.
It must never use reconciliation in `Prepared` or race cancellation against a
still-authorized commit. Directory cannot inspect Accounts' database; the private
Worker adapter must enforce this phase precondition and exact response pairing.

- Matching pending work validates its durable timestamp and gate, then atomically
  releases that gate and writes a completion receipt, even for long-lived intents.
- Matching released or hosted-rejected work can ACK absence. Rejected work is
  sealed as completed, without changing the hosted-game index or another gate.
- A fresh unknown intent writes a bounded completion receipt using trusted runtime
  time. A delayed acquire then returns `Completed`, not an orphaned gate.
- After receipts are purged, an unknown intent at/below the durable nonretreating
  floor can ACK absence without a new registry row: future acquire is inadmissible.
  An active gate belonging to another validated operation is preserved.
- Every retained row for the requested operation is validated **before** compaction,
  including expired rows: target mismatch rejects while evidence remains;
  `issued <= recorded_at <= now` and valid account identity are required. Conflicting
  state rows, orphan gates or missing/malformed pending metadata fail closed.

A reconciliation ACK proves only that this operation no longer holds, and cannot
newly acquire, an assignment gate. It is **not** an account-mutation grant or a
no-hosted-games proof: a hosted rejection can ACK safe abort while games still
exist. No public/browser or Gameplay ingress is authorized for reconciliation.
Normal unknown/future/stale acquire/release behavior is unchanged.

### Receipt retention scheduling

```rust,ignore
// The Worker adapter schedules the returned trusted epoch-ms deadline.
let deadline = directory.next_deadline()?;
// At the alarm, even without another acquire/release request:
directory.cleanup()?;
let next = directory.next_deadline()?;
```

`next_deadline(&self) -> Result<Option<i64>, DirectoryError>` observes the trusted
clock and returns the earliest receipt/rejection completion plus the shared
24-hour retention period. An already-expired backlog returns the current trusted
time, not `None`; the adapter must schedule another bounded cleanup pass. The
query does not delete retained evidence. Empty completion tables return `None`;
pending intents and assignment gates never create TTL deadlines.

`cleanup(&self) -> Result<(), DirectoryError>` observes the clock and advances the
persistent nonretreating admission floor in the same transaction as compaction.
It deletes at most `CLEANUP_BATCH_SIZE` (100) validated due receipts **and** 100 due
rejections per call, never pending rows, gates, or hosted assignments. Due rows
must have canonical typed operation/account identities and
`0 < issued <= completed_at <= now` before deletion. Clock/metadata errors,
malformed due rows, and SQL failures roll back both batches and the floor update.
Deadline addition is checked against the JavaScript-safe millisecond ceiling.
Scheduling/deleting actual Worker alarms and rescheduling the backlog belong to
the Worker adapter; no schema version or columns change for these additive APIs.

## Durable invariants

- `account_assignment_gates` has only its `account_id` key. Presence blocks
  assignments; no operation ID, timestamp, generation or phase was added.
- Separate owner-local pending rows bind one operation to one account. Matching
  acquire retries preserve the gate; another operation for the account is `Busy`.
- The first acquire writes the gate and checks the hosted-nonterminal index in
  one serialized transaction. A hosted assignment clears only that acquire's
  gate and records an idempotent rejection; no account mutation is authorized.
- Release requires the exact pending operation/account and a present gate.
  Completed-release receipts replay the same typed acknowledgement without
  touching a subsequently acquired gate. Unknown releases never produce ACKs.
- Pending intents never expire or get abandoned. Completed receipts/rejections
  have 24-hour retention, with bounded validated compaction and a persistent
  admission floor. UUID-v7 time bounds **new** operation admission only; trusted
  runtime time governs storage and retention. Old operations cannot resurrect
  after receipt deletion. A known long-running pending operation remains
  recoverable beyond the fresh-admission window.
- SQL errors, missing or unsupported schema, uncertain gate metadata and invalid
  or regressed trusted clocks fail closed. Operational failures roll back the
  entire local transaction, including the clock observation and receipt writes.

## Nonterminal index boundary

`directory_hosted_nonterminal_games` is a deliberately private minimal index:
`game_id` and `designated_host_id`, with row presence representing a nonterminal
assignment. It is not the full GameIndexRecord, a public projection, or an
assertion that a separately deployed game namespace is empty.

Fresh initialization is permitted only for an empty isolated application database.
Only the exact verified runtime-owned tables `__miniflare_do_name` (local DO name metadata) and `_cf_METADATA` (Workerd metadata created when alarms are persisted) are excluded from application-table checks. Their data is untouched. This does not whitelist `__miniflare_*`, `_cf_*`, `_cf_KV`, or any other unobserved provider table. Other unrecognized or partial
application storage is rejected at initialization, service construction and each transaction, never interpreted as an empty game proof. Existing databases must contain exactly the approved Directory application-table inventory; adding an unrelated legacy table fails closed even when the stored schema version is supported.
This auth/CLI-only slice has **no production game ingress**. The sole assignment
writer is compiled only for tests and checks gate absence and changes the index
within the same transaction.

Before introducing any game creation or transfer, **all** assignment ingress
must be wired to this serialized gate/index boundary and the approved authoritative
Game/Accounts revision, eligibility and delivery protocols. An eventually delivered
host projection alone cannot establish the removal guard's freshness. This slice
does not implement those future game protocols or authorize their implementation.

Native tests compile the owned module by path to exercise the test-only assignment
tracer without exposing an assignment method from the production library. The
module is also exported by the parent integration. The parent owns private typed
Acquire/Release/Reconcile HTTP routing, exact operation/account response-pair verification,
alarm-driven Accounts recovery, Directory retention alarms, and actual Worker/Wasm verification.
