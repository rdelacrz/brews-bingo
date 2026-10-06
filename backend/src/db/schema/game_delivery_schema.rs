//! Normalized delivery metadata schema, independent of Game's schema counter.
use brews_contracts::games::{DELIVERY_ID_ENCODED_BYTES, GAME_FRAME_MAX_BYTES, MAX_SAFE_REVISION};
pub const DELIVERY_SCHEMA_VERSION: i64 = 1;
pub const DELIVERY_TABLES: [&str; 3] = [
    "game_delivery_connections",
    "game_delivery_metadata",
    "game_delivery_pending",
];
pub const DELIVERY_QUEUE_MAX_BYTES: usize = 1024 * 1024;
pub const DELIVERY_PENDING_MAX: usize = 64;
pub fn statements() -> [String; 5] {
    [
        format!("CREATE TABLE game_delivery_metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL CHECK(schema_version>0),last_observed_ms INTEGER NOT NULL CHECK(last_observed_ms BETWEEN 0 AND {MAX_SAFE_REVISION})) STRICT"),
        format!("CREATE TABLE game_delivery_connections(connection_id TEXT PRIMARY KEY NOT NULL CHECK(length(connection_id)=36),session_id TEXT NOT NULL CHECK(length(session_id)=36),auth_epoch INTEGER NOT NULL CHECK(auth_epoch BETWEEN 0 AND {MAX_SAFE_REVISION}),expires_at INTEGER NOT NULL CHECK(expires_at BETWEEN 1 AND {MAX_SAFE_REVISION})) STRICT"),
        format!("CREATE TABLE game_delivery_pending(delivery_id TEXT PRIMARY KEY NOT NULL CHECK(length(delivery_id)={DELIVERY_ID_ENCODED_BYTES}),connection_id TEXT NOT NULL REFERENCES game_delivery_connections(connection_id) ON DELETE CASCADE,frame_bytes INTEGER NOT NULL CHECK(frame_bytes BETWEEN 1 AND {GAME_FRAME_MAX_BYTES}),view_revision INTEGER NOT NULL CHECK(view_revision BETWEEN 0 AND {MAX_SAFE_REVISION}),issued_at INTEGER NOT NULL CHECK(issued_at BETWEEN 0 AND {MAX_SAFE_REVISION})) STRICT"),
        "CREATE INDEX game_delivery_expiry ON game_delivery_connections(expires_at,connection_id)".into(),
        "CREATE INDEX game_delivery_connection_pending ON game_delivery_pending(connection_id,delivery_id)".into(),
    ]
}
