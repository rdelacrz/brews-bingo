use super::*;
use brews_backend::db::game::{AudienceMutation, SpectatorAuthority};
use brews_backend::game::{SPECTATOR_GRACE_MS, SecretCookie, SpectatorConnectionGrant};
use brews_contracts::games::{GameOutcome, JoinPlayer, ParticipantChange, ParticipantIdentity};
use brews_domain::games::GameState;

fn audience(f: &Fixture, code: &brews_domain::games::GameCode, n: u8) -> AudienceMutation {
    let context = f
        .service()
        .spectator_admission_context(code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    f.service()
        .join_spectator(f.command(n), context.token(), None, code)
        .unwrap()
}
fn connect(f: &Fixture, cookie: &SecretCookie, n: u8) -> SpectatorConnectionGrant {
    let grant = f
        .service()
        .prepare_spectator_connection(cookie.token(), id(&f.rt, n).parse().unwrap())
        .unwrap();
    f.service().accept_spectator_connection(&grant).unwrap();
    grant
}
fn proof(f: &Fixture, cookie: &SecretCookie) -> SpectatorAuthority {
    f.service().sync_spectator(cookie.token(), None).unwrap().1
}
fn count(f: &Fixture, table: &str) -> i64 {
    let rows = f
        .game
        .query(&format!("SELECT count(*) FROM {table}"), &[])
        .unwrap();
    match rows[0][0] {
        SqlValue::Integer(n) => n,
        _ => panic!("count"),
    }
}
fn contents(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    [
        "game_players",
        "game_recovery",
        "game_sessions",
        "game_spectators",
        "game_spectator_sessions",
        "game_current_connections",
        "game_connection_grants",
        "game_socket_closures",
        "game_view_revisions",
        "game_receipts",
    ]
    .map(|t| {
        f.game
            .query(&format!("SELECT * FROM {t} ORDER BY 1"), &[])
            .unwrap()
    })
    .to_vec()
}
#[test]
fn spectators_capacity_zero_and_fifty_are_separate_from_players() {
    for capacity in [0, 1, 50] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration {
            spectator_capacity: capacity,
            ..GameConfiguration::default()
        });
        for n in 0..capacity {
            audience(&f, &code, 40 + n);
        }
        let context = f
            .service()
            .spectator_admission_context(&code, None, None)
            .unwrap()
            .cookie
            .unwrap();
        assert!(matches!(
            f.service()
                .join_spectator(f.command(120), context.token(), None, &code),
            Err(GameError::Capacity)
        ));
        assert_eq!(count(&f, "game_spectators"), i64::from(capacity));
        assert_eq!(count(&f, "game_players"), 0);
    }
}
#[test]
fn spectator_retry_is_actor_scoped_secret_free_and_allocates_once() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let context = f
        .service()
        .spectator_admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let repeated = f
        .service()
        .spectator_admission_context(&code, Some(context.token()), None)
        .unwrap();
    assert!(repeated.cookie.is_none());
    let out = f
        .service()
        .join_spectator(f.command(50), context.token(), None, &code)
        .unwrap();
    let retry = f
        .service()
        .join_spectator(f.command(50), context.token(), None, &code)
        .unwrap();
    assert!(retry.participant.cookie.is_none());
    assert!(matches!(
        retry.participant.response,
        GameResponse::Committed { .. }
    ));
    assert_eq!(count(&f, "game_spectators"), 1);
    let serialized = out.participant.response.encode_json().unwrap();
    assert!(
        !String::from_utf8(serialized)
            .unwrap()
            .contains(out.participant.cookie.as_ref().unwrap().token())
    );
    let another = f
        .service()
        .spectator_admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let other = f
        .service()
        .join_spectator(f.command(50), another.token(), None, &code)
        .unwrap();
    assert!(other.participant.cookie.is_some());
    assert_eq!(count(&f, "game_spectators"), 2);
    assert!(
        f.service()
            .join_spectator(f.command(51), context.token(), None, &code)
            .is_err()
    );
    assert!(
        f.service()
            .join_spectator(f.command(50), "absent", None, &code)
            .is_err()
    );
}
#[test]
fn initial_unconnected_seat_has_bounded_exact_grace_and_clock_rollback_fence() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let out = audience(&f, &code, 40);
    let cookie = out.participant.cookie.unwrap();
    let original = proof(&f, &cookie);
    let before = f.rt.now.get();
    f.rt.now.set(before + SPECTATOR_GRACE_MS - 1);
    assert!(f.service().spectator_view(cookie.token()).is_ok());
    f.rt.now.set(before + SPECTATOR_GRACE_MS);
    assert!(f.service().spectator_view(cookie.token()).is_err());
    assert_eq!(count(&f, "game_spectators"), 0);
    assert_eq!(count(&f, "game_spectator_sessions"), 0);
    f.rt.now.set(before);
    assert!(
        f.service()
            .release_spectator_output(cookie.token(), &original)
            .is_err()
    );
    assert_eq!(count(&f, "game_spectators"), 0);
}
#[test]
fn spectator_disconnect_duplicate_stale_close_and_exclusive_reconnect() {
    for offset in [
        SPECTATOR_GRACE_MS - 1,
        SPECTATOR_GRACE_MS,
        SPECTATOR_GRACE_MS + 1,
    ] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let out = audience(&f, &code, 40);
        let cookie = out.participant.cookie.unwrap();
        let original = proof(&f, &cookie);
        let first = connect(&f, &cookie, 60);
        let second = connect(&f, &cookie, 61);
        f.service().disconnect_spectator(&first).unwrap();
        assert_eq!(
            f.service()
                .spectator_connection(second.connection_id())
                .unwrap(),
            Some(second.clone())
        );
        f.service().disconnect_spectator(&second).unwrap();
        let disconnect = f.rt.now.get();
        f.rt.now.set(disconnect + 1);
        f.service().disconnect_spectator(&second).unwrap();
        let rows = f
            .game
            .query(
                "SELECT disconnected_at,grace_expires_at FROM game_spectators",
                &[],
            )
            .unwrap();
        assert_eq!(
            rows,
            vec![vec![
                SqlValue::Integer(disconnect),
                SqlValue::Integer(disconnect + SPECTATOR_GRACE_MS)
            ]]
        );
        f.rt.now.set(disconnect + offset);
        let reconnect = f
            .service()
            .prepare_spectator_connection(cookie.token(), id(&f.rt, 62).parse().unwrap());
        if offset < SPECTATOR_GRACE_MS {
            let g = reconnect.unwrap();
            f.service().accept_spectator_connection(&g).unwrap();
            let p = proof(&f, &cookie);
            assert_eq!(p.spectator_id, original.spectator_id);
            assert_eq!(p.session_expires_at, original.session_expires_at);
            f.rt.now.set(disconnect + SPECTATOR_GRACE_MS);
            f.service().cleanup().unwrap();
            assert_eq!(count(&f, "game_spectators"), 1);
        } else {
            assert!(reconnect.is_err());
            assert_eq!(count(&f, "game_spectators"), 0);
        }
    }
}
#[test]
fn spectator_replacement_preparation_does_not_supersede_until_acceptance() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let out = audience(&f, &code, 40);
    let cookie = out.participant.cookie.unwrap();
    let old = connect(&f, &cookie, 60);
    let pending = f
        .service()
        .prepare_spectator_connection(cookie.token(), id(&f.rt, 61).parse().unwrap())
        .unwrap();
    assert!(f.service().spectator_connection_view(&old).is_ok());
    assert!(f.service().spectator_connection_view(&pending).is_err());
    f.service().accept_spectator_connection(&pending).unwrap();
    assert!(f.service().spectator_connection_view(&old).is_err());
    f.service().disconnect_spectator(&old).unwrap();
    assert!(f.service().spectator_connection_view(&pending).is_ok());
}
#[test]
fn spectator_leave_deletes_immediately_and_deleted_cookie_cannot_retry() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let out = audience(&f, &code, 40);
    let cookie = out.participant.cookie.unwrap();
    let g = connect(&f, &cookie, 60);
    let command = f.command(70);
    let leave = f
        .service()
        .leave_spectator(command, cookie.token(), GameState::AwaitingPlayers)
        .unwrap();
    assert!(leave.clears_spectator_cookie);
    assert!(leave.participant.cookie.is_none());
    assert_eq!(count(&f, "game_spectators"), 0);
    assert!(f.service().spectator_connection_view(&g).is_err());
    assert!(
        f.service()
            .leave_spectator(command, cookie.token(), GameState::AwaitingPlayers)
            .is_err()
    );
    let fresh = audience(&f, &code, 71);
    assert!(fresh.participant.cookie.is_some());
}
#[test]
fn role_switch_uses_fresh_identity_deletes_answer_and_preserves_absolute_expiry() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let player = f.player(&code, 30, "Exact");
    let old = f.service().player_view(player.token()).unwrap();
    let player_id = match old {
        GameView::Player { player_id, .. } => player_id,
        _ => panic!("player"),
    };
    f.service()
        .manage_recovery_answer(f.command(31), player.token(), Some("my protected answer"))
        .unwrap();
    let expiry = player.expires_at();
    f.rt.now.set(f.rt.now.get() + 10);
    let switched = f
        .service()
        .switch_to_spectator(f.command(40), player.token())
        .unwrap();
    let spectator = switched.participant.cookie.unwrap();
    assert_eq!(spectator.expires_at(), expiry);
    assert_eq!(count(&f, "game_players"), 0);
    assert_eq!(count(&f, "game_recovery"), 0);
    assert_eq!(count(&f, "game_sessions"), 0);
    assert!(f.service().player_view(player.token()).is_err());
    let switched=f.service().switch_to_player(f.command(41),spectator.token(),brews_contracts::games::JoinPlayer::decode_json(format!(r#"{{"game_code":"{}","alias":"Exact","recovery_answer":"fresh protected answer"}}"#,code.as_str()).as_bytes()).unwrap()).unwrap();
    let new_player = switched.participant.cookie.unwrap();
    assert_eq!(new_player.expires_at(), expiry);
    let GameView::Player {
        player_id: new_id, ..
    } = f.service().player_view(new_player.token()).unwrap()
    else {
        panic!("player")
    };
    assert_ne!(new_id, player_id);
    assert_eq!(count(&f, "game_recovery"), 1);
    assert_eq!(count(&f, "game_spectators"), 0);
    assert!(f.service().spectator_view(spectator.token()).is_err());
    assert_eq!(count(&f, "game_boards"), 0);
}
#[test]
fn target_capacity_and_alias_failures_preserve_entire_old_role() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration {
        spectator_capacity: 1,
        player_capacity: 2,
        ..GameConfiguration::default()
    });
    let p = f.player(&code, 30, "Taken");
    let second = f.player(&code, 31, "Other");
    let out = audience(&f, &code, 40);
    let spectator = out.participant.cookie.unwrap();
    let before = contents(&f);
    assert!(matches!(
        f.service().switch_to_spectator(f.command(50), p.token()),
        Err(GameError::Capacity)
    ));
    assert_eq!(contents(&f), before);
    for alias in ["Taken", "Free"] {
        assert!(
            f.service()
                .switch_to_player(
                    f.command(51),
                    spectator.token(),
                    JoinPlayer {
                        game_code: code.clone(),
                        alias: alias.into(),
                        recovery_answer: None
                    }
                )
                .is_err()
        );
        assert_eq!(contents(&f), before);
    }
    assert!(f.service().player_view(second.token()).is_ok());
}
#[test]
fn spectator_switch_ignored_insert_delete_and_receipt_faults_roll_back() {
    for sql in [
        "CREATE TRIGGER fault BEFORE INSERT ON game_spectators BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER fault BEFORE DELETE ON game_sessions BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER fault BEFORE INSERT ON game_receipts BEGIN SELECT RAISE(IGNORE); END",
    ] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let player = f.player(&code, 30, "Fault");
        f.service()
            .manage_recovery_answer(f.command(31), player.token(), Some("private answer"))
            .unwrap();
        let before = contents(&f);
        f.game.conn.borrow().execute_batch(sql).unwrap();
        assert!(matches!(
            f.service()
                .switch_to_spectator(f.command(40), player.token()),
            Err(GameError::Storage)
        ));
        assert_eq!(contents(&f), before);
        assert!(f.service().player_view(player.token()).is_ok());
    }
}
#[test]
fn spectator_public_view_no_private_roster_answer_or_start_eligibility() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let p = f.player(&code, 30, "PRIVATE-ALIAS");
    let out = audience(&f, &code, 40);
    let spectator = out.participant.cookie.unwrap();
    let g = connect(&f, &spectator, 60);
    let before = f.service().spectator_view(spectator.token()).unwrap();
    let raw = serde_json::to_string(&before).unwrap();
    for forbidden in [
        "PRIVATE-ALIAS",
        "\"player_id\"",
        "\"board\"",
        "qualifying_lines",
        "recovery",
        "\"players\"",
    ] {
        assert!(!raw.contains(forbidden));
    }
    f.service()
        .rename_my_alias(f.command(31), p.token(), "PRIVATE-RENAME")
        .unwrap();
    let after = f.service().spectator_view(spectator.token()).unwrap();
    assert_eq!(after.view_revision(), before.view_revision());
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[g.connection_id()],
        )
        .unwrap();
    let rev = host.view_revision();
    assert!(matches!(
        f.service().start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(70),
            rev,
            &f.directory()
                .confirm_reservation(f.service().directory_projection().unwrap().game_id())
                .unwrap(),
            &[g.connection_id()]
        ),
        Err(GameError::Conflict)
    ));
}
#[test]
fn spectator_session_absolute_expiry_deletes_connected_identity() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let out = audience(&f, &code, 40);
    let spectator = out.participant.cookie.unwrap();
    connect(&f, &spectator, 60);
    f.rt.now.set(spectator.expires_at());
    assert!(f.service().spectator_view(spectator.token()).is_err());
    assert_eq!(count(&f, "game_spectators"), 0);
    assert_eq!(count(&f, "game_spectator_sessions"), 0);
}
#[test]
fn spectator_cancel_notice_is_public_bounded_single_use_and_authority_deleted() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let out = audience(&f, &code, 40);
    let spectator = out.participant.cookie.unwrap();
    let g = connect(&f, &spectator, 60);
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(70),
            GameState::AwaitingPlayers,
            true,
        )
        .unwrap();
    assert_eq!(count(&f, "game_spectators"), 1);
    assert_eq!(count(&f, "game_spectator_sessions"), 1);
    assert_eq!(count(&f, "game_history"), 0);
    assert!(f.service().spectator_view(spectator.token()).is_err());
    assert!(f.service().spectator_connection(g.connection_id()).is_err());
    let mut notices = f.service().begin_spectator_terminal_attempt().unwrap();
    assert_eq!(notices.len(), 1);
    let notice = notices.pop().unwrap();
    assert_eq!(notice.connection, g);
    assert!(
        f.service()
            .validate_spectator_notice_delivery(&notice)
            .is_ok()
    );
    assert!(notice.notice.contains("terminal_notice"));
    assert!(!notice.notice.contains("board"));
    assert!(!notice.notice.contains(&g.spectator_id().to_string()));
    assert!(
        f.service()
            .begin_spectator_terminal_attempt()
            .unwrap()
            .is_empty()
    );
    assert_eq!(count(&f, "game_spectator_sessions"), 1);
    f.service().finish_spectator_terminal_attempt().unwrap();
    assert_eq!(count(&f, "game_spectator_sessions"), 0);
    assert_eq!(count(&f, "game_spectators"), 0);
    assert_eq!(count(&f, "game_spectator_contexts"), 0);
    assert!(
        f.service()
            .validate_spectator_notice_delivery(&notice)
            .is_err()
    );
}
#[test]
fn spectator_admission_clock_decision_after_entropy_cannot_issue_expired_lobby_authority() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let context = f
        .service()
        .spectator_admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    f.rt.advance_random_ms.set(86_400_000);
    assert!(
        f.service()
            .join_spectator(f.command(40), context.token(), None, &code)
            .is_err()
    );
    assert_eq!(count(&f, "game_spectators"), 0);
}
#[test]
fn spectator_owner_schema_forward_migration_preserves_existing_rows_and_rejects_marker_fault() {
    for fault in [false, true] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let p = f.player(&code, 30, "Retained");
        let before = contents(&f);
        f.game.conn.borrow().execute_batch("DROP TABLE game_spectator_terminal_work; DROP TABLE game_spectator_notices; DROP TABLE game_spectator_contexts; DROP TABLE game_spectator_sessions; DROP TABLE game_spectators; UPDATE game_metadata SET schema_version=3;").unwrap();
        if fault {
            f.game.conn.borrow().execute_batch("CREATE TRIGGER fault BEFORE UPDATE OF schema_version ON game_metadata BEGIN SELECT RAISE(IGNORE); END").unwrap();
            assert_eq!(
                brews_backend::db::game::migrate_game(&f.game),
                Err(GameError::Storage)
            );
            assert!(
                f.game
                    .query(
                        "SELECT name FROM sqlite_master WHERE name='game_spectators'",
                        &[]
                    )
                    .unwrap()
                    .is_empty()
            );
        } else {
            brews_backend::db::game::migrate_game(&f.game).unwrap();
            assert_eq!(contents(&f), before);
            assert!(f.service().player_view(p.token()).is_ok());
        }
    }
}
#[test]
fn spectator_receipt_release_requires_original_context_or_current_target_authority() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let context = f
        .service()
        .spectator_admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let out = f
        .service()
        .join_spectator(f.command(40), context.token(), None, &code)
        .unwrap();
    let GameResponse::ParticipantChanged { receipt } = &out.participant.response else {
        panic!("receipt")
    };
    assert!(matches!(
        receipt.outcome,
        GameOutcome::ParticipantChanged {
            change: ParticipantChange::Admitted,
            identity: ParticipantIdentity::Spectator { .. },
            ..
        }
    ));
    assert!(
        f.service()
            .release_audience_receipt(receipt, None, None)
            .is_err()
    );
    assert!(
        f.service()
            .release_audience_receipt(receipt, Some(&f.token), None)
            .is_err()
    );
    assert!(
        f.service()
            .release_audience_receipt(receipt, None, Some(context.token()))
            .is_ok()
    );
}

#[test]
fn spectator_migration_accepts_sqlite_renamed_identifiers_from_previous_owner_migrations() {
    let db = Sqlite::new();
    db.conn
        .borrow()
        .execute_batch(include_str!("../fixtures/game-schema-v1.sql"))
        .unwrap();
    brews_backend::db::game::migrate_game(&db).unwrap();
    db.conn.borrow().execute_batch("DROP TABLE game_spectator_terminal_work; DROP TABLE game_spectator_notices; DROP TABLE game_spectator_contexts; DROP TABLE game_spectator_sessions; DROP TABLE game_spectators; UPDATE game_metadata SET schema_version=3;").unwrap();
    brews_backend::db::game::migrate_game(&db).unwrap();
    assert_eq!(
        db.query("SELECT schema_version FROM game_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(4)]]
    );
}

struct EnrollmentRaceRuntime<'a> {
    runtime: &'a TestRuntime,
    change: std::cell::RefCell<Option<Box<dyn FnOnce() + 'a>>>,
}
impl brews_backend::auth::Runtime for EnrollmentRaceRuntime<'_> {
    fn now_ms(&self) -> i64 {
        self.runtime.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), brews_backend::auth::AuthError> {
        brews_backend::auth::Runtime::fill_random(self.runtime, bytes)?;
        if let Some(change) = self.change.borrow_mut().take() {
            change();
        }
        Ok(())
    }
}
#[test]
fn spectator_player_enrollment_revalidates_actual_start_leave_terminal_and_capacity_races() {
    for race in ["start", "leave", "terminal", "alias", "full"] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration {
            player_capacity: 3,
            ..GameConfiguration::default()
        });
        let first = f.player(&code, 30, "First");
        let second = f.player(&code, 31, "Second");
        let mut observed = Vec::new();
        for (n, cookie) in [(60, &first), (61, &second)] {
            let grant = f
                .service()
                .prepare_player_connection(cookie.token(), id(&f.rt, n).parse().unwrap())
                .unwrap();
            f.service().accept_player_connection(&grant).unwrap();
            observed.push(grant.connection_id());
        }
        let spectator = audience(&f, &code, 40).participant.cookie.unwrap();
        let answer = brews_backend::security::new_token(&f.rt).unwrap();
        let rt = EnrollmentRaceRuntime {
            runtime: &f.rt,
            change: std::cell::RefCell::new(Some(Box::new(|| match race {
                "start" => {
                    let host = f
                        .service()
                        .account_view(
                            f.auth().authorize_game_account(&f.token).unwrap(),
                            &observed,
                        )
                        .unwrap();
                    let reservation = f
                        .directory()
                        .confirm_reservation(f.service().directory_projection().unwrap().game_id())
                        .unwrap();
                    f.service()
                        .start(
                            f.auth().authorize_game_account(&f.token).unwrap(),
                            f.command(70),
                            host.view_revision(),
                            &reservation,
                            &observed,
                        )
                        .unwrap();
                }
                "leave" => {
                    f.service()
                        .leave_spectator(
                            f.command(71),
                            spectator.token(),
                            GameState::AwaitingPlayers,
                        )
                        .unwrap();
                }
                "terminal" => {
                    f.service()
                        .cancel_game(
                            f.auth().authorize_game_account(&f.token).unwrap(),
                            f.command(72),
                            GameState::AwaitingPlayers,
                            true,
                        )
                        .unwrap();
                }
                "alias" | "full" => {
                    f.player(
                        &code,
                        32,
                        if race == "alias" {
                            "RaceAlias"
                        } else {
                            "Third"
                        },
                    );
                }
                _ => unreachable!(),
            }))),
        };
        let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let result = service.switch_to_player(
            f.command(50),
            spectator.token(),
            JoinPlayer {
                game_code: code.clone(),
                alias: "RaceAlias".into(),
                recovery_answer: Some(
                    serde_json::from_value(serde_json::Value::String(answer)).unwrap(),
                ),
            },
        );
        assert!(result.is_err());
        assert_eq!(count(&f, "game_recovery"), 0);
        if ["start", "alias", "full"].contains(&race) {
            assert_eq!(count(&f, "game_spectators"), 1);
            assert!(f.service().spectator_view(spectator.token()).is_ok());
        } else {
            if race == "terminal" {
                assert!(f.service().spectator_view(spectator.token()).is_err());
                f.service().begin_spectator_terminal_attempt().unwrap();
                f.service().finish_spectator_terminal_attempt().unwrap();
            }
            assert_eq!(count(&f, "game_spectators"), 0);
        }
        assert_eq!(
            f.game
                .query("SELECT 1 FROM game_players WHERE alias='RaceAlias'", &[])
                .unwrap()
                .len(),
            usize::from(race == "alias")
        );
    }
}

#[test]
fn spectator_terminal_notice_contains_the_maximum_legal_ordered_call_pool() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration {
        numeric_upper_bound: 1000,
        board_side_length: 2,
        free_cells_enabled: false,
        free_cell_positions: Vec::new(),
        ..GameConfiguration::default()
    });
    let mut observed = Vec::new();
    for (n, alias) in [(30, "First"), (31, "Second")] {
        let cookie = f.player(&code, n, alias);
        let grant = f
            .service()
            .prepare_player_connection(cookie.token(), id(&f.rt, n + 30).parse().unwrap())
            .unwrap();
        f.service().accept_player_connection(&grant).unwrap();
        observed.push(grant.connection_id());
    }
    let spectator = audience(&f, &code, 40).participant.cookie.unwrap();
    let grant = connect(&f, &spectator, 62);
    observed.push(grant.connection_id());
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let reservation = f
        .directory()
        .confirm_reservation(f.service().directory_projection().unwrap().game_id())
        .unwrap();
    let started = f
        .service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(70),
            host.view_revision(),
            &reservation,
            &observed,
        )
        .unwrap();
    let GameResponse::Started {
        mut view_revision, ..
    } = started
    else {
        panic!("Start")
    };
    for value in 1_u32..=1000 {
        let mut bytes = *uuid::Uuid::parse_str(&id(&f.rt, 0)).unwrap().as_bytes();
        bytes[10..12].copy_from_slice(&[1, 1]);
        bytes[12..].copy_from_slice(&value.to_be_bytes());
        let command = uuid::Uuid::from_bytes(bytes).to_string().parse().unwrap();
        let called = f
            .service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &value.to_string(),
                view_revision,
            )
            .unwrap();
        let GameResponse::CallAccepted {
            view_revision: next,
            ..
        } = called
        else {
            panic!("call")
        };
        view_revision = next;
    }
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(71),
            GameState::InProgress,
            true,
        )
        .unwrap();
    let notices = f.service().begin_spectator_terminal_attempt().unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].connection, grant);
    assert_eq!(count(&f, "game_spectator_sessions"), 1);
    assert!(f.service().spectator_view(spectator.token()).is_err());
    let raw = &notices[0].notice;
    let notice: serde_json::Value = serde_json::from_str(raw).unwrap();
    assert_eq!(notice["calls"].as_array().unwrap().len(), 1000);
    assert_eq!(notice["calls"][999]["value"], "1000");
    assert!(raw.len() > 32768);
    assert!(raw.len() < brews_contracts::games::GAME_FRAME_MAX_BYTES);
    f.service().finish_spectator_terminal_attempt().unwrap();
    assert_eq!(count(&f, "game_spectator_sessions"), 0);
}

#[test]
fn spectator_terminal_attempt_cleanup_fault_reconstructs_without_replay_or_blocking_release() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let spectator = audience(&f, &code, 40).participant.cookie.unwrap();
    let grant = connect(&f, &spectator, 60);
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(70),
            GameState::AwaitingPlayers,
            true,
        )
        .unwrap();
    let notices = f.service().begin_spectator_terminal_attempt().unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].connection, grant);
    // Actual native send boundary: terminal committed, authority denied, rows retained.
    assert_eq!(
        f.service().directory_projection().unwrap().state(),
        GameState::Cancelled
    );
    assert!(f.service().spectator_view(spectator.token()).is_err());
    assert_eq!(count(&f, "game_spectators"), 1);
    assert_eq!(count(&f, "game_spectator_sessions"), 1);
    assert!(
        f.service()
            .validate_spectator_notice_delivery(&notices[0])
            .is_ok()
    );
    f.game.conn.borrow().execute_batch("CREATE TRIGGER spectator_cleanup BEFORE DELETE ON game_spectator_sessions BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert_eq!(
        f.service().finish_spectator_terminal_attempt(),
        Err(GameError::Storage)
    );
    f.service().retry_spectator_terminal_cleanup().unwrap();
    assert_eq!(count(&f, "game_spectator_sessions"), 1);
    assert!(
        f.service()
            .begin_spectator_terminal_attempt()
            .unwrap()
            .is_empty()
    );
    let release = f
        .service()
        .pending_work(100)
        .unwrap()
        .into_iter()
        .find(|w| w.kind() == brews_backend::game::WorkKind::Release)
        .unwrap();
    let proof = f.service().cleanup().unwrap().unwrap();
    let ack = f.directory().release_game(proof).unwrap();
    f.service().acknowledge_release(&release, ack).unwrap();
    assert!(f.directory().confirm_reservation(grant.game_id()).is_err());
    assert!(
        f.service().next_deadline().unwrap().unwrap()
            <= f.rt.now.get() + brews_backend::game::RETRY_INITIAL_MS
    );
    f.game
        .conn
        .borrow()
        .execute_batch("DROP TRIGGER spectator_cleanup")
        .unwrap();
    let reconstructed = f.service();
    assert!(
        reconstructed
            .begin_spectator_terminal_attempt()
            .unwrap()
            .is_empty()
    );
    reconstructed.finish_spectator_terminal_attempt().unwrap();
    for table in [
        "game_spectators",
        "game_spectator_sessions",
        "game_spectator_contexts",
        "game_spectator_notices",
        "game_spectator_terminal_work",
    ] {
        assert_eq!(count(&f, table), 0);
    }
    assert!(reconstructed.spectator_view(spectator.token()).is_err());
}

#[test]
fn spectator_terminal_crash_before_attempt_is_finite_and_cannot_reconnect_or_fetch() {
    for offset in [-1, 0, 1] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let spectator = audience(&f, &code, 40).participant.cookie.unwrap();
        let grant = connect(&f, &spectator, 60);
        f.service()
            .cancel_game(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(70),
                GameState::AwaitingPlayers,
                true,
            )
            .unwrap();
        let rows = f
            .game
            .query(
                "SELECT attempt_deadline FROM game_spectator_terminal_work",
                &[],
            )
            .unwrap();
        let SqlValue::Integer(deadline) = rows[0][0] else {
            panic!("deadline")
        };
        f.rt.now.set(deadline + offset);
        let reconstructed = f.service();
        assert!(reconstructed.spectator_view(spectator.token()).is_err());
        assert!(
            reconstructed
                .prepare_spectator_connection(spectator.token(), id(&f.rt, 61).parse().unwrap())
                .is_err()
        );
        assert!(
            reconstructed
                .spectator_connection(grant.connection_id())
                .is_err()
        );
        let notices = reconstructed.begin_spectator_terminal_attempt().unwrap();
        assert_eq!(notices.len(), usize::from(offset < 0));
        reconstructed.finish_spectator_terminal_attempt().unwrap();
        assert_eq!(count(&f, "game_spectator_sessions"), 0);
        assert_eq!(count(&f, "game_spectators"), 0);
        assert!(
            reconstructed
                .begin_spectator_terminal_attempt()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn spectator_terminal_notice_excludes_disconnected_prepared_and_expired_authority() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let connected = audience(&f, &code, 40).participant.cookie.unwrap();
    let grant = connect(&f, &connected, 60);
    let disconnected = audience(&f, &code, 41).participant.cookie.unwrap();
    let old = connect(&f, &disconnected, 61);
    f.service().disconnect_spectator(&old).unwrap();
    let prepared = audience(&f, &code, 42).participant.cookie.unwrap();
    f.service()
        .prepare_spectator_connection(prepared.token(), id(&f.rt, 62).parse().unwrap())
        .unwrap();
    let expired = audience(&f, &code, 43).participant.cookie.unwrap();
    // Retain the current spectator while the never-connected spectators expire.
    f.rt.now.set(f.rt.now.get() + SPECTATOR_GRACE_MS);
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(70),
            GameState::AwaitingPlayers,
            true,
        )
        .unwrap();
    let notices = f.service().begin_spectator_terminal_attempt().unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].connection, grant);
    for cookie in [&connected, &disconnected, &prepared, &expired] {
        assert!(f.service().spectator_view(cookie.token()).is_err());
    }
    f.service().finish_spectator_terminal_attempt().unwrap();
    assert_eq!(count(&f, "game_spectator_sessions"), 0);
}

#[test]
fn spectator_preflight_and_admission_bad_selected_session_do_not_fall_back_or_allocate() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let context = f
        .service()
        .spectator_admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let before = contents(&f);
    for supplied_context in [None, Some(context.token())] {
        assert!(matches!(
            f.service()
                .spectator_admission_context(&code, supplied_context, Some(&f.token)),
            Err(GameError::Unauthorized)
        ));
        assert_eq!(contents(&f), before);
    }
    assert!(matches!(
        f.service()
            .join_spectator(f.command(40), context.token(), Some(&f.token), &code),
        Err(GameError::Unauthorized)
    ));
    assert_eq!(contents(&f), before);
    let out = f
        .service()
        .join_spectator(f.command(40), context.token(), None, &code)
        .unwrap();
    let spectator = out.participant.cookie.unwrap();
    let before = contents(&f);
    let reused = f
        .service()
        .spectator_admission_context(&code, Some(context.token()), Some(spectator.token()))
        .unwrap();
    assert!(reused.cookie.is_none());
    assert!(
        matches!(reused.response,GameResponse::AdmissionContext{expires_at} if expires_at==spectator.expires_at())
    );
    assert_eq!(contents(&f), before);
}

#[test]
fn spectator_all_terminal_causes_deny_before_notice_and_delete_after_attempt() {
    for ending in ["operator", "idle", "cancel", "winner"] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration {
            numeric_upper_bound: 4,
            board_side_length: 2,
            free_cells_enabled: false,
            free_cell_positions: Vec::new(),
            ..GameConfiguration::default()
        });
        let mut observed = Vec::new();
        let mut winner = None;
        if ["cancel", "winner"].contains(&ending) {
            for (n, alias) in [(30, "First"), (31, "Second")] {
                let p = f.player(&code, n, alias);
                let g = f
                    .service()
                    .prepare_player_connection(p.token(), id(&f.rt, n + 30).parse().unwrap())
                    .unwrap();
                f.service().accept_player_connection(&g).unwrap();
                winner = Some(g.player_id());
                observed.push(g.connection_id());
            }
            let host = f
                .service()
                .account_view(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    &observed,
                )
                .unwrap();
            let reservation = f
                .directory()
                .confirm_reservation(f.service().directory_projection().unwrap().game_id())
                .unwrap();
            f.service()
                .start(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(70),
                    host.view_revision(),
                    &reservation,
                    &observed,
                )
                .unwrap();
            if ending == "winner" {
                for n in 1..=4 {
                    let host = f
                        .service()
                        .account_view(
                            f.auth().authorize_game_account(&f.token).unwrap(),
                            &observed,
                        )
                        .unwrap();
                    f.service()
                        .call_manual_value(
                            f.auth().authorize_game_account(&f.token).unwrap(),
                            f.command(80 + n),
                            &n.to_string(),
                            host.view_revision(),
                        )
                        .unwrap();
                }
            }
        }
        let spectator = audience(&f, &code, 40).participant.cookie.unwrap();
        let g = connect(&f, &spectator, 62);
        observed.push(g.connection_id());
        let prior_authority = proof(&f, &spectator);
        assert_eq!(
            f.service().spectator_connection(g.connection_id()).unwrap(),
            Some(g.clone())
        );
        if ending == "idle" {
            let due = f.rt.now.get() + 1000;
            f.game
                .execute(
                    "UPDATE game_record SET last_host_activity=?,idle_due=?",
                    &[
                        SqlValue::Integer(due - brews_backend::game::HOST_IDLE_MS),
                        SqlValue::Integer(due),
                    ],
                )
                .unwrap();
            f.rt.now.set(due);
            f.service().cleanup().unwrap();
        } else if ending == "winner" {
            let host = f
                .service()
                .account_view(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    &observed,
                )
                .unwrap();
            f.service()
                .submit_winner(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(90),
                    winner.unwrap(),
                    host.view_revision(),
                )
                .unwrap();
        } else {
            f.service()
                .cancel_game(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(90),
                    if ending == "cancel" {
                        GameState::InProgress
                    } else {
                        GameState::AwaitingPlayers
                    },
                    true,
                )
                .unwrap();
        }
        assert!(f.service().spectator_view(spectator.token()).is_err());
        if ["cancel", "winner"].contains(&ending) {
            assert_eq!(
                f.service().spectator_connection(g.connection_id()).unwrap(),
                None
            );
        } else {
            assert_eq!(
                f.service().spectator_connection(g.connection_id()),
                Err(GameError::NotFound)
            );
        }
        assert_eq!(count(&f, "game_spectators"), 1);
        assert_eq!(count(&f, "game_spectator_sessions"), 1);
        assert!(f.service().sync_spectator(spectator.token(), None).is_err());
        assert!(
            f.service()
                .release_spectator_output(spectator.token(), &prior_authority)
                .is_err()
        );
        let notices = f.service().begin_spectator_terminal_attempt().unwrap();
        assert_eq!(notices.len(), 1);
        assert!(
            f.service()
                .validate_spectator_notice_delivery(&notices[0])
                .is_ok()
        );
        assert_eq!(count(&f, "game_spectator_sessions"), 1);
        let public: serde_json::Value = serde_json::from_str(&notices[0].notice).unwrap();
        assert_eq!(
            public["state"],
            if ending == "winner" {
                "resolved"
            } else {
                "cancelled"
            }
        );
        assert_eq!(
            count(&f, "game_history"),
            i64::from(["cancel", "winner"].contains(&ending))
        );
        f.service().finish_spectator_terminal_attempt().unwrap();
        assert_eq!(count(&f, "game_spectators"), 0);
        assert_eq!(count(&f, "game_spectator_sessions"), 0);
        assert!(
            f.game
                .query(
                    "SELECT 1 FROM game_view_revisions WHERE view_key=?",
                    &[SqlValue::Text(g.spectator_id().to_string())]
                )
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn spectator_release_intersects_new_grace_at_every_cut_and_preserves_original_cap() {
    for offset in [-1, 0, 1] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let cookie = audience(&f, &code, 40).participant.cookie.unwrap();
        let connected = connect(&f, &cookie, 60);
        let original = proof(&f, &cookie);
        f.service().disconnect_spectator(&connected).unwrap();
        let grace = f.rt.now.get() + SPECTATOR_GRACE_MS;
        assert!(original.valid_until > grace);
        let cut = f
            .service()
            .release_spectator_output(cookie.token(), &original)
            .unwrap();
        assert_eq!(cut.valid_until, grace);
        assert_eq!(cut.checked_at, f.rt.now.get());
        let mut capped = original.clone();
        capped.valid_until = grace - 2;
        assert_eq!(
            f.service()
                .release_spectator_output(cookie.token(), &capped)
                .unwrap()
                .valid_until,
            grace - 2
        );
        f.rt.now.set(grace + offset);
        let result = f
            .service()
            .release_spectator_output(cookie.token(), &original);
        if offset < 0 {
            assert_eq!(result.unwrap().valid_until, grace);
        } else {
            assert!(matches!(result, Err(GameError::Unauthorized)));
            assert_eq!(count(&f, "game_spectator_sessions"), 0);
            f.rt.now.set(grace - 1);
            assert!(matches!(
                f.service()
                    .release_spectator_output(cookie.token(), &original),
                Err(GameError::Storage)
            ));
        }
    }
}

#[test]
fn spectator_replacement_captures_exact_close_and_retires_delivery_atomically() {
    use brews_backend::db::game_delivery::{
        DeliveryBinding, DeliveryService, migrate_game_delivery,
    };
    for fault in [None, Some("close"), Some("delivery"), Some("current")] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        migrate_game_delivery(&f.game).unwrap();
        let cookie = audience(&f, &code, 40).participant.cookie.unwrap();
        let old = connect(&f, &cookie, 60);
        let delivery = DeliveryService::new(&f.game, &f.rt).unwrap();
        let binding = DeliveryBinding {
            connection_id: old.connection_id(),
            session_id: old.session_id(),
            auth_epoch: old.epoch() as u64,
            expires_at: old.expires_at(),
        };
        delivery.register(binding).unwrap();
        let replacement = f
            .service()
            .prepare_spectator_connection(cookie.token(), id(&f.rt, 61).parse().unwrap())
            .unwrap();
        if let Some(fault) = fault {
            let predicate = match fault {
                "close" => "BEFORE INSERT ON game_socket_closures",
                "delivery" => "BEFORE DELETE ON game_delivery_connections",
                "current" => "BEFORE INSERT ON game_current_connections",
                _ => unreachable!("finite fault cases"),
            };
            f.game.execute(&format!("CREATE TRIGGER reject_predecessor_capture {predicate} BEGIN SELECT RAISE(IGNORE); END"), &[]).unwrap();
            assert!(matches!(
                f.service().accept_spectator_connection(&replacement),
                Err(GameError::Storage)
            ));
            assert!(f.service().spectator_connection_view(&old).is_ok());
            assert_eq!(count(&f, "game_socket_closures"), 0);
            assert!(delivery.budget(binding).is_ok());
        } else {
            f.service()
                .accept_spectator_connection(&replacement)
                .unwrap();
            assert!(
                f.service()
                    .spectator_socket_is_fenced(
                        old.connection_id(),
                        old.session_id(),
                        old.spectator_id(),
                        old.epoch()
                    )
                    .unwrap()
            );
            assert!(
                !f.service()
                    .spectator_socket_is_fenced(
                        old.connection_id(),
                        replacement.session_id(),
                        replacement.spectator_id(),
                        old.epoch() + 1
                    )
                    .unwrap()
            );
            assert_eq!(count(&f, "game_delivery_connections"), 0);
            let captured = f.service().pending_recovery_socket_closes().unwrap();
            assert_eq!(captured.len(), 1);
            f.service()
                .retry_recovery_socket_close(&captured[0])
                .unwrap();
            let deferred = f.service().pending_recovery_socket_closes().unwrap();
            f.service().disconnect_spectator(&old).unwrap();
            assert_eq!(
                f.service().pending_recovery_socket_closes().unwrap(),
                deferred
            );
            assert!(f.service().spectator_connection_view(&replacement).is_ok());
            f.service()
                .leave_spectator(f.command(70), cookie.token(), GameState::AwaitingPlayers)
                .unwrap();
            assert!(
                f.service()
                    .spectator_socket_is_fenced(
                        old.connection_id(),
                        old.session_id(),
                        old.spectator_id(),
                        old.epoch()
                    )
                    .unwrap()
            );
        }
    }
}
