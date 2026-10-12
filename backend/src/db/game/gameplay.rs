//! Start, ordered calls, and automatic board evaluation within the owner transaction.
use super::SQL_SELECT_GAME_BOARD_PLAYER_IDS;
use super::{
    CELL_FREE, CELL_VALUE, Database, GameError, GameService, HOST_VIEW, JS_SAFE_INTEGER_MAX,
    PLAYER_KIND, PendingWork, RETAINED_PLAYER_LIMIT, Record, Runtime, SqlValue, account_actor,
    add_time, advance, authorize_mutation, command_fingerprint, ensure_before_idle, int, make_id,
    number, revision, string, text,
};
use crate::auth::GameAccountAuthority;
use crate::db::Row;
use crate::directory::games::ReservationProof;
use crate::game::{WorkKind, WorkPhase};
use brews_contracts::games::{BoardView, CallView, GameOutcome, GameResponse};
use brews_domain::games::{
    BoardCell, BoardCellKind, CallMode, CellPosition, CompletedLine, GameState, SingleLinePattern,
};
use brews_domain::ids::{AccountId, CommandId, ConnectionId, PlayerId};
const SQL_ADVANCE_PENDING_START_PROJECTION: &str = "UPDATE game_pending_work SET fence_revision=? WHERE operation_id=? AND kind=? AND phase=? AND fence_revision=?";
const SQL_SELECT_GAME_CURRENT_CONNECTIONS_COUNT_DISTINCT_PRINCIPAL_ID: &str = "SELECT count(DISTINCT principal_id) FROM game_current_connections WHERE viewer_kind=? AND expires_at>?";
const SQL_SELECT_GAME_PLAYERS_PLAYER_ID: &str =
    "SELECT player_id FROM game_players ORDER BY player_id LIMIT 21";
const SQL_INSERT_GAME_BOARDS: &str = "INSERT INTO game_boards VALUES(?,?,?,0)";
const SQL_INSERT_GAME_BOARD_CELLS: &str = "INSERT INTO game_board_cells VALUES(?,?,?,?,?,?)";
const SQL_INSERT_GAME_COMPLETED_LINES: &str = "INSERT INTO game_completed_lines VALUES(?,?,?,?)";
const SQL_SELECT_GAME_CALLS_FOR_GAME: &str = "SELECT sequence_no,value,mode,called_by_account_id,command_id,called_at FROM game_calls WHERE game_id=? ORDER BY sequence_no LIMIT 1001";
const SQL_INSERT_GAME_CALL: &str = "INSERT INTO game_calls VALUES(?,?,?,?,?,?,?)";
const SQL_MARK_GAME_BOARD_CELLS_VALUE_MATCHED: &str =
    "UPDATE game_board_cells SET is_matched=1 WHERE kind=? AND value=? AND is_matched=0";
const SQL_UPDATE_GAME_BOARD_EVALUATED_THROUGH_CALL: &str =
    "UPDATE game_boards SET evaluated_through_call=? WHERE player_id=?";
const SQL_DELETE_GAME_COMPLETED_LINES_FOR_PLAYER: &str =
    "DELETE FROM game_completed_lines WHERE player_id=?";
const SQL_INSERT_GAME_CALL_REVISION: &str =
    "UPDATE game_record SET revision=? WHERE singleton=1 AND state=? AND revision=?";
const SQL_MARK_GAME_IN_PROGRESS: &str =
    "UPDATE game_record SET state=?,revision=?,started_at=?,idle_due=NULL WHERE singleton=1";
const SQL_SELECT_GAME_BOARDS_SIDE_LENGTH_EVALUATED_THROUGH_CALL: &str =
    "SELECT side_length,evaluated_through_call FROM game_boards WHERE player_id=?";
const SQL_SELECT_GAME_COMPLETED_LINES_KIND_LINE_INDEX: &str =
    "SELECT kind,line_index FROM game_completed_lines WHERE player_id=? ORDER BY sequence LIMIT 23";
const LINE_ROW: &str = "row";
const LINE_COLUMN: &str = "column";
const LINE_MAIN: &str = "main_diagonal";
const LINE_ANTI: &str = "anti_diagonal";
pub(super) const SQL_SELECT_GAME_BOARD_CELLS_ROW_COLUMN_KIND: &str = "SELECT row,column,kind,value,is_matched FROM game_board_cells WHERE player_id=? ORDER BY row,column LIMIT 101";

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub fn start(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        expected: u64,
        reservation: &ReservationProof,
        observed: &[ConnectionId],
    ) -> Result<GameResponse, GameError> {
        let expected = revision(expected)?;
        let fp = command_fingerprint(WorkKind::StartProjection.tag(), expected);
        self.tx(|now| {
            let r = self.record()?;
            authorize_mutation(&authority, &r, now)?;
            if reservation.game_id() != r.id {
                return Err(GameError::Conflict);
            }
            let actor = account_actor(authority.account_id());
            if let Some(w) = self.command_work(&actor, command)?
                && (w.kind != WorkKind::StartProjection || w.fingerprint != fp)
            {
                return Err(GameError::Conflict);
            }
            if let Some(receipt) = self.receipt(&actor, command, fp, now)? {
                return Ok(GameResponse::Committed { receipt });
            }
            self.admit(command, now)?;
            if r.state != GameState::AwaitingPlayers || r.published.is_none() {
                return Err(GameError::Conflict);
            }
            ensure_before_idle(&r, now)?;
            self.reconcile_presence(observed, &r, now)?;
            if self.view_revision(HOST_VIEW)? != expected {
                return Err(GameError::StaleRevision);
            }
            let count = self.db.query(
                SQL_SELECT_GAME_CURRENT_CONNECTIONS_COUNT_DISTINCT_PRINCIPAL_ID,
                &[text(PLAYER_KIND), int(now)],
            )?;
            if number(count.first().ok_or(GameError::Storage)?, 0)?
                < i64::from(crate::game::START_CONNECTED_PLAYER_MIN)
            {
                return Err(GameError::Conflict);
            }
            let rows = self.db.query(SQL_SELECT_GAME_PLAYERS_PLAYER_ID, &[])?;
            if rows.len() > RETAINED_PLAYER_LIMIT {
                return Err(GameError::Storage);
            }
            let players = rows
                .iter()
                .map(|row| string(row, 0)?.parse().map_err(|_| GameError::Storage))
                .collect::<Result<Vec<PlayerId>, GameError>>()?;
            let config = self.configuration()?;
            config
                .validate_start_feasibility(players.len())
                .map_err(|_| GameError::Infeasible)?;
            let boards = brews_domain::games::generate_boards(&config, &players, &mut |bytes| {
                self.runtime
                    .fill_random(bytes)
                    .map_err(|_| brews_domain::games::RandomSourceError)
            })
            .map_err(|error| match error {
                brews_domain::games::BoardGenerationError::RandomUnavailable => {
                    GameError::RandomUnavailable
                }
                brews_domain::games::BoardGenerationError::CandidateBudgetExhausted
                | brews_domain::games::BoardGenerationError::RejectionBudgetExhausted => {
                    GameError::GenerationExhausted
                }
                _ => GameError::Infeasible,
            })?;
            let operation = make_id(self.runtime, self.runtime.now_ms())?
                .try_into()
                .map_err(|_| GameError::Storage)?;
            let commit_now = self.observe_clock()?;
            if self.expire_due(commit_now)? {
                return Err(GameError::Expired);
            }
            ensure_before_idle(&r, commit_now)?;
            authorize_mutation(&authority, &r, commit_now)?;
            self.reconcile_presence(observed, &r, commit_now)?;
            if self.view_revision(HOST_VIEW)? != expected {
                return Err(GameError::StaleRevision);
            }
            let rev = advance(r.revision)?;
            let view = advance(expected)?;
            for board in &boards {
                self.db.execute(
                    SQL_INSERT_GAME_BOARDS,
                    &[
                        text(board.player_id),
                        int(i64::from(board.side_length)),
                        int(commit_now),
                    ],
                )?;
                for cell in &board.cells {
                    let (kind, value) = match &cell.kind {
                        BoardCellKind::Free => (CELL_FREE, SqlValue::Null),
                        BoardCellKind::Value(s) => (CELL_VALUE, text(s)),
                    };
                    self.db.execute(
                        SQL_INSERT_GAME_BOARD_CELLS,
                        &[
                            text(board.player_id),
                            int(i64::from(cell.position.row)),
                            int(i64::from(cell.position.column)),
                            text(kind),
                            value,
                            int(i64::from(cell.is_matched)),
                        ],
                    )?;
                }
                for (sequence, line) in board.qualifying_lines.iter().enumerate() {
                    let (kind, index) = line_parts(line);
                    self.db.execute(
                        SQL_INSERT_GAME_COMPLETED_LINES,
                        &[
                            text(board.player_id),
                            int(sequence as i64),
                            text(kind),
                            index.map_or(SqlValue::Null, int),
                        ],
                    )?;
                }
                let private = board.player_id.to_string();
                self.set_view_revision(&private, advance(self.view_revision(&private)?)?)?;
            }
            self.db.execute(
                SQL_MARK_GAME_IN_PROGRESS,
                &[text(GameState::InProgress), int(rev), int(commit_now)],
            )?;
            self.set_view_revision(HOST_VIEW, view)?;
            self.advance_audience_views()?;
            let receipt = self.store_receipt(
                &actor,
                command,
                fp,
                GameOutcome::Started {
                    view_revision: view as u64,
                    started_at: commit_now,
                },
                r.id,
                commit_now,
            )?;
            let w = PendingWork {
                operation,
                kind: WorkKind::StartProjection,
                phase: WorkPhase::AwaitingAcknowledgement,
                actor,
                command,
                fingerprint: fp,
                expected,
                fence: rev,
                created: commit_now,
                next: add_time(commit_now, crate::game::RETRY_INITIAL_MS)?,
                attempts: 0,
            };
            self.insert_work(&w)?;
            let actual = self.record()?;
            if actual.state != GameState::InProgress || actual.started != Some(commit_now) {
                return Err(GameError::Storage);
            }
            for board in boards {
                let stored = self
                    .board_view(board.player_id, &actual)?
                    .ok_or(GameError::Storage)?;
                if stored.cells != board.cells || stored.qualifying_lines != board.qualifying_lines
                {
                    return Err(GameError::Storage);
                }
            }
            Ok(GameResponse::Started {
                game_id: r.id,
                state: GameState::InProgress,
                started_at: commit_now,
                view_revision: view as u64,
                receipt,
            })
        })
    }
    pub fn call_random(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        expected_revision: u64,
    ) -> Result<GameResponse, GameError> {
        self.commit_call(
            authority,
            command,
            expected_revision,
            CallMode::Random,
            None,
        )
    }
    pub fn call_manual_value(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        value: &str,
        expected_revision: u64,
    ) -> Result<GameResponse, GameError> {
        if !valid_pool_value(value, brews_domain::games::NUMERIC_UPPER_BOUND_MAX) {
            return Err(GameError::InvalidInput);
        }
        self.commit_call(
            authority,
            command,
            expected_revision,
            CallMode::Manual,
            Some(value),
        )
    }
    fn commit_call(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        expected_revision: u64,
        mode: CallMode,
        requested_value: Option<&str>,
    ) -> Result<GameResponse, GameError> {
        let expected = revision(expected_revision)?;
        if matches!(
            (mode, requested_value),
            (CallMode::Random, Some(_)) | (CallMode::Manual, None)
        ) {
            return Err(GameError::InvalidInput);
        }
        let fingerprint = call_fingerprint(mode, expected, requested_value);
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
            let configuration = self.configuration()?;
            let calls = self.call_history(&record, configuration.numeric_upper_bound)?;
            if calls.len() >= configuration.numeric_upper_bound as usize {
                return Err(GameError::Conflict);
            }
            let value = match requested_value {
                Some(value) if valid_pool_value(value, configuration.numeric_upper_bound) => {
                    value.to_owned()
                }
                Some(_) => return Err(GameError::InvalidInput),
                None => {
                    choose_remaining_value(self.runtime, configuration.numeric_upper_bound, &calls)?
                }
            };
            // Entropy can cross an absolute credential deadline; use the final trusted cut.
            let now = self.observe_clock()?;
            authorize_mutation(&authority, &record, now)?;
            self.admit(command, now)?;
            if calls.iter().any(|call| call.view.value == value) {
                return Err(GameError::Conflict);
            }
            let sequence_no = u32::try_from(calls.len() + 1).map_err(|_| GameError::Storage)?;
            let next_game_revision = advance(record.revision)?;
            let next_host_revision = advance(expected)?;
            let player_rows = self.db.query(SQL_SELECT_GAME_PLAYERS_PLAYER_ID, &[])?;
            let board_rows = self.db.query(SQL_SELECT_GAME_BOARD_PLAYER_IDS, &[])?;
            if player_rows.len() < 2
                || player_rows.len() > RETAINED_PLAYER_LIMIT
                || player_rows.len() != board_rows.len()
            {
                return Err(GameError::Storage);
            }
            let players = player_rows
                .iter()
                .map(|row| string(row, 0)?.parse().map_err(|_| GameError::Storage))
                .collect::<Result<Vec<PlayerId>, GameError>>()?;
            let boards = board_rows
                .iter()
                .map(|row| string(row, 0)?.parse().map_err(|_| GameError::Storage))
                .collect::<Result<Vec<PlayerId>, GameError>>()?;
            if players != boards {
                return Err(GameError::Storage);
            }
            let mut updated_boards = Vec::with_capacity(players.len());
            for player in &players {
                let current = self
                    .board_view_at(*player, &record, calls.len() as u32)?
                    .ok_or(GameError::Storage)?;
                let mut cells = current.cells;
                let (lines, _) = brews_domain::games::apply_called_value(
                    current.side_length,
                    &mut cells,
                    &value,
                )
                .map_err(|_| GameError::Storage)?;
                updated_boards.push((*player, current.side_length, cells, lines));
            }
            self.db.execute(
                SQL_INSERT_GAME_CALL,
                &[
                    text(record.id),
                    int(i64::from(sequence_no)),
                    text(&value),
                    text(mode),
                    text(authority.account_id()),
                    text(command),
                    int(now),
                ],
            )?;
            self.db.execute(
                SQL_MARK_GAME_BOARD_CELLS_VALUE_MATCHED,
                &[text(CELL_VALUE), text(&value)],
            )?;
            for (player, side_length, _cells, lines) in &updated_boards {
                self.db.execute(
                    SQL_UPDATE_GAME_BOARD_EVALUATED_THROUGH_CALL,
                    &[int(i64::from(sequence_no)), text(player)],
                )?;
                if self.db.query(
                    SQL_SELECT_GAME_BOARDS_SIDE_LENGTH_EVALUATED_THROUGH_CALL,
                    &[text(player)],
                )? != vec![vec![
                    int(i64::from(*side_length)),
                    int(i64::from(sequence_no)),
                ]] {
                    return Err(GameError::Storage);
                }
                self.db
                    .execute(SQL_DELETE_GAME_COMPLETED_LINES_FOR_PLAYER, &[text(player)])?;
                for (index, line) in lines.iter().enumerate() {
                    let (kind, line_index) = line_parts(line);
                    self.db.execute(
                        SQL_INSERT_GAME_COMPLETED_LINES,
                        &[
                            text(player),
                            int(i64::try_from(index).map_err(|_| GameError::Storage)?),
                            text(kind),
                            line_index.map_or(SqlValue::Null, int),
                        ],
                    )?;
                }
            }
            for (player, side_length, cells, lines) in &updated_boards {
                let stored = self
                    .board_view_at(*player, &record, sequence_no)?
                    .ok_or(GameError::Storage)?;
                if stored.side_length != *side_length
                    || stored.cells != *cells
                    || stored.qualifying_lines != *lines
                    || stored.qualified != !lines.is_empty()
                {
                    return Err(GameError::Storage);
                }
            }
            self.db.execute(
                SQL_INSERT_GAME_CALL_REVISION,
                &[
                    int(next_game_revision),
                    text(GameState::InProgress),
                    int(record.revision),
                ],
            )?;
            if self.record()?.revision != next_game_revision {
                return Err(GameError::Storage);
            }
            self.advance_pending_start_projection(record.revision, next_game_revision)?;
            self.set_view_revision(HOST_VIEW, next_host_revision)?;
            self.advance_audience_views()?;
            for player in &players {
                let key = player.to_string();
                let next = advance(self.view_revision(&key)?)?;
                self.set_view_revision(&key, next)?;
            }
            let stored_calls = self.call_history(&record, configuration.numeric_upper_bound)?;
            let stored_call = stored_calls.last().ok_or(GameError::Storage)?;
            if stored_calls.len() != calls.len() + 1
                || stored_call.view.sequence_no != sequence_no
                || stored_call.view.value != value
                || stored_call.mode != mode
                || stored_call.called_by != authority.account_id()
                || stored_call.command != command
                || stored_call.called_at != now
            {
                return Err(GameError::Storage);
            }
            let remaining_count = configuration.numeric_upper_bound - sequence_no;
            let exhausted = remaining_count == 0;
            let view_revision = next_host_revision as u64;
            let receipt = self.store_receipt(
                &actor,
                command,
                fingerprint,
                GameOutcome::CallAccepted {
                    call: stored_call.view.clone(),
                    remaining_count,
                    exhausted,
                    view_revision,
                },
                record.id,
                now,
            )?;
            Ok(GameResponse::CallAccepted {
                call: stored_call.view.clone(),
                remaining_count,
                exhausted,
                view_revision,
                receipt,
            })
        })
    }
    fn advance_pending_start_projection(&self, previous: i64, next: i64) -> Result<(), GameError> {
        let work = self.all_work(2)?;
        let mut current = match work.as_slice() {
            [] => return Ok(()),
            [work]
                if work.kind == WorkKind::StartProjection
                    && work.phase == WorkPhase::AwaitingAcknowledgement
                    && work.fence == previous =>
            {
                work.clone()
            }
            _ => return Err(GameError::Storage),
        };
        // Retain the original Start intent/receipt; only its current publication fence advances.
        self.db.execute(
            SQL_ADVANCE_PENDING_START_PROJECTION,
            &[
                int(next),
                text(current.operation),
                text(current.kind.tag()),
                text(current.phase.tag()),
                int(previous),
            ],
        )?;
        current.fence = next;
        if self.operation_work(current.operation)?.as_ref() != Some(&current) {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    pub(super) fn call_history(
        &self,
        record: &Record,
        upper_bound: u32,
    ) -> Result<Vec<StoredGameCall>, GameError> {
        let rows = self
            .db
            .query(SQL_SELECT_GAME_CALLS_FOR_GAME, &[text(record.id)])?;
        if rows.len() > upper_bound as usize || rows.len() > 1_000 {
            return Err(GameError::Storage);
        }
        let Some(started_at) = record.started else {
            return if rows.is_empty() {
                Ok(Vec::new())
            } else {
                Err(GameError::Storage)
            };
        };
        let mut seen = vec![false; upper_bound as usize + 1];
        let mut calls = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let sequence_no = u32::try_from(index + 1).map_err(|_| GameError::Storage)?;
            let stored = parse_game_call(row, sequence_no, upper_bound, started_at)?;
            let value = stored
                .view
                .value
                .parse::<u32>()
                .map_err(|_| GameError::Storage)? as usize;
            if seen[value] {
                return Err(GameError::Storage);
            }
            seen[value] = true;
            calls.push(stored);
        }
        Ok(calls)
    }
    pub(super) fn board_view(
        &self,
        player: PlayerId,
        r: &Record,
    ) -> Result<Option<BoardView>, GameError> {
        if r.state != GameState::InProgress {
            return Ok(None);
        }
        let configuration = self.configuration()?;
        let calls = self.call_history(r, configuration.numeric_upper_bound)?;
        self.board_view_at(player, r, calls.len() as u32)
    }
    pub(super) fn board_view_at(
        &self,
        player: PlayerId,
        r: &Record,
        evaluated_through_call: u32,
    ) -> Result<Option<BoardView>, GameError> {
        if r.state != GameState::InProgress {
            return Ok(None);
        }
        let rows = self.db.query(
            SQL_SELECT_GAME_BOARDS_SIDE_LENGTH_EVALUATED_THROUGH_CALL,
            &[text(player)],
        )?;
        let row = rows.first().ok_or(GameError::Storage)?;
        let side = u8::try_from(number(row, 0)?).map_err(|_| GameError::Storage)?;
        if number(row, 1)? != i64::from(evaluated_through_call) {
            return Err(GameError::Storage);
        }
        let cells = self
            .db
            .query(SQL_SELECT_GAME_BOARD_CELLS_ROW_COLUMN_KIND, &[text(player)])?
            .iter()
            .map(|row| {
                let kind = match string(row, 2)? {
                    CELL_FREE if row.get(3) == Some(&SqlValue::Null) => BoardCellKind::Free,
                    CELL_VALUE => BoardCellKind::Value(string(row, 3)?.to_owned()),
                    _ => return Err(GameError::Storage),
                };
                Ok(BoardCell {
                    position: CellPosition {
                        row: u8::try_from(number(row, 0)?).map_err(|_| GameError::Storage)?,
                        column: u8::try_from(number(row, 1)?).map_err(|_| GameError::Storage)?,
                    },
                    kind,
                    is_matched: number(row, 4)? == 1,
                })
            })
            .collect::<Result<Vec<_>, GameError>>()?;
        let lines = self
            .db
            .query(
                SQL_SELECT_GAME_COMPLETED_LINES_KIND_LINE_INDEX,
                &[text(player)],
            )?
            .iter()
            .map(parse_line)
            .collect::<Result<Vec<_>, GameError>>()?;
        let evaluated = brews_domain::games::SingleLine::evaluate(side, &cells)
            .map_err(|_| GameError::Storage)?;
        if evaluated != lines {
            return Err(GameError::Storage);
        }
        Ok(Some(BoardView {
            side_length: side,
            cells,
            qualified: !lines.is_empty(),
            qualifying_lines: lines,
        }))
    }
}

pub(super) struct StoredGameCall {
    pub(super) view: CallView,
    mode: CallMode,
    called_by: AccountId,
    command: CommandId,
    called_at: i64,
}
fn parse_game_call(
    row: &Row,
    expected_sequence: u32,
    upper_bound: u32,
    started_at: i64,
) -> Result<StoredGameCall, GameError> {
    let sequence_no = u32::try_from(number(row, 0)?).map_err(|_| GameError::Storage)?;
    let value = string(row, 1)?.to_owned();
    let called_at = number(row, 5)?;
    if sequence_no != expected_sequence
        || !valid_pool_value(&value, upper_bound)
        || called_at < started_at
        || called_at > JS_SAFE_INTEGER_MAX
    {
        return Err(GameError::Storage);
    }
    Ok(StoredGameCall {
        view: CallView { sequence_no, value },
        mode: string(row, 2)?.parse().map_err(|_| GameError::Storage)?,
        called_by: string(row, 3)?.parse().map_err(|_| GameError::Storage)?,
        command: string(row, 4)?.parse().map_err(|_| GameError::Storage)?,
        called_at,
    })
}
fn valid_pool_value(value: &str, upper_bound: u32) -> bool {
    !value.is_empty()
        && value.len() <= 4
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.as_bytes()[0] != b'0'
        && value.parse::<u32>().ok().is_some_and(|number| {
            (1..=upper_bound).contains(&number) && number.to_string() == value
        })
}
fn choose_remaining_value(
    runtime: &impl Runtime,
    upper_bound: u32,
    calls: &[StoredGameCall],
) -> Result<String, GameError> {
    let mut called = vec![false; upper_bound as usize + 1];
    for call in calls {
        let value = call
            .view
            .value
            .parse::<u32>()
            .map_err(|_| GameError::Storage)?;
        let slot = called.get_mut(value as usize).ok_or(GameError::Storage)?;
        if *slot {
            return Err(GameError::Storage);
        }
        *slot = true;
    }
    let remaining = upper_bound
        .checked_sub(u32::try_from(calls.len()).map_err(|_| GameError::Storage)?)
        .filter(|count| *count > 0)
        .ok_or(GameError::Conflict)?;
    let target = bounded_random_index(remaining, runtime)?;
    let mut offset = 0;
    for value in 1..=upper_bound {
        if !called[value as usize] {
            if offset == target {
                return Ok(value.to_string());
            }
            offset += 1;
        }
    }
    Err(GameError::Storage)
}
fn bounded_random_index(bound: u32, runtime: &impl Runtime) -> Result<u32, GameError> {
    if bound == 0 {
        return Err(GameError::Conflict);
    }
    let threshold = bound.wrapping_neg() % bound;
    for _ in 0..brews_domain::games::RANDOM_INDEX_REJECTION_BUDGET {
        let mut bytes = [0; 4];
        runtime
            .fill_random(&mut bytes)
            .map_err(|_| GameError::RandomUnavailable)?;
        let value = u32::from_le_bytes(bytes);
        if value >= threshold {
            return Ok(value % bound);
        }
    }
    Err(GameError::GenerationExhausted)
}
fn call_fingerprint(mode: CallMode, expected: i64, value: Option<&str>) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"brews-game-call-v1\\0");
    hash.update(mode.to_string().as_bytes());
    hash.update([0]);
    hash.update(expected.to_be_bytes());
    if let Some(value) = value {
        hash.update((value.len() as u32).to_be_bytes());
        hash.update(value.as_bytes());
    } else {
        hash.update(0u32.to_be_bytes());
    }
    hash.finalize().into()
}
fn line_parts(line: &CompletedLine) -> (&'static str, Option<i64>) {
    match line {
        CompletedLine::Row(i) => (LINE_ROW, Some(i64::from(*i))),
        CompletedLine::Column(i) => (LINE_COLUMN, Some(i64::from(*i))),
        CompletedLine::MainDiagonal => (LINE_MAIN, None),
        CompletedLine::AntiDiagonal => (LINE_ANTI, None),
    }
}
fn parse_line(row: &Row) -> Result<CompletedLine, GameError> {
    let index = || u8::try_from(number(row, 1)?).map_err(|_| GameError::Storage);
    match string(row, 0)? {
        LINE_ROW => Ok(CompletedLine::Row(index()?)),
        LINE_COLUMN => Ok(CompletedLine::Column(index()?)),
        LINE_MAIN if row.get(1) == Some(&SqlValue::Null) => Ok(CompletedLine::MainDiagonal),
        LINE_ANTI if row.get(1) == Some(&SqlValue::Null) => Ok(CompletedLine::AntiDiagonal),
        _ => Err(GameError::Storage),
    }
}
