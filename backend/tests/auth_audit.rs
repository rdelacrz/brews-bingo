#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Audit fixtures fail fast."
)]
mod support;
use brews_backend::{
    auth::{
        AuthCommand, AuthError, AuthOutcome, AuthPolicy, AuthService, CookieEffect,
        ManagementError, ManagementPrincipal, RequestContext, Runtime,
    },
    db::{Database, Row, SqlValue, StorageError, migrate},
};
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::{
    accounts::{AccessLinkPurpose, AccountRole, SessionScope},
    ids::{AccountId, CommandId, ConnectionId, GameId},
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use support::{Sqlite, TestRuntime};

const CLOCK_READ: &str = "SELECT last_observed_ms FROM storage_metadata WHERE singleton=1";
const CLOCK_READBACK: &str =
    "SELECT last_observed_ms,command_floor_ms FROM storage_metadata WHERE singleton=1";
const ORIGIN: &str = "https://app.example.test";
const USERNAME: &str = "AuditPerson01";
const SCOPES: [SessionScope; 2] = [
    SessionScope::EnrollmentOnly,
    SessionScope::PasswordResetOnly,
];

struct ScriptedRuntime {
    base: TestRuntime,
    samples: RefCell<VecDeque<i64>>,
}
impl ScriptedRuntime {
    fn new() -> Self {
        Self {
            base: TestRuntime::new(),
            samples: RefCell::new(VecDeque::new()),
        }
    }
    fn script(&self, samples: &[i64]) {
        *self.samples.borrow_mut() = samples.iter().copied().collect();
    }
}
impl Runtime for ScriptedRuntime {
    fn now_ms(&self) -> i64 {
        if let Some(now) = self.samples.borrow_mut().pop_front() {
            self.base.now.set(now);
        }
        self.base.now.get()
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        self.base.fill_random(bytes)
    }
}

#[derive(Clone, Copy)]
enum Fault {
    IgnoreClock,
    AbortClock,
    RegressClock,
    RegressFloor,
    ReadbackError,
    ReadbackMalformed,
    RateWrite,
}
struct AuditDb {
    inner: Sqlite,
    fault: Cell<Option<(Fault, i64)>>,
    mutate_after_clock_reads: Cell<usize>,
    mutation: RefCell<Option<&'static str>>,
}
impl AuditDb {
    fn new() -> Self {
        Self {
            inner: Sqlite::new(),
            fault: Cell::new(None),
            mutate_after_clock_reads: Cell::new(0),
            mutation: RefCell::new(None),
        }
    }
    fn mutate_after_proof(&self, sql: &'static str) {
        // execute fence, eligibility transaction and rate preflight use five samples.
        self.mutate_after_clock_reads.set(6);
        *self.mutation.borrow_mut() = Some(sql);
    }
}
impl Database for AuditDb {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        if sql == CLOCK_READ && self.mutate_after_clock_reads.get() > 0 {
            let remaining = self.mutate_after_clock_reads.get() - 1;
            self.mutate_after_clock_reads.set(remaining);
            if remaining == 0
                && let Some(mutation) = self.mutation.borrow_mut().take()
            {
                self.inner.execute(mutation, &[])?;
            }
        }
        let rows = self.inner.query(sql, params)?;
        if sql == CLOCK_READBACK
            && let Some((fault, cutoff)) = self.fault.get()
            && matches!(rows[0][0], SqlValue::Integer(now) if now >= cutoff)
        {
            match fault {
                Fault::ReadbackError => return Err(StorageError),
                Fault::ReadbackMalformed => return Ok(vec![]),
                _ => {}
            }
        }
        Ok(rows)
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        if let Some((fault, cutoff)) = self.fault.get() {
            if matches!(fault, Fault::RateWrite)
                && sql.starts_with("INSERT INTO rate_limit_buckets")
            {
                return Err(StorageError);
            }
            if sql.starts_with("UPDATE storage_metadata SET last_observed_ms=")
                && matches!(params[0], SqlValue::Integer(now) if now >= cutoff)
            {
                match fault {
                    Fault::IgnoreClock => return Ok(()),
                    Fault::AbortClock => return Err(StorageError),
                    Fault::RegressClock => {
                        let old = self.inner.query(CLOCK_READ, &[])?[0][0].clone();
                        self.inner.execute(sql, params)?;
                        return self.inner.execute(
                            "UPDATE storage_metadata SET last_observed_ms=? WHERE singleton=1",
                            &[old],
                        );
                    }
                    Fault::RegressFloor => {
                        self.inner.execute(sql, params)?;
                        return self.inner.execute(
                            "UPDATE storage_metadata SET command_floor_ms=0 WHERE singleton=1",
                            &[],
                        );
                    }
                    _ => {}
                }
            }
        }
        self.inner.execute(sql, params)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.inner.transaction(operation)
    }
}
fn service<'a>(
    db: &'a AuditDb,
    rt: &'a ScriptedRuntime,
) -> AuthService<'a, AuditDb, ScriptedRuntime> {
    AuthService::new(db, rt, AuthPolicy::default(), &[9; 32]).unwrap()
}
fn id(rt: &ScriptedRuntime, n: u8) -> CommandId {
    let mut bytes = [0; 16];
    bytes[..6].copy_from_slice(&(rt.base.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes).try_into().unwrap()
}
fn context(rt: &ScriptedRuntime, n: u8) -> RequestContext {
    RequestContext {
        command_id: Some(id(rt, n).to_string()),
        caller_identity: "audit-fixture".into(),
    }
}
fn fence(db: &AuditDb) -> i64 {
    let SqlValue::Integer(now) = db.inner.query(CLOCK_READ, &[]).unwrap()[0][0] else {
        panic!("missing clock fence")
    };
    now
}
fn seed(db: &AuditDb, rt: &ScriptedRuntime) -> (String, i64) {
    migrate(db).unwrap();
    let account: AccountId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let token = brews_backend::security::new_token(rt).unwrap();
    let digest = brews_backend::security::token_digest(&token).unwrap();
    let now = rt.base.now.get();
    let expiry = now + 1000;
    db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at) VALUES(?,?,'admin','verified',?,0,?,?)", &[SqlValue::Text(account.to_string()), SqlValue::Text(USERNAME.into()), SqlValue::Text("non-login-fixture".into()), SqlValue::Integer(now), SqlValue::Integer(now)]).unwrap();
    db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at) VALUES(?, ?, ?, 'normal',0,?,?)", &[SqlValue::Text("01900000-0000-7000-8000-000000000002".into()), SqlValue::Text(account.to_string()), SqlValue::Blob(digest.to_vec()), SqlValue::Integer(now), SqlValue::Integer(expiry)]).unwrap();
    (token, expiry)
}
#[derive(Clone, Copy)]
enum Path {
    Current,
    Game,
    Register,
    Connection,
    Admin,
}
const PATHS: [Path; 5] = [
    Path::Current,
    Path::Game,
    Path::Register,
    Path::Connection,
    Path::Admin,
];
fn game_id() -> GameId {
    "01900000-0000-7000-8000-000000000003".parse().unwrap()
}
fn connection_id() -> ConnectionId {
    "01900000-0000-7000-8000-000000000004".parse().unwrap()
}
fn authorize(
    db: &AuditDb,
    rt: &ScriptedRuntime,
    token: &str,
    path: Path,
) -> Result<bool, AuthError> {
    let svc = service(db, rt);
    match path {
        Path::Current => svc
            .execute(
                AuthCommand::Current {
                    token: Some(token.into()),
                },
                context(rt, 1),
            )
            .map(|out| out.body["authenticated"] == true),
        Path::Game => svc.authorize_game_account(token).map(|_| true),
        Path::Register => svc
            .register_game_account_connection(token, game_id(), connection_id())
            .map(|_| true),
        Path::Connection => svc
            .authorize_game_account_connection(
                "01900000-0000-7000-8000-000000000001".parse().unwrap(),
                "01900000-0000-7000-8000-000000000002".parse().unwrap(),
                0,
                game_id(),
                connection_id(),
            )
            .map(|_| true),
        Path::Admin => svc
            .execute_management(
                ManagementCommand::ListAccounts {
                    after: None,
                    limit: 1,
                },
                ManagementPrincipal::AdminSession(token.into()),
                None,
                ORIGIN,
            )
            .map(|_| true)
            .map_err(|error| match error {
                ManagementError::Unauthorized => AuthError::Unauthorized,
                ManagementError::Storage => AuthError::Storage,
                ManagementError::Crypto => AuthError::Crypto,
                _ => panic!("unexpected management error"),
            }),
    }
}
fn prepare_path(db: &AuditDb, rt: &ScriptedRuntime, token: &str, path: Path, expiry: i64) {
    if matches!(path, Path::Connection) {
        service(db, rt)
            .register_game_account_connection(token, game_id(), connection_id())
            .unwrap();
    }
    let samples: &[i64] = match path {
        Path::Current => &[expiry - 1, expiry - 1, expiry],
        Path::Admin => &[expiry - 1, expiry - 1, expiry, expiry],
        _ => &[expiry - 1, expiry],
    };
    rt.script(samples);
}
#[test]
fn decision_sample_fences_current_game_connection_and_admin_expiry_denials() {
    for path in PATHS {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        let (token, expiry) = seed(&db, &rt);
        prepare_path(&db, &rt, &token, path, expiry);
        let result = authorize(&db, &rt, &token, path);
        if matches!(path, Path::Current) {
            assert_eq!(result, Ok(false));
        } else {
            assert_eq!(result, Err(AuthError::Unauthorized));
        }
        assert_eq!(fence(&db), expiry);
        rt.script(&[]);
        rt.base.now.set(expiry - 1);
        assert_eq!(authorize(&db, &rt, &token, path), Err(AuthError::Crypto));
        rt.base.now.set(expiry);
        let result = authorize(&db, &rt, &token, path);
        assert!(matches!(result, Ok(false) | Err(AuthError::Unauthorized)));
    }
}
#[test]
fn ignored_clock_writes_and_readback_faults_fail_closed_on_all_auth_paths() {
    for path in PATHS {
        for fault in [
            Fault::IgnoreClock,
            Fault::AbortClock,
            Fault::RegressClock,
            Fault::RegressFloor,
            Fault::ReadbackError,
            Fault::ReadbackMalformed,
        ] {
            let db = AuditDb::new();
            let rt = ScriptedRuntime::new();
            let (token, expiry) = seed(&db, &rt);
            prepare_path(&db, &rt, &token, path, expiry);
            db.fault.set(Some((fault, expiry)));
            assert_eq!(authorize(&db, &rt, &token, path), Err(AuthError::Storage));
            // Fatal persistence failures roll back the transaction, including the earlier sample.
            assert!(fence(&db) < expiry);
        }
    }
}
struct Completed {
    token: String,
    password: String,
    command: String,
    expiry: i64,
    scope: SessionScope,
}
impl Completed {
    fn retry(
        &self,
        db: &AuditDb,
        rt: &ScriptedRuntime,
        password: &str,
        caller: &str,
    ) -> Result<AuthOutcome, AuthError> {
        service(db, rt).execute(
            AuthCommand::Complete {
                scope: self.scope,
                token: self.token.clone(),
                new_password: password.into(),
            },
            RequestContext {
                command_id: Some(self.command.clone()),
                caller_identity: caller.into(),
            },
        )
    }
}
fn completed(db: &AuditDb, rt: &ScriptedRuntime, scope: SessionScope) -> Completed {
    migrate(db).unwrap();
    let svc = service(db, rt);
    let ManagementResponse::Issued { receipt, url } = svc
        .execute_management(
            ManagementCommand::CreateAccount {
                username: USERNAME.into(),
                role: AccountRole::Host,
            },
            ManagementPrincipal::DeveloperCli,
            Some(id(rt, 1)),
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("missing link")
    };
    let mut purpose = AccessLinkPurpose::Enrollment;
    let mut link = url;
    if scope == SessionScope::PasswordResetOnly {
        let out = svc
            .execute(
                AuthCommand::Redeem {
                    purpose,
                    token: link.split_once('#').unwrap().1.into(),
                },
                context(rt, 2),
            )
            .unwrap();
        let CookieEffect::Set { token, .. } = out.cookie else {
            panic!("missing cookie")
        };
        let initial_password = brews_backend::security::new_token(rt).unwrap();
        svc.execute(
            AuthCommand::Complete {
                scope: SessionScope::EnrollmentOnly,
                token,
                new_password: initial_password,
            },
            context(rt, 3),
        )
        .unwrap();
        let ManagementResponse::Issued { url, .. } = svc
            .execute_management(
                ManagementCommand::ResetPassword {
                    account_id: receipt.account_id,
                },
                ManagementPrincipal::DeveloperCli,
                Some(id(rt, 4)),
                ORIGIN,
            )
            .unwrap()
        else {
            panic!("missing reset link")
        };
        purpose = AccessLinkPurpose::PasswordReset;
        link = url;
    }
    let out = svc
        .execute(
            AuthCommand::Redeem {
                purpose,
                token: link.split_once('#').unwrap().1.into(),
            },
            context(rt, 5),
        )
        .unwrap();
    let CookieEffect::Set { token, expires_at } = out.cookie else {
        panic!("missing restricted cookie")
    };
    let password = brews_backend::security::new_token(rt).unwrap();
    rt.base.now.set(rt.base.now.get() + 100);
    let command = id(rt, 6).to_string();
    let out = svc
        .execute(
            AuthCommand::Complete {
                scope,
                token: token.clone(),
                new_password: password.clone(),
            },
            context(rt, 6),
        )
        .unwrap();
    match scope {
        SessionScope::EnrollmentOnly => assert!(
            matches!(out.cookie, CookieEffect::Set { expires_at: deadline, .. } if deadline == expires_at)
        ),
        SessionScope::PasswordResetOnly => assert!(matches!(out.cookie, CookieEffect::Clear)),
        SessionScope::Normal => unreachable!(),
    }
    Completed {
        token,
        password,
        command,
        expiry: expires_at,
        scope,
    }
}
#[test]
fn retired_enrollment_and_reset_retry_recheck_expiry_after_password_proof() {
    for scope in SCOPES {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        let fixture = completed(&db, &rt, scope);
        rt.script(&[
            fixture.expiry - 1,
            fixture.expiry - 1,
            fixture.expiry - 1,
            fixture.expiry - 1,
            fixture.expiry - 1,
            fixture.expiry - 1,
            fixture.expiry,
        ]);
        assert_eq!(
            fixture
                .retry(&db, &rt, &fixture.password, "audit-fixture")
                .unwrap_err(),
            AuthError::Unauthorized
        );
        assert_eq!(fence(&db), fixture.expiry);
        rt.base.now.set(fixture.expiry - 1);
        assert_eq!(
            fixture
                .retry(&db, &rt, &fixture.password, "audit-fixture")
                .unwrap_err(),
            AuthError::Crypto
        );
    }
}
#[test]
fn retired_enrollment_and_reset_retry_cannot_disclose_receipts_when_clock_fencing_fails() {
    for scope in SCOPES {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        let fixture = completed(&db, &rt, scope);
        for fault in [
            Fault::IgnoreClock,
            Fault::AbortClock,
            Fault::RegressClock,
            Fault::RegressFloor,
            Fault::ReadbackError,
            Fault::ReadbackMalformed,
        ] {
            rt.script(&[
                fixture.expiry - 1,
                fixture.expiry - 1,
                fixture.expiry - 1,
                fixture.expiry - 1,
                fixture.expiry - 1,
                fixture.expiry - 1,
                fixture.expiry,
            ]);
            db.fault.set(Some((fault, fixture.expiry)));
            assert_eq!(
                fixture
                    .retry(&db, &rt, &fixture.password, "audit-fixture")
                    .unwrap_err(),
                AuthError::Storage
            );
            assert!(fence(&db) < fixture.expiry);
        }
    }
}
#[test]
fn retired_completion_retry_rechecks_exact_session_and_account_binding_after_proof() {
    for scope in SCOPES {
        for mutation in [
            "DELETE FROM account_sessions",
            "UPDATE account_sessions SET revoked_at=revoked_at+1",
            "UPDATE account_sessions SET expires_at=expires_at-1",
            "UPDATE account_sessions SET credential_epoch=credential_epoch+1",
            "UPDATE account_sessions SET scope='normal'",
            "UPDATE accounts SET credential_epoch=credential_epoch+1",
            "UPDATE accounts SET disabled_at=created_at",
            "UPDATE accounts SET status='reset_required'",
            "UPDATE accounts SET verifier='non-login-fixture'",
        ] {
            let db = AuditDb::new();
            let rt = ScriptedRuntime::new();
            let fixture = completed(&db, &rt, scope);
            db.mutate_after_proof(mutation);
            assert_eq!(
                fixture
                    .retry(&db, &rt, &fixture.password, "audit-fixture")
                    .unwrap_err(),
                AuthError::Unauthorized,
                "mutation: {mutation}"
            );
            assert!(db.mutation.borrow().is_none());
        }
    }
}
#[test]
fn redemption_expiry_decision_is_fenced_before_rollback() {
    for fault in [None, Some(Fault::IgnoreClock), Some(Fault::ReadbackError)] {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        migrate(&db).unwrap();
        let ManagementResponse::Issued { receipt, url } = service(&db, &rt)
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: USERNAME.into(),
                    role: AccountRole::Host,
                },
                ManagementPrincipal::DeveloperCli,
                Some(id(&rt, 1)),
                ORIGIN,
            )
            .unwrap()
        else {
            panic!("missing link");
        };
        let expires = receipt.link_expires_at.unwrap();
        let token = url.split_once('#').unwrap().1.to_owned();
        let command = id(&rt, 2).to_string();
        let redeem = || {
            service(&db, &rt).execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token: token.clone(),
                },
                RequestContext {
                    command_id: Some(command.clone()),
                    caller_identity: "audit-fixture".into(),
                },
            )
        };
        db.fault.set(fault.map(|fault| (fault, expires)));
        rt.script(&[expires - 1, expires - 1, expires]);
        assert_eq!(
            redeem().unwrap_err(),
            if fault.is_some() {
                AuthError::Storage
            } else {
                AuthError::InvalidLink
            }
        );
        if fault.is_some() {
            assert_eq!(fence(&db), expires - 1);
            db.fault.set(None);
            assert_eq!(redeem().unwrap_err(), AuthError::InvalidLink);
        }
        assert_eq!(fence(&db), expires);
        rt.base.now.set(expires - 1);
        assert_eq!(redeem().unwrap_err(), AuthError::Crypto);
        assert!(
            db.query("SELECT session_id FROM account_sessions", &[])
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn login_receipt_expiry_decision_is_fenced_before_rollback() {
    let db = AuditDb::new();
    let rt = ScriptedRuntime::new();
    let fixture = completed(&db, &rt, SessionScope::EnrollmentOnly);
    let command = id(&rt, 8).to_string();
    let attempt = || {
        service(&db, &rt).execute(
            AuthCommand::Login {
                username: USERNAME.into(),
                password: fixture.password.clone(),
            },
            RequestContext {
                command_id: Some(command.clone()),
                caller_identity: "audit-fixture".into(),
            },
        )
    };
    attempt().unwrap();
    let rows = db
        .query(
            "SELECT expires_at FROM command_receipts WHERE command_id=?",
            &[SqlValue::Text(command.clone())],
        )
        .unwrap();
    let SqlValue::Integer(expires) = rows[0][0] else {
        panic!("missing receipt expiry");
    };
    rt.script(&[expires - 1, expires - 1, expires - 1, expires - 1, expires]);
    assert_eq!(attempt().unwrap_err(), AuthError::StaleCommand);
    assert_eq!(fence(&db), expires);
    rt.base.now.set(expires - 1);
    assert_eq!(attempt().unwrap_err(), AuthError::Crypto);
}

fn login(
    db: &AuditDb,
    rt: &ScriptedRuntime,
    username: &str,
    password: &str,
    caller: &str,
    n: u8,
) -> Result<AuthOutcome, AuthError> {
    service(db, rt).execute(
        AuthCommand::Login {
            username: username.into(),
            password: password.into(),
        },
        RequestContext {
            command_id: Some(id(rt, n).to_string()),
            caller_identity: caller.into(),
        },
    )
}
#[test]
fn retired_enrollment_and_reset_bad_password_proofs_share_login_account_and_caller_throttles() {
    for scope in SCOPES {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        let fixture = completed(&db, &rt, scope);
        let wrong = brews_backend::security::new_token(&rt).unwrap();
        for _ in 0..5 {
            assert_eq!(
                fixture
                    .retry(&db, &rt, &wrong, "audit-fixture")
                    .unwrap_err(),
                AuthError::Unauthorized
            );
        }
        let buckets = db.query("SELECT attempt_count,blocked_until IS NOT NULL FROM rate_limit_buckets WHERE scope='account_login'", &[]).unwrap();
        assert_eq!(
            buckets,
            vec![vec![SqlValue::Integer(5), SqlValue::Integer(1)]; 2]
        );
        // Same account/different caller, and same caller/different account, both block.
        assert_eq!(
            fixture
                .retry(&db, &rt, &fixture.password, "other-fixture")
                .unwrap_err(),
            AuthError::RateLimited
        );
        assert_eq!(
            login(&db, &rt, USERNAME, &fixture.password, "other-fixture", 20).unwrap_err(),
            AuthError::RateLimited
        );
        assert_eq!(
            login(&db, &rt, "OtherAccount1", &wrong, "audit-fixture", 21).unwrap_err(),
            AuthError::RateLimited
        );
        assert_eq!(
            fixture
                .retry(&db, &rt, &fixture.password, "audit-fixture")
                .unwrap_err(),
            AuthError::RateLimited
        );
        rt.base.now.set(rt.base.now.get() + 900_000);
        let replay = fixture
            .retry(&db, &rt, &fixture.password, "audit-fixture")
            .unwrap();
        assert_eq!(replay.body["replayed"], true);
        assert!(matches!(replay.cookie, CookieEffect::None));
        let receipt = serde_json::to_string(&replay.body).unwrap();
        assert!(!receipt.contains(&fixture.password));
        assert!(!receipt.contains(&fixture.token));
        let SqlValue::Integer(count) = db
            .query(
                "SELECT count(*) FROM rate_limit_buckets WHERE scope='account_login'",
                &[],
            )
            .unwrap()[0][0]
        else {
            panic!("count missing")
        };
        assert_eq!(count, 0);
    }
}
#[test]
fn login_failures_block_retired_completion_proof_in_both_directions() {
    for scope in SCOPES {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        let fixture = completed(&db, &rt, scope);
        let wrong = brews_backend::security::new_token(&rt).unwrap();
        for n in 20..25 {
            assert_eq!(
                login(&db, &rt, USERNAME, &wrong, "audit-fixture", n).unwrap_err(),
                AuthError::InvalidCredentials
            );
        }
        assert_eq!(
            fixture
                .retry(&db, &rt, &fixture.password, "audit-fixture")
                .unwrap_err(),
            AuthError::RateLimited
        );
    }
}
#[test]
fn retired_completion_bad_proof_does_not_swallow_rate_storage_failure() {
    for scope in SCOPES {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        let fixture = completed(&db, &rt, scope);
        let wrong = brews_backend::security::new_token(&rt).unwrap();
        db.fault.set(Some((Fault::RateWrite, 0)));
        assert_eq!(
            fixture
                .retry(&db, &rt, &wrong, "audit-fixture")
                .unwrap_err(),
            AuthError::Storage
        );
        assert!(
            db.query(
                "SELECT scope FROM rate_limit_buckets WHERE scope='account_login'",
                &[]
            )
            .unwrap()
            .is_empty()
        );
    }
}
#[test]
fn retired_completion_rechecks_login_block_after_password_proof() {
    for scope in SCOPES {
        let db = AuditDb::new();
        let rt = ScriptedRuntime::new();
        let fixture = completed(&db, &rt, scope);
        let wrong = brews_backend::security::new_token(&rt).unwrap();
        assert_eq!(
            fixture
                .retry(&db, &rt, &wrong, "audit-fixture")
                .unwrap_err(),
            AuthError::Unauthorized
        );
        db.mutate_after_proof("UPDATE rate_limit_buckets SET blocked_until=expires_at");
        assert_eq!(
            fixture
                .retry(&db, &rt, &fixture.password, "audit-fixture")
                .unwrap_err(),
            AuthError::RateLimited
        );
    }
}
