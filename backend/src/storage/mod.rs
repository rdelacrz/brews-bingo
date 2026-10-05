//! SQL port shared by real SQLite native tests and the Durable Object adapter.
//! Allocation-conscious; values are owned only at the adapter boundary.
use crate::limits::JS_SAFE_INTEGER_MAX;
use std::fmt;

pub const CURRENT_SCHEMA_VERSION: i64 = 2;
const VERSION_ONE: i64 = 1;
pub(crate) mod management_schema;

#[derive(Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
}
impl fmt::Debug for SqlValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Null => "Null",
            Self::Integer(_) => "Integer([redacted])",
            Self::Text(_) => "Text([redacted])",
            Self::Blob(_) => "Blob([redacted])",
        })
    }
}
pub type Row = Vec<SqlValue>;
mod error;
pub use error::StorageError;

/// Implementations must roll back on outer Err. Nested business errors may commit
/// durable rejection metadata. The closure must execute synchronously/serialized.
pub trait Database {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError>;
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError>;
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError>;
}

pub mod schema;

fn metadata_statement() -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS storage_metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL CHECK(schema_version>={VERSION_ONE}),last_observed_ms INTEGER NOT NULL CHECK(last_observed_ms BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),command_floor_ms INTEGER NOT NULL CHECK(command_floor_ms BETWEEN 0 AND {JS_SAFE_INTEGER_MAX})) STRICT"
    )
}

/// Apply contiguous v1 -> v2 migrations atomically; unknown versions fail closed.
pub fn migrate<D: Database>(db: &D) -> Result<(), StorageError> {
    db.transaction(|| {
        db.execute(&metadata_statement(), &[])?;
        let rows = db.query("SELECT schema_version FROM storage_metadata WHERE singleton=1", &[])?;
        let mut version = if rows.is_empty() {
            for sql in schema::v1_statements() { db.execute(&sql, &[])?; }
            db.execute("INSERT INTO storage_metadata(singleton,schema_version,last_observed_ms,command_floor_ms) VALUES(1,?,0,0)", &[SqlValue::Integer(VERSION_ONE)])?;
            VERSION_ONE
        } else {
            match rows.as_slice() {
                [row] => match row.as_slice() { [SqlValue::Integer(v)] => *v, _ => return Err(StorageError) },
                _ => return Err(StorageError),
            }
        };
        if version == VERSION_ONE {
            for sql in management_schema::v2_statements() { db.execute(&sql, &[])?; }
            db.execute("UPDATE storage_metadata SET schema_version=2 WHERE singleton=1 AND schema_version=1", &[])?;
            version = 2;
        }
        if version != CURRENT_SCHEMA_VERSION { return Err(StorageError); }
        Ok(())
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn metadata_ddl_matches_the_pre_refactor_snapshot() {
        assert_eq!(
            format!("{:x}", Sha256::digest(metadata_statement().as_bytes())),
            "f02dd125871f423424133d513e42fed25a3976b5795e149d57ab7c567d217b0e"
        );
    }

    #[test]
    fn metadata_clock_bounds_match_the_named_safe_integer_limit() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(&metadata_statement()).unwrap();
        for time in [0, JS_SAFE_INTEGER_MAX, JS_SAFE_INTEGER_MAX + 1] {
            assert_eq!(
                db.execute("INSERT INTO storage_metadata(singleton,schema_version,last_observed_ms,command_floor_ms) VALUES(1,?,?,?)", rusqlite::params![CURRENT_SCHEMA_VERSION, time, time]).is_ok(),
                time <= JS_SAFE_INTEGER_MAX
            );
            db.execute("DELETE FROM storage_metadata", []).unwrap();
        }
    }
}
