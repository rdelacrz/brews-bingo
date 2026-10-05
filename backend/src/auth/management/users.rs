//! Public Users projections share the audited management transaction.
use super::{ManagementError, ManagementPrincipal};
use crate::{
    auth::{AuthService, Runtime},
    db::Database,
};
use brews_contracts::{
    management::{ManagementCommand, ManagementResponse},
    users::UserResponse,
};
use brews_domain::ids::CommandId;

impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    /// Reprove live authority after the transport's final await; never mutates state.
    #[cfg_attr(
        not(any(target_arch = "wasm32", test)),
        allow(dead_code, reason = "The Worker calls this final delivery guard.")
    )]
    pub(crate) fn authorize_users_release(
        &self,
        principal: &ManagementPrincipal,
    ) -> Result<(), ManagementError> {
        self.db.transaction(|| {
            Ok((|| {
                if !matches!(principal, ManagementPrincipal::AdminSession(_)) {
                    return Err(ManagementError::Forbidden);
                }
                self.management_actor(principal).map(|_| ())
            })())
        })?
    }
    /// Project genuine removal work/completion under one live authorized read.
    #[cfg_attr(
        not(any(target_arch = "wasm32", test)),
        allow(dead_code, reason = "The Worker projects coordinated Users removals.")
    )]
    pub(crate) fn users_removal_response(
        &self,
        command: &ManagementCommand,
        principal: &ManagementPrincipal,
        response: ManagementResponse,
    ) -> Result<UserResponse, ManagementError> {
        use crate::db::SqlValue;
        use brews_contracts::management::ReceiptOperation;
        let projection = || {
            if !matches!(principal, ManagementPrincipal::AdminSession(_)) {
                return Err(ManagementError::Forbidden);
            }
            let actor = self.management_actor(principal)?;
            let (account_id, operation) = match command {
                ManagementCommand::DisableAccount { account_id } => {
                    (*account_id, ReceiptOperation::DisableAccount)
                }
                ManagementCommand::DeleteAccount { account_id } => {
                    (*account_id, ReceiptOperation::DeleteAccount)
                }
                _ => return Err(ManagementError::InvalidInput),
            };
            match response {
                ManagementResponse::Pending { operation_id } => {
                    let work = self
                        .removal_operation(operation_id)?
                        .ok_or(ManagementError::Conflict)?;
                    if work.actor != actor
                        || work.target_account_id != account_id
                        || work.operation != operation
                    {
                        return Err(ManagementError::Conflict);
                    }
                    match self.pending_removal_response(actor, work.command_id, command)? {
                        Some(ManagementResponse::Pending {
                            operation_id: found,
                        }) if found == operation_id => Ok(UserResponse::Pending { operation_id }),
                        _ => Err(ManagementError::Conflict),
                    }
                }
                ManagementResponse::Committed { receipt } => {
                    if receipt.account_id != account_id || receipt.operation != operation {
                        return Err(ManagementError::Conflict);
                    }
                    let stored = self
                        .management_receipt(actor, receipt.command_id, command, self.now()?)?
                        .ok_or(ManagementError::Conflict)?;
                    if serde_json::to_vec(&stored).map_err(|_| ManagementError::Storage)?
                        != serde_json::to_vec(&receipt).map_err(|_| ManagementError::Storage)?
                    {
                        return Err(ManagementError::Conflict);
                    }
                    let rows = self.db.query(
                        "SELECT operation_id FROM management_receipts WHERE actor=? AND command_id=?",
                        &[
                            SqlValue::Text(super::actor_key(actor)),
                            SqlValue::Text(receipt.command_id.to_string()),
                        ],
                    )?;
                    let operation_id: brews_domain::ids::OperationId = match rows.as_slice() {
                        [row] => match row.as_slice() {
                            [SqlValue::Text(id)] => {
                                id.parse().map_err(|_| ManagementError::Storage)?
                            }
                            _ => return Err(ManagementError::Conflict),
                        },
                        _ => return Err(ManagementError::Conflict),
                    };
                    if let Some(work) = self.removal_operation(operation_id)? {
                        if work.actor != actor
                            || work.command_id != receipt.command_id
                            || work.target_account_id != account_id
                            || work.operation != operation
                        {
                            return Err(ManagementError::Conflict);
                        }
                        return Ok(UserResponse::Pending { operation_id });
                    }
                    match operation {
                        ReceiptOperation::DisableAccount => {
                            let account =
                                self.account(account_id)?.ok_or(ManagementError::NotFound)?;
                            Ok(UserResponse::Disabled {
                                account_id,
                                status: account.status,
                                disabled: account.disabled.is_some(),
                                receipt,
                            })
                        }
                        ReceiptOperation::DeleteAccount => {
                            if self.account(account_id)?.is_some() {
                                return Err(ManagementError::Conflict);
                            }
                            Ok(UserResponse::Deleted {
                                account_id,
                                deleted: true,
                                receipt,
                            })
                        }
                        _ => Err(ManagementError::InvalidInput),
                    }
                }
                _ => Err(ManagementError::InvalidInput),
            }
        };
        self.db.transaction(|| Ok(projection()))?
    }

    /// Accept only server-derived administrator sessions, never CLI authority.
    pub fn execute_users(
        &self,
        command: ManagementCommand,
        principal: &ManagementPrincipal,
        command_id: Option<CommandId>,
        canonical_origin: &str,
    ) -> Result<UserResponse, ManagementError> {
        if !matches!(principal, ManagementPrincipal::AdminSession(_)) {
            return Err(ManagementError::Forbidden);
        }
        self.management_attempt(principal, &command, |actor| {
            if matches!(command, ManagementCommand::ListAudit { .. }) {
                return Err(ManagementError::Forbidden);
            }
            if command.is_mutating() {
                let id = command_id.ok_or(ManagementError::InvalidInput)?;
                if let Some(ManagementResponse::Pending { operation_id }) =
                    self.pending_removal_response(actor, id, &command)?
                {
                    return Ok(UserResponse::Pending { operation_id });
                }
                if let Some(receipt) = self.management_receipt(actor, id, &command, self.now()?)? {
                    return Ok(UserResponse::Committed { receipt });
                }
            }
            match self.execute_management_checked(&command, actor, command_id, canonical_origin)? {
                ManagementResponse::Issued { receipt, url }
                    if matches!(command, ManagementCommand::CreateAccount { .. }) =>
                {
                    let account = super::reads::safe(
                        self.account(receipt.account_id)?
                            .ok_or(ManagementError::Storage)?,
                    );
                    let link_expires_at =
                        receipt.link_expires_at.ok_or(ManagementError::Storage)?;
                    Ok(UserResponse::Created {
                        account,
                        enrollment_url: url,
                        link_expires_at,
                        receipt,
                    })
                }
                ManagementResponse::Issued { receipt, url } => {
                    let link_expires_at =
                        receipt.link_expires_at.ok_or(ManagementError::Storage)?;
                    match &command {
                        ManagementCommand::ReissueEnrollment { account_id }
                            if *account_id == receipt.account_id =>
                        {
                            Ok(UserResponse::EnrollmentLink {
                                account_id: *account_id,
                                enrollment_url: url,
                                link_expires_at,
                                receipt,
                            })
                        }
                        ManagementCommand::ResetPassword { account_id }
                            if *account_id == receipt.account_id =>
                        {
                            let account =
                                self.account(*account_id)?.ok_or(ManagementError::Storage)?;
                            Ok(UserResponse::PasswordReset {
                                account_id: *account_id,
                                status: account.status,
                                reset_url: url,
                                link_expires_at,
                                receipt,
                            })
                        }
                        _ => Err(ManagementError::Storage),
                    }
                }
                ManagementResponse::Committed { receipt }
                    if matches!(command, ManagementCommand::EnableAccount { .. }) =>
                {
                    let account = self
                        .account(receipt.account_id)?
                        .ok_or(ManagementError::Storage)?;
                    Ok(UserResponse::Enabled {
                        account_id: account.id,
                        status: account.status,
                        disabled: account.disabled.is_some(),
                        receipt,
                    })
                }
                ManagementResponse::Accounts {
                    accounts,
                    next_cursor,
                } => Ok(UserResponse::Users {
                    users: accounts,
                    next_cursor,
                }),
                ManagementResponse::Account { account } => Ok(UserResponse::Account { account }),
                ManagementResponse::Pending { operation_id } => {
                    Ok(UserResponse::Pending { operation_id })
                }
                _ => Err(ManagementError::Storage),
            }
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Real SQLite fixtures use fixed failure diagnostics."
)]
mod tests {
    use super::super::test_support::{Sqlite, TestRuntime};
    use super::*;
    use crate::{
        auth::AuthPolicy,
        db::{SqlValue, migrate},
        security::{hash_password, new_token, token_digest},
    };
    use brews_domain::{accounts::AccountRole, ids::AccountId};
    const ORIGIN: &str = "https://app.example.test";
    fn id(rt: &TestRuntime, n: u8) -> CommandId {
        let mut bytes = [0; 16];
        bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        bytes[15] = n;
        uuid::Uuid::from_bytes(bytes).try_into().unwrap()
    }
    fn fixture(db: &Sqlite, rt: &TestRuntime) -> (AccountId, ManagementPrincipal) {
        migrate(db).unwrap();
        let service = AuthService::new(db, rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let ManagementResponse::Issued { receipt, .. } = service
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: "Administrator01".into(),
                    role: AccountRole::Admin,
                },
                ManagementPrincipal::DeveloperCli,
                Some(id(rt, 200)),
                ORIGIN,
            )
            .unwrap()
        else {
            panic!("admin fixture missing")
        };
        let password = new_token(rt).unwrap();
        let phc = hash_password(&password, AuthPolicy::default(), rt).unwrap();
        db.execute(
            "UPDATE accounts SET status='verified',verifier=? WHERE account_id=?",
            &[
                SqlValue::Text(phc),
                SqlValue::Text(receipt.account_id.to_string()),
            ],
        )
        .unwrap();
        let token = new_token(rt).unwrap();
        db.execute("INSERT INTO account_sessions(session_id,account_id,scope,token_verifier,credential_epoch,issued_at,expires_at,revoked_at) VALUES(?,?,'normal',?,0,?,?,NULL)",&[SqlValue::Text(id(rt,201).to_string()),SqlValue::Text(receipt.account_id.to_string()),SqlValue::Blob(token_digest(&token).unwrap().to_vec()),SqlValue::Integer(rt.now.get()),SqlValue::Integer(rt.now.get()+86_400_000)]).unwrap();
        (receipt.account_id, ManagementPrincipal::AdminSession(token))
    }
    fn changes(db: &Sqlite) -> Vec<crate::db::Row> {
        db.query("SELECT total_changes()", &[]).unwrap()
    }

    #[test]
    fn users_removal_projects_authorized_bound_completion_without_mutating_resolution() {
        use super::super::{RemovalGateGrant, RemovalReleaseAck};
        for delete in [false, true] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            let (_, principal) = fixture(&db, &rt);
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
            let UserResponse::Created { account, .. } = service
                .execute_users(
                    ManagementCommand::CreateAccount {
                        username: "RemovalTarget01".into(),
                        role: AccountRole::Host,
                    },
                    &principal,
                    Some(id(&rt, 1)),
                    ORIGIN,
                )
                .unwrap()
            else {
                panic!("target missing")
            };
            let command = if delete {
                ManagementCommand::DeleteAccount {
                    account_id: account.account_id,
                }
            } else {
                ManagementCommand::DisableAccount {
                    account_id: account.account_id,
                }
            };
            let UserResponse::Pending { operation_id } = service
                .execute_users(command.clone(), &principal, Some(id(&rt, 2)), ORIGIN)
                .unwrap()
            else {
                panic!("coordination missing")
            };
            assert!(
                matches!(service.execute_users(command.clone(),&principal,Some(id(&rt,2)),ORIGIN).unwrap(),UserResponse::Pending {operation_id:found} if found==operation_id)
            );
            let before = changes(&db);
            assert!(
                matches!(service.users_removal_response(&command,&principal,ManagementResponse::Pending {operation_id}).unwrap(),UserResponse::Pending {operation_id:found} if found==operation_id)
            );
            assert_eq!(changes(&db), before);
            let wrong = ManagementCommand::DisableAccount {
                account_id: id(&rt, 99).to_string().parse().unwrap(),
            };
            assert_eq!(
                service
                    .users_removal_response(
                        &wrong,
                        &principal,
                        ManagementResponse::Pending { operation_id }
                    )
                    .unwrap_err(),
                ManagementError::Conflict
            );
            assert!(matches!(
                service
                    .commit_removal_with_principal(
                        operation_id,
                        &principal,
                        &RemovalGateGrant::verified(operation_id, account.account_id)
                    )
                    .unwrap(),
                ManagementResponse::Pending { .. }
            ));
            let response = service
                .finish_removal(
                    operation_id,
                    &RemovalReleaseAck::verified(operation_id, account.account_id),
                )
                .unwrap();
            let before = changes(&db);
            let result = service
                .users_removal_response(&command, &principal, response.clone())
                .unwrap();
            assert_eq!(changes(&db), before);
            if delete {
                assert!(
                    matches!(result,UserResponse::Deleted {account_id,deleted:true,..} if account_id==account.account_id)
                );
            } else {
                assert!(
                    matches!(result,UserResponse::Disabled {account_id,status:brews_domain::accounts::AccountStatus::PendingEnrollment,disabled:true,..} if account_id==account.account_id)
                );
            }
            assert!(matches!(
                service
                    .execute_users(command.clone(), &principal, Some(id(&rt, 2)), ORIGIN)
                    .unwrap(),
                UserResponse::Committed { .. }
            ));
            assert_eq!(
                service
                    .users_removal_response(
                        &command,
                        &ManagementPrincipal::DeveloperCli,
                        response.clone()
                    )
                    .unwrap_err(),
                ManagementError::Forbidden
            );
            assert_eq!(
                service
                    .users_removal_response(&wrong, &principal, response.clone())
                    .unwrap_err(),
                ManagementError::Conflict
            );
            db.execute("UPDATE account_sessions SET revoked_at=issued_at", &[])
                .unwrap();
            let before = changes(&db);
            assert_eq!(
                service
                    .users_removal_response(&command, &principal, response)
                    .unwrap_err(),
                ManagementError::Unauthorized
            );
            assert_eq!(changes(&db), before);
        }
    }

    #[test]
    fn self_reset_commits_but_final_release_does_not_revive_the_retired_session() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        let (admin, principal) = fixture(&db, &rt);
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        assert!(
            matches!(service.execute_users(ManagementCommand::ResetPassword {account_id:admin},&principal,Some(id(&rt,1)),ORIGIN).unwrap(),UserResponse::PasswordReset {account_id,..} if account_id==admin)
        );
        let before = changes(&db);
        assert_eq!(
            service.authorize_users_release(&principal),
            Err(ManagementError::Unauthorized)
        );
        assert_eq!(changes(&db), before);
        assert!(
            service
                .management_receipt(
                    brews_contracts::management::AuditActor::Account(admin),
                    id(&rt, 1),
                    &ManagementCommand::ResetPassword { account_id: admin },
                    rt.now.get()
                )
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn session_principal_zeroizes_its_owned_token_and_redacts_debug() {
        use zeroize::{Zeroize, ZeroizeOnDrop};
        fn require_drop<T: ZeroizeOnDrop>(_: &T) {}
        let rt = TestRuntime::new();
        let token = new_token(&rt).unwrap();
        let mut principal = ManagementPrincipal::AdminSession(token.clone());
        require_drop(&principal);
        assert!(!format!("{principal:?}").contains(&token));
        principal.zeroize();
        assert!(matches!(&principal,ManagementPrincipal::AdminSession(token) if token.is_empty()));
    }

    #[test]
    fn final_release_reproves_live_admin_without_writes_or_audit() {
        for (sql, error) in [
            (
                "UPDATE account_sessions SET revoked_at=issued_at",
                ManagementError::Unauthorized,
            ),
            (
                "UPDATE accounts SET credential_epoch=credential_epoch+1",
                ManagementError::Unauthorized,
            ),
            (
                "UPDATE accounts SET disabled_at=created_at",
                ManagementError::Unauthorized,
            ),
            (
                "UPDATE accounts SET role='host'",
                ManagementError::Forbidden,
            ),
            (
                "UPDATE account_sessions SET expires_at=issued_at+1",
                ManagementError::Unauthorized,
            ),
        ] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            let (_, principal) = fixture(&db, &rt);
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
            let before = changes(&db);
            assert!(service.authorize_users_release(&principal).is_ok());
            assert_eq!(changes(&db), before);
            db.execute(sql, &[]).unwrap();
            rt.now.set(rt.now.get() + 1);
            let before = changes(&db);
            assert_eq!(service.authorize_users_release(&principal), Err(error));
            assert_eq!(changes(&db), before);
            assert_eq!(
                service.authorize_users_release(&ManagementPrincipal::DeveloperCli),
                Err(ManagementError::Forbidden)
            );
            assert_eq!(
                service.authorize_users_release(&ManagementPrincipal::AdminSession(String::new())),
                Err(ManagementError::Unauthorized)
            );
        }
    }
}
