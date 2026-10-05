use super::{ManagementError, ManagementPrincipal, actor_key, deadline, receipts};
use crate::{
    auth::{
        AuthService, Runtime,
        records::{Account, integer, text},
    },
    db::{Database, SqlValue, schema::management_schema::REMOVAL_INITIAL_DELAY_MS},
    limits::JS_SAFE_INTEGER_MAX,
};
use brews_contracts::management::{
    AuditActor, AuditOutcome, ManagementCommand, ManagementReceipt, ManagementResponse,
    ReceiptOperation,
};
use brews_domain::{
    accounts::{AccountRole, AccountStatus},
    ids::{AccountId, CommandId, OperationId},
};
fn target(command: &ManagementCommand) -> Result<AccountId, ManagementError> {
    match command {
        ManagementCommand::DisableAccount { account_id }
        | ManagementCommand::DeleteAccount { account_id } => Ok(*account_id),
        _ => Err(ManagementError::InvalidInput),
    }
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(super) fn target_unlocked(&self, id: AccountId) -> Result<(), ManagementError> {
        if !self
            .db
            .query(
                "SELECT operation_id FROM pending_account_removals WHERE target_account_id=?",
                &[SqlValue::Text(id.to_string())],
            )?
            .is_empty()
        {
            return Err(ManagementError::Busy);
        }
        Ok(())
    }
    fn removal_target(&self, id: AccountId, actor: AuditActor) -> Result<Account, ManagementError> {
        if actor == AuditActor::Account(id) {
            return Err(ManagementError::Forbidden);
        }
        let a = self.account(id)?.ok_or(ManagementError::NotFound)?;
        let rows = self.db.query(
            "SELECT count(*) FROM accounts WHERE role=? AND status=? AND disabled_at IS NULL",
            &[
                SqlValue::Text(AccountRole::Admin.to_string()),
                SqlValue::Text(AccountStatus::Verified.to_string()),
            ],
        )?;
        let count = integer(rows.first().ok_or(ManagementError::Storage)?, 0)?;
        let subtract = i64::from(
            a.role == AccountRole::Admin
                && a.status == AccountStatus::Verified
                && a.disabled.is_none(),
        );
        if subtract == 1 && count <= 1 {
            return Err(ManagementError::LastAdmin);
        }
        Ok(a)
    }
    pub(super) fn pending_removal_response(
        &self,
        actor: AuditActor,
        id: CommandId,
        command: &ManagementCommand,
    ) -> Result<Option<ManagementResponse>, ManagementError> {
        let rows = self.db.query(
            "SELECT operation_id,request_fingerprint FROM pending_account_removals WHERE actor=? AND command_id=?",
            &[
                SqlValue::Text(actor_key(actor)),
                SqlValue::Text(id.to_string()),
            ],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        self.check_auth_receipt_conflict(actor, id)?;
        if row.len() != 2 {
            return Err(ManagementError::Storage);
        }
        if row[1] != SqlValue::Blob(receipts::fingerprint(command)?.to_vec()) {
            return Err(ManagementError::Conflict);
        }
        Ok(Some(ManagementResponse::Pending {
            operation_id: text(row, 0)?
                .parse()
                .map_err(|_| ManagementError::Storage)?,
        }))
    }
    /// Durable intent precedes every private Directory call. This does not mutate credentials.
    pub fn prepare_removal(
        &self,
        command: ManagementCommand,
        principal: ManagementPrincipal,
        command_id: CommandId,
    ) -> Result<ManagementResponse, ManagementError> {
        self.management_attempt(&principal, &command, |actor| {
            self.prepare_removal_checked(&command, actor, command_id)
        })
    }
    pub(super) fn prepare_removal_checked(
        &self,
        command: &ManagementCommand,
        actor: AuditActor,
        command_id: CommandId,
    ) -> Result<ManagementResponse, ManagementError> {
        let account_id = target(command)?;
        if let Some(response) = self.pending_removal_response(actor, command_id, command)? {
            return Ok(response);
        }
        if let Some(receipt) = self.management_receipt(actor, command_id, command, self.now()?)? {
            return Ok(ManagementResponse::Committed { receipt });
        }
        self.removal_target(account_id, actor)?;
        self.target_unlocked(account_id)?;
        let now = self.now()?;
        let operation_id: OperationId = self
            .new_uuid(now)?
            .try_into()
            .map_err(|_| ManagementError::Crypto)?;
        let operation = match command {
            ManagementCommand::DisableAccount { .. } => ReceiptOperation::DisableAccount,
            ManagementCommand::DeleteAccount { .. } => ReceiptOperation::DeleteAccount,
            _ => return Err(ManagementError::InvalidInput),
        };
        self.db.execute(
            "INSERT INTO pending_account_removals(operation_id,actor,command_id,target_account_id,operation,request_fingerprint,phase,created_at,next_attempt_at,attempt_count) VALUES(?,?,?,?,?,?,?,?,?,0)",
            &[
                SqlValue::Text(operation_id.to_string()),
                SqlValue::Text(actor_key(actor)),
                SqlValue::Text(command_id.to_string()),
                SqlValue::Text(account_id.to_string()),
                SqlValue::Text(super::operation_tag(operation)?),
                SqlValue::Blob(receipts::fingerprint(command)?.to_vec()),
                SqlValue::Text(RemovalPhase::Prepared.as_str().into()),
                SqlValue::Integer(now),
                SqlValue::Integer(deadline(now, REMOVAL_INITIAL_DELAY_MS)?),
            ],
        )?;
        Ok(ManagementResponse::Pending { operation_id })
    }
}

/// Construct only from a verified private Directory response at the trusted Worker boundary.
/// Client payloads cannot deserialize this proof.
/// ```compile_fail
/// use brews_backend::auth::RemovalGateGrant;
/// fn request_payload<T: serde::de::DeserializeOwned>() {}
/// request_payload::<RemovalGateGrant>();
/// ```
/// External callers cannot manufacture the trusted proof.
/// ```compile_fail
/// use brews_backend::auth::RemovalGateGrant;
/// use brews_domain::ids::{AccountId, OperationId};
/// fn forge(operation_id: OperationId, target: AccountId) {
///     let _ = RemovalGateGrant::verified(operation_id, target);
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RemovalGateGrant {
    operation_id: OperationId,
    account_id: AccountId,
}
impl RemovalGateGrant {
    #[cfg_attr(
        not(any(target_arch = "wasm32", test)),
        allow(dead_code, reason = "Trusted Worker boundary constructs grants.")
    )]
    pub(crate) fn verified(operation_id: OperationId, account_id: AccountId) -> Self {
        Self {
            operation_id,
            account_id,
        }
    }
}
/// A verified compare-by-operation-and-target Directory release acknowledgement.
#[derive(Clone, Copy, Debug)]
pub struct RemovalReleaseAck {
    pub(super) operation_id: OperationId,
    pub(super) account_id: AccountId,
}
impl RemovalReleaseAck {
    #[cfg_attr(
        not(any(target_arch = "wasm32", test)),
        allow(
            dead_code,
            reason = "Trusted Worker boundary constructs acknowledgements."
        )
    )]
    pub(crate) fn verified(operation_id: OperationId, account_id: AccountId) -> Self {
        Self {
            operation_id,
            account_id,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemovalPhase {
    Prepared,
    Committed,
    Aborting,
}
impl RemovalPhase {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Committed => "committed",
            Self::Aborting => "aborting",
        }
    }
}
#[derive(Clone, Debug)]
pub struct RemovalWork {
    pub operation_id: OperationId,
    pub actor: AuditActor,
    pub command_id: CommandId,
    pub target_account_id: AccountId,
    pub operation: ReceiptOperation,
    pub phase: RemovalPhase,
    pub created_at: i64,
    pub next_attempt_at: i64,
    pub attempt_count: u32,
}
impl RemovalWork {
    fn command(&self) -> Result<ManagementCommand, ManagementError> {
        match self.operation {
            ReceiptOperation::DisableAccount => Ok(ManagementCommand::DisableAccount {
                account_id: self.target_account_id,
            }),
            ReceiptOperation::DeleteAccount => Ok(ManagementCommand::DeleteAccount {
                account_id: self.target_account_id,
            }),
            _ => Err(ManagementError::Storage),
        }
    }
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    /// Read-only resolution after an await; never admits or retries a new mutation.
    #[cfg_attr(
        not(any(target_arch = "wasm32", test)),
        allow(dead_code, reason = "Trusted Worker recovery uses this lookup.")
    )]
    pub(crate) fn removal_resolution(
        &self,
        operation_id: OperationId,
        command: &ManagementCommand,
        principal: &ManagementPrincipal,
        command_id: CommandId,
    ) -> Result<ManagementResponse, ManagementError> {
        self.db.transaction(|| {
            Ok((|| {
                let actor = self.management_actor(principal)?;
                let account_id = target(command)?;
                if let Some(work) = self.removal_operation(operation_id)? {
                    if work.actor != actor
                        || work.command_id != command_id
                        || work.target_account_id != account_id
                        || receipts::fingerprint(&work.command()?)?
                            != receipts::fingerprint(command)?
                    {
                        return Err(ManagementError::Conflict);
                    }
                    return match self.pending_removal_response(actor, command_id, command)? {
                        Some(ManagementResponse::Pending {
                            operation_id: found,
                        }) if found == operation_id => {
                            Ok(ManagementResponse::Pending { operation_id })
                        }
                        _ => Err(ManagementError::Conflict),
                    };
                }
                let receipt = self
                    .management_receipt(actor, command_id, command, self.now()?)?
                    .ok_or(ManagementError::Conflict)?;
                let rows = self.db.query(
                    "SELECT operation_id FROM management_receipts WHERE actor=? AND command_id=?",
                    &[
                        SqlValue::Text(actor_key(actor)),
                        SqlValue::Text(command_id.to_string()),
                    ],
                )?;
                if receipt.account_id != account_id
                    || rows.len() != 1
                    || rows[0].len() != 1
                    || rows[0][0] != SqlValue::Text(operation_id.to_string())
                {
                    return Err(ManagementError::Conflict);
                }
                Ok(ManagementResponse::Committed { receipt })
            })())
        })?
    }

    pub fn removal_operation(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<RemovalWork>, ManagementError> {
        let rows = self.db.query(
            "SELECT operation_id,actor,command_id,target_account_id,operation,phase,created_at,next_attempt_at,attempt_count FROM pending_account_removals WHERE operation_id=?",
            &[SqlValue::Text(operation_id.to_string())],
        )?;
        rows.first().map(parse_work).transpose()
    }
    /// Apply credentials/receipt atomically, but return Pending until the release ACK.
    pub fn commit_removal(
        &self,
        operation_id: OperationId,
        principal: ManagementPrincipal,
        grant: &RemovalGateGrant,
    ) -> Result<ManagementResponse, ManagementError> {
        self.commit_removal_with_principal(operation_id, &principal, grant)
    }
    /// Borrow session authority so the Worker can reprove delivery after awaits.
    pub(crate) fn commit_removal_with_principal(
        &self,
        operation_id: OperationId,
        principal: &ManagementPrincipal,
        grant: &RemovalGateGrant,
    ) -> Result<ManagementResponse, ManagementError> {
        let work = self
            .removal_operation(operation_id)?
            .ok_or(ManagementError::NotFound)?;
        let command = work.command()?;
        self.management_attempt(principal, &command, |actor| {
            if actor != work.actor {
                return Err(ManagementError::Forbidden);
            }
            if grant.operation_id != operation_id || grant.account_id != work.target_account_id {
                return Err(ManagementError::Conflict);
            }
            let work = self
                .removal_operation(operation_id)?
                .ok_or(ManagementError::NotFound)?;
            if work.phase == RemovalPhase::Committed {
                return Ok(ManagementResponse::Pending { operation_id });
            }
            if work.phase != RemovalPhase::Prepared {
                return Err(ManagementError::Conflict);
            }
            let a = self.removal_target(work.target_account_id, actor)?;
            let now = self.now()?;
            if work.operation == ReceiptOperation::DisableAccount
                && a.disabled.is_none()
                && a.epoch >= JS_SAFE_INTEGER_MAX
            {
                return Err(ManagementError::Conflict);
            }
            self.invalidate_account_credentials(a.id, now)?;
            match work.operation {
                ReceiptOperation::DisableAccount if a.disabled.is_none() => {
                    let epoch = a
                        .epoch
                        .checked_add(1)
                        .filter(|v| *v <= JS_SAFE_INTEGER_MAX)
                        .ok_or(ManagementError::Conflict)?;
                    self.db.execute(
                        "UPDATE accounts SET disabled_at=?,credential_epoch=? WHERE account_id=? AND disabled_at IS NULL",
                        &[
                            SqlValue::Integer(now),
                            SqlValue::Integer(epoch),
                            SqlValue::Text(a.id.to_string()),
                        ],
                    )?;
                }
                ReceiptOperation::DisableAccount => {}
                ReceiptOperation::DeleteAccount => {
                    self.db.execute(
                        "DELETE FROM account_sessions WHERE account_id=?",
                        &[SqlValue::Text(a.id.to_string())],
                    )?;
                    self.db.execute(
                        "DELETE FROM access_links WHERE account_id=?",
                        &[SqlValue::Text(a.id.to_string())],
                    )?;
                    self.db.execute(
                        "DELETE FROM accounts WHERE account_id=?",
                        &[SqlValue::Text(a.id.to_string())],
                    )?;
                }
                _ => return Err(ManagementError::Storage),
            }
            let receipt = ManagementReceipt {
                version: 1,
                command_id: work.command_id,
                operation: work.operation,
                account_id: a.id,
                link_id: None,
                purpose: None,
                link_expires_at: None,
                completed_at: now,
                expires_at: deadline(now, crate::limits::COMMAND_RECEIPT_RETENTION_MS)?,
            };
            self.store_management_receipt(actor, &command, &receipt)?;
            self.db.execute(
                "UPDATE management_receipts SET operation_id=? WHERE actor=? AND command_id=?",
                &[
                    SqlValue::Text(operation_id.to_string()),
                    SqlValue::Text(actor_key(actor)),
                    SqlValue::Text(work.command_id.to_string()),
                ],
            )?;
            self.db.execute(
                "UPDATE pending_account_removals SET phase=? WHERE operation_id=?",
                &[
                    SqlValue::Text(RemovalPhase::Committed.as_str().into()),
                    SqlValue::Text(operation_id.to_string()),
                ],
            )?;
            self.management_audit(
                actor,
                work.operation,
                brews_contracts::management::AuditTarget::Account(a.id),
                AuditOutcome::Succeeded,
                now,
            )?;
            Ok(ManagementResponse::Pending { operation_id })
        })
    }
    pub fn finish_removal(
        &self,
        operation_id: OperationId,
        ack: &RemovalReleaseAck,
    ) -> Result<ManagementResponse, ManagementError> {
        self.management_transaction(|| {
            if ack.operation_id != operation_id {
                return Err(ManagementError::Conflict);
            }
            let receipt = self
                .removal_receipt(operation_id)?
                .ok_or(ManagementError::NotFound)?;
            if receipt.account_id != ack.account_id {
                return Err(ManagementError::Conflict);
            }
            if let Some(work) = self.removal_operation(operation_id)? {
                if receipt.command_id != work.command_id || receipt.operation != work.operation {
                    return Err(ManagementError::Storage);
                }
                if work.target_account_id != ack.account_id || work.phase != RemovalPhase::Committed
                {
                    return Err(ManagementError::Conflict);
                }
                if !self
                    .db
                    .query(
                        "SELECT connection_id FROM account_socket_close_work WHERE account_id=? LIMIT 1",
                        &[SqlValue::Text(work.target_account_id.to_string())],
                    )?
                    .is_empty()
                {
                    return Ok(ManagementResponse::Pending { operation_id });
                }
                self.db.execute(
                    "DELETE FROM pending_account_removals WHERE operation_id=?",
                    &[SqlValue::Text(operation_id.to_string())],
                )?;
            }
            Ok(ManagementResponse::Committed { receipt })
        })
    }
    pub(super) fn removal_receipt(
        &self,
        id: OperationId,
    ) -> Result<Option<ManagementReceipt>, ManagementError> {
        let rows = self.db.query(
            "SELECT outcome FROM management_receipts WHERE operation_id=?",
            &[SqlValue::Text(id.to_string())],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        if row.len() != 1 {
            return Err(ManagementError::Storage);
        }
        let payload = text(row, 0)?;
        if payload.len() > crate::limits::COMMAND_RECEIPT_MAX_BYTES {
            return Err(ManagementError::Storage);
        }
        let receipt: ManagementReceipt =
            serde_json::from_str(payload).map_err(|_| ManagementError::Storage)?;
        if receipt.version != 1
            || !matches!(
                receipt.operation,
                ReceiptOperation::DisableAccount | ReceiptOperation::DeleteAccount
            )
        {
            return Err(ManagementError::Storage);
        }
        receipts::validate_receipt(&receipt)?;
        Ok(Some(receipt))
    }
}
pub(super) fn parse_work(row: &crate::db::Row) -> Result<RemovalWork, ManagementError> {
    if row.len() != 9 {
        return Err(ManagementError::Storage);
    }
    let actor = match text(row, 1)? {
        "developer_cli" => AuditActor::DeveloperCli,
        id => AuditActor::Account(id.parse().map_err(|_| ManagementError::Storage)?),
    };
    let operation = match text(row, 4)? {
        "disable_account" => ReceiptOperation::DisableAccount,
        "delete_account" => ReceiptOperation::DeleteAccount,
        _ => return Err(ManagementError::Storage),
    };
    let phase_tag = text(row, 5)?;
    let phase = [
        RemovalPhase::Prepared,
        RemovalPhase::Committed,
        RemovalPhase::Aborting,
    ]
    .into_iter()
    .find(|phase| phase.as_str() == phase_tag)
    .ok_or(ManagementError::Storage)?;
    Ok(RemovalWork {
        operation_id: text(row, 0)?
            .parse()
            .map_err(|_| ManagementError::Storage)?,
        actor,
        command_id: text(row, 2)?
            .parse()
            .map_err(|_| ManagementError::Storage)?,
        target_account_id: text(row, 3)?
            .parse()
            .map_err(|_| ManagementError::Storage)?,
        operation,
        phase,
        created_at: integer(row, 6)?,
        next_attempt_at: integer(row, 7)?,
        attempt_count: u32::try_from(integer(row, 8)?).map_err(|_| ManagementError::Storage)?,
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "Test fixtures fail fast."
    )]
    use super::super::test_support::{Sqlite, TestRuntime};
    use super::super::*;
    use crate::{auth::AuthPolicy, db::migrate};
    use brews_domain::ids::OperationId;
    const ORIGIN: &str = "https://app.example.test";
    fn command_id(rt: &TestRuntime, n: u8) -> CommandId {
        let mut b = [0; 16];
        b[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
        b[6] = 0x70;
        b[8] = 0x80;
        b[15] = n;
        uuid::Uuid::from_bytes(b).try_into().unwrap()
    }
    fn seed_admin(db: &Sqlite, rt: &TestRuntime) -> AccountId {
        let id: AccountId = command_id(rt, 200).to_string().parse().unwrap();
        let password = crate::security::new_token(rt).unwrap();
        let phc = crate::security::hash_password(&password, AuthPolicy::default(), rt).unwrap();
        db.execute(
            "INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,'AdminPerson01','admin','verified',?,0,?,?,NULL)",
            &[
                SqlValue::Text(id.to_string()),
                SqlValue::Text(phc),
                SqlValue::Integer(rt.now.get()),
                SqlValue::Integer(rt.now.get()),
            ],
        )
        .unwrap();
        id
    }
    fn host(service: &AuthService<'_, Sqlite, TestRuntime>, rt: &TestRuntime) -> AccountId {
        match service
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: "HostPerson01".into(),
                    role: AccountRole::Host,
                },
                ManagementPrincipal::DeveloperCli,
                Some(command_id(rt, 1)),
                ORIGIN,
            )
            .unwrap()
        {
            ManagementResponse::Issued { receipt, .. } => receipt.account_id,
            _ => panic!("link missing"),
        }
    }
    fn pending(response: ManagementResponse) -> OperationId {
        match response {
            ManagementResponse::Pending { operation_id } => operation_id,
            _ => panic!("must remain pending"),
        }
    }
    #[test]
    fn committed_removal_phase_is_bound_and_persisted_in_real_sqlite() {
        use std::cell::RefCell;
        struct CapturingSqlite {
            sqlite: Sqlite,
            phase_statements: RefCell<Vec<(String, Vec<SqlValue>)>>,
        }
        impl Database for CapturingSqlite {
            fn query(
                &self,
                sql: &str,
                params: &[SqlValue],
            ) -> Result<Vec<crate::db::Row>, StorageError> {
                self.sqlite.query(sql, params)
            }
            fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
                if sql.starts_with("UPDATE pending_account_removals SET phase=") {
                    self.phase_statements
                        .borrow_mut()
                        .push((sql.into(), params.to_vec()));
                }
                self.sqlite.execute(sql, params)
            }
            fn transaction<T>(
                &self,
                operation: impl FnOnce() -> Result<T, StorageError>,
            ) -> Result<T, StorageError> {
                self.sqlite.transaction(operation)
            }
        }
        let sqlite = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&sqlite).unwrap();
        let target = {
            let service = AuthService::new(&sqlite, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
            host(&service, &rt)
        };
        let db = CapturingSqlite {
            sqlite,
            phase_statements: RefCell::new(Vec::new()),
        };
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let operation = pending(
            service
                .prepare_removal(
                    ManagementCommand::DisableAccount { account_id: target },
                    ManagementPrincipal::DeveloperCli,
                    command_id(&rt, 2),
                )
                .unwrap(),
        );
        let grant = RemovalGateGrant::verified(operation, target);
        for _ in 0..2 {
            assert_eq!(
                pending(
                    service
                        .commit_removal(operation, ManagementPrincipal::DeveloperCli, &grant)
                        .unwrap()
                ),
                operation
            );
        }
        assert_eq!(
            service.removal_operation(operation).unwrap().unwrap().phase,
            RemovalPhase::Committed
        );
        assert_eq!(
            service.begin_removal_abort(operation).unwrap_err(),
            ManagementError::Conflict
        );
        assert_eq!(
            db.sqlite
                .query("SELECT phase FROM pending_account_removals", &[])
                .unwrap(),
            vec![vec![SqlValue::Text("committed".into())]]
        );
        assert_eq!(
            db.sqlite
                .query(
                    "SELECT credential_epoch,disabled_at FROM accounts WHERE account_id=?",
                    &[SqlValue::Text(target.to_string())]
                )
                .unwrap(),
            vec![vec![SqlValue::Integer(1), SqlValue::Integer(rt.now.get())]]
        );
        let statements = db.phase_statements.borrow();
        assert_eq!(statements.len(), 1);
        assert!(!statements[0].0.contains("'committed'"));
        assert_eq!(
            statements[0].1,
            vec![
                SqlValue::Text("committed".into()),
                SqlValue::Text(operation.to_string())
            ]
        );
    }
    #[test]
    fn removal_resolution_is_read_only_and_bound_to_actor_command_target_and_operation() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let admin = seed_admin(&db, &rt);
        let token = admin_session(&db, &rt, admin);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let command = ManagementCommand::DisableAccount { account_id: target };
        let id = command_id(&rt, 2);
        let op = pending(
            service
                .prepare_removal(command.clone(), ManagementPrincipal::DeveloperCli, id)
                .unwrap(),
        );
        let resolve =
            || service.removal_resolution(op, &command, &ManagementPrincipal::DeveloperCli, id);
        assert_eq!(pending(resolve().unwrap()), op);
        service
            .commit_removal(
                op,
                ManagementPrincipal::DeveloperCli,
                &RemovalGateGrant::verified(op, target),
            )
            .unwrap();
        assert_eq!(pending(resolve().unwrap()), op);
        let committed = service
            .finish_removal(op, &RemovalReleaseAck::verified(op, target))
            .unwrap();
        let audits = db.query("SELECT count(*) FROM admin_audit", &[]).unwrap();
        assert_eq!(
            serde_json::to_value(resolve().unwrap()).unwrap(),
            serde_json::to_value(committed).unwrap()
        );
        assert_eq!(
            service
                .removal_resolution(op, &command, &ManagementPrincipal::AdminSession(token), id)
                .unwrap_err(),
            ManagementError::Conflict
        );
        assert_eq!(
            service
                .removal_resolution(
                    op,
                    &ManagementCommand::DeleteAccount { account_id: target },
                    &ManagementPrincipal::DeveloperCli,
                    id
                )
                .unwrap_err(),
            ManagementError::Conflict
        );
        assert_eq!(
            service
                .removal_resolution(
                    op,
                    &ManagementCommand::DisableAccount { account_id: admin },
                    &ManagementPrincipal::DeveloperCli,
                    id
                )
                .unwrap_err(),
            ManagementError::Conflict
        );
        assert_eq!(
            service
                .removal_resolution(
                    command_id(&rt, 9).to_string().parse().unwrap(),
                    &command,
                    &ManagementPrincipal::DeveloperCli,
                    id
                )
                .unwrap_err(),
            ManagementError::Conflict
        );
        assert_eq!(
            service
                .removal_resolution(
                    op,
                    &command,
                    &ManagementPrincipal::DeveloperCli,
                    command_id(&rt, 8)
                )
                .unwrap_err(),
            ManagementError::Conflict
        );
        assert_eq!(
            db.query("SELECT count(*) FROM admin_audit", &[]).unwrap(),
            audits
        );
        rt.now
            .set(rt.now.get() + crate::limits::COMMAND_RECEIPT_RETENTION_MS);
        assert_eq!(resolve().unwrap_err(), ManagementError::StaleCommand);
    }

    #[test]
    fn removal_resolution_rejects_changed_work_semantics_and_never_restarts_aborted_work() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let command = ManagementCommand::DisableAccount { account_id: target };
        let id = command_id(&rt, 2);
        let op = pending(
            service
                .prepare_removal(command.clone(), ManagementPrincipal::DeveloperCli, id)
                .unwrap(),
        );
        db.execute(
            "UPDATE pending_account_removals SET operation='delete_account' WHERE operation_id=?",
            &[SqlValue::Text(op.to_string())],
        )
        .unwrap();
        assert_eq!(
            service
                .removal_resolution(op, &command, &ManagementPrincipal::DeveloperCli, id)
                .unwrap_err(),
            ManagementError::Conflict
        );
        db.execute(
            "UPDATE pending_account_removals SET operation='disable_account' WHERE operation_id=?",
            &[SqlValue::Text(op.to_string())],
        )
        .unwrap();
        service.begin_removal_abort(op).unwrap();
        service
            .abort_removal(op, &RemovalReleaseAck::verified(op, target))
            .unwrap();
        let audits = db.query("SELECT count(*) FROM admin_audit", &[]).unwrap();
        assert_eq!(
            service
                .removal_resolution(op, &command, &ManagementPrincipal::DeveloperCli, id)
                .unwrap_err(),
            ManagementError::Conflict
        );
        assert!(service.removal_operation(op).unwrap().is_none());
        assert!(service.account(target).unwrap().unwrap().disabled.is_none());
        assert_eq!(
            db.query("SELECT count(*) FROM admin_audit", &[]).unwrap(),
            audits
        );
    }

    #[test]
    fn deletion_requires_matching_gate_and_retains_intent_until_release_ack() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let admin = seed_admin(&db, &rt);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let command = ManagementCommand::DeleteAccount { account_id: target };
        let id = command_id(&rt, 2);
        let op = pending(
            service
                .prepare_removal(command.clone(), ManagementPrincipal::DeveloperCli, id)
                .unwrap(),
        );
        assert_eq!(
            service
                .commit_removal(
                    op,
                    ManagementPrincipal::DeveloperCli,
                    &RemovalGateGrant::verified(op, admin)
                )
                .unwrap_err(),
            ManagementError::Conflict
        );
        let grant = RemovalGateGrant::verified(op, target);
        assert_eq!(
            pending(
                service
                    .commit_removal(op, ManagementPrincipal::DeveloperCli, &grant)
                    .unwrap()
            ),
            op
        );
        assert_eq!(
            pending(
                service
                    .commit_removal(op, ManagementPrincipal::DeveloperCli, &grant)
                    .unwrap()
            ),
            op
        );
        assert!(service.account(target).unwrap().is_none());
        assert_eq!(
            db.query("SELECT count(*) FROM pending_account_removals", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
        assert!(matches!(
            service
                .finish_removal(op, &RemovalReleaseAck::verified(op, target))
                .unwrap(),
            ManagementResponse::Committed { .. }
        ));
        assert_eq!(
            db.query("SELECT count(*) FROM pending_account_removals", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(0)]]
        );
        assert!(matches!(
            service
                .execute_management(command, ManagementPrincipal::DeveloperCli, Some(id), ORIGIN)
                .unwrap(),
            ManagementResponse::Committed { .. }
        ));
    }

    #[test]
    fn socket_close_acknowledgements_gate_removal_completion() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let _ = seed_admin(&db, &rt);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let connection = command_id(&rt, 9).to_string();
        db.execute(
            "INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) VALUES(?,?,?,?,0,?)",
            &[
                SqlValue::Text(target.to_string()),
                SqlValue::Text(command_id(&rt, 7).to_string()),
                SqlValue::Text(command_id(&rt, 8).to_string()),
                SqlValue::Text(connection.clone()),
                SqlValue::Integer(rt.now.get() + 86_400_000),
            ],
        )
        .unwrap();
        let op = pending(
            service
                .prepare_removal(
                    ManagementCommand::DeleteAccount { account_id: target },
                    ManagementPrincipal::DeveloperCli,
                    command_id(&rt, 2),
                )
                .unwrap(),
        );
        let _ = service
            .commit_removal(
                op,
                ManagementPrincipal::DeveloperCli,
                &RemovalGateGrant::verified(op, target),
            )
            .unwrap();
        let ack = RemovalReleaseAck::verified(op, target);
        assert_eq!(pending(service.finish_removal(op, &ack).unwrap()), op);
        assert!(service.removal_operation(op).unwrap().is_some());
        db.execute(
            "DELETE FROM account_socket_close_work WHERE connection_id=?",
            &[SqlValue::Text(connection.clone())],
        )
        .unwrap();
        db.execute(
            "DELETE FROM account_socket_subscriptions WHERE connection_id=?",
            &[SqlValue::Text(connection)],
        )
        .unwrap();
        assert!(matches!(
            service.finish_removal(op, &ack).unwrap(),
            ManagementResponse::Committed { .. }
        ));
    }

    #[test]
    fn durable_recovery_has_bounded_backoff_and_requires_ack_to_abort() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let _ = seed_admin(&db, &rt);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let op = pending(
            service
                .prepare_removal(
                    ManagementCommand::DisableAccount { account_id: target },
                    ManagementPrincipal::DeveloperCli,
                    command_id(&rt, 2),
                )
                .unwrap(),
        );
        assert!(service.removal_work(100).unwrap().is_empty());
        rt.now.set(rt.now.get() + 1000);
        assert_eq!(service.removal_work(100).unwrap().len(), 1);
        for attempt in 0..12 {
            service.retry_removal(op).unwrap();
            let work = service.removal_operation(op).unwrap().unwrap();
            assert_eq!(
                work.next_attempt_at - rt.now.get(),
                (1000i64 * (1i64 << attempt)).min(300_000)
            );
            assert_eq!(work.attempt_count, attempt + 1);
        }
        service.begin_removal_abort(op).unwrap();
        assert_eq!(
            service.removal_operation(op).unwrap().unwrap().phase,
            RemovalPhase::Aborting
        );
        let bad: OperationId = command_id(&rt, 99).to_string().parse().unwrap();
        assert_eq!(
            service
                .abort_removal(op, &RemovalReleaseAck::verified(bad, target))
                .unwrap_err(),
            ManagementError::Conflict
        );
        assert!(service.removal_operation(op).unwrap().is_some());
        service
            .abort_removal(op, &RemovalReleaseAck::verified(op, target))
            .unwrap();
        assert!(service.removal_operation(op).unwrap().is_none());
        assert!(service.account(target).unwrap().unwrap().disabled.is_none());
    }

    #[test]
    fn unresolved_removal_receipts_do_not_spin_the_retention_alarm() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let _ = seed_admin(&db, &rt);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let op = pending(
            service
                .prepare_removal(
                    ManagementCommand::DeleteAccount { account_id: target },
                    ManagementPrincipal::DeveloperCli,
                    command_id(&rt, 2),
                )
                .unwrap(),
        );
        let _ = service
            .commit_removal(
                op,
                ManagementPrincipal::DeveloperCli,
                &RemovalGateGrant::verified(op, target),
            )
            .unwrap();
        rt.now.set(rt.now.get() + 86_400_001);
        service.retry_removal(op).unwrap();
        service.cleanup_management().unwrap();
        assert_eq!(
            service.next_management_deadline().unwrap(),
            Some(rt.now.get() + 1000)
        );
        assert!(service.removal_receipt(op).unwrap().is_some());
    }

    #[test]
    fn deletion_does_not_require_an_unused_credential_epoch_increment() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let _ = seed_admin(&db, &rt);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        db.execute(
            "UPDATE accounts SET credential_epoch=? WHERE account_id=?",
            &[
                SqlValue::Integer(crate::limits::JS_SAFE_INTEGER_MAX),
                SqlValue::Text(target.to_string()),
            ],
        )
        .unwrap();
        let op = pending(
            service
                .prepare_removal(
                    ManagementCommand::DeleteAccount { account_id: target },
                    ManagementPrincipal::DeveloperCli,
                    command_id(&rt, 2),
                )
                .unwrap(),
        );
        let _ = service
            .commit_removal(
                op,
                ManagementPrincipal::DeveloperCli,
                &RemovalGateGrant::verified(op, target),
            )
            .unwrap();
        assert!(service.account(target).unwrap().is_none());
    }

    fn admin_session(db: &Sqlite, rt: &TestRuntime, admin: AccountId) -> String {
        let token = crate::security::new_token(rt).unwrap();
        let digest = crate::security::token_digest(&token).unwrap();
        db.execute(
            "INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at,revoked_at) VALUES(?,?,?,'normal',0,?,?,NULL)",
            &[
                SqlValue::Text(command_id(rt, 210).to_string()),
                SqlValue::Text(admin.to_string()),
                SqlValue::Blob(digest.to_vec()),
                SqlValue::Integer(rt.now.get()),
                SqlValue::Integer(rt.now.get() + 86_400_000),
            ],
        )
        .unwrap();
        token
    }
    #[test]
    fn live_actor_authority_is_revalidated_after_the_directory_round_trip() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let admin = seed_admin(&db, &rt);
        let token = admin_session(&db, &rt, admin);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let op = pending(
            service
                .prepare_removal(
                    ManagementCommand::DeleteAccount { account_id: target },
                    ManagementPrincipal::AdminSession(token.clone()),
                    command_id(&rt, 2),
                )
                .unwrap(),
        );
        db.execute(
            "UPDATE account_sessions SET revoked_at=? WHERE account_id=?",
            &[
                SqlValue::Integer(rt.now.get()),
                SqlValue::Text(admin.to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            service
                .commit_removal(
                    op,
                    ManagementPrincipal::AdminSession(token),
                    &RemovalGateGrant::verified(op, target)
                )
                .unwrap_err(),
            ManagementError::Unauthorized
        );
        assert!(service.account(target).unwrap().is_some());
        assert_eq!(
            service.removal_operation(op).unwrap().unwrap().phase,
            RemovalPhase::Prepared
        );
    }
    #[test]
    fn app_admin_self_disable_delete_and_enable_are_forbidden() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let admin = seed_admin(&db, &rt);
        let token = admin_session(&db, &rt, admin);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        for (n, command) in [
            (1, ManagementCommand::DisableAccount { account_id: admin }),
            (2, ManagementCommand::DeleteAccount { account_id: admin }),
            (3, ManagementCommand::EnableAccount { account_id: admin }),
        ] {
            assert_eq!(
                service
                    .execute_management(
                        command,
                        ManagementPrincipal::AdminSession(token.clone()),
                        Some(command_id(&rt, n)),
                        ORIGIN
                    )
                    .unwrap_err(),
                ManagementError::Forbidden
            );
        }
        assert!(service.account(admin).unwrap().unwrap().disabled.is_none());
    }
    #[test]
    fn developer_cli_cannot_remove_the_last_enabled_verified_admin() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let admin = seed_admin(&db, &rt);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        for (n, command) in [
            (1, ManagementCommand::DisableAccount { account_id: admin }),
            (2, ManagementCommand::DeleteAccount { account_id: admin }),
        ] {
            assert_eq!(
                service
                    .prepare_removal(
                        command,
                        ManagementPrincipal::DeveloperCli,
                        command_id(&rt, n)
                    )
                    .unwrap_err(),
                ManagementError::LastAdmin
            );
        }
        assert!(service.account(admin).unwrap().unwrap().disabled.is_none());
    }
    #[test]
    fn last_admin_is_checked_again_when_the_gate_grant_arrives() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let first = seed_admin(&db, &rt);
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let second = host(&service, &rt);
        db.execute(
            "UPDATE accounts SET role='admin',status='verified',verifier=(SELECT verifier FROM accounts WHERE account_id=?) WHERE account_id=?",
            &[
                SqlValue::Text(first.to_string()),
                SqlValue::Text(second.to_string()),
            ],
        )
        .unwrap();
        let op = pending(
            service
                .prepare_removal(
                    ManagementCommand::DisableAccount { account_id: second },
                    ManagementPrincipal::DeveloperCli,
                    command_id(&rt, 2),
                )
                .unwrap(),
        );
        db.execute(
            "UPDATE accounts SET disabled_at=? WHERE account_id=?",
            &[
                SqlValue::Integer(rt.now.get()),
                SqlValue::Text(first.to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            service
                .commit_removal(
                    op,
                    ManagementPrincipal::DeveloperCli,
                    &RemovalGateGrant::verified(op, second)
                )
                .unwrap_err(),
            ManagementError::LastAdmin
        );
        assert!(service.account(second).unwrap().unwrap().disabled.is_none());
    }
    #[test]
    fn disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let target = host(&service, &rt);
        let first = rt.now.get();
        for n in [2, 3] {
            let op = pending(
                service
                    .prepare_removal(
                        ManagementCommand::DisableAccount { account_id: target },
                        ManagementPrincipal::DeveloperCli,
                        command_id(&rt, n),
                    )
                    .unwrap(),
            );
            let _ = service
                .commit_removal(
                    op,
                    ManagementPrincipal::DeveloperCli,
                    &RemovalGateGrant::verified(op, target),
                )
                .unwrap();
            let _ = service
                .finish_removal(op, &RemovalReleaseAck::verified(op, target))
                .unwrap();
            rt.now.set(rt.now.get() + 1000);
        }
        let a = service.account(target).unwrap().unwrap();
        assert_eq!(a.epoch, 1);
        assert_eq!(a.disabled, Some(first));
        assert_eq!(
            a.status,
            brews_domain::accounts::AccountStatus::PendingEnrollment
        );
    }
    #[test]
    fn pending_and_committed_removal_survive_real_sqlite_reopen_and_expired_cleanup() {
        use std::{cell::RefCell, fs::OpenOptions, os::unix::fs::OpenOptionsExt};
        struct FileGuard(std::path::PathBuf);
        impl Drop for FileGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let rt = TestRuntime::new();
        let filename = format!(
            "brews-management-{}-{}.sqlite",
            std::process::id(),
            command_id(&rt, 221)
        );
        let file = FileGuard(std::env::temp_dir().join(filename));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file.0)
            .unwrap();
        let open = || {
            let conn = rusqlite::Connection::open(&file.0).unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
            Sqlite {
                conn: RefCell::new(conn),
            }
        };
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let id = command_id(&rt, 2);
        let (target, op) = {
            let db = open();
            migrate(&db).unwrap();
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
            let target = host(&service, &rt);
            let op = pending(
                service
                    .prepare_removal(
                        ManagementCommand::DeleteAccount { account_id: target },
                        ManagementPrincipal::DeveloperCli,
                        id,
                    )
                    .unwrap(),
            );
            (target, op)
        };
        {
            let db = open();
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
            assert_eq!(
                service.removal_operation(op).unwrap().unwrap().phase,
                RemovalPhase::Prepared
            );
            let _ = service
                .commit_removal(
                    op,
                    ManagementPrincipal::DeveloperCli,
                    &RemovalGateGrant::verified(op, target),
                )
                .unwrap();
        }
        {
            let db = open();
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
            assert_eq!(
                pending(
                    service
                        .execute_management(
                            ManagementCommand::DeleteAccount { account_id: target },
                            ManagementPrincipal::DeveloperCli,
                            Some(id),
                            ORIGIN
                        )
                        .unwrap()
                ),
                op
            );
            let _ = service
                .finish_removal(op, &RemovalReleaseAck::verified(op, target))
                .unwrap();
        }
        {
            let db = open();
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
            assert!(matches!(
                service
                    .execute_management(
                        ManagementCommand::DeleteAccount { account_id: target },
                        ManagementPrincipal::DeveloperCli,
                        Some(id),
                        ORIGIN
                    )
                    .unwrap(),
                ManagementResponse::Committed { .. }
            ));
            rt.now.set(rt.now.get() + 86_400_000);
            service.cleanup_management().unwrap();
            assert_eq!(
                service
                    .execute_management(
                        ManagementCommand::DeleteAccount { account_id: target },
                        ManagementPrincipal::DeveloperCli,
                        Some(id),
                        ORIGIN
                    )
                    .unwrap_err(),
                ManagementError::StaleCommand
            );
        }
    }
}
