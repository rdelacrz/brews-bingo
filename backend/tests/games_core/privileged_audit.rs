//! Slice 8 owner-local attribution, atomic capture and fixed retention regressions.
use super::*;
use brews_contracts::{
    game_audit::GameAuditOperation,
    management::{AuditActor, AuditOutcome},
};
use brews_domain::{games::GameState, ids::GameId};

fn events(f: &Fixture) -> Vec<brews_backend::db::Row> {
    f.game.query("SELECT actor_account_id,target_game_id,operation,outcome,occurred_at,expires_at FROM game_admin_audit ORDER BY occurred_at,audit_id", &[]).unwrap()
}
fn target(f: &Fixture) -> GameId {
    f.service().directory_projection().unwrap().game_id()
}
fn page(f: &Fixture, game: GameId) -> Vec<brews_contracts::game_audit::GameAuditEvent> {
    let ManagementResponse::GameAudit { events, .. } =
        f.service().list_game_audit(game, None, 100).unwrap()
    else {
        panic!("typed page")
    };
    events
}
fn domain_rows(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    brews_backend::db::schema::game_schema::GAME_TABLES
        .into_iter()
        .filter(|t| !matches!(*t, "game_admin_audit" | "game_metadata"))
        .map(|t| {
            f.game
                .query(&format!("SELECT * FROM {t} ORDER BY 1"), &[])
                .unwrap()
        })
        .collect()
}
#[test]
fn privileged_calls_winner_actual_cross_admin_and_exact_replay() {
    let f = Fixture::new();
    let (_, first, _, observed) = f.start_two_player_game();
    let game = target(&f);
    let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    for value in 1..=4 {
        let view = f
            .service()
            .account_view(
                f.auth().authorize_game_account(&f.token).unwrap(),
                &observed,
            )
            .unwrap();
        let command = f.command(80 + value);
        let before = domain_rows(&f);
        let result = f
            .service()
            .call_manual_value(
                f.auth().authorize_game_account(&token).unwrap(),
                command,
                &value.to_string(),
                view.view_revision(),
            )
            .unwrap();
        assert!(matches!(result, GameResponse::CallAccepted { .. }));
        assert_ne!(domain_rows(&f), before);
        let after = domain_rows(&f);
        let count = events(&f).len();
        assert!(matches!(
            f.service()
                .call_manual_value(
                    f.auth().authorize_game_account(&token).unwrap(),
                    command,
                    &value.to_string(),
                    view.view_revision()
                )
                .unwrap(),
            GameResponse::Committed { .. }
        ));
        assert_eq!(domain_rows(&f), after);
        assert_eq!(events(&f).len(), count);
    }
    let GameView::Player { player_id, .. } = f.service().player_view(first.token()).unwrap() else {
        panic!("player")
    };
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    f.service()
        .submit_winner(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(90),
            player_id,
            view.view_revision(),
        )
        .unwrap();
    let rows = events(&f);
    assert_eq!(rows.len(), 5);
    assert!(
        rows.iter()
            .all(|r| r[0] == SqlValue::Text(admin.to_string())
                && r[1] == SqlValue::Text(game.to_string())
                && r[3] == SqlValue::Text("succeeded".into()))
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row[2] == SqlValue::Text("winner".into()))
            .count(),
        1
    );
    let count = events(&f).len();
    f.service()
        .submit_winner(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(90),
            player_id,
            view.view_revision(),
        )
        .unwrap();
    assert_eq!(events(&f).len(), count);
    let snapshot = f
        .service()
        .history(f.auth().authorize_game_account(&token).unwrap(), game)
        .unwrap();
    assert_eq!(snapshot.game_id, game);
    assert_eq!(
        events(&f)
            .iter()
            .filter(|row| row[2] == SqlValue::Text("history_read".into()))
            .count(),
        1
    );
    let count = events(&f).len();
    f.service()
        .history_for_response(f.auth().authorize_game_account(&token).unwrap(), game)
        .unwrap();
    assert_eq!(events(&f).len(), count);
}
#[test]
fn privileged_winner_capture_failure_rolls_back_terminal_history_release_and_receipt() {
    let f = Fixture::new();
    let (_, first, _, observed) = f.start_two_player_game();
    let (_, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    for value in 1..=4 {
        let view = f
            .service()
            .account_view(
                f.auth().authorize_game_account(&f.token).unwrap(),
                &observed,
            )
            .unwrap();
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(80 + value),
                &value.to_string(),
                view.view_revision(),
            )
            .unwrap();
    }
    let GameView::Player { player_id, .. } = f.service().player_view(first.token()).unwrap() else {
        panic!("player")
    };
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let before = domain_rows(&f);
    f.game.execute("CREATE TRIGGER deny_winner_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap();
    assert_eq!(
        f.service()
            .submit_winner(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(90),
                player_id,
                view.view_revision()
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(domain_rows(&f), before);
    assert!(events(&f).is_empty());
    f.game
        .execute("DROP TRIGGER deny_winner_audit", &[])
        .unwrap();
    f.service()
        .submit_winner(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(90),
            player_id,
            view.view_revision(),
        )
        .unwrap();
    let after = domain_rows(&f);
    f.service()
        .submit_winner(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(90),
            player_id,
            view.view_revision(),
        )
        .unwrap();
    assert_eq!(domain_rows(&f), after);
    assert_eq!(events(&f).len(), 1);
}
#[test]
fn privileged_random_call_success_and_exact_replay_attribute_actual_admin_once() {
    let f = Fixture::new();
    let (_, _, _, observed) = f.start_two_player_game();
    let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    assert!(matches!(
        f.service()
            .call_random(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(80),
                view.view_revision()
            )
            .unwrap(),
        GameResponse::CallAccepted { .. }
    ));
    let after = domain_rows(&f);
    assert!(matches!(
        f.service()
            .call_random(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(80),
                view.view_revision()
            )
            .unwrap(),
        GameResponse::Committed { .. }
    ));
    assert_eq!(domain_rows(&f), after);
    let rows = events(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], SqlValue::Text(admin.to_string()));
    assert_eq!(rows[0][2], SqlValue::Text("call_random".into()));
    assert_eq!(rows[0][3], SqlValue::Text("succeeded".into()));
}
#[test]
fn privileged_owner_rejections_are_secret_free_and_host_traffic_is_excluded() {
    let f = Fixture::new();
    super::operator::saved(&f);
    let game = target(&f);
    let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    for (i, value) in ["private-submitted-call", "0"].into_iter().enumerate() {
        assert_eq!(
            f.service()
                .call_manual_value(
                    f.auth().authorize_game_account(&token).unwrap(),
                    f.command(80 + i as u8),
                    value,
                    0
                )
                .err(),
            Some(GameError::Conflict)
        );
    }
    assert_eq!(
        f.service()
            .cancel_game(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(82),
                GameState::New,
                false
            )
            .err(),
        Some(GameError::InvalidInput)
    );
    assert_eq!(events(&f).len(), 3);
    assert!(
        events(&f)
            .iter()
            .all(|row| row[0] == SqlValue::Text(admin.to_string())
                && row[3] == SqlValue::Text("rejected".into()))
    );
    f.service()
        .sync_account(
            f.auth().authorize_game_account(&f.token).unwrap(),
            None,
            &[],
        )
        .unwrap();
    assert_eq!(events(&f).len(), 3);
    let out = page(&f, game);
    let serialized = serde_json::to_string(&out).unwrap();
    assert!(!serialized.contains("private-submitted-call"));
    assert!(!serialized.contains("alias"));
    let last = f
        .game
        .query(
            "SELECT actor_account_id,operation FROM game_admin_audit WHERE operation=?",
            &[SqlValue::Text(
                GameAuditOperation::ListGameAudit.tag().into(),
            )],
        )
        .unwrap();
    assert_eq!(
        last,
        vec![vec![
            SqlValue::Text("developer_cli".into()),
            SqlValue::Text("list_game_audit".into())
        ]]
    );
}
#[test]
fn privileged_call_and_cancel_ignored_or_failed_capture_rolls_back_every_effect_receipt_and_history()
 {
    for operation in ["call", "cancel"] {
        for denial in ["IGNORE", "ABORT,'controlled failure'"] {
            let f = Fixture::new();
            let (_, _, _, observed) = f.start_two_player_game();
            let (_, token) = super::operator::actor(&f, 70, AccountRole::Admin);
            let before = domain_rows(&f);
            let view = f
                .service()
                .account_view(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    &observed,
                )
                .unwrap();
            f.game.execute(&format!("CREATE TRIGGER deny_game_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE({denial}); END"),&[]).unwrap();
            let result = if operation == "call" {
                f.service().call_manual_value(
                    f.auth().authorize_game_account(&token).unwrap(),
                    f.command(81),
                    "1",
                    view.view_revision(),
                )
            } else {
                f.service().cancel_game(
                    f.auth().authorize_game_account(&token).unwrap(),
                    f.command(81),
                    GameState::InProgress,
                    true,
                )
            };
            assert_eq!(result.err(), Some(GameError::Storage));
            assert_eq!(domain_rows(&f), before);
            assert!(events(&f).is_empty());
        }
    }
}
#[test]
fn privileged_action_storage_error_rolls_back_and_captures_failed_using_actual_actor() {
    let f = Fixture::new();
    let (_, _, _, observed) = f.start_two_player_game();
    let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    let before = domain_rows(&f);
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    f.game
        .execute(
            "CREATE TRIGGER deny_call BEFORE INSERT ON game_calls BEGIN SELECT RAISE(IGNORE); END",
            &[],
        )
        .unwrap();
    assert_eq!(
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(81),
                "1",
                view.view_revision()
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(domain_rows(&f), before);
    let rows = events(&f);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], SqlValue::Text(admin.to_string()));
    assert_eq!(rows[0][3], SqlValue::Text("failed".into()));
}
#[test]
fn privileged_context_discovery_and_cli_fail_closed_without_activity_renewal() {
    let f = Fixture::new();
    super::operator::saved(&f);
    let game = target(&f);
    let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    let activity = f
        .game
        .query("SELECT idle_due,last_host_activity FROM game_record", &[])
        .unwrap();
    f.service()
        .sync_account(f.auth().authorize_game_account(&token).unwrap(), None, &[])
        .unwrap();
    let proof = f.auth().authorize_game_account(&token).unwrap();
    f.service()
        .discovery(
            game,
            brews_backend::db::game::DiscoveryViewer::Summary(&proof),
            None,
            true,
            &[],
        )
        .unwrap();
    let before = domain_rows(&f);
    let entries = page(&f, game);
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|e| e.actor == AuditActor::Account(admin))
    );
    assert_eq!(domain_rows(&f), before);
    assert_eq!(
        f.game
            .query("SELECT idle_due,last_host_activity FROM game_record", &[])
            .unwrap(),
        activity
    );
    f.game.execute("CREATE TRIGGER deny_game_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(
        f.service().list_game_audit(game, None, 100).err(),
        Some(GameError::Storage)
    );
    assert_eq!(
        f.service()
            .sync_account(f.auth().authorize_game_account(&token).unwrap(), None, &[])
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(
        f.service()
            .discovery(
                game,
                brews_backend::db::game::DiscoveryViewer::Summary(&proof),
                None,
                true,
                &[]
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(domain_rows(&f), before);
}
#[test]
fn privileged_cli_exact_limits_pages_target_and_retention_equality_reconstruction() {
    let f = Fixture::new();
    super::operator::saved(&f);
    let game = target(&f);
    let (_, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(80),
            GameState::New,
            true,
        )
        .unwrap();
    f.service().cleanup().unwrap();
    let first = page(&f, game);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].outcome, AuditOutcome::Succeeded);
    let expiry = first[0].expires_at;
    let ManagementResponse::GameAudit {
        events,
        next_cursor,
        ..
    } = f.service().list_game_audit(game, None, 1).unwrap()
    else {
        panic!("page")
    };
    assert_eq!(events.len(), 1);
    assert_eq!(next_cursor, Some(first[0].audit_id));
    let ManagementResponse::GameAudit { events, .. } =
        f.service().list_game_audit(game, next_cursor, 1).unwrap()
    else {
        panic!("page")
    };
    assert!(events[0].audit_id > first[0].audit_id);
    for limit in [0, 101, u32::MAX] {
        assert_eq!(
            f.service().list_game_audit(game, None, limit).err(),
            Some(GameError::InvalidInput)
        );
    }
    let wrong = id(&f.rt, 99).parse().unwrap();
    assert_eq!(
        f.service().list_game_audit(wrong, None, 100).err(),
        Some(GameError::Forbidden)
    );
    f.rt.now.set(expiry);
    assert!(page(&f, game).is_empty());
    assert!(f.service().next_deadline().unwrap().is_some());
    f.rt.now.set(expiry - 1);
    assert_eq!(
        f.service().list_game_audit(game, None, 100).err(),
        Some(GameError::Storage)
    );
    f.rt.now.set(expiry);
    f.service().cleanup().unwrap();
    assert!(
        !self::events(&f)
            .iter()
            .any(|row| row[2] == SqlValue::Text("cancel".into()))
    );
}

struct FirstEntropyFailure<'a> {
    inner: &'a TestRuntime,
    draws: std::cell::Cell<u32>,
}
struct FirstEntropyCollision<'a> {
    inner: &'a TestRuntime,
    draws: std::cell::Cell<u32>,
    audit: uuid::Uuid,
}
impl brews_backend::auth::Runtime for FirstEntropyCollision<'_> {
    fn now_ms(&self) -> i64 {
        self.inner.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), brews_backend::auth::AuthError> {
        let draw = self.draws.get();
        self.draws.set(draw + 1);
        if draw == 0 {
            bytes.copy_from_slice(self.audit.as_bytes());
            return Ok(());
        }
        brews_backend::auth::Runtime::fill_random(self.inner, bytes)
    }
}
#[test]
fn ignored_capture_with_identical_existing_entropy_collision_cannot_commit_action() {
    let f = Fixture::new();
    let (_, _, _, observed) = f.start_two_player_game();
    let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    let game = target(&f);
    let audit: uuid::Uuid = id(&f.rt, 200).parse().unwrap();
    f.game
        .execute(
            "INSERT INTO game_admin_audit VALUES(?,?,?,?,?,?,?)",
            &[
                SqlValue::Text(audit.to_string()),
                SqlValue::Text(admin.to_string()),
                SqlValue::Text(game.to_string()),
                SqlValue::Text("call_manual".into()),
                SqlValue::Text("succeeded".into()),
                SqlValue::Integer(f.rt.now.get()),
                SqlValue::Integer(f.rt.now.get() + 7_776_000_000),
            ],
        )
        .unwrap();
    f.game.execute("CREATE TRIGGER ignore_collision BEFORE INSERT ON game_admin_audit WHEN NEW.audit_id=(SELECT audit_id FROM game_admin_audit WHERE operation='call_manual' AND outcome='succeeded') BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap();
    let before = domain_rows(&f);
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let runtime = FirstEntropyCollision {
        inner: &f.rt,
        draws: std::cell::Cell::new(0),
        audit,
    };
    let service = GameService::new(&f.game, &runtime, AuthPolicy::default(), &[9; 32]).unwrap();
    assert_eq!(
        service
            .call_manual_value(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(81),
                "1",
                view.view_revision()
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(domain_rows(&f), before);
    assert_eq!(events(&f).len(), 2);
    assert_eq!(
        events(&f)
            .iter()
            .filter(|r| r[3] == SqlValue::Text("failed".into()))
            .count(),
        1
    );
}
impl brews_backend::auth::Runtime for FirstEntropyFailure<'_> {
    fn now_ms(&self) -> i64 {
        self.inner.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), brews_backend::auth::AuthError> {
        let draw = self.draws.get();
        self.draws.set(draw + 1);
        if draw == 0 {
            return Err(brews_backend::auth::AuthError::Crypto);
        }
        brews_backend::auth::Runtime::fill_random(self.inner, bytes)
    }
}
#[test]
fn privileged_action_and_capture_entropy_errors_rollback_then_durably_record_failed() {
    for random in [false, true] {
        let f = Fixture::new();
        let (_, _, _, observed) = f.start_two_player_game();
        let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
        let before = domain_rows(&f);
        let view = f
            .service()
            .account_view(
                f.auth().authorize_game_account(&f.token).unwrap(),
                &observed,
            )
            .unwrap();
        let runtime = FirstEntropyFailure {
            inner: &f.rt,
            draws: std::cell::Cell::new(0),
        };
        let service = GameService::new(&f.game, &runtime, AuthPolicy::default(), &[9; 32]).unwrap();
        let proof = f.auth().authorize_game_account(&token).unwrap();
        let result = if random {
            service.call_random(proof, f.command(81), view.view_revision())
        } else {
            service.call_manual_value(proof, f.command(81), "1", view.view_revision())
        };
        assert_eq!(
            result.err(),
            Some(if random {
                GameError::RandomUnavailable
            } else {
                GameError::Storage
            })
        );
        assert_eq!(domain_rows(&f), before);
        let rows = events(&f);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], SqlValue::Text(admin.to_string()));
        assert_eq!(rows[0][3], SqlValue::Text("failed".into()));
    }
}
#[test]
fn privileged_creation_ignored_capture_rollback_and_fresh_exact_replay_single_success() {
    let f = Fixture::new();
    f.accounts
        .execute(
            "UPDATE accounts SET role=? WHERE account_id=?",
            &[
                SqlValue::Text(AccountRole::Admin.to_string()),
                SqlValue::Text(f.account.to_string()),
            ],
        )
        .unwrap();
    let config = GameConfiguration::default();
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let before = domain_rows(&f);
    f.game.execute("CREATE TRIGGER deny_creation_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(
        f.service()
            .initialize(
                &work,
                f.auth().authorize_game_account(&f.token).unwrap(),
                &config
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(domain_rows(&f), before);
    f.game
        .execute("DROP TRIGGER deny_creation_audit", &[])
        .unwrap();
    f.service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    let after = domain_rows(&f);
    f.service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    assert_eq!(domain_rows(&f), after);
    assert_eq!(events(&f).len(), 1);
    assert_eq!(events(&f)[0][2], SqlValue::Text("creation".into()));
}
#[test]
fn privileged_start_capture_rolls_back_boards_receipt_then_commits_actual_admin_once() {
    let f = Fixture::new();
    let config = GameConfiguration {
        numeric_upper_bound: 4,
        board_side_length: 2,
        free_cells_enabled: false,
        free_cell_positions: vec![],
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let first = f.player(&code, 20, "First");
    let second = f.player(&code, 21, "Second");
    let observed = [
        id(&f.rt, 40).parse().unwrap(),
        id(&f.rt, 41).parse().unwrap(),
    ];
    for (token, connection) in [(first.token(), observed[0]), (second.token(), observed[1])] {
        let grant = f
            .service()
            .prepare_player_connection(token, connection)
            .unwrap();
        f.service().accept_player_connection(&grant).unwrap();
    }
    let (admin, token) = super::operator::actor(&f, 70, AccountRole::Admin);
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let reservation = f.directory().confirm_reservation(target(&f)).unwrap();
    let before = domain_rows(&f);
    f.game.execute("CREATE TRIGGER deny_start_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(
        f.service()
            .start(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(81),
                view.view_revision(),
                &reservation,
                &observed
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(domain_rows(&f), before);
    f.game
        .execute("DROP TRIGGER deny_start_audit", &[])
        .unwrap();
    f.service()
        .start(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(81),
            view.view_revision(),
            &reservation,
            &observed,
        )
        .unwrap();
    let after = domain_rows(&f);
    f.service()
        .start(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(81),
            view.view_revision(),
            &reservation,
            &observed,
        )
        .unwrap();
    assert_eq!(domain_rows(&f), after);
    assert_eq!(events(&f).len(), 1);
    assert_eq!(events(&f)[0][0], SqlValue::Text(admin.to_string()));
    assert_eq!(events(&f)[0][2], SqlValue::Text("start".into()));
}
