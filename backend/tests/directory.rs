#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
// Compile the owned module directly so this target needs no sibling lib.rs edit.
#[path = "../src/directory/mod.rs"]
mod directory;
#[path = "../src/db/directory.rs"]
mod directory_db;
#[path = "../src/limits.rs"]
#[allow(
    dead_code,
    reason = "The scoped target shares backend policy constants."
)]
mod limits;
mod support;
use brews_backend::{auth, db};
use brews_domain::ids::{AccountId, OperationId};
use db::{Database, SqlValue};
use directory::RemovalGrant;
use directory_db::{DirectoryService, migrate_directory};
use support::{Sqlite, TestRuntime};

#[test]
fn fresh_directory_rejects_sqlite_prefix_lookalikes() {
    let db = Sqlite::new();
    db.execute(
        "CREATE TABLE sqliteXlegacy_games(designated_host_id TEXT NOT NULL) STRICT",
        &[],
    )
    .unwrap();
    db.execute(
        "INSERT INTO sqliteXlegacy_games VALUES(?)",
        &[SqlValue::Text(account().to_string())],
    )
    .unwrap();
    assert_eq!(
        migrate_directory(&db),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        db.query("SELECT count(*) FROM sqliteXlegacy_games", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert!(
        db.query(
            "SELECT name FROM sqlite_master WHERE name='directory_metadata'",
            &[]
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn initialized_directory_rejects_sqlite_prefix_lookalikes() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    db.execute(
        "CREATE TABLE sqliteXlegacy_games(designated_host_id TEXT NOT NULL) STRICT",
        &[],
    )
    .unwrap();
    db.execute(
        "INSERT INTO sqliteXlegacy_games VALUES(?)",
        &[SqlValue::Text(account().to_string())],
    )
    .unwrap();
    assert_eq!(
        (
            migrate_directory(&db),
            matches!(
                DirectoryService::new(&db, &rt),
                Err(directory::DirectoryError::Storage)
            ),
            service.acquire_removal(operation(&rt, 1), account()),
        ),
        (
            Err(directory::DirectoryError::Storage),
            true,
            Err(directory::DirectoryError::Storage),
        )
    );
    assert!(
        db.query("SELECT account_id FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.query("SELECT count(*) FROM sqliteXlegacy_games", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn sqlite_sequence_metadata_does_not_change_the_approved_application_inventory() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    db.execute(
        "CREATE TABLE sequence_probe(id INTEGER PRIMARY KEY AUTOINCREMENT)",
        &[],
    )
    .unwrap();
    db.execute("INSERT INTO sequence_probe DEFAULT VALUES", &[])
        .unwrap();
    db.execute("DROP TABLE sequence_probe", &[]).unwrap();
    assert_eq!(
        db.query(
            "SELECT name FROM sqlite_master WHERE name='sqlite_sequence'",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Text("sqlite_sequence".into())]]
    );
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service.acquire_removal(op, account()).unwrap();
    service.release_removal(op, account()).unwrap();
    service.cleanup().unwrap();
    assert!(service.next_deadline().unwrap().is_some());
}

#[test]
fn workerd_alarm_metadata_does_not_change_the_approved_application_inventory() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    db.execute(
        "CREATE TABLE _cf_METADATA (key INTEGER PRIMARY KEY, value BLOB)",
        &[],
    )
    .unwrap();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service.acquire_removal(op, account()).unwrap();
    service.release_removal(op, account()).unwrap();
    service.cleanup().unwrap();
    assert!(service.next_deadline().unwrap().is_some());
    db.execute("CREATE TABLE _cf_LEGACY_GAMES (host TEXT)", &[])
        .unwrap();
    assert_eq!(
        migrate_directory(&db),
        Err(directory::DirectoryError::Storage)
    );
}

#[test]
fn initialized_directory_rejects_unrecognized_application_tables_before_granting_removal() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    db.execute(
        "CREATE TABLE legacy_games(designated_host_id TEXT NOT NULL) STRICT",
        &[],
    )
    .unwrap();
    db.execute(
        "INSERT INTO legacy_games VALUES(?)",
        &[SqlValue::Text(account().to_string())],
    )
    .unwrap();
    assert_eq!(
        migrate_directory(&db),
        Err(directory::DirectoryError::Storage)
    );
    assert!(matches!(
        DirectoryService::new(&db, &rt),
        Err(directory::DirectoryError::Storage)
    ));
    assert_eq!(
        service.acquire_removal(operation(&rt, 1), account()),
        Err(directory::DirectoryError::Storage)
    );
    assert!(
        db.query("SELECT account_id FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.query("SELECT count(*) FROM legacy_games", &[]).unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn competing_operation_returns_busy_without_replacing_the_current_gate() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let first = operation(&rt, 1);
    service.acquire_removal(first, account()).unwrap();
    assert_eq!(
        format!(
            "{:?}",
            service
                .acquire_removal(operation(&rt, 2), account())
                .unwrap_err()
        ),
        "Busy"
    );
    assert_eq!(
        service.acquire_removal(first, account()),
        Ok(RemovalGrant {
            operation_id: first,
            account_id: account()
        })
    );
}

#[test]
fn gate_acquired_first_rejects_later_host_assignment() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    service
        .acquire_removal(operation(&rt, 1), account())
        .unwrap();
    let game = "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap();
    assert_eq!(
        service.seed_nonterminal_assignment(game, account()),
        Err(directory::DirectoryError::AssignmentBlocked)
    );
    assert_eq!(
        db.query(
            "SELECT count(*) FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}

#[test]
fn host_assignment_committed_first_rejects_removal_and_clears_its_gate() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let game = "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap();
    service
        .seed_nonterminal_assignment(game, account())
        .unwrap();
    assert_eq!(
        format!(
            "{:?}",
            service
                .acquire_removal(operation(&rt, 1), account())
                .unwrap_err()
        ),
        "HostedGame"
    );
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT * FROM directory_removal_pending", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn matching_release_is_durable_and_replays_the_same_ack_after_restart() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let op = operation(&rt, 1);
    let service = DirectoryService::new(&db, &rt).unwrap();
    service.acquire_removal(op, account()).unwrap();
    let ack = service.release_removal(op, account()).unwrap();
    assert_eq!(
        ack,
        directory::RemovalReleaseAck {
            operation_id: op,
            account_id: account()
        }
    );
    assert_eq!(
        DirectoryService::new(&db, &rt)
            .unwrap()
            .release_removal(op, account()),
        Ok(ack)
    );
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT * FROM directory_removal_pending", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.query(
            "SELECT operation_id,account_id FROM directory_removal_receipts",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text(op.to_string()),
            SqlValue::Text(account().to_string())
        ]]
    );
}

#[test]
fn operation_identity_cannot_be_retargeted_to_another_account() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    let other = "01890f3e-53b7-7d28-9b05-4f65092d5788".parse().unwrap();
    service.acquire_removal(op, account()).unwrap();
    assert_eq!(
        format!("{:?}", service.acquire_removal(op, other).unwrap_err()),
        "OperationMismatch"
    );
    assert_eq!(
        format!("{:?}", service.release_removal(op, other).unwrap_err()),
        "OperationMismatch"
    );
    assert_eq!(
        service.acquire_removal(op, account()),
        Ok(RemovalGrant {
            operation_id: op,
            account_id: account()
        })
    );
    let ack = service.release_removal(op, account()).unwrap();
    assert_eq!(
        format!("{:?}", service.release_removal(op, other).unwrap_err()),
        "OperationMismatch"
    );
    assert_eq!(service.release_removal(op, account()), Ok(ack));
}

#[test]
fn completed_operation_cannot_reacquire_or_clear_a_newer_gate() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let old = operation(&rt, 1);
    service.acquire_removal(old, account()).unwrap();
    let old_ack = service.release_removal(old, account()).unwrap();
    assert_eq!(
        format!("{:?}", service.acquire_removal(old, account()).unwrap_err()),
        "Completed"
    );
    let current = operation(&rt, 2);
    service.acquire_removal(current, account()).unwrap();
    assert_eq!(service.release_removal(old, account()), Ok(old_ack));
    assert_eq!(
        service.acquire_removal(current, account()),
        Ok(RemovalGrant {
            operation_id: current,
            account_id: account()
        })
    );
    assert_eq!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(account().to_string())]]
    );
}

#[test]
fn unknown_release_never_manufactures_an_ack_or_clears_a_current_gate() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let unknown = operation(&rt, 3);
    assert_eq!(
        format!(
            "{:?}",
            service.release_removal(unknown, account()).unwrap_err()
        ),
        "UnknownOperation"
    );
    let current = operation(&rt, 2);
    service.acquire_removal(current, account()).unwrap();
    assert_eq!(
        format!(
            "{:?}",
            service.release_removal(unknown, account()).unwrap_err()
        ),
        "UnknownOperation"
    );
    assert_eq!(
        service.acquire_removal(current, account()),
        Ok(RemovalGrant {
            operation_id: current,
            account_id: account()
        })
    );
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn lost_gate_metadata_cannot_produce_a_grant_or_completion_ack() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service.acquire_removal(op, account()).unwrap();
    db.conn.borrow().execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM account_assignment_gates; PRAGMA foreign_keys=ON;").unwrap();
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        service.release_removal(op, account()),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        service.reconcile_removal(op, account()),
        Err(directory::DirectoryError::Storage)
    );
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unsupported_schema_versions_fail_closed_even_for_an_existing_service_handle() {
    for version in [0, 3] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        db.execute(
            "UPDATE directory_metadata SET schema_version=?",
            &[SqlValue::Integer(version)],
        )
        .unwrap();
        assert!(matches!(
            DirectoryService::new(&db, &rt),
            Err(directory::DirectoryError::Storage)
        ));
        assert_eq!(
            migrate_directory(&db),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            service.acquire_removal(operation(&rt, 1), account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            service.release_removal(operation(&rt, 1), account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            service.reconcile_removal(operation(&rt, 1), account()),
            Err(directory::DirectoryError::Storage)
        );
        assert!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.query("SELECT schema_version FROM directory_metadata", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(version)]]
        );
    }
}

#[test]
fn unrecognized_existing_storage_is_never_treated_as_an_empty_host_index() {
    let db = Sqlite::new();
    db.execute("CREATE TABLE legacy_games(host TEXT NOT NULL)", &[])
        .unwrap();
    db.execute(
        "INSERT INTO legacy_games(host) VALUES(?)",
        &[SqlValue::Text(account().to_string())],
    )
    .unwrap();
    assert_eq!(
        migrate_directory(&db),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        db.query(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Text("legacy_games".into())]]
    );
}

#[test]
fn invalid_or_regressed_clocks_fail_closed_without_releasing_the_gate() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let now = rt.now.get();
    let op = operation(&rt, 1);
    service.acquire_removal(op, account()).unwrap();
    for invalid in [-1, limits::JS_SAFE_INTEGER_MAX + 1, now - 1] {
        rt.now.set(invalid);
        assert_eq!(
            format!("{:?}", service.acquire_removal(op, account()).unwrap_err()),
            "Clock"
        );
        assert_eq!(
            format!("{:?}", service.release_removal(op, account()).unwrap_err()),
            "Clock"
        );
        assert_eq!(
            service.reconcile_removal(op, account()),
            Err(directory::DirectoryError::Clock)
        );
        assert_eq!(
            db.query("SELECT last_observed_ms FROM directory_metadata", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(now)]]
        );
        assert_eq!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap(),
            vec![vec![SqlValue::Text(account().to_string())]]
        );
    }
    rt.now.set(now);
    assert!(service.release_removal(op, account()).is_ok());
}

#[test]
fn expired_receipt_compaction_preserves_a_durable_floor_against_resurrection() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let old = operation(&rt, 1);
    service.acquire_removal(old, account()).unwrap();
    service.release_removal(old, account()).unwrap();
    rt.now
        .set(rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS);
    let current = operation(&rt, 2);
    service.acquire_removal(current, account()).unwrap();
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        format!("{:?}", service.release_removal(old, account()).unwrap_err()),
        "StaleOperation"
    );
    assert_eq!(
        format!("{:?}", service.acquire_removal(old, account()).unwrap_err()),
        "StaleOperation"
    );
    assert_eq!(
        service.acquire_removal(current, account()),
        Ok(RemovalGrant {
            operation_id: current,
            account_id: account()
        })
    );
}

#[test]
fn hosted_game_rejection_is_bound_to_the_operation_even_after_assignment_disappears() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service
        .seed_nonterminal_assignment(
            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
            account(),
        )
        .unwrap();
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::HostedGame)
    );
    db.execute("DELETE FROM directory_hosted_nonterminal_games", &[])
        .unwrap();
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::HostedGame)
    );
    assert_eq!(
        service.release_removal(op, account()),
        Err(directory::DirectoryError::HostedGame)
    );
    let other = "01890f3e-53b7-7d28-9b05-4f65092d5788".parse().unwrap();
    assert_eq!(
        service.acquire_removal(op, other),
        Err(directory::DirectoryError::OperationMismatch)
    );
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .acquire_removal(operation(&rt, 2), account())
            .is_ok()
    );
}

#[test]
fn corrupted_pending_timestamps_cannot_authorize_an_ack_or_retry_grant() {
    for corrupt in [-1, 1_800_000_000_001] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let op = operation(&rt, 1);
        service.acquire_removal(op, account()).unwrap();
        db.execute(
            "UPDATE directory_removal_pending SET created_at=?",
            &[SqlValue::Integer(corrupt)],
        )
        .unwrap();
        assert_eq!(
            service.acquire_removal(op, account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            service.release_removal(op, account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap(),
            vec![vec![SqlValue::Text(account().to_string())]]
        );
    }
}

#[test]
fn corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() {
    for age in [1, limits::COMMAND_RECEIPT_RETENTION_MS] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let op = operation(&rt, 1);
        service.acquire_removal(op, account()).unwrap();
        service.release_removal(op, account()).unwrap();
        db.execute(
            "UPDATE directory_removal_receipts SET completed_at=?",
            &[SqlValue::Integer(rt.now.get() - age)],
        )
        .unwrap();
        assert_eq!(
            service.acquire_removal(op, account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            service.release_removal(op, account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            db.query("SELECT count(*) FROM directory_removal_receipts", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
    }
}

#[test]
fn expired_rejection_is_denied_even_when_bounded_compaction_leaves_its_row() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    service
        .seed_nonterminal_assignment(
            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
            account(),
        )
        .unwrap();
    let mut last = operation(&rt, 1);
    for n in 1..=101 {
        last = operation(&rt, n);
        assert_eq!(
            service.acquire_removal(last, account()),
            Err(directory::DirectoryError::HostedGame)
        );
    }
    rt.now
        .set(rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS);
    assert_eq!(
        service.acquire_removal(last, account()),
        Err(directory::DirectoryError::StaleOperation)
    );
}

#[test]
fn acquired_gate_rejects_transfer_without_changing_the_original_host() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let game = "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap();
    let original = "01890f3e-53b7-7d28-9b05-4f65092d5788".parse().unwrap();
    service.seed_nonterminal_assignment(game, original).unwrap();
    service
        .acquire_removal(operation(&rt, 1), account())
        .unwrap();
    assert_eq!(
        service.seed_nonterminal_assignment(game, account()),
        Err(directory::DirectoryError::AssignmentBlocked)
    );
    assert_eq!(
        db.query(
            "SELECT designated_host_id FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Text(original.to_string())]]
    );
}

#[test]
fn orphan_gate_is_retained_without_inventing_no_games_proof() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    db.execute(
        "INSERT INTO account_assignment_gates(account_id) VALUES(?)",
        &[SqlValue::Text(account().to_string())],
    )
    .unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        service.release_removal(op, account()),
        Err(directory::DirectoryError::UnknownOperation)
    );
    assert_eq!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(account().to_string())]]
    );
}

#[test]
fn sqlite_write_failure_rolls_back_the_gate_and_operation_metadata() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    db.conn.borrow().execute_batch("CREATE TRIGGER refuse_pending BEFORE INSERT ON directory_removal_pending BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::Storage)
    );
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT * FROM directory_removal_pending", &[])
            .unwrap()
            .is_empty()
    );
    db.execute("DROP TRIGGER refuse_pending", &[]).unwrap();
    service.acquire_removal(op, account()).unwrap();
    let observed = rt.now.get();
    rt.now.set(observed + 1);
    db.conn.borrow().execute_batch("CREATE TRIGGER refuse_receipt BEFORE INSERT ON directory_removal_receipts BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
    assert_eq!(
        service.release_removal(op, account()),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(account().to_string())]]
    );
    assert_eq!(
        db.query("SELECT last_observed_ms FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(observed)]]
    );
    assert_eq!(
        db.query("SELECT operation_id FROM directory_removal_pending", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(op.to_string())]]
    );
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    db.execute("DROP TRIGGER refuse_receipt", &[]).unwrap();
    assert!(service.release_removal(op, account()).is_ok());
}

#[test]
fn pending_intent_survives_database_reopen_beyond_the_fresh_admission_window() {
    let rt = TestRuntime::new();
    let path = std::env::temp_dir().join(format!(
        "brews-directory-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let op = operation(&rt, 1);
    {
        let db = Sqlite {
            conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
        };
        db.conn
            .borrow()
            .execute_batch("PRAGMA foreign_keys=ON")
            .unwrap();
        migrate_directory(&db).unwrap();
        DirectoryService::new(&db, &rt)
            .unwrap()
            .acquire_removal(op, account())
            .unwrap();
    }
    rt.now
        .set(rt.now.get() + 2 * limits::COMMAND_RECEIPT_RETENTION_MS);
    {
        let db = Sqlite {
            conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
        };
        db.conn
            .borrow()
            .execute_batch("PRAGMA foreign_keys=ON")
            .unwrap();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        assert_eq!(
            service.acquire_removal(op, account()),
            Ok(RemovalGrant {
                operation_id: op,
                account_id: account()
            })
        );
        let ack = service.release_removal(op, account()).unwrap();
        let current = operation(&rt, 2);
        service.acquire_removal(current, account()).unwrap();
        assert_eq!(service.release_removal(op, account()), Ok(ack));
        assert!(service.acquire_removal(current, account()).is_ok());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn fresh_admission_rejects_future_or_floor_equal_operation_ids() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let now = rt.now.get();
    rt.now.set(now + 1);
    let future = operation(&rt, 1);
    rt.now.set(now - limits::COMMAND_RECEIPT_RETENTION_MS);
    let floor = operation(&rt, 2);
    rt.now.set(now);
    let service = DirectoryService::new(&db, &rt).unwrap();
    for op in [future, floor] {
        assert_eq!(
            service.acquire_removal(op, account()),
            Err(directory::DirectoryError::StaleOperation)
        );
        assert_eq!(
            service.release_removal(op, account()),
            Err(directory::DirectoryError::StaleOperation)
        );
    }
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
}

struct InspectHostedRead {
    db: Sqlite,
    held: std::cell::Cell<bool>,
}
impl Database for InspectHostedRead {
    fn query(&self, sql: &str, values: &[SqlValue]) -> Result<Vec<db::Row>, db::StorageError> {
        if sql.starts_with("SELECT game_id FROM directory_hosted_nonterminal_games WHERE") {
            assert_eq!(
                self.db
                    .query("SELECT * FROM account_assignment_gates", &[])?,
                vec![vec![SqlValue::Text(account().to_string())]]
            );
            self.held.set(true);
            return Err(db::StorageError);
        }
        self.db.query(sql, values)
    }
    fn execute(&self, sql: &str, values: &[SqlValue]) -> Result<(), db::StorageError> {
        self.db.execute(sql, values)
    }
    fn transaction<T>(
        &self,
        f: impl FnOnce() -> Result<T, db::StorageError>,
    ) -> Result<T, db::StorageError> {
        self.db.transaction(f)
    }
}
#[test]
fn failed_hosted_query_is_made_while_the_gate_is_held_and_rolls_back() {
    let db = InspectHostedRead {
        db: Sqlite::new(),
        held: std::cell::Cell::new(false),
    };
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(
        service.acquire_removal(operation(&rt, 1), account()),
        Err(directory::DirectoryError::Storage)
    );
    assert!(db.held.get());
    assert!(
        db.db
            .query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.db
            .query("SELECT * FROM directory_removal_pending", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy() {
    let path = std::env::temp_dir().join(format!(
        "brews-directory-concurrent-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let db = Sqlite {
            conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
        };
        db.conn
            .borrow()
            .execute_batch("PRAGMA foreign_keys=ON")
            .unwrap();
        migrate_directory(&db).unwrap();
    }
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = [1, 2]
            .into_iter()
            .map(|n| {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || {
                    let db = Sqlite {
                        conn: std::cell::RefCell::new(rusqlite::Connection::open(path).unwrap()),
                    };
                    db.conn
                        .borrow()
                        .busy_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    db.conn
                        .borrow()
                        .execute_batch("PRAGMA foreign_keys=ON")
                        .unwrap();
                    let rt = TestRuntime::new();
                    let service = DirectoryService::new(&db, &rt).unwrap();
                    barrier.wait();
                    service.acquire_removal(operation(&rt, n), account())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(directory::DirectoryError::Busy))
            .count(),
        1
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn reconcile_hosted_rejection_returns_durable_abort_ack_without_mutation_grant() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service
        .seed_nonterminal_assignment(
            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
            account(),
        )
        .unwrap();
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::HostedGame)
    );
    assert_eq!(
        service.release_removal(op, account()),
        Err(directory::DirectoryError::HostedGame)
    );
    let ack = directory::RemovalReleaseAck {
        operation_id: op,
        account_id: account(),
    };
    assert_eq!(service.reconcile_removal(op, account()), Ok(ack));
    assert_eq!(
        DirectoryService::new(&db, &rt)
            .unwrap()
            .release_removal(op, account()),
        Ok(ack)
    );
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::Completed)
    );
    assert_eq!(
        db.query(
            "SELECT count(*) FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reconcile_unknown_cancels_delayed_acquire_with_a_durable_completion_seal() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    let ack = directory::RemovalReleaseAck {
        operation_id: op,
        account_id: account(),
    };
    assert_eq!(service.reconcile_removal(op, account()), Ok(ack));
    let restarted = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(restarted.reconcile_removal(op, account()), Ok(ack));
    assert_eq!(restarted.release_removal(op, account()), Ok(ack));
    assert_eq!(
        restarted.acquire_removal(op, account()),
        Err(directory::DirectoryError::Completed)
    );
    assert_eq!(
        db.query(
            "SELECT operation_id,account_id,completed_at FROM directory_removal_receipts",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text(op.to_string()),
            SqlValue::Text(account().to_string()),
            SqlValue::Integer(rt.now.get())
        ]]
    );
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reconcile_pending_releases_only_its_validated_gate_after_admission_expiry() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service.acquire_removal(op, account()).unwrap();
    rt.now
        .set(rt.now.get() + 2 * limits::COMMAND_RECEIPT_RETENTION_MS);
    let ack = directory::RemovalReleaseAck {
        operation_id: op,
        account_id: account(),
    };
    assert_eq!(service.reconcile_removal(op, account()), Ok(ack));
    assert_eq!(service.release_removal(op, account()), Ok(ack));
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT * FROM directory_removal_pending", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.query("SELECT completed_at FROM directory_removal_receipts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(rt.now.get())]]
    );
}

#[test]
fn reconcile_old_release_after_receipt_purge_acks_without_new_rows_or_gate_changes() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let old = operation(&rt, 1);
    service.acquire_removal(old, account()).unwrap();
    let ack = service.release_removal(old, account()).unwrap();
    rt.now
        .set(rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS);
    let current = operation(&rt, 2);
    service.acquire_removal(current, account()).unwrap();
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service.release_removal(old, account()),
        Err(directory::DirectoryError::StaleOperation)
    );
    assert_eq!(service.reconcile_removal(old, account()), Ok(ack));
    assert_eq!(
        DirectoryService::new(&db, &rt)
            .unwrap()
            .reconcile_removal(old, account()),
        Ok(ack)
    );
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service.acquire_removal(old, account()),
        Err(directory::DirectoryError::StaleOperation)
    );
    assert!(service.acquire_removal(current, account()).is_ok());
    assert_eq!(
        db.query("SELECT operation_id FROM directory_removal_pending", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(current.to_string())]]
    );
}

#[test]
fn reconcile_rejects_target_mismatch_before_expired_evidence_is_compacted() {
    for state in ["pending", "released", "rejected", "cancelled"] {
        for age in [0, limits::COMMAND_RECEIPT_RETENTION_MS] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            migrate_directory(&db).unwrap();
            let service = DirectoryService::new(&db, &rt).unwrap();
            let op = operation(&rt, 1);
            match state {
                "pending" => {
                    service.acquire_removal(op, account()).unwrap();
                }
                "released" => {
                    service.acquire_removal(op, account()).unwrap();
                    service.release_removal(op, account()).unwrap();
                }
                "rejected" => {
                    service
                        .seed_nonterminal_assignment(
                            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
                            account(),
                        )
                        .unwrap();
                    assert_eq!(
                        service.acquire_removal(op, account()),
                        Err(directory::DirectoryError::HostedGame)
                    );
                }
                "cancelled" => {
                    service.reconcile_removal(op, account()).unwrap();
                }
                _ => unreachable!(),
            }
            rt.now.set(rt.now.get() + age);
            let other = "01890f3e-53b7-7d28-9b05-4f65092d5788".parse().unwrap();
            assert_eq!(
                service.reconcile_removal(op, other),
                Err(directory::DirectoryError::OperationMismatch),
                "{state} age {age}"
            );
            let table = match state {
                "pending" => "directory_removal_pending",
                "rejected" => "directory_removal_rejections",
                _ => "directory_removal_receipts",
            };
            assert_eq!(
                db.query(&format!("SELECT account_id FROM {table}"), &[])
                    .unwrap(),
                vec![vec![SqlValue::Text(account().to_string())]]
            );
        }
    }
}

#[test]
fn reconcile_orphan_gate_fails_closed_for_unknown_and_completed_operations() {
    for state in ["unknown", "released", "rejected"] {
        for age in [0, limits::COMMAND_RECEIPT_RETENTION_MS] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            migrate_directory(&db).unwrap();
            let service = DirectoryService::new(&db, &rt).unwrap();
            let op = operation(&rt, 1);
            if state == "released" {
                service.acquire_removal(op, account()).unwrap();
                service.release_removal(op, account()).unwrap();
            } else if state == "rejected" {
                service
                    .seed_nonterminal_assignment(
                        "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
                        account(),
                    )
                    .unwrap();
                assert_eq!(
                    service.acquire_removal(op, account()),
                    Err(directory::DirectoryError::HostedGame)
                );
            }
            db.execute(
                "INSERT INTO account_assignment_gates(account_id) VALUES(?)",
                &[SqlValue::Text(account().to_string())],
            )
            .unwrap();
            let before = db
                .query(
                    "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
                    &[],
                )
                .unwrap();
            rt.now.set(rt.now.get() + age);
            assert_eq!(
                service.reconcile_removal(op, account()),
                Err(directory::DirectoryError::Storage),
                "{state} age {age}"
            );
            assert_eq!(
                db.query("SELECT * FROM account_assignment_gates", &[])
                    .unwrap(),
                vec![vec![SqlValue::Text(account().to_string())]]
            );
            assert_eq!(
                db.query(
                    "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
                    &[]
                )
                .unwrap(),
                before
            );
        }
    }
}

#[test]
fn reconcile_conflicting_operation_states_never_authorize_absence() {
    for (left, right) in [
        ("directory_removal_pending", "directory_removal_receipts"),
        ("directory_removal_pending", "directory_removal_rejections"),
        ("directory_removal_receipts", "directory_removal_rejections"),
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let op = operation(&rt, 1);
        if left == "directory_removal_pending" {
            service.acquire_removal(op, account()).unwrap();
        } else {
            service.reconcile_removal(op, account()).unwrap();
        }
        db.execute(
            &format!("INSERT INTO {right}(operation_id,account_id,completed_at) VALUES(?,?,?)"),
            &[
                SqlValue::Text(op.to_string()),
                SqlValue::Text(account().to_string()),
                SqlValue::Integer(rt.now.get()),
            ],
        )
        .unwrap();
        assert_eq!(
            service.reconcile_removal(op, account()),
            Err(directory::DirectoryError::Storage),
            "{left} plus {right}"
        );
        assert_eq!(
            db.query(&format!("SELECT count(*) FROM {left}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
        assert_eq!(
            db.query(&format!("SELECT count(*) FROM {right}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
    }
}

#[test]
fn reconcile_preserves_another_operations_active_gate_for_every_absent_state() {
    for state in ["unknown", "released", "rejected", "cancelled"] {
        for age in [0, limits::COMMAND_RECEIPT_RETENTION_MS] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            migrate_directory(&db).unwrap();
            let service = DirectoryService::new(&db, &rt).unwrap();
            let old = operation(&rt, 1);
            match state {
                "released" => {
                    service.acquire_removal(old, account()).unwrap();
                    service.release_removal(old, account()).unwrap();
                }
                "rejected" => {
                    service
                        .seed_nonterminal_assignment(
                            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
                            account(),
                        )
                        .unwrap();
                    assert_eq!(
                        service.acquire_removal(old, account()),
                        Err(directory::DirectoryError::HostedGame)
                    );
                    db.execute("DELETE FROM directory_hosted_nonterminal_games", &[])
                        .unwrap();
                }
                "cancelled" => {
                    service.reconcile_removal(old, account()).unwrap();
                }
                _ => {}
            }
            rt.now.set(rt.now.get() + age);
            let current = operation(&rt, 2);
            let grant = service.acquire_removal(current, account()).unwrap();
            let before = db
                .query(
                    "SELECT operation_id,account_id,created_at FROM directory_removal_pending",
                    &[],
                )
                .unwrap();
            let ack = directory::RemovalReleaseAck {
                operation_id: old,
                account_id: account(),
            };
            assert_eq!(
                service.reconcile_removal(old, account()),
                Ok(ack),
                "{state} age {age}"
            );
            assert_eq!(
                db.query(
                    "SELECT operation_id,account_id,created_at FROM directory_removal_pending",
                    &[]
                )
                .unwrap(),
                before
            );
            assert_eq!(
                db.query("SELECT * FROM account_assignment_gates", &[])
                    .unwrap(),
                vec![vec![SqlValue::Text(account().to_string())]]
            );
            assert_eq!(service.acquire_removal(current, account()), Ok(grant));
            assert_eq!(
                service.acquire_removal(old, account()),
                Err(if age == 0 {
                    directory::DirectoryError::Completed
                } else {
                    directory::DirectoryError::StaleOperation
                })
            );
        }
    }
}

#[test]
fn reconcile_validates_known_row_shape_even_when_completion_is_expired() {
    for table in [
        "directory_removal_pending",
        "directory_removal_receipts",
        "directory_removal_rejections",
    ] {
        for corrupt in ["account", "before_issue", "negative", "future"] {
            for age in [0, limits::COMMAND_RECEIPT_RETENTION_MS] {
                let db = Sqlite::new();
                let rt = TestRuntime::new();
                migrate_directory(&db).unwrap();
                let service = DirectoryService::new(&db, &rt).unwrap();
                let issued = rt.now.get();
                let op = operation(&rt, 1);
                if table == "directory_removal_pending" {
                    service.acquire_removal(op, account()).unwrap();
                } else {
                    db.execute(&format!("INSERT INTO {table}(operation_id,account_id,completed_at) VALUES(?,?,?)"), &[SqlValue::Text(op.to_string()),SqlValue::Text(account().to_string()),SqlValue::Integer(issued)]).unwrap();
                }
                rt.now.set(issued + age);
                if corrupt == "account" {
                    db.conn
                        .borrow()
                        .execute_batch("PRAGMA foreign_keys=OFF")
                        .unwrap();
                    db.execute(
                        &format!("UPDATE {table} SET account_id=?"),
                        &[SqlValue::Text("not-an-account".into())],
                    )
                    .unwrap();
                    db.conn
                        .borrow()
                        .execute_batch("PRAGMA foreign_keys=ON")
                        .unwrap();
                } else {
                    let column = if table == "directory_removal_pending" {
                        "created_at"
                    } else {
                        "completed_at"
                    };
                    let at = match corrupt {
                        "before_issue" => issued - 1,
                        "negative" => -1,
                        _ => rt.now.get() + 1,
                    };
                    db.execute(
                        &format!("UPDATE {table} SET {column}=?"),
                        &[SqlValue::Integer(at)],
                    )
                    .unwrap();
                }
                let before = db
                    .query(
                        "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
                        &[],
                    )
                    .unwrap();
                assert_eq!(
                    service.reconcile_removal(op, account()),
                    Err(directory::DirectoryError::Storage),
                    "{table} {corrupt} age {age}"
                );
                assert_eq!(
                    db.query(
                        "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
                        &[]
                    )
                    .unwrap(),
                    before
                );
                assert_eq!(
                    db.query(&format!("SELECT count(*) FROM {table}"), &[])
                        .unwrap(),
                    vec![vec![SqlValue::Integer(1)]]
                );
            }
        }
    }
}

#[test]
fn reconcile_storage_failure_rolls_back_release_cancel_and_rejection_seals() {
    for state in ["unknown", "pending", "rejected"] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let op = operation(&rt, 1);
        if state == "pending" {
            service.acquire_removal(op, account()).unwrap();
        } else if state == "rejected" {
            service
                .seed_nonterminal_assignment(
                    "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
                    account(),
                )
                .unwrap();
            assert_eq!(
                service.acquire_removal(op, account()),
                Err(directory::DirectoryError::HostedGame)
            );
        }
        let before = db
            .query(
                "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
                &[],
            )
            .unwrap();
        let gates = db
            .query("SELECT * FROM account_assignment_gates", &[])
            .unwrap();
        let pending = db
            .query("SELECT * FROM directory_removal_pending", &[])
            .unwrap();
        let rejected = db
            .query("SELECT * FROM directory_removal_rejections", &[])
            .unwrap();
        rt.now
            .set(rt.now.get() + 2 * limits::COMMAND_RECEIPT_RETENTION_MS);
        // Unknown cancellation stays fresh; pending/rejection recovery may be old.
        let op = if state == "unknown" {
            operation(&rt, 2)
        } else {
            op
        };
        db.conn.borrow().execute_batch("CREATE TRIGGER refuse_reconcile BEFORE INSERT ON directory_removal_receipts BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
        assert_eq!(
            service.reconcile_removal(op, account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            db.query(
                "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
                &[]
            )
            .unwrap(),
            before
        );
        assert_eq!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap(),
            gates
        );
        assert_eq!(
            db.query("SELECT * FROM directory_removal_pending", &[])
                .unwrap(),
            pending
        );
        assert_eq!(
            db.query("SELECT * FROM directory_removal_rejections", &[])
                .unwrap(),
            rejected
        );
        assert!(
            db.query("SELECT * FROM directory_removal_receipts", &[])
                .unwrap()
                .is_empty()
        );
        db.execute("DROP TRIGGER refuse_reconcile", &[]).unwrap();
        assert!(service.reconcile_removal(op, account()).is_ok());
    }
}

#[test]
fn reconcile_rejection_delete_failure_rolls_back_the_inserted_ack_receipt() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service
        .seed_nonterminal_assignment(
            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
            account(),
        )
        .unwrap();
    assert_eq!(
        service.acquire_removal(op, account()),
        Err(directory::DirectoryError::HostedGame)
    );
    let before = rt.now.get();
    rt.now.set(before + 1);
    db.conn.borrow().execute_batch("CREATE TRIGGER refuse_rejection_delete BEFORE DELETE ON directory_removal_rejections BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
    assert_eq!(
        service.reconcile_removal(op, account()),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        db.query("SELECT count(*) FROM directory_removal_rejections", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.query("SELECT last_observed_ms FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(before)]]
    );
    db.execute("DROP TRIGGER refuse_rejection_delete", &[])
        .unwrap();
    assert!(service.reconcile_removal(op, account()).is_ok());
}

#[test]
fn reconcile_absence_fence_survives_database_reopen_purge_and_clock_rollback() {
    let rt = TestRuntime::new();
    let path = std::env::temp_dir().join(format!(
        "brews-directory-reconcile-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let old = operation(&rt, 1);
    {
        let db = Sqlite {
            conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
        };
        db.conn
            .borrow()
            .execute_batch("PRAGMA foreign_keys=ON")
            .unwrap();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        service.reconcile_removal(old, account()).unwrap();
    }
    rt.now
        .set(rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS);
    let observed = rt.now.get();
    let current = operation(&rt, 2);
    {
        let db = Sqlite {
            conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
        };
        db.conn
            .borrow()
            .execute_batch("PRAGMA foreign_keys=ON")
            .unwrap();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        service.acquire_removal(current, account()).unwrap();
        assert!(
            db.query("SELECT * FROM directory_removal_receipts", &[])
                .unwrap()
                .is_empty()
        );
    }
    {
        let db = Sqlite {
            conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
        };
        db.conn
            .borrow()
            .execute_batch("PRAGMA foreign_keys=ON")
            .unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        rt.now.set(observed - 1);
        assert_eq!(
            service.reconcile_removal(old, account()),
            Err(directory::DirectoryError::Clock)
        );
        rt.now.set(observed);
        let ack = directory::RemovalReleaseAck {
            operation_id: old,
            account_id: account(),
        };
        assert_eq!(service.reconcile_removal(old, account()), Ok(ack));
        assert_eq!(
            service.acquire_removal(old, account()),
            Err(directory::DirectoryError::StaleOperation)
        );
        assert!(service.acquire_removal(current, account()).is_ok());
        assert!(
            db.query("SELECT * FROM directory_removal_receipts", &[])
                .unwrap()
                .is_empty()
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn reconcile_uses_saved_nonretreating_floor_not_the_current_window_alone() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let now = rt.now.get();
    let saved_floor = now - 1;
    rt.now.set(saved_floor);
    let old_unknown = operation(&rt, 1);
    rt.now.set(now);
    db.execute(
        "UPDATE directory_metadata SET last_observed_ms=?,command_floor_ms=?",
        &[SqlValue::Integer(now), SqlValue::Integer(saved_floor)],
    )
    .unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let current = operation(&rt, 2);
    service.acquire_removal(current, account()).unwrap();
    assert_eq!(
        service.reconcile_removal(old_unknown, account()),
        Ok(directory::RemovalReleaseAck {
            operation_id: old_unknown,
            account_id: account()
        })
    );
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service.acquire_removal(old_unknown, account()),
        Err(directory::DirectoryError::StaleOperation)
    );
    assert_eq!(
        db.query("SELECT command_floor_ms FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(saved_floor)]]
    );
    assert!(service.acquire_removal(current, account()).is_ok());
}

#[test]
fn reconcile_future_and_zero_time_unknown_operations_cannot_create_ack_receipts() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let now = rt.now.get();
    rt.now.set(now + 1);
    let future = operation(&rt, 1);
    rt.now.set(0);
    let zero = operation(&rt, 2);
    rt.now.set(now);
    let service = DirectoryService::new(&db, &rt).unwrap();
    for op in [future, zero] {
        assert_eq!(
            service.reconcile_removal(op, account()),
            Err(directory::DirectoryError::StaleOperation)
        );
    }
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reconcile_validates_other_active_gate_metadata_before_sealing_an_unknown_operation() {
    for corrupt in ["operation", "timestamp", "missing_gate"] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let current = operation(&rt, 1);
        service.acquire_removal(current, account()).unwrap();
        match corrupt {
            "operation" => {
                db.execute(
                    "UPDATE directory_removal_pending SET operation_id='invalid-operation'",
                    &[],
                )
                .unwrap();
            }
            "timestamp" => {
                db.execute("UPDATE directory_removal_pending SET created_at=-1", &[])
                    .unwrap();
            }
            _ => {
                db.conn.borrow().execute_batch("PRAGMA foreign_keys=OFF; DELETE FROM account_assignment_gates; PRAGMA foreign_keys=ON;").unwrap();
            }
        }
        let gates = db
            .query("SELECT * FROM account_assignment_gates", &[])
            .unwrap();
        let pending = db
            .query("SELECT * FROM directory_removal_pending", &[])
            .unwrap();
        assert_eq!(
            service.reconcile_removal(operation(&rt, 2), account()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap(),
            gates
        );
        assert_eq!(
            db.query("SELECT * FROM directory_removal_pending", &[])
                .unwrap(),
            pending
        );
        assert!(
            db.query("SELECT * FROM directory_removal_receipts", &[])
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn reconcile_expired_completion_proof_survives_bounded_compaction_backlog() {
    for table in ["directory_removal_receipts", "directory_removal_rejections"] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        for n in 1..=101 {
            db.execute(
                &format!("INSERT INTO {table}(operation_id,account_id,completed_at) VALUES(?,?,?)"),
                &[
                    SqlValue::Text(operation(&rt, n).to_string()),
                    SqlValue::Text(account().to_string()),
                    SqlValue::Integer(rt.now.get()),
                ],
            )
            .unwrap();
        }
        let last = operation(&rt, 101);
        rt.now
            .set(rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS);
        assert_eq!(
            service.reconcile_removal(last, account()),
            Ok(directory::RemovalReleaseAck {
                operation_id: last,
                account_id: account()
            })
        );
        assert_eq!(
            db.query(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(
                if table == "directory_removal_receipts" {
                    1
                } else {
                    0
                }
            )]]
        );
        assert!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap()
                .is_empty()
        );
        assert!(
            db.query("SELECT * FROM directory_removal_pending", &[])
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn fresh_directory_migration_ignores_only_verified_miniflare_name_metadata() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    db.execute(
        "CREATE TABLE IF NOT EXISTS __miniflare_do_name (id INTEGER PRIMARY KEY, name TEXT)",
        &[],
    )
    .unwrap();
    db.execute(
        "INSERT INTO __miniflare_do_name(id,name) VALUES(1,'directory')",
        &[],
    )
    .unwrap();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service.acquire_removal(op, account()).unwrap();
    assert_eq!(
        service.release_removal(op, account()),
        Ok(directory::RemovalReleaseAck {
            operation_id: op,
            account_id: account()
        })
    );
    migrate_directory(&db).unwrap();
    assert_eq!(
        db.query("SELECT name FROM __miniflare_do_name", &[])
            .unwrap(),
        vec![vec![SqlValue::Text("directory".into())]]
    );
    assert_eq!(
        db.query("SELECT schema_version FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
}

#[test]
fn miniflare_metadata_exception_never_admits_unknown_or_partial_application_storage() {
    for unknown in [
        "legacy_games",
        "_cf_KV",
        "__miniflare_do_name_extra",
        "__miniflare_metadata",
        "directory_removal_pending",
    ] {
        let db = Sqlite::new();
        db.execute(
            "CREATE TABLE __miniflare_do_name (id INTEGER PRIMARY KEY, name TEXT)",
            &[],
        )
        .unwrap();
        db.execute(&format!("CREATE TABLE {unknown}(host TEXT NOT NULL)"), &[])
            .unwrap();
        db.execute(
            &format!("INSERT INTO {unknown}(host) VALUES(?)"),
            &[SqlValue::Text(account().to_string())],
        )
        .unwrap();
        let before = db
            .query(
                "SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name",
                &[],
            )
            .unwrap();
        assert_eq!(
            migrate_directory(&db),
            Err(directory::DirectoryError::Storage),
            "{unknown}"
        );
        assert_eq!(
            db.query(
                "SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name",
                &[]
            )
            .unwrap(),
            before
        );
        assert_eq!(
            db.query(&format!("SELECT host FROM {unknown}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Text(account().to_string())]]
        );
    }
    let db = Sqlite::new();
    db.execute(
        "CREATE TABLE __MINIFLARE_DO_NAME (id INTEGER PRIMARY KEY, name TEXT)",
        &[],
    )
    .unwrap();
    assert_eq!(
        migrate_directory(&db),
        Err(directory::DirectoryError::Storage)
    );
}

#[test]
fn retention_deadline_uses_completion_time_without_waiting_for_another_request() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    service.acquire_removal(op, account()).unwrap();
    rt.now.set(rt.now.get() + 10);
    service.release_removal(op, account()).unwrap();
    let deadline = rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS;
    assert_eq!(service.next_deadline(), Ok(Some(deadline)));
    rt.now.set(deadline);
    assert_eq!(service.next_deadline(), Ok(Some(deadline)));
    assert_eq!(
        db.query("SELECT count(*) FROM directory_removal_receipts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn retention_cleanup_purges_due_receipt_without_a_peer_request() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let old = operation(&rt, 1);
    service.acquire_removal(old, account()).unwrap();
    service.release_removal(old, account()).unwrap();
    let completed = rt.now.get();
    rt.now
        .set(completed + limits::COMMAND_RECEIPT_RETENTION_MS - 1);
    service.cleanup().unwrap();
    assert_eq!(
        db.query("SELECT count(*) FROM directory_removal_receipts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    rt.now.set(completed + limits::COMMAND_RECEIPT_RETENTION_MS);
    service.cleanup().unwrap();
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(service.next_deadline(), Ok(None));
    assert_eq!(
        db.query("SELECT command_floor_ms FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(completed)]]
    );
    let restarted = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(
        restarted.acquire_removal(old, account()),
        Err(directory::DirectoryError::StaleOperation)
    );
    assert_eq!(
        restarted.release_removal(old, account()),
        Err(directory::DirectoryError::StaleOperation)
    );
}

#[test]
fn retention_deadline_rejects_expiry_outside_javascript_safe_milliseconds() {
    for completed in [
        limits::JS_SAFE_INTEGER_MAX - limits::COMMAND_RECEIPT_RETENTION_MS,
        limits::JS_SAFE_INTEGER_MAX - limits::COMMAND_RECEIPT_RETENTION_MS + 1,
        limits::JS_SAFE_INTEGER_MAX,
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let op = operation(&rt, 1);
        service.acquire_removal(op, account()).unwrap();
        rt.now.set(completed);
        service.release_removal(op, account()).unwrap();
        let before = db.query("SELECT * FROM directory_metadata", &[]).unwrap();
        rt.now.set(completed + 1);
        if completed == limits::JS_SAFE_INTEGER_MAX {
            rt.now.set(completed);
        }
        assert_eq!(
            service.next_deadline(),
            if completed == limits::JS_SAFE_INTEGER_MAX - limits::COMMAND_RECEIPT_RETENTION_MS {
                Ok(Some(limits::JS_SAFE_INTEGER_MAX))
            } else {
                Err(directory::DirectoryError::Storage)
            }
        );
        if completed > limits::JS_SAFE_INTEGER_MAX - limits::COMMAND_RECEIPT_RETENTION_MS {
            assert_eq!(
                db.query("SELECT * FROM directory_metadata", &[]).unwrap(),
                before
            );
        }
        assert_eq!(
            db.query("SELECT count(*) FROM directory_removal_receipts", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
    }
}

#[test]
fn retention_deadline_selects_the_earliest_receipt_or_hosted_rejection() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(service.next_deadline(), Ok(None));
    service
        .seed_nonterminal_assignment(
            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
            account(),
        )
        .unwrap();
    let rejected = operation(&rt, 1);
    assert_eq!(
        service.acquire_removal(rejected, account()),
        Err(directory::DirectoryError::HostedGame)
    );
    let rejection_deadline = rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS;
    db.execute("DELETE FROM directory_hosted_nonterminal_games", &[])
        .unwrap();
    rt.now.set(rt.now.get() + 10);
    let released = operation(&rt, 2);
    service.acquire_removal(released, account()).unwrap();
    service.release_removal(released, account()).unwrap();
    assert_eq!(service.next_deadline(), Ok(Some(rejection_deadline)));
    rt.now.set(rejection_deadline);
    service.cleanup().unwrap();
    assert!(
        db.query("SELECT * FROM directory_removal_rejections", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(service.next_deadline(), Ok(Some(rejection_deadline + 10)));
    assert_eq!(
        service.acquire_removal(rejected, account()),
        Err(directory::DirectoryError::StaleOperation)
    );
}

#[test]
fn retention_cleanup_keeps_pending_intent_and_gate_without_a_spin_deadline() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    let grant = service.acquire_removal(op, account()).unwrap();
    let pending = db
        .query("SELECT * FROM directory_removal_pending", &[])
        .unwrap();
    let gates = db
        .query("SELECT * FROM account_assignment_gates", &[])
        .unwrap();
    rt.now
        .set(rt.now.get() + 2 * limits::COMMAND_RECEIPT_RETENTION_MS);
    for _ in 0..3 {
        service.cleanup().unwrap();
        assert_eq!(service.next_deadline(), Ok(None));
        assert_eq!(
            db.query("SELECT * FROM directory_removal_pending", &[])
                .unwrap(),
            pending
        );
        assert_eq!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap(),
            gates
        );
    }
    assert_eq!(service.acquire_removal(op, account()), Ok(grant));
}

#[test]
fn retention_cleanup_drains_bounded_batches_without_hiding_expired_backlog() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let total = 2 * limits::CLEANUP_BATCH_SIZE + 1;
    for n in 1..=total {
        let op = operation(&rt, u8::try_from(n).unwrap());
        service.acquire_removal(op, account()).unwrap();
        service.release_removal(op, account()).unwrap();
    }
    let issued = rt.now.get();
    rt.now.set(issued + 1);
    service
        .seed_nonterminal_assignment(
            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
            account(),
        )
        .unwrap();
    for n in 1..=total {
        let op = operation(&rt, u8::try_from(n).unwrap());
        assert_eq!(
            service.acquire_removal(op, account()),
            Err(directory::DirectoryError::HostedGame)
        );
    }
    rt.now
        .set(issued + limits::COMMAND_RECEIPT_RETENTION_MS + 5);
    let due = rt.now.get();
    assert_eq!(service.next_deadline(), Ok(Some(due)));
    for remaining in [limits::CLEANUP_BATCH_SIZE + 1, 1, 0] {
        service.cleanup().unwrap();
        for table in ["directory_removal_receipts", "directory_removal_rejections"] {
            assert_eq!(
                db.query(&format!("SELECT count(*) FROM {table}"), &[])
                    .unwrap(),
                vec![vec![SqlValue::Integer(remaining)]]
            );
        }
        assert_eq!(
            service.next_deadline(),
            Ok(if remaining == 0 { None } else { Some(due) })
        );
    }
    assert_eq!(
        db.query("SELECT command_floor_ms FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(issued + 5)]]
    );
}

#[test]
fn retention_malformed_completion_never_erases_evidence_or_releases_a_gate() {
    for table in ["directory_removal_receipts", "directory_removal_rejections"] {
        for corrupt in [
            "operation",
            "account",
            "before_issue",
            "negative",
            "future",
            "unsafe",
        ] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            migrate_directory(&db).unwrap();
            let service = DirectoryService::new(&db, &rt).unwrap();
            let issued = rt.now.get();
            let old = operation(&rt, 1);
            service.reconcile_removal(old, account()).unwrap();
            if table == "directory_removal_rejections" {
                db.execute("DELETE FROM directory_removal_receipts", &[])
                    .unwrap();
                db.execute("INSERT INTO directory_removal_rejections(operation_id,account_id,completed_at) VALUES(?,?,?)", &[SqlValue::Text(old.to_string()),SqlValue::Text(account().to_string()),SqlValue::Integer(issued)]).unwrap();
            }
            let active = operation(&rt, 2);
            service.acquire_removal(active, account()).unwrap();
            rt.now.set(issued + limits::COMMAND_RECEIPT_RETENTION_MS);
            let update = match corrupt {
                "operation" => format!("UPDATE {table} SET operation_id='invalid-operation'"),
                "account" => format!("UPDATE {table} SET account_id='invalid-account'"),
                _ => {
                    let at = match corrupt {
                        "before_issue" => issued - 1,
                        "negative" => -1,
                        "future" => rt.now.get() + 1,
                        _ => limits::JS_SAFE_INTEGER_MAX + 1,
                    };
                    format!("UPDATE {table} SET completed_at={at}")
                }
            };
            db.execute(&update, &[]).unwrap();
            let metadata = db.query("SELECT * FROM directory_metadata", &[]).unwrap();
            let rows = db.query(&format!("SELECT * FROM {table}"), &[]).unwrap();
            let pending = db
                .query("SELECT * FROM directory_removal_pending", &[])
                .unwrap();
            let gates = db
                .query("SELECT * FROM account_assignment_gates", &[])
                .unwrap();
            assert_eq!(
                service.next_deadline(),
                Err(directory::DirectoryError::Storage),
                "{table} {corrupt}"
            );
            assert_eq!(
                db.query("SELECT * FROM directory_metadata", &[]).unwrap(),
                metadata
            );
            // Future/unsafe evidence is not a due deletion candidate.
            if !matches!(corrupt, "future" | "unsafe") {
                assert_eq!(
                    service.cleanup(),
                    Err(directory::DirectoryError::Storage),
                    "{table} {corrupt}"
                );
                assert_eq!(
                    db.query("SELECT * FROM directory_metadata", &[]).unwrap(),
                    metadata
                );
            }
            assert_eq!(
                db.query(&format!("SELECT * FROM {table}"), &[]).unwrap(),
                rows
            );
            assert_eq!(
                db.query("SELECT * FROM directory_removal_pending", &[])
                    .unwrap(),
                pending
            );
            assert_eq!(
                db.query("SELECT * FROM account_assignment_gates", &[])
                    .unwrap(),
                gates
            );
        }
    }
}

#[test]
fn retention_methods_fail_closed_for_invalid_clocks_and_owner_metadata() {
    for corrupt in [
        "negative_clock",
        "unsafe_clock",
        "regressed_clock",
        "missing",
        "older_schema",
        "higher_schema",
        "negative_last",
        "unsafe_last",
        "negative_floor",
        "floor_after_last",
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let issued = rt.now.get();
        service
            .reconcile_removal(operation(&rt, 1), account())
            .unwrap();
        service
            .acquire_removal(operation(&rt, 2), account())
            .unwrap();
        let error = match corrupt {
            "negative_clock" => {
                rt.now.set(-1);
                directory::DirectoryError::Clock
            }
            "unsafe_clock" => {
                rt.now.set(limits::JS_SAFE_INTEGER_MAX + 1);
                directory::DirectoryError::Clock
            }
            "regressed_clock" => {
                rt.now.set(issued - 1);
                directory::DirectoryError::Clock
            }
            _ => {
                rt.now.set(issued + limits::COMMAND_RECEIPT_RETENTION_MS);
                let update = match corrupt {
                    "missing" => "DELETE FROM directory_metadata".to_owned(),
                    "older_schema" => "UPDATE directory_metadata SET schema_version=0".to_owned(),
                    "higher_schema" => "UPDATE directory_metadata SET schema_version=3".to_owned(),
                    "negative_last" => {
                        "UPDATE directory_metadata SET last_observed_ms=-1".to_owned()
                    }
                    "unsafe_last" => format!(
                        "UPDATE directory_metadata SET last_observed_ms={}",
                        limits::JS_SAFE_INTEGER_MAX + 1
                    ),
                    "negative_floor" => {
                        "UPDATE directory_metadata SET command_floor_ms=-1".to_owned()
                    }
                    _ => format!(
                        "UPDATE directory_metadata SET command_floor_ms={}",
                        issued + 1
                    ),
                };
                db.execute(&update, &[]).unwrap();
                directory::DirectoryError::Storage
            }
        };
        let metadata = db.query("SELECT * FROM directory_metadata", &[]).unwrap();
        let receipt = db
            .query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap();
        let pending = db
            .query("SELECT * FROM directory_removal_pending", &[])
            .unwrap();
        let gates = db
            .query("SELECT * FROM account_assignment_gates", &[])
            .unwrap();
        assert_eq!(service.next_deadline(), Err(error), "{corrupt}");
        assert_eq!(service.cleanup(), Err(error), "{corrupt}");
        assert_eq!(
            db.query("SELECT * FROM directory_metadata", &[]).unwrap(),
            metadata
        );
        assert_eq!(
            db.query("SELECT * FROM directory_removal_receipts", &[])
                .unwrap(),
            receipt
        );
        assert_eq!(
            db.query("SELECT * FROM directory_removal_pending", &[])
                .unwrap(),
            pending
        );
        assert_eq!(
            db.query("SELECT * FROM account_assignment_gates", &[])
                .unwrap(),
            gates
        );
    }
}

#[test]
fn retention_cleanup_delete_failure_rolls_back_both_batches_and_the_floor() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    service
        .reconcile_removal(operation(&rt, 1), account())
        .unwrap();
    service
        .seed_nonterminal_assignment(
            "01890f3e-53b7-7d28-9b05-4f65092d5799".parse().unwrap(),
            account(),
        )
        .unwrap();
    assert_eq!(
        service.acquire_removal(operation(&rt, 2), account()),
        Err(directory::DirectoryError::HostedGame)
    );
    let metadata = db.query("SELECT * FROM directory_metadata", &[]).unwrap();
    let receipts = db
        .query("SELECT * FROM directory_removal_receipts", &[])
        .unwrap();
    let rejections = db
        .query("SELECT * FROM directory_removal_rejections", &[])
        .unwrap();
    rt.now
        .set(rt.now.get() + limits::COMMAND_RECEIPT_RETENTION_MS);
    db.conn.borrow().execute_batch("CREATE TRIGGER refuse_cleanup BEFORE DELETE ON directory_removal_rejections BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;").unwrap();
    assert_eq!(service.cleanup(), Err(directory::DirectoryError::Storage));
    assert_eq!(
        db.query("SELECT * FROM directory_metadata", &[]).unwrap(),
        metadata
    );
    assert_eq!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap(),
        receipts
    );
    assert_eq!(
        db.query("SELECT * FROM directory_removal_rejections", &[])
            .unwrap(),
        rejections
    );
    db.execute("DROP TRIGGER refuse_cleanup", &[]).unwrap();
    service.cleanup().unwrap();
    assert!(
        db.query("SELECT * FROM directory_removal_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT * FROM directory_removal_rejections", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(service.next_deadline(), Ok(None));
}

#[test]
fn retention_malformed_rejection_rolls_back_already_compacted_receipts() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let issued = rt.now.get();
    service
        .reconcile_removal(operation(&rt, 1), account())
        .unwrap();
    db.execute("INSERT INTO directory_removal_rejections(operation_id,account_id,completed_at) VALUES(?,?,?)", &[SqlValue::Text(operation(&rt, 2).to_string()),SqlValue::Text(account().to_string()),SqlValue::Integer(issued - 1)]).unwrap();
    service
        .acquire_removal(operation(&rt, 3), account())
        .unwrap();
    let metadata = db.query("SELECT * FROM directory_metadata", &[]).unwrap();
    rt.now.set(issued + limits::COMMAND_RECEIPT_RETENTION_MS);
    assert_eq!(service.cleanup(), Err(directory::DirectoryError::Storage));
    assert_eq!(
        db.query("SELECT * FROM directory_metadata", &[]).unwrap(),
        metadata
    );
    for table in [
        "directory_removal_receipts",
        "directory_removal_rejections",
        "directory_removal_pending",
        "account_assignment_gates",
    ] {
        assert_eq!(
            db.query(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
    }
}

#[test]
fn retention_cleanup_floor_survives_database_reopen_and_prevents_resurrection() {
    let rt = TestRuntime::new();
    let issued = rt.now.get();
    let old = operation(&rt, 1);
    let active = operation(&rt, 2);
    let path = std::env::temp_dir().join(format!(
        "brews-directory-cleanup-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let open = || {
        let db = Sqlite {
            conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
        };
        db.conn
            .borrow()
            .execute_batch("PRAGMA foreign_keys=ON")
            .unwrap();
        db
    };
    {
        let db = open();
        migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        service.reconcile_removal(old, account()).unwrap();
        service.acquire_removal(active, account()).unwrap();
        rt.now.set(issued + limits::COMMAND_RECEIPT_RETENTION_MS);
        service.cleanup().unwrap();
    }
    {
        let db = open();
        let service = DirectoryService::new(&db, &rt).unwrap();
        assert_eq!(service.next_deadline(), Ok(None));
        assert_eq!(
            db.query("SELECT command_floor_ms FROM directory_metadata", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(issued)]]
        );
        assert_eq!(
            service.acquire_removal(old, account()),
            Err(directory::DirectoryError::StaleOperation)
        );
        assert_eq!(
            service.release_removal(old, account()),
            Err(directory::DirectoryError::StaleOperation)
        );
        assert_eq!(
            service.acquire_removal(active, account()),
            Ok(RemovalGrant {
                operation_id: active,
                account_id: account()
            })
        );
        rt.now.set(rt.now.get() - 1);
        assert_eq!(service.cleanup(), Err(directory::DirectoryError::Clock));
        assert_eq!(
            service.next_deadline(),
            Err(directory::DirectoryError::Clock)
        );
        assert_eq!(
            db.query("SELECT count(*) FROM account_assignment_gates", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
        assert!(
            db.query("SELECT * FROM directory_removal_receipts", &[])
                .unwrap()
                .is_empty()
        );
    }
    std::fs::remove_file(path).unwrap();
}

fn account() -> AccountId {
    "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap()
}
fn operation(rt: &TestRuntime, n: u8) -> OperationId {
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes).to_string().parse().unwrap()
}

#[test]
fn acquire_persists_a_presence_only_gate_with_separate_operation_identity() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let service = DirectoryService::new(&db, &rt).unwrap();
    let op = operation(&rt, 1);
    assert_eq!(
        service.acquire_removal(op, account()),
        Ok(RemovalGrant {
            operation_id: op,
            account_id: account()
        })
    );
    assert_eq!(
        db.query("SELECT * FROM account_assignment_gates", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(account().to_string())]]
    );
    assert_eq!(
        db.query(
            "SELECT operation_id,account_id FROM directory_removal_pending",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text(op.to_string()),
            SqlValue::Text(account().to_string())
        ]]
    );
}

#[test]
fn matching_acquire_retry_survives_service_restart() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let op = operation(&rt, 2);
    let expected = DirectoryService::new(&db, &rt)
        .unwrap()
        .acquire_removal(op, account())
        .unwrap();
    let restarted = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(restarted.acquire_removal(op, account()), Ok(expected));
    assert_eq!(
        db.query("SELECT count(*) FROM account_assignment_gates", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}
