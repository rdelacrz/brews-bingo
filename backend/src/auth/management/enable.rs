use super::{ManagementError, deadline};
use crate::{
    auth::{AuthService, Runtime},
    limits::COMMAND_RECEIPT_RETENTION_MS,
    storage::{Database, SqlValue},
};
use brews_contracts::management::{
    AuditActor, AuditOutcome, ManagementCommand, ManagementReceipt, ManagementResponse,
    ReceiptOperation,
};
use brews_domain::ids::{AccountId, CommandId};
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn enable_account(
        &self,
        id: AccountId,
        actor: AuditActor,
        command: &ManagementCommand,
        command_id: CommandId,
    ) -> Result<ManagementResponse, ManagementError> {
        if actor == AuditActor::Account(id) {
            return Err(ManagementError::Forbidden);
        }
        self.target_unlocked(id)?;
        self.account(id)?.ok_or(ManagementError::NotFound)?;
        let now = self.now()?;
        self.db.execute(
            "UPDATE accounts SET disabled_at=NULL WHERE account_id=? AND disabled_at IS NOT NULL",
            &[SqlValue::Text(id.to_string())],
        )?;
        let receipt = ManagementReceipt {
            version: 1,
            command_id,
            operation: ReceiptOperation::EnableAccount,
            account_id: id,
            link_id: None,
            purpose: None,
            link_expires_at: None,
            completed_at: now,
            expires_at: deadline(now, COMMAND_RECEIPT_RETENTION_MS)?,
        };
        self.store_management_receipt(actor, command, &receipt)?;
        self.management_audit(
            actor,
            receipt.operation,
            brews_contracts::management::AuditTarget::Account(id),
            AuditOutcome::Succeeded,
            now,
        )?;
        Ok(ManagementResponse::Committed { receipt })
    }
}
