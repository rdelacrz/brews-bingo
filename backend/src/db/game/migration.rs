//! Validate and migrate the game-owner schema in one storage transaction.
use super::{Database, GameError, SqlValue, StorageError, int, number, text};
use super::{
    SQL_INSERT_GAME_CONFIGURATION, SQL_INSERT_GAME_FREE_CELLS,
    SQL_SELECT_GAME_CONFIGURATION_NUMERIC_UPPER_BOUND_BOARD_SIDE_LENGTH_FREE_CELLS_ENABLED,
    SQL_SELECT_GAME_FREE_CELLS_ROW_COLUMN,
};
use crate::db::schema::game_schema::{
    self, DELIVERY_TABLES, GAME_SCHEMA_VERSION, GAME_TABLES, PREVIOUS_GAME_SCHEMA_VERSION,
    PREVIOUS_GAME_TABLES,
};
use brews_domain::games::GameState;
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
    for statement in game_schema::recovery_coordination_statements() {
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
    for statement in game_schema::recovery_coordination_statements()
        .into_iter()
        .filter(|sql| sql.starts_with("CREATE INDEX"))
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
fn migrate_recovery_coordination<D: Database>(db: &D) -> Result<(), StorageError> {
    let old = GAME_TABLES
        .iter()
        .copied()
        .filter(|name| !matches!(*name, "game_recovery_buckets" | "game_socket_closures"))
        .collect::<Vec<_>>();
    let actual = inventory(db)?;
    for table in &old {
        let expected = schema_statement_for(table)?;
        if db.query(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
            &[text(table)],
        )? != vec![vec![text(expected)]]
        {
            return Err(StorageError);
        }
    }
    if actual != migration_table_set(&old, false) && actual != migration_table_set(&old, true) {
        return Err(StorageError);
    }
    for statement in game_schema::recovery_coordination_statements() {
        db.execute(&statement, &[])?;
    }
    db.execute(
        SQL_UPDATE_GAME_SCHEMA_VERSION,
        &[int(GAME_SCHEMA_VERSION), int(2)],
    )?;
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
            } else if version == 2 {
                migrate_recovery_coordination(db)?;
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
    fn forward_schema_migration_accepts_legitimate_prestart_idle_cancellation_and_pending_release()
    {
        let db = Sqlite::new();
        db.conn
            .borrow()
            .execute_batch(include_str!("../../../tests/fixtures/game-schema-v1.sql"))
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
            .execute_batch(include_str!("../../../tests/fixtures/game-schema-v1.sql"))
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
}
