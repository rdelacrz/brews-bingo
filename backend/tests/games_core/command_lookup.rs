//! G4 exercises owner SQL and current Accounts/participant sessions.
use super::*;
use brews_backend::api::{ReadRole, ReadSelection};
use brews_contracts::games::{
    GameOutcome,
    commands::*,
    reads::{ReadKind, ReadProjection},
};
use brews_domain::games::GameState;
fn lookup(
    f: &Fixture,
    command: CommandId,
    role: ReadRole,
    token: &str,
) -> Result<CommandResult, GameError> {
    let row = f
        .game
        .query("SELECT game_id FROM game_record", &[])
        .unwrap();
    let game_id = match &row[0][0] {
        SqlValue::Text(s) => s.parse().unwrap(),
        _ => panic!("game id"),
    };
    let authority = if role == ReadRole::Account {
        Some(
            f.auth()
                .authorize_game_account(token)
                .map_err(|e| match e {
                    brews_backend::auth::AuthError::Unauthorized => GameError::Unauthorized,
                    _ => GameError::Storage,
                })?,
        )
    } else {
        None
    };
    match f.service().dedicated_read(
        ReadSelection {
            game_id,
            kind: ReadKind::CommandLookup(command),
            role,
            player_id: None,
        },
        token,
        authority.as_ref(),
        &[],
    )? {
        ReadProjection::CommandLookup(v) => Ok(v),
        _ => panic!("lookup projection"),
    }
}
fn snapshot(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .filter(|t| **t != "game_metadata")
        .map(|t| {
            f.game
                .query(&format!("SELECT * FROM {t} ORDER BY 1,2"), &[])
                .unwrap()
        })
        .collect()
}
fn committed(result: CommandResult) -> GameOutcome {
    assert!(result.encode_json().unwrap().len() <= COMMAND_RESULT_MAX_BYTES);
    match result.result_status {
        CommandResultStatus::Committed { receipt, .. } => receipt.outcome,
        _ => panic!("committed"),
    }
}
#[test]
fn command_lookup_creation_lobby_own_actor_unknown_cross_actor_and_no_effects() {
    let f = Fixture::new();
    f.open(&GameConfiguration::default());
    let (_, other) = operator::actor(&f, 60, AccountRole::Host);
    let (_, admin) = operator::actor(&f, 70, AccountRole::Admin);
    let before = snapshot(&f);
    assert!(matches!(
        committed(lookup(&f, f.command(10), ReadRole::Account, &f.token).unwrap()),
        GameOutcome::Created { .. }
    ));
    assert!(matches!(
        committed(lookup(&f, f.command(11), ReadRole::Account, &f.token).unwrap()),
        GameOutcome::LobbyOpened { .. }
    ));
    assert!(matches!(
        lookup(&f, f.command(99), ReadRole::Account, &other)
            .unwrap()
            .result_status,
        CommandResultStatus::Unknown {}
    ));
    assert_eq!(
        serde_json::to_value(
            lookup(&f, f.command(10), ReadRole::Account, &other)
                .unwrap()
                .result_status
        )
        .unwrap(),
        serde_json::to_value(
            lookup(&f, f.command(99), ReadRole::Account, &other)
                .unwrap()
                .result_status
        )
        .unwrap()
    );
    assert_eq!(snapshot(&f), before);
    assert!(matches!(
        lookup(&f, f.command(10), ReadRole::Account, &admin)
            .unwrap()
            .result_status,
        CommandResultStatus::Unknown {}
    ));
    let rows=f.game.query("SELECT actor_account_id,target_game_id,operation,outcome,expires_at-occurred_at FROM game_admin_audit",&[]).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][2], SqlValue::Text("command_lookup".into()));
    assert_eq!(rows[0][4], SqlValue::Integer(7_776_000_000));
}
#[test]
fn command_lookup_current_player_admission_alias_recovery_switch_and_reuse() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration {
        spectator_capacity: 2,
        ..GameConfiguration::default()
    });
    let p = f.player(&code, 30, "LookupPlayer");
    let other = f.player(&code, 31, "OtherPlayer");
    assert!(matches!(
        committed(lookup(&f, f.command(30), ReadRole::Player, p.token()).unwrap()),
        GameOutcome::PlayerJoined { .. }
    ));
    assert!(matches!(
        lookup(&f, f.command(30), ReadRole::Player, other.token())
            .unwrap()
            .result_status,
        CommandResultStatus::Unknown {}
    ));
    f.service()
        .rename_my_alias(f.command(32), p.token(), "RenamedLookup")
        .unwrap();
    f.service()
        .manage_recovery_answer(f.command(33), p.token(), Some("Lookup test answer"))
        .unwrap();
    let recovered = f
        .service()
        .recover_player(
            f.command(34),
            &code,
            "RenamedLookup",
            "Lookup test answer",
            "192.0.2.3",
        )
        .unwrap();
    let fresh = recovered.cookie.unwrap();
    assert_eq!(
        lookup(&f, f.command(20), ReadRole::Player, p.token()).err(),
        Some(GameError::Unauthorized)
    );
    for n in [30, 32, 33, 34] {
        committed(lookup(&f, f.command(n), ReadRole::Player, fresh.token()).unwrap());
    }
    let switched = f
        .service()
        .switch_to_spectator(f.command(35), fresh.token())
        .unwrap();
    assert_eq!(
        lookup(&f, f.command(30), ReadRole::Player, fresh.token()).err(),
        Some(GameError::Unauthorized)
    );
    assert!(matches!(
        lookup(
            &f,
            f.command(35),
            ReadRole::Spectator,
            switched.participant.cookie.as_ref().unwrap().token()
        )
        .unwrap()
        .result_status,
        CommandResultStatus::Unknown {}
    ));
    let reused = f.player(&code, 36, "RenamedLookup");
    assert!(matches!(
        lookup(&f, f.command(30), ReadRole::Player, reused.token())
            .unwrap()
            .result_status,
        CommandResultStatus::Unknown {}
    ));
}
#[test]
fn command_lookup_start_call_terminal_original_grants_and_exit() {
    let f = Fixture::new();
    let (_, p, _, observed) = f.start_two_player_game();
    let rev = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap()
        .view_revision();
    f.service()
        .call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(50),
            "1",
            rev,
        )
        .unwrap();
    assert!(matches!(
        committed(lookup(&f, f.command(50), ReadRole::Account, &f.token).unwrap()),
        GameOutcome::CallAccepted { .. }
    ));
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(51),
            GameState::InProgress,
            true,
        )
        .unwrap();
    assert!(matches!(
        lookup(&f, f.command(51), ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Committed {
            follow_up_pending: true,
            ..
        }
    ));
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
    assert!(matches!(
        lookup(&f, f.command(51), ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Committed {
            follow_up_pending: false,
            ..
        }
    ));
    committed(lookup(&f, f.command(50), ReadRole::Account, &f.token).unwrap());
    committed(lookup(&f, f.command(20), ReadRole::Player, p.token()).unwrap());
    let (_, other) = operator::actor(&f, 60, AccountRole::Host);
    assert_eq!(
        lookup(&f, f.command(50), ReadRole::Account, &other).err(),
        Some(GameError::Unauthorized)
    );
    f.service().exit_player(p.token(), f.command(52)).unwrap();
    assert_eq!(
        lookup(&f, f.command(20), ReadRole::Player, p.token()).err(),
        Some(GameError::Unauthorized)
    );
    f.service()
        .exit_account(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(53),
        )
        .unwrap();
    assert_eq!(
        lookup(&f, f.command(50), ReadRole::Account, &f.token).err(),
        Some(GameError::Unauthorized)
    );
}
#[test]
fn command_lookup_pending_prepared_and_committed_follow_up_are_not_peer_completion() {
    let f = Fixture::new();
    operator::saved(&f);
    let w = match f
        .service()
        .prepare_lobby(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(40),
            0,
        )
        .unwrap()
    {
        brews_backend::game::LobbyPreparation::Ready(w) => w,
        _ => panic!("pending"),
    };
    let before = snapshot(&f);
    assert!(matches!(
        lookup(&f, f.command(40), ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Pending {
            operation: PendingCommandOperation::LobbyPublication,
            state: PendingCommandState::Prepared
        }
    ));
    assert_eq!(snapshot(&f), before);
    let work = f
        .directory()
        .creation_work(f.service().directory_projection().unwrap().game_id())
        .unwrap()
        .unwrap();
    let initialized = f
        .service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &GameConfiguration::default(),
        )
        .unwrap();
    let ack = f
        .directory()
        .acknowledge_creation(initialized.ready)
        .unwrap();
    let grant = f.directory().allocate_game_code(ack).unwrap();
    f.service()
        .commit_lobby(
            &w,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &grant,
        )
        .unwrap();
    let before = snapshot(&f);
    assert!(matches!(
        lookup(&f, f.command(40), ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Committed {
            follow_up_pending: true,
            ..
        }
    ));
    assert_eq!(snapshot(&f), before);
}
#[test]
fn command_lookup_future_floor_exact_expiry_rollback_and_ignored_clock() {
    let f = Fixture::new();
    operator::saved(&f);
    let command = f.command(10);
    let unseen_at_floor = f.command(96);
    let now = f.rt.now.get();
    f.rt.now.set(now + 1);
    let future = f.command(99);
    f.rt.now.set(now);
    assert!(matches!(
        lookup(&f, future, ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Inadmissible {}
    ));
    // Keep the same actor valid through the receipt boundary using a genuine fresh login.
    f.rt.now.set(now + 86_400_000 - 1);
    let token = cookie(
        f.auth()
            .execute(
                AuthCommand::Login {
                    username: "GameHostPerson".into(),
                    password: f.password.to_string(),
                },
                RequestContext {
                    command_id: Some(id(&f.rt, 98)),
                    caller_identity: "trusted-fixture".into(),
                },
            )
            .unwrap(),
    );
    f.game
        .execute(
            "UPDATE game_record SET idle_due=?,last_host_activity=?",
            &[
                SqlValue::Integer(now + 172_800_000),
                SqlValue::Integer(now + 86_400_000),
            ],
        )
        .unwrap();
    committed(lookup(&f, command, ReadRole::Account, &token).unwrap());
    f.rt.now.set(now + 86_400_000);
    assert!(matches!(
        lookup(&f, unseen_at_floor, ReadRole::Account, &token)
            .unwrap()
            .result_status,
        CommandResultStatus::Expired {}
    ));
    assert!(matches!(
        lookup(&f, command, ReadRole::Account, &token)
            .unwrap()
            .result_status,
        CommandResultStatus::Expired {}
    ));
    assert!(matches!(
        lookup(
            &f,
            id(&f.rt, 97).parse().unwrap(),
            ReadRole::Account,
            &token
        )
        .unwrap()
        .result_status,
        CommandResultStatus::Unknown {}
    ));
    f.rt.now.set(now + 86_400_000 - 1);
    assert_eq!(
        lookup(&f, command, ReadRole::Account, &token).err(),
        Some(GameError::Storage)
    );
    let g = Fixture::new();
    operator::saved(&g);
    g.game.execute("CREATE TRIGGER ignore_lookup_clock BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    g.rt.now.set(g.rt.now.get() + 1);
    assert_eq!(
        lookup(&g, g.command(99), ReadRole::Account, &g.token).err(),
        Some(GameError::Storage)
    );
}
#[test]
fn command_lookup_corrupt_receipt_pair_and_columns_fail_closed() {
    for fault in ["game", "command", "time", "oversize", "type", "actor"] {
        let f = Fixture::new();
        operator::saved(&f);
        let row = f
            .game
            .query("SELECT outcome FROM game_receipts", &[])
            .unwrap();
        let mut v: serde_json::Value = serde_json::from_str(match &row[0][0] {
            SqlValue::Text(s) => s,
            _ => panic!("text"),
        })
        .unwrap();
        match fault {
            "game" => v["game_id"] = id(&f.rt, 80).into(),
            "command" => v["command_id"] = id(&f.rt, 81).into(),
            "time" => v["completed_at"] = 0.into(),
            "oversize" => v["padding"] = "x".repeat(4096).into(),
            "type" => v["outcome"]["view_revision"] = "0".into(),
            "actor" => {
                v["outcome"] = serde_json::json!({"operation":"player_joined","player_id":id(&f.rt,82),"view_revision":0,"session_expires_at":f.rt.now.get()+86_400_000})
            }
            _ => unreachable!(),
        };
        f.game
            .execute("PRAGMA ignore_check_constraints=ON", &[])
            .unwrap();
        f.game
            .execute(
                "UPDATE game_receipts SET outcome=?",
                &[SqlValue::Text(v.to_string())],
            )
            .unwrap();
        assert_eq!(
            lookup(&f, f.command(10), ReadRole::Account, &f.token).err(),
            Some(GameError::Storage),
            "{fault}"
        );
    }
}
#[test]
fn command_lookup_directory_creation_exact_original_account_only_and_passive() {
    let f = Fixture::new();
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&GameConfiguration::default()).unwrap(),
        )
        .unwrap();
    let before = f
        .directory
        .query("SELECT * FROM directory_game_creations", &[])
        .unwrap();
    assert_eq!(
        f.directory()
            .command_creation_pending(f.account, work.game_id(), f.command(10))
            .unwrap(),
        Some(work.created_at())
    );
    assert_eq!(
        f.directory()
            .command_creation_pending(
                id(&f.rt, 80).parse().unwrap(),
                work.game_id(),
                f.command(10)
            )
            .unwrap(),
        None
    );
    assert_eq!(
        f.directory()
            .command_creation_pending(f.account, id(&f.rt, 81).parse().unwrap(), f.command(10))
            .unwrap(),
        None
    );
    assert_eq!(
        f.directory
            .query("SELECT * FROM directory_game_creations", &[])
            .unwrap(),
        before
    );
}
#[test]
fn command_lookup_prepared_transfer_rejects_changed_semantic_inputs() {
    for field in ["target", "assignment"] {
        let f = Fixture::new();
        operator::saved(&f);
        let (target, _) = operator::actor(&f, 70, AccountRole::Host);
        let command = f.command(50);
        let input = brews_contracts::games::HostTransferInput {
            target_account_id: target,
            confirmed: true,
            expected_host_assignment_revision: 0,
        };
        f.service()
            .prepare_host_transfer(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &input,
            )
            .unwrap();
        assert!(matches!(
            lookup(&f, command, ReadRole::Account, &f.token)
                .unwrap()
                .result_status,
            CommandResultStatus::Pending { .. }
        ));
        if field == "target" {
            f.game
                .execute(
                    "UPDATE game_host_transfers SET target_account_id=? WHERE command_id=?",
                    &[
                        SqlValue::Text(id(&f.rt, 90)),
                        SqlValue::Text(command.to_string()),
                    ],
                )
                .unwrap();
        } else {
            f.game
                .execute(
                    "UPDATE game_host_transfers SET expected_assignment_revision=1 WHERE command_id=?",
                    &[SqlValue::Text(command.to_string())],
                )
                .unwrap();
        }
        assert_eq!(
            lookup(&f, command, ReadRole::Account, &f.token).err(),
            Some(GameError::Storage),
            "{field}"
        );
    }
}

#[test]
fn command_lookup_outgoing_host_own_transfer_receipt_survives_loss_of_mutation_privilege() {
    let f = Fixture::new();
    operator::saved(&f);
    let (target, target_token) = operator::actor(&f, 70, AccountRole::Host);
    let command = f.command(50);
    let input = brews_contracts::games::HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            &input,
        )
        .unwrap()
    {
        brews_backend::db::game::TransferPreparation::Prepared(w) => w,
        _ => panic!("work"),
    };
    assert!(matches!(
        lookup(&f, command, ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Pending {
            operation: PendingCommandOperation::HostTransfer,
            state: PendingCommandState::Prepared
        }
    ));
    let grant = f.directory().prepare_host_transfer(&work).unwrap();
    let (actor, recipient) = f
        .auth()
        .authorize_host_transfer_recipient(&f.token, &work)
        .unwrap();
    f.service()
        .commit_host_transfer(actor, recipient, &grant)
        .unwrap();
    assert!(matches!(
        lookup(&f, command, ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Committed {
            follow_up_pending: true,
            ..
        }
    ));
    assert!(matches!(
        lookup(&f, command, ReadRole::Account, &target_token)
            .unwrap()
            .result_status,
        CommandResultStatus::Unknown {}
    ));
    assert_eq!(
        f.service()
            .prepare_lobby(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(51),
                1
            )
            .err(),
        Some(GameError::Forbidden)
    );
    let proof = f.service().host_transfer_proof(&work, false).unwrap();
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    assert!(matches!(
        lookup(&f, command, ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Committed {
            follow_up_pending: false,
            ..
        }
    ));
}
#[test]
fn command_lookup_old_required_work_remains_pending_but_committed_receipt_expires() {
    for commit in [false, true] {
        let f = Fixture::new();
        operator::saved(&f);
        let now = f.rt.now.get();
        let command = f.command(40);
        let work = match f
            .service()
            .prepare_lobby(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                0,
            )
            .unwrap()
        {
            brews_backend::game::LobbyPreparation::Ready(w) => w,
            _ => panic!("work"),
        };
        if commit {
            let creation = f
                .directory()
                .creation_work(f.service().directory_projection().unwrap().game_id())
                .unwrap()
                .unwrap();
            let ready = f
                .service()
                .initialize(
                    &creation,
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    &GameConfiguration::default(),
                )
                .unwrap();
            let ack = f.directory().acknowledge_creation(ready.ready).unwrap();
            let grant = f.directory().allocate_game_code(ack).unwrap();
            f.service()
                .commit_lobby(
                    &work,
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    &grant,
                )
                .unwrap();
        }
        f.rt.now.set(now + 86_400_001);
        let token = cookie(
            f.auth()
                .execute(
                    AuthCommand::Login {
                        username: "GameHostPerson".into(),
                        password: f.password.to_string(),
                    },
                    RequestContext {
                        command_id: Some(id(&f.rt, 98)),
                        caller_identity: "trusted-fixture".into(),
                    },
                )
                .unwrap(),
        );
        f.game
            .execute(
                "UPDATE game_record SET last_host_activity=?,idle_due=?",
                &[
                    SqlValue::Integer(f.rt.now.get()),
                    SqlValue::Integer(f.rt.now.get() + 86_400_000),
                ],
            )
            .unwrap();
        let before = f
            .game
            .query("SELECT * FROM game_pending_work", &[])
            .unwrap();
        let status = lookup(&f, command, ReadRole::Account, &token)
            .unwrap()
            .result_status;
        if commit {
            assert!(matches!(status, CommandResultStatus::Expired {}));
        } else {
            assert!(matches!(status, CommandResultStatus::Pending { .. }));
        }
        assert_eq!(
            f.game
                .query("SELECT * FROM game_pending_work", &[])
                .unwrap(),
            before
        );
    }
}
#[test]
fn command_lookup_transfer_abort_retains_pending_until_required_ack() {
    let f = Fixture::new();
    operator::saved(&f);
    let (target, _) = operator::actor(&f, 70, AccountRole::Host);
    let command = f.command(50);
    let input = brews_contracts::games::HostTransferInput {
        target_account_id: target,
        confirmed: true,
        expected_host_assignment_revision: 0,
    };
    let work = match f
        .service()
        .prepare_host_transfer(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            &input,
        )
        .unwrap()
    {
        brews_backend::db::game::TransferPreparation::Prepared(w) => w,
        _ => panic!("work"),
    };
    f.directory().prepare_host_transfer(&work).unwrap();
    let proof = f.service().host_transfer_proof(&work, true).unwrap();
    let before = f
        .game
        .query("SELECT * FROM game_host_transfers", &[])
        .unwrap();
    assert!(matches!(
        lookup(&f, command, ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Pending {
            operation: PendingCommandOperation::HostTransfer,
            state: PendingCommandState::AwaitingAcknowledgement
        }
    ));
    assert_eq!(
        f.game
            .query("SELECT * FROM game_host_transfers", &[])
            .unwrap(),
        before
    );
    let ack = f.directory().settle_host_transfer(&proof).unwrap();
    f.service().acknowledge_host_transfer(&proof, ack).unwrap();
    assert!(matches!(
        lookup(&f, command, ReadRole::Account, &f.token)
            .unwrap()
            .result_status,
        CommandResultStatus::Unknown {}
    ));
}
