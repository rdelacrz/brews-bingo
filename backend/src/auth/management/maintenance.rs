use super::ManagementError;
use crate::{
    auth::{AuthService, Runtime, records::optional_integer},
    db::{Database, SqlValue},
    limits::{CLEANUP_BATCH_SIZE, JS_SAFE_INTEGER_MAX},
};
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    /// Include this deadline with auth deadlines in the AccountsObject durable alarm.
    pub fn next_management_deadline(&self) -> Result<Option<i64>, ManagementError> {
        let rows=self.db.query("SELECT (SELECT min(expires_at) FROM management_receipts r WHERE NOT EXISTS(SELECT 1 FROM pending_account_removals p WHERE p.actor=r.actor AND p.command_id=r.command_id)),(SELECT min(expires_at) FROM admin_audit),(SELECT min(next_attempt_at) FROM pending_account_removals)",&[])?;
        let row = rows.first().ok_or(ManagementError::Storage)?;
        if row.len() != 3 {
            return Err(ManagementError::Storage);
        }
        let mut result: Option<i64> = None;
        for i in 0..3 {
            if let Some(v) = optional_integer(row, i)? {
                if !(0..=JS_SAFE_INTEGER_MAX).contains(&v) {
                    return Err(ManagementError::Storage);
                }
                result = Some(result.map_or(v, |old| old.min(v)));
            }
        }
        Ok(result)
    }
    pub fn cleanup_management(&self) -> Result<(), ManagementError> {
        self.management_transaction(|| {
            let now=self.now()?;
            self.db.execute("DELETE FROM management_receipts WHERE (actor,command_id) IN (SELECT actor,command_id FROM management_receipts WHERE expires_at<=? AND NOT EXISTS(SELECT 1 FROM pending_account_removals p WHERE p.actor=management_receipts.actor AND p.command_id=management_receipts.command_id) LIMIT ?)",&[SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?;
            self.db.execute("DELETE FROM admin_audit WHERE audit_id IN (SELECT audit_id FROM admin_audit WHERE expires_at<=? LIMIT ?)",&[SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?;
            Ok(())
        })
    }
}
