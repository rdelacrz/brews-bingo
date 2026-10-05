use super::{ManagementError, actor_key};
use crate::{
    auth::{
        AuthService, Runtime,
        records::{integer, text},
    },
    limits::COMMAND_RECEIPT_MAX_BYTES,
    storage::{Database, SqlValue},
};
use brews_contracts::management::{AuditActor, ManagementCommand, ManagementReceipt};
use brews_domain::ids::CommandId;
use sha2::{Digest, Sha256};
pub(super) fn fingerprint(
    command: &ManagementCommand,
) -> Result<[u8; crate::security::DIGEST_BYTES], ManagementError> {
    let semantic = match command {
        ManagementCommand::CreateAccount { username, role } => ManagementCommand::CreateAccount {
            username: brews_domain::accounts::validate_username(username)
                .map_err(|_| ManagementError::InvalidInput)?
                .to_owned(),
            role: *role,
        },
        command => command.clone(),
    };
    Ok(Sha256::digest(
        serde_json::to_vec(&("brews-management-v1", &semantic))
            .map_err(|_| ManagementError::Storage)?,
    )
    .into())
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    /// Call inside the management transaction, after live actor authorization.
    pub(super) fn check_auth_receipt_conflict(
        &self,
        actor: AuditActor,
        id: CommandId,
    ) -> Result<(), ManagementError> {
        if let AuditActor::Account(account) = actor
            && !self.db.query(
                "SELECT 1 FROM command_receipts WHERE actor_account_id=? AND command_id=? LIMIT 1",
                &[SqlValue::Text(account.to_string()), SqlValue::Text(id.to_string())],
            )?.is_empty()
        {
            return Err(ManagementError::Conflict);
        }
        Ok(())
    }
    pub(super) fn management_receipt(
        &self,
        actor: AuditActor,
        id: CommandId,
        command: &ManagementCommand,
        now: i64,
    ) -> Result<Option<ManagementReceipt>, ManagementError> {
        self.check_auth_receipt_conflict(actor, id)?;
        let rows=self.db.query("SELECT request_fingerprint,outcome,completed_at,expires_at FROM management_receipts WHERE actor=? AND command_id=?",&[SqlValue::Text(actor_key(actor)),SqlValue::Text(id.to_string())])?;
        let Some(row) = rows.first() else {
            // Longer issuance retention permits replay, never fresh admission.
            self.admit_command(id, now).map_err(|error| match error {
                crate::auth::AuthError::StaleCommand => ManagementError::StaleCommand,
                crate::auth::AuthError::InvalidInput => ManagementError::InvalidInput,
                error => ManagementError::from(error),
            })?;
            return Ok(None);
        };
        if row.len() != 4 {
            return Err(ManagementError::Storage);
        }
        if row[0] != SqlValue::Blob(fingerprint(command)?.to_vec()) {
            return Err(ManagementError::Conflict);
        }
        if integer(row, 3)? <= now {
            return Err(ManagementError::StaleCommand);
        }
        let payload = text(row, 1)?;
        if payload.len() > COMMAND_RECEIPT_MAX_BYTES {
            return Err(ManagementError::Storage);
        }
        let receipt: ManagementReceipt =
            serde_json::from_str(payload).map_err(|_| ManagementError::Storage)?;
        if receipt.version != 1
            || receipt.command_id != id
            || receipt.completed_at != integer(row, 2)?
            || receipt.expires_at != integer(row, 3)?
        {
            return Err(ManagementError::Storage);
        }
        validate_receipt(&receipt)?;
        if brews_contracts::management::AuditOperation::from(receipt.operation)
            != super::audit::action(command).0
        {
            return Err(ManagementError::Storage);
        }
        Ok(Some(receipt))
    }
}

pub(super) fn validate_receipt(receipt: &ManagementReceipt) -> Result<(), ManagementError> {
    use brews_contracts::management::ReceiptOperation;
    use brews_domain::accounts::AccessLinkPurpose;
    let purpose = match receipt.operation {
        ReceiptOperation::CreateAccount | ReceiptOperation::ReissueEnrollment => {
            Some(AccessLinkPurpose::Enrollment)
        }
        ReceiptOperation::ResetPassword => Some(AccessLinkPurpose::PasswordReset),
        ReceiptOperation::DisableAccount
        | ReceiptOperation::DeleteAccount
        | ReceiptOperation::EnableAccount => None,
    };
    let period = if purpose.is_some() {
        crate::storage::management_schema::LINK_RECEIPT_RETENTION_MS
    } else {
        crate::limits::COMMAND_RECEIPT_RETENTION_MS
    };
    if receipt.version != 1
        || !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&receipt.completed_at)
        || super::deadline(receipt.completed_at, period)? != receipt.expires_at
    {
        return Err(ManagementError::Storage);
    }
    if let Some(purpose) = purpose {
        if receipt.link_id.is_none()
            || receipt.purpose != Some(purpose)
            || receipt.link_expires_at
                != Some(super::deadline(
                    receipt.completed_at,
                    crate::limits::ACCESS_LINK_LIFETIME_MS,
                )?)
        {
            return Err(ManagementError::Storage);
        }
    } else if receipt.link_id.is_some()
        || receipt.purpose.is_some()
        || receipt.link_expires_at.is_some()
    {
        return Err(ManagementError::Storage);
    }
    Ok(())
}
