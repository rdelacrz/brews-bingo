//! Legitimate previous-schema terminal rows are preserved by migration, then maintained.
use super::*;

const PREVIOUS_IDLE_CANCELLATION: &str = "INSERT INTO game_record(singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,ended_at,cancellation_reason) VALUES(1,'01890f3e-53b7-7d28-9b05-4f65092d5701','01890f3e-53b7-7d28-9b05-4f65092d5702','01890f3e-53b7-7d28-9b05-4f65092d5703',zeroblob(32),'01890f3e-53b7-7d28-9b05-4f65092d5702',0,'cancelled',1,1000,1000,86401000,'host_idle_timeout'); INSERT INTO game_view_revisions VALUES('host',1); INSERT INTO game_pending_work VALUES('01890f3e-53b7-7d28-9b05-4f65092d5703','release','awaiting_acknowledgement','system','01890f3e-53b7-7d28-9b05-4f65092d5703',zeroblob(32),0,1,86401000,86402000,0);";
const ORIGINAL_RECORD_SCALARS: &str = "SELECT singleton,game_id,creator_id,creation_command,fingerprint,host_id,host_assignment_revision,state,revision,created_at,last_host_activity,idle_due,lobby_opened_at,started_at,ended_at,game_code,published_at,cancellation_reason FROM game_record";

fn migrated_prestart() -> Fixture {
    let f = Fixture::new();
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    connection
        .execute_batch(include_str!("../fixtures/game-schema-v1.sql"))
        .unwrap();
    drop(f.game.conn.replace(connection));
    f.game
        .conn
        .borrow()
        .execute_batch(PREVIOUS_IDLE_CANCELLATION)
        .unwrap();
    let source = f.game.query(ORIGINAL_RECORD_SCALARS, &[]).unwrap();
    let counter = f
        .game
        .query("SELECT * FROM game_view_revisions", &[])
        .unwrap();
    let release = f
        .game
        .query("SELECT * FROM game_pending_work", &[])
        .unwrap();
    brews_backend::db::game::migrate_game(&f.game).unwrap();
    assert!(
        f.game.query(ORIGINAL_RECORD_SCALARS, &[]).unwrap() == source,
        "Forward migration must preserve all original source scalars"
    );
    assert!(
        f.game
            .query("SELECT * FROM game_view_revisions", &[])
            .unwrap()
            == counter,
        "Migration preservation precedes lifecycle maintenance"
    );
    assert!(
        f.game
            .query("SELECT * FROM game_pending_work", &[])
            .unwrap()
            == release
    );
    assert_eq!(count(&f, "game_history"), 0);
    assert_eq!(count(&f, "game_connection_grants"), 0);
    assert_eq!(count(&f, "game_terminal_views"), 0);
    f
}

#[test]
fn migrated_prestart_idle_cancellation_retires_obsolete_counter_in_real_maintenance() {
    let f = migrated_prestart();
    assert_eq!(count(&f, "game_view_revisions"), 1);
    let record = f.game.query("SELECT * FROM game_record", &[]).unwrap();
    let release = f
        .game
        .query("SELECT * FROM game_pending_work", &[])
        .unwrap();
    let proof = f.service().cleanup().unwrap().unwrap();
    assert_eq!(proof.projection().state(), GameState::Cancelled);
    assert_eq!(proof.projection().source_revision(), 1);
    assert!(proof.projection().started_at().is_none());
    assert!(proof.projection().history_expires_at().is_none());
    assert_eq!(
        count(&f, "game_view_revisions"),
        0,
        "A pre-start terminal game has no eligible final projection counter"
    );
    assert!(
        f.game.query("SELECT * FROM game_record", &[]).unwrap() == record,
        "Counter retirement must preserve terminal routing proof"
    );
    assert!(
        f.game
            .query("SELECT * FROM game_pending_work", &[])
            .unwrap()
            == release,
        "Matching unresolved release work must survive maintenance"
    );
    assert_eq!(count(&f, "game_history"), 0);
    assert_eq!(count(&f, "game_terminal_views"), 0);
    let after = all_rows(&f);
    assert_eq!(
        f.service().cleanup().unwrap().unwrap().projection(),
        proof.projection()
    );
    assert!(all_rows(&f) == after, "Repeated maintenance is idempotent");
}

#[test]
fn migrated_prestart_counter_retirement_ignored_or_aborted_delete_rolls_back_clock_and_release() {
    for mode in ["IGNORE", "ABORT,'counter fault'"] {
        let f = migrated_prestart();
        let before = all_rows(&f);
        let record = f.game.query("SELECT * FROM game_record", &[]).unwrap();
        let release = f
            .game
            .query("SELECT * FROM game_pending_work", &[])
            .unwrap();
        f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER counter_fault BEFORE DELETE ON game_view_revisions BEGIN SELECT RAISE({mode}); END")).unwrap();
        f.rt.now.set(f.rt.now.get() + 1);
        assert_eq!(
            f.service().cleanup().err(),
            Some(GameError::Storage),
            "Counter deletion requires exact absence readback"
        );
        assert!(
            all_rows(&f) == before,
            "Failed counter retirement rolls back all app tables including clock"
        );
        f.game
            .conn
            .borrow()
            .execute_batch("DROP TRIGGER counter_fault")
            .unwrap();
        assert!(f.service().cleanup().unwrap().is_some());
        assert_eq!(count(&f, "game_view_revisions"), 0);
        assert!(f.game.query("SELECT * FROM game_record", &[]).unwrap() == record);
        assert!(
            f.game
                .query("SELECT * FROM game_pending_work", &[])
                .unwrap()
                == release
        );
        assert_eq!(count(&f, "game_history"), 0);
        assert_eq!(count(&f, "game_terminal_views"), 0);
    }
}
