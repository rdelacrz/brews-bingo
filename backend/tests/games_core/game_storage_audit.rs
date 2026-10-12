//! Permanent real-SQLite regressions for the game/storage audit.
use super::*;
use brews_backend::game::WorkKind;
use brews_domain::ids::SessionId;

fn account_session(f: &Fixture, serial: u32, expires: Option<i64>) -> (String, SessionId) {
    let token = brews_backend::security::new_token(&f.rt).unwrap();
    let digest = brews_backend::security::token_digest(&token).unwrap();
    let mut bytes = [0; 16];
    bytes[..6].copy_from_slice(&(f.rt.now.get() as u64).to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    bytes[12..].copy_from_slice(&(1000 + serial).to_be_bytes());
    let session: SessionId = uuid::Uuid::from_bytes(bytes).to_string().parse().unwrap();
    // Session fixtures avoid hundreds of KDFs; all Game admission uses real Accounts proofs.
    f.accounts.execute("INSERT INTO account_sessions(session_id,account_id,token_verifier,scope,credential_epoch,issued_at,expires_at,revoked_at) SELECT ?,account_id,?,scope,credential_epoch,issued_at,COALESCE(?,expires_at),NULL FROM account_sessions WHERE token_verifier=?", &[SqlValue::Text(session.to_string()), SqlValue::Blob(digest.to_vec()), expires.map_or(SqlValue::Null, SqlValue::Integer), SqlValue::Blob(brews_backend::security::token_digest(&f.token).unwrap().to_vec())]).unwrap();
    (token, session)
}

#[test]
fn final_gameplay_expiry_denial_fences_exact_sample_and_rollback() {
    let f = Fixture::new();
    f.start_two_player_game();
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    let expires = proof.expires_at();
    let before = all_rows(&f);
    f.rt.now.set(expires);
    assert_eq!(
        f.service().authorize_gameplay_response(proof),
        Err(GameError::Unauthorized)
    );
    assert_eq!(
        f.game
            .query("SELECT last_observed_ms FROM game_metadata", &[])
            .unwrap(),
        vec![vec![SqlValue::Integer(expires)]]
    );
    let after = all_rows(&f);
    for (index, table) in brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .enumerate()
    {
        if *table != "game_metadata" {
            assert_eq!(before[index], after[index], "{table}");
        }
    }
    f.rt.now.set(expires - 1);
    let fresh = f.auth().authorize_game_account(&f.token).unwrap();
    assert_eq!(
        f.service()
            .authorize_gameplay_response(f.auth().authorize_game_account(&f.token).unwrap()),
        Err(GameError::Storage)
    );
    assert_eq!(
        f.service().call_random(fresh, f.command(99), 0).err(),
        Some(GameError::Storage)
    );
}

#[test]
fn final_gameplay_expiry_fence_ignored_or_failed_write_is_unavailable() {
    for mode in ["IGNORE", "ABORT,'test-only fence failure'"] {
        let f = Fixture::new();
        f.start_two_player_game();
        let proof = f.auth().authorize_game_account(&f.token).unwrap();
        let before = all_rows(&f);
        f.game.execute(&format!("CREATE TRIGGER audit_fence BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE({mode}); END"), &[]).unwrap();
        f.rt.now.set(proof.expires_at());
        assert_eq!(
            f.service().authorize_gameplay_response(proof),
            Err(GameError::Storage)
        );
        assert_eq!(all_rows(&f), before);
    }
}

#[test]
fn shared_grant_capacity_preserves_accepted_viewers_and_allows_both_terminal_outcomes() {
    for winner in [false, true] {
        let f = Fixture::new();
        let (_, first, second, connections) = f.start_two_player_game();
        let mut viewers = Vec::new();
        for serial in 0..197 {
            let (token, _) = account_session(&f, serial, None);
            f.service()
                .account_view(
                    f.auth().authorize_game_account(&token).unwrap(),
                    &connections,
                )
                .unwrap();
            viewers.push(token);
        }
        // Two player preparations share the same budget as 198 accepted account anchors.
        let old = f
            .service()
            .prepare_player_connection(first.token(), id(&f.rt, 80).parse().unwrap())
            .unwrap();
        let replacement = f
            .service()
            .prepare_player_connection(first.token(), id(&f.rt, 81).parse().unwrap())
            .unwrap();
        f.service()
            .prepare_player_connection(second.token(), id(&f.rt, 82).parse().unwrap())
            .unwrap();
        assert_eq!(count(&f, "game_connection_grants"), 200);
        assert_eq!(
            f.service().accept_player_connection(&old),
            Err(GameError::Unauthorized)
        );
        let (overflow, _) = account_session(&f, 198, None);
        let before = all_rows(&f);
        assert_eq!(
            f.service()
                .account_view(
                    f.auth().authorize_game_account(&overflow).unwrap(),
                    &connections
                )
                .err(),
            Some(GameError::Capacity)
        );
        assert_eq!(
            f.service()
                .prepare_account_connection(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    id(&f.rt, 83).parse().unwrap()
                )
                .err(),
            Some(GameError::Capacity)
        );
        assert_eq!(all_rows(&f), before);
        // Existing HTTP access and preparation replacement remain available at the bound.
        f.service()
            .account_view(
                f.auth().authorize_game_account(&viewers[0]).unwrap(),
                &connections,
            )
            .unwrap();
        f.service()
            .prepare_player_connection(first.token(), id(&f.rt, 84).parse().unwrap())
            .unwrap();
        assert_eq!(
            f.service().accept_player_connection(&replacement),
            Err(GameError::Unauthorized)
        );
        if winner {
            for (n, value) in ["1", "2", "3", "4"].iter().enumerate() {
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
                        f.command(60 + n as u8),
                        value,
                        revision,
                    )
                    .unwrap();
            }
            let player = match f.service().player_view(first.token()).unwrap() {
                GameView::Player { player_id, .. } => player_id,
                _ => panic!("player"),
            };
            let revision = f
                .service()
                .account_view(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    &connections,
                )
                .unwrap()
                .view_revision();
            f.service()
                .submit_winner(
                    f.auth().authorize_game_account(&f.token).unwrap(),
                    f.command(90),
                    player,
                    revision,
                )
                .unwrap();
        } else {
            end_without_winner(&f, f.command(90));
        }
        assert_eq!(count(&f, "game_history"), 1);
        assert_eq!(count(&f, "game_connection_grants"), 200);
        for token in viewers {
            f.service()
                .account_view(f.auth().authorize_game_account(&token).unwrap(), &[])
                .unwrap();
        }
        let work = f
            .service()
            .pending_work(100)
            .unwrap()
            .into_iter()
            .find(|work| work.kind() == WorkKind::Release)
            .unwrap();
        let ack = f
            .directory()
            .release_game(f.service().cleanup().unwrap().unwrap())
            .unwrap();
        f.service().acknowledge_release(&work, ack).unwrap();
        assert!(
            f.directory()
                .confirm_reservation(f.service().directory_projection().unwrap().game_id())
                .is_err()
        );
    }
}

#[test]
fn grant_expiry_reclaims_capacity_and_account_reconnect_reuses_preparation_slot() {
    let f = Fixture::new();
    let (_, first, _, connections) = f.start_two_player_game();
    let mut last = None;
    for serial in 0..198 {
        let (token, _) = account_session(&f, serial, (serial == 0).then_some(f.rt.now.get() + 10));
        f.service()
            .account_view(
                f.auth().authorize_game_account(&token).unwrap(),
                &connections,
            )
            .unwrap();
        last = Some(token);
    }
    let last = last.unwrap();
    let old = f
        .service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&last).unwrap(),
            id(&f.rt, 80).parse().unwrap(),
        )
        .unwrap();
    f.service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&last).unwrap(),
            id(&f.rt, 81).parse().unwrap(),
        )
        .unwrap();
    assert_eq!(count(&f, "game_connection_grants"), 200);
    assert_eq!(
        f.service()
            .prepare_player_connection(first.token(), id(&f.rt, 82).parse().unwrap())
            .err(),
        Some(GameError::Capacity)
    );
    let (new, _) = account_session(&f, 199, None);
    assert_eq!(
        f.service()
            .prepare_account_connection(
                f.auth().authorize_game_account(&new).unwrap(),
                id(&f.rt, 83).parse().unwrap()
            )
            .err(),
        Some(GameError::Capacity)
    );
    assert_eq!(count(&f, "game_connection_grants"), 200);
    f.rt.now.set(f.rt.now.get() + 10);
    f.service()
        .account_view(f.auth().authorize_game_account(&new).unwrap(), &connections)
        .unwrap();
    assert_eq!(count(&f, "game_connection_grants"), 200);
    assert_eq!(
        f.service()
            .accept_account_connection(&old, f.auth().authorize_game_account(&last).unwrap()),
        Err(GameError::Unauthorized)
    );
    end_without_winner(&f, f.command(90));
}

#[test]
fn prepared_only_recovery_queues_exact_predecessor_and_retry_deadlines_survive_reconstruction() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let old = f.player(&code, 30, "RecoverPlayer");
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), old.token(), Some(&answer))
        .unwrap();
    let grant = f
        .service()
        .prepare_player_connection(old.token(), id(&f.rt, 40).parse().unwrap())
        .unwrap();
    let successor = f
        .service()
        .recover_player(f.command(32), &code, "RecoverPlayer", &answer, "192.0.2.41")
        .unwrap()
        .cookie
        .unwrap();
    let close = f
        .service()
        .pending_recovery_socket_closes()
        .unwrap()
        .remove(0);
    assert_eq!(
        (close.connection, close.session, close.player, close.epoch),
        (
            grant.connection_id(),
            grant.session_id(),
            grant.player_id(),
            grant.epoch()
        )
    );
    assert_eq!(count(&f, "game_connection_grants"), 0);
    assert_eq!(
        f.service().accept_player_connection(&grant),
        Err(GameError::Unauthorized)
    );
    f.service().retry_recovery_socket_close(&close).unwrap();
    let retry = f
        .service()
        .pending_recovery_socket_closes()
        .unwrap()
        .remove(0);
    assert!(f.service().due_recovery_socket_closes().unwrap().is_empty());
    f.service().retry_recovery_socket_close(&close).unwrap(); // stale snapshot
    f.service().retry_recovery_socket_close(&retry).unwrap(); // early snapshot
    assert_eq!(
        f.service().pending_recovery_socket_closes().unwrap(),
        vec![retry.clone()]
    );
    let new = f
        .service()
        .prepare_player_connection(successor.token(), id(&f.rt, 41).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&new).unwrap();
    f.rt.now.set(retry.next_attempt_at - 1);
    assert!(f.service().due_recovery_socket_closes().unwrap().is_empty());
    f.rt.now.set(retry.next_attempt_at);
    assert_eq!(
        f.service().due_recovery_socket_closes().unwrap(),
        vec![retry.clone()]
    );
    f.service().retry_recovery_socket_close(&retry).unwrap();
    let next = f
        .service()
        .pending_recovery_socket_closes()
        .unwrap()
        .remove(0);
    assert_eq!(next.attempt_count, 2);
    assert_eq!(next.next_attempt_at, f.rt.now.get() + 4000);
    // ACK identifies the target, independently of a stale retry snapshot or future deadline.
    f.service()
        .acknowledge_recovery_socket_close(&close)
        .unwrap();
    f.service().retry_recovery_socket_close(&next).unwrap();
    assert!(
        f.service()
            .pending_recovery_socket_closes()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.service().player_connection(new.connection_id()).unwrap(),
        Some(new)
    );
    assert!(f.service().player_view(successor.token()).is_ok());
}

#[test]
fn current_connection_capacity_preserves_existing_bindings_and_reclaims_expired_slot() {
    let f = Fixture::new();
    let (_, first, _, _) = f.start_two_player_game();
    for serial in 0..98 {
        let (token, _) = account_session(&f, serial, (serial == 0).then_some(f.rt.now.get() + 10));
        let mut bytes = [0; 16];
        bytes[..6].copy_from_slice(&(f.rt.now.get() as u64).to_be_bytes()[2..]);
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        bytes[12..].copy_from_slice(&(3000 + serial).to_be_bytes());
        let connection = uuid::Uuid::from_bytes(bytes).to_string().parse().unwrap();
        let grant = f
            .service()
            .prepare_account_connection(
                f.auth().authorize_game_account(&token).unwrap(),
                connection,
            )
            .unwrap();
        f.service()
            .accept_account_connection(&grant, f.auth().authorize_game_account(&token).unwrap())
            .unwrap();
    }
    assert_eq!(count(&f, "game_current_connections"), 100);
    let (token, _) = account_session(&f, 100, None);
    let grant = f
        .service()
        .prepare_account_connection(
            f.auth().authorize_game_account(&token).unwrap(),
            id(&f.rt, 80).parse().unwrap(),
        )
        .unwrap();
    let before = all_rows(&f);
    assert_eq!(
        f.service()
            .accept_account_connection(&grant, f.auth().authorize_game_account(&token).unwrap()),
        Err(GameError::Capacity)
    );
    assert_eq!(all_rows(&f), before);
    let reconnect = f
        .service()
        .prepare_player_connection(first.token(), id(&f.rt, 81).parse().unwrap())
        .unwrap();
    f.service().accept_player_connection(&reconnect).unwrap();
    assert_eq!(count(&f, "game_current_connections"), 100);
    f.rt.now.set(f.rt.now.get() + 10);
    f.service()
        .accept_account_connection(&grant, f.auth().authorize_game_account(&token).unwrap())
        .unwrap();
    assert_eq!(count(&f, "game_current_connections"), 100);
    end_without_winner(&f, f.command(90));
    assert_eq!(count(&f, "game_history"), 1);
}
