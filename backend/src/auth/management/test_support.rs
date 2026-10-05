#![allow(
    dead_code,
    reason = "Shared fixtures differ across integration targets."
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
use crate::storage::{Database, Row, SqlValue, StorageError};
use rusqlite::{
    Connection,
    types::{Value, ValueRef},
};
use std::cell::{Cell, RefCell};

pub struct Sqlite {
    pub conn: RefCell<Connection>,
}
impl Sqlite {
    pub fn new() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        Self {
            conn: RefCell::new(conn),
        }
    }
}
impl Database for Sqlite {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        let conn = self.conn.borrow();
        let mut statement = conn.prepare(sql).map_err(|_| StorageError)?;
        let count = statement.column_count();
        let values: Vec<Value> = params.iter().map(value).collect();
        let mut cursor = statement
            .query(rusqlite::params_from_iter(values))
            .map_err(|_| StorageError)?;
        let mut out = Vec::new();
        while let Some(row) = cursor.next().map_err(|_| StorageError)? {
            let mut cells = Vec::new();
            for i in 0..count {
                cells.push(match row.get_ref(i).map_err(|_| StorageError)? {
                    ValueRef::Null => SqlValue::Null,
                    ValueRef::Integer(v) => SqlValue::Integer(v),
                    ValueRef::Text(v) => {
                        SqlValue::Text(std::str::from_utf8(v).map_err(|_| StorageError)?.to_owned())
                    }
                    ValueRef::Blob(v) => SqlValue::Blob(v.to_vec()),
                    ValueRef::Real(_) => return Err(StorageError),
                });
            }
            out.push(cells);
        }
        Ok(out)
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        let values: Vec<Value> = params.iter().map(value).collect();
        self.conn
            .borrow()
            .execute(sql, rusqlite::params_from_iter(values))
            .map(|_| ())
            .map_err(|_| StorageError)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.conn
            .borrow()
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| StorageError)?;
        match operation() {
            Ok(out) => {
                self.conn
                    .borrow()
                    .execute_batch("COMMIT")
                    .map_err(|_| StorageError)?;
                Ok(out)
            }
            Err(e) => {
                self.conn
                    .borrow()
                    .execute_batch("ROLLBACK")
                    .map_err(|_| StorageError)?;
                Err(e)
            }
        }
    }
}
fn value(v: &SqlValue) -> Value {
    match v {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => Value::Integer(*i),
        SqlValue::Text(s) => Value::Text(s.clone()),
        SqlValue::Blob(b) => Value::Blob(b.clone()),
    }
}

pub struct TestRuntime {
    pub now: Cell<i64>,
    pub random: Cell<u64>,
    pub fail: Cell<bool>,
    pub advance_random_ms: Cell<i64>,
}
impl crate::auth::Runtime for TestRuntime {
    fn now_ms(&self) -> i64 {
        self.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), crate::auth::AuthError> {
        if self.fail.get() {
            return Err(crate::auth::AuthError::Crypto);
        }
        // Deterministic fixture entropy only, never production randomness.
        for b in bytes {
            let n = self
                .random
                .get()
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1);
            self.random.set(n);
            *b = (n >> 32) as u8;
        }
        self.now
            .set(self.now.get() + self.advance_random_ms.replace(0));
        Ok(())
    }
}
impl TestRuntime {
    pub fn new() -> Self {
        Self {
            now: Cell::new(1_800_000_000_000),
            random: Cell::new(1),
            fail: Cell::new(false),
            advance_random_ms: Cell::new(0),
        }
    }
}
