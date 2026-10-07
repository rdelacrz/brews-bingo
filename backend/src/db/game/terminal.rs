//! Terminal access is derived from immutable History and original grants only.
use super::{
    ACCOUNT_KIND, ConnectionGrant, Database, GameAccountAuthority, GameError, GameService,
    HOST_VIEW, JS_SAFE_INTEGER_MAX, PLAYER_KIND, PlayerSession, Record, Runtime,
    SQL_DELETE_CONNECTION_GRANT_BY_ID, SQL_DELETE_CURRENT_CONNECTION_BY_SESSION, SqlValue,
    add_three_calendar_months, int, number, optional_text, string, text,
};
use brews_contracts::games::{BoardView, CallView, FinalBoardView, GameView, WinnerView};
use brews_domain::{
    games::{BoardCell, BoardCellKind, CellPosition, GameState, SingleLine, SingleLinePattern},
    ids::{PlayerId, SessionId},
};

const FINAL_GRANT_LIMIT: usize = 100;
const SQL_DELETE_FINAL_PLAYER_ACCESS: &str = "DELETE FROM game_terminal_views WHERE player_id=?";
const SQL_SELECT_ANY_FINAL_PLAYER_ACCESS: &str =
    "SELECT 1 FROM game_terminal_views WHERE player_id=? LIMIT 1";
const SQL_DELETE_FINAL_PRINCIPAL_CONNECTION_GRANTS: &str =
    "DELETE FROM game_connection_grants WHERE viewer_kind=? AND principal_id=?";
const SQL_SELECT_ANY_FINAL_PRINCIPAL_CONNECTION_GRANT: &str =
    "SELECT 1 FROM game_connection_grants WHERE viewer_kind=? AND principal_id=? LIMIT 1";
const SQL_DELETE_FINAL_PRINCIPAL_CONNECTIONS: &str =
    "DELETE FROM game_current_connections WHERE viewer_kind=? AND principal_id=?";
const SQL_SELECT_ANY_FINAL_PRINCIPAL_CONNECTION: &str =
    "SELECT 1 FROM game_current_connections WHERE viewer_kind=? AND principal_id=? LIMIT 1";
const SQL_SELECT_TERMINAL_ROUTING_PROJECTION: &str = "SELECT game_id,host_id,fingerprint,state,revision,game_code,created_at,started_at,ended_at,history_expires_at FROM game_terminal_route WHERE singleton=1";
const SQL_INSERT_TERMINAL_ROUTING_PROJECTION: &str = "INSERT INTO game_terminal_route(singleton,game_id,host_id,fingerprint,state,revision,game_code,created_at,started_at,ended_at,history_expires_at) VALUES(1,?,?,?,?,?,?,?,?,?,?)";
const SQL_DELETE_HISTORY_CELLS: &str = "DELETE FROM game_history_board_cells";
const SQL_SELECT_ANY_HISTORY_CELL: &str = "SELECT 1 FROM game_history_board_cells LIMIT 1";
const SQL_DELETE_HISTORY_CALLS: &str = "DELETE FROM game_history_calls";
const SQL_SELECT_ANY_HISTORY_CALL: &str = "SELECT 1 FROM game_history_calls LIMIT 1";
const SQL_DELETE_HISTORY_PLAYERS: &str = "DELETE FROM game_history_players";
const SQL_SELECT_ANY_HISTORY_PLAYER: &str = "SELECT 1 FROM game_history_players LIMIT 1";
const SQL_DELETE_HISTORY_PARENT: &str = "DELETE FROM game_history";
const SQL_SELECT_ANY_HISTORY_PARENT: &str = "SELECT 1 FROM game_history LIMIT 1";
const SQL_DELETE_FINAL_PLAYER_GRANTS: &str = "DELETE FROM game_terminal_views";
const SQL_SELECT_ANY_FINAL_PLAYER_GRANT: &str = "SELECT 1 FROM game_terminal_views LIMIT 1";
const SQL_DELETE_VIEW_REVISIONS: &str = "DELETE FROM game_view_revisions";
const SQL_SELECT_ANY_VIEW_REVISION: &str = "SELECT 1 FROM game_view_revisions LIMIT 1";
const SQL_DELETE_LIVE_RECORD: &str = "DELETE FROM game_record";
const SQL_SELECT_ANY_LIVE_RECORD: &str = "SELECT 1 FROM game_record LIMIT 1";
const EXPIRED_HISTORY_PURGE: [(&str, &str); 10] = [
    (SQL_DELETE_HISTORY_CELLS, SQL_SELECT_ANY_HISTORY_CELL),
    (SQL_DELETE_HISTORY_CALLS, SQL_SELECT_ANY_HISTORY_CALL),
    (SQL_DELETE_HISTORY_PLAYERS, SQL_SELECT_ANY_HISTORY_PLAYER),
    (SQL_DELETE_HISTORY_PARENT, SQL_SELECT_ANY_HISTORY_PARENT),
    (
        SQL_DELETE_FINAL_PLAYER_GRANTS,
        SQL_SELECT_ANY_FINAL_PLAYER_GRANT,
    ),
    (
        super::SQL_DELETE_GAME_CONNECTIONS,
        super::SQL_SELECT_ANY_GAME_CONNECTION,
    ),
    (
        super::SQL_DELETE_GAME_CONNECTION_GRANTS_ALL,
        super::SQL_SELECT_ANY_GAME_CONNECTION_GRANT,
    ),
    (SQL_DELETE_VIEW_REVISIONS, SQL_SELECT_ANY_VIEW_REVISION),
    (
        super::SQL_DELETE_GAME_RECEIPTS,
        super::SQL_SELECT_ANY_GAME_RECEIPT,
    ),
    (SQL_DELETE_LIVE_RECORD, SQL_SELECT_ANY_LIVE_RECORD),
];
const SQL_SELECT_ELIGIBLE_FINAL_PLAYER_SESSIONS: &str = "SELECT s.session_id,s.player_id,s.token_verifier,s.session_epoch,s.expires_at FROM game_sessions s JOIN game_players p ON p.player_id=s.player_id AND p.session_epoch=s.session_epoch WHERE s.game_id=? AND s.game_code=? AND s.access=? AND s.revoked_at IS NULL AND s.issued_at<=? AND s.expires_at>? ORDER BY s.session_id LIMIT 101";
const SQL_INSERT_FINAL_PLAYER_SESSION: &str = "INSERT INTO game_terminal_views(session_id,player_id,token_verifier,session_epoch,expires_at) VALUES(?,?,?,?,?)";
const SQL_SELECT_FINAL_PLAYER_SESSIONS: &str = "SELECT session_id,player_id,token_verifier,session_epoch,expires_at FROM game_terminal_views ORDER BY session_id LIMIT 101";
const SQL_SELECT_FINAL_PLAYER_TOKEN: &str = "SELECT session_id,player_id,session_epoch,expires_at FROM game_terminal_views WHERE token_verifier=?";
const SQL_SELECT_EXPIRED_FINAL_PLAYER_SESSIONS: &str = "SELECT session_id FROM game_terminal_views WHERE expires_at<=? ORDER BY expires_at,session_id LIMIT 100";
const SQL_DELETE_FINAL_PLAYER_SESSION: &str = "DELETE FROM game_terminal_views WHERE session_id=?";
const SQL_SELECT_ANY_FINAL_PLAYER_SESSION: &str =
    "SELECT 1 FROM game_terminal_views WHERE session_id=?";
const SQL_SELECT_UNUSED_FINAL_VIEW_COUNTERS: &str = "SELECT view_key FROM game_view_revisions WHERE (view_key=? AND NOT EXISTS(SELECT 1 FROM game_connection_grants WHERE viewer_kind=? AND connection_id IS NULL AND expires_at>?)) OR (view_key<>? AND NOT EXISTS(SELECT 1 FROM game_terminal_views WHERE player_id=view_key AND expires_at>?)) ORDER BY view_key LIMIT 22";
const SQL_DELETE_FINAL_VIEW_COUNTER: &str = "DELETE FROM game_view_revisions WHERE view_key=?";
const SQL_SELECT_ANY_FINAL_VIEW_COUNTER: &str =
    "SELECT 1 FROM game_view_revisions WHERE view_key=?";
const SQL_SELECT_FINAL_PLAYER_SESSION: &str =
    "SELECT player_id,session_epoch,expires_at FROM game_terminal_views WHERE session_id=?";
const SQL_SELECT_HISTORY_CALLS: &str = "SELECT sequence_no,value FROM game_history_calls WHERE game_id=? ORDER BY sequence_no LIMIT 1001";
const SQL_SELECT_HISTORY_PLAYERS: &str = "SELECT player_id,alias,side_length FROM game_history_players WHERE game_id=? ORDER BY player_id LIMIT 21";
const SQL_SELECT_HISTORY_BOARD: &str = "SELECT row,column,kind,value,is_matched FROM game_history_board_cells WHERE game_id=? AND player_id=? ORDER BY row,column LIMIT 101";
const SQL_SELECT_ALL_CONNECTION_GRANTS: &str = "SELECT grant_id,viewer_kind,session_id,principal_id,connection_id,epoch,entered_at,expires_at FROM game_connection_grants ORDER BY grant_id LIMIT 201";
const SQL_SELECT_ALL_CURRENT_CONNECTIONS: &str = "SELECT viewer_kind,session_id,principal_id,connection_id,epoch,expires_at FROM game_current_connections ORDER BY viewer_kind,session_id LIMIT 101";
const SQL_SELECT_CONNECTION_GRANT: &str = "SELECT 1 FROM game_connection_grants WHERE grant_id=?";
const SQL_SELECT_CURRENT_CONNECTION: &str = "SELECT 1 FROM game_current_connections WHERE viewer_kind=? AND session_id=? AND connection_id=?";

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    /// Final response-release cut: current trusted Accounts proof, never viewer admission.
    /// The transport must obtain this proof after its final await and release without another await.
    pub fn authorize_gameplay_response(
        &self,
        authority: GameAccountAuthority,
    ) -> Result<(), GameError> {
        self.db
            .transaction(|| {
                super::validate_schema(self.db)?;
                let clock = self.db.query(
                    super::SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
                    &[],
                )?;
                let row = clock.first().ok_or(super::StorageError)?;
                let previous = number(row, 0).map_err(|_| super::StorageError)?;
                let floor = number(row, 1).map_err(|_| super::StorageError)?;
                let now = self.runtime.now_ms();
                if !(0..=JS_SAFE_INTEGER_MAX).contains(&now) || now < previous || floor > previous {
                    return Err(super::StorageError);
                }
                let result = self.record_optional().and_then(|record| {
                    let record = record.ok_or(GameError::NotFound)?;
                    super::authorize_mutation(&authority, &record, now)
                });
                match result {
                    Err(GameError::Storage) => Err(super::StorageError),
                    result => Ok(result),
                }
            })
            .map_err(|_| GameError::Storage)?
    }

    /// Ends final-view access for the stable player across every original session.
    /// Once grants are gone, a player token cannot authenticate even an exact retry.
    pub fn exit_player(
        &self,
        token: &str,
        command: brews_domain::ids::CommandId,
    ) -> Result<brews_contracts::games::GameResponse, GameError> {
        let fingerprint = super::command_fingerprint("exit_player", 0);
        self.tx(|now| {
            let record = self.record()?;
            let session = self.final_player_session(token, &record, now)?;
            let actor = super::player_actor(session.player);
            if self.command_work(&actor, command)?.is_some() {
                return Err(GameError::Conflict);
            }
            if let Some(receipt) = self.receipt(&actor, command, fingerprint, now)? {
                return Ok(brews_contracts::games::GameResponse::Committed { receipt });
            }
            self.admit(command, now)?;
            self.db
                .execute(SQL_DELETE_FINAL_PLAYER_ACCESS, &[text(session.player)])?;
            if !self
                .db
                .query(SQL_SELECT_ANY_FINAL_PLAYER_ACCESS, &[text(session.player)])?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
            self.revoke_final_principal_connections(PLAYER_KIND, &session.player.to_string())?;
            self.retire_unused_final_counters(now)?;
            let receipt = self.store_receipt(
                &actor,
                command,
                fingerprint,
                brews_contracts::games::GameOutcome::Exited {},
                record.id,
                now,
            )?;
            Ok(brews_contracts::games::GameResponse::Exited {
                game_id: record.id,
                receipt,
            })
        })
    }

    /// Ends final-view access for the account/game across all bound account sessions.
    /// An exact, currently authenticated account retry returns only the secret-free receipt.
    pub fn exit_account(
        &self,
        authority: GameAccountAuthority,
        command: brews_domain::ids::CommandId,
    ) -> Result<brews_contracts::games::GameResponse, GameError> {
        let fingerprint = super::command_fingerprint("exit_account", 0);
        self.tx(|now| {
            super::authorize(&authority, now)?;
            let record = self.record()?;
            self.require_final_history(&record, now)?;
            let actor = super::account_actor(authority.account_id());
            if self.command_work(&actor, command)?.is_some() {
                return Err(GameError::Conflict);
            }
            if let Some(receipt) = self.receipt(&actor, command, fingerprint, now)? {
                return Ok(brews_contracts::games::GameResponse::Committed { receipt });
            }
            self.admit(command, now)?;
            self.enter_account(&authority, now)?;
            self.revoke_final_principal_connections(
                ACCOUNT_KIND,
                &authority.account_id().to_string(),
            )?;
            self.retire_unused_final_counters(now)?;
            let receipt = self.store_receipt(
                &actor,
                command,
                fingerprint,
                brews_contracts::games::GameOutcome::Exited {},
                record.id,
                now,
            )?;
            Ok(brews_contracts::games::GameResponse::Exited {
                game_id: record.id,
                receipt,
            })
        })
    }

    fn revoke_final_principal_connections(
        &self,
        kind: &str,
        principal: &str,
    ) -> Result<(), GameError> {
        let binding = [text(kind), text(principal)];
        for (delete, read) in [
            (
                SQL_DELETE_FINAL_PRINCIPAL_CONNECTION_GRANTS,
                SQL_SELECT_ANY_FINAL_PRINCIPAL_CONNECTION_GRANT,
            ),
            (
                SQL_DELETE_FINAL_PRINCIPAL_CONNECTIONS,
                SQL_SELECT_ANY_FINAL_PRINCIPAL_CONNECTION,
            ),
        ] {
            self.db.execute(delete, &binding)?;
            if !self.db.query(read, &binding)?.is_empty() {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }

    pub(super) fn retire_prestart_counters(&self) -> Result<(), GameError> {
        self.db.execute(SQL_DELETE_VIEW_REVISIONS, &[])?;
        if !self.db.query(SQL_SELECT_ANY_VIEW_REVISION, &[])?.is_empty() {
            return Err(GameError::Storage);
        }
        Ok(())
    }

    pub(super) fn terminal_routing_projection(
        &self,
    ) -> Result<Option<super::GameProjection>, GameError> {
        let rows = self.db.query(SQL_SELECT_TERMINAL_ROUTING_PROJECTION, &[])?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let state: GameState = string(row, 3)?.parse().map_err(|_| GameError::Storage)?;
        let created = number(row, 6)?;
        let started = number(row, 7)?;
        let ended = number(row, 8)?;
        let expires = number(row, 9)?;
        if !matches!(state, GameState::Resolved | GameState::Cancelled)
            || !(0..=JS_SAFE_INTEGER_MAX).contains(&created)
            || created > started
            || started > ended
            || expires != add_three_calendar_months(ended)?
        {
            return Err(GameError::Storage);
        }
        Ok(Some(super::GameProjection::new(
            string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
            string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
            super::CreationFingerprint::from_digest(
                super::blob(row, 2)?
                    .try_into()
                    .map_err(|_| GameError::Storage)?,
            ),
            state,
            super::SourceRevision::try_from(number(row, 4)?).map_err(|_| GameError::Storage)?,
            Some(string(row, 5)?.parse().map_err(|_| GameError::Storage)?),
            created,
            Some(started),
            Some(ended),
            Some(expires),
        )))
    }

    pub(super) fn purge_expired_history(&self, now: i64) -> Result<(), GameError> {
        let Some(record) = self
            .record_optional()?
            .filter(|r| r.history_expires.is_some_and(|expires| now >= expires))
        else {
            return Ok(());
        };
        // Validate the self-contained snapshot before erasing it or retaining its routing proof.
        self.final_snapshot(&record)?;
        if self.terminal_routing_projection()?.is_some() {
            return Err(GameError::Storage);
        }
        let route = vec![
            text(record.id),
            text(record.host),
            SqlValue::Blob(record.fp.digest().to_vec()),
            text(record.state),
            int(record.revision),
            text(record.code.as_ref().ok_or(GameError::Storage)?.as_str()),
            int(record.created),
            int(record.started.ok_or(GameError::Storage)?),
            int(record.ended.ok_or(GameError::Storage)?),
            int(record.history_expires.ok_or(GameError::Storage)?),
        ];
        self.db
            .execute(SQL_INSERT_TERMINAL_ROUTING_PROJECTION, &route)?;
        if self.db.query(SQL_SELECT_TERMINAL_ROUTING_PROJECTION, &[])? != vec![route] {
            return Err(GameError::Storage);
        }
        let work = self.all_work(2)?;
        match work.as_slice() {
            [] => self.enqueue_terminal_release(&record, record.revision, now)?,
            [work]
                if work.kind == super::WorkKind::Release
                    && work.fence == record.revision
                    && work.command == record.creation_command
                    && work.fingerprint == *record.fp.digest()
                    && work.actor == super::SYSTEM_ACTOR
                    && work.phase == super::WorkPhase::AwaitingAcknowledgement => {}
            _ => return Err(GameError::Storage),
        }
        for (delete, read) in EXPIRED_HISTORY_PURGE {
            self.db.execute(delete, &[])?;
            if !self.db.query(read, &[])?.is_empty() {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }

    pub(super) fn require_final_history(&self, record: &Record, now: i64) -> Result<(), GameError> {
        let expiry = record.history_expires.ok_or(GameError::NotFound)?;
        if record.started.is_none()
            || !matches!(record.state, GameState::Resolved | GameState::Cancelled)
        {
            return Err(GameError::NotFound);
        }
        if now >= expiry {
            return Err(GameError::Expired);
        }
        Ok(())
    }

    pub(super) fn retain_final_grants(&self, record: &Record, now: i64) -> Result<(), GameError> {
        let sessions = self.db.query(
            SQL_SELECT_ELIGIBLE_FINAL_PLAYER_SESSIONS,
            &[
                text(record.id),
                text(record.code.as_ref().ok_or(GameError::Storage)?.as_str()),
                text(super::LIVE_ACCESS),
                int(now),
                int(now),
            ],
        )?;
        if sessions.len() > FINAL_GRANT_LIMIT {
            return Err(GameError::Storage);
        }
        for session in &sessions {
            self.db.execute(SQL_INSERT_FINAL_PLAYER_SESSION, session)?;
        }
        if self.db.query(SQL_SELECT_FINAL_PLAYER_SESSIONS, &[])? != sessions {
            return Err(GameError::Storage);
        }
        self.prune_final_connections(record, now)?;
        self.retire_unused_final_counters(now)
    }

    pub(super) fn cleanup_terminal_access(&self, now: i64) -> Result<(), GameError> {
        let Some(record) = self
            .record_optional()?
            .filter(|r| matches!(r.state, GameState::Resolved | GameState::Cancelled))
        else {
            return Ok(());
        };
        if record.started.is_none() {
            return self.retire_prestart_counters();
        }
        for row in self
            .db
            .query(SQL_SELECT_EXPIRED_FINAL_PLAYER_SESSIONS, &[int(now)])?
        {
            let session: SessionId = string(&row, 0)?.parse().map_err(|_| GameError::Storage)?;
            self.db
                .execute(SQL_DELETE_FINAL_PLAYER_SESSION, &[text(session)])?;
            if !self
                .db
                .query(SQL_SELECT_ANY_FINAL_PLAYER_SESSION, &[text(session)])?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        self.prune_final_connections(&record, now)?;
        self.retire_unused_final_counters(now)
    }

    fn retire_unused_final_counters(&self, now: i64) -> Result<(), GameError> {
        let counters = self.db.query(
            SQL_SELECT_UNUSED_FINAL_VIEW_COUNTERS,
            &[
                text(HOST_VIEW),
                text(ACCOUNT_KIND),
                int(now),
                text(HOST_VIEW),
                int(now),
            ],
        )?;
        if counters.len() > super::RETAINED_PLAYER_LIMIT + 1 {
            return Err(GameError::Storage);
        }
        for row in counters {
            let key = string(&row, 0)?;
            if key != HOST_VIEW {
                key.parse::<PlayerId>().map_err(|_| GameError::Storage)?;
            }
            self.db
                .execute(SQL_DELETE_FINAL_VIEW_COUNTER, &[text(key)])?;
            if !self
                .db
                .query(SQL_SELECT_ANY_FINAL_VIEW_COUNTER, &[text(key)])?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }

    pub(super) fn final_player_session(
        &self,
        token: &str,
        record: &Record,
        now: i64,
    ) -> Result<PlayerSession, GameError> {
        self.require_final_history(record, now)?;
        let digest = crate::security::token_digest(token).map_err(|_| GameError::Unauthorized)?;
        let rows = self.db.query(
            SQL_SELECT_FINAL_PLAYER_TOKEN,
            &[SqlValue::Blob(digest.to_vec())],
        )?;
        let row = rows.first().ok_or(GameError::Unauthorized)?;
        let session = PlayerSession {
            session: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
            player: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
            epoch: number(row, 2)?,
            expires: number(row, 3)?,
        };
        if session.expires <= now {
            return Err(GameError::Unauthorized);
        }
        Ok(session)
    }

    pub(super) fn validate_final_player_connection(
        &self,
        grant: &ConnectionGrant,
        record: &Record,
        now: i64,
    ) -> Result<(), GameError> {
        self.require_final_history(record, now)?;
        let rows = self
            .db
            .query(SQL_SELECT_FINAL_PLAYER_SESSION, &[text(grant.session)])?;
        let row = rows.first().ok_or(GameError::Unauthorized)?;
        if grant.game != record.id
            || grant.expires <= now
            || string(row, 0)? != grant.player.to_string()
            || number(row, 1)? != grant.epoch
            || number(row, 2)? != grant.expires
        {
            return Err(GameError::Unauthorized);
        }
        Ok(())
    }

    pub(super) fn validate_committed_history(&self, record: &Record) -> Result<(), GameError> {
        self.final_snapshot(record).map(|_| ())
    }

    pub(super) fn final_host_projection(
        &self,
        record: &Record,
        now: i64,
    ) -> Result<GameView, GameError> {
        self.require_final_history(record, now)?;
        let mut view = self.final_snapshot(record)?;
        let GameView::FinalHost { view_revision, .. } = &mut view else {
            return Err(GameError::Storage);
        };
        *view_revision = self.view_revision(HOST_VIEW)? as u64;
        Ok(view)
    }

    pub(super) fn final_player_projection(
        &self,
        record: &Record,
        player: PlayerId,
    ) -> Result<GameView, GameError> {
        let GameView::FinalHost {
            players,
            calls,
            winner,
            ..
        } = self.final_snapshot(record)?
        else {
            return Err(GameError::Storage);
        };
        let own = players
            .into_iter()
            .find(|p| p.player_id == player)
            .ok_or(GameError::Storage)?;
        let view = GameView::FinalPlayer {
            game_id: record.id,
            game_code: record.code.clone().ok_or(GameError::Storage)?,
            state: record.state,
            started_at: record.started.ok_or(GameError::Storage)?,
            ended_at: record.ended.ok_or(GameError::Storage)?,
            history_expires_at: record.history_expires.ok_or(GameError::Storage)?,
            winner,
            view_revision: self.view_revision(&player.to_string())? as u64,
            calls,
            player_id: player,
            alias: own.alias,
            board: own.board,
        };
        view.validate().map_err(|_| GameError::Storage)?;
        Ok(view)
    }

    pub(super) fn final_snapshot(&self, record: &Record) -> Result<GameView, GameError> {
        let parent = self
            .db
            .query(super::SQL_SELECT_GAME_HISTORY_PARENT, &[text(record.id)])?;
        let [row] = parent.as_slice() else {
            return Err(GameError::Storage);
        };
        if string(row, 0)? != record.code.as_ref().ok_or(GameError::Storage)?.as_str()
            || string(row, 1)? != record.host.to_string()
            || Some(number(row, 2)?) != record.started
            || Some(number(row, 3)?) != record.ended
            || Some(number(row, 4)?) != record.history_expires
            || string(row, 5)? != record.state.to_string()
            || optional_text(row, 6)?.map(str::to_owned)
                != record.winner_player.map(|id| id.to_string())
            || optional_text(row, 7)? != record.winner_alias.as_deref()
            || number(row, 4)? != add_three_calendar_months(number(row, 3)?)?
        {
            return Err(GameError::Storage);
        }
        let calls = self
            .db
            .query(SQL_SELECT_HISTORY_CALLS, &[text(record.id)])?
            .iter()
            .map(|row| {
                Ok(CallView {
                    sequence_no: u32::try_from(number(row, 0)?).map_err(|_| GameError::Storage)?,
                    value: string(row, 1)?.to_owned(),
                })
            })
            .collect::<Result<Vec<_>, GameError>>()?;
        let player_rows = self
            .db
            .query(SQL_SELECT_HISTORY_PLAYERS, &[text(record.id)])?;
        if !(2..=super::RETAINED_PLAYER_LIMIT).contains(&player_rows.len()) {
            return Err(GameError::Storage);
        }
        let mut players = Vec::with_capacity(player_rows.len());
        for row in player_rows {
            let player_id: PlayerId = string(&row, 0)?.parse().map_err(|_| GameError::Storage)?;
            let side_length = u8::try_from(number(&row, 2)?).map_err(|_| GameError::Storage)?;
            let cells = self
                .db
                .query(
                    SQL_SELECT_HISTORY_BOARD,
                    &[text(record.id), text(player_id)],
                )?
                .iter()
                .map(|row| {
                    let kind = match string(row, 2)? {
                        super::CELL_FREE if row.get(3) == Some(&SqlValue::Null) => {
                            BoardCellKind::Free
                        }
                        super::CELL_VALUE => BoardCellKind::Value(string(row, 3)?.to_owned()),
                        _ => return Err(GameError::Storage),
                    };
                    let matched = number(row, 4)?;
                    if !(0..=1).contains(&matched) {
                        return Err(GameError::Storage);
                    }
                    Ok(BoardCell {
                        position: CellPosition {
                            row: u8::try_from(number(row, 0)?).map_err(|_| GameError::Storage)?,
                            column: u8::try_from(number(row, 1)?)
                                .map_err(|_| GameError::Storage)?,
                        },
                        kind,
                        is_matched: matched == 1,
                    })
                })
                .collect::<Result<Vec<_>, GameError>>()?;
            let lines =
                SingleLine::evaluate(side_length, &cells).map_err(|_| GameError::Storage)?;
            players.push(FinalBoardView {
                player_id,
                alias: string(&row, 1)?.to_owned(),
                board: BoardView {
                    side_length,
                    cells,
                    qualified: !lines.is_empty(),
                    qualifying_lines: lines,
                },
            });
        }
        let winner = match (record.winner_player, record.winner_alias.as_ref()) {
            (Some(player_id), Some(alias)) => Some(WinnerView {
                player_id,
                alias: alias.clone(),
            }),
            (None, None) => None,
            _ => return Err(GameError::Storage),
        };
        // Validation is independent of grants/counters, including after their original expiry.
        let view = GameView::FinalHost {
            game_id: record.id,
            game_code: record.code.clone().ok_or(GameError::Storage)?,
            designated_host_id: record.host,
            state: record.state,
            started_at: record.started.ok_or(GameError::Storage)?,
            ended_at: record.ended.ok_or(GameError::Storage)?,
            history_expires_at: record.history_expires.ok_or(GameError::Storage)?,
            winner,
            view_revision: 0,
            calls,
            players,
        };
        view.validate().map_err(|_| GameError::Storage)?;
        Ok(view)
    }

    pub(super) fn prune_final_connections(
        &self,
        record: &Record,
        now: i64,
    ) -> Result<(), GameError> {
        let grants = self.db.query(SQL_SELECT_ALL_CONNECTION_GRANTS, &[])?;
        if grants.len() > FINAL_GRANT_LIMIT * 2 {
            return Err(GameError::Storage);
        }
        // Anchors are retained independently of socket preparations; no new anchor is minted.
        for row in &grants {
            if number(row, 7)? <= now {
                self.delete_final_connection_grant(string(row, 0)?)?;
            }
        }
        for row in &grants {
            let kind = string(row, 1)?;
            let session: SessionId = string(row, 2)?.parse().map_err(|_| GameError::Storage)?;
            let eligible = match kind {
                PLAYER_KIND => self
                    .db
                    .query(SQL_SELECT_FINAL_PLAYER_SESSION, &[text(session)])?
                    .first()
                    .is_some_and(|s| {
                        s.first() == row.get(3) && s.get(1) == row.get(5) && s.get(2) == row.get(7)
                    }),
                ACCOUNT_KIND => self
                    .db
                    .query(
                        super::SQL_SELECT_ACCOUNT_PREPARED_GRANT_DETAILS,
                        &[text(session), text(ACCOUNT_KIND)],
                    )?
                    .first()
                    .is_some_and(|s| {
                        s.first() == row.get(3) && s.get(1) == row.get(5) && s.get(2) == row.get(7)
                    }),
                _ => false,
            };
            if !eligible || number(row, 7)? <= now {
                self.delete_final_connection_grant(string(row, 0)?)?;
            }
        }
        let connections = self.db.query(SQL_SELECT_ALL_CURRENT_CONNECTIONS, &[])?;
        if connections.len() > FINAL_GRANT_LIMIT {
            return Err(GameError::Storage);
        }
        for row in connections {
            let kind = string(&row, 0)?;
            let session: SessionId = string(&row, 1)?.parse().map_err(|_| GameError::Storage)?;
            let eligible = match kind {
                PLAYER_KIND => self
                    .db
                    .query(SQL_SELECT_FINAL_PLAYER_SESSION, &[text(session)])?
                    .first()
                    .is_some_and(|s| {
                        s.first() == row.get(2) && s.get(1) == row.get(4) && s.get(2) == row.get(5)
                    }),
                ACCOUNT_KIND => self
                    .db
                    .query(
                        super::SQL_SELECT_ACCOUNT_PREPARED_GRANT_DETAILS,
                        &[text(session), text(ACCOUNT_KIND)],
                    )?
                    .first()
                    .is_some_and(|s| {
                        s.first() == row.get(2) && s.get(1) == row.get(4) && s.get(2) == row.get(5)
                    }),
                _ => false,
            };
            if !eligible
                || number(&row, 5)? <= now
                || record.history_expires.is_some_and(|expiry| now >= expiry)
            {
                let binding = [text(kind), text(session), text(string(&row, 3)?)];
                self.db
                    .execute(SQL_DELETE_CURRENT_CONNECTION_BY_SESSION, &binding)?;
                if !self
                    .db
                    .query(SQL_SELECT_CURRENT_CONNECTION, &binding)?
                    .is_empty()
                {
                    return Err(GameError::Storage);
                }
            }
        }
        Ok(())
    }

    fn delete_final_connection_grant(&self, grant: &str) -> Result<(), GameError> {
        let binding = [text(grant)];
        self.db
            .execute(SQL_DELETE_CONNECTION_GRANT_BY_ID, &binding)?;
        if !self
            .db
            .query(SQL_SELECT_CONNECTION_GRANT, &binding)?
            .is_empty()
        {
            return Err(GameError::Storage);
        }
        Ok(())
    }
}
