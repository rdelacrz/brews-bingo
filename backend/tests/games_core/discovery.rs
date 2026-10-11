//! Discovery uses real owning SQLite state and never enters account/player views.
use super::*;
use brews_domain::games::GameState;
fn all_rows(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .map(|table| {
            f.game
                .query(&format!("SELECT * FROM {table} ORDER BY 1"), &[])
                .unwrap()
        })
        .collect()
}

use brews_backend::db::game::{DiscoveryValue, DiscoveryViewer};

fn summary(f: &Fixture) -> brews_backend::db::game::DiscoveryCut {
    let routing = f.directory().discovery_route().unwrap().unwrap();
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    f.service()
        .discovery(
            routing.game_id,
            DiscoveryViewer::Summary(&proof),
            routing.published_code.as_ref(),
            true,
            &[],
        )
        .unwrap()
}
#[test]
fn discovery_read_only_cuts_hide_allocations_and_do_not_create_grants_or_timers() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let route = f.directory().discovery_route().unwrap().unwrap();
    assert_eq!(route.published_code, Some(code.clone()));
    let before = all_rows(&f);
    f.rt.now.set(f.rt.now.get() + 1);
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    let cut = f
        .service()
        .discovery(
            route.game_id,
            DiscoveryViewer::Summary(&proof),
            Some(&code),
            true,
            &[],
        )
        .unwrap();
    let DiscoveryValue::Summary(Some(game)) = cut.value else {
        panic!("safe live summary required")
    };
    assert_eq!(game.state, GameState::AwaitingPlayers);
    assert!(game.may_operate);
    assert_eq!(game.game_code, Some(code.clone()));
    let value = serde_json::to_value(game).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 9);
    let cut = f
        .service()
        .discovery(
            route.game_id,
            DiscoveryViewer::Summary(&proof),
            None,
            true,
            &[],
        )
        .unwrap();
    let DiscoveryValue::Summary(Some(game)) = cut.value else {
        panic!("safe live summary required")
    };
    assert!(
        game.game_code.is_none(),
        "Directory publication is required too"
    );
    for viewer in [
        DiscoveryViewer::Entry(&code),
        DiscoveryViewer::Account(&proof),
    ] {
        f.service()
            .discovery(route.game_id, viewer, Some(&code), true, &[])
            .unwrap();
    }
    assert!(
        all_rows(&f) == before,
        "all successful reads must be write-free including metadata"
    );
    assert_eq!(
        f.game
            .query(
                "SELECT count(*) FROM game_connection_grants WHERE viewer_kind='account'",
                &[]
            )
            .unwrap(),
        vec![vec![SqlValue::Integer(0)]]
    );
}
#[test]
fn discovery_availability_counts_valid_observed_connections_without_reconciliation() {
    let f = Fixture::new();
    let config = GameConfiguration {
        player_capacity: 2,
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let game = f.directory().discovery_route().unwrap().unwrap().game_id;
    let first = f.player(&code, 20, "First");
    let second = f.player(&code, 21, "Second");
    let grants = [
        f.service()
            .prepare_player_connection(first.token(), id(&f.rt, 40).parse().unwrap())
            .unwrap(),
        f.service()
            .prepare_player_connection(second.token(), id(&f.rt, 41).parse().unwrap())
            .unwrap(),
    ];
    for grant in &grants {
        f.service().accept_player_connection(grant).unwrap();
    }
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    let before = all_rows(&f);
    for (observed, expected) in [(&grants[..], 2), (&grants[..1], 1), (&grants[..0], 0)] {
        let cut = f
            .service()
            .discovery(
                game,
                DiscoveryViewer::Account(&proof),
                Some(&code),
                true,
                observed,
            )
            .unwrap();
        let DiscoveryValue::Availability(brews_contracts::discovery::GameAvailability::Account {
            player_count,
            connected_player_count,
            can_start,
            can_join_player,
            can_join_spectator,
            ..
        }) = cut.value
        else {
            panic!("account availability")
        };
        assert_eq!(player_count, 2);
        assert_eq!(connected_player_count, expected);
        assert_eq!(can_start, expected == 2);
        assert!(!can_join_player);
        assert!(!can_join_spectator);
    }
    let cut = f
        .service()
        .discovery(
            game,
            DiscoveryViewer::Player(first.token()),
            Some(&code),
            true,
            &[],
        )
        .unwrap();
    let DiscoveryValue::Availability(availability) = cut.value else {
        panic!("own player availability")
    };
    assert_eq!(
        serde_json::to_value(availability)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        4
    );
    assert!(all_rows(&f) == before);
    f.game
        .execute(
            "UPDATE game_sessions SET revoked_at=? WHERE session_id=?",
            &[
                SqlValue::Integer(f.rt.now.get()),
                SqlValue::Text(grants[0].session_id().to_string()),
            ],
        )
        .unwrap();
    let before = all_rows(&f);
    let cut = f
        .service()
        .discovery(
            game,
            DiscoveryViewer::Account(&proof),
            Some(&code),
            true,
            &grants,
        )
        .unwrap();
    assert!(matches!(
        cut.value,
        DiscoveryValue::Availability(brews_contracts::discovery::GameAvailability::Account {
            connected_player_count: 1,
            can_start: false,
            ..
        })
    ));
    assert!(
        all_rows(&f) == before,
        "invalid presence is excluded without removal"
    );
    assert_eq!(
        f.service()
            .discovery(
                game,
                DiscoveryViewer::Player(first.token()),
                Some(&code),
                true,
                &[]
            )
            .err(),
        Some(GameError::Unauthorized)
    );
}
#[test]
fn discovery_idle_denial_is_durable_write_free_recut_and_rollback_fenced() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let route = f.directory().discovery_route().unwrap().unwrap();
    let cut = summary(&f);
    let DiscoveryValue::Summary(Some(game)) = cut.value else {
        panic!("live")
    };
    let due = game.idle_due.unwrap();
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    // Isolate the idle boundary from account fixed expiry (fresh authority at due).
    f.game
        .execute(
            "UPDATE game_record SET idle_due=?,last_host_activity=?",
            &[
                SqlValue::Integer(f.rt.now.get() + 1000),
                SqlValue::Integer(f.rt.now.get() + 1000 - brews_backend::game::HOST_IDLE_MS),
            ],
        )
        .unwrap();
    let observed = f.rt.now.get() + 1000;
    assert!(observed < due);
    f.rt.now.set(observed);
    let cut = f
        .service()
        .discovery(
            route.game_id,
            DiscoveryViewer::Summary(&proof),
            Some(&code),
            true,
            &[],
        )
        .unwrap();
    assert!(cut.requires_sync);
    assert!(matches!(cut.value, DiscoveryValue::Summary(None)));
    let before = all_rows(&f);
    let cut = f
        .service()
        .discovery(
            route.game_id,
            DiscoveryViewer::Summary(&proof),
            Some(&code),
            true,
            &[],
        )
        .unwrap();
    assert!(!cut.requires_sync);
    assert!(all_rows(&f) == before);
    f.rt.now.set(observed - 1);
    assert_eq!(
        f.service()
            .discovery(
                route.game_id,
                DiscoveryViewer::Entry(&code),
                Some(&code),
                true,
                &[]
            )
            .err(),
        Some(GameError::Storage)
    );
}
#[test]
fn discovery_fence_storage_fault_is_unavailable_and_rolls_back() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let route = f.directory().discovery_route().unwrap().unwrap();
    let observed = f.rt.now.get() + 1000;
    f.game
        .execute(
            "UPDATE game_record SET idle_due=?",
            &[SqlValue::Integer(observed)],
        )
        .unwrap();
    f.game.conn.borrow().execute_batch("CREATE TRIGGER discovery_clock_fault BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END").unwrap();
    let before = all_rows(&f);
    f.rt.now.set(observed);
    assert_eq!(
        f.service()
            .discovery(
                route.game_id,
                DiscoveryViewer::Entry(&code),
                Some(&code),
                true,
                &[]
            )
            .err(),
        Some(GameError::Storage)
    );
    assert!(all_rows(&f) == before);
}
#[test]
fn discovery_authoritative_state_advances_without_directory_revision_equality() {
    let f = Fixture::new();
    let (code, first, _, grants) = f.start_two_player_game();
    let route = f.directory().discovery_route().unwrap().unwrap();
    assert!(route.published_code.is_some());
    let cut = summary(&f);
    assert!(
        matches!(cut.value, DiscoveryValue::Summary(Some(game)) if game.state == GameState::InProgress)
    );
    f.service()
        .discovery(
            route.game_id,
            DiscoveryViewer::Player(first.token()),
            Some(&code),
            true,
            &[],
        )
        .unwrap();
    let _ = grants;
    f.service()
        .cancel_game(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(90),
            GameState::InProgress,
            true,
        )
        .unwrap();
    let cut = summary(&f);
    assert!(matches!(cut.value, DiscoveryValue::Summary(None)));
    assert_eq!(
        f.service()
            .discovery(
                route.game_id,
                DiscoveryViewer::Entry(&code),
                Some(&code),
                true,
                &[]
            )
            .err(),
        Some(GameError::NotFound)
    );
}

#[test]
fn discovery_player_deadline_denial_survives_clock_rollback_without_other_writes() {
    let f = Fixture::new();
    let (code, first, _, _) = f.start_two_player_game();
    let game = f.directory().discovery_route().unwrap().unwrap().game_id;
    let expiry = first.expires_at();
    let before = all_rows(&f);
    f.rt.now.set(expiry);
    assert_eq!(
        f.service()
            .discovery(
                game,
                DiscoveryViewer::Player(first.token()),
                Some(&code),
                true,
                &[]
            )
            .err(),
        Some(GameError::Unauthorized)
    );
    let after = all_rows(&f);
    for (name, (before, after)) in brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .zip(before.iter().zip(&after))
    {
        if *name != "game_metadata" {
            assert_eq!(after, before, "player expiry cannot change {name}");
        }
    }
    f.rt.now.set(expiry - 1);
    assert_eq!(
        f.service()
            .discovery(
                game,
                DiscoveryViewer::Player(first.token()),
                Some(&code),
                true,
                &[]
            )
            .err(),
        Some(GameError::Storage)
    );
}

#[test]
fn discovery_start_availability_checks_retained_layout_feasibility_without_entropy() {
    use brews_domain::games::CellPosition;
    let f = Fixture::new();
    let configuration = GameConfiguration {
        numeric_upper_bound: 1,
        board_side_length: 2,
        free_cells_enabled: true,
        free_cell_positions: vec![
            CellPosition { row: 1, column: 1 },
            CellPosition { row: 1, column: 2 },
            CellPosition { row: 2, column: 1 },
        ],
        player_capacity: 2,
        ..GameConfiguration::default()
    };
    let code = f.open(&configuration);
    let game = f.directory().discovery_route().unwrap().unwrap().game_id;
    let first = f.player(&code, 20, "First");
    let second = f.player(&code, 21, "Second");
    let grants = [
        f.service()
            .prepare_player_connection(first.token(), id(&f.rt, 40).parse().unwrap())
            .unwrap(),
        f.service()
            .prepare_player_connection(second.token(), id(&f.rt, 41).parse().unwrap())
            .unwrap(),
    ];
    for grant in &grants {
        f.service().accept_player_connection(grant).unwrap();
    }
    let before = all_rows(&f);
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    let cut = f
        .service()
        .discovery(
            game,
            DiscoveryViewer::Account(&proof),
            Some(&code),
            true,
            &grants,
        )
        .unwrap();
    assert!(matches!(
        cut.value,
        DiscoveryValue::Availability(brews_contracts::discovery::GameAvailability::Account {
            connected_player_count: 2,
            can_start: false,
            ..
        })
    ));
    assert_eq!(
        f.service()
            .discovery(
                game,
                DiscoveryViewer::Account(&proof),
                Some(&code),
                false,
                &grants
            )
            .err(),
        Some(GameError::NotFound)
    );
    assert_eq!(all_rows(&f), before);
}

#[test]
fn discovery_remote_expiry_fences_the_original_sample_without_other_writes() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let game = f.directory().discovery_route().unwrap().unwrap().game_id;
    let deadline = f
        .service()
        .discovery(game, DiscoveryViewer::Entry(&code), Some(&code), true, &[])
        .unwrap()
        .valid_until
        .unwrap();
    let before = all_rows(&f);
    f.rt.now.set(deadline - 1);
    f.service()
        .fence_discovery_expiry(game, deadline, deadline + 7)
        .unwrap();
    let after = all_rows(&f);
    for (name, (before, after)) in brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .zip(before.iter().zip(&after))
    {
        if *name != "game_metadata" {
            assert_eq!(after, before, "denial cannot change {name}");
        }
    }
    assert_eq!(
        f.game
            .query("SELECT last_observed_ms FROM game_metadata", &[])
            .unwrap()[0][0],
        brews_backend::db::SqlValue::Integer(deadline + 7)
    );
    assert_eq!(
        f.service()
            .discovery(game, DiscoveryViewer::Entry(&code), Some(&code), true, &[])
            .err(),
        Some(GameError::Storage)
    );
}

#[test]
fn discovery_remote_expiry_rejects_wrong_identity_and_invalid_samples() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let game = f.directory().discovery_route().unwrap().unwrap().game_id;
    let deadline = f
        .service()
        .discovery(game, DiscoveryViewer::Entry(&code), Some(&code), true, &[])
        .unwrap()
        .valid_until
        .unwrap();
    let before = all_rows(&f);
    for (target, until, sample) in [
        (id(&f.rt, 70).parse().unwrap(), deadline, deadline),
        (game, deadline, deadline - 1),
        (game, 0, deadline),
        (game, deadline, 9_007_199_254_740_992),
    ] {
        assert_eq!(
            f.service()
                .fence_discovery_expiry(target, until, sample)
                .err(),
            Some(GameError::Storage)
        );
        assert_eq!(all_rows(&f), before);
    }
}

#[test]
fn discovery_remote_expiry_requires_checked_metadata_write_and_readback() {
    for statement in [
        "CREATE TRIGGER deny_expiry BEFORE UPDATE OF last_observed_ms ON game_metadata BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER deny_expiry BEFORE UPDATE OF last_observed_ms ON game_metadata BEGIN SELECT RAISE(ABORT,'denial unavailable'); END",
        "CREATE TRIGGER deny_expiry AFTER UPDATE OF last_observed_ms ON game_metadata BEGIN UPDATE game_metadata SET last_observed_ms=OLD.last_observed_ms; END",
    ] {
        let f = Fixture::new();
        let code = f.open(&GameConfiguration::default());
        let game = f.directory().discovery_route().unwrap().unwrap().game_id;
        let deadline = f
            .service()
            .discovery(game, DiscoveryViewer::Entry(&code), Some(&code), true, &[])
            .unwrap()
            .valid_until
            .unwrap();
        let before = all_rows(&f);
        f.game.execute(statement, &[]).unwrap();
        f.rt.now.set(deadline - 1);
        assert_eq!(
            f.service()
                .fence_discovery_expiry(game, deadline, deadline)
                .err(),
            Some(GameError::Storage)
        );
        assert_eq!(all_rows(&f), before);
    }
}
