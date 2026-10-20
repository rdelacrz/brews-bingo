//! Pure, bounded presentation rules over authorized public contracts.
use brews_contracts::games::{CallView, GameView, SnapshotFrame, WinnerView};
use brews_domain::{
    games::{GameState, WinningPattern},
    ids::GameId,
};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ViewContext {
    Account,
    Player,
    Spectator,
}
impl ViewContext {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "account" => Some(Self::Account),
            "player" => Some(Self::Player),
            "spectator" => Some(Self::Spectator),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Player => "player",
            Self::Spectator => "spectator",
        }
    }
    pub fn accepts(self, view: &GameView) -> bool {
        matches!(
            (self, view),
            (
                Self::Account,
                GameView::Host { .. } | GameView::FinalHost { .. }
            ) | (
                Self::Player,
                GameView::Player { .. } | GameView::FinalPlayer { .. }
            ) | (Self::Spectator, GameView::Spectator { .. })
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Connection {
    Connecting,
    Synchronizing,
    Live,
    Reconnecting,
    Paused,
    Denied,
    Expired,
    Ended,
    Unsupported,
}
impl Connection {
    pub fn tone(self) -> crate::components::StatusTone {
        use crate::components::StatusTone;
        match self {
            Self::Live => StatusTone::Success,
            Self::Connecting | Self::Synchronizing => StatusTone::Info,
            Self::Reconnecting | Self::Paused => StatusTone::Warning,
            Self::Denied | Self::Expired => StatusTone::Danger,
            Self::Ended | Self::Unsupported => StatusTone::Neutral,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting",
            Self::Synchronizing => "Synchronizing",
            Self::Live => "Live",
            Self::Reconnecting => "Reconnecting — controls paused",
            Self::Paused => "Live updates paused",
            Self::Denied => "Access is unavailable or has ended",
            Self::Expired => "Access expired",
            Self::Ended => "Game ended",
            Self::Unsupported => "Live connections require a browser",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum GameUiError {
    #[error("The server returned a different game or viewing context.")]
    Context,
    #[error("The game update is invalid or out of order. Reconnect to synchronize.")]
    Snapshot,
    #[error("The original access deadline has passed.")]
    Expired,
    #[error("Live connection is unavailable. Reconnect to synchronize.")]
    Transport,
}

pub(super) fn state(view: &GameView) -> GameState {
    match view {
        GameView::Host { game, .. } => game.state,
        GameView::Player { state, .. }
        | GameView::Spectator { state, .. }
        | GameView::FinalHost { state, .. }
        | GameView::FinalPlayer { state, .. } => *state,
    }
}

pub(super) fn terminal(view: &GameView) -> bool {
    matches!(state(view), GameState::Resolved | GameState::Cancelled)
}
pub(super) fn line_label(line: &brews_domain::games::CompletedLine) -> String {
    use brews_domain::games::CompletedLine;
    match line {
        CompletedLine::Row(row) => format!("Completed row {row}"),
        CompletedLine::Column(column) => format!("Completed column {column}"),
        CompletedLine::MainDiagonal => "Completed main diagonal".to_string(),
        CompletedLine::AntiDiagonal => "Completed opposite diagonal".to_string(),
    }
}
pub(super) fn calls(view: &GameView) -> Vec<CallView> {
    match view {
        GameView::Host { calls, .. }
        | GameView::Player { calls, .. }
        | GameView::Spectator { calls, .. }
        | GameView::FinalHost { calls, .. }
        | GameView::FinalPlayer { calls, .. } => calls.clone(),
    }
}
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "Browser watchdog and pure tests.")
)]
pub(super) fn access_expired(deadline: Option<i64>, idle_due: Option<i64>, now: i64) -> bool {
    deadline.is_some_and(|d| now < 0 || d <= now) || idle_due.is_some_and(|d| now < 0 || d <= now)
}
pub(super) fn cap(current: Option<i64>, proposed: i64, now: i64) -> Result<i64, GameUiError> {
    let deadline = current.map_or(proposed, |current| current.min(proposed));
    if now < 0 || deadline <= now {
        Err(GameUiError::Expired)
    } else {
        Ok(deadline)
    }
}
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "Validated by browser transport and native contract tests."
    )
)]
pub(super) fn validate_frame(
    frame: &SnapshotFrame,
    id: GameId,
    context: ViewContext,
    previous: Option<&GameView>,
    now: i64,
) -> Result<(), GameUiError> {
    frame.validate().map_err(|_| GameUiError::Snapshot)?;
    if frame.game_id != id || !context.accepts(&frame.view) {
        return Err(GameUiError::Context);
    }
    if frame.session_expires_at <= now {
        return Err(GameUiError::Expired);
    }
    if let Some(previous) = previous {
        validate_transition(previous, &frame.view)?;
    }
    Ok(())
}
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn validate_stream_frame(
    frame: &SnapshotFrame,
    id: GameId,
    context: ViewContext,
    previous: Option<&GameView>,
    http_revision: u64,
    now: i64,
) -> Result<bool, GameUiError> {
    validate_frame(frame, id, context, None, now)?;
    if let Some(previous) = previous {
        if previous.game_id() != id || !same_identity(previous, &frame.view) {
            return Err(GameUiError::Snapshot);
        }
        // ACK, but never render, a queued frame covered by a newer full HTTP read.
        if frame.view_revision < previous.view_revision() && frame.view_revision <= http_revision {
            validate_transition(&frame.view, previous)?;
            return Ok(false);
        }
        validate_transition(previous, &frame.view)?;
    }
    Ok(true)
}

// The HTTP request validates its start snapshot separately. A socket snapshot
// can then overtake that authorized read without allowing a real rollback.
pub(super) fn validate_http_snapshot(
    current: Option<&GameView>,
    authorized: &GameView,
) -> Result<bool, GameUiError> {
    if let Some(current) = current {
        if authorized.view_revision() < current.view_revision() {
            validate_transition(authorized, current)?;
            return Ok(false);
        }
        validate_transition(current, authorized)?;
    }
    Ok(true)
}

pub(super) fn validate_transition(previous: &GameView, next: &GameView) -> Result<(), GameUiError> {
    let allowed = match state(previous) {
        // Full snapshots can skip every intermediate published state.
        GameState::New => true,
        GameState::AwaitingPlayers => !matches!(state(next), GameState::New),
        GameState::InProgress => matches!(
            state(next),
            GameState::InProgress | GameState::Resolved | GameState::Cancelled
        ),
        end => state(next) == end,
    };
    if previous.game_id() != next.game_id()
        || next.view_revision() < previous.view_revision()
        || !same_identity(previous, next)
        || !allowed
    {
        Err(GameUiError::Snapshot)
    } else {
        Ok(())
    }
}
fn same_identity(a: &GameView, b: &GameView) -> bool {
    match (a, b) {
        (
            GameView::Player { player_id: a, .. } | GameView::FinalPlayer { player_id: a, .. },
            GameView::Player { player_id: b, .. } | GameView::FinalPlayer { player_id: b, .. },
        ) => a == b,
        (
            GameView::Spectator {
                spectator_id: a, ..
            },
            GameView::Spectator {
                spectator_id: b, ..
            },
        ) => a == b,
        (
            GameView::Host { .. } | GameView::FinalHost { .. },
            GameView::Host { .. } | GameView::FinalHost { .. },
        ) => true,
        _ => false,
    }
}

// The public terminal notice is emitted by the Worker, which has no
// shared StreamMessage DTO. Keep this closed adapter aligned with that wire.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "Wire fields are validated by the browser transport."
    )
)]
pub(super) struct TerminalNotice {
    pub version: u8,
    pub kind: String,
    pub game_id: GameId,
    pub state: GameState,
    pub ended_at: i64,
    pub objective: WinningPattern,
    pub calls: Vec<CallView>,
    #[serde(deserialize_with = "Option::<WinnerView>::deserialize")]
    pub winner: Option<WinnerView>,
}
impl TerminalNotice {
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(
            dead_code,
            reason = "Called by browser transport and native contract tests."
        )
    )]
    pub fn decode(
        text: &str,
        id: GameId,
        previous: &GameView,
        now: i64,
    ) -> Result<Self, GameUiError> {
        if text.len() > brews_contracts::games::GAME_FRAME_MAX_BYTES {
            return Err(GameUiError::Snapshot);
        }
        let value: Self = serde_json::from_str(text).map_err(|_| GameUiError::Snapshot)?;
        if value.version != 1
            || value.kind != "terminal_notice"
            || value.game_id != id
            || value.ended_at > now
            || value.ended_at < 0
            || !matches!(value.state, GameState::Resolved | GameState::Cancelled)
            || (value.state == GameState::Resolved) != value.winner.is_some()
            || value.objective != WinningPattern::SingleLine
        {
            return Err(GameUiError::Snapshot);
        }
        let GameView::Spectator {
            configuration,
            calls,
            ..
        } = previous
        else {
            return Err(GameUiError::Context);
        };
        if value.calls.len() > configuration.numeric_upper_bound as usize
            || value.calls.len() < calls.len()
        {
            return Err(GameUiError::Snapshot);
        }
        for (index, call) in value.calls.iter().enumerate() {
            if call.sequence_no != index as u32 + 1
                || call.value.parse::<u32>().ok().is_none_or(|n| {
                    n == 0 || n > configuration.numeric_upper_bound || n.to_string() != call.value
                })
                || value.calls[..index].iter().any(|c| c.value == call.value)
                || calls.get(index).is_some_and(|old| old.value != call.value)
            {
                return Err(GameUiError::Snapshot);
            }
        }
        Ok(value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Fixed test fixtures.")]
mod tests {
    use super::*;
    #[test]
    fn connection_badges_distinguish_live_waiting_and_denied_states() {
        use crate::components::StatusTone;
        for (connection, tone) in [
            (Connection::Connecting, StatusTone::Info),
            (Connection::Synchronizing, StatusTone::Info),
            (Connection::Live, StatusTone::Success),
            (Connection::Reconnecting, StatusTone::Warning),
            (Connection::Paused, StatusTone::Warning),
            (Connection::Denied, StatusTone::Danger),
            (Connection::Expired, StatusTone::Danger),
            (Connection::Ended, StatusTone::Neutral),
            (Connection::Unsupported, StatusTone::Neutral),
        ] {
            assert_eq!(connection.tone(), tone);
            assert!(!connection.label().is_empty());
        }
    }
    use brews_contracts::games::{GameSummary, SnapshotKind};
    use brews_domain::games::GameConfiguration;
    const ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5701";
    const OTHER: &str = "01890f3e-53b7-7d28-9b05-4f65092d5702";
    fn host(revision: u64) -> GameView {
        GameView::Host {
            game: GameSummary {
                game_id: ID.parse().unwrap(),
                state: GameState::New,
                designated_host_id: ID.parse().unwrap(),
                created_at: 1,
                view_revision: revision,
            },
            configuration: GameConfiguration::default(),
            game_code: None,
            started_at: None,
            ended_at: None,
            idle_cancel_due_at: Some(86_400_001),
            players: vec![],
            calls: vec![],
            connected_player_count: 0,
            spectator_count: 0,
            connected_spectator_count: 0,
        }
    }
    fn frame(view: GameView) -> SnapshotFrame {
        SnapshotFrame {
            version: 1,
            kind: SnapshotKind::Snapshot,
            game_id: view.game_id(),
            view_revision: view.view_revision(),
            connection_id: ID.parse().unwrap(),
            session_expires_at: 200,
            view,
            delivery_id: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                .parse()
                .unwrap(),
        }
    }
    #[test]
    fn authorized_http_read_cannot_replace_a_newer_socket_snapshot() {
        let start = host(3);
        let authorized = host(4);
        let current = host(5);
        assert_eq!(validate_transition(&start, &authorized), Ok(()));
        assert_eq!(
            validate_http_snapshot(Some(&current), &authorized),
            Ok(false)
        );
        // Coverage advances only to the HTTP revision, not the displayed revision.
        assert_eq!(
            validate_stream_frame(
                &frame(host(4)),
                current.game_id(),
                ViewContext::Account,
                Some(&current),
                4,
                100
            ),
            Ok(false)
        );
        assert_eq!(
            validate_stream_frame(
                &frame(host(5)),
                current.game_id(),
                ViewContext::Account,
                Some(&host(6)),
                4,
                100
            ),
            Err(GameUiError::Snapshot)
        );
        assert_eq!(
            validate_http_snapshot(Some(&authorized), &current),
            Ok(true)
        );
        // A server rollback relative to request start is still invalid.
        assert_eq!(
            validate_transition(&start, &host(2)),
            Err(GameUiError::Snapshot)
        );
    }

    #[test]
    fn overtaken_http_reads_still_reject_identity_and_lifecycle_changes() {
        let authorized = GameView::Spectator {
            game_id: ID.parse().unwrap(),
            state: GameState::InProgress,
            configuration: GameConfiguration::default(),
            game_code: "AB12CD34".parse().unwrap(),
            started_at: Some(1),
            view_revision: 3,
            spectator_id: ID.parse().unwrap(),
            calls: vec![],
            latest_call: None,
            exhausted: false,
        };
        let mut current = authorized.clone();
        if let GameView::Spectator { view_revision, .. } = &mut current {
            *view_revision = 4;
        }
        assert_eq!(
            validate_http_snapshot(Some(&current), &authorized),
            Ok(false)
        );
        if let GameView::Spectator { spectator_id, .. } = &mut current {
            *spectator_id = OTHER.parse().unwrap();
        }
        assert_eq!(
            validate_http_snapshot(Some(&current), &authorized),
            Err(GameUiError::Snapshot)
        );
        if let GameView::Spectator {
            spectator_id,
            state,
            ..
        } = &mut current
        {
            *spectator_id = ID.parse().unwrap();
            *state = GameState::AwaitingPlayers;
        }
        assert_eq!(
            validate_http_snapshot(Some(&current), &authorized),
            Err(GameUiError::Snapshot)
        );
        assert_eq!(
            validate_http_snapshot(Some(&host(4)), &authorized),
            Err(GameUiError::Snapshot)
        );
    }

    #[test]
    fn queued_frames_are_discarded_only_when_covered_by_authorized_http_state() {
        let current = host(5);
        let id = current.game_id();
        assert_eq!(
            validate_stream_frame(
                &frame(host(4)),
                id,
                ViewContext::Account,
                Some(&current),
                5,
                100
            ),
            Ok(false)
        );
        assert_eq!(
            validate_stream_frame(
                &frame(host(4)),
                id,
                ViewContext::Account,
                Some(&current),
                3,
                100
            ),
            Err(GameUiError::Snapshot)
        );
        assert_eq!(
            validate_stream_frame(
                &frame(host(6)),
                id,
                ViewContext::Account,
                Some(&current),
                5,
                100
            ),
            Ok(true)
        );
        assert_eq!(
            validate_stream_frame(
                &frame(host(4)),
                id,
                ViewContext::Account,
                Some(&current),
                5,
                200
            ),
            Err(GameUiError::Expired)
        );
        assert_eq!(
            validate_stream_frame(
                &frame(host(4)),
                OTHER.parse().unwrap(),
                ViewContext::Account,
                Some(&current),
                5,
                100
            ),
            Err(GameUiError::Context)
        );
    }

    #[test]
    fn full_frames_reject_wrong_context_game_expiry_and_revision_rollback() {
        let initial = host(5);
        let id = initial.game_id();
        let frame = frame(host(6));
        assert_eq!(
            validate_frame(&frame, id, ViewContext::Account, Some(&initial), 100),
            Ok(())
        );
        assert_eq!(
            validate_frame(&frame, id, ViewContext::Player, None, 100),
            Err(GameUiError::Context)
        );
        assert_eq!(
            validate_frame(
                &frame,
                OTHER.parse().unwrap(),
                ViewContext::Account,
                None,
                100
            ),
            Err(GameUiError::Context)
        );
        assert_eq!(
            validate_frame(&frame, id, ViewContext::Account, None, 200),
            Err(GameUiError::Expired)
        );
        assert_eq!(
            validate_frame(&frame, id, ViewContext::Account, Some(&host(7)), 100),
            Err(GameUiError::Snapshot)
        );
    }
    #[test]
    fn full_snapshot_can_skip_from_new_to_resolved_without_allowing_rollback() {
        use brews_contracts::games::{BoardView, FinalBoardView};
        use brews_domain::games::{BoardCell, BoardCellKind, CellPosition, CompletedLine};
        let board = BoardView {
            side_length: 2,
            cells: (0..4u8)
                .map(|index| BoardCell {
                    position: CellPosition {
                        row: index / 2 + 1,
                        column: index % 2 + 1,
                    },
                    kind: BoardCellKind::Value((index + 1).to_string()),
                    is_matched: index < 2,
                })
                .collect(),
            qualified: true,
            qualifying_lines: vec![CompletedLine::Row(1)],
        };
        let final_view = GameView::FinalHost {
            game_id: ID.parse().unwrap(),
            game_code: "AB12CD34".parse().unwrap(),
            designated_host_id: ID.parse().unwrap(),
            state: GameState::Resolved,
            started_at: 2,
            ended_at: 3,
            history_expires_at: 200,
            winner: Some(WinnerView {
                player_id: ID.parse().unwrap(),
                alias: "Alice".into(),
            }),
            view_revision: 10,
            calls: vec![
                CallView {
                    sequence_no: 1,
                    value: "1".into(),
                },
                CallView {
                    sequence_no: 2,
                    value: "2".into(),
                },
            ],
            players: vec![
                FinalBoardView {
                    player_id: ID.parse().unwrap(),
                    alias: "Alice".into(),
                    board: board.clone(),
                },
                FinalBoardView {
                    player_id: OTHER.parse().unwrap(),
                    alias: "Bob".into(),
                    board,
                },
            ],
        };
        assert!(final_view.validate().is_ok());
        assert_eq!(
            validate_frame(
                &frame(final_view.clone()),
                ID.parse().unwrap(),
                ViewContext::Account,
                Some(&host(1)),
                100
            ),
            Ok(())
        );
        assert_eq!(
            validate_transition(&host(11), &final_view),
            Err(GameUiError::Snapshot)
        );
        assert_eq!(
            validate_transition(&final_view, &host(11)),
            Err(GameUiError::Snapshot)
        );
        let mut changed_terminal = final_view.clone();
        if let GameView::FinalHost {
            state,
            winner,
            view_revision,
            ..
        } = &mut changed_terminal
        {
            *state = GameState::Cancelled;
            *winner = None;
            *view_revision = 11;
        }
        assert!(changed_terminal.validate().is_ok());
        assert_eq!(
            validate_transition(&final_view, &changed_terminal),
            Err(GameUiError::Snapshot)
        );
        let mut other_game = final_view.clone();
        if let GameView::FinalHost { game_id, .. } = &mut other_game {
            *game_id = OTHER.parse().unwrap();
        }
        assert_eq!(
            validate_transition(&host(1), &other_game),
            Err(GameUiError::Snapshot)
        );
    }
    #[test]
    fn participant_identity_cannot_change_in_the_same_context() {
        let previous = GameView::Spectator {
            game_id: ID.parse().unwrap(),
            state: GameState::AwaitingPlayers,
            configuration: GameConfiguration::default(),
            game_code: "AB12CD34".parse().unwrap(),
            started_at: None,
            view_revision: 1,
            spectator_id: ID.parse().unwrap(),
            calls: vec![],
            latest_call: None,
            exhausted: false,
        };
        let mut next = previous.clone();
        if let GameView::Spectator { spectator_id, .. } = &mut next {
            *spectator_id = OTHER.parse().unwrap();
        }
        assert_eq!(
            validate_transition(&previous, &next),
            Err(GameUiError::Snapshot)
        );
        let mut authoritative = previous.clone();
        if let GameView::Spectator { view_revision, .. } = &mut authoritative {
            *view_revision = 2;
        }
        assert_eq!(
            validate_stream_frame(
                &frame(next),
                previous.game_id(),
                ViewContext::Spectator,
                Some(&authoritative),
                2,
                100
            ),
            Err(GameUiError::Snapshot)
        );
        let notice = serde_json::json!({"version":1,"kind":"terminal_notice","game_id":ID,"state":"cancelled","ended_at":100,"objective":"single_line","calls":[],"winner":null});
        assert!(
            TerminalNotice::decode(&notice.to_string(), previous.game_id(), &previous, 100).is_ok()
        );
        let mut corrupt = notice.clone();
        corrupt["calls"] =
            serde_json::json!([{"sequence_no":1,"value":"4"},{"sequence_no":2,"value":"4"}]);
        assert!(
            TerminalNotice::decode(&corrupt.to_string(), previous.game_id(), &previous, 100)
                .is_err()
        );
        assert!(
            TerminalNotice::decode(&notice.to_string(), OTHER.parse().unwrap(), &previous, 100)
                .is_err()
        );
    }
    #[test]
    fn deadline_never_extends_and_expiry_is_closed() {
        assert_eq!(cap(Some(100), 200, 90), Ok(100));
        assert_eq!(cap(Some(100), 80, 70), Ok(80));
        assert_eq!(cap(Some(100), 200, 100), Err(GameUiError::Expired));
        assert_eq!(cap(None, 100, -1), Err(GameUiError::Expired));
    }
    #[test]
    fn expiry_does_not_depend_on_transport_progress() {
        assert!(!access_expired(None, None, 100));
        assert!(!access_expired(Some(200), Some(150), 149));
        assert!(access_expired(Some(200), Some(150), 150));
        assert!(access_expired(Some(200), None, 200));
        assert!(access_expired(Some(200), None, 10_000));
        assert!(access_expired(Some(200), None, -1));
    }
    #[test]
    fn context_selection_is_closed_and_explicit() {
        assert_eq!(ViewContext::parse("host"), None);
        assert_eq!(ViewContext::parse(""), None);
        assert_eq!(ViewContext::parse("player"), Some(ViewContext::Player));
    }
}
