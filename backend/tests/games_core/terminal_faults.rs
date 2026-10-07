//! SQLite RAISE(IGNORE)/RAISE(ABORT) are real write failures, not mocked rows.
use super::*;

const INITIAL_TERMINAL_FAULTS: [(&str, &str); 14] = [
    ("game_record", "UPDATE"),
    ("game_history", "INSERT"),
    ("game_history_calls", "INSERT"),
    ("game_history_players", "INSERT"),
    ("game_history_board_cells", "INSERT"),
    ("game_terminal_views", "INSERT"),
    ("game_pending_work", "DELETE"),
    ("game_calls", "DELETE"),
    ("game_board_cells", "DELETE"),
    ("game_boards", "DELETE"),
    ("game_sessions", "DELETE"),
    ("game_players", "DELETE"),
    ("game_configuration", "DELETE"),
    ("game_receipts", "INSERT"),
];
const EXPIRY_FAULTS: [(&str, &str); 11] = [
    ("game_terminal_route", "INSERT"),
    ("game_pending_work", "INSERT"),
    ("game_history_board_cells", "DELETE"),
    ("game_history_calls", "DELETE"),
    ("game_history_players", "DELETE"),
    ("game_history", "DELETE"),
    ("game_terminal_views", "DELETE"),
    ("game_connection_grants", "DELETE"),
    ("game_current_connections", "DELETE"),
    ("game_receipts", "DELETE"),
    ("game_record", "DELETE"),
];

fn call_once(f: &Fixture, connections: &[ConnectionId]) {
    let revision = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            connections,
        )
        .unwrap()
        .view_revision();
    f.service()
        .call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(50),
            "1",
            revision,
        )
        .unwrap();
}
fn fault(f: &Fixture, table: &str, event: &str, mode: &str) {
    f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER reject_core_write BEFORE {event} ON {table} BEGIN SELECT RAISE({mode}); END")).unwrap();
}
fn clear_fault(f: &Fixture) {
    f.game
        .conn
        .borrow()
        .execute_batch("DROP TRIGGER reject_core_write")
        .unwrap();
}

#[test]
fn every_required_terminal_write_has_real_sqlite_ignored_and_aborted_rollback() {
    for (table, event) in INITIAL_TERMINAL_FAULTS {
        for mode in ["IGNORE", "ABORT,'fault'"] {
            let f = Fixture::new();
            let (_, _, _, connections) = f.start_two_player_game();
            call_once(&f, &connections);
            let command = f.command(90);
            let before = all_rows(&f);
            fault(&f, table, event, mode);
            f.rt.now.set(f.rt.now.get() + 1);
            assert_eq!(
                f.service()
                    .cancel_game(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        command,
                        GameState::InProgress,
                        true
                    )
                    .err(),
                Some(GameError::Storage),
                "{event} {table} {mode}"
            );
            assert!(
                all_rows(&f) == before,
                "{event} {table} {mode} must roll back History, grants, live state and clock"
            );
            clear_fault(&f);
            end_without_winner(&f, command);
            assert_eq!(count(&f, "game_history"), 1);
        }
    }
}

#[test]
fn final_exit_ignored_and_aborted_writes_roll_back_all_access_and_history() {
    for player in [true, false] {
        let writes: &[(&str, &str)] = if player {
            &[
                ("game_terminal_views", "DELETE"),
                ("game_connection_grants", "DELETE"),
                ("game_current_connections", "DELETE"),
                ("game_view_revisions", "DELETE"),
                ("game_receipts", "INSERT"),
            ]
        } else {
            &[
                ("game_connection_grants", "DELETE"),
                ("game_current_connections", "DELETE"),
                ("game_view_revisions", "DELETE"),
                ("game_receipts", "INSERT"),
            ]
        };
        for (table, event) in writes {
            for mode in ["IGNORE", "ABORT,'fault'"] {
                let f = Fixture::new();
                let (_, first, _, _) = f.start_two_player_game();
                f.service()
                    .prepare_player_connection(first.token(), id(&f.rt, 42).parse().unwrap())
                    .unwrap();
                let account = f
                    .service()
                    .prepare_account_connection(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        id(&f.rt, 43).parse().unwrap(),
                    )
                    .unwrap();
                f.service()
                    .accept_account_connection(
                        &account,
                        f.auth().authorize_game_account(&f.token).unwrap(),
                    )
                    .unwrap();
                end_without_winner(&f, f.command(90));
                let command = f.command(91);
                let before = all_rows(&f);
                fault(&f, table, event, mode);
                f.rt.now.set(f.rt.now.get() + 1);
                let result = if player {
                    f.service().exit_player(first.token(), command)
                } else {
                    f.service()
                        .exit_account(f.auth().authorize_game_account(&f.token).unwrap(), command)
                };
                assert_eq!(
                    result.err(),
                    Some(GameError::Storage),
                    "{event} {table} {mode}"
                );
                assert!(
                    all_rows(&f) == before,
                    "Failed Exit must not retire any grant/counter or change History/clock"
                );
                clear_fault(&f);
                if player {
                    f.service().exit_player(first.token(), command).unwrap();
                } else {
                    f.service()
                        .exit_account(f.auth().authorize_game_account(&f.token).unwrap(), command)
                        .unwrap();
                }
            }
        }
    }
}

#[test]
fn history_purge_faults_roll_back_every_child_route_release_intent_and_clock() {
    for (table, event) in EXPIRY_FAULTS {
        for mode in ["IGNORE", "ABORT,'fault'"] {
            let f = Fixture::new();
            let (_, _, _, connections) = f.start_two_player_game();
            call_once(&f, &connections);
            end_without_winner(&f, f.command(90));
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
            let expires = f
                .service()
                .directory_projection()
                .unwrap()
                .history_expires_at()
                .unwrap();
            let before = all_rows(&f);
            fault(&f, table, event, mode);
            f.rt.now.set(expires);
            assert_eq!(
                f.service().cleanup().err(),
                Some(GameError::Storage),
                "{event} {table} {mode}"
            );
            assert!(
                all_rows(&f) == before,
                "{event} {table} {mode} must roll back the full purge and routing proof"
            );
            clear_fault(&f);
            assert!(f.service().cleanup().unwrap().is_some());
            assert_eq!(count(&f, "game_history"), 0);
            assert_eq!(count(&f, "game_terminal_route"), 1);
        }
    }
}
