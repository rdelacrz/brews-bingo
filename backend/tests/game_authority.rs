#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::{
    auth::{
        AuthCommand, AuthError, AuthPolicy, AuthService, CookieEffect, ManagementPrincipal,
        RequestContext,
    },
    db::{Database, SqlValue, migrate},
};
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::{
    accounts::{AccessLinkPurpose, AccountRole, SessionScope},
    ids::{AccountId, ConnectionId, GameId},
};
use support::{Sqlite, TestRuntime};

const ORIGIN: &str = "https://app.example.test";
fn id(rt: &TestRuntime, n: u8) -> String {
    let mut bytes = [0; 16];
    bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn context(rt: &TestRuntime, n: u8) -> RequestContext {
    RequestContext {
        command_id: Some(id(rt, n)),
        caller_identity: "trusted-test-caller".into(),
    }
}
fn bearer(out: brews_backend::auth::AuthOutcome) -> String {
    match out.cookie {
        CookieEffect::Set { token, .. } => token,
        _ => panic!("Expected newly issued cookie."),
    }
}
fn password(rt: &TestRuntime) -> String {
    brews_backend::security::new_token(rt).unwrap()
}
fn service<'a>(db: &'a Sqlite, rt: &'a TestRuntime) -> AuthService<'a, Sqlite, TestRuntime> {
    AuthService::new(db, rt, AuthPolicy::default(), &[9; 32]).unwrap()
}
fn enrolled(db: &Sqlite, rt: &TestRuntime, role: AccountRole) -> (AccountId, String, String) {
    migrate(db).unwrap();
    let s = service(db, rt);
    let created = s
        .execute_management(
            ManagementCommand::CreateAccount {
                username: "GameHostPerson".into(),
                role,
            },
            ManagementPrincipal::DeveloperCli,
            Some(id(rt, 1).parse().unwrap()),
            ORIGIN,
        )
        .unwrap();
    let (account, link) = match created {
        ManagementResponse::Issued { receipt, url } => (
            receipt.account_id,
            url.rsplit('#').next().unwrap().to_owned(),
        ),
        _ => panic!("Expected enrollment issuance."),
    };
    let restricted = bearer(
        s.execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token: link,
            },
            context(rt, 2),
        )
        .unwrap(),
    );
    let password = password(rt);
    let token = bearer(
        s.execute(
            AuthCommand::Complete {
                scope: SessionScope::EnrollmentOnly,
                token: restricted,
                new_password: password.clone(),
            },
            context(rt, 3),
        )
        .unwrap(),
    );
    (account, token, password)
}
fn game(rt: &TestRuntime) -> GameId {
    id(rt, 240).parse().unwrap()
}
fn connection(rt: &TestRuntime) -> ConnectionId {
    id(rt, 241).parse().unwrap()
}

#[test]
fn invalid_missing_revoked_disabled_epoch_and_lifecycle_fail_closed() {
    for mutation in [
        "DELETE FROM account_sessions",
        "UPDATE account_sessions SET revoked_at=issued_at",
        "UPDATE accounts SET disabled_at=created_at",
        "UPDATE accounts SET credential_epoch=credential_epoch+1",
        "UPDATE accounts SET status='reset_required'",
        "UPDATE accounts SET status='pending_enrollment'",
    ] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
        let s = service(&db, &rt);
        let g = game(&rt);
        let c = connection(&rt);
        let p = s.register_game_account_connection(&token, g, c).unwrap();
        db.execute(mutation, &[]).unwrap();
        assert_eq!(
            s.authorize_game_account(&token).unwrap_err(),
            AuthError::Unauthorized
        );
        assert_eq!(
            s.register_game_account_connection(&token, g, c)
                .unwrap_err(),
            AuthError::Unauthorized
        );
        assert_eq!(
            s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
                .unwrap_err(),
            AuthError::Unauthorized
        );
    }
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    for bad in [
        String::new(),
        "malformed".into(),
        brews_backend::security::new_token(&rt).unwrap(),
    ] {
        assert_eq!(
            s.authorize_game_account(&bad).unwrap_err(),
            AuthError::Unauthorized
        );
        assert_eq!(
            s.register_game_account_connection(&bad, game(&rt), connection(&rt))
                .unwrap_err(),
            AuthError::Unauthorized
        );
    }
    db.execute("DELETE FROM account_sessions", &[]).unwrap();
    db.execute("DELETE FROM accounts", &[]).unwrap();
    assert_eq!(
        s.authorize_game_account(&token).unwrap_err(),
        AuthError::Unauthorized
    );
}

#[test]
fn reset_and_enrollment_sessions_never_become_game_proof() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let p = s
        .register_game_account_connection(&token, game(&rt), connection(&rt))
        .unwrap();
    let out = s
        .execute_management(
            ManagementCommand::ResetPassword {
                account_id: account,
            },
            ManagementPrincipal::DeveloperCli,
            Some(id(&rt, 4).parse().unwrap()),
            ORIGIN,
        )
        .unwrap();
    let link = match out {
        ManagementResponse::Issued { url, .. } => url.rsplit('#').next().unwrap().to_owned(),
        _ => panic!("Expected reset link."),
    };
    let reset = bearer(
        s.execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::PasswordReset,
                token: link,
            },
            context(&rt, 5),
        )
        .unwrap(),
    );
    assert_eq!(
        s.authorize_game_account(&reset).unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.authorize_game_account(&token).unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 1, game(&rt), connection(&rt))
            .unwrap_err(),
        AuthError::Unauthorized
    );
    rt.now.set(rt.now.get() + 1000);
    assert_eq!(s.list_due_game_socket_closes(100).unwrap().len(), 1);
    let pending = Sqlite::new();
    migrate(&pending).unwrap();
    let s = service(&pending, &rt);
    let out = s
        .execute_management(
            ManagementCommand::CreateAccount {
                username: "OtherHostPerson".into(),
                role: AccountRole::Host,
            },
            ManagementPrincipal::DeveloperCli,
            Some(id(&rt, 6).parse().unwrap()),
            ORIGIN,
        )
        .unwrap();
    let link = match out {
        ManagementResponse::Issued { url, .. } => url.rsplit('#').next().unwrap().to_owned(),
        _ => panic!("Expected enrollment link."),
    };
    let enrollment = bearer(
        s.execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token: link,
            },
            context(&rt, 7),
        )
        .unwrap(),
    );
    assert_eq!(
        s.authorize_game_account(&enrollment).unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.register_game_account_connection(&enrollment, game(&rt), connection(&rt))
            .unwrap_err(),
        AuthError::Unauthorized
    );
}

#[test]
fn exact_expiry_denies_before_sweep_and_cleanup_retains_work_until_ack() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    rt.now.set(p.expires_at() - 1);
    assert_eq!(
        s.register_game_account_connection(&token, g, c)
            .unwrap()
            .expires_at(),
        p.expires_at()
    );
    rt.now.set(p.expires_at());
    assert_eq!(
        s.authorize_game_account(&token).unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.register_game_account_connection(&token, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    assert!(s.list_due_game_socket_closes(1).unwrap().is_empty());
    s.cleanup().unwrap();
    assert_eq!(
        db.query("SELECT count(*) FROM account_socket_subscriptions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    rt.now.set(rt.now.get() + 1000);
    let w = s.list_due_game_socket_closes(1).unwrap().remove(0);
    assert_eq!(w.expires_at(), p.expires_at());
    s.acknowledge_game_socket_closed(&w).unwrap();
    assert!(s.list_due_game_socket_closes(1).unwrap().is_empty());
}

#[test]
fn storage_faults_roll_back_registration_retry_and_ack_clock_and_targets() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let clock = db
        .query(
            "SELECT last_observed_ms,command_floor_ms FROM storage_metadata",
            &[],
        )
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    db.conn.borrow().execute_batch("CREATE TRIGGER fail_register BEFORE INSERT ON account_socket_subscriptions BEGIN SELECT RAISE(ABORT,'injected storage fault'); END").unwrap();
    assert_eq!(
        s.register_game_account_connection(&token, g, c)
            .unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(
        db.query(
            "SELECT last_observed_ms,command_floor_ms FROM storage_metadata",
            &[]
        )
        .unwrap(),
        clock
    );
    assert_eq!(
        db.query("SELECT count(*) FROM account_socket_subscriptions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
    db.conn
        .borrow()
        .execute_batch("DROP TRIGGER fail_register")
        .unwrap();
    s.register_game_account_connection(&token, g, c).unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let w = s.list_due_game_socket_closes(1).unwrap().remove(0);
    let before = db
        .query("SELECT * FROM account_socket_close_work", &[])
        .unwrap();
    let clock = db
        .query(
            "SELECT last_observed_ms,command_floor_ms FROM storage_metadata",
            &[],
        )
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    db.conn.borrow().execute_batch("CREATE TRIGGER fail_retry BEFORE UPDATE ON account_socket_close_work BEGIN SELECT RAISE(ABORT,'injected storage fault'); END").unwrap();
    assert_eq!(
        s.mark_game_socket_close_retry(&w).unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(
        db.query("SELECT * FROM account_socket_close_work", &[])
            .unwrap(),
        before
    );
    assert_eq!(
        db.query(
            "SELECT last_observed_ms,command_floor_ms FROM storage_metadata",
            &[]
        )
        .unwrap(),
        clock
    );
    db.conn.borrow().execute_batch("DROP TRIGGER fail_retry; CREATE TRIGGER fail_ack BEFORE DELETE ON account_socket_subscriptions BEGIN SELECT RAISE(ABORT,'injected storage fault'); END").unwrap();
    assert_eq!(
        s.acknowledge_game_socket_closed(&w).unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(
        db.query("SELECT * FROM account_socket_close_work", &[])
            .unwrap(),
        before
    );
    assert_eq!(
        db.query(
            "SELECT last_observed_ms,command_floor_ms FROM storage_metadata",
            &[]
        )
        .unwrap(),
        clock
    );
    db.conn
        .borrow()
        .execute_batch("DROP TRIGGER fail_ack")
        .unwrap();
    s.acknowledge_game_socket_closed(&w).unwrap();
}

#[test]
fn ignored_registration_unregistration_and_retry_never_claim_committed_effect() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    db.conn.borrow().execute_batch("CREATE TRIGGER ignore_insert BEFORE INSERT ON account_socket_subscriptions BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert_eq!(
        s.register_game_account_connection(&token, g, c)
            .unwrap_err(),
        AuthError::Storage
    );
    db.conn
        .borrow()
        .execute_batch("DROP TRIGGER ignore_insert")
        .unwrap();
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    db.conn.borrow().execute_batch("CREATE TRIGGER ignore_unregister BEFORE DELETE ON account_socket_subscriptions BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert_eq!(
        s.unregister_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Storage
    );
    db.conn
        .borrow()
        .execute_batch("DROP TRIGGER ignore_unregister")
        .unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let w = s.list_due_game_socket_closes(1).unwrap().remove(0);
    db.conn.borrow().execute_batch("CREATE TRIGGER ignore_retry BEFORE UPDATE ON account_socket_close_work BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert_eq!(
        s.mark_game_socket_close_retry(&w).unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(s.list_due_game_socket_closes(1).unwrap()[0], w);
}

#[test]
fn every_frame_reads_current_role_and_rejects_changed_subscription_expiry() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    db.execute(
        "UPDATE accounts SET role=?",
        &[SqlValue::Text(AccountRole::Admin.to_string())],
    )
    .unwrap();
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap()
            .role(),
        AccountRole::Admin
    );
    assert_eq!(
        s.authorize_game_account_connection(id(&rt, 230).parse().unwrap(), p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    db.execute(
        "UPDATE account_socket_subscriptions SET expires_at=expires_at+1",
        &[],
    )
    .unwrap();
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.register_game_account_connection(&token, g, c)
            .unwrap_err(),
        AuthError::Conflict
    );
}

#[test]
fn malformed_close_metadata_is_not_dispatchable_or_acknowledgeable() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    s.register_game_account_connection(&token, game(&rt), connection(&rt))
        .unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let w = s.list_due_game_socket_closes(1).unwrap().remove(0);
    db.execute(
        "UPDATE account_socket_close_work SET account_id=?",
        &[SqlValue::Text("malformed".into())],
    )
    .unwrap();
    assert_eq!(
        s.list_due_game_socket_closes(1).unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(
        s.acknowledge_game_socket_closed(&w).unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(
        s.mark_game_socket_close_retry(&w).unwrap_err(),
        AuthError::Storage
    );
    db.execute(
        "UPDATE account_socket_close_work SET account_id=?,created_at=next_attempt_at+1",
        &[SqlValue::Text(w.account_id().to_string())],
    )
    .unwrap();
    assert_eq!(
        s.list_due_game_socket_closes(1).unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(
        db.query("SELECT count(*) FROM account_socket_close_work", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn close_pages_are_bounded_stably_ordered_and_do_not_discard_backlog() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    for n in 0..101 {
        s.register_game_account_connection(&token, g, id(&rt, n).parse().unwrap())
            .unwrap();
    }
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let work = s.list_due_game_socket_closes(100).unwrap();
    assert_eq!(work.len(), 100);
    assert!(
        work.windows(2)
            .all(|pair| pair[0].connection_id() < pair[1].connection_id())
    );
    assert_eq!(s.list_due_game_socket_closes(1).unwrap()[0], work[0]);
    for w in work {
        s.acknowledge_game_socket_closed(&w).unwrap();
    }
    assert_eq!(s.list_due_game_socket_closes(100).unwrap().len(), 1);
}

#[test]
fn stale_close_identity_fields_never_modify_current_target() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    s.register_game_account_connection(&token, game(&rt), connection(&rt))
        .unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let original = s.list_due_game_socket_closes(1).unwrap().remove(0);
    for column in [
        "operation_id",
        "account_id",
        "session_id",
        "game_id",
        "credential_epoch",
        "expires_at",
    ] {
        let old = db
            .query(
                &format!("SELECT {column} FROM account_socket_close_work"),
                &[],
            )
            .unwrap()[0][0]
            .clone();
        let value = if matches!(column, "credential_epoch" | "expires_at") {
            match old {
                SqlValue::Integer(n) => SqlValue::Integer(n + 1),
                _ => panic!("Expected integer."),
            }
        } else {
            SqlValue::Text(id(&rt, 230))
        };
        db.execute(
            &format!("UPDATE account_socket_close_work SET {column}=?"),
            &[value],
        )
        .unwrap();
        let changed = db
            .query("SELECT * FROM account_socket_close_work", &[])
            .unwrap();
        assert_eq!(
            s.acknowledge_game_socket_closed(&original).unwrap_err(),
            AuthError::Conflict
        );
        assert_eq!(
            s.mark_game_socket_close_retry(&original).unwrap_err(),
            AuthError::Conflict
        );
        assert_eq!(
            db.query("SELECT * FROM account_socket_close_work", &[])
                .unwrap(),
            changed
        );
        db.execute(
            &format!("UPDATE account_socket_close_work SET {column}=?"),
            &[old],
        )
        .unwrap();
    }
}

#[test]
fn ignored_delete_is_not_a_close_ack_and_rolls_back_previous_deletion() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    s.register_game_account_connection(&token, game(&rt), connection(&rt))
        .unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let w = s.list_due_game_socket_closes(1).unwrap().remove(0);
    db.conn.borrow().execute_batch("CREATE TRIGGER ignore_ack BEFORE DELETE ON account_socket_subscriptions BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert_eq!(
        s.acknowledge_game_socket_closed(&w).unwrap_err(),
        AuthError::Storage
    );
    assert_eq!(db.query("SELECT (SELECT count(*) FROM account_socket_close_work),(SELECT count(*) FROM account_socket_subscriptions)",&[]).unwrap(),vec![vec![SqlValue::Integer(1),SqlValue::Integer(1)]]);
}

#[test]
fn clock_rollback_and_unavailable_storage_never_release_authority() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    s.authorize_game_account(&token).unwrap();
    rt.now.set(rt.now.get() - 1);
    assert_eq!(
        s.authorize_game_account(&token).unwrap_err(),
        AuthError::Crypto
    );
    rt.now.set(rt.now.get() + 1);
    db.conn
        .borrow()
        .execute_batch("DROP TABLE account_sessions")
        .unwrap();
    assert_eq!(
        s.authorize_game_account(&token).unwrap_err(),
        AuthError::Storage
    );
}

#[test]
fn pending_close_denies_frame_and_registration_even_when_account_session_is_live() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    db.execute("INSERT INTO account_socket_close_work(connection_id,operation_id,account_id,session_id,game_id,credential_epoch,expires_at,created_at,next_attempt_at,attempt_count) SELECT connection_id,?,account_id,session_id,game_id,credential_epoch,expires_at,?,?,0 FROM account_socket_subscriptions",&[SqlValue::Text(id(&rt,230)),SqlValue::Integer(rt.now.get()),SqlValue::Integer(rt.now.get())]).unwrap();
    s.authorize_game_account(&token).unwrap();
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.register_game_account_connection(&token, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        db.query("SELECT count(*) FROM account_socket_close_work", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
}

#[test]
fn trusted_close_ack_is_atomic_idempotent_and_cannot_delete_reused_connection() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, password) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let w = s.list_due_game_socket_closes(1).unwrap().remove(0);
    assert_eq!(
        s.unregister_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Conflict
    );
    s.acknowledge_game_socket_closed(&w).unwrap();
    assert_eq!(db.query("SELECT (SELECT count(*) FROM account_socket_close_work),(SELECT count(*) FROM account_socket_subscriptions)",&[]).unwrap(),vec![vec![SqlValue::Integer(0),SqlValue::Integer(0)]]);
    s.acknowledge_game_socket_closed(&w).unwrap();
    let second = bearer(
        s.execute(
            AuthCommand::Login {
                username: "GameHostPerson".into(),
                password,
            },
            context(&rt, 5),
        )
        .unwrap(),
    );
    let replacement = s.register_game_account_connection(&second, g, c).unwrap();
    assert_eq!(
        s.acknowledge_game_socket_closed(&w).unwrap_err(),
        AuthError::Conflict
    );
    assert_eq!(
        s.mark_game_socket_close_retry(&w).unwrap_err(),
        AuthError::Conflict
    );
    s.authorize_game_account_connection(account, replacement.session_id(), 1, g, c)
        .unwrap();
    s.execute(
        AuthCommand::Logout {
            token: Some(second),
        },
        context(&rt, 6),
    )
    .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let current = s.list_due_game_socket_closes(1).unwrap().remove(0);
    assert_ne!(current.operation_id(), w.operation_id());
    assert_eq!(
        s.acknowledge_game_socket_closed(&w).unwrap_err(),
        AuthError::Conflict
    );
    s.acknowledge_game_socket_closed(&current).unwrap();
}

#[test]
fn close_retry_is_durable_capped_and_saturates_count_and_deadline() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (_, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    s.register_game_account_connection(&token, g, c).unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    rt.now.set(rt.now.get() + 1000);
    let original = s.list_due_game_socket_closes(1).unwrap().remove(0);
    for count in 0..12u32 {
        let now = rt.now.get();
        s.mark_game_socket_close_retry(&original).unwrap();
        let expected = (1000i64 * (1i64 << count.min(9))).min(300_000);
        assert_eq!(
            db.query(
                "SELECT attempt_count,next_attempt_at FROM account_socket_close_work",
                &[]
            )
            .unwrap(),
            vec![vec![
                SqlValue::Integer(i64::from(count + 1)),
                SqlValue::Integer(now + expected)
            ]]
        );
        assert!(s.list_due_game_socket_closes(1).unwrap().is_empty());
        rt.now.set(now + expected);
    }
    db.execute(
        "UPDATE account_socket_close_work SET attempt_count=?",
        &[SqlValue::Integer(i64::from(u32::MAX))],
    )
    .unwrap();
    let max = 9_007_199_254_740_991;
    rt.now.set(max - 1);
    s.mark_game_socket_close_retry(&original).unwrap();
    assert_eq!(
        db.query(
            "SELECT attempt_count,next_attempt_at FROM account_socket_close_work",
            &[]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Integer(i64::from(u32::MAX)),
            SqlValue::Integer(max)
        ]]
    );
}

#[test]
fn due_close_work_is_bounded_and_preserves_exact_revoked_binding() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    assert!(s.list_due_game_socket_closes(1).unwrap().is_empty());
    let created = rt.now.get();
    rt.now.set(created + 1000);
    let work = s.list_due_game_socket_closes(1).unwrap();
    assert_eq!(work.len(), 1);
    let w = &work[0];
    assert_eq!(w.account_id(), account);
    assert_eq!(w.session_id(), p.session_id());
    assert_eq!(w.game_id(), g);
    assert_eq!(w.connection_id(), c);
    assert_eq!(w.credential_epoch(), 1);
    assert_eq!(w.expires_at(), p.expires_at());
    assert_eq!(w.created_at(), created);
    assert_eq!(w.next_attempt_at(), created + 1000);
    assert_eq!(w.attempt_count(), 0);
    assert_eq!(
        s.list_due_game_socket_closes(0).unwrap_err(),
        AuthError::InvalidInput
    );
    assert_eq!(
        s.list_due_game_socket_closes(101).unwrap_err(),
        AuthError::InvalidInput
    );
}

#[test]
fn exact_unregistration_cannot_remove_unrelated_or_replacement_binding() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    assert_eq!(
        s.unregister_game_account_connection(account, p.session_id(), 0, g, c)
            .unwrap_err(),
        AuthError::Conflict
    );
    s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
        .unwrap();
    s.unregister_game_account_connection(account, p.session_id(), 1, g, c)
        .unwrap();
    s.unregister_game_account_connection(account, p.session_id(), 1, g, c)
        .unwrap();
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    let replacement = id(&rt, 239).parse().unwrap();
    s.register_game_account_connection(&token, replacement, c)
        .unwrap();
    assert_eq!(
        s.unregister_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Conflict
    );
    s.authorize_game_account_connection(account, p.session_id(), 1, replacement, c)
        .unwrap();
}

#[test]
fn frame_authority_requires_exact_subscription_and_current_session() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, _) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), p.credential_epoch(), g, c)
            .unwrap()
            .expires_at(),
        p.expires_at()
    );
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 0, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.authorize_game_account_connection(
            account,
            p.session_id(),
            1,
            id(&rt, 239).parse().unwrap(),
            c
        )
        .unwrap_err(),
        AuthError::Unauthorized
    );
    assert_eq!(
        s.authorize_game_account_connection(
            account,
            p.session_id(),
            1,
            g,
            id(&rt, 242).parse().unwrap()
        )
        .unwrap_err(),
        AuthError::Unauthorized
    );
    s.execute(AuthCommand::Logout { token: Some(token) }, context(&rt, 4))
        .unwrap();
    assert_eq!(
        s.authorize_game_account_connection(account, p.session_id(), 1, g, c)
            .unwrap_err(),
        AuthError::Unauthorized
    );
}

#[test]
fn registration_is_exact_idempotent_and_preserves_original_expiry() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    let (account, token, password) = enrolled(&db, &rt, AccountRole::Host);
    let s = service(&db, &rt);
    let g = game(&rt);
    let c = connection(&rt);
    let p = s.register_game_account_connection(&token, g, c).unwrap();
    let original = p.expires_at();
    rt.now.set(rt.now.get() + 1000);
    assert_eq!(
        s.register_game_account_connection(&token, g, c)
            .unwrap()
            .expires_at(),
        original
    );
    assert_eq!(db.query("SELECT account_id,session_id,game_id,connection_id,credential_epoch,expires_at FROM account_socket_subscriptions",&[]).unwrap(),vec![vec![SqlValue::Text(account.to_string()),SqlValue::Text(p.session_id().to_string()),SqlValue::Text(g.to_string()),SqlValue::Text(c.to_string()),SqlValue::Integer(1),SqlValue::Integer(original)]]);
    assert_eq!(
        s.register_game_account_connection(&token, id(&rt, 239).parse().unwrap(), c)
            .unwrap_err(),
        AuthError::Conflict
    );
    let second = bearer(
        s.execute(
            AuthCommand::Login {
                username: "GameHostPerson".into(),
                password,
            },
            context(&rt, 4),
        )
        .unwrap(),
    );
    assert_eq!(
        s.register_game_account_connection(&second, g, c)
            .unwrap_err(),
        AuthError::Conflict
    );
    let independent = id(&rt, 242).parse().unwrap();
    s.register_game_account_connection(&second, g, independent)
        .unwrap();
    assert_eq!(
        db.query("SELECT count(*) FROM account_socket_subscriptions", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(2)]]
    );
}

#[test]
fn current_normal_host_and_admin_produce_private_game_authority() {
    for role in [AccountRole::Host, AccountRole::Admin] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        let (account, token, _) = enrolled(&db, &rt, role);
        let proof = service(&db, &rt).authorize_game_account(&token).unwrap();
        assert_eq!(proof.account_id(), account);
        assert_eq!(proof.role(), role);
        assert_eq!(proof.credential_epoch(), 1);
        assert_eq!(
            db.query(
                "SELECT session_id,expires_at FROM account_sessions WHERE scope=?",
                &[SqlValue::Text(SessionScope::Normal.to_string())]
            )
            .unwrap(),
            vec![vec![
                SqlValue::Text(proof.session_id().to_string()),
                SqlValue::Integer(proof.expires_at())
            ]]
        );
    }
}
