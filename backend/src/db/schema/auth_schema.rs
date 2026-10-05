//! Accounts authentication schema. Each item is one fixed DDL statement.
use crate::{
    limits::{
        ACCESS_LINK_LIFETIME_MS, COMMAND_RECEIPT_MAX_BYTES, COMMAND_RECEIPT_RETENTION_MS,
        JS_SAFE_INTEGER_MAX, RATE_LIMIT_MAX_FAILURES, SESSION_LIFETIME_MS,
    },
    security::{DIGEST_BYTES, MAX_PHC_LEN},
};
use brews_domain::accounts::{USERNAME_MAX_LEN, USERNAME_MIN_LEN};

// Change deployed constraints only through explicit migrations.
pub(in crate::db) fn auth_schema_statements() -> Vec<String> {
    let username_last_high_nibble = USERNAME_MAX_LEN * 2 - 1;
    vec![
        format!(
            "CREATE TABLE accounts(account_id TEXT PRIMARY KEY NOT NULL CHECK(length(CAST(account_id AS BLOB))=36 AND length(account_id)=36 AND account_id=lower(account_id) AND substr(account_id,9,1)='-' AND substr(account_id,14,1)='-' AND substr(account_id,19,1)='-' AND substr(account_id,24,1)='-' AND substr(account_id,15,1)='7' AND substr(account_id,20,1) IN ('8','9','a','b') AND length(replace(account_id,'-',''))=32 AND replace(account_id,'-','') NOT GLOB '*[^0-9a-f]*'), username TEXT COLLATE BINARY NOT NULL UNIQUE CHECK(length(CAST(username AS BLOB)) BETWEEN {USERNAME_MIN_LEN} AND {USERNAME_MAX_LEN} AND instr(username,char(9))=0 AND instr(username,char(10))=0 AND instr(username,char(11))=0 AND instr(username,char(12))=0 AND instr(username,char(13))=0 AND instr(username,' ')=0),role TEXT NOT NULL CHECK(role IN ('host','admin')),status TEXT NOT NULL CHECK(status IN ('pending_enrollment','verified','reset_required')),verifier TEXT CHECK(verifier IS NULL OR length(verifier)<={MAX_PHC_LEN}),credential_epoch INTEGER NOT NULL CHECK(credential_epoch BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),password_set_at INTEGER CHECK(password_set_at IS NULL OR password_set_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),disabled_at INTEGER CHECK(disabled_at IS NULL OR disabled_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),CHECK(status='pending_enrollment' OR verifier IS NOT NULL)) STRICT"
        ),
        format!(
            "CREATE TABLE account_sessions(session_id TEXT PRIMARY KEY NOT NULL CHECK(length(CAST(session_id AS BLOB))=36 AND length(session_id)=36 AND session_id=lower(session_id) AND substr(session_id,9,1)='-' AND substr(session_id,14,1)='-' AND substr(session_id,19,1)='-' AND substr(session_id,24,1)='-' AND substr(session_id,15,1)='7' AND substr(session_id,20,1) IN ('8','9','a','b') AND length(replace(session_id,'-',''))=32 AND replace(session_id,'-','') NOT GLOB '*[^0-9a-f]*'),account_id TEXT NOT NULL REFERENCES accounts(account_id),token_verifier BLOB NOT NULL UNIQUE CHECK(typeof(token_verifier)='blob' AND length(token_verifier)={DIGEST_BYTES}),scope TEXT NOT NULL CHECK(scope IN ('normal','enrollment_only','password_reset_only')),credential_epoch INTEGER NOT NULL CHECK(credential_epoch BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),issued_at INTEGER NOT NULL CHECK(issued_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),expires_at INTEGER NOT NULL CHECK(expires_at<={JS_SAFE_INTEGER_MAX} AND expires_at>issued_at AND expires_at-issued_at<={SESSION_LIFETIME_MS}),revoked_at INTEGER CHECK(revoked_at IS NULL OR revoked_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX})) STRICT"
        ),
        "CREATE INDEX account_sessions_account ON account_sessions(account_id)".to_owned(),
        "CREATE INDEX account_sessions_expiry ON account_sessions(expires_at)".to_owned(),
        format!(
            "CREATE TABLE access_links(link_id TEXT PRIMARY KEY NOT NULL CHECK(length(CAST(link_id AS BLOB))=36 AND length(link_id)=36 AND link_id=lower(link_id) AND substr(link_id,9,1)='-' AND substr(link_id,14,1)='-' AND substr(link_id,19,1)='-' AND substr(link_id,24,1)='-' AND substr(link_id,15,1)='7' AND substr(link_id,20,1) IN ('8','9','a','b') AND length(replace(link_id,'-',''))=32 AND replace(link_id,'-','') NOT GLOB '*[^0-9a-f]*'),account_id TEXT NOT NULL,purpose TEXT NOT NULL CHECK(purpose IN ('enrollment','password_reset')),token_verifier BLOB NOT NULL UNIQUE CHECK(typeof(token_verifier)='blob' AND length(token_verifier)={DIGEST_BYTES}),credential_epoch INTEGER NOT NULL CHECK(credential_epoch BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),issued_at INTEGER NOT NULL CHECK(issued_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),expires_at INTEGER NOT NULL CHECK(expires_at<={JS_SAFE_INTEGER_MAX} AND expires_at-issued_at={ACCESS_LINK_LIFETIME_MS}),consumed_at INTEGER CHECK(consumed_at IS NULL OR consumed_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),revoked_at INTEGER CHECK(revoked_at IS NULL OR revoked_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX})) STRICT"
        ),
        "CREATE INDEX access_links_account ON access_links(account_id)".to_owned(),
        "CREATE INDEX access_links_expiry ON access_links(expires_at)".to_owned(),
        format!(
            "CREATE TABLE rate_limit_buckets(scope TEXT NOT NULL CHECK(scope IN ('account_login','access_link_redemption')),subject_key BLOB NOT NULL CHECK(typeof(subject_key)='blob' AND length(subject_key)={DIGEST_BYTES}),window_started_at INTEGER NOT NULL CHECK(window_started_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),attempt_count INTEGER NOT NULL CHECK(attempt_count BETWEEN 1 AND {RATE_LIMIT_MAX_FAILURES}),blocked_until INTEGER CHECK(blocked_until IS NULL OR blocked_until<={JS_SAFE_INTEGER_MAX} AND blocked_until>=window_started_at),expires_at INTEGER NOT NULL CHECK(expires_at<={JS_SAFE_INTEGER_MAX} AND expires_at>window_started_at),PRIMARY KEY(scope,subject_key)) STRICT"
        ),
        "CREATE INDEX rate_limit_expiry ON rate_limit_buckets(expires_at)".to_owned(),
        format!(
            "CREATE TABLE command_receipts(actor_account_id TEXT NOT NULL,command_id TEXT NOT NULL CHECK(length(CAST(command_id AS BLOB))=36 AND length(command_id)=36 AND command_id=lower(command_id) AND substr(command_id,9,1)='-' AND substr(command_id,14,1)='-' AND substr(command_id,19,1)='-' AND substr(command_id,24,1)='-' AND substr(command_id,15,1)='7' AND substr(command_id,20,1) IN ('8','9','a','b') AND length(replace(command_id,'-',''))=32 AND replace(command_id,'-','') NOT GLOB '*[^0-9a-f]*'),request_fingerprint BLOB NOT NULL CHECK(typeof(request_fingerprint)='blob' AND length(request_fingerprint)={DIGEST_BYTES}),outcome TEXT NOT NULL CHECK(length(CAST(outcome AS BLOB))<={COMMAND_RECEIPT_MAX_BYTES} AND json_valid(outcome)),completed_at INTEGER NOT NULL CHECK(completed_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),expires_at INTEGER NOT NULL CHECK(expires_at<={JS_SAFE_INTEGER_MAX} AND expires_at-completed_at={COMMAND_RECEIPT_RETENTION_MS}),PRIMARY KEY(actor_account_id,command_id)) STRICT"
        ),
        "CREATE INDEX command_receipts_expiry ON command_receipts(expires_at)".to_owned(),
        format!(
            "CREATE TABLE account_socket_subscriptions(account_id TEXT NOT NULL,session_id TEXT NOT NULL CHECK(length(CAST(session_id AS BLOB))=36 AND length(session_id)=36 AND session_id=lower(session_id) AND substr(session_id,9,1)='-' AND substr(session_id,14,1)='-' AND substr(session_id,19,1)='-' AND substr(session_id,24,1)='-' AND substr(session_id,15,1)='7' AND substr(session_id,20,1) IN ('8','9','a','b') AND length(replace(session_id,'-',''))=32 AND replace(session_id,'-','') NOT GLOB '*[^0-9a-f]*'),game_id TEXT NOT NULL CHECK(length(CAST(game_id AS BLOB))=36 AND length(game_id)=36 AND game_id=lower(game_id) AND substr(game_id,9,1)='-' AND substr(game_id,14,1)='-' AND substr(game_id,19,1)='-' AND substr(game_id,24,1)='-' AND substr(game_id,15,1)='7' AND substr(game_id,20,1) IN ('8','9','a','b') AND length(replace(game_id,'-',''))=32 AND replace(game_id,'-','') NOT GLOB '*[^0-9a-f]*'),connection_id TEXT PRIMARY KEY NOT NULL CHECK(length(CAST(connection_id AS BLOB))=36 AND length(connection_id)=36 AND connection_id=lower(connection_id) AND substr(connection_id,9,1)='-' AND substr(connection_id,14,1)='-' AND substr(connection_id,19,1)='-' AND substr(connection_id,24,1)='-' AND substr(connection_id,15,1)='7' AND substr(connection_id,20,1) IN ('8','9','a','b') AND length(replace(connection_id,'-',''))=32 AND replace(connection_id,'-','') NOT GLOB '*[^0-9a-f]*'),credential_epoch INTEGER NOT NULL CHECK(credential_epoch BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),expires_at INTEGER NOT NULL CHECK(expires_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX})) STRICT"
        ),
        "CREATE INDEX account_subscriptions_session ON account_socket_subscriptions(session_id)"
            .to_owned(),
        "CREATE INDEX account_subscriptions_account ON account_socket_subscriptions(account_id)"
            .to_owned(),
        "CREATE INDEX account_subscriptions_expiry ON account_socket_subscriptions(expires_at)"
            .to_owned(),
        format!(
            "CREATE TABLE account_socket_close_work(connection_id TEXT PRIMARY KEY NOT NULL REFERENCES account_socket_subscriptions(connection_id),operation_id TEXT NOT NULL CHECK(length(CAST(operation_id AS BLOB))=36 AND length(operation_id)=36 AND operation_id=lower(operation_id) AND substr(operation_id,9,1)='-' AND substr(operation_id,14,1)='-' AND substr(operation_id,19,1)='-' AND substr(operation_id,24,1)='-' AND substr(operation_id,15,1)='7' AND substr(operation_id,20,1) IN ('8','9','a','b') AND length(replace(operation_id,'-',''))=32 AND replace(operation_id,'-','') NOT GLOB '*[^0-9a-f]*'),account_id TEXT NOT NULL,session_id TEXT NOT NULL,game_id TEXT NOT NULL,credential_epoch INTEGER NOT NULL CHECK(credential_epoch BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),expires_at INTEGER NOT NULL CHECK(expires_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),next_attempt_at INTEGER NOT NULL CHECK(next_attempt_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),attempt_count INTEGER NOT NULL CHECK(attempt_count BETWEEN 0 AND 4294967295)) STRICT"
        ),
        "CREATE INDEX account_close_work_deadline ON account_socket_close_work(next_attempt_at)"
            .to_owned(),
        format!(
            "CREATE TRIGGER accounts_ascii_insert BEFORE INSERT ON accounts WHEN EXISTS (WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+2 FROM n WHERE i<{username_last_high_nibble}) SELECT 1 FROM n WHERE substr(hex(CAST(NEW.username AS BLOB)),i,1) IN ('8','9','A','B','C','D','E','F')) BEGIN SELECT RAISE(ABORT,'invalid account input'); END"
        ),
        format!(
            "CREATE TRIGGER accounts_ascii_update BEFORE UPDATE OF username ON accounts WHEN EXISTS (WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+2 FROM n WHERE i<{username_last_high_nibble}) SELECT 1 FROM n WHERE substr(hex(CAST(NEW.username AS BLOB)),i,1) IN ('8','9','A','B','C','D','E','F')) BEGIN SELECT RAISE(ABORT,'invalid account input'); END"
        ),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
mod tests {
    use super::*;
    use crate::{
        limits::{
            ACCESS_LINK_LIFETIME_MS, COMMAND_RECEIPT_MAX_BYTES, COMMAND_RECEIPT_RETENTION_MS,
            JS_SAFE_INTEGER_MAX, RATE_LIMIT_MAX_FAILURES, SESSION_LIFETIME_MS,
        },
        security::{DIGEST_BYTES, MAX_PHC_LEN},
    };
    use brews_domain::accounts::{USERNAME_MAX_LEN, USERNAME_MIN_LEN, validate_username};
    use rusqlite::{Connection, params};
    use sha2::{Digest, Sha256};

    const ACCOUNT_ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    const ENTITY_ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5712";

    fn connection() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        for sql in auth_schema_statements() {
            db.execute_batch(&sql).unwrap();
        }
        db.execute(
            "INSERT INTO accounts(account_id,username,role,status,credential_epoch,created_at) VALUES(?,?,'host','pending_enrollment',0,0)",
            params![ACCOUNT_ID, "x".repeat(USERNAME_MIN_LEN)],
        )
        .unwrap();
        db
    }

    #[test]
    fn auth_schema_ddl_matches_the_pre_refactor_snapshot() {
        // Length framing checks every byte and statement boundary, including order.
        let mut digest = Sha256::new();
        let statements = auth_schema_statements();
        assert_eq!(statements.len(), 19);
        for sql in statements {
            digest.update((sql.len() as u64).to_be_bytes());
            digest.update(sql.as_bytes());
        }
        assert_eq!(
            format!("{:x}", digest.finalize()),
            "0e7775e4cfff9f2f4bef8db73e5eea7557329870b121ae6e02f60996b52f97bd"
        );
    }

    #[test]
    fn sqlite_username_boundaries_match_domain_validation_on_insert_and_update() {
        let mut names: Vec<_> = [
            USERNAME_MIN_LEN - 1,
            USERNAME_MIN_LEN,
            USERNAME_MAX_LEN,
            USERNAME_MAX_LEN + 1,
        ]
        .into_iter()
        .map(|len| "x".repeat(len))
        .collect();
        for suffix in [
            '\0', '\u{7f}', '\u{80}', 'é', ' ', '\t', '\n', '\r', '\u{0b}', '\u{0c}',
        ] {
            names.push(format!(
                "{}{suffix}",
                "x".repeat(USERNAME_MAX_LEN - suffix.len_utf8())
            ));
        }
        for name in names {
            let db = connection();
            let valid = validate_username(&name).is_ok_and(|normalized| normalized == name);
            assert_eq!(
                db.execute(
                    "UPDATE accounts SET username=? WHERE account_id=?",
                    params![name, ACCOUNT_ID]
                )
                .is_ok(),
                valid,
                "update: {name:?}"
            );
            db.execute("DELETE FROM accounts", []).unwrap();
            assert_eq!(
                db.execute("INSERT INTO accounts(account_id,username,role,status,credential_epoch,created_at) VALUES(?,?,'host','pending_enrollment',0,0)", params![ACCOUNT_ID, name]).is_ok(),
                valid,
                "insert: {name:?}"
            );
        }
    }

    #[test]
    fn sqlite_account_caps_match_verifier_and_safe_integer_limits() {
        let db = connection();
        for len in [MAX_PHC_LEN, MAX_PHC_LEN + 1] {
            assert_eq!(
                db.execute(
                    "UPDATE accounts SET verifier=? WHERE account_id=?",
                    params!["x".repeat(len), ACCOUNT_ID]
                )
                .is_ok(),
                len <= MAX_PHC_LEN
            );
        }
        for value in [0, JS_SAFE_INTEGER_MAX, JS_SAFE_INTEGER_MAX + 1] {
            assert_eq!(
                db.execute("UPDATE accounts SET credential_epoch=?,created_at=?,password_set_at=?,disabled_at=? WHERE account_id=?", params![value, value, value, value, ACCOUNT_ID]).is_ok(),
                value <= JS_SAFE_INTEGER_MAX
            );
        }
    }

    #[test]
    fn sqlite_session_and_link_deadlines_use_their_own_policy_limits() {
        let db = connection();
        for lifetime in [0, 1, SESSION_LIFETIME_MS, SESSION_LIFETIME_MS + 1] {
            for len in [DIGEST_BYTES - 1, DIGEST_BYTES, DIGEST_BYTES + 1] {
                assert_eq!(
                    db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at) VALUES(?,?,?,'normal',0,0,?)", params![ENTITY_ID, ACCOUNT_ID, vec![0u8; len], lifetime]).is_ok(),
                    (1..=SESSION_LIFETIME_MS).contains(&lifetime) && len == DIGEST_BYTES
                );
                db.execute("DELETE FROM account_sessions", []).unwrap();
            }
        }
        for lifetime in [
            ACCESS_LINK_LIFETIME_MS - 1,
            ACCESS_LINK_LIFETIME_MS,
            ACCESS_LINK_LIFETIME_MS + 1,
        ] {
            for len in [DIGEST_BYTES - 1, DIGEST_BYTES, DIGEST_BYTES + 1] {
                assert_eq!(
                    db.execute("INSERT INTO access_links(link_id,account_id,purpose,token_verifier,credential_epoch,issued_at,expires_at) VALUES(?,?,'enrollment',?,0,0,?)", params![ENTITY_ID, ACCOUNT_ID, vec![0u8; len], lifetime]).is_ok(),
                    lifetime == ACCESS_LINK_LIFETIME_MS && len == DIGEST_BYTES
                );
                db.execute("DELETE FROM access_links", []).unwrap();
            }
        }
    }

    #[test]
    fn sqlite_receipts_match_payload_digest_and_retention_limits() {
        let db = connection();
        for bytes in [COMMAND_RECEIPT_MAX_BYTES, COMMAND_RECEIPT_MAX_BYTES + 1] {
            let outcome = serde_json::to_string(&"x".repeat(bytes - 2)).unwrap();
            for lifetime in [
                COMMAND_RECEIPT_RETENTION_MS - 1,
                COMMAND_RECEIPT_RETENTION_MS,
                COMMAND_RECEIPT_RETENTION_MS + 1,
            ] {
                for len in [DIGEST_BYTES - 1, DIGEST_BYTES, DIGEST_BYTES + 1] {
                    assert_eq!(
                        db.execute("INSERT INTO command_receipts(actor_account_id,command_id,request_fingerprint,outcome,completed_at,expires_at) VALUES(?,?,?,?,0,?)", params![ACCOUNT_ID, ENTITY_ID, vec![0u8; len], outcome, lifetime]).is_ok(),
                        bytes <= COMMAND_RECEIPT_MAX_BYTES && lifetime == COMMAND_RECEIPT_RETENTION_MS && len == DIGEST_BYTES
                    );
                    db.execute("DELETE FROM command_receipts", []).unwrap();
                }
            }
        }
    }

    #[test]
    fn sqlite_rate_buckets_match_failure_count_and_digest_limits() {
        let db = connection();
        for count in [0, 1, RATE_LIMIT_MAX_FAILURES, RATE_LIMIT_MAX_FAILURES + 1] {
            for len in [DIGEST_BYTES - 1, DIGEST_BYTES, DIGEST_BYTES + 1] {
                assert_eq!(
                    db.execute("INSERT INTO rate_limit_buckets(scope,subject_key,window_started_at,attempt_count,expires_at) VALUES('account_login',?,0,?,1)", params![vec![0u8; len], count]).is_ok(),
                    (1..=RATE_LIMIT_MAX_FAILURES).contains(&count) && len == DIGEST_BYTES
                );
                db.execute("DELETE FROM rate_limit_buckets", []).unwrap();
            }
        }
    }
}
