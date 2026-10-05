use super::{ManagementError, ManagementPrincipal};
use crate::{
    auth::{AuthService, Runtime},
    storage::Database,
};
use brews_contracts::management::{
    AuditActor, AuditOperation, AuditOutcome, AuditTarget, ManagementCommand,
};
pub(super) fn action(command: &ManagementCommand) -> (AuditOperation, AuditTarget) {
    match command {
        ManagementCommand::ListAccounts { .. } => {
            (AuditOperation::ListAccounts, AuditTarget::AccountsOwner)
        }
        ManagementCommand::ListAudit { .. } => {
            (AuditOperation::ListAudit, AuditTarget::AccountsOwner)
        }
        ManagementCommand::CreateAccount { .. } => {
            (AuditOperation::CreateAccount, AuditTarget::AccountsOwner)
        }
        ManagementCommand::GetAccount { account_id } => (
            AuditOperation::GetAccount,
            AuditTarget::Account(*account_id),
        ),
        ManagementCommand::ReissueEnrollment { account_id } => (
            AuditOperation::ReissueEnrollment,
            AuditTarget::Account(*account_id),
        ),
        ManagementCommand::ResetPassword { account_id } => (
            AuditOperation::ResetPassword,
            AuditTarget::Account(*account_id),
        ),
        ManagementCommand::DisableAccount { account_id } => (
            AuditOperation::DisableAccount,
            AuditTarget::Account(*account_id),
        ),
        ManagementCommand::DeleteAccount { account_id } => (
            AuditOperation::DeleteAccount,
            AuditTarget::Account(*account_id),
        ),
        ManagementCommand::EnableAccount { account_id } => (
            AuditOperation::EnableAccount,
            AuditTarget::Account(*account_id),
        ),
    }
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn management_actor_hint(
        &self,
        principal: &ManagementPrincipal,
    ) -> Result<Option<AuditActor>, ManagementError> {
        match principal {
            ManagementPrincipal::DeveloperCli => Ok(Some(AuditActor::DeveloperCli)),
            ManagementPrincipal::AdminSession(token) => {
                let Some(session) = self.session(token)? else {
                    return Ok(None);
                };
                let Some(account) = self.account(session.account_id)? else {
                    return Ok(None);
                };
                Ok(session
                    .eligible(&account, self.now()?)
                    .then_some(AuditActor::Account(account.id)))
            }
        }
    }
    pub(super) fn management_attempt<T>(
        &self,
        principal: &ManagementPrincipal,
        command: &ManagementCommand,
        operation: impl FnOnce(AuditActor) -> Result<T, ManagementError>,
    ) -> Result<T, ManagementError> {
        let hint = std::cell::Cell::new(None);
        let (audit_operation, target) = action(command);
        let result = self.management_transaction(|| {
            hint.set(self.management_actor_hint(principal)?);
            let result = self.management_actor(principal).and_then(operation);
            match &result {
                Ok(_) if !command.is_mutating() => self.management_audit(
                    hint.get().ok_or(ManagementError::Unauthorized)?,
                    audit_operation,
                    target,
                    AuditOutcome::Succeeded,
                    self.now()?,
                )?,
                Err(error)
                    if !matches!(error, ManagementError::Storage | ManagementError::Crypto) =>
                {
                    if let Some(actor) = hint.get() {
                        self.management_audit(
                            actor,
                            audit_operation,
                            target,
                            AuditOutcome::Rejected,
                            self.now()?,
                        )?;
                    }
                }
                _ => {}
            }
            result
        });
        if matches!(
            result,
            Err(ManagementError::Storage | ManagementError::Crypto)
        ) && let Some(actor) = hint.get()
        {
            self.management_transaction(|| {
                self.management_audit(
                    actor,
                    audit_operation,
                    target,
                    AuditOutcome::Failed,
                    self.now()?,
                )
            })?;
        }
        result
    }
}
