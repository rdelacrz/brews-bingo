//! Additive v2 management storage; v1 DDL is deliberately unchanged.
use crate::limits::{
    COMMAND_RECEIPT_MAX_BYTES, COMMAND_RECEIPT_RETENTION_MS, JS_SAFE_INTEGER_MAX,
    MILLISECONDS_PER_DAY,
};
use crate::security::DIGEST_BYTES;
pub(crate) const LINK_RECEIPT_RETENTION_MS: i64 = 30 * MILLISECONDS_PER_DAY;
pub(crate) const AUDIT_RETENTION_MS: i64 = 90 * MILLISECONDS_PER_DAY;
pub(crate) const REMOVAL_INITIAL_DELAY_MS: i64 = 1_000;
pub(crate) const REMOVAL_MAX_DELAY_MS: i64 = 300_000;

fn id_check(name: &str) -> String {
    format!(
        "length(CAST({name} AS BLOB))=36 AND {name}=lower({name}) AND substr({name},9,1)='-' AND substr({name},14,1)='-' AND substr({name},19,1)='-' AND substr({name},24,1)='-' AND substr({name},15,1)='7' AND substr({name},20,1) IN ('8','9','a','b') AND length(replace({name},'-',''))=32 AND replace({name},'-','') NOT GLOB '*[^0-9a-f]*'"
    )
}
pub(super) fn v2_statements() -> Vec<String> {
    let command = id_check("command_id");
    let target = id_check("target_account_id");
    let actor = id_check("actor");
    let operation = id_check("operation_id");
    let audit = id_check("audit_id");
    vec![
        format!(
            "CREATE TABLE management_receipts(actor TEXT NOT NULL CHECK(actor='developer_cli' OR ({actor})),command_id TEXT NOT NULL CHECK({command}),request_fingerprint BLOB NOT NULL CHECK(typeof(request_fingerprint)='blob' AND length(request_fingerprint)={DIGEST_BYTES}),outcome TEXT NOT NULL CHECK(length(CAST(outcome AS BLOB))<={COMMAND_RECEIPT_MAX_BYTES} AND json_valid(outcome)),completed_at INTEGER NOT NULL CHECK(completed_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),expires_at INTEGER NOT NULL CHECK(expires_at<={JS_SAFE_INTEGER_MAX} AND expires_at-completed_at IN ({COMMAND_RECEIPT_RETENTION_MS},{LINK_RECEIPT_RETENTION_MS})),operation_id TEXT UNIQUE CHECK(operation_id IS NULL OR ({operation})),PRIMARY KEY(actor,command_id)) STRICT"
        ),
        "CREATE INDEX management_receipts_expiry ON management_receipts(expires_at)".into(),
        format!(
            "CREATE TABLE admin_audit(audit_id TEXT PRIMARY KEY NOT NULL CHECK({audit}),actor TEXT NOT NULL CHECK(actor='developer_cli' OR ({actor})),operation TEXT NOT NULL CHECK(operation IN ('list_accounts','get_account','create_account','reissue_enrollment','reset_password','disable_account','delete_account','enable_account','list_audit')),target_kind TEXT NOT NULL CHECK(target_kind IN ('account','accounts_owner')),target_account_id TEXT CHECK(target_account_id IS NULL OR ({target})),outcome TEXT NOT NULL CHECK(outcome IN ('succeeded','rejected','failed')),occurred_at INTEGER NOT NULL CHECK(occurred_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),expires_at INTEGER NOT NULL CHECK(expires_at<={JS_SAFE_INTEGER_MAX} AND expires_at-occurred_at={AUDIT_RETENTION_MS}),CHECK((target_kind='account' AND target_account_id IS NOT NULL) OR (target_kind='accounts_owner' AND target_account_id IS NULL))) STRICT"
        ),
        "CREATE INDEX admin_audit_expiry ON admin_audit(expires_at)".into(),
        format!(
            "CREATE TABLE pending_account_removals(operation_id TEXT PRIMARY KEY NOT NULL CHECK({operation}),actor TEXT NOT NULL CHECK(actor='developer_cli' OR ({actor})),command_id TEXT NOT NULL CHECK({command}),target_account_id TEXT NOT NULL UNIQUE CHECK({target}),operation TEXT NOT NULL CHECK(operation IN ('disable_account','delete_account')),request_fingerprint BLOB NOT NULL CHECK(typeof(request_fingerprint)='blob' AND length(request_fingerprint)={DIGEST_BYTES}),phase TEXT NOT NULL CHECK(phase IN ('prepared','committed','aborting')),created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),next_attempt_at INTEGER NOT NULL CHECK(next_attempt_at BETWEEN 0 AND {JS_SAFE_INTEGER_MAX}),attempt_count INTEGER NOT NULL CHECK(attempt_count BETWEEN 0 AND 4294967295),UNIQUE(actor,command_id)) STRICT"
        ),
        "CREATE INDEX pending_removals_deadline ON pending_account_removals(next_attempt_at)"
            .into(),
    ]
}
