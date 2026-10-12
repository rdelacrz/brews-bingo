use super::*;
use brews_backend::auth::{AuthError, Runtime};
use brews_backend::db::game::migrate_game;
use brews_contracts::games::GameOutcome;
use std::cell::{Cell, RefCell};
use zeroize::Zeroizing;

fn prepared() -> (
    Fixture,
    brews_domain::games::GameCode,
    brews_backend::game::SecretCookie,
    Zeroizing<String>,
) {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let player = f.player(&code, 30, "RecoverPlayer");
    let answer = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), player.token(), Some(&answer))
        .unwrap();
    (f, code, player, answer)
}
fn recover(
    f: &Fixture,
    code: &brews_domain::games::GameCode,
    answer: &str,
    n: u8,
) -> Result<brews_backend::db::game::RecoveryOutcome, GameError> {
    f.service()
        .recover_player(f.command(n), code, "RecoverPlayer", answer, "192.0.2.41")
}
fn epoch(f: &Fixture) -> i64 {
    match f
        .game
        .query("SELECT session_epoch FROM game_players", &[])
        .unwrap()[0][0]
    {
        SqlValue::Integer(epoch) => epoch,
        _ => panic!("epoch"),
    }
}
#[test]
fn restores_retained_identity_and_board_with_fixed_session_and_fresh_proof_only_retry() {
    let (f, code, old, answer) = prepared();
    let before = f
        .game
        .query(
            "SELECT player_id,alias,joined_at,last_explicit_leave_at FROM game_players",
            &[],
        )
        .unwrap();
    let idle = f
        .game
        .query("SELECT idle_due,last_host_activity FROM game_record", &[])
        .unwrap();
    let recovered = recover(&f, &code, &answer, 32).unwrap();
    let new = recovered.cookie.unwrap();
    assert_eq!(epoch(&f), 1);
    assert_eq!(new.expires_at(), f.rt.now.get() + 86_400_000);
    assert_eq!(
        f.service().player_view(old.token()).err(),
        Some(GameError::Unauthorized)
    );
    assert!(matches!(
        f.service().recovered_player_view(new.token()).unwrap(),
        GameView::Player { board: None, .. }
    ));
    assert_eq!(
        before,
        f.game
            .query(
                "SELECT player_id,alias,joined_at,last_explicit_leave_at FROM game_players",
                &[]
            )
            .unwrap()
    );
    assert_eq!(
        idle,
        f.game
            .query("SELECT idle_due,last_host_activity FROM game_record", &[])
            .unwrap()
    );
    assert!(recover(&f, &code, &answer, 32).unwrap().cookie.is_none());
    let wrong = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    assert_eq!(
        recover(&f, &code, &wrong, 32).err(),
        Some(GameError::InvalidRecovery)
    );
    assert_eq!(epoch(&f), 1);
    assert_eq!(
        recover(&f, &code, &format!(" {} ", answer.as_str()), 32).err(),
        Some(GameError::Conflict)
    );
    assert!(recover(&f, &code, &answer, 33).unwrap().cookie.is_some());
    assert_eq!(epoch(&f), 2);
    assert_eq!(
        f.service().player_view(new.token()).err(),
        Some(GameError::Unauthorized)
    );
}
#[test]
fn exact_current_alias_and_uniform_absent_wrong_disabled_proof() {
    let (f, code, old, answer) = prepared();
    assert_eq!(
        f.service()
            .recover_player(f.command(32), &code, "recoverplayer", &answer, "192.0.2.41")
            .err(),
        Some(GameError::InvalidRecovery)
    );
    assert!(
        f.service()
            .recover_player(
                f.command(33),
                &code,
                " RecoverPlayer ",
                &answer,
                "192.0.2.41"
            )
            .unwrap()
            .cookie
            .is_some()
    );
    assert_eq!(
        f.service()
            .manage_recovery_answer(f.command(34), old.token(), None)
            .err(),
        Some(GameError::Unauthorized)
    );
    let fresh = recover(&f, &code, &answer, 35).unwrap().cookie.unwrap();
    f.service()
        .manage_recovery_answer(f.command(36), fresh.token(), None)
        .unwrap();
    assert_eq!(
        recover(&f, &code, &answer, 37).err(),
        Some(GameError::InvalidRecovery)
    );
}
#[test]
fn rates_five_fixed_failures_block_without_extension_and_expire_after_restart() {
    let (f, code, _, answer) = prepared();
    let wrong = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    for n in 32..37 {
        assert_eq!(
            recover(&f, &code, &wrong, n).err(),
            Some(GameError::InvalidRecovery)
        );
    }
    let buckets = f
        .game
        .query(
            "SELECT * FROM game_recovery_buckets ORDER BY subject_key",
            &[],
        )
        .unwrap();
    assert_eq!(buckets.len(), 3);
    for bucket in &buckets {
        assert!(matches!(&bucket[0], SqlValue::Blob(key) if key.len()==32));
        assert_eq!(bucket[2], SqlValue::Integer(5));
    }
    f.rt.now.set(f.rt.now.get() + 1);
    assert_eq!(
        recover(&f, &code, &answer, 37).err(),
        Some(GameError::RecoveryBlocked)
    );
    assert_eq!(
        buckets,
        f.game
            .query(
                "SELECT * FROM game_recovery_buckets ORDER BY subject_key",
                &[]
            )
            .unwrap()
    );
    f.rt.now.set(f.rt.now.get() + 900_000);
    migrate_game(&f.game).unwrap();
    assert!(recover(&f, &code, &answer, 38).unwrap().cookie.is_some());
    assert!(
        f.game
            .query("SELECT * FROM game_recovery_buckets", &[])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn rate_deadline_denial_fences_clock_rollback_and_invalid_peer_does_not_count() {
    let (f, code, _, answer) = prepared();
    assert_eq!(
        f.service()
            .recover_player(f.command(32), &code, "RecoverPlayer", &answer, "untrusted")
            .err(),
        Some(GameError::Storage)
    );
    assert!(
        f.game
            .query("SELECT * FROM game_recovery_buckets", &[])
            .unwrap()
            .is_empty()
    );
    let wrong = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    for n in 32..37 {
        let _ = recover(&f, &code, &wrong, n);
    }
    let past = f.rt.now.get();
    f.rt.now.set(past + 50);
    assert_eq!(
        recover(&f, &code, &answer, 37).err(),
        Some(GameError::RecoveryBlocked)
    );
    f.rt.now.set(past);
    assert_eq!(
        recover(&f, &code, &answer, 38).err(),
        Some(GameError::Storage)
    );
}
#[test]
fn private_answer_noops_preserve_host_revision_session_and_idle_deadline() {
    let (f, code, old, answer) = prepared();
    let state = f.game.query("SELECT * FROM game_sessions", &[]).unwrap();
    let host = f
        .game
        .query(
            "SELECT revision FROM game_view_revisions WHERE view_key='host'",
            &[],
        )
        .unwrap();
    let own = f
        .game
        .query(
            "SELECT revision FROM game_view_revisions WHERE view_key<>'host'",
            &[],
        )
        .unwrap();
    let response = f
        .service()
        .manage_recovery_answer(
            f.command(32),
            old.token(),
            Some(&format!(" {} ", answer.as_str())),
        )
        .unwrap();
    assert!(
        matches!(response, GameResponse::RecoveryAnswerChanged { receipt, .. } if matches!(receipt.outcome, GameOutcome::RecoveryAnswerChanged { recovery_enabled: true, .. }))
    );
    assert_eq!(
        own,
        f.game
            .query(
                "SELECT revision FROM game_view_revisions WHERE view_key<>'host'",
                &[]
            )
            .unwrap()
    );
    f.service()
        .manage_recovery_answer(f.command(33), old.token(), None)
        .unwrap();
    let after_delete = f
        .game
        .query(
            "SELECT revision FROM game_view_revisions WHERE view_key<>'host'",
            &[],
        )
        .unwrap();
    f.service()
        .manage_recovery_answer(f.command(34), old.token(), None)
        .unwrap();
    assert_eq!(
        after_delete,
        f.game
            .query(
                "SELECT revision FROM game_view_revisions WHERE view_key<>'host'",
                &[]
            )
            .unwrap()
    );
    assert_eq!(
        host,
        f.game
            .query(
                "SELECT revision FROM game_view_revisions WHERE view_key='host'",
                &[]
            )
            .unwrap()
    );
    assert_eq!(
        state,
        f.game.query("SELECT * FROM game_sessions", &[]).unwrap()
    );
    assert_eq!(
        recover(&f, &code, &answer, 35).err(),
        Some(GameError::InvalidRecovery)
    );
}
struct HookRuntime<'a> {
    rt: &'a TestRuntime,
    hook: RefCell<Option<Box<dyn FnOnce() + 'a>>>,
    called: Cell<bool>,
}
impl Runtime for HookRuntime<'_> {
    fn now_ms(&self) -> i64 {
        self.rt.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        self.rt.fill_random(bytes)
    }
    fn player_recovery_proof_complete(&self) {
        self.called.set(true);
        if let Some(hook) = self.hook.borrow_mut().take() {
            hook();
        }
    }
}
#[test]
fn actual_kdf_gap_answer_replacement_deletion_alias_epoch_and_terminal_changes_fail_closed() {
    for mutation in ["updated", "delete", "alias", "epoch", "terminal"] {
        let (f, code, _, answer) = prepared();
        let sql = match mutation {
            "updated" => "UPDATE game_recovery SET updated_at=updated_at+1",
            "delete" => "DELETE FROM game_recovery",
            "alias" => "UPDATE game_players SET alias='ChangedAlias'",
            "epoch" => "UPDATE game_players SET session_epoch=session_epoch+1",
            _ => "UPDATE game_record SET published_at=NULL",
        };
        let rt = HookRuntime {
            rt: &f.rt,
            hook: RefCell::new(Some(Box::new(|| {
                f.game.execute(sql, &[]).unwrap();
            }))),
            called: Cell::new(false),
        };
        let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        assert_eq!(
            service
                .recover_player(f.command(32), &code, "RecoverPlayer", &answer, "192.0.2.41")
                .err(),
            Some(GameError::InvalidRecovery)
        );
        assert!(rt.called.get());
        assert_eq!(
            f.game
                .query("SELECT count(*) FROM game_sessions", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
        assert!(
            f.game
                .query("SELECT * FROM game_socket_closures", &[])
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn answer_maintenance_kdf_gap_revoked_session_rolls_back_private_update() {
    let (f, _, old, _) = prepared();
    let replacement = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    let original = f.game.query("SELECT * FROM game_recovery", &[]).unwrap();
    let rt = HookRuntime {
        rt: &f.rt,
        hook: RefCell::new(Some(Box::new(|| {
            f.game
                .execute("UPDATE game_players SET session_epoch=session_epoch+1", &[])
                .unwrap();
        }))),
        called: Cell::new(false),
    };
    let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    assert_eq!(
        service
            .manage_recovery_answer(f.command(32), old.token(), Some(&replacement))
            .err(),
        Some(GameError::Unauthorized)
    );
    assert_eq!(
        original,
        f.game.query("SELECT * FROM game_recovery", &[]).unwrap()
    );
}
#[test]
fn recovery_required_writes_ignore_or_abort_roll_back_all_authority_changes() {
    for (table, event) in [
        ("game_players", "UPDATE"),
        ("game_sessions", "UPDATE"),
        ("game_sessions", "INSERT"),
        ("game_receipts", "INSERT"),
        ("game_view_revisions", "UPDATE"),
    ] {
        for mode in ["IGNORE", "ABORT,'test fault'"] {
            let (f, code, old, answer) = prepared();
            let before = f.game.query("SELECT * FROM game_sessions", &[]).unwrap();
            f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER recovery_fault BEFORE {event} ON {table} BEGIN SELECT RAISE({mode}); END")).unwrap();
            assert_eq!(
                recover(&f, &code, &answer, 32).err(),
                Some(GameError::Storage),
                "{table}/{event}/{mode}"
            );
            assert_eq!(epoch(&f), 0);
            assert_eq!(
                before,
                f.game.query("SELECT * FROM game_sessions", &[]).unwrap()
            );
            assert!(f.service().player_view(old.token()).is_ok());
        }
    }
}
#[test]
fn recovery_closure_ledger_survives_reconstruction_and_ack_does_not_delete_successor() {
    let (f, code, old, answer) = prepared();
    let connection = id(&f.rt, 40).parse().unwrap();
    let grant = f
        .service()
        .prepare_player_connection(old.token(), connection)
        .unwrap();
    f.service().accept_player_connection(&grant).unwrap();
    let new = recover(&f, &code, &answer, 32).unwrap().cookie.unwrap();
    let closes = f.service().pending_recovery_socket_closes().unwrap();
    assert_eq!(closes.len(), 1);
    assert_eq!(closes[0].connection, connection);
    migrate_game(&f.game).unwrap();
    assert_eq!(
        closes,
        f.service().pending_recovery_socket_closes().unwrap()
    );
    assert!(
        f.service().player_connection(connection).is_err()
            || f.service().player_connection(connection).unwrap().is_none()
    );
    let successor = f
        .service()
        .prepare_player_connection(new.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&successor).unwrap();
    f.service()
        .acknowledge_recovery_socket_close(&closes[0])
        .unwrap();
    assert!(
        f.service()
            .pending_recovery_socket_closes()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.service()
            .player_connection(successor.connection_id())
            .unwrap(),
        Some(successor)
    );
}

#[test]
fn in_progress_recovery_preserves_complete_board_matching_layout_and_history() {
    let f = Fixture::new();
    let (code, old, other, _) = f.start_two_player_game();
    let answer = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), old.token(), Some(&answer))
        .unwrap();
    let boards = f
        .game
        .query(
            "SELECT * FROM game_board_cells ORDER BY player_id,row,column",
            &[],
        )
        .unwrap();
    let players = f.game.query("SELECT player_id,alias,joined_at,last_explicit_leave_at FROM game_players ORDER BY player_id", &[]).unwrap();
    let recovered = f
        .service()
        .recover_player(f.command(32), &code, "First", &answer, "192.0.2.41")
        .unwrap()
        .cookie
        .unwrap();
    assert!(matches!(
        f.service()
            .recovered_player_view(recovered.token())
            .unwrap(),
        GameView::Player { board: Some(_), .. }
    ));
    assert!(f.service().player_view(other.token()).is_ok());
    assert_eq!(
        boards,
        f.game
            .query(
                "SELECT * FROM game_board_cells ORDER BY player_id,row,column",
                &[]
            )
            .unwrap()
    );
    assert_eq!(players, f.game.query("SELECT player_id,alias,joined_at,last_explicit_leave_at FROM game_players ORDER BY player_id", &[]).unwrap());
}
#[test]
fn entropy_failure_unsupported_stored_profile_and_phc_fail_without_epoch_or_failure_accounting() {
    for mode in ["entropy", "version", "phc"] {
        let (f, code, _, answer) = prepared();
        match mode {
            "entropy" => f.rt.fail.set(true),
            "version" => f.game.conn.borrow().execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE game_recovery SET normalization_version=99; PRAGMA ignore_check_constraints=OFF").unwrap(),
            _ => f.game.execute("UPDATE game_recovery SET answer_verifier=?", &[SqlValue::Text(String::new())]).unwrap(),
        }
        let rt = HookRuntime {
            rt: &f.rt,
            hook: RefCell::new(None),
            called: Cell::new(false),
        };
        let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let error = service
            .recover_player(f.command(32), &code, "RecoverPlayer", &answer, "192.0.2.41")
            .err();
        assert_eq!(
            error,
            Some(if mode == "entropy" {
                GameError::RandomUnavailable
            } else {
                GameError::Storage
            })
        );
        assert_eq!(rt.called.get(), mode == "entropy");
        assert_eq!(epoch(&f), 0);
        assert!(
            f.game
                .query("SELECT * FROM game_recovery_buckets", &[])
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn aged_recovery_commands_are_rejected_after_receipt_cleanup_and_idle_denial_is_uniform() {
    let f = Fixture::new();
    let (code, old, _, _) = f.start_two_player_game();
    let answer = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), old.token(), Some(&answer))
        .unwrap();
    let original = f.command(32);
    f.service()
        .recover_player(original, &code, "First", &answer, "192.0.2.41")
        .unwrap();
    f.rt.now.set(f.rt.now.get() + 86_400_000);
    f.service().cleanup().unwrap();
    assert_eq!(
        f.service()
            .recover_player(original, &code, "First", &answer, "192.0.2.41")
            .err(),
        Some(GameError::StaleCommand)
    );
    assert!(
        f.service()
            .recover_player(f.command(33), &code, "First", &answer, "192.0.2.41")
            .unwrap()
            .cookie
            .is_some()
    );
    let (f, code, _, answer) = prepared();
    f.rt.now.set(f.rt.now.get() + 86_400_000);
    assert_eq!(
        recover(&f, &code, &answer, 32).err(),
        Some(GameError::InvalidRecovery)
    );
}
#[test]
fn recovery_rate_keys_are_domain_and_game_separated_without_raw_alias_or_caller() {
    let (f, code, _, _) = prepared();
    let wrong = Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    let _ = recover(&f, &code, &wrong, 32);
    let first = f
        .game
        .query(
            "SELECT subject_key,attempt_count FROM game_recovery_buckets ORDER BY subject_key",
            &[],
        )
        .unwrap();
    assert_eq!(first.len(), 3);
    let _ = f
        .service()
        .recover_player(f.command(33), &code, "MissingAlias", &wrong, "192.0.2.42");
    let second = f
        .game
        .query(
            "SELECT subject_key,attempt_count FROM game_recovery_buckets ORDER BY subject_key",
            &[],
        )
        .unwrap();
    assert_eq!(second.len(), 5);
    assert_eq!(
        second
            .iter()
            .filter(|r| r[1] == SqlValue::Integer(2))
            .count(),
        1
    );
    let (other, other_code, _, _) = prepared();
    let _ = recover(&other, &other_code, &wrong, 32);
    // Fixture deterministic IDs are identical; vary the key to model independent owner-managed key domains.
    let service =
        GameService::new(&other.game, &other.rt, AuthPolicy::default(), &[8; 32]).unwrap();
    let _ = service.recover_player(
        other.command(33),
        &other_code,
        "RecoverPlayer",
        &wrong,
        "192.0.2.41",
    );
    assert_eq!(
        other
            .game
            .query("SELECT count(*) FROM game_recovery_buckets", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(6)]]
    );
}
#[test]
fn slice_one_schema_forward_migration_preserves_rows_and_rejects_missing_or_altered_namespace() {
    for alteration in ["none", "missing", "altered"] {
        let (f, _, _, _) = prepared();
        let players = f.game.query("SELECT * FROM game_players", &[]).unwrap();
        f.game.conn.borrow().execute_batch("DROP TABLE game_admin_audit; DROP TABLE game_host_transfers; DROP TABLE game_operator_identity; DROP TABLE game_spectator_terminal_work; DROP TABLE game_spectator_notices; DROP TABLE game_spectator_contexts; DROP TABLE game_spectator_sessions; DROP TABLE game_spectators; DROP TABLE game_recovery_buckets; DROP TABLE game_socket_closures; UPDATE game_metadata SET schema_version=2").unwrap();
        match alteration {
            "missing" => f
                .game
                .conn
                .borrow()
                .execute_batch("DROP TABLE game_receipts")
                .unwrap(),
            "altered" => f
                .game
                .conn
                .borrow()
                .execute_batch("ALTER TABLE game_receipts ADD COLUMN unexpected TEXT")
                .unwrap(),
            _ => {}
        }
        assert_eq!(migrate_game(&f.game).is_ok(), alteration == "none");
        assert_eq!(
            players,
            f.game.query("SELECT * FROM game_players", &[]).unwrap()
        );
        if alteration != "none" {
            assert!(
                f.game
                    .query(
                        "SELECT name FROM sqlite_master WHERE name='game_recovery_buckets'",
                        &[]
                    )
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
struct FaultReadback<'a> {
    db: &'a Sqlite,
    write: &'a str,
    read: &'a str,
    armed: Cell<bool>,
}
impl Database for FaultReadback<'_> {
    fn query(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<brews_backend::db::Row>, brews_backend::db::StorageError> {
        if self.armed.get() && sql.contains(self.read) {
            return Err(brews_backend::db::StorageError);
        }
        self.db.query(sql, params)
    }
    fn execute(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<(), brews_backend::db::StorageError> {
        self.db.execute(sql, params)?;
        if sql.contains(self.write) {
            self.armed.set(true);
        }
        Ok(())
    }
    fn transaction<T>(
        &self,
        op: impl FnOnce() -> Result<T, brews_backend::db::StorageError>,
    ) -> Result<T, brews_backend::db::StorageError> {
        self.db.transaction(op)
    }
}
#[test]
fn recovery_authority_readback_faults_after_actual_writes_roll_back_epoch_session_and_receipt() {
    for (write, read) in [
        (
            "UPDATE game_players SET session_epoch",
            "SELECT p.player_id,p.session_epoch",
        ),
        (
            "INSERT INTO game_sessions",
            "SELECT s.session_id,s.player_id",
        ),
        (
            "INSERT INTO game_receipts",
            "SELECT actor,command_id,fingerprint,outcome",
        ),
    ] {
        let (f, code, old, answer) = prepared();
        let db = FaultReadback {
            db: &f.game,
            write,
            read,
            armed: Cell::new(false),
        };
        let service = GameService::new(&db, &f.rt, AuthPolicy::default(), &[9; 32]).unwrap();
        assert_eq!(
            service
                .recover_player(f.command(32), &code, "RecoverPlayer", &answer, "192.0.2.41")
                .err(),
            Some(GameError::Storage)
        );
        assert!(db.armed.get());
        assert_eq!(epoch(&f), 0);
        assert!(f.service().player_view(old.token()).is_ok());
    }
}
#[test]
fn close_retry_backoff_is_durable_bounded_and_ack_ignored_write_retains_work() {
    let (f, code, old, answer) = prepared();
    let grant = f
        .service()
        .prepare_player_connection(old.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&grant).unwrap();
    let _ = recover(&f, &code, &answer, 32).unwrap();
    for attempt in 1..=12 {
        let close = f
            .service()
            .pending_recovery_socket_closes()
            .unwrap()
            .remove(0);
        f.rt.now.set(close.next_attempt_at);
        f.service().retry_recovery_socket_close(&close).unwrap();
        let current = f
            .service()
            .pending_recovery_socket_closes()
            .unwrap()
            .remove(0);
        assert_eq!(current.attempt_count, attempt);
        assert_eq!(
            current.next_attempt_at - f.rt.now.get(),
            (1_000i64 << attempt.min(9)).min(300_000)
        );
    }
    let close = f
        .service()
        .pending_recovery_socket_closes()
        .unwrap()
        .remove(0);
    f.game.conn.borrow().execute_batch("CREATE TRIGGER close_ack_fault BEFORE DELETE ON game_socket_closures BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert_eq!(
        f.service().acknowledge_recovery_socket_close(&close),
        Err(GameError::Storage)
    );
    assert_eq!(
        f.service().pending_recovery_socket_closes().unwrap(),
        vec![close]
    );
}
