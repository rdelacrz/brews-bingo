use super::{ManagementError, deadline};
use crate::{
    auth::{AuthService, Runtime},
    limits::{ACCESS_LINK_LIFETIME_MS, JS_SAFE_INTEGER_MAX},
    storage::{Database, SqlValue, management_schema::LINK_RECEIPT_RETENTION_MS},
};
use brews_contracts::management::{
    AuditActor, AuditOutcome, ManagementCommand, ManagementReceipt, ManagementResponse,
    ReceiptOperation,
};
use brews_domain::{
    accounts::{AccessLinkPurpose, AccountStatus},
    ids::{AccountId, CommandId},
};
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn invalidate_account_credentials(
        &self,
        id: AccountId,
        now: i64,
    ) -> Result<(), ManagementError> {
        self.enqueue_account_closes(id, now)?;
        self.db.execute(
            "UPDATE account_sessions SET revoked_at=? WHERE account_id=? AND revoked_at IS NULL",
            &[SqlValue::Integer(now), SqlValue::Text(id.to_string())],
        )?;
        self.db.execute(
            "UPDATE access_links SET revoked_at=? WHERE account_id=? AND revoked_at IS NULL",
            &[SqlValue::Integer(now), SqlValue::Text(id.to_string())],
        )?;
        Ok(())
    }
    pub(super) fn replace_link(
        &self,
        id: AccountId,
        actor: AuditActor,
        command: &ManagementCommand,
        command_id: CommandId,
        origin: &str,
    ) -> Result<ManagementResponse, ManagementError> {
        self.target_unlocked(id)?;
        let a = self.account(id)?.ok_or(ManagementError::NotFound)?;
        let (purpose, operation, path, status) = match command {
            ManagementCommand::ReissueEnrollment { .. }
                if a.status == AccountStatus::PendingEnrollment =>
            {
                (
                    AccessLinkPurpose::Enrollment,
                    ReceiptOperation::ReissueEnrollment,
                    "enroll",
                    AccountStatus::PendingEnrollment,
                )
            }
            ManagementCommand::ResetPassword { .. }
                if matches!(
                    a.status,
                    AccountStatus::Verified | AccountStatus::ResetRequired
                ) =>
            {
                (
                    AccessLinkPurpose::PasswordReset,
                    ReceiptOperation::ResetPassword,
                    "password-reset",
                    AccountStatus::ResetRequired,
                )
            }
            _ => return Err(ManagementError::Conflict),
        };
        if a.disabled.is_some() {
            return Err(ManagementError::Conflict);
        }
        let now = self.now()?;
        let epoch = a
            .epoch
            .checked_add(1)
            .filter(|v| *v <= JS_SAFE_INTEGER_MAX)
            .ok_or(ManagementError::Conflict)?;
        self.invalidate_account_credentials(id, now)?;
        self.db.execute(
            "UPDATE accounts SET credential_epoch=?,status=? WHERE account_id=?",
            &[
                SqlValue::Integer(epoch),
                SqlValue::Text(status.to_string()),
                SqlValue::Text(id.to_string()),
            ],
        )?;
        let (link_id, token) = self.management_link(id, purpose, epoch, now)?;
        let receipt = ManagementReceipt {
            version: 1,
            command_id,
            operation,
            account_id: id,
            link_id: Some(link_id),
            purpose: Some(purpose),
            link_expires_at: Some(deadline(now, ACCESS_LINK_LIFETIME_MS)?),
            completed_at: now,
            expires_at: deadline(now, LINK_RECEIPT_RETENTION_MS)?,
        };
        self.store_management_receipt(actor, command, &receipt)?;
        self.management_audit(
            actor,
            receipt.operation,
            brews_contracts::management::AuditTarget::Account(id),
            AuditOutcome::Succeeded,
            now,
        )?;
        Ok(ManagementResponse::Issued {
            receipt,
            url: format!("{origin}/{path}#{}", token.as_str()),
        })
    }
}
