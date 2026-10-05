//! Allocation-conscious public Users contracts; issuance URLs are never replayed.
use crate::{SafeAccount, management::ManagementReceipt};
use brews_domain::{
    accounts::AccountStatus,
    ids::{AccountId, OperationId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUsername {
    pub username: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum UserResponse {
    Users {
        #[serde(deserialize_with = "deserialize_users")]
        users: Vec<SafeAccount>,
        next_cursor: Option<AccountId>,
    },
    Account {
        #[serde(deserialize_with = "deserialize_object")]
        account: SafeAccount,
    },
    Created {
        #[serde(deserialize_with = "deserialize_object")]
        account: SafeAccount,
        enrollment_url: String,
        link_expires_at: i64,
        #[serde(deserialize_with = "deserialize_object")]
        receipt: ManagementReceipt,
    },
    EnrollmentLink {
        account_id: AccountId,
        enrollment_url: String,
        link_expires_at: i64,
        #[serde(deserialize_with = "deserialize_object")]
        receipt: ManagementReceipt,
    },
    PasswordReset {
        account_id: AccountId,
        status: AccountStatus,
        reset_url: String,
        link_expires_at: i64,
        #[serde(deserialize_with = "deserialize_object")]
        receipt: ManagementReceipt,
    },
    Disabled {
        account_id: AccountId,
        status: AccountStatus,
        disabled: bool,
        #[serde(deserialize_with = "deserialize_object")]
        receipt: ManagementReceipt,
    },
    Deleted {
        account_id: AccountId,
        deleted: bool,
        #[serde(deserialize_with = "deserialize_object")]
        receipt: ManagementReceipt,
    },
    Enabled {
        account_id: AccountId,
        status: AccountStatus,
        disabled: bool,
        #[serde(deserialize_with = "deserialize_object")]
        receipt: ManagementReceipt,
    },
    Committed {
        #[serde(deserialize_with = "deserialize_object")]
        receipt: ManagementReceipt,
    },
    Pending {
        operation_id: OperationId,
    },
}

fn deserialize_users<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SafeAccount>, D::Error> {
    #[derive(Deserialize)]
    struct ObjectAccount(#[serde(deserialize_with = "deserialize_object")] SafeAccount);

    Vec::<ObjectAccount>::deserialize(deserializer)
        .map(|accounts| accounts.into_iter().map(|account| account.0).collect())
}

// Delegate the original map entries so duplicates and unknown fields stay visible.
fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a JSON object")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Invalid Users response JSON.")]
pub struct UserResponseDecodeError;

impl UserResponse {
    pub const fn status(&self) -> u16 {
        if matches!(self, Self::Pending { .. }) {
            202
        } else {
            200
        }
    }
    /// Decode bounded original JSON bytes, retaining duplicate-field rejection.
    /// Allocates owned fields; errors discard input and parser details.
    pub fn decode_json(bytes: &[u8]) -> Result<Self, UserResponseDecodeError> {
        if bytes
            .iter()
            .find(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
            != Some(&b'{')
        {
            return Err(UserResponseDecodeError);
        }
        serde_json::from_slice(bytes).map_err(|_| UserResponseDecodeError)
    }
}

impl std::fmt::Debug for UserResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Users { users, next_cursor } => f
                .debug_struct("Users")
                .field("users", users)
                .field("next_cursor", next_cursor)
                .finish(),
            Self::Account { account } => {
                f.debug_struct("Account").field("account", account).finish()
            }
            Self::Created {
                account, receipt, ..
            } => f
                .debug_struct("Created")
                .field("account", account)
                .field("receipt", receipt)
                .field("enrollment_url", &"[redacted]")
                .finish_non_exhaustive(),
            Self::EnrollmentLink { receipt, .. } => f
                .debug_struct("EnrollmentLink")
                .field("receipt", receipt)
                .field("enrollment_url", &"[redacted]")
                .finish_non_exhaustive(),
            Self::PasswordReset { receipt, .. } => f
                .debug_struct("PasswordReset")
                .field("receipt", receipt)
                .field("reset_url", &"[redacted]")
                .finish_non_exhaustive(),
            Self::Disabled { receipt, .. } => f
                .debug_struct("Disabled")
                .field("receipt", receipt)
                .finish_non_exhaustive(),
            Self::Deleted { receipt, .. } => f
                .debug_struct("Deleted")
                .field("receipt", receipt)
                .finish_non_exhaustive(),
            Self::Enabled { receipt, .. } => f
                .debug_struct("Enabled")
                .field("receipt", receipt)
                .finish_non_exhaustive(),
            Self::Committed { receipt } => f
                .debug_struct("Committed")
                .field("receipt", receipt)
                .finish(),
            Self::Pending { operation_id } => f
                .debug_struct("Pending")
                .field("operation_id", operation_id)
                .finish(),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Bounded public contract fixtures fail fast."
)]
mod tests {
    use super::*;

    const ACCOUNT_OBJECT: &str = r#"{"account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","username":"ExactCaseUser","role":"Host","status":"Verified","disabled":false,"created_at":1}"#;
    const ACCOUNT_SEQUENCE: &str =
        r#"["01890f3e-53b7-7d28-9b05-4f65092d5711","ExactCaseUser","Host","Verified",false,1]"#;
    const RECEIPT_OBJECT: &str = r#"{"version":1,"command_id":"01890f3e-53b7-7d28-9b05-4f65092d5712","operation":"create_account","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","link_id":null,"purpose":null,"link_expires_at":null,"completed_at":1,"expires_at":2}"#;

    const RECEIPT_SEQUENCE: &str = r#"[1,"01890f3e-53b7-7d28-9b05-4f65092d5712","create_account","01890f3e-53b7-7d28-9b05-4f65092d5711",null,null,null,1,2]"#;

    fn responses_with_receipt(receipt: &str) -> [String; 7] {
        [
            format!(r#"{{"result":"created","account":{ACCOUNT_OBJECT},"enrollment_url":"https://app.example.test/enroll","link_expires_at":2"#),
            r#"{"result":"enrollment_link","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","enrollment_url":"https://app.example.test/enroll","link_expires_at":2"#.to_owned(),
            r#"{"result":"password_reset","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","status":"ResetRequired","reset_url":"https://app.example.test/reset","link_expires_at":2"#.to_owned(),
            r#"{"result":"disabled","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","status":"Verified","disabled":true"#.to_owned(),
            r#"{"result":"deleted","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","deleted":true"#.to_owned(),
            r#"{"result":"enabled","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","status":"Verified","disabled":false"#.to_owned(),
            r#"{"result":"committed""#.to_owned(),
        ].map(|prefix| format!("{prefix},\"receipt\":{receipt}}}"))
    }

    fn assert_invalid_response(bytes: &[u8]) {
        let error = UserResponse::decode_json(bytes).unwrap_err();
        assert_eq!(error, UserResponseDecodeError);
        assert!(std::error::Error::source(&error).is_none());
        assert!(serde_json::from_slice::<UserResponse>(bytes).is_err());
    }

    #[test]
    fn every_public_variant_round_trips_with_safe_debug_and_http_status() {
        use std::hash::BuildHasher as _;
        let marker = std::collections::hash_map::RandomState::new()
            .hash_one("users-redaction-probe")
            .to_string();
        let url = format!("https://app.example.test/enroll#{marker}");
        let account = serde_json::json!({"account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","username":"HostPerson01","role":"Host","status":"PendingEnrollment","disabled":false,"created_at":1});
        let receipt = serde_json::json!({"version":1,"command_id":"01890f3e-53b7-7d28-9b05-4f65092d5712","operation":"create_account","account_id":account["account_id"],"link_id":null,"purpose":null,"link_expires_at":null,"completed_at":1,"expires_at":2});
        let id = account["account_id"].clone();
        for (value, status) in [
            (
                serde_json::json!({"result":"users","users":[],"next_cursor":null}),
                200,
            ),
            (
                serde_json::json!({"result":"account","account":account}),
                200,
            ),
            (
                serde_json::json!({"result":"created","account":account,"enrollment_url":url,"link_expires_at":2,"receipt":receipt}),
                200,
            ),
            (
                serde_json::json!({"result":"enrollment_link","account_id":id,"enrollment_url":url,"link_expires_at":2,"receipt":receipt}),
                200,
            ),
            (
                serde_json::json!({"result":"password_reset","account_id":id,"status":"ResetRequired","reset_url":url,"link_expires_at":2,"receipt":receipt}),
                200,
            ),
            (
                serde_json::json!({"result":"disabled","account_id":id,"status":"Verified","disabled":true,"receipt":receipt}),
                200,
            ),
            (
                serde_json::json!({"result":"deleted","account_id":id,"deleted":true,"receipt":receipt}),
                200,
            ),
            (
                serde_json::json!({"result":"enabled","account_id":id,"status":"Verified","disabled":false,"receipt":receipt}),
                200,
            ),
            (
                serde_json::json!({"result":"committed","receipt":receipt}),
                200,
            ),
            (
                serde_json::json!({"result":"pending","operation_id":id}),
                202,
            ),
        ] {
            let response = UserResponse::decode_json(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(response.status(), status);
            assert!(!format!("{response:?}").contains(&marker));
            assert!(!format!("{response:?}").contains(&url));
            assert!(serde_json::to_value(response).unwrap() == value);
        }
    }

    #[test]
    fn public_decoder_rejects_nonobjects_duplicates_and_unknown_fields_without_sources() {
        assert!(matches!(
            UserResponse::decode_json(br#" {"result":"users","users":[],"next_cursor":null} "#),
            Ok(UserResponse::Users { .. })
        ));
        for bytes in [
            b"".as_slice(), b"null", br#"["users",[],null]"#,
            br#"{"result":"users","users":[],"users":[],"next_cursor":null}"#,
            br#"{"result":"users","result":"users","users":[],"next_cursor":null}"#,
            br#"{"result":"users","users":[],"next_cursor":null,"secret":"hidden"}"#,
            br#"{"result":"account","account":{"account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","username":"HostPerson01","username":"HostPerson01","role":"Host","status":"Verified","disabled":false,"created_at":1}}"#,
            br#"{"result":"users","users":[],"next_cursor":null}{}"#,
        ] {
            assert!(UserResponse::decode_json(bytes).is_err());
        }
        let error = UserResponse::decode_json(b"null").err().unwrap();
        assert_eq!(error.to_string(), "Invalid Users response JSON.");
        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn account_fields_reject_positional_sequences() {
        for bytes in [
            format!(r#"{{"result":"account","account":{ACCOUNT_SEQUENCE}}}"#),
            format!(
                r#"{{"result":"created","account":{ACCOUNT_SEQUENCE},"enrollment_url":"https://app.example.test/enroll","link_expires_at":2,"receipt":{RECEIPT_OBJECT}}}"#
            ),
        ] {
            assert_invalid_response(bytes.as_bytes());
        }
    }

    #[test]
    fn users_list_elements_reject_positional_sequences() {
        for users in [
            format!("[{ACCOUNT_SEQUENCE}]"),
            format!("[{ACCOUNT_OBJECT},{ACCOUNT_SEQUENCE}]"),
        ] {
            let bytes = format!(r#"{{"result":"users","users":{users},"next_cursor":null}}"#);
            assert_invalid_response(bytes.as_bytes());
        }
    }

    #[test]
    fn receipt_fields_reject_positional_sequences() {
        for bytes in responses_with_receipt(RECEIPT_SEQUENCE) {
            assert_invalid_response(bytes.as_bytes());
        }
    }

    #[test]
    fn nested_objects_preserve_exact_wire_bytes() {
        for (index, bytes) in responses_with_receipt(RECEIPT_OBJECT)
            .into_iter()
            .chain([
                format!(r#"{{"result":"account","account":{ACCOUNT_OBJECT}}}"#),
                format!(r#"{{"result":"users","users":[{ACCOUNT_OBJECT}],"next_cursor":null}}"#),
            ])
            .enumerate()
        {
            let response = UserResponse::decode_json(bytes.as_bytes());
            assert!(response.is_ok(), "fixture {index}");
            let response = response.unwrap();
            assert_eq!(serde_json::to_vec(&response).unwrap(), bytes.as_bytes());
            let direct = serde_json::from_slice::<UserResponse>(bytes.as_bytes()).unwrap();
            assert_eq!(serde_json::to_vec(&direct).unwrap(), bytes.as_bytes());
        }
    }

    #[test]
    fn nested_accounts_preserve_duplicate_and_unknown_field_rejection() {
        for account in [
            ACCOUNT_OBJECT.replace(
                r#""username":"ExactCaseUser""#,
                r#""username":"ExactCaseUser","username":"ExactCaseUser""#,
            ),
            ACCOUNT_OBJECT.replace(
                r#""username":"ExactCaseUser""#,
                r#""username":"ExactCaseUser","user\u006eame":"ExactCaseUser""#,
            ),
            ACCOUNT_OBJECT.replace(r#""created_at":1"#, r#""created_at":1,"unknown":null"#),
        ] {
            for bytes in [
                format!(r#"{{"result":"account","account":{account}}}"#),
                format!(
                    r#"{{"result":"users","users":[{ACCOUNT_OBJECT},{account}],"next_cursor":null}}"#
                ),
                format!(
                    r#"{{"result":"created","account":{account},"enrollment_url":"https://app.example.test/enroll","link_expires_at":2,"receipt":{RECEIPT_OBJECT}}}"#
                ),
            ] {
                assert_invalid_response(bytes.as_bytes());
            }
        }
    }

    #[test]
    fn nested_receipts_preserve_nullable_duplicates_and_unknown_field_rejection() {
        for (field, escaped) in [
            ("link_id", r"link\u005fid"),
            ("purpose", r"pur\u0070ose"),
            ("link_expires_at", r"link_expires_\u0061t"),
        ] {
            for duplicate in [field, escaped] {
                let receipt = RECEIPT_OBJECT.replace(
                    &format!(r#""{field}":null"#),
                    &format!(r#""{field}":null,"{duplicate}":null"#),
                );
                for bytes in responses_with_receipt(&receipt) {
                    assert_invalid_response(bytes.as_bytes());
                }
            }
        }
        let receipt =
            RECEIPT_OBJECT.replace(r#""expires_at":2"#, r#""expires_at":2,"unknown":null"#);
        for bytes in responses_with_receipt(&receipt) {
            assert_invalid_response(bytes.as_bytes());
        }
        for bytes in [
            br#"{"result":"users","users":[],"next_cursor":null,"next_cursor":null}"#.as_slice(),
            br#"{"result":"users","users":[],"next_cursor":null,"next\u005fcursor":null}"#,
            br#"{"result":"users","res\u0075lt":"users","users":[],"next_cursor":null}"#,
        ] {
            assert_invalid_response(bytes);
        }
    }

    #[test]
    fn nested_objects_reject_other_nonobject_shapes() {
        for shape in ["null", "true", "42", r#""object""#, "[]", "{}"] {
            for bytes in [
                format!(r#"{{"result":"account","account":{shape}}}"#),
                format!(r#"{{"result":"users","users":[{shape}],"next_cursor":null}}"#),
                format!(r#"{{"result":"created","account":{shape},"enrollment_url":"https://app.example.test/enroll","link_expires_at":2,"receipt":{RECEIPT_OBJECT}}}"#),
            ].into_iter().chain(responses_with_receipt(shape)) {
                assert_invalid_response(bytes.as_bytes());
            }
        }
        assert_invalid_response(br#"{"result":"users","users":{},"next_cursor":null}"#);
    }

    #[test]
    fn users_adapters_leave_shared_management_decoding_compatible() {
        use crate::management::ManagementResponse;
        assert!(serde_json::from_str::<SafeAccount>(ACCOUNT_SEQUENCE).is_ok());
        assert!(serde_json::from_str::<ManagementReceipt>(RECEIPT_SEQUENCE).is_ok());
        let account = format!(r#"{{"result":"account","account":{ACCOUNT_SEQUENCE}}}"#);
        assert!(ManagementResponse::decode_json(account.as_bytes()).is_ok());
        let receipt = format!(r#"{{"result":"committed","receipt":{RECEIPT_SEQUENCE}}}"#);
        assert!(ManagementResponse::decode_json(receipt.as_bytes()).is_ok());
    }

    #[test]
    fn users_page_round_trips_the_public_users_field() {
        let response = UserResponse::Users {
            users: Vec::new(),
            next_cursor: None,
        };
        let bytes = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            bytes,
            br#"{"result":"users","users":[],"next_cursor":null}"#
        );
        assert!(
            matches!(serde_json::from_slice::<UserResponse>(&bytes), Ok(UserResponse::Users { users, next_cursor: None }) if users.is_empty())
        );
    }
}
