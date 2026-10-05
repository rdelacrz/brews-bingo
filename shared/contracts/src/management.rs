//! Allocation-conscious, bounded management-only transport contracts.
use crate::SafeAccount;
use brews_domain::{
    accounts::AccessLinkPurpose,
    ids::{CommandId, LinkId},
};
use brews_domain::{
    accounts::AccountRole,
    ids::{AccountId, OperationId},
};
use serde::{Deserialize, Serialize};
use std::fmt;

pub use brews_domain::ids::AuditId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManagementCommand {
    ListAccounts {
        after: Option<AccountId>,
        limit: u32,
    },
    GetAccount {
        account_id: AccountId,
    },
    CreateAccount {
        username: String,
        #[serde(with = "role")]
        role: AccountRole,
    },
    ReissueEnrollment {
        account_id: AccountId,
    },
    ResetPassword {
        account_id: AccountId,
    },
    DisableAccount {
        account_id: AccountId,
    },
    DeleteAccount {
        account_id: AccountId,
    },
    EnableAccount {
        account_id: AccountId,
    },
    ListAudit {
        after: Option<AuditId>,
        limit: u32,
    },
}
mod role {
    use super::AccountRole;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &AccountRole, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(value)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<AccountRole, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptOperation {
    CreateAccount,
    ReissueEnrollment,
    ResetPassword,
    DisableAccount,
    DeleteAccount,
    EnableAccount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementReceipt {
    #[serde(deserialize_with = "receipt_version")]
    pub version: u8,
    pub command_id: CommandId,
    pub operation: ReceiptOperation,
    pub account_id: AccountId,
    pub link_id: Option<LinkId>,
    pub purpose: Option<AccessLinkPurpose>,
    pub link_expires_at: Option<i64>,
    pub completed_at: i64,
    pub expires_at: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActor {
    Account(AccountId),
    DeveloperCli,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Succeeded,
    Rejected,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOperation {
    ListAccounts,
    GetAccount,
    CreateAccount,
    ReissueEnrollment,
    ResetPassword,
    DisableAccount,
    DeleteAccount,
    EnableAccount,
    ListAudit,
}
impl From<ReceiptOperation> for AuditOperation {
    fn from(value: ReceiptOperation) -> Self {
        match value {
            ReceiptOperation::CreateAccount => Self::CreateAccount,
            ReceiptOperation::ReissueEnrollment => Self::ReissueEnrollment,
            ReceiptOperation::ResetPassword => Self::ResetPassword,
            ReceiptOperation::DisableAccount => Self::DisableAccount,
            ReceiptOperation::DeleteAccount => Self::DeleteAccount,
            ReceiptOperation::EnableAccount => Self::EnableAccount,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditTarget {
    Account(AccountId),
    AccountsOwner,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEvent {
    pub audit_id: AuditId,
    pub actor: AuditActor,
    pub operation: AuditOperation,
    pub target: AuditTarget,
    pub outcome: AuditOutcome,
    pub occurred_at: i64,
    pub expires_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManagementResponse {
    Accounts {
        accounts: Vec<SafeAccount>,
        next_cursor: Option<AccountId>,
    },
    Account {
        account: SafeAccount,
    },
    Issued {
        receipt: ManagementReceipt,
        url: String,
    },
    Committed {
        receipt: ManagementReceipt,
    },
    Audit {
        events: Vec<AuditEvent>,
        next_cursor: Option<AuditId>,
    },
    Pending {
        operation_id: OperationId,
    },
}
/// A malformed management response without parser details or input values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Invalid management response JSON.")]
pub struct ManagementResponseDecodeError;

impl ManagementResponse {
    /// Decodes an object-root JSON response, rejecting unknown and duplicate fields.
    ///
    /// # Allocation
    ///
    /// Allocates the owned response fields directly from the input; no intermediate
    /// JSON value is constructed. Callers must bound the transport body before decoding.
    ///
    /// # Errors
    ///
    /// Returns a fixed, source-free error for malformed JSON or response shapes.
    ///
    /// ```
    /// use brews_contracts::management::ManagementResponse;
    ///
    /// let response = ManagementResponse::decode_json(
    ///     br#"{"result":"accounts","accounts":[],"next_cursor":null}"#,
    /// );
    /// assert!(matches!(response, Ok(ManagementResponse::Accounts { .. })));
    /// ```
    pub fn decode_json(bytes: &[u8]) -> Result<Self, ManagementResponseDecodeError> {
        if bytes
            .iter()
            .find(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
            != Some(&b'{')
        {
            return Err(ManagementResponseDecodeError);
        }
        // Parsing the original bytes preserves Serde's duplicate-field rejection.
        serde_json::from_slice(bytes).map_err(|_| ManagementResponseDecodeError)
    }
}

impl fmt::Debug for ManagementResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Issued { receipt, .. } => f
                .debug_struct("Issued")
                .field("receipt", receipt)
                .field("url", &"[redacted]")
                .finish(),
            Self::Committed { receipt } => f
                .debug_struct("Committed")
                .field("receipt", receipt)
                .finish(),
            Self::Pending { operation_id } => f
                .debug_struct("Pending")
                .field("operation_id", operation_id)
                .finish(),
            Self::Accounts {
                accounts,
                next_cursor,
            } => f
                .debug_struct("Accounts")
                .field("accounts", accounts)
                .field("next_cursor", next_cursor)
                .finish(),
            Self::Account { account } => {
                f.debug_struct("Account").field("account", account).finish()
            }
            Self::Audit {
                events,
                next_cursor,
            } => f
                .debug_struct("Audit")
                .field("events", events)
                .field("next_cursor", next_cursor)
                .finish(),
        }
    }
}
impl ManagementCommand {
    pub const fn is_mutating(&self) -> bool {
        !matches!(
            self,
            Self::ListAccounts { .. } | Self::GetAccount { .. } | Self::ListAudit { .. }
        )
    }
}

fn receipt_version<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let version = u8::deserialize(deserializer)?;
    if version != 1 {
        return Err(serde::de::Error::custom("Unsupported receipt version."));
    }
    Ok(version)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Test fixtures fail without logging values."
)]
mod unit_tests {
    use super::*;

    #[test]
    fn json_response_requires_an_object_root() {
        let valid = br#" {"result":"accounts","accounts":[],"next_cursor":null} "#;
        assert!(matches!(
            ManagementResponse::decode_json(valid),
            Ok(ManagementResponse::Accounts { .. })
        ));
        assert!(ManagementResponse::decode_json(br#"["accounts",[],null]"#).is_err());
    }

    #[test]
    fn json_response_preserves_duplicate_field_rejection() {
        for bytes in [
            br#"{"result":"accounts","accounts":[],"accounts":[],"next_cursor":null}"#.as_slice(),
            br#"{"result":"accounts","result":"accounts","accounts":[],"next_cursor":null}"#,
            br#"{"result":"accounts","accounts":[],"next_cursor":null,"next_cursor":null}"#,
            br#"{"result":"account","account":{"account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","username":"accountname","username":"accountname","role":"Host","status":"Verified","disabled":false,"created_at":1}}"#,
        ] {
            assert!(ManagementResponse::decode_json(bytes).is_err());
        }
    }

    #[test]
    fn json_response_rejects_unknown_fields_and_invalid_shapes() {
        for bytes in [
            b"".as_slice(),
            b" \t\r\n",
            b"null",
            b"42",
            b"\x0b{\"result\":\"accounts\",\"accounts\":[],\"next_cursor\":null}",
            br#"{"result":"accounts","accounts":[],"next_cursor":null,"unknown":null}"#,
            br#"{"result":"accounts","accounts":{},"next_cursor":null}"#,
            br#"{"result":"accounts","accounts":[],"next_cursor":null}{}"#,
            br#"{"result":"accounts","accounts":[],"next_cursor":null"#,
        ] {
            assert!(ManagementResponse::decode_json(bytes).is_err());
        }
    }

    #[test]
    fn json_decode_errors_discard_values_and_parser_sources() {
        use std::hash::BuildHasher as _;
        let marker = std::collections::hash_map::RandomState::new()
            .hash_one("response-redaction-probe")
            .to_string();
        let bytes = format!(r#"{{"result":"{marker}"}}"#);
        let error = ManagementResponse::decode_json(bytes.as_bytes()).unwrap_err();
        assert_eq!(error, ManagementResponseDecodeError);
        assert_eq!(error.to_string(), "Invalid management response JSON.");
        assert_eq!(format!("{error:?}"), "ManagementResponseDecodeError");
        assert!(!format!("{error} {error:?}").contains(&marker));
        assert!(std::error::Error::source(&error).is_none());
    }
}
