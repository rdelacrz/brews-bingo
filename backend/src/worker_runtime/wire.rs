//! Private, secret-bearing messages. Never exported through public DTOs.
use crate::auth::{AuthCommand, AuthError, AuthOutcome, CookieEffect};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerRequest {
    pub command: AuthCommand,
    pub command_id: Option<String>,
    pub caller_identity: String,
}
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum CookieWire {
    None,
    Set { token: String, expires_at: i64 },
    Clear,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerResponse {
    pub status: u16,
    pub body: serde_json::Value,
    pub cookie: CookieWire,
}
impl OwnerResponse {
    pub(super) fn outcome(result: Result<AuthOutcome, AuthError>) -> Self {
        match result {
            Ok(outcome) => Self {
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
                    status,
                    body: serde_json::json!({"error":{"code":code,"message":message}}),
                    cookie: CookieWire::None,
                }
            }
        }
    }
}
