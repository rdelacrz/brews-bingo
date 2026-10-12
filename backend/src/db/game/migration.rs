//! Validate and migrate the game-owner schema in one storage transaction.
use super::{Database, GameError, SqlValue, StorageError, int, number, text};
use super::{
    SQL_INSERT_GAME_CONFIGURATION,
    SQL_SELECT_GAME_CONFIGURATION_NUMERIC_UPPER_BOUND_BOARD_SIDE_LENGTH_FREE_CELLS_ENABLED,
    SQL_SELECT_GAME_FREE_CELLS_ROW_COLUMN,
};
use crate::db::schema::game_schema::{
    self, DELIVERY_TABLES, GAME_SCHEMA_VERSION, GAME_TABLES, PREVIOUS_GAME_SCHEMA_VERSION,
    PREVIOUS_GAME_TABLES,
};
use brews_domain::games::GameState;
#[path = "../schema/previous_admission_schema.rs"]
mod previous_admission_schema;
#[path = "../schema/previous_gameplay_schema.rs"]
mod previous_gameplay_schema;

const MINIFLARE_OWNER_METADATA_TABLE: &str = "__miniflare_do_name";
const CLOUDFLARE_OWNER_METADATA_TABLE: &str = "_cf_METADATA";
const SCHEMA_TABLE_INVENTORY_ROW_LIMIT: i64 =
    (GAME_TABLES.len() + DELIVERY_TABLES.len() + 1) as i64;
const SQL_SELECT_TABLE_DDL_BY_NAME: &str =
    "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
const SQL_SELECT_TABLE_DDL_CATALOG: &str = "SELECT name,sql FROM sqlite_master WHERE type='table' AND name NOT GLOB 'sqlite_*' AND name NOT IN (?,?) ORDER BY name LIMIT ?";
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

fn inventory<D: Database>(db: &D) -> Result<Vec<String>, StorageError> {
    db.query(
        SQL_SELECT_SQLITE_MASTER_NAME,
        &[
            text(MINIFLARE_OWNER_METADATA_TABLE),
            text(CLOUDFLARE_OWNER_METADATA_TABLE),
            int(SCHEMA_TABLE_INVENTORY_ROW_LIMIT),
        ],
    )?
    .into_iter()
    .map(|r| match r.as_slice() {
        [SqlValue::Text(s)] => Ok(s.clone()),
        _ => Err(StorageError),
    })
    .collect()
}
pub(super) fn validate_schema<D: Database>(db: &D) -> Result<(), StorageError> {
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
    validate_retained_schema(db, &actual.iter().map(String::as_str).collect::<Vec<_>>())?;
    for statement in game_schema::recovery_coordination_statements()
        .into_iter()
        .chain(game_schema::spectator_statements())
        .chain(crate::db::schema::game_operator_schema::statements())
    {
        let (kind, prefix) = if statement.starts_with("CREATE TABLE") {
            ("table", "CREATE TABLE ")
        } else {
            ("index", "CREATE INDEX ")
        };
        let name = statement
            .strip_prefix(prefix)
            .and_then(|rest| rest.split(['(', ' ']).next())
            .ok_or(StorageError)?;
        if db.query(
            "SELECT sql FROM sqlite_master WHERE type=? AND name=?",
            &[text(kind), text(name)],
        )? != vec![vec![text(statement)]]
        {
            return Err(StorageError);
        }
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
fn canonicalize_table_identifiers(table: &str, actual: &str, tables: &[&str]) -> String {
    // SQLite quotes renamed table and reference identifiers. Keep column and
    // constraint text, including policy string literals, unchanged.
    let mut recognized = actual.replacen(
        &format!("CREATE TABLE \"{table}\"("),
        &format!("CREATE TABLE {table}("),
        1,
    );
    for name in tables {
        recognized = recognized.replace(
            &format!("REFERENCES \"{name}\"("),
            &format!("REFERENCES {name}("),
        );
    }
    recognized
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
fn validate_previous_admission_schema<D: Database>(
    db: &D,
    tables: &[String],
) -> Result<(), StorageError> {
    let names = tables.iter().map(String::as_str).collect::<Vec<_>>();
    for table in tables {
        let prefix = format!("CREATE TABLE {table}(");
        let expected = previous_admission_schema::TABLE_STATEMENTS
            .iter()
            .find(|statement| statement.starts_with(&prefix))
            .ok_or(StorageError)?;
        let ddl = db.query(SQL_SELECT_TABLE_DDL_BY_NAME, &[text(table)])?;
        let [row] = ddl.as_slice() else {
            return Err(StorageError);
        };
        let [SqlValue::Text(actual)] = row.as_slice() else {
            return Err(StorageError);
        };
        let recognized = canonicalize_table_identifiers(table, actual, &names);
        if recognized != *expected {
            return Err(StorageError);
        }
    }
    Ok(())
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
        // Free cells are retained in their original table; reinserting them would
        // violate its primary key for a legitimate configured lobby.
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
    repair_completed_line_board_reference(db)?;
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
    for statement in game_schema::recovery_coordination_statements()
        .into_iter()
        .filter(|sql| sql.starts_with("CREATE INDEX"))
    {
        db.execute(&statement, &[])?;
    }
    for statement in game_schema::spectator_statements()
        .into_iter()
        .filter(|s| s.starts_with("CREATE INDEX"))
    {
        db.execute(&statement, &[])?;
    }
    for statement in crate::db::schema::game_operator_schema::statements()
        .into_iter()
        .filter(|s| s.starts_with("CREATE INDEX"))
    {
        db.execute(&statement, &[])?;
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
fn retained_schema_matches(table: &str, actual: &str, tables: &[&str], expected: &str) -> bool {
    if actual == expected {
        return true;
    }
    let normalized = canonicalize_table_identifiers(table, actual, tables);
    if normalized == expected {
        return true;
    }
    // Exact known forms retained by the original gameplay migration. No generic
    // constraint stripping or acceptance of unknown added columns is permitted.
    let legacy = match table {
        "game_players" => {
            let check = format!(
                " CHECK(session_epoch BETWEEN 0 AND {})",
                crate::limits::JS_SAFE_INTEGER_MAX
            );
            expected.replace(&format!("{check}{check}"), &check)
        }
        "game_sessions" => expected
            .replace(
                &format!(
                    " CHECK(session_epoch BETWEEN 0 AND {})",
                    crate::limits::JS_SAFE_INTEGER_MAX
                ),
                "",
            )
            .replace(
                &format!(
                    "CHECK(expires_at BETWEEN 0 AND {}) CHECK(expires_at>issued_at)",
                    crate::limits::JS_SAFE_INTEGER_MAX
                ),
                &format!(
                    "CHECK(expires_at BETWEEN 0 AND {} AND expires_at>issued_at)",
                    crate::limits::JS_SAFE_INTEGER_MAX
                ),
            ),
        "game_completed_lines" => expected.replace(
            "REFERENCES game_boards(",
            "REFERENCES \"game_boards_previous\"(",
        ),
        _ => return false,
    };
    normalized == legacy
}
fn validate_retained_schema<D: Database>(db: &D, tables: &[&str]) -> Result<(), StorageError> {
    let statements = game_schema::statements();
    validate_table_definitions(
        db,
        tables,
        &statements.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}
fn validate_historical_schema<D: Database>(
    db: &D,
    tables: &[&str],
    statements: &[&str],
) -> Result<(), StorageError> {
    validate_table_definitions(db, tables, statements)?;
    let mut expected = statements
        .iter()
        .filter(|s| s.starts_with("CREATE INDEX") || s.starts_with("CREATE UNIQUE INDEX"))
        .copied()
        .collect::<Vec<_>>();
    if DELIVERY_TABLES.iter().all(|t| tables.contains(t)) {
        expected.extend(previous_gameplay_schema::DELIVERY_INDEX_STATEMENTS);
    }
    let catalog = db.query("SELECT name,sql FROM sqlite_master WHERE type='index' AND sql IS NOT NULL ORDER BY name LIMIT ?", &[int(expected.len() as i64 + DELIVERY_TABLES.len() as i64 + 1)])?;
    if catalog.len() != expected.len() {
        return Err(StorageError);
    }
    for statement in expected {
        let name = statement
            .strip_prefix("CREATE INDEX ")
            .or_else(|| statement.strip_prefix("CREATE UNIQUE INDEX "))
            .and_then(|s| s.split(['(', ' ']).next())
            .ok_or(StorageError)?;
        if !catalog.iter().any(|row| matches!(row.as_slice(), [SqlValue::Text(n),SqlValue::Text(sql)] if n == name && sql.replace(&format!("\"{name}\""), name) == statement)) { return Err(StorageError); }
    }
    Ok(())
}
fn validate_table_definitions<D: Database>(
    db: &D,
    tables: &[&str],
    statements: &[&str],
) -> Result<(), StorageError> {
    let catalog = db.query(
        SQL_SELECT_TABLE_DDL_CATALOG,
        &[
            text(MINIFLARE_OWNER_METADATA_TABLE),
            text(CLOUDFLARE_OWNER_METADATA_TABLE),
            int(SCHEMA_TABLE_INVENTORY_ROW_LIMIT),
        ],
    )?;
    if catalog.len() != tables.len() {
        return Err(StorageError);
    }
    for table in tables {
        let prefix = format!("CREATE TABLE {table}(");
        // Optional delivery tables retain the already-audited frozen DDL; their
        // schema counter and transport indexes remain owned by Delivery.
        let expected = if DELIVERY_TABLES.contains(table) {
            previous_admission_schema::TABLE_STATEMENTS
                .iter()
                .copied()
                .find(|statement| statement.starts_with(&prefix))
        } else {
            statements
                .iter()
                .copied()
                .find(|statement| statement.starts_with(&prefix))
        }
        .ok_or(StorageError)?;
        let mut matching = catalog
            .iter()
            .filter(|row| matches!(row.first(), Some(SqlValue::Text(name)) if name == *table));
        let row = matching.next().ok_or(StorageError)?;
        if matching.next().is_some() {
            return Err(StorageError);
        }
        let [SqlValue::Text(_), SqlValue::Text(actual)] = row.as_slice() else {
            return Err(StorageError);
        };
        if !retained_schema_matches(table, actual, tables, expected) {
            return Err(StorageError);
        }
    }
    Ok(())
}
const COMPLETED_LINE_MIGRATION_LIMIT: i64 = (brews_domain::games::PLAYER_CAPACITY_MAX as i64)
    * (2 * brews_domain::games::BOARD_SIDE_MAX as i64 + 2);
const SQL_SELECT_COMPLETED_LINES_FOR_REFERENCE_MIGRATION: &str =
    "SELECT * FROM game_completed_lines ORDER BY player_id,sequence LIMIT ?";
const SQL_SELECT_COPIED_COMPLETED_LINES_FOR_REFERENCE_MIGRATION: &str =
    "SELECT * FROM game_completed_lines_reference_upgrade ORDER BY player_id,sequence LIMIT ?";
fn repair_completed_line_board_reference<D: Database>(db: &D) -> Result<(), StorageError> {
    let ddl = db.query(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='game_completed_lines'",
        &[],
    )?;
    let [row] = ddl.as_slice() else {
        return Err(StorageError);
    };
    let [SqlValue::Text(sql)] = row.as_slice() else {
        return Err(StorageError);
    };
    if !sql.contains("game_boards_previous") {
        return Ok(());
    }
    let original = db.query(
        SQL_SELECT_COMPLETED_LINES_FOR_REFERENCE_MIGRATION,
        &[int(COMPLETED_LINE_MIGRATION_LIMIT + 1)],
    )?;
    if original.len() as i64 > COMPLETED_LINE_MIGRATION_LIMIT {
        return Err(StorageError);
    }
    db.execute(
        &schema_statement_for("game_completed_lines")?.replacen(
            "CREATE TABLE game_completed_lines(",
            "CREATE TABLE game_completed_lines_reference_upgrade(",
            1,
        ),
        &[],
    )?;
    db.execute(
        "INSERT INTO game_completed_lines_reference_upgrade SELECT * FROM game_completed_lines",
        &[],
    )?;
    if db.query(
        SQL_SELECT_COPIED_COMPLETED_LINES_FOR_REFERENCE_MIGRATION,
        &[int(COMPLETED_LINE_MIGRATION_LIMIT + 1)],
    )? != original
    {
        return Err(StorageError);
    }
    db.execute("DROP TABLE game_completed_lines", &[])?;
    db.execute(
        "ALTER TABLE game_completed_lines_reference_upgrade RENAME TO game_completed_lines",
        &[],
    )?;
    if db.query(
        SQL_SELECT_COMPLETED_LINES_FOR_REFERENCE_MIGRATION,
        &[int(COMPLETED_LINE_MIGRATION_LIMIT + 1)],
    )? != original
    {
        return Err(StorageError);
    }
    Ok(())
}
fn migrate_recovery_coordination<D: Database>(db: &D) -> Result<(), StorageError> {
    let old = previous_gameplay_schema::GAMEPLAY_TABLES;
    let actual = inventory(db)?;
    if actual != migration_table_set(&old, false) && actual != migration_table_set(&old, true) {
        return Err(StorageError);
    }
    validate_historical_schema(
        db,
        &actual.iter().map(String::as_str).collect::<Vec<_>>(),
        &previous_gameplay_schema::GAMEPLAY_STATEMENTS,
    )?;
    repair_completed_line_board_reference(db)?;
    for statement in game_schema::recovery_coordination_statements() {
        db.execute(&statement, &[])?;
    }
    db.execute(
        SQL_UPDATE_GAME_SCHEMA_VERSION,
        &[int(GAME_SCHEMA_VERSION), int(2)],
    )?;
    for statement in game_schema::spectator_statements() {
        db.execute(&statement, &[])?;
    }
    for sql in crate::db::schema::game_operator_schema::statements() {
        db.execute(&sql, &[])?;
    }
    validate_schema(db)?;
    for name in ["game_recovery_bucket_expiry", "game_socket_closure_retry"] {
        if db.query(
            "SELECT name FROM sqlite_master WHERE type='index' AND name=?",
            &[text(name)],
        )? != vec![vec![text(name)]]
        {
            return Err(StorageError);
        }
    }
    Ok(())
}
fn migrate_spectator_authority<D: Database>(db: &D) -> Result<(), StorageError> {
    let old = previous_gameplay_schema::RECOVERY_TABLES;
    let actual = inventory(db)?;
    if actual != migration_table_set(&old, false) && actual != migration_table_set(&old, true) {
        return Err(StorageError);
    }
    validate_historical_schema(
        db,
        &actual.iter().map(String::as_str).collect::<Vec<_>>(),
        &previous_gameplay_schema::RECOVERY_STATEMENTS,
    )?;
    repair_completed_line_board_reference(db)?;
    for sql in game_schema::spectator_statements() {
        db.execute(&sql, &[])?;
    }
    db.execute(
        SQL_UPDATE_GAME_SCHEMA_VERSION,
        &[int(GAME_SCHEMA_VERSION), int(3)],
    )?;
    for sql in crate::db::schema::game_operator_schema::statements() {
        db.execute(&sql, &[])?;
    }
    validate_schema(db)
}
fn migrate_saved_operator_workflows<D: Database>(db: &D) -> Result<(), StorageError> {
    let old = previous_gameplay_schema::SPECTATOR_TABLES;
    let actual = inventory(db)?;
    if actual != migration_table_set(&old, false) && actual != migration_table_set(&old, true) {
        return Err(StorageError);
    }
    validate_historical_schema(
        db,
        &actual.iter().map(String::as_str).collect::<Vec<_>>(),
        &previous_gameplay_schema::SPECTATOR_STATEMENTS,
    )?;
    for statement in game_schema::recovery_coordination_statements()
        .into_iter()
        .chain(game_schema::spectator_statements())
        .filter(|s| s.starts_with("CREATE INDEX"))
    {
        let name = statement
            .strip_prefix("CREATE INDEX ")
            .and_then(|s| s.split(['(', ' ']).next())
            .ok_or(StorageError)?;
        if db.query(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name=?",
            &[text(name)],
        )? != vec![vec![text(&statement)]]
        {
            return Err(StorageError);
        }
    }
    for sql in crate::db::schema::game_operator_schema::statements() {
        db.execute(&sql, &[])?;
    }
    db.execute(
        SQL_UPDATE_GAME_SCHEMA_VERSION,
        &[int(GAME_SCHEMA_VERSION), int(4)],
    )?;
    validate_schema(db)
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
                validate_previous_admission_schema(db, &actual)?;
                migrate_previous_game_schema(db)?;
                let migrated_tables = inventory(db)?;
                validate_retained_schema(
                    db,
                    &migrated_tables
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                )?;
            } else if version == 2 {
                migrate_recovery_coordination(db)?;
            } else if version == 3 {
                migrate_spectator_authority(db)?;
            } else if version == 4 {
                migrate_saved_operator_workflows(db)?;
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
    use super::super::gameplay::SQL_SELECT_GAME_BOARD_CELLS_ROW_COLUMN_KIND;
    use super::super::{AuthPolicy, CELL_VALUE, GameService, WorkKind, string};
    use super::*;
    use crate::auth::test_support::{Sqlite, TestRuntime};
    use brews_domain::ids::PlayerId;
    #[test]
    fn table_identifier_compatibility_preserves_column_and_string_literal_text() {
        let actual = r#"CREATE TABLE "game_record"("value" TEXT CHECK("value"='"game_players"') REFERENCES "game_players"(player_id)) STRICT"#;
        let expected = r#"CREATE TABLE game_record("value" TEXT CHECK("value"='"game_players"') REFERENCES game_players(player_id)) STRICT"#;
        assert_eq!(
            canonicalize_table_identifiers("game_record", actual, &GAME_TABLES),
            expected,
        );
    }
    fn previous_admission_database(include_delivery: bool, renamed: bool) -> Sqlite {
        let db = Sqlite::new();
        db.conn
            .borrow()
            .execute_batch(include_str!("../../../tests/fixtures/game-schema-v1.sql"))
            .unwrap();
        if include_delivery {
            crate::db::game_delivery::migrate_game_delivery(&db).unwrap();
            db.conn.borrow().execute_batch("INSERT INTO game_delivery_connections VALUES('01890f3e-53b7-7d28-9b05-4f65092d5709','01890f3e-53b7-7d28-9b05-4f65092d5708',0,10000); INSERT INTO game_delivery_pending VALUES('AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA','01890f3e-53b7-7d28-9b05-4f65092d5709',1,0,1000)").unwrap();
        }
        if renamed {
            for table in inventory(&db).unwrap() {
                db.execute(
                    &format!("ALTER TABLE {table} RENAME TO admission_identifier_probe"),
                    &[],
                )
                .unwrap();
                db.execute(
                    &format!("ALTER TABLE admission_identifier_probe RENAME TO {table}"),
                    &[],
                )
                .unwrap();
            }
        }
        db
    }
    fn previous_admission_contents(db: &Sqlite) -> Vec<(String, Vec<crate::db::Row>)> {
        let mut contents = vec![(
            "sqlite_master".into(),
            db.query(
                "SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name",
                &[],
            )
            .unwrap(),
        )];
        for table in inventory(db).unwrap() {
            contents.push((
                table.clone(),
                db.query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
                    .unwrap(),
            ));
        }
        contents
    }
    #[test]
    fn previous_admission_schema_rejects_drift_in_every_source_table_without_writes() {
        for include_delivery in [false, true] {
            let tables = migration_table_set(&PREVIOUS_GAME_TABLES, include_delivery);
            for table in &tables {
                for drift in [
                    "extra_column",
                    "extra_check",
                    "altered_check",
                    "foreign_key",
                    "table_mode",
                ] {
                    let db = previous_admission_database(include_delivery, false);
                    // Delivery rows are covered by the populated controls; empty this
                    // group so altered checks and references can be installed here.
                    if include_delivery {
                        db.execute("DELETE FROM game_delivery_pending", &[])
                            .unwrap();
                        db.execute("DELETE FROM game_delivery_connections", &[])
                            .unwrap();
                    }
                    let rows = db.query(&format!("SELECT * FROM {table}"), &[]).unwrap();
                    if drift == "extra_column" {
                        db.execute(
                            &format!("ALTER TABLE {table} ADD COLUMN unapproved_column INTEGER"),
                            &[],
                        )
                        .unwrap();
                    } else {
                        let ddl = db
                            .query(SQL_SELECT_TABLE_DDL_BY_NAME, &[text(table)])
                            .unwrap();
                        let original = string(&ddl[0], 0).unwrap();
                        let first_column = original
                            .split_once('(')
                            .unwrap()
                            .1
                            .split_whitespace()
                            .next()
                            .unwrap();
                        let altered = match drift {
                            "extra_check" => original.replace(") STRICT", ",CHECK(1)) STRICT"),
                            "altered_check" => {
                                let start = original.find("CHECK(").unwrap();
                                let mut depth = 0;
                                let end = original[start..].char_indices().find_map(|(offset, character)| {
                                    match character {
                                        '(' => depth += 1,
                                        ')' => {
                                            depth -= 1;
                                            if depth == 0 { return Some(start + offset + 1); }
                                        }
                                        _ => {}
                                    }
                                    None
                                }).unwrap();
                                format!("{}CHECK(1){}", &original[..start], &original[end..])
                            }
                            "foreign_key" if original.contains(" REFERENCES ") => {
                                let start = original.find(" REFERENCES ").unwrap() + " REFERENCES ".len();
                                let end = start + original[start..].find('(').unwrap();
                                format!("{}game_unrecognized_reference{}", &original[..start], &original[end..])
                            }
                            "foreign_key" => original.replace(") STRICT", &format!(",FOREIGN KEY({first_column}) REFERENCES game_metadata(singleton)) STRICT")),
                            "table_mode" => original.strip_suffix(" STRICT").unwrap().to_owned(),
                            _ => unreachable!(),
                        };
                        db.execute(&format!("DROP TABLE {table}"), &[]).unwrap();
                        db.execute(&altered, &[]).unwrap();
                        for row in rows {
                            let placeholders = vec!["?"; row.len()].join(",");
                            db.execute(
                                &format!("INSERT INTO {table} VALUES({placeholders})"),
                                &row,
                            )
                            .unwrap();
                        }
                    }
                    let before = previous_admission_contents(&db);
                    assert_eq!(
                        migrate_game(&db),
                        Err(GameError::Storage),
                        "{include_delivery}/{table}/{drift}"
                    );
                    assert_eq!(
                        previous_admission_contents(&db),
                        before,
                        "{include_delivery}/{table}/{drift}"
                    );
                    assert_eq!(
                        db.query(SQL_SELECT_GAME_METADATA_SCHEMA_VERSION, &[])
                            .unwrap(),
                        vec![vec![int(PREVIOUS_GAME_SCHEMA_VERSION)]]
                    );
                }
            }
        }
    }
    #[test]
    fn previous_admission_schema_rejects_unknown_table_without_writes() {
        let db = previous_admission_database(false, false);
        db.execute(
            "CREATE TABLE game_unapproved(value INTEGER,other INTEGER) STRICT",
            &[],
        )
        .unwrap();
        let before = previous_admission_contents(&db);
        assert_eq!(migrate_game(&db), Err(GameError::Storage));
        assert_eq!(previous_admission_contents(&db), before);
    }
    #[test]
    fn previous_admission_schema_preserves_populated_lobby_and_quoted_delivery() {
        for include_delivery in [false, true] {
            for renamed in [false, true] {
                let db = previous_admission_database(include_delivery, renamed);
                db.conn.borrow().execute_batch("INSERT INTO game_record(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due,lobby_opened_at,game_code,published_at) VALUES(1,'01890f3e-53b7-7d28-9b05-4f65092d5701','01890f3e-53b7-7d28-9b05-4f65092d5702','01890f3e-53b7-7d28-9b05-4f65092d5703',zeroblob(32),'01890f3e-53b7-7d28-9b05-4f65092d5702',0,'awaiting_players',1,1000,1001,86401001,1001,'ABCDEFGH',1001); INSERT INTO game_configuration VALUES(1,4,2,1,2,1,'single_line'); INSERT INTO game_free_cells VALUES(1,1); INSERT INTO game_players VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705','Player',1002,0,NULL); INSERT INTO game_recovery VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705','fixture-verifier',1,1002); INSERT INTO game_sessions VALUES('01890f3e-53b7-7d28-9b05-4f65092d5706',zeroblob(32),'01890f3e-53b7-7d28-9b05-4f65092d5701','ABCDEFGH','01890f3e-53b7-7d28-9b05-4f65092d5705',0,'player',1002,86401002,NULL); INSERT INTO game_admission_contexts VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705',zeroblob(32),'ABCDEFGH',1001,2000,3000,'01890f3e-53b7-7d28-9b05-4f65092d5707'); INSERT INTO game_view_revisions VALUES('host',1); INSERT INTO game_receipts VALUES('host','01890f3e-53b7-7d28-9b05-4f65092d5707',zeroblob(32),'fixture-outcome',1001,3000)").unwrap();
                let before = previous_admission_contents(&db);
                migrate_game(&db).unwrap();
                for (table, rows) in before {
                    if !matches!(
                        table.as_str(),
                        "sqlite_master" | "game_record" | "game_metadata"
                    ) {
                        assert_eq!(
                            db.query(&format!("SELECT * FROM {table} ORDER BY 1,2"), &[])
                                .unwrap(),
                            rows,
                            "{include_delivery}/{renamed}/{table}"
                        );
                    }
                }
                let runtime = TestRuntime::new();
                let service =
                    GameService::new(&db, &runtime, AuthPolicy::default(), &[9; 32]).unwrap();
                assert_eq!(service.record().unwrap().state, GameState::AwaitingPlayers);
                migrate_game(&db).unwrap();
            }
        }
    }
    struct SwallowedMigrationStatement<'a> {
        db: &'a Sqlite,
        statement: &'static str,
    }
    impl Database for SwallowedMigrationStatement<'_> {
        fn query(
            &self,
            sql: &str,
            params: &[SqlValue],
        ) -> Result<Vec<crate::db::Row>, StorageError> {
            self.db.query(sql, params)
        }
        fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
            if sql == self.statement {
                Ok(())
            } else {
                self.db.execute(sql, params)
            }
        }
        fn transaction<T>(
            &self,
            operation: impl FnOnce() -> Result<T, StorageError>,
        ) -> Result<T, StorageError> {
            self.db.transaction(operation)
        }
    }
    #[test]
    fn previous_admission_schema_rolls_back_swallowed_destination_and_marker_writes() {
        for statement in [
            SQL_RENAME_GAME_RECORD_AFTER_SCHEMA_MIGRATION,
            SQL_RENAME_GAME_BOARD_CELLS_AFTER_SCHEMA_MIGRATION,
            SQL_UPDATE_GAME_SCHEMA_VERSION,
            "CREATE INDEX game_spectator_grace_deadline ON game_spectators(grace_expires_at)",
        ] {
            let db = previous_admission_database(true, true);
            let before = previous_admission_contents(&db);
            let fault = SwallowedMigrationStatement { db: &db, statement };
            assert_eq!(migrate_game(&fault), Err(GameError::Storage), "{statement}");
            assert_eq!(previous_admission_contents(&db), before, "{statement}");
        }
        for effect in ["IGNORE", "ABORT,'fixture storage fault'"] {
            let db = previous_admission_database(false, false);
            db.execute(&format!("CREATE TRIGGER admission_marker_fault BEFORE UPDATE OF schema_version ON game_metadata BEGIN SELECT RAISE({effect}); END"), &[]).unwrap();
            let before = previous_admission_contents(&db);
            assert_eq!(migrate_game(&db), Err(GameError::Storage));
            assert_eq!(previous_admission_contents(&db), before);
        }
    }
    #[test]
    fn previous_admission_schema_rejects_reviewer_extra_player_column_without_writes() {
        let db = Sqlite::new();
        db.conn
            .borrow()
            .execute_batch(include_str!("../../../tests/fixtures/game-schema-v1.sql"))
            .unwrap();
        db.execute(
            "ALTER TABLE game_players ADD COLUMN unapproved_column INTEGER",
            &[],
        )
        .unwrap();
        let before = db
            .query(
                "SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name",
                &[],
            )
            .unwrap();
        assert_eq!(migrate_game(&db), Err(GameError::Storage));
        assert_eq!(
            db.query(SQL_SELECT_GAME_METADATA_SCHEMA_VERSION, &[])
                .unwrap(),
            vec![vec![int(PREVIOUS_GAME_SCHEMA_VERSION)]]
        );
        assert_eq!(
            db.query(
                "SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name",
                &[]
            )
            .unwrap(),
            before
        );
        let runtime = TestRuntime::new();
        assert!(GameService::new(&db, &runtime, AuthPolicy::default(), &[9; 32]).is_err());
    }
    #[test]
    fn forward_schema_migration_accepts_legitimate_prestart_idle_cancellation_and_pending_release()
    {
        for include_delivery in [false, true] {
            for renamed in [false, true] {
                let db = previous_admission_database(include_delivery, renamed);
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
        }
    }
    #[test]
    fn forward_schema_migration_preserves_existing_game_configuration_and_board() {
        for include_delivery in [false, true] {
            for renamed in [false, true] {
                let db = previous_admission_database(include_delivery, renamed);
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
                let service =
                    GameService::new(&db, &runtime, AuthPolicy::default(), &[9; 32]).unwrap();
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
        }
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

    struct WriteCounter<'a> {
        inner: &'a Sqlite,
        writes: std::cell::Cell<usize>,
    }
    impl Database for WriteCounter<'_> {
        fn query(
            &self,
            sql: &str,
            params: &[SqlValue],
        ) -> Result<Vec<crate::db::Row>, StorageError> {
            self.inner.query(sql, params)
        }
        fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
            self.writes.set(self.writes.get() + 1);
            self.inner.execute(sql, params)
        }
        fn transaction<T>(
            &self,
            operation: impl FnOnce() -> Result<T, StorageError>,
        ) -> Result<T, StorageError> {
            self.inner.transaction(operation)
        }
    }
    fn spectator_schema_database(delivery: bool) -> Sqlite {
        let db = Sqlite::new();
        for statement in previous_gameplay_schema::SPECTATOR_STATEMENTS {
            db.execute(statement, &[]).unwrap();
        }
        db.execute(SQL_INSERT_GAME_METADATA, &[int(4)]).unwrap();
        if delivery {
            crate::db::game_delivery::migrate_game_delivery(&db).unwrap();
        }
        db
    }
    #[test]
    fn operator_migration_from_spectator_counter_preserves_optional_delivery_and_rejects_every_unknown_shape_before_writes()
     {
        for delivery in [false, true] {
            let db = spectator_schema_database(delivery);
            migrate_game(&db).unwrap();
            validate_schema(&db).unwrap();
            assert_eq!(
                db.query(SQL_SELECT_GAME_METADATA_SCHEMA_VERSION, &[])
                    .unwrap(),
                vec![vec![int(GAME_SCHEMA_VERSION)]]
            );
            for table in previous_gameplay_schema::SPECTATOR_TABLES
                .iter()
                .copied()
                .chain(if delivery {
                    DELIVERY_TABLES.to_vec()
                } else {
                    vec![]
                })
            {
                let db = spectator_schema_database(delivery);
                db.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN unknown_shape TEXT"),
                    &[],
                )
                .unwrap();
                let counted = WriteCounter {
                    inner: &db,
                    writes: std::cell::Cell::new(0),
                };
                assert_eq!(migrate_game(&counted), Err(GameError::Storage), "{table}");
                assert_eq!(counted.writes.get(), 0, "{table}");
            }
            let db = spectator_schema_database(delivery);
            db.execute("DROP INDEX game_spectator_session_expiry", &[])
                .unwrap();
            let counted = WriteCounter {
                inner: &db,
                writes: std::cell::Cell::new(0),
            };
            assert_eq!(migrate_game(&counted), Err(GameError::Storage));
            assert_eq!(counted.writes.get(), 0);
        }
    }
    #[test]
    fn historical_gameplay_recovery_spectator_inventories_reject_future_shapes_without_writes() {
        for (version, statements, tables) in [
            (
                2,
                previous_gameplay_schema::GAMEPLAY_STATEMENTS.as_slice(),
                previous_gameplay_schema::GAMEPLAY_TABLES.as_slice(),
            ),
            (
                3,
                previous_gameplay_schema::RECOVERY_STATEMENTS.as_slice(),
                previous_gameplay_schema::RECOVERY_TABLES.as_slice(),
            ),
            (
                4,
                previous_gameplay_schema::SPECTATOR_STATEMENTS.as_slice(),
                previous_gameplay_schema::SPECTATOR_TABLES.as_slice(),
            ),
        ] {
            for delivery in [false, true] {
                let origin = || {
                    let db = Sqlite::new();
                    for statement in statements {
                        db.execute(statement, &[]).unwrap();
                    }
                    db.execute(SQL_INSERT_GAME_METADATA, &[int(version)])
                        .unwrap();
                    if delivery {
                        crate::db::game_delivery::migrate_game_delivery(&db).unwrap();
                    }
                    db
                };
                let valid = origin();
                migrate_game(&valid).unwrap();
                validate_schema(&valid).unwrap();
                for table in tables {
                    let db = origin();
                    db.execute(
                        &format!("ALTER TABLE {table} ADD COLUMN future_shape TEXT"),
                        &[],
                    )
                    .unwrap();
                    let counted = WriteCounter {
                        inner: &db,
                        writes: std::cell::Cell::new(0),
                    };
                    assert_eq!(
                        migrate_game(&counted),
                        Err(GameError::Storage),
                        "{version}/{delivery}/{table}"
                    );
                    assert_eq!(counted.writes.get(), 0);
                }
                for drift in [
                    "unknown_table",
                    "unknown_index",
                    "index_shape",
                    "constraint",
                    "foreign_key",
                ] {
                    let db = origin();
                    match drift {
                        "unknown_table" => db
                            .execute(
                                "CREATE TABLE game_future_origin(id TEXT PRIMARY KEY) STRICT",
                                &[],
                            )
                            .unwrap(),
                        "unknown_index" => db
                            .execute(
                                "CREATE INDEX game_future_index ON game_metadata(last_observed_ms)",
                                &[],
                            )
                            .unwrap(),
                        "index_shape" => {
                            if version == 2 {
                                db.execute("CREATE INDEX game_recovery_bucket_expiry ON game_recovery(player_id)", &[]).unwrap();
                            } else {
                                db.execute("DROP INDEX game_recovery_bucket_expiry", &[])
                                    .unwrap();
                                db.execute("CREATE INDEX game_recovery_bucket_expiry ON game_recovery_buckets(attempt_count)", &[]).unwrap();
                            }
                        }
                        "constraint" => {
                            db.execute("DROP TABLE game_free_cells", &[]).unwrap();
                            let ddl = statements
                                .iter()
                                .find(|s| s.starts_with("CREATE TABLE game_free_cells("))
                                .unwrap()
                                .replace(
                                    "row INTEGER NOT NULL",
                                    "row INTEGER NOT NULL CHECK(row>0)",
                                );
                            db.execute(&ddl, &[]).unwrap();
                        }
                        "foreign_key" => {
                            db.execute("DROP TABLE game_free_cells", &[]).unwrap();
                            let ddl = statements
                                .iter()
                                .find(|s| s.starts_with("CREATE TABLE game_free_cells("))
                                .unwrap()
                                .replace(
                                    "row INTEGER NOT NULL",
                                    "row INTEGER NOT NULL REFERENCES game_metadata(singleton)",
                                );
                            db.execute(&ddl, &[]).unwrap();
                        }
                        _ => unreachable!(),
                    }
                    let counted = WriteCounter {
                        inner: &db,
                        writes: std::cell::Cell::new(0),
                    };
                    assert_eq!(
                        migrate_game(&counted),
                        Err(GameError::Storage),
                        "{version}/{delivery}/{drift}"
                    );
                    assert_eq!(counted.writes.get(), 0);
                }
            }
        }
    }
}
