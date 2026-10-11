use super::*;
use brews_backend::game::SecretCookie;
use brews_domain::games::GameState;

fn rows(f: &Fixture, table: &str) -> Vec<brews_backend::db::Row> {
    // Test-owned table names only; no request interpolation.
    f.game
        .query(&format!("SELECT * FROM {table} ORDER BY 1"), &[])
        .unwrap()
}
fn snapshot(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    [
        "game_players",
        "game_sessions",
        "game_recovery",
        "game_current_connections",
        "game_connection_grants",
        "game_socket_closures",
        "game_view_revisions",
        "game_receipts",
        "game_admission_contexts",
        "game_boards",
        "game_board_cells",
        "game_completed_lines",
        "game_record",
    ]
    .map(|table| rows(f, table))
    .to_vec()
}
fn lobby() -> (
    Fixture,
    brews_domain::games::GameCode,
    SecretCookie,
    SecretCookie,
) {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let a = f.player(&code, 20, "First");
    let b = f.player(&code, 21, "Second");
    (f, code, a, b)
}
fn connect(f: &Fixture, player: &SecretCookie, n: u8) -> brews_backend::game::ConnectionGrant {
    let g = f
        .service()
        .prepare_player_connection(player.token(), id(&f.rt, n).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&g).unwrap();
    g
}
#[test]
fn alias_exact_trim_noop_recovery_and_membership_session_invariants() {
    let (f, code, a, b) = lobby();
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(22), a.token(), Some(&answer))
        .unwrap();
    let sessions = rows(&f, "game_sessions");
    let proof = rows(&f, "game_recovery");
    let players = rows(&f, "game_players");
    let b_revision = f.service().player_view(b.token()).unwrap().view_revision();
    let old_revision = f.service().player_view(a.token()).unwrap().view_revision();
    let noop = f
        .service()
        .rename_my_alias(f.command(23), a.token(), " First ")
        .unwrap();
    assert!(
        matches!(noop.response, GameResponse::AliasRenamed { view_revision, .. } if view_revision == old_revision)
    );
    assert_eq!(rows(&f, "game_players"), players);
    let changed = f
        .service()
        .rename_my_alias(f.command(24), a.token(), " first!@#$%^&*()_+-= ")
        .unwrap();
    let GameResponse::AliasRenamed {
        player_id,
        alias,
        session_expires_at,
        view_revision,
        receipt,
    } = &changed.response
    else {
        panic!("rename")
    };
    assert_eq!(alias, "first!@#$%^&*()_+-=");
    assert_eq!(*session_expires_at, a.expires_at());
    assert_eq!(*view_revision, old_revision + 1);
    assert_eq!(
        f.service().player_view(b.token()).unwrap().view_revision(),
        b_revision
    );
    assert!(rows(&f, "game_sessions") == sessions);
    assert!(rows(&f, "game_recovery") == proof);
    let after = rows(&f, "game_players");
    let own = after
        .iter()
        .find(|row| row[0] == SqlValue::Text(player_id.to_string()))
        .unwrap();
    let prior = players.iter().find(|row| row[0] == own[0]).unwrap();
    assert_eq!(&own[2..], &prior[2..]);
    assert_eq!(
        f.service()
            .membership_output_deadline(a.token(), &changed)
            .unwrap(),
        Some(a.expires_at())
    );
    let retry = f
        .service()
        .rename_my_alias(f.command(24), a.token(), "first!@#$%^&*()_+-=")
        .unwrap();
    assert!(
        matches!(retry.response, GameResponse::Committed { receipt: r } if r.encode_json().unwrap()==receipt.encode_json().unwrap())
    );
    assert_eq!(
        f.service()
            .rename_my_alias(f.command(24), a.token(), "Another")
            .err(),
        Some(GameError::Conflict)
    );
    let before = snapshot(&f);
    for alias in [
        "",
        " ",
        "\tFirst",
        "\nFirst",
        "é",
        "abcdefghijklmnopqrstu",
        "Second",
    ] {
        assert!(
            f.service()
                .rename_my_alias(f.command(25), a.token(), alias)
                .is_err()
        );
        assert!(snapshot(&f) == before);
    }
    f.player(&code, 26, "First");
    assert!(
        f.service()
            .recover_player(f.command(27), &code, "First", &answer, "192.0.2.4")
            .is_err()
    );
    assert!(
        f.service()
            .recover_player(f.command(28), &code, alias, &answer, "192.0.2.4")
            .unwrap()
            .cookie
            .is_some()
    );
}
#[test]
fn lobby_leave_deletes_authority_proof_grants_view_and_old_alias_allows_fresh_identity() {
    let (f, code, a, _) = lobby();
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(22), a.token(), Some(&answer))
        .unwrap();
    let old = connect(&f, &a, 40);
    let prepared = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    let result = f
        .service()
        .leave_player(f.command(23), a.token(), GameState::AwaitingPlayers)
        .unwrap();
    assert!(result.clears_player_cookie());
    assert_eq!(
        f.service()
            .membership_output_deadline(a.token(), &result)
            .unwrap(),
        None
    );
    assert!(matches!(
        result.response,
        GameResponse::PlayerLeft {
            membership_retained: false,
            fresh_admission_required: true,
            session_expires_at: None,
            ..
        }
    ));
    for (table, key) in [
        ("game_players", "player_id"),
        ("game_recovery", "player_id"),
        ("game_sessions", "player_id"),
        ("game_current_connections", "principal_id"),
        ("game_connection_grants", "principal_id"),
        ("game_view_revisions", "view_key"),
        ("game_admission_contexts", "player_id"),
    ] {
        assert!(
            f.game
                .query(
                    &format!("SELECT 1 FROM {table} WHERE {key}=?"),
                    &[SqlValue::Text(old.player_id().to_string())]
                )
                .unwrap()
                .is_empty()
        );
    }
    assert!(f.service().player_view(a.token()).is_err());
    assert_eq!(
        f.service()
            .leave_player(f.command(23), a.token(), GameState::AwaitingPlayers)
            .err(),
        Some(GameError::Unauthorized)
    );
    assert!(
        f.service()
            .recover_player(f.command(24), &code, "First", &answer, "192.0.2.4")
            .is_err()
    );
    assert_eq!(
        f.service().accept_player_connection(&prepared).err(),
        Some(GameError::Unauthorized)
    );
    let pending = f.service().pending_recovery_socket_closes().unwrap();
    assert_eq!(pending.len(), 2);
    assert!(
        pending
            .iter()
            .any(|close| close.connection == old.connection_id())
    );
    assert!(
        pending
            .iter()
            .any(|close| close.connection == prepared.connection_id())
    );
    f.service()
        .reconcile_connections(&[old.connection_id(), prepared.connection_id()])
        .unwrap();
    assert!(
        f.service()
            .player_connection(old.connection_id())
            .unwrap()
            .is_none()
    );
    let fresh = f.player(&code, 25, "First");
    let fresh_g = connect(&f, &fresh, 42);
    assert_ne!(fresh_g.player_id(), old.player_id());
    f.service()
        .acknowledge_recovery_socket_close(&pending[0])
        .unwrap();
    f.service().disconnect(&old).unwrap();
    assert!(
        f.service()
            .player_connection(fresh_g.connection_id())
            .unwrap()
            .is_some()
    );
}
#[test]
fn inprogress_leave_retains_gameplay_return_authority_and_timestamp_and_retry_preserves_successor()
{
    let f = Fixture::new();
    let (_code, a, _b, connections) = f.start_two_player_game();
    let old = f
        .service()
        .player_connection(connections[0])
        .unwrap()
        .unwrap();
    let proof = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), a.token(), Some(&proof))
        .unwrap();
    let sessions = rows(&f, "game_sessions");
    let recovery = rows(&f, "game_recovery");
    let boards = rows(&f, "game_board_cells");
    let before = snapshot(&f);
    assert_eq!(
        f.service()
            .leave_player(f.command(32), a.token(), GameState::AwaitingPlayers)
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(
        f.service()
            .rename_my_alias(f.command(33), a.token(), "Late")
            .err(),
        Some(GameError::Conflict)
    );
    assert!(snapshot(&f) == before);
    let command = f.command(34);
    let result = f
        .service()
        .leave_player(command, a.token(), GameState::InProgress)
        .unwrap();
    assert!(!result.clears_player_cookie());
    assert_eq!(
        f.service()
            .membership_output_deadline(a.token(), &result)
            .unwrap(),
        Some(a.expires_at())
    );
    assert!(rows(&f, "game_sessions") == sessions);
    assert!(rows(&f, "game_recovery") == recovery);
    assert_eq!(rows(&f, "game_board_cells"), boards);
    let leave_time = f.rt.now.get();
    let successor = connect(&f, &a, 45);
    let after = snapshot(&f);
    f.rt.now.set(leave_time + 1);
    assert!(matches!(
        f.service()
            .leave_player(command, a.token(), GameState::InProgress)
            .unwrap()
            .response,
        GameResponse::Committed { .. }
    ));
    let output = f
        .service()
        .membership_output(command, a.token(), None, Some(GameState::InProgress))
        .unwrap();
    assert!(matches!(output.response, GameResponse::Committed { .. }));
    assert!(snapshot(&f) == after);
    f.service().disconnect(&old).unwrap();
    f.service()
        .reconcile_connections(&[
            old.connection_id(),
            successor.connection_id(),
            connections[1],
        ])
        .unwrap();
    assert!(
        f.service()
            .player_connection(successor.connection_id())
            .unwrap()
            .is_some()
    );
    f.service()
        .leave_player(f.command(35), a.token(), GameState::InProgress)
        .unwrap();
    let own = f
        .game
        .query(
            "SELECT last_explicit_leave_at,session_epoch FROM game_players WHERE player_id=?",
            &[SqlValue::Text(old.player_id().to_string())],
        )
        .unwrap();
    assert_eq!(
        own,
        vec![vec![
            SqlValue::Integer(leave_time + 1),
            SqlValue::Integer(0)
        ]]
    );
    assert!(f.service().player_view(a.token()).is_ok());
    for n in 1..=4 {
        let host = f
            .service()
            .account_view(
                f.auth().authorize_game_account(&f.token).unwrap(),
                &[connections[1]],
            )
            .unwrap();
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(50 + n),
                &n.to_string(),
                host.view_revision(),
            )
            .unwrap();
    }
    assert!(
        matches!(f.service().player_view(a.token()).unwrap(),GameView::Player{board:Some(ref board),..} if board.qualified)
    );
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[connections[1]],
        )
        .unwrap();
    f.service()
        .submit_winner(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(60),
            old.player_id(),
            host.view_revision(),
        )
        .unwrap();
    assert!(
        f.service()
            .leave_player(f.command(61), a.token(), GameState::InProgress)
            .is_err()
    );
    assert!(
        f.service()
            .rename_my_alias(f.command(62), a.token(), "Terminal")
            .is_err()
    );
    assert!(matches!(
        f.service().player_view(a.token()).unwrap(),
        GameView::FinalPlayer { .. }
    ));
}
#[test]
fn required_sql_ignore_and_abort_roll_back_alias_leave_receipt_revision_and_closure() {
    for fault in ["IGNORE", "ABORT, 'test storage fault'"] {
        for (table, event, leave) in [
            ("game_players", "UPDATE OF alias", false),
            ("game_receipts", "INSERT", false),
            ("game_socket_closures", "INSERT", true),
            ("game_sessions", "DELETE", true),
            ("game_recovery", "DELETE", true),
            ("game_current_connections", "DELETE", true),
            ("game_connection_grants", "DELETE", true),
            ("game_view_revisions", "DELETE", true),
            ("game_players", "DELETE", true),
            ("game_receipts", "INSERT", true),
        ] {
            let (f, _, a, _) = lobby();
            let proof = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
            f.service()
                .manage_recovery_answer(f.command(22), a.token(), Some(&proof))
                .unwrap();
            connect(&f, &a, 40);
            f.service()
                .prepare_player_connection(a.token(), id(&f.rt, 41).parse().unwrap())
                .unwrap();
            let before = snapshot(&f);
            f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER lifecycle_fault BEFORE {event} ON {table} BEGIN SELECT RAISE({fault}); END")).unwrap();
            let result = if leave {
                f.service()
                    .leave_player(f.command(23), a.token(), GameState::AwaitingPlayers)
            } else {
                f.service()
                    .rename_my_alias(f.command(23), a.token(), "Changed")
            };
            assert_eq!(
                result.err(),
                Some(GameError::Storage),
                "table {table} operation {event}"
            );
            assert!(snapshot(&f) == before, "table {table}");
        }
    }
}
#[test]
fn output_gate_rechecks_clock_epoch_alias_lifecycle_and_receipt_without_remutation() {
    let (f, _, a, _) = lobby();
    let outcome = f
        .service()
        .rename_my_alias(f.command(22), a.token(), "Changed")
        .unwrap();
    f.service()
        .rename_my_alias(f.command(23), a.token(), "Successor")
        .unwrap();
    assert_eq!(
        f.service()
            .membership_output_deadline(a.token(), &outcome)
            .err(),
        Some(GameError::Unauthorized)
    );
    let next = f
        .service()
        .rename_my_alias(f.command(24), a.token(), "Successor")
        .unwrap();
    f.game
        .execute(
            "UPDATE game_players SET session_epoch=session_epoch+1 WHERE alias=?",
            &[SqlValue::Text("Successor".into())],
        )
        .unwrap();
    assert_eq!(
        f.service()
            .membership_output_deadline(a.token(), &next)
            .err(),
        Some(GameError::Unauthorized)
    );
    let (f, _, a, _) = lobby();
    let outcome = f
        .service()
        .rename_my_alias(f.command(22), a.token(), "Changed")
        .unwrap();
    f.rt.now.set(a.expires_at());
    assert!(
        f.service()
            .membership_output_deadline(a.token(), &outcome)
            .is_err()
    );
    f.rt.now.set(a.expires_at() - 1);
    assert_eq!(
        f.service()
            .membership_output_deadline(a.token(), &outcome)
            .err(),
        Some(GameError::Storage)
    );
}

#[test]
fn lobby_leave_excludes_start_roster_and_prepared_socket_is_not_connected() {
    let (f, _, a, b) = lobby();
    let old = connect(&f, &a, 40);
    let prepared = f
        .service()
        .prepare_player_connection(b.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[old.connection_id(), prepared.connection_id()],
        )
        .unwrap();
    assert!(matches!(
        host,
        GameView::Host {
            connected_player_count: 1,
            ..
        }
    ));
    f.service().accept_player_connection(&prepared).unwrap();
    f.service()
        .leave_player(
            f.command(22),
            a.token(),
            brews_domain::games::GameState::AwaitingPlayers,
        )
        .unwrap();
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[old.connection_id(), prepared.connection_id()],
        )
        .unwrap();
    assert!(matches!(
        host,
        GameView::Host {
            connected_player_count: 1,
            ..
        }
    ));
    let game = f.service().directory_projection().unwrap().game_id();
    let reservation = f.directory().confirm_reservation(game).unwrap();
    assert_eq!(
        f.service()
            .start(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(23),
                host.view_revision(),
                &reservation,
                &[old.connection_id(), prepared.connection_id()]
            )
            .err(),
        Some(GameError::Conflict)
    );
    assert!(rows(&f, "game_boards").is_empty());
}

#[test]
fn durable_close_identity_and_stale_ack_never_touch_successor_or_expiry() {
    let f = Fixture::new();
    let (_, a, _, connections) = f.start_two_player_game();
    let old = f
        .service()
        .player_connection(connections[0])
        .unwrap()
        .unwrap();
    f.service()
        .leave_player(
            f.command(31),
            a.token(),
            brews_domain::games::GameState::InProgress,
        )
        .unwrap();
    let pending = f
        .service()
        .pending_recovery_socket_closes()
        .unwrap()
        .remove(0);
    assert!(
        f.service()
            .player_socket_is_fenced(
                old.game_id(),
                old.connection_id(),
                pending.session,
                old.player_id(),
                pending.epoch
            )
            .unwrap()
    );
    assert!(
        !f.service()
            .player_socket_is_fenced(
                old.game_id(),
                old.connection_id(),
                pending.session,
                old.player_id(),
                pending.epoch + 1
            )
            .unwrap()
    );
    let successor = connect(&f, &a, 45);
    let mut wrong = pending.clone();
    wrong.epoch += 1;
    f.service()
        .acknowledge_recovery_socket_close(&wrong)
        .unwrap();
    assert_eq!(
        f.service().pending_recovery_socket_closes().unwrap(),
        vec![pending.clone()]
    );
    f.service().retry_recovery_socket_close(&pending).unwrap();
    brews_backend::db::game::migrate_game(&f.game).unwrap();
    assert!(f.service().player_connection_view(&old).is_err());
    f.service().disconnect(&old).unwrap();
    assert!(f.service().player_connection_view(&successor).is_ok());
    f.service()
        .acknowledge_recovery_socket_close(&pending)
        .unwrap();
    assert!(f.service().player_connection_view(&successor).is_ok());
}

#[test]
fn inprogress_timestamp_ignore_abort_roll_back_closure_and_receipt_and_answer_recovery_retains_event()
 {
    for fault in ["IGNORE", "ABORT, 'test-only timestamp failure'"] {
        let f = Fixture::new();
        let (_, a, _, _) = f.start_two_player_game();
        let before = snapshot(&f);
        f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER lifecycle_time_fault BEFORE UPDATE OF last_explicit_leave_at ON game_players BEGIN SELECT RAISE({fault}); END")).unwrap();
        assert_eq!(
            f.service()
                .leave_player(
                    f.command(31),
                    a.token(),
                    brews_domain::games::GameState::InProgress
                )
                .err(),
            Some(GameError::Storage)
        );
        assert!(snapshot(&f) == before);
    }
    let f = Fixture::new();
    let (code, a, _, _) = f.start_two_player_game();
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), a.token(), Some(&answer))
        .unwrap();
    f.service()
        .leave_player(
            f.command(32),
            a.token(),
            brews_domain::games::GameState::InProgress,
        )
        .unwrap();
    let event = f
        .game
        .query(
            "SELECT player_id,last_explicit_leave_at FROM game_players ORDER BY player_id",
            &[],
        )
        .unwrap();
    f.rt.now.set(f.rt.now.get() + 1);
    let recovered = f
        .service()
        .recover_player(f.command(33), &code, "First", &answer, "192.0.2.4")
        .unwrap()
        .cookie
        .unwrap();
    assert!(f.service().player_view(a.token()).is_err());
    connect(&f, &recovered, 46);
    assert!(
        f.game
            .query(
                "SELECT player_id,last_explicit_leave_at FROM game_players ORDER BY player_id",
                &[]
            )
            .unwrap()
            == event
    );
}

#[test]
fn recovered_session_receipt_output_is_bounded_by_original_receipt_expiry() {
    let f = Fixture::new();
    let (code, a, _, _) = f.start_two_player_game();
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), a.token(), Some(&answer))
        .unwrap();
    let command = f.command(32);
    let initial = f
        .service()
        .leave_player(
            command,
            a.token(),
            brews_domain::games::GameState::InProgress,
        )
        .unwrap();
    let GameResponse::PlayerLeft { receipt, .. } = initial.response else {
        panic!("Leave")
    };
    let original_expiry = receipt.expires_at;
    f.rt.now.set(f.rt.now.get() + 60_000);
    let fresh = f
        .service()
        .recover_player(f.command(33), &code, "First", &answer, "192.0.2.4")
        .unwrap()
        .cookie
        .unwrap();
    assert!(fresh.expires_at() > original_expiry);
    let retry = f
        .service()
        .leave_player(
            command,
            fresh.token(),
            brews_domain::games::GameState::InProgress,
        )
        .unwrap();
    assert!(matches!(retry.response, GameResponse::Committed { .. }));
    assert_eq!(
        f.service()
            .membership_output_deadline(fresh.token(), &retry)
            .unwrap(),
        Some(original_expiry)
    );
    f.rt.now.set(original_expiry);
    assert!(
        f.service()
            .membership_output_deadline(fresh.token(), &retry)
            .is_err()
    );
    f.rt.now.set(original_expiry - 1);
    assert_eq!(
        f.service()
            .membership_output_deadline(fresh.token(), &retry)
            .err(),
        Some(GameError::Storage)
    );
}

#[test]
fn prepared_only_leave_queues_close_work_without_connected_presence_revision() {
    let f = Fixture::new();
    let (_, a, _, connections) = f.start_two_player_game();
    f.service()
        .leave_player(
            f.command(31),
            a.token(),
            brews_domain::games::GameState::InProgress,
        )
        .unwrap();
    let prepared = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 45).parse().unwrap())
        .unwrap();
    let before = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[connections[1], prepared.connection_id()],
        )
        .unwrap();
    assert!(matches!(
        before,
        GameView::Host {
            connected_player_count: 1,
            ..
        }
    ));
    f.rt.now.set(f.rt.now.get() + 1);
    f.service()
        .leave_player(
            f.command(32),
            a.token(),
            brews_domain::games::GameState::InProgress,
        )
        .unwrap();
    assert!(
        f.service()
            .pending_recovery_socket_closes()
            .unwrap()
            .iter()
            .any(|close| close.connection == prepared.connection_id())
    );
    assert_eq!(
        f.service().accept_player_connection(&prepared).err(),
        Some(GameError::Unauthorized)
    );
    let after = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[connections[1], prepared.connection_id()],
        )
        .unwrap();
    assert_eq!(after.view_revision(), before.view_revision());
    assert!(matches!(
        after,
        GameView::Host {
            connected_player_count: 1,
            ..
        }
    ));
}
