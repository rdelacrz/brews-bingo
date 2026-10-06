#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
// Compile this owned module directly until the parent wires module declarations.
#[path = "../src/db/game_delivery.rs"]
mod delivery;
mod support;
use brews_backend::{auth, db};
use db::{Database, SqlValue};
use delivery::{
    DeliveryBinding, DeliveryBudget, DeliveryError, DeliveryService, migrate_game_delivery,
};
use support::{Sqlite, TestRuntime};

#[test]
fn migration_initializes_only_normalized_strict_transport_metadata() {
    let db = Sqlite::new();
    let result = migrate_game_delivery(&db);
    assert_eq!(result, Ok(()), "delivery schema must initialize");
    assert_eq!(
        db.query(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
            &[]
        )
        .unwrap(),
        vec![
            vec![SqlValue::Text("game_delivery_connections".into())],
            vec![SqlValue::Text("game_delivery_metadata".into())],
            vec![SqlValue::Text("game_delivery_pending".into())],
        ]
    );
    assert_eq!(
        db.query(
            "SELECT count(*) FROM pragma_table_list WHERE name GLOB 'game_delivery_*' AND strict=1",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(3)]]
    );
    let columns = db
        .query(
            "SELECT name FROM pragma_table_info('game_delivery_pending')",
            &[],
        )
        .unwrap();
    assert_eq!(
        columns,
        [
            "delivery_id",
            "connection_id",
            "frame_bytes",
            "view_revision",
            "issued_at"
        ]
        .map(|s| vec![SqlValue::Text(s.into())])
    );
    migrate_game_delivery(&db).unwrap();
}
#[test]
fn migration_rejects_unknown_version_without_resetting_metadata() {
    let db = Sqlite::new();
    migrate_game_delivery(&db).unwrap();
    db.execute(
        "UPDATE game_delivery_metadata SET schema_version=2,last_observed_ms=8",
        &[],
    )
    .unwrap();
    assert_eq!(migrate_game_delivery(&db), Err(DeliveryError::Storage));
    assert_eq!(
        db.query("SELECT last_observed_ms FROM game_delivery_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(8)]]
    );
}
#[test]
fn migration_rejects_partial_or_drifted_delivery_group() {
    for damaged in [
        "DROP TABLE game_delivery_pending",
        "DROP TABLE game_delivery_connections",
        "CREATE TABLE game_delivery_extra(x INTEGER) STRICT",
        "DROP INDEX game_delivery_expiry",
    ] {
        let db = Sqlite::new();
        migrate_game_delivery(&db).unwrap();
        db.execute(damaged, &[]).unwrap();
        assert_eq!(
            migrate_game_delivery(&db),
            Err(DeliveryError::Storage),
            "{damaged}"
        );
    }
    let db = Sqlite::new();
    db.execute("CREATE TABLE game_delivery_pending(x INTEGER) STRICT", &[])
        .unwrap();
    assert_eq!(migrate_game_delivery(&db), Err(DeliveryError::Storage));
    assert!(
        db.query(
            "SELECT name FROM sqlite_master WHERE name='game_delivery_metadata'",
            &[]
        )
        .unwrap()
        .is_empty()
    );
}

fn binding(rt: &TestRuntime) -> DeliveryBinding {
    DeliveryBinding {
        connection_id: "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap(),
        session_id: "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
        auth_epoch: 1,
        expires_at: rt.now.get() + 1000,
    }
}
#[test]
fn registration_binds_exact_current_authority_metadata() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    assert_eq!(
        s.register(b),
        Ok(DeliveryBudget {
            pending_count: 0,
            outstanding_bytes: 0
        })
    );
    assert_eq!(
        db.query(
            "SELECT connection_id,session_id,auth_epoch,expires_at FROM game_delivery_connections",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text(b.connection_id.to_string()),
            SqlValue::Text(b.session_id.to_string()),
            SqlValue::Integer(1),
            SqlValue::Integer(b.expires_at)
        ]]
    );
}

#[test]
fn registration_rejects_expired_or_unsafe_authority_metadata() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    assert_eq!(
        s.register(DeliveryBinding {
            expires_at: rt.now.get(),
            ..b
        }),
        Err(DeliveryError::Expired)
    );
    assert_eq!(
        s.register(DeliveryBinding {
            auth_epoch: 9_007_199_254_740_992,
            ..b
        }),
        Err(DeliveryError::InvalidInput)
    );
    assert_eq!(
        s.register(DeliveryBinding {
            expires_at: 9_007_199_254_740_992,
            ..b
        }),
        Err(DeliveryError::InvalidInput)
    );
    assert!(
        db.query("SELECT connection_id FROM game_delivery_connections", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn trusted_clock_floor_survives_service_reconstruction_and_rejects_rollback() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let b = binding(&rt);
    DeliveryService::new(&db, &rt).unwrap().register(b).unwrap();
    rt.now.set(rt.now.get() - 1);
    assert_eq!(
        DeliveryService::new(&db, &rt).unwrap().register(b),
        Err(DeliveryError::Clock)
    );
    for now in [-1, 9_007_199_254_740_992] {
        rt.now.set(now);
        assert_eq!(
            DeliveryService::new(&db, &rt).unwrap().register(b),
            Err(DeliveryError::Clock)
        );
    }
    assert_eq!(
        db.query("SELECT last_observed_ms FROM game_delivery_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(b.expires_at - 1000)]]
    );
}

#[test]
fn reservation_persists_generated_id_and_exact_frame_credit_before_emission() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let reserved = s.reserve_delivery(b, 7, 100).unwrap();
    assert_eq!(
        reserved.budget,
        DeliveryBudget {
            pending_count: 1,
            outstanding_bytes: 100
        }
    );
    assert_eq!(reserved.delivery_id.as_str().len(), 43);
    assert_eq!(db.query("SELECT delivery_id,connection_id,frame_bytes,view_revision,issued_at FROM game_delivery_pending",&[]).unwrap(),vec![vec![SqlValue::Text(reserved.delivery_id.as_str().into()),SqlValue::Text(b.connection_id.to_string()),SqlValue::Integer(100),SqlValue::Integer(7),SqlValue::Integer(rt.now.get())]]);
    assert_eq!(s.budget(b), Ok(reserved.budget));
    migrate_game_delivery(&db).unwrap();
    assert_eq!(s.budget(b), Ok(reserved.budget));
}

#[test]
fn repeated_registration_preserves_credit_and_rejects_rebinding() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let reserved = s.reserve_delivery(b, 1, 77).unwrap();
    assert_eq!(s.register(b), Ok(reserved.budget));
    for wrong in [
        DeliveryBinding { auth_epoch: 2, ..b },
        DeliveryBinding {
            expires_at: b.expires_at + 1,
            ..b
        },
        DeliveryBinding {
            session_id: "01890f3e-53b7-7d28-9b05-4f65092d5713".parse().unwrap(),
            ..b
        },
    ] {
        assert_eq!(s.register(wrong), Err(DeliveryError::BindingMismatch));
        assert_eq!(
            s.reserve_delivery(wrong, 2, 10),
            Err(DeliveryError::BindingMismatch)
        );
        assert_eq!(s.budget(wrong), Err(DeliveryError::BindingMismatch));
    }
    assert_eq!(s.budget(b), Ok(reserved.budget));
}

#[test]
fn reservation_rejects_invalid_frame_sizes_without_new_credit() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    for n in [0, 256 * 1024 + 1, usize::MAX] {
        assert_eq!(
            s.reserve_delivery(b, 1, n),
            Err(DeliveryError::InvalidInput)
        );
    }
    assert_eq!(
        s.reserve_delivery(b, 9_007_199_254_740_992, 1),
        Err(DeliveryError::InvalidInput)
    );
    assert_eq!(
        s.budget(b),
        Ok(DeliveryBudget {
            pending_count: 0,
            outstanding_bytes: 0
        })
    );
}

#[test]
fn reservation_caps_credit_at_one_mib_without_overwrite() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    for n in 0..4 {
        s.reserve_delivery(b, n, 256 * 1024).unwrap();
    }
    let full = DeliveryBudget {
        pending_count: 4,
        outstanding_bytes: 1024 * 1024,
    };
    assert_eq!(s.budget(b), Ok(full));
    assert_eq!(s.reserve_delivery(b, 4, 1), Err(DeliveryError::Capacity));
    assert_eq!(s.budget(b), Ok(full));
}

#[test]
fn reservation_caps_small_frame_backlog_without_spilling() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    for n in 0..64 {
        s.reserve_delivery(b, n, 1).unwrap();
    }
    assert_eq!(s.reserve_delivery(b, 64, 1), Err(DeliveryError::Capacity));
    assert_eq!(
        s.budget(b),
        Ok(DeliveryBudget {
            pending_count: 64,
            outstanding_bytes: 64
        })
    );
}

#[test]
fn reservation_retries_random_collisions_with_a_finite_bound() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let first = s.reserve_delivery(b, 1, 12).unwrap();
    rt.random.set(1);
    let next = s.reserve_delivery(b, 2, 13);
    assert!(
        next.is_ok(),
        "collision must use fresh runtime entropy, not overwrite"
    );
    assert_ne!(next.unwrap().delivery_id, first.delivery_id);
    struct Fixed;
    impl auth::Runtime for Fixed {
        fn now_ms(&self) -> i64 {
            1_800_000_000_000
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), auth::AuthError> {
            bytes.fill(0);
            Ok(())
        }
    }
    let fixed = Fixed;
    let fixeds = DeliveryService::new(&db, &fixed).unwrap();
    fixeds.reserve_delivery(b, 3, 14).unwrap();
    assert_eq!(
        fixeds.reserve_delivery(b, 4, 15),
        Err(DeliveryError::Random)
    );
    rt.fail.set(true);
    assert_eq!(s.reserve_delivery(b, 4, 15), Err(DeliveryError::Random));
    assert_eq!(
        s.budget(b),
        Ok(DeliveryBudget {
            pending_count: 3,
            outstanding_bytes: 39
        })
    );
}

#[test]
fn reservation_rechecks_expiry_after_runtime_entropy() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    rt.advance_random_ms.set(1000);
    assert_eq!(s.reserve_delivery(b, 1, 10), Err(DeliveryError::Expired));
    assert!(
        db.query("SELECT delivery_id FROM game_delivery_pending", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(s.budget(b), Err(DeliveryError::Expired));
}

fn ack(
    b: DeliveryBinding,
    id: brews_contracts::games::DeliveryId,
    rev: u64,
) -> brews_contracts::games::SnapshotAck {
    brews_contracts::games::SnapshotAck {
        version: 1,
        kind: brews_contracts::games::SnapshotAckKind::SnapshotAck,
        connection_id: b.connection_id,
        delivery_id: id,
        view_revision: rev,
    }
}
#[test]
fn acknowledgement_releases_only_exact_outstanding_delivery() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let first = s.reserve_delivery(b, 1, 100).unwrap();
    let second = s.reserve_delivery(b, 2, 200).unwrap();
    let a = ack(b, first.delivery_id, 1);
    assert_eq!(
        s.acknowledge(b, &a),
        Ok(DeliveryBudget {
            pending_count: 1,
            outstanding_bytes: 200
        })
    );
    assert_eq!(s.acknowledge(b, &a), Err(DeliveryError::UnknownDelivery));
    assert_eq!(
        s.acknowledge(b, &ack(b, second.delivery_id, 2)),
        Ok(DeliveryBudget {
            pending_count: 0,
            outstanding_bytes: 0
        })
    );
}

#[test]
fn forged_ack_never_frees_credit_on_wrong_revision_connection_or_epoch() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let res = s.reserve_delivery(b, 7, 100).unwrap();
    let a = ack(b, res.delivery_id.clone(), 8);
    assert_eq!(s.acknowledge(b, &a), Err(DeliveryError::RevisionMismatch));
    let other = DeliveryBinding {
        connection_id: "01890f3e-53b7-7d28-9b05-4f65092d5714".parse().unwrap(),
        ..b
    };
    s.register(other).unwrap();
    assert_eq!(
        s.acknowledge(other, &ack(other, res.delivery_id.clone(), 7)),
        Err(DeliveryError::BindingMismatch)
    );
    assert_eq!(
        s.acknowledge(b, &ack(other, res.delivery_id.clone(), 7)),
        Err(DeliveryError::BindingMismatch)
    );
    assert_eq!(
        s.acknowledge(
            DeliveryBinding { auth_epoch: 2, ..b },
            &ack(b, res.delivery_id.clone(), 7)
        ),
        Err(DeliveryError::BindingMismatch)
    );
    assert_eq!(
        s.acknowledge(
            DeliveryBinding {
                session_id: other.connection_id.to_string().parse().unwrap(),
                ..b
            },
            &ack(b, res.delivery_id.clone(), 7)
        ),
        Err(DeliveryError::BindingMismatch)
    );
    let unknown = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        .parse()
        .unwrap();
    assert_eq!(
        s.acknowledge(b, &ack(b, unknown, 7)),
        Err(DeliveryError::UnknownDelivery)
    );
    let mut a = ack(b, res.delivery_id.clone(), 7);
    a.version = 2;
    assert_eq!(s.acknowledge(b, &a), Err(DeliveryError::InvalidInput));
    assert_eq!(s.budget(b), Ok(res.budget));
    rt.now.set(b.expires_at);
    assert_eq!(
        s.acknowledge(b, &ack(b, res.delivery_id, 7)),
        Err(DeliveryError::Expired)
    );
    assert_eq!(
        db.query("SELECT count(*) FROM game_delivery_pending", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn ack_refuses_corrupt_future_pending_metadata_without_releasing_credit() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let res = s.reserve_delivery(b, 7, 100).unwrap();
    db.execute(
        "UPDATE game_delivery_pending SET issued_at=?",
        &[SqlValue::Integer(rt.now.get() + 1)],
    )
    .unwrap();
    assert_eq!(
        s.acknowledge(b, &ack(b, res.delivery_id, 7)),
        Err(DeliveryError::Storage)
    );
    assert_eq!(
        db.query("SELECT count(*) FROM game_delivery_pending", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert_eq!(s.budget(b), Err(DeliveryError::Storage));
}

#[test]
fn ignored_sql_writes_never_claim_registration_reservation_or_ack_success() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    db.execute("CREATE TRIGGER ignore_registration BEFORE INSERT ON game_delivery_connections BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(s.register(b), Err(DeliveryError::Storage));
    db.execute("DROP TRIGGER ignore_registration", &[]).unwrap();
    s.register(b).unwrap();
    db.execute("CREATE TRIGGER ignore_reservation BEFORE INSERT ON game_delivery_pending BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(s.reserve_delivery(b, 1, 10), Err(DeliveryError::Storage));
    db.execute("DROP TRIGGER ignore_reservation", &[]).unwrap();
    let res = s.reserve_delivery(b, 1, 10).unwrap();
    db.execute("CREATE TRIGGER ignore_ack BEFORE DELETE ON game_delivery_pending BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(
        s.acknowledge(b, &ack(b, res.delivery_id, 1)),
        Err(DeliveryError::Storage)
    );
    assert_eq!(s.budget(b), Ok(res.budget));
}

#[test]
fn retirement_purges_only_the_exact_closed_connection_binding() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let res = s.reserve_delivery(b, 1, 10).unwrap();
    let replacement = DeliveryBinding {
        connection_id: "01890f3e-53b7-7d28-9b05-4f65092d5714".parse().unwrap(),
        ..b
    };
    s.register(replacement).unwrap();
    let replres = s.reserve_delivery(replacement, 2, 20).unwrap();
    assert_eq!(
        s.retire_connection(DeliveryBinding { auth_epoch: 2, ..b }),
        Err(DeliveryError::BindingMismatch)
    );
    assert_eq!(s.retire_connection(b), Ok(()));
    assert_eq!(s.retire_connection(b), Ok(()));
    assert_eq!(s.budget(replacement), Ok(replres.budget));
    assert_eq!(
        s.acknowledge(b, &ack(b, res.delivery_id, 1)),
        Err(DeliveryError::BindingMismatch)
    );
    assert_eq!(
        db.query("SELECT count(*) FROM game_delivery_pending", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn expiry_cleanup_is_bounded_and_never_renews_late_ack_authority() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let res = s.reserve_delivery(b, 1, 10).unwrap();
    assert_eq!(s.next_deadline(), Ok(Some(b.expires_at)));
    rt.now.set(b.expires_at - 1);
    assert_eq!(s.cleanup_expired(), Ok(0));
    rt.now.set(b.expires_at);
    assert_eq!(
        s.acknowledge(b, &ack(b, res.delivery_id, 1)),
        Err(DeliveryError::Expired)
    );
    assert_eq!(s.next_deadline(), Ok(Some(b.expires_at)));
    assert_eq!(s.cleanup_expired(), Ok(1));
    assert_eq!(s.next_deadline(), Ok(None));
    assert!(
        db.query("SELECT delivery_id FROM game_delivery_pending", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(s.register(b), Err(DeliveryError::Expired));
    for n in 0..101 {
        let id = format!("01890f3e-53b7-7d28-9b05-{n:012x}");
        db.execute(
            "INSERT INTO game_delivery_connections VALUES(?,?,1,?)",
            &[
                SqlValue::Text(id),
                SqlValue::Text(b.session_id.to_string()),
                SqlValue::Integer(b.expires_at),
            ],
        )
        .unwrap();
    }
    assert_eq!(s.cleanup_expired(), Ok(100));
    assert_eq!(s.next_deadline(), Ok(Some(b.expires_at)));
    assert_eq!(s.cleanup_expired(), Ok(1));
    assert_eq!(s.next_deadline(), Ok(None));
}

#[test]
fn sqlite_reopen_preserves_budget_delivery_identity_and_clock_floor() {
    let rt = TestRuntime::new();
    let b = binding(&rt);
    let path = std::env::temp_dir().join(format!(
        "game-delivery-{}-{}.sqlite",
        std::process::id(),
        b.connection_id
    ));
    let _ = std::fs::remove_file(&path);
    let first;
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        let db = Sqlite {
            conn: std::cell::RefCell::new(conn),
        };
        migrate_game_delivery(&db).unwrap();
        let s = DeliveryService::new(&db, &rt).unwrap();
        s.register(b).unwrap();
        first = s.reserve_delivery(b, 7, 100).unwrap();
    }
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        let db = Sqlite {
            conn: std::cell::RefCell::new(conn),
        };
        migrate_game_delivery(&db).unwrap();
        let s = DeliveryService::new(&db, &rt).unwrap();
        assert_eq!(s.budget(b), Ok(first.budget));
        rt.now.set(rt.now.get() - 1);
        assert_eq!(
            s.acknowledge(b, &ack(b, first.delivery_id.clone(), 7)),
            Err(DeliveryError::Clock)
        );
        rt.now.set(rt.now.get() + 1);
        assert_eq!(
            s.acknowledge(b, &ack(b, first.delivery_id, 7)),
            Ok(DeliveryBudget {
                pending_count: 0,
                outstanding_bytes: 0
            })
        );
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn sql_fault_rolls_back_credit_and_binding_mutations_atomically() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let res = s.reserve_delivery(b, 7, 100).unwrap();
    db.execute("CREATE TRIGGER fail_reserve BEFORE INSERT ON game_delivery_pending BEGIN SELECT RAISE(ABORT,'fault'); END",&[]).unwrap();
    assert_eq!(s.reserve_delivery(b, 8, 200), Err(DeliveryError::Storage));
    assert_eq!(s.budget(b), Ok(res.budget));
    db.execute("DROP TRIGGER fail_reserve", &[]).unwrap();
    db.execute("CREATE TRIGGER fail_ack BEFORE DELETE ON game_delivery_pending BEGIN SELECT RAISE(ABORT,'fault'); END",&[]).unwrap();
    assert_eq!(
        s.acknowledge(b, &ack(b, res.delivery_id, 7)),
        Err(DeliveryError::Storage)
    );
    assert_eq!(s.budget(b), Ok(res.budget));
    db.execute("DROP TRIGGER fail_ack", &[]).unwrap();
    db.execute("CREATE TRIGGER fail_retire BEFORE DELETE ON game_delivery_connections BEGIN SELECT RAISE(ABORT,'fault'); END",&[]).unwrap();
    assert_eq!(s.retire_connection(b), Err(DeliveryError::Storage));
    assert_eq!(s.budget(b), Ok(res.budget));
}
#[test]
fn ignored_clock_and_cleanup_writes_fail_closed_without_retiring_credit() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_game_delivery(&db).unwrap();
    let s = DeliveryService::new(&db, &rt).unwrap();
    let b = binding(&rt);
    s.register(b).unwrap();
    let res = s.reserve_delivery(b, 7, 100).unwrap();
    db.execute("CREATE TRIGGER ignore_clock BEFORE UPDATE ON game_delivery_metadata BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    rt.now.set(rt.now.get() + 1);
    assert_eq!(s.budget(b), Err(DeliveryError::Storage));
    db.execute("DROP TRIGGER ignore_clock", &[]).unwrap();
    assert_eq!(s.budget(b), Ok(res.budget));
    db.execute("CREATE TRIGGER ignore_retire BEFORE DELETE ON game_delivery_connections BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(s.retire_connection(b), Err(DeliveryError::Storage));
    assert_eq!(s.budget(b), Ok(res.budget));
    rt.now.set(b.expires_at);
    assert_eq!(s.cleanup_expired(), Err(DeliveryError::Storage));
    assert_eq!(
        db.query("SELECT count(*) FROM game_delivery_pending", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}
