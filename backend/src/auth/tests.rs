#![allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]

use super::*;
use crate::storage::{Row, SqlValue, StorageError};
use crate::{limits::*, security::DIGEST_BYTES, storage::migrate};
use brews_config::env::backend::{
    ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_PARALLELISM,
};
use rusqlite::{Connection, types::Value};
use std::cell::{Cell, RefCell};

struct Sqlite(RefCell<Connection>);
impl Sqlite {
    fn new() -> Self {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        Self(RefCell::new(db))
    }
}
fn sql_params(params: &[SqlValue]) -> Vec<Value> {
    params
        .iter()
        .map(|value| match value {
            SqlValue::Null => Value::Null,
            SqlValue::Integer(value) => Value::Integer(*value),
            SqlValue::Text(value) => Value::Text(value.clone()),
            SqlValue::Blob(value) => Value::Blob(value.clone()),
        })
        .collect()
}
impl Database for Sqlite {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        let db = self.0.borrow();
        let mut statement = db.prepare(sql).map_err(|_| StorageError)?;
        let count = statement.column_count();
        let rows = statement
            .query_map(rusqlite::params_from_iter(sql_params(params)), |row| {
                (0..count)
                    .map(|index| row.get::<_, Value>(index))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|_| StorageError)?;
        rows.map(|row| {
            row.map_err(|_| StorageError)?
                .into_iter()
                .map(|value| match value {
                    Value::Null => Ok(SqlValue::Null),
                    Value::Integer(value) => Ok(SqlValue::Integer(value)),
                    Value::Text(value) => Ok(SqlValue::Text(value)),
                    Value::Blob(value) => Ok(SqlValue::Blob(value)),
                    Value::Real(_) => Err(StorageError),
                })
                .collect()
        })
        .collect()
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        self.0
            .borrow()
            .execute(sql, rusqlite::params_from_iter(sql_params(params)))
            .map(|_| ())
            .map_err(|_| StorageError)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.0
            .borrow()
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| StorageError)?;
        let result = operation();
        self.0
            .borrow()
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })
            .map_err(|_| StorageError)?;
        result
    }
}
struct TestRuntime {
    now: Cell<i64>,
    random: Cell<u8>,
}
impl TestRuntime {
    fn new() -> Self {
        Self {
            now: Cell::new(0),
            random: Cell::new(0),
        }
    }
}
impl Runtime for TestRuntime {
    fn now_ms(&self) -> i64 {
        self.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        // Deterministic fixture entropy only.
        bytes.fill(self.random.get());
        self.random.set(self.random.get().wrapping_add(1));
        Ok(())
    }
}

const ACCOUNT_ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5711";

fn seed_account(db: &Sqlite) {
    migrate(db).unwrap();
    db.execute("INSERT INTO accounts(account_id,username,role,status,credential_epoch,created_at) VALUES(?,'abcdefghij','host','pending_enrollment',0,0)", &[SqlValue::Text(ACCOUNT_ID.into())]).unwrap();
}

fn entity_id(index: i64) -> String {
    format!("01890f3e-53b7-7d28-9b05-{index:012x}")
}

#[test]
fn auth_policy_defaults_match_configuration_defaults() {
    assert_eq!(
        AuthPolicy::default(),
        AuthPolicy {
            m_cost: ARGON2_DEFAULT_MEMORY_KIB,
            t_cost: ARGON2_DEFAULT_ITERATIONS,
            p_cost: ARGON2_PARALLELISM,
        }
    );
}

#[test]
fn cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size() {
    let db = Sqlite::new();
    seed_account(&db);
    let rt = TestRuntime::new();
    rt.now
        .set(ACCESS_LINK_LIFETIME_MS + ACCESS_LINK_METADATA_RETENTION_MS);
    for index in 0..=CLEANUP_BATCH_SIZE {
        let id = entity_id(index);
        let mut digest = vec![0u8; DIGEST_BYTES];
        digest[..8].copy_from_slice(&index.to_be_bytes());
        db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at) VALUES(?,?,?,'enrollment_only',0,0,?)", &[SqlValue::Text(id.clone()), SqlValue::Text(ACCOUNT_ID.into()), SqlValue::Blob(digest.clone()), SqlValue::Integer(SESSION_LIFETIME_MS)]).unwrap();
        db.execute("INSERT INTO command_receipts(actor_account_id,command_id,request_fingerprint,outcome,completed_at,expires_at) VALUES(?,?,?,'{}',0,?)", &[SqlValue::Text(ACCOUNT_ID.into()), SqlValue::Text(id.clone()), SqlValue::Blob(digest.clone()), SqlValue::Integer(COMMAND_RECEIPT_RETENTION_MS)]).unwrap();
        db.execute("INSERT INTO rate_limit_buckets(scope,subject_key,window_started_at,attempt_count,expires_at) VALUES('account_login',?,0,1,?)", &[SqlValue::Blob(digest.clone()), SqlValue::Integer(RATE_LIMIT_WINDOW_MS)]).unwrap();
        db.execute("INSERT INTO access_links(link_id,account_id,purpose,token_verifier,credential_epoch,issued_at,expires_at) VALUES(?,?,'enrollment',?,0,0,?)", &[SqlValue::Text(id.clone()), SqlValue::Text(ACCOUNT_ID.into()), SqlValue::Blob(digest), SqlValue::Integer(ACCESS_LINK_LIFETIME_MS)]).unwrap();
        db.execute("INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) VALUES(?,?,?,?,0,?)", &[SqlValue::Text(ACCOUNT_ID.into()), SqlValue::Text(id.clone()), SqlValue::Text(ACCOUNT_ID.into()), SqlValue::Text(id), SqlValue::Integer(SESSION_LIFETIME_MS)]).unwrap();
    }
    let key = [0u8; RATE_LIMIT_KEY_MIN_BYTES];
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    service.cleanup().unwrap();
    let counts = || {
        db.query("SELECT (SELECT count(*) FROM account_sessions),(SELECT count(*) FROM command_receipts),(SELECT count(*) FROM rate_limit_buckets),(SELECT count(*) FROM access_links),(SELECT count(*) FROM account_socket_close_work)", &[]).unwrap()
    };
    assert_eq!(
        counts(),
        vec![vec![
            SqlValue::Integer(1),
            SqlValue::Integer(1),
            SqlValue::Integer(1),
            SqlValue::Integer(1),
            SqlValue::Integer(CLEANUP_BATCH_SIZE)
        ]]
    );
    assert_eq!(
        db.query(
            "SELECT DISTINCT next_attempt_at FROM account_socket_close_work",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(
            rt.now.get() + SOCKET_CLOSE_INITIAL_DELAY_MS
        )]]
    );
    service.cleanup().unwrap();
    assert_eq!(
        counts(),
        vec![vec![
            SqlValue::Integer(0),
            SqlValue::Integer(0),
            SqlValue::Integer(0),
            SqlValue::Integer(0),
            SqlValue::Integer(CLEANUP_BATCH_SIZE + 1)
        ]]
    );
}

#[test]
fn access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline() {
    let db = Sqlite::new();
    seed_account(&db);
    let rt = TestRuntime::new();
    let consumed_at = ACCESS_LINK_LIFETIME_MS + 7;
    let revoked_at = consumed_at + 4;
    let deadline = revoked_at + ACCESS_LINK_METADATA_RETENTION_MS;
    rt.now.set(deadline - 1);
    db.execute("INSERT INTO access_links(link_id,account_id,purpose,token_verifier,credential_epoch,issued_at,expires_at,consumed_at,revoked_at) VALUES(?,?,'enrollment',?,0,0,?,?,?)", &[SqlValue::Text(entity_id(1)), SqlValue::Text(ACCOUNT_ID.into()), SqlValue::Blob(vec![0u8; DIGEST_BYTES]), SqlValue::Integer(ACCESS_LINK_LIFETIME_MS), SqlValue::Integer(consumed_at), SqlValue::Integer(revoked_at)]).unwrap();
    let key = [0u8; RATE_LIMIT_KEY_MIN_BYTES];
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert_eq!(service.next_deadline().unwrap(), Some(deadline));
    service.cleanup().unwrap();
    assert_eq!(
        db.query("SELECT count(*) FROM access_links", &[]).unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    rt.now.set(deadline);
    service.cleanup().unwrap();
    assert_eq!(
        db.query("SELECT count(*) FROM access_links", &[]).unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    assert_eq!(service.next_deadline().unwrap(), None);
}
