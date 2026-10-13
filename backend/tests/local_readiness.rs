//! Local wall-time and SQLite work observations, never provider capacity certification.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Measurement fixtures fail closed."
)]
#[path = "../src/directory/mod.rs"]
mod directory;
#[path = "../src/db/directory.rs"]
mod directory_db;
#[path = "../src/game/mod.rs"]
#[allow(
    dead_code,
    reason = "Exact production private types preserve module boundaries."
)]
mod game;
#[path = "../src/limits.rs"]
#[allow(dead_code, reason = "Exact production modules share policy constants.")]
mod limits;
mod support;
use auth::{AuthError, AuthPolicy, Runtime};
use brews_backend::{auth, db, security};
use brews_domain::{games::GameState, ids::GameId};
use db::{Database, Row, SqlValue, StorageError};
use rusqlite::{
    StatementStatus,
    types::{Value, ValueRef},
};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    io::Read,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use support::{Sqlite, TestRuntime};

struct MeasuredDb {
    sqlite: Sqlite,
    active: Cell<bool>,
    queries: Cell<usize>,
    samples: RefCell<Vec<Json>>,
}
fn binding(value: &SqlValue) -> Value {
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(n) => Value::Integer(*n),
        SqlValue::Text(s) => Value::Text(s.clone()),
        SqlValue::Blob(b) => Value::Blob(b.clone()),
    }
}
impl Database for MeasuredDb {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        if !self.active.get() {
            return self.sqlite.query(sql, params);
        }
        self.queries.set(self.queries.get() + 1);
        if !sql.contains("ORDER BY ended_at DESC,game_id DESC") {
            return self.sqlite.query(sql, params);
        }
        // The service supplies the exact SQL and parameters; no independent query copy.
        let conn = self.sqlite.conn.borrow();
        let values: Vec<_> = params.iter().map(binding).collect();
        let mut explain = conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .map_err(|_| StorageError)?;
        let plan: Vec<String> = explain
            .query_map(rusqlite::params_from_iter(&values), |r| r.get(3))
            .map_err(|_| StorageError)?
            .collect::<Result<_, _>>()
            .map_err(|_| StorageError)?;
        let mut statement = conn.prepare(sql).map_err(|_| StorageError)?;
        let count = statement.column_count();
        let start = Instant::now();
        let mut cursor = statement
            .query(rusqlite::params_from_iter(&values))
            .map_err(|_| StorageError)?;
        let mut out = Vec::new();
        while let Some(row) = cursor.next().map_err(|_| StorageError)? {
            let mut cells = Vec::new();
            for i in 0..count {
                cells.push(match row.get_ref(i).map_err(|_| StorageError)? {
                    ValueRef::Null => SqlValue::Null,
                    ValueRef::Integer(n) => SqlValue::Integer(n),
                    ValueRef::Text(s) => {
                        SqlValue::Text(std::str::from_utf8(s).map_err(|_| StorageError)?.to_owned())
                    }
                    ValueRef::Blob(b) => SqlValue::Blob(b.to_vec()),
                    ValueRef::Real(_) => return Err(StorageError),
                });
            }
            out.push(cells);
        }
        drop(cursor);
        let elapsed = start.elapsed().as_nanos();
        let vm = statement.get_status(StatementStatus::VmStep);
        let sort = statement.get_status(StatementStatus::Sort);
        let scan = statement.get_status(StatementStatus::FullscanStep);
        if vm <= 0 || sort < 0 || scan < 0 || elapsed == 0 || plan.is_empty() {
            return Err(StorageError);
        }
        self.samples.borrow_mut().push(json!({"sql":sql,"parameter_count":params.len(),"result_count":out.len(),"query_wall_ns":elapsed,"vm_steps":vm,"sort_count":sort,"fullscan_steps":scan,"query_plan":plan}));
        Ok(out)
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        self.sqlite.execute(sql, params)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.sqlite.transaction(operation)
    }
}
struct OsRuntime;
impl Runtime for OsRuntime {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(bytes))
            .map_err(|_| AuthError::Crypto)
    }
}
fn timed<T>(operation: impl FnOnce() -> T) -> (T, u128) {
    let start = Instant::now();
    let result = operation();
    let ns = start.elapsed().as_nanos();
    assert!(ns > 0);
    (result, ns)
}
fn history_measurements() -> Vec<Json> {
    let mut measurements = Vec::new();
    for population in [100, 1_000, 10_000] {
        let db = MeasuredDb {
            sqlite: Sqlite::new(),
            active: Cell::new(false),
            queries: Cell::new(0),
            samples: RefCell::new(Vec::new()),
        };
        directory_db::migrate_directory(&db).unwrap();
        let rt = TestRuntime::new();
        let now = rt.now.get();
        let mut model = Vec::new();
        let mut mix = [0; 4];
        db.transaction(|| {
            for n in 0..population {
                let id: GameId = format!("01890f3e-53b7-7d28-9b05-{n:012x}").parse().unwrap();
                let state = if n % 2 == 0 { GameState::Resolved } else { GameState::Cancelled };
                let ended = now - 10_000 + i64::from(n / 3); // Deliberate tie groups exercise UUID ordering.
                let kind = n % 10;
                let started = if kind == 0 { SqlValue::Null } else { SqlValue::Integer(now - 20_000) };
                let expiry = match kind { 0 => SqlValue::Null, 1 => SqlValue::Integer(now - 1), 2 => SqlValue::Integer(now), _ => SqlValue::Integer(now + 10_000) };
                mix[if kind < 3 { kind as usize } else { 3 }] += 1;
                if kind >= 3 { model.push((ended, id, state)); }
                db.execute("INSERT INTO directory_game_index(game_id,designated_host_id,fingerprint,state,source_revision,game_code,created_at,started_at,ended_at,history_expires_at,publication_state) VALUES(?,?,?,?,?,?,?,?,?,?,?)", &[
                    SqlValue::Text(id.to_string()), SqlValue::Text(format!("01890f3e-53b7-7d28-8b05-{:012x}",n%7)), SqlValue::Blob(vec![7;32]),
                    SqlValue::Text(if kind == 0 { GameState::Cancelled } else { state }.to_string()), SqlValue::Integer(3), SqlValue::Text(format!("G{n:07}")),
                    SqlValue::Integer(now - 30_000), started, SqlValue::Integer(ended), expiry, SqlValue::Text(if n%2==0 {"pending"} else {"published"}.into()),
                ])?;
            }
            Ok(())
        }).unwrap();
        model.sort_by_key(|a| std::cmp::Reverse((a.0, a.1.to_string())));
        let svc = directory_db::DirectoryService::new(&db, &rt).unwrap();
        for outcome in [None, Some(GameState::Resolved), Some(GameState::Cancelled)] {
            let eligible: Vec<_> = model
                .iter()
                .filter(|r| outcome.is_none_or(|s| s == r.2))
                .collect();
            for (page, offset, limit) in [
                ("first", 0, 50),
                ("cursor", 50.min(eligible.len()), 50),
                ("tail", eligible.len() - 1, 50),
                ("single", 0, 1),
            ] {
                let after = if offset == 0 {
                    None
                } else {
                    Some((eligible[offset - 1].0, eligible[offset - 1].1))
                };
                db.active.set(true);
                db.queries.set(0);
                db.samples.borrow_mut().clear();
                let (result, wall_ns) =
                    timed(|| svc.history_indexes(after, limit, outcome).unwrap());
                db.active.set(false);
                let expected: Vec<_> = eligible
                    .iter()
                    .skip(offset)
                    .take(limit as usize + 1)
                    .map(|r| r.1)
                    .collect();
                assert_eq!(
                    result.iter().map(|r| r.game_id()).collect::<Vec<_>>(),
                    expected
                );
                assert!(
                    result
                        .iter()
                        .all(|r| r.started_at().is_some() && r.history_expires_at().unwrap() > now)
                );
                let mut samples = db.samples.borrow_mut();
                assert_eq!(
                    samples.len(),
                    1,
                    "Measurement must intercept exactly one actual History query"
                );
                let mut sample = samples.pop().unwrap();
                sample["population"] = json!(population);
                sample["case"] = json!(page);
                sample["outcome"] = json!(outcome.map(|s| s.to_string()));
                sample["page_limit"] = json!(limit);
                sample["eligible_count"] = json!(eligible.len());
                sample["service_wall_ns"] = json!(wall_ns);
                sample["service_query_count"] = json!(db.queries.get());
                sample["mix_nonstarted_expired_exactexpiry_live"] = json!(mix);
                measurements.push(sample);
            }
        }
    }
    measurements
}

fn fresh_command(runtime: &OsRuntime) -> brews_domain::ids::CommandId {
    let mut bytes = [0; 16];
    runtime.fill_random(&mut bytes).unwrap();
    bytes[..6].copy_from_slice(&runtime.now_ms().to_be_bytes()[2..]);
    bytes[6] = (bytes[6] & 15) | 112;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes).try_into().unwrap()
}
fn recovery_owner(runtime: &OsRuntime, key: &[u8; 32]) -> (Sqlite, brews_domain::games::GameCode) {
    use brews_backend::{
        auth::{AuthCommand, AuthService, CookieEffect, ManagementPrincipal, RequestContext},
        db::{
            directory::{DirectoryService, migrate_directory},
            game::{GameService, migrate_game},
        },
        game::{LobbyPreparation, creation_fingerprint},
    };
    use brews_contracts::management::{ManagementCommand, ManagementResponse};
    use brews_domain::{
        accounts::{AccessLinkPurpose, AccountRole, SessionScope},
        games::GameConfiguration,
    };
    let accounts = Sqlite::new();
    db::migrate(&accounts).unwrap();
    let directory = Sqlite::new();
    migrate_directory(&directory).unwrap();
    let game = Sqlite::new();
    migrate_game(&game).unwrap();
    let auth = AuthService::new(&accounts, runtime, AuthPolicy::default(), key).unwrap();
    let issued = auth
        .execute_management(
            ManagementCommand::CreateAccount {
                username: "LocalReadinessHost".into(),
                role: AccountRole::Host,
            },
            ManagementPrincipal::DeveloperCli,
            Some(fresh_command(runtime)),
            "https://localhost:8787",
        )
        .unwrap();
    let (account, link) = match issued {
        ManagementResponse::Issued { receipt, url } => (
            receipt.account_id,
            url.rsplit('#').next().unwrap().to_owned(),
        ),
        _ => panic!("Expected ephemeral enrollment"),
    };
    let context = || RequestContext {
        command_id: Some(fresh_command(runtime).to_string()),
        caller_identity: "local-readiness".into(),
    };
    let cookie = |out: auth::AuthOutcome| match out.cookie {
        CookieEffect::Set { token, .. } => token,
        _ => panic!("Expected fresh cookie"),
    };
    let restricted = cookie(
        auth.execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token: link,
            },
            context(),
        )
        .unwrap(),
    );
    let secret = zeroize::Zeroizing::new(security::new_token(runtime).unwrap());
    let token = zeroize::Zeroizing::new(cookie(
        auth.execute(
            AuthCommand::Complete {
                scope: SessionScope::EnrollmentOnly,
                token: restricted,
                new_password: secret.to_string(),
            },
            context(),
        )
        .unwrap(),
    ));
    let config = GameConfiguration {
        numeric_upper_bound: 4,
        board_side_length: 2,
        free_cells_enabled: false,
        free_cell_positions: vec![],
        player_capacity: 4,
        spectator_capacity: 0,
        ..GameConfiguration::default()
    };
    let svc = GameService::new(&game, runtime, AuthPolicy::default(), key).unwrap();
    let dir = DirectoryService::new(&directory, runtime).unwrap();
    let work = dir
        .claim_game(
            account,
            fresh_command(runtime),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let created = svc
        .initialize(&work, auth.authorize_game_account(&token).unwrap(), &config)
        .unwrap();
    let ack = dir.acknowledge_creation(created.ready).unwrap();
    let intent = match svc
        .prepare_lobby(
            auth.authorize_game_account(&token).unwrap(),
            fresh_command(runtime),
            0,
        )
        .unwrap()
    {
        LobbyPreparation::Ready(work) => work,
        _ => panic!("Expected lobby intent"),
    };
    let code = dir.allocate_game_code(ack).unwrap();
    svc.commit_lobby(&intent, auth.authorize_game_account(&token).unwrap(), &code)
        .unwrap();
    let pending = svc.pending_work(1).unwrap().remove(0);
    let ack = dir
        .publish_game(svc.directory_projection().unwrap())
        .unwrap();
    svc.acknowledge_projection(&pending, ack).unwrap();
    (game, code.game_code().clone())
}

#[test]
fn local_slice9_measurements() {
    let history = history_measurements();
    let runtime = OsRuntime;
    let policy = AuthPolicy::default();
    let secret = zeroize::Zeroizing::new(security::new_token(&runtime).unwrap());
    let (phc, hash_ns) = timed(|| {
        zeroize::Zeroizing::new(security::hash_password(&secret, policy, &runtime).unwrap())
    });
    let (valid, verify_ns) = timed(|| security::verify_password(&secret, &phc).unwrap());
    assert!(valid);
    let other = zeroize::Zeroizing::new(security::new_token(&runtime).unwrap());
    let (invalid, mismatch_ns) = timed(|| security::verify_password(&other, &phc).unwrap());
    assert!(!invalid);
    let mut recovery = Vec::new();
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    runtime.fill_random(key.as_mut()).unwrap();
    let (owner_db, code) = recovery_owner(&runtime, &key);
    let owner = db::game::GameService::new(&owner_db, &runtime, policy, key.as_ref()).unwrap();
    for (case, source, equivalent, different) in [
        (
            "nfc_outer_whitespace_sharp_s",
            "\u{2003}CAFE\u{301} Straße\u{a0}",
            "café STRASSE",
            "cafe strasse",
        ),
        ("greek_sigma", "Σςσ", "σσΣ", "σσ"),
        ("non_turkic_i", "İIı", "i\u{307}iı", "iii"),
        ("internal_whitespace", " A  B! ", "a  b!", "a b!"),
    ] {
        // Random suffixes keep every proof ephemeral; only case labels leave the test.
        let suffix = zeroize::Zeroizing::new(security::new_token(&runtime).unwrap());
        let input = zeroize::Zeroizing::new(format!(
            "\u{2003}{} {}\u{a0}",
            source.trim(),
            suffix.as_str()
        ));
        let equivalent =
            zeroize::Zeroizing::new(format!("{} {}", equivalent.trim(), suffix.as_str()));
        let different =
            zeroize::Zeroizing::new(format!("{} {}", different.trim(), suffix.as_str()));
        let (normalized, normalize_ns) =
            timed(|| game::recovery_answers::normalize(&input).unwrap());
        let (verifier, enroll_ns) =
            timed(|| game::recovery_answers::enroll(&normalized, policy, &runtime).unwrap());
        let (valid, verify_ns) = timed(|| {
            security::verify_password(
                &game::recovery_answers::normalize(&equivalent).unwrap(),
                &verifier,
            )
            .unwrap()
        });
        let (invalid, mismatch_ns) = timed(|| {
            security::verify_password(
                &game::recovery_answers::normalize(&different).unwrap(),
                &verifier,
            )
            .unwrap()
        });
        assert!(valid);
        assert!(!invalid);
        let context = owner
            .admission_context(&code, None, None)
            .unwrap()
            .cookie
            .unwrap();
        let alias = format!("Readiness{}", recovery.len());
        let (joined, route_enroll_ns) = timed(|| {
            owner
                .join_player(
                    fresh_command(&runtime),
                    context.token(),
                    None,
                    brews_contracts::games::JoinPlayer {
                        game_code: code.clone(),
                        alias: alias.clone(),
                        recovery_answer: Some(
                            serde_json::from_value(json!(input.as_str())).unwrap(),
                        ),
                    },
                )
                .unwrap()
        });
        let old = joined.cookie.unwrap();
        let (restored, route_verify_ns) = timed(|| {
            owner
                .recover_player(
                    fresh_command(&runtime),
                    &code,
                    &alias,
                    &equivalent,
                    "192.0.2.41",
                )
                .unwrap()
        });
        let fresh = restored.cookie.unwrap();
        assert!(owner.player_view(old.token()).is_err());
        assert!(owner.recovered_player_view(fresh.token()).is_ok());
        let (denied, route_mismatch_ns) = timed(|| {
            owner
                .recover_player(
                    fresh_command(&runtime),
                    &code,
                    &alias,
                    &different,
                    "192.0.2.41",
                )
                .err()
        });
        assert_eq!(
            denied,
            Some(brews_backend::game::GameError::InvalidRecovery)
        );
        recovery.push(json!({"owner_route":{"enroll_operations":1,"recover_operations":2,"enroll_wall_ns":route_enroll_ns,"recover_wall_ns":route_verify_ns,"mismatch_wall_ns":route_mismatch_ns,"fresh_session_accepted":true,"old_session_denied":true},"case":case,"normalize_operations":3,"enroll_operations":1,"verify_operations":2,"normalize_wall_ns":normalize_ns,"enroll_wall_ns":enroll_ns,"verify_wall_ns":verify_ns,"mismatch_wall_ns":mismatch_ns,"equivalent_accepted":valid,"different_denied":!invalid}));
    }
    let summary = json!({"marker":"LOCAL-SLICE9-MEASUREMENTS","target":"native","profile":if cfg!(debug_assertions){"assertions-enabled"}else{"assertions-disabled"},"arch":std::env::consts::ARCH,"os":std::env::consts::OS,"sqlite_version":rusqlite::version(),"timing":"wall, not provider CPU","history":history,"argon2":{"m_cost_kib":policy.m_cost,"t_cost":policy.t_cost,"p_cost":policy.p_cost,"hash_operations":1,"verify_operations":2,"hash_wall_ns":hash_ns,"verify_wall_ns":verify_ns,"mismatch_wall_ns":mismatch_ns,"rng":"OS /dev/urandom, no fallback"},"recovery":recovery,"unicode":{"nfc":unicode_normalization::UNICODE_VERSION,"casefold":unicode_casefold::UNICODE_VERSION,"rust_whitespace":std::char::UNICODE_VERSION},"pending":["production profile/bounds/concurrency","provider CPU/memory/quotas/headroom","maximum legal game and owner forecast","deployed clock/CSPRNG/Unicode","RUNTIME-FINAL-01","Slice2/6/7 acceptance"]});
    // The host runner captures this safe line; tests never write machine-specific files.
    println!(
        "LOCAL-SLICE9-MEASUREMENTS {}",
        serde_json::to_string(&summary).unwrap()
    );
}
