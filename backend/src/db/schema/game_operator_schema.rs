//! Saved operator commands, unresolved assignment proofs and owner-local privileged audit.
use crate::limits::JS_SAFE_INTEGER_MAX;
pub(crate) fn statements() -> Vec<String> {
    use crate::db::game::{OperatorAuditOutcome, OperatorPurpose};
    use crate::game::TransferPhase;
    let purposes = [
        OperatorPurpose::Configuration,
        OperatorPurpose::Resume,
        OperatorPurpose::HostTransfer,
        OperatorPurpose::HostTransferTargets,
    ]
    .map(|p| format!("'{}'", p.tag()))
    .join(",");
    let outcomes = [
        OperatorAuditOutcome::Succeeded,
        OperatorAuditOutcome::Rejected,
        OperatorAuditOutcome::Failed,
    ]
    .map(|p| format!("'{}'", p.tag()))
    .join(",");
    let phases = [
        TransferPhase::Prepared,
        TransferPhase::Committed,
        TransferPhase::Aborted,
    ]
    .map(|p| format!("'{}'", p.tag()))
    .join(",");
    let max = JS_SAFE_INTEGER_MAX;
    let retry_max = crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT;
    let audit_retention_ms = crate::game::GAME_ADMIN_AUDIT_RETENTION_MS;
    vec![
        "CREATE TABLE game_operator_identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),game_id TEXT NOT NULL UNIQUE) STRICT".into(),
        format!(
            "CREATE TABLE game_admin_audit(audit_id TEXT PRIMARY KEY,actor_account_id TEXT NOT NULL,target_game_id TEXT NOT NULL,operation TEXT NOT NULL CHECK(operation IN ({purposes})),outcome TEXT NOT NULL CHECK(outcome IN ({outcomes})),occurred_at INTEGER NOT NULL CHECK(occurred_at BETWEEN 0 AND {max}),expires_at INTEGER NOT NULL CHECK(expires_at BETWEEN 0 AND {max}),CHECK(expires_at=occurred_at+{audit_retention_ms})) STRICT"
        ),
        "CREATE INDEX game_admin_audit_expiry ON game_admin_audit(expires_at,audit_id)".into(),
        format!(
            "CREATE TABLE game_host_transfers(operation_id TEXT PRIMARY KEY,game_id TEXT NOT NULL,actor_account_id TEXT NOT NULL,command_id TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),old_host_id TEXT NOT NULL,target_account_id TEXT NOT NULL,expected_assignment_revision INTEGER NOT NULL CHECK(expected_assignment_revision BETWEEN 0 AND {max}),prepared_game_revision INTEGER NOT NULL CHECK(prepared_game_revision BETWEEN 0 AND {max}),nonce BLOB NOT NULL CHECK(length(nonce)=32),phase TEXT NOT NULL CHECK(phase IN ({phases})),next_attempt_at INTEGER NOT NULL CHECK(next_attempt_at BETWEEN 0 AND {max}),retry_attempts INTEGER NOT NULL CHECK(retry_attempts BETWEEN 0 AND {retry_max}),UNIQUE(actor_account_id,command_id)) STRICT"
        ),
        "CREATE INDEX game_host_transfer_retry ON game_host_transfers(next_attempt_at)".into(),
    ]
}
