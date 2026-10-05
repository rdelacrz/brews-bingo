//! Secret-free owner/account-scoped receipts. Credentials never enter fingerprints.
use super::{
    AuthError, AuthOutcome, AuthService, CookieEffect, RequestContext, Runtime,
    records::{integer, text},
};
use crate::{
    db::{Database, SqlValue},
    limits::{COMMAND_RECEIPT_MAX_BYTES, COMMAND_RECEIPT_RETENTION_MS, JS_SAFE_INTEGER_MAX},
    security::DIGEST_BYTES,
};
use brews_domain::ids::{AccountId, CommandId, LinkId, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum CommandOutcome {
    Login {
        account_id: AccountId,
        session_id: SessionId,
        session_expires_at: i64,
    },
    Redeem {
        account_id: AccountId,
        link_id: LinkId,
        session_id: SessionId,
        session_expires_at: i64,
    },
    CompleteEnrollment {
        account_id: AccountId,
    },
    CompletePasswordReset {
        account_id: AccountId,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOutcome {
    version: u8,
    outcome: CommandOutcome,
}

pub(super) fn command_id(context: &RequestContext) -> Result<CommandId, AuthError> {
    context
        .command_id
        .as_deref()
        .ok_or(AuthError::InvalidInput)?
        .parse()
        .map_err(|_| AuthError::InvalidInput)
}
pub(super) fn fingerprint(
    operation: &str,
    semantic: &[&str],
) -> Result<[u8; DIGEST_BYTES], AuthError> {
    let canonical = serde_json::to_vec(&("brews-auth-v1", operation, semantic))
        .map_err(|_| AuthError::Storage)?;
    Ok(Sha256::digest(canonical).into())
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn admit_command(&self, command: CommandId, now: i64) -> Result<(), AuthError> {
        let id =
            uuid::Uuid::parse_str(&command.to_string()).map_err(|_| AuthError::InvalidInput)?;
        let b = id.as_bytes();
        let timestamp = i64::from_be_bytes([0, 0, b[0], b[1], b[2], b[3], b[4], b[5]]);
        let rows = self.db.query(
            "SELECT command_floor_ms FROM storage_metadata WHERE singleton=1",
            &[],
        )?;
        let floor = integer(rows.first().ok_or(AuthError::Storage)?, 0)?;
        // Untrusted UUID time gates new-command admission, never authority or expiry.
        // The stale cutoff includes equality and the initial zero floor.
        if timestamp <= floor || timestamp > now {
            return Err(AuthError::StaleCommand);
        }
        Ok(())
    }
    /// Call only AFTER authoritative account proof. Command-ID knowledge is not proof.
    pub(super) fn receipt(
        &self,
        account: AccountId,
        command: CommandId,
        fp: [u8; DIGEST_BYTES],
        now: i64,
    ) -> Result<Option<AuthOutcome>, AuthError> {
        if !self.db.query(
            "SELECT 1 FROM management_receipts WHERE actor=?1 AND command_id=?2 UNION ALL SELECT 1 FROM pending_account_removals WHERE actor=?1 AND command_id=?2 LIMIT 1",
            &[SqlValue::Text(account.to_string()), SqlValue::Text(command.to_string())],
        )?.is_empty() {
            return Err(AuthError::Conflict);
        }
        let rows=self.db.query("SELECT request_fingerprint,outcome,completed_at,expires_at FROM command_receipts WHERE actor_account_id=? AND command_id=?",&[SqlValue::Text(account.to_string()),SqlValue::Text(command.to_string())])?;
        let Some(row) = rows.first() else {
            self.admit_command(command, now)?;
            return Ok(None);
        };
        if row.len() != 4 {
            return Err(AuthError::Storage);
        }
        if integer(row, 3)? <= now {
            return Err(AuthError::StaleCommand);
        }
        match row.first() {
            Some(SqlValue::Blob(b)) if b.as_slice() == fp => {}
            Some(SqlValue::Blob(_)) => return Err(AuthError::Conflict),
            _ => return Err(AuthError::Storage),
        };
        let raw = text(row, 1)?;
        if raw.len() > COMMAND_RECEIPT_MAX_BYTES {
            return Err(AuthError::Storage);
        }
        let stored: StoredOutcome = serde_json::from_str(raw).map_err(|_| AuthError::Storage)?;
        let bound_account = match &stored.outcome {
            CommandOutcome::Login { account_id, .. }
            | CommandOutcome::Redeem { account_id, .. }
            | CommandOutcome::CompleteEnrollment { account_id }
            | CommandOutcome::CompletePasswordReset { account_id } => *account_id,
        };
        if stored.version != 1 || bound_account != account {
            return Err(AuthError::Storage);
        }
        Ok(Some(AuthOutcome {
            body: serde_json::json!({"replayed":true,"receipt":{"version":1,"command_id":command,"outcome":stored.outcome,"completed_at":integer(row,2)?,"expires_at":integer(row,3)?}}),
            cookie: CookieEffect::None,
        }))
    }
    pub(super) fn record_receipt(
        &self,
        account: AccountId,
        command: CommandId,
        fp: [u8; DIGEST_BYTES],
        outcome: CommandOutcome,
        now: i64,
    ) -> Result<(), AuthError> {
        let raw = serde_json::to_string(&StoredOutcome {
            version: 1,
            outcome,
        })
        .map_err(|_| AuthError::Storage)?;
        if raw.len() > COMMAND_RECEIPT_MAX_BYTES {
            return Err(AuthError::Storage);
        }
        let expires = now
            .checked_add(COMMAND_RECEIPT_RETENTION_MS)
            .filter(|v| *v <= JS_SAFE_INTEGER_MAX)
            .ok_or(AuthError::Crypto)?;
        self.db.execute("INSERT INTO command_receipts(actor_account_id,command_id,request_fingerprint,outcome,completed_at,expires_at) VALUES(?,?,?,?,?,?)",&[SqlValue::Text(account.to_string()),SqlValue::Text(command.to_string()),SqlValue::Blob(fp.to_vec()),SqlValue::Text(raw),SqlValue::Integer(now),SqlValue::Integer(expires)])?;
        Ok(())
    }
}
