//! Directory owner schema. Removal-only storage is upgraded in one transaction.
use crate::db::{Database, StorageError};
use crate::directory::games::CREATION_FINGERPRINT_BYTES;

pub const REMOVAL_SCHEMA_VERSION: i64 = 1;
pub const DIRECTORY_SCHEMA_VERSION: i64 = 2;
pub const REMOVAL_TABLES: [&str; 6] = [
    "account_assignment_gates",
    "directory_hosted_nonterminal_games",
    "directory_metadata",
    "directory_removal_pending",
    "directory_removal_receipts",
    "directory_removal_rejections",
];
pub const DIRECTORY_TABLES: [&str; 10] = [
    "account_assignment_gates",
    "directory_game_creations",
    "directory_game_index",
    "directory_global_reservation",
    "directory_hosted_nonterminal_games",
    "directory_metadata",
    "directory_removal_pending",
    "directory_removal_receipts",
    "directory_removal_rejections",
    "directory_retired_creations",
];

pub fn initialize_removal<D: Database>(db: &D) -> Result<(), StorageError> {
    for sql in [
        "CREATE TABLE directory_metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,last_observed_ms INTEGER NOT NULL,command_floor_ms INTEGER NOT NULL) STRICT",
        "CREATE TABLE account_assignment_gates(account_id TEXT PRIMARY KEY) STRICT",
        "CREATE TABLE directory_removal_rejections(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL,completed_at INTEGER NOT NULL) STRICT",
        "CREATE TABLE directory_removal_receipts(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL,completed_at INTEGER NOT NULL) STRICT",
        "CREATE TABLE directory_hosted_nonterminal_games(game_id TEXT PRIMARY KEY,designated_host_id TEXT NOT NULL) STRICT",
        "CREATE INDEX directory_hosted_nonterminal_by_host ON directory_hosted_nonterminal_games(designated_host_id)",
        "CREATE TABLE directory_removal_pending(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL UNIQUE REFERENCES account_assignment_gates(account_id),created_at INTEGER NOT NULL) STRICT",
    ] {
        db.execute(sql, &[])?;
    }
    Ok(())
}

pub fn add_game_coordination<D: Database>(db: &D) -> Result<(), StorageError> {
    db.execute(
        "CREATE TABLE directory_global_reservation(game_id TEXT) STRICT",
        &[],
    )?;
    db.execute(
        "CREATE UNIQUE INDEX directory_one_global_reservation ON directory_global_reservation((1))",
        &[],
    )?;
    db.execute(
        "INSERT INTO directory_global_reservation(game_id) VALUES(NULL)",
        &[],
    )?;
    db.execute(&format!("CREATE TABLE directory_game_creations(account_id TEXT NOT NULL,command_id TEXT NOT NULL,game_id TEXT NOT NULL UNIQUE,fingerprint BLOB NOT NULL CHECK(length(fingerprint)={CREATION_FINGERPRINT_BYTES}),created_at INTEGER NOT NULL,deadline INTEGER NOT NULL,next_retry_at INTEGER,attempts INTEGER NOT NULL,ready_revision INTEGER,completed_at INTEGER,PRIMARY KEY(account_id,command_id)) STRICT"), &[])?;
    db.execute(&format!("CREATE TABLE directory_game_index(game_id TEXT PRIMARY KEY,game_code TEXT UNIQUE,designated_host_id TEXT NOT NULL,state TEXT NOT NULL,source_revision INTEGER NOT NULL,publication_state TEXT NOT NULL,created_at INTEGER NOT NULL,started_at INTEGER,ended_at INTEGER,history_expires_at INTEGER,fingerprint BLOB NOT NULL CHECK(length(fingerprint)={CREATION_FINGERPRINT_BYTES})) STRICT"), &[])?;
    db.execute(&format!("CREATE TABLE directory_retired_creations(account_id TEXT NOT NULL,command_id TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)={CREATION_FINGERPRINT_BYTES}),completed_at INTEGER NOT NULL,PRIMARY KEY(account_id,command_id)) STRICT"), &[])?;
    db.execute(
        "CREATE INDEX directory_game_index_by_host ON directory_game_index(designated_host_id)",
        &[],
    )?;
    db.execute(
        "CREATE INDEX directory_game_index_by_state ON directory_game_index(state)",
        &[],
    )?;
    db.execute("CREATE INDEX directory_game_index_by_expiry ON directory_game_index(history_expires_at) WHERE history_expires_at IS NOT NULL", &[])?;
    Ok(())
}
