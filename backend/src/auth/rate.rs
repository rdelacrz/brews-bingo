use super::{
    AuthError, AuthOutcome, AuthService, RequestContext, Runtime,
    records::{integer, optional_integer},
};
use crate::{
    limits::{
        CALLER_IDENTITY_MAX_BYTES, RATE_LIMIT_BLOCK_MS, RATE_LIMIT_MAX_FAILURES,
        RATE_LIMIT_WINDOW_MS,
    },
    security::DIGEST_BYTES,
    storage::{Database, SqlValue},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn rate_keys(
        &self,
        scope: &str,
        subject: &[u8],
        context: &RequestContext,
    ) -> Result<[[u8; DIGEST_BYTES]; 2], AuthError> {
        if context.caller_identity.is_empty()
            || context.caller_identity.len() > CALLER_IDENTITY_MAX_BYTES
        {
            return Err(AuthError::InvalidInput);
        }
        Ok([
            self.subject_key(scope, "caller", context.caller_identity.as_bytes())?,
            self.subject_key(scope, "subject", subject)?,
        ])
    }
    fn subject_key(
        &self,
        scope: &str,
        layer: &str,
        subject: &[u8],
    ) -> Result<[u8; DIGEST_BYTES], AuthError> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.rate_key).map_err(|_| AuthError::Crypto)?;
        mac.update(b"brews-bingo:rate:v1\0");
        for bytes in [scope.as_bytes(), layer.as_bytes(), subject] {
            mac.update(&(bytes.len() as u64).to_be_bytes());
            mac.update(bytes);
        }
        Ok(mac.finalize().into_bytes().into())
    }
    pub(super) fn check_rate(
        &self,
        scope: &str,
        keys: &[[u8; DIGEST_BYTES]; 2],
        now: i64,
    ) -> Result<(), AuthError> {
        for key in keys {
            let rows = self.db.query(
                "SELECT blocked_until FROM rate_limit_buckets WHERE scope=? AND subject_key=?",
                &[SqlValue::Text(scope.into()), SqlValue::Blob(key.to_vec())],
            )?;
            if let Some(row) = rows.first()
                && optional_integer(row, 0)?.is_some_and(|deadline| deadline > now)
            {
                return Err(AuthError::RateLimited);
            }
        }
        Ok(())
    }
    pub(super) fn fail_rate(
        &self,
        scope: &str,
        keys: &[[u8; DIGEST_BYTES]; 2],
        now: i64,
    ) -> Result<(), AuthError> {
        for key in keys {
            let rows=self.db.query("SELECT window_started_at,attempt_count FROM rate_limit_buckets WHERE scope=? AND subject_key=?",&[SqlValue::Text(scope.into()),SqlValue::Blob(key.to_vec())])?;
            let (start, count) = if let Some(row) = rows.first() {
                let start = integer(row, 0)?;
                if now >= start + RATE_LIMIT_WINDOW_MS {
                    (now, 1)
                } else {
                    (start, (integer(row, 1)? + 1).min(RATE_LIMIT_MAX_FAILURES))
                }
            } else {
                (now, 1)
            };
            let block = if count >= RATE_LIMIT_MAX_FAILURES {
                Some(
                    now.checked_add(RATE_LIMIT_BLOCK_MS)
                        .ok_or(AuthError::Crypto)?,
                )
            } else {
                None
            };
            let expires = block
                .unwrap_or(start + RATE_LIMIT_WINDOW_MS)
                .max(start + RATE_LIMIT_WINDOW_MS);
            self.db.execute("INSERT INTO rate_limit_buckets(scope,subject_key,window_started_at,attempt_count,blocked_until,expires_at) VALUES(?,?,?,?,?,?) ON CONFLICT(scope,subject_key) DO UPDATE SET window_started_at=excluded.window_started_at,attempt_count=excluded.attempt_count,blocked_until=excluded.blocked_until,expires_at=excluded.expires_at",&[SqlValue::Text(scope.into()),SqlValue::Blob(key.to_vec()),SqlValue::Integer(start),SqlValue::Integer(count),block.map(SqlValue::Integer).unwrap_or(SqlValue::Null),SqlValue::Integer(expires)])?;
        }
        Ok(())
    }
    pub(super) fn clear_rate(
        &self,
        scope: &str,
        keys: &[[u8; DIGEST_BYTES]; 2],
    ) -> Result<(), AuthError> {
        for key in keys {
            self.db.execute(
                "DELETE FROM rate_limit_buckets WHERE scope=? AND subject_key=?",
                &[SqlValue::Text(scope.into()), SqlValue::Blob(key.to_vec())],
            )?;
        }
        Ok(())
    }
    pub(super) fn rejected_credentials(
        &self,
        keys: &[[u8; DIGEST_BYTES]; 2],
    ) -> Result<AuthOutcome, AuthError> {
        self.transaction(|| {
            let now = self.now()?;
            self.check_rate("account_login", keys, now)?;
            self.fail_rate("account_login", keys, now)?;
            Err(AuthError::InvalidCredentials)
        })
    }
}
