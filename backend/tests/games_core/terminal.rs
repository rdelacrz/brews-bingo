//! Real owner-core terminal contracts; no transport or fabricated storage rows.
use super::*;
use brews_domain::games::GameState;
#[path = "terminal_faults.rs"]
mod faults;
#[path = "history.rs"]
mod history;
#[path = "prestart_maintenance.rs"]
mod prestart_maintenance;
#[path = "start_publication.rs"]
mod publication;
#[path = "receipt_atomicity.rs"]
mod receipt_atomicity;
#[path = "terminal_regressions.rs"]
mod regressions;
#[path = "write_regressions.rs"]
mod write_regressions;

fn count(f: &Fixture, table: &str) -> i64 {
    match f
        .game
        .query(&format!("SELECT count(*) FROM {table}"), &[])
        .unwrap()[0][0]
    {
        SqlValue::Integer(count) => count,
        _ => panic!("Expected integer count"),
    }
}

fn end_without_winner(f: &Fixture, command: CommandId) -> brews_contracts::games::GameReceipt {
    match f
        .service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            GameState::InProgress,
            true,
        )
        .unwrap()
    {
        GameResponse::Terminalized {
            state,
            winner,
            history_available,
            receipt,
            ..
        } => {
            assert_eq!(state, GameState::Cancelled);
            assert!(winner.is_none());
            assert!(history_available);
            receipt
        }
        _ => panic!("Expected terminal receipt"),
    }
}

fn all_rows(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .map(|table| {
            f.game
                .query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
                .unwrap()
        })
        .collect()
}

#[test]
fn gameplay_response_guard_is_read_only_and_accepts_terminal_without_new_view_grant() {
    for started in [false, true] {
        let f = Fixture::new();
        if started {
            f.start_two_player_game();
            f.game
                .execute(
                    "DELETE FROM game_connection_grants WHERE viewer_kind='account'",
                    &[],
                )
                .unwrap();
        } else {
            f.open(&GameConfiguration::default());
        }
        assert_eq!(count(&f, "game_connection_grants"), 0);
        f.service()
            .cancel_game(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(90),
                if started {
                    GameState::InProgress
                } else {
                    GameState::AwaitingPlayers
                },
                true,
            )
            .unwrap();
        let before = all_rows(&f);
        f.rt.now.set(f.rt.now.get() + 1);
        let proof = f.auth().authorize_game_account(&f.token).unwrap();
        f.service().authorize_gameplay_response(proof).unwrap();
        assert!(
            all_rows(&f) == before,
            "Response-release checks must not write any Game row, including clock/grants/receipts"
        );
        assert_eq!(count(&f, "game_connection_grants"), 0);
        let proof = f.auth().authorize_game_account(&f.token).unwrap();
        f.rt.now.set(proof.expires_at());
        assert_eq!(
            f.service().authorize_gameplay_response(proof),
            Err(GameError::Unauthorized)
        );
        assert!(all_rows(&f) == before);
    }
}

#[test]
fn terminal_reconnect_preserves_expiry_fences_and_does_not_change_final_revisions() {
    let f = Fixture::new();
    let (_, first, _, connections) = f.start_two_player_game();
    let old = f
        .service()
        .player_connection(connections[0])
        .unwrap()
        .unwrap();
    let account = f
        .service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&f.token).unwrap(),
            id(&f.rt, 42).parse().unwrap(),
        )
        .unwrap();
    f.service()
        .accept_account_connection(&account, f.auth().authorize_game_account(&f.token).unwrap())
        .unwrap();
    end_without_winner(&f, f.command(90));
    let before = f
        .game
        .query("SELECT * FROM game_view_revisions ORDER BY view_key", &[])
        .unwrap();
    f.service().disconnect(&old).unwrap();
    assert!(
        f.game
            .query("SELECT * FROM game_view_revisions ORDER BY view_key", &[])
            .unwrap()
            == before,
        "Terminal socket presence is not part of the immutable final projection"
    );
    f.rt.now.set(f.rt.now.get() + 1000);
    let replacement = f
        .service()
        .prepare_player_connection(first.token(), id(&f.rt, 43).parse().unwrap())
        .unwrap();
    assert_eq!(replacement.expires_at(), old.expires_at());
    assert_eq!(replacement.epoch(), old.epoch());
    f.service().accept_player_connection(&replacement).unwrap();
    assert!(matches!(
        f.service().player_connection_view(&replacement).unwrap(),
        GameView::FinalPlayer { .. }
    ));
    assert!(f.service().player_connection_view(&old).is_err());
    f.service().disconnect(&old).unwrap();
    let account_next = f
        .service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&f.token).unwrap(),
            id(&f.rt, 44).parse().unwrap(),
        )
        .unwrap();
    assert_eq!(account_next.expires_at(), account.expires_at());
    assert_eq!(account_next.epoch(), account.epoch());
    f.service()
        .accept_account_connection(
            &account_next,
            f.auth().authorize_game_account(&f.token).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        f.service()
            .account_connection_view(
                &account_next,
                f.auth().authorize_game_account(&f.token).unwrap()
            )
            .unwrap(),
        GameView::FinalHost { .. }
    ));
    assert!(
        f.service()
            .account_connection_view(&account, f.auth().authorize_game_account(&f.token).unwrap())
            .is_err()
    );
    assert!(
        f.game
            .query("SELECT * FROM game_view_revisions ORDER BY view_key", &[])
            .unwrap()
            == before
    );
}

#[test]
fn history_expiry_denies_immediately_then_purges_with_recoverable_directory_release() {
    let f = Fixture::new();
    let (_, first, _, _) = f.start_two_player_game();
    end_without_winner(&f, f.command(90));
    let original = f.service().directory_projection().unwrap();
    let expires = original.history_expires_at().unwrap();
    let release = f
        .service()
        .pending_work(10)
        .unwrap()
        .into_iter()
        .find(|w| w.kind() == brews_backend::game::WorkKind::Release)
        .unwrap();
    let ack = f
        .directory()
        .release_game(f.service().cleanup().unwrap().unwrap())
        .unwrap();
    f.service().acknowledge_release(&release, ack).unwrap();
    f.rt.now.set(expires);
    assert_eq!(
        f.service().player_view(first.token()).err(),
        Some(GameError::Expired)
    );
    assert_eq!(
        count(&f, "game_history"),
        1,
        "Read denial must not depend on timely physical purge"
    );
    let terminal = f.service().cleanup().unwrap().unwrap();
    for table in [
        "game_history",
        "game_history_calls",
        "game_history_players",
        "game_history_board_cells",
        "game_terminal_views",
        "game_connection_grants",
        "game_current_connections",
        "game_view_revisions",
        "game_receipts",
        "game_record",
    ] {
        assert_eq!(
            count(&f, table),
            0,
            "Expired {table} must be atomically purged"
        );
    }
    assert_eq!(count(&f, "game_terminal_route"), 1);
    let projection = terminal.projection();
    assert_eq!(projection.game_id(), original.game_id());
    assert_eq!(projection.source_revision(), original.source_revision());
    assert_eq!(projection.history_expires_at(), Some(expires));
    let work = f
        .service()
        .pending_work(10)
        .unwrap()
        .into_iter()
        .find(|w| w.kind() == brews_backend::game::WorkKind::Release)
        .unwrap();
    let ack = f.directory().release_game(terminal).unwrap();
    f.service().acknowledge_release(&work, ack).unwrap();
    assert!(f.service().pending_work(10).unwrap().is_empty());
    assert_eq!(f.service().next_deadline().unwrap(), None);
    assert!(f.service().cleanup().unwrap().is_some());
    assert!(f.service().player_view(first.token()).is_err());
    f.rt.now.set(expires - 1);
    assert_eq!(f.service().cleanup().err(), Some(GameError::Storage));
}

#[test]
fn original_final_session_expiry_retires_grants_connections_and_counters_without_history_change() {
    let f = Fixture::new();
    let (_, first, second, _) = f.start_two_player_game();
    end_without_winner(&f, f.command(90));
    let history = f.game.query("SELECT * FROM game_history", &[]).unwrap();
    let release = f
        .service()
        .pending_work(10)
        .unwrap()
        .into_iter()
        .find(|w| w.kind() == brews_backend::game::WorkKind::Release)
        .unwrap();
    let ack = f
        .directory()
        .release_game(f.service().cleanup().unwrap().unwrap())
        .unwrap();
    f.service().acknowledge_release(&release, ack).unwrap();
    let expires = first.expires_at();
    assert_eq!(expires, second.expires_at());
    f.rt.now.set(expires);
    assert!(f.service().player_view(first.token()).is_err());
    assert_eq!(
        count(&f, "game_terminal_views"),
        2,
        "Authorization must deny before physical deletion"
    );
    f.service().cleanup().unwrap();
    for table in [
        "game_terminal_views",
        "game_connection_grants",
        "game_current_connections",
        "game_view_revisions",
    ] {
        assert_eq!(
            count(&f, table),
            0,
            "Last eligible fixed-expiry access must retire {table}"
        );
    }
    assert!(f.game.query("SELECT * FROM game_history", &[]).unwrap() == history);
    assert_eq!(
        f.service().next_deadline().unwrap(),
        Some(
            f.service()
                .directory_projection()
                .unwrap()
                .history_expires_at()
                .unwrap()
        )
    );
}

#[test]
fn final_exit_removes_all_player_or_account_grants_but_never_changes_history() {
    let f = Fixture::new();
    let (_, first, second, connections) = f.start_two_player_game();
    let other_account_token = cookie(
        f.auth()
            .execute(
                AuthCommand::Login {
                    username: "GameHostPerson".into(),
                    password: f.password.to_string(),
                },
                RequestContext {
                    command_id: Some(id(&f.rt, 120)),
                    caller_identity: "trusted-fixture".into(),
                },
            )
            .unwrap(),
    );
    f.service()
        .account_view(
            f.auth()
                .authorize_game_account(&other_account_token)
                .unwrap(),
            &connections,
        )
        .unwrap();
    let original = f
        .service()
        .player_connection(connections[0])
        .unwrap()
        .unwrap();
    let additional_player_token = brews_backend::security::new_token(&f.rt).unwrap();
    let digest = brews_backend::security::token_digest(&additional_player_token).unwrap();
    f.game.execute("INSERT INTO game_sessions(session_id,token_verifier,game_id,game_code,player_id,session_epoch,access,issued_at,expires_at,revoked_at) SELECT ?,?,game_id,game_code,player_id,session_epoch,access,issued_at,expires_at,NULL FROM game_sessions WHERE session_id=?", &[
        SqlValue::Text(id(&f.rt, 121)), SqlValue::Blob(digest.to_vec()), SqlValue::Text(original.session_id().to_string()),
    ]).unwrap();
    let extra = f
        .service()
        .prepare_player_connection(&additional_player_token, id(&f.rt, 122).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&extra).unwrap();
    end_without_winner(&f, f.command(90));
    let history_before = [
        "game_history",
        "game_history_players",
        "game_history_calls",
        "game_history_board_cells",
    ]
    .map(|table| {
        f.game
            .query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
            .unwrap()
    });
    assert_eq!(count(&f, "game_terminal_views"), 3);
    assert!(matches!(
        f.service()
            .exit_player(first.token(), f.command(91))
            .unwrap(),
        GameResponse::Exited { .. }
    ));
    assert_eq!(count(&f, "game_terminal_views"), 1);
    assert!(f.service().player_view(first.token()).is_err());
    assert!(f.service().player_view(&additional_player_token).is_err());
    assert!(f.service().player_connection_view(&original).is_err());
    assert!(f.service().player_connection_view(&extra).is_err());
    assert!(
        f.service()
            .exit_player(first.token(), f.command(91))
            .is_err()
    );
    assert!(f.service().player_view(second.token()).is_ok());
    let account_command = f.command(92);
    assert!(matches!(
        f.service()
            .exit_account(
                f.auth().authorize_game_account(&f.token).unwrap(),
                account_command
            )
            .unwrap(),
        GameResponse::Exited { .. }
    ));
    assert!(
        f.service()
            .account_view(
                f.auth()
                    .authorize_game_account(&other_account_token)
                    .unwrap(),
                &connections
            )
            .is_err()
    );
    assert!(matches!(
        f.service()
            .exit_account(
                f.auth()
                    .authorize_game_account(&other_account_token)
                    .unwrap(),
                account_command
            )
            .unwrap(),
        GameResponse::Committed { .. }
    ));
    assert!(
        f.service()
            .prepare_account_connection(
                f.auth()
                    .authorize_game_account(&other_account_token)
                    .unwrap(),
                id(&f.rt, 123).parse().unwrap()
            )
            .is_err()
    );
    assert_eq!(count(&f, "game_connection_grants"), 0);
    assert_eq!(count(&f, "game_view_revisions"), 1);
    let history_after = [
        "game_history",
        "game_history_players",
        "game_history_calls",
        "game_history_board_cells",
    ]
    .map(|table| {
        f.game
            .query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
            .unwrap()
    });
    assert!(history_before == history_after);
}

#[test]
fn random_call_rechecks_authority_at_the_post_entropy_commit_time() {
    let f = Fixture::new();
    let (_, _, _, connections) = f.start_two_player_game();
    let expected = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &connections,
        )
        .unwrap()
        .view_revision();
    let authority = f.auth().authorize_game_account(&f.token).unwrap();
    let command = f.command(50);
    let revisions = f
        .game
        .query("SELECT * FROM game_view_revisions ORDER BY view_key", &[])
        .unwrap();
    f.rt.advance_random_ms
        .set(authority.expires_at() - f.rt.now.get());
    assert_eq!(
        f.service().call_random(authority, command, expected).err(),
        Some(GameError::Unauthorized)
    );
    assert_eq!(count(&f, "game_calls"), 0);
    assert_eq!(
        f.game
            .query(
                "SELECT count(*) FROM game_board_cells WHERE is_matched=1",
                &[]
            )
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    assert!(
        f.game
            .query("SELECT * FROM game_view_revisions ORDER BY view_key", &[])
            .unwrap()
            == revisions
    );
    assert_eq!(
        f.game.query("SELECT state FROM game_record", &[]).unwrap(),
        vec![vec![SqlValue::Text("in_progress".into())]]
    );
}

#[test]
fn malformed_history_cannot_be_erased_without_validating_its_self_contained_snapshot() {
    let f = Fixture::new();
    f.start_two_player_game();
    end_without_winner(&f, f.command(90));
    let projection = f.service().directory_projection().unwrap();
    f.game
        .execute(
            "INSERT INTO game_history_calls VALUES(?,1,?)",
            &[
                SqlValue::Text(projection.game_id().to_string()),
                SqlValue::Text("1001".into()),
            ],
        )
        .unwrap();
    let before = all_rows(&f);
    f.rt.now.set(projection.history_expires_at().unwrap());
    assert_eq!(
        f.service().cleanup().err(),
        Some(GameError::Storage),
        "The History purge must validate the same bounded immutable snapshot shape as final reads"
    );
    assert!(all_rows(&f) == before);
}

#[test]
fn terminal_commit_rejects_qualification_not_explained_by_committed_calls() {
    let f = Fixture::new();
    let (_, first, _, connections) = f.start_two_player_game();
    let expected = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &connections,
        )
        .unwrap()
        .view_revision();
    let player = match f.service().player_view(first.token()).unwrap() {
        GameView::Player { player_id, .. } => player_id,
        _ => panic!("Expected live member"),
    };
    f.game
        .execute(
            "UPDATE game_board_cells SET is_matched=1 WHERE player_id=? AND row=1",
            &[SqlValue::Text(player.to_string())],
        )
        .unwrap();
    f.game
        .execute(
            "INSERT INTO game_completed_lines VALUES(?,0,'row',1)",
            &[SqlValue::Text(player.to_string())],
        )
        .unwrap();
    let before = all_rows(&f);
    assert_eq!(
        f.service()
            .submit_winner(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(90),
                player,
                expected
            )
            .err(),
        Some(GameError::Storage),
        "Copied History must reject cells/qualification not justified by the accepted call ledger"
    );
    assert!(
        all_rows(&f) == before,
        "Malformed source projection must not create terminal state, History, receipts or grants"
    );
}

#[test]
fn explicit_prestart_cancellation_is_atomic_replayable_and_has_no_history() {
    for expected_state in [GameState::New, GameState::AwaitingPlayers] {
        let f = Fixture::new();
        if expected_state == GameState::New {
            let config = GameConfiguration::default();
            let work = f
                .directory()
                .claim_game(
                    f.account,
                    f.command(10),
                    creation_fingerprint(&config).unwrap(),
                )
                .unwrap();
            f.service()
                .initialize(
                    &work,
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    &config,
                )
                .unwrap();
        } else {
            let code = f.open(&GameConfiguration::default());
            f.player(&code, 20, "First");
        }
        let command = f.command(90);
        let receipt = match f
            .service()
            .cancel_game(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                expected_state,
                true,
            )
            .unwrap()
        {
            GameResponse::Terminalized {
                expected_state: saved,
                state,
                history_available,
                history_expires_at,
                winner,
                receipt,
                ..
            } => {
                assert_eq!(saved, expected_state);
                assert_eq!(state, GameState::Cancelled);
                assert!(!history_available);
                assert!(history_expires_at.is_none());
                assert!(winner.is_none());
                receipt
            }
            _ => panic!("Expected confirmed prestart cancellation"),
        };
        for table in [
            "game_history",
            "game_players",
            "game_sessions",
            "game_terminal_views",
            "game_configuration",
            "game_connection_grants",
            "game_current_connections",
            "game_boards",
            "game_recovery",
            "game_view_revisions",
        ] {
            assert_eq!(
                count(&f, table),
                0,
                "Prestart cancellation must remove {table}"
            );
        }
        assert!(matches!(f.service().cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(), command, expected_state, true,
        ).unwrap(), GameResponse::Committed { receipt: saved } if saved.encode_json().unwrap() == receipt.encode_json().unwrap()));
        assert!(f.service().cleanup().unwrap().is_some());
    }
}

#[test]
fn started_terminal_preserves_only_original_final_grants_and_history_views() {
    let f = Fixture::new();
    let (_, first, second, connections) = f.start_two_player_game();
    let first_before = f
        .service()
        .player_view(first.token())
        .unwrap()
        .view_revision();
    let host_before = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &connections,
        )
        .unwrap()
        .view_revision();
    let sessions_before = f.game.query(
        "SELECT session_id,player_id,token_verifier,session_epoch,expires_at FROM game_sessions ORDER BY session_id", &[],
    ).unwrap();
    let receipt = end_without_winner(&f, f.command(90));
    assert_eq!(
        count(&f, "game_terminal_views"),
        2,
        "Existing eligible sessions must become minimal final grants"
    );
    assert_eq!(
        count(&f, "game_current_connections"),
        2,
        "Final eligible sockets must survive the terminal transaction"
    );
    assert_eq!(f.game.query(
        "SELECT session_id,player_id,token_verifier,session_epoch,expires_at FROM game_terminal_views ORDER BY session_id", &[],
    ).unwrap(), sessions_before);
    for table in [
        "game_configuration",
        "game_free_cells",
        "game_players",
        "game_recovery",
        "game_sessions",
        "game_calls",
        "game_boards",
        "game_board_cells",
        "game_completed_lines",
        "game_admission_contexts",
    ] {
        assert_eq!(count(&f, table), 0, "Live source {table} must be removed");
    }
    let first_view = f.service().player_view(first.token()).unwrap();
    first_view.validate().unwrap();
    assert_eq!(first_view.view_revision(), first_before + 1);
    let first_json = serde_json::to_value(first_view).unwrap();
    assert_eq!(first_json["role"], "final_player");
    assert!(first_json.get("configuration").is_none());
    assert!(first_json.get("players").is_none());
    assert_eq!(first_json["board"]["cells"].as_array().unwrap().len(), 4);
    let second_view = f.service().sync_player(second.token(), None).unwrap();
    second_view.validate().unwrap();
    let host_view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &connections,
        )
        .unwrap();
    host_view.validate().unwrap();
    assert_eq!(host_view.view_revision(), host_before + 1);
    let host_json = serde_json::to_value(host_view).unwrap();
    assert_eq!(host_json["role"], "final_host");
    assert!(host_json.get("configuration").is_none());
    assert_eq!(host_json["players"].as_array().unwrap().len(), 2);
    let replay = f
        .service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            receipt.command_id,
            GameState::InProgress,
            true,
        )
        .unwrap();
    match replay {
        GameResponse::Committed { receipt: replay } => assert_eq!(
            replay.encode_json().unwrap(),
            receipt.encode_json().unwrap()
        ),
        _ => panic!("Expected secret-free exact replay"),
    }
}
