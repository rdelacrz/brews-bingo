//! Shared bounds for accepted anchors, socket preparations and current connections.
use super::{
    CONNECTION_GRANT_LIMIT, GameService, OBSERVED_CONNECTION_LIMIT,
    SQL_CHECK_CONNECTION_GRANT_EXISTS_BY_ID, SQL_DELETE_CONNECTION_GRANT_BY_ID, int, number,
    string, text,
};
use crate::{auth::Runtime, db::Database, game::GameError};
use brews_domain::ids::SessionId;

const SQL_COUNT_RETAINED_GRANTS: &str = "SELECT count(*) FROM game_connection_grants WHERE expires_at>? AND NOT (viewer_kind=? AND session_id=? AND connection_id IS NOT NULL)";
const SQL_SELECT_EXPIRED_GRANTS: &str = "SELECT grant_id FROM game_connection_grants WHERE expires_at<=? ORDER BY expires_at,grant_id LIMIT ?";
const SQL_SELECT_EXPIRED_CURRENT_CONNECTIONS: &str = "SELECT viewer_kind,session_id,connection_id FROM game_current_connections WHERE expires_at<=? ORDER BY expires_at,connection_id LIMIT ?";
const SQL_CHECK_CURRENT_CONNECTION_IDENTITY: &str = "SELECT 1 FROM game_current_connections WHERE viewer_kind=? AND session_id=? AND connection_id=?";
const SQL_COUNT_OTHER_CURRENT_CONNECTIONS: &str = "SELECT count(*) FROM game_current_connections WHERE expires_at>? AND NOT (viewer_kind=? AND session_id=?)";

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    /// Check the whole addition before changing anchors or replacement preparations.
    pub(super) fn reserve_connection_grants(
        &self,
        additions: usize,
        replacement: Option<(&str, SessionId)>,
        now: i64,
    ) -> Result<(), GameError> {
        let (kind, session) = replacement
            .map(|(kind, session)| (kind, session.to_string()))
            .unwrap_or(("", String::new()));
        let rows = self.db.query(
            SQL_COUNT_RETAINED_GRANTS,
            &[int(now), text(kind), text(session)],
        )?;
        let retained = number(rows.first().ok_or(GameError::Storage)?, 0)?;
        if retained < 0 {
            return Err(GameError::Storage);
        }
        if retained
            > CONNECTION_GRANT_LIMIT
                .checked_sub(additions)
                .ok_or(GameError::Storage)? as i64
        {
            return Err(GameError::Capacity);
        }
        let expired = self.db.query(
            SQL_SELECT_EXPIRED_GRANTS,
            &[int(now), int((CONNECTION_GRANT_LIMIT + 1) as i64)],
        )?;
        if expired.len() > CONNECTION_GRANT_LIMIT {
            return Err(GameError::Storage);
        }
        for row in expired {
            let binding = [text(string(&row, 0)?)];
            self.db
                .execute(SQL_DELETE_CONNECTION_GRANT_BY_ID, &binding)?;
            if !self
                .db
                .query(SQL_CHECK_CONNECTION_GRANT_EXISTS_BY_ID, &binding)?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }

    pub(super) fn require_current_connection_capacity(
        &self,
        kind: &str,
        session: SessionId,
        now: i64,
    ) -> Result<(), GameError> {
        let rows = self.db.query(
            SQL_COUNT_OTHER_CURRENT_CONNECTIONS,
            &[int(now), text(kind), text(session)],
        )?;
        let retained = number(rows.first().ok_or(GameError::Storage)?, 0)?;
        if retained < 0 {
            return Err(GameError::Storage);
        }
        if retained >= OBSERVED_CONNECTION_LIMIT as i64 {
            return Err(GameError::Capacity);
        }
        let expired = self.db.query(
            SQL_SELECT_EXPIRED_CURRENT_CONNECTIONS,
            &[int(now), int((OBSERVED_CONNECTION_LIMIT + 1) as i64)],
        )?;
        if expired.len() > OBSERVED_CONNECTION_LIMIT {
            return Err(GameError::Storage);
        }
        for binding in expired {
            self.db
                .execute(super::SQL_DELETE_CURRENT_CONNECTION_BY_SESSION, &binding)?;
            if !self
                .db
                .query(SQL_CHECK_CURRENT_CONNECTION_IDENTITY, &binding)?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }
}
