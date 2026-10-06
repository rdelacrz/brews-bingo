//! Fresh Accounts-owned game authority. Allocation-conscious, bounded SQL work.
use super::{
    AuthError, AuthService, Runtime,
    records::{Session, integer, text},
};
use crate::{
    db::{Database, Row, SqlValue},
    limits::{CLEANUP_BATCH_SIZE, JS_SAFE_INTEGER_MAX, SOCKET_CLOSE_INITIAL_DELAY_MS},
};
use brews_domain::{
    accounts::{AccountRole, SessionScope},
    ids::{AccountId, ConnectionId, GameId, OperationId, SessionId},
};

const SOCKET_CLOSE_MAX_DELAY_MS: i64 = 300_000;
const SOCKET_CLOSE_BACKOFF_MAX_SHIFT: u32 = 9;

/// A fresh proof, not a client payload or a reusable positive authorization cache.
/// Client JSON cannot manufacture this authority.
/// ```compile_fail
/// fn client_payload<T: serde::de::DeserializeOwned>() {}
/// client_payload::<brews_backend::auth::GameAccountAuthority>();
/// ```
/// Its trusted-peer constructor is not public.
/// ```compile_fail
/// use brews_backend::auth::GameAccountAuthority;
/// use brews_domain::{accounts::AccountRole, ids::{AccountId, SessionId}};
/// fn forge(account: AccountId, session: SessionId) {
///     let _ = GameAccountAuthority::from_trusted_peer(account, session, AccountRole::Host, 0, 1);
/// }
/// ```
#[derive(Debug)]
pub struct GameAccountAuthority {
    account_id: AccountId,
    session_id: SessionId,
    role: AccountRole,
    credential_epoch: i64,
    expires_at: i64,
}
impl GameAccountAuthority {
    /// Reconstruct only metadata returned by a fresh private Accounts peer check.
    /// Shape validation is not independent account authority.
    pub(crate) fn from_trusted_peer(
        account_id: AccountId,
        session_id: SessionId,
        role: AccountRole,
        credential_epoch: i64,
        expires_at: i64,
    ) -> Result<Self, AuthError> {
        if !(0..=JS_SAFE_INTEGER_MAX).contains(&credential_epoch)
            || !(1..=JS_SAFE_INTEGER_MAX).contains(&expires_at)
        {
            return Err(AuthError::Unauthorized);
        }
        Ok(Self {
            account_id,
            session_id,
            role,
            credential_epoch,
            expires_at,
        })
    }
    pub fn account_id(&self) -> AccountId {
        self.account_id
    }
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
    pub fn role(&self) -> AccountRole {
        self.role
    }
    pub fn credential_epoch(&self) -> i64 {
        self.credential_epoch
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
}

/// Durable per-connection target. Contains metadata only; delivery is not an ACK.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameSocketCloseWork {
    account_id: AccountId,
    session_id: SessionId,
    game_id: GameId,
    connection_id: ConnectionId,
    operation_id: OperationId,
    credential_epoch: i64,
    expires_at: i64,
    created_at: i64,
    next_attempt_at: i64,
    attempt_count: u32,
}
impl GameSocketCloseWork {
    /// Shape-checked private close target, not current authority or a closure ACK.
    /// The receiver must fence the exact target and prove SDK closure/absence.
    ///
    /// ```compile_fail
    /// use brews_backend::auth::GameSocketCloseWork;
    /// use brews_domain::ids::{AccountId, SessionId, GameId, ConnectionId, OperationId};
    /// fn forge(a: AccountId, s: SessionId, g: GameId, c: ConnectionId, o: OperationId) {
    ///     let _ = GameSocketCloseWork::from_trusted_peer(a, s, g, c, o, 0, 1, 0);
    /// }
    /// ```
    #[cfg(any(test, target_arch = "wasm32"))]
    #[allow(
        clippy::too_many_arguments,
        reason = "Exact closed-wire target identity; never public authority."
    )]
    pub(crate) fn from_trusted_peer(
        account_id: AccountId,
        session_id: SessionId,
        game_id: GameId,
        connection_id: ConnectionId,
        operation_id: OperationId,
        credential_epoch: i64,
        expires_at: i64,
        created_at: i64,
    ) -> Result<Self, AuthError> {
        if !(0..=JS_SAFE_INTEGER_MAX).contains(&credential_epoch)
            || !(1..=JS_SAFE_INTEGER_MAX).contains(&expires_at)
            || !(0..=JS_SAFE_INTEGER_MAX).contains(&created_at)
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self {
            account_id,
            session_id,
            game_id,
            connection_id,
            operation_id,
            credential_epoch,
            expires_at,
            created_at,
            next_attempt_at: created_at,
            attempt_count: 0,
        })
    }
    fn same_target(&self, other: &Self) -> bool {
        self.connection_id == other.connection_id
            && self.operation_id == other.operation_id
            && self.account_id == other.account_id
            && self.session_id == other.session_id
            && self.game_id == other.game_id
            && self.credential_epoch == other.credential_epoch
            && self.expires_at == other.expires_at
            && self.created_at == other.created_at
    }
    pub fn account_id(&self) -> AccountId {
        self.account_id
    }
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
    pub fn game_id(&self) -> GameId {
        self.game_id
    }
    pub fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }
    pub fn operation_id(&self) -> OperationId {
        self.operation_id
    }
    pub fn credential_epoch(&self) -> i64 {
        self.credential_epoch
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn next_attempt_at(&self) -> i64 {
        self.next_attempt_at
    }
    pub fn attempt_count(&self) -> u32 {
        self.attempt_count
    }
}

fn subscription_values(
    account_id: AccountId,
    session_id: SessionId,
    game_id: GameId,
    credential_epoch: i64,
    expires_at: i64,
) -> [SqlValue; 5] {
    [
        SqlValue::Text(account_id.to_string()),
        SqlValue::Text(session_id.to_string()),
        SqlValue::Text(game_id.to_string()),
        SqlValue::Integer(credential_epoch),
        SqlValue::Integer(expires_at),
    ]
}

impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    /// Reprove current Normal account authority in the serialized Accounts transaction.
    /// This proves neither designated-host assignment nor a Game-owned view grant.
    pub fn authorize_game_account(
        &self,
        session_token: &str,
    ) -> Result<GameAccountAuthority, AuthError> {
        self.transaction(|| {
            let session = self
                .session(session_token)?
                .ok_or(AuthError::Unauthorized)?;
            self.game_authority(&session)
        })
    }

    /// Register only the exact binding; reconnect never renews credential expiry.
    pub fn register_game_account_connection(
        &self,
        token: &str,
        game_id: GameId,
        connection_id: ConnectionId,
    ) -> Result<GameAccountAuthority, AuthError> {
        self.transaction(|| {
            let session = self.session(token)?.ok_or(AuthError::Unauthorized)?;
            let proof = self.game_authority(&session)?;
            if self.game_socket_close_pending(connection_id)? { return Err(AuthError::Unauthorized); }
            let binding = subscription_values(proof.account_id, proof.session_id, game_id, proof.credential_epoch, proof.expires_at);
            if let Some(row) = self.game_socket_subscription(connection_id)? {
                if row != binding { return Err(AuthError::Conflict); }
            } else {
                self.db.execute(
                    "INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,credential_epoch,expires_at,connection_id) VALUES(?,?,?,?,?,?)",
                    &[
                        SqlValue::Text(proof.account_id.to_string()),
                        SqlValue::Text(proof.session_id.to_string()),
                        SqlValue::Text(game_id.to_string()),
                        SqlValue::Integer(proof.credential_epoch),
                        SqlValue::Integer(proof.expires_at),
                        SqlValue::Text(connection_id.to_string()),
                    ],
                )?;
                if self.game_socket_subscription(connection_id)?.as_deref() != Some(binding.as_slice()) {
                    return Err(AuthError::Storage);
                }
            }
            Ok(proof)
        })
    }

    /// Attachment metadata is only a hint. Reprove this binding before every frame.
    pub fn authorize_game_account_connection(
        &self,
        account_id: AccountId,
        session_id: SessionId,
        credential_epoch: i64,
        game_id: GameId,
        connection_id: ConnectionId,
    ) -> Result<GameAccountAuthority, AuthError> {
        self.transaction(|| {
            let rows = self.db.query(
                "SELECT session_id,account_id,scope,credential_epoch,expires_at,revoked_at FROM account_sessions WHERE session_id=?",
                &[SqlValue::Text(session_id.to_string())],
            )?;
            let session = rows.first().map(Session::parse).transpose()?.ok_or(AuthError::Unauthorized)?;
            let proof = self.game_authority(&session)?;
            if proof.account_id != account_id || proof.credential_epoch != credential_epoch {
                return Err(AuthError::Unauthorized);
            }
            let binding = subscription_values(account_id, session_id, game_id, credential_epoch, proof.expires_at);
            if self.game_socket_subscription(connection_id)?.as_deref() != Some(binding.as_slice())
                || self.game_socket_close_pending(connection_id)?
            {
                return Err(AuthError::Unauthorized);
            }
            Ok(proof)
        })
    }

    /// Trusted Game absence acknowledgement for an exact binding. Pending close work
    /// must use the operation-fenced close acknowledgement instead.
    pub fn unregister_game_account_connection(
        &self,
        account_id: AccountId,
        session_id: SessionId,
        credential_epoch: i64,
        game_id: GameId,
        connection_id: ConnectionId,
    ) -> Result<(), AuthError> {
        self.transaction(|| {
            if self.game_socket_close_pending(connection_id)? { return Err(AuthError::Conflict); }
            let Some(row) = self.game_socket_subscription(connection_id)? else { return Ok(()); };
            let binding = subscription_values(account_id, session_id, game_id, credential_epoch, integer(&row, 4)?);
            if row != binding { return Err(AuthError::Conflict); }
            self.db.execute(
                "DELETE FROM account_socket_subscriptions WHERE connection_id=? AND account_id=? AND session_id=? AND game_id=? AND credential_epoch=?",
                &[
                    SqlValue::Text(connection_id.to_string()),
                    SqlValue::Text(account_id.to_string()),
                    SqlValue::Text(session_id.to_string()),
                    SqlValue::Text(game_id.to_string()),
                    SqlValue::Integer(credential_epoch),
                ],
            )?;
            if self.game_socket_subscription(connection_id)?.is_some() { return Err(AuthError::Storage); }
            Ok(())
        })
    }

    /// Return due targets in stable order, with an explicit 1..=100 page bound.
    pub fn list_due_game_socket_closes(
        &self,
        limit: u32,
    ) -> Result<Vec<GameSocketCloseWork>, AuthError> {
        self.transaction(|| {
            if limit == 0 || i64::from(limit) > CLEANUP_BATCH_SIZE { return Err(AuthError::InvalidInput); }
            let rows = self.db.query(
                "SELECT account_id,session_id,game_id,connection_id,operation_id,credential_epoch,expires_at,created_at,next_attempt_at,attempt_count FROM account_socket_close_work WHERE next_attempt_at<=? ORDER BY next_attempt_at,connection_id LIMIT ?",
                &[SqlValue::Integer(self.now()?), SqlValue::Integer(i64::from(limit))],
            )?;
            rows.iter().map(|row| self.parse_game_socket_close(row)).collect()
        })
    }

    /// Keep unresolved work; delay grows from 1s to 5min and never wraps.
    pub fn mark_game_socket_close_retry(
        &self,
        work: &GameSocketCloseWork,
    ) -> Result<(), AuthError> {
        self.transaction(|| {
            let current = self.game_socket_close_work(work.connection_id)?.ok_or(AuthError::Conflict)?;
            if !current.same_target(work) { return Err(AuthError::Conflict); }
            let delay = SOCKET_CLOSE_INITIAL_DELAY_MS
                .saturating_mul(1i64 << current.attempt_count.min(SOCKET_CLOSE_BACKOFF_MAX_SHIFT))
                .min(SOCKET_CLOSE_MAX_DELAY_MS);
            let deadline = self.now()?.saturating_add(delay).min(JS_SAFE_INTEGER_MAX);
            let attempt_count = current.attempt_count.saturating_add(1);
            self.db.execute(
                "UPDATE account_socket_close_work SET attempt_count=?,next_attempt_at=? WHERE connection_id=? AND operation_id=?",
                &[
                    SqlValue::Integer(i64::from(attempt_count)),
                    SqlValue::Integer(deadline),
                    SqlValue::Text(work.connection_id.to_string()),
                    SqlValue::Text(work.operation_id.to_string()),
                ],
            )?;
            let stored = self.game_socket_close_work(work.connection_id)?.ok_or(AuthError::Storage)?;
            if !stored.same_target(work) || stored.attempt_count != attempt_count || stored.next_attempt_at != deadline {
                return Err(AuthError::Storage);
            }
            Ok(())
        })
    }

    /// Trusted dispatcher only: call AFTER the owning Game confirms this exact
    /// operation/account/session/game/epoch/connection is closed or already absent.
    /// Merely enqueueing or delivering a command is not confirmation.
    pub fn acknowledge_game_socket_closed(
        &self,
        work: &GameSocketCloseWork,
    ) -> Result<(), AuthError> {
        self.transaction(|| {
            let subscription = self.game_socket_subscription(work.connection_id)?;
            let Some(current) = self.game_socket_close_work(work.connection_id)? else {
                return if subscription.is_none() { Ok(()) } else { Err(AuthError::Conflict) };
            };
            if !current.same_target(work) { return Err(AuthError::Conflict); }
            let binding = subscription_values(work.account_id, work.session_id, work.game_id, work.credential_epoch, work.expires_at);
            if subscription.as_deref() != Some(binding.as_slice()) { return Err(AuthError::Storage); }
            self.db.execute(
                "DELETE FROM account_socket_close_work WHERE connection_id=? AND operation_id=?",
                &[SqlValue::Text(work.connection_id.to_string()), SqlValue::Text(work.operation_id.to_string())],
            )?;
            self.db.execute(
                "DELETE FROM account_socket_subscriptions WHERE connection_id=? AND account_id=? AND session_id=? AND game_id=? AND credential_epoch=? AND expires_at=?",
                &[
                    SqlValue::Text(work.connection_id.to_string()),
                    SqlValue::Text(work.account_id.to_string()),
                    SqlValue::Text(work.session_id.to_string()),
                    SqlValue::Text(work.game_id.to_string()),
                    SqlValue::Integer(work.credential_epoch),
                    SqlValue::Integer(work.expires_at),
                ],
            )?;
            if self.game_socket_subscription(work.connection_id)?.is_some()
                || self.game_socket_close_pending(work.connection_id)?
            { return Err(AuthError::Storage); }
            Ok(())
        })
    }

    fn game_authority(&self, session: &Session) -> Result<GameAccountAuthority, AuthError> {
        let account = self
            .account(session.account_id)?
            .ok_or(AuthError::Unauthorized)?;
        if session.scope != SessionScope::Normal || !session.eligible(&account, self.now()?) {
            return Err(AuthError::Unauthorized);
        }
        GameAccountAuthority::from_trusted_peer(
            account.id,
            session.id,
            account.role,
            account.epoch,
            session.expires,
        )
    }
    fn game_socket_subscription(
        &self,
        connection_id: ConnectionId,
    ) -> Result<Option<Row>, AuthError> {
        let rows = self.db.query(
            "SELECT account_id,session_id,game_id,credential_epoch,expires_at FROM account_socket_subscriptions WHERE connection_id=?",
            &[SqlValue::Text(connection_id.to_string())],
        )?;
        if rows.first().is_some_and(|row| row.len() != 5) {
            return Err(AuthError::Storage);
        }
        Ok(rows.into_iter().next())
    }
    fn game_socket_close_pending(&self, connection_id: ConnectionId) -> Result<bool, AuthError> {
        Ok(!self
            .db
            .query(
                "SELECT connection_id FROM account_socket_close_work WHERE connection_id=?",
                &[SqlValue::Text(connection_id.to_string())],
            )?
            .is_empty())
    }
    fn game_socket_close_work(
        &self,
        connection_id: ConnectionId,
    ) -> Result<Option<GameSocketCloseWork>, AuthError> {
        let rows = self.db.query(
            "SELECT account_id,session_id,game_id,connection_id,operation_id,credential_epoch,expires_at,created_at,next_attempt_at,attempt_count FROM account_socket_close_work WHERE connection_id=?",
            &[SqlValue::Text(connection_id.to_string())],
        )?;
        rows.first()
            .map(|row| self.parse_game_socket_close(row))
            .transpose()
    }
    fn parse_game_socket_close(&self, row: &Row) -> Result<GameSocketCloseWork, AuthError> {
        if row.len() != 10 {
            return Err(AuthError::Storage);
        }
        let work = GameSocketCloseWork {
            account_id: text(row, 0)?.parse().map_err(|_| AuthError::Storage)?,
            session_id: text(row, 1)?.parse().map_err(|_| AuthError::Storage)?,
            game_id: text(row, 2)?.parse().map_err(|_| AuthError::Storage)?,
            connection_id: text(row, 3)?.parse().map_err(|_| AuthError::Storage)?,
            operation_id: text(row, 4)?.parse().map_err(|_| AuthError::Storage)?,
            credential_epoch: integer(row, 5)?,
            expires_at: integer(row, 6)?,
            created_at: integer(row, 7)?,
            next_attempt_at: integer(row, 8)?,
            attempt_count: u32::try_from(integer(row, 9)?).map_err(|_| AuthError::Storage)?,
        };
        if [
            work.credential_epoch,
            work.expires_at,
            work.created_at,
            work.next_attempt_at,
        ]
        .iter()
        .any(|n| !(0..=JS_SAFE_INTEGER_MAX).contains(n))
            || work.created_at > self.now()?
            || work.next_attempt_at < work.created_at
        {
            return Err(AuthError::Storage);
        }
        Ok(work)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Real SQLite test fixtures fail fast.")]
mod test_support {
    use crate::{
        auth::{AuthError, Runtime},
        db::{Database, Row, SqlValue, StorageError},
    };
    use rusqlite::{
        Connection,
        types::{Value, ValueRef},
    };
    use std::cell::Cell;
    pub struct Sqlite(Connection);
    impl Sqlite {
        pub fn new() -> Self {
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
            Self(db)
        }
    }
    fn value(v: &SqlValue) -> Value {
        match v {
            SqlValue::Null => Value::Null,
            SqlValue::Integer(v) => Value::Integer(*v),
            SqlValue::Text(v) => Value::Text(v.clone()),
            SqlValue::Blob(v) => Value::Blob(v.clone()),
        }
    }
    impl Database for Sqlite {
        fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
            let mut stmt = self.0.prepare(sql).map_err(|_| StorageError)?;
            let columns = stmt.column_count();
            let mut cursor = stmt
                .query(rusqlite::params_from_iter(params.iter().map(value)))
                .map_err(|_| StorageError)?;
            let mut out = Vec::new();
            while let Some(row) = cursor.next().map_err(|_| StorageError)? {
                let mut values = Vec::new();
                for i in 0..columns {
                    values.push(match row.get_ref(i).map_err(|_| StorageError)? {
                        ValueRef::Null => SqlValue::Null,
                        ValueRef::Integer(v) => SqlValue::Integer(v),
                        ValueRef::Text(v) => SqlValue::Text(
                            std::str::from_utf8(v).map_err(|_| StorageError)?.to_owned(),
                        ),
                        ValueRef::Blob(v) => SqlValue::Blob(v.to_vec()),
                        ValueRef::Real(_) => return Err(StorageError),
                    });
                }
                out.push(values);
            }
            Ok(out)
        }
        fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
            self.0
                .execute(sql, rusqlite::params_from_iter(params.iter().map(value)))
                .map(|_| ())
                .map_err(|_| StorageError)
        }
        fn transaction<T>(
            &self,
            op: impl FnOnce() -> Result<T, StorageError>,
        ) -> Result<T, StorageError> {
            self.0
                .execute_batch("BEGIN IMMEDIATE")
                .map_err(|_| StorageError)?;
            match op() {
                Ok(out) => {
                    self.0.execute_batch("COMMIT").map_err(|_| StorageError)?;
                    Ok(out)
                }
                Err(e) => {
                    self.0.execute_batch("ROLLBACK").map_err(|_| StorageError)?;
                    Err(e)
                }
            }
        }
    }
    pub struct TestRuntime {
        pub now: Cell<i64>,
        random: Cell<u64>,
    }
    impl TestRuntime {
        pub fn new() -> Self {
            Self {
                now: Cell::new(1_800_000_000_000),
                random: Cell::new(1),
            }
        }
    }
    impl Runtime for TestRuntime {
        fn now_ms(&self) -> i64 {
            self.now.get()
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
            // Deterministic fixture entropy, never production randomness.
            for byte in bytes {
                let n = self
                    .random
                    .get()
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1);
                self.random.set(n);
                *byte = (n >> 32) as u8;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod tests {
    use super::*;
    fn command(rt: &test_support::TestRuntime, n: u8) -> brews_domain::ids::CommandId {
        let mut bytes = [0; 16];
        bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        bytes[15] = n;
        uuid::Uuid::from_bytes(bytes).try_into().unwrap()
    }
    #[test]
    fn trusted_close_metadata_preserves_each_identity_without_granting_account_authority() {
        let text = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        let account: AccountId = text.parse().unwrap();
        let session: SessionId = text.parse().unwrap();
        let game: GameId = text.parse().unwrap();
        let connection: ConnectionId = text.parse().unwrap();
        let operation: OperationId = text.parse().unwrap();
        let work = GameSocketCloseWork::from_trusted_peer(
            account,
            session,
            game,
            connection,
            operation,
            2,
            1_700_000_000_000,
            1_800_000_000_000,
        )
        .unwrap();
        assert_eq!(work.account_id(), account);
        assert_eq!(work.session_id(), session);
        assert_eq!(work.game_id(), game);
        assert_eq!(work.connection_id(), connection);
        assert_eq!(work.operation_id(), operation);
        assert_eq!(work.credential_epoch(), 2);
        assert_eq!(work.expires_at(), 1_700_000_000_000);
        assert_eq!(work.created_at(), 1_800_000_000_000);
        for (epoch, expiry, created) in [
            (-1, 1, 1),
            (JS_SAFE_INTEGER_MAX + 1, 1, 1),
            (0, 0, 1),
            (0, JS_SAFE_INTEGER_MAX + 1, 1),
            (0, 1, -1),
            (0, 1, JS_SAFE_INTEGER_MAX + 1),
        ] {
            assert!(
                GameSocketCloseWork::from_trusted_peer(
                    account, session, game, connection, operation, epoch, expiry, created
                )
                .is_err()
            );
        }
    }
    #[test]
    fn removal_finishes_only_after_all_exact_socket_close_acknowledgements() {
        use crate::{
            auth::{
                AuthCommand, AuthPolicy, CookieEffect, ManagementPrincipal, RemovalGateGrant,
                RemovalReleaseAck, RequestContext,
            },
            db::{
                SqlValue,
                directory::{DirectoryService, migrate_directory},
                migrate,
            },
        };
        use brews_contracts::management::{ManagementCommand, ManagementResponse};
        use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
        for delete in [false, true] {
            let db = test_support::Sqlite::new();
            let dir_db = test_support::Sqlite::new();
            let rt = test_support::TestRuntime::new();
            migrate(&db).unwrap();
            migrate_directory(&dir_db).unwrap();
            let s = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
            let out = s
                .execute_management(
                    ManagementCommand::CreateAccount {
                        username: "RemovalGameHost".into(),
                        role: AccountRole::Host,
                    },
                    ManagementPrincipal::DeveloperCli,
                    Some(command(&rt, 1)),
                    "https://app.example.test",
                )
                .unwrap();
            let (account, link) = match out {
                ManagementResponse::Issued { receipt, url } => (
                    receipt.account_id,
                    url.rsplit('#').next().unwrap().to_owned(),
                ),
                _ => panic!("Expected enrollment."),
            };
            let context = |n| RequestContext {
                command_id: Some(command(&rt, n).to_string()),
                caller_identity: "trusted-test-caller".into(),
            };
            let out = s
                .execute(
                    AuthCommand::Redeem {
                        purpose: AccessLinkPurpose::Enrollment,
                        token: link,
                    },
                    context(2),
                )
                .unwrap();
            let restricted = match out.cookie {
                CookieEffect::Set { token, .. } => token,
                _ => panic!("Expected cookie."),
            };
            let password = crate::security::new_token(&rt).unwrap();
            let out = s
                .execute(
                    AuthCommand::Complete {
                        scope: SessionScope::EnrollmentOnly,
                        token: restricted,
                        new_password: password,
                    },
                    context(3),
                )
                .unwrap();
            let token = match out.cookie {
                CookieEffect::Set { token, .. } => token,
                _ => panic!("Expected cookie."),
            };
            let game = command(&rt, 240).to_string().parse().unwrap();
            let connection = command(&rt, 241).to_string().parse().unwrap();
            let p = s
                .register_game_account_connection(&token, game, connection)
                .unwrap();
            s.register_game_account_connection(
                &token,
                game,
                command(&rt, 242).to_string().parse().unwrap(),
            )
            .unwrap();
            let operation = if delete {
                ManagementCommand::DeleteAccount {
                    account_id: account,
                }
            } else {
                ManagementCommand::DisableAccount {
                    account_id: account,
                }
            };
            let op = match s
                .prepare_removal(
                    operation,
                    ManagementPrincipal::DeveloperCli,
                    command(&rt, 4),
                )
                .unwrap()
            {
                ManagementResponse::Pending { operation_id } => operation_id,
                _ => panic!("Expected pending."),
            };
            let dir = DirectoryService::new(&dir_db, &rt).unwrap();
            let grant = dir.acquire_removal(op, account).unwrap();
            let grant = RemovalGateGrant::verified(grant.operation_id, grant.account_id);
            s.commit_removal(op, ManagementPrincipal::DeveloperCli, &grant)
                .unwrap();
            let released = dir.release_removal(op, account).unwrap();
            let released = RemovalReleaseAck::verified(released.operation_id, released.account_id);
            assert!(matches!(
                s.finish_removal(op, &released).unwrap(),
                ManagementResponse::Pending { .. }
            ));
            assert_eq!(
                s.authorize_game_account_connection(
                    account,
                    p.session_id(),
                    p.credential_epoch(),
                    game,
                    connection
                )
                .unwrap_err(),
                AuthError::Unauthorized
            );
            rt.now.set(rt.now.get() + 1000);
            let work = s.list_due_game_socket_closes(100).unwrap();
            assert_eq!(work.len(), 2);
            s.acknowledge_game_socket_closed(&work[0]).unwrap();
            assert!(matches!(
                s.finish_removal(op, &released).unwrap(),
                ManagementResponse::Pending { .. }
            ));
            s.acknowledge_game_socket_closed(&work[1]).unwrap();
            assert!(matches!(
                s.finish_removal(op, &released).unwrap(),
                ManagementResponse::Committed { .. }
            ));
            assert_eq!(
                db.query("SELECT count(*) FROM pending_account_removals", &[])
                    .unwrap(),
                vec![vec![SqlValue::Integer(0)]]
            );
        }
    }
    #[test]
    fn trusted_peer_metadata_reconstruction_is_range_checked() {
        let a = "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap();
        let s = "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap();
        let p = GameAccountAuthority::from_trusted_peer(a, s, AccountRole::Host, 0, 1).unwrap();
        assert_eq!(p.account_id(), a);
        assert_eq!(p.session_id(), s);
        for (epoch, expiry) in [
            (-1, 1),
            (crate::limits::JS_SAFE_INTEGER_MAX + 1, 1),
            (0, 0),
            (0, crate::limits::JS_SAFE_INTEGER_MAX + 1),
        ] {
            assert_eq!(
                GameAccountAuthority::from_trusted_peer(a, s, AccountRole::Host, epoch, expiry)
                    .unwrap_err(),
                AuthError::Unauthorized
            );
        }
    }
}
