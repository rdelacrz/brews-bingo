//! Publication receipt UPDATE is a required atomic write, not a business retry.
use super::*;
use brews_backend::game::{LobbyPreparation, PendingWork, WorkKind};
use brews_contracts::games::{GameOutcome, GameReceipt};

const RECEIPT_UPDATE_FAULTS: [(&str, &str); 11] = [
    (
        "fingerprint",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET fingerprint=zeroblob(32) WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "expires_at",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET expires_at=NEW.completed_at WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "completed_at",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET completed_at=NEW.completed_at+1 WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "actor",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET actor=NEW.actor||':changed' WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "command_id",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET command_id='01890f3e-53b7-7d28-9b05-4f65092d57ff' WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "outcome",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET outcome=json_set(NEW.outcome,'$.completed_at',NEW.completed_at+1) WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "outcome_bytes",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET outcome=NEW.outcome||' ' WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "read_error",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN UPDATE game_receipts SET outcome=CAST(x'80' AS TEXT) WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "missing",
        "AFTER UPDATE OF outcome ON game_receipts BEGIN DELETE FROM game_receipts WHERE actor=NEW.actor AND command_id=NEW.command_id; END",
    ),
    (
        "ignore",
        "BEFORE UPDATE OF outcome ON game_receipts BEGIN SELECT RAISE(IGNORE); END",
    ),
    (
        "abort",
        "BEFORE UPDATE OF outcome ON game_receipts BEGIN SELECT RAISE(ABORT,'publication receipt fault'); END",
    ),
];

fn all_rows(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .map(|table| {
            let columns = f
                .game
                .query(&format!("PRAGMA table_info({table})"), &[])
                .unwrap()
                .len();
            let order = (1..=columns)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(",");
            f.game
                .query(&format!("SELECT * FROM {table} ORDER BY {order}"), &[])
                .unwrap()
        })
        .collect()
}
fn scalar(f: &Fixture, sql: &str) -> i64 {
    match f.game.query(sql, &[]).unwrap()[0][0] {
        SqlValue::Integer(value) => value,
        _ => panic!("Expected scalar"),
    }
}
fn receipt_rows(f: &Fixture, command: CommandId) -> Vec<brews_backend::db::Row> {
    f.game
        .query(
            "SELECT actor,command_id,fingerprint,outcome,completed_at,expires_at FROM game_receipts WHERE command_id=?",
            &[SqlValue::Text(command.to_string())],
        )
        .unwrap()
}
fn unpublished_lobby(f: &Fixture) -> PendingWork {
    let config = GameConfiguration::default();
    let creation = f
        .directory()
        .claim_game(
            f.account,
            f.command(10),
            creation_fingerprint(&config).unwrap(),
        )
        .unwrap();
    let initialized = f
        .service()
        .initialize(
            &creation,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &config,
        )
        .unwrap();
    let ready = f
        .directory()
        .acknowledge_creation(initialized.ready)
        .unwrap();
    let intent = match f
        .service()
        .prepare_lobby(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(11),
            0,
        )
        .unwrap()
    {
        LobbyPreparation::Ready(work) => work,
        _ => panic!("Expected original lobby intent"),
    };
    let grant = f.directory().allocate_game_code(ready).unwrap();
    f.service()
        .commit_lobby(
            &intent,
            f.auth().authorize_game_account(&f.token).unwrap(),
            &grant,
        )
        .unwrap();
    let pending = f.service().pending_work(2).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind(), WorkKind::Lobby);
    assert_eq!(scalar(f, "SELECT published_at IS NULL FROM game_record"), 1);
    pending.into_iter().next().unwrap()
}

#[test]
fn receipt_update_faults_roll_back_then_original_ack_and_command_recover() {
    let mut failures = 0;
    for (field, trigger) in RECEIPT_UPDATE_FAULTS {
        let f = Fixture::new();
        assert_eq!(scalar(&f, "PRAGMA foreign_keys"), 1);
        let pending = unpublished_lobby(&f);
        let ack = f
            .directory()
            .publish_game(f.service().directory_projection().unwrap())
            .unwrap();
        let original_row = receipt_rows(&f, pending.command_id());
        assert_eq!(original_row.len(), 1);
        let original = match &original_row[0][3] {
            SqlValue::Text(raw) => GameReceipt::decode_json(raw.as_bytes()).unwrap(),
            _ => panic!("Expected original receipt encoding"),
        };
        let original_revision = scalar(
            &f,
            "SELECT revision FROM game_view_revisions WHERE view_key='host'",
        );
        let source_revision = scalar(&f, "SELECT revision FROM game_record");
        f.game
            .conn
            .borrow()
            .execute_batch(&format!("CREATE TRIGGER receipt_update_fault {trigger}"))
            .unwrap();
        let before = all_rows(&f);
        f.rt.now.set(f.rt.now.get() + 1);
        let error = f.service().acknowledge_projection(&pending, ack).err();
        // Direct SQL first: a service read could otherwise commit maintenance/clock changes.
        let unchanged = all_rows(&f) == before;
        eprintln!("publication_receipt field={field} error={error:?} durable_equal={unchanged}");
        f.game
            .conn
            .borrow()
            .execute_batch("DROP TRIGGER receipt_update_fault")
            .unwrap();
        if error != Some(GameError::Storage) || !unchanged {
            failures += 1;
            continue;
        }
        let committed = match f.service().acknowledge_projection(&pending, ack).unwrap() {
            GameResponse::LobbyOpened {
                view_revision,
                receipt,
                ..
            } => {
                assert_eq!(view_revision, (original_revision + 1) as u64);
                receipt
            }
            _ => panic!("Expected recovery with the original genuine Directory ACK"),
        };
        assert_eq!(committed.command_id, original.command_id);
        assert_eq!(committed.game_id, original.game_id);
        assert_eq!(committed.completed_at, original.completed_at);
        assert_eq!(committed.expires_at, original.expires_at);
        assert!(committed.completed_at < f.rt.now.get());
        assert!(matches!(
            committed.outcome,
            GameOutcome::LobbyOpened { view_revision } if view_revision == (original_revision + 1) as u64
        ));
        let encoded = committed.encode_json().unwrap();
        let mut expected_row = original_row;
        expected_row[0][3] = SqlValue::Text(String::from_utf8(encoded.clone()).unwrap());
        assert!(
            receipt_rows(&f, pending.command_id()) == expected_row,
            "Only outcome bytes may change; receipt identity/fingerprint/completion/expiry stay fixed"
        );
        assert_eq!(
            scalar(&f, "SELECT published_at FROM game_record"),
            f.rt.now.get()
        );
        assert_eq!(
            scalar(&f, "SELECT revision FROM game_record"),
            source_revision
        );
        assert_eq!(scalar(&f, "SELECT count(*) FROM game_pending_work"), 0);
        let after = all_rows(&f);
        let retry = f
            .service()
            .prepare_lobby(
                f.auth().authorize_game_account(&f.token).unwrap(),
                pending.command_id(),
                0,
            )
            .unwrap();
        let replayed = match retry {
            LobbyPreparation::Committed(receipt) => receipt,
            _ => panic!("Expected the original authorized command's exact receipt"),
        };
        assert!(replayed.encode_json().unwrap() == encoded);
        assert!(
            all_rows(&f) == after,
            "Exact authorized retry must not change any durable row or advance another revision"
        );
        assert_eq!(
            f.service().acknowledge_projection(&pending, ack).err(),
            Some(GameError::Conflict)
        );
        assert!(all_rows(&f) == after);
    }
    eprintln!(
        "publication_receipt_matrix_cases={} failures={failures}",
        RECEIPT_UPDATE_FAULTS.len()
    );
    assert_eq!(
        failures, 0,
        "Required receipt UPDATE readback must be Storage and roll back all Game tables including clock"
    );
}
