#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real SQLite fixtures fail without exposing credentials."
)]
mod support;
use brews_backend::{
    auth::{AuthPolicy, AuthService, ManagementError, ManagementPrincipal, Runtime},
    db::{Database, SqlValue, migrate},
    security::{hash_password, new_token, token_digest},
};
use brews_contracts::{management::ManagementCommand, users::UserResponse};
use brews_domain::{
    accounts::{AccountRole, AccountStatus, SessionScope},
    ids::{AccountId, CommandId},
};
use support::{Sqlite, TestRuntime};
const ORIGIN: &str = "https://app.example.test";

fn command_id(rt: &TestRuntime, n: u8) -> CommandId {
    let mut bytes = [0; 16];
    bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes).try_into().unwrap()
}
struct Fixture {
    db: Sqlite,
    rt: TestRuntime,
    key: [u8; 32],
    admin: AccountId,
    principal: ManagementPrincipal,
}
impl Fixture {
    fn new() -> Self {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let mut key = [0; 32];
        rt.fill_random(&mut key).unwrap();
        let admin = seed(&db, &rt, 200, AccountRole::Admin, AccountStatus::Verified);
        let token = session(&db, &rt, admin, 201, SessionScope::Normal);
        Self {
            db,
            rt,
            key,
            admin,
            principal: ManagementPrincipal::AdminSession(token),
        }
    }
    fn service(&self) -> AuthService<'_, Sqlite, TestRuntime> {
        AuthService::new(&self.db, &self.rt, AuthPolicy::default(), &self.key).unwrap()
    }
    fn run(
        &self,
        command: ManagementCommand,
        n: Option<u8>,
    ) -> Result<UserResponse, ManagementError> {
        self.service().execute_users(
            command,
            &self.principal,
            n.map(|n| command_id(&self.rt, n)),
            ORIGIN,
        )
    }
}
fn seed(
    db: &Sqlite,
    rt: &TestRuntime,
    n: u8,
    role: AccountRole,
    status: AccountStatus,
) -> AccountId {
    let id = command_id(rt, n).to_string().parse().unwrap();
    let password = new_token(rt).unwrap();
    let verifier = if status == AccountStatus::PendingEnrollment {
        SqlValue::Null
    } else {
        SqlValue::Text(hash_password(&password, AuthPolicy::default(), rt).unwrap())
    };
    db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?,?,?,?,0,?,NULL,NULL)", &[
        SqlValue::Text(format!("{id}")), SqlValue::Text(format!("AccountPerson{n:03}")), SqlValue::Text(role.to_string()), SqlValue::Text(status.to_string()), verifier, SqlValue::Integer(rt.now.get()),
    ]).unwrap();
    id
}
fn session(
    db: &Sqlite,
    rt: &TestRuntime,
    account: AccountId,
    n: u8,
    scope: SessionScope,
) -> String {
    let token = new_token(rt).unwrap();
    db.execute("INSERT INTO account_sessions(session_id,account_id,scope,token_verifier,credential_epoch,issued_at,expires_at,revoked_at) VALUES(?,?,?,?,0,?,?,NULL)", &[
        SqlValue::Text(command_id(rt,n).to_string()), SqlValue::Text(account.to_string()), SqlValue::Text(scope.to_string()), SqlValue::Blob(token_digest(&token).unwrap().to_vec()), SqlValue::Integer(rt.now.get()), SqlValue::Integer(rt.now.get()+86_400_000),
    ]).unwrap();
    token
}
fn list() -> ManagementCommand {
    ManagementCommand::ListAccounts {
        after: None,
        limit: 50,
    }
}
fn create(role: AccountRole) -> ManagementCommand {
    ManagementCommand::CreateAccount {
        username: " \tCreatedPerson01\r".into(),
        role,
    }
}
fn count(db: &Sqlite, table: &str) -> i64 {
    match db
        .query(&format!("SELECT count(*) FROM {table}"), &[])
        .unwrap()[0][0]
    {
        SqlValue::Integer(n) => n,
        _ => panic!("count shape"),
    }
}

#[test]
fn users_enable_first_result_projects_lifecycle_but_replay_never_reads_target() {
    let f = Fixture::new();
    let target = seed(
        &f.db,
        &f.rt,
        10,
        AccountRole::Host,
        AccountStatus::ResetRequired,
    );
    f.db.execute(
        "UPDATE accounts SET disabled_at=? WHERE account_id=?",
        &[
            SqlValue::Integer(f.rt.now.get()),
            SqlValue::Text(target.to_string()),
        ],
    )
    .unwrap();
    let command = ManagementCommand::EnableAccount { account_id: target };
    assert!(
        matches!(f.run(command.clone(),Some(1)).unwrap(),UserResponse::Enabled { account_id,status:AccountStatus::ResetRequired,disabled:false,receipt } if account_id==target && receipt.account_id==target)
    );
    f.db.execute(
        "DELETE FROM accounts WHERE account_id=?",
        &[SqlValue::Text(target.to_string())],
    )
    .unwrap();
    assert!(
        matches!(f.run(command,Some(1)).unwrap(),UserResponse::Committed { receipt } if receipt.account_id==target)
    );
    assert_eq!(count(&f.db, "admin_audit"), 1);
}

#[test]
fn users_link_replacement_returns_only_the_first_enrollment_or_reset_url() {
    let f = Fixture::new();
    let pending = seed(
        &f.db,
        &f.rt,
        10,
        AccountRole::Host,
        AccountStatus::PendingEnrollment,
    );
    let verified = seed(&f.db, &f.rt, 11, AccountRole::Host, AccountStatus::Verified);
    let reissue = ManagementCommand::ReissueEnrollment {
        account_id: pending,
    };
    let reset = ManagementCommand::ResetPassword {
        account_id: verified,
    };
    match f.run(reissue.clone(), Some(1)).unwrap() {
        UserResponse::EnrollmentLink {
            account_id,
            enrollment_url,
            link_expires_at,
            receipt,
        } => {
            assert_eq!(account_id, pending);
            assert_eq!(Some(link_expires_at), receipt.link_expires_at);
            assert!(enrollment_url.starts_with(&format!("{ORIGIN}/enroll#")));
        }
        _ => panic!("enrollment handoff missing"),
    }
    match f.run(reset.clone(), Some(2)).unwrap() {
        UserResponse::PasswordReset {
            account_id,
            status,
            reset_url,
            link_expires_at,
            receipt,
        } => {
            assert_eq!(account_id, verified);
            assert_eq!(status, AccountStatus::ResetRequired);
            assert_eq!(Some(link_expires_at), receipt.link_expires_at);
            assert!(reset_url.starts_with(&format!("{ORIGIN}/password-reset#")));
        }
        _ => panic!("reset handoff missing"),
    }
    for (command, n, target) in [(reissue, 1, pending), (reset, 2, verified)] {
        f.db.execute(
            "DELETE FROM access_links WHERE account_id=?",
            &[SqlValue::Text(target.to_string())],
        )
        .unwrap();
        f.db.execute(
            "DELETE FROM accounts WHERE account_id=?",
            &[SqlValue::Text(target.to_string())],
        )
        .unwrap();
        assert!(
            matches!(f.run(command,Some(n)).unwrap(), UserResponse::Committed { receipt } if receipt.account_id==target)
        );
    }
    assert_eq!(count(&f.db, "admin_audit"), 2);
}

#[test]
fn users_creation_projects_committed_metadata_once_for_both_roles() {
    for role in [AccountRole::Host, AccountRole::Admin] {
        let f = Fixture::new();
        let response = f.run(create(role), Some(1)).unwrap();
        let UserResponse::Created {
            account,
            enrollment_url,
            link_expires_at,
            receipt,
        } = response
        else {
            panic!("fresh creation projection missing")
        };
        assert_eq!(account.username, "CreatedPerson01");
        assert_eq!(account.role, role);
        assert_eq!(account.status, AccountStatus::PendingEnrollment);
        assert!(!account.disabled);
        assert_eq!(account.account_id, receipt.account_id);
        assert_eq!(account.created_at, receipt.completed_at);
        assert_eq!(Some(link_expires_at), receipt.link_expires_at);
        assert!(enrollment_url.starts_with(&format!("{ORIGIN}/enroll#")));
        assert_eq!(count(&f.db, "admin_audit"), 1);
        assert_eq!(count(&f.db, "management_receipts"), 1);
        let persisted =
            f.db.query("SELECT outcome FROM management_receipts", &[])
                .unwrap();
        let token = enrollment_url.split('#').nth(1).unwrap();
        assert!(
            persisted
                .iter()
                .flatten()
                .all(|v| !matches!(v,SqlValue::Text(s) if s.contains(token)))
        );
        f.db.execute(
            "DELETE FROM access_links WHERE account_id=?",
            &[SqlValue::Text(account.account_id.to_string())],
        )
        .unwrap();
        f.db.execute(
            "DELETE FROM accounts WHERE account_id=?",
            &[SqlValue::Text(account.account_id.to_string())],
        )
        .unwrap();
        assert!(
            matches!(f.run(create(role),Some(1)).unwrap(),UserResponse::Committed { receipt: replay } if replay.account_id==account.account_id)
        );
        assert_eq!(count(&f.db, "admin_audit"), 1);
    }
}

struct TrackingDb<'a> {
    sqlite: &'a Sqlite,
    admin: AccountId,
    target_reads: std::cell::Cell<usize>,
    transactions: std::cell::Cell<usize>,
}
impl Database for TrackingDb<'_> {
    fn query(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<brews_backend::db::Row>, brews_backend::db::StorageError> {
        if sql.contains("FROM accounts WHERE account_id=?")
            && params.first() != Some(&SqlValue::Text(self.admin.to_string()))
        {
            assert!(
                !self.sqlite.conn.borrow().is_autocommit(),
                "target projection escaped its transaction"
            );
            self.target_reads.set(self.target_reads.get() + 1);
        }
        self.sqlite.query(sql, params)
    }
    fn execute(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<(), brews_backend::db::StorageError> {
        self.sqlite.execute(sql, params)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, brews_backend::db::StorageError>,
    ) -> Result<T, brews_backend::db::StorageError> {
        self.transactions.set(self.transactions.get() + 1);
        self.sqlite.transaction(operation)
    }
}

#[test]
fn users_new_metadata_is_in_one_transaction_and_replays_have_zero_target_reads() {
    let f = Fixture::new();
    let verified = seed(&f.db, &f.rt, 10, AccountRole::Host, AccountStatus::Verified);
    let db = TrackingDb {
        sqlite: &f.db,
        admin: f.admin,
        target_reads: std::cell::Cell::new(0),
        transactions: std::cell::Cell::new(0),
    };
    let service = AuthService::new(&db, &f.rt, AuthPolicy::default(), &f.key).unwrap();
    db.transactions.set(0);
    let UserResponse::Created { account, .. } = service
        .execute_users(
            create(AccountRole::Host),
            &f.principal,
            Some(command_id(&f.rt, 1)),
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("create missing")
    };
    assert_eq!(db.target_reads.get(), 1);
    assert_eq!(db.transactions.get(), 1);
    db.target_reads.set(0);
    assert!(matches!(
        service
            .execute_users(
                create(AccountRole::Host),
                &f.principal,
                Some(command_id(&f.rt, 1)),
                ORIGIN
            )
            .unwrap(),
        UserResponse::Committed { .. }
    ));
    assert_eq!(db.target_reads.get(), 0);
    for (n, command) in [
        (
            2,
            ManagementCommand::EnableAccount {
                account_id: account.account_id,
            },
        ),
        (
            3,
            ManagementCommand::ReissueEnrollment {
                account_id: account.account_id,
            },
        ),
        (
            4,
            ManagementCommand::ResetPassword {
                account_id: verified,
            },
        ),
    ] {
        db.target_reads.set(0);
        db.transactions.set(0);
        assert!(
            service
                .execute_users(
                    command.clone(),
                    &f.principal,
                    Some(command_id(&f.rt, n)),
                    ORIGIN
                )
                .is_ok()
        );
        assert!(db.target_reads.get() > 0);
        assert_eq!(db.transactions.get(), 1);
        db.target_reads.set(0);
        db.transactions.set(0);
        assert!(matches!(
            service
                .execute_users(command, &f.principal, Some(command_id(&f.rt, n)), ORIGIN)
                .unwrap(),
            UserResponse::Committed { .. }
        ));
        assert_eq!(db.target_reads.get(), 0);
        assert_eq!(db.transactions.get(), 1);
    }
}

#[test]
fn users_authority_rejects_unknown_host_restricted_disabled_and_expired_sessions() {
    let f = Fixture::new();
    for token in [String::new(), new_token(&f.rt).unwrap()] {
        assert_eq!(
            f.service()
                .execute_users(
                    list(),
                    &ManagementPrincipal::AdminSession(token),
                    None,
                    ORIGIN
                )
                .unwrap_err(),
            ManagementError::Unauthorized
        );
    }
    assert_eq!(count(&f.db, "admin_audit"), 0);
    let host = seed(&f.db, &f.rt, 10, AccountRole::Host, AccountStatus::Verified);
    let host_token = session(&f.db, &f.rt, host, 11, SessionScope::Normal);
    assert_eq!(
        f.service()
            .execute_users(
                create(AccountRole::Host),
                &ManagementPrincipal::AdminSession(host_token),
                Some(command_id(&f.rt, 1)),
                ORIGIN
            )
            .unwrap_err(),
        ManagementError::Forbidden
    );
    assert_eq!(count(&f.db, "admin_audit"), 1);
    for (n, status, scope) in [
        (
            12,
            AccountStatus::PendingEnrollment,
            SessionScope::EnrollmentOnly,
        ),
        (
            14,
            AccountStatus::ResetRequired,
            SessionScope::PasswordResetOnly,
        ),
    ] {
        let target = seed(&f.db, &f.rt, n, AccountRole::Admin, status);
        let token = session(&f.db, &f.rt, target, n + 1, scope);
        assert_eq!(
            f.service()
                .execute_users(
                    list(),
                    &ManagementPrincipal::AdminSession(token),
                    None,
                    ORIGIN
                )
                .unwrap_err(),
            ManagementError::Unauthorized
        );
    }
    let before = count(&f.db, "admin_audit");
    f.db.execute(
        "UPDATE accounts SET disabled_at=? WHERE account_id=?",
        &[
            SqlValue::Integer(f.rt.now.get()),
            SqlValue::Text(f.admin.to_string()),
        ],
    )
    .unwrap();
    assert_eq!(
        f.run(list(), None).unwrap_err(),
        ManagementError::Unauthorized
    );
    f.db.execute(
        "UPDATE accounts SET disabled_at=NULL WHERE account_id=?",
        &[SqlValue::Text(f.admin.to_string())],
    )
    .unwrap();
    f.rt.now.set(f.rt.now.get() + 86_400_000);
    assert_eq!(
        f.run(list(), None).unwrap_err(),
        ManagementError::Unauthorized
    );
    assert_eq!(count(&f.db, "admin_audit"), before);
    assert_eq!(count(&f.db, "management_receipts"), 0);
}

#[test]
fn users_self_account_guards_and_pending_target_serialization_are_unchanged() {
    let f = Fixture::new();
    for (n, command) in [
        (
            1,
            ManagementCommand::DisableAccount {
                account_id: f.admin,
            },
        ),
        (
            2,
            ManagementCommand::DeleteAccount {
                account_id: f.admin,
            },
        ),
        (
            3,
            ManagementCommand::EnableAccount {
                account_id: f.admin,
            },
        ),
    ] {
        assert_eq!(
            f.run(command, Some(n)).unwrap_err(),
            ManagementError::Forbidden
        );
    }
    assert_eq!(count(&f.db, "pending_account_removals"), 0);
    assert_eq!(
        f.service()
            .execute_management(
                ManagementCommand::DeleteAccount {
                    account_id: f.admin
                },
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&f.rt, 7)),
                ORIGIN
            )
            .unwrap_err(),
        ManagementError::LastAdmin
    );
    let target = seed(
        &f.db,
        &f.rt,
        10,
        AccountRole::Host,
        AccountStatus::PendingEnrollment,
    );
    let command = ManagementCommand::DisableAccount { account_id: target };
    let first = f.run(command.clone(), Some(4)).unwrap();
    assert_eq!(first.status(), 202);
    assert!(matches!(first, UserResponse::Pending { .. }));
    assert!(matches!(
        f.run(command, Some(4)).unwrap(),
        UserResponse::Pending { .. }
    ));
    assert_eq!(
        f.run(
            ManagementCommand::EnableAccount { account_id: target },
            Some(5)
        )
        .unwrap_err(),
        ManagementError::Busy
    );
    assert_eq!(
        f.run(
            ManagementCommand::ReissueEnrollment { account_id: target },
            Some(6)
        )
        .unwrap_err(),
        ManagementError::Busy
    );
    assert_eq!(count(&f.db, "pending_account_removals"), 1);
}

#[test]
fn users_pages_use_default_fifty_maximum_hundred_and_exclusive_ascending_cursor() {
    let f = Fixture::new();
    for n in 1..=101 {
        seed(
            &f.db,
            &f.rt,
            n,
            AccountRole::Host,
            AccountStatus::PendingEnrollment,
        );
    }
    let UserResponse::Users { users, next_cursor } = f
        .run(
            ManagementCommand::ListAccounts {
                after: None,
                limit: 0,
            },
            None,
        )
        .unwrap()
    else {
        panic!("page missing")
    };
    assert_eq!(users.len(), 50);
    assert_eq!(next_cursor, users.last().map(|a| a.account_id));
    assert!(users.windows(2).all(|a| a[0].account_id < a[1].account_id));
    let UserResponse::Users {
        users: second,
        next_cursor: None,
    } = f
        .run(
            ManagementCommand::ListAccounts {
                after: next_cursor,
                limit: 100,
            },
            None,
        )
        .unwrap()
    else {
        panic!("continuation missing")
    };
    assert_eq!(second.len(), 52);
    assert!(
        second
            .iter()
            .all(|account| Some(account.account_id) > next_cursor)
    );
    let UserResponse::Users {
        users,
        next_cursor: Some(_),
    } = f
        .run(
            ManagementCommand::ListAccounts {
                after: None,
                limit: 100,
            },
            None,
        )
        .unwrap()
    else {
        panic!("maximum page missing")
    };
    assert_eq!(users.len(), 100);
    assert_eq!(
        f.run(
            ManagementCommand::ListAccounts {
                after: None,
                limit: 101
            },
            None
        )
        .unwrap_err(),
        ManagementError::InvalidInput
    );
}

#[test]
fn users_withhold_reads_and_roll_back_mutations_when_success_audit_fails() {
    for command in [list(), create(AccountRole::Host)] {
        let f = Fixture::new();
        f.db.execute("CREATE TRIGGER block_audit BEFORE INSERT ON admin_audit WHEN NEW.outcome='succeeded' BEGIN SELECT RAISE(ABORT,'injected audit failure'); END",&[]).unwrap();
        let id = command.is_mutating().then_some(1);
        assert_eq!(f.run(command, id).unwrap_err(), ManagementError::Storage);
        assert_eq!(count(&f.db, "accounts"), 1);
        assert_eq!(count(&f.db, "access_links"), 0);
        assert_eq!(count(&f.db, "management_receipts"), 0);
        assert_eq!(count(&f.db, "admin_audit"), 1);
        assert_eq!(
            f.db.query("SELECT outcome FROM admin_audit", &[]).unwrap(),
            vec![vec![SqlValue::Text("failed".into())]]
        );
    }
}

#[test]
fn users_projection_failure_rolls_back_the_same_create_link_receipt_and_audit() {
    let f = Fixture::new();
    f.db.execute("CREATE TRIGGER remove_projection AFTER INSERT ON admin_audit WHEN NEW.operation='create_account' AND NEW.outcome='succeeded' BEGIN DELETE FROM access_links WHERE account_id=NEW.target_account_id; DELETE FROM accounts WHERE account_id=NEW.target_account_id; END",&[]).unwrap();
    assert_eq!(
        f.run(create(AccountRole::Host), Some(1)).unwrap_err(),
        ManagementError::Storage
    );
    assert_eq!(count(&f.db, "accounts"), 1);
    assert_eq!(count(&f.db, "access_links"), 0);
    assert_eq!(count(&f.db, "management_receipts"), 0);
    assert_eq!(
        f.db.query("SELECT outcome FROM admin_audit", &[]).unwrap(),
        vec![vec![SqlValue::Text("failed".into())]]
    );
}

#[test]
fn users_and_auth_share_one_actor_command_namespace_in_both_directions() {
    use brews_backend::auth::{AuthCommand, AuthError, RequestContext};
    let f = Fixture::new();
    let password = new_token(&f.rt).unwrap();
    let phc = hash_password(&password, AuthPolicy::default(), &f.rt).unwrap();
    f.db.execute(
        "UPDATE accounts SET verifier=? WHERE account_id=?",
        &[SqlValue::Text(phc), SqlValue::Text(f.admin.to_string())],
    )
    .unwrap();
    let context = |n| RequestContext {
        command_id: Some(command_id(&f.rt, n).to_string()),
        caller_identity: "trusted-users-fixture".into(),
    };
    assert!(
        f.service()
            .execute(
                AuthCommand::Login {
                    username: "AccountPerson200".into(),
                    password: password.clone()
                },
                context(1)
            )
            .is_ok()
    );
    assert_eq!(
        f.run(create(AccountRole::Host), Some(1)).unwrap_err(),
        ManagementError::Conflict
    );
    assert_eq!(count(&f.db, "accounts"), 1);
    assert!(matches!(
        f.run(create(AccountRole::Host), Some(2)).unwrap(),
        UserResponse::Created { .. }
    ));
    let sessions = count(&f.db, "account_sessions");
    assert_eq!(
        f.service()
            .execute(
                AuthCommand::Login {
                    username: "AccountPerson200".into(),
                    password
                },
                context(2)
            )
            .unwrap_err(),
        AuthError::Conflict
    );
    assert_eq!(count(&f.db, "account_sessions"), sessions);
    assert_eq!(count(&f.db, "command_receipts"), 1);
    assert_eq!(count(&f.db, "management_receipts"), 1);
}

#[test]
fn users_reuses_cli_actor_namespace_but_fingerprint_conflicts_remain_owner_bound() {
    use brews_contracts::management::ManagementResponse;
    let f = Fixture::new();
    let UserResponse::Created { account, .. } = f.run(create(AccountRole::Host), Some(1)).unwrap()
    else {
        panic!("create missing")
    };
    assert!(
        matches!(f.service().execute_management(create(AccountRole::Host),match &f.principal {ManagementPrincipal::AdminSession(token)=>ManagementPrincipal::AdminSession(token.clone()),_=>panic!("admin fixture missing")},Some(command_id(&f.rt,1)),ORIGIN).unwrap(),ManagementResponse::Committed {receipt} if receipt.account_id==account.account_id)
    );
    assert_eq!(
        f.run(create(AccountRole::Admin), Some(1)).unwrap_err(),
        ManagementError::Conflict
    );
    assert_eq!(
        f.run(list(), Some(2)).unwrap_err(),
        ManagementError::InvalidInput
    );
    assert_eq!(
        f.run(create(AccountRole::Host), None).unwrap_err(),
        ManagementError::InvalidInput
    );
    assert_eq!(
        f.service()
            .execute_users(
                create(AccountRole::Host),
                &f.principal,
                Some(command_id(&f.rt, 2)),
                "https://app.example.test/"
            )
            .unwrap_err(),
        ManagementError::InvalidInput
    );
    let other = ManagementCommand::CreateAccount {
        username: "SeparateCliPerson01".into(),
        role: AccountRole::Host,
    };
    assert!(matches!(
        f.service()
            .execute_management(
                other,
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&f.rt, 1)),
                ORIGIN
            )
            .unwrap(),
        ManagementResponse::Issued { .. }
    ));
    assert_eq!(count(&f.db, "management_receipts"), 2);
}

#[test]
fn users_self_reset_commits_projection_but_retired_session_cannot_replay() {
    let f = Fixture::new();
    let command = ManagementCommand::ResetPassword {
        account_id: f.admin,
    };
    assert!(
        matches!(f.run(command.clone(),Some(1)).unwrap(),UserResponse::PasswordReset {account_id,status:AccountStatus::ResetRequired,..} if account_id==f.admin)
    );
    assert_eq!(count(&f.db, "management_receipts"), 1);
    assert_eq!(
        f.run(command, Some(1)).unwrap_err(),
        ManagementError::Unauthorized
    );
}

#[test]
fn users_reads_require_admin_sessions_and_audit_safe_results() {
    let f = Fixture::new();
    let response = f.run(list(), None).unwrap();
    assert!(
        matches!(response, UserResponse::Users { users, next_cursor: None } if users.len()==1 && users[0].account_id==f.admin)
    );
    assert!(
        matches!(f.run(ManagementCommand::GetAccount { account_id: f.admin },None).unwrap(), UserResponse::Account { account } if account.account_id==f.admin)
    );
    assert_eq!(count(&f.db, "admin_audit"), 2);
    assert_eq!(
        f.service()
            .execute_users(list(), &ManagementPrincipal::DeveloperCli, None, ORIGIN)
            .unwrap_err(),
        ManagementError::Forbidden
    );
    assert_eq!(
        f.run(
            ManagementCommand::ListAudit {
                after: None,
                limit: 50
            },
            None
        )
        .unwrap_err(),
        ManagementError::Forbidden
    );
}
