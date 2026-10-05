#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::storage::{Database, SqlValue, StorageError, migrate};
use support::Sqlite;

#[test]
fn migration_is_atomic_idempotent_and_refuses_newer_versions() {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    migrate(&db).unwrap();
    assert_eq!(
        db.query(
            "SELECT schema_version FROM storage_metadata WHERE singleton=1",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    db.execute(
        "UPDATE storage_metadata SET schema_version=2 WHERE singleton=1",
        &[],
    )
    .unwrap();
    assert_eq!(migrate(&db), Err(StorageError));
}

#[test]
fn migration_preserves_initialized_version_one_ddl_and_clock_metadata() {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    db.execute(
        "UPDATE storage_metadata SET last_observed_ms=123,command_floor_ms=100 WHERE singleton=1",
        &[],
    )
    .unwrap();
    let before = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    migrate(&db).unwrap();
    assert_eq!(
        db.query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
            .unwrap(),
        before
    );
    assert_eq!(
        db.query(
            "SELECT last_observed_ms,command_floor_ms FROM storage_metadata WHERE singleton=1",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(123), SqlValue::Integer(100)]]
    );
}

#[test]
fn failed_version_one_initialization_rolls_back_metadata_and_partial_ddl() {
    let db = Sqlite::new();
    db.execute("CREATE TABLE accounts(existing INTEGER)", &[])
        .unwrap();
    let before = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    assert_eq!(migrate(&db), Err(StorageError));
    assert_eq!(
        db.query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
            .unwrap(),
        before
    );
}

#[test]
fn sql_constraints_preserve_control_bytes_and_reject_unsafe_values() {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let username = "User\0name\x7f1";
    let insert = |id: &str, name: &str, created: i64| {
        db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?,'host','pending_enrollment',NULL,0,?,NULL,NULL)",&[SqlValue::Text(id.into()),SqlValue::Text(name.into()),SqlValue::Integer(created)])
    };
    insert(id, username, 0).unwrap();
    assert_eq!(
        db.query(
            "SELECT username FROM accounts WHERE username=? COLLATE BINARY",
            &[SqlValue::Text(username.into())]
        )
        .unwrap(),
        vec![vec![SqlValue::Text(username.into())]]
    );
    assert!(insert("01890f3e-53b7-7d28-9b05-4f65092d5712", username, 0).is_err());
    insert(
        "01890f3e-53b7-7d28-9b05-4f65092d5712",
        &username.to_ascii_lowercase(),
        0,
    )
    .unwrap();
    for name in ["abcde\u{0b}fghij", "abcdefghijé"] {
        assert!(insert("01890f3e-53b7-7d28-9b05-4f65092d5713", name, 0).is_err());
    }
    assert!(insert("01890f3e-53b7-7d28-fb05-4f65092d5713", "abcdefghij", 0).is_err());
    assert!(
        insert(
            "01890f3e-53b7-7d28-9b05-4f65092d5713",
            "abcdefghij",
            9_007_199_254_740_992
        )
        .is_err()
    );
    assert!(
        db.execute(
            "UPDATE accounts SET credential_epoch=0.5 WHERE account_id=?",
            &[SqlValue::Text(id.into())]
        )
        .is_err()
    );
    assert!(
        db.execute(
            "UPDATE accounts SET role='unknown' WHERE account_id=?",
            &[SqlValue::Text(id.into())]
        )
        .is_err()
    );
    assert!(db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at,revoked_at) VALUES(?,?,?,'normal',0,0,86400000,NULL)",&[SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5713".into()),SqlValue::Text(id.into()),SqlValue::Blob(vec![0;31])]).is_err());
}

#[test]
fn real_sqlite_transaction_rolls_back_on_storage_failure() {
    let db = Sqlite::new();
    db.execute("CREATE TABLE probe(value INTEGER NOT NULL UNIQUE)", &[])
        .unwrap();
    let result: Result<(), StorageError> = db.transaction(|| {
        db.execute(
            "INSERT INTO probe(value) VALUES(?)",
            &[SqlValue::Integer(1)],
        )?;
        db.execute(
            "INSERT INTO probe(value) VALUES(?)",
            &[SqlValue::Integer(1)],
        )
    });
    assert_eq!(result, Err(StorageError));
    assert!(db.query("SELECT value FROM probe", &[]).unwrap().is_empty());
}
