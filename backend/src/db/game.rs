//! Game-owner SQLite service; fixed statements and bounded owner-local work.
mod history;
mod terminal;
use super::{Database, SqlValue, StorageError};
use crate::auth::{AuthPolicy, GameAccountAuthority, Runtime};
use crate::directory::games::{CodeGrant, GameProjection, ProjectionAck};
use crate::directory::games::{
    CreationFingerprint, CreationReadyProof, CreationWork, SourceRevision,
};
use crate::directory::games::{ReleaseAck, ReservationProof, TerminalProof};
use crate::game::CreationResult;
use crate::game::{ACCOUNT_KIND, AccountConnectionGrant, ConnectionGrant, PLAYER_KIND};
use crate::game::{
    ADMISSION_LIMIT, ADMISSION_MS, HOST_IDLE_MS, HOST_VIEW, LIVE_ACCESS, LobbyPreparation,
    PLAYER_SESSION_MS, ParticipantOutcome, PendingWork, RECEIPT_MS, SecretCookie, WorkKind,
    WorkPhase,
};
use crate::limits::JS_SAFE_INTEGER_MAX;
use crate::{
    db::schema::game_schema::{
        self, DELIVERY_TABLES, GAME_SCHEMA_VERSION, GAME_TABLES, PREVIOUS_GAME_SCHEMA_VERSION,
        PREVIOUS_GAME_TABLES,
    },
    game::GameError,
};
use brews_contracts::games::{BoardView, CallView, GameView, HostPlayerView};
use brews_contracts::games::{GameOutcome, GameReceipt, GameResponse, GameSummary};
use brews_domain::games::GameConfiguration;
use brews_domain::games::{BoardCell, BoardCellKind, CallMode, CompletedLine, SingleLinePattern};
use brews_domain::ids::{ConnectionId, PlayerId, SessionId};
use brews_domain::{
    accounts::AccountRole,
    games::{CellPosition, GameCode, GameState},
    ids::{AccountId, CommandId, GameId},
};

// Fixed owner-local statements, shared only inside this Game SQL service.
const SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT: &str =
    "SELECT answer_verifier,normalization_version,updated_at FROM game_recovery WHERE player_id=?";
const SQL_SELECT_GAME_CURRENT_CONNECTIONS_EXPIRED_ACCOUNT: &str = "SELECT session_id,connection_id FROM game_current_connections WHERE viewer_kind=? AND expires_at<=? ORDER BY expires_at LIMIT ?";
const SQL_DELETE_GAME_COMPLETED_LINES: &str = "DELETE FROM game_completed_lines";
const SQL_SELECT_ANY_COMPLETED_LINE: &str = "SELECT 1 FROM game_completed_lines LIMIT 1";
const SQL_DELETE_GAME_BOARD_CELLS: &str = "DELETE FROM game_board_cells";
const SQL_SELECT_ANY_BOARD_CELL: &str = "SELECT 1 FROM game_board_cells LIMIT 1";
const SQL_DELETE_GAME_BOARDS: &str = "DELETE FROM game_boards";
const SQL_SELECT_ANY_BOARD: &str = "SELECT 1 FROM game_boards LIMIT 1";
const SQL_DELETE_GAME_RECOVERY: &str = "DELETE FROM game_recovery";
const SQL_SELECT_ANY_PLAYER_RECOVERY: &str = "SELECT 1 FROM game_recovery LIMIT 1";
const SQL_DELETE_GAME_SESSIONS: &str = "DELETE FROM game_sessions";
const SQL_SELECT_ANY_PLAYER_SESSION: &str = "SELECT 1 FROM game_sessions LIMIT 1";
const SQL_DELETE_GAME_CURRENT_CONNECTIONS: &str = "DELETE FROM game_current_connections";
const SQL_SELECT_ANY_CURRENT_CONNECTION: &str = "SELECT 1 FROM game_current_connections LIMIT 1";
const SQL_DELETE_GAME_CONNECTION_GRANTS: &str = "DELETE FROM game_connection_grants";
const SQL_SELECT_ANY_CONNECTION_GRANT: &str = "SELECT 1 FROM game_connection_grants LIMIT 1";
const SQL_DELETE_GAME_ADMISSION_CONTEXTS: &str = "DELETE FROM game_admission_contexts";
const SQL_SELECT_ANY_ADMISSION_CONTEXT: &str = "SELECT 1 FROM game_admission_contexts LIMIT 1";
const SQL_DELETE_GAME_PLAYERS: &str = "DELETE FROM game_players";
const SQL_SELECT_ANY_PLAYER: &str = "SELECT 1 FROM game_players LIMIT 1";
const SQL_DELETE_GAME_FREE_CELLS: &str = "DELETE FROM game_free_cells";
const SQL_SELECT_ANY_FREE_CELL: &str = "SELECT 1 FROM game_free_cells LIMIT 1";
const SQL_DELETE_GAME_CONFIGURATION: &str = "DELETE FROM game_configuration";
const SQL_SELECT_ANY_GAME_CONFIGURATION: &str = "SELECT 1 FROM game_configuration LIMIT 1";

const SQL_DELETE_GAME_RECEIPTS: &str = "DELETE FROM game_receipts";
const SQL_SELECT_ANY_GAME_RECEIPT: &str = "SELECT 1 FROM game_receipts LIMIT 1";
const SQL_DELETE_GAME_PENDING_WORK: &str = "DELETE FROM game_pending_work";
const SQL_SELECT_ANY_PENDING_GAME_WORK: &str = "SELECT 1 FROM game_pending_work LIMIT 1";
const SQL_INSERT_GAME_RECORD: &str = "INSERT INTO game_record(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due) VALUES(1,?,?,?,?,?,0,?,0,?,?,?)";
const SQL_INSERT_GAME_CONFIGURATION: &str = "INSERT INTO game_configuration VALUES(1,?,?,?,?,?,?)";
const SQL_INSERT_GAME_FREE_CELLS: &str = "INSERT INTO game_free_cells VALUES(?,?)";
const SQL_INSERT_GAME_VIEW_REVISIONS: &str = "INSERT INTO game_view_revisions VALUES(?,0)";
const SQL_UPDATE_GAME_RECORD: &str = "UPDATE game_record SET state=?,game_code=?,lobby_opened_at=?,revision=?,idle_due=?,last_host_activity=CASE WHEN host_id=? THEN ? ELSE last_host_activity END WHERE singleton=1";
const SQL_UPDATE_GAME_PENDING_WORK: &str =
    "UPDATE game_pending_work SET phase=?,fence_revision=?,next_attempt_at=? WHERE operation_id=?";
const SQL_ADVANCE_PENDING_START_PROJECTION: &str = "UPDATE game_pending_work SET fence_revision=? WHERE operation_id=? AND kind=? AND phase=? AND fence_revision=?";
const SQL_SET_GAME_PUBLICATION_TIME_ONCE: &str =
    "UPDATE game_record SET published_at=COALESCE(published_at,?) WHERE singleton=1";
const SQL_SELECT_GAME_VIEW_REVISIONS_REVISION: &str =
    "SELECT revision FROM game_view_revisions WHERE view_key=?";
const SQL_UPDATE_GAME_VIEW_REVISIONS: &str =
    "UPDATE game_view_revisions SET revision=? WHERE view_key=?";
const SQL_INSERT_GAME_PENDING_WORK: &str =
    "INSERT INTO game_pending_work VALUES(?,?,?,?,?,?,?,?,?,?,?)";
const SQL_SELECT_GAME_PENDING_WORK_OPERATION_ID_KIND_PHASE: &str = "SELECT operation_id,kind,phase,actor,command_id,fingerprint,expected_revision,fence_revision,created_at,next_attempt_at,attempt_count FROM game_pending_work ORDER BY next_attempt_at,operation_id LIMIT ?";
const SQL_SELECT_PENDING_WORK_BY_ACTOR_AND_COMMAND: &str = "SELECT operation_id,kind,phase,actor,command_id,fingerprint,expected_revision,fence_revision,created_at,next_attempt_at,attempt_count FROM game_pending_work WHERE actor=? AND command_id=?";
const SQL_SELECT_PENDING_WORK_BY_OPERATION_ID: &str = "SELECT operation_id,kind,phase,actor,command_id,fingerprint,expected_revision,fence_revision,created_at,next_attempt_at,attempt_count FROM game_pending_work WHERE operation_id=?";
const SQL_DELETE_PENDING_WORK_BY_OPERATION_ID: &str =
    "DELETE FROM game_pending_work WHERE operation_id=?";
const SQL_DELETE_EXPIRED_ADMISSION_CONTEXTS_BATCH: &str = "DELETE FROM game_admission_contexts WHERE player_id IN (SELECT player_id FROM game_admission_contexts WHERE retain_until<=? ORDER BY retain_until LIMIT ?)";
const SQL_SELECT_GAME_ADMISSION_CONTEXTS_COUNT: &str =
    "SELECT count(*) FROM game_admission_contexts";
const SQL_INSERT_GAME_ADMISSION_CONTEXTS: &str =
    "INSERT INTO game_admission_contexts VALUES(?,?,?,?,?,?,NULL)";
const SQL_INSERT_GAME_PLAYERS: &str = "INSERT INTO game_players VALUES(?,?,?,0,NULL)";
const SQL_INSERT_GAME_SESSIONS: &str = "INSERT INTO game_sessions VALUES(?,?,?,?,?,0,?,?,?,NULL)";
const SQL_INSERT_GAME_RECOVERY: &str = "INSERT INTO game_recovery VALUES(?,?,?,?)";
const SQL_UPDATE_GAME_ADMISSION_CONTEXTS: &str =
    "UPDATE game_admission_contexts SET consumed_command=?,retain_until=? WHERE player_id=?";
const SQL_SELECT_GAME_ADMISSION_CONTEXTS_PLAYER_ID_GAME_CODE_ISSUED_AT: &str = "SELECT player_id,game_code,issued_at,expires_at,retain_until,consumed_command FROM game_admission_contexts WHERE token_verifier=?";
const SQL_SELECT_PLAYER_BY_ALIAS: &str = "SELECT 1 FROM game_players WHERE alias=?";
const SQL_SELECT_GAME_PLAYERS_COUNT: &str = "SELECT count(*) FROM game_players";
const SQL_SELECT_GAME_SESSIONS_S_SESSION_ID_S_PLAYER_ID_S_SESSION_EPOCH: &str = "SELECT s.session_id,s.player_id,s.session_epoch,s.expires_at,s.game_id,s.game_code,s.issued_at FROM game_sessions s JOIN game_players p ON p.player_id=s.player_id AND p.session_epoch=s.session_epoch WHERE s.token_verifier=? AND s.revoked_at IS NULL AND s.access=?";
const SQL_DELETE_PREPARED_CONNECTION_GRANTS_FOR_SESSION: &str = "DELETE FROM game_connection_grants WHERE viewer_kind=? AND session_id=? AND connection_id IS NOT NULL";
const SQL_SELECT_PREPARED_CONNECTION_FOR_SESSION: &str = "SELECT 1 FROM game_connection_grants WHERE viewer_kind=? AND session_id=? AND connection_id IS NOT NULL LIMIT 1";
const SQL_INSERT_GAME_CONNECTION_GRANTS: &str =
    "INSERT INTO game_connection_grants VALUES(?,?,?,?,?,?,?,?)";
const SQL_INSERT_GAME_CURRENT_CONNECTIONS: &str = "INSERT INTO game_current_connections VALUES(?,?,?,?,?,?) ON CONFLICT(viewer_kind,session_id) DO UPDATE SET principal_id=excluded.principal_id,connection_id=excluded.connection_id,epoch=excluded.epoch,expires_at=excluded.expires_at";
const SQL_DELETE_CONNECTION_GRANT_BY_ID: &str =
    "DELETE FROM game_connection_grants WHERE grant_id=?";
const SQL_DELETE_CURRENT_CONNECTION_BY_SESSION: &str =
    "DELETE FROM game_current_connections WHERE viewer_kind=? AND session_id=? AND connection_id=?";
const SQL_SELECT_GAME_CONNECTION_GRANTS_SESSION_ID_PRINCIPAL_ID_CONNECTION_ID: &str = "SELECT session_id,principal_id,connection_id,epoch,expires_at FROM game_connection_grants WHERE grant_id=? AND viewer_kind=?";
const SQL_SELECT_GAME_CURRENT_CONNECTIONS_SESSION_ID_PRINCIPAL_ID_CONNECTION_ID: &str = "SELECT session_id,principal_id,connection_id,epoch,expires_at FROM game_current_connections WHERE viewer_kind=? AND session_id=?";
const SQL_SELECT_GAME_SESSIONS_S_PLAYER_ID_S_SESSION_EPOCH_S_EXPIRES_AT: &str = "SELECT s.player_id,s.session_epoch,s.expires_at,s.game_id,s.game_code FROM game_sessions s JOIN game_players p ON p.player_id=s.player_id AND p.session_epoch=s.session_epoch WHERE s.session_id=? AND s.revoked_at IS NULL AND s.access=?";
const SQL_SELECT_VIEWER_CONNECTIONS_FOR_RECONCILIATION: &str = "SELECT session_id,principal_id,connection_id,epoch,expires_at FROM game_current_connections WHERE viewer_kind=? ORDER BY session_id LIMIT 21";
const SQL_SELECT_GAME_CONNECTION_GRANTS_PRINCIPAL_ID_EPOCH_EXPIRES_AT: &str = "SELECT principal_id,epoch,expires_at FROM game_connection_grants WHERE grant_id=? AND viewer_kind=?";
const SQL_INSERT_ACCOUNT_CONNECTION_GRANT: &str =
    "INSERT INTO game_connection_grants VALUES(?,?,?,?,NULL,?,?,?)";
const SQL_SELECT_GAME_CONNECTION_GRANTS_EXPIRES_AT: &str =
    "SELECT expires_at FROM game_connection_grants WHERE grant_id=?";
const SQL_SELECT_GAME_PLAYERS_PLAYER_ID_ALIAS: &str =
    "SELECT player_id,alias FROM game_players ORDER BY player_id LIMIT 21";
const SQL_CHECK_CURRENT_CONNECTION_FOR_PRINCIPAL: &str = "SELECT 1 FROM game_current_connections WHERE viewer_kind=? AND principal_id=? AND expires_at>? LIMIT 1";
const SQL_SELECT_GAME_PLAYERS_ALIAS: &str = "SELECT alias FROM game_players WHERE player_id=?";
const SQL_SELECT_GAME_CURRENT_CONNECTIONS_COUNT_DISTINCT_PRINCIPAL_ID: &str = "SELECT count(DISTINCT principal_id) FROM game_current_connections WHERE viewer_kind=? AND expires_at>?";
const SQL_SELECT_GAME_PLAYERS_PLAYER_ID: &str =
    "SELECT player_id FROM game_players ORDER BY player_id LIMIT 21";
const SQL_INSERT_GAME_BOARDS: &str = "INSERT INTO game_boards VALUES(?,?,?,0)";
const SQL_INSERT_GAME_BOARD_CELLS: &str = "INSERT INTO game_board_cells VALUES(?,?,?,?,?,?)";
const SQL_INSERT_GAME_COMPLETED_LINES: &str = "INSERT INTO game_completed_lines VALUES(?,?,?,?)";
const SQL_SELECT_GAME_BOARD_PLAYER_IDS: &str =
    "SELECT player_id FROM game_boards ORDER BY player_id LIMIT 21";
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
const SQL_SELECT_GAME_BOARD_CELLS_ROW_COLUMN_KIND: &str = "SELECT row,column,kind,value,is_matched FROM game_board_cells WHERE player_id=? ORDER BY row,column LIMIT 101";
const SQL_SELECT_GAME_COMPLETED_LINES_KIND_LINE_INDEX: &str =
    "SELECT kind,line_index FROM game_completed_lines WHERE player_id=? ORDER BY sequence LIMIT 23";
const SQL_CANCEL_UNSTARTED_GAME: &str = "UPDATE game_record SET state=?,revision=?,idle_due=NULL,ended_at=?,cancellation_reason=?,terminal_actor_kind=?,terminal_actor_id=?,published_at=NULL,lobby_opened_at=NULL,last_host_activity=created_at WHERE singleton=1 AND state=? AND revision=?";
const SQL_DELETE_PLAYER_VIEW_REVISIONS: &str = "DELETE FROM game_view_revisions WHERE view_key<>?";
const SQL_SELECT_ANY_PLAYER_VIEW_REVISION: &str =
    "SELECT 1 FROM game_view_revisions WHERE view_key<>? LIMIT 1";
const SQL_SELECT_GAME_RECEIPTS_ACTOR_COMMAND_ID_FINGERPRINT: &str = "SELECT actor,command_id,fingerprint,outcome,completed_at,expires_at FROM game_receipts r WHERE expires_at<=? AND NOT EXISTS(SELECT 1 FROM game_pending_work p WHERE p.actor=r.actor AND p.command_id=r.command_id) ORDER BY expires_at LIMIT ?";
const SQL_DELETE_RECEIPT_BY_ACTOR_AND_COMMAND: &str =
    "DELETE FROM game_receipts WHERE actor=? AND command_id=?";
const SQL_CHECK_RECEIPT_EXISTS_FOR_ACTOR_AND_COMMAND: &str =
    "SELECT 1 FROM game_receipts WHERE actor=? AND command_id=?";
const SQL_SELECT_GAME_ADMISSION_CONTEXTS_PLAYER_ID: &str = "SELECT player_id FROM game_admission_contexts WHERE retain_until<=? ORDER BY retain_until LIMIT ?";
const SQL_DELETE_ADMISSION_CONTEXT_BY_PLAYER: &str =
    "DELETE FROM game_admission_contexts WHERE player_id=?";
const SQL_CHECK_ADMISSION_CONTEXT_EXISTS_FOR_PLAYER: &str =
    "SELECT 1 FROM game_admission_contexts WHERE player_id=?";
const SQL_SELECT_GAME_SESSIONS_SESSION_ID: &str = "SELECT session_id FROM game_sessions WHERE expires_at<=? OR revoked_at IS NOT NULL ORDER BY expires_at LIMIT ?";
const SQL_DELETE_SESSION_BY_ID: &str = "DELETE FROM game_sessions WHERE session_id=?";
const SQL_CHECK_SESSION_EXISTS_BY_ID: &str = "SELECT 1 FROM game_sessions WHERE session_id=?";
const SQL_SELECT_GAME_CONNECTION_GRANTS_GRANT_ID: &str =
    "SELECT grant_id FROM game_connection_grants WHERE expires_at<=? ORDER BY expires_at LIMIT ?";
const SQL_CHECK_CONNECTION_GRANT_EXISTS_BY_ID: &str =
    "SELECT 1 FROM game_connection_grants WHERE grant_id=?";
const GAME_DEADLINE_SOURCE_COUNT: usize = 9;
// Scalar aggregates avoid Workerd's bounded compound-SELECT terms.
const SQL_SELECT_GAME_DEADLINES: &str = "SELECT (SELECT min(idle_due) FROM game_record), (SELECT min(next_attempt_at) FROM game_pending_work), (SELECT min(retain_until) FROM game_admission_contexts), (SELECT min(expires_at) FROM game_receipts r WHERE NOT EXISTS(SELECT 1 FROM game_pending_work p WHERE p.actor=r.actor AND p.command_id=r.command_id)), (SELECT min(expires_at) FROM game_current_connections), (SELECT min(expires_at) FROM game_connection_grants), (SELECT min(expires_at) FROM game_sessions), (SELECT min(expires_at) FROM game_terminal_views), (SELECT min(history_expires_at) FROM game_record)";
const SQL_UPDATE_PENDING_WORK_RETRY: &str =
    "UPDATE game_pending_work SET attempt_count=?,next_attempt_at=? WHERE operation_id=?";
const SQL_INSERT_CANCELLED_GAME_RECORD: &str = "INSERT INTO game_record(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,ended_at,cancellation_reason,terminal_actor_kind,terminal_actor_id) VALUES(1,?,?,?,?,?,0,?,1,?,?,?,?,'system',NULL)";
const SQL_SELECT_ACCOUNT_PREPARED_GRANT_DETAILS: &str = "SELECT principal_id,epoch,expires_at FROM game_connection_grants WHERE grant_id=? AND viewer_kind=? AND connection_id IS NULL";
const SQL_DELETE_EXACT_ACCOUNT_CONNECTION_GRANT: &str = "DELETE FROM game_connection_grants WHERE grant_id=? AND viewer_kind=? AND session_id=? AND principal_id=? AND epoch=? AND expires_at=?";
const SQL_SELECT_EXACT_ACCOUNT_CLOSE_PREPARATION: &str = "SELECT 1 FROM game_connection_grants WHERE grant_id=? AND viewer_kind=? AND session_id=? AND principal_id=? AND epoch=? AND expires_at=? LIMIT 1";
const SQL_SELECT_CURRENT_CONNECTION_BY_VIEWER_AND_CONNECTION_ID: &str = "SELECT session_id,principal_id,connection_id,epoch,expires_at FROM game_current_connections WHERE viewer_kind=? AND connection_id=?";
const SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS: &str =
    "SELECT last_observed_ms,command_floor_ms FROM game_metadata WHERE singleton=1";
const SQL_UPDATE_GAME_METADATA: &str =
    "UPDATE game_metadata SET last_observed_ms=?,command_floor_ms=? WHERE singleton=1";
const SQL_SELECT_GAME_RECORD_GAME_ID_CREATOR_ID_CREATION_COMMAND: &str = "SELECT game_id,creator_id,creation_command,fingerprint,host_id,state,revision,created_at,idle_due,game_code,published_at,started_at,ended_at,cancellation_reason,winner_player_id,winner_alias,winner_awarded_by_account_id,terminal_actor_kind,terminal_actor_id,history_expires_at FROM game_record WHERE singleton=1";
const SQL_SELECT_GAME_CONFIGURATION_NUMERIC_UPPER_BOUND_BOARD_SIDE_LENGTH_FREE_CELLS_ENABLED: &str = "SELECT numeric_upper_bound,board_side_length,free_cells_enabled,player_capacity,spectator_capacity,winning_pattern FROM game_configuration WHERE singleton=1";
const SQL_SELECT_GAME_FREE_CELLS_ROW_COLUMN: &str =
    "SELECT row,column FROM game_free_cells ORDER BY row,column LIMIT 101";
const SQL_SELECT_GAME_METADATA_COMMAND_FLOOR_MS: &str =
    "SELECT command_floor_ms FROM game_metadata WHERE singleton=1";
const SQL_SELECT_GAME_RECEIPTS_FINGERPRINT_OUTCOME_EXPIRES_AT: &str =
    "SELECT fingerprint,outcome,expires_at FROM game_receipts WHERE actor=? AND command_id=?";
const SQL_SELECT_REQUIRED_GAME_RECEIPT: &str = "SELECT actor,command_id,fingerprint,outcome,completed_at,expires_at FROM game_receipts WHERE actor=? AND command_id=?";
const SQL_INSERT_GAME_RECEIPTS: &str = "INSERT INTO game_receipts VALUES(?,?,?,?,?,?)";
const SQL_UPDATE_GAME_RECEIPT_OUTCOME: &str =
    "UPDATE game_receipts SET outcome=? WHERE actor=? AND command_id=? AND fingerprint=?";
const SQL_SELECT_SQLITE_MASTER_NAME: &str = "SELECT name FROM sqlite_master WHERE type='table' AND name NOT GLOB 'sqlite_*' AND name NOT IN (?,?) ORDER BY name LIMIT ?";
const SQL_SELECT_GAME_METADATA_SCHEMA_VERSION: &str =
    "SELECT schema_version FROM game_metadata WHERE singleton=1";
const SQL_INSERT_GAME_METADATA: &str = "INSERT INTO game_metadata VALUES(1,?,0,0)";
const SQL_SELECT_GAME_RECORD_FOR_SCHEMA_MIGRATION: &str = "SELECT singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due,lobby_opened_at,started_at,ended_at,game_code,published_at,cancellation_reason FROM game_record";
const SQL_COPY_GAME_RECORD_TO_SCHEMA_MIGRATION: &str = "INSERT INTO game_record_schema_upgrade(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due,lobby_opened_at,started_at,ended_at,game_code,published_at,cancellation_reason,winner_player_id,winner_alias,winner_awarded_by_account_id,terminal_actor_kind,terminal_actor_id,history_expires_at) SELECT singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due,lobby_opened_at,started_at,ended_at,game_code,published_at,cancellation_reason,NULL,NULL,NULL,CASE WHEN state=? AND started_at IS NULL AND cancellation_reason=? THEN ? ELSE NULL END,NULL,NULL FROM game_record";
const SQL_SELECT_GAME_RECORD_SCHEMA_UPGRADE: &str = "SELECT singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due,lobby_opened_at,started_at,ended_at,game_code,published_at,cancellation_reason,winner_player_id,winner_alias,winner_awarded_by_account_id,terminal_actor_kind,terminal_actor_id,history_expires_at FROM game_record_schema_upgrade";
const SQL_DELETE_GAME_CONFIGURATION_FOR_SCHEMA_MIGRATION: &str = "DELETE FROM game_configuration";
const SQL_SELECT_GAME_CONFIGURATION_AFTER_SCHEMA_MIGRATION: &str =
    "SELECT * FROM game_configuration WHERE singleton=1";
const SQL_DROP_PREVIOUS_GAME_RECORD_FOR_SCHEMA_MIGRATION: &str = "DROP TABLE game_record";
const SQL_RENAME_GAME_RECORD_AFTER_SCHEMA_MIGRATION: &str =
    "ALTER TABLE game_record_schema_upgrade RENAME TO game_record";
const SQL_RENAME_GAME_BOARD_CELLS_FOR_SCHEMA_MIGRATION: &str =
    "ALTER TABLE game_board_cells RENAME TO game_board_cells_previous";
const SQL_COPY_GAME_BOARD_CELLS_TO_SCHEMA_MIGRATION: &str =
    "INSERT INTO game_board_cells_schema_upgrade SELECT * FROM game_board_cells_previous";

const SQL_SELECT_GAME_BOARDS_FOR_SCHEMA_MIGRATION: &str = "SELECT player_id,side_length,assigned_at,evaluated_through_call FROM game_boards ORDER BY player_id";
const SQL_RENAME_GAME_BOARDS_FOR_SCHEMA_MIGRATION: &str =
    "ALTER TABLE game_boards RENAME TO game_boards_previous";
const SQL_COPY_GAME_BOARDS_TO_SCHEMA_MIGRATION: &str =
    "INSERT INTO game_boards SELECT * FROM game_boards_previous";
const SQL_SELECT_PREVIOUS_GAME_BOARDS: &str = "SELECT player_id,side_length,assigned_at,evaluated_through_call FROM game_boards_previous ORDER BY player_id";
const SQL_DROP_PREVIOUS_GAME_BOARDS_FOR_SCHEMA_MIGRATION: &str = "DROP TABLE game_boards_previous";
const SQL_DROP_PREVIOUS_GAME_BOARD_CELLS_FOR_SCHEMA_MIGRATION: &str =
    "DROP TABLE game_board_cells_previous";
const SQL_RENAME_GAME_BOARD_CELLS_AFTER_SCHEMA_MIGRATION: &str =
    "ALTER TABLE game_board_cells_schema_upgrade RENAME TO game_board_cells";
const SQL_UPDATE_GAME_SCHEMA_VERSION: &str =
    "UPDATE game_metadata SET schema_version=? WHERE singleton=1 AND schema_version=?";
const SQL_INSERT_HISTORY_CALLS: &str =
    "INSERT INTO game_history_calls(game_id,sequence_no,value) VALUES(?,?,?)";
const SQL_INSERT_HISTORY_PLAYERS: &str =
    "INSERT INTO game_history_players(game_id,player_id,alias,side_length) VALUES(?,?,?,?)";
const SQL_INSERT_HISTORY_BOARD_CELLS: &str = "INSERT INTO game_history_board_cells(game_id,player_id,row,column,kind,value,is_matched) VALUES(?,?,?,?,?,?,?)";
const SQL_INSERT_GAME_HISTORY: &str = "INSERT INTO game_history(game_id,game_code,designated_host_id,started_at,ended_at,expires_at,outcome,winner_player_id,winner_alias) VALUES(?,?,?,?,?,?,?,?,?)";
const SQL_UPDATE_GAME_RECORD_TERMINAL: &str = "UPDATE game_record SET state=?,revision=?,ended_at=?,cancellation_reason=?,winner_player_id=?,winner_alias=?,winner_awarded_by_account_id=?,terminal_actor_kind=?,terminal_actor_id=?,history_expires_at=?,idle_due=NULL WHERE singleton=1 AND state=? AND revision=?";
const SQL_SELECT_GAME_RECORD_TERMINAL_FIELDS: &str = "SELECT state,revision,ended_at,cancellation_reason,winner_player_id,winner_alias,winner_awarded_by_account_id,terminal_actor_kind,terminal_actor_id,history_expires_at FROM game_record WHERE singleton=1";
const SQL_SELECT_GAME_HISTORY_PARENT: &str = "SELECT game_code,designated_host_id,started_at,ended_at,expires_at,outcome,winner_player_id,winner_alias FROM game_history WHERE game_id=?";
const SQL_SELECT_GAME_HISTORY_CALLS: &str =
    "SELECT sequence_no,value FROM game_history_calls WHERE game_id=? ORDER BY sequence_no";
const SQL_SELECT_GAME_HISTORY_PLAYERS: &str = "SELECT player_id,alias,side_length FROM game_history_players WHERE game_id=? ORDER BY player_id";
const SQL_SELECT_GAME_HISTORY_CELLS: &str = "SELECT player_id,row,column,kind,value,is_matched FROM game_history_board_cells WHERE game_id=? ORDER BY player_id,row,column";
const SQL_DELETE_GAME_CALLS: &str = "DELETE FROM game_calls";
const SQL_SELECT_ANY_GAME_CALL: &str = "SELECT 1 FROM game_calls LIMIT 1";

const SQL_DELETE_GAME_CONNECTIONS: &str = "DELETE FROM game_current_connections";
const SQL_SELECT_ANY_GAME_CONNECTION: &str = "SELECT 1 FROM game_current_connections LIMIT 1";
const SQL_DELETE_GAME_CONNECTION_GRANTS_ALL: &str = "DELETE FROM game_connection_grants";
const SQL_SELECT_ANY_GAME_CONNECTION_GRANT: &str = "SELECT 1 FROM game_connection_grants LIMIT 1";
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

const SYSTEM_ACTOR: &str = "system";
const OBSERVED_CONNECTION_LIMIT: usize = 100;
const RETAINED_PLAYER_LIMIT: usize = 20;
const PENDING_WORK_BATCH_LIMIT: u32 = 100;
const PRESTART_PURGE: [(&str, &str); 14] = [
    (
        SQL_DELETE_GAME_COMPLETED_LINES,
        SQL_SELECT_ANY_COMPLETED_LINE,
    ),
    (SQL_DELETE_GAME_BOARD_CELLS, SQL_SELECT_ANY_BOARD_CELL),
    (SQL_DELETE_GAME_BOARDS, SQL_SELECT_ANY_BOARD),
    (SQL_DELETE_GAME_RECOVERY, SQL_SELECT_ANY_PLAYER_RECOVERY),
    (SQL_DELETE_GAME_SESSIONS, SQL_SELECT_ANY_PLAYER_SESSION),
    (
        SQL_DELETE_GAME_CURRENT_CONNECTIONS,
        SQL_SELECT_ANY_CURRENT_CONNECTION,
    ),
    (
        SQL_DELETE_GAME_CONNECTION_GRANTS,
        SQL_SELECT_ANY_CONNECTION_GRANT,
    ),
    (
        SQL_DELETE_GAME_ADMISSION_CONTEXTS,
        SQL_SELECT_ANY_ADMISSION_CONTEXT,
    ),
    (SQL_DELETE_GAME_PLAYERS, SQL_SELECT_ANY_PLAYER),
    (SQL_DELETE_GAME_FREE_CELLS, SQL_SELECT_ANY_FREE_CELL),
    (
        SQL_DELETE_GAME_CONFIGURATION,
        SQL_SELECT_ANY_GAME_CONFIGURATION,
    ),
    (SQL_DELETE_GAME_CALLS, SQL_SELECT_ANY_GAME_CALL),
    (SQL_DELETE_GAME_RECEIPTS, SQL_SELECT_ANY_GAME_RECEIPT),
    (
        SQL_DELETE_GAME_PENDING_WORK,
        SQL_SELECT_ANY_PENDING_GAME_WORK,
    ),
];
const TERMINAL_LIVE_PURGE: [(&str, &str); 11] = [
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
const CELL_FREE: &str = brews_domain::games::BOARD_CELL_KIND_FREE;
const CELL_VALUE: &str = brews_domain::games::BOARD_CELL_KIND_VALUE;
const LINE_ROW: &str = "row";
const LINE_COLUMN: &str = "column";
const LINE_MAIN: &str = "main_diagonal";
const LINE_ANTI: &str = "anti_diagonal";
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

pub struct GameService<'a, D: Database, R: Runtime> {
    db: &'a D,
    runtime: &'a R,
    policy: AuthPolicy,
    rate_key: &'a [u8],
}
impl<'a, D: Database, R: Runtime> GameService<'a, D, R> {
    pub fn new(
        db: &'a D,
        runtime: &'a R,
        policy: AuthPolicy,
        rate_key: &'a [u8],
    ) -> Result<Self, GameError> {
        validate_schema(db)?;
        crate::security::validate_policy(policy).map_err(|_| GameError::InvalidInput)?;
        if !(32..=64).contains(&rate_key.len()) {
            return Err(GameError::InvalidInput);
        }
        Ok(Self {
            db,
            runtime,
            policy,
            rate_key,
        })
    }
    pub fn initialize(
        &self,
        work: &CreationWork,
        authority: GameAccountAuthority,
        config: &GameConfiguration,
    ) -> Result<CreationResult, GameError> {
        let fp = crate::game::creation_fingerprint(config)?;
        self.tx(|now| {
            authorize(&authority, now)?;
            if authority.account_id() != work.account_id() {
                return Err(GameError::Forbidden);
            }
            if fp != work.fingerprint() {
                return Err(GameError::Conflict);
            }
            let existing = self.record_optional()?;
            let ready = CreationReadyProof::new(
                work.game_id(),
                work.account_id(),
                work.command_id(),
                fp,
                work.created_at(),
                SourceRevision::INITIAL,
            );
            if let Some(record) = existing {
                if record.id != work.game_id()
                    || record.creator != work.account_id()
                    || record.creation_command != work.command_id()
                    || record.fp != fp
                    || record.created != work.created_at()
                {
                    return Err(GameError::Conflict);
                }
                if record.state == GameState::Cancelled {
                    return Err(GameError::Expired);
                }
                let receipt = self
                    .receipt(
                        &account_actor(work.account_id()),
                        work.command_id(),
                        *fp.digest(),
                        now,
                    )?
                    .ok_or(GameError::StaleCommand)?;
                return Ok(CreationResult {
                    ready,
                    response: GameResponse::Committed { receipt },
                });
            }
            if now >= work.deadline() || work.created_at() > now {
                return Err(GameError::Expired);
            }
            self.admit(work.command_id(), now)?;
            let due = add_time(work.created_at(), HOST_IDLE_MS)?;
            self.db.execute(
                SQL_INSERT_GAME_RECORD,
                &[
                    text(work.game_id()),
                    text(work.account_id()),
                    text(work.command_id()),
                    SqlValue::Blob(fp.digest().to_vec()),
                    text(work.account_id()),
                    text(GameState::New),
                    int(work.created_at()),
                    int(work.created_at()),
                    int(due),
                ],
            )?;
            self.db.execute(
                SQL_INSERT_GAME_CONFIGURATION,
                &[
                    int(i64::from(config.numeric_upper_bound)),
                    int(i64::from(config.board_side_length)),
                    int(i64::from(config.free_cells_enabled)),
                    int(i64::from(config.player_capacity)),
                    int(i64::from(config.spectator_capacity)),
                    text(config.winning_pattern),
                ],
            )?;
            for p in &config.free_cell_positions {
                self.db.execute(
                    SQL_INSERT_GAME_FREE_CELLS,
                    &[int(i64::from(p.row)), int(i64::from(p.column))],
                )?;
            }
            self.db
                .execute(SQL_INSERT_GAME_VIEW_REVISIONS, &[text(HOST_VIEW)])?;
            if self.view_revision(HOST_VIEW)? != 0 {
                return Err(GameError::Storage);
            }
            let receipt = self.store_receipt(
                &account_actor(work.account_id()),
                work.command_id(),
                *fp.digest(),
                GameOutcome::Created {
                    view_revision: 0,
                    idle_cancel_due_at: due,
                },
                work.game_id(),
                now,
            )?;
            let record = self.record()?;
            if record.id != work.game_id()
                || crate::game::creation_fingerprint(&self.configuration()?)? != fp
            {
                return Err(GameError::Storage);
            }
            Ok(CreationResult {
                ready,
                response: GameResponse::Created {
                    game: record.summary(0),
                    configuration: self.configuration()?,
                    idle_cancel_due_at: due,
                    receipt,
                },
            })
        })
    }
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn creation_ready(
        &self,
        work: &CreationWork,
    ) -> Result<CreationReadyProof, GameError> {
        self.maintenance(|_| {
            let record = self.record_optional()?.ok_or(GameError::NotFound)?;
            if record.id != work.game_id()
                || record.creator != work.account_id()
                || record.creation_command != work.command_id()
                || record.fp != work.fingerprint()
                || record.created != work.created_at()
                || record.state != GameState::New
                || record.revision != 0
                || record.code.is_some()
                || record.started.is_some()
                || record.ended.is_some()
            {
                return Err(GameError::Conflict);
            }
            Ok(CreationReadyProof::new(
                record.id,
                record.creator,
                record.creation_command,
                record.fp,
                record.created,
                SourceRevision::INITIAL,
            ))
        })
    }
    pub fn prepare_lobby(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        expected: u64,
    ) -> Result<LobbyPreparation, GameError> {
        let expected = revision(expected)?;
        let fp = command_fingerprint(WorkKind::Lobby.tag(), expected);
        self.tx(|now| {
            let record = self.record()?;
            authorize_mutation(&authority, &record, now)?;
            let actor = account_actor(authority.account_id());
            if let Some(pending) = self.command_work(&actor, command)? {
                if pending.fingerprint != fp {
                    return Err(GameError::Conflict);
                }
                return Ok(LobbyPreparation::Ready(pending));
            }
            if let Some(receipt) = self.receipt(&actor, command, fp, now)? {
                return Ok(LobbyPreparation::Committed(receipt));
            }
            self.admit(command, now)?;
            if record.state != GameState::New {
                return Err(GameError::Conflict);
            }
            if self.view_revision(HOST_VIEW)? != expected {
                return Err(GameError::StaleRevision);
            }
            ensure_before_idle(&record, now)?;
            if !self.all_work(1)?.is_empty() {
                return Err(GameError::Conflict);
            }
            let work = PendingWork {
                operation: make_id(self.runtime, now)?
                    .try_into()
                    .map_err(|_| GameError::Storage)?,
                kind: WorkKind::Lobby,
                phase: WorkPhase::Prepared,
                actor,
                command,
                fingerprint: fp,
                expected,
                fence: record.revision,
                created: now,
                next: add_time(now, crate::game::RETRY_INITIAL_MS)?,
                attempts: 0,
            };
            self.insert_work(&work)?;
            Ok(LobbyPreparation::Ready(work))
        })
    }
    pub fn commit_lobby(
        &self,
        work: &PendingWork,
        authority: GameAccountAuthority,
        grant: &CodeGrant,
    ) -> Result<GameResponse, GameError> {
        self.tx(|now| {
            let record = self.record()?;
            authorize_mutation(&authority, &record, now)?;
            if account_actor(authority.account_id()) != work.actor {
                return Err(GameError::Forbidden);
            }
            let current = self
                .operation_work(work.operation)?
                .ok_or(GameError::Conflict)?;
            if !same_work_target(&current, work) || current.kind != WorkKind::Lobby {
                return Err(GameError::Conflict);
            }
            let proof = grant.creation().proof();
            if grant.game_id() != record.id
                || proof.account_id() != record.creator
                || proof.command_id() != record.creation_command
                || proof.fingerprint() != record.fp
                || proof.created_at() != record.created
                || proof.source_revision() != 0
            {
                return Err(GameError::Conflict);
            }
            ensure_before_idle(&record, now)?;
            if current.phase == WorkPhase::AwaitingAcknowledgement {
                return Ok(GameResponse::Pending {
                    operation_id: current.operation,
                });
            }
            if record.state != GameState::New
                || record.revision != current.fence
                || self.view_revision(HOST_VIEW)? != current.expected
            {
                return Err(GameError::StaleRevision);
            }
            self.configuration()?
                .validate()
                .map_err(|_| GameError::Storage)?;
            let rev = advance(record.revision)?;
            let view = advance(current.expected)?;
            let due = if authority.account_id() == record.host {
                add_time(now, HOST_IDLE_MS)?
            } else {
                record.due.ok_or(GameError::Storage)?
            };
            self.db.execute(
                SQL_UPDATE_GAME_RECORD,
                &[
                    text(GameState::AwaitingPlayers),
                    text(grant.game_code().as_str()),
                    int(now),
                    int(rev),
                    int(due),
                    text(authority.account_id()),
                    int(now),
                ],
            )?;
            self.set_view_revision(HOST_VIEW, view)?;
            self.store_receipt(
                &current.actor,
                current.command,
                current.fingerprint,
                GameOutcome::LobbyOpened {
                    view_revision: view as u64,
                },
                record.id,
                now,
            )?;
            self.db.execute(
                SQL_UPDATE_GAME_PENDING_WORK,
                &[
                    text(WorkPhase::AwaitingAcknowledgement.tag()),
                    int(rev),
                    int(add_time(now, crate::game::RETRY_INITIAL_MS)?),
                    text(current.operation),
                ],
            )?;
            let reloaded = self
                .operation_work(current.operation)?
                .ok_or(GameError::Storage)?;
            if reloaded.phase != WorkPhase::AwaitingAcknowledgement
                || reloaded.fence != rev
                || self.record()?.code.as_ref() != Some(grant.game_code())
            {
                return Err(GameError::Storage);
            }
            Ok(GameResponse::Pending {
                operation_id: current.operation,
            })
        })
    }
    pub fn directory_projection(&self) -> Result<GameProjection, GameError> {
        self.maintenance(|_| {
            if let Some(record) = self.record_optional()? {
                return record.projection();
            }
            self.terminal_routing_projection()?
                .ok_or(GameError::NotFound)
        })
    }
    pub fn pending_work(&self, limit: u32) -> Result<Vec<PendingWork>, GameError> {
        if limit == 0 || limit > PENDING_WORK_BATCH_LIMIT {
            return Err(GameError::InvalidInput);
        }
        self.maintenance(|_| self.all_work(limit))
    }
    /// Acknowledges the exact publication target and lifecycle fence, not retry timing/count.
    pub fn acknowledge_projection(
        &self,
        work: &PendingWork,
        ack: ProjectionAck,
    ) -> Result<GameResponse, GameError> {
        self.tx(|now| {
            let r = self.record()?;
            let current = self
                .operation_work(work.operation)?
                .ok_or(GameError::Conflict)?;
            if !same_work_target(&current, work)
                || current.phase != WorkPhase::AwaitingAcknowledgement
                || current.fence != work.fence
                || ack.game_id() != r.id
                || ack.source_revision() != current.fence
                || !ack.published()
                || r.revision != current.fence
                || !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress)
            {
                return Err(GameError::Conflict);
            }
            let mut receipt =
                match self.receipt(&current.actor, current.command, current.fingerprint, now) {
                    Ok(value) => value,
                    Err(GameError::StaleCommand) => None,
                    Err(error) => return Err(error),
                };
            if current.kind == WorkKind::Lobby && r.published.is_none() {
                self.db
                    .execute(SQL_SET_GAME_PUBLICATION_TIME_ONCE, &[int(now)])?;
                if self.record()?.published != Some(now) {
                    return Err(GameError::Storage);
                }
                let view = advance(self.view_revision(HOST_VIEW)?)?;
                self.set_view_revision(HOST_VIEW, view)?;
                if let Some(value) = &mut receipt {
                    if !matches!(value.outcome, GameOutcome::LobbyOpened { .. })
                        || value.game_id != r.id
                    {
                        return Err(GameError::Storage);
                    }
                    value.outcome = GameOutcome::LobbyOpened {
                        view_revision: view as u64,
                    };
                    // Publication changes the outcome revision, not receipt retention.
                    let raw =
                        String::from_utf8(value.encode_json().map_err(|_| GameError::Storage)?)
                            .map_err(|_| GameError::Storage)?;
                    self.db.execute(
                        SQL_UPDATE_GAME_RECEIPT_OUTCOME,
                        &[
                            text(&raw),
                            text(&current.actor),
                            text(current.command),
                            SqlValue::Blob(current.fingerprint.to_vec()),
                        ],
                    )?;
                    self.verify_required_receipt(
                        &current.actor,
                        current.command,
                        vec![
                            text(&current.actor),
                            text(current.command),
                            SqlValue::Blob(current.fingerprint.to_vec()),
                            text(raw),
                            int(value.completed_at),
                            int(value.expires_at),
                        ],
                    )?;
                }
            }
            self.delete_work(current.operation)?;
            let receipt = receipt.ok_or(GameError::StaleCommand)?;
            if current.kind == WorkKind::Lobby {
                Ok(GameResponse::LobbyOpened {
                    game_id: r.id,
                    state: r.state,
                    game_code: r.code.ok_or(GameError::Storage)?,
                    configuration: self.configuration()?,
                    view_revision: self.view_revision(HOST_VIEW)? as u64,
                    receipt,
                })
            } else {
                Ok(GameResponse::Committed { receipt })
            }
        })
    }
    fn view_revision(&self, key: &str) -> Result<i64, GameError> {
        let rows = self
            .db
            .query(SQL_SELECT_GAME_VIEW_REVISIONS_REVISION, &[text(key)])?;
        number(rows.first().ok_or(GameError::Storage)?, 0)
    }
    fn set_view_revision(&self, key: &str, value: i64) -> Result<(), GameError> {
        self.db
            .execute(SQL_UPDATE_GAME_VIEW_REVISIONS, &[int(value), text(key)])?;
        if self.view_revision(key)? != value {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    fn insert_work(&self, w: &PendingWork) -> Result<(), GameError> {
        self.db.execute(
            SQL_INSERT_GAME_PENDING_WORK,
            &[
                text(w.operation),
                text(w.kind.tag()),
                text(w.phase.tag()),
                text(&w.actor),
                text(w.command),
                SqlValue::Blob(w.fingerprint.to_vec()),
                int(w.expected),
                int(w.fence),
                int(w.created),
                int(w.next),
                int(i64::from(w.attempts)),
            ],
        )?;
        if self.operation_work(w.operation)?.as_ref() != Some(w) {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    fn all_work(&self, limit: u32) -> Result<Vec<PendingWork>, GameError> {
        self.db
            .query(
                SQL_SELECT_GAME_PENDING_WORK_OPERATION_ID_KIND_PHASE,
                &[int(i64::from(limit))],
            )?
            .iter()
            .map(parse_work)
            .collect()
    }
    fn command_work(
        &self,
        actor: &str,
        command: CommandId,
    ) -> Result<Option<PendingWork>, GameError> {
        self.db
            .query(
                SQL_SELECT_PENDING_WORK_BY_ACTOR_AND_COMMAND,
                &[text(actor), text(command)],
            )?
            .first()
            .map(parse_work)
            .transpose()
    }
    fn operation_work(
        &self,
        operation: brews_domain::ids::OperationId,
    ) -> Result<Option<PendingWork>, GameError> {
        self.db
            .query(SQL_SELECT_PENDING_WORK_BY_OPERATION_ID, &[text(operation)])?
            .first()
            .map(parse_work)
            .transpose()
    }
    fn delete_work(&self, operation: brews_domain::ids::OperationId) -> Result<(), GameError> {
        self.db
            .execute(SQL_DELETE_PENDING_WORK_BY_OPERATION_ID, &[text(operation)])?;
        if self.operation_work(operation)?.is_some() {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    pub fn admission_context(
        &self,
        code: &GameCode,
        context: Option<&str>,
        player: Option<&str>,
    ) -> Result<ParticipantOutcome, GameError> {
        self.tx(|now| {
            let r = self.record()?;
            require_admission(&r, code, now)?;
            if let Some(token) = player {
                match self.player_session(token, &r, now) {
                    Ok(session) => {
                        return Ok(ParticipantOutcome {
                            response: GameResponse::AdmissionContext {
                                expires_at: session.expires,
                            },
                            cookie: None,
                        });
                    }
                    Err(GameError::Unauthorized) => {}
                    Err(error) => return Err(error),
                }
            }
            if let Some(token) = context {
                match self.context(token, code, now) {
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
                    Err(error) => return Err(error),
                }
            }
            self.db.execute(
                SQL_DELETE_EXPIRED_ADMISSION_CONTEXTS_BATCH,
                &[int(now), int(ADMISSION_LIMIT)],
            )?;
            let rows = self
                .db
                .query(SQL_SELECT_GAME_ADMISSION_CONTEXTS_COUNT, &[])?;
            if number(rows.first().ok_or(GameError::Storage)?, 0)? >= ADMISSION_LIMIT {
                return Err(GameError::Capacity);
            }
            let token = zeroize::Zeroizing::new(
                crate::security::new_token(self.runtime)
                    .map_err(|_| GameError::RandomUnavailable)?,
            );
            let digest =
                crate::security::token_digest(&token).map_err(|_| GameError::RandomUnavailable)?;
            let player: PlayerId = make_id(self.runtime, now)?
                .try_into()
                .map_err(|_| GameError::Storage)?;
            let now = self.observe_clock()?;
            if self.expire_due(now)? {
                return Err(GameError::Expired);
            }
            require_admission(&self.record()?, code, now)?;
            let expires = add_time(now, ADMISSION_MS)?;
            self.db.execute(
                SQL_INSERT_GAME_ADMISSION_CONTEXTS,
                &[
                    text(player),
                    SqlValue::Blob(digest.to_vec()),
                    text(code.as_str()),
                    int(now),
                    int(expires),
                    int(expires),
                ],
            )?;
            let stored = self
                .context(&token, code, now)
                .map_err(|_| GameError::Storage)?;
            if stored.player != player || stored.expires != expires {
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
    pub fn join_player(
        &self,
        command: CommandId,
        context: &str,
        player: Option<&str>,
        input: brews_contracts::games::JoinPlayer,
    ) -> Result<ParticipantOutcome, GameError> {
        let alias = brews_domain::games::validate_alias(&input.alias)
            .map_err(|_| GameError::InvalidInput)?
            .to_owned();
        let answer = input
            .recovery_answer
            .as_ref()
            .map(|s| crate::game::recovery_answers::normalize(s.as_str()))
            .transpose()?;
        let fp = join_fingerprint(
            &input.game_code,
            &alias,
            answer.as_deref().map(String::as_str),
            self.rate_key,
        )?;
        let (prepared, replay) = self.tx(|now| {
            self.join_check(
                (command, fp),
                context,
                player,
                &input.game_code,
                &alias,
                now,
            )
        })?;
        if let Some(receipt) = replay {
            return Ok(ParticipantOutcome {
                response: GameResponse::Committed { receipt },
                cookie: None,
            });
        }
        let token = zeroize::Zeroizing::new(
            crate::security::new_token(self.runtime).map_err(|_| GameError::RandomUnavailable)?,
        );
        let digest =
            crate::security::token_digest(&token).map_err(|_| GameError::RandomUnavailable)?;
        let verifier = answer
            .as_ref()
            .map(|value| crate::game::recovery_answers::enroll(value, self.policy, self.runtime))
            .transpose()?;
        let sid: brews_domain::ids::SessionId = make_id(self.runtime, self.runtime.now_ms())?
            .try_into()
            .map_err(|_| GameError::Storage)?;
        self.tx(|now| {
            let (current, replay) = self.join_check(
                (command, fp),
                context,
                player,
                &input.game_code,
                &alias,
                now,
            )?;
            if let Some(receipt) = replay {
                return Ok(ParticipantOutcome {
                    response: GameResponse::Committed { receipt },
                    cookie: None,
                });
            }
            if current.player != prepared.player {
                return Err(GameError::Conflict);
            }
            let r = self.record()?;
            let expires = add_time(now, PLAYER_SESSION_MS)?;
            self.db.execute(
                SQL_INSERT_GAME_PLAYERS,
                &[text(current.player), text(&alias), int(now)],
            )?;
            self.db.execute(
                SQL_INSERT_GAME_SESSIONS,
                &[
                    text(sid),
                    SqlValue::Blob(digest.to_vec()),
                    text(r.id),
                    text(input.game_code.as_str()),
                    text(current.player),
                    text(LIVE_ACCESS),
                    int(now),
                    int(expires),
                ],
            )?;
            if let Some(phc) = &verifier {
                self.db.execute(
                    SQL_INSERT_GAME_RECOVERY,
                    &[
                        text(current.player),
                        text(phc.as_str()),
                        int(crate::game::recovery_answers::NORMALIZATION_VERSION),
                        int(now),
                    ],
                )?;
            }
            self.db
                .execute(SQL_INSERT_GAME_VIEW_REVISIONS, &[text(current.player)])?;
            if self.view_revision(&current.player.to_string())? != 0 {
                return Err(GameError::Storage);
            }
            self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
            self.db.execute(
                SQL_UPDATE_GAME_ADMISSION_CONTEXTS,
                &[
                    text(command),
                    int(add_time(now, RECEIPT_MS)?),
                    text(current.player),
                ],
            )?;
            let receipt = self.store_receipt(
                &player_actor(current.player),
                command,
                fp,
                GameOutcome::PlayerJoined {
                    player_id: current.player,
                    view_revision: 0,
                    session_expires_at: expires,
                },
                r.id,
                now,
            )?;
            if let Some(phc) = &verifier {
                let rows = self.db.query(
                    SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT,
                    &[text(current.player)],
                )?;
                let stored = rows.first().ok_or(GameError::Storage)?;
                if string(stored, 0)? != phc.as_str()
                    || number(stored, 1)? != crate::game::recovery_answers::NORMALIZATION_VERSION
                    || number(stored, 2)? != now
                {
                    return Err(GameError::Storage);
                }
            }
            let actual = self
                .player_session(&token, &r, now)
                .map_err(|_| GameError::Storage)?;
            if actual.player != current.player
                || actual.expires != expires
                || self.context(context, &input.game_code, now)?.consumed != Some(command)
            {
                return Err(GameError::Storage);
            }
            Ok(ParticipantOutcome {
                response: GameResponse::PlayerJoined {
                    player_id: current.player,
                    alias,
                    session_expires_at: expires,
                    view_revision: 0,
                    receipt,
                },
                cookie: Some(SecretCookie { token, expires }),
            })
        })
    }
    fn context(&self, token: &str, code: &GameCode, now: i64) -> Result<Admission, GameError> {
        let digest = crate::security::token_digest(token).map_err(|_| GameError::Unauthorized)?;
        let rows = self.db.query(
            SQL_SELECT_GAME_ADMISSION_CONTEXTS_PLAYER_ID_GAME_CODE_ISSUED_AT,
            &[SqlValue::Blob(digest.to_vec())],
        )?;
        let row = rows.first().ok_or(GameError::Unauthorized)?;
        if string(row, 1)? != code.as_str() || number(row, 2)? > now || number(row, 4)? <= now {
            return Err(GameError::Unauthorized);
        }
        Ok(Admission {
            player: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
            expires: number(row, 3)?,
            consumed: if row.get(5) == Some(&SqlValue::Null) {
                None
            } else {
                Some(string(row, 5)?.parse().map_err(|_| GameError::Storage)?)
            },
        })
    }
    fn join_check(
        &self,
        command_binding: (CommandId, [u8; 32]),
        context: &str,
        player: Option<&str>,
        code: &GameCode,
        alias: &str,
        now: i64,
    ) -> Result<(Admission, Option<GameReceipt>), GameError> {
        let (command, fp) = command_binding;
        let r = self.record()?;
        if !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress)
            || r.published.is_none()
            || r.code.as_ref() != Some(code)
        {
            return Err(GameError::Conflict);
        }
        ensure_before_idle(&r, now)?;
        let c = self.context(context, code, now)?;
        if let Some(receipt) = self.receipt(&player_actor(c.player), command, fp, now)? {
            return Ok((c, Some(receipt)));
        }
        if c.consumed.is_some() {
            return Err(GameError::Conflict);
        }
        require_admission(&r, code, now)?;
        if c.expires <= now {
            return Err(GameError::Unauthorized);
        }
        if let Some(token) = player {
            match self.player_session(token, &r, now) {
                Ok(_) => return Err(GameError::Conflict),
                Err(GameError::Unauthorized) => {}
                Err(error) => return Err(error),
            }
        }
        self.admit(command, now)?;
        if !self
            .db
            .query(SQL_SELECT_PLAYER_BY_ALIAS, &[text(alias)])?
            .is_empty()
        {
            return Err(GameError::Conflict);
        }
        let count = self.db.query(SQL_SELECT_GAME_PLAYERS_COUNT, &[])?;
        if number(count.first().ok_or(GameError::Storage)?, 0)?
            >= i64::from(self.configuration()?.player_capacity)
        {
            return Err(GameError::Capacity);
        }
        Ok((c, None))
    }
    fn player_session(
        &self,
        token: &str,
        r: &Record,
        now: i64,
    ) -> Result<PlayerSession, GameError> {
        if matches!(r.state, GameState::Resolved | GameState::Cancelled) {
            return self.final_player_session(token, r, now);
        }
        let digest = crate::security::token_digest(token).map_err(|_| GameError::Unauthorized)?;
        let rows = self.db.query(
            SQL_SELECT_GAME_SESSIONS_S_SESSION_ID_S_PLAYER_ID_S_SESSION_EPOCH,
            &[SqlValue::Blob(digest.to_vec()), text(LIVE_ACCESS)],
        )?;
        let row = rows.first().ok_or(GameError::Unauthorized)?;
        if !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress)
            || r.published.is_none()
            || string(row, 4)? != r.id.to_string()
            || r.code.as_ref().map(GameCode::as_str) != Some(string(row, 5)?)
            || number(row, 3)? <= now
            || number(row, 6)? > now
        {
            return Err(GameError::Unauthorized);
        }
        Ok(PlayerSession {
            session: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
            player: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
            epoch: number(row, 2)?,
            expires: number(row, 3)?,
        })
    }
    pub fn prepare_player_connection(
        &self,
        token: &str,
        connection: ConnectionId,
    ) -> Result<ConnectionGrant, GameError> {
        self.tx(|now| {
            let r = self.record()?;
            let s = self.player_session(token, &r, now)?;
            let grant = ConnectionGrant {
                game: r.id,
                session: s.session,
                player: s.player,
                connection,
                epoch: s.epoch,
                expires: s.expires,
            };
            self.delete_prepared_connections(PLAYER_KIND, s.session)?;
            self.db.execute(
                SQL_INSERT_GAME_CONNECTION_GRANTS,
                &[
                    text(connection),
                    text(PLAYER_KIND),
                    text(s.session),
                    text(s.player),
                    text(connection),
                    int(s.epoch),
                    int(now),
                    int(s.expires),
                ],
            )?;
            if !self.has_prepared_connection(&grant)? {
                return Err(GameError::Storage);
            }
            Ok(grant)
        })
    }
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn validate_prepared_player_connection(
        &self,
        grant: &ConnectionGrant,
    ) -> Result<(), GameError> {
        self.tx(|now| {
            self.validate_player_connection(grant, &self.record()?, now)?;
            if !self.has_prepared_connection(grant)? {
                return Err(GameError::Unauthorized);
            }
            Ok(())
        })
    }
    pub fn accept_player_connection(&self, grant: &ConnectionGrant) -> Result<(), GameError> {
        self.tx(|now| {
            let r = self.record()?;
            self.validate_player_connection(grant, &r, now)?;
            let old = self.current_player_connection(grant.session)?;
            if old.as_ref() == Some(grant) {
                return Ok(());
            }
            if !self.has_prepared_connection(grant)? {
                return Err(GameError::Unauthorized);
            }
            self.db.execute(
                SQL_INSERT_GAME_CURRENT_CONNECTIONS,
                &[
                    text(PLAYER_KIND),
                    text(grant.session),
                    text(grant.player),
                    text(grant.connection),
                    int(grant.epoch),
                    int(grant.expires),
                ],
            )?;
            if self.current_player_connection(grant.session)?.as_ref() != Some(grant) {
                return Err(GameError::Storage);
            }
            self.db
                .execute(SQL_DELETE_CONNECTION_GRANT_BY_ID, &[text(grant.connection)])?;
            if self.has_prepared_connection(grant)? {
                return Err(GameError::Storage);
            }
            if old.is_none() && !matches!(r.state, GameState::Resolved | GameState::Cancelled) {
                self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
            }
            Ok(())
        })
    }
    pub fn disconnect(&self, grant: &ConnectionGrant) -> Result<(), GameError> {
        self.tx(|_| self.disconnect_inner(grant))
    }
    fn disconnect_inner(&self, grant: &ConnectionGrant) -> Result<(), GameError> {
        if self.remove_player_connection(grant)?
            && !matches!(
                self.record()?.state,
                GameState::Resolved | GameState::Cancelled
            )
        {
            self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
        }
        Ok(())
    }
    fn remove_player_connection(&self, grant: &ConnectionGrant) -> Result<bool, GameError> {
        if self.current_player_connection(grant.session)?.as_ref() != Some(grant) {
            return Ok(false);
        }
        self.db.execute(
            SQL_DELETE_CURRENT_CONNECTION_BY_SESSION,
            &[
                text(PLAYER_KIND),
                text(grant.session),
                text(grant.connection),
            ],
        )?;
        if self.current_player_connection(grant.session)?.is_some() {
            return Err(GameError::Storage);
        }
        Ok(true)
    }
    fn delete_prepared_connections(&self, kind: &str, session: SessionId) -> Result<(), GameError> {
        let binding = [text(kind), text(session)];
        self.db
            .execute(SQL_DELETE_PREPARED_CONNECTION_GRANTS_FOR_SESSION, &binding)?;
        if !self
            .db
            .query(SQL_SELECT_PREPARED_CONNECTION_FOR_SESSION, &binding)?
            .is_empty()
        {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    fn has_prepared_connection(&self, g: &ConnectionGrant) -> Result<bool, GameError> {
        let rows = self.db.query(
            SQL_SELECT_GAME_CONNECTION_GRANTS_SESSION_ID_PRINCIPAL_ID_CONNECTION_ID,
            &[text(g.connection), text(PLAYER_KIND)],
        )?;
        let Some(r) = rows.first() else {
            return Ok(false);
        };
        Ok(string(r, 0)? == g.session.to_string()
            && string(r, 1)? == g.player.to_string()
            && string(r, 2)? == g.connection.to_string()
            && number(r, 3)? == g.epoch
            && number(r, 4)? == g.expires)
    }
    fn current_player_connection(
        &self,
        session: SessionId,
    ) -> Result<Option<ConnectionGrant>, GameError> {
        let r = self.record()?;
        self.db
            .query(
                SQL_SELECT_GAME_CURRENT_CONNECTIONS_SESSION_ID_PRINCIPAL_ID_CONNECTION_ID,
                &[text(PLAYER_KIND), text(session)],
            )?
            .first()
            .map(|row| parse_connection(row, r.id))
            .transpose()
    }
    fn validate_player_connection(
        &self,
        g: &ConnectionGrant,
        r: &Record,
        now: i64,
    ) -> Result<(), GameError> {
        if matches!(r.state, GameState::Resolved | GameState::Cancelled) {
            return self.validate_final_player_connection(g, r, now);
        }
        if g.game != r.id
            || g.expires <= now
            || r.published.is_none()
            || !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress)
        {
            return Err(GameError::Unauthorized);
        }
        let rows = self.db.query(
            SQL_SELECT_GAME_SESSIONS_S_PLAYER_ID_S_SESSION_EPOCH_S_EXPIRES_AT,
            &[text(g.session), text(LIVE_ACCESS)],
        )?;
        let row = rows.first().ok_or(GameError::Unauthorized)?;
        if string(row, 0)? != g.player.to_string()
            || number(row, 1)? != g.epoch
            || number(row, 2)? != g.expires
            || string(row, 3)? != r.id.to_string()
            || r.code.as_ref().map(GameCode::as_str) != Some(string(row, 4)?)
        {
            return Err(GameError::Unauthorized);
        }
        Ok(())
    }
    fn reconcile_presence(
        &self,
        observed: &[ConnectionId],
        r: &Record,
        now: i64,
    ) -> Result<(), GameError> {
        if observed.len() > OBSERVED_CONNECTION_LIMIT {
            return Err(GameError::InvalidInput);
        }
        let rows = self.db.query(
            SQL_SELECT_VIEWER_CONNECTIONS_FOR_RECONCILIATION,
            &[text(PLAYER_KIND)],
        )?;
        if rows.len() > RETAINED_PLAYER_LIMIT {
            return Err(GameError::Storage);
        }
        let mut changed = false;
        for row in rows {
            let grant = parse_connection(&row, r.id)?;
            if !observed.contains(&grant.connection) {
                changed |= self.remove_player_connection(&grant)?;
                continue;
            }
            match self.validate_player_connection(&grant, r, now) {
                Ok(()) => {}
                Err(GameError::Unauthorized) => changed |= self.remove_player_connection(&grant)?,
                Err(error) => return Err(error),
            }
        }
        if changed && !matches!(r.state, GameState::Resolved | GameState::Cancelled) {
            self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
        }
        Ok(())
    }
    pub fn account_view(
        &self,
        authority: GameAccountAuthority,
        observed: &[ConnectionId],
    ) -> Result<GameView, GameError> {
        self.tx(|now| {
            authorize(&authority, now)?;
            let r = self.record()?;
            ensure_before_idle(&r, now)?;
            self.enter_account(&authority, now)?;
            self.reconcile_presence(observed, &r, now)?;
            self.host_projection(&r, now)
        })
    }
    fn enter_account(&self, a: &GameAccountAuthority, now: i64) -> Result<(), GameError> {
        let record = self.record()?;
        let terminal = matches!(record.state, GameState::Resolved | GameState::Cancelled);
        if terminal {
            self.require_final_history(&record, now)?;
        }
        let rows = self.db.query(
            SQL_SELECT_GAME_CONNECTION_GRANTS_PRINCIPAL_ID_EPOCH_EXPIRES_AT,
            &[text(a.session_id()), text(ACCOUNT_KIND)],
        )?;
        if let Some(r) = rows.first() {
            if string(r, 0)? != a.account_id().to_string()
                || number(r, 1)? != a.credential_epoch()
                || number(r, 2)? != a.expires_at()
            {
                return Err(GameError::Unauthorized);
            }
            return Ok(());
        }
        if terminal {
            return Err(GameError::Unauthorized);
        }
        self.db.execute(
            SQL_INSERT_ACCOUNT_CONNECTION_GRANT,
            &[
                text(a.session_id()),
                text(ACCOUNT_KIND),
                text(a.session_id()),
                text(a.account_id()),
                int(a.credential_epoch()),
                int(now),
                int(a.expires_at()),
            ],
        )?;
        let exact = self.db.query(
            SQL_SELECT_GAME_CONNECTION_GRANTS_EXPIRES_AT,
            &[text(a.session_id())],
        )?;
        if number(exact.first().ok_or(GameError::Storage)?, 0)? != a.expires_at() {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    fn host_projection(&self, r: &Record, now: i64) -> Result<GameView, GameError> {
        if matches!(r.state, GameState::Resolved | GameState::Cancelled) {
            return self.final_host_projection(r, now);
        }
        let configuration = self.configuration()?;
        let calls = self.call_history(r, configuration.numeric_upper_bound)?;
        let call_views = calls.iter().map(|call| call.view.clone()).collect();
        let mut players = Vec::new();
        let rows = self
            .db
            .query(SQL_SELECT_GAME_PLAYERS_PLAYER_ID_ALIAS, &[])?;
        if rows.len() > RETAINED_PLAYER_LIMIT {
            return Err(GameError::Storage);
        }
        for row in rows {
            let player: PlayerId = string(&row, 0)?.parse().map_err(|_| GameError::Storage)?;
            let connected = self
                .db
                .query(
                    SQL_CHECK_CURRENT_CONNECTION_FOR_PRINCIPAL,
                    &[text(PLAYER_KIND), text(player), int(now)],
                )?
                .len()
                == 1;
            players.push(HostPlayerView {
                player_id: player,
                alias: string(&row, 1)?.to_owned(),
                connected,
                board: self.board_view_at(player, r, calls.len() as u32)?,
            });
        }
        let count = players.iter().filter(|p| p.connected).count() as u8;
        Ok(GameView::Host {
            game: r.summary(self.view_revision(HOST_VIEW)? as u64),
            configuration,
            game_code: if r.published.is_some() {
                r.code.clone()
            } else {
                None
            },
            started_at: r.started,
            ended_at: r.ended,
            idle_cancel_due_at: r.due,
            players,
            calls: call_views,
            connected_player_count: count,
            spectator_count: 0,
            connected_spectator_count: 0,
        })
    }
    pub fn player_view(&self, token: &str) -> Result<GameView, GameError> {
        self.tx(|now| {
            let r = self.record()?;
            ensure_before_idle(&r, now)?;
            let s = self.player_session(token, &r, now)?;
            self.player_projection(&r, s.player)
        })
    }
    fn player_projection(&self, r: &Record, player: PlayerId) -> Result<GameView, GameError> {
        if matches!(r.state, GameState::Resolved | GameState::Cancelled) {
            return self.final_player_projection(r, player);
        }
        let configuration = self.configuration()?;
        let calls = self.call_history(r, configuration.numeric_upper_bound)?;
        let call_views = calls.iter().map(|call| call.view.clone()).collect();
        let board = self.board_view_at(player, r, calls.len() as u32)?;
        let rows = self
            .db
            .query(SQL_SELECT_GAME_PLAYERS_ALIAS, &[text(player)])?;
        Ok(GameView::Player {
            game_id: r.id,
            state: r.state,
            configuration,
            game_code: r.code.clone(),
            started_at: r.started,
            ended_at: r.ended,
            view_revision: self.view_revision(&player.to_string())? as u64,
            player_id: player,
            alias: string(rows.first().ok_or(GameError::Storage)?, 0)?.to_owned(),
            calls: call_views,
            board,
        })
    }
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
            if number(count.first().ok_or(GameError::Storage)?, 0)? < 2 {
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
    fn enqueue_terminal_release(
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
    fn call_history(
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
    fn board_view(&self, player: PlayerId, r: &Record) -> Result<Option<BoardView>, GameError> {
        if r.state != GameState::InProgress {
            return Ok(None);
        }
        let configuration = self.configuration()?;
        let calls = self.call_history(r, configuration.numeric_upper_bound)?;
        self.board_view_at(player, r, calls.len() as u32)
    }
    fn board_view_at(
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
    pub fn cleanup(&self) -> Result<Option<TerminalProof>, GameError> {
        self.maintenance(|now| {
            self.compact(now)?;
            self.terminal_inner()
        })
    }
    fn terminal_inner(&self) -> Result<Option<TerminalProof>, GameError> {
        if let Some(projection) = self.terminal_routing_projection()? {
            return Ok(Some(TerminalProof::new(projection)));
        }
        self.record_optional()?
            .filter(|record| matches!(record.state, GameState::Cancelled | GameState::Resolved))
            .map(|r| r.projection().map(TerminalProof::new))
            .transpose()
    }
    fn expire_due(&self, now: i64) -> Result<bool, GameError> {
        let Some(r) = self.record_optional()? else {
            return Ok(false);
        };
        if !matches!(r.state, GameState::New | GameState::AwaitingPlayers)
            || !r.due.is_some_and(|due| now >= due)
        {
            return Ok(false);
        }
        let rev = advance(r.revision)?;
        let host_view_revision = advance(self.view_revision(HOST_VIEW)?)?;
        self.db.execute(
            SQL_CANCEL_UNSTARTED_GAME,
            &[
                text(GameState::Cancelled),
                int(rev),
                int(now),
                text(crate::game::HOST_IDLE_REASON),
                text(crate::game::TERMINAL_ACTOR_SYSTEM),
                SqlValue::Null,
                text(r.state),
                int(r.revision),
            ],
        )?;
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
        self.set_view_revision(HOST_VIEW, host_view_revision)?;
        self.retire_prestart_counters()?;
        let w = PendingWork {
            operation: r
                .creation_command
                .to_string()
                .parse()
                .map_err(|_| GameError::Storage)?,
            kind: WorkKind::Release,
            phase: WorkPhase::AwaitingAcknowledgement,
            actor: SYSTEM_ACTOR.into(),
            command: r.creation_command,
            fingerprint: *r.fp.digest(),
            expected: 0,
            fence: rev,
            created: now,
            next: add_time(now, crate::game::RETRY_INITIAL_MS)?,
            attempts: 0,
        };
        self.insert_work(&w)?;
        let after = self.record_optional()?.ok_or(GameError::Storage)?;
        if after.state != GameState::Cancelled || after.ended != Some(now) {
            return Err(GameError::Storage);
        }
        Ok(true)
    }
    fn compact(&self, now: i64) -> Result<(), GameError> {
        self.purge_expired_history(now)?;
        self.cleanup_terminal_access(now)?;
        if let Some(r) = self.record_optional()?.filter(|r| {
            matches!(
                r.state,
                GameState::New | GameState::AwaitingPlayers | GameState::InProgress
            )
        }) {
            let rows = self.db.query(
                SQL_SELECT_VIEWER_CONNECTIONS_FOR_RECONCILIATION,
                &[text(PLAYER_KIND)],
            )?;
            if rows.len() > RETAINED_PLAYER_LIMIT {
                return Err(GameError::Storage);
            }
            let mut changed = false;
            for row in rows {
                let g = parse_connection(&row, r.id)?;
                match self.validate_player_connection(&g, &r, now) {
                    Ok(()) => {}
                    Err(GameError::Unauthorized) => changed |= self.remove_player_connection(&g)?,
                    Err(error) => return Err(error),
                }
            }
            if changed {
                self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
            }
        }
        let account_connections = self.db.query(
            SQL_SELECT_GAME_CURRENT_CONNECTIONS_EXPIRED_ACCOUNT,
            &[
                text(ACCOUNT_KIND),
                int(now),
                int(crate::limits::CLEANUP_BATCH_SIZE),
            ],
        )?;
        for row in account_connections {
            let session: SessionId = string(&row, 0)?.parse().map_err(|_| GameError::Storage)?;
            let connection: ConnectionId =
                string(&row, 1)?.parse().map_err(|_| GameError::Storage)?;
            self.db.execute(
                SQL_DELETE_CURRENT_CONNECTION_BY_SESSION,
                &[text(ACCOUNT_KIND), text(session), text(connection)],
            )?;
            if self.current_account_connection(session)?.is_some() {
                return Err(GameError::Storage);
            }
        }
        let receipts = self.db.query(
            SQL_SELECT_GAME_RECEIPTS_ACTOR_COMMAND_ID_FINGERPRINT,
            &[int(now), int(crate::limits::CLEANUP_BATCH_SIZE)],
        )?;
        for row in receipts {
            let receipt = GameReceipt::decode_json(string(&row, 3)?.as_bytes())
                .map_err(|_| GameError::Storage)?;
            let command: CommandId = string(&row, 1)?.parse().map_err(|_| GameError::Storage)?;
            if blob(&row, 2)?.len() != 32
                || receipt.command_id != command
                || receipt.completed_at != number(&row, 4)?
                || receipt.expires_at != number(&row, 5)?
                || receipt.expires_at != add_time(receipt.completed_at, RECEIPT_MS)?
            {
                return Err(GameError::Storage);
            }
            self.db.execute(
                SQL_DELETE_RECEIPT_BY_ACTOR_AND_COMMAND,
                &[text(string(&row, 0)?), text(command)],
            )?;
            if !self
                .db
                .query(
                    SQL_CHECK_RECEIPT_EXISTS_FOR_ACTOR_AND_COMMAND,
                    &[text(string(&row, 0)?), text(command)],
                )?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        let contexts = self.db.query(
            SQL_SELECT_GAME_ADMISSION_CONTEXTS_PLAYER_ID,
            &[int(now), int(ADMISSION_LIMIT)],
        )?;
        for row in contexts {
            let player: PlayerId = string(&row, 0)?.parse().map_err(|_| GameError::Storage)?;
            self.db
                .execute(SQL_DELETE_ADMISSION_CONTEXT_BY_PLAYER, &[text(player)])?;
            if !self
                .db
                .query(
                    SQL_CHECK_ADMISSION_CONTEXT_EXISTS_FOR_PLAYER,
                    &[text(player)],
                )?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        let sessions = self.db.query(
            SQL_SELECT_GAME_SESSIONS_SESSION_ID,
            &[int(now), int(crate::limits::CLEANUP_BATCH_SIZE)],
        )?;
        for row in sessions {
            let session: SessionId = string(&row, 0)?.parse().map_err(|_| GameError::Storage)?;
            self.db
                .execute(SQL_DELETE_SESSION_BY_ID, &[text(session)])?;
            if !self
                .db
                .query(SQL_CHECK_SESSION_EXISTS_BY_ID, &[text(session)])?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        let grants = self.db.query(
            SQL_SELECT_GAME_CONNECTION_GRANTS_GRANT_ID,
            &[int(now), int(crate::limits::CLEANUP_BATCH_SIZE)],
        )?;
        for row in grants {
            let id = string(&row, 0)?;
            self.db
                .execute(SQL_DELETE_CONNECTION_GRANT_BY_ID, &[text(id)])?;
            if !self
                .db
                .query(SQL_CHECK_CONNECTION_GRANT_EXISTS_BY_ID, &[text(id)])?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }
    pub fn acknowledge_release(
        &self,
        work: &PendingWork,
        ack: ReleaseAck,
    ) -> Result<(), GameError> {
        self.maintenance(|_| {
            let proof = self.terminal_inner()?.ok_or(GameError::NotFound)?;
            let projection = proof.projection();
            let current = self
                .operation_work(work.operation)?
                .ok_or(GameError::Conflict)?;
            if current != *work
                || current.kind != WorkKind::Release
                || current.fence != projection.source_revision()
                || ack.game_id() != projection.game_id()
            {
                return Err(GameError::Conflict);
            }
            self.delete_work(current.operation)
        })
    }
    pub fn next_deadline(&self) -> Result<Option<i64>, GameError> {
        self.maintenance(|_| {
            let rows = self.db.query(SQL_SELECT_GAME_DEADLINES, &[])?;
            let [row] = rows.as_slice() else {
                return Err(GameError::Storage);
            };
            if row.len() != GAME_DEADLINE_SOURCE_COUNT {
                return Err(GameError::Storage);
            }
            let mut earliest = None;
            for index in 0..GAME_DEADLINE_SOURCE_COUNT {
                if let Some(due) = optional_number(row, index)? {
                    earliest = Some(earliest.map_or(due, |prior: i64| prior.min(due)));
                }
            }
            Ok(earliest)
        })
    }
    pub fn retry_work(&self, work: &PendingWork) -> Result<PendingWork, GameError> {
        self.maintenance(|now| {
            let current = self
                .operation_work(work.operation)?
                .ok_or(GameError::Conflict)?;
            if current != *work {
                return Err(GameError::Conflict);
            }
            let attempts = current.attempts.saturating_add(1);
            let delay = crate::game::RETRY_INITIAL_MS
                .saturating_mul(1i64 << attempts.min(9))
                .min(crate::game::RETRY_MAX_MS);
            self.db.execute(
                SQL_UPDATE_PENDING_WORK_RETRY,
                &[
                    int(i64::from(attempts)),
                    int(add_time(now, delay)?),
                    text(current.operation),
                ],
            )?;
            let actual = self
                .operation_work(current.operation)?
                .ok_or(GameError::Storage)?;
            if actual.attempts != attempts || actual.next != add_time(now, delay)? {
                return Err(GameError::Storage);
            }
            Ok(actual)
        })
    }
    pub fn seal_abandoned_creation(&self, work: &CreationWork) -> Result<TerminalProof, GameError> {
        self.maintenance(|now| {
            if now < work.deadline() {
                return Err(GameError::Conflict);
            }
            if let Some(r) = self.record_optional()? {
                if r.id != work.game_id()
                    || r.creator != work.account_id()
                    || r.creation_command != work.command_id()
                    || r.fp != work.fingerprint()
                    || r.created != work.created_at()
                {
                    return Err(GameError::Conflict);
                }
                return self.terminal_inner()?.ok_or(GameError::Conflict);
            }
            self.db.execute(
                SQL_INSERT_CANCELLED_GAME_RECORD,
                &[
                    text(work.game_id()),
                    text(work.account_id()),
                    text(work.command_id()),
                    SqlValue::Blob(work.fingerprint().digest().to_vec()),
                    text(work.account_id()),
                    text(GameState::Cancelled),
                    int(work.created_at()),
                    int(work.created_at()),
                    int(now),
                    text(crate::game::HOST_IDLE_REASON),
                ],
            )?;
            let pending = PendingWork {
                operation: work
                    .command_id()
                    .to_string()
                    .parse()
                    .map_err(|_| GameError::Storage)?,
                kind: WorkKind::Release,
                phase: WorkPhase::AwaitingAcknowledgement,
                actor: SYSTEM_ACTOR.into(),
                command: work.command_id(),
                fingerprint: *work.fingerprint().digest(),
                expected: 0,
                fence: 1,
                created: now,
                next: add_time(now, crate::game::RETRY_INITIAL_MS)?,
                attempts: 0,
            };
            self.insert_work(&pending)?;
            self.terminal_inner()?.ok_or(GameError::Storage)
        })
    }
    pub fn prepare_account_connection(
        &self,
        authority: GameAccountAuthority,
        connection: ConnectionId,
    ) -> Result<AccountConnectionGrant, GameError> {
        self.tx(|now| {
            authorize(&authority, now)?;
            let r = self.record()?;
            self.enter_account(&authority, now)?;
            let grant = AccountConnectionGrant {
                game: r.id,
                session: authority.session_id(),
                account: authority.account_id(),
                connection,
                epoch: authority.credential_epoch(),
                expires: authority.expires_at(),
            };
            self.delete_prepared_connections(ACCOUNT_KIND, grant.session)?;
            self.db.execute(
                SQL_INSERT_GAME_CONNECTION_GRANTS,
                &[
                    text(connection),
                    text(ACCOUNT_KIND),
                    text(grant.session),
                    text(grant.account),
                    text(connection),
                    int(grant.epoch),
                    int(now),
                    int(grant.expires),
                ],
            )?;
            if !self.has_account_prepared(&grant)? {
                return Err(GameError::Storage);
            }
            Ok(grant)
        })
    }
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn validate_prepared_account_connection(
        &self,
        grant: &AccountConnectionGrant,
        authority: &GameAccountAuthority,
    ) -> Result<(), GameError> {
        self.tx(|now| {
            self.validate_account_grant(grant, authority, now)?;
            if !self.has_account_prepared(grant)? {
                return Err(GameError::Unauthorized);
            }
            Ok(())
        })
    }
    pub fn accept_account_connection(
        &self,
        grant: &AccountConnectionGrant,
        authority: GameAccountAuthority,
    ) -> Result<(), GameError> {
        self.tx(|now| {
            self.validate_account_grant(grant, &authority, now)?;
            if self.current_account_connection(grant.session)?.as_ref() == Some(grant) {
                return Ok(());
            }
            if !self.has_account_prepared(grant)? {
                return Err(GameError::Unauthorized);
            }
            self.db.execute(
                SQL_INSERT_GAME_CURRENT_CONNECTIONS,
                &[
                    text(ACCOUNT_KIND),
                    text(grant.session),
                    text(grant.account),
                    text(grant.connection),
                    int(grant.epoch),
                    int(grant.expires),
                ],
            )?;
            if self.current_account_connection(grant.session)?.as_ref() != Some(grant) {
                return Err(GameError::Storage);
            }
            self.db
                .execute(SQL_DELETE_CONNECTION_GRANT_BY_ID, &[text(grant.connection)])?;
            if self.has_account_prepared(grant)? {
                return Err(GameError::Storage);
            }
            Ok(())
        })
    }
    pub fn account_connection_view(
        &self,
        grant: &AccountConnectionGrant,
        authority: GameAccountAuthority,
    ) -> Result<GameView, GameError> {
        self.tx(|now| {
            self.validate_account_grant(grant, &authority, now)?;
            if self.current_account_connection(grant.session)?.as_ref() != Some(grant) {
                return Err(GameError::Unauthorized);
            }
            self.host_projection(&self.record()?, now)
        })
    }
    fn validate_account_grant(
        &self,
        g: &AccountConnectionGrant,
        a: &GameAccountAuthority,
        now: i64,
    ) -> Result<(), GameError> {
        authorize(a, now)?;
        if self.record()?.id != g.game
            || g.account != a.account_id()
            || g.session != a.session_id()
            || g.epoch != a.credential_epoch()
            || g.expires != a.expires_at()
        {
            return Err(GameError::Unauthorized);
        }
        let rows = self.db.query(
            SQL_SELECT_ACCOUNT_PREPARED_GRANT_DETAILS,
            &[text(g.session), text(ACCOUNT_KIND)],
        )?;
        let r = rows.first().ok_or(GameError::Unauthorized)?;
        if string(r, 0)? != g.account.to_string()
            || number(r, 1)? != g.epoch
            || number(r, 2)? != g.expires
            || g.expires <= now
        {
            return Err(GameError::Unauthorized);
        }
        Ok(())
    }
    fn has_account_prepared(&self, g: &AccountConnectionGrant) -> Result<bool, GameError> {
        let rows = self.db.query(
            SQL_SELECT_GAME_CONNECTION_GRANTS_SESSION_ID_PRINCIPAL_ID_CONNECTION_ID,
            &[text(g.connection), text(ACCOUNT_KIND)],
        )?;
        let Some(r) = rows.first() else {
            return Ok(false);
        };
        Ok(string(r, 0)? == g.session.to_string()
            && string(r, 1)? == g.account.to_string()
            && string(r, 2)? == g.connection.to_string()
            && number(r, 3)? == g.epoch
            && number(r, 4)? == g.expires)
    }
    fn current_account_connection(
        &self,
        session: SessionId,
    ) -> Result<Option<AccountConnectionGrant>, GameError> {
        let r = self.record_optional()?.ok_or(GameError::NotFound)?;
        self.db
            .query(
                SQL_SELECT_GAME_CURRENT_CONNECTIONS_SESSION_ID_PRINCIPAL_ID_CONNECTION_ID,
                &[text(ACCOUNT_KIND), text(session)],
            )?
            .first()
            .map(|row| {
                Ok(AccountConnectionGrant {
                    game: r.id,
                    session: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
                    account: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
                    connection: string(row, 2)?.parse().map_err(|_| GameError::Storage)?,
                    epoch: number(row, 3)?,
                    expires: number(row, 4)?,
                })
            })
            .transpose()
    }
    pub fn close_account_connection(
        &self,
        work: &crate::auth::GameSocketCloseWork,
    ) -> Result<(), GameError> {
        self.maintenance(|_| {
            let Some(r) = self.record_optional()? else {
                return Ok(());
            };
            if r.id != work.game_id() {
                return Err(GameError::Conflict);
            }
            if let Some(g) = self.current_account_connection(work.session_id())?
                && g.connection == work.connection_id()
                && g.account == work.account_id()
                && g.epoch == work.credential_epoch()
                && g.expires == work.expires_at()
            {
                self.db.execute(
                    SQL_DELETE_CURRENT_CONNECTION_BY_SESSION,
                    &[text(ACCOUNT_KIND), text(g.session), text(g.connection)],
                )?;
                if self.current_account_connection(g.session)?.as_ref() == Some(&g) {
                    return Err(GameError::Storage);
                }
            }
            let target = [
                text(work.connection_id()),
                text(ACCOUNT_KIND),
                text(work.session_id()),
                text(work.account_id()),
                int(work.credential_epoch()),
                int(work.expires_at()),
            ];
            self.db
                .execute(SQL_DELETE_EXACT_ACCOUNT_CONNECTION_GRANT, &target)?;
            if !self
                .db
                .query(SQL_SELECT_EXACT_ACCOUNT_CLOSE_PREPARATION, &target)?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
            Ok(())
        })
    }
    pub fn sync_player(
        &self,
        token: &str,
        known: Option<u64>,
    ) -> Result<brews_contracts::games::SyncResponse, GameError> {
        if let Some(known) = known {
            revision(known)?;
        }
        let view = self.player_view(token)?;
        let rev = view.view_revision();
        Ok(brews_contracts::games::SyncResponse {
            up_to_date: known == Some(rev),
            view_revision: rev,
            snapshot: if known == Some(rev) { None } else { Some(view) },
        })
    }
    pub fn player_connection(
        &self,
        connection: ConnectionId,
    ) -> Result<Option<ConnectionGrant>, GameError> {
        self.tx(|now| {
            let r = self.record()?;
            let rows = self.db.query(
                SQL_SELECT_CURRENT_CONNECTION_BY_VIEWER_AND_CONNECTION_ID,
                &[text(PLAYER_KIND), text(connection)],
            )?;
            let Some(row) = rows.first() else {
                return Ok(None);
            };
            let g = parse_connection(row, r.id)?;
            self.validate_player_connection(&g, &r, now)?;
            Ok(Some(g))
        })
    }
    pub fn player_connection_view(&self, grant: &ConnectionGrant) -> Result<GameView, GameError> {
        self.tx(|now| {
            let r = self.record()?;
            self.validate_player_connection(grant, &r, now)?;
            if self.current_player_connection(grant.session)?.as_ref() != Some(grant) {
                return Err(GameError::Unauthorized);
            }
            self.player_projection(&r, grant.player)
        })
    }
    pub fn reconcile_connections(&self, observed: &[ConnectionId]) -> Result<(), GameError> {
        self.maintenance(|now| {
            if let Some(r) = self
                .record_optional()?
                .filter(|r| r.state != GameState::Cancelled)
            {
                self.reconcile_presence(observed, &r, now)?;
            }
            Ok(())
        })
    }
    pub fn sync_account(
        &self,
        authority: GameAccountAuthority,
        known: Option<u64>,
        observed: &[ConnectionId],
    ) -> Result<brews_contracts::games::SyncResponse, GameError> {
        if let Some(known) = known {
            revision(known)?;
        }
        let view = self.account_view(authority, observed)?;
        let rev = view.view_revision();
        Ok(brews_contracts::games::SyncResponse {
            up_to_date: known == Some(rev),
            view_revision: rev,
            snapshot: if known == Some(rev) { None } else { Some(view) },
        })
    }
    fn tx<T>(&self, operation: impl FnOnce(i64) -> Result<T, GameError>) -> Result<T, GameError> {
        self.transaction_cut(true, operation)
    }
    fn maintenance<T>(
        &self,
        operation: impl FnOnce(i64) -> Result<T, GameError>,
    ) -> Result<T, GameError> {
        self.transaction_cut(false, operation)
    }
    fn transaction_cut<T>(
        &self,
        reject_expired: bool,
        operation: impl FnOnce(i64) -> Result<T, GameError>,
    ) -> Result<T, GameError> {
        self.db
            .transaction(|| {
                validate_schema(self.db)?;
                let now = self.observe_clock().map_err(|_| StorageError)?;
                let expired = self.expire_due(now).map_err(|_| StorageError)?;
                if expired && reject_expired {
                    return Ok(Err(GameError::Expired));
                }
                match operation(now) {
                    Err(GameError::Storage) => Err(StorageError),
                    out => Ok(out),
                }
            })
            .map_err(|_| GameError::Storage)?
    }
    fn observe_clock(&self) -> Result<i64, GameError> {
        let rows = self.db.query(
            SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
            &[],
        )?;
        let row = rows.first().ok_or(GameError::Storage)?;
        let previous = number(row, 0)?;
        let floor = number(row, 1)?;
        let now = self.runtime.now_ms();
        if !(0..=JS_SAFE_INTEGER_MAX).contains(&now) || now < previous || floor > previous {
            return Err(GameError::Storage);
        }
        let next_floor = floor.max(now.saturating_sub(RECEIPT_MS));
        self.db
            .execute(SQL_UPDATE_GAME_METADATA, &[int(now), int(next_floor)])?;
        let actual = self.db.query(
            SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
            &[],
        )?;
        let row = actual.first().ok_or(GameError::Storage)?;
        if number(row, 0)? != now || number(row, 1)? != next_floor {
            return Err(GameError::Storage);
        }
        Ok(now)
    }
    fn record_optional(&self) -> Result<Option<Record>, GameError> {
        let rows = self.db.query(
            SQL_SELECT_GAME_RECORD_GAME_ID_CREATOR_ID_CREATION_COMMAND,
            &[],
        )?;
        rows.first().map(Record::parse).transpose()
    }
    fn record(&self) -> Result<Record, GameError> {
        self.record_optional()?
            .filter(|r| r.state != GameState::Cancelled || r.started.is_some())
            .ok_or(GameError::NotFound)
    }
    fn configuration(&self) -> Result<GameConfiguration, GameError> {
        let rows = self.db.query(
            SQL_SELECT_GAME_CONFIGURATION_NUMERIC_UPPER_BOUND_BOARD_SIDE_LENGTH_FREE_CELLS_ENABLED,
            &[],
        )?;
        let r = rows.first().ok_or(GameError::Storage)?;
        let positions = self
            .db
            .query(SQL_SELECT_GAME_FREE_CELLS_ROW_COLUMN, &[])?
            .iter()
            .map(|r| {
                Ok(CellPosition {
                    row: u8::try_from(number(r, 0)?).map_err(|_| GameError::Storage)?,
                    column: u8::try_from(number(r, 1)?).map_err(|_| GameError::Storage)?,
                })
            })
            .collect::<Result<Vec<_>, GameError>>()?;
        let config = GameConfiguration {
            numeric_upper_bound: u32::try_from(number(r, 0)?).map_err(|_| GameError::Storage)?,
            board_side_length: u8::try_from(number(r, 1)?).map_err(|_| GameError::Storage)?,
            free_cells_enabled: number(r, 2)? == 1,
            player_capacity: u8::try_from(number(r, 3)?).map_err(|_| GameError::Storage)?,
            spectator_capacity: u8::try_from(number(r, 4)?).map_err(|_| GameError::Storage)?,
            winning_pattern: string(r, 5)?.parse().map_err(|_| GameError::Storage)?,
            free_cell_positions: positions,
        };
        config.validate().map_err(|_| GameError::Storage)?;
        Ok(config)
    }
    fn admit(&self, command: CommandId, now: i64) -> Result<(), GameError> {
        let id =
            uuid::Uuid::parse_str(&command.to_string()).map_err(|_| GameError::InvalidInput)?;
        let b = id.as_bytes();
        let stamp = i64::from_be_bytes([0, 0, b[0], b[1], b[2], b[3], b[4], b[5]]);
        let r = self
            .db
            .query(SQL_SELECT_GAME_METADATA_COMMAND_FLOOR_MS, &[])?;
        if stamp <= number(r.first().ok_or(GameError::Storage)?, 0)? || stamp > now {
            return Err(GameError::StaleCommand);
        }
        Ok(())
    }
    fn receipt(
        &self,
        actor: &str,
        command: CommandId,
        fp: [u8; 32],
        now: i64,
    ) -> Result<Option<GameReceipt>, GameError> {
        let r = self.db.query(
            SQL_SELECT_GAME_RECEIPTS_FINGERPRINT_OUTCOME_EXPIRES_AT,
            &[text(actor), text(command)],
        )?;
        let Some(row) = r.first() else {
            return Ok(None);
        };
        if blob(row, 0)? != fp {
            return Err(GameError::Conflict);
        }
        if number(row, 2)? <= now {
            return Err(GameError::StaleCommand);
        }
        let receipt =
            GameReceipt::decode_json(string(row, 1)?.as_bytes()).map_err(|_| GameError::Storage)?;
        if receipt.command_id != command || receipt.expires_at != number(row, 2)? {
            return Err(GameError::Storage);
        }
        Ok(Some(receipt))
    }
    fn store_receipt(
        &self,
        actor: &str,
        command: CommandId,
        fp: [u8; 32],
        outcome: GameOutcome,
        game: GameId,
        now: i64,
    ) -> Result<GameReceipt, GameError> {
        let receipt = GameReceipt {
            version: 1,
            command_id: command,
            game_id: game,
            outcome,
            completed_at: now,
            expires_at: add_time(now, RECEIPT_MS)?,
        };
        let raw = String::from_utf8(receipt.encode_json().map_err(|_| GameError::Storage)?)
            .map_err(|_| GameError::Storage)?;
        let persisted = vec![
            text(actor),
            text(command),
            SqlValue::Blob(fp.to_vec()),
            text(raw),
            int(now),
            int(receipt.expires_at),
        ];
        self.db.execute(SQL_INSERT_GAME_RECEIPTS, &persisted)?;
        self.verify_required_receipt(actor, command, persisted)?;
        Ok(receipt)
    }
    fn verify_required_receipt(
        &self,
        actor: &str,
        command: CommandId,
        expected: super::Row,
    ) -> Result<(), GameError> {
        // Required-write verification is not a business retry: any inconsistency rolls back.
        if self.db.query(
            SQL_SELECT_REQUIRED_GAME_RECEIPT,
            &[text(actor), text(command)],
        )? != vec![expected]
        {
            return Err(GameError::Storage);
        }
        Ok(())
    }
}
struct StoredGameCall {
    view: CallView,
    mode: CallMode,
    called_by: AccountId,
    command: CommandId,
    called_at: i64,
}
fn parse_game_call(
    row: &super::Row,
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
pub(crate) fn add_three_calendar_months(timestamp: i64) -> Result<i64, GameError> {
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
fn account_actor(id: AccountId) -> String {
    format!("account:{id}")
}
fn player_actor(id: PlayerId) -> String {
    format!("player:{id}")
}
fn int(value: i64) -> SqlValue {
    SqlValue::Integer(value)
}
fn text(value: impl ToString) -> SqlValue {
    SqlValue::Text(value.to_string())
}
fn number(row: &super::Row, n: usize) -> Result<i64, GameError> {
    match row.get(n) {
        Some(SqlValue::Integer(i)) => Ok(*i),
        _ => Err(GameError::Storage),
    }
}
fn string(row: &super::Row, n: usize) -> Result<&str, GameError> {
    match row.get(n) {
        Some(SqlValue::Text(s)) => Ok(s),
        _ => Err(GameError::Storage),
    }
}
fn blob(row: &super::Row, n: usize) -> Result<&[u8], GameError> {
    match row.get(n) {
        Some(SqlValue::Blob(s)) => Ok(s),
        _ => Err(GameError::Storage),
    }
}
fn optional_number(row: &super::Row, n: usize) -> Result<Option<i64>, GameError> {
    if row.get(n) == Some(&SqlValue::Null) {
        Ok(None)
    } else {
        number(row, n).map(Some)
    }
}
fn optional_text(row: &super::Row, n: usize) -> Result<Option<&str>, GameError> {
    match row.get(n) {
        Some(SqlValue::Null) => Ok(None),
        Some(SqlValue::Text(value)) => Ok(Some(value)),
        _ => Err(GameError::Storage),
    }
}
fn add_time(now: i64, duration: i64) -> Result<i64, GameError> {
    now.checked_add(duration)
        .filter(|v| *v <= JS_SAFE_INTEGER_MAX)
        .ok_or(GameError::Storage)
}
fn authorize(authority: &GameAccountAuthority, now: i64) -> Result<(), GameError> {
    if authority.expires_at() <= now
        || !matches!(authority.role(), AccountRole::Host | AccountRole::Admin)
    {
        return Err(GameError::Unauthorized);
    }
    Ok(())
}
struct Record {
    id: GameId,
    creator: AccountId,
    creation_command: CommandId,
    fp: CreationFingerprint,
    host: AccountId,
    state: GameState,
    revision: i64,
    created: i64,
    due: Option<i64>,
    code: Option<GameCode>,
    published: Option<i64>,
    started: Option<i64>,
    ended: Option<i64>,
    cancellation_reason: Option<String>,
    winner_player: Option<PlayerId>,
    winner_alias: Option<String>,
    winner_awarded_by: Option<AccountId>,
    terminal_actor_kind: Option<String>,
    terminal_actor_id: Option<AccountId>,
    history_expires: Option<i64>,
}
impl Record {
    fn parse(r: &super::Row) -> Result<Self, GameError> {
        let record = Self {
            id: string(r, 0)?.parse().map_err(|_| GameError::Storage)?,
            creator: string(r, 1)?.parse().map_err(|_| GameError::Storage)?,
            creation_command: string(r, 2)?.parse().map_err(|_| GameError::Storage)?,
            fp: CreationFingerprint::from_digest(
                blob(r, 3)?.try_into().map_err(|_| GameError::Storage)?,
            ),
            host: string(r, 4)?.parse().map_err(|_| GameError::Storage)?,
            state: string(r, 5)?.parse().map_err(|_| GameError::Storage)?,
            revision: number(r, 6)?,
            created: number(r, 7)?,
            due: optional_number(r, 8)?,
            code: if r.get(9) == Some(&SqlValue::Null) {
                None
            } else {
                Some(string(r, 9)?.parse().map_err(|_| GameError::Storage)?)
            },
            published: optional_number(r, 10)?,
            started: optional_number(r, 11)?,
            ended: optional_number(r, 12)?,
            cancellation_reason: optional_text(r, 13)?.map(str::to_owned),
            winner_player: optional_text(r, 14)?
                .map(str::parse)
                .transpose()
                .map_err(|_| GameError::Storage)?,
            winner_alias: optional_text(r, 15)?.map(str::to_owned),
            winner_awarded_by: optional_text(r, 16)?
                .map(str::parse)
                .transpose()
                .map_err(|_| GameError::Storage)?,
            terminal_actor_kind: optional_text(r, 17)?.map(str::to_owned),
            terminal_actor_id: optional_text(r, 18)?
                .map(str::parse)
                .transpose()
                .map_err(|_| GameError::Storage)?,
            history_expires: optional_number(r, 19)?,
        };
        record.validate_terminal()?;
        Ok(record)
    }
    fn validate_terminal(&self) -> Result<(), GameError> {
        let winner_fields = (
            self.winner_player.is_some(),
            self.winner_alias.is_some(),
            self.winner_awarded_by.is_some(),
        );
        let valid = match self.state {
            GameState::Resolved => {
                winner_fields == (true, true, true)
                    && self.started.is_some()
                    && self.ended.is_some()
                    && self
                        .history_expires
                        .is_some_and(|expires| self.ended.is_some_and(|end| expires > end))
                    && self.cancellation_reason.is_none()
                    && self.terminal_actor_kind.as_deref()
                        == Some(crate::game::TERMINAL_ACTOR_ACCOUNT)
                    && self.terminal_actor_id.is_some()
                    && self.winner_awarded_by == self.terminal_actor_id
            }
            GameState::Cancelled => {
                let no_winner = winner_fields == (false, false, false);
                let operator = self.cancellation_reason.as_deref()
                    == Some(crate::game::OPERATOR_CANCEL_REASON)
                    && self.terminal_actor_kind.as_deref()
                        == Some(crate::game::TERMINAL_ACTOR_ACCOUNT)
                    && self.terminal_actor_id.is_some();
                let idle = self.cancellation_reason.as_deref()
                    == Some(crate::game::HOST_IDLE_REASON)
                    && self.terminal_actor_kind.as_deref()
                        == Some(crate::game::TERMINAL_ACTOR_SYSTEM)
                    && self.terminal_actor_id.is_none();
                let history_matches_start = match (self.started, self.ended, self.history_expires) {
                    (None, Some(_), None) => true,
                    (Some(start), Some(end), Some(expires)) => start <= end && expires > end,
                    _ => false,
                };
                no_winner && (operator || idle) && history_matches_start
            }
            _ => {
                winner_fields == (false, false, false)
                    && self.cancellation_reason.is_none()
                    && self.terminal_actor_kind.is_none()
                    && self.terminal_actor_id.is_none()
                    && self.history_expires.is_none()
                    && self.ended.is_none()
            }
        };
        if valid {
            Ok(())
        } else {
            Err(GameError::Storage)
        }
    }
    fn projection(&self) -> Result<GameProjection, GameError> {
        Ok(GameProjection::new(
            self.id,
            self.host,
            self.fp,
            self.state,
            SourceRevision::try_from(self.revision).map_err(|_| GameError::Storage)?,
            self.code.clone(),
            self.created,
            self.started,
            self.ended,
            self.history_expires,
        ))
    }
    fn summary(&self, revision: u64) -> GameSummary {
        GameSummary {
            game_id: self.id,
            state: self.state,
            designated_host_id: self.host,
            created_at: self.created,
            view_revision: revision,
        }
    }
}
fn revision(value: u64) -> Result<i64, GameError> {
    if value > JS_SAFE_INTEGER_MAX as u64 {
        Err(GameError::InvalidInput)
    } else {
        Ok(value as i64)
    }
}
fn advance(value: i64) -> Result<i64, GameError> {
    value
        .checked_add(1)
        .filter(|v| *v <= JS_SAFE_INTEGER_MAX)
        .ok_or(GameError::Storage)
}
fn authorize_mutation(a: &GameAccountAuthority, r: &Record, now: i64) -> Result<(), GameError> {
    authorize(a, now)?;
    if a.account_id() != r.host && a.role() != AccountRole::Admin {
        return Err(GameError::Forbidden);
    }
    Ok(())
}
fn ensure_before_idle(r: &Record, now: i64) -> Result<(), GameError> {
    if r.due.is_some_and(|due| now >= due) {
        return Err(GameError::Expired);
    }
    Ok(())
}
fn command_fingerprint(operation: &str, expected: i64) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"brews-game-command-v1\0");
    h.update(operation.as_bytes());
    h.update([0]);
    h.update(expected.to_be_bytes());
    h.finalize().into()
}
fn make_id(runtime: &impl Runtime, now: i64) -> Result<uuid::Uuid, GameError> {
    if !(1..=0xffff_ffff_ffff).contains(&now) {
        return Err(GameError::Storage);
    }
    let mut b = [0; 16];
    runtime
        .fill_random(&mut b)
        .map_err(|_| GameError::RandomUnavailable)?;
    b[..6].copy_from_slice(&(now as u64).to_be_bytes()[2..]);
    b[6] = (b[6] & 0x0f) | 0x70;
    b[8] = (b[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(b))
}
fn parse_work(r: &super::Row) -> Result<PendingWork, GameError> {
    Ok(PendingWork {
        operation: string(r, 0)?.parse().map_err(|_| GameError::Storage)?,
        kind: string(r, 1)?.parse()?,
        phase: string(r, 2)?.parse()?,
        actor: string(r, 3)?.to_owned(),
        command: string(r, 4)?.parse().map_err(|_| GameError::Storage)?,
        fingerprint: blob(r, 5)?.try_into().map_err(|_| GameError::Storage)?,
        expected: number(r, 6)?,
        fence: number(r, 7)?,
        created: number(r, 8)?,
        next: number(r, 9)?,
        attempts: u32::try_from(number(r, 10)?).map_err(|_| GameError::Storage)?,
    })
}
fn same_work_target(a: &PendingWork, b: &PendingWork) -> bool {
    a.operation == b.operation
        && a.kind == b.kind
        && a.actor == b.actor
        && a.command == b.command
        && a.fingerprint == b.fingerprint
        && a.expected == b.expected
        && a.created == b.created
}
struct Admission {
    player: PlayerId,
    expires: i64,
    consumed: Option<CommandId>,
}
struct PlayerSession {
    session: SessionId,
    player: PlayerId,
    epoch: i64,
    expires: i64,
}
fn require_admission(r: &Record, code: &GameCode, now: i64) -> Result<(), GameError> {
    ensure_before_idle(r, now)?;
    if r.state != GameState::AwaitingPlayers
        || r.published.is_none()
        || r.code.as_ref() != Some(code)
    {
        return Err(GameError::Conflict);
    }
    Ok(())
}
fn join_fingerprint(
    code: &GameCode,
    alias: &str,
    answer: Option<&str>,
    key: &[u8],
) -> Result<[u8; 32], GameError> {
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"brews-game-join-v1\0");
    h.update(code.as_str().as_bytes());
    h.update((alias.len() as u16).to_be_bytes());
    h.update(alias.as_bytes());
    if let Some(answer) = answer {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| GameError::Storage)?;
        mac.update(b"brews-game-join-answer-profile1\0");
        mac.update(code.as_str().as_bytes());
        mac.update(&(answer.len() as u32).to_be_bytes());
        mac.update(answer.as_bytes());
        h.update([1]);
        h.update(mac.finalize().into_bytes());
    } else {
        h.update([0]);
    }
    Ok(h.finalize().into())
}
fn parse_connection(row: &super::Row, game: GameId) -> Result<ConnectionGrant, GameError> {
    Ok(ConnectionGrant {
        game,
        session: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
        player: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
        connection: string(row, 2)?.parse().map_err(|_| GameError::Storage)?,
        epoch: number(row, 3)?,
        expires: number(row, 4)?,
    })
}
fn line_parts(line: &CompletedLine) -> (&'static str, Option<i64>) {
    match line {
        CompletedLine::Row(i) => (LINE_ROW, Some(i64::from(*i))),
        CompletedLine::Column(i) => (LINE_COLUMN, Some(i64::from(*i))),
        CompletedLine::MainDiagonal => (LINE_MAIN, None),
        CompletedLine::AntiDiagonal => (LINE_ANTI, None),
    }
}
fn parse_line(row: &super::Row) -> Result<CompletedLine, GameError> {
    let index = || u8::try_from(number(row, 1)?).map_err(|_| GameError::Storage);
    match string(row, 0)? {
        LINE_ROW => Ok(CompletedLine::Row(index()?)),
        LINE_COLUMN => Ok(CompletedLine::Column(index()?)),
        LINE_MAIN if row.get(1) == Some(&SqlValue::Null) => Ok(CompletedLine::MainDiagonal),
        LINE_ANTI if row.get(1) == Some(&SqlValue::Null) => Ok(CompletedLine::AntiDiagonal),
        _ => Err(GameError::Storage),
    }
}
fn inventory<D: Database>(db: &D) -> Result<Vec<String>, StorageError> {
    db.query(
        SQL_SELECT_SQLITE_MASTER_NAME,
        &[
            SqlValue::Text("__miniflare_do_name".into()),
            SqlValue::Text("_cf_METADATA".into()),
            SqlValue::Integer((GAME_TABLES.len() + DELIVERY_TABLES.len() + 1) as i64),
        ],
    )?
    .into_iter()
    .map(|r| match r.as_slice() {
        [SqlValue::Text(s)] => Ok(s.clone()),
        _ => Err(StorageError),
    })
    .collect()
}
fn validate_schema<D: Database>(db: &D) -> Result<(), StorageError> {
    let actual = inventory(db)?;
    let core = GAME_TABLES
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut complete = core.clone();
    complete.extend(DELIVERY_TABLES.map(str::to_owned));
    complete.sort();
    if actual != core && actual != complete {
        return Err(StorageError);
    }
    if db.query(SQL_SELECT_GAME_METADATA_SCHEMA_VERSION, &[])?
        != vec![vec![SqlValue::Integer(GAME_SCHEMA_VERSION)]]
    {
        return Err(StorageError);
    }
    Ok(())
}
fn schema_statement_for(table: &str) -> Result<String, StorageError> {
    let prefix = format!("CREATE TABLE {table}(");
    game_schema::statements()
        .into_iter()
        .find(|statement| statement.starts_with(&prefix))
        .ok_or(StorageError)
}
fn migration_table_set(tables: &[&str], include_delivery: bool) -> Vec<String> {
    let mut names = tables
        .iter()
        .map(|table| (*table).to_owned())
        .collect::<Vec<_>>();
    if include_delivery {
        names.extend(DELIVERY_TABLES.map(str::to_owned));
    }
    names.sort();
    names
}
fn migrate_previous_game_schema<D: Database>(db: &D) -> Result<(), StorageError> {
    let previous_record = db.query(SQL_SELECT_GAME_RECORD_FOR_SCHEMA_MIGRATION, &[])?;
    if previous_record.len() > 1 {
        return Err(StorageError);
    }
    let configuration = db.query(
        SQL_SELECT_GAME_CONFIGURATION_NUMERIC_UPPER_BOUND_BOARD_SIDE_LENGTH_FREE_CELLS_ENABLED,
        &[],
    )?;
    if configuration.len() > 1 {
        return Err(StorageError);
    }
    let free_cells = db.query(SQL_SELECT_GAME_FREE_CELLS_ROW_COLUMN, &[])?;
    if configuration.is_empty() && !free_cells.is_empty() {
        return Err(StorageError);
    }

    let ddl = schema_statement_for("game_record")?.replacen(
        "CREATE TABLE game_record(",
        "CREATE TABLE game_record_schema_upgrade(",
        1,
    );
    db.execute(&ddl, &[])?;
    db.execute(
        SQL_COPY_GAME_RECORD_TO_SCHEMA_MIGRATION,
        &[
            text(GameState::Cancelled),
            text(crate::game::HOST_IDLE_REASON),
            text(crate::game::TERMINAL_ACTOR_SYSTEM),
        ],
    )?;
    let copied_record = db.query(SQL_SELECT_GAME_RECORD_SCHEMA_UPGRADE, &[])?;
    match previous_record.as_slice() {
        [] if copied_record.is_empty() => {}
        [old] => {
            let mut expected = old.clone();
            expected.extend([
                SqlValue::Null,
                SqlValue::Null,
                SqlValue::Null,
                if old.get(7) == Some(&text(GameState::Cancelled))
                    && old.get(13) == Some(&SqlValue::Null)
                    && old.get(17) == Some(&text(crate::game::HOST_IDLE_REASON))
                {
                    text(crate::game::TERMINAL_ACTOR_SYSTEM)
                } else {
                    SqlValue::Null
                },
                SqlValue::Null,
                SqlValue::Null,
            ]);
            if copied_record != vec![expected] {
                return Err(StorageError);
            }
        }
        _ => return Err(StorageError),
    }

    db.execute(SQL_DELETE_GAME_CONFIGURATION_FOR_SCHEMA_MIGRATION, &[])?;
    if !db
        .query(SQL_SELECT_GAME_CONFIGURATION_AFTER_SCHEMA_MIGRATION, &[])?
        .is_empty()
    {
        return Err(StorageError);
    }
    db.execute(SQL_DROP_PREVIOUS_GAME_RECORD_FOR_SCHEMA_MIGRATION, &[])?;
    if !db
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='game_record'",
            &[],
        )?
        .is_empty()
    {
        return Err(StorageError);
    }
    db.execute(SQL_RENAME_GAME_RECORD_AFTER_SCHEMA_MIGRATION, &[])?;
    if db.query(SQL_SELECT_GAME_RECORD_FOR_SCHEMA_MIGRATION, &[])? != previous_record {
        return Err(StorageError);
    }
    if let [row] = configuration.as_slice() {
        db.execute(SQL_INSERT_GAME_CONFIGURATION, row)?;
        if db.query(SQL_SELECT_GAME_CONFIGURATION_AFTER_SCHEMA_MIGRATION, &[])?
            != vec![
                std::iter::once(SqlValue::Integer(1))
                    .chain(row.iter().cloned())
                    .collect::<Vec<_>>(),
            ]
        {
            return Err(StorageError);
        }
        for cell in &free_cells {
            db.execute(SQL_INSERT_GAME_FREE_CELLS, cell)?;
        }
        if db.query(SQL_SELECT_GAME_FREE_CELLS_ROW_COLUMN, &[])? != free_cells {
            return Err(StorageError);
        }
    }

    let old_boards = db.query(SQL_SELECT_GAME_BOARDS_FOR_SCHEMA_MIGRATION, &[])?;
    let old_cells = db.query(
        "SELECT player_id,row,column,kind,value,is_matched FROM game_board_cells ORDER BY player_id,row,column",
        &[],
    )?;
    db.execute(SQL_RENAME_GAME_BOARD_CELLS_FOR_SCHEMA_MIGRATION, &[])?;
    db.execute(SQL_RENAME_GAME_BOARDS_FOR_SCHEMA_MIGRATION, &[])?;
    db.execute(&schema_statement_for("game_boards")?, &[])?;
    db.execute(SQL_COPY_GAME_BOARDS_TO_SCHEMA_MIGRATION, &[])?;
    if db.query(SQL_SELECT_GAME_BOARDS_FOR_SCHEMA_MIGRATION, &[])? != old_boards
        || db.query(SQL_SELECT_PREVIOUS_GAME_BOARDS, &[])? != old_boards
    {
        return Err(StorageError);
    }
    let cells_ddl = schema_statement_for("game_board_cells")?.replacen(
        "CREATE TABLE game_board_cells(",
        "CREATE TABLE game_board_cells_schema_upgrade(",
        1,
    );
    db.execute(&cells_ddl, &[])?;
    db.execute(SQL_COPY_GAME_BOARD_CELLS_TO_SCHEMA_MIGRATION, &[])?;
    let copied_cells = db.query(
        "SELECT player_id,row,column,kind,value,is_matched FROM game_board_cells_schema_upgrade ORDER BY player_id,row,column",
        &[],
    )?;
    if copied_cells != old_cells {
        return Err(StorageError);
    }
    db.execute(SQL_DROP_PREVIOUS_GAME_BOARD_CELLS_FOR_SCHEMA_MIGRATION, &[])?;
    db.execute(SQL_DROP_PREVIOUS_GAME_BOARDS_FOR_SCHEMA_MIGRATION, &[])?;
    db.execute(SQL_RENAME_GAME_BOARD_CELLS_AFTER_SCHEMA_MIGRATION, &[])?;
    if db.query(
        "SELECT player_id,row,column,kind,value,is_matched FROM game_board_cells ORDER BY player_id,row,column",
        &[],
    )? != old_cells
    {
        return Err(StorageError);
    }

    let statements = game_schema::statements();
    for statement in statements.iter().filter(|statement| {
        GAME_TABLES.iter().any(|table| {
            !PREVIOUS_GAME_TABLES.contains(table)
                && statement.starts_with(&format!("CREATE TABLE {table}("))
        })
    }) {
        db.execute(statement, &[])?;
        let name = GAME_TABLES
            .iter()
            .find(|table| statement.starts_with(&format!("CREATE TABLE {table}(")))
            .ok_or(StorageError)?;
        if db.query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
            &[text(name)],
        )? != vec![vec![text(name)]]
        {
            return Err(StorageError);
        }
    }
    db.execute(
        SQL_UPDATE_GAME_SCHEMA_VERSION,
        &[int(GAME_SCHEMA_VERSION), int(PREVIOUS_GAME_SCHEMA_VERSION)],
    )?;
    if db.query(SQL_SELECT_GAME_METADATA_SCHEMA_VERSION, &[])?
        != vec![vec![int(GAME_SCHEMA_VERSION)]]
    {
        return Err(StorageError);
    }
    Ok(())
}
pub fn migrate_game<D: Database>(db: &D) -> Result<(), GameError> {
    db.transaction(|| {
        let actual = inventory(db)?;
        if actual.is_empty() {
            for sql in game_schema::statements() {
                db.execute(&sql, &[])?;
            }
            db.execute(
                SQL_INSERT_GAME_METADATA,
                &[SqlValue::Integer(GAME_SCHEMA_VERSION)],
            )?;
        } else {
            let version = db.query(SQL_SELECT_GAME_METADATA_SCHEMA_VERSION, &[])?;
            let version = match version.as_slice() {
                [row] => number(row, 0).map_err(|_| StorageError)?,
                _ => return Err(StorageError),
            };
            if version == PREVIOUS_GAME_SCHEMA_VERSION {
                let previous = migration_table_set(&PREVIOUS_GAME_TABLES, false);
                let previous_with_delivery = migration_table_set(&PREVIOUS_GAME_TABLES, true);
                if actual != previous && actual != previous_with_delivery {
                    return Err(StorageError);
                }
                migrate_previous_game_schema(db)?;
            } else if version != GAME_SCHEMA_VERSION {
                return Err(StorageError);
            }
        }
        validate_schema(db)
    })
    .map_err(|_| GameError::Storage)
}
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real SQLite fixtures fail fast."
)]
mod tests {
    use super::*;
    use crate::auth::{
        GameSocketCloseWork,
        test_support::{Sqlite, TestRuntime},
    };
    use crate::db::directory::{DirectoryService, migrate_directory};

    fn id<T: std::str::FromStr>(rt: &TestRuntime, n: u8) -> T
    where
        T::Err: std::fmt::Debug,
    {
        let mut bytes = [0; 16];
        bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        bytes[15] = n;
        uuid::Uuid::from_bytes(bytes).to_string().parse().unwrap()
    }
    fn authority(rt: &TestRuntime, session: u8) -> GameAccountAuthority {
        GameAccountAuthority::from_trusted_peer(
            id(rt, 1),
            id(rt, session),
            AccountRole::Host,
            0,
            rt.now.get() + 100_000,
        )
        .unwrap()
    }
    fn initialized(db: &Sqlite, rt: &TestRuntime) {
        migrate_game(db).unwrap();
        let directory = Sqlite::new();
        migrate_directory(&directory).unwrap();
        let config = GameConfiguration::default();
        let work = DirectoryService::new(&directory, rt)
            .unwrap()
            .claim_game(
                id(rt, 1),
                id(rt, 3),
                crate::game::creation_fingerprint(&config).unwrap(),
            )
            .unwrap();
        GameService::new(db, rt, AuthPolicy::default(), &[9; 32])
            .unwrap()
            .initialize(&work, authority(rt, 2), &config)
            .unwrap();
    }
    fn close_work(rt: &TestRuntime, grant: &AccountConnectionGrant) -> GameSocketCloseWork {
        GameSocketCloseWork::from_trusted_peer(
            grant.account,
            grant.session,
            grant.game,
            grant.connection,
            id(rt, 6),
            grant.epoch,
            grant.expires,
            rt.now.get(),
        )
        .unwrap()
    }
    fn rows(db: &Sqlite) -> Vec<Vec<super::super::Row>> {
        GAME_TABLES
            .iter()
            .map(|table| {
                db.query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
                    .unwrap()
            })
            .collect()
    }
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
    #[test]
    fn forward_schema_migration_accepts_legitimate_prestart_idle_cancellation_and_pending_release()
    {
        let db = Sqlite::new();
        db.conn
            .borrow()
            .execute_batch(include_str!("../../tests/fixtures/game-schema-v1.sql"))
            .unwrap();
        db.conn.borrow().execute_batch("INSERT INTO game_record(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,ended_at,cancellation_reason) VALUES(1,'01890f3e-53b7-7d28-9b05-4f65092d5701','01890f3e-53b7-7d28-9b05-4f65092d5702','01890f3e-53b7-7d28-9b05-4f65092d5703',zeroblob(32),'01890f3e-53b7-7d28-9b05-4f65092d5702',0,'cancelled',1,1000,1000,86401000,'host_idle_timeout'); INSERT INTO game_view_revisions VALUES('host',1); INSERT INTO game_pending_work VALUES('01890f3e-53b7-7d28-9b05-4f65092d5703','release','awaiting_acknowledgement','system','01890f3e-53b7-7d28-9b05-4f65092d5703',zeroblob(32),0,1,86401000,86402000,0);").unwrap();
        migrate_game(&db).unwrap();
        let rt = TestRuntime::new();
        let service = GameService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let record = service.record_optional().unwrap().unwrap();
        assert_eq!(record.state, GameState::Cancelled);
        assert_eq!(
            record.terminal_actor_kind.as_deref(),
            Some(crate::game::TERMINAL_ACTOR_SYSTEM)
        );
        assert!(record.terminal_actor_id.is_none());
        assert!(record.history_expires.is_none());
        assert_eq!(
            service.pending_work(1).unwrap()[0].kind(),
            WorkKind::Release
        );
        assert!(service.cleanup().unwrap().is_some());
        assert!(
            db.query("SELECT 1 FROM game_history", &[])
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn forward_schema_migration_preserves_existing_game_configuration_and_board() {
        let db = Sqlite::new();
        db.conn
            .borrow()
            .execute_batch(include_str!("../../tests/fixtures/game-schema-v1.sql"))
            .unwrap();
        db.conn.borrow().execute_batch("INSERT INTO game_record(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due,lobby_opened_at,started_at,ended_at,game_code,published_at,cancellation_reason) VALUES(1,'01890f3e-53b7-7d28-9b05-4f65092d5701','01890f3e-53b7-7d28-9b05-4f65092d5702','01890f3e-53b7-7d28-9b05-4f65092d5703',zeroblob(32),'01890f3e-53b7-7d28-9b05-4f65092d5704',0,'in_progress',4,1000,2000,NULL,1001,1002,NULL,'ABCDEFGH',1001,NULL); INSERT INTO game_configuration VALUES(1,4,2,0,2,0,'single_line'); INSERT INTO game_players VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705','Player',1002,0,NULL); INSERT INTO game_boards VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705',2,1002,0); INSERT INTO game_board_cells VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705',1,1,'value','1',1),('01890f3e-53b7-7d28-9b05-4f65092d5705',1,2,'value','2',0),('01890f3e-53b7-7d28-9b05-4f65092d5705',2,1,'value','3',0),('01890f3e-53b7-7d28-9b05-4f65092d5705',2,2,'value','4',0); INSERT INTO game_view_revisions VALUES('host',2),('01890f3e-53b7-7d28-9b05-4f65092d5705',1);").unwrap();

        migrate_game(&db).unwrap();
        assert_eq!(
            db.query(SQL_SELECT_GAME_METADATA_SCHEMA_VERSION, &[])
                .unwrap(),
            vec![vec![int(GAME_SCHEMA_VERSION)]]
        );
        assert_eq!(
            db.query(SQL_SELECT_GAME_CONFIGURATION_AFTER_SCHEMA_MIGRATION, &[])
                .unwrap(),
            vec![vec![
                int(1),
                int(4),
                int(2),
                int(0),
                int(2),
                int(0),
                text("single_line")
            ]]
        );
        assert_eq!(
            db.query(
                SQL_SELECT_GAME_BOARD_CELLS_ROW_COLUMN_KIND,
                &[text("01890f3e-53b7-7d28-9b05-4f65092d5705")]
            )
            .unwrap(),
            vec![
                vec![int(1), int(1), text(CELL_VALUE), text("1"), int(1)],
                vec![int(1), int(2), text(CELL_VALUE), text("2"), int(0)],
                vec![int(2), int(1), text(CELL_VALUE), text("3"), int(0)],
                vec![int(2), int(2), text(CELL_VALUE), text("4"), int(0)],
            ]
        );
        let runtime = TestRuntime::new();
        let service = GameService::new(&db, &runtime, AuthPolicy::default(), &[9; 32]).unwrap();
        let record = service.record().unwrap();
        let player = "01890f3e-53b7-7d28-9b05-4f65092d5705"
            .parse::<PlayerId>()
            .unwrap();
        assert_eq!(record.state, GameState::InProgress);
        assert_eq!(
            service
                .board_view(player, &record)
                .unwrap()
                .unwrap()
                .cells
                .len(),
            4
        );
        assert_eq!(
            db.query("SELECT count(*) FROM game_history", &[]).unwrap(),
            vec![vec![int(0)]]
        );
    }
    #[test]
    fn gameplay_schema_supports_resolved_games_and_unique_ordered_calls() {
        let db = Sqlite::new();
        migrate_game(&db).unwrap();
        let state_ddl = db
            .query(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='game_record'",
                &[],
            )
            .unwrap();
        let state_ddl = string(state_ddl.first().unwrap(), 0).unwrap();
        assert!(state_ddl.contains("'resolved'"));

        let call_ddl = db
            .query(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='game_calls'",
                &[],
            )
            .unwrap();
        assert_eq!(call_ddl.len(), 1);
        assert!(
            string(call_ddl.first().unwrap(), 0)
                .unwrap()
                .contains("UNIQUE(game_id,value)")
        );
        db.conn.borrow().execute_batch("INSERT INTO game_record(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity) VALUES(1,'game-test','creator','create',zeroblob(32),'host',0,'new',0,1,1); INSERT INTO game_calls(game_id,sequence_no,value,mode,called_by_account_id,command_id,called_at) VALUES('game-test',1,'1','manual','host','command-one',1); INSERT INTO game_calls(game_id,sequence_no,value,mode,called_by_account_id,command_id,called_at) VALUES('game-test',2,'1','random','host','command-two',2);").unwrap_err();
        let ordered = db
            .query(
                "SELECT sequence_no,value FROM game_calls ORDER BY sequence_no",
                &[],
            )
            .unwrap();
        assert_eq!(
            ordered,
            vec![vec![SqlValue::Integer(1), SqlValue::Text("1".into())]]
        );
    }
    #[test]
    fn prepared_account_close_requires_durable_deletion_before_success() {
        for fault in ["IGNORE", "ABORT,'fault'"] {
            for accepted in [false, true] {
                let db = Sqlite::new();
                let rt = TestRuntime::new();
                initialized(&db, &rt);
                let svc = GameService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
                let grant = svc
                    .prepare_account_connection(authority(&rt, 2), id(&rt, 5))
                    .unwrap();
                if accepted {
                    svc.accept_account_connection(&grant, authority(&rt, 2))
                        .unwrap();
                    svc.prepare_account_connection(authority(&rt, 2), grant.connection)
                        .unwrap();
                }
                let unrelated = svc
                    .prepare_account_connection(authority(&rt, 7), id(&rt, 8))
                    .unwrap();
                let work = close_work(&rt, &grant);
                let before = rows(&db);
                db.conn.borrow().execute_batch(&format!("CREATE TRIGGER fail_close BEFORE DELETE ON game_connection_grants WHEN OLD.grant_id='{}' BEGIN SELECT RAISE({fault}); END", grant.connection)).unwrap();
                rt.now.set(rt.now.get() + 1);
                assert_eq!(svc.close_account_connection(&work), Err(GameError::Storage));
                assert_eq!(
                    rows(&db),
                    before,
                    "Failed fence must roll back all rows, clock and current binding"
                );
                db.conn
                    .borrow()
                    .execute_batch("DROP TRIGGER fail_close")
                    .unwrap();
                svc.close_account_connection(&work).unwrap();
                svc.close_account_connection(&work).unwrap();
                assert!(!svc.has_account_prepared(&grant).unwrap());
                assert!(
                    svc.current_account_connection(grant.session)
                        .unwrap()
                        .is_none()
                );
                let fresh = GameAccountAuthority::from_trusted_peer(
                    grant.account,
                    grant.session,
                    AccountRole::Host,
                    grant.epoch,
                    grant.expires,
                )
                .unwrap();
                assert_eq!(
                    svc.accept_account_connection(&grant, fresh),
                    Err(GameError::Unauthorized)
                );
                assert!(svc.has_account_prepared(&unrelated).unwrap());
            }
        }
    }
    #[test]
    fn account_acceptance_preflight_rejects_fenced_or_replaced_preparations_without_accepting() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        initialized(&db, &rt);
        let svc = GameService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let proof = authority(&rt, 2);
        let old = svc
            .prepare_account_connection(authority(&rt, 2), id(&rt, 5))
            .unwrap();
        let before = rows(&db);
        svc.validate_prepared_account_connection(&old, &proof)
            .unwrap();
        assert_eq!(rows(&db), before);
        assert!(
            svc.current_account_connection(old.session)
                .unwrap()
                .is_none()
        );
        let successor = svc
            .prepare_account_connection(authority(&rt, 2), id(&rt, 8))
            .unwrap();
        assert_eq!(
            svc.validate_prepared_account_connection(&old, &proof),
            Err(GameError::Unauthorized)
        );
        svc.validate_prepared_account_connection(&successor, &proof)
            .unwrap();
        for field in 0..6 {
            let mut stale = successor.clone();
            match field {
                0 => stale.game = id(&rt, 9),
                1 => stale.account = id(&rt, 9),
                2 => stale.session = id(&rt, 9),
                3 => stale.connection = id(&rt, 9),
                4 => stale.epoch += 1,
                _ => stale.expires += 1,
            }
            assert!(
                svc.validate_prepared_account_connection(&stale, &proof)
                    .is_err()
            );
        }
        svc.close_account_connection(&close_work(&rt, &successor))
            .unwrap();
        assert_eq!(
            svc.validate_prepared_account_connection(&successor, &proof),
            Err(GameError::Unauthorized)
        );
        assert!(
            svc.current_account_connection(successor.session)
                .unwrap()
                .is_none()
        );
        let fresh = svc
            .prepare_account_connection(authority(&rt, 2), id(&rt, 10))
            .unwrap();
        let before = rows(&db);
        svc.validate_prepared_account_connection(&fresh, &proof)
            .unwrap();
        assert_eq!(rows(&db), before);
        rt.now.set(proof.expires_at());
        assert_eq!(
            svc.validate_prepared_account_connection(&fresh, &proof),
            Err(GameError::Unauthorized)
        );
    }
    #[test]
    fn stale_close_metadata_does_not_delete_prepared_account_target() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        initialized(&db, &rt);
        let svc = GameService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let grant = svc
            .prepare_account_connection(authority(&rt, 2), id(&rt, 5))
            .unwrap();
        for field in 0..5 {
            let mut target = grant.clone();
            match field {
                0 => target.account = id(&rt, 9),
                1 => target.session = id(&rt, 9),
                2 => target.connection = id(&rt, 9),
                3 => target.epoch += 1,
                _ => target.expires += 1,
            }
            svc.close_account_connection(&close_work(&rt, &target))
                .unwrap();
            assert!(svc.has_account_prepared(&grant).unwrap());
        }
        let mut other = grant.clone();
        other.game = id(&rt, 9);
        assert_eq!(
            svc.close_account_connection(&close_work(&rt, &other)),
            Err(GameError::Conflict)
        );
        assert!(svc.has_account_prepared(&grant).unwrap());
        svc.accept_account_connection(&grant, authority(&rt, 2))
            .unwrap();
    }
}
