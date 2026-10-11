//! Accounts-owned denial fence for expiry sampled by a private discovery caller.
use super::{Database, SqlValue, StorageError};
use crate::{auth::GameAccountAuthority, limits::JS_SAFE_INTEGER_MAX};
use brews_domain::accounts::SessionScope;

const SQL_MATCH_EXPIRY_SESSION: &str = "SELECT s.session_id FROM account_sessions s JOIN accounts a ON a.account_id=s.account_id WHERE s.token_verifier=? AND s.session_id=? AND s.account_id=? AND s.scope=? AND s.credential_epoch=? AND s.expires_at=? AND a.role=?";
const SQL_READ_EXPIRY_FENCE: &str =
    "SELECT last_observed_ms FROM storage_metadata WHERE singleton=1";
const SQL_WRITE_EXPIRY_FENCE: &str =
    "UPDATE storage_metadata SET last_observed_ms=? WHERE singleton=1 AND last_observed_ms=?";

pub(crate) fn fence_account_expiry<D: Database>(
    db: &D,
    token_digest: &[u8],
    proof: &GameAccountAuthority,
    observed_at: i64,
) -> Result<(), StorageError> {
    if !(proof.expires_at()..=JS_SAFE_INTEGER_MAX).contains(&observed_at) {
        return Err(StorageError);
    }
    db.transaction(|| {
        let rows = db.query(
            SQL_MATCH_EXPIRY_SESSION,
            &[
                SqlValue::Blob(token_digest.to_vec()),
                SqlValue::Text(proof.session_id().to_string()),
                SqlValue::Text(proof.account_id().to_string()),
                SqlValue::Text(SessionScope::Normal.to_string()),
                SqlValue::Integer(proof.credential_epoch()),
                SqlValue::Integer(proof.expires_at()),
                SqlValue::Text(proof.role().to_string()),
            ],
        )?;
        if rows.as_slice() != [vec![SqlValue::Text(proof.session_id().to_string())]] {
            return Err(StorageError);
        }
        let previous = read_fence(db)?;
        let required = previous.max(observed_at);
        if required != previous {
            db.execute(
                SQL_WRITE_EXPIRY_FENCE,
                &[SqlValue::Integer(required), SqlValue::Integer(previous)],
            )?;
        }
        if read_fence(db)? != required {
            return Err(StorageError);
        }
        Ok(())
    })
}

fn read_fence<D: Database>(db: &D) -> Result<i64, StorageError> {
    match db.query(SQL_READ_EXPIRY_FENCE, &[])?.as_slice() {
        [row] => match row.as_slice() {
            [SqlValue::Integer(time)] if (0..=JS_SAFE_INTEGER_MAX).contains(time) => Ok(*time),
            _ => Err(StorageError),
        },
        _ => Err(StorageError),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real SQLite fault fixtures fail fast."
)]
mod tests {
    use super::*;
    use crate::auth::{
        AuthError, AuthPolicy, AuthService,
        test_support::{Sqlite, TestRuntime},
    };
    use crate::db::Row;
    use brews_domain::{
        accounts::AccountRole,
        ids::{AccountId, SessionId},
    };
    use std::cell::Cell;

    struct Fixture {
        db: Sqlite,
        rt: TestRuntime,
        token: String,
        proof: GameAccountAuthority,
    }
    fn fixture() -> Fixture {
        let db = Sqlite::new();
        super::super::migrate(&db).unwrap();
        let rt = TestRuntime::new();
        let account: AccountId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
        let session: SessionId = "01900000-0000-7000-8000-000000000002".parse().unwrap();
        let token = crate::security::new_token(&rt).unwrap();
        let digest = crate::security::token_digest(&token).unwrap();
        db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at) VALUES(?,?,?,?,?,?,?,?)", &[
            SqlValue::Text(account.to_string()), SqlValue::Text("ExpiryFenceHost".into()),
            SqlValue::Text(AccountRole::Host.to_string()),
            SqlValue::Text(brews_domain::accounts::AccountStatus::Verified.to_string()),
            SqlValue::Text("native-verifier-placeholder".into()), SqlValue::Integer(3),
            SqlValue::Integer(rt.now.get()), SqlValue::Integer(rt.now.get()),
        ]).unwrap();
        db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at) VALUES(?,?,?,?,?,?,?)", &[
            SqlValue::Text(session.to_string()), SqlValue::Text(account.to_string()),
            SqlValue::Blob(digest.to_vec()), SqlValue::Text(SessionScope::Normal.to_string()),
            SqlValue::Integer(3), SqlValue::Integer(rt.now.get()), SqlValue::Integer(rt.now.get()+5000),
        ]).unwrap();
        let proof = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32])
            .unwrap()
            .authorize_game_account(&token)
            .unwrap();
        Fixture {
            db,
            rt,
            token,
            proof,
        }
    }

    #[test]
    fn discovery_accounts_owner_fences_original_sample_despite_rollback_without_renewal() {
        let f = fixture();
        let observed = f.proof.expires_at() + 7;
        let before = f.db.query("SELECT * FROM account_sessions", &[]).unwrap();
        let floor =
            f.db.query("SELECT command_floor_ms FROM storage_metadata", &[])
                .unwrap();
        f.rt.now.set(f.proof.expires_at() - 1);
        let service = AuthService::new(&f.db, &f.rt, AuthPolicy::default(), &[9; 32]).unwrap();
        service
            .fence_game_account_expiry(&f.token, &f.proof, observed)
            .unwrap();
        assert_eq!(read_fence(&f.db).unwrap(), observed);
        assert_eq!(
            f.db.query("SELECT * FROM account_sessions", &[]).unwrap(),
            before
        );
        assert_eq!(
            f.db.query("SELECT command_floor_ms FROM storage_metadata", &[])
                .unwrap(),
            floor
        );
        assert_eq!(
            service.authorize_game_account(&f.token).unwrap_err(),
            AuthError::Crypto
        );
        // Replay cannot lower a fence, even when the owner clock has advanced.
        service
            .fence_game_account_expiry(&f.token, &f.proof, observed + 1)
            .unwrap();
        service
            .fence_game_account_expiry(&f.token, &f.proof, observed)
            .unwrap();
        assert_eq!(read_fence(&f.db).unwrap(), observed + 1);
    }

    #[test]
    fn discovery_expiry_fence_rejects_wrong_token_identity_metadata_and_sample() {
        let f = fixture();
        let before = read_fence(&f.db).unwrap();
        let digest = crate::security::token_digest(&f.token).unwrap();
        for field in 0..7 {
            let proof = GameAccountAuthority::from_trusted_peer(
                if field == 0 {
                    "01900000-0000-7000-8000-000000000009".parse().unwrap()
                } else {
                    f.proof.account_id()
                },
                if field == 1 {
                    "01900000-0000-7000-8000-000000000009".parse().unwrap()
                } else {
                    f.proof.session_id()
                },
                if field == 2 {
                    AccountRole::Admin
                } else {
                    f.proof.role()
                },
                f.proof.credential_epoch() + i64::from(field == 3),
                f.proof.expires_at() + i64::from(field == 4),
            )
            .unwrap();
            let sampled = if field == 5 {
                f.proof.expires_at() - 1
            } else {
                f.proof.expires_at()
            };
            let token = if field == 6 { &[0; 32] } else { &digest };
            assert_eq!(
                fence_account_expiry(&f.db, token, &proof, sampled),
                Err(StorageError)
            );
            assert_eq!(read_fence(&f.db).unwrap(), before);
        }
        for sample in [-1, JS_SAFE_INTEGER_MAX + 1] {
            assert_eq!(
                fence_account_expiry(&f.db, &digest, &f.proof, sample),
                Err(StorageError)
            );
        }
    }

    #[test]
    fn discovery_expiry_fence_sqlite_ignore_abort_and_tampered_readback_never_ack() {
        for trigger in [
            "CREATE TRIGGER expiry_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata BEGIN SELECT RAISE(IGNORE); END",
            "CREATE TRIGGER expiry_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata BEGIN SELECT RAISE(ABORT,'fixture'); END",
            "CREATE TRIGGER expiry_fault AFTER UPDATE OF last_observed_ms ON storage_metadata BEGIN UPDATE storage_metadata SET last_observed_ms=OLD.last_observed_ms WHERE singleton=1; END",
        ] {
            let f = fixture();
            let before = read_fence(&f.db).unwrap();
            f.db.execute(trigger, &[]).unwrap();
            let service = AuthService::new(&f.db, &f.rt, AuthPolicy::default(), &[9; 32]).unwrap();
            assert_eq!(
                service.fence_game_account_expiry(&f.token, &f.proof, f.proof.expires_at()),
                Err(AuthError::Storage)
            );
            assert_eq!(read_fence(&f.db).unwrap(), before);
        }
    }

    struct ReadbackFault<'a> {
        db: &'a Sqlite,
        wrote: Cell<bool>,
        commit_fault: bool,
    }
    impl Database for ReadbackFault<'_> {
        fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
            if sql == SQL_READ_EXPIRY_FENCE && self.wrote.get() && !self.commit_fault {
                return Err(StorageError);
            }
            self.db.query(sql, params)
        }
        fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
            self.db.execute(sql, params)?;
            if sql == SQL_WRITE_EXPIRY_FENCE {
                self.wrote.set(true);
            }
            Ok(())
        }
        fn transaction<T>(
            &self,
            operation: impl FnOnce() -> Result<T, StorageError>,
        ) -> Result<T, StorageError> {
            self.db.transaction(|| {
                let value = operation()?;
                if self.commit_fault {
                    Err(StorageError)
                } else {
                    Ok(value)
                }
            })
        }
    }
    #[test]
    fn discovery_expiry_fence_readback_and_transaction_failure_roll_back() {
        for commit_fault in [false, true] {
            let f = fixture();
            let before = read_fence(&f.db).unwrap();
            let faulty = ReadbackFault {
                db: &f.db,
                wrote: Cell::new(false),
                commit_fault,
            };
            let digest = crate::security::token_digest(&f.token).unwrap();
            assert_eq!(
                fence_account_expiry(&faulty, &digest, &f.proof, f.proof.expires_at()),
                Err(StorageError)
            );
            assert_eq!(read_fence(&f.db).unwrap(), before);
        }
    }
}
