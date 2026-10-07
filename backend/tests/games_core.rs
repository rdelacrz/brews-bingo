#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real SQLite tests fail fast."
)]
#[path = "games_core/publication_receipt.rs"]
mod publication_receipt;
mod support;
#[path = "games_core/terminal.rs"]
mod terminal;
use brews_backend::db::{Database, SqlValue};
use brews_backend::{
    auth::{
        AuthCommand, AuthPolicy, AuthService, CookieEffect, ManagementPrincipal, RequestContext,
    },
    db::{
        directory::{DirectoryService, migrate_directory},
        game::GameService,
        migrate,
    },
    game::{GameError, creation_fingerprint},
};
use brews_contracts::games::{GameResponse, GameView};
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::{
    accounts::{AccessLinkPurpose, AccountRole, SessionScope},
    games::{BOARD_CELL_KIND_VALUE, GameConfiguration},
    ids::{AccountId, CommandId, ConnectionId},
};
use support::{Sqlite, TestRuntime};
fn id(rt: &TestRuntime, n: u8) -> String {
    let mut b = [0; 16];
    b[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    b[6] = 0x70;
    b[8] = 0x80;
    b[15] = n;
    uuid::Uuid::from_bytes(b).to_string()
}
fn cookie(out: brews_backend::auth::AuthOutcome) -> String {
    match out.cookie {
        CookieEffect::Set { token, .. } => token,
        _ => panic!("No fresh credential"),
    }
}
struct Fixture {
    accounts: Sqlite,
    directory: Sqlite,
    game: Sqlite,
    rt: TestRuntime,
    account: AccountId,
    token: String,
    password: zeroize::Zeroizing<String>,
}
impl Fixture {
    fn new() -> Self {
        let accounts = Sqlite::new();
        let directory = Sqlite::new();
        let game = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&accounts).unwrap();
        migrate_directory(&directory).unwrap();
        brews_backend::db::game::migrate_game(&game).unwrap();
        let auth = AuthService::new(&accounts, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let created = auth
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: "GameHostPerson".into(),
                    role: AccountRole::Host,
                },
                ManagementPrincipal::DeveloperCli,
                Some(id(&rt, 1).parse().unwrap()),
                "https://app.example.test",
            )
            .unwrap();
        let (account, link) = match created {
            ManagementResponse::Issued { receipt, url } => (
                receipt.account_id,
                url.rsplit('#').next().unwrap().to_owned(),
            ),
            _ => panic!("No enrollment"),
        };
        let ctx = |n| RequestContext {
            command_id: Some(id(&rt, n)),
            caller_identity: "trusted-fixture".into(),
        };
        let restricted = cookie(
            auth.execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token: link,
                },
                ctx(2),
            )
            .unwrap(),
        );
        let password = brews_backend::security::new_token(&rt).unwrap();
        let token = cookie(
            auth.execute(
                AuthCommand::Complete {
                    scope: SessionScope::EnrollmentOnly,
                    token: restricted,
                    new_password: password.clone(),
                },
                ctx(3),
            )
            .unwrap(),
        );
        Self {
            accounts,
            directory,
            game,
            rt,
            account,
            token,
            password: zeroize::Zeroizing::new(password),
        }
    }
    fn service(&self) -> GameService<'_, Sqlite, TestRuntime> {
        GameService::new(&self.game, &self.rt, AuthPolicy::default(), &[9; 32]).unwrap()
    }
    fn auth(&self) -> AuthService<'_, Sqlite, TestRuntime> {
        AuthService::new(&self.accounts, &self.rt, AuthPolicy::default(), &[9; 32]).unwrap()
    }
    fn directory(&self) -> DirectoryService<'_, Sqlite, TestRuntime> {
        DirectoryService::new(&self.directory, &self.rt).unwrap()
    }
    fn command(&self, n: u8) -> CommandId {
        id(&self.rt, n).parse().unwrap()
    }
    fn player(
        &self,
        code: &brews_domain::games::GameCode,
        n: u8,
        alias: &str,
    ) -> brews_backend::game::SecretCookie {
        let context = self
            .service()
            .admission_context(code, None, None)
            .unwrap()
            .cookie
            .unwrap();
        self.service()
            .join_player(
                self.command(n),
                context.token(),
                None,
                brews_contracts::games::JoinPlayer {
                    game_code: code.clone(),
                    alias: alias.into(),
                    recovery_answer: None,
                },
            )
            .unwrap()
            .cookie
            .unwrap()
    }
    fn open(&self, config: &GameConfiguration) -> brews_domain::games::GameCode {
        use brews_backend::game::LobbyPreparation;
        let work = self
            .directory()
            .claim_game(
                self.account,
                self.command(10),
                creation_fingerprint(config).unwrap(),
            )
            .unwrap();
        let created = self
            .service()
            .initialize(
                &work,
                self.auth().authorize_game_account(&self.token).unwrap(),
                config,
            )
            .unwrap();
        let ack = self
            .directory()
            .acknowledge_creation(created.ready)
            .unwrap();
        let intent = match self
            .service()
            .prepare_lobby(
                self.auth().authorize_game_account(&self.token).unwrap(),
                self.command(11),
                0,
            )
            .unwrap()
        {
            LobbyPreparation::Ready(w) => w,
            _ => panic!("Missing intent"),
        };
        let code = self.directory().allocate_game_code(ack).unwrap();
        self.service()
            .commit_lobby(
                &intent,
                self.auth().authorize_game_account(&self.token).unwrap(),
                &code,
            )
            .unwrap();
        let pending = self.service().pending_work(1).unwrap().remove(0);
        let ack = self
            .directory()
            .publish_game(self.service().directory_projection().unwrap())
            .unwrap();
        self.service()
            .acknowledge_projection(&pending, ack)
            .unwrap();
        code.game_code().clone()
    }
    fn start_two_player_game(
        &self,
    ) -> (
        brews_domain::games::GameCode,
        brews_backend::game::SecretCookie,
        brews_backend::game::SecretCookie,
        [ConnectionId; 2],
    ) {
        let config = GameConfiguration {
            numeric_upper_bound: 4,
            board_side_length: 2,
            free_cells_enabled: false,
            free_cell_positions: vec![],
            player_capacity: 2,
            spectator_capacity: 0,
            ..GameConfiguration::default()
        };
        let code = self.open(&config);
        let first = self.player(&code, 20, "First");
        let second = self.player(&code, 21, "Second");
        let connections = [
            id(&self.rt, 40).parse().unwrap(),
            id(&self.rt, 41).parse().unwrap(),
        ];
        for (player, connection) in [(&first, connections[0]), (&second, connections[1])] {
            let grant = self
                .service()
                .prepare_player_connection(player.token(), connection)
                .unwrap();
            self.service().accept_player_connection(&grant).unwrap();
        }
        let observed = connections;
        let host_view = self
            .service()
            .account_view(
                self.auth().authorize_game_account(&self.token).unwrap(),
                &observed,
            )
            .unwrap();
        let game_id = self.service().directory_projection().unwrap().game_id();
        let reservation = self.directory().confirm_reservation(game_id).unwrap();
        assert!(matches!(
            self.service()
                .start(
                    self.auth().authorize_game_account(&self.token).unwrap(),
                    self.command(30),
                    host_view.view_revision(),
                    &reservation,
                    &observed,
                )
                .unwrap(),
            GameResponse::Started { .. }
        ));
        (code, first, second, connections)
    }
}

fn game_rows(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    [
        "game_admission_contexts",
        "game_board_cells",
        "game_boards",
        "game_completed_lines",
        "game_configuration",
        "game_connection_grants",
        "game_current_connections",
        "game_free_cells",
        "game_metadata",
        "game_pending_work",
        "game_players",
        "game_receipts",
        "game_record",
        "game_recovery",
        "game_sessions",
        "game_view_revisions",
    ]
    .iter()
    .map(|table| {
        f.game
            .query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
            .unwrap()
    })
    .collect()
}

#[test]
fn manual_call_matches_every_card_and_exact_replay_does_not_advance_views() {
    let f = Fixture::new();
    let (_, first, second, connections) = f.start_two_player_game();
    let first_id = f
        .game
        .query(
            "SELECT player_id FROM game_boards ORDER BY player_id LIMIT 1",
            &[],
        )
        .unwrap()[0][0]
        .clone();
    let value = f
        .game
        .query(
            "SELECT value FROM game_board_cells WHERE player_id=? AND row=1 AND column=1",
            &[first_id],
        )
        .unwrap()[0][0]
        .clone();
    let value = match value {
        SqlValue::Text(value) => value,
        _ => panic!("board value must be text"),
    };
    let host_before = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &connections,
        )
        .unwrap();
    let expected_revision = host_before.view_revision();
    let first_revision = f
        .service()
        .player_view(first.token())
        .unwrap()
        .view_revision();
    let second_revision = f
        .service()
        .player_view(second.token())
        .unwrap()
        .view_revision();
    let command = f.command(50);

    let response = f
        .service()
        .call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            &value,
            expected_revision,
        )
        .unwrap();
    let (receipt, sequence_no, returned_value, remaining_count, exhausted, view_revision) =
        match response {
            GameResponse::CallAccepted {
                call,
                remaining_count,
                exhausted,
                view_revision,
                receipt,
            } => (
                receipt,
                call.sequence_no,
                call.value,
                remaining_count,
                exhausted,
                view_revision,
            ),
            other => panic!("unexpected call response: {other:?}"),
        };
    assert_eq!(sequence_no, 1);
    assert_eq!(returned_value, value);
    assert_eq!(remaining_count, 3);
    assert!(!exhausted);
    assert_eq!(view_revision, expected_revision + 1);
    assert_eq!(
        f.game
            .query(
                "SELECT count(*) FROM game_board_cells WHERE kind=? AND value=? AND is_matched=1",
                &[
                    SqlValue::Text(BOARD_CELL_KIND_VALUE.into()),
                    SqlValue::Text(value.clone()),
                ],
            )
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
    let first_view = f.service().player_view(first.token()).unwrap();
    let second_view = f.service().player_view(second.token()).unwrap();
    assert_eq!(first_view.view_revision(), first_revision + 1);
    assert_eq!(second_view.view_revision(), second_revision + 1);
    assert!(matches!(
        first_view,
        GameView::Player { board: Some(_), .. }
    ));

    let replay = f
        .service()
        .call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            &value,
            expected_revision,
        )
        .unwrap();
    let replayed_receipt = match replay {
        GameResponse::Committed { receipt } => receipt,
        other => panic!("unexpected retry response: {other:?}"),
    };
    assert_eq!(
        receipt.encode_json().unwrap(),
        replayed_receipt.encode_json().unwrap()
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_calls", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert!(matches!(
        f.service().call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(51),
            "1",
            expected_revision,
        ),
        Err(GameError::StaleRevision)
    ));
    assert!(matches!(
        f.service().call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(52),
            &value,
            expected_revision + 1,
        ),
        Err(GameError::Conflict)
    ));
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_calls", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn random_calls_draw_unique_remaining_values_and_exhaustion_does_not_end_game() {
    let f = Fixture::new();
    let (_, _, _, connections) = f.start_two_player_game();
    let mut expected_revision = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &connections,
        )
        .unwrap()
        .view_revision();
    let mut values = Vec::new();
    for index in 0..4 {
        let response = f
            .service()
            .call_random(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(60 + index),
                expected_revision,
            )
            .unwrap();
        match response {
            GameResponse::CallAccepted {
                call,
                remaining_count,
                exhausted,
                view_revision,
                ..
            } => {
                assert_eq!(call.sequence_no, u32::from(index) + 1);
                assert_eq!(remaining_count, 3 - u32::from(index));
                assert_eq!(exhausted, index == 3);
                expected_revision = view_revision;
                values.push(call.value);
            }
            other => panic!("unexpected random call response: {other:?}"),
        }
    }
    values.sort();
    values.dedup();
    assert_eq!(values.len(), 4);
    assert!(matches!(
        f.service().call_random(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(70),
            expected_revision,
        ),
        Err(GameError::Conflict)
    ));
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_calls", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(4)]]
    );
    assert_eq!(
        f.game.query("SELECT state FROM game_record", &[]).unwrap(),
        vec![vec![SqlValue::Text("in_progress".into())]]
    );
}

#[test]
fn random_entropy_or_board_write_failure_rolls_back_call_and_matches() {
    let f = Fixture::new();
    let (_, _, _, connections) = f.start_two_player_game();
    let expected_revision = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &connections,
        )
        .unwrap()
        .view_revision();
    f.rt.fail.set(true);
    assert!(matches!(
        f.service().call_random(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(60),
            expected_revision,
        ),
        Err(GameError::RandomUnavailable)
    ));
    f.rt.fail.set(false);
    f.game.conn.borrow().execute_batch("CREATE TRIGGER reject_gameplay_cell_updates BEFORE UPDATE ON game_board_cells BEGIN SELECT RAISE(ABORT,'injected write fault'); END;").unwrap();
    let value: String = f
        .game
        .conn
        .borrow()
        .query_row(
            "SELECT value FROM game_board_cells ORDER BY player_id,row,column LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(matches!(
        f.service().call_manual_value(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(61),
            &value,
            expected_revision,
        ),
        Err(GameError::Storage)
    ));
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_calls", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    assert_eq!(
        f.game
            .query("SELECT revision FROM game_record", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
}

#[test]
fn initial_host_revision_must_be_written_as_zero_or_creation_rolls_back() {
    for trigger in [
        "CREATE TRIGGER revision_fault BEFORE INSERT ON game_view_revisions BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER revision_fault AFTER INSERT ON game_view_revisions BEGIN UPDATE game_view_revisions SET revision=1 WHERE view_key=NEW.view_key; END",
    ] {
        let f = Fixture::new();
        let config = GameConfiguration::default();
        let work = f
            .directory()
            .claim_game(
                f.account,
                f.command(10),
                creation_fingerprint(&config).unwrap(),
            )
            .unwrap();
        let baseline = game_rows(&f);
        f.game.execute(trigger, &[]).unwrap();
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
        assert_eq!(
            game_rows(&f),
            baseline,
            "all creation writes must roll back"
        );
        f.game.execute("DROP TRIGGER revision_fault", &[]).unwrap();
        let created = f
            .service()
            .initialize(
                &work,
                f.auth().authorize_game_account(&f.token).unwrap(),
                &config,
            )
            .unwrap();
        created.response.encode_json().unwrap();
        assert_eq!(
            f.service()
                .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
                .unwrap()
                .view_revision(),
            0
        );
        assert_eq!(
            f.game
                .query("SELECT view_key,revision FROM game_view_revisions", &[])
                .unwrap(),
            vec![vec![SqlValue::Text("host".into()), SqlValue::Integer(0)]]
        );
    }
}

#[test]
fn initial_player_revision_must_be_written_as_zero_or_join_rolls_back() {
    for trigger in [
        "CREATE TRIGGER revision_fault BEFORE INSERT ON game_view_revisions BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER revision_fault AFTER INSERT ON game_view_revisions BEGIN UPDATE game_view_revisions SET revision=1 WHERE view_key=NEW.view_key; END",
    ] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let context = f
            .service()
            .admission_context(&code, None, None)
            .unwrap()
            .cookie
            .unwrap();
        let command = f.command(20);
        let input = || {
            brews_contracts::games::JoinPlayer::decode_json(
            serde_json::json!({"game_code":code.as_str(), "alias":"Alice", "recovery_answer":"Straße"}).to_string().as_bytes()
        ).unwrap()
        };
        let baseline = game_rows(&f);
        f.game.execute(trigger, &[]).unwrap();
        assert_eq!(
            f.service()
                .join_player(command, context.token(), None, input())
                .err(),
            Some(GameError::Storage)
        );
        assert_eq!(
            game_rows(&f),
            baseline,
            "membership, session, answer, context, receipt and host revision must roll back"
        );
        f.game.execute("DROP TRIGGER revision_fault", &[]).unwrap();
        let joined = f
            .service()
            .join_player(command, context.token(), None, input())
            .unwrap();
        joined.response.encode_json().unwrap();
        let player = joined.cookie.unwrap();
        assert_eq!(
            f.service()
                .player_view(player.token())
                .unwrap()
                .view_revision(),
            0
        );
        let replay = f
            .service()
            .join_player(command, context.token(), None, input())
            .unwrap();
        assert!(replay.cookie.is_none());
        replay.response.encode_json().unwrap();
    }
}

#[test]
fn ignored_player_grant_supersession_rolls_back_and_stale_grants_cannot_replace_successor() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let player = f.player(&code, 20, "Alice");
    let old = f
        .service()
        .prepare_player_connection(player.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let baseline = game_rows(&f);
    f.game.execute("CREATE TRIGGER grant_ignore BEFORE DELETE ON game_connection_grants BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap();
    assert_eq!(
        f.service()
            .prepare_player_connection(player.token(), id(&f.rt, 41).parse().unwrap())
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(
        game_rows(&f),
        baseline,
        "an ignored supersession must not mint another grant"
    );
    f.game.execute("DROP TRIGGER grant_ignore", &[]).unwrap();
    let new = f
        .service()
        .prepare_player_connection(player.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&new).unwrap();
    let accepted = game_rows(&f);
    f.service().accept_player_connection(&new).unwrap();
    assert_eq!(
        f.service().accept_player_connection(&old),
        Err(GameError::Unauthorized)
    );
    f.service().disconnect(&old).unwrap();
    assert_eq!(
        game_rows(&f),
        accepted,
        "accept retry and stale close must leave successor unchanged"
    );
    assert_eq!(
        f.service().player_connection(new.connection_id()).unwrap(),
        Some(new)
    );
    assert!(
        f.service()
            .player_connection(old.connection_id())
            .unwrap()
            .is_none()
    );
}

#[test]
fn ignored_account_grant_supersession_rolls_back_without_deleting_other_viewer_bindings() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let player = f.player(&code, 20, "Alice");
    let player_grant = f
        .service()
        .prepare_player_connection(player.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let old = f
        .service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&f.token).unwrap(),
            id(&f.rt, 60).parse().unwrap(),
        )
        .unwrap();
    let baseline = game_rows(&f);
    f.game.execute("CREATE TRIGGER grant_ignore BEFORE DELETE ON game_connection_grants BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap();
    assert_eq!(
        f.service()
            .prepare_account_connection(
                f.auth().authorize_game_account(&f.token).unwrap(),
                id(&f.rt, 61).parse().unwrap()
            )
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(game_rows(&f), baseline);
    f.game.execute("DROP TRIGGER grant_ignore", &[]).unwrap();
    let new = f
        .service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&f.token).unwrap(),
            id(&f.rt, 61).parse().unwrap(),
        )
        .unwrap();
    assert_eq!(f.game.query("SELECT connection_id FROM game_connection_grants WHERE viewer_kind='account' AND connection_id IS NULL", &[]).unwrap(), vec![vec![SqlValue::Null]]);
    f.service()
        .accept_account_connection(&new, f.auth().authorize_game_account(&f.token).unwrap())
        .unwrap();
    let accepted = game_rows(&f);
    f.service()
        .accept_account_connection(&new, f.auth().authorize_game_account(&f.token).unwrap())
        .unwrap();
    assert_eq!(
        f.service()
            .accept_account_connection(&old, f.auth().authorize_game_account(&f.token).unwrap()),
        Err(GameError::Unauthorized)
    );
    assert_eq!(game_rows(&f), accepted);
    f.service()
        .account_connection_view(&new, f.auth().authorize_game_account(&f.token).unwrap())
        .unwrap();
    f.service().accept_player_connection(&player_grant).unwrap();
}

#[test]
fn creation_uses_directory_identity_and_is_atomic_replayable_and_uncoded() {
    let f = Fixture::new();
    let config = GameConfiguration::default();
    let fp = creation_fingerprint(&config).unwrap();
    let work = f
        .directory()
        .claim_game(f.account, f.command(10), fp)
        .unwrap();
    let out = f
        .service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    assert_eq!(out.ready.game_id(), work.game_id());
    assert_eq!(out.ready.source_revision(), 0);
    assert_eq!(f.game.query("SELECT state,revision,host_assignment_revision,created_at,last_host_activity,idle_due,game_code FROM game_record",&[]).unwrap(),vec![vec![SqlValue::Text("new".into()),SqlValue::Integer(0),SqlValue::Integer(0),SqlValue::Integer(work.created_at()),SqlValue::Integer(work.created_at()),SqlValue::Integer(work.created_at()+86_400_000),SqlValue::Null]]);
    f.directory().acknowledge_creation(out.ready).unwrap();
    f.rt.now.set(f.rt.now.get() + 1);
    let replay = f
        .service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    assert_eq!(replay.ready, out.ready);
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_receipts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert!(
        f.game
            .query("SELECT player_id FROM game_boards", &[])
            .unwrap()
            .is_empty()
    );
    let mut changed = config.clone();
    changed.numeric_upper_bound = 76;
    assert_ne!(creation_fingerprint(&changed).unwrap(), fp);
    assert_eq!(
        f.service()
            .initialize(
                &work,
                f.auth().authorize_game_account(&f.token).unwrap(),
                &changed
            )
            .err(),
        Some(GameError::Conflict)
    );
}

#[test]
fn lobby_intent_precedes_code_and_success_waits_for_exact_publication_ack() {
    use brews_backend::game::LobbyPreparation;
    use brews_contracts::games::GameResponse;
    let f = Fixture::new();
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
    let ack = f.directory().acknowledge_creation(out.ready).unwrap();
    let command = f.command(11);
    let prep = match f
        .service()
        .prepare_lobby(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            0,
        )
        .unwrap()
    {
        LobbyPreparation::Ready(w) => w,
        _ => panic!("Need intent"),
    };
    assert_eq!(
        f.game
            .query("SELECT phase FROM game_pending_work", &[])
            .unwrap(),
        vec![vec![SqlValue::Text("prepared".into())]]
    );
    assert_eq!(
        f.service()
            .prepare_lobby(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                1
            )
            .err(),
        Some(GameError::Conflict)
    );
    let grant = f.directory().allocate_game_code(ack).unwrap();
    f.rt.now.set(f.rt.now.get() + 10);
    assert!(
        matches!(f.service().commit_lobby(&prep,f.auth().authorize_game_account(&f.token).unwrap(),&grant).unwrap(),GameResponse::Pending{operation_id} if operation_id==prep.operation_id())
    );
    assert!(
        f.directory()
            .lookup_game_code(grant.game_code())
            .unwrap()
            .is_none()
    );
    let before = f
        .service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap();
    assert!(matches!(
        before,
        brews_contracts::games::GameView::Host {
            game_code: None,
            ..
        }
    ));
    let receipt_before = f
        .game
        .query(
            "SELECT completed_at,expires_at FROM game_receipts WHERE command_id=?",
            &[SqlValue::Text(command.to_string())],
        )
        .unwrap();
    let projection = f.service().directory_projection().unwrap();
    let pending = f.service().pending_work(10).unwrap().remove(0);
    let publication = f.directory().publish_game(projection).unwrap();
    let foreign = Fixture::new();
    foreign.rt.random.set(2);
    foreign.open(&config);
    let foreign_projection = foreign.service().directory_projection().unwrap();
    assert_ne!(foreign_projection.game_id(), work.game_id());
    let foreign_ack = foreign
        .directory()
        .publish_game(foreign_projection)
        .unwrap();
    let before_foreign_ack = game_rows(&f);
    assert_eq!(
        f.service()
            .acknowledge_projection(&pending, foreign_ack)
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(game_rows(&f), before_foreign_ack);
    assert_eq!(
        f.service().acknowledge_projection(&prep, publication).err(),
        Some(GameError::Conflict)
    );
    let retry = f.service().retry_work(&pending).unwrap();
    assert_eq!(retry.attempt_count(), pending.attempt_count() + 1);
    // Retry metadata is not part of the publication target; the target fence is.
    for trigger in [
        "CREATE TRIGGER publication_ignore BEFORE UPDATE OF published_at ON game_record BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER publication_ignore BEFORE UPDATE ON game_view_revisions BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER publication_ignore BEFORE UPDATE ON game_receipts BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER publication_ignore BEFORE DELETE ON game_pending_work BEGIN SELECT RAISE(IGNORE); END",
    ] {
        let queries = [
            "SELECT * FROM game_record",
            "SELECT * FROM game_view_revisions ORDER BY view_key",
            "SELECT * FROM game_receipts ORDER BY actor,command_id",
            "SELECT * FROM game_pending_work",
        ];
        let baseline: Vec<_> = queries
            .iter()
            .map(|sql| f.game.query(sql, &[]).unwrap())
            .collect();
        f.game.execute(trigger, &[]).unwrap();
        assert_eq!(
            f.service()
                .acknowledge_projection(&pending, publication)
                .err(),
            Some(GameError::Storage)
        );
        f.game
            .execute("DROP TRIGGER publication_ignore", &[])
            .unwrap();
        let after_fault: Vec<_> = queries
            .iter()
            .map(|sql| f.game.query(sql, &[]).unwrap())
            .collect();
        assert_eq!(
            after_fault, baseline,
            "publication must roll back every required write"
        );
    }
    f.rt.now.set(f.rt.now.get() + 10);
    let response = f
        .service()
        .acknowledge_projection(&pending, publication)
        .unwrap();
    let after = f
        .service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap();
    assert_eq!(
        after.view_revision(),
        before.view_revision() + 1,
        "publication changes the visible host projection"
    );
    assert!(
        matches!(after, brews_contracts::games::GameView::Host { game_code: Some(ref code), .. } if code == grant.game_code())
    );
    let sync = f
        .service()
        .sync_account(
            f.auth().authorize_game_account(&f.token).unwrap(),
            Some(before.view_revision()),
            &[],
        )
        .unwrap();
    assert!(!sync.up_to_date);
    assert_eq!(
        serde_json::to_value(sync.snapshot.as_ref().unwrap()).unwrap(),
        serde_json::to_value(&after).unwrap()
    );
    sync.encode_json().unwrap();
    assert!(matches!(
        response,
        GameResponse::LobbyOpened { view_revision, .. } if view_revision == after.view_revision()
    ));
    response.encode_json().unwrap();
    let receipt = match &response {
        GameResponse::LobbyOpened { receipt, .. } => receipt,
        _ => panic!("No lobby success"),
    };
    receipt.encode_json().unwrap();
    assert!(
        matches!(receipt.outcome, brews_contracts::games::GameOutcome::LobbyOpened { view_revision } if view_revision == after.view_revision())
    );
    assert_eq!(
        f.game
            .query(
                "SELECT completed_at,expires_at FROM game_receipts WHERE command_id=?",
                &[SqlValue::Text(command.to_string())]
            )
            .unwrap(),
        receipt_before
    );
    assert_eq!(
        f.service()
            .acknowledge_projection(&pending, publication)
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(
        f.service()
            .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
            .unwrap()
            .view_revision(),
        after.view_revision()
    );
    let replay = f
        .service()
        .prepare_lobby(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            0,
        )
        .unwrap();
    match replay {
        LobbyPreparation::Committed(replayed) => assert_eq!(
            replayed.encode_json().unwrap(),
            receipt.encode_json().unwrap()
        ),
        _ => panic!("No exact receipt replay"),
    }
    assert_eq!(
        f.directory().lookup_game_code(grant.game_code()).unwrap(),
        Some(work.game_id())
    );
    assert!(f.service().pending_work(10).unwrap().is_empty());
    assert!(matches!(
        f.service()
            .prepare_lobby(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                0
            )
            .unwrap(),
        LobbyPreparation::Committed(_)
    ));
    assert_eq!(
        f.game
            .query(
                "SELECT revision,last_host_activity,idle_due,published_at FROM game_record",
                &[]
            )
            .unwrap(),
        vec![vec![
            SqlValue::Integer(1),
            receipt_before[0][0].clone(),
            receipt_before[0][1].clone(),
            SqlValue::Integer(f.rt.now.get())
        ]]
    );
}

#[test]
fn admission_is_verifier_only_short_lived_and_consumed_retries_never_mint_cookies() {
    use brews_contracts::games::{GameResponse, JoinPlayer};
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let context = f.service().admission_context(&code, None, None).unwrap();
    let context_cookie = context.cookie.unwrap();
    let raw = context_cookie.token().to_owned();
    assert_eq!(context_cookie.expires_at(), f.rt.now.get() + 900_000);
    let reused = f
        .service()
        .admission_context(&code, Some(&raw), None)
        .unwrap();
    assert!(reused.cookie.is_none());
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_players", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    let command = f.command(20);
    let input = || JoinPlayer {
        game_code: code.clone(),
        alias: "  Alice  ".into(),
        recovery_answer: None,
    };
    let joined = f
        .service()
        .join_player(command, &raw, None, input())
        .unwrap();
    let player_cookie = joined.cookie.unwrap();
    let expires = f.rt.now.get() + 86_400_000;
    assert_eq!(player_cookie.expires_at(), expires);
    assert!(
        matches!(joined.response,GameResponse::PlayerJoined{alias,view_revision:0,..} if alias=="Alice")
    );
    assert!(
        f.service()
            .join_player(command, &raw, None, input())
            .unwrap()
            .cookie
            .is_none()
    );
    f.rt.now.set(f.rt.now.get() + 900_000);
    let replay = f
        .service()
        .join_player(command, &raw, None, input())
        .unwrap();
    assert!(matches!(replay.response, GameResponse::Committed { .. }));
    assert!(replay.cookie.is_none());
    assert_eq!(
        f.service()
            .join_player(f.command(21), &raw, None, input())
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(
        f.service().admission_context(&code, Some(&raw), None).err(),
        Some(GameError::Conflict)
    );
    let existing = f
        .service()
        .admission_context(&code, None, Some(player_cookie.token()))
        .unwrap();
    assert!(existing.cookie.is_none());
    assert_eq!(
        f.service()
            .join_player(f.command(22), &raw, Some(player_cookie.token()), input())
            .err(),
        Some(GameError::Conflict)
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_players", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert!(
        f.game
            .query("SELECT player_id FROM game_boards", &[])
            .unwrap()
            .is_empty()
    );
    for table in ["game_admission_contexts", "game_sessions"] {
        let rows = f
            .game
            .query(&format!("SELECT token_verifier FROM {table}"), &[])
            .unwrap();
        assert!(matches!(&rows[0][0],SqlValue::Blob(v) if v.len()==32));
    }
}

#[test]
fn join_enrolls_salted_argon2_unicode_answers_with_keyed_normalized_retry_binding() {
    use brews_contracts::games::JoinPlayer;
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let context = f
        .service()
        .admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let input = |answer: &str| {
        JoinPlayer::decode_json(
            serde_json::json!({"game_code":code.as_str(),"alias":"Alice","recovery_answer":answer})
                .to_string()
                .as_bytes(),
        )
        .unwrap()
    };
    let command = f.command(20);
    f.service()
        .join_player(
            command,
            context.token(),
            None,
            input("\u{2003}CAFE\u{301} Straße\u{00a0}"),
        )
        .unwrap();
    let rows = f
        .game
        .query(
            "SELECT answer_verifier,normalization_version FROM game_recovery",
            &[],
        )
        .unwrap();
    let phc = match &rows[0][0] {
        SqlValue::Text(s) => s,
        _ => panic!("No verifier"),
    };
    assert_eq!(rows[0][1], SqlValue::Integer(1));
    assert!(phc.starts_with("$argon2id$v=19$"));
    assert!(brews_backend::security::verify_password("café strasse", phc).unwrap());
    let replay = f
        .service()
        .join_player(command, context.token(), None, input("café STRASSE"))
        .unwrap();
    assert!(replay.cookie.is_none());
    assert_eq!(
        f.service()
            .join_player(command, context.token(), None, input("different"))
            .err(),
        Some(GameError::Conflict)
    );
    let row = f
        .game
        .query(
            "SELECT fingerprint FROM game_receipts WHERE command_id=?",
            &[SqlValue::Text(command.to_string())],
        )
        .unwrap();
    use sha2::{Digest, Sha256};
    assert_ne!(
        row[0][0],
        SqlValue::Blob(Sha256::digest("café strasse".as_bytes()).to_vec())
    );
    let context2 = f
        .service()
        .admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let mut second = input("café STRASSE");
    second.alias = "Bob".into();
    f.service()
        .join_player(f.command(21), context2.token(), None, second)
        .unwrap();
    let all = f
        .game
        .query(
            "SELECT answer_verifier FROM game_recovery ORDER BY player_id",
            &[],
        )
        .unwrap();
    assert_ne!(all[0][0], all[1][0]);
}

#[test]
fn player_connections_are_durable_fenced_and_presence_changes_only_host_views() {
    use brews_contracts::games::{GameView, JoinPlayer};
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let ctx = f
        .service()
        .admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let joined = f
        .service()
        .join_player(
            f.command(20),
            ctx.token(),
            None,
            JoinPlayer {
                game_code: code.clone(),
                alias: "Alice".into(),
                recovery_answer: None,
            },
        )
        .unwrap();
    let token = joined.cookie.unwrap();
    let first = f
        .service()
        .prepare_player_connection(token.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let before = f
        .service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap()
        .view_revision();
    assert!(
        f.game
            .query("SELECT connection_id FROM game_current_connections", &[])
            .unwrap()
            .is_empty()
    );
    f.service().accept_player_connection(&first).unwrap();
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[first.connection_id()],
        )
        .unwrap();
    assert_eq!(host.view_revision(), before + 1);
    assert!(matches!(
        host,
        GameView::Host {
            connected_player_count: 1,
            ..
        }
    ));
    let player = f.service().player_view(token.token()).unwrap();
    assert_eq!(player.view_revision(), 0);
    let second = f
        .service()
        .prepare_player_connection(token.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&second).unwrap();
    f.service().disconnect(&first).unwrap();
    assert_eq!(
        f.game
            .query("SELECT connection_id FROM game_current_connections", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(second.connection_id().to_string())]]
    );
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &[second.connection_id()],
        )
        .unwrap();
    assert_eq!(host.view_revision(), before + 1);
    f.service().disconnect(&second).unwrap();
    let host = f
        .service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap();
    assert_eq!(host.view_revision(), before + 2);
    assert!(matches!(
        f.service().player_view(token.token()).unwrap(),
        GameView::Player {
            view_revision: 0,
            board: None,
            ..
        }
    ));
    assert_eq!(
        f.game
            .query("SELECT expires_at FROM game_sessions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(token.expires_at())]]
    );
}

#[test]
fn start_counts_observed_valid_players_and_assigns_every_retained_member_atomically() {
    use brews_contracts::games::{GameResponse, GameView};
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let a = f.player(&code, 20, "Alice");
    let b = f.player(&code, 21, "Bob");
    let c = f.player(&code, 22, "Offline");
    let ca = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let cb = f
        .service()
        .prepare_player_connection(b.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&ca).unwrap();
    f.service().accept_player_connection(&cb).unwrap();
    let observed = [ca.connection_id(), cb.connection_id()];
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let expected = view.view_revision();
    let game = view.game_id();
    let reservation = f.directory().confirm_reservation(game).unwrap();
    let command = f.command(50);
    let started = f
        .service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            expected,
            &reservation,
            &observed,
        )
        .unwrap();
    assert!(matches!(started,GameResponse::Started{view_revision,..} if view_revision==expected+1));
    assert_eq!(
        f.game
            .query("SELECT state,idle_due,revision FROM game_record", &[])
            .unwrap(),
        vec![vec![
            SqlValue::Text("in_progress".into()),
            SqlValue::Null,
            SqlValue::Integer(2)
        ]]
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_boards", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(3)]]
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_board_cells", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(75)]]
    );
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    assert!(
        matches!(host,GameView::Host{ref players,connected_player_count:2,..} if players.len()==3 && players.iter().all(|p|p.board.is_some()))
    );
    let player = f.service().player_view(c.token()).unwrap();
    assert_eq!(player.view_revision(), 1);
    let raw = serde_json::to_string(&player).unwrap();
    assert!(!raw.contains("Alice") && !raw.contains("Bob") && !raw.contains("players"));
    GameView::decode_json(raw.as_bytes()).unwrap();
    assert!(matches!(
        f.service()
            .start(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                expected,
                &reservation,
                &observed
            )
            .unwrap(),
        GameResponse::Committed { .. }
    ));
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_boards", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(3)]]
    );
}

#[test]
fn exact_idle_deadline_cancels_deletes_prestart_data_and_requires_release_ack() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let player = f.player(&code, 20, "Alice");
    f.service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap();
    let due = f.rt.now.get() + 86_400_000;
    f.rt.now.set(due);
    let terminal = f.service().cleanup().unwrap().unwrap();
    assert_eq!(
        terminal.projection().state(),
        brews_domain::games::GameState::Cancelled
    );
    for table in [
        "game_players",
        "game_sessions",
        "game_admission_contexts",
        "game_configuration",
        "game_free_cells",
        "game_recovery",
        "game_connection_grants",
    ] {
        assert_eq!(
            f.game
                .query(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(0)]]
        );
    }
    assert_eq!(
        f.game
            .query("SELECT view_key,revision FROM game_view_revisions", &[])
            .unwrap(),
        Vec::<brews_backend::db::Row>::new()
    );
    assert_eq!(
        f.service().player_view(player.token()).err(),
        Some(GameError::NotFound)
    );
    assert!(
        f.directory()
            .confirm_reservation(terminal.projection().game_id())
            .is_ok()
    );
    let pending = f.service().pending_work(1).unwrap().remove(0);
    assert_eq!(pending.kind(), brews_backend::game::WorkKind::Release);
    let ack = f.directory().release_game(terminal).unwrap();
    f.service().acknowledge_release(&pending, ack).unwrap();
    assert!(f.service().pending_work(1).unwrap().is_empty());
    assert!(f.directory().lookup_game_code(&code).unwrap().is_none());
    assert!(f.service().cleanup().unwrap().is_some());
}

#[test]
fn recovery_deadlines_backoff_and_absent_initializer_sealing_survive_clock_rollback() {
    let f = Fixture::new();
    let config = GameConfiguration::default();
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    assert_eq!(f.service().next_deadline().unwrap(), None);
    f.rt.now.set(work.deadline());
    let terminal = f.service().seal_abandoned_creation(&work).unwrap();
    assert_eq!(terminal.projection().game_id(), work.game_id());
    let pending = f.service().pending_work(1).unwrap().remove(0);
    assert_eq!(
        f.service().next_deadline().unwrap(),
        Some(f.rt.now.get() + 1_000)
    );
    let retry = f.service().retry_work(&pending).unwrap();
    assert_eq!(retry.attempt_count(), 1);
    assert_eq!(retry.next_attempt_at(), f.rt.now.get() + 2_000);
    assert_eq!(
        f.service().retry_work(&pending).err(),
        Some(GameError::Conflict)
    );
    f.rt.now.set(f.rt.now.get() - 1);
    assert_eq!(f.service().next_deadline().err(), Some(GameError::Storage));
    f.rt.now.set(work.deadline());
    assert!(
        f.game
            .query("SELECT singleton FROM game_configuration", &[])
            .unwrap()
            .is_empty()
    );
    let ack = f.directory().release_game(terminal).unwrap();
    f.service().acknowledge_release(&retry, ack).unwrap();
    assert_eq!(f.service().next_deadline().unwrap(), None);
}

#[test]
fn start_faults_never_leave_boards_receipts_or_in_progress_state() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let a = f.player(&code, 20, "Alice");
    let b = f.player(&code, 21, "Bob");
    let ca = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let cb = f
        .service()
        .prepare_player_connection(b.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&ca).unwrap();
    f.service().accept_player_connection(&cb).unwrap();
    let observed = [ca.connection_id(), cb.connection_id()];
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let reservation = f.directory().confirm_reservation(host.game_id()).unwrap();
    let command = f.command(50);
    let start = || {
        f.service().start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            command,
            host.view_revision(),
            &reservation,
            &observed,
        )
    };
    f.rt.fail.set(true);
    assert_eq!(start().err(), Some(GameError::RandomUnavailable));
    f.rt.fail.set(false);
    for trigger in [
        "CREATE TRIGGER board_fault BEFORE INSERT ON game_board_cells BEGIN SELECT RAISE(ABORT,'fault'); END",
        "CREATE TRIGGER board_fault BEFORE INSERT ON game_board_cells BEGIN SELECT RAISE(IGNORE); END",
    ] {
        f.game.execute(trigger, &[]).unwrap();
        assert_eq!(start().err(), Some(GameError::Storage));
        f.game.execute("DROP TRIGGER board_fault", &[]).unwrap();
        assert_eq!(
            f.game.query("SELECT state FROM game_record", &[]).unwrap(),
            vec![vec![SqlValue::Text("awaiting_players".into())]]
        );
        assert_eq!(
            f.game
                .query("SELECT count(*) FROM game_boards", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(0)]]
        );
        assert!(
            f.game
                .query(
                    "SELECT command_id FROM game_receipts WHERE command_id=?",
                    &[SqlValue::Text(command.to_string())]
                )
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        f.service()
            .start(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                host.view_revision() + 1,
                &reservation,
                &observed
            )
            .err(),
        Some(GameError::StaleRevision)
    );
    start().unwrap();
}

#[test]
fn unpublished_lobby_context_capacity_and_alias_failure_paths_are_real_sqlite() {
    use brews_backend::game::LobbyPreparation;
    let f = Fixture::new();
    let config = GameConfiguration::default();
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let created = f
        .service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    let ack = f.directory().acknowledge_creation(created.ready).unwrap();
    let w = match f
        .service()
        .prepare_lobby(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(11),
            0,
        )
        .unwrap()
    {
        LobbyPreparation::Ready(w) => w,
        _ => panic!("No intent"),
    };
    let grant = f.directory().allocate_game_code(ack).unwrap();
    f.service()
        .commit_lobby(
            &w,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &grant,
        )
        .unwrap();
    assert_eq!(
        f.service()
            .admission_context(grant.game_code(), None, None)
            .err(),
        Some(GameError::Conflict)
    );
    let view = f
        .service()
        .account_view(f.auth().authorize_game_account(&f.token).unwrap(), &[])
        .unwrap();
    assert!(matches!(
        view,
        brews_contracts::games::GameView::Host {
            game_code: None,
            ..
        }
    ));
    let ack = f
        .directory()
        .publish_game(f.service().directory_projection().unwrap())
        .unwrap();
    let pending = f.service().pending_work(1).unwrap().remove(0);
    f.game.execute("CREATE TRIGGER ack_ignore BEFORE DELETE ON game_pending_work BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(
        f.service().acknowledge_projection(&pending, ack).err(),
        Some(GameError::Storage)
    );
    f.game.execute("DROP TRIGGER ack_ignore", &[]).unwrap();
    f.service().acknowledge_projection(&pending, ack).unwrap();
    for _ in 0..100 {
        f.service()
            .admission_context(grant.game_code(), None, None)
            .unwrap();
    }
    assert_eq!(
        f.service()
            .admission_context(grant.game_code(), None, None)
            .err(),
        Some(GameError::Capacity)
    );
    f.rt.now.set(f.rt.now.get() + 900_000);
    assert!(
        f.service()
            .admission_context(grant.game_code(), None, None)
            .unwrap()
            .cookie
            .is_some()
    );
}

#[test]
fn random_gap_preflight_uses_fresh_time_and_start_deadline_gap_commits_cancellation() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    f.rt.advance_random_ms.set(1_000);
    let context = f
        .service()
        .admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    assert_eq!(context.expires_at(), f.rt.now.get() + 900_000);
    let a = f.player(&code, 20, "Alice");
    let b = f.player(&code, 21, "Bob");
    let ca = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let cb = f
        .service()
        .prepare_player_connection(b.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&ca).unwrap();
    f.service().accept_player_connection(&cb).unwrap();
    let observed = [ca.connection_id(), cb.connection_id()];
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let reservation = f.directory().confirm_reservation(host.game_id()).unwrap();
    f.rt.advance_random_ms.set(86_400_000);
    assert_eq!(
        f.service()
            .start(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(50),
                host.view_revision(),
                &reservation,
                &observed
            )
            .err(),
        Some(GameError::Expired)
    );
    assert_eq!(
        f.game.query("SELECT state FROM game_record", &[]).unwrap(),
        vec![vec![SqlValue::Text("cancelled".into())]]
    );
    assert!(
        f.game
            .query("SELECT player_id FROM game_boards", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn expired_connections_and_sessions_cleanup_changes_host_revision_not_private_boards() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let a = f.player(&code, 20, "Alice");
    let b = f.player(&code, 21, "Bob");
    let ca = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let cb = f
        .service()
        .prepare_player_connection(b.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&ca).unwrap();
    f.service().accept_player_connection(&cb).unwrap();
    let observed = [ca.connection_id(), cb.connection_id()];
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let reservation = f.directory().confirm_reservation(host.game_id()).unwrap();
    f.service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(50),
            host.view_revision(),
            &reservation,
            &observed,
        )
        .unwrap();
    let before = f
        .game
        .query(
            "SELECT revision FROM game_view_revisions WHERE view_key='host'",
            &[],
        )
        .unwrap();
    let connection = id(&f.rt, 60).parse().unwrap();
    let game = host.game_id();
    let authority = f
        .auth()
        .register_game_account_connection(&f.token, game, connection)
        .unwrap();
    let grant = f
        .service()
        .prepare_account_connection(authority, connection)
        .unwrap();
    f.service()
        .accept_account_connection(
            &grant,
            f.auth()
                .authorize_game_account_connection(
                    f.account,
                    grant.session_id(),
                    grant.epoch(),
                    game,
                    connection,
                )
                .unwrap(),
        )
        .unwrap();
    f.rt.now.set(a.expires_at());
    assert!(f.service().cleanup().unwrap().is_none());
    assert!(
        f.game
            .query("SELECT session_id FROM game_current_connections", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        f.game
            .query("SELECT session_id FROM game_sessions", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_boards", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
    let prior = match before[0][0] {
        SqlValue::Integer(n) => n,
        _ => panic!("No revision"),
    };
    assert_eq!(
        f.game
            .query(
                "SELECT revision FROM game_view_revisions WHERE view_key='host'",
                &[]
            )
            .unwrap(),
        vec![vec![SqlValue::Integer(prior + 1)]]
    );
    assert_eq!(
        f.game
            .query(
                "SELECT revision FROM game_view_revisions WHERE view_key<>'host'",
                &[]
            )
            .unwrap(),
        vec![vec![SqlValue::Integer(1)], vec![SqlValue::Integer(1)]]
    );
    assert_eq!(
        f.service().player_view(a.token()).err(),
        Some(GameError::Unauthorized)
    );
}

#[test]
fn account_socket_grants_require_fresh_accounts_and_do_not_renew_expiry_or_presence() {
    let f = Fixture::new();
    f.open(&GameConfiguration::default());
    let connection = id(&f.rt, 60).parse().unwrap();
    let game = f.service().directory_projection().unwrap().game_id();
    let proof = f
        .auth()
        .register_game_account_connection(&f.token, game, connection)
        .unwrap();
    let grant = f
        .service()
        .prepare_account_connection(proof, connection)
        .unwrap();
    let before = f
        .game
        .query(
            "SELECT revision FROM game_view_revisions WHERE view_key='host'",
            &[],
        )
        .unwrap();
    f.service()
        .accept_account_connection(
            &grant,
            f.auth()
                .authorize_game_account_connection(
                    f.account,
                    grant.session_id(),
                    grant.epoch(),
                    game,
                    connection,
                )
                .unwrap(),
        )
        .unwrap();
    f.service()
        .account_connection_view(
            &grant,
            f.auth()
                .authorize_game_account_connection(
                    f.account,
                    grant.session_id(),
                    grant.epoch(),
                    game,
                    connection,
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        f.game
            .query(
                "SELECT revision FROM game_view_revisions WHERE view_key='host'",
                &[]
            )
            .unwrap(),
        before
    );
    assert_eq!(
        f.game
            .query(
                "SELECT expires_at FROM game_current_connections WHERE viewer_kind='account'",
                &[]
            )
            .unwrap(),
        vec![vec![SqlValue::Integer(grant.expires_at())]]
    );
    f.auth()
        .execute(
            AuthCommand::Logout {
                token: Some(f.token.clone()),
            },
            RequestContext {
                command_id: None,
                caller_identity: "trusted-fixture".into(),
            },
        )
        .unwrap();
    assert!(
        f.auth()
            .authorize_game_account_connection(
                f.account,
                grant.session_id(),
                grant.epoch(),
                game,
                connection
            )
            .is_err()
    );
    f.rt.now.set(f.rt.now.get() + 1_000);
    let closes = f.auth().list_due_game_socket_closes(100).unwrap();
    assert_eq!(closes.len(), 1);
    f.service().close_account_connection(&closes[0]).unwrap();
    assert!(
        f.game
            .query("SELECT connection_id FROM game_current_connections", &[])
            .unwrap()
            .is_empty()
    );
    f.service().close_account_connection(&closes[0]).unwrap();
    let sync = f.service().sync_player("invalid", None);
    assert_eq!(sync.err(), Some(GameError::Unauthorized));
}

#[test]
fn socket_player_projection_and_sync_reprove_current_session_without_private_gaps() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let a = f.player(&code, 20, "Alice");
    let b = f.player(&code, 21, "Bob");
    let g = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&g).unwrap();
    let current = f
        .service()
        .player_connection(g.connection_id())
        .unwrap()
        .unwrap();
    assert_eq!(current, g);
    let view = f.service().player_connection_view(&g).unwrap();
    assert_eq!(view.view_revision(), 0);
    let sync = f.service().sync_player(a.token(), Some(0)).unwrap();
    assert!(sync.up_to_date && sync.snapshot.is_none());
    sync.encode_json().unwrap();
    let sync = f.service().sync_player(a.token(), None).unwrap();
    assert!(!sync.up_to_date && sync.snapshot.is_some());
    let host = f
        .service()
        .sync_account(
            f.auth().authorize_game_account(&f.token).unwrap(),
            None,
            &[g.connection_id()],
        )
        .unwrap();
    assert!(!host.up_to_date);
    f.game
        .execute(
            "UPDATE game_players SET session_epoch=session_epoch+1 WHERE player_id=?",
            &[SqlValue::Text(g.player_id().to_string())],
        )
        .unwrap();
    assert_eq!(
        f.service().player_connection_view(&g).err(),
        Some(GameError::Unauthorized)
    );
    assert_eq!(
        f.service().player_view(a.token()).err(),
        Some(GameError::Unauthorized)
    );
    f.service()
        .reconcile_connections(&[g.connection_id()])
        .unwrap();
    assert!(
        f.service()
            .player_connection(g.connection_id())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.service().player_view(b.token()).unwrap().view_revision(),
        0
    );
}

#[test]
fn malformed_existing_player_authority_never_falls_back_to_fresh_admission() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let a = f.player(&code, 20, "Alice");
    f.game
        .execute("UPDATE game_sessions SET session_id='malformed'", &[])
        .unwrap();
    assert_eq!(
        f.service()
            .admission_context(&code, None, Some(a.token()))
            .err(),
        Some(GameError::Storage)
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_admission_contexts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn known_start_publication_outlives_receipt_floor_and_late_ack_clears_exact_work() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let a = f.player(&code, 20, "Alice");
    let b = f.player(&code, 21, "Bob");
    let ca = f
        .service()
        .prepare_player_connection(a.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let cb = f
        .service()
        .prepare_player_connection(b.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&ca).unwrap();
    f.service().accept_player_connection(&cb).unwrap();
    let observed = [ca.connection_id(), cb.connection_id()];
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let reservation = f.directory().confirm_reservation(host.game_id()).unwrap();
    f.service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(50),
            host.view_revision(),
            &reservation,
            &observed,
        )
        .unwrap();
    let pending = f.service().pending_work(1).unwrap().remove(0);
    let projection = f.service().directory_projection().unwrap();
    f.rt.now.set(f.rt.now.get() + 86_400_000);
    f.service().cleanup().unwrap();
    assert_eq!(
        f.service().pending_work(1).unwrap()[0].operation_id(),
        pending.operation_id()
    );
    let ack = f.directory().publish_game(projection).unwrap();
    assert_eq!(
        f.service().acknowledge_projection(&pending, ack).err(),
        Some(GameError::StaleCommand)
    );
    assert!(f.service().pending_work(1).unwrap().is_empty());
    f.service().cleanup().unwrap();
    assert_eq!(f.service().next_deadline().unwrap(), None);
}

#[test]
fn actual_sqlite_reopen_preserves_lobby_intent_and_lost_publication_ack() {
    let f = Fixture::new();
    let path = std::path::PathBuf::from(std::env::var("TMPDIR").unwrap())
        .join(format!("game-core-reopen-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    *f.game.conn.borrow_mut() = rusqlite::Connection::open(&path).unwrap();
    f.game
        .conn
        .borrow()
        .execute_batch("PRAGMA foreign_keys=ON")
        .unwrap();
    brews_backend::db::game::migrate_game(&f.game).unwrap();
    let config = GameConfiguration::default();
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let created = f
        .service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    let ack = f.directory().acknowledge_creation(created.ready).unwrap();
    let intent = match f
        .service()
        .prepare_lobby(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(11),
            0,
        )
        .unwrap()
    {
        brews_backend::game::LobbyPreparation::Ready(w) => w,
        _ => panic!("No intent"),
    };
    let grant = f.directory().allocate_game_code(ack).unwrap();
    f.service()
        .commit_lobby(
            &intent,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &grant,
        )
        .unwrap();
    let pending = f.service().pending_work(1).unwrap().remove(0);
    let ack = f
        .directory()
        .publish_game(f.service().directory_projection().unwrap())
        .unwrap();
    *f.game.conn.borrow_mut() = rusqlite::Connection::open_in_memory().unwrap();
    *f.game.conn.borrow_mut() = rusqlite::Connection::open(&path).unwrap();
    f.game
        .conn
        .borrow()
        .execute_batch("PRAGMA foreign_keys=ON")
        .unwrap();
    assert_eq!(f.service().pending_work(1).unwrap(), vec![pending.clone()]);
    f.service().acknowledge_projection(&pending, ack).unwrap();
    assert!(f.service().pending_work(1).unwrap().is_empty());
    f.rt.now.set(f.rt.now.get() - 1);
    assert_eq!(f.service().next_deadline().err(), Some(GameError::Storage));
    drop(f);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn delivery_inventory_is_exact_complete_group_or_none_and_checked_each_transaction() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    brews_backend::db::game::migrate_game(&db).unwrap();
    db.execute("CREATE TABLE game_delivery_metadata(value INTEGER)", &[])
        .unwrap();
    assert!(GameService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).is_err());
    db.execute("CREATE TABLE game_delivery_connections(value INTEGER)", &[])
        .unwrap();
    db.execute("CREATE TABLE game_delivery_pending(value INTEGER)", &[])
        .unwrap();
    let service = GameService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    assert!(service.next_deadline().unwrap().is_none());
    db.execute("CREATE TABLE unrelated(value INTEGER)", &[])
        .unwrap();
    assert_eq!(service.next_deadline().err(), Some(GameError::Storage));
}

#[test]
fn prestart_delete_fault_rolls_back_terminal_and_keeps_release_reservation() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let _player = f.player(&code, 20, "Alice");
    f.rt.now.set(f.rt.now.get() + 86_400_000);
    f.game.execute("CREATE TRIGGER purge_ignore BEFORE DELETE ON game_players BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    assert_eq!(f.service().cleanup().err(), Some(GameError::Storage));
    assert_eq!(
        f.game.query("SELECT state FROM game_record", &[]).unwrap(),
        vec![vec![SqlValue::Text("awaiting_players".into())]]
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_players", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    f.game.execute("DROP TRIGGER purge_ignore", &[]).unwrap();
    f.service().cleanup().unwrap();
}

#[test]
fn whole_start_candidate_budget_all_free_feasibility_and_initial_qualification() {
    use brews_domain::games::CellPosition;
    for mode in 0..3 {
        let f = Fixture::new();
        let mut config = GameConfiguration {
            board_side_length: 2,
            free_cell_positions: vec![
                CellPosition { row: 1, column: 1 },
                CellPosition { row: 1, column: 2 },
            ],
            numeric_upper_bound: 3,
            ..GameConfiguration::default()
        };
        if mode == 0 {
            config.free_cell_positions.extend([
                CellPosition { row: 2, column: 1 },
                CellPosition { row: 2, column: 2 },
            ]);
        }
        let code = f.open(&config);
        let a = f.player(&code, 20, "Alice");
        let b = f.player(&code, 21, "Bob");
        let ca = f
            .service()
            .prepare_player_connection(a.token(), id(&f.rt, 40).parse().unwrap())
            .unwrap();
        let cb = f
            .service()
            .prepare_player_connection(b.token(), id(&f.rt, 41).parse().unwrap())
            .unwrap();
        f.service().accept_player_connection(&ca).unwrap();
        f.service().accept_player_connection(&cb).unwrap();
        let observed = [ca.connection_id(), cb.connection_id()];
        let host = f
            .service()
            .account_view(
                f.auth().authorize_game_account(&f.token).unwrap(),
                &observed,
            )
            .unwrap();
        let reservation = f.directory().confirm_reservation(host.game_id()).unwrap();
        if mode == 0 {
            assert_eq!(
                f.service()
                    .start(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        f.command(50),
                        host.view_revision(),
                        &reservation,
                        &observed
                    )
                    .err(),
                Some(GameError::Infeasible)
            );
        } else if mode == 1 {
            struct Repeated<'a>(&'a TestRuntime, std::cell::Cell<usize>);
            impl brews_backend::auth::Runtime for Repeated<'_> {
                fn now_ms(&self) -> i64 {
                    self.0.now.get()
                }
                fn fill_random(
                    &self,
                    bytes: &mut [u8],
                ) -> Result<(), brews_backend::auth::AuthError> {
                    assert_eq!(bytes.len(), 4);
                    self.1.set(self.1.get() + 1);
                    bytes.fill(7);
                    Ok(())
                }
            }
            let zero = Repeated(&f.rt, std::cell::Cell::new(0));
            let service =
                GameService::new(&f.game, &zero, AuthPolicy::default(), &[9; 32]).unwrap();
            assert_eq!(
                service
                    .start(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        f.command(50),
                        host.view_revision(),
                        &reservation,
                        &observed
                    )
                    .err(),
                Some(GameError::GenerationExhausted)
            );
            assert_eq!(zero.1.get(), 2048);
        } else {
            f.service()
                .start(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(50),
                    host.view_revision(),
                    &reservation,
                    &observed,
                )
                .unwrap();
            assert!(
                matches!(f.service().player_view(a.token()).unwrap(),brews_contracts::games::GameView::Player{board:Some(ref board),..} if board.qualified && board.qualifying_lines.contains(&brews_domain::games::CompletedLine::Row(1)))
            );
        }
        if mode < 2 {
            assert!(
                f.game
                    .query("SELECT player_id FROM game_boards", &[])
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                f.game.query("SELECT state FROM game_record", &[]).unwrap(),
                vec![vec![SqlValue::Text("awaiting_players".into())]]
            );
        }
    }
}

#[test]
fn normalized_storage_rejects_unsafe_session_epochs_and_deadlines() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let _a = f.player(&code, 20, "Alice");
    let too_large = 9_007_199_254_740_992i64;
    for sql in [
        "UPDATE game_sessions SET session_epoch=?",
        "UPDATE game_sessions SET expires_at=?",
        "UPDATE game_record SET idle_due=?",
    ] {
        assert!(
            f.game
                .execute(sql, &[SqlValue::Integer(too_large)])
                .is_err(),
            "Unsafe persisted authority must fail SQLite checks"
        );
    }
}

#[test]
fn command_receipts_use_variant_scoped_account_and_player_actor_namespaces() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let _player = f.player(&code, 20, "Alice");
    let actors = f
        .game
        .query("SELECT actor FROM game_receipts ORDER BY actor", &[])
        .unwrap();
    assert_eq!(actors.len(), 3);
    assert!(actors.iter().all(|row|matches!(&row[0],SqlValue::Text(s) if s.starts_with("account:") || s.starts_with("player:"))));
}

#[test]
fn answer_kdf_runs_outside_sqlite_write_and_rechecks_real_competing_admission() {
    let f = Fixture::new();
    let config = GameConfiguration {
        player_capacity: 2,
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let _alice = f.player(&code, 20, "Alice");
    let context = f
        .service()
        .admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    let competing = f
        .service()
        .admission_context(&code, None, None)
        .unwrap()
        .cookie
        .unwrap();
    struct Outside<'a> {
        rt: &'a TestRuntime,
        db: &'a Sqlite,
        hook: std::cell::RefCell<Option<Box<dyn FnOnce() + 'a>>>,
    }
    impl brews_backend::auth::Runtime for Outside<'_> {
        fn now_ms(&self) -> i64 {
            self.rt.now.get()
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), brews_backend::auth::AuthError> {
            assert!(
                self.db.conn.borrow().is_autocommit(),
                "Join KDF/credential entropy must not hold write transaction"
            );
            brews_backend::auth::Runtime::fill_random(self.rt, bytes)?;
            if bytes.len() == 16
                && let Some(hook) = self.hook.borrow_mut().take()
            {
                hook();
            }
            Ok(())
        }
    }
    let runtime = Outside {
        rt: &f.rt,
        db: &f.game,
        hook: std::cell::RefCell::new(Some(Box::new(|| {
            f.service()
                .join_player(
                    f.command(22),
                    competing.token(),
                    None,
                    brews_contracts::games::JoinPlayer {
                        game_code: code.clone(),
                        alias: "Charlie".into(),
                        recovery_answer: None,
                    },
                )
                .unwrap();
        }))),
    };
    let service = GameService::new(&f.game, &runtime, AuthPolicy::default(), &[9; 32]).unwrap();
    let input = brews_contracts::games::JoinPlayer::decode_json(
        serde_json::json!({"game_code":code.as_str(),"alias":"Bob","recovery_answer":"Straße"})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        service
            .join_player(f.command(21), context.token(), None, input)
            .err(),
        Some(GameError::Capacity)
    );
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_players", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
    assert!(
        f.game
            .query("SELECT answer_verifier FROM game_recovery", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        f.game
            .query("SELECT player_id FROM game_players WHERE alias='Bob'", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn creation_configuration_set_order_is_canonical_without_a_blob_authority() {
    use brews_domain::games::CellPosition;
    let f = Fixture::new();
    let config = GameConfiguration {
        free_cell_positions: vec![
            CellPosition { row: 3, column: 3 },
            CellPosition { row: 1, column: 1 },
        ],
        ..GameConfiguration::default()
    };
    let mut reordered = config.clone();
    reordered.free_cell_positions.reverse();
    assert_eq!(
        creation_fingerprint(&config).unwrap(),
        creation_fingerprint(&reordered).unwrap()
    );
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let created = f
        .service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    f.directory().acknowledge_creation(created.ready).unwrap();
    f.service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &reordered,
        )
        .unwrap();
}

#[test]
fn ignored_configuration_insert_rolls_back_creation_and_cannot_leave_partial_identity() {
    let f = Fixture::new();
    let config = GameConfiguration::default();
    let work = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    f.game.execute("CREATE TRIGGER config_ignore BEFORE INSERT ON game_configuration BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
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
    assert!(
        f.game
            .query("SELECT game_id FROM game_record", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        f.game
            .query("SELECT command_id FROM game_receipts", &[])
            .unwrap()
            .is_empty()
    );
    f.game.execute("DROP TRIGGER config_ignore", &[]).unwrap();
    f.service()
        .initialize(
            &work,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
}

#[test]
fn ignored_session_or_answer_insert_rolls_back_join_and_leaves_context_unconsumed() {
    for table in ["game_sessions", "game_recovery"] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let context = f
            .service()
            .admission_context(&code, None, None)
            .unwrap()
            .cookie
            .unwrap();
        f.game.execute(&format!("CREATE TRIGGER join_ignore BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END"),&[]).unwrap();
        let input=brews_contracts::games::JoinPlayer::decode_json(serde_json::json!({"game_code":code.as_str(),"alias":"Alice","recovery_answer":"Straße"}).to_string().as_bytes()).unwrap();
        assert_eq!(
            f.service()
                .join_player(f.command(20), context.token(), None, input)
                .err(),
            Some(GameError::Storage)
        );
        assert!(
            f.game
                .query("SELECT player_id FROM game_players", &[])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            f.game
                .query("SELECT consumed_command FROM game_admission_contexts", &[])
                .unwrap(),
            vec![vec![SqlValue::Null]]
        );
    }
}

#[test]
fn other_hosts_can_read_but_only_designated_host_or_admin_mutate_without_admin_idle_renewal() {
    fn second_actor(f: &Fixture, role: AccountRole) -> String {
        let created = f
            .auth()
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: format!("SecondGame{:?}Person", role),
                    role,
                },
                ManagementPrincipal::DeveloperCli,
                Some(f.command(100)),
                "https://app.example.test",
            )
            .unwrap();
        let link = match created {
            ManagementResponse::Issued { url, .. } => url.rsplit('#').next().unwrap().to_owned(),
            _ => panic!("No enrollment"),
        };
        let ctx = |n| RequestContext {
            command_id: Some(id(&f.rt, n)),
            caller_identity: "trusted-fixture".into(),
        };
        let restricted = cookie(
            f.auth()
                .execute(
                    AuthCommand::Redeem {
                        purpose: AccessLinkPurpose::Enrollment,
                        token: link,
                    },
                    ctx(101),
                )
                .unwrap(),
        );
        cookie(
            f.auth()
                .execute(
                    AuthCommand::Complete {
                        scope: SessionScope::EnrollmentOnly,
                        token: restricted,
                        new_password: brews_backend::security::new_token(&f.rt).unwrap(),
                    },
                    ctx(102),
                )
                .unwrap(),
        )
    }
    for role in [AccountRole::Host, AccountRole::Admin] {
        let f = Fixture::new();
        let second = second_actor(&f, role);
        let config = GameConfiguration::default();
        let work = f
            .directory()
            .claim_game(
                f.account,
                f.command(10),
                creation_fingerprint(&config).unwrap(),
            )
            .unwrap();
        let created = f
            .service()
            .initialize(
                &work,
                f.auth().authorize_game_account(&f.token).unwrap(),
                &config,
            )
            .unwrap();
        let ack = f.directory().acknowledge_creation(created.ready).unwrap();
        let original_due = work.created_at() + 86_400_000;
        f.rt.now.set(f.rt.now.get() + 1_000);
        f.service()
            .account_view(f.auth().authorize_game_account(&second).unwrap(), &[])
            .unwrap();
        let prepare = f.service().prepare_lobby(
            f.auth().authorize_game_account(&second).unwrap(),
            f.command(11),
            0,
        );
        if role == AccountRole::Host {
            assert_eq!(prepare.err(), Some(GameError::Forbidden));
        } else {
            let w = match prepare.unwrap() {
                brews_backend::game::LobbyPreparation::Ready(w) => w,
                _ => panic!("No intent"),
            };
            let grant = f.directory().allocate_game_code(ack).unwrap();
            f.service()
                .commit_lobby(
                    &w,
                    f.auth().authorize_game_account(&second).unwrap(),
                    &grant,
                )
                .unwrap();
            assert_eq!(
                f.game
                    .query("SELECT idle_due,last_host_activity FROM game_record", &[])
                    .unwrap(),
                vec![vec![
                    SqlValue::Integer(original_due),
                    SqlValue::Integer(work.created_at())
                ]]
            );
        }
    }
}

#[test]
fn fresh_game_schema_is_normalized_and_rejects_unrelated_owner_tables() {
    use brews_backend::db::schema::game_schema::{GAME_SCHEMA_VERSION, GAME_TABLES};

    let db = Sqlite::new();
    brews_backend::db::game::migrate_game(&db).unwrap();
    let tables = db
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
            &[],
        )
        .unwrap();
    let expected_tables = GAME_TABLES
        .iter()
        .map(|name| vec![SqlValue::Text((*name).into())])
        .collect::<Vec<_>>();
    assert_eq!(tables, expected_tables);
    assert_eq!(
        db.query("SELECT schema_version FROM game_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(GAME_SCHEMA_VERSION)]]
    );
    brews_backend::db::game::migrate_game(&db).unwrap();
    db.execute("CREATE TABLE sqliteXsecret(value TEXT)", &[])
        .unwrap();
    assert!(brews_backend::db::game::migrate_game(&db).is_err());
    let other = Sqlite::new();
    other
        .execute("CREATE TABLE accounts(value TEXT)", &[])
        .unwrap();
    assert!(brews_backend::db::game::migrate_game(&other).is_err());
    assert!(
        other
            .query(
                "SELECT name FROM sqlite_master WHERE name='game_metadata'",
                &[]
            )
            .unwrap()
            .is_empty()
    );
}
