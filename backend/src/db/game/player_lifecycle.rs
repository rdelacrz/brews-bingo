//! Allocation-conscious own-membership mutations and receipt-bound output cuts.
use super::{
    GameService, PlayerSession, RETAINED_PLAYER_LIMIT, Record,
    SQL_CHECK_ADMISSION_CONTEXT_EXISTS_FOR_PLAYER, SQL_DELETE_ADMISSION_CONTEXT_BY_PLAYER,
    SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT,
    SQL_SELECT_GAME_VIEW_REVISIONS_REVISION, SQL_SELECT_PLAYER_BY_ALIAS, advance, int, number,
    player_actor, string, text,
};
use crate::auth::Runtime;
use crate::db::Database;
use crate::game::{GameError, HOST_VIEW, PLAYER_KIND};
use brews_contracts::games::{GameOutcome, GameReceipt, GameResponse};
use brews_domain::{games::GameState, ids::CommandId};
use sha2::{Digest as _, Sha256};

const SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID: &str = "SELECT player_id,alias,joined_at,session_epoch,last_explicit_leave_at FROM game_players WHERE player_id=?";
const SQL_RENAME_OWN_ALIAS_WITH_PREVIOUS_ALIAS: &str =
    "UPDATE game_players SET alias=? WHERE player_id=? AND alias=?";
const SQL_UPDATE_EXPLICIT_LEAVE_TIME_BY_PLAYER: &str =
    "UPDATE game_players SET last_explicit_leave_at=? WHERE player_id=?";
const SQL_SELECT_CURRENT_PLAYER_CONNECTIONS: &str = "SELECT connection_id,session_id,epoch FROM game_current_connections WHERE viewer_kind=? AND principal_id=? ORDER BY connection_id LIMIT ?";
const PLAYER_CLOSE_CANDIDATE_LIMIT: usize =
    super::OBSERVED_CONNECTION_LIMIT + super::CONNECTION_GRANT_LIMIT;
const SQL_SELECT_CURRENT_AND_PREPARED_PLAYER_CONNECTIONS: &str = "SELECT connection_id,session_id,epoch FROM game_current_connections WHERE viewer_kind=? AND principal_id=? UNION SELECT connection_id,session_id,epoch FROM game_connection_grants WHERE viewer_kind=? AND principal_id=? AND connection_id IS NOT NULL ORDER BY connection_id LIMIT ?";
const SQL_QUEUE_CAPTURED_PLAYER_SOCKET_CLOSE: &str =
    "INSERT INTO game_socket_closures VALUES(?,?,?,?,?,0) ON CONFLICT(connection_id) DO NOTHING";
const SQL_SELECT_PLAYER_SOCKET_CLOSE_IDENTITY: &str =
    "SELECT session_id,player_id,epoch FROM game_socket_closures WHERE connection_id=?";
const SQL_DELETE_CURRENT_PLAYER_CONNECTIONS: &str =
    "DELETE FROM game_current_connections WHERE viewer_kind=? AND principal_id=?";
const SQL_DELETE_PREPARED_PLAYER_GRANTS: &str =
    "DELETE FROM game_connection_grants WHERE viewer_kind=? AND principal_id=?";
const SQL_CHECK_ANY_PREPARED_PLAYER_GRANT: &str =
    "SELECT 1 FROM game_connection_grants WHERE viewer_kind=? AND principal_id=? LIMIT 1";
const SQL_DELETE_MEMBERSHIP_RECOVERY: &str = "DELETE FROM game_recovery WHERE player_id=?";
const SQL_DELETE_MEMBERSHIP_SESSIONS: &str = "DELETE FROM game_sessions WHERE player_id=?";
const SQL_CHECK_ANY_MEMBERSHIP_SESSION: &str =
    "SELECT 1 FROM game_sessions WHERE player_id=? LIMIT 1";
const SQL_DELETE_MEMBERSHIP_VIEW_REVISION: &str =
    "DELETE FROM game_view_revisions WHERE view_key=?";
const SQL_DELETE_OWN_PLAYER_MEMBERSHIP: &str = "DELETE FROM game_players WHERE player_id=?";

/// Private commit proof; no Debug because the acknowledgement can contain own alias.
pub struct MembershipOutcome {
    pub response: GameResponse,
    session: PlayerSession,
    fingerprint: [u8; 32],
    lobby_deleted: bool,
}
impl MembershipOutcome {
    pub fn clears_player_cookie(&self) -> bool {
        self.lobby_deleted
    }
    fn receipt(&self) -> Result<&GameReceipt, GameError> {
        match &self.response {
            GameResponse::AliasRenamed { receipt, .. }
            | GameResponse::PlayerLeft { receipt, .. }
            | GameResponse::Committed { receipt } => Ok(receipt),
            _ => Err(GameError::Storage),
        }
    }
}
fn fingerprint(operation: &str, value: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"brews-player-lifecycle-v1\0");
    hash.update((operation.len() as u64).to_be_bytes());
    hash.update(operation.as_bytes());
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
    hash.finalize().into()
}
impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub(super) fn detach_player_connections(
        &self,
        player: brews_domain::ids::PlayerId,
        now: i64,
    ) -> Result<bool, GameError> {
        let current = self.db.query(
            SQL_SELECT_CURRENT_PLAYER_CONNECTIONS,
            &[
                text(PLAYER_KIND),
                text(player),
                int((super::OBSERVED_CONNECTION_LIMIT + 1) as i64),
            ],
        )?;
        if current.len() > super::OBSERVED_CONNECTION_LIMIT {
            return Err(GameError::Storage);
        }
        // A grant may already have a native socket even if acceptance rolled back.
        let closes = self.db.query(
            SQL_SELECT_CURRENT_AND_PREPARED_PLAYER_CONNECTIONS,
            &[
                text(PLAYER_KIND),
                text(player),
                text(PLAYER_KIND),
                text(player),
                int((PLAYER_CLOSE_CANDIDATE_LIMIT + 1) as i64),
            ],
        )?;
        if closes.len() > PLAYER_CLOSE_CANDIDATE_LIMIT {
            return Err(GameError::Storage);
        }
        for row in &closes {
            let connection = text(string(row, 0)?);
            let identity = [text(string(row, 1)?), text(player), int(number(row, 2)?)];
            self.db.execute(
                SQL_QUEUE_CAPTURED_PLAYER_SOCKET_CLOSE,
                &[
                    connection.clone(),
                    identity[0].clone(),
                    identity[1].clone(),
                    identity[2].clone(),
                    int(now),
                ],
            )?;
            if self
                .db
                .query(SQL_SELECT_PLAYER_SOCKET_CLOSE_IDENTITY, &[connection])?
                != vec![identity.to_vec()]
            {
                return Err(GameError::Storage);
            }
        }
        let viewer = [text(PLAYER_KIND), text(player)];
        self.db
            .execute(SQL_DELETE_CURRENT_PLAYER_CONNECTIONS, &viewer)?;
        self.db
            .execute(SQL_DELETE_PREPARED_PLAYER_GRANTS, &viewer)?;
        if !self
            .db
            .query(
                SQL_SELECT_CURRENT_PLAYER_CONNECTIONS,
                &[
                    viewer[0].clone(),
                    viewer[1].clone(),
                    int((RETAINED_PLAYER_LIMIT + 1) as i64),
                ],
            )?
            .is_empty()
            || !self
                .db
                .query(SQL_CHECK_ANY_PREPARED_PLAYER_GRANT, &viewer)?
                .is_empty()
        {
            return Err(GameError::Storage);
        }
        Ok(!current.is_empty())
    }
    /// Exact durable detachment proof, independent of provider close completion.
    pub fn player_socket_is_fenced(
        &self,
        game: brews_domain::ids::GameId,
        connection: brews_domain::ids::ConnectionId,
        session: brews_domain::ids::SessionId,
        player: brews_domain::ids::PlayerId,
        epoch: i64,
    ) -> Result<bool, GameError> {
        self.maintenance(|_| {
            if self.record_optional()?.is_none_or(|r| r.id != game) {
                return Ok(false);
            }
            let rows = self
                .db
                .query(SQL_SELECT_PLAYER_SOCKET_CLOSE_IDENTITY, &[text(connection)])?;
            Ok(rows == vec![vec![text(session), text(player), int(epoch)]])
        })
    }
    fn membership_session(
        &self,
        token: &str,
        now: i64,
    ) -> Result<(Record, PlayerSession), GameError> {
        let r = self.record_optional()?.ok_or(GameError::Unauthorized)?;
        if !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress) {
            return Err(GameError::Unauthorized);
        }
        let session = self.player_session(token, &r, now)?;
        Ok((r, session))
    }
    pub fn rename_my_alias(
        &self,
        command: CommandId,
        token: &str,
        alias: &str,
    ) -> Result<MembershipOutcome, GameError> {
        let alias =
            brews_domain::games::validate_alias(alias).map_err(|_| GameError::InvalidInput)?;
        let fp = fingerprint("rename", alias);
        self.maintenance(|now| {
            let (r, session) = self.membership_session(token, now)?;
            if let Some(receipt) = self.receipt(&player_actor(session.player), command, fp, now)? {
                return Ok(MembershipOutcome {
                    response: GameResponse::Committed { receipt },
                    session,
                    fingerprint: fp,
                    lobby_deleted: false,
                });
            }
            self.admit(command, now)?;
            if r.state != GameState::AwaitingPlayers {
                return Err(GameError::Conflict);
            }
            let bindings = [text(session.player)];
            let before = self
                .db
                .query(SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID, &bindings)?;
            let row = before.first().ok_or(GameError::Storage)?;
            let old = string(row, 1)?;
            let mut revision = self.view_revision(&session.player.to_string())?;
            if old != alias {
                if !self
                    .db
                    .query(SQL_SELECT_PLAYER_BY_ALIAS, &[text(alias)])?
                    .is_empty()
                {
                    return Err(GameError::Conflict);
                }
                self.db.execute(
                    SQL_RENAME_OWN_ALIAS_WITH_PREVIOUS_ALIAS,
                    &[text(alias), text(session.player), text(old)],
                )?;
                let mut expected = before.clone();
                expected[0][1] = text(alias);
                if self
                    .db
                    .query(SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID, &bindings)?
                    != expected
                {
                    return Err(GameError::Storage);
                }
                revision = advance(revision)?;
                self.set_view_revision(&session.player.to_string(), revision)?;
                self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
            }
            let receipt = self.store_receipt(
                &player_actor(session.player),
                command,
                fp,
                GameOutcome::AliasRenamed {
                    player_id: session.player,
                    alias: alias.to_owned(),
                    session_expires_at: session.expires,
                    view_revision: revision as u64,
                },
                r.id,
                now,
            )?;
            Ok(MembershipOutcome {
                response: GameResponse::AliasRenamed {
                    player_id: session.player,
                    alias: alias.to_owned(),
                    session_expires_at: session.expires,
                    view_revision: revision as u64,
                    receipt,
                },
                session,
                fingerprint: fp,
                lobby_deleted: false,
            })
        })
    }
    pub fn leave_player(
        &self,
        command: CommandId,
        token: &str,
        expected_state: GameState,
    ) -> Result<MembershipOutcome, GameError> {
        if !matches!(
            expected_state,
            GameState::AwaitingPlayers | GameState::InProgress
        ) {
            return Err(GameError::InvalidInput);
        }
        let fp = fingerprint("leave", &expected_state.to_string());
        self.maintenance(|now| {
            let (r, session) = self.membership_session(token, now)?;
            if let Some(receipt) = self.receipt(&player_actor(session.player), command, fp, now)? {
                return Ok(MembershipOutcome {
                    response: GameResponse::Committed { receipt },
                    session,
                    fingerprint: fp,
                    lobby_deleted: false,
                });
            }
            self.admit(command, now)?;
            if r.state != expected_state {
                return Err(GameError::Conflict);
            }
            let bindings = [text(session.player)];
            let before = self
                .db
                .query(SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID, &bindings)?;
            if before.len() != 1 {
                return Err(GameError::Storage);
            }
            let was_connected = self.detach_player_connections(session.player, now)?;
            let retained = expected_state == GameState::InProgress;
            let revision =
                if retained {
                    self.db.execute(
                        SQL_UPDATE_EXPLICIT_LEAVE_TIME_BY_PLAYER,
                        &[int(now), text(session.player)],
                    )?;
                    let mut expected = before;
                    expected[0][4] = int(now);
                    if self
                        .db
                        .query(SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID, &bindings)?
                        != expected
                    {
                        return Err(GameError::Storage);
                    }
                    // Own public/private view contains no presence or Leave timestamp.
                    self.view_revision(&session.player.to_string())?
                } else {
                    let revision = self.view_revision(&session.player.to_string())?;
                    for (delete, check) in [
                    (
                        SQL_DELETE_MEMBERSHIP_RECOVERY,
                        SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT,
                    ),
                    (SQL_DELETE_MEMBERSHIP_SESSIONS, SQL_CHECK_ANY_MEMBERSHIP_SESSION),
                    (SQL_DELETE_MEMBERSHIP_VIEW_REVISION, SQL_SELECT_GAME_VIEW_REVISIONS_REVISION),
                    (
                        SQL_DELETE_ADMISSION_CONTEXT_BY_PLAYER,
                        SQL_CHECK_ADMISSION_CONTEXT_EXISTS_FOR_PLAYER,
                    ),
                    (SQL_DELETE_OWN_PLAYER_MEMBERSHIP, SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID),
                ] {
                    self.db.execute(delete, &bindings)?;
                    if !self.db.query(check, &bindings)?.is_empty() {
                        return Err(GameError::Storage);
                    }
                }
                    revision
                };
            if !retained || was_connected {
                self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
            }
            let expiry = retained.then_some(session.expires);
            let receipt = self.store_receipt(
                &player_actor(session.player),
                command,
                fp,
                GameOutcome::PlayerLeft {
                    left: true,
                    expected_state,
                    membership_retained: retained,
                    fresh_admission_required: !retained,
                    player_id: session.player,
                    session_expires_at: expiry,
                    view_revision: revision as u64,
                },
                r.id,
                now,
            )?;
            Ok(MembershipOutcome {
                response: GameResponse::PlayerLeft {
                    left: true,
                    expected_state,
                    membership_retained: retained,
                    fresh_admission_required: !retained,
                    player_id: session.player,
                    session_expires_at: expiry,
                    view_revision: revision as u64,
                    receipt,
                },
                session,
                fingerprint: fp,
                lobby_deleted: !retained,
            })
        })
    }
    /// A second cut resolves only an existing receipt; it cannot mutate or detach.
    pub fn membership_output(
        &self,
        command: CommandId,
        token: &str,
        alias: Option<&str>,
        expected_state: Option<GameState>,
    ) -> Result<MembershipOutcome, GameError> {
        let fp = match (alias, expected_state) {
            (Some(alias), None) => fingerprint(
                "rename",
                brews_domain::games::validate_alias(alias).map_err(|_| GameError::InvalidInput)?,
            ),
            (None, Some(state))
                if matches!(state, GameState::AwaitingPlayers | GameState::InProgress) =>
            {
                fingerprint("leave", &state.to_string())
            }
            _ => return Err(GameError::InvalidInput),
        };
        let mut outcome = self.membership_receipt_cut(command, token, fp)?;
        if alias.is_some() {
            let receipt = outcome.receipt()?.clone();
            let GameOutcome::AliasRenamed {
                player_id,
                alias,
                session_expires_at,
                view_revision,
            } = &receipt.outcome
            else {
                return Err(GameError::Unauthorized);
            };
            outcome.response = GameResponse::AliasRenamed {
                player_id: *player_id,
                alias: alias.clone(),
                session_expires_at: *session_expires_at,
                view_revision: *view_revision,
                receipt,
            };
            self.membership_output_deadline(token, &outcome)?;
        }
        Ok(outcome)
    }
    pub fn membership_receipt_output(
        &self,
        command: CommandId,
        token: &str,
        alias: &str,
    ) -> Result<MembershipOutcome, GameError> {
        let alias =
            brews_domain::games::validate_alias(alias).map_err(|_| GameError::InvalidInput)?;
        self.membership_receipt_cut(command, token, fingerprint("rename", alias))
    }
    fn membership_receipt_cut(
        &self,
        command: CommandId,
        token: &str,
        fp: [u8; 32],
    ) -> Result<MembershipOutcome, GameError> {
        self.maintenance(|now| {
            let (_, session) = self.membership_session(token, now)?;
            let receipt = self
                .receipt(&player_actor(session.player), command, fp, now)?
                .ok_or(GameError::Unauthorized)?;
            Ok(MembershipOutcome {
                response: GameResponse::Committed { receipt },
                session,
                fingerprint: fp,
                lobby_deleted: false,
            })
        })
    }
    /// Call immediately after every await, and once more at the private output cut.
    pub fn membership_output_deadline(
        &self,
        token: &str,
        outcome: &MembershipOutcome,
    ) -> Result<Option<i64>, GameError> {
        self.maintenance(|now| {
            let receipt = outcome.receipt()?;
            let stored = self
                .receipt(
                    &player_actor(outcome.session.player),
                    receipt.command_id,
                    outcome.fingerprint,
                    now,
                )?
                .ok_or(GameError::Unauthorized)?;
            if stored.encode_json().map_err(|_| GameError::Storage)?
                != receipt.encode_json().map_err(|_| GameError::Storage)?
            {
                return Err(GameError::Unauthorized);
            }
            if outcome.lobby_deleted {
                // First acknowledgement only: no authentication, recreation or retry grant.
                if !self
                    .db
                    .query(
                        SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID,
                        &[text(outcome.session.player)],
                    )?
                    .is_empty()
                    || !self
                        .db
                        .query(
                            SQL_CHECK_ANY_MEMBERSHIP_SESSION,
                            &[text(outcome.session.player)],
                        )?
                        .is_empty()
                {
                    return Err(GameError::Unauthorized);
                }
                return Ok(None);
            }
            let (r, current) = self.membership_session(token, now)?;
            if current.session != outcome.session.session
                || current.player != outcome.session.player
                || current.epoch != outcome.session.epoch
                || current.expires != outcome.session.expires
            {
                return Err(GameError::Unauthorized);
            }
            if let GameResponse::AliasRenamed {
                alias,
                view_revision,
                ..
            } = &outcome.response
            {
                let rows = self
                    .db
                    .query(SQL_SELECT_MEMBERSHIP_PLAYER_BY_ID, &[text(current.player)])?;
                if r.state != GameState::AwaitingPlayers
                    || string(rows.first().ok_or(GameError::Storage)?, 1)? != alias
                    || self.view_revision(&current.player.to_string())? as u64 != *view_revision
                {
                    return Err(GameError::Unauthorized);
                }
            }
            Ok(Some(
                r.due
                    .map_or(current.expires, |due| due.min(current.expires))
                    .min(receipt.expires_at),
            ))
        })
    }
}
