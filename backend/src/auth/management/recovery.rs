use super::{ManagementError, RemovalPhase, RemovalReleaseAck, RemovalWork, deadline};
use crate::{
    auth::{AuthService, Runtime},
    storage::{
        Database, SqlValue,
        management_schema::{REMOVAL_INITIAL_DELAY_MS, REMOVAL_MAX_DELAY_MS},
    },
};
use brews_contracts::management::AuditOutcome;
use brews_domain::ids::OperationId;
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    /// Only due work, bounded and stably ordered. Never drops unresolved intents.
    pub fn removal_work(&self, limit: u32) -> Result<Vec<RemovalWork>, ManagementError> {
        let limit = super::reads::page_size(limit)?;
        let rows=self.db.query("SELECT operation_id,actor,command_id,target_account_id,operation,phase,created_at,next_attempt_at,attempt_count FROM pending_account_removals WHERE next_attempt_at<=? ORDER BY next_attempt_at,operation_id LIMIT ?",&[SqlValue::Integer(self.now()?),SqlValue::Integer(i64::from(limit))])?;
        rows.iter().map(super::removal::parse_work).collect()
    }
    pub fn retry_removal(&self, operation_id: OperationId) -> Result<(), ManagementError> {
        self.management_transaction(|| {
            let work=self.removal_operation(operation_id)?.ok_or(ManagementError::NotFound)?;
            let delay=REMOVAL_INITIAL_DELAY_MS.saturating_mul(1i64<<work.attempt_count.min(9)).min(REMOVAL_MAX_DELAY_MS);
            self.db.execute("UPDATE pending_account_removals SET attempt_count=?,next_attempt_at=? WHERE operation_id=?",&[SqlValue::Integer(i64::from(work.attempt_count.saturating_add(1))),SqlValue::Integer(deadline(self.now()?,delay)?),SqlValue::Text(operation_id.to_string())])?;
            Ok(())
        })
    }
    /// Trusted coordinator first persists abort intent, then requests peer release.
    pub fn begin_removal_abort(&self, operation_id: OperationId) -> Result<(), ManagementError> {
        self.management_transaction(|| {
            let work = self
                .removal_operation(operation_id)?
                .ok_or(ManagementError::NotFound)?;
            match work.phase {
                RemovalPhase::Committed => return Err(ManagementError::Conflict),
                RemovalPhase::Aborting => return Ok(()),
                RemovalPhase::Prepared => {}
            }
            self.db.execute(
                "UPDATE pending_account_removals SET phase=? WHERE operation_id=?",
                &[
                    SqlValue::Text(RemovalPhase::Aborting.as_str().into()),
                    SqlValue::Text(operation_id.to_string()),
                ],
            )?;
            self.management_audit(
                work.actor,
                work.operation,
                brews_contracts::management::AuditTarget::Account(work.target_account_id),
                AuditOutcome::Rejected,
                self.now()?,
            )?;
            Ok(())
        })
    }
    /// No mutation may commit after abort intent; discard only after exact release proof.
    pub fn abort_removal(
        &self,
        operation_id: OperationId,
        ack: &RemovalReleaseAck,
    ) -> Result<(), ManagementError> {
        self.management_transaction(|| {
            if ack.operation_id != operation_id {
                return Err(ManagementError::Conflict);
            }
            let Some(work) = self.removal_operation(operation_id)? else {
                if self.removal_receipt(operation_id)?.is_some() {
                    return Err(ManagementError::Conflict);
                }
                return Ok(());
            };
            if work.phase != RemovalPhase::Aborting || ack.account_id != work.target_account_id {
                return Err(ManagementError::Conflict);
            }
            self.db.execute(
                "DELETE FROM pending_account_removals WHERE operation_id=?",
                &[SqlValue::Text(operation_id.to_string())],
            )?;
            Ok(())
        })
    }
}
