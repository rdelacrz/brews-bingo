//! Exact required receipt writes must fail closed, separately from business retries.
use super::*;
use brews_contracts::games::GameReceipt;
use brews_domain::{games::GameState, ids::PlayerId};

const RECEIPT_FAULTS: [(&str, &str); 9] = [
    (
        "readback_error",
        "AFTER INSERT ON game_receipts BEGIN UPDATE game_receipts SET outcome=CAST(x'80' AS TEXT) WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "fingerprint",
        "AFTER INSERT ON game_receipts BEGIN UPDATE game_receipts SET fingerprint=zeroblob(32) WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "expires_at",
        "AFTER INSERT ON game_receipts BEGIN UPDATE game_receipts SET expires_at=NEW.completed_at WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "completed_at",
        "AFTER INSERT ON game_receipts BEGIN UPDATE game_receipts SET completed_at=NEW.completed_at+1 WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "actor",
        "AFTER INSERT ON game_receipts BEGIN UPDATE game_receipts SET actor=NEW.actor||':changed' WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "command_id",
        "AFTER INSERT ON game_receipts BEGIN UPDATE game_receipts SET command_id='01890f3e-53b7-7d28-9b05-4f65092d57ff' WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "outcome",
        "AFTER INSERT ON game_receipts BEGIN UPDATE game_receipts SET outcome=json_set(NEW.outcome,'$.completed_at',NEW.completed_at+1) WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "ignore",
        "BEFORE INSERT ON game_receipts BEGIN SELECT RAISE(IGNORE); END",
    ),
    (
        "abort",
        "BEFORE INSERT ON game_receipts BEGIN SELECT RAISE(ABORT,'receipt fault'); END",
    ),
];

#[derive(Clone, Copy, Debug)]
enum Operation {
    ManualCall,
    RandomCall,
    Winner,
    StartedCancel,
    NewCancel,
    LobbyCancel,
    PlayerExit,
    AccountExit,
}

struct Prepared {
    f: Fixture,
    player: Option<brews_backend::game::SecretCookie>,
    winner: Option<PlayerId>,
    expected: u64,
    command: CommandId,
}
impl Prepared {
    fn new(operation: Operation) -> Self {
        let f = Fixture::new();
        let mut player = None;
        let mut winner = None;
        let mut expected = 0;
        match operation {
            Operation::NewCancel => {
                let config = GameConfiguration::default();
                let work = f
                    .directory()
                    .claim_game(
                        f.account,
                        f.command(10),
                        creation_fingerprint(&config).unwrap(),
                    )
                    .unwrap();
                let created = f
                    .service()
                    .initialize(
                        &work,
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        &config,
                    )
                    .unwrap();
                f.directory().acknowledge_creation(created.ready).unwrap();
            }
            Operation::LobbyCancel => {
                let code = f.open(&GameConfiguration::default());
                player = Some(f.player(&code, 20, "First"));
            }
            _ => {
                let (_, first, _, connections) = f.start_two_player_game();
                if matches!(operation, Operation::Winner) {
                    for value in 1..=4_u8 {
                        let revision = f
                            .service()
                            .account_view(
                                f.auth().authorize_game_account(&f.token).unwrap(),
                                &connections,
                            )
                            .unwrap()
                            .view_revision();
                        f.service()
                            .call_manual_value(
                                f.auth().authorize_game_account(&f.token).unwrap(),
                                f.command(50 + value),
                                &value.to_string(),
                                revision,
                            )
                            .unwrap();
                    }
                    winner = Some(match f.service().player_view(first.token()).unwrap() {
                        GameView::Player {
                            player_id,
                            board: Some(board),
                            ..
                        } => {
                            assert!(board.qualified);
                            player_id
                        }
                        _ => panic!("Expected qualifying player"),
                    });
                }
                expected = f
                    .service()
                    .account_view(
                        f.auth().authorize_game_account(&f.token).unwrap(),
                        &connections,
                    )
                    .unwrap()
                    .view_revision();
                if matches!(operation, Operation::PlayerExit | Operation::AccountExit) {
                    let account = f
                        .service()
                        .prepare_account_connection(
                            f.auth().authorize_game_account(&f.token).unwrap(),
                            id(&f.rt, 42).parse().unwrap(),
                        )
                        .unwrap();
                    f.service()
                        .accept_account_connection(
                            &account,
                            f.auth().authorize_game_account(&f.token).unwrap(),
                        )
                        .unwrap();
                    end_without_winner(&f, f.command(80));
                }
                player = Some(first);
            }
        }
        let command = f.command(90);
        Self {
            f,
            player,
            winner,
            expected,
            command,
        }
    }
    fn execute(&self, operation: Operation) -> Result<GameResponse, GameError> {
        let authority = self.f.auth().authorize_game_account(&self.f.token).unwrap();
        let service = self.f.service();
        match operation {
            Operation::ManualCall => {
                service.call_manual_value(authority, self.command, "1", self.expected)
            }
            Operation::RandomCall => service.call_random(authority, self.command, self.expected),
            Operation::Winner => {
                service.submit_winner(authority, self.command, self.winner.unwrap(), self.expected)
            }
            Operation::StartedCancel => {
                service.cancel_game(authority, self.command, GameState::InProgress, true)
            }
            Operation::NewCancel => {
                service.cancel_game(authority, self.command, GameState::New, true)
            }
            Operation::LobbyCancel => {
                service.cancel_game(authority, self.command, GameState::AwaitingPlayers, true)
            }
            Operation::PlayerExit => {
                service.exit_player(self.player.as_ref().unwrap().token(), self.command)
            }
            Operation::AccountExit => service.exit_account(authority, self.command),
        }
    }
}
fn receipt(response: GameResponse) -> GameReceipt {
    match response {
        GameResponse::CallAccepted { receipt, .. }
        | GameResponse::Terminalized { receipt, .. }
        | GameResponse::Exited { receipt, .. }
        | GameResponse::Committed { receipt } => receipt,
        _ => panic!("Expected receipt"),
    }
}
fn required_receipt_faults(operation: Operation) {
    // Run all cells even on RED, without printing private rows or credentials.
    let mut failures = 0;
    for (field, trigger) in RECEIPT_FAULTS {
        let prepared = Prepared::new(operation);
        let f = &prepared.f;
        f.game
            .conn
            .borrow()
            .execute_batch(&format!("CREATE TRIGGER receipt_fault {trigger}"))
            .unwrap();
        let before = all_rows(f);
        f.rt.now.set(f.rt.now.get() + 1);
        let error = prepared.execute(operation).err();
        let unchanged = all_rows(f) == before;
        if error != Some(GameError::Storage) || !unchanged {
            eprintln!(
                "required_receipt_fault operation={operation:?} scalar={field} error={error:?} durable_equal={unchanged}"
            );
            failures += 1;
            continue;
        }
        f.game
            .conn
            .borrow()
            .execute_batch("DROP TRIGGER receipt_fault")
            .unwrap();
        let original = receipt(prepared.execute(operation).unwrap());
        assert_eq!(original.command_id, prepared.command);
        assert_eq!(original.completed_at, f.rt.now.get());
        assert_eq!(
            original.expires_at,
            original.completed_at + brews_backend::game::RECEIPT_MS
        );
        let encoded = String::from_utf8(original.encode_json().unwrap()).unwrap();
        let rows = f.game.query("SELECT command_id,outcome,completed_at,expires_at FROM game_receipts WHERE command_id=?", &[SqlValue::Text(prepared.command.to_string())]).unwrap();
        assert!(
            rows == vec![vec![
                SqlValue::Text(prepared.command.to_string()),
                SqlValue::Text(encoded),
                SqlValue::Integer(original.completed_at),
                SqlValue::Integer(original.expires_at)
            ]],
            "Original retry must persist the exact receipt scalars"
        );
        if matches!(
            operation,
            Operation::Winner
                | Operation::StartedCancel
                | Operation::NewCancel
                | Operation::LobbyCancel
        ) {
            assert_eq!(count(f, "game_pending_work"), 1);
            assert_eq!(
                f.service().pending_work(1).unwrap()[0].kind(),
                brews_backend::game::WorkKind::Release
            );
            assert!(f.service().cleanup().unwrap().is_some());
        }
        let after = all_rows(f);
        if matches!(operation, Operation::PlayerExit) {
            assert_eq!(
                prepared.execute(operation).err(),
                Some(GameError::Unauthorized)
            );
        } else {
            let retry = receipt(prepared.execute(operation).unwrap());
            assert!(
                retry.encode_json().unwrap() == original.encode_json().unwrap(),
                "Exact retry must preserve completion and expiry"
            );
        }
        assert!(
            all_rows(f) == after,
            "Committed retry must not mutate durable rows"
        );
    }
    assert_eq!(
        failures, 0,
        "Required receipt readback must return Storage and roll back all Game tables including clock"
    );
}

#[test]
fn manual_call_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::ManualCall);
}
#[test]
fn random_call_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::RandomCall);
}
#[test]
fn winner_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::Winner);
}
#[test]
fn started_cancel_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::StartedCancel);
}
#[test]
fn new_cancel_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::NewCancel);
}
#[test]
fn lobby_cancel_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::LobbyCancel);
}
#[test]
fn player_exit_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::PlayerExit);
}
#[test]
fn account_exit_receipt_scalar_faults_roll_back_then_original_command_retries() {
    required_receipt_faults(Operation::AccountExit);
}
