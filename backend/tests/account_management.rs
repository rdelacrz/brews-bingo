#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::{
    auth::{AuthPolicy, AuthService, ManagementPrincipal, Runtime},
    db::{Database, SqlValue, migrate},
};
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::{
    accounts::AccountRole,
    ids::{AccountId, CommandId},
};
use support::{Sqlite, TestRuntime};
const ORIGIN: &str = "https://app.example.test";
fn key(rt: &TestRuntime) -> [u8; 32] {
    let mut key = [0; 32];
    rt.fill_random(&mut key).unwrap();
    key
}
fn command_id(rt: &TestRuntime, n: u8) -> CommandId {
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes).try_into().unwrap()
}
fn create() -> ManagementCommand {
    ManagementCommand::CreateAccount {
        username: " \tHostPerson01\r".into(),
        role: AccountRole::Host,
    }
}
fn issued(response: ManagementResponse) -> (AccountId, String) {
    let debug = format!("{response:?}");
    match response {
        ManagementResponse::Issued { receipt, url } => {
            assert!(!debug.contains(&url));
            assert!(!debug.contains(url.split('#').nth(1).unwrap()));
            (receipt.account_id, url)
        }
        _ => panic!("expected freshly issued link"),
    }
}
#[test]
fn create_commits_pending_account_link_receipt_and_audit_before_handoff() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (id, url) = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    assert!(url.starts_with(&format!("{ORIGIN}/enroll#")));
    assert_eq!(
        db.query(
            "SELECT username,status,credential_epoch,verifier FROM accounts WHERE account_id=?",
            &[SqlValue::Text(id.to_string())]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text("HostPerson01".into()),
            SqlValue::Text("pending_enrollment".into()),
            SqlValue::Integer(0),
            SqlValue::Null
        ]]
    );
    for table in ["access_links", "management_receipts", "admin_audit"] {
        assert_eq!(
            db.query(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
    }
    let dump = db
        .query("SELECT outcome FROM management_receipts", &[])
        .unwrap();
    let token = url.split('#').nth(1).unwrap();
    assert!(
        dump.iter()
            .flatten()
            .all(|v| !matches!(v,SqlValue::Text(text) if text.contains(token)))
    );
}

#[test]
fn same_command_replays_secret_free_receipt_without_a_second_link() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let id = command_id(&rt, 1);
    let _ = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(id),
                ORIGIN,
            )
            .unwrap(),
    );
    assert!(matches!(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(id),
                ORIGIN
            )
            .unwrap(),
        ManagementResponse::Committed { .. }
    ));
    assert_eq!(
        db.query("SELECT count(*) FROM access_links", &[]).unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

fn enrolled_account(
    db: &Sqlite,
    rt: &TestRuntime,
    key: &[u8],
    role: AccountRole,
) -> (AccountId, String) {
    use brews_backend::auth::{AuthCommand, CookieEffect, RequestContext};
    use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
    let service = AuthService::new(db, rt, AuthPolicy::default(), key).unwrap();
    let (id, url) = issued(
        service
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: "AdminPerson01".into(),
                    role,
                },
                ManagementPrincipal::DeveloperCli,
                Some(command_id(rt, 201)),
                ORIGIN,
            )
            .unwrap(),
    );
    let context = |n| RequestContext {
        command_id: Some(command_id(rt, n).to_string()),
        caller_identity: "trusted-fixture".into(),
    };
    let restricted = service
        .execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token: url.split('#').nth(1).unwrap().to_owned(),
            },
            context(202),
        )
        .unwrap();
    let CookieEffect::Set { token, .. } = restricted.cookie else {
        panic!("restricted session missing")
    };
    let password = brews_backend::security::new_token(rt).unwrap();
    let normal = service
        .execute(
            AuthCommand::Complete {
                scope: SessionScope::EnrollmentOnly,
                token,
                new_password: password,
            },
            context(203),
        )
        .unwrap();
    let CookieEffect::Set { token, .. } = normal.cookie else {
        panic!("normal session missing")
    };
    (id, token)
}
#[test]
fn users_read_requires_live_normal_verified_enabled_admin_on_every_call() {
    use brews_backend::auth::ManagementError;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (id, token) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let command = ManagementCommand::ListAccounts {
        after: None,
        limit: 50,
    };
    assert!(
        matches!(service.execute_management(command.clone(),ManagementPrincipal::AdminSession(token.clone()),None,ORIGIN).unwrap(),ManagementResponse::Accounts{accounts,..} if accounts.len()==1)
    );
    db.execute(
        "UPDATE accounts SET disabled_at=? WHERE account_id=?",
        &[
            SqlValue::Integer(rt.now.get()),
            SqlValue::Text(id.to_string()),
        ],
    )
    .unwrap();
    assert_eq!(
        service
            .execute_management(
                command,
                ManagementPrincipal::AdminSession(token),
                None,
                ORIGIN
            )
            .unwrap_err(),
        ManagementError::Unauthorized
    );
}

#[test]
fn authenticated_rejections_are_durably_audited() {
    use brews_backend::auth::ManagementError;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let _ = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    assert_eq!(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 2)),
                ORIGIN
            )
            .unwrap_err(),
        ManagementError::Conflict
    );
    assert_eq!(
        db.query(
            "SELECT count(*) FROM admin_audit WHERE outcome='rejected'",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn failed_authenticated_mutation_rolls_back_then_records_failure() {
    use brews_backend::auth::ManagementError;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    db.execute("CREATE TRIGGER block_links BEFORE INSERT ON access_links BEGIN SELECT RAISE(ABORT,'injected failure'); END",&[]).unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert_eq!(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN
            )
            .unwrap_err(),
        ManagementError::Storage
    );
    assert_eq!(
        db.query("SELECT count(*) FROM accounts", &[]).unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    assert_eq!(
        db.query(
            "SELECT count(*) FROM admin_audit WHERE outcome='failed'",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn enrollment_successor_fences_prior_link_without_replaying_secrets() {
    use brews_backend::auth::{AuthCommand, AuthError, RequestContext};
    use brews_domain::accounts::AccessLinkPurpose;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (id, url) = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    let (same, new_url) = issued(
        service
            .execute_management(
                ManagementCommand::ReissueEnrollment { account_id: id },
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 2)),
                ORIGIN,
            )
            .unwrap(),
    );
    assert_eq!(same, id);
    assert!(new_url != url);
    assert_eq!(
        service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token: url.split('#').nth(1).unwrap().into()
                },
                RequestContext {
                    command_id: Some(command_id(&rt, 3).to_string()),
                    caller_identity: "trusted-fixture".into()
                }
            )
            .unwrap_err(),
        AuthError::InvalidLink
    );
    assert_eq!(
        db.query(
            "SELECT credential_epoch,status FROM accounts WHERE account_id=?",
            &[SqlValue::Text(id.to_string())]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Integer(1),
            SqlValue::Text("pending_enrollment".into())
        ]]
    );
}

#[test]
fn reset_immediately_invalidates_normal_session_and_keeps_identity() {
    use brews_backend::auth::{AuthCommand, RequestContext};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (id, token) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (same, url) = issued(
        service
            .execute_management(
                ManagementCommand::ResetPassword { account_id: id },
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    assert_eq!(same, id);
    assert!(url.starts_with(&format!("{ORIGIN}/password-reset#")));
    let current = service
        .execute(
            AuthCommand::Current { token: Some(token) },
            RequestContext {
                command_id: None,
                caller_identity: "trusted-fixture".into(),
            },
        )
        .unwrap();
    assert_eq!(current.body["authenticated"], false);
    assert_eq!(
        db.query(
            "SELECT credential_epoch,status FROM accounts WHERE account_id=?",
            &[SqlValue::Text(id.to_string())]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Integer(2),
            SqlValue::Text("reset_required".into())
        ]]
    );
}

#[test]
fn new_link_commands_reject_exact_cutoff_and_future_uuid_time() {
    use brews_backend::auth::ManagementError;
    for offset in [-2 * 86_400_000, -86_400_000, -30 * 86_400_000, 1] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let key = key(&rt);
        let now = rt.now.get();
        rt.now.set(now + offset);
        let command = command_id(&rt, 1);
        rt.now.set(now);
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        assert_eq!(
            service
                .execute_management(
                    create(),
                    ManagementPrincipal::DeveloperCli,
                    Some(command),
                    ORIGIN
                )
                .unwrap_err(),
            ManagementError::StaleCommand
        );
        assert_eq!(
            db.query("SELECT count(*) FROM accounts", &[]).unwrap(),
            vec![vec![SqlValue::Integer(0)]]
        );
    }
}

#[test]
fn enable_preserves_lifecycle_and_epoch_without_reviving_credentials() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (id, _) = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    db.execute(
        "UPDATE accounts SET disabled_at=?,credential_epoch=1 WHERE account_id=?",
        &[
            SqlValue::Integer(rt.now.get()),
            SqlValue::Text(id.to_string()),
        ],
    )
    .unwrap();
    db.execute(
        "UPDATE access_links SET revoked_at=?",
        &[SqlValue::Integer(rt.now.get())],
    )
    .unwrap();
    assert!(matches!(
        service
            .execute_management(
                ManagementCommand::EnableAccount { account_id: id },
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 2)),
                ORIGIN
            )
            .unwrap(),
        ManagementResponse::Committed { .. }
    ));
    assert_eq!(
        db.query(
            "SELECT disabled_at,status,credential_epoch FROM accounts WHERE account_id=?",
            &[SqlValue::Text(id.to_string())]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Null,
            SqlValue::Text("pending_enrollment".into()),
            SqlValue::Integer(1)
        ]]
    );
    assert_eq!(
        db.query(
            "SELECT count(*) FROM access_links WHERE revoked_at IS NULL",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}

#[test]
fn account_detail_is_safe_and_deleted_or_unknown_target_is_not_found() {
    use brews_backend::auth::ManagementError;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (id, _) = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    let detail = service
        .execute_management(
            ManagementCommand::GetAccount { account_id: id },
            ManagementPrincipal::DeveloperCli,
            None,
            ORIGIN,
        )
        .unwrap();
    assert!(
        matches!(detail,ManagementResponse::Account{account} if account.account_id==id && account.username=="HostPerson01")
    );
    db.execute(
        "DELETE FROM accounts WHERE account_id=?",
        &[SqlValue::Text(id.to_string())],
    )
    .unwrap();
    assert_eq!(
        service
            .execute_management(
                ManagementCommand::GetAccount { account_id: id },
                ManagementPrincipal::DeveloperCli,
                None,
                ORIGIN
            )
            .unwrap_err(),
        ManagementError::NotFound
    );
}

#[test]
fn developer_audit_read_denies_events_at_the_ninety_day_deadline() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let _ = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    assert!(
        matches!(service.execute_management(ManagementCommand::ListAudit{after:None,limit:50},ManagementPrincipal::DeveloperCli,None,ORIGIN).unwrap(),ManagementResponse::Audit{events,..} if events.len()==1)
    );
    rt.now.set(rt.now.get() + 90 * 86_400_000);
    assert!(
        matches!(service.execute_management(ManagementCommand::ListAudit{after:None,limit:50},ManagementPrincipal::DeveloperCli,None,ORIGIN).unwrap(),ManagementResponse::Audit{events,..} if events.is_empty())
    );
}

#[test]
fn prepare_removal_persists_actor_bound_intent_and_serializes_target() {
    use brews_backend::auth::ManagementError;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let _admin = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (id, _) = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    let command = ManagementCommand::DisableAccount { account_id: id };
    let response = service
        .prepare_removal(
            command.clone(),
            ManagementPrincipal::DeveloperCli,
            command_id(&rt, 2),
        )
        .unwrap();
    let ManagementResponse::Pending { operation_id } = response else {
        panic!("removal must be pending")
    };
    assert!(
        matches!(service.prepare_removal(command,ManagementPrincipal::DeveloperCli,command_id(&rt,2)).unwrap(),ManagementResponse::Pending{operation_id:retry} if retry==operation_id)
    );
    assert_eq!(
        service
            .prepare_removal(
                ManagementCommand::DeleteAccount { account_id: id },
                ManagementPrincipal::DeveloperCli,
                command_id(&rt, 3)
            )
            .unwrap_err(),
        ManagementError::Busy
    );
    assert_eq!(
        db.query(
            "SELECT disabled_at FROM accounts WHERE account_id=?",
            &[SqlValue::Text(id.to_string())]
        )
        .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    assert_eq!(
        db.query("SELECT count(*) FROM pending_account_removals", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn management_retention_is_durable_and_cleanup_preserves_required_work() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let _ = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    assert_eq!(
        service.next_management_deadline().unwrap(),
        Some(rt.now.get() + 30 * 86_400_000)
    );
    rt.now.set(rt.now.get() + 90 * 86_400_000);
    service.cleanup_management().unwrap();
    assert_eq!(
        db.query("SELECT count(*) FROM admin_audit", &[]).unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    assert_eq!(
        db.query("SELECT count(*) FROM management_receipts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}

#[test]
fn pending_removal_serializes_other_target_mutations() {
    use brews_backend::auth::ManagementError;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let _ = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (id, _) = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    let _ = service
        .prepare_removal(
            ManagementCommand::DeleteAccount { account_id: id },
            ManagementPrincipal::DeveloperCli,
            command_id(&rt, 2),
        )
        .unwrap();
    for (n, command) in [
        (3, ManagementCommand::EnableAccount { account_id: id }),
        (4, ManagementCommand::ReissueEnrollment { account_id: id }),
    ] {
        assert_eq!(
            service
                .execute_management(
                    command,
                    ManagementPrincipal::DeveloperCli,
                    Some(command_id(&rt, n)),
                    ORIGIN
                )
                .unwrap_err(),
            ManagementError::Busy
        );
    }
}

#[test]
fn link_handoff_rejects_noncanonical_or_unprotected_origin_before_mutation() {
    use brews_backend::auth::ManagementError;
    for origin in [
        "http://app.example.test",
        "https://app.example.test/path",
        "https://user@app.example.test",
        "https://app.example.test?x=1",
        "https://app.example.test#x",
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let key = key(&rt);
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        assert_eq!(
            service
                .execute_management(
                    create(),
                    ManagementPrincipal::DeveloperCli,
                    Some(command_id(&rt, 1)),
                    origin
                )
                .unwrap_err(),
            ManagementError::InvalidInput
        );
        assert_eq!(
            db.query("SELECT count(*) FROM accounts", &[]).unwrap(),
            vec![vec![SqlValue::Integer(0)]]
        );
    }
}

#[test]
fn fingerprint_uses_trimmed_case_sensitive_username_semantics() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let id = command_id(&rt, 1);
    let _ = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(id),
                ORIGIN,
            )
            .unwrap(),
    );
    let same = ManagementCommand::CreateAccount {
        username: "HostPerson01".into(),
        role: AccountRole::Host,
    };
    assert!(matches!(
        service
            .execute_management(same, ManagementPrincipal::DeveloperCli, Some(id), ORIGIN)
            .unwrap(),
        ManagementResponse::Committed { .. }
    ));
    let changed = ManagementCommand::CreateAccount {
        username: "hostPerson01".into(),
        role: AccountRole::Host,
    };
    assert_eq!(
        service
            .execute_management(changed, ManagementPrincipal::DeveloperCli, Some(id), ORIGIN)
            .unwrap_err(),
        brews_backend::auth::ManagementError::Conflict
    );
}

#[test]
fn read_result_is_withheld_if_its_audit_cannot_commit() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    db.execute("CREATE TRIGGER block_audit BEFORE INSERT ON admin_audit BEGIN SELECT RAISE(ABORT,'injected failure'); END",&[]).unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert_eq!(
        service
            .execute_management(
                ManagementCommand::ListAccounts {
                    after: None,
                    limit: 50
                },
                ManagementPrincipal::DeveloperCli,
                None,
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::Storage
    );
}

fn enrolled_admin(db: &Sqlite, rt: &TestRuntime, key: &[u8]) -> (AccountId, String) {
    enrolled_account(db, rt, key, AccountRole::Admin)
}
#[test]
fn successful_account_reads_have_durable_closed_owner_targets() {
    use brews_contracts::management::{AuditOperation, AuditTarget};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let _ = service
        .execute_management(
            ManagementCommand::ListAccounts {
                after: None,
                limit: 50,
            },
            ManagementPrincipal::DeveloperCli,
            None,
            ORIGIN,
        )
        .unwrap();
    let ManagementResponse::Audit { events, .. } = service
        .execute_management(
            ManagementCommand::ListAudit {
                after: None,
                limit: 50,
            },
            ManagementPrincipal::DeveloperCli,
            None,
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("audit page missing")
    };
    assert!(
        events
            .iter()
            .any(|event| event.operation == AuditOperation::ListAccounts
                && event.target == AuditTarget::AccountsOwner)
    );
    assert_eq!(
        db.query(
            "SELECT count(*) FROM admin_audit WHERE operation='list_audit' AND outcome='succeeded'",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}
#[test]
fn rejected_create_uses_accounts_owner_and_never_the_submitted_username() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let submitted = "invalid submitted username";
    assert_eq!(
        service
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: submitted.into(),
                    role: AccountRole::Host
                },
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::InvalidInput
    );
    assert_eq!(
        db.query(
            "SELECT target_kind,target_account_id FROM admin_audit WHERE outcome='rejected'",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text("accounts_owner".into()),
            SqlValue::Null
        ]]
    );
    let rows=db.query("SELECT audit_id,actor,operation,target_kind,target_account_id,outcome FROM admin_audit",&[]).unwrap();
    assert!(
        rows.iter()
            .flatten()
            .all(|v| !matches!(v,SqlValue::Text(text) if text.contains(submitted)))
    );
}
#[test]
fn missing_account_read_audits_the_explicit_account_target() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let id: AccountId = command_id(&rt, 1).to_string().parse().unwrap();
    assert_eq!(
        service
            .execute_management(
                ManagementCommand::GetAccount { account_id: id },
                ManagementPrincipal::DeveloperCli,
                None,
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::NotFound
    );
    assert_eq!(
        db.query(
            "SELECT target_kind,target_account_id,operation,outcome FROM admin_audit",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text("account".into()),
            SqlValue::Text(id.to_string()),
            SqlValue::Text("get_account".into()),
            SqlValue::Text("rejected".into())
        ]]
    );
}
#[test]
fn a_current_host_session_is_attributed_when_admin_authority_is_denied() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (id, token) = enrolled_account(&db, &rt, &key, AccountRole::Host);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert_eq!(
        service
            .execute_management(
                ManagementCommand::ListAccounts {
                    after: None,
                    limit: 50
                },
                ManagementPrincipal::AdminSession(token),
                None,
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::Forbidden
    );
    assert_eq!(db.query("SELECT actor,target_kind FROM admin_audit WHERE operation='list_accounts' AND outcome='rejected'",&[]).unwrap(),vec![vec![SqlValue::Text(id.to_string()),SqlValue::Text("accounts_owner".into())]]);
}
#[test]
fn invalid_and_expired_sessions_create_no_audit_events() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (_, token) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let before = db.query("SELECT count(*) FROM admin_audit", &[]).unwrap();
    rt.now.set(rt.now.get() + 86_400_000);
    for token in [token, "invalid".into()] {
        assert_eq!(
            service
                .execute_management(
                    ManagementCommand::ListAccounts {
                        after: None,
                        limit: 50
                    },
                    ManagementPrincipal::AdminSession(token),
                    None,
                    ORIGIN
                )
                .unwrap_err(),
            brews_backend::auth::ManagementError::Unauthorized
        );
    }
    assert_eq!(
        db.query("SELECT count(*) FROM admin_audit", &[]).unwrap(),
        before
    );
}
#[test]
fn audit_write_failure_rolls_back_account_link_and_receipt() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    db.execute("CREATE TRIGGER block_audit BEFORE INSERT ON admin_audit BEGIN SELECT RAISE(ABORT,'injected failure'); END",&[]).unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert_eq!(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::Storage
    );
    for table in ["accounts", "access_links", "management_receipts"] {
        assert_eq!(
            db.query(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(0)]]
        );
    }
}

#[test]
fn host_removal_does_not_invent_a_required_existing_admin() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let (id, _) = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap(),
    );
    assert!(matches!(
        service
            .prepare_removal(
                ManagementCommand::DisableAccount { account_id: id },
                ManagementPrincipal::DeveloperCli,
                command_id(&rt, 2)
            )
            .unwrap(),
        ManagementResponse::Pending { .. }
    ));
}

#[test]
fn new_management_admission_honors_the_persisted_nonretreating_floor() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    db.execute(
        "UPDATE storage_metadata SET command_floor_ms=?",
        &[SqlValue::Integer(rt.now.get() + 29 * 86_400_000)],
    )
    .unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert_eq!(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::StaleCommand
    );
    assert_eq!(
        db.query("SELECT count(*) FROM accounts", &[]).unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}

#[test]
fn account_pages_are_bounded_and_continue_without_duplicates() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    for (n, name) in [
        (1, "HostPerson01"),
        (2, "HostPerson02"),
        (3, "HostPerson03"),
    ] {
        let _ = issued(
            service
                .execute_management(
                    ManagementCommand::CreateAccount {
                        username: name.into(),
                        role: AccountRole::Host,
                    },
                    ManagementPrincipal::DeveloperCli,
                    Some(command_id(&rt, n)),
                    ORIGIN,
                )
                .unwrap(),
        );
    }
    let mut after = None;
    let mut ids = Vec::new();
    loop {
        let ManagementResponse::Accounts {
            accounts,
            next_cursor,
        } = service
            .execute_management(
                ManagementCommand::ListAccounts { after, limit: 1 },
                ManagementPrincipal::DeveloperCli,
                None,
                ORIGIN,
            )
            .unwrap()
        else {
            panic!("accounts page missing")
        };
        assert!(accounts.len() <= 1);
        ids.extend(accounts.iter().map(|a| a.account_id));
        if next_cursor.is_none() {
            break;
        }
        after = next_cursor;
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 3);
    assert_eq!(
        service
            .execute_management(
                ManagementCommand::ListAccounts {
                    after: None,
                    limit: 101
                },
                ManagementPrincipal::DeveloperCli,
                None,
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::InvalidInput
    );
}

#[test]
fn malformed_retained_receipt_does_not_become_a_successful_replay() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let id = command_id(&rt, 1);
    let _ = issued(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(id),
                ORIGIN,
            )
            .unwrap(),
    );
    db.execute(
        "UPDATE management_receipts SET outcome=json_set(outcome,'$.link_id',NULL)",
        &[],
    )
    .unwrap();
    assert_eq!(
        service
            .execute_management(
                create(),
                ManagementPrincipal::DeveloperCli,
                Some(id),
                ORIGIN
            )
            .unwrap_err(),
        brews_backend::auth::ManagementError::Storage
    );
}
