#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::{
    auth::{
        AuthCommand, AuthPolicy, AuthService, CookieEffect, ManagementPrincipal, RemovalPhase,
        RequestContext,
    },
    db::{Database, Row, SqlValue, StorageError, migrate},
};
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::{
    accounts::{AccessLinkPurpose, AccountRole, SessionScope},
    ids::{AccountId, CommandId},
};
use std::cell::RefCell;
use support::{Sqlite, TestRuntime};

const ORIGIN: &str = "https://app.example.test";
struct RecordedSqlite {
    sqlite: Sqlite,
    statements: RefCell<Vec<(String, Vec<SqlValue>)>>,
}
impl RecordedSqlite {
    fn new() -> Self {
        let sqlite = Sqlite::new();
        migrate(&sqlite).unwrap();
        Self {
            sqlite,
            statements: RefCell::new(Vec::new()),
        }
    }
    fn assert_bound(&self, prefix: &str, index: usize, tag: &str) {
        let statements = self.statements.borrow();
        let matches: Vec<_> = statements
            .iter()
            .filter(|(sql, _)| sql.starts_with(prefix))
            .collect();
        assert!(
            !matches.is_empty(),
            "runtime statement was not exercised: {prefix}"
        );
        for (sql, params) in matches {
            assert!(
                !sql.contains(&format!("'{tag}'")),
                "domain tag remains a SQL literal: {sql}"
            );
            assert!(
                params.get(index) == Some(&SqlValue::Text(tag.into())),
                "bound SQL tag mismatch: statement {prefix}, parameter {index}, expected {tag}"
            );
        }
    }
}
impl Database for RecordedSqlite {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        self.statements
            .borrow_mut()
            .push((sql.into(), params.to_vec()));
        self.sqlite.query(sql, params)
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        self.statements
            .borrow_mut()
            .push((sql.into(), params.to_vec()));
        self.sqlite.execute(sql, params)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.sqlite.transaction(operation)
    }
}
fn command_id(rt: &TestRuntime, n: u8) -> CommandId {
    let mut bytes = [0; 16];
    bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes).try_into().unwrap()
}
fn context(rt: &TestRuntime, n: u8) -> RequestContext {
    RequestContext {
        command_id: Some(command_id(rt, n).to_string()),
        caller_identity: "trusted-fixture".into(),
    }
}
fn create(
    service: &AuthService<'_, RecordedSqlite, TestRuntime>,
    rt: &TestRuntime,
) -> (AccountId, String) {
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
        ManagementResponse::Issued { receipt, url } => {
            (receipt.account_id, url.split_once('#').unwrap().1.into())
        }
        _ => panic!("enrollment link missing"),
    }
}
fn cookie(effect: CookieEffect) -> String {
    match effect {
        CookieEffect::Set { token, .. } => token,
        _ => panic!("session missing"),
    }
}

#[test]
fn enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags() {
    let db = RecordedSqlite::new();
    let rt = TestRuntime::new();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let (account, token) = create(&service, &rt);
    let restricted = service
        .execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token,
            },
            context(&rt, 2),
        )
        .unwrap();
    let password = brews_backend::security::new_token(&rt).unwrap();
    let normal = service
        .execute(
            AuthCommand::Complete {
                scope: SessionScope::EnrollmentOnly,
                token: cookie(restricted.cookie),
                new_password: password.clone(),
            },
            context(&rt, 3),
        )
        .unwrap();
    assert!(matches!(normal.cookie, CookieEffect::Set { .. }));
    assert_eq!(
        db.sqlite
            .query("SELECT status,credential_epoch FROM accounts", &[])
            .unwrap(),
        vec![vec![
            SqlValue::Text("verified".into()),
            SqlValue::Integer(1)
        ]]
    );
    let reset_token = match service
        .execute_management(
            ManagementCommand::ResetPassword {
                account_id: account,
            },
            ManagementPrincipal::DeveloperCli,
            Some(command_id(&rt, 4)),
            ORIGIN,
        )
        .unwrap()
    {
        ManagementResponse::Issued { url, .. } => url.split_once('#').unwrap().1.into(),
        _ => panic!("reset link missing"),
    };
    assert_eq!(
        db.sqlite
            .query("SELECT status,credential_epoch FROM accounts", &[])
            .unwrap(),
        vec![vec![
            SqlValue::Text("reset_required".into()),
            SqlValue::Integer(2)
        ]]
    );
    let restricted = service
        .execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::PasswordReset,
                token: reset_token,
            },
            context(&rt, 5),
        )
        .unwrap();
    let completed = service
        .execute(
            AuthCommand::Complete {
                scope: SessionScope::PasswordResetOnly,
                token: cookie(restricted.cookie),
                new_password: password,
            },
            context(&rt, 6),
        )
        .unwrap();
    assert!(matches!(completed.cookie, CookieEffect::Clear));
    assert_eq!(
        db.sqlite
            .query("SELECT status,credential_epoch FROM accounts", &[])
            .unwrap(),
        vec![vec![
            SqlValue::Text("verified".into()),
            SqlValue::Integer(3)
        ]]
    );
    db.assert_bound("INSERT INTO accounts", 2, "host");
    db.assert_bound("INSERT INTO accounts", 3, "pending_enrollment");
    db.assert_bound("UPDATE accounts SET verifier=", 1, "verified");
    db.assert_bound("UPDATE accounts SET credential_epoch=", 1, "reset_required");
    let statements = db.statements.borrow();
    for (table, index, expected) in [
        (
            "INSERT INTO access_links",
            2,
            vec!["enrollment", "password_reset"],
        ),
        (
            "INSERT INTO account_sessions",
            3,
            vec!["enrollment_only", "normal", "password_reset_only"],
        ),
    ] {
        let tags: Vec<_> = statements
            .iter()
            .filter(|(sql, _)| sql.starts_with(table))
            .map(|(sql, params)| {
                assert!(!sql.contains('\''));
                match &params[index] {
                    SqlValue::Text(tag) => tag.as_str(),
                    _ => panic!("text tag missing"),
                }
            })
            .collect();
        assert_eq!(tags, expected);
    }
}

#[test]
fn removal_preparation_and_abort_bind_phase_and_last_admin_filters() {
    let db = RecordedSqlite::new();
    let rt = TestRuntime::new();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let (account, _) = create(&service, &rt);
    let operation = match service
        .prepare_removal(
            ManagementCommand::DisableAccount {
                account_id: account,
            },
            ManagementPrincipal::DeveloperCli,
            command_id(&rt, 2),
        )
        .unwrap()
    {
        ManagementResponse::Pending { operation_id } => operation_id,
        _ => panic!("durable removal missing"),
    };
    assert_eq!(
        service.removal_operation(operation).unwrap().unwrap().phase,
        RemovalPhase::Prepared
    );
    service.begin_removal_abort(operation).unwrap();
    service.begin_removal_abort(operation).unwrap();
    assert_eq!(
        service.removal_operation(operation).unwrap().unwrap().phase,
        RemovalPhase::Aborting
    );
    assert_eq!(
        db.sqlite
            .query(
                "SELECT phase,attempt_count FROM pending_account_removals",
                &[]
            )
            .unwrap(),
        vec![vec![
            SqlValue::Text("aborting".into()),
            SqlValue::Integer(0)
        ]]
    );
    assert_eq!(
        db.sqlite
            .query("SELECT disabled_at,credential_epoch FROM accounts", &[])
            .unwrap(),
        vec![vec![SqlValue::Null, SqlValue::Integer(0)]]
    );
    db.assert_bound("INSERT INTO pending_account_removals", 6, "prepared");
    db.assert_bound("UPDATE pending_account_removals SET phase=", 0, "aborting");
    db.assert_bound("SELECT count(*) FROM accounts WHERE role=", 0, "admin");
    db.assert_bound("SELECT count(*) FROM accounts WHERE role=", 1, "verified");
}

fn assert_verifier_rehashed(rows: &[Row], previous: &str) {
    assert!(
        rows != [vec![SqlValue::Text(previous.into())]],
        "password verifier was not rehashed"
    );
}

#[test]
fn assertion_failure_diagnostics_do_not_disclose_verifiers() {
    const CHILD_FLAG: &str = "BREWS_SQL_ASSERTION_PRIVACY_CHILD";
    if std::env::var_os(CHILD_FLAG).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "assertion_failure_diagnostics_do_not_disclose_verifiers",
                "--nocapture",
            ])
            .env(CHILD_FLAG, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "SQL assertion diagnostics did not meet the redaction contract."
        );
        return;
    }
    // Isolate the panic hook from concurrent tests, and never print captured payloads.
    std::panic::set_hook(Box::new(|_| {}));
    let rt = TestRuntime::new();
    let password = brews_backend::security::new_token(&rt).unwrap();
    let verifier =
        brews_backend::security::hash_password(&password, AuthPolicy::default(), &rt).unwrap();
    let db = RecordedSqlite::new();
    let rows = db
        .query("SELECT ?", &[SqlValue::Text(verifier.clone())])
        .unwrap();
    let failures = [
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            db.assert_bound("SELECT ?", 0, "verified")
        })),
        std::panic::catch_unwind(|| assert_verifier_rehashed(&rows, &verifier)),
    ];
    let expected = [
        "bound SQL tag mismatch: statement SELECT ?, parameter 0, expected verified",
        "password verifier was not rehashed",
    ];
    let mut safe = true;
    for (failure, expected) in failures.into_iter().zip(expected) {
        let payload = failure.expect_err("Privacy probe did not exercise assertion failure.");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap();
        safe &= message == expected
            && !message.contains(&verifier)
            && !message.contains(&password)
            && !message.contains("$argon2");
    }
    assert!(safe, "SQL assertion diagnostics must be secret-free.");
}

#[test]
fn rehash_compare_and_swap_binds_verified_status() {
    let db = RecordedSqlite::new();
    let rt = TestRuntime::new();
    let account = command_id(&rt, 100).to_string();
    let password = brews_backend::security::new_token(&rt).unwrap();
    let old_policy = AuthPolicy::default();
    let policy = AuthPolicy {
        t_cost: old_policy.t_cost + 1,
        ..old_policy
    };
    let old_verifier = brews_backend::security::hash_password(&password, old_policy, &rt).unwrap();
    db.sqlite.execute(
        "INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at) VALUES(?,'HostPerson01','host','verified',?,0,?,?)",
        &[SqlValue::Text(account), SqlValue::Text(old_verifier.clone()), SqlValue::Integer(rt.now.get()), SqlValue::Integer(rt.now.get())],
    ).unwrap();
    let service = AuthService::new(&db, &rt, policy, &[9; 32]).unwrap();
    assert!(matches!(
        service
            .execute(
                AuthCommand::Login {
                    username: "HostPerson01".into(),
                    password
                },
                context(&rt, 1)
            )
            .unwrap()
            .cookie,
        CookieEffect::Set { .. }
    ));
    assert_verifier_rehashed(
        &db.sqlite
            .query("SELECT verifier FROM accounts", &[])
            .unwrap(),
        &old_verifier,
    );
    db.assert_bound("UPDATE accounts SET verifier=", 4, "verified");
}
