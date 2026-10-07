//! Grounded additional write/entropy probes; previously passing regression coverage.
use super::*;
use brews_domain::games::CellPosition;
use std::cell::Cell;

fn host_revision(f: &Fixture, connections: &[ConnectionId]) -> u64 {
    f.service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            connections,
        )
        .unwrap()
        .view_revision()
}
struct ScriptRuntime<'a> {
    clock: &'a TestRuntime,
    calls: Cell<usize>,
    reject_only: bool,
}
impl brews_backend::auth::Runtime for ScriptRuntime<'_> {
    fn now_ms(&self) -> i64 {
        self.clock.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), brews_backend::auth::AuthError> {
        assert_eq!(
            bytes.len(),
            4,
            "Random call consumes bounded-index entropy only"
        );
        let call = self.calls.get();
        self.calls.set(call + 1);
        let word = if self.reject_only || call == 0 {
            0_u32
        } else {
            4_u32
        };
        bytes.copy_from_slice(&word.to_le_bytes());
        Ok(())
    }
}
#[test]
fn random_call_unbiased_rejection_chooses_remaining_index_and_replay_uses_no_entropy() {
    for reject_only in [false, true] {
        let f = Fixture::new();
        let (_, _, _, connections) = f.start_two_player_game();
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(50),
                "1",
                host_revision(&f, &connections),
            )
            .unwrap();
        let revision = host_revision(&f, &connections);
        let command = f.command(51);
        let before = all_rows(&f);
        let rt = ScriptRuntime {
            clock: &f.rt,
            calls: Cell::new(0),
            reject_only,
        };
        let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let result = service.call_random(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            revision,
        );
        if reject_only {
            assert_eq!(result.err(), Some(GameError::GenerationExhausted));
            assert_eq!(
                rt.calls.get(),
                brews_domain::games::RANDOM_INDEX_REJECTION_BUDGET
            );
            assert!(all_rows(&f) == before);
            println!(
                "random_rejection_budget_verified={} durable_equal=true",
                rt.calls.get()
            );
        } else {
            assert!(
                matches!(result.unwrap(),GameResponse::CallAccepted {call,..} if call.sequence_no==2 && call.value=="3")
            );
            assert_eq!(rt.calls.get(), 2);
            assert!(matches!(
                service
                    .call_random(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        command,
                        revision
                    )
                    .unwrap(),
                GameResponse::Committed { .. }
            ));
            assert_eq!(rt.calls.get(), 2);
            println!(
                "random_unbiased_remaining_verified=zero_rejected_then_index_one_selects_3; entropy_calls=2; retry_entropy_calls=0"
            );
        }
    }
}
fn started_with_cleanup_sources(f: &Fixture) {
    let config = GameConfiguration {
        numeric_upper_bound: 5,
        board_side_length: 2,
        free_cells_enabled: true,
        free_cell_positions: vec![CellPosition { row: 2, column: 2 }],
        player_capacity: 2,
        spectator_capacity: 0,
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let context = f
        .service()
        .admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let answer = brews_backend::security::new_token(&f.rt).unwrap();
    let input = brews_contracts::games::JoinPlayer::decode_json(
        format!(
            "{{\"game_code\":\"{}\",\"alias\":\"First\",\"recovery_answer\":\"{answer}\"}}",
            code.as_str()
        )
        .as_bytes(),
    )
    .unwrap();
    let first = f
        .service()
        .join_player(f.command(20), context.token(), None, input)
        .unwrap()
        .cookie
        .unwrap();
    let second = f.player(&code, 21, "Second");
    let connections = [
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
    let reservation = f
        .directory()
        .confirm_reservation(f.service().directory_projection().unwrap().game_id())
        .unwrap();
    f.service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(30),
            host_revision(f, &connections),
            &reservation,
            &connections,
        )
        .unwrap();
    for n in 1..=5_u8 {
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(50 + n),
                &n.to_string(),
                host_revision(f, &connections),
            )
            .unwrap();
    }
    // Include an unused prepared binding so terminal retirement must delete a grant as well as the current socket.
    f.service()
        .prepare_player_connection(first.token(), id(&f.rt, 42).parse().unwrap())
        .unwrap();
    // Retiring one original player session forces connection/grant and unused-counter cleanup.
    f.game
        .execute(
            "UPDATE game_sessions SET revoked_at=? WHERE token_verifier=?",
            &[
                SqlValue::Integer(f.rt.now.get()),
                SqlValue::Blob(
                    brews_backend::security::token_digest(first.token())
                        .unwrap()
                        .to_vec(),
                ),
            ],
        )
        .unwrap();
    for t in [
        "game_free_cells",
        "game_recovery",
        "game_admission_contexts",
        "game_completed_lines",
    ] {
        assert!(
            count(f, t) > 0,
            "Required fault target must actually have rows"
        );
    }
}
#[test]
fn additional_terminal_required_writes_ignore_abort_rollback_and_exact_retry() {
    let mut faults = 0;
    for (table, event) in [
        ("game_completed_lines", "DELETE"),
        ("game_recovery", "DELETE"),
        ("game_free_cells", "DELETE"),
        ("game_admission_contexts", "DELETE"),
        ("game_connection_grants", "DELETE"),
        ("game_current_connections", "DELETE"),
        ("game_view_revisions", "UPDATE"),
        ("game_view_revisions", "DELETE"),
        ("game_pending_work", "INSERT"),
    ] {
        for mode in ["IGNORE", "ABORT,'independent fault'"] {
            let f = Fixture::new();
            started_with_cleanup_sources(&f);
            let command = f.command(90);
            let before = all_rows(&f);
            f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER terminal_fault BEFORE {event} ON {table} BEGIN SELECT RAISE({mode}); END")).unwrap();
            f.rt.now.set(f.rt.now.get() + 1);
            let err = f
                .service()
                .cancel_game(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    command,
                    GameState::InProgress,
                    true,
                )
                .err();
            let equal = all_rows(&f) == before;
            println!(
                "extra_terminal_fault table={table} event={event} mode={mode} error={err:?} durable_equal={equal}"
            );
            assert_eq!(err, Some(GameError::Storage));
            assert!(equal);
            f.game
                .conn
                .borrow()
                .execute_batch("DROP TRIGGER terminal_fault")
                .unwrap();
            f.service()
                .cancel_game(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    command,
                    GameState::InProgress,
                    true,
                )
                .unwrap();
            assert_eq!(count(&f, "game_history"), 1);
            assert_eq!(
                count(&f, "game_terminal_views"),
                1,
                "Revoked session cannot become final grant"
            );
            faults += 1;
        }
    }
    println!("additional_terminal_fault_matrix_verified={faults}");
}

#[test]
fn all_call_write_faults_rollback_then_original_command_retries() {
    let mut faults = 0;
    for (table, event) in [
        ("game_calls", "INSERT"),
        ("game_board_cells", "UPDATE"),
        ("game_boards", "UPDATE"),
        ("game_completed_lines", "INSERT"),
        ("game_completed_lines", "DELETE"),
        ("game_record", "UPDATE"),
        ("game_view_revisions", "UPDATE"),
        ("game_receipts", "INSERT"),
    ] {
        for mode in ["IGNORE", "ABORT,'independent fault'"] {
            let f = Fixture::new();
            let (_, first, _, connections) = f.start_two_player_game();
            let own = match f.service().player_view(first.token()).unwrap() {
                GameView::Player {
                    board: Some(board), ..
                } => board,
                _ => panic!("Expected live board"),
            };
            // Three accepted cells ensure INSERT and DELETE of completed lines both have work.
            let values: Vec<String> = own
                .cells
                .iter()
                .map(|c| match &c.kind {
                    brews_domain::games::BoardCellKind::Value(v) => v.clone(),
                    _ => panic!("Expected ordinary cell"),
                })
                .collect();
            for (i, value) in values.iter().take(3).enumerate() {
                let expected = host_revision(&f, &connections);
                f.service()
                    .call_manual_value(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        f.command(50 + i as u8),
                        value,
                        expected,
                    )
                    .unwrap();
            }
            let expected = host_revision(&f, &connections);
            let command = f.command(60);
            let before = all_rows(&f);
            f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER reject_call_write BEFORE {event} ON {table} BEGIN SELECT RAISE({mode}); END")).unwrap();
            f.rt.now.set(f.rt.now.get() + 1);
            let error = f
                .service()
                .call_manual_value(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    command,
                    &values[3],
                    expected,
                )
                .err();
            let equal = all_rows(&f) == before;
            println!(
                "call_fault table={table} event={event} mode={mode} error={error:?} durable_equal={equal}"
            );
            assert_eq!(error, Some(GameError::Storage), "Fault must be fatal");
            assert!(
                equal,
                "Fault must roll back every durable row, including clock"
            );
            f.game
                .conn
                .borrow()
                .execute_batch("DROP TRIGGER reject_call_write")
                .unwrap();
            assert!(matches!(
                f.service()
                    .call_manual_value(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        command,
                        &values[3],
                        expected
                    )
                    .unwrap(),
                GameResponse::CallAccepted { .. }
            ));
            assert_eq!(count(&f, "game_calls"), 4);
            faults += 1;
        }
    }
    println!("call_fault_matrix_verified={faults}");
}
