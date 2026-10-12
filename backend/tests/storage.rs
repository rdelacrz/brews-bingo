#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::db::{CURRENT_SCHEMA_VERSION, Database, SqlValue, StorageError, migrate};
use support::Sqlite;

#[test]
fn migration_is_atomic_idempotent_and_refuses_newer_versions() {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    migrate(&db).unwrap();
    assert_eq!(
        db.query(
            "SELECT schema_version FROM storage_metadata WHERE singleton=1",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(CURRENT_SCHEMA_VERSION)]]
    );
    db.execute(
        "UPDATE storage_metadata SET schema_version=? WHERE singleton=1",
        &[SqlValue::Integer(CURRENT_SCHEMA_VERSION + 1)],
    )
    .unwrap();
    assert_eq!(migrate(&db), Err(StorageError));
}

#[test]
fn migration_preserves_initialized_ddl_and_clock_metadata() {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    db.execute(
        "UPDATE storage_metadata SET last_observed_ms=123,command_floor_ms=100 WHERE singleton=1",
        &[],
    )
    .unwrap();
    let before = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    migrate(&db).unwrap();
    assert_eq!(
        db.query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
            .unwrap(),
        before
    );
    assert_eq!(
        db.query(
            "SELECT last_observed_ms,command_floor_ms FROM storage_metadata WHERE singleton=1",
            &[]
        )
        .unwrap(),
        vec![vec![SqlValue::Integer(123), SqlValue::Integer(100)]]
    );
}

#[test]
fn failed_auth_initialization_rolls_back_metadata_and_partial_ddl() {
    let db = Sqlite::new();
    db.execute("CREATE TABLE accounts(existing INTEGER)", &[])
        .unwrap();
    let before = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    assert_eq!(migrate(&db), Err(StorageError));
    assert_eq!(
        db.query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
            .unwrap(),
        before
    );
}

#[test]
fn ignored_initial_schema_marker_rolls_back_all_auth_tables() {
    let initialized = Sqlite::new();
    migrate(&initialized).unwrap();
    let ddl = initialized
        .query(
            "SELECT sql FROM sqlite_master WHERE name='storage_metadata'",
            &[],
        )
        .unwrap();
    let SqlValue::Text(metadata_ddl) = &ddl[0][0] else {
        panic!("metadata DDL missing");
    };
    let db = Sqlite::new();
    db.execute(metadata_ddl, &[]).unwrap();
    db.execute(
        "CREATE TRIGGER ignore_marker BEFORE INSERT ON storage_metadata BEGIN SELECT RAISE(IGNORE); END",
        &[],
    )
    .unwrap();
    let before = db
        .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
        .unwrap();
    assert_eq!(migrate(&db), Err(StorageError));
    assert_eq!(
        db.query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
            .unwrap(),
        before
    );
    assert!(
        db.query("SELECT * FROM storage_metadata", &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ignored_or_corrupted_upgrade_marker_rolls_back_management_ddl() {
    for trigger in [
        "CREATE TRIGGER marker_fault BEFORE UPDATE OF schema_version ON storage_metadata BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER marker_fault AFTER UPDATE OF schema_version ON storage_metadata BEGIN UPDATE storage_metadata SET schema_version=3; END",
    ] {
        let db = Sqlite::new();
        migrate(&db).unwrap();
        for table in [
            "management_receipts",
            "admin_audit",
            "pending_account_removals",
        ] {
            db.execute(&format!("DROP TABLE {table}"), &[]).unwrap();
        }
        db.execute("UPDATE storage_metadata SET schema_version=1,last_observed_ms=123,command_floor_ms=100", &[]).unwrap();
        db.execute(trigger, &[]).unwrap();
        let before = db
            .query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
            .unwrap();
        let metadata = db.query("SELECT * FROM storage_metadata", &[]).unwrap();
        assert_eq!(migrate(&db), Err(StorageError));
        assert_eq!(
            db.query("SELECT name,sql FROM sqlite_master ORDER BY name", &[])
                .unwrap(),
            before
        );
        assert_eq!(
            db.query("SELECT * FROM storage_metadata", &[]).unwrap(),
            metadata
        );
    }
}

#[test]
fn sql_constraints_preserve_control_bytes_and_reject_unsafe_values() {
    let db = Sqlite::new();
    migrate(&db).unwrap();
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let username = "User\0name\x7f1";
    let insert = |id: &str, name: &str, created: i64| {
        db.execute("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?,'host','pending_enrollment',NULL,0,?,NULL,NULL)",&[SqlValue::Text(id.into()),SqlValue::Text(name.into()),SqlValue::Integer(created)])
    };
    insert(id, username, 0).unwrap();
    assert_eq!(
        db.query(
            "SELECT username FROM accounts WHERE username=? COLLATE BINARY",
            &[SqlValue::Text(username.into())]
        )
        .unwrap(),
        vec![vec![SqlValue::Text(username.into())]]
    );
    assert!(insert("01890f3e-53b7-7d28-9b05-4f65092d5712", username, 0).is_err());
    insert(
        "01890f3e-53b7-7d28-9b05-4f65092d5712",
        &username.to_ascii_lowercase(),
        0,
    )
    .unwrap();
    for name in ["abcde\u{0b}fghij", "abcdefghijé"] {
        assert!(insert("01890f3e-53b7-7d28-9b05-4f65092d5713", name, 0).is_err());
    }
    assert!(insert("01890f3e-53b7-7d28-fb05-4f65092d5713", "abcdefghij", 0).is_err());
    assert!(
        insert(
            "01890f3e-53b7-7d28-9b05-4f65092d5713",
            "abcdefghij",
            9_007_199_254_740_992
        )
        .is_err()
    );
    assert!(
        db.execute(
            "UPDATE accounts SET credential_epoch=0.5 WHERE account_id=?",
            &[SqlValue::Text(id.into())]
        )
        .is_err()
    );
    assert!(
        db.execute(
            "UPDATE accounts SET role='unknown' WHERE account_id=?",
            &[SqlValue::Text(id.into())]
        )
        .is_err()
    );
    assert!(db.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at,revoked_at) VALUES(?,?,?,'normal',0,0,86400000,NULL)",&[SqlValue::Text("01890f3e-53b7-7d28-9b05-4f65092d5713".into()),SqlValue::Text(id.into()),SqlValue::Blob(vec![0;31])]).is_err());
}

#[test]
fn real_sqlite_transaction_rolls_back_on_storage_failure() {
    let db = Sqlite::new();
    db.execute("CREATE TABLE probe(value INTEGER NOT NULL UNIQUE)", &[])
        .unwrap();
    let result: Result<(), StorageError> = db.transaction(|| {
        db.execute(
            "INSERT INTO probe(value) VALUES(?)",
            &[SqlValue::Integer(1)],
        )?;
        db.execute(
            "INSERT INTO probe(value) VALUES(?)",
            &[SqlValue::Integer(1)],
        )
    });
    assert_eq!(result, Err(StorageError));
    assert!(db.query("SELECT value FROM probe", &[]).unwrap().is_empty());
}

#[derive(Debug, PartialEq)]
struct DbDeepSnapshot {
    ddl: Vec<brews_backend::db::Row>,
    tables: Vec<TableSnapshot>,
}
#[derive(Debug, PartialEq)]
struct TableSnapshot {
    name: &'static str,
    columns: Vec<brews_backend::db::Row>,
    rows: Vec<brews_backend::db::Row>,
}
impl DbDeepSnapshot {
    fn capture(db: &Sqlite) -> Self {
        use brews_backend::db::schema::game_schema::{DELIVERY_TABLES, GAME_TABLES};
        let ddl = db
            .query(
                "SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name",
                &[],
            )
            .unwrap();
        let mut tables = Vec::new();
        for table in GAME_TABLES
            .into_iter()
            .chain(DELIVERY_TABLES)
            .chain(["__miniflare_do_name", "_cf_METADATA"])
        {
            if db
                .query(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
                    &[SqlValue::Text(table.into())],
                )
                .unwrap()
                .is_empty()
            {
                continue;
            }
            tables.push(TableSnapshot {
                name: table,
                columns: db
                    .query(&format!("PRAGMA table_info({table})"), &[])
                    .unwrap(),
                rows: db
                    .query(&format!("SELECT * FROM {table} ORDER BY 1"), &[])
                    .unwrap(),
            });
        }
        Self { ddl, tables }
    }
}

fn current_game_database(from_admission: bool, include_delivery: bool) -> Sqlite {
    let db = Sqlite::new();
    if from_admission {
        db.conn
            .borrow()
            .execute_batch(include_str!("fixtures/game-schema-v1.sql"))
            .unwrap();
    }
    brews_backend::db::game::migrate_game(&db).unwrap();
    if include_delivery {
        brews_backend::db::game_delivery::migrate_game_delivery(&db).unwrap();
    }
    db
}

fn assert_game_shape_denied_without_writes(db: &Sqlite, label: &str) {
    use brews_backend::{
        auth::AuthPolicy,
        db::game::{GameService, migrate_game},
        game::GameError,
    };
    let before = DbDeepSnapshot::capture(db);
    let observed = ObservedWrites {
        db,
        executions: std::cell::Cell::new(0),
    };
    let result = migrate_game(&observed);
    let rt = support::TestRuntime::new();
    let service_error = GameService::new(&observed, &rt, AuthPolicy::default(), &[9; 32]).err();
    assert_eq!(DbDeepSnapshot::capture(db), before, "{label}");
    assert_eq!(result, Err(GameError::Storage), "{label}");
    assert_eq!(service_error, Some(GameError::Storage), "{label}");
    assert_eq!(
        observed.executions.get(),
        0,
        "{label}: attempted state/schema write"
    );
}

struct ObservedWrites<'a> {
    db: &'a Sqlite,
    executions: std::cell::Cell<usize>,
}
impl Database for ObservedWrites<'_> {
    fn query(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<brews_backend::db::Row>, StorageError> {
        self.db.query(sql, params)
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        self.executions.set(self.executions.get() + 1);
        self.db.execute(sql, params)
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.db.transaction(operation)
    }
}

#[test]
fn current_game_schema_rejects_unknown_columns_in_every_core_and_optional_delivery_table() {
    use brews_backend::db::schema::game_schema::{DELIVERY_TABLES, GAME_TABLES};
    let mut cases = 0;
    for from_admission in [false, true] {
        for include_delivery in [false, true] {
            for table in GAME_TABLES
                .into_iter()
                .chain(DELIVERY_TABLES.into_iter().filter(|_| include_delivery))
            {
                let db = current_game_database(from_admission, include_delivery);
                db.conn.borrow().execute_batch("UPDATE game_metadata SET last_observed_ms=123,command_floor_ms=100; INSERT INTO game_players VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705','Preserved',100,0,NULL); INSERT INTO game_view_revisions VALUES('host',2); CREATE TABLE __miniflare_do_name(name TEXT); INSERT INTO __miniflare_do_name VALUES('test-owner'); CREATE TABLE _cf_METADATA(key TEXT,value INTEGER); INSERT INTO _cf_METADATA VALUES('test',1)").unwrap();
                if include_delivery {
                    db.conn.borrow().execute_batch("INSERT INTO game_delivery_connections VALUES('01890f3e-53b7-7d28-9b05-4f65092d5709','01890f3e-53b7-7d28-9b05-4f65092d5708',0,10000); INSERT INTO game_delivery_pending VALUES('AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA','01890f3e-53b7-7d28-9b05-4f65092d5709',1,0,1000)").unwrap();
                }
                db.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN unapproved_column INTEGER"),
                    &[],
                )
                .unwrap();
                assert_game_shape_denied_without_writes(
                    &db,
                    &format!("admission={from_admission}/delivery={include_delivery}/{table}"),
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 126);
}

#[test]
fn previous_gameplay_and_recovery_sources_reject_unknown_columns_in_every_table() {
    use brews_backend::db::schema::game_schema::{DELIVERY_TABLES, GAME_TABLES};
    let mut cases = 0;
    for version in [2, 3] {
        for include_delivery in [false, true] {
            let tables = GAME_TABLES
                .into_iter()
                .filter(|name| {
                    !name.starts_with("game_spectator")
                        && !(version == 2
                            && ["game_recovery_buckets", "game_socket_closures"].contains(name))
                })
                .chain(DELIVERY_TABLES.into_iter().filter(|_| include_delivery));
            for table in tables {
                let db = current_game_database(true, include_delivery);
                for name in GAME_TABLES.into_iter().filter(|name| {
                    name.starts_with("game_spectator")
                        || (version == 2
                            && ["game_recovery_buckets", "game_socket_closures"].contains(name))
                }) {
                    db.execute(&format!("DROP TABLE {name}"), &[]).unwrap();
                }
                db.execute("UPDATE game_metadata SET schema_version=?,last_observed_ms=123,command_floor_ms=100", &[SqlValue::Integer(version)]).unwrap();
                db.execute("INSERT INTO game_players VALUES('01890f3e-53b7-7d28-9b05-4f65092d5705','Preserved',100,0,NULL)", &[]).unwrap();
                db.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN unapproved_column INTEGER"),
                    &[],
                )
                .unwrap();
                assert_game_shape_denied_without_writes(
                    &db,
                    &format!("{version}/delivery={include_delivery}/{table}"),
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 102);
}

fn table_definition_parts(ddl: &str) -> (&str, Vec<&str>) {
    let (prefix, body) = ddl.split_once('(').unwrap();
    let body = body.strip_suffix(") STRICT").unwrap();
    let mut fields = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, character) in body.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    fields.push(&body[start..]);
    (prefix, fields)
}

fn replace_known_table_definition(db: &Sqlite, table: &'static str, ddl: &str) {
    let before = DbDeepSnapshot::capture(db);
    let old = before
        .tables
        .iter()
        .find(|entry| entry.name == table)
        .unwrap();
    let column_names = old
        .columns
        .iter()
        .map(|row| {
            let SqlValue::Text(name) = &row[1] else {
                panic!("column name");
            };
            name.as_str()
        })
        .collect::<Vec<_>>()
        .join(",");
    let indexes = db.query(
        "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name=? AND sql IS NOT NULL ORDER BY name",
        &[SqlValue::Text(table.into())],
    ).unwrap();
    db.execute(&format!("DROP TABLE {table}"), &[]).unwrap();
    db.execute(ddl, &[]).unwrap();
    for row in &old.rows {
        let placeholders = vec!["?"; row.len()].join(",");
        db.execute(
            &format!("INSERT INTO {table}({column_names}) VALUES({placeholders})"),
            row,
        )
        .unwrap();
    }
    for row in indexes {
        let [SqlValue::Text(sql)] = row.as_slice() else {
            panic!("explicit index DDL");
        };
        db.execute(sql, &[]).unwrap();
    }
}

#[test]
fn current_game_schema_rejects_full_column_and_constraint_drift_with_indexes_intact() {
    use brews_backend::db::schema::game_schema::{DELIVERY_TABLES, GAME_TABLES};
    let mut cases = 0;
    for table in GAME_TABLES.into_iter().chain(DELIVERY_TABLES) {
        for drift in [
            "type",
            "default",
            "nullability",
            "primary_key",
            "order",
            "check",
            "foreign_key",
            "strict",
        ] {
            let db = current_game_database(false, true);
            let result = db
                .query(
                    "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
                    &[SqlValue::Text(table.into())],
                )
                .unwrap();
            let [SqlValue::Text(original)] = result[0].as_slice() else {
                panic!("table DDL");
            };
            let (prefix, parts) = table_definition_parts(original);
            let mut fields = parts
                .iter()
                .map(|field| (*field).to_owned())
                .collect::<Vec<_>>();
            match drift {
                "type" => {
                    let mut first = fields[0].splitn(3, ' ');
                    let name = first.next().unwrap();
                    let kind = first.next().unwrap();
                    let rest = first.next().unwrap();
                    let changed = match kind {
                        "INTEGER" | "BLOB" => "TEXT",
                        "TEXT" => "BLOB",
                        _ => panic!("known column type"),
                    };
                    fields[0] = format!("{name} {changed} {rest}");
                }
                "default" => fields[0].push_str(" DEFAULT NULL"),
                "nullability" => {
                    let field = fields
                        .iter_mut()
                        .find(|field| field.contains(" NOT NULL"))
                        .unwrap();
                    *field = field.replacen(" NOT NULL", "", 1);
                }
                "primary_key" => {
                    if let Some(index) = fields
                        .iter()
                        .position(|field| field.starts_with("PRIMARY KEY("))
                    {
                        fields.remove(index);
                    } else {
                        let field = fields
                            .iter_mut()
                            .find(|field| field.contains(" PRIMARY KEY"))
                            .unwrap();
                        *field = field.replacen(" PRIMARY KEY", "", 1);
                    }
                }
                "order" => fields.swap(0, 1),
                "check" => fields.push("CHECK(1)".into()),
                "foreign_key" => {
                    let column = fields[0].split_whitespace().next().unwrap();
                    fields.push(format!(
                        "FOREIGN KEY({column}) REFERENCES game_metadata(singleton)"
                    ));
                }
                "strict" => {}
                _ => unreachable!(),
            }
            let suffix = if drift == "strict" { "" } else { " STRICT" };
            let altered = format!("{prefix}({}){suffix}", fields.join(","));
            replace_known_table_definition(&db, table, &altered);
            assert_game_shape_denied_without_writes(&db, &format!("{table}/{drift}"));
            cases += 1;
        }
    }
    assert_eq!(cases, 264);
}

#[test]
fn current_game_schema_rejects_unapproved_policy_state_literals() {
    let db = current_game_database(false, false);
    let result = db
        .query(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
            &[SqlValue::Text("game_record".into())],
        )
        .unwrap();
    let [SqlValue::Text(original)] = result[0].as_slice() else {
        panic!("table DDL");
    };
    let altered = original.replacen(
        "'resolved','cancelled'",
        "'resolved','cancelled','unapproved'",
        1,
    );
    assert_ne!(altered, *original);
    replace_known_table_definition(&db, "game_record", &altered);
    assert_game_shape_denied_without_writes(&db, "unapproved state literal");
}

#[test]
fn current_game_schema_accepts_known_materialized_forms_delivery_and_provider_metadata() {
    use brews_backend::{
        auth::AuthPolicy,
        db::game::{GameService, migrate_game},
    };
    for from_admission in [false, true] {
        for include_delivery in [false, true] {
            let db = current_game_database(from_admission, include_delivery);
            db.conn.borrow().execute_batch("CREATE TABLE __miniflare_do_name(name TEXT); INSERT INTO __miniflare_do_name VALUES('test-owner'); CREATE TABLE _cf_METADATA(key TEXT,value INTEGER); INSERT INTO _cf_METADATA VALUES('test',1)").unwrap();
            let before = DbDeepSnapshot::capture(&db);
            let observed = ObservedWrites {
                db: &db,
                executions: std::cell::Cell::new(0),
            };
            let rt = support::TestRuntime::new();
            for _ in 0..2 {
                migrate_game(&observed).unwrap();
                assert!(GameService::new(&observed, &rt, AuthPolicy::default(), &[9; 32]).is_ok());
            }
            assert_eq!(DbDeepSnapshot::capture(&db), before);
            assert_eq!(observed.executions.get(), 0);
        }
    }
}

#[test]
fn current_game_schema_rejects_reported_extra_player_column_without_writes() {
    use brews_backend::{
        auth::AuthPolicy,
        db::game::{GameService, migrate_game},
        game::GameError,
    };
    let db = Sqlite::new();
    migrate_game(&db).unwrap();
    db.execute(
        "ALTER TABLE game_players ADD COLUMN unapproved_column INTEGER",
        &[],
    )
    .unwrap();
    let before = DbDeepSnapshot::capture(&db);
    let result = migrate_game(&db);
    let rt = support::TestRuntime::new();
    let service_error = GameService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).err();
    assert_eq!(DbDeepSnapshot::capture(&db), before);
    assert_eq!(result, Err(GameError::Storage));
    assert_eq!(service_error, Some(GameError::Storage));
}
