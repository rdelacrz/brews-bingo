//! Trusted account management reuses AccountsObject auth/storage authority.
//! Allocation-conscious: pages, receipt payloads and entropy retries are bounded.
use super::{AuthError, AuthService, Runtime};
mod audit;
mod enable;
mod links;
mod maintenance;
mod reads;
mod receipts;
mod recovery;
mod removal;
mod users;
pub use removal::{RemovalGateGrant, RemovalPhase, RemovalReleaseAck, RemovalWork};
#[cfg(test)]
pub(crate) mod test_support;
use crate::{
    db::{
        Database, SqlValue, StorageError,
        schema::management_schema::{AUDIT_RETENTION_MS, LINK_RECEIPT_RETENTION_MS},
    },
    limits::{
        ACCESS_LINK_LIFETIME_MS, COMMAND_RECEIPT_MAX_BYTES, JS_SAFE_INTEGER_MAX,
        TOKEN_GENERATION_MAX_ATTEMPTS,
    },
    security::{new_token, token_digest},
};
use brews_contracts::management::{
    AuditActor, AuditOperation, AuditOutcome, AuditTarget, ManagementCommand, ManagementReceipt,
    ManagementResponse, ReceiptOperation,
};
use brews_domain::{
    accounts::{AccessLinkPurpose, AccountRole, AccountStatus, validate_username},
    ids::{AccountId, CommandId, LinkId},
};
use std::{cell::Cell, fmt};
use zeroize::Zeroizing;

pub enum ManagementPrincipal {
    DeveloperCli,
    AdminSession(String),
}
impl zeroize::Zeroize for ManagementPrincipal {
    fn zeroize(&mut self) {
        if let Self::AdminSession(token) = self {
            token.zeroize();
        }
    }
}
impl zeroize::ZeroizeOnDrop for ManagementPrincipal {}
impl Drop for ManagementPrincipal {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(self);
    }
}
impl fmt::Debug for ManagementPrincipal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DeveloperCli => "DeveloperCli",
            Self::AdminSession(_) => "AdminSession([redacted])",
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ManagementError {
    #[error("Invalid management input.")]
    InvalidInput,
    #[error("Authentication required.")]
    Unauthorized,
    #[error("Management action forbidden.")]
    Forbidden,
    #[error("Account not found.")]
    NotFound,
    #[error("Management command conflict.")]
    Conflict,
    #[error("Command outside its retry window.")]
    StaleCommand,
    #[error("At least one enabled verified admin is required.")]
    LastAdmin,
    #[error("Account hosts a nonterminal game.")]
    HostedGame,
    #[error("Account coordination is pending.")]
    Busy,
    #[error("Management unavailable.")]
    Storage,
    #[error("Management unavailable.")]
    Crypto,
}
impl ManagementError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::StaleCommand => "stale_command",
            Self::LastAdmin => "last_admin",
            Self::HostedGame => "hosted_game",
            Self::Busy => "busy",
            Self::Storage | Self::Crypto => "unavailable",
        }
    }
    pub const fn status(self) -> u16 {
        match self {
            Self::InvalidInput => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::Conflict
            | Self::StaleCommand
            | Self::LastAdmin
            | Self::HostedGame
            | Self::Busy => 409,
            Self::Storage | Self::Crypto => 503,
        }
    }
}
impl From<StorageError> for ManagementError {
    fn from(_: StorageError) -> Self {
        Self::Storage
    }
}
impl From<AuthError> for ManagementError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Crypto => Self::Crypto,
            AuthError::Storage => Self::Storage,
            _ => Self::Unauthorized,
        }
    }
}
fn deadline(now: i64, period: i64) -> Result<i64, ManagementError> {
    now.checked_add(period)
        .filter(|n| *n <= JS_SAFE_INTEGER_MAX)
        .ok_or(ManagementError::Crypto)
}
fn actor_key(actor: AuditActor) -> String {
    match actor {
        AuditActor::DeveloperCli => "developer_cli".into(),
        AuditActor::Account(id) => id.to_string(),
    }
}
fn operation_tag(operation: ReceiptOperation) -> Result<String, ManagementError> {
    serde_json::to_value(operation)
        .map_err(|_| ManagementError::Storage)?
        .as_str()
        .map(str::to_owned)
        .ok_or(ManagementError::Storage)
}

impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    fn management_transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, ManagementError>,
    ) -> Result<T, ManagementError> {
        let fatal = Cell::new(None);
        let result = self.db.transaction(|| {
            let result = self
                .observe_clock()
                .map_err(ManagementError::from)
                .and_then(|_| operation());
            match result {
                Err(e @ (ManagementError::Storage | ManagementError::Crypto)) => {
                    fatal.set(Some(e));
                    Err(StorageError)
                }
                result => Ok(result),
            }
        });
        result.map_err(|_| fatal.get().unwrap_or(ManagementError::Storage))?
    }
    fn management_actor(
        &self,
        principal: &ManagementPrincipal,
    ) -> Result<AuditActor, ManagementError> {
        match principal {
            ManagementPrincipal::DeveloperCli => Ok(AuditActor::DeveloperCli),
            ManagementPrincipal::AdminSession(token) => {
                let session = self.session(token)?.ok_or(ManagementError::Unauthorized)?;
                let account = self
                    .account(session.account_id)?
                    .ok_or(ManagementError::Unauthorized)?;
                if session.scope != brews_domain::accounts::SessionScope::Normal
                    || !session.eligible(&account, self.now()?)
                {
                    return Err(ManagementError::Unauthorized);
                }
                if account.role != AccountRole::Admin {
                    return Err(ManagementError::Forbidden);
                }
                Ok(AuditActor::Account(account.id))
            }
        }
    }
    pub fn execute_management(
        &self,
        command: ManagementCommand,
        principal: ManagementPrincipal,
        command_id: Option<CommandId>,
        canonical_origin: &str,
    ) -> Result<ManagementResponse, ManagementError> {
        self.management_attempt(&principal, &command, |actor| {
            self.execute_management_checked(&command, actor, command_id, canonical_origin)
        })
    }
    fn execute_management_checked(
        &self,
        command: &ManagementCommand,
        actor: AuditActor,
        command_id: Option<CommandId>,
        canonical_origin: &str,
    ) -> Result<ManagementResponse, ManagementError> {
        if command.is_mutating() {
            let id = command_id.ok_or(ManagementError::InvalidInput)?;
            if let Some(pending) = self.pending_removal_response(actor, id, command)? {
                return Ok(pending);
            }
            if let Some(receipt) = self.management_receipt(actor, id, command, self.now()?)? {
                return Ok(ManagementResponse::Committed { receipt });
            }
        }
        if matches!(
            command,
            ManagementCommand::CreateAccount { .. }
                | ManagementCommand::ReissueEnrollment { .. }
                | ManagementCommand::ResetPassword { .. }
        ) {
            validate_origin(canonical_origin)?;
        }
        if !command.is_mutating() && command_id.is_some() {
            return Err(ManagementError::InvalidInput);
        }
        match command {
            ManagementCommand::ListAccounts { after, limit } => {
                self.management_accounts(*after, *limit)
            }
            ManagementCommand::CreateAccount { username, role } => {
                let command_id = command_id.ok_or(ManagementError::InvalidInput)?;
                let username =
                    validate_username(username).map_err(|_| ManagementError::InvalidInput)?;
                if !self
                    .db
                    .query(
                        "SELECT account_id FROM accounts WHERE username=? COLLATE BINARY",
                        &[SqlValue::Text(username.into())],
                    )?
                    .is_empty()
                {
                    return Err(ManagementError::Conflict);
                }
                let now = self.now()?;
                let account_id: AccountId = self
                    .new_uuid(now)?
                    .try_into()
                    .map_err(|_| ManagementError::Crypto)?;
                self.db.execute(
                        "INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?,?,?,NULL,0,?,NULL,NULL)",
                        &[
                            SqlValue::Text(account_id.to_string()),
                            SqlValue::Text(username.into()),
                            SqlValue::Text(role.to_string()),
                            SqlValue::Text(AccountStatus::PendingEnrollment.to_string()),
                            SqlValue::Integer(now),
                        ],
                    )?;
                let (link_id, token) =
                    self.management_link(account_id, AccessLinkPurpose::Enrollment, 0, now)?;
                let receipt = ManagementReceipt {
                    version: 1,
                    command_id,
                    operation: ReceiptOperation::CreateAccount,
                    account_id,
                    link_id: Some(link_id),
                    purpose: Some(AccessLinkPurpose::Enrollment),
                    link_expires_at: Some(deadline(now, ACCESS_LINK_LIFETIME_MS)?),
                    completed_at: now,
                    expires_at: deadline(now, LINK_RECEIPT_RETENTION_MS)?,
                };
                self.store_management_receipt(actor, command, &receipt)?;
                self.management_audit(
                    actor,
                    receipt.operation,
                    AuditTarget::Account(account_id),
                    AuditOutcome::Succeeded,
                    now,
                )?;
                Ok(ManagementResponse::Issued {
                    receipt,
                    url: format!("{canonical_origin}/enroll#{}", token.as_str()),
                })
            }
            ManagementCommand::ReissueEnrollment { account_id } => self.replace_link(
                *account_id,
                actor,
                command,
                command_id.ok_or(ManagementError::InvalidInput)?,
                canonical_origin,
            ),
            ManagementCommand::ResetPassword { account_id } => self.replace_link(
                *account_id,
                actor,
                command,
                command_id.ok_or(ManagementError::InvalidInput)?,
                canonical_origin,
            ),
            ManagementCommand::EnableAccount { account_id } => self.enable_account(
                *account_id,
                actor,
                command,
                command_id.ok_or(ManagementError::InvalidInput)?,
            ),
            ManagementCommand::GetAccount { account_id } => Ok(ManagementResponse::Account {
                account: reads::safe(
                    self.account(*account_id)?
                        .ok_or(ManagementError::NotFound)?,
                ),
            }),
            ManagementCommand::ListAudit { after, limit } => {
                if actor != AuditActor::DeveloperCli {
                    return Err(ManagementError::Forbidden);
                }
                self.management_audit_page(*after, *limit)
            }
            ManagementCommand::DisableAccount { .. } | ManagementCommand::DeleteAccount { .. } => {
                self.prepare_removal_checked(
                    command,
                    actor,
                    command_id.ok_or(ManagementError::InvalidInput)?,
                )
            }
        }
    }
    fn management_link(
        &self,
        account: AccountId,
        purpose: AccessLinkPurpose,
        epoch: i64,
        now: i64,
    ) -> Result<(LinkId, Zeroizing<String>), ManagementError> {
        for _ in 0..TOKEN_GENERATION_MAX_ATTEMPTS {
            let token = Zeroizing::new(new_token(self.runtime)?);
            let digest = token_digest(&token)?;
            let link: LinkId = self
                .new_uuid(now)?
                .try_into()
                .map_err(|_| ManagementError::Crypto)?;
            if !self
                .db
                .query(
                    "SELECT link_id FROM access_links WHERE token_verifier=? OR link_id=?",
                    &[
                        SqlValue::Blob(digest.to_vec()),
                        SqlValue::Text(link.to_string()),
                    ],
                )?
                .is_empty()
            {
                continue;
            }
            self.db.execute("INSERT INTO access_links(link_id,account_id,purpose,token_verifier,credential_epoch,issued_at,expires_at,consumed_at,revoked_at) VALUES(?,?,?,?,?,?,?,NULL,NULL)",&[SqlValue::Text(link.to_string()),SqlValue::Text(account.to_string()),SqlValue::Text(purpose.to_string()),SqlValue::Blob(digest.to_vec()),SqlValue::Integer(epoch),SqlValue::Integer(now),SqlValue::Integer(deadline(now,ACCESS_LINK_LIFETIME_MS)?)])?;
            return Ok((link, token));
        }
        Err(ManagementError::Crypto)
    }
    fn store_management_receipt(
        &self,
        actor: AuditActor,
        command: &ManagementCommand,
        receipt: &ManagementReceipt,
    ) -> Result<(), ManagementError> {
        receipts::validate_receipt(receipt)?;
        let fingerprint = receipts::fingerprint(command)?;
        let outcome = serde_json::to_string(receipt).map_err(|_| ManagementError::Storage)?;
        if outcome.len() > COMMAND_RECEIPT_MAX_BYTES {
            return Err(ManagementError::Storage);
        }
        self.db.execute("INSERT INTO management_receipts(actor,command_id,request_fingerprint,outcome,completed_at,expires_at) VALUES(?,?,?,?,?,?)", &[SqlValue::Text(actor_key(actor)),SqlValue::Text(receipt.command_id.to_string()),SqlValue::Blob(fingerprint.to_vec()),SqlValue::Text(outcome),SqlValue::Integer(receipt.completed_at),SqlValue::Integer(receipt.expires_at)])?;
        Ok(())
    }
    fn management_audit(
        &self,
        actor: AuditActor,
        operation: impl Into<AuditOperation>,
        target: AuditTarget,
        outcome: AuditOutcome,
        now: i64,
    ) -> Result<(), ManagementError> {
        let id: brews_contracts::management::AuditId = self
            .new_uuid(now)?
            .try_into()
            .map_err(|_| ManagementError::Crypto)?;
        let outcome = serde_json::to_value(outcome)
            .map_err(|_| ManagementError::Storage)?
            .as_str()
            .map(str::to_owned)
            .ok_or(ManagementError::Storage)?;
        let operation = serde_json::to_value(operation.into())
            .map_err(|_| ManagementError::Storage)?
            .as_str()
            .map(str::to_owned)
            .ok_or(ManagementError::Storage)?;
        let (kind, target) = match target {
            AuditTarget::Account(id) => ("account", SqlValue::Text(id.to_string())),
            AuditTarget::AccountsOwner => ("accounts_owner", SqlValue::Null),
        };
        self.db.execute("INSERT INTO admin_audit(audit_id,actor,operation,target_kind,target_account_id,outcome,occurred_at,expires_at) VALUES(?,?,?,?,?,?,?,?)",&[SqlValue::Text(id.to_string()),SqlValue::Text(actor_key(actor)),SqlValue::Text(operation),SqlValue::Text(kind.into()),target,SqlValue::Text(outcome),SqlValue::Integer(now),SqlValue::Integer(deadline(now,AUDIT_RETENTION_MS)?)])?;
        Ok(())
    }
}

fn validate_origin(origin: &str) -> Result<(), ManagementError> {
    let uri: http::Uri = origin.parse().map_err(|_| ManagementError::InvalidInput)?;
    let authority = uri.authority().ok_or(ManagementError::InvalidInput)?;
    if origin.len() > brews_config::env::backend::APP_ORIGIN_MAX_BYTES
        || uri.scheme_str() != Some("https")
        || uri.path() != "/"
        || uri.query().is_some()
        || origin.contains(['#', '@', '%'])
        || authority.as_str() != authority.as_str().to_ascii_lowercase()
        || origin != format!("https://{authority}")
        || authority.port_u16() == Some(443)
    {
        return Err(ManagementError::InvalidInput);
    }
    Ok(())
}
