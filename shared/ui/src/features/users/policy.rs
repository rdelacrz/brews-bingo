use brews_contracts::SafeAccount;
use brews_domain::{
    accounts::{AccountRole, AccountStatus},
    ids::AccountId,
};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Action {
    Enrollment,
    Reset,
    Disable,
    Enable,
    Delete,
}
impl Action {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Enrollment => "Reissue enrollment link",
            Self::Reset => "Issue / reissue reset link",
            Self::Disable => "Disable account",
            Self::Enable => "Enable account",
            Self::Delete => "Delete account",
        }
    }
    pub(super) fn path(self, id: AccountId) -> String {
        let suffix = match self {
            Self::Enrollment => "/enrollment-links",
            Self::Reset => "/password-reset-links",
            Self::Disable => "/disable",
            Self::Enable => "/enable",
            Self::Delete => "",
        };
        format!("/api/users/{id}{suffix}")
    }
    pub(super) fn method(self) -> &'static str {
        if self == Self::Delete {
            "DELETE"
        } else {
            "POST"
        }
    }
    pub(super) fn allowed(self, target: &SafeAccount, actor: AccountId) -> bool {
        match self {
            Self::Enrollment => {
                !target.disabled && target.status == AccountStatus::PendingEnrollment
            }
            Self::Reset => {
                !target.disabled
                    && matches!(
                        target.status,
                        AccountStatus::Verified | AccountStatus::ResetRequired
                    )
            }
            Self::Disable => actor != target.account_id && !target.disabled,
            Self::Enable => actor != target.account_id && target.disabled,
            Self::Delete => actor != target.account_id,
        }
    }
    pub(super) fn consequence(self) -> &'static str {
        match self {
            Self::Enrollment => {
                "Previous enrollment links and restricted sessions become invalid. Deliver the new link privately."
            }
            Self::Reset => {
                "The old password and account sessions become invalid immediately. Link expiry does not restore the old password. Deliver the new link privately."
            }
            Self::Disable => {
                "This account loses access. The server will reject this action if the target hosts a nonterminal game or is the last enabled Verified admin. A game is never automatically cancelled or transferred."
            }
            Self::Enable => {
                "This restores account access while preserving its lifecycle and revoked credentials. Pending enrollment or required password reset still needs completion."
            }
            Self::Delete => {
                "This permanently removes the account. Unexpired game History remains. The server protects active hosts and the last enabled Verified admin."
            }
        }
    }
}
pub(super) use crate::components::visible_text as username_display;
pub(super) fn role_label(role: AccountRole) -> &'static str {
    match role {
        AccountRole::Host => "Host",
        AccountRole::Admin => "Admin",
    }
}
pub(super) fn status_label(status: AccountStatus) -> &'static str {
    match status {
        AccountStatus::PendingEnrollment => "Pending enrollment",
        AccountStatus::Verified => "Verified",
        AccountStatus::ResetRequired => "Reset required",
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "Fixed test fixtures.")]
mod tests {
    use super::*;
    fn account() -> SafeAccount {
        SafeAccount {
            account_id: "019ce56a-1b00-7000-8000-000000000001".parse().expect("id"),
            username: "ExampleHost01".into(),
            role: AccountRole::Host,
            status: AccountStatus::PendingEnrollment,
            disabled: false,
            created_at: 1,
        }
    }
    #[test]
    fn actions_keep_disable_lifecycle_and_self_guards_separate() {
        let mut target = account();
        let actor = "019ce56a-1b00-7000-8000-000000000002".parse().expect("id");
        assert!(Action::Enrollment.allowed(&target, actor));
        assert!(!Action::Reset.allowed(&target, actor));
        assert!(!Action::Delete.allowed(&target, target.account_id));
        assert!(!Action::Disable.allowed(&target, target.account_id));
        target.status = AccountStatus::ResetRequired;
        assert!(Action::Reset.allowed(&target, actor));
        assert!(!Action::Enrollment.allowed(&target, actor));
        target.disabled = true;
        assert!(Action::Enable.allowed(&target, actor));
        assert!(!Action::Reset.allowed(&target, actor));
        assert!(!Action::Disable.allowed(&target, actor));
        assert!(!Action::Enable.allowed(&target, target.account_id));
    }
    #[test]
    fn controls_are_visible_without_changing_exact_username() {
        let original = "Name\0with\u{7f}controls";
        assert_eq!(username_display(original), "Name\\x00with\\x7Fcontrols");
        assert_eq!(original.as_bytes()[4], 0);
        assert_ne!(
            username_display("Host\0Person01"),
            username_display(r"Host\x00Person01")
        );
        assert_eq!(username_display(r"Host\x00Person01"), r"Host\\x00Person01");
    }
}
