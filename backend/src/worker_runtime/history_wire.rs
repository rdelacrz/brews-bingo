//! Closed History peer replies and semantic release validation.
use super::game_peers::{GamePeerError, SecretToken, object};
use brews_contracts::{
    games::{BoardView, CallView, FinalBoardView, GameView, WinnerView},
    history::{History, HistorySummary},
};
use brews_domain::{
    games::{GameState, SingleLine, SingleLinePattern, validate_alias},
    ids::GameId,
};
use serde::{Deserialize, Serialize};

pub(super) const HISTORY_REPLY_MAX_BYTES: usize = 256 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryOwnerRequest {
    pub game_id: GameId,
    pub token: SecretToken,
    pub summary: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum HistoryOwnerOutcome {
    Summary { summary: HistorySummary },
    Detail { history: History },
    NotFound {},
    Unauthorized {},
    Unavailable {},
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryOwnerReply {
    pub game_id: GameId,
    #[serde(deserialize_with = "object")]
    pub outcome: HistoryOwnerOutcome,
}
pub(super) fn validate_summary(s: &HistorySummary, now: i64) -> Result<(), GamePeerError> {
    if !(1..=now).contains(&s.started_at)
        || !(s.started_at..=now).contains(&s.ended_at)
        || crate::db::game::add_three_calendar_months(s.ended_at).ok() != Some(s.expires_at)
        || !matches!(
            (s.outcome, s.winner.as_ref()),
            (GameState::Cancelled, None) | (GameState::Resolved, Some(_))
        )
        || s.winner
            .as_ref()
            .is_some_and(|w| !validate_alias(&w.alias).is_ok_and(|alias| alias == w.alias))
    {
        return Err(GamePeerError::Unavailable);
    }
    Ok(())
}
pub(super) fn validate_history(h: &History, now: i64) -> Result<(), GamePeerError> {
    validate_summary(&h.summary(), now)?;
    if !h
        .players
        .windows(2)
        .all(|p| p[0].player_id < p[1].player_id)
        || h.players
            .iter()
            .any(|p| !p.cells.windows(2).all(|c| c[0].position < c[1].position))
    {
        return Err(GamePeerError::Unavailable);
    }
    let players = h
        .players
        .iter()
        .map(|p| {
            let lines = SingleLine::evaluate(p.side_length, &p.cells)
                .map_err(|_| GamePeerError::Unavailable)?;
            Ok(FinalBoardView {
                player_id: p.player_id,
                alias: p.alias.clone(),
                board: BoardView {
                    side_length: p.side_length,
                    cells: p.cells.clone(),
                    qualified: !lines.is_empty(),
                    qualifying_lines: lines,
                },
            })
        })
        .collect::<Result<Vec<_>, GamePeerError>>()?;
    let view = GameView::FinalHost {
        game_id: h.game_id,
        game_code: h.game_code.clone(),
        designated_host_id: h.designated_host_id,
        state: h.outcome,
        started_at: h.started_at,
        ended_at: h.ended_at,
        history_expires_at: h.expires_at,
        winner: h.winner.as_ref().map(|w| WinnerView {
            player_id: w.player_id,
            alias: w.alias.clone(),
        }),
        view_revision: 0,
        calls: h
            .ordered_calls
            .iter()
            .enumerate()
            .map(|(i, v)| CallView {
                sequence_no: (i + 1) as u32,
                value: v.clone(),
            })
            .collect(),
        players,
    };
    view.validate().map_err(|_| GamePeerError::Unavailable)
}
pub(super) fn verify_owner(
    request: &HistoryOwnerRequest,
    reply: HistoryOwnerReply,
    now: i64,
) -> Result<HistoryOwnerOutcome, GamePeerError> {
    if reply.game_id != request.game_id {
        return Err(GamePeerError::Unavailable);
    }
    match &reply.outcome {
        HistoryOwnerOutcome::Summary { summary }
            if request.summary && summary.game_id == request.game_id =>
        {
            validate_summary(summary, now)?
        }
        HistoryOwnerOutcome::Detail { history }
            if !request.summary && history.game_id == request.game_id =>
        {
            validate_history(history, now)?
        }
        HistoryOwnerOutcome::NotFound {}
        | HistoryOwnerOutcome::Unauthorized {}
        | HistoryOwnerOutcome::Unavailable {} => {}
        _ => return Err(GamePeerError::Unavailable),
    }
    Ok(reply.outcome)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Closed fixture IDs fail fast.")]
mod tests {
    use super::*;
    fn summary() -> HistorySummary {
        let id = "019ce56a-1b00-7000-8000-000000000001";
        HistorySummary {
            game_id: id.parse().unwrap(),
            game_code: "GAME0001".parse().unwrap(),
            designated_host_id: id.parse().unwrap(),
            outcome: GameState::Resolved,
            started_at: 1000,
            ended_at: 2000,
            expires_at: crate::db::game::add_three_calendar_months(2000).unwrap(),
            winner: Some(brews_contracts::history::HistoryWinner {
                player_id: id.parse().unwrap(),
                alias: "First".into(),
            }),
        }
    }
    #[test]
    fn history_summary_rejects_alias_normalization_instead_of_repairing_owner_bytes() {
        let mut s = summary();
        s.winner.as_mut().unwrap().alias = " First ".into();
        assert!(
            validate_summary(&s, 2000).is_err(),
            "owner winner alias must already be canonical"
        );
    }
}
