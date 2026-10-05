use super::ManagementError;
use crate::{
    auth::{AuthService, Runtime, records::Account},
    storage::{Database, SqlValue},
};
use brews_contracts::{SafeAccount, management::ManagementResponse};
use brews_domain::ids::AccountId;
pub(super) const MAX_PAGE_SIZE: u32 = 100;
pub(super) const DEFAULT_PAGE_SIZE: u32 = 50;
pub(super) fn page_size(limit: u32) -> Result<u32, ManagementError> {
    if limit > MAX_PAGE_SIZE {
        return Err(ManagementError::InvalidInput);
    }
    Ok(if limit == 0 { DEFAULT_PAGE_SIZE } else { limit })
}
pub(super) fn safe(a: Account) -> SafeAccount {
    SafeAccount {
        account_id: a.id,
        username: a.username,
        role: a.role,
        status: a.status,
        disabled: a.disabled.is_some(),
        created_at: a.created,
    }
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn management_accounts(
        &self,
        after: Option<AccountId>,
        limit: u32,
    ) -> Result<ManagementResponse, ManagementError> {
        let limit = page_size(limit)?;
        let rows=self.db.query("SELECT account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at FROM accounts WHERE account_id>? ORDER BY account_id LIMIT ?",&[SqlValue::Text(after.map_or_else(String::new,|id|id.to_string())),SqlValue::Integer(i64::from(limit)+1)])?;
        let mut accounts = rows
            .iter()
            .map(Account::parse)
            .map(|r| r.map(safe).map_err(ManagementError::from))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if accounts.len() > limit as usize {
            accounts.truncate(limit as usize);
            accounts.last().map(|a| a.account_id)
        } else {
            None
        };
        Ok(ManagementResponse::Accounts {
            accounts,
            next_cursor,
        })
    }
}

impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn management_audit_page(
        &self,
        after: Option<brews_domain::ids::AuditId>,
        limit: u32,
    ) -> Result<ManagementResponse, ManagementError> {
        use crate::auth::records::{integer, optional_text, text};
        use brews_contracts::management::{AuditActor, AuditEvent, AuditTarget};
        let limit = page_size(limit)?;
        let rows=self.db.query("SELECT audit_id,actor,operation,target_kind,target_account_id,outcome,occurred_at,expires_at FROM admin_audit WHERE audit_id>? AND expires_at>? ORDER BY audit_id LIMIT ?",&[SqlValue::Text(after.map_or_else(String::new,|id|id.to_string())),SqlValue::Integer(self.now()?),SqlValue::Integer(i64::from(limit)+1)])?;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            if row.len() != 8 {
                return Err(ManagementError::Storage);
            }
            let actor = match text(&row, 1)? {
                "developer_cli" => AuditActor::DeveloperCli,
                id => AuditActor::Account(id.parse().map_err(|_| ManagementError::Storage)?),
            };
            let parse_enum = |index| {
                serde_json::from_value::<serde_json::Value>(serde_json::Value::String(
                    text(&row, index)?.into(),
                ))
                .map_err(|_| ManagementError::Storage)
            };
            events.push(AuditEvent {
                audit_id: text(&row, 0)?
                    .parse()
                    .map_err(|_| ManagementError::Storage)?,
                actor,
                operation: serde_json::from_value(parse_enum(2)?)
                    .map_err(|_| ManagementError::Storage)?,
                target: match (text(&row, 3)?, optional_text(&row, 4)?) {
                    ("account", Some(id)) => {
                        AuditTarget::Account(id.parse().map_err(|_| ManagementError::Storage)?)
                    }
                    ("accounts_owner", None) => AuditTarget::AccountsOwner,
                    _ => return Err(ManagementError::Storage),
                },
                outcome: serde_json::from_value(parse_enum(5)?)
                    .map_err(|_| ManagementError::Storage)?,
                occurred_at: integer(&row, 6)?,
                expires_at: integer(&row, 7)?,
            });
        }
        let next_cursor = if events.len() > limit as usize {
            events.truncate(limit as usize);
            events.last().map(|e| e.audit_id)
        } else {
            None
        };
        Ok(ManagementResponse::Audit {
            events,
            next_cursor,
        })
    }
}
