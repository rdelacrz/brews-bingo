//! Pending Start publication follows accepted gameplay atomically, not abandoned work.
use super::*;
use brews_backend::game::{PendingWork, WorkKind};

fn pending_start(f: &Fixture) -> PendingWork {
    let work = f.service().pending_work(2).unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].kind(), WorkKind::StartProjection);
    work.into_iter().next().unwrap()
}
fn pending_row(f: &Fixture) -> Vec<brews_backend::db::Row> {
    f.game
        .query("SELECT * FROM game_pending_work ORDER BY operation_id", &[])
        .unwrap()
}
fn start_receipt_row(f: &Fixture, work: &PendingWork) -> Vec<brews_backend::db::Row> {
    f.game
        .query(
            "SELECT * FROM game_receipts WHERE command_id=?",
            &[SqlValue::Text(work.command_id().to_string())],
        )
        .unwrap()
}
fn host_revision(f: &Fixture, connections: &[ConnectionId]) -> u64 {
    f.service()
        .account_view(
            f.auth().authorize_game_account(&f.token).unwrap(),
            connections,
        )
        .unwrap()
        .view_revision()
}
fn directory_rows(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    brews_backend::db::schema::directory_schema::DIRECTORY_TABLES
        .iter()
        .map(|table| {
            // Single-column tables require ORDER BY 1; all columns make row ordering exact.
            let columns = f
                .directory
                .query(&format!("PRAGMA table_info({table})"), &[])
                .unwrap()
                .len();
            let order = (1..=columns)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(",");
            f.directory
                .query(&format!("SELECT * FROM {table} ORDER BY {order}"), &[])
                .unwrap()
        })
        .collect()
}

#[test]
fn accepted_calls_reconcile_pending_start_and_reject_stale_real_acknowledgements() {
    let f = Fixture::new();
    let (_, _, _, connections) = f.start_two_player_game();
    let original = pending_start(&f);
    let original_projection = f.service().directory_projection().unwrap();
    let original_receipt = start_receipt_row(&f, &original);
    let mut old_ack = f
        .directory()
        .publish_game(original_projection.clone())
        .unwrap();
    let expected_start = match &original_receipt[0][3] {
        SqlValue::Text(raw) => {
            match brews_contracts::games::GameReceipt::decode_json(raw.as_bytes())
                .unwrap()
                .outcome
            {
                brews_contracts::games::GameOutcome::Started { view_revision, .. } => {
                    view_revision - 1
                }
                _ => panic!("Expected original Start receipt"),
            }
        }
        _ => panic!("Expected encoded receipt"),
    };
    let reservation = f
        .directory()
        .confirm_reservation(original_projection.game_id())
        .unwrap();
    let original_row = pending_row(&f);
    let mut previous = original.clone();
    for value in 1..=3_u8 {
        let expected = host_revision(&f, &connections);
        let command = f.command(50 + value);
        f.rt.now.set(f.rt.now.get() + 1);
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &value.to_string(),
                expected,
            )
            .unwrap();
        let projection = f.service().directory_projection().unwrap();
        let current = pending_start(&f);
        let durable_work = pending_row(&f);
        let before = all_rows(&f);
        let directory_before = directory_rows(&f);
        assert_eq!(
            f.service().acknowledge_projection(&previous, old_ack).err(),
            Some(GameError::Conflict)
        );
        assert_eq!(
            f.service().acknowledge_projection(&current, old_ack).err(),
            Some(GameError::Conflict)
        );
        assert!(
            all_rows(&f) == before,
            "A stale real ACK must preserve successor work and all owner rows"
        );
        assert!(directory_rows(&f) == directory_before);
        assert!(
            start_receipt_row(&f, &original) == original_receipt,
            "Calls must preserve original Start outcome/completion/expiry"
        );
        let retry = f
            .service()
            .start(
                f.auth().authorize_game_account(&f.token).unwrap(),
                original.command_id(),
                expected_start,
                &reservation,
                &connections,
            )
            .unwrap();
        assert!(matches!(retry, GameResponse::Committed { .. }));
        assert!(
            all_rows(&f) == before,
            "Original Start retry must not regenerate boards or change receipts/work"
        );
        let call_retry = f
            .service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &value.to_string(),
                expected,
            )
            .unwrap();
        assert!(matches!(call_retry, GameResponse::Committed { .. }));
        assert!(
            all_rows(&f) == before,
            "Call retry must not advance the successor fence again"
        );
        let current_ack = f.directory().publish_game(projection.clone()).unwrap();
        assert_eq!(
            f.service()
                .acknowledge_projection(&previous, current_ack)
                .err(),
            Some(GameError::Conflict),
            "An obsolete work snapshot cannot clear successor publication even with a current ACK"
        );
        assert!(all_rows(&f) == before);
        if value == 3 {
            let error = f
                .service()
                .acknowledge_projection(&current, current_ack)
                .err();
            eprintln!(
                "start_publication source_revision={} pending_fence={} current_ack_error={error:?}",
                projection.source_revision(),
                current.fence_revision()
            );
            assert_eq!(
                error, None,
                "A genuine current projection ACK must recover pending Start after calls"
            );
            assert!(f.service().pending_work(2).unwrap().is_empty());
        }
        let mut expected_row = original_row.clone();
        expected_row[0][7] = SqlValue::Integer(projection.source_revision());
        assert!(
            durable_work == expected_row,
            "Only pending Start source fence changes; original intent and retry metadata remain fixed"
        );
        previous = current;
        old_ack = current_ack;
    }
    assert!(start_receipt_row(&f, &original) == original_receipt);
}

#[test]
fn pending_start_fence_required_write_faults_roll_back_calls_and_exact_retries() {
    for random in [false, true] {
        for trigger in [
            "AFTER UPDATE ON game_pending_work BEGIN UPDATE game_pending_work SET command_id=upper(NEW.command_id) WHERE operation_id=NEW.operation_id; END",
            "BEFORE UPDATE ON game_pending_work BEGIN SELECT RAISE(IGNORE); END",
            "BEFORE UPDATE ON game_pending_work BEGIN SELECT RAISE(ABORT,'publication fault'); END",
            "AFTER UPDATE ON game_pending_work BEGIN UPDATE game_pending_work SET fence_revision=OLD.fence_revision WHERE operation_id=NEW.operation_id; END",
            "AFTER UPDATE ON game_pending_work BEGIN UPDATE game_pending_work SET next_attempt_at=NEW.next_attempt_at+1 WHERE operation_id=NEW.operation_id; END",
        ] {
            let f = Fixture::new();
            let (_, _, _, connections) = f.start_two_player_game();
            let original = pending_start(&f);
            let original_receipt = start_receipt_row(&f, &original);
            let expected = host_revision(&f, &connections);
            let command = f.command(90);
            let before = all_rows(&f);
            f.game
                .conn
                .borrow()
                .execute_batch(&format!("CREATE TRIGGER publication_fault {trigger}"))
                .unwrap();
            f.rt.now.set(f.rt.now.get() + 1);
            let call = || {
                let authority = f.auth().authorize_game_account(&f.token).unwrap();
                if random {
                    f.service().call_random(authority, command, expected)
                } else {
                    f.service()
                        .call_manual_value(authority, command, "1", expected)
                }
            };
            assert_eq!(
                call().err(),
                Some(GameError::Storage),
                "A required pending-fence write cannot partially commit a call"
            );
            assert!(
                all_rows(&f) == before,
                "Pending fence fault must roll back all app tables and clock"
            );
            f.game
                .conn
                .borrow()
                .execute_batch("DROP TRIGGER publication_fault")
                .unwrap();
            assert!(matches!(call().unwrap(), GameResponse::CallAccepted { .. }));
            assert!(start_receipt_row(&f, &original) == original_receipt);
            let projection = f.service().directory_projection().unwrap();
            let mut current = pending_start(&f);
            assert_eq!(current.fence_revision(), projection.source_revision());
            for _ in 0..3 {
                current = f.service().retry_work(&current).unwrap();
            }
            assert_eq!(current.attempt_count(), 3);
            assert_eq!(current.fence_revision(), projection.source_revision());
            let ack = f.directory().publish_game(projection).unwrap();
            f.service().acknowledge_projection(&current, ack).unwrap();
            assert!(f.service().pending_work(2).unwrap().is_empty());
            let after = all_rows(&f);
            assert!(matches!(call().unwrap(), GameResponse::Committed { .. }));
            assert!(all_rows(&f) == after);
        }
    }
}
