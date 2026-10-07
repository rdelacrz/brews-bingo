//! History permission is independent of original final-view admission.
use super::*;

fn fresh_history_login(f: &Fixture, n: u8) -> String {
    cookie(
        f.auth()
            .execute(
                AuthCommand::Login {
                    username: "GameHostPerson".into(),
                    password: f.password.to_string(),
                },
                RequestContext {
                    command_id: Some(id(&f.rt, n)),
                    caller_identity: "history-expiry-fixture".into(),
                },
            )
            .unwrap(),
    )
}

fn history_at_last_retained_millisecond() -> (Fixture, brews_domain::ids::GameId, i64, String) {
    let f = Fixture::new();
    f.start_two_player_game();
    super::end_without_winner(&f, f.command(90));
    let game = f.service().directory_projection().unwrap().game_id();
    let expiry = f
        .service()
        .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
        .unwrap()
        .expires_at;
    f.rt.now.set(expiry - 1);
    let token = fresh_history_login(&f, 100);
    f.service()
        .history(f.auth().authorize_game_account(&token).unwrap(), game)
        .unwrap();
    (f, game, expiry, token)
}

#[test]
fn history_final_response_expiry_fence_survives_rollback_and_file_reopen() {
    let (mut f, game, expiry, token) = history_at_last_retained_millisecond();
    let proof = f.auth().authorize_game_account(&token).unwrap();
    let before = super::all_rows(&f);
    f.service().history_for_response(proof, game).unwrap();
    assert!(
        super::all_rows(&f) == before,
        "successful final cut must be read-only"
    );
    let proof = f.auth().authorize_game_account(&token).unwrap();
    f.rt.now.set(expiry);
    assert_eq!(
        f.service().history_for_response(proof, game).err(),
        Some(GameError::NotFound)
    );
    assert_eq!(
        f.game
            .query("SELECT last_observed_ms FROM game_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(expiry)]],
        "expiry first observed at the final cut must be fenced durably"
    );
    let after = super::all_rows(&f);
    let metadata = brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .position(|table| *table == "game_metadata")
        .unwrap();
    for (index, rows) in after.iter().enumerate() {
        if index != metadata {
            assert!(
                *rows == before[index],
                "expiry fence must not alter History, grants or revisions"
            );
        }
    }
    let directory = std::path::PathBuf::from(std::env::var("TMPDIR").unwrap())
        .join(format!("history-expiry-reopen-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.join("game.sqlite");
    f.game
        .execute(
            "VACUUM INTO ?",
            &[SqlValue::Text(path.to_string_lossy().into_owned())],
        )
        .unwrap();
    f.game = Sqlite {
        conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
    };
    f.game
        .conn
        .borrow()
        .execute_batch("PRAGMA foreign_keys=ON")
        .unwrap();
    brews_backend::db::game::migrate_game(&f.game).unwrap();
    f.rt.now.set(expiry - 1);
    let fresh = fresh_history_login(&f, 101);
    let proof = f.auth().authorize_game_account(&fresh).unwrap();
    assert_eq!(
        f.service().history(proof, game).err(),
        Some(GameError::Storage)
    );
    let proof = f.auth().authorize_game_account(&fresh).unwrap();
    assert_eq!(
        f.service().history_for_response(proof, game).err(),
        Some(GameError::Storage)
    );
    assert!(
        super::all_rows(&f) == after,
        "rollback denial must not change retained rows"
    );
    f.rt.now.set(expiry);
    assert_eq!(
        f.service()
            .history_for_response(f.auth().authorize_game_account(&fresh).unwrap(), game)
            .err(),
        Some(GameError::NotFound)
    );
    drop(f);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn history_final_response_expiry_fence_faults_roll_back_all_rows() {
    for trigger in [
        "CREATE TRIGGER expiry_fault BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER expiry_fault BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(ABORT,'test expiry clock failure'); END",
        "CREATE TRIGGER expiry_fault AFTER UPDATE ON game_metadata BEGIN UPDATE game_metadata SET last_observed_ms=OLD.last_observed_ms; END",
        "CREATE TRIGGER expiry_fault AFTER UPDATE ON game_metadata BEGIN UPDATE game_metadata SET command_floor_ms=OLD.command_floor_ms-1; END",
        "CREATE TRIGGER expiry_fault AFTER UPDATE ON game_metadata BEGIN DELETE FROM game_metadata; END",
    ] {
        let (f, game, expiry, token) = history_at_last_retained_millisecond();
        f.game.conn.borrow().execute_batch(trigger).unwrap();
        let before = super::all_rows(&f);
        let proof = f.auth().authorize_game_account(&token).unwrap();
        f.rt.now.set(expiry);
        assert_eq!(
            f.service().history_for_response(proof, game).err(),
            Some(GameError::Storage),
            "required fence write/readback failure must not become NotFound"
        );
        assert!(
            super::all_rows(&f) == before,
            "failed fence must roll back every row"
        );
        f.game
            .conn
            .borrow()
            .execute_batch("DROP TRIGGER expiry_fault")
            .unwrap();
        f.rt.now.set(expiry - 1);
        let proof = f.auth().authorize_game_account(&token).unwrap();
        f.rt.now.set(expiry);
        assert_eq!(
            f.service().history_for_response(proof, game).err(),
            Some(GameError::NotFound)
        );
    }
}

#[test]
fn history_final_response_expiry_read_error_rolls_back_required_write() {
    struct ReadbackFault<'a> {
        db: &'a Sqlite,
        wrote: std::cell::Cell<bool>,
    }
    impl Database for ReadbackFault<'_> {
        fn query(
            &self,
            sql: &str,
            params: &[SqlValue],
        ) -> Result<Vec<brews_backend::db::Row>, brews_backend::db::StorageError> {
            if self.wrote.get()
                && sql
                    == "SELECT last_observed_ms,command_floor_ms FROM game_metadata WHERE singleton=1"
            {
                return self
                    .db
                    .query("SELECT missing_expiry_clock_column FROM game_metadata", &[]);
            }
            self.db.query(sql, params)
        }
        fn execute(
            &self,
            sql: &str,
            params: &[SqlValue],
        ) -> Result<(), brews_backend::db::StorageError> {
            self.db.execute(sql, params)?;
            if sql
                == "UPDATE game_metadata SET last_observed_ms=?,command_floor_ms=? WHERE singleton=1"
            {
                self.wrote.set(true);
            }
            Ok(())
        }
        fn transaction<T>(
            &self,
            operation: impl FnOnce() -> Result<T, brews_backend::db::StorageError>,
        ) -> Result<T, brews_backend::db::StorageError> {
            self.db.transaction(operation)
        }
    }
    let (f, game, expiry, token) = history_at_last_retained_millisecond();
    let before = super::all_rows(&f);
    let proof = f.auth().authorize_game_account(&token).unwrap();
    let db = ReadbackFault {
        db: &f.game,
        wrote: std::cell::Cell::new(false),
    };
    let service = GameService::new(&db, &f.rt, AuthPolicy::default(), &[9; 32]).unwrap();
    f.rt.now.set(expiry);
    assert_eq!(
        service.history_for_response(proof, game).err(),
        Some(GameError::Storage)
    );
    assert!(
        db.wrote.get(),
        "fault must occur only after a real SQLite fence write"
    );
    assert!(
        super::all_rows(&f) == before,
        "readback error must roll back the fence"
    );
}

#[test]
fn history_reads_complete_cancelled_snapshot_without_final_grants() {
    let f = Fixture::new();
    let (_, _, _, _) = f.start_two_player_game();
    let game = f.service().directory_projection().unwrap().game_id();
    let revision = f
        .service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap()
        .view_revision();
    f.service()
        .call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(80),
            "1",
            revision,
        )
        .unwrap();
    super::end_without_winner(&f, f.command(90));
    f.game
        .execute("DELETE FROM game_connection_grants", &[])
        .unwrap();
    f.game
        .execute("DELETE FROM game_terminal_views", &[])
        .unwrap();
    f.game
        .execute("DELETE FROM game_view_revisions", &[])
        .unwrap();
    let before = super::all_rows(&f);
    let history = f
        .service()
        .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
        .expect("current account must read immutable History without grants");
    let value = serde_json::to_value(history).unwrap();
    assert_eq!(value["game_id"], game.to_string());
    assert_eq!(value["outcome"], "cancelled");
    assert_eq!(value["winner"], serde_json::Value::Null);
    assert_eq!(value["ordered_calls"], serde_json::json!(["1"]));
    assert_eq!(value["players"].as_array().unwrap().len(), 2);
    assert_eq!(value.as_object().unwrap().len(), 10);
    for player in value["players"].as_array().unwrap() {
        assert_eq!(player.as_object().unwrap().len(), 4);
        assert!(player["cells"].as_array().unwrap().len() >= 4);
    }
    assert!(
        super::all_rows(&f) == before,
        "History must not mutate any game row"
    );
}

#[test]
fn history_nonterminal_prestart_missing_and_expired_are_the_same_not_found() {
    let f = Fixture::new();
    let unknown = f.command(99).to_string().parse().unwrap();
    assert_eq!(
        f.service()
            .history(f.auth().authorize_game_account(&f.token).unwrap(), unknown)
            .err(),
        Some(GameError::NotFound)
    );
    f.open(&GameConfiguration::default());
    let game = f.service().directory_projection().unwrap().game_id();
    assert_eq!(
        f.service()
            .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
            .err(),
        Some(GameError::NotFound)
    );
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(90),
            GameState::AwaitingPlayers,
            true,
        )
        .unwrap();
    assert_eq!(
        f.service()
            .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
            .err(),
        Some(GameError::NotFound)
    );
    let f = Fixture::new();
    f.start_two_player_game();
    super::end_without_winner(&f, f.command(90));
    let game = f.service().directory_projection().unwrap().game_id();
    let h = f
        .service()
        .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
        .unwrap();
    f.rt.now.set(h.expires_at);
    let login = f
        .auth()
        .execute(
            AuthCommand::Login {
                username: "GameHostPerson".into(),
                password: f.password.to_string(),
            },
            RequestContext {
                command_id: Some(id(&f.rt, 100)),
                caller_identity: "history-clock-fixture".into(),
            },
        )
        .unwrap();
    let token = cookie(login);
    assert_eq!(
        f.service()
            .history(f.auth().authorize_game_account(&token).unwrap(), game)
            .err(),
        Some(GameError::NotFound)
    );
    let proof = f.auth().authorize_game_account(&token).unwrap();
    f.rt.now.set(h.expires_at - 1);
    assert_eq!(
        f.service().history_for_response(proof, game).err(),
        Some(GameError::Storage)
    );
}
#[test]
fn history_rejects_corrupt_snapshot_and_ignored_clock_write_without_changes() {
    let f = Fixture::new();
    f.start_two_player_game();
    super::end_without_winner(&f, f.command(90));
    let game = f.service().directory_projection().unwrap().game_id();
    f.game
        .execute(
            "UPDATE game_history_board_cells SET is_matched=1 WHERE row=1 AND column=1",
            &[],
        )
        .unwrap();
    let before = super::all_rows(&f);
    assert_eq!(
        f.service()
            .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
            .err(),
        Some(GameError::Storage)
    );
    assert!(super::all_rows(&f) == before);
    f.game
        .execute("UPDATE game_history_board_cells SET is_matched=0", &[])
        .unwrap();
    f.game.conn.borrow().execute_batch("CREATE TRIGGER ignore_history_clock BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    let before = super::all_rows(&f);
    f.rt.now.set(f.rt.now.get() + 1);
    assert_eq!(
        f.service()
            .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
            .err(),
        Some(GameError::Storage)
    );
    assert!(super::all_rows(&f) == before);
}
#[test]
fn history_release_rechecks_expiry_authority_without_writing_after_await() {
    let f = Fixture::new();
    f.start_two_player_game();
    super::end_without_winner(&f, f.command(90));
    let game = f.service().directory_projection().unwrap().game_id();
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    let before = super::all_rows(&f);
    f.rt.now.set(f.rt.now.get() + 1);
    let h = f.service().history_for_response(proof, game).unwrap();
    assert!(super::all_rows(&f) == before);
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    f.rt.now.set(proof.expires_at());
    assert_eq!(
        f.service().history_for_response(proof, game).err(),
        Some(GameError::Unauthorized)
    );
    assert!(super::all_rows(&f) == before);
    assert!(
        h.players
            .windows(2)
            .all(|p| p[0].player_id < p[1].player_id)
    );
}

#[test]
fn immutable_history_survives_actual_file_reopen_after_final_grants_exit() {
    let mut f = Fixture::new();
    f.start_two_player_game();
    super::end_without_winner(&f, f.command(90));
    let game = f.service().directory_projection().unwrap().game_id();
    f.service()
        .exit_account(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(91),
        )
        .unwrap();
    let h = f
        .service()
        .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
        .unwrap();
    let path = std::path::PathBuf::from(std::env::var("TMPDIR").unwrap())
        .join(format!("history-reopen-{}.sqlite", std::process::id()));
    f.game
        .execute(
            "VACUUM INTO ?",
            &[SqlValue::Text(path.to_string_lossy().into_owned())],
        )
        .unwrap();
    f.game = Sqlite {
        conn: std::cell::RefCell::new(rusqlite::Connection::open(&path).unwrap()),
    };
    f.game
        .conn
        .borrow()
        .execute_batch("PRAGMA foreign_keys=ON")
        .unwrap();
    brews_backend::db::game::migrate_game(&f.game).unwrap();
    let actual = f
        .service()
        .history(f.auth().authorize_game_account(&f.token).unwrap(), game)
        .unwrap();
    assert!(
        serde_json::to_vec(&actual).unwrap() == serde_json::to_vec(&h).unwrap(),
        "reopened History must remain exact"
    );
    f.rt.now.set(f.rt.now.get() + 1);
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    f.service().history(proof, game).unwrap();
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    f.rt.now.set(f.rt.now.get() - 1);
    assert_eq!(
        f.service().history_for_response(proof, game).err(),
        Some(GameError::Storage)
    );
    drop(f);
    std::fs::remove_file(path).unwrap();
}
