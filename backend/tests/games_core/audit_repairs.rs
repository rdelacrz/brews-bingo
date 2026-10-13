//! Focused Slice8 immutable-review regression cases.

use super::*;
use brews_contracts::games::{ConfigurationPatch, HostTransferInput};
use brews_domain::games::GameState;
fn names(rows: &[brews_backend::db::Row]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| {
            r.iter()
                .map(|c| match c {
                    SqlValue::Text(t) => t.clone(),
                    _ => "unexpected".into(),
                })
                .collect()
        })
        .collect()
}
fn audit(f: &Fixture) -> Vec<brews_backend::db::Row> {
    f.game
        .query(
            "SELECT operation,outcome FROM game_admin_audit ORDER BY audit_id",
            &[],
        )
        .unwrap()
}
#[test]
fn typed_transfer_invalid_confirmation_has_required_rejected_audit() {
    let f = Fixture::new();
    operator::saved(&f);
    let (_, token) = operator::actor(&f, 70, AccountRole::Admin);
    let result = f.service().prepare_host_transfer(
        f.auth().authorize_game_account(&token).unwrap(),
        f.command(80),
        &HostTransferInput {
            target_account_id: f.account,
            confirmed: false,
            expected_host_assignment_revision: 0,
        },
    );
    assert_eq!(result.err(), Some(GameError::InvalidInput));
    let rows = audit(&f);
    println!(
        "confirmation=false result=InvalidInput audit_rows={}",
        rows.len()
    );
    assert_eq!(
        rows,
        vec![vec![
            SqlValue::Text("host_transfer".into()),
            SqlValue::Text("rejected".into())
        ]]
    );
}
#[test]
fn typed_transfer_invalid_revision_has_required_rejected_audit() {
    let f = Fixture::new();
    operator::saved(&f);
    let (_, token) = operator::actor(&f, 70, AccountRole::Admin);
    let result = f.service().prepare_host_transfer(
        f.auth().authorize_game_account(&token).unwrap(),
        f.command(80),
        &HostTransferInput {
            target_account_id: f.account,
            confirmed: true,
            expected_host_assignment_revision: u64::MAX,
        },
    );
    assert_eq!(result.err(), Some(GameError::InvalidInput));
    let rows = audit(&f);
    println!(
        "revision=u64::MAX result=InvalidInput audit_rows={}",
        rows.len()
    );
    assert_eq!(
        rows,
        vec![vec![
            SqlValue::Text("host_transfer".into()),
            SqlValue::Text("rejected".into())
        ]]
    );
}
struct Exhaust<'a> {
    inner: &'a TestRuntime,
    word: u32,
    draws: std::cell::Cell<usize>,
}
impl brews_backend::auth::Runtime for Exhaust<'_> {
    fn now_ms(&self) -> i64 {
        self.inner.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), brews_backend::auth::AuthError> {
        if bytes.len() == 4 {
            self.draws.set(self.draws.get() + 1);
            bytes.copy_from_slice(&self.word.to_le_bytes());
            Ok(())
        } else {
            brews_backend::auth::Runtime::fill_random(self.inner, bytes)
        }
    }
}
#[test]
fn start_generation_budget_is_required_failed_audit() {
    let f = Fixture::new();
    let config = GameConfiguration {
        board_side_length: 2,
        numeric_upper_bound: 3,
        free_cell_positions: vec![
            brews_domain::games::CellPosition { row: 1, column: 1 },
            brews_domain::games::CellPosition { row: 1, column: 2 },
        ],
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let a = f.player(&code, 20, "One");
    let b = f.player(&code, 21, "Two");
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
    let reservation = f.directory().confirm_reservation(view.game_id()).unwrap();
    let (_, token) = operator::actor(&f, 70, AccountRole::Admin);
    let rt = Exhaust {
        inner: &f.rt,
        word: 0x07070707,
        draws: std::cell::Cell::new(0),
    };
    let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let result = service.start(
        f.auth().authorize_game_account(&token).unwrap(),
        f.command(80),
        view.view_revision(),
        &reservation,
        &observed,
    );
    assert_eq!(result.err(), Some(GameError::GenerationExhausted));
    let rows = audit(&f);
    println!(
        "start result=GenerationExhausted draws={} audit={:?}",
        rt.draws.get(),
        names(&rows)
    );
    assert!(
        f.game
            .query("SELECT player_id FROM game_boards", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        rows,
        vec![vec![
            SqlValue::Text("start".into()),
            SqlValue::Text("failed".into())
        ]]
    );
}
#[test]
fn random_generation_budget_is_required_failed_audit() {
    let f = Fixture::new();
    let (_, _, _, observed) = f.start_two_player_game();
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
            f.command(60),
            "1",
            view.view_revision(),
        )
        .unwrap();
    let view = f
        .service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            &observed,
        )
        .unwrap();
    let (_, token) = operator::actor(&f, 70, AccountRole::Admin);
    let before = f
        .game
        .query("SELECT * FROM game_calls ORDER BY sequence_no", &[])
        .unwrap();
    let rt = Exhaust {
        inner: &f.rt,
        word: 0,
        draws: std::cell::Cell::new(0),
    };
    let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let result = service.call_random(
        f.auth().authorize_game_account(&token).unwrap(),
        f.command(80),
        view.view_revision(),
    );
    assert_eq!(result.err(), Some(GameError::GenerationExhausted));
    let rows = audit(&f);
    println!(
        "random result=GenerationExhausted draws={} audit={:?}",
        rt.draws.get(),
        names(&rows)
    );
    assert_eq!(
        f.game
            .query("SELECT * FROM game_calls ORDER BY sequence_no", &[])
            .unwrap(),
        before
    );
    assert_eq!(
        rows,
        vec![vec![
            SqlValue::Text("call_random".into()),
            SqlValue::Text("failed".into())
        ]]
    );
}
#[test]
fn stream_preparation_does_not_claim_success_before_acceptance() {
    let f = Fixture::new();
    operator::saved(&f);
    let (_, token) = operator::actor(&f, 70, AccountRole::Admin);
    let _grant = f
        .service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&token).unwrap(),
            id(&f.rt, 80).parse().unwrap(),
        )
        .unwrap();
    let accepted = f
        .game
        .query("SELECT * FROM game_current_connections", &[])
        .unwrap();
    assert!(accepted.is_empty());
    let rows = audit(&f);
    println!(
        "stream accepted_connections={} audit={:?}",
        accepted.len(),
        names(&rows)
    );
    assert!(
        rows.is_empty(),
        "preparation has not established a connection"
    );
}
#[test]
fn invalid_typed_call_winner_cancel_configuration_sync_all_capture_independently() {
    let f = Fixture::new();
    operator::saved(&f);
    let (_, token) = operator::actor(&f, 70, AccountRole::Admin);
    let auth = || f.auth().authorize_game_account(&token).unwrap();
    assert_eq!(
        f.service()
            .call_manual_value(auth(), f.command(80), "invalid", u64::MAX)
            .err(),
        Some(GameError::InvalidInput)
    );
    assert_eq!(
        f.service()
            .submit_winner(
                auth(),
                f.command(81),
                id(&f.rt, 90).parse().unwrap(),
                u64::MAX
            )
            .err(),
        Some(GameError::InvalidInput)
    );
    assert_eq!(
        f.service()
            .cancel_game(auth(), f.command(82), GameState::New, false)
            .err(),
        Some(GameError::InvalidInput)
    );
    let patch =
        ConfigurationPatch::decode_json(br#"{"expected_revision":0,"player_capacity":0}"#).unwrap();
    assert_eq!(
        f.service()
            .update_configuration(auth(), f.command(83), &patch)
            .err(),
        Some(GameError::InvalidInput)
    );
    assert_eq!(
        f.service().sync_account(auth(), Some(u64::MAX), &[]).err(),
        Some(GameError::InvalidInput)
    );
    let rows = audit(&f);
    println!(
        "independent_invalid_typed_attempts=5 rejection_audits={}",
        rows.len()
    );
    assert_eq!(rows.len(), 5);
    assert!(
        rows.iter()
            .all(|r| r[1] == SqlValue::Text("rejected".into()))
    );
}

#[test]
fn bounded_audit_page_does_not_scan_every_live_row() {
    let f = Fixture::new();
    operator::saved(&f);
    let game = f.service().directory_projection().unwrap().game_id();
    let mut steps = vec![];
    for count in [400_u32, 4000_u32] {
        f.game.execute("DELETE FROM game_admin_audit", &[]).unwrap();
        for serial in 0..count {
            let mut bytes = [0_u8; 16];
            bytes[0..6].copy_from_slice(&(f.rt.now.get() as u64).to_be_bytes()[2..]);
            bytes[6] = 0x70;
            bytes[8] = 0x80;
            bytes[12..].copy_from_slice(&serial.to_be_bytes());
            f.game
                .execute(
                    "INSERT INTO game_admin_audit VALUES(?,?,?,?,?,?,?)",
                    &[
                        SqlValue::Text(uuid::Uuid::from_bytes(bytes).to_string()),
                        SqlValue::Text(f.account.to_string()),
                        SqlValue::Text(game.to_string()),
                        SqlValue::Text("configuration".into()),
                        SqlValue::Text("succeeded".into()),
                        SqlValue::Integer(f.rt.now.get()),
                        SqlValue::Integer(f.rt.now.get() + 7776000000),
                    ],
                )
                .unwrap();
        }
        let conn = f.game.conn.borrow();
        let mut statement=conn.prepare("SELECT audit_id,actor_account_id,target_game_id,operation,outcome,occurred_at,expires_at FROM game_admin_audit WHERE target_game_id=? AND expires_at>? AND (? IS NULL OR audit_id>?) ORDER BY audit_id LIMIT ?").unwrap();
        let mut cursor = statement
            .query(rusqlite::params![
                game.to_string(),
                f.rt.now.get(),
                Option::<String>::None,
                Option::<String>::None,
                2_i64
            ])
            .unwrap();
        let mut returned = 0;
        while cursor.next().unwrap().is_some() {
            returned += 1;
        }
        assert_eq!(returned, 2);
        drop(cursor);
        let full = statement.get_status(rusqlite::StatementStatus::FullscanStep);
        let vm = statement.get_status(rusqlite::StatementStatus::VmStep);
        println!(
            "audit_page_limit=1 live_rows={} returned=2 full_scan_steps={} sqlite_vm_steps={}",
            count, full, vm
        );
        steps.push(vm);
    }
    assert!(
        steps[1] <= steps[0] * 2,
        "tenfold live storage growth must not give tenfold work to a limit=1 page"
    );
}

struct SingleIdentityFailure<'a> {
    inner: &'a Sqlite,
    remaining: std::cell::Cell<bool>,
}
impl Database for SingleIdentityFailure<'_> {
    fn query(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<brews_backend::db::Row>, brews_backend::db::StorageError> {
        if sql == "SELECT game_id FROM game_operator_identity WHERE singleton=1"
            && self.remaining.replace(false)
        {
            return Err(brews_backend::db::StorageError);
        }
        self.inner.query(sql, params)
    }
    fn execute(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<(), brews_backend::db::StorageError> {
        self.inner.execute(sql, params)
    }
    fn transaction<T>(
        &self,
        f: impl FnOnce() -> Result<T, brews_backend::db::StorageError>,
    ) -> Result<T, brews_backend::db::StorageError> {
        self.inner.transaction(f)
    }
}
#[test]
fn explicit_creation_target_transient_identity_query_failure_is_capturable() {
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
    let db = SingleIdentityFailure {
        inner: &f.game,
        remaining: std::cell::Cell::new(true),
    };
    let service = GameService::new(&db, &f.rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let result = service.initialize(
        &work,
        f.auth().authorize_game_account(&f.token).unwrap(),
        &config,
    );
    assert_eq!(result.err(), Some(GameError::Storage));
    assert!(!db.remaining.get());
    assert!(
        f.game
            .query("SELECT * FROM game_record", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        audit(&f),
        vec![vec![
            SqlValue::Text("creation".into()),
            SqlValue::Text("failed".into())
        ]]
    );
}

#[test]
fn privileged_stream_acceptance_and_success_are_atomic_and_replay_is_quiet() {
    for fault in ["IGNORE", "ABORT,'controlled stream audit fault'"] {
        let f = Fixture::new();
        operator::saved(&f);
        let (_, token) = operator::actor(&f, 70, AccountRole::Admin);
        let proof = || f.auth().authorize_game_account(&token).unwrap();
        let predecessor = f
            .service()
            .prepare_account_connection(proof(), id(&f.rt, 80).parse().unwrap())
            .unwrap();
        assert!(audit(&f).is_empty());
        f.service()
            .accept_account_connection(&predecessor, proof())
            .unwrap();
        let current = f
            .game
            .query("SELECT * FROM game_current_connections", &[])
            .unwrap();
        f.service()
            .accept_account_connection(&predecessor, proof())
            .unwrap();
        assert_eq!(audit(&f).len(), 1);
        let successor = f
            .service()
            .prepare_account_connection(proof(), id(&f.rt, 81).parse().unwrap())
            .unwrap();
        let prepared = f
            .game
            .query("SELECT * FROM game_connection_grants ORDER BY 1", &[])
            .unwrap();
        f.game.execute(&format!("CREATE TRIGGER deny_stream_success BEFORE INSERT ON game_admin_audit WHEN NEW.outcome='succeeded' BEGIN SELECT RAISE({fault}); END"), &[]).unwrap();
        assert_eq!(
            f.service()
                .accept_account_connection(&successor, proof())
                .err(),
            Some(GameError::Storage)
        );
        assert_eq!(
            f.game
                .query("SELECT * FROM game_current_connections", &[])
                .unwrap(),
            current
        );
        assert_eq!(
            f.game
                .query("SELECT * FROM game_connection_grants ORDER BY 1", &[])
                .unwrap(),
            prepared
        );
        let mut outcomes = names(&audit(&f));
        outcomes.sort();
        assert_eq!(
            outcomes,
            vec![
                vec!["account_stream".to_owned(), "failed".to_owned()],
                vec!["account_stream".to_owned(), "succeeded".to_owned()]
            ]
        );
        f.game
            .execute("DROP TRIGGER deny_stream_success", &[])
            .unwrap();
        f.service()
            .accept_account_connection(&successor, proof())
            .unwrap();
        let count = audit(&f).len();
        f.service()
            .accept_account_connection(&successor, proof())
            .unwrap();
        assert_eq!(audit(&f).len(), count);
    }
}
