#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::{
    auth::{AuthCommand, AuthError, AuthPolicy, AuthService, CookieEffect, RequestContext},
    security::hash_password,
    storage::{Database, SqlValue, migrate},
};
use support::{Sqlite, TestRuntime};

const USER: &str = "HostAccount1";
fn password() -> String {
    let mut bytes = vec![120; 10];
    bytes.extend_from_slice(&[32, 0, 127, 9, 32]);
    String::from_utf8(bytes).unwrap()
}
const ACCOUNT: &str = "01890f3e-53b7-7d28-9b05-4f65092d5711";
fn context(rt: &TestRuntime, n: u8) -> RequestContext {
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    RequestContext {
        command_id: Some(uuid::Uuid::from_bytes(bytes).to_string()),
        caller_identity: "trusted-test-caller".into(),
    }
}
fn seed(db: &Sqlite, rt: &TestRuntime) {
    let phc = hash_password(&password(), AuthPolicy::default(), rt).unwrap();
    db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?,?,?,?,?,?, ?,NULL)", &[SqlValue::Text(ACCOUNT.into()),SqlValue::Text(USER.into()),SqlValue::Text("host".into()),SqlValue::Text("verified".into()),SqlValue::Text(phc),SqlValue::Integer(0),SqlValue::Integer(rt.now.get()),SqlValue::Integer(rt.now.get())]).unwrap();
}
fn login() -> AuthCommand {
    AuthCommand::Login {
        username: format!("\t {USER}\r"),
        password: password(),
    }
}
fn seed_link(
    db: &Sqlite,
    rt: &TestRuntime,
    purpose: brews_domain::accounts::AccessLinkPurpose,
) -> String {
    use brews_domain::accounts::AccessLinkPurpose;
    if purpose == AccessLinkPurpose::Enrollment {
        db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?, 'host','pending_enrollment',NULL,0,?,NULL,NULL)",&[SqlValue::Text(ACCOUNT.into()),SqlValue::Text(USER.into()),SqlValue::Integer(rt.now.get())]).unwrap();
    } else {
        seed(db, rt);
        db.execute("UPDATE accounts SET status='reset_required'", &[])
            .unwrap();
    }
    let token = brews_backend::security::new_token(rt).unwrap();
    let digest = brews_backend::security::token_digest(&token).unwrap();
    db.execute("INSERT INTO access_links(link_id,account_id,purpose,token_verifier,credential_epoch,issued_at,expires_at,consumed_at,revoked_at) VALUES(?,?,?,?,0,?,?,NULL,NULL)",&[SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5712".into()),SqlValue::Text(ACCOUNT.into()),SqlValue::Text(purpose.to_string()),SqlValue::Blob(digest.to_vec()),SqlValue::Integer(rt.now.get()),SqlValue::Integer(rt.now.get()+86_400_000)]).unwrap();
    token
}

#[test]
fn redemption_consumes_once_and_creates_a_restricted_session() {
    use brews_domain::accounts::AccessLinkPurpose;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let token = seed_link(&db, &rt, AccessLinkPurpose::Enrollment);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    assert_eq!(
        service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::PasswordReset,
                    token: token.clone()
                },
                context(&rt, 1)
            )
            .unwrap_err(),
        AuthError::InvalidLink
    );
    let out = service
        .execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token: token.clone(),
            },
            context(&rt, 2),
        )
        .unwrap();
    assert_eq!(out.body["setup_required"], true);
    assert_eq!(out.body["session"]["scope"], "EnrollmentOnly");
    assert_eq!(
        service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token: token.clone()
                },
                context(&rt, 2)
            )
            .unwrap_err(),
        AuthError::InvalidLink
    );
    assert_eq!(
        service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token
                },
                context(&rt, 3)
            )
            .unwrap_err(),
        AuthError::InvalidLink
    );
    assert_eq!(
        db.query("SELECT COUNT(*) FROM account_sessions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert_eq!(
        db.query("SELECT credential_epoch FROM accounts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}

#[test]
fn completion_advances_epoch_once_with_purpose_specific_session_effects() {
    use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
    for purpose in [
        AccessLinkPurpose::Enrollment,
        AccessLinkPurpose::PasswordReset,
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let token = seed_link(&db, &rt, purpose);
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let redeemed = service
            .execute(AuthCommand::Redeem { purpose, token }, context(&rt, 1))
            .unwrap();
        let restricted = bearer(&redeemed);
        let original = redeemed.body["session"]["expires_at"].as_i64().unwrap();
        rt.now.set(rt.now.get() + 3_600_000);
        let scope = if purpose == AccessLinkPurpose::Enrollment {
            SessionScope::EnrollmentOnly
        } else {
            SessionScope::PasswordResetOnly
        };
        let out = service
            .execute(
                AuthCommand::Complete {
                    scope,
                    token: restricted.clone(),
                    new_password: password(),
                },
                context(&rt, 2),
            )
            .unwrap();
        assert_eq!(
            db.query("SELECT status,credential_epoch FROM accounts", &[])
                .unwrap(),
            vec![vec![
                SqlValue::Text("verified".into()),
                SqlValue::Integer(1)
            ]]
        );
        assert_eq!(
            service
                .execute(
                    AuthCommand::Current {
                        token: Some(restricted.clone())
                    },
                    context(&rt, 3)
                )
                .unwrap()
                .body["authenticated"],
            false
        );
        if purpose == AccessLinkPurpose::Enrollment {
            assert_eq!(out.body["session"]["scope"], "Normal");
            assert_eq!(out.body["session"]["expires_at"], original);
            let normal = bearer(&out);
            assert!(normal != restricted);
            assert_eq!(
                service
                    .execute(
                        AuthCommand::Current {
                            token: Some(normal)
                        },
                        context(&rt, 4)
                    )
                    .unwrap()
                    .body["authenticated"],
                true
            );
        } else {
            assert!(matches!(out.cookie, CookieEffect::Clear));
            assert_eq!(out.body["reset_completed"], true);
            assert_eq!(out.body["next_action"], "login");
            assert_eq!(
                db.query(
                    "SELECT COUNT(*) FROM account_sessions WHERE scope='normal'",
                    &[]
                )
                .unwrap(),
                vec![vec![SqlValue::Integer(0)]]
            );
            assert!(service.execute(login(), context(&rt, 5)).is_ok());
        }
    }
}

#[test]
fn failure_only_rate_buckets_survive_services_and_do_not_extend_blocks() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    for n in 1..=5 {
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        assert_eq!(
            service
                .execute(
                    AuthCommand::Login {
                        username: USER.into(),
                        password: String::from_utf8(vec![121; 15]).unwrap()
                    },
                    context(&rt, n)
                )
                .unwrap_err(),
            AuthError::InvalidCredentials
        );
    }
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    assert_eq!(
        service.execute(login(), context(&rt, 6)).unwrap_err(),
        AuthError::RateLimited
    );
    let deadline = db
        .query(
            "SELECT blocked_until FROM rate_limit_buckets ORDER BY subject_key",
            &[],
        )
        .unwrap();
    rt.now.set(rt.now.get() + 600_000);
    assert_eq!(
        service.execute(login(), context(&rt, 7)).unwrap_err(),
        AuthError::RateLimited
    );
    assert_eq!(
        deadline,
        db.query(
            "SELECT blocked_until FROM rate_limit_buckets ORDER BY subject_key",
            &[]
        )
        .unwrap()
    );
    rt.now.set(rt.now.get() + 300_000);
    assert!(service.execute(login(), context(&rt, 8)).is_ok());
    assert!(
        db.query("SELECT scope FROM rate_limit_buckets", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn redemption_rate_limits_are_durable_and_separate_from_login_buckets() {
    use brews_domain::accounts::AccessLinkPurpose;
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let token = seed_link(&db, &rt, AccessLinkPurpose::Enrollment);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    for n in 1..=5 {
        assert_eq!(
            service
                .execute(
                    AuthCommand::Redeem {
                        purpose: AccessLinkPurpose::PasswordReset,
                        token: token.clone()
                    },
                    context(&rt, n)
                )
                .unwrap_err(),
            AuthError::InvalidLink
        );
    }
    assert_eq!(
        service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token: token.clone()
                },
                context(&rt, 6)
            )
            .unwrap_err(),
        AuthError::RateLimited
    );
    assert!(
        db.query(
            "SELECT scope FROM rate_limit_buckets WHERE scope='account_login'",
            &[]
        )
        .unwrap()
        .is_empty()
    );
    rt.now.set(rt.now.get() + 900_000);
    assert!(
        service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token
                },
                context(&rt, 7)
            )
            .is_ok()
    );
    assert!(
        db.query("SELECT scope FROM rate_limit_buckets", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn login_retry_requires_password_proof_and_returns_only_a_secret_free_receipt() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let first = service.execute(login(), context(&rt, 1)).unwrap();
    let token = bearer(&first);
    let retry = service.execute(login(), context(&rt, 1)).unwrap();
    assert!(matches!(retry.cookie, CookieEffect::None));
    assert_eq!(retry.body["replayed"], true);
    assert!(retry.body.get("account").is_none());
    assert!(retry.body.get("session").is_none());
    assert!(!retry.body.to_string().contains(&token));
    assert_eq!(
        db.query("SELECT COUNT(*) FROM account_sessions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    let rows = db
        .query("SELECT outcome FROM command_receipts", &[])
        .unwrap();
    assert_eq!(rows.len(), 1);
    let SqlValue::Text(text) = &rows[0][0] else {
        panic!("wrong SQL type")
    };
    assert!(text.len() <= 4096);
    assert!(!text.contains(&token));
    assert!(!text.contains(&password()));
    assert!(!text.contains("username"));
    assert!(!text.contains("verifier"));
    assert_eq!(
        service
            .execute(
                AuthCommand::Login {
                    username: USER.into(),
                    password: String::from_utf8(vec![121; 15]).unwrap()
                },
                context(&rt, 1)
            )
            .unwrap_err(),
        AuthError::InvalidCredentials
    );
    let mut missing = context(&rt, 2);
    missing.command_id = None;
    assert_eq!(
        service.execute(login(), missing).unwrap_err(),
        AuthError::InvalidInput
    );
}

#[test]
fn completion_retry_reproves_account_and_reused_semantics_conflict() {
    use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
    for purpose in [
        AccessLinkPurpose::Enrollment,
        AccessLinkPurpose::PasswordReset,
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let token = seed_link(&db, &rt, purpose);
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let restricted = bearer(
            &service
                .execute(AuthCommand::Redeem { purpose, token }, context(&rt, 1))
                .unwrap(),
        );
        let scope = if purpose == AccessLinkPurpose::Enrollment {
            SessionScope::EnrollmentOnly
        } else {
            SessionScope::PasswordResetOnly
        };
        let completion = || AuthCommand::Complete {
            scope,
            token: restricted.clone(),
            new_password: password(),
        };
        assert_eq!(
            service.execute(completion(), context(&rt, 1)).unwrap_err(),
            AuthError::Conflict
        );
        service.execute(completion(), context(&rt, 2)).unwrap();
        let retry = service.execute(completion(), context(&rt, 2)).unwrap();
        assert_eq!(retry.body["replayed"], true);
        assert!(matches!(retry.cookie, CookieEffect::None));
        assert!(retry.body.get("account").is_none());
        assert!(retry.body.get("session").is_none());
        assert_eq!(
            db.query("SELECT credential_epoch FROM accounts", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
        assert_eq!(
            db.query("SELECT COUNT(*) FROM command_receipts", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(2)]]
        );
        assert_eq!(
            service
                .execute(
                    AuthCommand::Complete {
                        scope,
                        token: restricted,
                        new_password: String::from_utf8(vec![121; 15]).unwrap()
                    },
                    context(&rt, 2)
                )
                .unwrap_err(),
            AuthError::Unauthorized
        );
    }
}

#[test]
fn command_admission_rejects_expired_retry_before_and_after_exact_cleanup_restart() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let start = rt.now.get();
    let expiry = start + 86_400_000;
    let old = context(&rt, 1).command_id.unwrap();
    let old_context = || RequestContext {
        command_id: Some(old.clone()),
        caller_identity: "trusted-test-caller".into(),
    };
    {
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        service.execute(login(), old_context()).unwrap();
        assert_eq!(
            db.query("SELECT completed_at,expires_at FROM command_receipts", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(start), SqlValue::Integer(expiry)]]
        );
        assert_eq!(service.next_deadline().unwrap(), Some(expiry));
        rt.now.set(expiry);
        let retry = service.execute(login(), old_context());
        assert!(matches!(retry, Err(AuthError::StaleCommand)));
        assert_eq!(
            db.query("SELECT COUNT(*) FROM command_receipts", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
        assert_eq!(
            db.query("SELECT COUNT(*) FROM account_sessions", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(1)]]
        );
        service.cleanup().unwrap();
    }
    assert_eq!(
        db.query("SELECT command_floor_ms FROM storage_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(start)]]
    );
    assert!(
        db.query("SELECT command_id FROM command_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT session_id FROM account_sessions", &[])
            .unwrap()
            .is_empty()
    );
    let restarted = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let retry = restarted.execute(login(), old_context());
    assert!(matches!(retry, Err(AuthError::StaleCommand)));
    assert!(
        db.query("SELECT command_id FROM command_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT session_id FROM account_sessions", &[])
            .unwrap()
            .is_empty()
    );
    let just_above_floor = TestRuntime::new();
    just_above_floor.now.set(start + 1);
    let fresh = restarted
        .execute(login(), context(&just_above_floor, 2))
        .unwrap();
    assert_eq!(fresh.body["session"]["expires_at"], expiry + 86_400_000);
}

#[test]
fn command_admission_treats_initial_zero_floor_as_a_stale_cutoff() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    rt.now.set(0);
    migrate(&db).unwrap();
    seed(&db, &rt);
    let zero = context(&rt, 1);
    assert!(
        zero.command_id
            .as_deref()
            .unwrap()
            .parse::<brews_domain::ids::CommandId>()
            .is_ok()
    );
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    assert_eq!(
        db.query("SELECT command_floor_ms FROM storage_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    let result = service.execute(login(), zero);
    assert!(matches!(result, Err(AuthError::StaleCommand)));
    assert!(
        db.query("SELECT command_id FROM command_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT session_id FROM account_sessions", &[])
            .unwrap()
            .is_empty()
    );
    rt.now.set(1);
    assert!(service.execute(login(), context(&rt, 2)).is_ok());
}

#[test]
fn cleanup_keeps_old_commands_rejected_after_service_restart_and_clock_rollback() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let start = rt.now.get();
    let old = context(&rt, 1).command_id.unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    service.execute(login(), context(&rt, 1)).unwrap();
    assert_eq!(service.next_deadline().unwrap(), Some(start + 86_400_000));
    rt.now.set(start + 86_400_001);
    service.cleanup().unwrap();
    assert!(
        db.query("SELECT command_id FROM command_receipts", &[])
            .unwrap()
            .is_empty()
    );
    assert!(
        db.query("SELECT session_id FROM account_sessions", &[])
            .unwrap()
            .is_empty()
    );
    let restarted = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let old_context = || RequestContext {
        command_id: Some(old.clone()),
        caller_identity: "trusted-test-caller".into(),
    };
    assert_eq!(
        restarted.execute(login(), old_context()).unwrap_err(),
        AuthError::StaleCommand
    );
    rt.now.set(start);
    assert_eq!(
        restarted.execute(login(), old_context()).unwrap_err(),
        AuthError::Crypto
    );
}

#[test]
fn logout_and_expiry_keep_durable_socket_close_intents_until_acknowledged() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let out = service.execute(login(), context(&rt, 1)).unwrap();
    let token = bearer(&out);
    let expiry = out.body["session"]["expires_at"].as_i64().unwrap();
    db.execute("INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) SELECT account_id,session_id,?, ?,credential_epoch,expires_at FROM account_sessions",&[SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5713".into()),SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5714".into())]).unwrap();
    service
        .execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 2))
        .unwrap();
    service.cleanup().unwrap();
    assert_eq!(
        db.query("SELECT COUNT(*) FROM account_socket_close_work", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert_eq!(
        db.query("SELECT COUNT(*) FROM account_socket_subscriptions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert_eq!(service.next_deadline().unwrap(), Some(rt.now.get() + 1000));
    let second = service.execute(login(), context(&rt, 3)).unwrap();
    db.execute("INSERT INTO account_socket_subscriptions(account_id,session_id,game_id,connection_id,credential_epoch,expires_at) SELECT account_id,session_id,?, ?,credential_epoch,expires_at FROM account_sessions",&[SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5713".into()),SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5715".into())]).unwrap();
    drop(second);
    rt.now.set(expiry);
    service.cleanup().unwrap();
    assert_eq!(
        db.query("SELECT COUNT(*) FROM account_socket_close_work", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
    assert_eq!(
        db.query("SELECT COUNT(*) FROM account_socket_subscriptions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
}

#[test]
fn successful_login_upgrades_supported_old_phc_without_rotating_account_epoch() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let before = db.query("SELECT verifier FROM accounts", &[]).unwrap();
    let policy = AuthPolicy {
        m_cost: 19_456,
        t_cost: 3,
        p_cost: 1,
    };
    let service = AuthService::new(&db, &rt, policy, &[9; 32]).unwrap();
    let out = service.execute(login(), context(&rt, 1)).unwrap();
    let after = db.query("SELECT verifier FROM accounts", &[]).unwrap();
    assert!(before != after);
    let SqlValue::Text(phc) = &after[0][0] else {
        panic!("wrong SQL type")
    };
    assert!(phc.starts_with("$argon2id$v=19$m=19456,t=3,p=1$"));
    assert_eq!(
        brews_backend::security::verify_password(&password(), phc),
        Ok(true)
    );
    assert_eq!(
        db.query("SELECT credential_epoch FROM accounts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    assert_eq!(out.body["session"]["expires_at"], rt.now.get() + 86_400_000);
}

#[test]
fn session_checks_are_authoritative_for_expiry_epoch_disable_and_revocation() {
    for mutation in ["expiry", "epoch", "disable", "revoke", "reset"] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        seed(&db, &rt);
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let out = service.execute(login(), context(&rt, 1)).unwrap();
        let token = bearer(&out);
        assert_eq!(
            service
                .execute(
                    AuthCommand::Current {
                        token: Some(token.clone())
                    },
                    context(&rt, 2)
                )
                .unwrap()
                .body["authenticated"],
            true
        );
        match mutation {
            "expiry" => rt
                .now
                .set(out.body["session"]["expires_at"].as_i64().unwrap()),
            "epoch" => db
                .execute("UPDATE accounts SET credential_epoch=1", &[])
                .unwrap(),
            "disable" => db
                .execute("UPDATE accounts SET disabled_at=0", &[])
                .unwrap(),
            "revoke" => db
                .execute("UPDATE account_sessions SET revoked_at=0", &[])
                .unwrap(),
            "reset" => db
                .execute("UPDATE accounts SET status='reset_required'", &[])
                .unwrap(),
            _ => unreachable!(),
        }
        let current = service
            .execute(AuthCommand::Current { token: Some(token) }, context(&rt, 3))
            .unwrap();
        assert_eq!(current.body["authenticated"], false);
        assert!(matches!(current.cookie, CookieEffect::Clear));
        assert!(current.body.get("account").is_none());
    }
}

#[test]
fn invalid_link_states_share_one_safe_error_and_issue_no_session() {
    use brews_domain::accounts::AccessLinkPurpose;
    for mutation in ["expiry", "epoch", "disable", "revoke", "consume"] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let token = seed_link(&db, &rt, AccessLinkPurpose::Enrollment);
        match mutation {
            "expiry" => rt.now.set(rt.now.get() + 86_400_000),
            "epoch" => db
                .execute("UPDATE accounts SET credential_epoch=1", &[])
                .unwrap(),
            "disable" => db
                .execute("UPDATE accounts SET disabled_at=0", &[])
                .unwrap(),
            "revoke" => db
                .execute("UPDATE access_links SET revoked_at=0", &[])
                .unwrap(),
            "consume" => db
                .execute("UPDATE access_links SET consumed_at=0", &[])
                .unwrap(),
            _ => unreachable!(),
        }
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        assert_eq!(
            service
                .execute(
                    AuthCommand::Redeem {
                        purpose: AccessLinkPurpose::Enrollment,
                        token
                    },
                    context(&rt, 1)
                )
                .unwrap_err(),
            AuthError::InvalidLink
        );
        assert!(
            db.query("SELECT session_id FROM account_sessions", &[])
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn setup_rechecks_expiry_after_hashing_and_entropy_failure_never_consumes_links() {
    use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let token = seed_link(&db, &rt, AccessLinkPurpose::Enrollment);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    rt.fail.set(true);
    assert_eq!(
        service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token: token.clone()
                },
                context(&rt, 1)
            )
            .unwrap_err(),
        AuthError::Crypto
    );
    assert_eq!(
        db.query("SELECT consumed_at FROM access_links", &[])
            .unwrap(),
        vec![vec![SqlValue::Null]]
    );
    rt.fail.set(false);
    let restricted = bearer(
        &service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token,
                },
                context(&rt, 2),
            )
            .unwrap(),
    );
    rt.advance_random_ms.set(86_400_000);
    assert_eq!(
        service
            .execute(
                AuthCommand::Complete {
                    scope: SessionScope::EnrollmentOnly,
                    token: restricted,
                    new_password: password()
                },
                context(&rt, 3)
            )
            .unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        db.query("SELECT status,credential_epoch,verifier FROM accounts", &[])
            .unwrap(),
        vec![vec![
            SqlValue::Text("pending_enrollment".into()),
            SqlValue::Integer(0),
            SqlValue::Null
        ]]
    );
}

#[test]
fn digest_collisions_retry_bounded_fresh_entropy_without_retiring_existing_sessions() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let start_entropy = rt.random.get();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let first = bearer(&service.execute(login(), context(&rt, 1)).unwrap());
    rt.random.set(start_entropy);
    let second = bearer(&service.execute(login(), context(&rt, 2)).unwrap());
    assert!(first != second);
    assert_eq!(
        db.query(
            "SELECT COUNT(*) FROM account_sessions WHERE revoked_at IS NULL",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
}

#[test]
fn completion_receipt_failure_rolls_back_the_entire_credential_transition() {
    use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let token = seed_link(&db, &rt, AccessLinkPurpose::Enrollment);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let restricted = bearer(
        &service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token,
                },
                context(&rt, 1),
            )
            .unwrap(),
    );
    db.execute("CREATE TRIGGER fail_completion BEFORE INSERT ON command_receipts WHEN NEW.outcome LIKE '%complete_enrollment%' BEGIN SELECT RAISE(ABORT,'injected'); END",&[]).unwrap();
    assert_eq!(
        service
            .execute(
                AuthCommand::Complete {
                    scope: SessionScope::EnrollmentOnly,
                    token: restricted,
                    new_password: password()
                },
                context(&rt, 2)
            )
            .unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(
        db.query("SELECT status,credential_epoch,verifier FROM accounts", &[])
            .unwrap(),
        vec![vec![
            SqlValue::Text("pending_enrollment".into()),
            SqlValue::Integer(0),
            SqlValue::Null
        ]]
    );
    assert_eq!(
        db.query("SELECT scope,revoked_at FROM account_sessions", &[])
            .unwrap(),
        vec![vec![
            SqlValue::Text("enrollment_only".into()),
            SqlValue::Null
        ]]
    );
}

#[test]
fn login_sql_round_trips_nul_del_and_exact_case_distinct_usernames() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let upper = "User\0name\x7f1";
    let lower = upper.to_ascii_lowercase();
    db.execute(
        "UPDATE accounts SET username=? WHERE account_id=?",
        &[SqlValue::Text(upper.into()), SqlValue::Text(ACCOUNT.into())],
    )
    .unwrap();
    db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) SELECT ?,?,'admin',status,verifier,credential_epoch,created_at,password_set_at,NULL FROM accounts",&[SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5712".into()),SqlValue::Text(lower.clone())]).unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let host = service
        .execute(
            AuthCommand::Login {
                username: format!("\u{0b}{upper}\u{0b}"),
                password: password(),
            },
            context(&rt, 1),
        )
        .unwrap();
    let admin = service
        .execute(
            AuthCommand::Login {
                username: lower,
                password: password(),
            },
            context(&rt, 2),
        )
        .unwrap();
    assert_eq!(host.body["account"]["role"], "Host");
    assert_eq!(admin.body["account"]["role"], "Admin");
    assert!(host.body["account"]["account_id"] != admin.body["account"]["account_id"]);
}

#[test]
fn receipt_payload_cannot_claim_a_different_account_even_if_storage_is_corrupted() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    service.execute(login(), context(&rt, 1)).unwrap();
    db.execute(
        "UPDATE command_receipts SET outcome=json_set(outcome,'$.outcome.account_id',?)",
        &[SqlValue::Text(
            "01890f3e-53b7-7d28-9b05-4f65092d5712".into(),
        )],
    )
    .unwrap();
    assert_eq!(
        service.execute(login(), context(&rt, 1)).unwrap_err(),
        AuthError::Storage
    );
}

#[test]
fn unknown_account_denial_does_real_bounded_password_work_before_failure_accounting() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let start = rt.now.get();
    rt.advance_random_ms.set(1);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    assert_eq!(
        service.execute(login(), context(&rt, 1)).unwrap_err(),
        AuthError::InvalidCredentials
    );
    assert_eq!(rt.now.get(), start + 1);
    assert!(
        db.query("SELECT session_id FROM account_sessions", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.query("SELECT attempt_count FROM rate_limit_buckets", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)], vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn verifier_upgrade_revalidates_concurrent_reset_disable_and_optional_hash_failure() {
    use brews_backend::auth::Runtime;
    use std::cell::Cell;
    struct Mutating<'a> {
        db: &'a Sqlite,
        base: &'a TestRuntime,
        replacement: Option<&'a str>,
        disable: bool,
        fail_once: bool,
        first: Cell<bool>,
    }
    impl Runtime for Mutating<'_> {
        fn now_ms(&self) -> i64 {
            self.base.now.get()
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
            if self.first.replace(false) {
                if let Some(phc) = self.replacement {
                    self.db
                        .execute(
                            "UPDATE accounts SET verifier=?,credential_epoch=1",
                            &[SqlValue::Text(phc.into())],
                        )
                        .map_err(|_| AuthError::Storage)?;
                }
                if self.disable {
                    self.db
                        .execute("UPDATE accounts SET disabled_at=0,credential_epoch=1", &[])
                        .map_err(|_| AuthError::Storage)?;
                }
                if self.fail_once {
                    return Err(AuthError::Crypto);
                }
            }
            self.base.fill_random(bytes)
        }
    }
    for action in ["same", "different", "disable", "hash_failure"] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        seed(&db, &rt);
        let current = if action == "different" {
            String::from_utf8(vec![121; 15]).unwrap()
        } else {
            password()
        };
        let policy = AuthPolicy {
            m_cost: 19_456,
            t_cost: 3,
            p_cost: 1,
        };
        let replacement = hash_password(&current, policy, &rt).unwrap();
        let mutator = Mutating {
            db: &db,
            base: &rt,
            replacement: if matches!(action, "same" | "different") {
                Some(&replacement)
            } else {
                None
            },
            disable: action == "disable",
            fail_once: action == "hash_failure",
            first: Cell::new(true),
        };
        let service = AuthService::new(&db, &mutator, policy, &[9; 32]).unwrap();
        let result = service.execute(login(), context(&rt, 1));
        if matches!(action, "different" | "disable") {
            assert_eq!(result.unwrap_err(), AuthError::InvalidCredentials);
            assert!(
                db.query("SELECT session_id FROM account_sessions", &[])
                    .unwrap()
                    .is_empty()
            );
        } else {
            assert!(result.is_ok());
        }
        if matches!(action, "same" | "different") {
            let rows = db
                .query("SELECT verifier,credential_epoch FROM accounts", &[])
                .unwrap();
            assert!(rows[0][0] == SqlValue::Text(replacement));
            assert_eq!(rows[0][1], SqlValue::Integer(1));
        }
    }
}

#[test]
fn logout_is_idempotent_and_does_not_retire_other_normal_sessions() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let first = bearer(&service.execute(login(), context(&rt, 1)).unwrap());
    let second = bearer(&service.execute(login(), context(&rt, 2)).unwrap());
    for _ in 0..2 {
        let out = service
            .execute(
                AuthCommand::Logout {
                    token: Some(first.clone()),
                },
                context(&rt, 3),
            )
            .unwrap();
        assert_eq!(out.body["logged_out"], true);
        assert!(matches!(out.cookie, CookieEffect::Clear));
    }
    assert_eq!(
        service
            .execute(AuthCommand::Current { token: Some(first) }, context(&rt, 4))
            .unwrap()
            .body["authenticated"],
        false
    );
    assert_eq!(
        service
            .execute(
                AuthCommand::Current {
                    token: Some(second)
                },
                context(&rt, 5)
            )
            .unwrap()
            .body["authenticated"],
        true
    );
    assert_eq!(
        service
            .execute(AuthCommand::Logout { token: None }, context(&rt, 6))
            .unwrap()
            .body["logged_out"],
        true
    );
}

fn bearer(out: &brews_backend::auth::AuthOutcome) -> String {
    match &out.cookie {
        CookieEffect::Set { token, .. } => token.clone(),
        _ => panic!("cookie missing"),
    }
}

#[test]
fn login_creates_fixed_life_normal_authority_in_real_sqlite() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    seed(&db, &rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
    let out = service.execute(login(), context(&rt, 1)).unwrap();
    assert_eq!(out.body["account"]["username"], USER);
    assert_eq!(out.body["session"]["scope"], "Normal");
    assert_eq!(out.body["session"]["expires_at"], rt.now.get() + 86_400_000);
    let token = bearer(&out);
    assert!(!out.body.to_string().contains(&token));
    assert!(!format!("{out:?}").contains(&token));
    let current = service
        .execute(AuthCommand::Current { token: Some(token) }, context(&rt, 2))
        .unwrap();
    assert_eq!(current.body["authenticated"], true);
    assert_eq!(
        db.query("SELECT credential_epoch FROM accounts", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}
