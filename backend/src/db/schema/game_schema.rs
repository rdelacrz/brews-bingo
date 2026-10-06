//! Normalized, isolated Game-owner schema. No authoritative JSON collections.
use crate::limits::JS_SAFE_INTEGER_MAX;
use brews_domain::games::GameState;
pub const GAME_SCHEMA_VERSION: i64 = 1;
pub const GAME_TABLES: [&str; 16] = [
    "game_admission_contexts",
    "game_board_cells",
    "game_boards",
    "game_completed_lines",
    "game_configuration",
    "game_connection_grants",
    "game_current_connections",
    "game_free_cells",
    "game_metadata",
    "game_pending_work",
    "game_players",
    "game_receipts",
    "game_record",
    "game_recovery",
    "game_sessions",
    "game_view_revisions",
];
pub const DELIVERY_TABLES: [&str; 3] = [
    "game_delivery_connections",
    "game_delivery_metadata",
    "game_delivery_pending",
];
pub fn statements() -> Vec<String> {
    let max = JS_SAFE_INTEGER_MAX;
    let states = [
        GameState::New,
        GameState::AwaitingPlayers,
        GameState::InProgress,
        GameState::Cancelled,
    ]
    .map(|s| format!("'{s}'"))
    .join(",");
    let mut statements = vec![
    format!("CREATE TABLE game_metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL CHECK(schema_version>0),last_observed_ms INTEGER NOT NULL CHECK(last_observed_ms BETWEEN 0 AND {max}),command_floor_ms INTEGER NOT NULL CHECK(command_floor_ms BETWEEN 0 AND {max})) STRICT"),
    format!("CREATE TABLE game_record(singleton INTEGER PRIMARY KEY CHECK(singleton=1),game_id TEXT NOT NULL UNIQUE,creator_id TEXT NOT NULL,creation_command TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),host_id TEXT NOT NULL,host_assignment_revision INTEGER NOT NULL CHECK(host_assignment_revision BETWEEN 0 AND {max}),state TEXT NOT NULL CHECK(state IN ({states})),revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND {max}),created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND {max}),last_host_activity INTEGER NOT NULL,idle_due INTEGER,lobby_opened_at INTEGER,started_at INTEGER,ended_at INTEGER,game_code TEXT,published_at INTEGER,cancellation_reason TEXT) STRICT"),
    "CREATE TABLE game_configuration(singleton INTEGER PRIMARY KEY CHECK(singleton=1) REFERENCES game_record(singleton),numeric_upper_bound INTEGER NOT NULL CHECK(numeric_upper_bound BETWEEN 1 AND 1000),board_side_length INTEGER NOT NULL CHECK(board_side_length BETWEEN 2 AND 10),free_cells_enabled INTEGER NOT NULL CHECK(free_cells_enabled IN (0,1)),player_capacity INTEGER NOT NULL CHECK(player_capacity BETWEEN 2 AND 20),spectator_capacity INTEGER NOT NULL CHECK(spectator_capacity BETWEEN 0 AND 50),winning_pattern TEXT NOT NULL) STRICT".into(),
    "CREATE TABLE game_free_cells(row INTEGER NOT NULL CHECK(row BETWEEN 1 AND 10),column INTEGER NOT NULL CHECK(column BETWEEN 1 AND 10),PRIMARY KEY(row,column)) STRICT".into(),
    format!("CREATE TABLE game_players(player_id TEXT PRIMARY KEY,alias TEXT NOT NULL COLLATE BINARY UNIQUE CHECK(length(CAST(alias AS BLOB)) BETWEEN 1 AND 20),joined_at INTEGER NOT NULL,session_epoch INTEGER NOT NULL CHECK(session_epoch BETWEEN 0 AND {max}),last_explicit_leave_at INTEGER) STRICT"),
    "CREATE TABLE game_recovery(player_id TEXT PRIMARY KEY REFERENCES game_players(player_id),answer_verifier TEXT NOT NULL CHECK(length(answer_verifier)<=256),normalization_version INTEGER NOT NULL CHECK(normalization_version=1),updated_at INTEGER NOT NULL) STRICT".into(),
    "CREATE TABLE game_sessions(session_id TEXT PRIMARY KEY,token_verifier BLOB NOT NULL UNIQUE CHECK(length(token_verifier)=32),game_id TEXT NOT NULL,game_code TEXT NOT NULL,player_id TEXT NOT NULL REFERENCES game_players(player_id),session_epoch INTEGER NOT NULL CHECK(session_epoch>=0),access TEXT NOT NULL,issued_at INTEGER NOT NULL,expires_at INTEGER NOT NULL CHECK(expires_at>issued_at),revoked_at INTEGER) STRICT".into(),
    "CREATE TABLE game_boards(player_id TEXT PRIMARY KEY REFERENCES game_players(player_id),side_length INTEGER NOT NULL CHECK(side_length BETWEEN 2 AND 10),assigned_at INTEGER NOT NULL,evaluated_through_call INTEGER NOT NULL CHECK(evaluated_through_call=0)) STRICT".into(),
    "CREATE TABLE game_board_cells(player_id TEXT NOT NULL REFERENCES game_boards(player_id),row INTEGER NOT NULL,column INTEGER NOT NULL,kind TEXT NOT NULL,value TEXT,is_matched INTEGER NOT NULL CHECK(is_matched IN (0,1)),PRIMARY KEY(player_id,row,column)) STRICT".into(),
    "CREATE TABLE game_completed_lines(player_id TEXT NOT NULL REFERENCES game_boards(player_id),sequence INTEGER NOT NULL CHECK(sequence>=0),kind TEXT NOT NULL,line_index INTEGER,PRIMARY KEY(player_id,sequence)) STRICT".into(),
    format!("CREATE TABLE game_view_revisions(view_key TEXT PRIMARY KEY,revision INTEGER NOT NULL CHECK(revision BETWEEN 0 AND {max})) STRICT"),
    "CREATE TABLE game_current_connections(viewer_kind TEXT NOT NULL,session_id TEXT NOT NULL,principal_id TEXT NOT NULL,connection_id TEXT NOT NULL UNIQUE,epoch INTEGER NOT NULL,expires_at INTEGER NOT NULL,PRIMARY KEY(viewer_kind,session_id)) STRICT".into(),
    "CREATE TABLE game_connection_grants(grant_id TEXT PRIMARY KEY,viewer_kind TEXT NOT NULL,session_id TEXT NOT NULL,principal_id TEXT NOT NULL,connection_id TEXT,epoch INTEGER NOT NULL,entered_at INTEGER NOT NULL,expires_at INTEGER NOT NULL,UNIQUE(viewer_kind,session_id,connection_id)) STRICT".into(),
    "CREATE TABLE game_receipts(actor TEXT NOT NULL,command_id TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),outcome TEXT NOT NULL CHECK(length(CAST(outcome AS BLOB))<=4096),completed_at INTEGER NOT NULL,expires_at INTEGER NOT NULL,PRIMARY KEY(actor,command_id)) STRICT".into(),
    "CREATE TABLE game_pending_work(operation_id TEXT PRIMARY KEY,kind TEXT NOT NULL,phase TEXT NOT NULL,actor TEXT NOT NULL,command_id TEXT NOT NULL,fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),expected_revision INTEGER NOT NULL,fence_revision INTEGER NOT NULL,created_at INTEGER NOT NULL,next_attempt_at INTEGER NOT NULL,attempt_count INTEGER NOT NULL CHECK(attempt_count BETWEEN 0 AND 4294967295),UNIQUE(actor,command_id)) STRICT".into(),
    "CREATE TABLE game_admission_contexts(player_id TEXT PRIMARY KEY,token_verifier BLOB NOT NULL UNIQUE CHECK(length(token_verifier)=32),game_code TEXT NOT NULL,issued_at INTEGER NOT NULL,expires_at INTEGER NOT NULL,retain_until INTEGER NOT NULL,consumed_command TEXT) STRICT".into(),
    ];
    // Trusted unreleased DDL identifiers only; wire-visible authority uses safe integers.
    for statement in &mut statements {
        for column in [
            "session_epoch",
            "epoch",
            "expected_revision",
            "fence_revision",
            "created_at",
            "last_host_activity",
            "issued_at",
            "expires_at",
            "retain_until",
            "joined_at",
            "updated_at",
            "assigned_at",
            "entered_at",
            "completed_at",
            "next_attempt_at",
        ] {
            let source = format!(",{column} INTEGER NOT NULL");
            let bounded = format!(",{column} INTEGER NOT NULL CHECK({column} BETWEEN 0 AND {max})");
            *statement = statement.replace(&source, &bounded);
        }
        for column in [
            "idle_due",
            "lobby_opened_at",
            "started_at",
            "ended_at",
            "published_at",
            "revoked_at",
            "last_explicit_leave_at",
        ] {
            let source = format!(",{column} INTEGER");
            let bounded = format!(
                ",{column} INTEGER CHECK({column} IS NULL OR {column} BETWEEN 0 AND {max})"
            );
            *statement = statement.replace(&source, &bounded);
        }
    }
    statements
}
