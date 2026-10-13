//! Bounded, secret-free Game-owned developer audit projections.
use crate::management::{AuditActor, AuditOutcome};
use brews_domain::ids::{AuditId, GameId};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameAuditOperation {
    Configuration,
    Resume,
    HostTransfer,
    HostTransferTargets,
    GameRead,
    MembershipRead,
    PlayersRead,
    CallsRead,
    RemainingValuesRead,
    OwnBoardRead,
    BoardsRead,
    QualificationRead,
    ResultRead,
    CommandLookup,
    Creation,
    Lobby,
    Start,
    CallRandom,
    CallManual,
    Winner,
    Cancel,
    AccountContext,
    AccountStream,
    HistoryRead,
    DiscoveryRead,
    ListGameAudit,
    AccountExit,
}
impl GameAuditOperation {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Resume => "resume",
            Self::HostTransfer => "host_transfer",
            Self::HostTransferTargets => "host_transfer_targets",
            Self::GameRead => "game_read",
            Self::MembershipRead => "membership_read",
            Self::PlayersRead => "players_read",
            Self::CallsRead => "calls_read",
            Self::RemainingValuesRead => "remaining_values_read",
            Self::OwnBoardRead => "own_board_read",
            Self::BoardsRead => "boards_read",
            Self::QualificationRead => "qualification_read",
            Self::ResultRead => "result_read",
            Self::CommandLookup => "command_lookup",
            Self::Creation => "creation",
            Self::Lobby => "lobby",
            Self::Start => "start",
            Self::CallRandom => "call_random",
            Self::CallManual => "call_manual",
            Self::Winner => "winner",
            Self::Cancel => "cancel",
            Self::AccountContext => "account_context",
            Self::AccountStream => "account_stream",
            Self::HistoryRead => "history_read",
            Self::DiscoveryRead => "discovery_read",
            Self::ListGameAudit => "list_game_audit",
            Self::AccountExit => "account_exit",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameAuditEvent {
    pub audit_id: AuditId,
    pub actor: AuditActor,
    pub target_game_id: GameId,
    pub operation: GameAuditOperation,
    pub outcome: AuditOutcome,
    pub occurred_at: i64,
    pub expires_at: i64,
}
pub const GAME_AUDIT_PAGE_MAX: u32 = 100;
pub const GAME_AUDIT_RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// Checks exact targeting, bounded pagination, ordering and immutable retention.
pub fn validate_page(
    game: GameId,
    after: Option<AuditId>,
    limit: u32,
    actual: GameId,
    events: &[GameAuditEvent],
    next: Option<AuditId>,
) -> bool {
    if game != actual
        || !(1..=GAME_AUDIT_PAGE_MAX).contains(&limit)
        || events.len() > limit as usize
    {
        return false;
    }
    let mut previous = after;
    for event in events {
        if event.target_game_id != game
            || previous.is_some_and(|id| event.audit_id <= id)
            || event.occurred_at < 0
            || event.expires_at > 9_007_199_254_740_991
            || event.occurred_at.checked_add(GAME_AUDIT_RETENTION_MS) != Some(event.expires_at)
        {
            return false;
        }
        previous = Some(event.audit_id);
    }
    next.is_none()
        || (events.len() == limit as usize && next == events.last().map(|event| event.audit_id))
}
