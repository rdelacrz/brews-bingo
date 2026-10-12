//! One bounded public notice attempt precedes physical audience deletion.
use super::*;
use crate::game::{RETRY_INITIAL_MS, SPECTATOR_NOTICE_ATTEMPT_MS};

const SQL_SELECT_SPECTATOR_IDS: &str =
    "SELECT spectator_id FROM game_spectators ORDER BY spectator_id LIMIT ?";
const SQL_SELECT_ANY_SPECTATOR_CONTEXT: &str = "SELECT 1 FROM game_spectator_contexts LIMIT 1";
const SQL_INSERT_TERMINAL_NOTICE: &str = "INSERT INTO game_spectator_notices VALUES(?,?,?,?,?,?,?)";
const SQL_SELECT_TERMINAL_NOTICE_BY_CONNECTION: &str = "SELECT session_id,spectator_id,epoch,session_expires_at,notice,expires_at FROM game_spectator_notices WHERE connection_id=?";
const SQL_SELECT_TERMINAL_NOTICES: &str = "SELECT connection_id,session_id,spectator_id,epoch,session_expires_at,notice,expires_at FROM game_spectator_notices ORDER BY connection_id LIMIT ?";
const SQL_INSERT_TERMINAL_SPECTATOR_WORK: &str =
    "INSERT INTO game_spectator_terminal_work VALUES(1,?,0,?)";
const SQL_SELECT_TERMINAL_SPECTATOR_WORK: &str = "SELECT attempt_deadline,attempted,next_attempt_at FROM game_spectator_terminal_work WHERE singleton=1";
const SQL_MARK_TERMINAL_NOTICE_ATTEMPT: &str = "UPDATE game_spectator_terminal_work SET attempted=1,next_attempt_at=attempt_deadline WHERE singleton=1";
const SQL_RETRY_TERMINAL_SPECTATOR_CLEANUP: &str =
    "UPDATE game_spectator_terminal_work SET attempted=1,next_attempt_at=? WHERE singleton=1";
const TERMINAL_SPECTATOR_PURGE: [(&str, &str); 3] = [
    (
        "DELETE FROM game_spectator_contexts",
        "SELECT 1 FROM game_spectator_contexts LIMIT 1",
    ),
    (
        "DELETE FROM game_spectator_notices",
        "SELECT 1 FROM game_spectator_notices LIMIT 1",
    ),
    (
        "DELETE FROM game_spectator_terminal_work",
        "SELECT 1 FROM game_spectator_terminal_work LIMIT 1",
    ),
];

#[derive(serde::Serialize)]
struct PublicTerminalNotice<'a> {
    version: u8,
    kind: &'static str,
    game_id: brews_domain::ids::GameId,
    state: GameState,
    ended_at: i64,
    objective: brews_domain::games::WinningPattern,
    calls: &'a [brews_contracts::games::CallView],
    winner: Option<&'a brews_contracts::games::WinnerView>,
}

/// Captured current endpoint, never a reconnect or fetch grant.
pub struct SpectatorTerminalNotice {
    pub connection: SpectatorConnectionGrant,
    pub notice: String,
    pub expires_at: i64,
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    /// The same transaction commits terminal denial and release intent; no transport runs here.
    pub(in crate::db::game) fn terminal_spectators(
        &self,
        r: &Record,
        state: GameState,
        winner: Option<&brews_contracts::games::WinnerView>,
        now: i64,
    ) -> Result<(), GameError> {
        let deadline = add_time(now, SPECTATOR_NOTICE_ATTEMPT_MS)?;
        let rows = self.db.query(
            SQL_SELECT_SPECTATOR_CONNECTIONS_ORDERED_BY_SESSION,
            &[text(SPECTATOR_KIND), int((SPECTATOR_LIMIT + 1) as i64)],
        )?;
        if rows.len() > SPECTATOR_LIMIT {
            return Err(GameError::Storage);
        }
        if !rows.is_empty() {
            let config = self.configuration()?;
            let calls = self
                .call_history(r, config.numeric_upper_bound)?
                .into_iter()
                .map(|c| c.view)
                .collect::<Vec<_>>();
            let notice = serde_json::to_string(&PublicTerminalNotice {
                version: 1,
                kind: "terminal_notice",
                game_id: r.id,
                state,
                ended_at: now,
                objective: config.winning_pattern,
                calls: &calls,
                winner,
            })
            .map_err(|_| GameError::Storage)?;
            if notice.len() > brews_contracts::games::GAME_FRAME_MAX_BYTES {
                return Err(GameError::Storage);
            }
            for row in &rows {
                let g = parse_grant(row, r)?;
                // Only the exact accepted current, unexpired session is eligible.
                match self.validate_spectator_grant(&g, r, now) {
                    Ok(()) => {}
                    Err(GameError::Unauthorized) => continue,
                    Err(error) => return Err(error),
                }
                let expires = g.expires.min(deadline);
                let saved = vec![
                    text(g.session),
                    text(g.spectator),
                    int(g.epoch),
                    int(g.expires),
                    text(&notice),
                    int(expires),
                ];
                let mut insert = vec![text(g.connection)];
                insert.extend(saved.clone());
                self.db.execute(SQL_INSERT_TERMINAL_NOTICE, &insert)?;
                if self.db.query(
                    SQL_SELECT_TERMINAL_NOTICE_BY_CONNECTION,
                    &[text(g.connection)],
                )? != vec![saved]
                {
                    return Err(GameError::Storage);
                }
            }
        }
        // Capture current AND prepared close work, retire transport bindings, keep physical identity.
        let members = self.terminal_spectator_ids()?;
        if members.is_empty()
            && self
                .db
                .query(SQL_SELECT_ANY_SPECTATOR_CONTEXT, &[])?
                .is_empty()
        {
            return Ok(());
        }
        for id in members {
            self.detach_spectator(id, now)?;
        }
        self.db.execute(
            SQL_INSERT_TERMINAL_SPECTATOR_WORK,
            &[int(deadline), int(now)],
        )?;
        if self.db.query(SQL_SELECT_TERMINAL_SPECTATOR_WORK, &[])?
            != vec![vec![int(deadline), int(0), int(now)]]
        {
            return Err(GameError::Storage);
        }
        Ok(())
    }

    fn terminal_spectator_ids(&self) -> Result<Vec<SpectatorId>, GameError> {
        let rows = self.db.query(
            SQL_SELECT_SPECTATOR_IDS,
            &[int((SPECTATOR_LIMIT + 1) as i64)],
        )?;
        if rows.len() > SPECTATOR_LIMIT {
            return Err(GameError::Storage);
        }
        rows.iter()
            .map(|r| string(r, 0)?.parse().map_err(|_| GameError::Storage))
            .collect()
    }

    /// Persist consumption before native send. A crash never replays an attempted batch.
    pub fn begin_spectator_terminal_attempt(
        &self,
    ) -> Result<Vec<SpectatorTerminalNotice>, GameError> {
        self.maintenance(|now| {
            let work = self.db.query(SQL_SELECT_TERMINAL_SPECTATOR_WORK, &[])?;
            let Some(work) = work.first() else {
                return Ok(Vec::new());
            };
            if number(work, 1)? != 0 {
                return Ok(Vec::new());
            }
            let deadline = number(work, 0)?;
            let mut notices = Vec::new();
            if now < deadline {
                let r = self.record_optional()?.ok_or(GameError::Storage)?;
                if !matches!(r.state, GameState::Cancelled | GameState::Resolved) {
                    return Err(GameError::Storage);
                }
                let rows = self.db.query(
                    SQL_SELECT_TERMINAL_NOTICES,
                    &[int((SPECTATOR_LIMIT + 1) as i64)],
                )?;
                if rows.len() > SPECTATOR_LIMIT {
                    return Err(GameError::Storage);
                }
                for row in rows {
                    let expires = number(&row, 6)?;
                    if now >= expires {
                        continue;
                    }
                    notices.push(SpectatorTerminalNotice {
                        connection: SpectatorConnectionGrant {
                            game: r.id,
                            connection: string(&row, 0)?.parse().map_err(|_| GameError::Storage)?,
                            session: string(&row, 1)?.parse().map_err(|_| GameError::Storage)?,
                            spectator: string(&row, 2)?.parse().map_err(|_| GameError::Storage)?,
                            epoch: number(&row, 3)?,
                            expires: number(&row, 4)?,
                        },
                        notice: string(&row, 5)?.to_owned(),
                        expires_at: expires,
                    });
                }
            }
            self.db.execute(SQL_MARK_TERMINAL_NOTICE_ATTEMPT, &[])?;
            if self.db.query(SQL_SELECT_TERMINAL_SPECTATOR_WORK, &[])?
                != vec![vec![int(deadline), int(1), int(deadline)]]
            {
                return Err(GameError::Storage);
            }
            Ok(notices)
        })
    }

    /// Last synchronous cut matches captured endpoint metadata, never attachment-supplied authority.
    pub fn validate_spectator_notice_delivery(
        &self,
        notice: &SpectatorTerminalNotice,
    ) -> Result<(), GameError> {
        self.maintenance(|now| {
            let g = &notice.connection;
            if self.record_optional()?.is_none_or(|r| {
                r.id != g.game || !matches!(r.state, GameState::Cancelled | GameState::Resolved)
            }) {
                return Err(GameError::Unauthorized);
            }
            if now >= notice.expires_at {
                return Err(GameError::Unauthorized);
            }
            let work = self.db.query(SQL_SELECT_TERMINAL_SPECTATOR_WORK, &[])?;
            if work.first().is_none_or(|w| {
                number(w, 1) != Ok(1) || number(w, 0).is_ok_and(|deadline| now >= deadline)
            }) {
                return Err(GameError::Unauthorized);
            }
            if self.db.query(
                SQL_SELECT_TERMINAL_NOTICE_BY_CONNECTION,
                &[text(g.connection)],
            )? != vec![vec![
                text(g.session),
                text(g.spectator),
                int(g.epoch),
                int(g.expires),
                text(&notice.notice),
                int(notice.expires_at),
            ]] {
                return Err(GameError::Unauthorized);
            }
            Ok(())
        })
    }

    /// Always run after the bounded attempt, including unavailable/closed/throwing transports.
    /// Failed storage deletion rolls back only this cleanup, retaining durable alarm work.
    pub fn finish_spectator_terminal_attempt(&self) -> Result<(), GameError> {
        self.maintenance(|now| {
            if self
                .db
                .query(SQL_SELECT_TERMINAL_SPECTATOR_WORK, &[])?
                .is_empty()
            {
                return Ok(());
            }
            for id in self.terminal_spectator_ids()? {
                self.delete_spectator(id, now)?;
            }
            for (delete, check) in TERMINAL_SPECTATOR_PURGE {
                self.db.execute(delete, &[])?;
                if !self.db.query(check, &[])?.is_empty() {
                    return Err(GameError::Storage);
                }
            }
            Ok(())
        })
    }

    pub fn retry_spectator_terminal_cleanup(&self) -> Result<(), GameError> {
        self.maintenance(|now| {
            let rows = self.db.query(SQL_SELECT_TERMINAL_SPECTATOR_WORK, &[])?;
            let Some(row) = rows.first() else {
                return Ok(());
            };
            let deadline = number(row, 0)?;
            let retry = add_time(now, RETRY_INITIAL_MS)?;
            self.db
                .execute(SQL_RETRY_TERMINAL_SPECTATOR_CLEANUP, &[int(retry)])?;
            if self.db.query(SQL_SELECT_TERMINAL_SPECTATOR_WORK, &[])?
                != vec![vec![int(deadline), int(1), int(retry)]]
            {
                return Err(GameError::Storage);
            }
            Ok(())
        })
    }
}
