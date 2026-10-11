use super::*;
use brews_backend::db::{Row, StorageError};
use brews_backend::game::{ConnectionGrant, SecretCookie};
use brews_contracts::games::GameOutcome;
use brews_domain::games::GameState;
use std::cell::Cell;

fn table_rows(f: &Fixture, table: &str) -> Vec<Row> {
    f.game
        .query(&format!("SELECT * FROM {table} ORDER BY 1"), &[])
        .unwrap()
}
fn durable_snapshot(f: &Fixture) -> Vec<Vec<Row>> {
    [
        "game_players",
        "game_sessions",
        "game_recovery",
        "game_current_connections",
        "game_connection_grants",
        "game_socket_closures",
        "game_view_revisions",
        "game_receipts",
        "game_admission_contexts",
        "game_boards",
        "game_board_cells",
        "game_completed_lines",
        "game_record",
        "game_metadata",
    ]
    .map(|table| table_rows(f, table))
    .to_vec()
}
fn lobby() -> (Fixture, SecretCookie) {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let player = f.player(&code, 20, "ReviewPlayer");
    (f, player)
}
fn connect(f: &Fixture, player: &SecretCookie, n: u8) -> ConnectionGrant {
    let connection = f
        .service()
        .prepare_player_connection(player.token(), id(&f.rt, n).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&connection).unwrap();
    connection
}
struct ReadbackFault<'a> {
    db: &'a Sqlite,
    write: &'a str,
    read: &'a str,
    armed: Cell<bool>,
}
impl Database for ReadbackFault<'_> {
    fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StorageError> {
        if self.armed.get() && sql.contains(self.read) {
            return Err(StorageError);
        }
        self.db.query(sql, params)
    }
    fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<(), StorageError> {
        self.db.execute(sql, params)?;
        if sql.contains(self.write) {
            self.armed.set(true);
        }
        Ok(())
    }
    fn transaction<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        self.db.transaction(operation)
    }
}
#[test]
fn required_readback_faults_roll_back_every_lifecycle_effect_and_clock_metadata() {
    for (write, read, leave) in [
        (
            "UPDATE game_players SET alias",
            "SELECT player_id,alias,joined_at",
            false,
        ),
        (
            "UPDATE game_view_revisions",
            "SELECT revision FROM game_view_revisions",
            false,
        ),
        (
            "INSERT INTO game_receipts",
            "SELECT actor,command_id,fingerprint,outcome",
            false,
        ),
        (
            "INSERT INTO game_socket_closures",
            "SELECT session_id,player_id,epoch FROM game_socket_closures",
            true,
        ),
        (
            "DELETE FROM game_current_connections",
            "SELECT connection_id,session_id,epoch FROM game_current_connections",
            true,
        ),
        (
            "DELETE FROM game_connection_grants",
            "SELECT 1 FROM game_connection_grants",
            true,
        ),
        (
            "DELETE FROM game_sessions",
            "SELECT 1 FROM game_sessions",
            true,
        ),
        (
            "INSERT INTO game_receipts",
            "SELECT actor,command_id,fingerprint,outcome",
            true,
        ),
    ] {
        let (f, player) = lobby();
        connect(&f, &player, 80);
        f.service()
            .prepare_player_connection(player.token(), id(&f.rt, 81).parse().unwrap())
            .unwrap();
        let before = durable_snapshot(&f);
        let db = ReadbackFault {
            db: &f.game,
            write,
            read,
            armed: Cell::new(false),
        };
        let service = GameService::new(&db, &f.rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let result = if leave {
            service.leave_player(f.command(70), player.token(), GameState::AwaitingPlayers)
        } else {
            service.rename_my_alias(f.command(70), player.token(), "Successor")
        };
        assert_eq!(
            result.err(),
            Some(GameError::Storage),
            "required readback {write}"
        );
        assert!(db.armed.get(), "fault must actually arm for {write}");
        assert!(
            durable_snapshot(&f) == before,
            "all owner writes must roll back for {write}"
        );
    }
}
#[test]
fn ignored_or_aborted_explicit_leave_timestamp_rolls_back_detachment_and_receipt() {
    for fault in ["IGNORE", "ABORT, 'test-only Leave timestamp fault'"] {
        let f = Fixture::new();
        let (_, player, _, _) = f.start_two_player_game();
        let before = durable_snapshot(&f);
        f.game.conn.borrow().execute_batch(&format!("CREATE TRIGGER boundary_stamp_fault BEFORE UPDATE OF last_explicit_leave_at ON game_players BEGIN SELECT RAISE({fault}); END")).unwrap();
        assert_eq!(
            f.service()
                .leave_player(f.command(70), player.token(), GameState::InProgress)
                .err(),
            Some(GameError::Storage)
        );
        assert!(durable_snapshot(&f) == before);
    }
}
#[test]
fn newer_recovery_session_cannot_extend_original_leave_receipt_release_deadline() {
    let f = Fixture::new();
    let (code, player, _, _) = f.start_two_player_game();
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(70), player.token(), Some(&answer))
        .unwrap();
    let command = f.command(71);
    let left = f
        .service()
        .leave_player(command, player.token(), GameState::InProgress)
        .unwrap();
    let GameResponse::PlayerLeft { receipt, .. } = left.response else {
        panic!("first Leave")
    };
    f.rt.now.set(f.rt.now.get() + 1000);
    let recovered = f
        .service()
        .recover_player(f.command(72), &code, "First", &answer, "192.0.2.7")
        .unwrap()
        .cookie
        .unwrap();
    let repeated = f
        .service()
        .leave_player(command, recovered.token(), GameState::InProgress)
        .unwrap();
    let deadline = f
        .service()
        .membership_output_deadline(recovered.token(), &repeated)
        .unwrap()
        .unwrap();
    assert!(
        deadline <= receipt.expires_at,
        "fresh current session cannot extend original receipt retention"
    );
    assert!(
        matches!(repeated.response, GameResponse::Committed { receipt: r } if matches!(r.outcome, GameOutcome::PlayerLeft { .. }) && r.command_id == command)
    );
}
