#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::storage::{Database, SqlValue, migrate};
use support::Sqlite;

#[test]
fn migration_installs_management_tables_atomically() {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    assert_eq!(
        db.query("SELECT schema_version FROM storage_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
    for table in [
        "management_receipts",
        "admin_audit",
        "pending_account_removals",
    ] {
        assert!(db.query(&format!("SELECT * FROM {table}"), &[]).is_ok());
    }
}

fn version_one_database() -> Sqlite {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    for table in [
        "management_receipts",
        "admin_audit",
        "pending_account_removals",
    ] {
        db.execute(&format!("DROP TABLE {table}"), &[]).unwrap();
    }
    db.execute(
        "UPDATE storage_metadata SET schema_version=1,last_observed_ms=123,command_floor_ms=100",
        &[],
    )
    .unwrap();
    db
}
#[test]
fn forward_v1_migration_preserves_all_existing_ddl_data_and_clock_metadata() {
    let db = version_one_database();
    db.execute("INSERT INTO accounts(account_id,username,role,status,credential_epoch,created_at) VALUES('01890f3e-53b7-7d28-9b05-4f65092d5711','HostPerson01','host','pending_enrollment',0,0)",&[]).unwrap();
    let before = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    migrate(&db).unwrap();
    let after = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    assert!(before.iter().all(|row| after.contains(row)));
    assert_eq!(
        db.query(
            "SELECT schema_version,last_observed_ms,command_floor_ms FROM storage_metadata",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Integer(2),
            SqlValue::Integer(123),
            SqlValue::Integer(100)
        ]]
    );
    assert_eq!(
        db.query("SELECT username FROM accounts", &[]).unwrap(),
        vec![vec![SqlValue::Text("HostPerson01".into())]]
    );
}
#[test]
fn failed_v2_forward_migration_rolls_back_all_partial_ddl() {
    let db = version_one_database();
    db.execute("CREATE TABLE admin_audit(existing INTEGER)", &[])
        .unwrap();
    let before = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    assert!(migrate(&db).is_err());
    assert_eq!(
        db.query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
            .unwrap(),
        before
    );
    assert_eq!(
        db.query("SELECT schema_version FROM storage_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}
