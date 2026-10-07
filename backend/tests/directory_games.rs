#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
#[path = "../src/directory/mod.rs"]
mod directory;
#[path = "../src/db/directory.rs"]
mod directory_db;
#[path = "../src/limits.rs"]
#[allow(dead_code, reason = "Scoped tests share backend constants.")]
mod limits;
mod support;
use brews_backend::{auth, db};
use brews_domain::games::{GameCode, GameState};
use brews_domain::ids::{AccountId, CommandId, OperationId};
use db::{Database, SqlValue};
use directory::games::{CreationFingerprint, CreationReadyProof, GameProjection, TerminalProof};
use directory_db::{DirectoryService, migrate_directory};
use support::{Sqlite, TestRuntime};
fn account(n: u8) -> AccountId {
    format!("01890f3e-53b7-7d28-9b05-4f65092d57{n:02x}")
        .parse()
        .unwrap()
}
fn command(rt: &TestRuntime, n: u8) -> CommandId {
    let mut b = [n; 16];
    b[..6].copy_from_slice(&rt.now.get().to_be_bytes()[2..]);
    b[6] = 0x70;
    b[8] = 0x80;
    uuid::Uuid::from_bytes(b).try_into().unwrap()
}
fn fingerprint() -> CreationFingerprint {
    CreationFingerprint::from_digest([7; 32])
}
fn creation_proof(
    game: brews_domain::ids::GameId,
    account: AccountId,
    command: CommandId,
    fp: CreationFingerprint,
    created: i64,
    revision: i64,
) -> CreationReadyProof {
    CreationReadyProof::new(
        game,
        account,
        command,
        fp,
        created,
        revision.try_into().unwrap(),
    )
}
#[allow(
    clippy::too_many_arguments,
    reason = "Typed Game committed-evidence fixture."
)]
fn projection(
    game: brews_domain::ids::GameId,
    host: AccountId,
    fp: CreationFingerprint,
    state: GameState,
    revision: i64,
    code: Option<GameCode>,
    created: i64,
    started: Option<i64>,
    ended: Option<i64>,
    expiry: Option<i64>,
) -> GameProjection {
    GameProjection::new(
        game,
        host,
        fp,
        state,
        revision.try_into().unwrap(),
        code,
        created,
        started,
        ended,
        expiry,
    )
}
#[test]
fn claim_reserves_identity_indexes_host_and_replays_same_command() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let cmd = command(&rt, 1);
    let first = svc.claim_game(account(1), cmd, fingerprint()).unwrap();
    assert_eq!(first.created_at(), rt.now.get());
    assert_eq!(first.account_id(), account(1));
    assert_eq!(first.ready_revision(), None);
    rt.now.set(rt.now.get() + 1);
    assert_eq!(
        svc.claim_game(account(1), cmd, fingerprint()).unwrap(),
        first
    );
    assert_eq!(
        svc.claim_game(account(1), cmd, CreationFingerprint::from_digest([8; 32])),
        Err(directory::DirectoryError::CommandConflict)
    );
    assert_eq!(
        svc.claim_game(account(2), command(&rt, 2), fingerprint()),
        Err(directory::DirectoryError::ReservationOccupied)
    );
    let op: OperationId = command(&rt, 3).to_string().parse().unwrap();
    assert_eq!(
        svc.acquire_removal(op, account(1)),
        Err(directory::DirectoryError::HostedGame)
    );
    assert_eq!(
        db.query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Text(first.game_id().to_string())]]
    );
}

#[test]
fn matching_initializer_evidence_acknowledges_exact_identity() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let work = svc
        .claim_game(account(1), command(&rt, 1), fingerprint())
        .unwrap();
    let proof = creation_proof(
        work.game_id(),
        work.account_id(),
        work.command_id(),
        work.fingerprint(),
        work.created_at(),
        0,
    );
    let ack = svc.acknowledge_creation(proof).unwrap();
    assert_eq!(ack.game_id(), work.game_id());
    assert_eq!(svc.acknowledge_creation(proof).unwrap(), ack);
    assert_eq!(
        svc.creation_work(work.game_id())
            .unwrap()
            .unwrap()
            .ready_revision(),
        Some(0)
    );
    let wrong = creation_proof(
        work.game_id(),
        work.account_id(),
        work.command_id(),
        CreationFingerprint::from_digest([8; 32]),
        work.created_at(),
        0,
    );
    assert_eq!(
        svc.acknowledge_creation(wrong),
        Err(directory::DirectoryError::ProofMismatch)
    );
}

fn ready(
    svc: &DirectoryService<'_, Sqlite, TestRuntime>,
    rt: &TestRuntime,
) -> directory::games::CreationAck {
    let w = svc
        .claim_game(account(1), command(rt, 1), fingerprint())
        .unwrap();
    svc.acknowledge_creation(creation_proof(
        w.game_id(),
        w.account_id(),
        w.command_id(),
        w.fingerprint(),
        w.created_at(),
        0,
    ))
    .unwrap()
}
#[test]
fn lobby_code_stays_hidden_until_exact_newer_game_commit_projection() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let grant = svc.allocate_game_code(ack).unwrap();
    assert_eq!(svc.allocate_game_code(ack).unwrap(), grant);
    assert_eq!(svc.lookup_game_code(grant.game_code()).unwrap(), None);
    let p = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::AwaitingPlayers,
        1,
        Some(grant.game_code().clone()),
        ack.proof().created_at(),
        None,
        None,
        None,
    );
    let published = svc.publish_game(p.clone()).unwrap();
    assert!(published.published());
    assert_eq!(published.source_revision(), 1);
    assert_eq!(
        svc.lookup_game_code(grant.game_code()).unwrap(),
        Some(ack.game_id())
    );
    assert_eq!(svc.publish_game(p).unwrap(), published);
}

#[test]
fn terminal_proof_releases_only_its_game_and_purges_prestart_index() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    assert_eq!(
        svc.confirm_reservation(ack.game_id()).unwrap().game_id(),
        ack.game_id()
    );
    let terminal = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::Cancelled,
        1,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        None,
        Some(rt.now.get()),
        None,
    );
    let release = svc
        .release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    assert_eq!(release.game_id(), ack.game_id());
    assert!(
        db.query("SELECT game_id FROM directory_game_index", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query(
            "SELECT game_id FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        db.query("SELECT game_id FROM directory_game_creations", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        svc.claim_game(account(1), ack.proof().command_id(), fingerprint()),
        Err(directory::DirectoryError::Completed)
    );
    let next = svc
        .claim_game(account(2), command(&rt, 2), fingerprint())
        .unwrap();
    svc.release_game(TerminalProof::new(terminal)).unwrap();
    assert_eq!(
        svc.confirm_reservation(next.game_id()).unwrap().game_id(),
        next.game_id()
    );
}

#[test]
fn pending_creation_alarm_retries_through_deadline_without_silent_discard() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let w = svc
        .claim_game(account(1), command(&rt, 1), fingerprint())
        .unwrap();
    assert_eq!(svc.due_creation_work(1).unwrap(), vec![w]);
    assert_eq!(svc.next_deadline().unwrap(), Some(rt.now.get()));
    let mut current = svc.retry_creation(w).unwrap();
    assert_eq!(current.attempts(), 1);
    assert_eq!(current.next_retry_at(), Some(rt.now.get() + 1_000));
    assert_eq!(svc.retry_creation(w).unwrap(), current);
    assert!(svc.due_creation_work(1).unwrap().is_empty());
    for _ in 0..12 {
        rt.now.set(current.next_retry_at().unwrap());
        current = svc.retry_creation(current).unwrap();
    }
    assert_eq!(current.next_retry_at(), Some(rt.now.get() + 300_000));
    rt.now.set(w.deadline());
    let due = svc.due_creation_work(1).unwrap();
    assert_eq!(due.len(), 1);
    let late = creation_proof(
        w.game_id(),
        w.account_id(),
        w.command_id(),
        w.fingerprint(),
        w.created_at(),
        0,
    );
    assert_eq!(
        svc.acknowledge_creation(late),
        Err(directory::DirectoryError::Completed)
    );
    current = svc.retry_creation(due[0]).unwrap();
    assert_eq!(svc.next_deadline().unwrap(), current.next_retry_at());
    assert!(svc.creation_work(w.game_id()).unwrap().is_some());
    assert_eq!(
        svc.claim_game(w.account_id(), w.command_id(), w.fingerprint())
            .unwrap()
            .game_id(),
        w.game_id()
    );
}

#[test]
fn completion_cleanup_retires_commands_at_exact_boundary_but_keeps_pending() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let w = svc.creation_work(ack.game_id()).unwrap().unwrap();
    assert_eq!(
        svc.next_deadline().unwrap(),
        Some(w.created_at() + 86_400_000)
    );
    rt.now.set(w.created_at() + 86_400_000 - 1);
    svc.cleanup().unwrap();
    assert!(svc.creation_work(w.game_id()).unwrap().is_some());
    rt.now.set(rt.now.get() + 1);
    svc.cleanup().unwrap();
    assert!(svc.creation_work(w.game_id()).unwrap().is_none());
    assert_eq!(
        svc.claim_game(w.account_id(), w.command_id(), w.fingerprint()),
        Err(directory::DirectoryError::StaleOperation)
    );
    assert_eq!(
        svc.confirm_reservation(w.game_id()).unwrap().game_id(),
        w.game_id()
    );
}

#[test]
fn clock_advance_during_entropy_rechecks_common_fresh_admission_floor() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let cmd = command(&rt, 1);
    rt.advance_random_ms.set(86_400_000);
    assert_eq!(
        svc.claim_game(account(1), cmd, fingerprint()),
        Err(directory::DirectoryError::StaleOperation)
    );
    assert_eq!(
        db.query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    assert!(
        db.query(
            "SELECT game_id FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap()
        .is_empty()
    );
}

fn legacy_directory(db: &Sqlite) {
    for sql in [
        "CREATE TABLE directory_metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,last_observed_ms INTEGER NOT NULL,command_floor_ms INTEGER NOT NULL) STRICT",
        "CREATE TABLE account_assignment_gates(account_id TEXT PRIMARY KEY) STRICT",
        "CREATE TABLE directory_removal_rejections(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL,completed_at INTEGER NOT NULL) STRICT",
        "CREATE TABLE directory_removal_receipts(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL,completed_at INTEGER NOT NULL) STRICT",
        "CREATE TABLE directory_hosted_nonterminal_games(game_id TEXT PRIMARY KEY,designated_host_id TEXT NOT NULL) STRICT",
        "CREATE INDEX directory_hosted_nonterminal_by_host ON directory_hosted_nonterminal_games(designated_host_id)",
        "CREATE TABLE directory_removal_pending(operation_id TEXT PRIMARY KEY,account_id TEXT NOT NULL UNIQUE REFERENCES account_assignment_gates(account_id),created_at INTEGER NOT NULL) STRICT",
        "INSERT INTO directory_metadata VALUES(1,1,0,0)",
    ] {
        db.execute(sql, &[]).unwrap();
    }
}
#[test]
fn legacy_host_index_is_preserved_without_inventing_projection_or_claiming_another_slot() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    legacy_directory(&db);
    let game = "01890f3e-53b7-7d28-9b05-4f65092d5799";
    db.execute(
        "INSERT INTO directory_hosted_nonterminal_games VALUES(?,?)",
        &[
            SqlValue::Text(game.into()),
            SqlValue::Text(account(1).to_string()),
        ],
    )
    .unwrap();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(
        svc.claim_game(account(2), command(&rt, 2), fingerprint()),
        Err(directory::DirectoryError::ReservationOccupied)
    );
    assert_eq!(
        db.query(
            "SELECT game_id FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Text(game.into())]]
    );
    assert!(
        db.query("SELECT game_id FROM directory_game_index", &[])
            .unwrap()
            .is_empty()
    );
}

fn sql_text(s: impl ToString) -> SqlValue {
    SqlValue::Text(s.to_string())
}
fn scratch_db(name: &str) -> std::path::PathBuf {
    static SEQUENCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "brews-directory-{name}-{}-{}.sqlite",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}
fn open_sqlite(path: &std::path::Path) -> Sqlite {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=10000")
        .unwrap();
    Sqlite {
        conn: std::cell::RefCell::new(conn),
    }
}
#[test]
fn migration_reopens_old_schema_preserving_gate_pending_and_floor() {
    let path = scratch_db("upgrade");
    let rt = TestRuntime::new();
    let op = command(&rt, 1).to_string();
    {
        let db = open_sqlite(&path);
        legacy_directory(&db);
        db.execute(
            "INSERT INTO account_assignment_gates VALUES(?)",
            &[sql_text(account(1))],
        )
        .unwrap();
        db.execute(
            "INSERT INTO directory_removal_pending VALUES(?,?,?)",
            &[
                sql_text(&op),
                sql_text(account(1)),
                SqlValue::Integer(rt.now.get()),
            ],
        )
        .unwrap();
        db.execute(
            "UPDATE directory_metadata SET last_observed_ms=?,command_floor_ms=?",
            &[
                SqlValue::Integer(rt.now.get()),
                SqlValue::Integer(rt.now.get() - 86_400_000),
            ],
        )
        .unwrap();
    }
    let db = open_sqlite(&path);
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(
        svc.acquire_removal(op.parse().unwrap(), account(1))
            .unwrap()
            .account_id,
        account(1)
    );
    assert_eq!(
        svc.claim_game(account(1), command(&rt, 2), fingerprint()),
        Err(directory::DirectoryError::AssignmentBlocked)
    );
    svc.reconcile_removal(op.parse().unwrap(), account(1))
        .unwrap();
    svc.claim_game(account(1), command(&rt, 2), fingerprint())
        .unwrap();
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn migration_version_write_failure_rolls_back_all_game_ddl() {
    let db = Sqlite::new();
    legacy_directory(&db);
    db.execute("CREATE TRIGGER fail_upgrade BEFORE UPDATE OF schema_version ON directory_metadata BEGIN SELECT RAISE(ABORT,'fixture storage fault'); END",&[]).unwrap();
    assert_eq!(
        migrate_directory(&db),
        Err(directory::DirectoryError::Storage)
    );
    assert!(
        db.query(
            "SELECT name FROM sqlite_master WHERE name='directory_global_reservation'",
            &[]
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        db.query("SELECT schema_version FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    db.execute("DROP TRIGGER fail_upgrade", &[]).unwrap();
    migrate_directory(&db).unwrap();
}
#[test]
fn migration_unknown_versions_and_unexpected_application_tables_fail_closed() {
    for version in [0, 3, 99] {
        let db = Sqlite::new();
        legacy_directory(&db);
        db.execute(
            "UPDATE directory_metadata SET schema_version=?",
            &[SqlValue::Integer(version)],
        )
        .unwrap();
        assert_eq!(
            migrate_directory(&db),
            Err(directory::DirectoryError::Storage)
        );
        assert!(
            db.query(
                "SELECT name FROM sqlite_master WHERE name='directory_game_index'",
                &[]
            )
            .unwrap()
            .is_empty()
        );
    }
    let db = Sqlite::new();
    legacy_directory(&db);
    db.execute("CREATE TABLE _cf_APPLICATION(game_id TEXT)", &[])
        .unwrap();
    assert_eq!(
        migrate_directory(&db),
        Err(directory::DirectoryError::Storage)
    );
}
#[test]
fn initializer_and_code_identity_survive_actual_database_reopen() {
    let path = scratch_db("lost-ack");
    let rt = TestRuntime::new();
    let cmd = command(&rt, 1);
    let (work, code) = {
        let db = open_sqlite(&path);
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        let w = svc.claim_game(account(1), cmd, fingerprint()).unwrap();
        let a = svc
            .acknowledge_creation(creation_proof(
                w.game_id(),
                w.account_id(),
                w.command_id(),
                w.fingerprint(),
                w.created_at(),
                0,
            ))
            .unwrap();
        (w, svc.allocate_game_code(a).unwrap())
    };
    let db = open_sqlite(&path);
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(
        svc.claim_game(account(1), cmd, fingerprint())
            .unwrap()
            .game_id(),
        work.game_id()
    );
    assert_eq!(svc.allocate_game_code(code.creation()).unwrap(), code);
    assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn independent_connections_serialize_competing_hosts_to_one_reserved_game() {
    let path = scratch_db("race");
    {
        let db = open_sqlite(&path);
        migrate_directory(&db).unwrap();
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let joins: Vec<_> = (1..=2)
        .map(|n| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let db = open_sqlite(&path);
                let rt = TestRuntime::new();
                let svc = DirectoryService::new(&db, &rt).unwrap();
                barrier.wait();
                svc.claim_game(account(n), command(&rt, n), fingerprint())
            })
        })
        .collect();
    let results: Vec<_> = joins.into_iter().map(|j| j.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(directory::DirectoryError::ReservationOccupied))
            .count(),
        1
    );
    let db = open_sqlite(&path);
    assert_eq!(
        db.query(
            "SELECT count(*) FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn projection_duplicates_old_revisions_and_wrong_fingerprints_cannot_change_publication() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    let project = |state, revision, fp, host, start| {
        projection(
            ack.game_id(),
            host,
            fp,
            state,
            revision,
            Some(code.game_code().clone()),
            ack.proof().created_at(),
            start,
            None,
            None,
        )
    };
    assert_eq!(
        svc.publish_game(project(
            GameState::AwaitingPlayers,
            1,
            CreationFingerprint::from_digest([8; 32]),
            account(1),
            None
        )),
        Err(directory::DirectoryError::ProofMismatch)
    );
    assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
    let published = svc
        .publish_game(project(
            GameState::AwaitingPlayers,
            1,
            fingerprint(),
            account(1),
            None,
        ))
        .unwrap();
    assert_eq!(
        svc.publish_game(project(
            GameState::AwaitingPlayers,
            1,
            fingerprint(),
            account(2),
            None
        )),
        Err(directory::DirectoryError::ProofMismatch)
    );
    let started = svc
        .publish_game(project(
            GameState::InProgress,
            2,
            fingerprint(),
            account(1),
            Some(rt.now.get()),
        ))
        .unwrap();
    assert_eq!(started.source_revision(), 2);
    assert_eq!(
        svc.publish_game(project(
            GameState::AwaitingPlayers,
            1,
            fingerprint(),
            account(1),
            None
        ))
        .unwrap(),
        started
    );
    assert_ne!(published, started);
    assert_eq!(
        svc.lookup_game_code(code.game_code()).unwrap(),
        Some(ack.game_id())
    );
}
#[test]
fn assignment_gate_blocks_newer_projection_to_removing_host() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    let projection = |host, revision| {
        projection(
            ack.game_id(),
            host,
            fingerprint(),
            GameState::AwaitingPlayers,
            revision,
            Some(code.game_code().clone()),
            ack.proof().created_at(),
            None,
            None,
            None,
        )
    };
    svc.publish_game(projection(account(1), 1)).unwrap();
    svc.acquire_removal(command(&rt, 2).to_string().parse().unwrap(), account(2))
        .unwrap();
    assert_eq!(
        svc.publish_game(projection(account(2), 2)),
        Err(directory::DirectoryError::AssignmentBlocked)
    );
    assert_eq!(
        db.query(
            "SELECT designated_host_id FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap(),
        vec![vec![sql_text(account(1))]]
    );
}
#[test]
fn real_index_insert_failure_rolls_back_claim_reservation_and_clock() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    db.execute("CREATE TRIGGER fail_index BEFORE INSERT ON directory_game_index BEGIN SELECT RAISE(ABORT,'fixture storage fault'); END",&[]).unwrap();
    assert_eq!(
        svc.claim_game(account(1), command(&rt, 1), fingerprint()),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(
        db.query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    assert!(
        db.query(
            "SELECT game_id FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        db.query("SELECT last_observed_ms FROM directory_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}
#[test]
fn game_release_delete_failure_rolls_back_code_index_host_and_retirement() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    db.execute("CREATE TRIGGER fail_release BEFORE DELETE ON directory_hosted_nonterminal_games BEGIN SELECT RAISE(ABORT,'fixture storage fault'); END",&[]).unwrap();
    let p = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::Cancelled,
        1,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        None,
        Some(rt.now.get()),
        None,
    );
    assert_eq!(
        svc.release_game(TerminalProof::new(p)),
        Err(directory::DirectoryError::Storage)
    );
    assert!(svc.creation_work(ack.game_id()).unwrap().is_some());
    assert_eq!(svc.allocate_game_code(ack).unwrap(), code);
    assert_eq!(
        svc.confirm_reservation(ack.game_id()).unwrap().game_id(),
        ack.game_id()
    );
    assert!(
        db.query("SELECT command_id FROM directory_retired_creations", &[])
            .unwrap()
            .is_empty()
    );
}
#[test]
fn invalid_terminal_evidence_cannot_release_reservation() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let p = projection(
        ack.game_id(),
        account(1),
        CreationFingerprint::from_digest([9; 32]),
        GameState::Cancelled,
        1,
        None,
        ack.proof().created_at(),
        None,
        Some(rt.now.get()),
        None,
    );
    assert_eq!(
        svc.release_game(TerminalProof::new(p)),
        Err(directory::DirectoryError::ProofMismatch)
    );
    assert_eq!(
        svc.confirm_reservation(ack.game_id()).unwrap().game_id(),
        ack.game_id()
    );
}
#[test]
fn ready_ack_delayed_past_cancellation_cannot_touch_newer_game() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let w = svc
        .claim_game(account(1), command(&rt, 1), fingerprint())
        .unwrap();
    let proof = creation_proof(
        w.game_id(),
        w.account_id(),
        w.command_id(),
        w.fingerprint(),
        w.created_at(),
        0,
    );
    svc.release_game(TerminalProof::new(projection(
        w.game_id(),
        account(1),
        fingerprint(),
        GameState::Cancelled,
        1,
        None,
        w.created_at(),
        None,
        Some(rt.now.get()),
        None,
    )))
    .unwrap();
    let next = svc
        .claim_game(account(2), command(&rt, 2), fingerprint())
        .unwrap();
    assert_eq!(
        svc.acknowledge_creation(proof),
        Err(directory::DirectoryError::ProofMismatch)
    );
    assert_eq!(
        svc.confirm_reservation(next.game_id()).unwrap().game_id(),
        next.game_id()
    );
}
#[test]
fn durable_command_floor_survives_reopen_and_rejects_clock_rollback() {
    let path = scratch_db("floor");
    let rt = TestRuntime::new();
    let old = command(&rt, 1);
    {
        let db = open_sqlite(&path);
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        rt.now.set(rt.now.get() + 86_400_000);
        svc.cleanup().unwrap();
    }
    let db = open_sqlite(&path);
    let svc = DirectoryService::new(&db, &rt).unwrap();
    assert_eq!(
        svc.claim_game(account(1), old, fingerprint()),
        Err(directory::DirectoryError::StaleOperation)
    );
    rt.now.set(rt.now.get() - 1);
    assert_eq!(
        svc.claim_game(account(1), old, fingerprint()),
        Err(directory::DirectoryError::Clock)
    );
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn entropy_failure_rolls_back_claim_without_reserving_identity() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    rt.fail.set(true);
    assert_eq!(
        svc.claim_game(account(1), command(&rt, 1), fingerprint()),
        Err(directory::DirectoryError::Entropy)
    );
    assert_eq!(
        db.query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    rt.fail.set(false);
    svc.claim_game(account(1), command(&rt, 1), fingerprint())
        .unwrap();
}
#[test]
fn retry_attempts_saturate_and_pending_never_expires_into_another_identity() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let w = svc
        .claim_game(account(1), command(&rt, 1), fingerprint())
        .unwrap();
    db.execute(
        "UPDATE directory_game_creations SET attempts=?",
        &[SqlValue::Integer(i64::from(u32::MAX))],
    )
    .unwrap();
    let current = svc.creation_work(w.game_id()).unwrap().unwrap();
    let next = svc.retry_creation(current).unwrap();
    assert_eq!(next.attempts(), u32::MAX);
    assert_eq!(next.next_retry_at(), Some(rt.now.get() + 300_000));
    rt.now.set(w.deadline() + 86_400_000);
    svc.cleanup().unwrap();
    assert_eq!(
        svc.claim_game(w.account_id(), w.command_id(), w.fingerprint())
            .unwrap()
            .game_id(),
        w.game_id()
    );
    assert_eq!(
        svc.claim_game(account(2), command(&rt, 2), fingerprint()),
        Err(directory::DirectoryError::ReservationOccupied)
    );
}

struct ByteRuntime {
    now: std::cell::Cell<i64>,
    byte: std::cell::Cell<u8>,
    calls: std::cell::Cell<usize>,
}
impl auth::Runtime for ByteRuntime {
    fn now_ms(&self) -> i64 {
        self.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), auth::AuthError> {
        self.calls.set(self.calls.get() + 1);
        bytes.fill(self.byte.get());
        Ok(())
    }
}
#[test]
fn retained_history_code_collisions_hit_explicit_candidate_bound_without_partial_association() {
    let db = Sqlite::new();
    let rt = ByteRuntime {
        now: std::cell::Cell::new(1_800_000_000_000),
        byte: std::cell::Cell::new(0),
        calls: std::cell::Cell::new(0),
    };
    let clock = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let w = svc
        .claim_game(account(1), command(&clock, 1), fingerprint())
        .unwrap();
    let a = svc
        .acknowledge_creation(creation_proof(
            w.game_id(),
            w.account_id(),
            w.command_id(),
            w.fingerprint(),
            w.created_at(),
            0,
        ))
        .unwrap();
    let code = svc.allocate_game_code(a).unwrap();
    assert_eq!(code.game_code().as_str(), "AAAAAAAA");
    let p = |state, revision, start, end, expiry| {
        projection(
            w.game_id(),
            account(1),
            fingerprint(),
            state,
            revision,
            Some(code.game_code().clone()),
            w.created_at(),
            start,
            end,
            expiry,
        )
    };
    svc.publish_game(p(GameState::AwaitingPlayers, 1, None, None, None))
        .unwrap();
    svc.publish_game(p(GameState::InProgress, 2, Some(rt.now.get()), None, None))
        .unwrap();
    svc.release_game(TerminalProof::new(p(
        GameState::Cancelled,
        3,
        Some(rt.now.get()),
        Some(rt.now.get()),
        Some(rt.now.get() + 86_400_000),
    )))
    .unwrap();
    rt.now.set(rt.now.get() + 1);
    clock.now.set(rt.now.get());
    let w2 = svc
        .claim_game(account(2), command(&clock, 2), fingerprint())
        .unwrap();
    let a2 = svc
        .acknowledge_creation(creation_proof(
            w2.game_id(),
            w2.account_id(),
            w2.command_id(),
            w2.fingerprint(),
            w2.created_at(),
            0,
        ))
        .unwrap();
    let before = rt.calls.get();
    assert_eq!(
        svc.allocate_game_code(a2),
        Err(directory::DirectoryError::CodeExhausted)
    );
    assert_eq!(
        rt.calls.get() - before,
        directory::games::GAME_CODE_MAX_CANDIDATES
    );
    assert_eq!(
        db.query(
            "SELECT game_code FROM directory_game_index WHERE game_id=?",
            &[sql_text(w2.game_id())]
        )
        .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    assert_eq!(
        db.query(
            "SELECT game_code FROM directory_game_index WHERE game_id=?",
            &[sql_text(w.game_id())]
        )
        .unwrap(),
        vec![vec![sql_text("AAAAAAAA")]]
    );
    assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
}
#[test]
fn code_rejection_sampling_has_bounded_entropy_and_rolls_back_on_exhaustion() {
    let db = Sqlite::new();
    let rt = ByteRuntime {
        now: std::cell::Cell::new(1_800_000_000_000),
        byte: std::cell::Cell::new(0),
        calls: std::cell::Cell::new(0),
    };
    let clock = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let w = svc
        .claim_game(account(1), command(&clock, 1), fingerprint())
        .unwrap();
    let a = svc
        .acknowledge_creation(creation_proof(
            w.game_id(),
            w.account_id(),
            w.command_id(),
            w.fingerprint(),
            w.created_at(),
            0,
        ))
        .unwrap();
    rt.byte.set(255);
    let before = rt.calls.get();
    assert_eq!(
        svc.allocate_game_code(a),
        Err(directory::DirectoryError::CodeExhausted)
    );
    assert_eq!(
        rt.calls.get() - before,
        directory::games::GAME_CODE_ENTROPY_BATCHES
    );
    assert_eq!(
        db.query("SELECT game_code FROM directory_game_index", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    rt.byte.set(251);
    assert_eq!(
        svc.allocate_game_code(a).unwrap().game_code().as_str(),
        "99999999"
    );
}
#[test]
fn prestart_retirement_compaction_is_exactly_bounded_and_prevents_reallocation_after_floor() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let old = command(&rt, 1);
    for n in 1..=101 {
        let w = svc
            .claim_game(account(1), command(&rt, n), fingerprint())
            .unwrap();
        svc.release_game(TerminalProof::new(projection(
            w.game_id(),
            account(1),
            fingerprint(),
            GameState::Cancelled,
            1,
            None,
            w.created_at(),
            None,
            Some(rt.now.get()),
            None,
        )))
        .unwrap();
    }
    let count = || {
        db.query("SELECT count(*) FROM directory_retired_creations", &[])
            .unwrap()
    };
    assert_eq!(count(), vec![vec![SqlValue::Integer(101)]]);
    let due = svc.next_deadline().unwrap().unwrap();
    rt.now.set(due - 1);
    svc.cleanup().unwrap();
    assert_eq!(count(), vec![vec![SqlValue::Integer(101)]]);
    rt.now.set(due);
    svc.cleanup().unwrap();
    assert_eq!(count(), vec![vec![SqlValue::Integer(1)]]);
    assert_eq!(svc.next_deadline().unwrap(), Some(due));
    svc.cleanup().unwrap();
    assert_eq!(count(), vec![vec![SqlValue::Integer(0)]]);
    assert_eq!(svc.next_deadline().unwrap(), None);
    assert_eq!(
        svc.claim_game(account(1), old, fingerprint()),
        Err(directory::DirectoryError::StaleOperation)
    );
}
#[test]
fn corrupt_retirement_evidence_rolls_back_earlier_ready_cleanup_batch() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let created = rt.now.get();
    db.execute(
        "INSERT INTO directory_retired_creations VALUES(?,?,?,?)",
        &[
            sql_text(account(2)),
            sql_text(command(&rt, 2)),
            SqlValue::Blob(vec![9; 32]),
            SqlValue::Integer(created - 1),
        ],
    )
    .unwrap();
    rt.now.set(created + 86_400_000);
    assert_eq!(svc.cleanup(), Err(directory::DirectoryError::Storage));
    assert_eq!(
        db.query("SELECT count(*) FROM directory_game_creations", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert_eq!(
        db.query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![sql_text(ack.game_id())]]
    );
}
#[test]
fn publication_write_failure_keeps_pending_code_hidden_and_replays_same_association() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    db.execute("CREATE TRIGGER fail_publication BEFORE UPDATE OF publication_state ON directory_game_index BEGIN SELECT RAISE(ABORT,'fixture storage fault'); END",&[]).unwrap();
    let p = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::AwaitingPlayers,
        1,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        None,
        None,
        None,
    );
    assert_eq!(
        svc.publish_game(p.clone()),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
    assert_eq!(svc.allocate_game_code(ack).unwrap(), code);
    db.execute("DROP TRIGGER fail_publication", &[]).unwrap();
    svc.publish_game(p).unwrap();
    assert_eq!(
        svc.lookup_game_code(code.game_code()).unwrap(),
        Some(ack.game_id())
    );
}
#[test]
fn missing_initializer_requires_original_resend_and_never_allocates_code_or_ready_success() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let w = svc
        .claim_game(account(1), command(&rt, 1), fingerprint())
        .unwrap();
    assert_eq!(w.ready_revision(), None);
    assert_eq!(svc.due_creation_work(1).unwrap(), vec![w]);
    assert_eq!(
        db.query("SELECT game_code FROM directory_game_index", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    let columns = db
        .query(
            "SELECT name FROM pragma_table_info('directory_game_creations')",
            &[],
        )
        .unwrap();
    assert!(columns.iter().all(|row|!matches!(row.as_slice(),[SqlValue::Text(column)] if column.contains("config")||column.contains("password")||column.contains("token"))));
    assert_eq!(
        svc.lookup_game_code(&"UNKNOWN1".parse::<GameCode>().unwrap())
            .unwrap(),
        None
    );
}

#[test]
fn negative_game_revision_evidence_cannot_mutate_directory() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    assert_eq!(
        directory::games::SourceRevision::try_from(-1),
        Err(directory::DirectoryError::ProofMismatch)
    );
    assert_eq!(
        directory::games::SourceRevision::try_from(0).unwrap(),
        directory::games::SourceRevision::INITIAL
    );
    assert_eq!(
        directory::games::SourceRevision::try_from(i64::MAX)
            .unwrap()
            .value(),
        i64::MAX
    );
    assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
}
#[test]
fn delayed_older_projection_after_terminal_release_is_a_stale_noop() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    let lobby = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::AwaitingPlayers,
        1,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        None,
        None,
        None,
    );
    svc.publish_game(lobby.clone()).unwrap();
    let started = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::InProgress,
        2,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        Some(rt.now.get()),
        None,
        None,
    );
    svc.publish_game(started).unwrap();
    let ended = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::Cancelled,
        3,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        Some(rt.now.get()),
        Some(rt.now.get()),
        Some(rt.now.get() + 86_400_000),
    );
    svc.release_game(TerminalProof::new(ended)).unwrap();
    let next = svc
        .claim_game(account(2), command(&rt, 2), fingerprint())
        .unwrap();
    assert_eq!(svc.publish_game(lobby).unwrap().source_revision(), 3);
    assert_eq!(
        svc.confirm_reservation(next.game_id()).unwrap().game_id(),
        next.game_id()
    );
    assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
}
#[test]
fn unresolved_code_publication_outlives_creation_receipt_floor() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    rt.now.set(rt.now.get() + 86_400_000 + 1);
    svc.cleanup().unwrap();
    assert_eq!(svc.allocate_game_code(ack).unwrap(), code);
    let p = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::AwaitingPlayers,
        1,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        None,
        None,
        None,
    );
    svc.publish_game(p).unwrap();
    assert_eq!(
        svc.lookup_game_code(code.game_code()).unwrap(),
        Some(ack.game_id())
    );
}
#[test]
fn pending_publication_is_due_known_game_work_until_matching_revision_ack() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    let code = svc.allocate_game_code(ack).unwrap();
    let pending = svc.due_creation_work(1).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].ready_revision(), Some(0));
    assert_eq!(svc.next_deadline().unwrap(), Some(rt.now.get()));
    let next = svc.retry_creation(pending[0]).unwrap();
    assert_eq!(next.next_retry_at(), Some(rt.now.get() + 1_000));
    rt.now.set(next.next_retry_at().unwrap());
    assert_eq!(svc.due_creation_work(1).unwrap(), vec![next]);
    let p = projection(
        ack.game_id(),
        account(1),
        fingerprint(),
        GameState::AwaitingPlayers,
        1,
        Some(code.game_code().clone()),
        ack.proof().created_at(),
        None,
        None,
        None,
    );
    svc.publish_game(p).unwrap();
    assert!(svc.due_creation_work(1).unwrap().is_empty());
    assert_eq!(
        svc.next_deadline().unwrap(),
        Some(ack.proof().created_at() + 86_400_000)
    );
}
#[test]
fn trusted_recovery_reads_pending_association_without_public_code_discovery() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let ack = ready(&svc, &rt);
    assert_eq!(svc.game_code(ack.game_id()).unwrap(), None);
    let code = svc.allocate_game_code(ack).unwrap();
    assert_eq!(
        svc.game_code(ack.game_id()).unwrap(),
        Some(code.game_code().clone())
    );
    assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
}
#[test]
fn migration_installs_minimal_single_reservation() {
    let db = Sqlite::new();
    migrate_directory(&db).unwrap();
    let tables = db.query("SELECT name FROM sqlite_master WHERE type='table' AND name='directory_global_reservation'", &[]).unwrap();
    assert_eq!(
        tables,
        vec![vec![SqlValue::Text("directory_global_reservation".into())]]
    );
    assert_eq!(
        db.query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    let columns = db
        .query(
            "SELECT name FROM pragma_table_info('directory_global_reservation')",
            &[],
        )
        .unwrap();
    assert_eq!(columns, vec![vec![SqlValue::Text("game_id".into())]]);
    assert!(
        db.execute("INSERT INTO directory_global_reservation VALUES(NULL)", &[])
            .is_err()
    );
    migrate_directory(&db).unwrap();
    DirectoryService::new(&db, &TestRuntime::new()).unwrap();
}

// 2027-01-15T08:00Z + three UTC calendar months.
const HISTORY_EXPIRY: i64 = 1_807_776_000_000;

fn started_terminal<R: auth::Runtime>(
    svc: &DirectoryService<'_, Sqlite, R>,
    rt: &R,
    cmd: CommandId,
    outcome: GameState,
) -> GameProjection {
    let work = svc.claim_game(account(1), cmd, fingerprint()).unwrap();
    let ack = svc
        .acknowledge_creation(creation_proof(
            work.game_id(),
            work.account_id(),
            work.command_id(),
            work.fingerprint(),
            work.created_at(),
            0,
        ))
        .unwrap();
    let code = svc.allocate_game_code(ack).unwrap();
    svc.publish_game(projection(
        work.game_id(),
        account(1),
        fingerprint(),
        GameState::AwaitingPlayers,
        1,
        Some(code.game_code().clone()),
        work.created_at(),
        None,
        None,
        None,
    ))
    .unwrap();
    svc.publish_game(projection(
        work.game_id(),
        account(1),
        fingerprint(),
        GameState::InProgress,
        2,
        Some(code.game_code().clone()),
        work.created_at(),
        Some(rt.now_ms()),
        None,
        None,
    ))
    .unwrap();
    projection(
        work.game_id(),
        account(1),
        fingerprint(),
        outcome,
        3,
        Some(code.game_code().clone()),
        work.created_at(),
        Some(rt.now_ms()),
        Some(rt.now_ms()),
        Some(HISTORY_EXPIRY),
    )
}

fn index_row(db: &Sqlite, game: brews_domain::ids::GameId) -> Vec<db::Row> {
    db.query("SELECT game_id,game_code,designated_host_id,state,source_revision,publication_state,created_at,started_at,ended_at,history_expires_at,fingerprint FROM directory_game_index WHERE game_id=?", &[sql_text(game)]).unwrap()
}

#[test]
fn accepted_started_terminal_proof_cannot_rewrite_original_expiry_with_newer_revision() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&rt, 1), GameState::Resolved);
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    let before = index_row(&db, terminal.game_id());
    let altered = projection(
        terminal.game_id(),
        terminal.designated_host_id(),
        terminal.fingerprint(),
        terminal.state(),
        terminal.source_revision() + 1,
        terminal.game_code().cloned(),
        terminal.created_at(),
        terminal.started_at(),
        terminal.ended_at(),
        Some(HISTORY_EXPIRY + 1),
    );
    assert_eq!(
        svc.release_game(TerminalProof::new(altered)),
        Err(directory::DirectoryError::ProofMismatch)
    );
    assert_eq!(index_row(&db, terminal.game_id()), before);
    assert_eq!(
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap()
            .game_id(),
        terminal.game_id()
    );
}

fn seed_history_backlog(db: &Sqlite, terminal: &GameProjection, clock: &TestRuntime, count: u8) {
    for n in 1..=count {
        let game: brews_domain::ids::GameId = command(clock, n).to_string().parse().unwrap();
        db.execute("INSERT INTO directory_game_index(game_id,game_code,designated_host_id,state,source_revision,publication_state,created_at,started_at,ended_at,history_expires_at,fingerprint) SELECT ?,?,designated_host_id,state,source_revision,publication_state,created_at,started_at,ended_at,history_expires_at,fingerprint FROM directory_game_index WHERE game_id=?", &[sql_text(game), sql_text(format!("HIS{n:05}")), sql_text(terminal.game_id())]).unwrap();
    }
}

fn coordination_snapshot(db: &Sqlite) -> Vec<Vec<db::Row>> {
    [
        "SELECT game_id,game_code,state,source_revision,history_expires_at FROM directory_game_index ORDER BY game_id",
        "SELECT game_id FROM directory_global_reservation",
        "SELECT game_id,designated_host_id FROM directory_hosted_nonterminal_games ORDER BY game_id",
        "SELECT game_id,next_retry_at,ready_revision FROM directory_game_creations ORDER BY game_id",
        "SELECT account_id,command_id,completed_at FROM directory_retired_creations ORDER BY account_id,command_id",
        "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
    ].map(|sql| db.query(sql, &[]).unwrap()).into()
}

#[test]
fn terminal_release_sqlite_faults_rollback_every_required_write_without_false_ack() {
    for target in [
        "INSERT ON directory_retired_creations",
        "DELETE ON directory_game_creations",
        "UPDATE OF state ON directory_game_index",
        "DELETE ON directory_hosted_nonterminal_games",
        "UPDATE OF game_id ON directory_global_reservation",
    ] {
        for fault in ["IGNORE", "ABORT,'fixture terminal write'"] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            migrate_directory(&db).unwrap();
            let svc = DirectoryService::new(&db, &rt).unwrap();
            let terminal = started_terminal(&svc, &rt, command(&rt, 1), GameState::Resolved);
            let before = coordination_snapshot(&db);
            db.execute(
                &format!(
                    "CREATE TRIGGER terminal_fault BEFORE {target} BEGIN SELECT RAISE({fault}); END"
                ),
                &[],
            )
            .unwrap();
            rt.now.set(rt.now.get() + 1);
            assert_eq!(
                svc.release_game(TerminalProof::new(terminal.clone())),
                Err(directory::DirectoryError::Storage),
                "{target} {fault}"
            );
            assert_eq!(coordination_snapshot(&db), before, "{target} {fault}");
            db.execute("DROP TRIGGER terminal_fault", &[]).unwrap();
            svc.release_game(TerminalProof::new(terminal.clone()))
                .unwrap();
            assert_eq!(
                index_row(&db, terminal.game_id())[0][9],
                SqlValue::Integer(HISTORY_EXPIRY)
            );
        }
    }
}

#[test]
fn history_expiry_cleanup_sqlite_faults_rollback_index_retirement_and_clock() {
    for fault in ["IGNORE", "ABORT,'fixture history purge'"] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        let terminal = started_terminal(&svc, &rt, command(&rt, 1), GameState::Cancelled);
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap();
        let before = coordination_snapshot(&db);
        db.execute(&format!("CREATE TRIGGER history_fault BEFORE DELETE ON directory_game_index BEGIN SELECT RAISE({fault}); END"), &[]).unwrap();
        rt.now.set(HISTORY_EXPIRY);
        assert_eq!(svc.cleanup(), Err(directory::DirectoryError::Storage));
        assert_eq!(coordination_snapshot(&db), before);
        db.execute("DROP TRIGGER history_fault", &[]).unwrap();
        svc.cleanup().unwrap();
        assert!(index_row(&db, terminal.game_id()).is_empty());
    }
}

#[test]
fn history_expiry_boundary_survives_actual_reopen_without_clock_or_code_resurrection() {
    let path = scratch_db("history-expiry");
    let clock = TestRuntime::new();
    let rt = ByteRuntime {
        now: std::cell::Cell::new(clock.now.get()),
        byte: std::cell::Cell::new(251),
        calls: std::cell::Cell::new(0),
    };
    let terminal = {
        let db = open_sqlite(&path);
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        let terminal = started_terminal(&svc, &rt, command(&clock, 1), GameState::Resolved);
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap();
        seed_history_backlog(&db, &terminal, &clock, 101);
        rt.now.set(HISTORY_EXPIRY - 1);
        assert_eq!(
            svc.game_code(terminal.game_id()).unwrap(),
            terminal.game_code().cloned()
        );
        terminal
    };
    {
        let db = open_sqlite(&path);
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        rt.now.set(HISTORY_EXPIRY);
        assert_eq!(svc.game_code(terminal.game_id()).unwrap(), None);
        assert_eq!(index_row(&db, terminal.game_id()).len(), 1);
        // Expiry denial committed a durable clock floor even though target purge lagged.
    }
    let db = open_sqlite(&path);
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    rt.now.set(HISTORY_EXPIRY - 1);
    assert_eq!(
        svc.game_code(terminal.game_id()),
        Err(directory::DirectoryError::Clock)
    );
    assert_eq!(
        svc.release_game(TerminalProof::new(terminal.clone())),
        Err(directory::DirectoryError::Clock)
    );
    rt.now.set(HISTORY_EXPIRY);
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    assert!(index_row(&db, terminal.game_id()).is_empty());
    clock.now.set(HISTORY_EXPIRY);
    let newer = svc
        .claim_game(account(2), command(&clock, 2), fingerprint())
        .unwrap();
    let ack = svc
        .acknowledge_creation(creation_proof(
            newer.game_id(),
            newer.account_id(),
            newer.command_id(),
            newer.fingerprint(),
            newer.created_at(),
            0,
        ))
        .unwrap();
    let code = svc.allocate_game_code(ack).unwrap();
    assert_eq!(code.game_code(), terminal.game_code().unwrap());
    let before = coordination_snapshot(&db);
    svc.release_game(TerminalProof::new(terminal)).unwrap();
    assert_eq!(coordination_snapshot(&db), before);
    drop(db);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn ignored_expiry_release_delete_beyond_compaction_batch_never_acknowledges_purge() {
    let db = Sqlite::new();
    let clock = TestRuntime::new();
    let rt = ByteRuntime {
        now: std::cell::Cell::new(clock.now.get()),
        byte: std::cell::Cell::new(251),
        calls: std::cell::Cell::new(0),
    };
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&clock, 1), GameState::Resolved);
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    seed_history_backlog(&db, &terminal, &clock, 201);
    let before = index_row(&db, terminal.game_id());
    db.execute("CREATE TRIGGER suppress_target_purge BEFORE DELETE ON directory_game_index WHEN OLD.game_code='99999999' BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap();
    rt.now.set(HISTORY_EXPIRY);
    assert_eq!(
        svc.release_game(TerminalProof::new(terminal.clone())),
        Err(directory::DirectoryError::Storage)
    );
    assert_eq!(index_row(&db, terminal.game_id()), before);
    assert_eq!(
        db.query("SELECT count(*) FROM directory_game_index", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(202)]]
    );
    db.execute("DROP TRIGGER suppress_target_purge", &[])
        .unwrap();
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    assert!(index_row(&db, terminal.game_id()).is_empty());
}

#[test]
fn ignored_code_allocation_never_returns_an_uncommitted_grant() {
    for trigger in [
        "CREATE TRIGGER suppress_allocation BEFORE UPDATE OF game_code ON directory_game_index BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER suppress_allocation BEFORE UPDATE OF next_retry_at ON directory_game_creations BEGIN SELECT RAISE(IGNORE); END",
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        let ready = ready(&svc, &rt);
        let before = index_row(&db, ready.game_id());
        let creation = svc.creation_work(ready.game_id()).unwrap();
        db.execute(trigger, &[]).unwrap();
        assert_eq!(
            svc.allocate_game_code(ready),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(index_row(&db, ready.game_id()), before);
        assert_eq!(svc.creation_work(ready.game_id()).unwrap(), creation);
        db.execute("DROP TRIGGER suppress_allocation", &[]).unwrap();
        let grant = svc.allocate_game_code(ready).unwrap();
        assert_eq!(
            svc.game_code(ready.game_id()).unwrap().as_ref(),
            Some(grant.game_code())
        );
        assert_eq!(svc.allocate_game_code(ready).unwrap(), grant);
    }
}

#[test]
fn ignored_clock_observation_cannot_acknowledge_history_expiry() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&rt, 1), GameState::Cancelled);
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    let before = index_row(&db, terminal.game_id());
    let metadata = db
        .query(
            "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
            &[],
        )
        .unwrap();
    db.execute("CREATE TRIGGER suppress_clock BEFORE UPDATE OF last_observed_ms ON directory_metadata BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap();
    rt.now.set(HISTORY_EXPIRY);
    assert_eq!(svc.cleanup(), Err(directory::DirectoryError::Storage));
    assert_eq!(index_row(&db, terminal.game_id()), before);
    assert_eq!(
        db.query(
            "SELECT last_observed_ms,command_floor_ms FROM directory_metadata",
            &[]
        )
        .unwrap(),
        metadata
    );
    db.execute("DROP TRIGGER suppress_clock", &[]).unwrap();
    svc.cleanup().unwrap();
    assert!(index_row(&db, terminal.game_id()).is_empty());
}

#[test]
fn expired_terminal_publication_can_finish_matching_release_before_compaction() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&rt, 1), GameState::Resolved);
    svc.publish_game(terminal.clone()).unwrap();
    rt.now.set(HISTORY_EXPIRY);
    assert_eq!(
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap()
            .game_id(),
        terminal.game_id()
    );
    assert!(index_row(&db, terminal.game_id()).is_empty());
    assert_eq!(
        db.query("SELECT game_id FROM directory_global_reservation", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    assert!(
        db.query(
            "SELECT game_id FROM directory_hosted_nonterminal_games",
            &[]
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn exact_terminal_release_retry_does_not_rewrite_index_or_touch_new_game() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&rt, 1), GameState::Cancelled);
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    let before = index_row(&db, terminal.game_id());
    let newer = svc
        .claim_game(account(2), command(&rt, 2), fingerprint())
        .unwrap();
    db.execute("CREATE TRIGGER prevent_terminal_rewrite BEFORE UPDATE ON directory_game_index BEGIN SELECT RAISE(ABORT,'fixture immutable index'); END", &[]).unwrap();
    assert_eq!(
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap()
            .game_id(),
        terminal.game_id()
    );
    assert_eq!(index_row(&db, terminal.game_id()), before);
    assert_eq!(
        svc.confirm_reservation(newer.game_id()).unwrap().game_id(),
        newer.game_id()
    );
}

#[test]
fn terminal_publication_cannot_regress_to_newer_in_progress_revision() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&rt, 1), GameState::Resolved);
    svc.publish_game(terminal.clone()).unwrap();
    let before = index_row(&db, terminal.game_id());
    let delayed = projection(
        terminal.game_id(),
        terminal.designated_host_id(),
        terminal.fingerprint(),
        GameState::InProgress,
        terminal.source_revision() + 1,
        terminal.game_code().cloned(),
        terminal.created_at(),
        terminal.started_at(),
        None,
        None,
    );
    assert_eq!(
        svc.publish_game(delayed),
        Err(directory::DirectoryError::ProofMismatch)
    );
    assert_eq!(index_row(&db, terminal.game_id()), before);
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    assert_eq!(index_row(&db, terminal.game_id()), before);
}

#[test]
fn ignored_publication_writes_never_acknowledge_uncommitted_metadata() {
    for trigger in [
        "CREATE TRIGGER suppress_publication BEFORE UPDATE OF publication_state ON directory_game_index BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER suppress_publication BEFORE UPDATE OF designated_host_id ON directory_hosted_nonterminal_games BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER suppress_publication BEFORE UPDATE OF next_retry_at ON directory_game_creations BEGIN SELECT RAISE(IGNORE); END",
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        let ack = ready(&svc, &rt);
        let code = svc.allocate_game_code(ack).unwrap();
        let before = index_row(&db, ack.game_id());
        let pending = svc.creation_work(ack.game_id()).unwrap();
        db.execute(trigger, &[]).unwrap();
        let lobby = projection(
            ack.game_id(),
            account(2),
            fingerprint(),
            GameState::AwaitingPlayers,
            1,
            Some(code.game_code().clone()),
            ack.proof().created_at(),
            None,
            None,
            None,
        );
        assert_eq!(
            svc.publish_game(lobby.clone()),
            Err(directory::DirectoryError::Storage)
        );
        assert_eq!(index_row(&db, ack.game_id()), before);
        assert_eq!(svc.creation_work(ack.game_id()).unwrap(), pending);
        assert_eq!(svc.lookup_game_code(code.game_code()).unwrap(), None);
        assert_eq!(
            db.query(
                "SELECT designated_host_id FROM directory_hosted_nonterminal_games",
                &[]
            )
            .unwrap(),
            vec![vec![sql_text(account(1))]]
        );
        db.execute("DROP TRIGGER suppress_publication", &[])
            .unwrap();
        assert!(svc.publish_game(lobby).unwrap().published());
    }
}

#[test]
fn owner_code_lookup_denies_expired_terminal_index_remaining_beyond_cleanup_batch() {
    let db = Sqlite::new();
    let clock = TestRuntime::new();
    let rt = ByteRuntime {
        now: std::cell::Cell::new(clock.now.get()),
        byte: std::cell::Cell::new(251),
        calls: std::cell::Cell::new(0),
    };
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&clock, 1), GameState::Cancelled);
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    seed_history_backlog(&db, &terminal, &clock, 101);
    rt.now.set(HISTORY_EXPIRY - 1);
    assert_eq!(
        svc.game_code(terminal.game_id()).unwrap(),
        terminal.game_code().cloned()
    );
    rt.now.set(HISTORY_EXPIRY);
    assert_eq!(svc.game_code(terminal.game_id()).unwrap(), None);
    // Read denial is independent of physical deletion of the target.
    assert_eq!(index_row(&db, terminal.game_id()).len(), 1);
    assert_eq!(
        svc.lookup_game_code(terminal.game_code().unwrap()).unwrap(),
        None
    );
}

#[test]
fn expired_history_code_is_reusable_despite_bounded_backlog_and_delayed_primary_purge() {
    let db = Sqlite::new();
    let clock = TestRuntime::new();
    let rt = ByteRuntime {
        now: std::cell::Cell::new(clock.now.get()),
        byte: std::cell::Cell::new(251),
        calls: std::cell::Cell::new(0),
    };
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let terminal = started_terminal(&svc, &rt, command(&clock, 1), GameState::Resolved);
    assert_eq!(terminal.game_code().unwrap().as_str(), "99999999");
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    seed_history_backlog(&db, &terminal, &clock, 201);
    clock.now.set(HISTORY_EXPIRY - 1);
    rt.now.set(clock.now.get());
    let next = svc
        .claim_game(account(2), command(&clock, 2), fingerprint())
        .unwrap();
    let ready = svc
        .acknowledge_creation(creation_proof(
            next.game_id(),
            next.account_id(),
            next.command_id(),
            next.fingerprint(),
            next.created_at(),
            0,
        ))
        .unwrap();
    rt.now.set(HISTORY_EXPIRY);
    clock.now.set(HISTORY_EXPIRY);
    let allocated = svc.allocate_game_code(ready).unwrap();
    assert_eq!(allocated.game_code(), terminal.game_code().unwrap());
    assert!(index_row(&db, terminal.game_id()).is_empty());
    let lobby = projection(
        next.game_id(),
        next.account_id(),
        next.fingerprint(),
        GameState::AwaitingPlayers,
        1,
        Some(allocated.game_code().clone()),
        next.created_at(),
        None,
        None,
        None,
    );
    svc.publish_game(lobby).unwrap();
    let newer_index = index_row(&db, next.game_id());
    svc.release_game(TerminalProof::new(terminal.clone()))
        .unwrap();
    assert_eq!(index_row(&db, next.game_id()), newer_index);
    assert_eq!(
        svc.lookup_game_code(allocated.game_code()).unwrap(),
        Some(next.game_id())
    );
    assert_eq!(
        svc.confirm_reservation(next.game_id()).unwrap().game_id(),
        next.game_id()
    );
}

#[test]
fn terminal_index_expiry_alarm_drains_exactly_bounded_batches() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let svc = DirectoryService::new(&db, &rt).unwrap();
    for n in 1..=101 {
        let terminal = started_terminal(&svc, &rt, command(&rt, n), GameState::Cancelled);
        svc.release_game(TerminalProof::new(terminal)).unwrap();
    }
    let count = || {
        db.query("SELECT count(*) FROM directory_game_index", &[])
            .unwrap()
    };
    rt.now.set(HISTORY_EXPIRY - 1);
    svc.cleanup().unwrap();
    svc.cleanup().unwrap();
    assert_eq!(count(), vec![vec![SqlValue::Integer(101)]]);
    assert_eq!(svc.next_deadline().unwrap(), Some(HISTORY_EXPIRY));
    rt.now.set(HISTORY_EXPIRY);
    svc.cleanup().unwrap();
    assert_eq!(count(), vec![vec![SqlValue::Integer(1)]]);
    assert_eq!(svc.next_deadline().unwrap(), Some(HISTORY_EXPIRY));
    svc.cleanup().unwrap();
    assert_eq!(count(), vec![vec![SqlValue::Integer(0)]]);
    assert_eq!(svc.next_deadline().unwrap(), None);
}

#[test]
fn terminal_release_at_history_expiry_purges_only_old_game_and_never_resurrects_it() {
    for outcome in [GameState::Resolved, GameState::Cancelled] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let svc = DirectoryService::new(&db, &rt).unwrap();
        let terminal = started_terminal(&svc, &rt, command(&rt, 1), outcome);
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap();
        let before = index_row(&db, terminal.game_id());
        assert_eq!(before.len(), 1);
        assert!(
            db.query("SELECT game_id FROM directory_game_creations", &[])
                .unwrap()
                .is_empty()
        );
        assert!(
            db.query(
                "SELECT game_id FROM directory_hosted_nonterminal_games",
                &[]
            )
            .unwrap()
            .is_empty()
        );
        rt.now.set(HISTORY_EXPIRY - 1);
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap();
        assert_eq!(index_row(&db, terminal.game_id()), before);
        let next = svc
            .claim_game(account(2), command(&rt, 2), fingerprint())
            .unwrap();
        rt.now.set(HISTORY_EXPIRY);
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap();
        assert!(index_row(&db, terminal.game_id()).is_empty());
        assert_eq!(svc.game_code(terminal.game_id()).unwrap(), None);
        assert_eq!(
            svc.confirm_reservation(next.game_id()).unwrap().game_id(),
            next.game_id()
        );
        let old_publication = projection(
            terminal.game_id(),
            terminal.designated_host_id(),
            terminal.fingerprint(),
            GameState::InProgress,
            2,
            terminal.game_code().cloned(),
            terminal.created_at(),
            terminal.started_at(),
            None,
            None,
        );
        assert_eq!(
            svc.publish_game(old_publication),
            Err(directory::DirectoryError::UnknownGame)
        );
        svc.release_game(TerminalProof::new(terminal.clone()))
            .unwrap();
        assert!(index_row(&db, terminal.game_id()).is_empty());
        assert_eq!(
            svc.confirm_reservation(next.game_id()).unwrap().game_id(),
            next.game_id()
        );
    }
}
