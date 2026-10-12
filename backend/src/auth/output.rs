//! Secret-free session identity for a fresh transport release cut.
use super::{AuthError, AuthService, Runtime};
use crate::{
    db::{Database, SqlValue, StorageError},
    limits::JS_SAFE_INTEGER_MAX,
};
use brews_domain::{
    accounts::{AccountRole, AccountStatus, SessionScope},
    ids::{AccountId, SessionId},
};
use serde::{Deserialize, Serialize};

const SQL_SELECT_OUTPUT_CLOCK_FENCE: &str =
    "SELECT last_observed_ms FROM storage_metadata WHERE singleton=1";
const SQL_ADVANCE_OUTPUT_CLOCK_FENCE: &str =
    "UPDATE storage_metadata SET last_observed_ms=? WHERE singleton=1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthOutputAuthority {
    pub(crate) account_id: AccountId,
    pub(crate) session_id: SessionId,
    pub(crate) scope: SessionScope,
    pub(crate) role: AccountRole,
    pub(crate) status: AccountStatus,
    pub(crate) epoch: i64,
    pub(crate) expires_at: i64,
}
impl<D: Database, R: Runtime> AuthService<'_, D, R> {
    pub(crate) fn auth_output_authority(
        &self,
        token: &str,
    ) -> Result<AuthOutputAuthority, AuthError> {
        let now = self.now()?;
        let session = self.session(token)?.ok_or(AuthError::Unauthorized)?;
        let account = self
            .account(session.account_id)?
            .ok_or(AuthError::Unauthorized)?;
        let authority = AuthOutputAuthority {
            account_id: account.id,
            session_id: session.id,
            scope: session.scope,
            role: account.role,
            status: account.status,
            epoch: session.epoch,
            expires_at: session.expires,
        };
        if session.expires <= now {
            self.fence_auth_output_expiry(token, &authority, now)?;
        }
        if !session.eligible(&account, now) {
            return Err(AuthError::Unauthorized);
        }
        Ok(authority)
    }
    pub(crate) fn validate_auth_output(
        &self,
        token: &str,
        expected: &AuthOutputAuthority,
        body: &serde_json::Value,
    ) -> Result<(), AuthError> {
        if self.auth_output_authority(token)? != *expected {
            return Err(AuthError::Unauthorized);
        }
        let session = self.session(token)?.ok_or(AuthError::Unauthorized)?;
        let account = self
            .account(session.account_id)?
            .ok_or(AuthError::Unauthorized)?;
        if body.get("session") != Some(&session.safe(&account))
            || body
                .get("account")
                .is_some_and(|value| *value != account.safe())
        {
            return Err(AuthError::Unauthorized);
        }
        Ok(())
    }
    /// The receiving clock is authoritative only for denial; never renew a session.
    pub(crate) fn fence_auth_output_expiry(
        &self,
        token: &str,
        expected: &AuthOutputAuthority,
        observed_at: i64,
    ) -> Result<(), AuthError> {
        if !(1..=JS_SAFE_INTEGER_MAX).contains(&expected.expires_at)
            || !(expected.expires_at..=JS_SAFE_INTEGER_MAX).contains(&observed_at)
        {
            return Err(AuthError::InvalidInput);
        }
        self.db
            .transaction(|| {
                let session = self
                    .session(token)
                    .map_err(|_| StorageError)?
                    .ok_or(StorageError)?;
                let account = self
                    .account(session.account_id)
                    .map_err(|_| StorageError)?
                    .ok_or(StorageError)?;
                if session.id != expected.session_id
                    || session.account_id != expected.account_id
                    || session.scope != expected.scope
                    || session.epoch != expected.epoch
                    || session.expires != expected.expires_at
                    || account.role != expected.role
                    || account.status != expected.status
                {
                    return Err(StorageError);
                }
                let read = || match self
                    .db
                    .query(SQL_SELECT_OUTPUT_CLOCK_FENCE, &[])?
                    .as_slice()
                {
                    [row] => match row.as_slice() {
                        [SqlValue::Integer(time)] if (0..=JS_SAFE_INTEGER_MAX).contains(time) => {
                            Ok(*time)
                        }
                        _ => Err(StorageError),
                    },
                    _ => Err(StorageError),
                };
                let required = read()?.max(observed_at);
                self.db.execute(
                    SQL_ADVANCE_OUTPUT_CLOCK_FENCE,
                    &[SqlValue::Integer(required)],
                )?;
                if read()? != required {
                    return Err(StorageError);
                }
                Ok(())
            })
            .map_err(AuthError::from)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "SQL output fixtures fail fast."
)]
mod tests {
    use super::*;
    use crate::auth::records::ACCOUNT_SELECT;
    use crate::auth::{
        AuthPolicy,
        test_support::{Sqlite, TestRuntime},
    };

    fn fixture() -> (Sqlite, TestRuntime, String) {
        let db = Sqlite::new();
        crate::db::migrate(&db).unwrap();
        let rt = TestRuntime::new();
        let token = crate::security::new_token(&rt).unwrap();
        let account = "01900000-0000-7000-8000-000000000001";
        let session = "01900000-0000-7000-8000-000000000002";
        db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at) VALUES(?,?,?,?,?,?,?,?)", &[
            SqlValue::Text(account.into()), SqlValue::Text("OutputHost".into()),
            SqlValue::Text(AccountRole::Host.to_string()), SqlValue::Text(AccountStatus::Verified.to_string()),
            SqlValue::Text("native-verifier-placeholder".into()), SqlValue::Integer(1),
            SqlValue::Integer(rt.now.get()), SqlValue::Integer(rt.now.get()),
        ]).unwrap();
        db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at) VALUES(?,?,?,?,?,?,?)", &[
            SqlValue::Text(session.into()), SqlValue::Text(account.into()),
            SqlValue::Blob(crate::security::token_digest(&token).unwrap().to_vec()),
            SqlValue::Text(SessionScope::Normal.to_string()), SqlValue::Integer(1),
            SqlValue::Integer(rt.now.get()), SqlValue::Integer(rt.now.get()+5000),
        ]).unwrap();
        (db, rt, token)
    }
    #[test]
    fn output_fence_preserves_original_sample_and_never_renews_or_resurrects() {
        let (db, rt, token) = fixture();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let proof = service.auth_output_authority(&token).unwrap();
        let account = service.account(proof.account_id).unwrap().unwrap();
        let session = service.session(&token).unwrap().unwrap();
        let body = serde_json::json!({"account":account.safe(), "session":session.safe(&account)});
        service.validate_auth_output(&token, &proof, &body).unwrap();
        let before = db.query("SELECT * FROM account_sessions", &[]).unwrap();
        rt.now.set(proof.expires_at - 1);
        service
            .fence_auth_output_expiry(&token, &proof, proof.expires_at + 3)
            .unwrap();
        assert_eq!(
            db.query("SELECT * FROM account_sessions", &[]).unwrap(),
            before
        );
        assert_eq!(
            db.query("SELECT last_observed_ms FROM storage_metadata", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(proof.expires_at + 3)]]
        );
        assert_eq!(
            service.auth_output_authority(&token).unwrap_err(),
            AuthError::Crypto
        );
        assert_eq!(
            service.validate_auth_output(&token, &proof, &body),
            Err(AuthError::Crypto)
        );
        service
            .fence_auth_output_expiry(&token, &proof, proof.expires_at)
            .unwrap();
        assert_eq!(
            db.query("SELECT last_observed_ms FROM storage_metadata", &[])
                .unwrap(),
            vec![vec![SqlValue::Integer(proof.expires_at + 3)]]
        );
    }
    #[test]
    fn output_fence_requires_exact_identity_deadline_sample_and_verified_write() {
        for fault in [
            "wrong_deadline",
            "wrong_identity",
            "early_sample",
            "ignore_write",
            "failed_write",
            "tampered_readback",
        ] {
            let (db, rt, token) = fixture();
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
            let mut proof = service.auth_output_authority(&token).unwrap();
            let before = db
                .query("SELECT last_observed_ms FROM storage_metadata", &[])
                .unwrap();
            let sample = proof.expires_at;
            match fault {
                "wrong_deadline" => proof.expires_at -= 1,
                "wrong_identity" => proof.epoch += 1,
                "ignore_write" => db.execute("CREATE TRIGGER output_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap(),
                "failed_write" => db.execute("CREATE TRIGGER output_fault BEFORE UPDATE OF last_observed_ms ON storage_metadata BEGIN SELECT RAISE(ABORT,'fixture'); END", &[]).unwrap(),
                "tampered_readback" => db.execute("CREATE TRIGGER output_fault AFTER UPDATE OF last_observed_ms ON storage_metadata BEGIN UPDATE storage_metadata SET last_observed_ms=OLD.last_observed_ms; END", &[]).unwrap(),
                _ => {},
            }
            assert!(
                service
                    .fence_auth_output_expiry(
                        &token,
                        &proof,
                        sample - i64::from(fault == "early_sample")
                    )
                    .is_err()
            );
            assert_eq!(
                db.query("SELECT last_observed_ms FROM storage_metadata", &[])
                    .unwrap(),
                before
            );
        }
    }
    #[test]
    fn output_release_rechecks_revocation_lifecycle_role_and_safe_projection() {
        for change in [
            "revocation",
            "disablement",
            "epoch",
            "role",
            "lifecycle",
            "username",
        ] {
            let (db, rt, token) = fixture();
            let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
            let proof = service.auth_output_authority(&token).unwrap();
            let account = service.account(proof.account_id).unwrap().unwrap();
            let session = service.session(&token).unwrap().unwrap();
            let body =
                serde_json::json!({"account":account.safe(), "session":session.safe(&account)});
            let sql = match change {
                "revocation" => "UPDATE account_sessions SET revoked_at=issued_at",
                "disablement" => "UPDATE accounts SET disabled_at=created_at",
                "epoch" => "UPDATE accounts SET credential_epoch=credential_epoch+1",
                "role" => "UPDATE accounts SET role='admin'",
                "lifecycle" => "UPDATE accounts SET status='reset_required'",
                "username" => "UPDATE accounts SET username='ChangedHost'",
                _ => unreachable!(),
            };
            db.execute(sql, &[]).unwrap();
            assert_eq!(
                service.validate_auth_output(&token, &proof, &body),
                Err(AuthError::Unauthorized)
            );
            assert_eq!(
                db.query(
                    ACCOUNT_SELECT,
                    &[SqlValue::Text(proof.account_id.to_string())]
                )
                .unwrap()
                .len(),
                1
            );
        }
    }
}
