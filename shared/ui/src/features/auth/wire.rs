use crate::client::optional_object;
use brews_contracts::{SafeAccount, SessionView};
use brews_domain::accounts::{AccountStatus, SessionScope};
use brews_domain::ids::{AccountId, CommandId, LinkId, SessionId};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuthReply {
    #[serde(default, deserialize_with = "optional_object")]
    pub account: Option<SafeAccount>,
    #[serde(default, deserialize_with = "optional_object")]
    pub session: Option<SessionView>,
    #[serde(default)]
    pub setup_required: Option<bool>,
    #[serde(default)]
    pub reset_required: Option<bool>,
    #[serde(default)]
    pub reset_completed: Option<bool>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub replayed: Option<bool>,
    #[serde(default, deserialize_with = "optional_object")]
    pub receipt: Option<Receipt>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    version: u32,
    command_id: CommandId,
    outcome: ReceiptOutcome,
    completed_at: i64,
    expires_at: i64,
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "Full wire validation; bearer credentials are absent."
)]
enum ReceiptOutcome {
    Login {
        account_id: AccountId,
        session_id: SessionId,
        session_expires_at: i64,
    },
    Redeem {
        account_id: AccountId,
        link_id: LinkId,
        session_id: SessionId,
        session_expires_at: i64,
    },
    CompleteEnrollment {
        account_id: AccountId,
    },
    CompletePasswordReset {
        account_id: AccountId,
    },
}
#[derive(Clone, Copy)]
pub(super) enum Operation {
    Login,
    Redeem,
    Enrollment,
    Reset,
}
impl AuthReply {
    pub fn normal_reply(&self) -> bool {
        matches!((&self.account, &self.session), (Some(account), Some(session))
            if !account.disabled && account.status == AccountStatus::Verified && session.scope == SessionScope::Normal
                && account.account_id == session.account_id && account.role == session.role)
            && self.setup_required.is_none()
            && self.reset_required.is_none()
            && self.reset_completed.is_none()
            && self.next_action.is_none()
            && self.replayed.is_none()
            && self.receipt.is_none()
    }
    pub fn restricted_reply(&self, reset: bool) -> bool {
        self.account.is_none()
            && self.session.as_ref().is_some_and(|session| {
                session.scope
                    == if reset {
                        SessionScope::PasswordResetOnly
                    } else {
                        SessionScope::EnrollmentOnly
                    }
            })
            && if reset {
                self.reset_required == Some(true) && self.setup_required.is_none()
            } else {
                self.setup_required == Some(true) && self.reset_required.is_none()
            }
            && self.reset_completed.is_none()
            && self.next_action.is_none()
            && self.replayed.is_none()
            && self.receipt.is_none()
    }
    pub fn reset_reply(&self) -> bool {
        self.reset_completed == Some(true)
            && self.next_action.as_deref() == Some("login")
            && self.account.is_none()
            && self.session.is_none()
            && self.setup_required.is_none()
            && self.reset_required.is_none()
            && self.replayed.is_none()
            && self.receipt.is_none()
    }
    pub fn replay(
        &self,
        operation: Operation,
        command_id: &str,
        expected_account: Option<AccountId>,
        now: i64,
    ) -> bool {
        self.replayed == Some(true)
            && self.account.is_none()
            && self.session.is_none()
            && self.setup_required.is_none()
            && self.reset_required.is_none()
            && self.reset_completed.is_none()
            && self.next_action.is_none()
            && self.receipt.as_ref().is_some_and(|receipt| {
                receipt.version == 1
                    && command_id.parse::<CommandId>().ok() == Some(receipt.command_id)
                    && expected_account.is_none_or(|expected| self.account_id() == Some(expected))
                    && (!matches!(operation, Operation::Enrollment | Operation::Reset)
                        || expected_account.is_some())
                    && receipt.completed_at >= 0
                    && receipt.expires_at > receipt.completed_at
                    && receipt.expires_at > now
                    && matches!(
                        (operation, &receipt.outcome),
                        (Operation::Login, ReceiptOutcome::Login { .. })
                            | (Operation::Redeem, ReceiptOutcome::Redeem { .. })
                            | (
                                Operation::Enrollment,
                                ReceiptOutcome::CompleteEnrollment { .. }
                            )
                            | (
                                Operation::Reset,
                                ReceiptOutcome::CompletePasswordReset { .. }
                            )
                    )
            })
    }
    pub fn account_id(&self) -> Option<AccountId> {
        self.account
            .as_ref()
            .map(|account| account.account_id)
            .or_else(|| self.session.as_ref().map(|session| session.account_id))
            .or_else(|| {
                self.receipt.as_ref().map(|receipt| match receipt.outcome {
                    ReceiptOutcome::Login { account_id, .. }
                    | ReceiptOutcome::Redeem { account_id, .. }
                    | ReceiptOutcome::CompleteEnrollment { account_id }
                    | ReceiptOutcome::CompletePasswordReset { account_id } => account_id,
                })
            })
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Fixed test fixtures.")]
mod tests {
    use super::*;
    #[test]
    fn real_reset_completion_is_not_a_login_response() {
        let parsed = serde_json::from_slice::<AuthReply>(
            br#"{"reset_completed":true,"next_action":"login"}"#,
        );
        assert!(parsed.is_ok_and(|r| r.reset_completed == Some(true)
            && r.session.is_none()
            && r.next_action.as_deref() == Some("login")));
    }
    #[test]
    fn receipts_cannot_be_confused_with_a_different_auth_operation() {
        let parsed = serde_json::from_slice::<AuthReply>(br#"{"replayed":true,"receipt":{"version":1,"command_id":"01890f3e-53b7-7d28-9b05-4f65092d5712","outcome":{"operation":"complete_password_reset","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711"},"completed_at":1,"expires_at":2}}"#);
        assert!(parsed.is_ok_and(|reply| reply.replay(
            Operation::Reset,
            "01890f3e-53b7-7d28-9b05-4f65092d5712",
            reply.account_id(),
            1
        ) && !reply.replay(
            Operation::Login,
            "01890f3e-53b7-7d28-9b05-4f65092d5712",
            None,
            1
        ) && !reply.normal_reply()));
        for invalid in [
            br#"{"account":[]}"#.as_slice(),
            br#"{"session":[]}"#,
            br#"{"receipt":[]}"#,
            br#"{"reset_completed":true,"reset_completed":true}"#,
        ] {
            assert!(serde_json::from_slice::<AuthReply>(invalid).is_err());
        }
    }
    #[test]
    fn replay_requires_original_id_account_and_unexpired_receipt() {
        let reply: AuthReply = serde_json::from_str(r#"{"replayed":true,"receipt":{"version":1,"command_id":"01890f3e-53b7-7d28-9b05-4f65092d5712","outcome":{"operation":"complete_password_reset","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711"},"completed_at":100,"expires_at":200}}"#).unwrap();
        let id = "01890f3e-53b7-7d28-9b05-4f65092d5712";
        let other = "01890f3e-53b7-7d28-9b05-4f65092d5713";
        let expected = reply.account_id();
        assert!(reply.replay(Operation::Reset, id, expected, 199));
        assert!(!reply.replay(Operation::Reset, other, expected, 199));
        assert!(!reply.replay(Operation::Reset, id, Some(other.parse().unwrap()), 199));
        assert!(!reply.replay(Operation::Reset, id, None, 199));
        assert!(!reply.replay(Operation::Reset, id, expected, 200));
    }
    #[test]
    fn login_session_is_an_object_not_a_bearer() {
        let parsed = serde_json::from_slice::<AuthReply>(br#"{"account":{"account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","username":"ExampleHost","role":"Host","status":"Verified","disabled":false,"created_at":1},"session":{"kind":"Account","scope":"Normal","expires_at":2,"account_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","role":"Host"}}"#);
        assert!(parsed.is_ok_and(|r| r.account.is_some() && r.session.is_some()));
        assert!(serde_json::from_slice::<AuthReply>(br#"{"session":"secret"}"#).is_err());
    }
}
