#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::{
    auth::{AuthPolicy, AuthService, ManagementError, ManagementPrincipal, Runtime},
    storage::{Database, SqlValue, migrate},
};
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::{
    accounts::AccountRole,
    ids::{AccountId, CommandId},
};
use support::{Sqlite, TestRuntime};
const ORIGIN: &str = "https://app.example.test";
const DAY_MS: i64 = 86_400_000;
fn key(rt: &TestRuntime) -> [u8; 32] {
    let mut key = [0; 32];
    rt.fill_random(&mut key).unwrap();
    key
}
fn command_id(rt: &TestRuntime, n: u8) -> CommandId {
    let mut bytes = [0; 16];
    bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[15] = n;
    uuid::Uuid::from_bytes(bytes).try_into().unwrap()
}
fn create(username: &str) -> ManagementCommand {
    ManagementCommand::CreateAccount {
        username: username.into(),
        role: AccountRole::Host,
    }
}
fn count(db: &Sqlite, table: &str) -> i64 {
    let rows = db
        .query(&format!("SELECT count(*) FROM {table}"), &[])
        .unwrap();
    let SqlValue::Integer(value) = rows[0][0] else {
        panic!("count missing")
    };
    value
}
fn enrolled_admin(db: &Sqlite, rt: &TestRuntime, key: &[u8]) -> (AccountId, String, String) {
    use brews_backend::auth::{AuthCommand, CookieEffect};
    use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
    let service = AuthService::new(db, rt, AuthPolicy::default(), key).unwrap();
    let ManagementResponse::Issued { receipt, url } = service
        .execute_management(
            ManagementCommand::CreateAccount {
                username: "AdminPerson01".into(),
                role: AccountRole::Admin,
            },
            ManagementPrincipal::DeveloperCli,
            Some(command_id(rt, 201)),
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("issuance missing")
    };
    let restricted = service
        .execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token: url.split('#').nth(1).unwrap().into(),
            },
            context(rt, 202),
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
                new_password: password.clone(),
            },
            context(rt, 203),
        )
        .unwrap();
    let CookieEffect::Set { token, .. } = normal.cookie else {
        panic!("normal session missing")
    };
    (receipt.account_id, token, password)
}
fn context(rt: &TestRuntime, n: u8) -> brews_backend::auth::RequestContext {
    brews_backend::auth::RequestContext {
        command_id: Some(command_id(rt, n).to_string()),
        caller_identity: "trusted-fixture".into(),
    }
}
#[test]
fn auth_completion_id_conflicts_with_management_for_the_same_proven_actor() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (admin, token, _) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let id = command_id(&rt, 203);
    assert_eq!(
        db.query(
            "SELECT count(*) FROM command_receipts WHERE actor_account_id=? AND command_id=?",
            &[
                SqlValue::Text(admin.to_string()),
                SqlValue::Text(id.to_string())
            ]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(1)]]
    );
    assert!(matches!(
        service.execute_management(
            create("HostPerson01"),
            ManagementPrincipal::AdminSession(token),
            Some(id),
            ORIGIN
        ),
        Err(ManagementError::Conflict)
    ));
    assert_eq!(count(&db, "accounts"), 1);
    assert_eq!(count(&db, "management_receipts"), 1);
}
#[test]
fn management_id_conflicts_with_auth_login_for_the_same_proven_actor() {
    use brews_backend::auth::{AuthCommand, AuthError};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (_, token, password) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let id = command_id(&rt, 1);
    let _ = service
        .execute_management(
            create("HostPerson01"),
            ManagementPrincipal::AdminSession(token),
            Some(id),
            ORIGIN,
        )
        .unwrap();
    let sessions = count(&db, "account_sessions");
    let receipts = count(&db, "command_receipts");
    assert!(matches!(
        service.execute(
            AuthCommand::Login {
                username: "AdminPerson01".into(),
                password
            },
            context(&rt, 1)
        ),
        Err(AuthError::Conflict)
    ));
    assert_eq!(count(&db, "account_sessions"), sessions);
    assert_eq!(count(&db, "command_receipts"), receipts);
}
#[test]
fn pending_management_id_conflicts_with_auth_login() {
    use brews_backend::auth::{AuthCommand, AuthError};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (_, token, password) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let ManagementResponse::Issued { receipt, .. } = service
        .execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(command_id(&rt, 2)),
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("issuance missing")
    };
    assert!(matches!(
        service
            .prepare_removal(
                ManagementCommand::DeleteAccount {
                    account_id: receipt.account_id
                },
                ManagementPrincipal::AdminSession(token),
                command_id(&rt, 1)
            )
            .unwrap(),
        ManagementResponse::Pending { .. }
    ));
    let sessions = count(&db, "account_sessions");
    let receipts = count(&db, "command_receipts");
    assert!(matches!(
        service.execute(
            AuthCommand::Login {
                username: "AdminPerson01".into(),
                password
            },
            context(&rt, 1)
        ),
        Err(AuthError::Conflict)
    ));
    assert_eq!(count(&db, "account_sessions"), sessions);
    assert_eq!(count(&db, "command_receipts"), receipts);
    assert_eq!(count(&db, "pending_account_removals"), 1);
}
#[test]
fn pending_retry_cannot_bypass_a_conflicting_auth_receipt_in_restored_state() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (_, token, _) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let ManagementResponse::Issued { receipt, .. } = service
        .execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(command_id(&rt, 2)),
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("issuance missing")
    };
    let command = ManagementCommand::DeleteAccount {
        account_id: receipt.account_id,
    };
    let ManagementResponse::Pending { operation_id } = service
        .prepare_removal(
            command.clone(),
            ManagementPrincipal::AdminSession(token.clone()),
            command_id(&rt, 1),
        )
        .unwrap()
    else {
        panic!("intent missing")
    };
    assert!(
        matches!(service.execute_management(command.clone(), ManagementPrincipal::AdminSession(token.clone()), Some(command_id(&rt, 1)), ORIGIN).unwrap(), ManagementResponse::Pending { operation_id: retry } if retry == operation_id)
    );
    assert!(matches!(
        service.execute_management(
            ManagementCommand::DisableAccount {
                account_id: receipt.account_id
            },
            ManagementPrincipal::AdminSession(token.clone()),
            Some(command_id(&rt, 1)),
            ORIGIN
        ),
        Err(ManagementError::Conflict)
    ));
    db.execute(
        "UPDATE pending_account_removals SET command_id=? WHERE operation_id=?",
        &[
            SqlValue::Text(command_id(&rt, 203).to_string()),
            SqlValue::Text(operation_id.to_string()),
        ],
    )
    .unwrap();
    assert!(matches!(
        service.execute_management(
            command.clone(),
            ManagementPrincipal::AdminSession(token.clone()),
            Some(command_id(&rt, 203)),
            ORIGIN
        ),
        Err(ManagementError::Conflict)
    ));
    assert!(matches!(
        service.prepare_removal(
            command,
            ManagementPrincipal::AdminSession(token),
            command_id(&rt, 203)
        ),
        Err(ManagementError::Conflict)
    ));
    assert_eq!(count(&db, "pending_account_removals"), 1);
    assert_eq!(count(&db, "accounts"), 2);
}
#[test]
fn unknown_two_day_old_issuance_id_is_stale_without_mutation() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let id = command_id(&rt, 1);
    rt.now.set(rt.now.get() + 2 * DAY_MS);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert!(matches!(
        service.execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(id),
            ORIGIN
        ),
        Err(ManagementError::StaleCommand)
    ));
    for table in ["accounts", "access_links", "management_receipts"] {
        assert_eq!(count(&db, table), 0);
    }
}

#[test]
fn purged_ordinary_id_cannot_be_reused_for_issuance_after_sqlite_reopen() {
    use std::{cell::RefCell, fs::OpenOptions, os::unix::fs::OpenOptionsExt};
    struct FileGuard(std::path::PathBuf);
    impl Drop for FileGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let rt = TestRuntime::new();
    let file = FileGuard(std::env::temp_dir().join(format!(
        "brews-ordinary-link-reuse-{}-{}.sqlite",
        std::process::id(),
        command_id(&rt, 220)
    )));
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
    let key = key(&rt);
    let reused = command_id(&rt, 2);
    {
        let db = open();
        migrate(&db).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let ManagementResponse::Issued { receipt, .. } = service
            .execute_management(
                create("HostPerson01"),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, 1)),
                ORIGIN,
            )
            .unwrap()
        else {
            panic!("issuance missing")
        };
        let _ = service
            .execute_management(
                ManagementCommand::EnableAccount {
                    account_id: receipt.account_id,
                },
                ManagementPrincipal::DeveloperCli,
                Some(reused),
                ORIGIN,
            )
            .unwrap();
        rt.now.set(rt.now.get() + 2 * DAY_MS);
        service.cleanup_management().unwrap();
        assert_eq!(
            db.query(
                "SELECT count(*) FROM management_receipts WHERE command_id=?",
                &[SqlValue::Text(reused.to_string())]
            )
            .unwrap(),
            vec![vec![SqlValue::Integer(0)]]
        );
        assert_eq!(
            db.query("SELECT command_floor_ms FROM storage_metadata", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(rt.now.get() - DAY_MS)]]
        );
    }
    let db = open();
    migrate(&db).unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert!(matches!(
        service.execute_management(
            create("HostPerson02"),
            ManagementPrincipal::DeveloperCli,
            Some(reused),
            ORIGIN
        ),
        Err(ManagementError::StaleCommand)
    ));
    assert_eq!(count(&db, "accounts"), 1);
    assert_eq!(count(&db, "access_links"), 1);
    assert_eq!(count(&db, "management_receipts"), 1);
}

#[test]
fn retained_issuance_receipts_replay_secret_free_until_the_thirty_day_deadline() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (admin, _, _) = enrolled_admin(&db, &rt, &key);
    let issued_at = rt.now.get();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let ManagementResponse::Issued { receipt, url } = service
        .execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(command_id(&rt, 1)),
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("issuance missing")
    };
    let target = receipt.account_id;
    let mut saved = vec![(create("HostPerson01"), receipt, url)];
    for (n, command) in [
        (
            2,
            ManagementCommand::ReissueEnrollment { account_id: target },
        ),
        (3, ManagementCommand::ResetPassword { account_id: admin }),
    ] {
        let ManagementResponse::Issued { receipt, url } = service
            .execute_management(
                command.clone(),
                ManagementPrincipal::DeveloperCli,
                Some(command_id(&rt, n)),
                ORIGIN,
            )
            .unwrap()
        else {
            panic!("issuance missing")
        };
        saved.push((command, receipt, url));
    }
    let accounts = count(&db, "accounts");
    let links = count(&db, "access_links");
    for age in [2 * DAY_MS, 30 * DAY_MS - 1] {
        rt.now.set(issued_at + age);
        service.cleanup_management().unwrap();
        let restarted = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        for (command, original, url) in &saved {
            let response = restarted
                .execute_management(
                    command.clone(),
                    ManagementPrincipal::DeveloperCli,
                    Some(original.command_id),
                    ORIGIN,
                )
                .unwrap();
            let raw = serde_json::to_string(&response).unwrap();
            assert!(!raw.contains("\"url\""));
            assert!(!raw.contains(url));
            assert!(!raw.contains(url.split('#').nth(1).unwrap()));
            let ManagementResponse::Committed { receipt } = response else {
                panic!("replay must be committed")
            };
            assert_eq!(
                serde_json::to_value(&receipt).unwrap(),
                serde_json::to_value(original).unwrap()
            );
            assert!(matches!(
                restarted.execute_management(
                    ManagementCommand::EnableAccount { account_id: target },
                    ManagementPrincipal::DeveloperCli,
                    Some(original.command_id),
                    ORIGIN
                ),
                Err(ManagementError::Conflict)
            ));
        }
        assert_eq!(count(&db, "accounts"), accounts);
        assert_eq!(count(&db, "access_links"), links);
    }
    rt.now.set(issued_at + 30 * DAY_MS);
    for (command, receipt, _) in &saved {
        assert!(matches!(
            service.execute_management(
                command.clone(),
                ManagementPrincipal::DeveloperCli,
                Some(receipt.command_id),
                ORIGIN
            ),
            Err(ManagementError::StaleCommand)
        ));
    }
    service.cleanup_management().unwrap();
    for (command, receipt, _) in saved {
        assert!(matches!(
            service.execute_management(
                command,
                ManagementPrincipal::DeveloperCli,
                Some(receipt.command_id),
                ORIGIN
            ),
            Err(ManagementError::StaleCommand)
        ));
    }
}

#[test]
fn all_unknown_issuance_operations_use_the_exact_common_admission_floor() {
    for offset in [-DAY_MS - 1, -DAY_MS, -DAY_MS + 1, 0, 1] {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate(&db).unwrap();
        let key = key(&rt);
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
        let now = rt.now.get();
        rt.now.set(now + offset);
        let id = command_id(&rt, 1);
        rt.now.set(now);
        let result = service.execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(id),
            ORIGIN,
        );
        if offset > -DAY_MS && offset <= 0 {
            assert!(matches!(result, Ok(ManagementResponse::Issued { .. })));
        } else {
            assert!(matches!(result, Err(ManagementError::StaleCommand)));
            assert_eq!(count(&db, "accounts"), 0);
            let unknown: AccountId = command_id(&rt, 9).to_string().parse().unwrap();
            for command in [
                ManagementCommand::ReissueEnrollment {
                    account_id: unknown,
                },
                ManagementCommand::ResetPassword {
                    account_id: unknown,
                },
            ] {
                assert!(matches!(
                    service.execute_management(
                        command,
                        ManagementPrincipal::DeveloperCli,
                        Some(id),
                        ORIGIN
                    ),
                    Err(ManagementError::StaleCommand)
                ));
            }
        }
    }
}

#[test]
fn restored_ahead_floor_is_not_projected_back_for_unknown_issuance() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let id = command_id(&rt, 1);
    db.execute(
        "UPDATE storage_metadata SET command_floor_ms=?",
        &[SqlValue::Integer(rt.now.get() + 1)],
    )
    .unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    assert!(matches!(
        service.execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(id),
            ORIGIN
        ),
        Err(ManagementError::StaleCommand)
    ));
    assert_eq!(
        db.query("SELECT command_floor_ms FROM storage_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(rt.now.get() + 1)]]
    );
    assert_eq!(count(&db, "accounts"), 0);
}

#[test]
fn developer_cli_and_account_actors_have_distinct_command_namespaces() {
    use brews_backend::auth::{AuthCommand, CookieEffect};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (_, token, password) = enrolled_admin(&db, &rt, &key);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let _ = service
        .execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(command_id(&rt, 203)),
            ORIGIN,
        )
        .unwrap();
    let login = service
        .execute(
            AuthCommand::Login {
                username: "AdminPerson01".into(),
                password,
            },
            context(&rt, 201),
        )
        .unwrap();
    assert!(matches!(login.cookie, CookieEffect::Set { .. }));
    let _ = service
        .execute_management(
            create("HostPerson02"),
            ManagementPrincipal::DeveloperCli,
            Some(command_id(&rt, 1)),
            ORIGIN,
        )
        .unwrap();
    let _ = service
        .execute_management(
            create("HostPerson03"),
            ManagementPrincipal::AdminSession(token.clone()),
            Some(command_id(&rt, 1)),
            ORIGIN,
        )
        .unwrap();
    assert!(matches!(
        service.execute_management(
            create("HostPerson04"),
            ManagementPrincipal::AdminSession(token),
            Some(command_id(&rt, 203)),
            ORIGIN
        ),
        Err(ManagementError::Conflict)
    ));
    assert_eq!(count(&db, "accounts"), 4);
}

#[test]
fn completion_rechecks_reciprocal_receipts_after_password_hashing() {
    use brews_backend::auth::{AuthCommand, AuthError, CookieEffect};
    use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
    use std::cell::Cell;
    struct InterleavingRuntime<'a> {
        runtime: &'a TestRuntime,
        db: &'a Sqlite,
        account: AccountId,
        command: CommandId,
        inject: Cell<bool>,
    }
    impl Runtime for InterleavingRuntime<'_> {
        fn now_ms(&self) -> i64 {
            self.runtime.now_ms()
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
            self.runtime.fill_random(bytes)?;
            if self.inject.replace(false) {
                // Restore conflicting persisted evidence during the KDF gap.
                self.db.execute(
                    "UPDATE management_receipts SET actor=? WHERE command_id=?",
                    &[
                        SqlValue::Text(self.account.to_string()),
                        SqlValue::Text(self.command.to_string()),
                    ],
                )?;
            }
            Ok(())
        }
    }
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let command = command_id(&rt, 1);
    let ManagementResponse::Issued { receipt, url } = service
        .execute_management(
            create("HostPerson01"),
            ManagementPrincipal::DeveloperCli,
            Some(command),
            ORIGIN,
        )
        .unwrap()
    else {
        panic!("issuance missing")
    };
    let restricted = service
        .execute(
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token: url.split('#').nth(1).unwrap().into(),
            },
            context(&rt, 2),
        )
        .unwrap();
    let CookieEffect::Set { token, .. } = restricted.cookie else {
        panic!("restricted session missing")
    };
    let password = brews_backend::security::new_token(&rt).unwrap();
    let interleaving = InterleavingRuntime {
        runtime: &rt,
        db: &db,
        account: receipt.account_id,
        command,
        inject: Cell::new(true),
    };
    let service = AuthService::new(&db, &interleaving, AuthPolicy::default(), &key).unwrap();
    assert!(matches!(
        service.execute(
            AuthCommand::Complete {
                scope: SessionScope::EnrollmentOnly,
                token,
                new_password: password
            },
            context(&rt, 1)
        ),
        Err(AuthError::Conflict)
    ));
    assert_eq!(
        db.query(
            "SELECT status,credential_epoch,verifier FROM accounts WHERE account_id=?",
            &[SqlValue::Text(receipt.account_id.to_string())]
        )
        .unwrap(),
        vec![vec![
            SqlValue::Text("pending_enrollment".into()),
            SqlValue::Integer(0),
            SqlValue::Null
        ]]
    );
    assert_eq!(count(&db, "command_receipts"), 1);
    assert_eq!(count(&db, "account_sessions"), 1);
}

#[test]
fn cross_operation_command_knowledge_never_substitutes_for_actor_proof() {
    use brews_backend::auth::{AuthCommand, AuthError};
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate(&db).unwrap();
    let key = key(&rt);
    let (_, token, _) = enrolled_admin(&db, &rt, &key);
    let wrong = brews_backend::security::new_token(&rt).unwrap();
    let service = AuthService::new(&db, &rt, AuthPolicy::default(), &key).unwrap();
    let _ = service
        .execute_management(
            create("HostPerson01"),
            ManagementPrincipal::AdminSession(token),
            Some(command_id(&rt, 1)),
            ORIGIN,
        )
        .unwrap();
    assert!(matches!(
        service.execute(
            AuthCommand::Login {
                username: "AdminPerson01".into(),
                password: wrong.clone()
            },
            context(&rt, 1)
        ),
        Err(AuthError::InvalidCredentials)
    ));
    assert!(matches!(
        service.execute_management(
            create("HostPerson02"),
            ManagementPrincipal::AdminSession(wrong),
            Some(command_id(&rt, 203)),
            ORIGIN
        ),
        Err(ManagementError::Unauthorized)
    ));
    assert_eq!(count(&db, "accounts"), 2);
}
