//! Private, secret-bearing messages. Never exported through public DTOs.
use crate::auth::{AuthCommand, AuthError, AuthOutcome, CookieEffect};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerRequest {
    #[serde(deserialize_with = "super::game_peers::object")]
    pub command: AuthCommand,
    pub command_id: Option<String>,
    pub caller_identity: String,
}
#[derive(Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum CookieWire {
    None,
    Set { token: String, expires_at: i64 },
    Clear,
}
impl<'de> Deserialize<'de> for CookieWire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Object {
            None {},
            Set { token: String, expires_at: i64 },
            Clear {},
        }
        Ok(match Object::deserialize(deserializer)? {
            Object::None {} => Self::None,
            Object::Set { token, expires_at } => Self::Set { token, expires_at },
            Object::Clear {} => Self::Clear,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerResponse {
    pub status: u16,
    #[serde(deserialize_with = "auth_body")]
    pub body: serde_json::Value,
    #[serde(deserialize_with = "super::game_peers::object")]
    pub cookie: CookieWire,
    #[serde(deserialize_with = "super::game_peers::optional_object")]
    pub authority: Option<crate::auth::AuthOutputAuthority>,
}
fn session_field<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<brews_contracts::SessionView>>, D::Error> {
    super::game_peers::optional_object(deserializer).map(Some)
}
fn auth_body<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<serde_json::Value, D::Error> {
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ErrorFields {
        code: String,
        message: String,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Receipt {
        version: u32,
        command_id: brews_domain::ids::CommandId,
        #[serde(deserialize_with = "super::game_peers::object")]
        outcome: serde_json::Value,
        completed_at: i64,
        expires_at: i64,
    }
    #[derive(Default, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Body {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authenticated: Option<bool>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "super::game_peers::optional_object"
        )]
        account: Option<brews_contracts::SafeAccount>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "session_field"
        )]
        session: Option<Option<brews_contracts::SessionView>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        setup_required: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reset_required: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reset_completed: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_action: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        logged_out: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replayed: Option<bool>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "super::game_peers::optional_object"
        )]
        receipt: Option<Receipt>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "super::game_peers::optional_object"
        )]
        error: Option<ErrorFields>,
    }
    let body: Body = super::game_peers::object(deserializer)?;
    serde_json::to_value(body).map_err(serde::de::Error::custom)
}
impl OwnerResponse {
    pub(super) fn anonymous() -> Self {
        Self {
            status: 200,
            body: serde_json::json!({"authenticated":false,"session":null}),
            cookie: CookieWire::Clear,
            authority: None,
        }
    }
    pub(super) fn outcome(result: Result<AuthOutcome, AuthError>) -> Self {
        match result {
            Ok(outcome) => Self {
                authority: None,
                status: 200,
                body: outcome.body,
                cookie: match outcome.cookie {
                    CookieEffect::None => CookieWire::None,
                    CookieEffect::Clear => CookieWire::Clear,
                    CookieEffect::Set { token, expires_at } => {
                        CookieWire::Set { token, expires_at }
                    }
                },
            },
            Err(error) => {
                let (status, code, message) = match error {
                    AuthError::InvalidInput => (400, "invalid_input", "invalid request"),
                    AuthError::InvalidCredentials => {
                        (401, "invalid_credentials", "invalid credentials")
                    }
                    AuthError::InvalidLink => (400, "invalid_link", "invalid link"),
                    AuthError::Unauthorized => (401, "unauthorized", "authentication required"),
                    AuthError::RateLimited => (
                        429,
                        "rate_limited",
                        "authentication temporarily rate limited",
                    ),
                    AuthError::Conflict => (
                        409,
                        "command_conflict",
                        "command conflicts with an earlier request",
                    ),
                    AuthError::StaleCommand => {
                        (409, "stale_command", "command is no longer admissible")
                    }
                    AuthError::Storage | AuthError::Crypto => {
                        (503, "service_unavailable", "service unavailable")
                    }
                };
                Self {
                    authority: None,
                    status,
                    body: serde_json::json!({"error":{"code":code,"message":message}}),
                    cookie: CookieWire::None,
                }
            }
        }
    }
}
