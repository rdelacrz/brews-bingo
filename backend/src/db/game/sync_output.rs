//! Final Sync cuts retain the existing session, idle and History deadlines.
use super::{GameService, ensure_before_idle};
use crate::{
    auth::{GameAccountAuthority, Runtime},
    db::{Database, StorageError},
    game::GameError,
};
use brews_contracts::games::SyncResponse;
use brews_domain::{
    games::GameState,
    ids::{ConnectionId, GameId, PlayerId, SessionId},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SyncPlayerBinding {
    pub game_id: GameId,
    pub session_id: SessionId,
    pub player_id: PlayerId,
    pub epoch: i64,
    pub session_expires_at: i64,
    pub valid_until: i64,
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub(crate) fn sync_player_output(
        &self,
        token: &str,
        known: Option<u64>,
    ) -> Result<(SyncResponse, SyncPlayerBinding), GameError> {
        self.tx(|now| {
            if let Some(known) = known {
                super::revision(known)?;
            }
            let record = self.record()?;
            ensure_before_idle(&record, now)?;
            let session = self.player_session(token, &record, now)?;
            let view = self.player_projection(&record, session.player)?;
            let view_revision = view.view_revision();
            let deadline = if matches!(record.state, GameState::Resolved | GameState::Cancelled) {
                session
                    .expires
                    .min(record.history_expires.ok_or(GameError::Storage)?)
            } else {
                record
                    .due
                    .map_or(session.expires, |due| due.min(session.expires))
            };
            Ok((
                SyncResponse {
                    up_to_date: known == Some(view_revision),
                    view_revision,
                    snapshot: if known == Some(view_revision) {
                        None
                    } else {
                        Some(view)
                    },
                },
                SyncPlayerBinding {
                    game_id: record.id,
                    session_id: session.session,
                    player_id: session.player,
                    epoch: session.epoch,
                    session_expires_at: session.expires,
                    valid_until: deadline,
                },
            ))
        })
    }
    /// Successful release cuts are read-only; denial fences survive clock rollback.
    pub(crate) fn release_player_sync(
        &self,
        token: &str,
        original: &SyncPlayerBinding,
    ) -> Result<i64, GameError> {
        self.db
            .transaction(|| {
                super::validate_schema(self.db)?;
                let now = self.discovery_clock().map_err(|_| StorageError)?;
                let result = (|| {
                    if !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&original.valid_until)
                        || original.valid_until > original.session_expires_at
                        || original.epoch < 0
                    {
                        return Err(GameError::Storage);
                    }
                    if now >= original.valid_until {
                        return Err(GameError::Unauthorized);
                    }
                    let record = self.record_optional()?.ok_or(GameError::Unauthorized)?;
                    if record.id != original.game_id {
                        return Err(GameError::Unauthorized);
                    }
                    ensure_before_idle(&record, now)?;
                    let session = self.player_session(token, &record, now)?;
                    if session.session != original.session_id
                        || session.player != original.player_id
                        || session.epoch != original.epoch
                        || session.expires != original.session_expires_at
                    {
                        return Err(GameError::Unauthorized);
                    }
                    Ok(now)
                })();
                match result {
                    Err(GameError::Unauthorized | GameError::Expired | GameError::NotFound) => {
                        self.fence_read_deadline(now).map_err(|_| StorageError)?;
                        Ok(Err(GameError::Unauthorized))
                    }
                    Err(GameError::Storage) => Err(StorageError),
                    result => Ok(result),
                }
            })
            .map_err(|_| GameError::Storage)?
    }
    pub(crate) fn sync_account_output(
        &self,
        authority: GameAccountAuthority,
        known: Option<u64>,
        observed: &[ConnectionId],
    ) -> Result<(SyncResponse, i64), GameError> {
        let expires_at = authority.expires_at();
        let response = self.tx(|now| self.sync_account_at(&authority, known, observed, now))?;
        let record = self.record()?;
        let deadline = if matches!(record.state, GameState::Resolved | GameState::Cancelled) {
            expires_at.min(record.history_expires.ok_or(GameError::Storage)?)
        } else {
            record.due.map_or(expires_at, |due| due.min(expires_at))
        };
        Ok((response, deadline))
    }
}
