//! Owner-local fixed SQL. No browser commands, peer database or game ingress.
#![cfg_attr(
    test,
    allow(
        dead_code,
        reason = "Exact-source integration targets exercise different owner surfaces."
    )
)]
#[path = "directory_games.rs"]
mod game_sql;
use crate::directory::{DirectoryError, RemovalGrant, RemovalReleaseAck};
use crate::{
    auth::Runtime,
    db::{Database, SqlValue, StorageError},
    limits::{CLEANUP_BATCH_SIZE, COMMAND_RECEIPT_RETENTION_MS, JS_SAFE_INTEGER_MAX},
};
use brews_domain::ids::{AccountId, OperationId};
/// Bind only to the environment's private `GAME_DIRECTORY` singleton `directory`.
/// Accounts must persist an authorized removal intent before invoking this service.
pub struct DirectoryService<'a, D: Database, R: Runtime> {
    db: &'a D,
    runtime: &'a R,
}

use crate::db::schema::directory_schema::{
    self, DIRECTORY_SCHEMA_VERSION, DIRECTORY_TABLES, REMOVAL_SCHEMA_VERSION, REMOVAL_TABLES,
};
// Verified runtime metadata is not application state: DO names and alarm storage.
const MINIFLARE_METADATA_TABLE: &str = "__miniflare_do_name";
const WORKERD_METADATA_TABLE: &str = "_cf_METADATA";

/// Initialize only this isolated Directory owner's versioned schema.
pub fn migrate_directory<D: Database>(db: &D) -> Result<(), DirectoryError> {
    db.transaction(|| {
        let tables = application_tables(db, DIRECTORY_TABLES.len() + 1)?;
        let version = if tables.is_empty() {
            directory_schema::initialize_removal(db)?;
            db.execute("INSERT INTO directory_metadata(singleton,schema_version,last_observed_ms,command_floor_ms) VALUES(1,?,0,0)", &[SqlValue::Integer(REMOVAL_SCHEMA_VERSION)])?;
            REMOVAL_SCHEMA_VERSION
        } else { schema_version(db)? };
        if version == REMOVAL_SCHEMA_VERSION {
            validate_inventory(db, &REMOVAL_TABLES)?;
            directory_schema::add_game_coordination(db)?;
            db.execute("UPDATE directory_metadata SET schema_version=? WHERE singleton=1 AND schema_version=?", &[SqlValue::Integer(DIRECTORY_SCHEMA_VERSION), SqlValue::Integer(REMOVAL_SCHEMA_VERSION)])?;
        } else if version != DIRECTORY_SCHEMA_VERSION { return Err(StorageError); }
        validate_schema(db).map_err(|_| StorageError)?;
        Ok(())
    }).map_err(DirectoryError::from)
}
fn application_tables<D: Database>(
    db: &D,
    limit: usize,
) -> Result<Vec<crate::db::Row>, StorageError> {
    db.query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT GLOB 'sqlite_*' AND name NOT IN (?,?) ORDER BY name LIMIT ?", &[SqlValue::Text(MINIFLARE_METADATA_TABLE.into()), SqlValue::Text(WORKERD_METADATA_TABLE.into()), SqlValue::Integer(limit as i64)])
}
fn schema_version<D: Database>(db: &D) -> Result<i64, StorageError> {
    let rows = db.query(
        "SELECT schema_version FROM directory_metadata WHERE singleton=1",
        &[],
    )?;
    match rows.as_slice() {
        [row] => match row.as_slice() {
            [SqlValue::Integer(version)] => Ok(*version),
            _ => Err(StorageError),
        },
        _ => Err(StorageError),
    }
}
fn validate_inventory<D: Database>(db: &D, expected: &[&str]) -> Result<(), StorageError> {
    let tables = db.query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT GLOB 'sqlite_*' AND name NOT IN (?,?) ORDER BY name LIMIT ?",
        &[SqlValue::Text(MINIFLARE_METADATA_TABLE.into()), SqlValue::Text(WORKERD_METADATA_TABLE.into()), SqlValue::Integer((expected.len() + 1) as i64)],
    )?;
    if tables.len() != expected.len()
        || !tables.iter().zip(expected).all(|(row, expected)| matches!(row.as_slice(), [SqlValue::Text(actual)] if actual == expected)) {
        return Err(StorageError);
    }
    Ok(())
}
fn validate_schema<D: Database>(db: &D) -> Result<(), DirectoryError> {
    validate_inventory(db, &DIRECTORY_TABLES)?;
    if schema_version(db)? != DIRECTORY_SCHEMA_VERSION {
        return Err(DirectoryError::Storage);
    }
    Ok(())
}

fn operation_timestamp(operation: OperationId) -> Result<i64, DirectoryError> {
    let id = uuid::Uuid::parse_str(&operation.to_string()).map_err(|_| DirectoryError::Storage)?;
    let b = id.as_bytes();
    Ok(i64::from_be_bytes([
        0, 0, b[0], b[1], b[2], b[3], b[4], b[5],
    ]))
}
fn parse_operation_row(
    rows: &[crate::db::Row],
    operation: OperationId,
    now: i64,
) -> Result<Option<AccountId>, DirectoryError> {
    if rows.is_empty() {
        return Ok(None);
    }
    let [row] = rows else {
        return Err(DirectoryError::Storage);
    };
    let [SqlValue::Text(account), SqlValue::Integer(at)] = row.as_slice() else {
        return Err(DirectoryError::Storage);
    };
    let account = account
        .parse::<AccountId>()
        .map_err(|_| DirectoryError::Storage)?;
    let issued = operation_timestamp(operation)?;
    if issued <= 0 || !(issued..=now).contains(at) {
        return Err(DirectoryError::Storage);
    }
    Ok(Some(account))
}

fn parse_completion_row(
    row: &crate::db::Row,
    now: i64,
) -> Result<(OperationId, i64), DirectoryError> {
    let [SqlValue::Text(operation), account, SqlValue::Integer(at)] = row.as_slice() else {
        return Err(DirectoryError::Storage);
    };
    let operation = operation
        .parse::<OperationId>()
        .map_err(|_| DirectoryError::Storage)?;
    parse_operation_row(
        &[vec![account.clone(), SqlValue::Integer(*at)]],
        operation,
        now,
    )?;
    Ok((operation, *at))
}

impl<'a, D: Database, R: Runtime> DirectoryService<'a, D, R> {
    /// Bind to an explicitly migrated isolated Directory database.
    /// Clock and durable coordination state are revalidated in each transaction.
    pub fn new(db: &'a D, runtime: &'a R) -> Result<Self, DirectoryError> {
        validate_schema(db)?;
        Ok(Self { db, runtime })
    }
    /// Earliest creation retry or completion expiry in trusted epoch milliseconds.
    /// Does not compact rows; an expired backlog stays due until cleanup drains it.
    /// Pending creation has retry/deadline work, never TTL deletion; removal gates do not expire.
    ///
    /// # Errors
    /// Fails closed for invalid clocks, metadata, or earliest completion evidence.
    pub fn next_deadline(&self) -> Result<Option<i64>, DirectoryError> {
        self.transaction_before_compaction(|now| {
            let mut deadline = self.next_creation_deadline(now)?;
            for select in [
                "SELECT operation_id,account_id,completed_at FROM directory_removal_receipts ORDER BY completed_at,operation_id LIMIT 1",
                "SELECT operation_id,account_id,completed_at FROM directory_removal_rejections ORDER BY completed_at,operation_id LIMIT 1",
            ] {
                for row in self.db.query(select, &[])? {
                    let (_, completed) = parse_completion_row(&row, now)?;
                    let expires = completed
                        .checked_add(COMMAND_RECEIPT_RETENTION_MS)
                        .filter(|at| *at <= JS_SAFE_INTEGER_MAX)
                        .ok_or(DirectoryError::Storage)?;
                    deadline = Some(deadline.map_or(expires, |earliest: i64| earliest.min(expires)));
                }
            }
            Ok(deadline.map(|at| at.max(now)))
        })
    }
    /// Compact at most CLEANUP_BATCH_SIZE completions per table, atomically.
    /// Advances the durable admission floor but never purges pending work or gates.
    /// Call next_deadline afterwards to reschedule any remaining bounded backlog.
    ///
    /// # Errors
    /// Invalid clock/metadata, malformed due rows and SQL failures roll back all work.
    pub fn cleanup(&self) -> Result<(), DirectoryError> {
        self.transaction_before_compaction(|now| self.compact_completions(now))
    }
    /// Acquire before the hosted-game check; keep the grant until Accounts has
    /// committed its authorized removal intent. Matching active retries replay it.
    /// Errors never authorize an account mutation; `Busy` belongs to another intent.
    pub fn acquire_removal(
        &self,
        operation_id: OperationId,
        account_id: AccountId,
    ) -> Result<RemovalGrant, DirectoryError> {
        self.transaction(|now| {
            self.prior_rejection(operation_id, account_id, now)?;
            let receipt = self.receipt(operation_id, now)?;
            if receipt == Some(account_id) { return Err(DirectoryError::Completed); }
            if receipt.is_some() { return Err(DirectoryError::OperationMismatch); }
            let pending = self.pending(operation_id, now)?;
            if pending == Some(account_id) {
                self.require_gate(account_id)?;
                return Ok(RemovalGrant { operation_id, account_id });
            }
            if pending.is_some() { return Err(DirectoryError::OperationMismatch); }
            self.admit_operation(operation_id, now)?;
            if !self.db.query("SELECT operation_id FROM directory_removal_pending WHERE account_id=?", &[SqlValue::Text(account_id.to_string())])?.is_empty() {
                return Err(DirectoryError::Busy);
            }
            self.db.execute("INSERT INTO account_assignment_gates(account_id) VALUES(?)", &[SqlValue::Text(account_id.to_string())])?;
            self.db.execute("INSERT INTO directory_removal_pending(operation_id,account_id,created_at) VALUES(?,?,?)", &[SqlValue::Text(operation_id.to_string()),SqlValue::Text(account_id.to_string()),SqlValue::Integer(now)])?;
            if !self.db.query("SELECT game_id FROM directory_hosted_nonterminal_games WHERE designated_host_id=? LIMIT 1", &[SqlValue::Text(account_id.to_string())])?.is_empty() {
                self.db.execute("DELETE FROM directory_removal_pending WHERE operation_id=?", &[SqlValue::Text(operation_id.to_string())])?;
                self.db.execute("DELETE FROM account_assignment_gates WHERE account_id=?", &[SqlValue::Text(account_id.to_string())])?;
                self.db.execute("INSERT INTO directory_removal_rejections(operation_id,account_id,completed_at) VALUES(?,?,?)", &[SqlValue::Text(operation_id.to_string()),SqlValue::Text(account_id.to_string()),SqlValue::Integer(now)])?;
                return Err(DirectoryError::HostedGame);
            }
            Ok(RemovalGrant { operation_id, account_id })
        })
    }
    /// Complete only the exact active intent, or replay its retained completion
    /// receipt without touching a newer gate. Unknown/stale commands never ACK.
    pub fn release_removal(
        &self,
        operation_id: OperationId,
        account_id: AccountId,
    ) -> Result<RemovalReleaseAck, DirectoryError> {
        self.transaction(|now| {
            self.prior_rejection(operation_id, account_id, now)?;
            let receipt = self.receipt(operation_id, now)?;
            if receipt == Some(account_id) {
                return Ok(RemovalReleaseAck {
                    operation_id,
                    account_id,
                });
            }
            if receipt.is_some() {
                return Err(DirectoryError::OperationMismatch);
            }
            let pending = self.pending(operation_id, now)?;
            if pending.is_none() {
                self.admit_operation(operation_id, now)?;
                return Err(DirectoryError::UnknownOperation);
            }
            if pending != Some(account_id) {
                return Err(DirectoryError::OperationMismatch);
            }
            self.clear_pending_gate(operation_id, account_id)?;
            self.record_release(operation_id, account_id, now)?;
            Ok(RemovalReleaseAck {
                operation_id,
                account_id,
            })
        })
    }
    /// Trusted Accounts-only cancellation/recovery, never account mutation authority.
    /// Accounts must durably enter Aborting or Committed before invoking this method.
    /// Fresh unknown intents are sealed against delayed acquire; old absent intents
    /// use the nonretreating admission floor, including after receipt compaction.
    /// Validate retained identity/time evidence before compaction and preserve any
    /// other operation's gate. No public or gameplay caller may invoke this boundary.
    ///
    /// # Errors
    /// Rejects target mismatch, malformed or conflicting coordination state, orphan
    /// gates, invalid/regressed clocks and future or zero-time unknown operations.
    pub fn reconcile_removal(
        &self,
        operation_id: OperationId,
        account_id: AccountId,
    ) -> Result<RemovalReleaseAck, DirectoryError> {
        self.transaction_before_compaction(|now| {
            let rows = self.db.query(
                "SELECT account_id,completed_at FROM directory_removal_rejections WHERE operation_id=?",
                &[SqlValue::Text(operation_id.to_string())],
            )?;
            let rejected = parse_operation_row(&rows, operation_id, now)?;
            let receipt_rows = self.db.query(
                "SELECT account_id,completed_at FROM directory_removal_receipts WHERE operation_id=?",
                &[SqlValue::Text(operation_id.to_string())],
            )?;
            let receipt = parse_operation_row(&receipt_rows, operation_id, now)?;
            let pending = self.pending(operation_id, now)?;
            let states = [pending, receipt, rejected];
            if states.into_iter().flatten().any(|account| account != account_id) {
                return Err(DirectoryError::OperationMismatch);
            }
            if states.into_iter().flatten().count() > 1 { return Err(DirectoryError::Storage); }
            self.validate_account_gate(account_id, now)?;
            self.compact_completions(now)?;
            if receipt.is_some() { return Ok(RemovalReleaseAck { operation_id, account_id }); }
            if pending.is_some() {
                self.clear_pending_gate(operation_id, account_id)?;
            } else if rejected.is_none() {
                let issued = operation_timestamp(operation_id)?;
                if !(1..=now).contains(&issued) { return Err(DirectoryError::StaleOperation); }
                // The durable floor is the seal once this ID can never be admitted again.
                if issued <= self.command_floor()? { return Ok(RemovalReleaseAck { operation_id, account_id }); }
            }
            self.record_release(operation_id, account_id, now)?;
            self.db.execute("DELETE FROM directory_removal_rejections WHERE operation_id=?", &[SqlValue::Text(operation_id.to_string())])?;
            Ok(RemovalReleaseAck { operation_id, account_id })
        })
    }
    /// Test-only future-assignment tracer: all real assignment ingress must use the
    /// same serialized gate check and index write before gameplay is introduced.
    #[cfg(test)]
    pub fn seed_nonterminal_assignment(
        &self,
        game_id: brews_domain::ids::GameId,
        account_id: AccountId,
    ) -> Result<(), DirectoryError> {
        self.transaction(|_| {
            if !self.db.query("SELECT account_id FROM account_assignment_gates WHERE account_id=?", &[SqlValue::Text(account_id.to_string())])?.is_empty() {
                return Err(DirectoryError::AssignmentBlocked);
            }
            self.db.execute("INSERT INTO directory_hosted_nonterminal_games(game_id,designated_host_id) VALUES(?,?) ON CONFLICT(game_id) DO UPDATE SET designated_host_id=excluded.designated_host_id", &[SqlValue::Text(game_id.to_string()), SqlValue::Text(account_id.to_string())])?;
            Ok(())
        })
    }
    fn receipt(
        &self,
        operation_id: OperationId,
        now: i64,
    ) -> Result<Option<AccountId>, DirectoryError> {
        let rows = self.db.query(
            "SELECT account_id,completed_at FROM directory_removal_receipts WHERE operation_id=?",
            &[SqlValue::Text(operation_id.to_string())],
        )?;
        let account = parse_operation_row(&rows, operation_id, now)?;
        if let Some(row) = rows.first() {
            let [_, SqlValue::Integer(completed)] = row.as_slice() else {
                return Err(DirectoryError::Storage);
            };
            if now - completed >= COMMAND_RECEIPT_RETENTION_MS {
                return Err(DirectoryError::StaleOperation);
            }
        }
        Ok(account)
    }
    fn pending(
        &self,
        operation_id: OperationId,
        now: i64,
    ) -> Result<Option<AccountId>, DirectoryError> {
        let rows = self.db.query(
            "SELECT account_id,created_at FROM directory_removal_pending WHERE operation_id=?",
            &[SqlValue::Text(operation_id.to_string())],
        )?;
        parse_operation_row(&rows, operation_id, now)
    }
    fn prior_rejection(
        &self,
        operation_id: OperationId,
        account_id: AccountId,
        now: i64,
    ) -> Result<(), DirectoryError> {
        let rows = self.db.query(
            "SELECT account_id,completed_at FROM directory_removal_rejections WHERE operation_id=?",
            &[SqlValue::Text(operation_id.to_string())],
        )?;
        let Some(account) = parse_operation_row(&rows, operation_id, now)? else {
            return Ok(());
        };
        if let Some(row) = rows.first() {
            let [_, SqlValue::Integer(completed)] = row.as_slice() else {
                return Err(DirectoryError::Storage);
            };
            if now - completed >= COMMAND_RECEIPT_RETENTION_MS {
                return Err(DirectoryError::StaleOperation);
            }
        }
        if account != account_id {
            return Err(DirectoryError::OperationMismatch);
        }
        Err(DirectoryError::HostedGame)
    }
    fn clear_pending_gate(
        &self,
        operation_id: OperationId,
        account_id: AccountId,
    ) -> Result<(), DirectoryError> {
        self.require_gate(account_id)?;
        self.db.execute(
            "DELETE FROM directory_removal_pending WHERE operation_id=? AND account_id=?",
            &[
                SqlValue::Text(operation_id.to_string()),
                SqlValue::Text(account_id.to_string()),
            ],
        )?;
        self.db.execute(
            "DELETE FROM account_assignment_gates WHERE account_id=?",
            &[SqlValue::Text(account_id.to_string())],
        )?;
        Ok(())
    }
    fn record_release(
        &self,
        operation_id: OperationId,
        account_id: AccountId,
        now: i64,
    ) -> Result<(), DirectoryError> {
        self.db.execute("INSERT INTO directory_removal_receipts(operation_id,account_id,completed_at) VALUES(?,?,?)", &[SqlValue::Text(operation_id.to_string()),SqlValue::Text(account_id.to_string()),SqlValue::Integer(now)])?;
        Ok(())
    }
    fn validate_account_gate(&self, account_id: AccountId, now: i64) -> Result<(), DirectoryError> {
        let gates = self.db.query(
            "SELECT account_id FROM account_assignment_gates WHERE account_id=?",
            &[SqlValue::Text(account_id.to_string())],
        )?;
        let pending = self.db.query(
            "SELECT operation_id,created_at FROM directory_removal_pending WHERE account_id=?",
            &[SqlValue::Text(account_id.to_string())],
        )?;
        if gates.is_empty() && pending.is_empty() {
            return Ok(());
        }
        self.require_gate(account_id)?;
        let [row] = pending.as_slice() else {
            return Err(DirectoryError::Storage);
        };
        let [SqlValue::Text(operation), at] = row.as_slice() else {
            return Err(DirectoryError::Storage);
        };
        let operation = operation
            .parse::<OperationId>()
            .map_err(|_| DirectoryError::Storage)?;
        parse_operation_row(
            &[vec![SqlValue::Text(account_id.to_string()), at.clone()]],
            operation,
            now,
        )?;
        Ok(())
    }
    fn require_gate(&self, account_id: AccountId) -> Result<(), DirectoryError> {
        let gate = self.db.query(
            "SELECT account_id FROM account_assignment_gates WHERE account_id=?",
            &[SqlValue::Text(account_id.to_string())],
        )?;
        if gate != vec![vec![SqlValue::Text(account_id.to_string())]] {
            return Err(DirectoryError::Storage);
        }
        Ok(())
    }
    fn admit_operation(&self, operation_id: OperationId, now: i64) -> Result<(), DirectoryError> {
        let timestamp = operation_timestamp(operation_id)?;
        // UUID time only bounds fresh admission; known durable intents never expire.
        if timestamp <= self.command_floor()? || timestamp > now {
            return Err(DirectoryError::StaleOperation);
        }
        Ok(())
    }
    fn command_floor(&self) -> Result<i64, DirectoryError> {
        let rows = self.db.query(
            "SELECT command_floor_ms FROM directory_metadata WHERE singleton=1",
            &[],
        )?;
        let [row] = rows.as_slice() else {
            return Err(DirectoryError::Storage);
        };
        let [SqlValue::Integer(floor)] = row.as_slice() else {
            return Err(DirectoryError::Storage);
        };
        Ok(*floor)
    }
    fn observe_clock(&self) -> Result<i64, DirectoryError> {
        validate_schema(self.db)?;
        let now = self.runtime.now_ms();
        if !(0..=JS_SAFE_INTEGER_MAX).contains(&now) {
            return Err(DirectoryError::Clock);
        }
        let metadata = self.db.query(
            "SELECT last_observed_ms,command_floor_ms FROM directory_metadata WHERE singleton=1",
            &[],
        )?;
        let [row] = metadata.as_slice() else {
            return Err(DirectoryError::Storage);
        };
        let [SqlValue::Integer(last), SqlValue::Integer(floor)] = row.as_slice() else {
            return Err(DirectoryError::Storage);
        };
        if !(0..=JS_SAFE_INTEGER_MAX).contains(last) || !(0..=*last).contains(floor) {
            return Err(DirectoryError::Storage);
        }
        if now < *last {
            return Err(DirectoryError::Clock);
        }
        self.db.execute("UPDATE directory_metadata SET last_observed_ms=?,command_floor_ms=max(command_floor_ms,?) WHERE singleton=1", &[SqlValue::Integer(now),SqlValue::Integer(now.saturating_sub(COMMAND_RECEIPT_RETENTION_MS).max(0))])?;
        Ok(now)
    }
    fn compact_completions(&self, now: i64) -> Result<(), DirectoryError> {
        self.compact_creations(now)?;
        // Never delete unvalidated tombstones: malformed time could otherwise
        // remove a fresh operation fence and allow its resurrection.
        for (select, delete) in [
            (
                "SELECT operation_id,account_id,completed_at FROM directory_removal_receipts WHERE completed_at<=(SELECT command_floor_ms FROM directory_metadata WHERE singleton=1) LIMIT ?",
                "DELETE FROM directory_removal_receipts WHERE operation_id=?",
            ),
            (
                "SELECT operation_id,account_id,completed_at FROM directory_removal_rejections WHERE completed_at<=(SELECT command_floor_ms FROM directory_metadata WHERE singleton=1) LIMIT ?",
                "DELETE FROM directory_removal_rejections WHERE operation_id=?",
            ),
        ] {
            for row in self
                .db
                .query(select, &[SqlValue::Integer(CLEANUP_BATCH_SIZE)])?
            {
                let (operation, _) = parse_completion_row(&row, now)?;
                self.db
                    .execute(delete, &[SqlValue::Text(operation.to_string())])?;
            }
        }
        Ok(())
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce(i64) -> Result<T, DirectoryError>,
    ) -> Result<T, DirectoryError> {
        self.transaction_before_compaction(|now| {
            self.compact_completions(now)?;
            operation(now)
        })
    }
    fn transaction_before_compaction<T>(
        &self,
        operation: impl FnOnce(i64) -> Result<T, DirectoryError>,
    ) -> Result<T, DirectoryError> {
        let fatal = std::cell::Cell::new(None);
        let result = self
            .db
            .transaction(|| match self.observe_clock().and_then(operation) {
                Err(
                    error @ (DirectoryError::Storage
                    | DirectoryError::Clock
                    | DirectoryError::Entropy
                    | DirectoryError::CodeExhausted),
                ) => {
                    fatal.set(Some(error));
                    Err(StorageError)
                }
                result => Ok(result),
            });
        match result {
            Ok(result) => result,
            Err(_) => Err(fatal.get().unwrap_or(DirectoryError::Storage)),
        }
    }
}
