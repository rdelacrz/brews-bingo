//! Directory owner schema. Removal-only storage is upgraded in one transaction.
use crate::db::{Database, StorageError};
use crate::directory::games::CREATION_FINGERPRINT_BYTES;

pub const REMOVAL_SCHEMA_VERSION: i64 = 1;
pub const DIRECTORY_SCHEMA_VERSION: i64 = 3;
pub const REMOVAL_TABLES: [&str; 6] = [
    "account_assignment_gates",
    "directory_hosted_nonterminal_games",
    "directory_metadata",
    "directory_removal_pending",
    "directory_removal_receipts",
    "directory_removal_rejections",
];
pub const DIRECTORY_TABLES: [&str; 11] = [
    "account_assignment_gates",
    "directory_game_creations",
    "directory_game_index",
    "directory_global_reservation",
    "directory_host_transfers",
    "directory_hosted_nonterminal_games",
    "directory_metadata",
    "directory_removal_pending",
    "directory_removal_receipts",
    "directory_removal_rejections",
    "directory_retired_creations",
];

pub fn removal_statements() -> Vec<String> {
    vec![
        "CREATE TABLE directory_metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,last_observed_ms INTEGER NOT NULL,command_floor_ms INTEGER NOT NULL) STRICT".into(),
        "CREATE TABLE account_assignment_gates(account_id TEXT PRIMARY KEY) STRICT".into(),
        "CREATE TABLE directory_removal_rejections(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL,completed_at INTEGER NOT NULL) STRICT".into(),
        "CREATE TABLE directory_removal_receipts(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL,completed_at INTEGER NOT NULL) STRICT".into(),
        "CREATE TABLE directory_hosted_nonterminal_games(game_id TEXT PRIMARY KEY,designated_host_id TEXT NOT NULL) STRICT".into(),
        "CREATE INDEX directory_hosted_nonterminal_by_host ON directory_hosted_nonterminal_games(designated_host_id)".into(),
        "CREATE TABLE directory_removal_pending(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL UNIQUE REFERENCES account_assignment_gates(account_id),created_at INTEGER NOT NULL) STRICT".into(),
    ]
}
pub fn game_coordination_statements() -> Vec<String> {
    vec![
        "CREATE TABLE directory_global_reservation(game_id TEXT) STRICT".into(),
        "CREATE UNIQUE INDEX directory_one_global_reservation ON directory_global_reservation((1))".into(),
        format!("CREATE TABLE directory_game_creations(account_id TEXT NOT NULL,command_id TEXT NOT NULL,game_id TEXT NOT NULL UNIQUE,fingerprint BLOB NOT NULL CHECK(length(fingerprint)={CREATION_FINGERPRINT_BYTES}),created_at INTEGER NOT NULL,deadline INTEGER NOT NULL,next_retry_at INTEGER,attempts INTEGER NOT NULL,ready_revision INTEGER,completed_at INTEGER,PRIMARY KEY(account_id,command_id)) STRICT"),
        format!("CREATE TABLE directory_game_index(game_id TEXT PRIMARY KEY,game_code TEXT UNIQUE,designated_host_id TEXT NOT NULL,state TEXT NOT NULL,source_revision INTEGER NOT NULL,publication_state TEXT NOT NULL,created_at INTEGER NOT NULL,started_at INTEGER,ended_at INTEGER,history_expires_at INTEGER,fingerprint BLOB NOT NULL CHECK(length(fingerprint)={CREATION_FINGERPRINT_BYTES})) STRICT"),
        format!("CREATE TABLE directory_retired_creations(account_id TEXT NOT NULL,command_id TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)={CREATION_FINGERPRINT_BYTES}),completed_at INTEGER NOT NULL,PRIMARY KEY(account_id,command_id)) STRICT"),
        "CREATE INDEX directory_game_index_by_host ON directory_game_index(designated_host_id)".into(),
        "CREATE INDEX directory_game_index_by_state ON directory_game_index(state)".into(),
        "CREATE INDEX directory_game_index_by_expiry ON directory_game_index(history_expires_at) WHERE history_expires_at IS NOT NULL".into(),
    ]
}
pub fn host_transfer_statements() -> Vec<String> {
    use crate::game::TransferPhase;
    let phases = [
        TransferPhase::Prepared,
        TransferPhase::Committed,
        TransferPhase::Aborted,
    ]
    .map(|p| format!("'{}'", p.tag()))
    .join(",");
    let max = crate::limits::JS_SAFE_INTEGER_MAX;
    vec![
        format!(
            "CREATE TABLE directory_host_transfers(operation_id TEXT PRIMARY KEY,game_id TEXT NOT NULL,actor_account_id TEXT NOT NULL,command_id TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),old_host_id TEXT NOT NULL,target_account_id TEXT NOT NULL,expected_assignment_revision INTEGER NOT NULL,prepared_game_revision INTEGER NOT NULL,nonce BLOB NOT NULL CHECK(length(nonce)=32),phase TEXT NOT NULL CHECK(phase IN ({phases})),next_attempt_at INTEGER NOT NULL,retry_attempts INTEGER NOT NULL CHECK(retry_attempts BETWEEN 0 AND {}),completion_acknowledged_at INTEGER CHECK(completion_acknowledged_at IS NULL OR completion_acknowledged_at BETWEEN 0 AND {max})) STRICT",
            crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT
        ),
        "CREATE INDEX directory_transfer_guard_by_game ON directory_host_transfers(game_id,phase)"
            .into(),
    ]
}
fn install<D: Database>(db: &D, statements: Vec<String>) -> Result<(), StorageError> {
    for sql in statements {
        db.execute(&sql, &[])?;
    }
    Ok(())
}
pub fn initialize_removal<D: Database>(db: &D) -> Result<(), StorageError> {
    install(db, removal_statements())
}
pub fn add_game_coordination<D: Database>(db: &D) -> Result<(), StorageError> {
    install(db, game_coordination_statements())?;
    db.execute(
        "INSERT INTO directory_global_reservation(game_id) VALUES(NULL)",
        &[],
    )?;
    Ok(())
}
pub fn add_host_transfer_coordination<D: Database>(db: &D) -> Result<(), StorageError> {
    install(db, host_transfer_statements())
}
