//! Allocation-conscious owner-local transport credit; never socket or authority state.
//! The derived unacknowledged budget is conservative, not observable network buffering.
//! The parent owns authority proof, durable sync, sockets, supersession and resynchronization.
use crate::{
    auth::Runtime,
    db::{Database, StorageError},
};
#[path = "schema/game_delivery_schema.rs"]
mod schema;
pub const DELIVERY_CLEANUP_BATCH_SIZE: usize = 100;
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DeliveryError {
    #[error("Delivery storage unavailable.")]
    Storage,
    #[error("Delivery authority expired.")]
    Expired,
    #[error("Delivery binding mismatch.")]
    BindingMismatch,
    #[error("Invalid delivery metadata.")]
    InvalidInput,
    #[error("Delivery clock unavailable.")]
    Clock,
    #[error("Delivery randomness unavailable.")]
    Random,
    #[error("Delivery capacity exhausted; close and resynchronize.")]
    Capacity,
    #[error("Unknown outstanding delivery.")]
    UnknownDelivery,
    #[error("Delivery revision mismatch.")]
    RevisionMismatch,
}
impl From<StorageError> for DeliveryError {
    fn from(_: StorageError) -> Self {
        Self::Storage
    }
}
/// Initialize this three-table group inside the Game owner's database.
/// Call separately after Game migration (not inside a nested transaction). Existing
/// pending deliveries are never reset; unknown, partial or drifted groups fail closed.
pub fn migrate_game_delivery<D: Database>(db: &D) -> Result<(), DeliveryError> {
    db.transaction(|| {
        if db
            .query(
                "SELECT name FROM sqlite_master WHERE name='game_delivery_metadata'",
                &[],
            )?
            .is_empty()
        {
            for sql in schema::statements() {
                db.execute(&sql, &[])?;
            }
            db.execute(
                "INSERT INTO game_delivery_metadata VALUES(1,?,0)",
                &[crate::db::SqlValue::Integer(
                    schema::DELIVERY_SCHEMA_VERSION,
                )],
            )?;
        }
        let rows = db.query(
            "SELECT schema_version FROM game_delivery_metadata WHERE singleton=1",
            &[],
        )?;
        if rows
            != vec![vec![crate::db::SqlValue::Integer(
                schema::DELIVERY_SCHEMA_VERSION,
            )]]
        {
            return Err(StorageError);
        }
        validate_schema(db)?;
        Ok(())
    })
    .map_err(DeliveryError::from)
}
fn validate_schema<D: Database>(db: &D) -> Result<(), StorageError> {
    let tables=db.query("SELECT name FROM sqlite_master WHERE type='table' AND name GLOB 'game_delivery_*' ORDER BY name LIMIT 4",&[])?;
    if tables != schema::DELIVERY_TABLES.map(|s| vec![crate::db::SqlValue::Text(s.into())]) {
        return Err(StorageError);
    }
    for sql in schema::statements() {
        let name = sql
            .split_whitespace()
            .nth(2)
            .and_then(|n| n.split('(').next())
            .ok_or(StorageError)?;
        if db.query(
            "SELECT sql FROM sqlite_master WHERE name=?",
            &[crate::db::SqlValue::Text(name.into())],
        )? != vec![vec![crate::db::SqlValue::Text(sql)]]
        {
            return Err(StorageError);
        }
    }
    if db.query(
        "SELECT schema_version FROM game_delivery_metadata WHERE singleton=1",
        &[],
    )? != vec![vec![crate::db::SqlValue::Integer(
        schema::DELIVERY_SCHEMA_VERSION,
    )]] {
        return Err(StorageError);
    }
    Ok(())
}
use crate::db::SqlValue;
use brews_contracts::games::{
    DELIVERY_ID_BYTES, DeliveryId, GAME_FRAME_MAX_BYTES, MAX_SAFE_REVISION, SnapshotAck,
};
use brews_domain::ids::{ConnectionId, SessionId};
pub use schema::{DELIVERY_PENDING_MAX, DELIVERY_QUEUE_MAX_BYTES};
pub const DELIVERY_ID_GENERATION_MAX_ATTEMPTS: usize = 3;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Non-bearer metadata copied only from current owner-validated authority.
/// This value is not a reusable authorization grant or a client request DTO.
pub struct DeliveryBinding {
    pub connection_id: ConnectionId,
    pub session_id: SessionId,
    pub auth_epoch: u64,
    pub expires_at: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryBudget {
    pub pending_count: usize,
    pub outstanding_bytes: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReservedDelivery {
    pub delivery_id: DeliveryId,
    pub budget: DeliveryBudget,
}
/// Serialized SQL service; borrowed Database/Runtime are private and owner-selected.
pub struct DeliveryService<'a, D: Database, R: Runtime> {
    db: &'a D,
    runtime: &'a R,
}
impl<'a, D: Database, R: Runtime> DeliveryService<'a, D, R> {
    pub fn new(db: &'a D, runtime: &'a R) -> Result<Self, DeliveryError> {
        validate_schema(db)?;
        Ok(Self { db, runtime })
    }
    /// Caller must first reconstruct current Game/Accounts authority. No bearer is accepted or stored.
    pub fn register(&self, binding: DeliveryBinding) -> Result<DeliveryBudget, DeliveryError> {
        self.transaction(|now| self.register_at(binding, now))
    }
    fn register_at(
        &self,
        binding: DeliveryBinding,
        now: i64,
    ) -> Result<DeliveryBudget, DeliveryError> {
        validate_binding(binding, now)?;
        if self.binding_exists(binding)? {
            return self.read_budget(binding.connection_id, now);
        }
        self.db.execute("INSERT INTO game_delivery_connections(connection_id,session_id,auth_epoch,expires_at) VALUES(?,?,?,?)",&binding_params(binding))?;
        if !self
            .binding_exists(binding)
            .map_err(|_| DeliveryError::Storage)?
        {
            return Err(DeliveryError::Storage);
        }
        Ok(DeliveryBudget {
            pending_count: 0,
            outstanding_bytes: 0,
        })
    }
}
fn binding_params(b: DeliveryBinding) -> [SqlValue; 4] {
    [
        SqlValue::Text(b.connection_id.to_string()),
        SqlValue::Text(b.session_id.to_string()),
        SqlValue::Integer(b.auth_epoch as i64),
        SqlValue::Integer(b.expires_at),
    ]
}
fn validate_binding(b: DeliveryBinding, now: i64) -> Result<(), DeliveryError> {
    if b.auth_epoch > MAX_SAFE_REVISION
        || b.expires_at < 1
        || b.expires_at > MAX_SAFE_REVISION as i64
    {
        return Err(DeliveryError::InvalidInput);
    }
    if b.expires_at <= now {
        return Err(DeliveryError::Expired);
    }
    Ok(())
}
impl<D: Database, R: Runtime> DeliveryService<'_, D, R> {
    fn transaction<T>(
        &self,
        operation: impl FnOnce(i64) -> Result<T, DeliveryError>,
    ) -> Result<T, DeliveryError> {
        self.db
            .transaction(|| {
                validate_schema(self.db)?;
                let now = match self.observe_time() {
                    Ok(now) => now,
                    Err(DeliveryError::Storage) => return Err(StorageError),
                    Err(error) => return Ok(Err(error)),
                };
                match operation(now) {
                    Err(DeliveryError::Storage) => Err(StorageError),
                    result => Ok(result),
                }
            })
            .map_err(DeliveryError::from)?
    }
    fn observe_time(&self) -> Result<i64, DeliveryError> {
        let now = self.runtime.now_ms();
        let rows = self.db.query(
            "SELECT last_observed_ms FROM game_delivery_metadata WHERE singleton=1",
            &[],
        )?;
        let floor = match rows.as_slice() {
            [row] => match row.as_slice() {
                [SqlValue::Integer(n)] => *n,
                _ => return Err(DeliveryError::Storage),
            },
            _ => return Err(DeliveryError::Storage),
        };
        if !(0..=MAX_SAFE_REVISION as i64).contains(&now)
            || !(0..=MAX_SAFE_REVISION as i64).contains(&floor)
            || now < floor
        {
            return Err(DeliveryError::Clock);
        }
        self.db.execute(
            "UPDATE game_delivery_metadata SET last_observed_ms=? WHERE singleton=1",
            &[SqlValue::Integer(now)],
        )?;
        if self.db.query(
            "SELECT last_observed_ms FROM game_delivery_metadata WHERE singleton=1",
            &[],
        )? != vec![vec![SqlValue::Integer(now)]]
        {
            return Err(DeliveryError::Storage);
        }
        Ok(now)
    }
}
impl<D: Database, R: Runtime> DeliveryService<'_, D, R> {
    /// Reserve exact serialized frame bytes (including delivery_id) before storage sync/send.
    /// Fixed-length IDs permit sizing with a placeholder; replace it before emission.
    pub fn reserve_delivery(
        &self,
        binding: DeliveryBinding,
        view_revision: u64,
        frame_bytes: usize,
    ) -> Result<ReservedDelivery, DeliveryError> {
        self.transaction(|now| self.reserve_at(binding, view_revision, frame_bytes, now))
    }
    fn reserve_at(
        &self,
        binding: DeliveryBinding,
        view_revision: u64,
        frame_bytes: usize,
        now: i64,
    ) -> Result<ReservedDelivery, DeliveryError> {
        use base64::Engine as _;
        self.require_binding(binding, now)?;
        if frame_bytes == 0
            || frame_bytes > GAME_FRAME_MAX_BYTES
            || view_revision > MAX_SAFE_REVISION
        {
            return Err(DeliveryError::InvalidInput);
        }
        let before = self.read_budget(binding.connection_id, now)?;
        if before.pending_count >= DELIVERY_PENDING_MAX
            || before
                .outstanding_bytes
                .checked_add(frame_bytes)
                .is_none_or(|n| n > DELIVERY_QUEUE_MAX_BYTES)
        {
            return Err(DeliveryError::Capacity);
        }
        let mut delivery_id = None;
        for _ in 0..DELIVERY_ID_GENERATION_MAX_ATTEMPTS {
            let mut random = [0u8; DELIVERY_ID_BYTES];
            self.runtime
                .fill_random(&mut random)
                .map_err(|_| DeliveryError::Random)?;
            let candidate: DeliveryId = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(random)
                .parse()
                .map_err(|_| DeliveryError::Storage)?;
            if self
                .db
                .query(
                    "SELECT delivery_id FROM game_delivery_pending WHERE delivery_id=?",
                    &[SqlValue::Text(candidate.as_str().into())],
                )?
                .is_empty()
            {
                delivery_id = Some(candidate);
                break;
            }
        }
        let delivery_id = delivery_id.ok_or(DeliveryError::Random)?;
        let now = self.observe_time()?;
        self.require_binding(binding, now)?;
        self.db.execute("INSERT INTO game_delivery_pending(delivery_id,connection_id,frame_bytes,view_revision,issued_at) VALUES(?,?,?,?,?)",&[
            SqlValue::Text(delivery_id.as_str().into()),SqlValue::Text(binding.connection_id.to_string()),SqlValue::Integer(frame_bytes as i64),SqlValue::Integer(view_revision as i64),SqlValue::Integer(now)
        ])?;
        if self.db.query("SELECT delivery_id,connection_id,frame_bytes,view_revision,issued_at FROM game_delivery_pending WHERE delivery_id=?",&[SqlValue::Text(delivery_id.as_str().into())])? != vec![vec![SqlValue::Text(delivery_id.as_str().into()),SqlValue::Text(binding.connection_id.to_string()),SqlValue::Integer(frame_bytes as i64),SqlValue::Integer(view_revision as i64),SqlValue::Integer(now)]] {return Err(DeliveryError::Storage)}
        Ok(ReservedDelivery {
            delivery_id,
            budget: self.read_budget(binding.connection_id, now)?,
        })
    }
    pub fn budget(&self, binding: DeliveryBinding) -> Result<DeliveryBudget, DeliveryError> {
        self.transaction(|now| {
            self.require_binding(binding, now)?;
            self.read_budget(binding.connection_id, now)
        })
    }
    fn binding_exists(&self, b: DeliveryBinding) -> Result<bool, DeliveryError> {
        let rows=self.db.query("SELECT connection_id,session_id,auth_epoch,expires_at FROM game_delivery_connections WHERE connection_id=?",&[SqlValue::Text(b.connection_id.to_string())])?;
        if rows.is_empty() {
            return Ok(false);
        }
        if rows != vec![binding_params(b).to_vec()] {
            return Err(DeliveryError::BindingMismatch);
        }
        Ok(true)
    }
    fn require_binding(&self, b: DeliveryBinding, now: i64) -> Result<(), DeliveryError> {
        validate_binding(b, now)?;
        if !self.binding_exists(b)? {
            return Err(DeliveryError::BindingMismatch);
        }
        Ok(())
    }
    fn read_budget(
        &self,
        connection_id: ConnectionId,
        now: i64,
    ) -> Result<DeliveryBudget, DeliveryError> {
        let rows=self.db.query("SELECT delivery_id,frame_bytes,view_revision,issued_at FROM game_delivery_pending WHERE connection_id=? ORDER BY delivery_id LIMIT ?",&[SqlValue::Text(connection_id.to_string()),SqlValue::Integer((DELIVERY_PENDING_MAX+1) as i64)])?;
        if rows.len() > DELIVERY_PENDING_MAX {
            return Err(DeliveryError::Storage);
        }
        let mut bytes = 0usize;
        for row in &rows {
            let [
                SqlValue::Text(id),
                SqlValue::Integer(size),
                SqlValue::Integer(revision),
                SqlValue::Integer(issued),
            ] = row.as_slice()
            else {
                return Err(DeliveryError::Storage);
            };
            id.parse::<DeliveryId>()
                .map_err(|_| DeliveryError::Storage)?;
            if !(1..=GAME_FRAME_MAX_BYTES as i64).contains(size)
                || !(0..=MAX_SAFE_REVISION as i64).contains(revision)
                || !(0..=now).contains(issued)
            {
                return Err(DeliveryError::Storage);
            }
            bytes = bytes
                .checked_add(*size as usize)
                .filter(|n| *n <= DELIVERY_QUEUE_MAX_BYTES)
                .ok_or(DeliveryError::Storage)?;
        }
        Ok(DeliveryBudget {
            pending_count: rows.len(),
            outstanding_bytes: bytes,
        })
    }
}
impl<D: Database, R: Runtime> DeliveryService<'_, D, R> {
    /// Caller must revalidate live session, current connection and selected Game authority
    /// immediately before this serialized transaction. An ACK never supplies authority.
    /// Unknown/duplicate deliveries fail closed; no unbounded completion receipts are kept.
    pub fn acknowledge(
        &self,
        binding: DeliveryBinding,
        ack: &SnapshotAck,
    ) -> Result<DeliveryBudget, DeliveryError> {
        self.transaction(|now| self.acknowledge_at(binding, ack, now))
    }
    fn acknowledge_at(
        &self,
        binding: DeliveryBinding,
        ack: &SnapshotAck,
        now: i64,
    ) -> Result<DeliveryBudget, DeliveryError> {
        self.require_binding(binding, now)?;
        self.read_budget(binding.connection_id, now)?;
        if ack.version != 1 || ack.view_revision > MAX_SAFE_REVISION {
            return Err(DeliveryError::InvalidInput);
        }
        if ack.connection_id != binding.connection_id {
            return Err(DeliveryError::BindingMismatch);
        }
        let rows = self.db.query(
            "SELECT connection_id,view_revision FROM game_delivery_pending WHERE delivery_id=?",
            &[SqlValue::Text(ack.delivery_id.as_str().into())],
        )?;
        if rows.is_empty() {
            return Err(DeliveryError::UnknownDelivery);
        }
        let [row] = rows.as_slice() else {
            return Err(DeliveryError::Storage);
        };
        let [SqlValue::Text(connection), SqlValue::Integer(revision)] = row.as_slice() else {
            return Err(DeliveryError::Storage);
        };
        if connection != &binding.connection_id.to_string() {
            return Err(DeliveryError::BindingMismatch);
        }
        if *revision != ack.view_revision as i64 {
            return Err(DeliveryError::RevisionMismatch);
        }
        self.db.execute(
            "DELETE FROM game_delivery_pending WHERE delivery_id=?",
            &[SqlValue::Text(ack.delivery_id.as_str().into())],
        )?;
        if !self
            .db
            .query(
                "SELECT delivery_id FROM game_delivery_pending WHERE delivery_id=?",
                &[SqlValue::Text(ack.delivery_id.as_str().into())],
            )?
            .is_empty()
        {
            return Err(DeliveryError::Storage);
        }
        self.read_budget(binding.connection_id, now)
    }
}
impl<D: Database, R: Runtime> DeliveryService<'_, D, R> {
    /// Invoke only after the parent proves this exact socket closed/absent, or after
    /// authoritative supersession. No caller-supplied "closed" boolean is accepted.
    pub fn retire_connection(&self, binding: DeliveryBinding) -> Result<(), DeliveryError> {
        self.transaction(|_| {
            if self.binding_exists(binding)? {
                self.purge_connection(binding.connection_id)?;
            }
            Ok(())
        })
    }
    fn purge_connection(&self, connection_id: ConnectionId) -> Result<(), DeliveryError> {
        let params = [SqlValue::Text(connection_id.to_string())];
        self.db.execute(
            "DELETE FROM game_delivery_pending WHERE connection_id=?",
            &params,
        )?;
        self.db.execute(
            "DELETE FROM game_delivery_connections WHERE connection_id=?",
            &params,
        )?;
        for query in [
            "SELECT connection_id FROM game_delivery_pending WHERE connection_id=? LIMIT 1",
            "SELECT connection_id FROM game_delivery_connections WHERE connection_id=?",
        ] {
            if !self.db.query(query, &params)?.is_empty() {
                return Err(DeliveryError::Storage);
            }
        }
        Ok(())
    }
    /// Earliest ledger expiry, clamped to now for a due bounded cleanup backlog.
    /// The parent also schedules actual session/socket deadlines independently.
    pub fn next_deadline(&self) -> Result<Option<i64>, DeliveryError> {
        self.transaction(|now| {
            let rows=self.db.query("SELECT connection_id,session_id,auth_epoch,expires_at FROM game_delivery_connections ORDER BY expires_at,connection_id LIMIT 1",&[])?;
            rows.first().map(|row|parse_binding_row(row).map(|b|b.expires_at.max(now))).transpose()
        })
    }
    /// Remove at most DELIVERY_CLEANUP_BATCH_SIZE expired ledger bindings only.
    /// Does not close sockets or mutate parent Game sessions/presence.
    pub fn cleanup_expired(&self) -> Result<usize, DeliveryError> {
        self.transaction(|now| {
            let rows=self.db.query("SELECT connection_id,session_id,auth_epoch,expires_at FROM game_delivery_connections WHERE expires_at<=? ORDER BY expires_at,connection_id LIMIT ?",&[SqlValue::Integer(now),SqlValue::Integer(DELIVERY_CLEANUP_BATCH_SIZE as i64)])?;
            if rows.len()>DELIVERY_CLEANUP_BATCH_SIZE {return Err(DeliveryError::Storage)}
            for row in &rows {
                let b=parse_binding_row(row)?;
                if b.expires_at>now {return Err(DeliveryError::Storage)}
                self.purge_connection(b.connection_id)?;
            } Ok(rows.len())
        })
    }
}
fn parse_binding_row(row: &crate::db::Row) -> Result<DeliveryBinding, DeliveryError> {
    let [
        SqlValue::Text(connection),
        SqlValue::Text(session),
        SqlValue::Integer(epoch),
        SqlValue::Integer(expires),
    ] = row.as_slice()
    else {
        return Err(DeliveryError::Storage);
    };
    if !(0..=MAX_SAFE_REVISION as i64).contains(epoch)
        || !(1..=MAX_SAFE_REVISION as i64).contains(expires)
    {
        return Err(DeliveryError::Storage);
    }
    Ok(DeliveryBinding {
        connection_id: connection.parse().map_err(|_| DeliveryError::Storage)?,
        session_id: session.parse().map_err(|_| DeliveryError::Storage)?,
        auth_epoch: *epoch as u64,
        expires_at: *expires,
    })
}
