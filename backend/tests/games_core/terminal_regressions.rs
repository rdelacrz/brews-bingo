//! Native regression evidence for stable identity, immutable snapshots and restart.
use super::*;
use brews_backend::game::WorkKind;
use brews_domain::games::BoardCellKind;

fn host_revision(f: &Fixture, connections: &[ConnectionId]) -> u64 {
    f.service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            connections,
        )
        .unwrap()
        .view_revision()
}
fn logged_in(f: &Fixture, command: u8) -> String {
    cookie(
        f.auth()
            .execute(
                AuthCommand::Login {
                    username: "GameHostPerson".into(),
                    password: f.password.to_string(),
                },
                RequestContext {
                    command_id: Some(id(&f.rt, command)),
                    caller_identity: "trusted-fixture".into(),
                },
            )
            .unwrap(),
    )
}

#[test]
fn winner_commits_once_for_qualified_retained_departed_player_and_final_views_need_original_grants()
{
    let f = Fixture::new();
    let config = GameConfiguration {
        numeric_upper_bound: 4,
        board_side_length: 2,
        free_cells_enabled: false,
        free_cell_positions: vec![],
        player_capacity: 3,
        spectator_capacity: 0,
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let first = f.player(&code, 20, "First");
    let second = f.player(&code, 21, "Second");
    let third = f.player(&code, 22, "Departed");
    let connections: [ConnectionId; 2] = [
        id(&f.rt, 40).parse().unwrap(),
        id(&f.rt, 41).parse().unwrap(),
    ];
    for (cookie, connection) in [(&first, connections[0]), (&second, connections[1])] {
        let grant = f
            .service()
            .prepare_player_connection(cookie.token(), connection)
            .unwrap();
        f.service().accept_player_connection(&grant).unwrap();
    }
    let game_id = f.service().directory_projection().unwrap().game_id();
    let reservation = f.directory().confirm_reservation(game_id).unwrap();
    f.service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(30),
            host_revision(&f, &connections),
            &reservation,
            &connections,
        )
        .unwrap();
    let (third_id, board) = match f.service().player_view(third.token()).unwrap() {
        GameView::Player {
            player_id,
            board: Some(board),
            ..
        } => (player_id, board),
        _ => panic!("Own assigned board required"),
    };
    let initial_revision = host_revision(&f, &connections);
    assert_eq!(
        f.service()
            .submit_winner(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(70),
                third_id,
                initial_revision
            )
            .err(),
        Some(GameError::Conflict)
    );
    f.game
        .execute(
            "UPDATE game_players SET last_explicit_leave_at=? WHERE player_id=?",
            &[
                SqlValue::Integer(f.rt.now.get()),
                SqlValue::Text(third_id.to_string()),
            ],
        )
        .unwrap();
    for (index, cell) in board.cells.iter().take(2).enumerate() {
        let BoardCellKind::Value(value) = &cell.kind else {
            panic!("Ordinary board expected");
        };
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(50 + u8::try_from(index).unwrap()),
                value,
                host_revision(&f, &connections),
            )
            .unwrap();
        assert_eq!(
            f.game
                .query(
                    "SELECT count(*) FROM game_boards WHERE evaluated_through_call=?",
                    &[SqlValue::Integer(i64::try_from(index + 1).unwrap())]
                )
                .unwrap(),
            vec![vec![SqlValue::Integer(3)]]
        );
    }
    let expected = host_revision(&f, &connections);
    assert_eq!(
        f.service()
            .submit_winner(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(71),
                third_id,
                expected - 1
            )
            .err(),
        Some(GameError::StaleRevision)
    );
    let missing = id(&f.rt, 240).parse().unwrap();
    assert_eq!(
        f.service()
            .submit_winner(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(72),
                missing,
                expected
            )
            .err(),
        Some(GameError::NotFound)
    );
    let command = f.command(90);
    let receipt = match f
        .service()
        .submit_winner(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            third_id,
            expected,
        )
        .unwrap()
    {
        GameResponse::Terminalized {
            state,
            winner: Some(winner),
            receipt,
            ..
        } => {
            assert_eq!(state, GameState::Resolved);
            assert_eq!(winner.player_id, third_id);
            assert_eq!(winner.alias, "Departed");
            receipt
        }
        _ => panic!("Winner submission itself must resolve"),
    };
    assert_eq!(count(&f, "game_history_players"), 3);
    let third_view = f.service().player_view(third.token()).unwrap();
    third_view.validate().unwrap();
    assert!(
        matches!(third_view, GameView::FinalPlayer { player_id, board, .. } if player_id==third_id && board.qualified)
    );
    let before = all_rows(&f);
    assert!(
        matches!(f.service().submit_winner(f.auth().authorize_game_account(&f.token).unwrap(), command, third_id, expected).unwrap(), GameResponse::Committed { receipt: saved } if saved.encode_json().unwrap()==receipt.encode_json().unwrap())
    );
    assert_eq!(
        f.service()
            .submit_winner(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(91),
                missing,
                expected
            )
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(
        f.service()
            .cancel_game(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(92),
                GameState::InProgress,
                true
            )
            .err(),
        Some(GameError::Conflict)
    );
    assert!(all_rows(&f) == before);
    let later_login = logged_in(&f, 120);
    assert!(
        f.service()
            .account_view(
                f.auth().authorize_game_account(&later_login).unwrap(),
                &connections
            )
            .is_err()
    );
    assert!(
        f.service()
            .prepare_account_connection(
                f.auth().authorize_game_account(&later_login).unwrap(),
                id(&f.rt, 121).parse().unwrap()
            )
            .is_err()
    );
    f.service()
        .authorize_gameplay_response(f.auth().authorize_game_account(&later_login).unwrap())
        .unwrap();
}

#[test]
fn calendar_history_deadline_is_the_same_persisted_game_and_snapshot_value() {
    for (ended, expected) in [
        (1_801_401_255_678, 1_809_090_855_678),
        (1_827_619_199_999, 1_835_481_599_999),
        (1_859_241_599_999, 1_867_017_599_999),
    ] {
        let mut f = Fixture::new();
        f.rt.now.set(ended);
        f.token = logged_in(&f, 120);
        f.start_two_player_game();
        end_without_winner(&f, f.command(90));
        assert_eq!(
            f.game
                .query("SELECT ended_at,history_expires_at FROM game_record", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(ended), SqlValue::Integer(expected)]]
        );
        assert_eq!(
            f.game
                .query("SELECT ended_at,expires_at FROM game_history", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(ended), SqlValue::Integer(expected)]]
        );
        assert_eq!(
            f.service()
                .directory_projection()
                .unwrap()
                .history_expires_at(),
            Some(expected)
        );
    }
}

#[test]
fn terminal_grants_receipt_purge_and_release_route_survive_actual_sqlite_reopen() {
    let f = Fixture::new();
    let path = std::env::temp_dir().join(format!(
        "brews-terminal-reopen-{}.sqlite",
        std::process::id()
    ));
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    drop(f.game.conn.replace(conn));
    brews_backend::db::game::migrate_game(&f.game).unwrap();
    let (_, first, _, _) = f.start_two_player_game();
    let receipt = end_without_winner(&f, f.command(90));
    let original = f.service().directory_projection().unwrap();
    let reopen = || {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        drop(f.game.conn.replace(conn));
        brews_backend::db::game::migrate_game(&f.game).unwrap();
    };
    reopen();
    assert!(matches!(
        f.service().player_view(first.token()).unwrap(),
        GameView::FinalPlayer { .. }
    ));
    assert!(matches!(
        f.service()
            .cancel_game(
                f.auth().authorize_game_account(&f.token).unwrap(),
                receipt.command_id,
                GameState::InProgress,
                true
            )
            .unwrap(),
        GameResponse::Committed { .. }
    ));
    f.rt.now.set(original.history_expires_at().unwrap());
    f.service().cleanup().unwrap();
    reopen();
    assert_eq!(count(&f, "game_history"), 0);
    assert_eq!(count(&f, "game_record"), 0);
    assert_eq!(count(&f, "game_terminal_route"), 1);
    assert!(f.service().player_view(first.token()).is_err());
    assert!(
        f.service()
            .pending_work(10)
            .unwrap()
            .iter()
            .any(|w| w.kind() == WorkKind::Release)
    );
    assert_eq!(
        f.service()
            .directory_projection()
            .unwrap()
            .history_expires_at(),
        original.history_expires_at()
    );
    f.rt.now.set(original.history_expires_at().unwrap() - 1);
    assert_eq!(f.service().cleanup().err(), Some(GameError::Storage));
    drop(
        f.game
            .conn
            .replace(rusqlite::Connection::open_in_memory().unwrap()),
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn expired_call_receipts_after_cleanup_never_readmit_old_commands_or_clock_rollback() {
    let f = Fixture::new();
    let (_, _, _, connections) = f.start_two_player_game();
    let command = f.command(50);
    let expected = host_revision(&f, &connections);
    f.service()
        .call_random(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            expected,
        )
        .unwrap();
    let next = f.rt.now.get() + 86_400_000;
    f.rt.now.set(next);
    let token = logged_in(&f, 120);
    f.service().cleanup().unwrap();
    assert_eq!(
        f.service()
            .call_random(
                f.auth().authorize_game_account(&token).unwrap(),
                command,
                expected
            )
            .err(),
        Some(GameError::StaleCommand)
    );
    let proof = f.auth().authorize_game_account(&token).unwrap();
    let before = all_rows(&f);
    f.rt.now.set(next - 1);
    assert_eq!(
        f.service().call_random(proof, command, expected).err(),
        Some(GameError::Storage)
    );
    assert!(all_rows(&f) == before);
}
