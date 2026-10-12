//! Bounded audience authority and occupancy; no board or recovery bindings.
use super::{
    GameService, PlayerSession, Record, SQL_INSERT_GAME_PLAYERS, SQL_INSERT_GAME_RECOVERY,
    SQL_INSERT_GAME_SESSIONS, SQL_INSERT_GAME_VIEW_REVISIONS, add_time, advance, int, make_id,
    number, player_actor, string, text,
};

use crate::{
    auth::Runtime,
    db::{Database, SqlValue},
    game::{
        ADMISSION_LIMIT, ADMISSION_MS, GameError, HOST_VIEW, LIVE_ACCESS, PLAYER_SESSION_MS,
        ParticipantOutcome, RECEIPT_MS, SPECTATOR_GRACE_MS, SPECTATOR_KIND, SecretCookie,
        SpectatorConnectionGrant,
    },
};

use brews_contracts::games::{
    GameOutcome, GameResponse, GameView, JoinPlayer, ParticipantChange, ParticipantIdentity,
    SyncResponse,
};

use brews_domain::{
    games::{GameCode, GameState},
    ids::{CommandId, ConnectionId, PlayerId, SessionId, SpectatorId},
};

use sha2::{Digest, Sha256};

const SQL_SELECT_SPECTATOR_SESSION_BY_TOKEN: &str = "SELECT s.session_id,s.spectator_id,s.session_epoch,s.expires_at,s.game_id,s.game_code,s.issued_at,p.grace_expires_at FROM game_spectator_sessions s JOIN game_spectators p ON p.spectator_id=s.spectator_id AND p.session_epoch=s.session_epoch WHERE s.token_verifier=?";

const SQL_SELECT_SPECTATOR_SESSION_BY_ID: &str = "SELECT session_id,spectator_id,session_epoch,expires_at,game_id,game_code,issued_at FROM game_spectator_sessions WHERE session_id=?";

const SQL_SELECT_SPECTATOR_CONTEXT_BY_TOKEN: &str = "SELECT spectator_id,game_code,issued_at,expires_at,retain_until,consumed_command FROM game_spectator_contexts WHERE token_verifier=?";

const SQL_SELECT_CURRENT_SPECTATOR_CONNECTION_BY_SESSION: &str = "SELECT session_id,principal_id,connection_id,epoch,expires_at FROM game_current_connections WHERE viewer_kind=? AND session_id=?";

const SQL_SELECT_SPECTATOR_CONNECTIONS_ORDERED_BY_SESSION: &str = "SELECT session_id,principal_id,connection_id,epoch,expires_at FROM game_current_connections WHERE viewer_kind=? ORDER BY session_id LIMIT ?";

const SQL_SELECT_PREPARED_SPECTATOR_CONNECTION_BY_GRANT: &str = "SELECT session_id,principal_id,connection_id,epoch,expires_at FROM game_connection_grants WHERE viewer_kind=? AND grant_id=?";

const SQL_SELECT_SPECTATOR_PRESENCE_BY_ID: &str = "SELECT spectator_id,joined_at,session_epoch,disconnected_at,grace_expires_at FROM game_spectators WHERE spectator_id=?";

const SQL_SELECT_SPECTATOR_MEMBERSHIP_COUNT: &str = "SELECT count(*) FROM game_spectators";

const SQL_INSERT_DISCONNECTED_SPECTATOR: &str = "INSERT INTO game_spectators VALUES(?,?,0,?,?)";

const SQL_INSERT_SPECTATOR_SESSION: &str =
    "INSERT INTO game_spectator_sessions VALUES(?,?,?,?,?,0,?,?)";

const SQL_SELECT_SPECTATORS_WITH_EXPIRED_GRACE_OR_SESSION: &str = "SELECT p.spectator_id FROM game_spectators p JOIN game_spectator_sessions s ON s.spectator_id=p.spectator_id WHERE p.grace_expires_at<=? OR s.expires_at<=? ORDER BY p.spectator_id LIMIT ?";

const SQL_SELECT_CURRENT_AND_PREPARED_SPECTATOR_CLOSE_TUPLES: &str = "SELECT connection_id,session_id,epoch FROM game_current_connections WHERE viewer_kind=? AND principal_id=? UNION SELECT connection_id,session_id,epoch FROM game_connection_grants WHERE viewer_kind=? AND principal_id=? AND connection_id IS NOT NULL LIMIT ?";

const SQL_UPDATE_SPECTATOR_INITIAL_DISCONNECT_GRACE: &str = "UPDATE game_spectators SET disconnected_at=?,grace_expires_at=? WHERE spectator_id=? AND disconnected_at IS NULL";

const SQL_CLEAR_SPECTATOR_GRACE_ON_ACCEPT: &str =
    "UPDATE game_spectators SET disconnected_at=NULL,grace_expires_at=NULL WHERE spectator_id=?";

const SQL_QUEUE_SPECTATOR_SOCKET_CLOSE: &str =
    "INSERT INTO game_socket_closures VALUES(?,?,?,?,?,0) ON CONFLICT(connection_id) DO NOTHING";
const SQL_SELECT_SPECTATOR_SOCKET_CLOSE_IDENTITY: &str =
    "SELECT session_id,player_id,epoch FROM game_socket_closures WHERE connection_id=?";
const SQL_CHECK_DELIVERY_CONNECTION_TABLE: &str =
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='game_delivery_connections'";
const SQL_SELECT_PREDECESSOR_DELIVERY_BINDING: &str =
    "SELECT session_id,auth_epoch,expires_at FROM game_delivery_connections WHERE connection_id=?";
const SQL_DELETE_PREDECESSOR_DELIVERIES: &str =
    "DELETE FROM game_delivery_pending WHERE connection_id=?";
const SQL_CHECK_PREDECESSOR_DELIVERIES: &str =
    "SELECT 1 FROM game_delivery_pending WHERE connection_id=? LIMIT 1";
const SQL_DELETE_PREDECESSOR_DELIVERY_CONNECTION: &str =
    "DELETE FROM game_delivery_connections WHERE connection_id=?";
const SQL_CHECK_PREDECESSOR_DELIVERY_CONNECTION: &str =
    "SELECT 1 FROM game_delivery_connections WHERE connection_id=?";

const SPECTATOR_LIMIT: usize = brews_domain::games::SPECTATOR_CAPACITY_MAX as usize;
const SPECTATOR_CLOSE_LIMIT: usize = super::CONNECTION_GRANT_LIMIT + 1;
mod terminal;
pub use terminal::SpectatorTerminalNotice;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectatorAuthority {
    pub game_id: brews_domain::ids::GameId,
    pub session_id: SessionId,
    pub spectator_id: SpectatorId,
    pub epoch: i64,
    pub session_expires_at: i64,
    pub valid_until: i64,
}

/// Current output deadline intersected with the captured authority cap.
pub struct SpectatorOutputCut {
    pub checked_at: i64,
    pub valid_until: i64,
}

pub(super) struct SpectatorSession {
    pub(super) session: SessionId,
    pub(super) spectator: SpectatorId,
    pub(super) epoch: i64,
    pub(super) expires: i64,
    pub(super) grace: Option<i64>,
}

struct AudienceCommand {
    command: CommandId,
    actor: String,
    fingerprint: [u8; 32],
}

struct SpectatorContext {
    spectator: SpectatorId,
    expires: i64,
    consumed: Option<CommandId>,
}

pub struct AudienceMutation {
    pub participant: ParticipantOutcome,
    pub clears_player_cookie: bool,
    pub clears_spectator_cookie: bool,
    actor: String,
    fingerprint: [u8; 32],
}

fn actor(id: SpectatorId) -> String {
    format!("spectator:{id}")
}

fn digest(token: &str) -> Result<SqlValue, GameError> {
    Ok(SqlValue::Blob(
        crate::security::token_digest(token)
            .map_err(|_| GameError::Unauthorized)?
            .to_vec(),
    ))
}

fn fingerprint(operation: &str, value: &str) -> [u8; 32] {
    let mut hash = Sha256::new();

    hash.update(b"brews-audience-command-v1\0");

    hash.update(operation.as_bytes());

    hash.update([0]);

    hash.update(value.as_bytes());

    hash.finalize().into()
}

fn live(r: &Record, code: Option<&GameCode>) -> Result<(), GameError> {
    if !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress)
        || r.published.is_none()
        || code.is_some_and(|c| r.code.as_ref() != Some(c))
    {
        return Err(GameError::Conflict);
    }

    Ok(())
}

fn parse_grant(row: &crate::db::Row, r: &Record) -> Result<SpectatorConnectionGrant, GameError> {
    Ok(SpectatorConnectionGrant {
        game: r.id,
        session: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
        spectator: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
        connection: string(row, 2)?.parse().map_err(|_| GameError::Storage)?,
        epoch: number(row, 3)?,
        expires: number(row, 4)?,
    })
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    fn spectator_credential(&self) -> Result<(zeroize::Zeroizing<String>, SessionId), GameError> {
        let token = zeroize::Zeroizing::new(
            crate::security::new_token(self.runtime).map_err(|_| GameError::RandomUnavailable)?,
        );

        let id: SessionId = make_id(self.runtime, self.runtime.now_ms())?
            .try_into()
            .map_err(|_| GameError::Storage)?;

        Ok((token, id))
    }

    pub(super) fn spectator_discovery_authority(
        &self,
        token: &str,
        r: &Record,
        now: i64,
    ) -> Result<(SpectatorId, i64), GameError> {
        let s = self.spectator_session(token, r, now)?;

        Ok((s.spectator, s.expires.min(s.grace.unwrap_or(s.expires))))
    }

    pub(super) fn spectator_session(
        &self,
        token: &str,
        r: &Record,
        now: i64,
    ) -> Result<SpectatorSession, GameError> {
        live(r, None).map_err(|_| GameError::Unauthorized)?;

        let rows = self
            .db
            .query(SQL_SELECT_SPECTATOR_SESSION_BY_TOKEN, &[digest(token)?])?;

        let row = rows.first().ok_or(GameError::Unauthorized)?;

        let grace = if row[7] == SqlValue::Null {
            None
        } else {
            Some(number(row, 7)?)
        };

        if number(row, 3)? <= now
            || number(row, 6)? > now
            || string(row, 4)? != r.id.to_string()
            || r.code.as_ref().map(GameCode::as_str) != Some(string(row, 5)?)
            || grace.is_some_and(|g| now >= g)
        {
            return Err(GameError::Unauthorized);
        }

        Ok(SpectatorSession {
            session: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
            spectator: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
            epoch: number(row, 2)?,
            expires: number(row, 3)?,
            grace,
        })
    }

    fn spectator_context(
        &self,
        token: &str,
        code: &GameCode,
        now: i64,
    ) -> Result<SpectatorContext, GameError> {
        let rows = self
            .db
            .query(SQL_SELECT_SPECTATOR_CONTEXT_BY_TOKEN, &[digest(token)?])?;

        let row = rows.first().ok_or(GameError::Unauthorized)?;

        if string(row, 1)? != code.as_str() || number(row, 2)? > now || number(row, 4)? <= now {
            return Err(GameError::Unauthorized);
        }

        Ok(SpectatorContext {
            spectator: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
            expires: number(row, 3)?,
            consumed: if row[5] == SqlValue::Null {
                None
            } else {
                Some(string(row, 5)?.parse().map_err(|_| GameError::Storage)?)
            },
        })
    }

    pub fn spectator_admission_context(
        &self,
        code: &GameCode,
        context: Option<&str>,
        session: Option<&str>,
    ) -> Result<ParticipantOutcome, GameError> {
        let (token, _) = self.spectator_credential()?;

        let id: SpectatorId = make_id(self.runtime, self.runtime.now_ms())?
            .try_into()
            .map_err(|_| GameError::Storage)?;

        self.tx(|now| {
            let r = self.record()?;

            live(&r, Some(code))?;

            if let Some(token) = session {
                let s = self.spectator_session(token, &r, now)?;
                return Ok(ParticipantOutcome {
                    response: GameResponse::AdmissionContext {
                        expires_at: s.expires,
                    },
                    cookie: None,
                });
            }

            if let Some(token) = context {
                match self.spectator_context(token, code, now) {
                    Ok(c) => {
                        if c.consumed.is_some() {
                            return Err(GameError::Conflict);
                        }

                        if c.expires > now {
                            return Ok(ParticipantOutcome {
                                response: GameResponse::AdmissionContext {
                                    expires_at: c.expires,
                                },
                                cookie: None,
                            });
                        }
                    }

                    Err(GameError::Unauthorized) => {}

                    Err(e) => return Err(e),
                }
            }

            self.db.execute(
                "DELETE FROM game_spectator_contexts WHERE retain_until<=?",
                &[int(now)],
            )?;

            let count = self
                .db
                .query("SELECT count(*) FROM game_spectator_contexts", &[])?;

            if number(count.first().ok_or(GameError::Storage)?, 0)? >= ADMISSION_LIMIT {
                return Err(GameError::Capacity);
            }

            let expires = add_time(now, ADMISSION_MS)?;

            self.db.execute(
                "INSERT INTO game_spectator_contexts VALUES(?,?,?,?,?,?,NULL)",
                &[
                    text(id),
                    digest(&token)?,
                    text(code.as_str()),
                    int(now),
                    int(expires),
                    int(expires),
                ],
            )?;

            let stored = self.spectator_context(&token, code, now)?;

            if stored.spectator != id || stored.expires != expires {
                return Err(GameError::Storage);
            }

            Ok(ParticipantOutcome {
                response: GameResponse::AdmissionContext {
                    expires_at: expires,
                },
                cookie: Some(SecretCookie { token, expires }),
            })
        })
    }

    pub(super) fn spectator_count(&self) -> Result<u8, GameError> {
        let rows = self.db.query(SQL_SELECT_SPECTATOR_MEMBERSHIP_COUNT, &[])?;

        let count = u8::try_from(number(rows.first().ok_or(GameError::Storage)?, 0)?)
            .map_err(|_| GameError::Storage)?;

        if usize::from(count) > SPECTATOR_LIMIT {
            return Err(GameError::Storage);
        }

        Ok(count)
    }

    fn require_spectator_capacity(&self) -> Result<(), GameError> {
        if self.spectator_count()? >= self.configuration()?.spectator_capacity {
            Err(GameError::Capacity)
        } else {
            Ok(())
        }
    }

    fn issue_spectator(
        &self,
        id: SpectatorId,
        r: &Record,
        now: i64,
        expires: i64,
        credential: (zeroize::Zeroizing<String>, SessionId),
    ) -> Result<SecretCookie, GameError> {
        let (token, session) = credential;

        let grace = add_time(now, SPECTATOR_GRACE_MS)?;

        self.db.execute(
            SQL_INSERT_DISCONNECTED_SPECTATOR,
            &[text(id), int(now), int(now), int(grace)],
        )?;

        self.db.execute(
            SQL_INSERT_SPECTATOR_SESSION,
            &[
                text(session),
                digest(&token)?,
                text(r.id),
                text(r.code.as_ref().ok_or(GameError::Storage)?.as_str()),
                text(id),
                int(now),
                int(expires),
            ],
        )?;

        self.db
            .execute(SQL_INSERT_GAME_VIEW_REVISIONS, &[text(id)])?;

        let actual = self
            .spectator_session(&token, r, now)
            .map_err(|_| GameError::Storage)?;

        if actual.session != session
            || actual.spectator != id
            || actual.expires != expires
            || actual.grace != Some(grace)
            || self.view_revision(&id.to_string())? != 0
        {
            return Err(GameError::Storage);
        }

        Ok(SecretCookie { token, expires })
    }

    fn audience_result(
        &self,
        r: &Record,
        request: AudienceCommand,
        identity: ParticipantIdentity,
        change: ParticipantChange,
        cookie: Option<SecretCookie>,
        now: i64,
    ) -> Result<AudienceMutation, GameError> {
        let AudienceCommand {
            command,
            actor,
            fingerprint: fp,
        } = request;

        let (expiry, revision) = match &identity {
            ParticipantIdentity::Player { player_id, .. } => (
                cookie.as_ref().map(SecretCookie::expires_at),
                self.view_revision(&player_id.to_string())?,
            ),
            ParticipantIdentity::Spectator { spectator_id } => (
                cookie.as_ref().map(SecretCookie::expires_at),
                if change == ParticipantChange::Left {
                    0
                } else {
                    self.view_revision(&spectator_id.to_string())?
                },
            ),
        };

        let receipt = self.store_receipt(
            &actor,
            command,
            fp,
            GameOutcome::ParticipantChanged {
                change,
                identity,
                session_expires_at: expiry,
                view_revision: revision as u64,
            },
            r.id,
            now,
        )?;

        Ok(AudienceMutation {
            participant: ParticipantOutcome {
                response: GameResponse::ParticipantChanged { receipt },
                cookie,
            },
            clears_player_cookie: false,
            clears_spectator_cookie: false,
            actor,
            fingerprint: fp,
        })
    }

    pub fn join_spectator(
        &self,
        command: CommandId,
        context: &str,
        session: Option<&str>,
        code: &GameCode,
    ) -> Result<AudienceMutation, GameError> {
        let fp = fingerprint("admit", code.as_str());

        let credential = self.spectator_credential()?;

        self.tx(|now|{

            let r=self.record()?;
live(&r,Some(code))?;

            // A current session proves only its own existing identity and never adds a seat.
            if let Some(token)=session {

                match self.spectator_session(token,&r,now) {

                    Ok(s)=>{

                        let receipt=if let Some(receipt)=self.receipt(&actor(s.spectator),command,fp,now)? {
receipt}
 else {

                            self.admit(command,now)?;

                            self.store_receipt(&actor(s.spectator),command,fp,GameOutcome::ParticipantChanged{
change:ParticipantChange::Reconnected,identity:ParticipantIdentity::Spectator{
spectator_id:s.spectator}
,session_expires_at:Some(s.expires),view_revision:self.view_revision(&s.spectator.to_string())? as u64}
,r.id,now)?
                        }
;

                        return Ok(AudienceMutation{
participant:ParticipantOutcome{
response:GameResponse::Committed{
receipt}
,cookie:None}
,clears_player_cookie:false,clears_spectator_cookie:false,actor:actor(s.spectator),fingerprint:fp}
);

                    }
,
                    Err(e)=>return Err(e),
                }

            }

            let c=self.spectator_context(context,code,now)?;

            if let Some(receipt)=self.receipt(&actor(c.spectator),command,fp,now)? {
return Ok(AudienceMutation{
participant:ParticipantOutcome{
response:GameResponse::Committed{
receipt}
,cookie:None}
,clears_player_cookie:false,clears_spectator_cookie:false,actor:actor(c.spectator),fingerprint:fp}
);
}

            if c.consumed.is_some() || c.expires<=now{
return Err(GameError::Unauthorized);
}

            self.admit(command,now)?;
self.require_spectator_capacity()?;

            let cookie=self.issue_spectator(c.spectator,&r,now,add_time(now,PLAYER_SESSION_MS)?,credential)?;

            let until=add_time(now,RECEIPT_MS)?;

            self.db.execute("UPDATE game_spectator_contexts SET consumed_command=?,retain_until=? WHERE spectator_id=?",&[text(command),int(until),text(c.spectator)])?;

            if self.spectator_context(context,code,now)?.consumed!=Some(command){
return Err(GameError::Storage);
}

            self.set_view_revision(HOST_VIEW,advance(self.view_revision(HOST_VIEW)?)?)?;

            self.audience_result(&r,AudienceCommand{
command,actor:actor(c.spectator),fingerprint:fp}
,ParticipantIdentity::Spectator{
spectator_id:c.spectator}
,ParticipantChange::Admitted,Some(cookie),now)
        }
)
    }

    fn detach_spectator(&self, id: SpectatorId, now: i64) -> Result<(), GameError> {
        let rows = self.db.query(
            SQL_SELECT_CURRENT_AND_PREPARED_SPECTATOR_CLOSE_TUPLES,
            &[
                text(SPECTATOR_KIND),
                text(id),
                text(SPECTATOR_KIND),
                text(id),
                int((SPECTATOR_CLOSE_LIMIT + 1) as i64),
            ],
        )?;

        if rows.len() > SPECTATOR_CLOSE_LIMIT {
            return Err(GameError::Storage);
        }

        for row in rows {
            // The legacy closure column is a UUID principal, not a player membership FK.
            self.db.execute(
                SQL_QUEUE_SPECTATOR_SOCKET_CLOSE,
                &[
                    text(string(&row, 0)?),
                    text(string(&row, 1)?),
                    text(id),
                    int(number(&row, 2)?),
                    int(now),
                ],
            )?;

            if self.db.query(
                SQL_SELECT_SPECTATOR_SOCKET_CLOSE_IDENTITY,
                &[text(string(&row, 0)?)],
            )? != vec![vec![
                text(string(&row, 1)?),
                text(id),
                int(number(&row, 2)?),
            ]] {
                return Err(GameError::Storage);
            }
        }

        for (delete, check) in [
            (
                "DELETE FROM game_current_connections WHERE viewer_kind=? AND principal_id=?",
                "SELECT 1 FROM game_current_connections WHERE viewer_kind=? AND principal_id=? LIMIT 1",
            ),
            (
                "DELETE FROM game_connection_grants WHERE viewer_kind=? AND principal_id=?",
                "SELECT 1 FROM game_connection_grants WHERE viewer_kind=? AND principal_id=? LIMIT 1",
            ),
        ] {
            let b = [text(SPECTATOR_KIND), text(id)];

            self.db.execute(delete, &b)?;

            if !self.db.query(check, &b)?.is_empty() {
                return Err(GameError::Storage);
            }
        }

        Ok(())
    }

    fn delete_spectator(&self, id: SpectatorId, now: i64) -> Result<(), GameError> {
        self.detach_spectator(id, now)?;

        for (delete, check) in [
            (
                "DELETE FROM game_spectator_sessions WHERE spectator_id=?",
                "SELECT 1 FROM game_spectator_sessions WHERE spectator_id=? LIMIT 1",
            ),
            (
                "DELETE FROM game_spectator_contexts WHERE spectator_id=?",
                "SELECT 1 FROM game_spectator_contexts WHERE spectator_id=? LIMIT 1",
            ),
            (
                "DELETE FROM game_view_revisions WHERE view_key=?",
                "SELECT 1 FROM game_view_revisions WHERE view_key=? LIMIT 1",
            ),
            (
                "DELETE FROM game_spectators WHERE spectator_id=?",
                "SELECT 1 FROM game_spectators WHERE spectator_id=? LIMIT 1",
            ),
        ] {
            self.db.execute(delete, &[text(id)])?;

            if !self.db.query(check, &[text(id)])?.is_empty() {
                return Err(GameError::Storage);
            }
        }

        Ok(())
    }

    /// Discovery remains read-only until a spectator's exclusive deadline is reached.
    pub(super) fn expire_discovered_spectators(&self, now: i64) -> Result<bool, GameError> {
        if self
            .db
            .query(
                SQL_SELECT_SPECTATORS_WITH_EXPIRED_GRACE_OR_SESSION,
                &[int(now), int(now), int((SPECTATOR_LIMIT + 1) as i64)],
            )?
            .is_empty()
        {
            return Ok(false);
        }
        self.fence_read_deadline(now)?;
        self.expire_spectators(now)?;
        Ok(true)
    }
    pub(super) fn expire_spectators(&self, now: i64) -> Result<(), GameError> {
        if self
            .record_optional()?
            .is_none_or(|r| matches!(r.state, GameState::Cancelled | GameState::Resolved))
        {
            return Ok(());
        }
        let rows = self.db.query(
            SQL_SELECT_SPECTATORS_WITH_EXPIRED_GRACE_OR_SESSION,
            &[int(now), int(now), int((SPECTATOR_LIMIT + 1) as i64)],
        )?;

        if rows.len() > SPECTATOR_LIMIT {
            return Err(GameError::Storage);
        }

        for row in &rows {
            self.delete_spectator(
                string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
                now,
            )?;
        }

        if !rows.is_empty() {
            self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
        }

        self.db.execute(
            "DELETE FROM game_spectator_contexts WHERE retain_until<=?",
            &[int(now)],
        )?;

        if !self
            .db
            .query(
                "SELECT 1 FROM game_spectator_contexts WHERE retain_until<=? LIMIT 1",
                &[int(now)],
            )?
            .is_empty()
        {
            return Err(GameError::Storage);
        }

        Ok(())
    }

    pub fn leave_spectator(
        &self,
        command: CommandId,
        token: &str,
        expected: GameState,
    ) -> Result<AudienceMutation, GameError> {
        if !matches!(expected, GameState::AwaitingPlayers | GameState::InProgress) {
            return Err(GameError::InvalidInput);
        }

        let fp = fingerprint("leave", &expected.to_string());

        self.tx(|now| {
            let r = self.record()?;

            let s = self.spectator_session(token, &r, now)?;

            self.admit(command, now)?;

            if r.state != expected {
                return Err(GameError::Conflict);
            }

            self.delete_spectator(s.spectator, now)?;

            self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;

            let mut result = self.audience_result(
                &r,
                AudienceCommand {
                    command,
                    actor: actor(s.spectator),
                    fingerprint: fp,
                },
                ParticipantIdentity::Spectator {
                    spectator_id: s.spectator,
                },
                ParticipantChange::Left,
                None,
                now,
            )?;

            result.clears_spectator_cookie = true;

            Ok(result)
        })
    }

    fn remove_player_for_switch(&self, s: &PlayerSession, now: i64) -> Result<(), GameError> {
        self.detach_player_connections(s.player, now)?;

        for (delete, check) in [
            (
                "DELETE FROM game_recovery WHERE player_id=?",
                "SELECT 1 FROM game_recovery WHERE player_id=?",
            ),
            (
                "DELETE FROM game_sessions WHERE player_id=?",
                "SELECT 1 FROM game_sessions WHERE player_id=?",
            ),
            (
                "DELETE FROM game_admission_contexts WHERE player_id=?",
                "SELECT 1 FROM game_admission_contexts WHERE player_id=?",
            ),
            (
                "DELETE FROM game_view_revisions WHERE view_key=?",
                "SELECT 1 FROM game_view_revisions WHERE view_key=?",
            ),
            (
                "DELETE FROM game_players WHERE player_id=?",
                "SELECT 1 FROM game_players WHERE player_id=?",
            ),
        ] {
            self.db.execute(delete, &[text(s.player)])?;

            if !self.db.query(check, &[text(s.player)])?.is_empty() {
                return Err(GameError::Storage);
            }
        }

        Ok(())
    }

    pub fn switch_to_spectator(
        &self,
        command: CommandId,
        token: &str,
    ) -> Result<AudienceMutation, GameError> {
        let credential = self.spectator_credential()?;

        let id: SpectatorId = make_id(self.runtime, self.runtime.now_ms())?
            .try_into()
            .map_err(|_| GameError::Storage)?;

        self.tx(|now| {
            let r = self.record()?;

            let s = self.player_session(token, &r, now)?;

            if r.state != GameState::AwaitingPlayers {
                return Err(GameError::Conflict);
            }

            self.admit(command, now)?;

            self.require_spectator_capacity()?;

            // Generate replacement before destructive writes; failures roll back the entire cut.
            let cookie = self.issue_spectator(id, &r, now, s.expires, credential)?;

            self.remove_player_for_switch(&s, now)?;

            self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;

            let mut out = self.audience_result(
                &r,
                AudienceCommand {
                    command,
                    actor: player_actor(s.player),
                    fingerprint: fingerprint("switch-spectator", ""),
                },
                ParticipantIdentity::Spectator { spectator_id: id },
                ParticipantChange::Switched,
                Some(cookie),
                now,
            )?;

            out.clears_player_cookie = true;

            Ok(out)
        })
    }

    pub fn switch_to_player(
        &self,
        command: CommandId,
        token: &str,
        input: JoinPlayer,
    ) -> Result<AudienceMutation, GameError> {
        let alias = brews_domain::games::validate_alias(&input.alias)
            .map_err(|_| GameError::InvalidInput)?
            .to_owned();

        let answer = input
            .recovery_answer
            .as_ref()
            .map(|a| crate::game::recovery_answers::normalize(a.as_str()))
            .transpose()?;

        let fp = super::join_fingerprint(
            &input.game_code,
            &alias,
            answer.as_deref().map(String::as_str),
            self.rate_key,
        )?;

        let prepared = self.tx(|now| {
            let r = self.record()?;

            live(&r, Some(&input.game_code))?;

            let s = self.spectator_session(token, &r, now)?;

            self.player_switch_capacity(&r, &alias)?;

            self.admit(command, now)?;

            Ok(s)
        })?;

        let verifier = answer
            .as_ref()
            .map(|a| crate::game::recovery_answers::enroll(a, self.policy, self.runtime))
            .transpose()?;

        let token_new = zeroize::Zeroizing::new(
            crate::security::new_token(self.runtime).map_err(|_| GameError::RandomUnavailable)?,
        );

        let token_digest = digest(&token_new)?;

        let player: PlayerId = make_id(self.runtime, self.runtime.now_ms())?
            .try_into()
            .map_err(|_| GameError::Storage)?;

        let sid: SessionId = make_id(self.runtime, self.runtime.now_ms())?
            .try_into()
            .map_err(|_| GameError::Storage)?;

        self.tx(|now|{
let r=self.record()?;
live(&r,Some(&input.game_code))?;
let s=self.spectator_session(token,&r,now)?;
if s.session!=prepared.session || s.spectator!=prepared.spectator || s.epoch!=prepared.epoch || s.expires!=prepared.expires {
return Err(GameError::Unauthorized);
}
self.player_switch_capacity(&r,&alias)?;
self.admit(command,now)?;

            self.db.execute(SQL_INSERT_GAME_PLAYERS,&[text(player),text(&alias),int(now)])?;

            self.db.execute(SQL_INSERT_GAME_SESSIONS,&[text(sid),token_digest,text(r.id),text(input.game_code.as_str()),text(player),text(LIVE_ACCESS),int(now),int(s.expires)])?;

            self.db.execute(SQL_INSERT_GAME_VIEW_REVISIONS,&[text(player)])?;

            if let Some(phc)=&verifier{
self.db.execute(SQL_INSERT_GAME_RECOVERY,&[text(player),text(phc.as_str()),int(crate::game::recovery_answers::NORMALIZATION_VERSION),int(now)])?;
let rows=self.db.query(super::SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT,&[text(player)])?;
if rows!=vec![vec![text(phc.as_str()),int(crate::game::recovery_answers::NORMALIZATION_VERSION),int(now)]]{
return Err(GameError::Storage);
}
}

            let actual=self.player_session(&token_new,&r,now).map_err(|_|GameError::Storage)?;
if actual.session!=sid || actual.player!=player || actual.expires!=s.expires{
return Err(GameError::Storage);
}

            self.delete_spectator(s.spectator,now)?;
self.set_view_revision(HOST_VIEW,advance(self.view_revision(HOST_VIEW)?)?)?;

            let mut out=self.audience_result(&r,AudienceCommand{
command,actor:actor(s.spectator),fingerprint:fp}
,ParticipantIdentity::Player{
player_id:player,alias}
,ParticipantChange::Switched,Some(SecretCookie{
token:token_new,expires:s.expires}
),now)?;
out.clears_spectator_cookie=true;
Ok(out)
        }
)
    }

    fn player_switch_capacity(&self, r: &Record, alias: &str) -> Result<(), GameError> {
        if r.state != GameState::AwaitingPlayers {
            return Err(GameError::Conflict);
        }

        if !self
            .db
            .query(super::SQL_SELECT_PLAYER_BY_ALIAS, &[text(alias)])?
            .is_empty()
        {
            return Err(GameError::Conflict);
        }

        let count = self.db.query(super::SQL_SELECT_GAME_PLAYERS_COUNT, &[])?;

        if number(count.first().ok_or(GameError::Storage)?, 0)?
            >= i64::from(self.configuration()?.player_capacity)
        {
            return Err(GameError::Capacity);
        }

        Ok(())
    }

    /// Recheck after each await; no old-role retry grant survives a switch or Leave.
    pub fn audience_mutation_deadline(
        &self,
        out: &AudienceMutation,
        context: Option<&str>,
        current_token: Option<&str>,
    ) -> Result<Option<i64>, GameError> {
        self.maintenance(|now| {
            let receipt = match &out.participant.response {
                GameResponse::ParticipantChanged { receipt }
                | GameResponse::Committed { receipt } => receipt,
                _ => return Err(GameError::Storage),
            };

            let stored = self
                .receipt(&out.actor, receipt.command_id, out.fingerprint, now)?
                .ok_or(GameError::Unauthorized)?;

            if stored.encode_json().map_err(|_| GameError::Storage)?
                != receipt.encode_json().map_err(|_| GameError::Storage)?
            {
                return Err(GameError::Unauthorized);
            }

            let GameOutcome::ParticipantChanged {
                change,
                identity,
                session_expires_at,
                ..
            } = &receipt.outcome
            else {
                return Err(GameError::Storage);
            };

            if *change == ParticipantChange::Left {
                return Ok(None);
            }

            let r = self.record()?;

            live(&r, None).map_err(|_| GameError::Unauthorized)?;

            let token = out
                .participant
                .cookie
                .as_ref()
                .map(SecretCookie::token)
                .or(current_token);

            let expiry = if let Some(token) = token {
                match identity {
                    ParticipantIdentity::Spectator { spectator_id } => {
                        let s = self.spectator_session(token, &r, now)?;

                        if s.spectator != *spectator_id || Some(s.expires) != *session_expires_at {
                            return Err(GameError::Unauthorized);
                        }

                        s.grace.map_or(s.expires, |g| g.min(s.expires))
                    }

                    ParticipantIdentity::Player { player_id, .. } => {
                        let s = self.player_session(token, &r, now)?;

                        if s.player != *player_id || Some(s.expires) != *session_expires_at {
                            return Err(GameError::Unauthorized);
                        }

                        s.expires
                    }
                }
            } else {
                // Context proves only an admitted actor's bounded secret-free receipt.
                if *change != ParticipantChange::Admitted {
                    return Err(GameError::Unauthorized);
                }

                let c = self.spectator_context(
                    context.ok_or(GameError::Unauthorized)?,
                    r.code.as_ref().ok_or(GameError::Storage)?,
                    now,
                )?;

                if actor(c.spectator) != out.actor || c.consumed != Some(receipt.command_id) {
                    return Err(GameError::Unauthorized);
                }

                receipt.expires_at
            };

            Ok(Some(
                expiry.min(receipt.expires_at).min(r.due.unwrap_or(expiry)),
            ))
        })
    }

    pub fn spectator_view(&self, token: &str) -> Result<GameView, GameError> {
        self.tx(|now| {
            let r = self.record()?;

            let s = self.spectator_session(token, &r, now)?;

            self.spectator_projection(&r, s.spectator)
        })
    }

    pub(super) fn spectator_projection(
        &self,
        r: &Record,
        id: SpectatorId,
    ) -> Result<GameView, GameError> {
        let configuration = self.configuration()?;

        let calls = self
            .call_history(r, configuration.numeric_upper_bound)?
            .into_iter()
            .map(|c| c.view)
            .collect::<Vec<_>>();

        Ok(GameView::Spectator {
            game_id: r.id,
            state: r.state,
            game_code: r.code.clone().ok_or(GameError::Storage)?,
            started_at: r.started,
            view_revision: self.view_revision(&id.to_string())? as u64,
            spectator_id: id,
            exhausted: calls.len() == configuration.numeric_upper_bound as usize,
            latest_call: calls.last().cloned(),
            calls,
            configuration,
        })
    }

    pub fn sync_spectator(
        &self,
        token: &str,
        known: Option<u64>,
    ) -> Result<(SyncResponse, SpectatorAuthority), GameError> {
        self.tx(|now| {
            if let Some(k) = known {
                super::revision(k)?;
            }

            let r = self.record()?;

            let s = self.spectator_session(token, &r, now)?;

            let view = self.spectator_projection(&r, s.spectator)?;

            let rev = view.view_revision();

            let deadline = s
                .expires
                .min(s.grace.unwrap_or(s.expires))
                .min(r.due.unwrap_or(s.expires));

            Ok((
                SyncResponse {
                    up_to_date: known == Some(rev),
                    view_revision: rev,
                    snapshot: if known == Some(rev) { None } else { Some(view) },
                },
                SpectatorAuthority {
                    game_id: r.id,
                    session_id: s.session,
                    spectator_id: s.spectator,
                    epoch: s.epoch,
                    session_expires_at: s.expires,
                    valid_until: deadline,
                },
            ))
        })
    }

    pub fn release_spectator_output(
        &self,
        token: &str,
        proof: &SpectatorAuthority,
    ) -> Result<SpectatorOutputCut, GameError> {
        self.maintenance(|now| {
            if now >= proof.valid_until {
                return Err(GameError::Unauthorized);
            }

            let r = self.record_optional()?.ok_or(GameError::Unauthorized)?;

            let s = self.spectator_session(token, &r, now)?;

            if r.id != proof.game_id
                || s.session != proof.session_id
                || s.spectator != proof.spectator_id
                || s.epoch != proof.epoch
                || s.expires != proof.session_expires_at
                || proof.valid_until > s.expires
                || proof.valid_until <= 0
            {
                return Err(GameError::Unauthorized);
            }

            let valid_until = proof
                .valid_until
                .min(s.expires)
                .min(s.grace.unwrap_or(s.expires))
                .min(r.due.unwrap_or(s.expires));
            if now >= valid_until {
                return Err(GameError::Unauthorized);
            }
            Ok(SpectatorOutputCut {
                checked_at: now,
                valid_until,
            })
        })
    }

    pub(super) fn advance_audience_views(&self) -> Result<(), GameError> {
        let rows = self.db.query(
            "SELECT spectator_id FROM game_spectators ORDER BY spectator_id LIMIT ?",
            &[int((SPECTATOR_LIMIT + 1) as i64)],
        )?;

        if rows.len() > SPECTATOR_LIMIT {
            return Err(GameError::Storage);
        }

        for row in rows {
            let key = string(&row, 0)?;

            self.set_view_revision(key, advance(self.view_revision(key)?)?)?;
        }

        Ok(())
    }

    pub fn prepare_spectator_connection(
        &self,
        token: &str,
        connection: ConnectionId,
    ) -> Result<SpectatorConnectionGrant, GameError> {
        self.tx(|now| {
            let r = self.record()?;

            let s = self.spectator_session(token, &r, now)?;

            let grant = SpectatorConnectionGrant {
                game: r.id,
                session: s.session,
                spectator: s.spectator,
                connection,
                epoch: s.epoch,
                expires: s.expires,
            };

            self.reserve_connection_grants(1, Some((SPECTATOR_KIND, s.session)), now)?;

            self.delete_prepared_connections(SPECTATOR_KIND, s.session)?;

            self.db.execute(
                super::SQL_INSERT_GAME_CONNECTION_GRANTS,
                &[
                    text(connection),
                    text(SPECTATOR_KIND),
                    text(s.session),
                    text(s.spectator),
                    text(connection),
                    int(s.epoch),
                    int(now),
                    int(s.expires),
                ],
            )?;

            if self.prepared_spectator(&grant)?.as_ref() != Some(&grant) {
                return Err(GameError::Storage);
            }

            Ok(grant)
        })
    }

    fn prepared_spectator(
        &self,
        g: &SpectatorConnectionGrant,
    ) -> Result<Option<SpectatorConnectionGrant>, GameError> {
        let r = self.record()?;

        self.db
            .query(
                SQL_SELECT_PREPARED_SPECTATOR_CONNECTION_BY_GRANT,
                &[text(SPECTATOR_KIND), text(g.connection)],
            )?
            .first()
            .map(|row| parse_grant(row, &r))
            .transpose()
    }

    fn current_spectator(
        &self,
        session: SessionId,
    ) -> Result<Option<SpectatorConnectionGrant>, GameError> {
        let r = self.record()?;

        self.db
            .query(
                SQL_SELECT_CURRENT_SPECTATOR_CONNECTION_BY_SESSION,
                &[text(SPECTATOR_KIND), text(session)],
            )?
            .first()
            .map(|row| parse_grant(row, &r))
            .transpose()
    }

    fn validate_spectator_grant(
        &self,
        g: &SpectatorConnectionGrant,
        r: &Record,
        now: i64,
    ) -> Result<(), GameError> {
        live(r, None).map_err(|_| GameError::Unauthorized)?;

        if g.game != r.id || g.expires <= now {
            return Err(GameError::Unauthorized);
        }

        let rows = self
            .db
            .query(SQL_SELECT_SPECTATOR_SESSION_BY_ID, &[text(g.session)])?;

        let row = rows.first().ok_or(GameError::Unauthorized)?;

        let presence = self
            .db
            .query(SQL_SELECT_SPECTATOR_PRESENCE_BY_ID, &[text(g.spectator)])?;

        let p = presence.first().ok_or(GameError::Unauthorized)?;

        if string(row, 1)? != g.spectator.to_string()
            || number(row, 2)? != g.epoch
            || number(row, 3)? != g.expires
            || string(row, 4)? != r.id.to_string()
            || r.code.as_ref().map(GameCode::as_str) != Some(string(row, 5)?)
            || number(row, 6)? > now
            || number(p, 2)? != g.epoch
            || (p[4] != SqlValue::Null && number(p, 4)? <= now)
        {
            return Err(GameError::Unauthorized);
        }

        Ok(())
    }

    pub fn validate_prepared_spectator_connection(
        &self,
        g: &SpectatorConnectionGrant,
    ) -> Result<(), GameError> {
        self.tx(|now| {
            let r = self.record()?;

            self.validate_spectator_grant(g, &r, now)?;

            if self.prepared_spectator(g)?.as_ref() != Some(g) {
                return Err(GameError::Unauthorized);
            }

            Ok(())
        })
    }

    fn capture_spectator_predecessor(
        &self,
        old: &SpectatorConnectionGrant,
        now: i64,
    ) -> Result<(), GameError> {
        self.db.execute(
            SQL_QUEUE_SPECTATOR_SOCKET_CLOSE,
            &[
                text(old.connection),
                text(old.session),
                text(old.spectator),
                int(old.epoch),
                int(now),
            ],
        )?;
        if self.db.query(
            SQL_SELECT_SPECTATOR_SOCKET_CLOSE_IDENTITY,
            &[text(old.connection)],
        )? != vec![vec![text(old.session), text(old.spectator), int(old.epoch)]]
        {
            return Err(GameError::Storage);
        }
        // Delivery is optional in native service-only owners. If present, retire only
        // the exact captured predecessor in the supersession transaction.
        if !self
            .db
            .query(SQL_CHECK_DELIVERY_CONNECTION_TABLE, &[])?
            .is_empty()
        {
            let binding = self.db.query(
                SQL_SELECT_PREDECESSOR_DELIVERY_BINDING,
                &[text(old.connection)],
            )?;
            if !binding.is_empty()
                && binding != vec![vec![text(old.session), int(old.epoch), int(old.expires)]]
            {
                return Err(GameError::Storage);
            }
            for (delete, check) in [
                (
                    SQL_DELETE_PREDECESSOR_DELIVERIES,
                    SQL_CHECK_PREDECESSOR_DELIVERIES,
                ),
                (
                    SQL_DELETE_PREDECESSOR_DELIVERY_CONNECTION,
                    SQL_CHECK_PREDECESSOR_DELIVERY_CONNECTION,
                ),
            ] {
                self.db.execute(delete, &[text(old.connection)])?;
                if !self.db.query(check, &[text(old.connection)])?.is_empty() {
                    return Err(GameError::Storage);
                }
            }
        }
        Ok(())
    }

    /// Captured close work survives physical identity and pre-start record deletion.
    pub fn spectator_socket_is_fenced(
        &self,
        connection: ConnectionId,
        session: SessionId,
        spectator: SpectatorId,
        epoch: i64,
    ) -> Result<bool, GameError> {
        self.maintenance(|_| {
            Ok(self.db.query(
                SQL_SELECT_SPECTATOR_SOCKET_CLOSE_IDENTITY,
                &[text(connection)],
            )? == vec![vec![text(session), text(spectator), int(epoch)]])
        })
    }

    pub fn accept_spectator_connection(
        &self,
        g: &SpectatorConnectionGrant,
    ) -> Result<(), GameError> {
        self.tx(|now| {
            let r = self.record()?;

            self.validate_spectator_grant(g, &r, now)?;

            let old = self.current_spectator(g.session)?;

            if old.as_ref() == Some(g) {
                return Ok(());
            }

            if self.prepared_spectator(g)?.as_ref() != Some(g) {
                return Err(GameError::Unauthorized);
            }

            self.require_current_connection_capacity(SPECTATOR_KIND, g.session, now)?;

            if let Some(predecessor) = &old {
                self.capture_spectator_predecessor(predecessor, now)?;
            }

            self.db.execute(
                super::SQL_INSERT_GAME_CURRENT_CONNECTIONS,
                &[
                    text(SPECTATOR_KIND),
                    text(g.session),
                    text(g.spectator),
                    text(g.connection),
                    int(g.epoch),
                    int(g.expires),
                ],
            )?;

            self.db
                .execute(SQL_CLEAR_SPECTATOR_GRACE_ON_ACCEPT, &[text(g.spectator)])?;

            self.db.execute(
                super::SQL_DELETE_CONNECTION_GRANT_BY_ID,
                &[text(g.connection)],
            )?;

            let presence = self
                .db
                .query(SQL_SELECT_SPECTATOR_PRESENCE_BY_ID, &[text(g.spectator)])?;

            if self.current_spectator(g.session)?.as_ref() != Some(g)
                || self.prepared_spectator(g)?.is_some()
                || presence
                    .first()
                    .is_none_or(|p| p[3] != SqlValue::Null || p[4] != SqlValue::Null)
            {
                return Err(GameError::Storage);
            }

            if old.is_none() {
                self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
            }

            Ok(())
        })
    }

    pub fn spectator_connection(
        &self,
        connection: ConnectionId,
    ) -> Result<Option<SpectatorConnectionGrant>, GameError> {
        self.tx(|now| {
            let r = self.record()?;

            let rows = self.db.query(
                super::SQL_SELECT_CURRENT_CONNECTION_BY_VIEWER_AND_CONNECTION_ID,
                &[text(SPECTATOR_KIND), text(connection)],
            )?;

            let Some(row) = rows.first() else {
                return Ok(None);
            };

            let g = parse_grant(row, &r)?;

            self.validate_spectator_grant(&g, &r, now)?;

            Ok(Some(g))
        })
    }

    pub fn spectator_connection_view(
        &self,
        g: &SpectatorConnectionGrant,
    ) -> Result<GameView, GameError> {
        self.tx(|now| {
            let r = self.record()?;

            self.validate_spectator_grant(g, &r, now)?;

            if self.current_spectator(g.session)?.as_ref() != Some(g) {
                return Err(GameError::Unauthorized);
            }

            self.spectator_projection(&r, g.spectator)
        })
    }

    pub fn disconnect_spectator(&self, g: &SpectatorConnectionGrant) -> Result<(), GameError> {
        self.maintenance(|now| self.disconnect_spectator_inner(g, now))
    }

    fn disconnect_spectator_inner(
        &self,
        g: &SpectatorConnectionGrant,
        now: i64,
    ) -> Result<(), GameError> {
        if self.current_spectator(g.session)?.as_ref() != Some(g) {
            return Ok(());
        }

        let r = self.record()?;

        self.validate_spectator_grant(g, &r, now)?;

        self.db.execute(
            super::SQL_DELETE_CURRENT_CONNECTION_BY_SESSION,
            &[text(SPECTATOR_KIND), text(g.session), text(g.connection)],
        )?;

        self.db.execute(
            SQL_UPDATE_SPECTATOR_INITIAL_DISCONNECT_GRACE,
            &[
                int(now),
                int(add_time(now, SPECTATOR_GRACE_MS)?),
                text(g.spectator),
            ],
        )?;

        let rows = self
            .db
            .query(SQL_SELECT_SPECTATOR_PRESENCE_BY_ID, &[text(g.spectator)])?;

        let p = rows.first().ok_or(GameError::Storage)?;

        if self.current_spectator(g.session)?.is_some()
            || number(p, 3)? != now
            || number(p, 4)? != add_time(now, SPECTATOR_GRACE_MS)?
        {
            return Err(GameError::Storage);
        }

        self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;

        Ok(())
    }

    pub(super) fn reconcile_spectators(
        &self,
        observed: &[ConnectionId],
        now: i64,
    ) -> Result<(), GameError> {
        let Some(r) = self.record_optional()? else {
            return Ok(());
        };

        if !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress) {
            return Ok(());
        }

        let rows = self.db.query(
            SQL_SELECT_SPECTATOR_CONNECTIONS_ORDERED_BY_SESSION,
            &[text(SPECTATOR_KIND), int((SPECTATOR_LIMIT + 1) as i64)],
        )?;

        if rows.len() > SPECTATOR_LIMIT {
            return Err(GameError::Storage);
        }

        for row in rows {
            let g = parse_grant(&row, &r)?;

            if !observed.contains(&g.connection) {
                self.disconnect_spectator_inner(&g, now)?;
            }
        }

        Ok(())
    }

    pub(super) fn connected_spectator_count(&self, now: i64) -> Result<u8, GameError> {
        let rows = self.db.query(
            SQL_SELECT_SPECTATOR_CONNECTIONS_ORDERED_BY_SESSION,
            &[text(SPECTATOR_KIND), int((SPECTATOR_LIMIT + 1) as i64)],
        )?;

        if rows.len() > SPECTATOR_LIMIT {
            return Err(GameError::Storage);
        }

        Ok(rows
            .iter()
            .filter(|r| number(r, 4).is_ok_and(|expiry| expiry > now))
            .count() as u8)
    }
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub fn release_audience_receipt(
        &self,
        receipt: &brews_contracts::games::GameReceipt,
        token: Option<&str>,
        context: Option<&str>,
    ) -> Result<i64, GameError> {
        self.maintenance(|now|{

            receipt.validate().map_err(|_|GameError::InvalidInput)?;

            if now>=receipt.expires_at{
return Err(GameError::Unauthorized);
}

            let r=self.record()?;
if r.id!=receipt.game_id{
return Err(GameError::Unauthorized);
}
live(&r,None).map_err(|_|GameError::Unauthorized)?;

            let GameOutcome::ParticipantChanged{
change,identity,session_expires_at,..}
=&receipt.outcome else{
return Err(GameError::Unauthorized)}
;

            if *change==ParticipantChange::Left{
return Err(GameError::Unauthorized);
}

            let original=String::from_utf8(receipt.encode_json().map_err(|_|GameError::Storage)?).map_err(|_|GameError::Storage)?;

            if self.db.query("SELECT 1 FROM game_receipts WHERE command_id=? AND outcome=? AND expires_at>? LIMIT 1",&[text(receipt.command_id),text(original),int(now)])?.is_empty(){
return Err(GameError::Unauthorized);
}

            let expires=if let Some(token)=token {

                match identity {

                    ParticipantIdentity::Spectator{
spectator_id}
=>{
let s=self.spectator_session(token,&r,now)?;
if s.spectator!=*spectator_id || Some(s.expires)!=*session_expires_at{
return Err(GameError::Unauthorized);
}
s.expires.min(s.grace.unwrap_or(s.expires))}
,
                    ParticipantIdentity::Player{
player_id,..}
=>{
let s=self.player_session(token,&r,now)?;
if s.player!=*player_id || Some(s.expires)!=*session_expires_at{
return Err(GameError::Unauthorized);
}
s.expires}
,
                }

            }
 else {

                let c=self.spectator_context(context.ok_or(GameError::Unauthorized)?,r.code.as_ref().ok_or(GameError::Storage)?,now)?;

                if *change!=ParticipantChange::Admitted || !matches!(identity,ParticipantIdentity::Spectator{
spectator_id}
 if *spectator_id==c.spectator) || c.consumed!=Some(receipt.command_id){
return Err(GameError::Unauthorized);
}

                receipt.expires_at
            }
;

            Ok(expires.min(r.due.unwrap_or(expires)).min(receipt.expires_at))
        }
)
    }
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub fn release_spectator_context(
        &self,
        token: &str,
        game: brews_domain::ids::GameId,
        code: &GameCode,
        expires: i64,
    ) -> Result<i64, GameError> {
        self.maintenance(|now| {
            let r = self.record()?;

            if r.id != game {
                return Err(GameError::Unauthorized);
            }

            live(&r, Some(code)).map_err(|_| GameError::Unauthorized)?;

            if now >= expires {
                return Err(GameError::Unauthorized);
            }

            match self.spectator_context(token, code, now) {
                Ok(c) => {
                    if c.consumed.is_some() || c.expires != expires {
                        return Err(GameError::Unauthorized);
                    }
                }

                Err(GameError::Unauthorized) => {
                    let s = self.spectator_session(token, &r, now)?;

                    if s.expires != expires {
                        return Err(GameError::Unauthorized);
                    }

                    return Ok(s
                        .expires
                        .min(s.grace.unwrap_or(s.expires))
                        .min(r.due.unwrap_or(s.expires)));
                }

                Err(e) => return Err(e),
            }

            Ok(expires.min(r.due.unwrap_or(expires)))
        })
    }
}
