//! Terminal commands commit immutable History, retire live state, and enqueue release.
use super::{
    CELL_FREE, CELL_VALUE, Database, GameError, GameService, HOST_VIEW, JS_SAFE_INTEGER_MAX,
    PRESTART_PURGE, PendingWork, RETAINED_PLAYER_LIMIT, Record, Runtime, SYSTEM_ACTOR, SqlValue,
    account_actor, advance, authorize_mutation, int, revision, string, text,
};
use super::{
    SQL_CANCEL_UNSTARTED_GAME, SQL_DELETE_GAME_CALLS, SQL_DELETE_PLAYER_VIEW_REVISIONS,
    SQL_SELECT_ANY_GAME_CALL, SQL_SELECT_ANY_PLAYER_VIEW_REVISION,
    SQL_SELECT_GAME_BOARD_PLAYER_IDS, SQL_SELECT_GAME_HISTORY_PARENT,
    SQL_SELECT_GAME_PLAYERS_ALIAS, SQL_SELECT_GAME_PLAYERS_PLAYER_ID_ALIAS,
};
use crate::auth::GameAccountAuthority;
use crate::game::{WorkKind, WorkPhase};
use brews_contracts::games::{GameOutcome, GameReceipt, GameResponse};
use brews_domain::games::{BoardCellKind, GameState};
use brews_domain::ids::{AccountId, CommandId, GameId, PlayerId};
const SQL_INSERT_HISTORY_CALLS: &str =
    "INSERT INTO game_history_calls(game_id,sequence_no,value) VALUES(?,?,?)";
const SQL_INSERT_HISTORY_PLAYERS: &str =
    "INSERT INTO game_history_players(game_id,player_id,alias,side_length) VALUES(?,?,?,?)";
const SQL_INSERT_HISTORY_BOARD_CELLS: &str = "INSERT INTO game_history_board_cells(game_id,player_id,row,column,kind,value,is_matched) VALUES(?,?,?,?,?,?,?)";
const SQL_INSERT_GAME_HISTORY: &str = "INSERT INTO game_history(game_id,game_code,designated_host_id,started_at,ended_at,expires_at,outcome,winner_player_id,winner_alias) VALUES(?,?,?,?,?,?,?,?,?)";
const SQL_UPDATE_GAME_RECORD_TERMINAL: &str = "UPDATE game_record SET state=?,revision=?,ended_at=?,cancellation_reason=?,winner_player_id=?,winner_alias=?,winner_awarded_by_account_id=?,terminal_actor_kind=?,terminal_actor_id=?,history_expires_at=?,idle_due=NULL WHERE singleton=1 AND state=? AND revision=?";
const SQL_SELECT_GAME_RECORD_TERMINAL_FIELDS: &str = "SELECT state,revision,ended_at,cancellation_reason,winner_player_id,winner_alias,winner_awarded_by_account_id,terminal_actor_kind,terminal_actor_id,history_expires_at FROM game_record WHERE singleton=1";
const SQL_SELECT_GAME_HISTORY_CALLS: &str =
    "SELECT sequence_no,value FROM game_history_calls WHERE game_id=? ORDER BY sequence_no";
const SQL_SELECT_GAME_HISTORY_PLAYERS: &str = "SELECT player_id,alias,side_length FROM game_history_players WHERE game_id=? ORDER BY player_id";
const SQL_SELECT_GAME_HISTORY_CELLS: &str = "SELECT player_id,row,column,kind,value,is_matched FROM game_history_board_cells WHERE game_id=? ORDER BY player_id,row,column";
const SQL_DELETE_GAME_SESSIONS_ALL: &str = "DELETE FROM game_sessions";
const SQL_SELECT_ANY_GAME_SESSION: &str = "SELECT 1 FROM game_sessions LIMIT 1";
const SQL_DELETE_GAME_RECOVERY_ALL: &str = "DELETE FROM game_recovery";
const SQL_SELECT_ANY_GAME_RECOVERY: &str = "SELECT 1 FROM game_recovery LIMIT 1";
const SQL_DELETE_GAME_PLAYERS_ALL: &str = "DELETE FROM game_players";
const SQL_SELECT_ANY_GAME_PLAYER: &str = "SELECT 1 FROM game_players LIMIT 1";
const SQL_DELETE_GAME_ADMISSION_CONTEXTS_ALL: &str = "DELETE FROM game_admission_contexts";
const SQL_SELECT_ANY_GAME_ADMISSION_CONTEXT: &str = "SELECT 1 FROM game_admission_contexts LIMIT 1";
const SQL_DELETE_GAME_BOARD_CELLS_ALL: &str = "DELETE FROM game_board_cells";
const SQL_SELECT_ANY_GAME_BOARD_CELL: &str = "SELECT 1 FROM game_board_cells LIMIT 1";
const SQL_DELETE_GAME_BOARDS_ALL: &str = "DELETE FROM game_boards";
const SQL_SELECT_ANY_GAME_BOARD: &str = "SELECT 1 FROM game_boards LIMIT 1";
const SQL_DELETE_GAME_COMPLETED_LINES_ALL: &str = "DELETE FROM game_completed_lines";
const SQL_SELECT_ANY_GAME_COMPLETED_LINE: &str = "SELECT 1 FROM game_completed_lines LIMIT 1";
const SQL_DELETE_GAME_CONFIGURATION_ALL: &str = "DELETE FROM game_configuration";
const SQL_SELECT_ANY_GAME_CONFIGURATION_ROW: &str = "SELECT 1 FROM game_configuration LIMIT 1";
const SQL_DELETE_GAME_FREE_CELLS_ALL: &str = "DELETE FROM game_free_cells";
const SQL_SELECT_ANY_GAME_FREE_CELL: &str = "SELECT 1 FROM game_free_cells LIMIT 1";
const SQL_DELETE_GAME_PENDING_WORK_ALL: &str = "DELETE FROM game_pending_work";
const SQL_SELECT_ANY_GAME_PENDING_WORK: &str = "SELECT 1 FROM game_pending_work LIMIT 1";
const TERMINAL_LIVE_PURGE: [(&str, &str); 12] = [
    (
        "DELETE FROM game_recovery_buckets",
        "SELECT 1 FROM game_recovery_buckets LIMIT 1",
    ),
    (
        SQL_DELETE_GAME_PENDING_WORK_ALL,
        SQL_SELECT_ANY_GAME_PENDING_WORK,
    ),
    (SQL_DELETE_GAME_CALLS, SQL_SELECT_ANY_GAME_CALL),
    (
        SQL_DELETE_GAME_COMPLETED_LINES_ALL,
        SQL_SELECT_ANY_GAME_COMPLETED_LINE,
    ),
    (
        SQL_DELETE_GAME_BOARD_CELLS_ALL,
        SQL_SELECT_ANY_GAME_BOARD_CELL,
    ),
    (SQL_DELETE_GAME_BOARDS_ALL, SQL_SELECT_ANY_GAME_BOARD),
    (SQL_DELETE_GAME_RECOVERY_ALL, SQL_SELECT_ANY_GAME_RECOVERY),
    (SQL_DELETE_GAME_SESSIONS_ALL, SQL_SELECT_ANY_GAME_SESSION),
    (
        SQL_DELETE_GAME_ADMISSION_CONTEXTS_ALL,
        SQL_SELECT_ANY_GAME_ADMISSION_CONTEXT,
    ),
    (SQL_DELETE_GAME_PLAYERS_ALL, SQL_SELECT_ANY_GAME_PLAYER),
    (
        SQL_DELETE_GAME_CONFIGURATION_ALL,
        SQL_SELECT_ANY_GAME_CONFIGURATION_ROW,
    ),
    (
        SQL_DELETE_GAME_FREE_CELLS_ALL,
        SQL_SELECT_ANY_GAME_FREE_CELL,
    ),
];
const MILLISECONDS_PER_UTC_DAY: i64 = 86_400_000;
const HISTORY_RETENTION_CALENDAR_MONTHS: i64 = 3;

struct TerminalCommit<'a> {
    expected_host_revision: i64,
    command: CommandId,
    actor: &'a str,
    fingerprint: [u8; 32],
    expected_state: GameState,
    state: GameState,
    selected_winner: Option<(PlayerId, String)>,
    initiated_by: AccountId,
    now: i64,
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub fn submit_winner(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        player: PlayerId,
        expected_revision: u64,
    ) -> Result<GameResponse, GameError> {
        let expected = revision(expected_revision)?;
        let fingerprint =
            terminal_fingerprint("winner", GameState::InProgress, Some(player), expected);
        self.tx(|now| {
            let record = self.record()?;
            authorize_mutation(&authority, &record, now)?;
            let actor = account_actor(authority.account_id());
            if self.command_work(&actor, command)?.is_some() {
                return Err(GameError::Conflict);
            }
            if let Some(receipt) = self.receipt(&actor, command, fingerprint, now)? {
                return Ok(GameResponse::Committed { receipt });
            }
            self.admit(command, now)?;
            if record.state != GameState::InProgress {
                return Err(GameError::Conflict);
            }
            if self.view_revision(HOST_VIEW)? != expected {
                return Err(GameError::StaleRevision);
            }
            let config = self.configuration()?;
            let calls = self.call_history(&record, config.numeric_upper_bound)?;
            let roster = self
                .db
                .query(SQL_SELECT_GAME_PLAYERS_ALIAS, &[text(player)])?;
            let alias = string(roster.first().ok_or(GameError::NotFound)?, 0)?.to_owned();
            let board = self
                .board_view_at(player, &record, calls.len() as u32)?
                .ok_or(GameError::Conflict)?;
            if !board.qualified {
                return Err(GameError::Conflict);
            }
            self.commit_terminal(
                &record,
                TerminalCommit {
                    expected_host_revision: expected,
                    command,
                    actor: &actor,
                    fingerprint,
                    expected_state: GameState::InProgress,
                    state: GameState::Resolved,
                    selected_winner: Some((player, alias)),
                    initiated_by: authority.account_id(),
                    now,
                },
            )
        })
    }
    pub fn cancel_game(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        expected_state: GameState,
        confirmed: bool,
    ) -> Result<GameResponse, GameError> {
        if !confirmed
            || !matches!(
                expected_state,
                GameState::New | GameState::AwaitingPlayers | GameState::InProgress
            )
        {
            return Err(GameError::InvalidInput);
        }
        let fingerprint = terminal_fingerprint("cancel", expected_state, None, 0);
        if expected_state == GameState::InProgress {
            return self.tx(|now| {
                let record = self.record()?;
                authorize_mutation(&authority, &record, now)?;
                let actor = account_actor(authority.account_id());
                if self.command_work(&actor, command)?.is_some() {
                    return Err(GameError::Conflict);
                }
                if let Some(receipt) = self.receipt(&actor, command, fingerprint, now)? {
                    return Ok(GameResponse::Committed { receipt });
                }
                self.admit(command, now)?;
                if record.state != GameState::InProgress {
                    return Err(GameError::Conflict);
                }
                self.commit_terminal(
                    &record,
                    TerminalCommit {
                        expected_host_revision: self.view_revision(HOST_VIEW)?,
                        command,
                        actor: &actor,
                        fingerprint,
                        expected_state: GameState::InProgress,
                        state: GameState::Cancelled,
                        selected_winner: None,
                        initiated_by: authority.account_id(),
                        now,
                    },
                )
            });
        }
        self.tx(|now| {
            let record = self.record_optional()?.ok_or(GameError::NotFound)?;
            authorize_mutation(&authority, &record, now)?;
            let actor = account_actor(authority.account_id());
            if self.command_work(&actor, command)?.is_some() {
                return Err(GameError::Conflict);
            }
            if let Some(receipt) = self.receipt(&actor, command, fingerprint, now)? {
                return Ok(GameResponse::Committed { receipt });
            }
            self.admit(command, now)?;
            if record.state != expected_state
                || !matches!(record.state, GameState::New | GameState::AwaitingPlayers)
            {
                return Err(GameError::Conflict);
            }
            let game_revision = advance(record.revision)?;
            let host_revision = advance(self.view_revision(HOST_VIEW)?)?;
            self.db.execute(
                SQL_CANCEL_UNSTARTED_GAME,
                &[
                    text(GameState::Cancelled),
                    int(game_revision),
                    int(now),
                    text(crate::game::OPERATOR_CANCEL_REASON),
                    text(crate::game::TERMINAL_ACTOR_ACCOUNT),
                    text(authority.account_id()),
                    text(expected_state),
                    int(record.revision),
                ],
            )?;
            let after = self.record_optional()?.ok_or(GameError::Storage)?;
            if after.state != GameState::Cancelled
                || after.revision != game_revision
                || after.ended != Some(now)
                || after.cancellation_reason.as_deref() != Some(crate::game::OPERATOR_CANCEL_REASON)
                || after.terminal_actor_id != Some(authority.account_id())
            {
                return Err(GameError::Storage);
            }
            for (delete, read) in PRESTART_PURGE {
                self.db.execute(delete, &[])?;
                if !self.db.query(read, &[])?.is_empty() {
                    return Err(GameError::Storage);
                }
            }
            self.db
                .execute(SQL_DELETE_PLAYER_VIEW_REVISIONS, &[text(HOST_VIEW)])?;
            if !self
                .db
                .query(SQL_SELECT_ANY_PLAYER_VIEW_REVISION, &[text(HOST_VIEW)])?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
            self.set_view_revision(HOST_VIEW, host_revision)?;
            self.retire_prestart_counters()?;
            let terminal = TerminalResult {
                game_id: record.id,
                expected_state,
                state: GameState::Cancelled,
                ended_at: now,
                history_available: false,
                history_expires_at: None,
                winner: None,
                view_revision: host_revision as u64,
            };
            let receipt = self.store_receipt(
                &actor,
                command,
                fingerprint,
                terminal_outcome(&terminal),
                record.id,
                now,
            )?;
            self.enqueue_terminal_release(&record, game_revision, now)?;
            Ok(with_terminal_receipt(terminal, receipt))
        })
    }
    fn commit_terminal(
        &self,
        record: &Record,
        terminal: TerminalCommit<'_>,
    ) -> Result<GameResponse, GameError> {
        let TerminalCommit {
            expected_host_revision,
            command,
            actor,
            fingerprint,
            expected_state,
            state,
            selected_winner,
            initiated_by,
            now,
        } = terminal;
        if record.state != GameState::InProgress || expected_state != GameState::InProgress {
            return Err(GameError::Conflict);
        }
        if self.view_revision(HOST_VIEW)? != expected_host_revision {
            return Err(GameError::StaleRevision);
        }
        let configuration = self.configuration()?;
        let calls = self.call_history(record, configuration.numeric_upper_bound)?;
        let player_rows = self
            .db
            .query(SQL_SELECT_GAME_PLAYERS_PLAYER_ID_ALIAS, &[])?;
        let board_rows = self.db.query(SQL_SELECT_GAME_BOARD_PLAYER_IDS, &[])?;
        if player_rows.len() < 2
            || player_rows.len() > RETAINED_PLAYER_LIMIT
            || player_rows.len() != board_rows.len()
        {
            return Err(GameError::Storage);
        }
        let mut history_players = Vec::with_capacity(player_rows.len());
        let mut expected_cells = Vec::new();
        for (player_row, board_row) in player_rows.iter().zip(&board_rows) {
            let player: PlayerId = string(player_row, 0)?
                .parse()
                .map_err(|_| GameError::Storage)?;
            if string(board_row, 0)? != player.to_string() {
                return Err(GameError::Storage);
            }
            let alias = string(player_row, 1)?.to_owned();
            let board = self
                .board_view_at(player, record, calls.len() as u32)?
                .ok_or(GameError::Storage)?;
            if board.cells.len() != usize::from(board.side_length).pow(2) {
                return Err(GameError::Storage);
            }
            for cell in &board.cells {
                let (kind, value) = match &cell.kind {
                    BoardCellKind::Free => (text(CELL_FREE), SqlValue::Null),
                    BoardCellKind::Value(value) => (text(CELL_VALUE), text(value)),
                };
                expected_cells.push(vec![
                    text(player),
                    int(i64::from(cell.position.row)),
                    int(i64::from(cell.position.column)),
                    kind,
                    value,
                    int(i64::from(u8::from(cell.is_matched))),
                ]);
            }
            history_players.push((player, alias, board));
        }
        let winner = match selected_winner {
            Some((player, alias)) => {
                let saved = history_players
                    .iter()
                    .find(|(member, _, _)| *member == player)
                    .ok_or(GameError::Conflict)?;
                if saved.1 != alias || !saved.2.qualified || state != GameState::Resolved {
                    return Err(GameError::Conflict);
                }
                Some(brews_contracts::games::WinnerView {
                    player_id: player,
                    alias,
                })
            }
            None if state == GameState::Cancelled => None,
            None => return Err(GameError::InvalidInput),
        };
        let history_expires = add_three_calendar_months(now)?;
        let game_revision = advance(record.revision)?;
        let host_revision = advance(expected_host_revision)?;
        self.db.execute(
            SQL_UPDATE_GAME_RECORD_TERMINAL,
            &[
                text(state),
                int(game_revision),
                int(now),
                if state == GameState::Cancelled {
                    text(crate::game::OPERATOR_CANCEL_REASON)
                } else {
                    SqlValue::Null
                },
                winner
                    .as_ref()
                    .map_or(SqlValue::Null, |value| text(value.player_id)),
                winner
                    .as_ref()
                    .map_or(SqlValue::Null, |value| text(&value.alias)),
                winner
                    .as_ref()
                    .map_or(SqlValue::Null, |_| text(initiated_by)),
                text(crate::game::TERMINAL_ACTOR_ACCOUNT),
                text(initiated_by),
                int(history_expires),
                text(GameState::InProgress),
                int(record.revision),
            ],
        )?;
        let expected_terminal_fields = vec![vec![
            text(state),
            int(game_revision),
            int(now),
            if state == GameState::Cancelled {
                text(crate::game::OPERATOR_CANCEL_REASON)
            } else {
                SqlValue::Null
            },
            winner
                .as_ref()
                .map_or(SqlValue::Null, |value| text(value.player_id)),
            winner
                .as_ref()
                .map_or(SqlValue::Null, |value| text(&value.alias)),
            winner
                .as_ref()
                .map_or(SqlValue::Null, |_| text(initiated_by)),
            text(crate::game::TERMINAL_ACTOR_ACCOUNT),
            text(initiated_by),
            int(history_expires),
        ]];
        if self.db.query(SQL_SELECT_GAME_RECORD_TERMINAL_FIELDS, &[])? != expected_terminal_fields {
            return Err(GameError::Storage);
        }
        let record_after = self.record()?;
        if record_after.state != state
            || record_after.revision != game_revision
            || record_after.ended != Some(now)
            || record_after.history_expires != Some(history_expires)
            || record_after.terminal_actor_kind.as_deref()
                != Some(crate::game::TERMINAL_ACTOR_ACCOUNT)
            || record_after.terminal_actor_id != Some(initiated_by)
            || record_after.winner_awarded_by != winner.as_ref().map(|_| initiated_by)
            || record_after.cancellation_reason.as_deref()
                != (state == GameState::Cancelled).then_some(crate::game::OPERATOR_CANCEL_REASON)
        {
            return Err(GameError::Storage);
        }
        self.db.execute(
            SQL_INSERT_GAME_HISTORY,
            &[
                text(record.id),
                text(record.code.as_ref().ok_or(GameError::Storage)?.as_str()),
                text(record.host),
                int(record.started.ok_or(GameError::Storage)?),
                int(now),
                int(history_expires),
                text(state),
                winner
                    .as_ref()
                    .map_or(SqlValue::Null, |value| text(value.player_id)),
                winner
                    .as_ref()
                    .map_or(SqlValue::Null, |value| text(&value.alias)),
            ],
        )?;
        for call in &calls {
            self.db.execute(
                SQL_INSERT_HISTORY_CALLS,
                &[
                    text(record.id),
                    int(i64::from(call.view.sequence_no)),
                    text(&call.view.value),
                ],
            )?;
        }
        for (player, alias, board) in &history_players {
            self.db.execute(
                SQL_INSERT_HISTORY_PLAYERS,
                &[
                    text(record.id),
                    text(player),
                    text(alias),
                    int(i64::from(board.side_length)),
                ],
            )?;
            for cell in &board.cells {
                let (kind, value) = match &cell.kind {
                    BoardCellKind::Free => (text(CELL_FREE), SqlValue::Null),
                    BoardCellKind::Value(value) => (text(CELL_VALUE), text(value)),
                };
                self.db.execute(
                    SQL_INSERT_HISTORY_BOARD_CELLS,
                    &[
                        text(record.id),
                        text(player),
                        int(i64::from(cell.position.row)),
                        int(i64::from(cell.position.column)),
                        kind,
                        value,
                        int(i64::from(u8::from(cell.is_matched))),
                    ],
                )?;
            }
        }
        let expected_parent = vec![vec![
            text(record.code.as_ref().ok_or(GameError::Storage)?.as_str()),
            text(record.host),
            int(record.started.ok_or(GameError::Storage)?),
            int(now),
            int(history_expires),
            text(state),
            winner
                .as_ref()
                .map_or(SqlValue::Null, |value| text(value.player_id)),
            winner
                .as_ref()
                .map_or(SqlValue::Null, |value| text(&value.alias)),
        ]];
        if self
            .db
            .query(SQL_SELECT_GAME_HISTORY_PARENT, &[text(record.id)])?
            != expected_parent
        {
            return Err(GameError::Storage);
        }
        let expected_calls = calls
            .iter()
            .map(|call| {
                vec![
                    int(i64::from(call.view.sequence_no)),
                    text(&call.view.value),
                ]
            })
            .collect::<Vec<_>>();
        if self
            .db
            .query(SQL_SELECT_GAME_HISTORY_CALLS, &[text(record.id)])?
            != expected_calls
        {
            return Err(GameError::Storage);
        }
        let expected_players = history_players
            .iter()
            .map(|(player, alias, board)| {
                vec![
                    text(*player),
                    text(alias),
                    int(i64::from(board.side_length)),
                ]
            })
            .collect::<Vec<_>>();
        if self
            .db
            .query(SQL_SELECT_GAME_HISTORY_PLAYERS, &[text(record.id)])?
            != expected_players
            || self
                .db
                .query(SQL_SELECT_GAME_HISTORY_CELLS, &[text(record.id)])?
                != expected_cells
        {
            return Err(GameError::Storage);
        }
        self.validate_committed_history(&record_after)?;
        self.set_view_revision(HOST_VIEW, host_revision)?;
        for (player, _, _) in &history_players {
            let key = player.to_string();
            self.set_view_revision(&key, advance(self.view_revision(&key)?)?)?;
        }
        self.retain_final_grants(&record_after, now)?;
        for (delete, read) in TERMINAL_LIVE_PURGE {
            self.db.execute(delete, &[])?;
            if !self.db.query(read, &[])?.is_empty() {
                return Err(GameError::Storage);
            }
        }
        let host_view_revision = host_revision as u64;
        let terminal = TerminalResult {
            game_id: record.id,
            expected_state,
            state,
            ended_at: now,
            history_available: true,
            history_expires_at: Some(history_expires),
            winner,
            view_revision: host_view_revision,
        };
        let receipt = self.store_receipt(
            actor,
            command,
            fingerprint,
            terminal_outcome(&terminal),
            record.id,
            now,
        )?;
        self.enqueue_terminal_release(record, game_revision, now)?;
        Ok(with_terminal_receipt(terminal, receipt))
    }
    pub(super) fn enqueue_terminal_release(
        &self,
        record: &Record,
        fence: i64,
        now: i64,
    ) -> Result<(), GameError> {
        let work = PendingWork {
            operation: record
                .creation_command
                .to_string()
                .parse()
                .map_err(|_| GameError::Storage)?,
            kind: WorkKind::Release,
            phase: WorkPhase::AwaitingAcknowledgement,
            actor: SYSTEM_ACTOR.into(),
            command: record.creation_command,
            fingerprint: *record.fp.digest(),
            expected: 0,
            fence,
            created: now,
            next: now,
            attempts: 0,
        };
        self.insert_work(&work)
    }
}

fn terminal_fingerprint(
    operation: &str,
    expected_state: GameState,
    player: Option<PlayerId>,
    expected_revision: i64,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"brews-game-terminal-v1\0");
    hash.update(operation.as_bytes());
    hash.update([0]);
    hash.update(expected_state.to_string().as_bytes());
    hash.update([0]);
    hash.update(expected_revision.to_be_bytes());
    if let Some(player) = player {
        hash.update([1]);
        hash.update(player.to_string().as_bytes());
    } else {
        hash.update([0]);
    }
    hash.finalize().into()
}
pub(super) fn add_three_calendar_months(timestamp: i64) -> Result<i64, GameError> {
    if !(0..=JS_SAFE_INTEGER_MAX).contains(&timestamp) {
        return Err(GameError::Storage);
    }
    let days = timestamp.div_euclid(MILLISECONDS_PER_UTC_DAY);
    let time_of_day = timestamp.rem_euclid(MILLISECONDS_PER_UTC_DAY);
    let (year, month, day) = civil_from_days(days);
    let target_month_index = year
        .checked_mul(12)
        .and_then(|value| value.checked_add(i64::from(month - 1)))
        .and_then(|value| value.checked_add(HISTORY_RETENTION_CALENDAR_MONTHS))
        .ok_or(GameError::Storage)?;
    let target_year = target_month_index.div_euclid(12);
    let target_month = (target_month_index.rem_euclid(12) + 1) as u8;
    let target_day = day.min(days_in_month(target_year, target_month));
    let result = days_from_civil(target_year, target_month, target_day)
        .checked_mul(MILLISECONDS_PER_UTC_DAY)
        .and_then(|value| value.checked_add(time_of_day))
        .filter(|value| *value <= JS_SAFE_INTEGER_MAX)
        .ok_or(GameError::Storage)?;
    Ok(result)
}
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    }
    .div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u8;
    let month = (month_prime + if month_prime < 10 { 3 } else { -9 }) as u8;
    year += i64::from(month <= 2);
    (year, month, day)
}
fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        2 if year.rem_euclid(400) == 0
            || (year.rem_euclid(4) == 0 && year.rem_euclid(100) != 0) =>
        {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
struct TerminalResult {
    game_id: GameId,
    expected_state: GameState,
    state: GameState,
    ended_at: i64,
    history_available: bool,
    history_expires_at: Option<i64>,
    winner: Option<brews_contracts::games::WinnerView>,
    view_revision: u64,
}
fn terminal_outcome(result: &TerminalResult) -> GameOutcome {
    GameOutcome::Terminalized {
        expected_state: result.expected_state,
        state: result.state,
        ended_at: result.ended_at,
        history_available: result.history_available,
        history_expires_at: result.history_expires_at,
        winner: result.winner.clone(),
        view_revision: result.view_revision,
    }
}
fn with_terminal_receipt(result: TerminalResult, receipt: GameReceipt) -> GameResponse {
    GameResponse::Terminalized {
        game_id: result.game_id,
        expected_state: result.expected_state,
        state: result.state,
        ended_at: result.ended_at,
        history_available: result.history_available,
        history_expires_at: result.history_expires_at,
        winner: result.winner,
        view_revision: result.view_revision,
        receipt,
    }
}

#[cfg(test)]
mod tests {
    use super::{GameError, JS_SAFE_INTEGER_MAX, add_three_calendar_months};
    #[test]
    fn history_retention_preserves_utc_time_and_clamps_calendar_month_end() {
        for (ended, expected) in [
            (1_801_401_255_678, 1_809_090_855_678),
            (1_827_619_199_999, 1_835_481_599_999),
            (1_859_241_599_999, 1_867_017_599_999),
            (1_835_395_200_001, 1_843_171_200_001),
            (0, 7_776_000_000),
        ] {
            assert_eq!(add_three_calendar_months(ended), Ok(expected));
        }
        assert_eq!(
            add_three_calendar_months(JS_SAFE_INTEGER_MAX),
            Err(GameError::Storage)
        );
        assert_eq!(add_three_calendar_months(-1), Err(GameError::Storage));
    }
}
