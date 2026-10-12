//! Slice5: real owner SQLite, current Accounts proofs, and durable assignment/removal ordering.
use super::*;
use brews_backend::db::game::TransferPreparation;
use brews_contracts::games::{ConfigurationPatch, GameOutcome, HostTransferInput};
use brews_domain::games::GameState;
fn saved(f: &Fixture) {
    let config = GameConfiguration::default();
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let out = f
        .service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    f.directory().acknowledge_creation(out.ready).unwrap();
}
fn actor(f: &Fixture, n: u8, role: AccountRole) -> (AccountId, String) {
    let account: AccountId = id(&f.rt, n).parse().unwrap();
    f.accounts.execute("INSERT INTO accounts SELECT ?,?,?,status,verifier,credential_epoch,created_at,password_set_at,disabled_at FROM accounts WHERE account_id=?",&[SqlValue::Text(account.to_string()),SqlValue::Text(format!("OperatorPerson{n}")),SqlValue::Text(role.to_string()),SqlValue::Text(f.account.to_string())]).unwrap();
    let token = brews_backend::security::new_token(&f.rt).unwrap();
    let verifier = brews_backend::security::token_digest(&token).unwrap();
    f.accounts.execute("INSERT INTO account_sessions SELECT ?,?,?,scope,credential_epoch,issued_at,expires_at,NULL FROM account_sessions WHERE token_verifier=?",&[SqlValue::Text(id(&f.rt,n+1)),SqlValue::Text(account.to_string()),SqlValue::Blob(verifier.to_vec()),SqlValue::Blob(brews_backend::security::token_digest(&f.token).unwrap().to_vec())]).unwrap();
    (account, token)
}
fn patch(raw: &str) -> ConfigurationPatch {
    ConfigurationPatch::decode_json(raw.as_bytes()).unwrap()
}
fn configuration(f: &Fixture) -> GameConfiguration {
    match f
        .service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap()
    {
        GameView::Host { configuration, .. } => configuration,
        _ => panic!("Expected host snapshot"),
    }
}
fn counters(f: &Fixture) -> Vec<brews_backend::db::Row> {
    f.game.query("SELECT host_id,host_assignment_revision,revision,idle_due,last_host_activity FROM game_record",&[]).unwrap()
}
fn transfer(f: &Fixture, token: &str, target: AccountId, expected: u64, n: u8) -> GameResponse {
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: expected,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(token).unwrap(),
            f.command(n),
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(w) => w,
        _ => panic!("Expected work"),
    };
    let grant = f.directory().prepare_host_transfer(&work).unwrap();
    let (proof, recipient) = f
        .auth()
        .authorize_host_transfer_recipient(token, &work)
        .unwrap();
    let response = f
        .service()
        .commit_host_transfer(proof, recipient, &grant)
        .unwrap();
    let proof = f.service().host_transfer_proof(&work, false).unwrap();
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    response
}
#[test]
fn operator_patch_closed_shape_preserves_omissions_and_requires_atomic_effective_validity() {
    for raw in [
        "[]",
        "null",
        r#"{"expected_revision":0,"numeric_upper_bound":null}"#,
        r#"{"expected_revision":0,"player_capacity":null}"#,
        r#"{"expected_revision":0,"free_cell_positions":null}"#,
        r#"{"expected_revision":0,"unknown":1}"#,
        r#"{"expected_revision":0,"board_side_length":3,"board_side_length":4}"#,
        r#"{"board_side_length":3}"#,
        r#"{"expected_revision":9007199254740992}"#,
    ] {
        assert!(
            ConfigurationPatch::decode_json(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
    let f = Fixture::new();
    saved(&f);
    let before = configuration(&f);
    for raw in [
        r#"{"expected_revision":0,"board_side_length":2}"#,
        r#"{"expected_revision":0,"free_cells_enabled":false}"#,
        r#"{"expected_revision":0,"numeric_upper_bound":1}"#,
        r#"{"expected_revision":0,"free_cell_positions":[{"row":1,"column":1},{"row":1,"column":1}]}"#,
    ] {
        assert_eq!(
            f.service()
                .update_configuration(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(40),
                    &patch(raw)
                )
                .err(),
            Some(GameError::InvalidInput)
        );
        assert_eq!(configuration(&f), before);
        assert_eq!(counters(&f)[0][2], SqlValue::Integer(0));
    }
    f.rt.now.set(f.rt.now.get() + 123);
    let response=f.service().update_configuration(f.auth().authorize_game_account(&f.token).unwrap(),f.command(41),&patch(r#"{"expected_revision":0,"board_side_length":2,"free_cells_enabled":false,"free_cell_positions":[],"numeric_upper_bound":4}"#)).unwrap();
    assert_eq!(configuration(&f).player_capacity, before.player_capacity);
    assert_eq!(configuration(&f).board_side_length, 2);
    assert!(
        matches!(response,GameResponse::ConfigurationUpdated {receipt,..} if matches!(receipt.outcome,GameOutcome::ConfigurationUpdated {changed:true,..}))
    );
    assert_eq!(counters(&f)[0][2], SqlValue::Integer(1));
    assert_eq!(
        counters(&f)[0][3],
        SqlValue::Integer(f.rt.now.get() + 86_400_000)
    );
}
#[test]
fn operator_patch_noop_retry_admin_and_failure_never_renew_and_writes_rollback() {
    let f = Fixture::new();
    saved(&f);
    let (_, admin) = actor(&f, 70, AccountRole::Admin);
    let initial = counters(&f);
    f.rt.now.set(f.rt.now.get() + 100);
    let command = f.command(42);
    let input = patch(r#"{"expected_revision":0,"numeric_upper_bound":75}"#);
    let response = f
        .service()
        .update_configuration(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            &input,
        )
        .unwrap();
    assert_eq!(counters(&f), initial);
    assert!(
        matches!(response,GameResponse::ConfigurationUpdated {receipt,..} if matches!(receipt.outcome,GameOutcome::ConfigurationUpdated {changed:false,..}))
    );
    f.rt.now.set(f.rt.now.get() + 100);
    assert!(matches!(
        f.service()
            .update_configuration(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &input
            )
            .unwrap(),
        GameResponse::Committed { .. }
    ));
    assert_eq!(counters(&f), initial);
    f.service()
        .update_configuration(
            f.auth().authorize_game_account(&admin).unwrap(),
            f.command(43),
            &patch(r#"{"expected_revision":0,"player_capacity":3}"#),
        )
        .unwrap();
    assert_eq!(counters(&f)[0][3], initial[0][3]);
    let prior = configuration(&f);
    f.game.execute("CREATE TRIGGER fail_effective_config BEFORE INSERT ON game_free_cells BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(
        f.service()
            .update_configuration(
                f.auth().authorize_game_account(&admin).unwrap(),
                f.command(44),
                &patch(r#"{"expected_revision":1,"free_cell_positions":[{"row":1,"column":1}]}"#)
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(configuration(&f), prior);
    assert_eq!(
        f.game
            .query(
                "SELECT outcome FROM game_admin_audit ORDER BY occurred_at,audit_id",
                &[]
            )
            .unwrap()
            .len(),
        2
    );
    assert!(
        f.game
            .query("SELECT 1 FROM game_admin_audit WHERE outcome='failed'", &[])
            .unwrap()
            .len()
            == 1
    );
}
#[test]
fn operator_resume_existing_snapshot_no_grant_or_session_extension_and_logical_zero_noop() {
    let f = Fixture::new();
    saved(&f);
    let (_, other) = actor(&f, 70, AccountRole::Host);
    let (_, admin) = actor(&f, 80, AccountRole::Admin);
    let original = counters(&f);
    f.service()
        .resume_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(45),
        )
        .unwrap();
    assert_eq!(counters(&f), original);
    f.rt.now.set(f.rt.now.get() + 50);
    for (n, token) in [(46, &other), (47, &admin)] {
        f.service()
            .resume_game(
                f.auth().authorize_game_account(token).unwrap(),
                f.command(n),
            )
            .unwrap();
        assert_eq!(counters(&f), original);
    }
    let command = f.command(48);
    let response = f
        .service()
        .resume_game(f.auth().authorize_game_account(&f.token).unwrap(), command)
        .unwrap();
    let updated = counters(&f);
    assert_eq!(
        updated[0][3],
        SqlValue::Integer(f.rt.now.get() + 86_400_000)
    );
    assert_eq!(updated[0][2], SqlValue::Integer(1));
    f.rt.now.set(f.rt.now.get() + 20);
    assert!(matches!(
        f.service()
            .resume_game(f.auth().authorize_game_account(&f.token).unwrap(), command)
            .unwrap(),
        GameResponse::Committed { .. }
    ));
    assert_eq!(counters(&f), updated);
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_connection_grants", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    let final_authority = f.auth().authorize_game_account(&f.token).unwrap();
    f.rt.now.set(match updated[0][3] {
        SqlValue::Integer(v) => v,
        _ => panic!("due"),
    });
    assert!(
        f.service()
            .authorize_operator_output(&final_authority, &response)
            .is_err()
    );
    f.rt.now.set(f.rt.now.get() - 1);
    assert_eq!(f.service().cleanup().err(), Some(GameError::Storage));
}
#[test]
fn operator_transfer_directory_guard_wins_removal_both_sides_and_preserves_reservation() {
    let f = Fixture::new();
    saved(&f);
    let (target, token) = actor(&f, 70, AccountRole::Host);
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let command = f.command(50);
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(w) => w,
        _ => panic!("work"),
    };
    let grant = f.directory().prepare_host_transfer(&work).unwrap();
    for (n, id) in [(51, target), (52, f.account)] {
        assert_eq!(
            f.directory()
                .acquire_removal(id_of_operation(&f, n), id)
                .err(),
            Some(brews_backend::directory::DirectoryError::HostedGame)
        );
    }
    let (actor, recipient) = f
        .auth()
        .authorize_host_transfer_recipient(&f.token, &work)
        .unwrap();
    f.rt.now.set(f.rt.now.get() + 100);
    let result = f
        .service()
        .commit_host_transfer(actor, recipient, &grant)
        .unwrap();
    let deadline = counters(&f)[0][3].clone();
    assert_eq!(counters(&f)[0][0], SqlValue::Text(target.to_string()));
    assert_eq!(
        f.service()
            .prepare_lobby(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(53),
                1
            )
            .err(),
        Some(GameError::Forbidden)
    );
    assert!(
        f.service()
            .authorize_operator_output(&f.auth().authorize_game_account(&f.token).unwrap(), &result)
            .is_ok()
    );
    let proof = f.service().host_transfer_proof(&work, false).unwrap();
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    assert_eq!(
        f.directory()
            .acquire_removal(id_of_operation(&f, 54), target)
            .err(),
        Some(brews_backend::directory::DirectoryError::HostedGame)
    );
    assert!(
        f.directory()
            .acquire_removal(id_of_operation(&f, 55), f.account)
            .is_ok()
    );
    assert_eq!(
        f.directory
            .query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(work.game_id().to_string())]]
    );
    f.rt.now.set(f.rt.now.get() + 10);
    assert!(matches!(
        f.service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &input
            )
            .unwrap(),
        TransferPreparation::Committed(_)
    ));
    assert_eq!(counters(&f)[0][3], deadline);
    assert_eq!(counters(&f)[0][1], SqlValue::Integer(1));
    let current = f
        .service()
        .resume_game(
            f.auth().authorize_game_account(&token).unwrap(),
            f.command(56),
        )
        .unwrap();
    assert!(matches!(
        current,
        GameResponse::Resumed {
            can_mutate: true,
            ..
        }
    ));
}
fn id_of_operation(f: &Fixture, n: u8) -> brews_domain::ids::OperationId {
    id(&f.rt, n).parse().unwrap()
}
#[test]
fn operator_transfer_gate_first_rejects_and_proved_abort_releases_only_assignment_intent() {
    let f = Fixture::new();
    saved(&f);
    let (target, _) = actor(&f, 70, AccountRole::Host);
    let removal = f
        .directory()
        .acquire_removal(id_of_operation(&f, 51), target)
        .unwrap();
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let command = f.command(50);
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(w) => w,
        _ => panic!("work"),
    };
    assert_eq!(
        f.directory().prepare_host_transfer(&work).err(),
        Some(brews_backend::directory::DirectoryError::AssignmentBlocked)
    );
    let proof = f.service().host_transfer_proof(&work, true).unwrap();
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    assert!(f.directory().prepare_host_transfer(&work).is_err());
    f.directory()
        .release_removal(removal.operation_id, target)
        .unwrap();
    assert_eq!(counters(&f)[0][0], SqlValue::Text(f.account.to_string()));
    assert!(f.directory().confirm_reservation(work.game_id()).is_ok());
}
#[test]
fn operator_transfer_eligibility_revision_replay_and_admin_attribution() {
    let f = Fixture::new();
    saved(&f);
    let (target, _) = actor(&f, 70, AccountRole::Host);
    let (admin, admin_token) = actor(&f, 80, AccountRole::Admin);
    for status in ["pending_enrollment", "reset_required"] {
        // Lifecycle constraints require removal of password metadata for pending enrollment.
        if status == "pending_enrollment" {
            f.accounts.execute("UPDATE accounts SET status=?,verifier=NULL,password_set_at=NULL WHERE account_id=?",&[SqlValue::Text(status.into()),SqlValue::Text(target.to_string())]).unwrap();
        } else {
            f.accounts.execute("UPDATE accounts SET status=?,verifier=(SELECT verifier FROM accounts WHERE account_id=?) WHERE account_id=?",&[SqlValue::Text(status.into()),SqlValue::Text(f.account.to_string()),SqlValue::Text(target.to_string())]).unwrap();
        }
        assert!(
            f.auth()
                .authorize_host_recipient(&admin_token, target)
                .is_err()
        );
    }
    f.accounts.execute("UPDATE accounts SET status=?,verifier=(SELECT verifier FROM accounts WHERE account_id=?),password_set_at=created_at WHERE account_id=?",&[SqlValue::Text("verified".into()),SqlValue::Text(f.account.to_string()),SqlValue::Text(target.to_string())]).unwrap();
    assert!(
        f.auth()
            .authorize_host_recipient(&admin_token, admin)
            .is_err()
    );
    let old = counters(&f);
    f.rt.now.set(f.rt.now.get() + 100);
    transfer(&f, &admin_token, target, 0, 58);
    assert_eq!(counters(&f)[0][3], old[0][3]);
    let audits = f
        .game
        .query(
            "SELECT actor_account_id,target_game_id,operation,outcome FROM game_admin_audit",
            &[],
        )
        .unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0][0], SqlValue::Text(admin.to_string()));
    assert_eq!(audits[0][2], SqlValue::Text("host_transfer".into()));
    let stale = HostTransferInput {
        target_account_id: f.account,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    assert_eq!(
        f.service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&admin_token).unwrap(),
                f.command(59),
                &stale
            )
            .err(),
        Some(GameError::StaleRevision)
    );
    let changed = HostTransferInput {
        target_account_id: f.account,
        confirmed: true,
        expected_host_assignment_revision: 1,
    };
    assert_eq!(
        f.service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&admin_token).unwrap(),
                f.command(58),
                &changed
            )
            .err(),
        Some(GameError::Conflict)
    );
}
#[test]
fn operator_true_two_successor_transfers_and_stale_directory_projection_are_fenced() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let (b, btoken) = actor(&f, 70, AccountRole::Host);
    let (c, _) = actor(&f, 80, AccountRole::Host);
    let old = f.service().directory_projection().unwrap();
    transfer(&f, &f.token, b, 0, 60);
    let middle = f.service().directory_projection().unwrap();
    transfer(&f, &btoken, c, 1, 61);
    for projection in [old, middle] {
        let _ = f.directory().publish_game(projection);
    }
    assert_eq!(
        f.directory
            .query("SELECT designated_host_id FROM directory_game_index", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(c.to_string())]]
    );
    assert_eq!(
        f.directory().lookup_game_code(&code).unwrap(),
        Some(f.service().directory_projection().unwrap().game_id())
    );
    assert_eq!(counters(&f)[0][1], SqlValue::Integer(2));
}
#[test]
fn operator_audit_ignored_insert_rolls_back_action_and_exact_ninety_day_cleanup_fences_rollback() {
    let f = Fixture::new();
    saved(&f);
    let (_, admin) = actor(&f, 70, AccountRole::Admin);
    f.game.execute("CREATE TRIGGER fail_privileged_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    let before = counters(&f);
    assert_eq!(
        f.service()
            .update_configuration(
                f.auth().authorize_game_account(&admin).unwrap(),
                f.command(62),
                &patch(r#"{"expected_revision":0,"player_capacity":3}"#)
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(counters(&f), before);
    assert_eq!(configuration(&f).player_capacity, 20);
    f.game
        .execute("DROP TRIGGER fail_privileged_audit", &[])
        .unwrap();
    f.service()
        .update_configuration(
            f.auth().authorize_game_account(&admin).unwrap(),
            f.command(63),
            &patch(r#"{"expected_revision":0,"player_capacity":3}"#),
        )
        .unwrap();
    let audit = f
        .game
        .query("SELECT occurred_at,expires_at FROM game_admin_audit", &[])
        .unwrap();
    let occurred = match audit[0][0] {
        SqlValue::Integer(v) => v,
        _ => panic!("occurred"),
    };
    let expiry = match audit[0][1] {
        SqlValue::Integer(v) => v,
        _ => panic!("expiry"),
    };
    assert_eq!(expiry - occurred, 7_776_000_000);
    f.rt.now.set(expiry - 1);
    f.service().cleanup().unwrap();
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_admin_audit", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    f.rt.now.set(expiry);
    f.service().cleanup().unwrap();
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_admin_audit", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    f.rt.now.set(expiry - 1);
    assert_eq!(f.service().cleanup().err(), Some(GameError::Storage));
}

#[test]
fn operator_transfer_all_nonterminal_states_preserves_existing_participants_calls_and_boards() {
    for state in ["new", "awaiting_players", "in_progress"] {
        let f = Fixture::new();
        match state {
            "new" => saved(&f),
            "awaiting_players" => {
                f.open(&GameConfiguration::default());
            }
            _ => {
                f.start_two_player_game();
            }
        }
        for work in f.service().pending_work(100).unwrap() {
            if work.kind() == brews_backend::game::WorkKind::StartProjection {
                let ack = f
                    .directory()
                    .publish_game(f.service().directory_projection().unwrap())
                    .unwrap();
                f.service().acknowledge_projection(&work, ack).unwrap();
            }
        }
        let (target, token) = actor(&f, 70, AccountRole::Host);
        let tables = [
            "game_players",
            "game_spectators",
            "game_boards",
            "game_board_cells",
            "game_completed_lines",
            "game_calls",
            "game_sessions",
            "game_spectator_sessions",
        ];
        let before = tables.map(|table| {
            f.game
                .query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
                .unwrap()
        });
        let old = f.service().directory_projection().unwrap();
        let response = transfer(&f, &f.token, target, 0, 72);
        assert!(matches!(response, GameResponse::HostTransferred { .. }));
        for (table, rows) in tables.iter().zip(before) {
            assert_eq!(
                f.game
                    .query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
                    .unwrap(),
                rows,
                "{state}/{table}"
            );
        }
        let current = f.service().directory_projection().unwrap();
        assert_eq!(current.game_code(), old.game_code());
        assert_eq!(current.state(), old.state());
        let output = f
            .service()
            .resume_game(
                f.auth().authorize_game_account(&token).unwrap(),
                f.command(73),
            )
            .unwrap();
        assert!(matches!(
            output,
            GameResponse::Resumed {
                can_mutate: true,
                ..
            }
        ));
        if state == "in_progress" {
            assert_eq!(
                f.service()
                    .call_random(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        f.command(74),
                        0
                    )
                    .err(),
                Some(GameError::Forbidden)
            );
        }
    }
}
#[test]
fn operator_prepared_transfer_terminal_race_requires_proved_abort_before_release_and_survives_reopen()
 {
    let f = Fixture::new();
    saved(&f);
    let (target, _) = actor(&f, 70, AccountRole::Host);
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(75),
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(work) => work,
        _ => panic!("prepared"),
    };
    f.directory().prepare_host_transfer(&work).unwrap();
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(76),
            brews_domain::games::GameState::New,
            true,
        )
        .unwrap();
    let terminal = f.service().cleanup().unwrap().unwrap();
    assert_eq!(
        f.directory().release_game(terminal.clone()).err(),
        Some(brews_backend::directory::DirectoryError::Busy)
    );
    // Reconstruct services from retained SQLite owner state; no cached authority.
    brews_backend::db::game::migrate_game(&f.game).unwrap();
    brews_backend::db::directory::migrate_directory(&f.directory).unwrap();
    let proof = f.service().host_transfer_proof(&work, true).unwrap();
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    f.directory().release_game(terminal).unwrap();
    assert_eq!(
        f.directory
            .query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    assert!(
        f.directory()
            .acquire_removal(id_of_operation(&f, 77), target)
            .is_ok()
    );
}
#[test]
fn operator_transfer_expiry_exact_due_wins_before_commit_and_current_recipient_is_rechecked() {
    let f = Fixture::new();
    saved(&f);
    let (target, _) = actor(&f, 70, AccountRole::Host);
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(78),
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(work) => work,
        _ => panic!("prepared"),
    };
    let grant = f.directory().prepare_host_transfer(&work).unwrap();
    f.accounts
        .execute(
            "UPDATE accounts SET status=?,credential_epoch=credential_epoch+1 WHERE account_id=?",
            &[
                SqlValue::Text("reset_required".into()),
                SqlValue::Text(target.to_string()),
            ],
        )
        .unwrap();
    assert!(
        f.auth()
            .authorize_host_transfer_recipient(&f.token, &work)
            .is_err()
    );
    f.accounts
        .execute(
            "UPDATE accounts SET status=? WHERE account_id=?",
            &[
                SqlValue::Text("verified".into()),
                SqlValue::Text(target.to_string()),
            ],
        )
        .unwrap();
    let (actor, recipient) = f
        .auth()
        .authorize_host_transfer_recipient(&f.token, &work)
        .unwrap();
    let due = match counters(&f)[0][3] {
        SqlValue::Integer(t) => t,
        _ => panic!("due"),
    };
    f.rt.now.set(due);
    assert!(
        f.service()
            .commit_host_transfer(actor, recipient, &grant)
            .is_err()
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_history", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_configuration", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    f.rt.now.set(due - 1);
    assert_eq!(
        f.service().host_transfer_proof(&work, true).err(),
        Some(GameError::Storage)
    );
}
#[test]
fn operator_transfer_checked_assignment_and_game_counter_overflow_are_atomic() {
    for column in ["host_assignment_revision", "revision"] {
        let f = Fixture::new();
        saved(&f);
        let (target, _) = actor(&f, 70, AccountRole::Host);
        f.game
            .execute(
                &format!("UPDATE game_record SET {column}=?"),
                &[SqlValue::Integer(9_007_199_254_740_991)],
            )
            .unwrap();
        let input = HostTransferInput {
            target_account_id: target,
            confirmed: true,
            expected_host_assignment_revision: if column == "host_assignment_revision" {
                9_007_199_254_740_991
            } else {
                0
            },
        };
        let before = counters(&f);
        assert_eq!(
            f.service()
                .prepare_host_transfer(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(79),
                    &input
                )
                .err(),
            Some(GameError::Storage)
        );
        assert_eq!(counters(&f), before);
        assert!(f.service().host_transfer_work().unwrap().is_empty());
    }
}

#[test]
fn operator_lost_commit_ack_survives_directory_tombstone_cleanup() {
    let f = Fixture::new();
    f.start_two_player_game();
    for work in f.service().pending_work(100).unwrap() {
        if work.kind() == brews_backend::game::WorkKind::StartProjection {
            let ack = f
                .directory()
                .publish_game(f.service().directory_projection().unwrap())
                .unwrap();
            f.service().acknowledge_projection(&work, ack).unwrap();
        }
    }
    let (target, target_token) = actor(&f, 70, AccountRole::Host);
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(72),
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(w) => w,
        _ => panic!("expected prepared transfer"),
    };
    let grant = f.directory().prepare_host_transfer(&work).unwrap();
    let (authority, recipient) = f
        .auth()
        .authorize_host_transfer_recipient(&f.token, &work)
        .unwrap();
    f.service()
        .commit_host_transfer(authority, recipient, &grant)
        .unwrap();
    let proof = f.service().host_transfer_proof(&work, false).unwrap();
    // Directory committed; simulate transport loss before Game receives its ACK.
    let _lost_ack = f.directory().settle_host_transfer(&proof).unwrap();
    let required = f.directory().due_host_transfers().unwrap();
    assert!(required.is_empty());
    assert_eq!(f.service().host_transfer_work().unwrap().len(), 1);
    let before = f
        .directory
        .query(
            "SELECT phase,next_attempt_at FROM directory_host_transfers",
            &[],
        )
        .unwrap();
    let next = match before[0][1] {
        SqlValue::Integer(n) => n,
        _ => panic!("deadline"),
    };
    f.rt.now.set(next + 86_400_000);
    f.directory().cleanup().unwrap();
    let due = f.directory().due_host_transfers().unwrap();
    assert_eq!(due.len(), 1);
    f.directory().retry_host_transfer(&due[0]).unwrap();
    assert!(f.directory().due_host_transfers().unwrap().is_empty());
    println!(
        "Directory tombstones after cleanup: {:?}",
        f.directory
            .query("SELECT phase FROM directory_host_transfers", &[])
            .unwrap()
    );
    let proof = f.service().host_transfer_proof(&work, false).unwrap();
    let settled = f.directory().settle_host_transfer(&proof);
    println!("Delayed genuine committed proof retry: {:?}", settled);
    assert!(
        settled.is_ok(),
        "lost ACK recovery must survive Directory cleanup; Game still has required committed work"
    );
    let completed = f.service().host_transfer_work().unwrap()[0].clone();
    f.service()
        .acknowledge_host_transfer(&proof, settled.unwrap())
        .unwrap();
    f.directory()
        .confirm_host_transfer_completion(&completed)
        .unwrap();
    assert_eq!(
        f.directory().discovery_route().unwrap().unwrap().game_id,
        work.game_id()
    );
    // A new session for B is sufficient; A's expired credential is never needed.
    f.accounts
        .execute(
            "UPDATE account_sessions SET issued_at=?,expires_at=? WHERE account_id=?",
            &[
                SqlValue::Integer(f.rt.now.get()),
                SqlValue::Integer(f.rt.now.get() + 86_400_000),
                SqlValue::Text(target.to_string()),
            ],
        )
        .unwrap();
    let (next_host, _) = actor(&f, 80, AccountRole::Host);
    transfer(&f, &target_token, next_host, 1, 84);
    assert!(
        f.service()
            .acknowledge_host_transfer(&proof, f.directory().settle_host_transfer(&proof).unwrap())
            .is_err()
    );
    assert_eq!(
        f.service()
            .directory_projection()
            .unwrap()
            .designated_host_id(),
        next_host
    );
    f.rt.now.set(f.rt.now.get() + 86_400_000);
    f.directory().cleanup().unwrap();
    assert!(
        f.directory
            .query(
                "SELECT 1 FROM directory_host_transfers WHERE operation_id=?",
                &[SqlValue::Text(work.operation_id().to_string())]
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.directory().discovery_route().unwrap().unwrap().game_id,
        work.game_id()
    );
}

#[test]
fn operator_transfer_and_lobby_intents_exclude_each_other_and_terminal_release_finishes() {
    for lobby_first in [false, true] {
        let f = Fixture::new();
        let config = GameConfiguration::default();
        let creation = f
            .directory()
            .claim_game(
                f.account,
                f.command(10),
                creation_fingerprint(&config).unwrap(),
            )
            .unwrap();
        let initialized = f
            .service()
            .initialize(
                &creation,
                f.auth().authorize_game_account(&f.token).unwrap(),
                &config,
            )
            .unwrap();
        let creation_ack = f
            .directory()
            .acknowledge_creation(initialized.ready)
            .unwrap();
        let (target, _) = actor(&f, 70, AccountRole::Host);
        let input = HostTransferInput {
            target_account_id: target,
            confirmed: true,
            expected_host_assignment_revision: 0,
        };
        let (_, admin) = actor(&f, 80, AccountRole::Admin);
        if lobby_first {
            let lobby = match f
                .service()
                .prepare_lobby(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(73),
                    0,
                )
                .unwrap()
            {
                brews_backend::game::LobbyPreparation::Ready(work) => work,
                _ => panic!("Expected lobby intent"),
            };
            let code = f.directory().allocate_game_code(creation_ack).unwrap();
            assert_eq!(
                f.service()
                    .prepare_host_transfer(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        f.command(72),
                        &input
                    )
                    .err(),
                Some(GameError::Conflict)
            );
            f.service()
                .cancel_game(
                    f.auth().authorize_game_account(&admin).unwrap(),
                    f.command(74),
                    GameState::New,
                    true,
                )
                .unwrap();
            let terminal = f.service().cleanup().unwrap().unwrap();
            f.directory().release_game(terminal).unwrap();
            assert!(
                f.service()
                    .commit_lobby(
                        &lobby,
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        &code
                    )
                    .is_err()
            );
        } else {
            let work = match f
                .service()
                .prepare_host_transfer(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(72),
                    &input,
                )
                .unwrap()
            {
                TransferPreparation::Prepared(work) => work,
                _ => panic!("Expected transfer intent"),
            };
            let grant = f.directory().prepare_host_transfer(&work).unwrap();
            assert_eq!(
                f.service()
                    .prepare_lobby(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        f.command(73),
                        0
                    )
                    .err(),
                Some(GameError::Conflict)
            );
            assert_eq!(
                f.directory().allocate_game_code(creation_ack).err(),
                Some(brews_backend::directory::DirectoryError::Busy)
            );
            let (authority, recipient) = f
                .auth()
                .authorize_host_transfer_recipient(&f.token, &work)
                .unwrap();
            f.service()
                .commit_host_transfer(authority, recipient, &grant)
                .unwrap();
            f.service()
                .cancel_game(
                    f.auth().authorize_game_account(&admin).unwrap(),
                    f.command(74),
                    GameState::New,
                    true,
                )
                .unwrap();
            let terminal = f.service().cleanup().unwrap().unwrap();
            let proof = f.service().host_transfer_proof(&work, true).unwrap();
            let ack = f.directory().settle_host_transfer(&proof).unwrap();
            f.service().acknowledge_host_transfer(&proof, ack).unwrap();
            f.directory().release_game(terminal).unwrap();
        }
        assert_eq!(f.directory().discovery_route().unwrap(), None);
        let next = f
            .directory()
            .claim_game(
                f.account,
                f.command(75),
                creation_fingerprint(&config).unwrap(),
            )
            .unwrap();
        assert!(f.directory().allocate_game_code(creation_ack).is_err());
        assert_eq!(
            f.directory().discovery_route().unwrap().unwrap().game_id,
            next.game_id()
        );
    }
}

#[test]
fn operator_genuine_legacy_hidden_code_reconciles_before_publication_or_terminal_release() {
    for terminal in [false, true] {
        let f = Fixture::new();
        let config = GameConfiguration::default();
        let creation = f
            .directory()
            .claim_game(
                f.account,
                f.command(10),
                creation_fingerprint(&config).unwrap(),
            )
            .unwrap();
        let initialized = f
            .service()
            .initialize(
                &creation,
                f.auth().authorize_game_account(&f.token).unwrap(),
                &config,
            )
            .unwrap();
        let creation_ack = f
            .directory()
            .acknowledge_creation(initialized.ready)
            .unwrap();
        // Genuine old Directory delivery precedes the new local/Directory guard.
        let code = f.directory().allocate_game_code(creation_ack).unwrap();
        let (target, token) = actor(&f, 70, AccountRole::Host);
        let input = HostTransferInput {
            target_account_id: target,
            confirmed: true,
            expected_host_assignment_revision: 0,
        };
        let work = match f
            .service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(72),
                &input,
            )
            .unwrap()
        {
            TransferPreparation::Prepared(work) => work,
            _ => panic!("Expected transfer"),
        };
        let grant = f.directory().prepare_host_transfer(&work).unwrap();
        let (authority, recipient) = f
            .auth()
            .authorize_host_transfer_recipient(&f.token, &work)
            .unwrap();
        f.service()
            .commit_host_transfer(authority, recipient, &grant)
            .unwrap();
        let terminal_proof = if terminal {
            let (_, admin) = actor(&f, 80, AccountRole::Admin);
            f.service()
                .cancel_game(
                    f.auth().authorize_game_account(&admin).unwrap(),
                    f.command(74),
                    GameState::New,
                    true,
                )
                .unwrap();
            f.service().cleanup().unwrap()
        } else {
            None
        };
        let proof = f.service().host_transfer_proof(&work, false).unwrap();
        let ack = f.directory().settle_host_transfer(&proof).unwrap();
        f.service().acknowledge_host_transfer(&proof, ack).unwrap();
        assert_eq!(
            f.directory()
                .game_code(creation.game_id())
                .unwrap()
                .as_ref(),
            Some(code.game_code())
        );
        if let Some(terminal_proof) = terminal_proof {
            f.directory().release_game(terminal_proof).unwrap();
            assert_eq!(f.directory().discovery_route().unwrap(), None);
        } else {
            let lobby = match f
                .service()
                .prepare_lobby(
                    f.auth().authorize_game_account(&token).unwrap(),
                    f.command(75),
                    1,
                )
                .unwrap()
            {
                brews_backend::game::LobbyPreparation::Ready(work) => work,
                _ => panic!("Expected lobby"),
            };
            let same_code = f.directory().allocate_game_code(creation_ack).unwrap();
            assert_eq!(same_code.game_code(), code.game_code());
            f.service()
                .commit_lobby(
                    &lobby,
                    f.auth().authorize_game_account(&token).unwrap(),
                    &same_code,
                )
                .unwrap();
            let published = f
                .directory()
                .publish_game(f.service().directory_projection().unwrap())
                .unwrap();
            let current = f
                .service()
                .pending_work(100)
                .unwrap()
                .into_iter()
                .find(|w| w.operation_id() == lobby.operation_id())
                .unwrap();
            f.service()
                .acknowledge_projection(&current, published)
                .unwrap();
        }
    }
}

#[test]
fn operator_restored_competing_genuine_intents_are_fenced_again_at_commit() {
    let f = Fixture::new();
    let config = GameConfiguration::default();
    let creation = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let initialized = f
        .service()
        .initialize(
            &creation,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    let creation_ack = f
        .directory()
        .acknowledge_creation(initialized.ready)
        .unwrap();
    let lobby = match f
        .service()
        .prepare_lobby(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(73),
            0,
        )
        .unwrap()
    {
        brews_backend::game::LobbyPreparation::Ready(work) => work,
        _ => panic!("Expected genuine lobby intent"),
    };
    let code = f.directory().allocate_game_code(creation_ack).unwrap();
    let original_lobby = f
        .game
        .query("SELECT * FROM game_pending_work", &[])
        .unwrap();
    // Restore the valid overlapping intents an older owner could persist. Both
    // work rows and the Directory grants come from real service preparations.
    f.game
        .execute("DELETE FROM game_pending_work", &[])
        .unwrap();
    let (target, _) = actor(&f, 70, AccountRole::Host);
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(72),
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(work) => work,
        _ => panic!("Expected genuine transfer intent"),
    };
    let grant = f.directory().prepare_host_transfer(&work).unwrap();
    let row = &original_lobby[0];
    let placeholders = std::iter::repeat_n("?", row.len())
        .collect::<Vec<_>>()
        .join(",");
    f.game
        .execute(
            &format!("INSERT INTO game_pending_work VALUES({placeholders})"),
            row,
        )
        .unwrap();
    assert_eq!(
        f.service()
            .prepare_lobby(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(73),
                0
            )
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(
        f.service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(72),
                &input
            )
            .err(),
        Some(GameError::Conflict)
    );
    let before = counters(&f);
    let (authority, recipient) = f
        .auth()
        .authorize_host_transfer_recipient(&f.token, &work)
        .unwrap();
    assert_eq!(
        f.service()
            .commit_host_transfer(authority, recipient, &grant)
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(
        f.service()
            .commit_lobby(
                &lobby,
                f.auth().authorize_game_account(&f.token).unwrap(),
                &code
            )
            .err(),
        Some(GameError::StaleRevision)
    );
    assert_eq!(counters(&f), before);
    let (_, admin) = actor(&f, 80, AccountRole::Admin);
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&admin).unwrap(),
            f.command(74),
            GameState::New,
            true,
        )
        .unwrap();
    let terminal = f.service().cleanup().unwrap().unwrap();
    let proof = f.service().host_transfer_proof(&work, true).unwrap();
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    f.directory().release_game(terminal).unwrap();
    assert!(f.directory().discovery_route().unwrap().is_none());
}

#[test]
fn operator_undelivered_prepare_retires_after_admission_floor_without_reauthorizing() {
    let f = Fixture::new();
    f.start_two_player_game();
    for work in f.service().pending_work(100).unwrap() {
        if work.kind() == brews_backend::game::WorkKind::StartProjection {
            let ack = f
                .directory()
                .publish_game(f.service().directory_projection().unwrap())
                .unwrap();
            f.service().acknowledge_projection(&work, ack).unwrap();
        }
    }
    let (target, _) = actor(&f, 70, AccountRole::Host);
    let input = HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(72),
            &input,
        )
        .unwrap()
    {
        TransferPreparation::Prepared(work) => work,
        _ => panic!("Expected genuine prepared work"),
    };
    assert!(
        f.directory
            .query("SELECT * FROM directory_host_transfers", &[])
            .unwrap()
            .is_empty()
    );
    f.rt.now.set(f.rt.now.get() + 86_400_001);
    f.directory().cleanup().unwrap();
    let before = f
        .directory
        .query("SELECT * FROM directory_game_index", &[])
        .unwrap();
    let proof = f.service().host_transfer_proof(&work, true).unwrap();
    let completion = f.service().host_transfer_work().unwrap().pop().unwrap();
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    assert!(f.service().host_transfer_work().unwrap().is_empty());
    assert_eq!(
        f.directory
            .query("SELECT * FROM directory_game_index", &[])
            .unwrap(),
        before
    );
    assert_eq!(
        f.directory().discovery_route().unwrap().unwrap().game_id,
        work.game_id()
    );
    assert!(f.directory().prepare_host_transfer(&work).is_err());
    f.directory()
        .confirm_host_transfer_completion(&completion)
        .unwrap();
    f.rt.now.set(f.rt.now.get() + 86_400_001);
    f.directory().cleanup().unwrap();
    assert!(
        f.directory
            .query("SELECT * FROM directory_host_transfers", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.directory().prepare_host_transfer(&work).err(),
        Some(brews_backend::directory::DirectoryError::StaleOperation)
    );
    f.accounts
        .execute(
            "UPDATE account_sessions SET issued_at=?,expires_at=? WHERE account_id=?",
            &[
                SqlValue::Integer(f.rt.now.get()),
                SqlValue::Integer(f.rt.now.get() + 86_400_000),
                SqlValue::Text(f.account.to_string()),
            ],
        )
        .unwrap();
    assert!(matches!(
        f.service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(73),
                &input
            )
            .unwrap(),
        TransferPreparation::Prepared(_)
    ));
}

#[test]
fn operator_expired_absent_abort_requires_durable_evidence_before_ack() {
    for fault in ["IGNORE", "ABORT,'test-only expired abort persistence'"] {
        let f = Fixture::new();
        saved(&f);
        let (target, _) = actor(&f, 70, AccountRole::Host);
        let input = HostTransferInput {
            target_account_id: target,
            confirmed: true,
            expected_host_assignment_revision: 0,
        };
        let work = match f
            .service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(72),
                &input,
            )
            .unwrap()
        {
            TransferPreparation::Prepared(work) => work,
            _ => panic!("Expected genuine prepared work"),
        };
        f.rt.now.set(f.rt.now.get() + 86_400_001);
        f.directory().cleanup().unwrap();
        let proof = f.service().host_transfer_proof(&work, true).unwrap();
        f.directory.execute(&format!("CREATE TRIGGER expired_abort_fault BEFORE INSERT ON directory_host_transfers BEGIN SELECT RAISE({fault}); END"), &[]).unwrap();
        assert_eq!(
            f.directory().settle_host_transfer(&proof).err(),
            Some(brews_backend::directory::DirectoryError::Storage)
        );
        assert!(
            f.directory
                .query("SELECT * FROM directory_host_transfers", &[])
                .unwrap()
                .is_empty()
        );
        assert_eq!(f.service().host_transfer_work().unwrap().len(), 1);
        f.directory
            .execute("DROP TRIGGER expired_abort_fault", &[])
            .unwrap();
        let ack = f.directory().settle_host_transfer(&proof).unwrap();
        f.service().acknowledge_host_transfer(&proof, ack).unwrap();
        assert!(f.service().host_transfer_work().unwrap().is_empty());
    }
}
