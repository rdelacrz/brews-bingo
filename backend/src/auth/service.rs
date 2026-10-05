use super::{
    AuthCommand, AuthError, AuthOutcome, AuthPolicy, AuthService, CookieEffect, RequestContext,
    Runtime,
    records::{ACCOUNT_SELECT, Account, Session},
};
use crate::{
    limits::{
        ACCESS_LINK_METADATA_RETENTION_MS, CLEANUP_BATCH_SIZE, COMMAND_RECEIPT_RETENTION_MS,
        JS_SAFE_INTEGER_MAX, LOGIN_VERIFICATION_MAX_ATTEMPTS, RATE_LIMIT_KEY_MAX_BYTES,
        RATE_LIMIT_KEY_MIN_BYTES, REDEMPTION_SUBJECT_MAX_BYTES, SESSION_LIFETIME_MS,
        SOCKET_CLOSE_INITIAL_DELAY_MS, TOKEN_GENERATION_MAX_ATTEMPTS,
    },
    security::{hash_password, new_token, token_digest, validate_policy, verify_password},
    storage::{CURRENT_SCHEMA_VERSION, Database, SqlValue, StorageError},
};
use brews_domain::{
    accounts::{
        AccessLinkPurpose, AccountStatus, SessionScope, validate_password, validate_username,
    },
    ids::{AccountId, SessionId},
};
use std::cell::Cell;
use zeroize::Zeroizing;

impl<'a, D: Database, R: Runtime> AuthService<'a, D, R> {
    pub fn new(
        db: &'a D,
        runtime: &'a R,
        policy: AuthPolicy,
        rate_key: &'a [u8],
    ) -> Result<Self, AuthError> {
        validate_policy(policy)?;
        if !(RATE_LIMIT_KEY_MIN_BYTES..=RATE_LIMIT_KEY_MAX_BYTES).contains(&rate_key.len()) {
            return Err(AuthError::Crypto);
        }
        if db.query(
            "SELECT schema_version FROM storage_metadata WHERE singleton=1",
            &[],
        )? != vec![vec![SqlValue::Integer(CURRENT_SCHEMA_VERSION)]]
        {
            return Err(AuthError::Storage);
        }
        Ok(Self {
            db,
            runtime,
            policy,
            rate_key,
        })
    }
    pub fn cleanup(&self) -> Result<(), AuthError> {
        self.transaction(|| {
            let now=self.now()?;
            self.enqueue_expired_closes(now)?;
            self.db.execute("DELETE FROM account_sessions WHERE session_id IN (SELECT session_id FROM account_sessions WHERE expires_at<=? OR revoked_at IS NOT NULL LIMIT ?)",&[SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?;
            self.db.execute("DELETE FROM command_receipts WHERE (actor_account_id,command_id) IN (SELECT actor_account_id,command_id FROM command_receipts WHERE expires_at<=? LIMIT ?)",&[SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?;
            self.db.execute("DELETE FROM rate_limit_buckets WHERE (scope,subject_key) IN (SELECT scope,subject_key FROM rate_limit_buckets WHERE expires_at<=? LIMIT ?)",&[SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?;
            self.db.execute("DELETE FROM access_links WHERE link_id IN (SELECT link_id FROM access_links WHERE max(expires_at,coalesce(consumed_at,0),coalesce(revoked_at,0))+?<=? LIMIT ?)",&[SqlValue::Integer(ACCESS_LINK_METADATA_RETENTION_MS),SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?;
            Ok(())
        })
    }
    pub fn next_deadline(&self) -> Result<Option<i64>, AuthError> {
        // Cloudflare limits compound SELECTs to five terms.
        let rows = self.db.query("SELECT (SELECT min(CASE WHEN revoked_at IS NULL THEN expires_at ELSE revoked_at END) FROM account_sessions),(SELECT min(expires_at) FROM command_receipts),(SELECT min(expires_at) FROM rate_limit_buckets),(SELECT min(max(expires_at,coalesce(consumed_at,0),coalesce(revoked_at,0))+?) FROM access_links),(SELECT min(expires_at) FROM account_socket_subscriptions WHERE connection_id NOT IN (SELECT connection_id FROM account_socket_close_work)),(SELECT min(next_attempt_at) FROM account_socket_close_work)", &[SqlValue::Integer(ACCESS_LINK_METADATA_RETENTION_MS)])?;
        let row = rows.first().ok_or(AuthError::Storage)?;
        if row.len() != 6 {
            return Err(AuthError::Storage);
        }
        let mut earliest = None;
        for index in 0..row.len() {
            if let Some(deadline) = super::records::optional_integer(row, index)? {
                if !(0..=JS_SAFE_INTEGER_MAX).contains(&deadline) {
                    return Err(AuthError::Storage);
                }
                earliest = Some(earliest.map_or(deadline, |current: i64| current.min(deadline)));
            }
        }
        Ok(earliest)
    }
    pub fn execute(
        &self,
        command: AuthCommand,
        context: RequestContext,
    ) -> Result<AuthOutcome, AuthError> {
        if matches!(
            &command,
            AuthCommand::Login { .. } | AuthCommand::Redeem { .. } | AuthCommand::Complete { .. }
        ) {
            super::receipts::command_id(&context)?;
        }
        self.transaction(|| Ok(()))?;
        match &command {
            AuthCommand::Login { username, password } => self.login(username, password, &context),
            AuthCommand::Current { token } => self.current(token.as_deref()),
            AuthCommand::Logout { token } => self.logout(token.as_deref()),
            AuthCommand::Redeem { purpose, token } => self.redeem(*purpose, token, &context),
            AuthCommand::Complete {
                scope,
                token,
                new_password,
            } => self.complete(*scope, token, new_password, &context),
        }
    }
    pub(super) fn now(&self) -> Result<i64, AuthError> {
        let now = self.runtime.now_ms();
        if !(0..=JS_SAFE_INTEGER_MAX).contains(&now) {
            return Err(AuthError::Crypto);
        }
        let row = self.db.query(
            "SELECT last_observed_ms FROM storage_metadata WHERE singleton=1",
            &[],
        )?;
        let row = row.first().ok_or(AuthError::Storage)?;
        if now < super::records::integer(row, 0)? {
            return Err(AuthError::Crypto);
        }
        Ok(now)
    }
    pub(super) fn observe_clock(&self) -> Result<i64, AuthError> {
        let now = self.now()?;
        self.db.execute("UPDATE storage_metadata SET last_observed_ms=?,command_floor_ms=max(command_floor_ms,?) WHERE singleton=1",&[SqlValue::Integer(now),SqlValue::Integer(now.saturating_sub(COMMAND_RECEIPT_RETENTION_MS).max(0))])?;
        Ok(now)
    }
    pub(super) fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, AuthError>,
    ) -> Result<T, AuthError> {
        let fatal = Cell::new(None);
        let result = self
            .db
            .transaction(|| match self.observe_clock().and_then(|_| operation()) {
                Err(e @ (AuthError::Storage | AuthError::Crypto)) => {
                    fatal.set(Some(e));
                    Err(StorageError)
                }
                result => Ok(result),
            });
        match result {
            Ok(out) => out,
            Err(_) => Err(fatal.get().unwrap_or(AuthError::Storage)),
        }
    }
    pub(super) fn account(&self, id: AccountId) -> Result<Option<Account>, AuthError> {
        self.db
            .query(ACCOUNT_SELECT, &[SqlValue::Text(id.to_string())])?
            .first()
            .map(Account::parse)
            .transpose()
    }
    pub(super) fn session(&self, token: &str) -> Result<Option<Session>, AuthError> {
        let digest = match token_digest(token) {
            Ok(d) => d,
            Err(AuthError::InvalidInput) => return Ok(None),
            Err(e) => return Err(e),
        };
        self.db.query("SELECT session_id,account_id,scope,credential_epoch,expires_at,revoked_at FROM account_sessions WHERE token_verifier=?",&[SqlValue::Blob(digest.to_vec())])?.first().map(Session::parse).transpose()
    }
    fn current(&self, token: Option<&str>) -> Result<AuthOutcome, AuthError> {
        self.transaction(|| {
            let now=self.now()?;
            if let Some(token)=token
                && let Some(s)=self.session(token)?
                && let Some(a)=self.account(s.account_id)?
                && s.eligible(&a,now)
            {
                return Ok(AuthOutcome{body:serde_json::json!({"authenticated":true,"account":a.safe(),"session":s.safe(&a)}),cookie:CookieEffect::None});
            }
            Ok(AuthOutcome{body:serde_json::json!({"authenticated":false,"session":null}),cookie:if token.is_some(){CookieEffect::Clear}else{CookieEffect::None}})
        })
    }
    fn complete(
        &self,
        scope: SessionScope,
        token: &str,
        password: &str,
        context: &RequestContext,
    ) -> Result<AuthOutcome, AuthError> {
        if scope == SessionScope::Normal {
            return Err(AuthError::InvalidInput);
        }
        validate_password(password).map_err(|_| AuthError::InvalidInput)?;
        let original = self.session(token)?.ok_or(AuthError::Unauthorized)?;
        let account = self
            .account(original.account_id)?
            .ok_or(AuthError::Unauthorized)?;
        let command = super::receipts::command_id(context)?;
        let session_id = original.id.to_string();
        let scope_tag = scope.to_string();
        let fp = super::receipts::fingerprint("complete", &[&scope_tag, &session_id])?;
        if original.scope != scope {
            return Err(AuthError::Unauthorized);
        }
        if !original.eligible(&account, self.now()?) {
            // A retired cookie is a binding hint, not current authority. Reprove
            // the CURRENT account password before any safe receipt is returned.
            if !account.login_eligible()
                || original.expires <= self.now()?
                || original.epoch.checked_add(1) != Some(account.epoch)
                || original.revoked.is_none()
            {
                return Err(AuthError::Unauthorized);
            }
            if !verify_password(
                password,
                account.verifier.as_deref().ok_or(AuthError::Unauthorized)?,
            )? {
                return Err(AuthError::Unauthorized);
            }
            return self.transaction(|| {
                let now = self.now()?;
                let current = self.account(account.id)?.ok_or(AuthError::Unauthorized)?;
                if !current.login_eligible()
                    || current.epoch != account.epoch
                    || current.verifier != account.verifier
                {
                    return Err(AuthError::Unauthorized);
                }
                self.receipt(current.id, command, fp, now)?
                    .ok_or(AuthError::Unauthorized)
            });
        }
        self.transaction(|| {
            self.receipt(account.id, command, fp, self.now()?)
                .map(|_| ())
        })?;
        let phc = Zeroizing::new(hash_password(password, self.policy, self.runtime)?);
        self.transaction(|| {
            let now=self.now()?;
            let s=self.session(token)?.ok_or(AuthError::Unauthorized)?;
            let mut a=self.account(s.account_id)?.ok_or(AuthError::Unauthorized)?;
            if s.id!=original.id || s.scope!=scope || !s.eligible(&a,now) {return Err(AuthError::Unauthorized);}
            if let Some(receipt)=self.receipt(a.id,command,fp,now)? {return Ok(receipt);}
            a.epoch=a.epoch.checked_add(1).filter(|v|*v<=JS_SAFE_INTEGER_MAX).ok_or(AuthError::Conflict)?;
            self.enqueue_account_closes(a.id,now)?;
            self.db.execute(
                "UPDATE accounts SET verifier=?,status=?,password_set_at=?,credential_epoch=? WHERE account_id=?",
                &[
                    SqlValue::Text(phc.to_string()),
                    SqlValue::Text(AccountStatus::Verified.to_string()),
                    SqlValue::Integer(now),
                    SqlValue::Integer(a.epoch),
                    SqlValue::Text(a.id.to_string()),
                ],
            )?;
            self.db.execute("UPDATE account_sessions SET revoked_at=? WHERE account_id=? AND revoked_at IS NULL",&[SqlValue::Integer(now),SqlValue::Text(a.id.to_string())])?;
            self.db.execute("UPDATE access_links SET revoked_at=? WHERE account_id=? AND revoked_at IS NULL",&[SqlValue::Integer(now),SqlValue::Text(a.id.to_string())])?;
            a.status=AccountStatus::Verified;
            let outcome=if scope==SessionScope::EnrollmentOnly {super::receipts::CommandOutcome::CompleteEnrollment{account_id:a.id}}else{super::receipts::CommandOutcome::CompletePasswordReset{account_id:a.id}};
            self.record_receipt(a.id,command,fp,outcome,now)?;
            if scope==SessionScope::EnrollmentOnly {
                let (normal,bearer)=self.issue_session(&a,SessionScope::Normal,now,s.expires)?;
                Ok(AuthOutcome{body:serde_json::json!({"account":a.safe(),"session":normal.safe(&a)}),cookie:CookieEffect::Set{token:bearer,expires_at:s.expires}})
            }else{
                Ok(AuthOutcome{body:serde_json::json!({"reset_completed":true,"next_action":"login"}),cookie:CookieEffect::Clear})
            }
        })
    }
    fn redeem(
        &self,
        purpose: AccessLinkPurpose,
        token: &str,
        context: &RequestContext,
    ) -> Result<AuthOutcome, AuthError> {
        let subject = token
            .as_bytes()
            .get(..REDEMPTION_SUBJECT_MAX_BYTES)
            .unwrap_or(token.as_bytes());
        let keys = self.rate_keys("access_link_redemption", subject, context)?;
        self.transaction(|| {
            use super::records::{text,integer,optional_integer};
            let now=self.now()?;
            self.check_rate("access_link_redemption",&keys,now)?;
            let result=(|| {
            let digest=token_digest(token).map_err(|_|AuthError::InvalidLink)?;
            let rows=self.db.query("SELECT link_id,account_id,purpose,credential_epoch,expires_at,consumed_at,revoked_at FROM access_links WHERE token_verifier=?",&[SqlValue::Blob(digest.to_vec())])?;
            let row=rows.first().ok_or(AuthError::InvalidLink)?;
            if row.len()!=7 {return Err(AuthError::Storage);}
            let id:brews_domain::ids::LinkId=text(row,0)?.parse().map_err(|_|AuthError::Storage)?;
            let account_id:AccountId=text(row,1)?.parse().map_err(|_|AuthError::Storage)?;
            let stored_purpose:AccessLinkPurpose=text(row,2)?.parse().map_err(|_|AuthError::Storage)?;
            let a=self.account(account_id)?.ok_or(AuthError::InvalidLink)?;
            let (status,scope)=match purpose {AccessLinkPurpose::Enrollment=>(AccountStatus::PendingEnrollment,SessionScope::EnrollmentOnly),AccessLinkPurpose::PasswordReset=>(AccountStatus::ResetRequired,SessionScope::PasswordResetOnly)};
            if stored_purpose!=purpose || a.status!=status || a.disabled.is_some() || a.epoch!=integer(row,3)? || integer(row,4)?<=now || optional_integer(row,5)?.is_some() || optional_integer(row,6)?.is_some() {return Err(AuthError::InvalidLink);}
            let command=super::receipts::command_id(context)?;
            let purpose_tag=purpose.to_string();let link_id=id.to_string();
            let fp=super::receipts::fingerprint("redeem",&[&purpose_tag,&link_id])?;
            if self.receipt(a.id,command,fp,now)?.is_some() {return Err(AuthError::InvalidLink);}
            let deadline=add_deadline(now,SESSION_LIFETIME_MS)?;
            let (s,token)=self.issue_session(&a,scope,now,deadline)?;
            self.record_receipt(a.id,command,fp,super::receipts::CommandOutcome::Redeem{account_id:a.id,link_id:id,session_id:s.id,session_expires_at:deadline},now)?;
            self.db.execute("UPDATE access_links SET consumed_at=? WHERE link_id=? AND consumed_at IS NULL AND revoked_at IS NULL",&[SqlValue::Integer(now),SqlValue::Text(id.to_string())])?;
            let body=match purpose {AccessLinkPurpose::Enrollment=>serde_json::json!({"setup_required":true,"session":s.safe(&a)}),AccessLinkPurpose::PasswordReset=>serde_json::json!({"reset_required":true,"session":s.safe(&a)})};
            Ok(AuthOutcome{body,cookie:CookieEffect::Set{token,expires_at:deadline}})
            })();
            match &result {
                Err(AuthError::InvalidLink)=>self.fail_rate("access_link_redemption",&keys,now)?,
                Ok(_)=>self.clear_rate("access_link_redemption",&keys)?,
                _=>{},
            }
            result
        })
    }
    fn logout(&self, token: Option<&str>) -> Result<AuthOutcome, AuthError> {
        self.transaction(|| {
            let now=self.now()?;
            if let Some(token)=token && let Some(s)=self.session(token)? {
                self.enqueue_session_closes(s.id,now)?;
                self.db.execute("UPDATE account_sessions SET revoked_at=? WHERE session_id=? AND revoked_at IS NULL",&[SqlValue::Integer(now),SqlValue::Text(s.id.to_string())])?;
            }
            Ok(AuthOutcome{body:serde_json::json!({"logged_out":true}),cookie:CookieEffect::Clear})
        })
    }
    fn login(
        &self,
        username: &str,
        password: &str,
        context: &RequestContext,
    ) -> Result<AuthOutcome, AuthError> {
        let command = super::receipts::command_id(context)?;
        let username = validate_username(username).map_err(|_| AuthError::InvalidInput)?;
        let fp = super::receipts::fingerprint("login", &[username])?;
        validate_password(password).map_err(|_| AuthError::InvalidInput)?;
        let keys = self.rate_keys("account_login", username.as_bytes(), context)?;
        for _ in 0..LOGIN_VERIFICATION_MAX_ATTEMPTS {
            self.transaction(|| self.check_rate("account_login", &keys, self.now()?))?;
            let original=self.db.query("SELECT account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at FROM accounts WHERE username=? COLLATE BINARY",&[SqlValue::Text(username.to_owned())])?.first().map(Account::parse).transpose()?;
            let Some(original) = original else {
                // Real KDF work masks cheap denial; it grants no authority.
                let _denial = Zeroizing::new(hash_password(password, self.policy, self.runtime)?);
                return self.rejected_credentials(&keys);
            };
            if !original.login_eligible() {
                let _denial = Zeroizing::new(hash_password(password, self.policy, self.runtime)?);
                return self.rejected_credentials(&keys);
            }
            let phc = original
                .verifier
                .as_deref()
                .ok_or(AuthError::InvalidCredentials)?;
            if !verify_password(password, phc)? {
                return self.rejected_credentials(&keys);
            }
            let profile = crate::security::bounded_phc(phc)?;
            let m = profile.params.get_decimal("m").ok_or(AuthError::Crypto)?;
            let t = profile.params.get_decimal("t").ok_or(AuthError::Crypto)?;
            let upgrade = m <= self.policy.m_cost
                && t <= self.policy.t_cost
                && (m < self.policy.m_cost || t < self.policy.t_cost);
            let candidate = if upgrade {
                match hash_password(password, self.policy, self.runtime) {
                    Ok(phc) => Some(Zeroizing::new(phc)),
                    Err(_) => {
                        report_rehash_failure();
                        None
                    }
                }
            } else {
                None
            };
            let committed=self.transaction(|| {
                let now=self.now()?;self.check_rate("account_login",&keys,now)?;
                let a=self.account(original.id)?.ok_or(AuthError::InvalidCredentials)?;
                if !a.login_eligible() {return Err(AuthError::InvalidCredentials);}
                if a.epoch!=original.epoch || a.verifier!=original.verifier {return Ok(None);}
                if let Some(receipt)=self.receipt(a.id,command,fp,now)? {self.clear_rate("account_login",&keys)?;return Ok(Some(receipt));}
                if let Some(candidate)=&candidate {
                    self.db.execute(
                        "UPDATE accounts SET verifier=? WHERE account_id=? AND credential_epoch=? AND verifier=? AND status=? AND disabled_at IS NULL",
                        &[
                            SqlValue::Text(candidate.to_string()),
                            SqlValue::Text(a.id.to_string()),
                            SqlValue::Integer(a.epoch),
                            SqlValue::Text(phc.to_owned()),
                            SqlValue::Text(AccountStatus::Verified.to_string()),
                        ],
                    )?;
                }
                let deadline=add_deadline(now,SESSION_LIFETIME_MS)?;
                let (s,token)=self.issue_session(&a,SessionScope::Normal,now,deadline)?;
                self.record_receipt(a.id,command,fp,super::receipts::CommandOutcome::Login{account_id:a.id,session_id:s.id,session_expires_at:deadline},now)?;
                self.clear_rate("account_login",&keys)?;
                Ok(Some(AuthOutcome{body:serde_json::json!({"account":a.safe(),"session":s.safe(&a)}),cookie:CookieEffect::Set{token,expires_at:deadline}}))
            })?;
            if let Some(outcome) = committed {
                return Ok(outcome);
            }
        }
        Err(AuthError::InvalidCredentials)
    }
    fn issue_session(
        &self,
        a: &Account,
        scope: SessionScope,
        now: i64,
        expires: i64,
    ) -> Result<(Session, String), AuthError> {
        for _ in 0..TOKEN_GENERATION_MAX_ATTEMPTS {
            let token = Zeroizing::new(new_token(self.runtime)?);
            let digest = token_digest(&token)?;
            let id: SessionId = self
                .new_uuid(now)?
                .try_into()
                .map_err(|_| AuthError::Crypto)?;
            if !self.db.query("SELECT session_id FROM account_sessions WHERE token_verifier=? OR session_id=?",&[SqlValue::Blob(digest.to_vec()),SqlValue::Text(id.to_string())])?.is_empty() {continue;}
            self.db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at,revoked_at) VALUES(?,?,?,?,?,?,?,NULL)",&[SqlValue::Text(id.to_string()),SqlValue::Text(a.id.to_string()),SqlValue::Blob(digest.to_vec()),SqlValue::Text(scope.to_string()),SqlValue::Integer(a.epoch),SqlValue::Integer(now),SqlValue::Integer(expires)])?;
            return Ok((
                Session {
                    id,
                    account_id: a.id,
                    scope,
                    epoch: a.epoch,
                    expires,
                    revoked: None,
                },
                token.to_string(),
            ));
        }
        Err(AuthError::Crypto)
    }
    fn enqueue_session_closes(&self, session: SessionId, now: i64) -> Result<(), AuthError> {
        if self.db.query("SELECT connection_id FROM account_socket_subscriptions WHERE session_id=? AND connection_id NOT IN (SELECT connection_id FROM account_socket_close_work) LIMIT 1",&[SqlValue::Text(session.to_string())])?.is_empty() {return Ok(());}
        let operation: brews_domain::ids::OperationId = self
            .new_uuid(now)?
            .try_into()
            .map_err(|_| AuthError::Crypto)?;
        self.db.execute("INSERT INTO account_socket_close_work(connection_id,operation_id,account_id,session_id,game_id,credential_epoch,expires_at,created_at,next_attempt_at,attempt_count) SELECT connection_id,?,account_id,session_id,game_id,credential_epoch,expires_at,?,?,0 FROM account_socket_subscriptions WHERE session_id=? ON CONFLICT(connection_id) DO NOTHING",&[SqlValue::Text(operation.to_string()),SqlValue::Integer(now),SqlValue::Integer(add_deadline(now,SOCKET_CLOSE_INITIAL_DELAY_MS)?),SqlValue::Text(session.to_string())])?;
        Ok(())
    }
    pub(super) fn enqueue_account_closes(
        &self,
        account: AccountId,
        now: i64,
    ) -> Result<(), AuthError> {
        if self.db.query("SELECT connection_id FROM account_socket_subscriptions WHERE account_id=? AND connection_id NOT IN (SELECT connection_id FROM account_socket_close_work) LIMIT 1",&[SqlValue::Text(account.to_string())])?.is_empty() {return Ok(());}
        let operation: brews_domain::ids::OperationId = self
            .new_uuid(now)?
            .try_into()
            .map_err(|_| AuthError::Crypto)?;
        self.db.execute("INSERT INTO account_socket_close_work(connection_id,operation_id,account_id,session_id,game_id,credential_epoch,expires_at,created_at,next_attempt_at,attempt_count) SELECT connection_id,?,account_id,session_id,game_id,credential_epoch,expires_at,?,?,0 FROM account_socket_subscriptions WHERE account_id=? ON CONFLICT(connection_id) DO NOTHING",&[SqlValue::Text(operation.to_string()),SqlValue::Integer(now),SqlValue::Integer(add_deadline(now,SOCKET_CLOSE_INITIAL_DELAY_MS)?),SqlValue::Text(account.to_string())])?;
        Ok(())
    }
    fn enqueue_expired_closes(&self, now: i64) -> Result<(), AuthError> {
        if self.db.query("SELECT connection_id FROM account_socket_subscriptions WHERE expires_at<=? AND connection_id NOT IN (SELECT connection_id FROM account_socket_close_work) LIMIT 1",&[SqlValue::Integer(now)])?.is_empty() {return Ok(());}
        let operation: brews_domain::ids::OperationId = self
            .new_uuid(now)?
            .try_into()
            .map_err(|_| AuthError::Crypto)?;
        self.db.execute("INSERT INTO account_socket_close_work(connection_id,operation_id,account_id,session_id,game_id,credential_epoch,expires_at,created_at,next_attempt_at,attempt_count) SELECT connection_id,?,account_id,session_id,game_id,credential_epoch,expires_at,?,?,0 FROM account_socket_subscriptions WHERE expires_at<=? AND connection_id NOT IN (SELECT connection_id FROM account_socket_close_work) LIMIT ?",&[SqlValue::Text(operation.to_string()),SqlValue::Integer(now),SqlValue::Integer(add_deadline(now,SOCKET_CLOSE_INITIAL_DELAY_MS)?),SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?;
        Ok(())
    }
    pub(super) fn new_uuid(&self, now: i64) -> Result<uuid::Uuid, AuthError> {
        let now = u64::try_from(now).map_err(|_| AuthError::Crypto)?;
        if now >= (1u64 << 48) {
            return Err(AuthError::Crypto);
        }
        let mut b = [0u8; 16];
        self.runtime.fill_random(&mut b)?;
        b[..6].copy_from_slice(&now.to_be_bytes()[2..]);
        b[6] = (b[6] & 0x0f) | 0x70;
        b[8] = (b[8] & 0x3f) | 0x80;
        Ok(uuid::Uuid::from_bytes(b))
    }
}
fn report_rehash_failure() {
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("auth.rehash_unavailable");
    #[cfg(target_arch = "wasm32")]
    worker::console_error!("auth.rehash_unavailable");
}
fn add_deadline(now: i64, period: i64) -> Result<i64, AuthError> {
    now.checked_add(period)
        .filter(|n| *n <= JS_SAFE_INTEGER_MAX)
        .ok_or(AuthError::Crypto)
}
