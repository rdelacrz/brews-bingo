//! Bounded maximum VALUE-cell branch; native logical presence, deterministic TestRuntime.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Bounded measurement fixtures fail fast without printing protected values."
)]
mod support;
use brews_backend::db::{Database, SqlValue};
use brews_backend::db::{Row, StorageError};
use brews_backend::{
    auth::{
        AuthCommand, AuthPolicy, AuthService, CookieEffect, ManagementPrincipal, RequestContext,
    },
    db::{
        directory::{DirectoryService, migrate_directory},
        game::GameService,
        migrate,
    },
    game::creation_fingerprint,
};
use brews_contracts::games::GameResponse;
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::{
    accounts::{AccessLinkPurpose, AccountRole, SessionScope},
    games::{
        BOARD_CELLS_MAX, BOARD_SIDE_MAX, GameConfiguration, GameState, NUMERIC_UPPER_BOUND_MAX,
        PLAYER_CAPACITY_MAX, SPECTATOR_CAPACITY_MAX,
    },
    ids::{AccountId, CommandId, ConnectionId},
};
use serde_json::{Value, json};
use std::{cell::Cell, time::Instant};
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
    accounts: MeasuredDb,
    directory: MeasuredDb,
    game: MeasuredDb,
    rt: TestRuntime,
    account: AccountId,
    token: String,
}
impl Fixture {
    fn new() -> Self {
        let accounts = MeasuredDb::new();
        let directory = MeasuredDb::new();
        let game = MeasuredDb::new();
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
        }
    }
    fn service(&self) -> GameService<'_, MeasuredDb, TestRuntime> {
        GameService::new(&self.game, &self.rt, AuthPolicy::default(), &[9; 32]).unwrap()
    }
    fn auth(&self) -> AuthService<'_, MeasuredDb, TestRuntime> {
        AuthService::new(&self.accounts, &self.rt, AuthPolicy::default(), &[9; 32]).unwrap()
    }
    fn directory(&self) -> DirectoryService<'_, MeasuredDb, TestRuntime> {
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
}

#[derive(Default, Clone, Copy, serde::Serialize)]
struct SqlMetrics {
    query_calls: u64,
    execute_calls: u64,
    materialized_rows: u64,
    changed_rows: u64,
}
struct MeasuredDb {
    inner: Sqlite,
    metrics: Cell<SqlMetrics>,
}
impl MeasuredDb {
    fn new() -> Self {
        Self {
            inner: Sqlite::new(),
            metrics: Cell::new(SqlMetrics::default()),
        }
    }
    fn reset(&self) {
        self.metrics.set(SqlMetrics::default());
    }
    fn snapshot(&self) -> Value {
        let conn = self.inner.conn.borrow();
        let integer = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, u64>(0)).unwrap();
        let pages = integer("PRAGMA page_count");
        let page_size = integer("PRAGMA page_size");
        let free = integer("PRAGMA freelist_count");
        assert!(free <= pages);
        let mut statement = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap();
        let tables = statement
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let counts = tables
            .into_iter()
            .map(|t| {
                assert!(t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
                let n = integer(&format!("SELECT count(*) FROM {t}"));
                (t, json!(n))
            })
            .collect::<serde_json::Map<_, _>>();
        json!({"sqlite_page_image_allocation_bytes":pages*page_size,"page_count":pages,"page_size":page_size,"freelist_count":free,"tables":counts})
    }
}
impl Database for MeasuredDb {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        let mut metrics = self.metrics.get();
        metrics.query_calls += 1;
        let result = self.inner.query(sql, params);
        if let Ok(rows) = &result {
            metrics.materialized_rows += rows.len() as u64;
        }
        self.metrics.set(metrics);
        result
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        let mut metrics = self.metrics.get();
        metrics.execute_calls += 1;
        let before = self.inner.conn.borrow().total_changes();
        let result = self.inner.execute(sql, params);
        if result.is_ok() {
            metrics.changed_rows += self.inner.conn.borrow().total_changes() - before;
        }
        self.metrics.set(metrics);
        result
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.inner.transaction(operation)
    }
}
fn stage(f: &Fixture, name: &str, started: Instant, payloads: Value) -> Value {
    let out = json!({"name":name,"wall_ms":started.elapsed().as_secs_f64()*1000.0,"payload_json_bytes":payloads,
        "sql_application_work":{"accounts":f.accounts.metrics.get(),"directory":f.directory.metrics.get(),"game":f.game.metrics.get()},
        "storage":{"accounts":f.accounts.snapshot(),"directory":f.directory.snapshot(),"game":f.game.snapshot()}});
    f.accounts.reset();
    f.directory.reset();
    f.game.reset();
    out
}
fn bytes(value: &impl serde::Serialize) -> usize {
    serde_json::to_vec(value).unwrap().len()
}
fn table_count(f: &Fixture, table: &str) -> u64 {
    f.game.snapshot()["tables"][table].as_u64().unwrap()
}
#[test]
fn maximum_value_cell_game_resources() {
    let total = Instant::now();
    let setup = Instant::now();
    let f = Fixture::new();
    let mut stages = vec![stage(&f, "auth_and_migrations", setup, json!({}))];
    let configuration = GameConfiguration {
        numeric_upper_bound: NUMERIC_UPPER_BOUND_MAX,
        board_side_length: BOARD_SIDE_MAX,
        free_cells_enabled: false,
        free_cell_positions: vec![],
        player_capacity: PLAYER_CAPACITY_MAX,
        spectator_capacity: SPECTATOR_CAPACITY_MAX,
        ..GameConfiguration::default()
    };
    let lobby = Instant::now();
    let code = f.open(&configuration);
    let mut players = Vec::new();
    let mut observed: Vec<ConnectionId> = Vec::new();
    for n in 0..PLAYER_CAPACITY_MAX {
        let p = f.player(&code, 20 + n, &format!("Player{n:02}"));
        let connection = id(&f.rt, 100 + n).parse().unwrap();
        let grant = f
            .service()
            .prepare_player_connection(p.token(), connection)
            .unwrap();
        f.service().accept_player_connection(&grant).unwrap();
        players.push(p);
        observed.push(connection);
    }
    let mut spectators = Vec::new();
    for n in 0..SPECTATOR_CAPACITY_MAX {
        let context = f
            .service()
            .spectator_admission_context(&code, None, None)
            .unwrap()
            .cookie
            .unwrap();
        let s = f
            .service()
            .join_spectator(f.command(40 + n), context.token(), None, &code)
            .unwrap()
            .participant
            .cookie
            .unwrap();
        let grant = f
            .service()
            .prepare_spectator_connection(s.token(), id(&f.rt, 130 + n).parse().unwrap())
            .unwrap();
        f.service().accept_spectator_connection(&grant).unwrap();
        spectators.push(s);
    }
    assert_eq!(
        table_count(&f, "game_players"),
        u64::from(PLAYER_CAPACITY_MAX)
    );
    assert_eq!(
        table_count(&f, "game_spectators"),
        u64::from(SPECTATOR_CAPACITY_MAX)
    );
    assert_eq!(
        table_count(&f, "game_current_connections"),
        u64::from(PLAYER_CAPACITY_MAX + SPECTATOR_CAPACITY_MAX)
    );
    stages.push(stage(&f, "lobby", lobby, json!({})));
    let start = Instant::now();
    let host = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let game_id = f.service().directory_projection().unwrap().game_id();
    let reservation = f.directory().confirm_reservation(game_id).unwrap();
    let result = f
        .service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(12),
            host.view_revision(),
            &reservation,
            &observed,
        )
        .unwrap();
    assert!(matches!(result, GameResponse::Started { .. }));
    let pending = f.service().pending_work(100).unwrap();
    for work in pending {
        let ack = f
            .directory()
            .publish_game(f.service().directory_projection().unwrap())
            .unwrap();
        f.service().acknowledge_projection(&work, ack).unwrap();
    }
    assert_eq!(
        table_count(&f, "game_boards"),
        u64::from(PLAYER_CAPACITY_MAX)
    );
    assert_eq!(
        table_count(&f, "game_board_cells"),
        u64::from(PLAYER_CAPACITY_MAX) * BOARD_CELLS_MAX as u64
    );
    let payloads = || {
        json!({"host_view":bytes(&f.service().account_view(f.auth().authorize_game_account(&f.token).unwrap(),&observed).unwrap()),
        "player_sync":bytes(&f.service().sync_player(players[0].token(),None).unwrap()),
        "spectator_sync":bytes(&f.service().sync_spectator(spectators[0].token(),None).unwrap().0)})
    };
    stages.push(stage(&f, "started", start, payloads()));
    let calls = Instant::now();
    let mut revision = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap()
        .view_revision();
    for value in 1..=NUMERIC_UPPER_BOUND_MAX {
        assert!(
            total.elapsed().as_secs() < 180,
            "native measurement exceeded bounded duration"
        );
        // Real UUID-v7 commands, never seeded call rows.
        let mut command_bytes = [0; 16];
        brews_backend::auth::Runtime::fill_random(&f.rt, &mut command_bytes).unwrap();
        command_bytes[..6].copy_from_slice(&(f.rt.now.get() as u64).to_be_bytes()[2..]);
        command_bytes[6] = (command_bytes[6] & 15) | 0x70;
        command_bytes[8] = (command_bytes[8] & 63) | 0x80;
        let command = uuid::Uuid::from_bytes(command_bytes)
            .to_string()
            .parse()
            .unwrap();
        let accepted = f
            .service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &value.to_string(),
                revision,
            )
            .unwrap();
        assert!(matches!(accepted, GameResponse::CallAccepted { .. }));
        revision = f
            .service()
            .account_view(
                f.auth().authorize_game_account(&f.token).unwrap(),
                &observed,
            )
            .unwrap()
            .view_revision();
    }
    assert_eq!(
        table_count(&f, "game_calls"),
        u64::from(NUMERIC_UPPER_BOUND_MAX)
    );
    stages.push(stage(&f, "called", calls, payloads()));
    let end = Instant::now();
    let ended = f
        .service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(99),
            GameState::InProgress,
            true,
        )
        .unwrap();
    assert!(matches!(
        ended,
        GameResponse::Terminalized {
            history_available: true,
            winner: None,
            ..
        }
    ));
    let notices = f.service().begin_spectator_terminal_attempt().unwrap();
    assert_eq!(notices.len(), usize::from(SPECTATOR_CAPACITY_MAX));
    f.service().finish_spectator_terminal_attempt().unwrap();
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
    assert!(f.directory().confirm_reservation(game_id).is_err());
    let history = f
        .service()
        .history(f.auth().authorize_game_account(&f.token).unwrap(), game_id)
        .unwrap();
    assert_eq!(
        table_count(&f, "game_history_board_cells"),
        u64::from(PLAYER_CAPACITY_MAX) * BOARD_CELLS_MAX as u64
    );
    assert_eq!(
        table_count(&f, "game_history_calls"),
        u64::from(NUMERIC_UPPER_BOUND_MAX)
    );
    assert_eq!(table_count(&f, "game_spectators"), 0);
    stages.push(stage(
        &f,
        "history_cleanup_release",
        end,
        json!({"history":bytes(&history)}),
    ));
    let report = json!({"target":"native_sqlite","configuration":{"branch":"maximum_numeric_VALUE_cells_no_free_cells","numeric_pool":NUMERIC_UPPER_BOUND_MAX,"board_side":BOARD_SIDE_MAX,"players":PLAYER_CAPACITY_MAX,"spectators":SPECTATOR_CAPACITY_MAX},"sample":{"manual_calls":NUMERIC_UPPER_BOUND_MAX,"logical_connections":PLAYER_CAPACITY_MAX+SPECTATOR_CAPACITY_MAX,"socket_load":0,"entropy":"deterministic_TestRuntime","wall_ms":total.elapsed().as_secs_f64()*1000.0},"stages":stages,"limits_pending":["OS_entropy","actual_WSS","other_configuration_branches","worst_case_workload","deployed_capacity","owner_forecast","traffic_concurrency","provider_quota_headroom","RUNTIME-FINAL-01"],"metric_scope":"SQLite page image allocation, not logical payload or provider billable rows; changed rows include attempted successful statements, not net committed rows"});
    finite_metrics(&report);
    println!("BREWS-GAME-RESOURCES {report}");
}

fn finite_metrics(value: &Value) {
    match value {
        Value::Number(n) => {
            let n = n.as_f64().unwrap();
            assert!(n.is_finite() && n >= 0.0);
        }
        Value::Array(a) => a.iter().for_each(finite_metrics),
        Value::Object(o) => o.values().for_each(finite_metrics),
        _ => {}
    }
}
