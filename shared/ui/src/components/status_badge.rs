use brews_domain::{accounts::AccountStatus, games::GameState};
use dioxus::prelude::*;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) enum StatusTone {
    #[default]
    Neutral,
    Info,
    Warning,
    Success,
    Danger,
}

impl StatusTone {
    fn class(self) -> &'static str {
        match self {
            Self::Neutral => "status-badge status-neutral",
            Self::Info => "status-badge status-info",
            Self::Warning => "status-badge status-warning",
            Self::Success => "status-badge status-success",
            Self::Danger => "status-badge status-danger",
        }
    }
}

impl From<GameState> for StatusTone {
    fn from(state: GameState) -> Self {
        match state {
            GameState::New => Self::Neutral,
            GameState::AwaitingPlayers => Self::Warning,
            GameState::InProgress => Self::Info,
            GameState::Resolved => Self::Success,
            GameState::Cancelled => Self::Danger,
        }
    }
}

impl From<AccountStatus> for StatusTone {
    fn from(status: AccountStatus) -> Self {
        match status {
            AccountStatus::PendingEnrollment => Self::Warning,
            AccountStatus::Verified => Self::Success,
            AccountStatus::ResetRequired => Self::Danger,
        }
    }
}

pub(crate) fn game_state_label(state: GameState) -> &'static str {
    match state {
        GameState::New => "New",
        GameState::AwaitingPlayers => "Awaiting players",
        GameState::InProgress => "In progress",
        GameState::Resolved => "Resolved",
        GameState::Cancelled => "Cancelled",
    }
}

/// Intrinsic-width status with a text label; color is never the only indicator.
#[component]
pub(crate) fn StatusBadge(label: String, #[props(default)] tone: StatusTone) -> Element {
    rsx! { span { class: tone.class(), "{label}" } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_game_state_has_a_distinct_tone_and_readable_label() {
        let cases = [
            (GameState::New, "New", StatusTone::Neutral),
            (
                GameState::AwaitingPlayers,
                "Awaiting players",
                StatusTone::Warning,
            ),
            (GameState::InProgress, "In progress", StatusTone::Info),
            (GameState::Resolved, "Resolved", StatusTone::Success),
            (GameState::Cancelled, "Cancelled", StatusTone::Danger),
        ];
        for (state, label, tone) in cases {
            assert_eq!(game_state_label(state), label);
            assert_eq!(StatusTone::from(state), tone);
            assert!(tone.class().starts_with("status-badge "));
        }
    }

    #[test]
    fn account_lifecycle_tones_do_not_depend_on_disablement() {
        assert_eq!(
            StatusTone::from(AccountStatus::PendingEnrollment),
            StatusTone::Warning
        );
        assert_eq!(
            StatusTone::from(AccountStatus::Verified),
            StatusTone::Success
        );
        assert_eq!(
            StatusTone::from(AccountStatus::ResetRequired),
            StatusTone::Danger
        );
        assert_eq!(StatusTone::default(), StatusTone::Neutral);
    }
}
