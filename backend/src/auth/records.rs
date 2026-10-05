use crate::{
    auth::AuthError,
    limits::JS_SAFE_INTEGER_MAX,
    storage::{Row, SqlValue},
};
use brews_domain::{
    accounts::{AccountRole, AccountStatus, SessionScope, validate_username},
    ids::{AccountId, SessionId},
};

pub(super) const ACCOUNT_SELECT: &str = "SELECT account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at FROM accounts WHERE account_id=?";
pub(super) struct Account {
    pub id: AccountId,
    pub username: String,
    pub role: AccountRole,
    pub status: AccountStatus,
    pub verifier: Option<String>,
    pub epoch: i64,
    pub created: i64,
    pub disabled: Option<i64>,
}
impl Account {
    pub fn parse(row: &Row) -> Result<Self, AuthError> {
        if row.len() != 9 {
            return Err(AuthError::Storage);
        }
        let username = text(row, 1)?.to_owned();
        if validate_username(&username).map_err(|_| AuthError::Storage)? != username {
            return Err(AuthError::Storage);
        }
        let epoch = integer(row, 5)?;
        if !(0..=JS_SAFE_INTEGER_MAX).contains(&epoch) {
            return Err(AuthError::Storage);
        }
        Ok(Self {
            id: text(row, 0)?.parse().map_err(|_| AuthError::Storage)?,
            username,
            role: text(row, 2)?.parse().map_err(|_| AuthError::Storage)?,
            status: text(row, 3)?.parse().map_err(|_| AuthError::Storage)?,
            verifier: optional_text(row, 4)?.map(str::to_owned),
            epoch,
            created: integer(row, 6)?,
            disabled: optional_integer(row, 8)?,
        })
    }
    pub fn login_eligible(&self) -> bool {
        self.disabled.is_none() && self.status == AccountStatus::Verified && self.verifier.is_some()
    }
    pub fn safe(&self) -> serde_json::Value {
        serde_json::json!(brews_contracts::SafeAccount {
            account_id: self.id,
            username: self.username.clone(),
            role: self.role,
            status: self.status,
            disabled: self.disabled.is_some(),
            created_at: self.created
        })
    }
}
pub(super) struct Session {
    pub id: SessionId,
    pub account_id: AccountId,
    pub scope: SessionScope,
    pub epoch: i64,
    pub expires: i64,
    pub revoked: Option<i64>,
}
impl Session {
    pub fn parse(row: &Row) -> Result<Self, AuthError> {
        if row.len() != 6 {
            return Err(AuthError::Storage);
        }
        Ok(Self {
            id: text(row, 0)?.parse().map_err(|_| AuthError::Storage)?,
            account_id: text(row, 1)?.parse().map_err(|_| AuthError::Storage)?,
            scope: text(row, 2)?.parse().map_err(|_| AuthError::Storage)?,
            epoch: integer(row, 3)?,
            expires: integer(row, 4)?,
            revoked: optional_integer(row, 5)?,
        })
    }
    pub fn eligible(&self, a: &Account, now: i64) -> bool {
        let status = match self.scope {
            SessionScope::Normal => AccountStatus::Verified,
            SessionScope::EnrollmentOnly => AccountStatus::PendingEnrollment,
            SessionScope::PasswordResetOnly => AccountStatus::ResetRequired,
        };
        a.id == self.account_id
            && a.disabled.is_none()
            && a.status == status
            && a.epoch == self.epoch
            && self.expires > now
            && self.revoked.is_none()
    }
    pub fn safe(&self, a: &Account) -> serde_json::Value {
        serde_json::json!(brews_contracts::SessionView {
            kind: brews_contracts::SessionKind::Account,
            scope: self.scope,
            expires_at: self.expires,
            account_id: a.id,
            role: a.role
        })
    }
}
pub(super) fn text(row: &Row, i: usize) -> Result<&str, AuthError> {
    match row.get(i) {
        Some(SqlValue::Text(v)) => Ok(v),
        _ => Err(AuthError::Storage),
    }
}
pub(super) fn integer(row: &Row, i: usize) -> Result<i64, AuthError> {
    match row.get(i) {
        Some(SqlValue::Integer(v)) => Ok(*v),
        _ => Err(AuthError::Storage),
    }
}
pub(super) fn optional_text(row: &Row, i: usize) -> Result<Option<&str>, AuthError> {
    match row.get(i) {
        Some(SqlValue::Text(v)) => Ok(Some(v)),
        Some(SqlValue::Null) => Ok(None),
        _ => Err(AuthError::Storage),
    }
}
pub(super) fn optional_integer(row: &Row, i: usize) -> Result<Option<i64>, AuthError> {
    match row.get(i) {
        Some(SqlValue::Integer(v)) => Ok(Some(*v)),
        Some(SqlValue::Null) => Ok(None),
        _ => Err(AuthError::Storage),
    }
}
