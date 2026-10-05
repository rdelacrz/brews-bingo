//! Trusted internal auth transport; never a public app DTO.
use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::Zeroize;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthCommand {
    Login {
        username: String,
        password: String,
    },
    Current {
        token: Option<String>,
    },
    Redeem {
        purpose: AccessLinkPurpose,
        token: String,
    },
    Complete {
        scope: SessionScope,
        token: String,
        new_password: String,
    },
    Logout {
        token: Option<String>,
    },
}
impl fmt::Debug for AuthCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Login { .. } => "Login([redacted])",
            Self::Current { .. } => "Current([redacted])",
            Self::Redeem { .. } => "Redeem([redacted])",
            Self::Complete { .. } => "Complete([redacted])",
            Self::Logout { .. } => "Logout([redacted])",
        })
    }
}
impl Drop for AuthCommand {
    fn drop(&mut self) {
        match self {
            Self::Login { password, .. } => password.zeroize(),
            Self::Current { token } | Self::Logout { token } => token.zeroize(),
            Self::Redeem { token, .. } => token.zeroize(),
            Self::Complete {
                token,
                new_password,
                ..
            } => {
                token.zeroize();
                new_password.zeroize();
            }
        }
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
mod tests {
    use super::*;
    #[test]
    fn commands_do_not_debug_credentials_and_wire_tags_are_explicit() {
        let password = String::from_utf8(vec![120; 12]).unwrap();
        let command: AuthCommand = serde_json::from_value(
            serde_json::json!({"type":"login","username":"abcdefghij","password":password}),
        )
        .unwrap();
        assert_eq!(format!("{command:?}"), "Login([redacted])");
        assert_eq!(serde_json::to_value(&command).unwrap()["type"], "login");
        assert!(
            serde_json::from_value::<AuthCommand>(
                serde_json::json!({"type":"logout","token":null,"account_id":"unexpected"})
            )
            .is_err()
        );
    }
}
