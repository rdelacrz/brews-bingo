//! Dedicated Slice6 projections use actual Accounts authority and owner SQLite.
use super::*;
use brews_backend::api::{ReadRole, ReadSelection};
use brews_contracts::games::reads::*;
use brews_domain::{games::GameState, ids::PlayerId};
fn select(f: &Fixture, kind: ReadKind, role: ReadRole) -> ReadSelection {
    ReadSelection {
        game_id: f.service().directory_projection().unwrap().game_id(),
        kind,
        role,
        player_id: None,
    }
}
fn account(
    f: &Fixture,
    kind: ReadKind,
    token: &str,
    observed: &[ConnectionId],
) -> Result<ReadProjection, GameError> {
    f.service().dedicated_read(
        select(f, kind, ReadRole::Account),
        token,
        Some(&f.auth().authorize_game_account(token).unwrap()),
        observed,
    )
}
fn player(f: &Fixture, kind: ReadKind, token: &str) -> Result<ReadProjection, GameError> {
    f.service()
        .dedicated_read(select(f, kind, ReadRole::Player), token, None, &[])
}
fn protected_rows(f: &Fixture) -> Vec<Vec<brews_backend::db::Row>> {
    brews_backend::db::schema::game_schema::GAME_TABLES
        .iter()
        .filter(|t| **t != "game_metadata")
        .map(|t| {
            f.game
                .query(&format!("SELECT * FROM {t} ORDER BY 1,2"), &[])
                .unwrap()
        })
        .collect()
}
fn json(p: ReadProjection) -> serde_json::Value {
    serde_json::from_slice(&p.encode_json().unwrap()).unwrap()
}
#[test]
fn dedicated_reads_new_cross_host_are_passive_and_sync_still_enters() {
    let f = Fixture::new();
    super::operator::saved(&f);
    let (_, other) = super::operator::actor(&f, 60, AccountRole::Host);
    let before = protected_rows(&f);
    for kind in [
        ReadKind::Game,
        ReadKind::Players,
        ReadKind::Calls,
        ReadKind::RemainingValues,
        ReadKind::Boards,
        ReadKind::Qualification,
    ] {
        let p = account(&f, kind, &other, &[]).unwrap();
        assert_eq!(p.kind(), kind);
        assert!(p.encode_json().unwrap().len() < 256 * 1024);
    }
    assert_eq!(
        account(&f, ReadKind::Result, &other, &[]).err(),
        Some(GameError::ResultNotCommitted)
    );
    assert_eq!(protected_rows(&f), before);
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_connection_grants", &[])
            .unwrap()[0][0],
        SqlValue::Integer(0)
    );
    f.service()
        .sync_account(f.auth().authorize_game_account(&other).unwrap(), None, &[])
        .unwrap();
    assert_eq!(
        f.game
            .query("SELECT count(*) FROM game_connection_grants", &[])
            .unwrap()[0][0],
        SqlValue::Integer(1)
    );
}
#[test]
fn dedicated_reads_awaiting_own_membership_board_recovery_and_alias_are_current() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let p = f.player(&code, 20, "Reader");
    let initial = json(player(&f, ReadKind::Membership, p.token()).unwrap());
    assert_eq!(initial["membership"]["alias"], "Reader");
    assert_eq!(initial["membership"]["recovery_enabled"], false);
    assert_eq!(
        json(player(&f, ReadKind::OwnBoard, p.token()).unwrap())["board"],
        serde_json::Value::Null
    );
    assert_eq!(
        json(player(&f, ReadKind::Qualification, p.token()).unwrap())["qualified"],
        false
    );
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), p.token(), Some(&answer))
        .unwrap();
    assert_eq!(
        json(player(&f, ReadKind::Membership, p.token()).unwrap())["membership"]["recovery_enabled"],
        true
    );
    f.service()
        .manage_recovery_answer(f.command(32), p.token(), None)
        .unwrap();
    assert_eq!(
        json(player(&f, ReadKind::Membership, p.token()).unwrap())["membership"]["recovery_enabled"],
        false
    );
    f.service()
        .rename_my_alias(f.command(33), p.token(), "Renamed")
        .unwrap();
    assert_eq!(
        json(player(&f, ReadKind::Membership, p.token()).unwrap())["membership"]["alias"],
        "Renamed"
    );
    let before = protected_rows(&f);
    for kind in [
        ReadKind::Players,
        ReadKind::RemainingValues,
        ReadKind::Boards,
    ] {
        assert_eq!(
            player(&f, kind, p.token()).err(),
            Some(GameError::InvalidInput)
        );
    }
    assert_eq!(protected_rows(&f), before);
}
#[test]
fn dedicated_reads_complete_numeric_pool_calls_qualification_and_exhaustion() {
    let f = Fixture::new();
    let (_, first, _, observed) = f.start_two_player_game();
    let board =
        json(player(&f, ReadKind::OwnBoard, first.token()).unwrap())["board"]["cells"].clone();
    for n in 1..=4 {
        let before = json(account(&f, ReadKind::Calls, &f.token, &observed).unwrap());
        let rev = before["view_revision"].as_u64().unwrap();
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                f.command(70 + n as u8),
                &n.to_string(),
                rev,
            )
            .unwrap();
        let remaining = json(account(&f, ReadKind::RemainingValues, &f.token, &observed).unwrap());
        assert_eq!(remaining["remaining_count"], 4 - n);
        assert_eq!(
            remaining["remaining_values"],
            serde_json::json!(((n + 1)..=4).map(|v| v.to_string()).collect::<Vec<_>>())
        );
    }
    let calls = json(player(&f, ReadKind::Calls, first.token()).unwrap());
    assert_eq!(calls["calls"].as_array().unwrap().len(), 4);
    assert_eq!(calls["latest"], calls["calls"][3]);
    assert_eq!(
        json(player(&f, ReadKind::Qualification, first.token()).unwrap())["qualified"],
        true
    );
    assert_eq!(json(account(&f,ReadKind::Qualification,&f.token,&observed).unwrap())["qualified_players"].as_array().unwrap().len(),2);
    assert!(matches!(
        f.service().player_view(first.token()).unwrap(),
        GameView::Player {
            state: GameState::InProgress,
            ..
        }
    ));
    let after = json(player(&f, ReadKind::OwnBoard, first.token()).unwrap());
    let layout = |v: &serde_json::Value| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|c| (c["row"].clone(), c["column"].clone(), c["kind"].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(layout(&board), layout(&after["board"]["cells"]));
}
#[test]
fn dedicated_reads_roster_and_boards_order_selection_and_real_binding_presence() {
    let f = Fixture::new();
    let (_, first, _, observed) = f.start_two_player_game();
    let roster = json(account(&f, ReadKind::Players, &f.token, &observed).unwrap());
    let ids = roster["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["player_id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(ids.windows(2).all(|p| p[0] < p[1]));
    assert!(
        roster["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["connected"] == true && p["left_during_play"] == false)
    );
    assert!(
        json(account(&f, ReadKind::Players, &f.token, &[]).unwrap())["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["connected"] == false)
    );
    let mut selection = select(&f, ReadKind::Boards, ReadRole::Account);
    selection.player_id = Some(ids[0].parse::<PlayerId>().unwrap());
    let proof = f.auth().authorize_game_account(&f.token).unwrap();
    let selected = json(
        f.service()
            .dedicated_read(selection, &f.token, Some(&proof), &observed)
            .unwrap(),
    );
    assert_eq!(selected["boards"].as_array().unwrap().len(), 1);
    selection.player_id = Some(id(&f.rt, 99).parse().unwrap());
    assert_eq!(
        f.service()
            .dedicated_read(selection, &f.token, Some(&proof), &observed)
            .err(),
        Some(GameError::NotFound)
    );
    f.service()
        .leave_player(f.command(80), first.token(), GameState::InProgress)
        .unwrap();
    let roster = json(account(&f, ReadKind::Players, &f.token, &observed).unwrap());
    assert_eq!(
        roster["players"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["left_during_play"] == true)
            .count(),
        1
    );
    assert_eq!(roster["players"].as_array().unwrap().len(), 2);
    player(&f, ReadKind::OwnBoard, first.token()).unwrap();
}
#[test]
fn dedicated_reads_terminal_original_grants_null_leave_final_presence_and_exit() {
    let f = Fixture::new();
    let (_, first, second, observed) = f.start_two_player_game();
    let (_, outsider) = super::operator::actor(&f, 60, AccountRole::Host);
    account(&f, ReadKind::Game, &outsider, &observed).unwrap();
    super::terminal::end_without_winner(&f, f.command(90));
    assert!(
        f.game
            .query(
                "SELECT name FROM sqlite_master WHERE name LIKE 'game_history_read_%'",
                &[]
            )
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        account(&f, ReadKind::Result, &outsider, &observed).err(),
        Some(GameError::Unauthorized)
    );
    assert_eq!(
        account(&f, ReadKind::RemainingValues, &f.token, &observed).err(),
        Some(GameError::RemainingValuesUnavailable)
    );
    let roster = json(account(&f, ReadKind::Players, &f.token, &observed).unwrap());
    assert!(
        roster["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["left_during_play"].is_null()
                && p["membership_retained"] == true
                && p["connected"] == true)
    );
    assert_eq!(
        json(player(&f, ReadKind::Membership, first.token()).unwrap())["membership"]["recovery_enabled"],
        false
    );
    for kind in [
        ReadKind::Game,
        ReadKind::Calls,
        ReadKind::OwnBoard,
        ReadKind::Qualification,
        ReadKind::Result,
    ] {
        player(&f, kind, first.token()).unwrap();
    }
    assert_eq!(
        json(player(&f, ReadKind::Result, first.token()).unwrap())["result"]["role"],
        "final_player"
    );
    f.service()
        .exit_player(first.token(), f.command(91))
        .unwrap();
    assert_eq!(
        player(&f, ReadKind::Result, first.token()).err(),
        Some(GameError::Unauthorized)
    );
    player(&f, ReadKind::Result, second.token()).unwrap();
    f.service()
        .exit_account(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(92),
        )
        .unwrap();
    assert_eq!(
        account(&f, ReadKind::Game, &f.token, &observed).err(),
        Some(GameError::Unauthorized)
    );
    f.service()
        .history(
            f.auth().authorize_game_account(&f.token).unwrap(),
            select(&f, ReadKind::Game, ReadRole::Account).game_id,
        )
        .unwrap();
}
#[test]
fn dedicated_reads_maximum_legal_pool_roster_boards_and_policy_aliases_are_bounded() {
    let f = Fixture::new();
    let config = GameConfiguration {
        numeric_upper_bound: brews_domain::games::NUMERIC_UPPER_BOUND_MAX,
        player_capacity: brews_domain::games::PLAYER_CAPACITY_MAX,
        board_side_length: brews_domain::games::BOARD_SIDE_MAX,
        free_cells_enabled: false,
        free_cell_positions: vec![],
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let mut observed = vec![];
    let mut players = vec![];
    for n in 0..config.player_capacity {
        let p = f.player(&code, 20 + n, &format!("Player{n:02}012345678901"));
        let c = id(&f.rt, 70 + n).parse().unwrap();
        let grant = f.service().prepare_player_connection(p.token(), c).unwrap();
        f.service().accept_player_connection(&grant).unwrap();
        observed.push(c);
        players.push(p);
    }
    let remaining = json(account(&f, ReadKind::RemainingValues, &f.token, &observed).unwrap());
    assert_eq!(
        remaining["remaining_values"].as_array().unwrap().len(),
        1000
    );
    assert_eq!(remaining["remaining_values"][1], "2");
    assert_eq!(remaining["remaining_values"][9], "10");
    let game = f.service().directory_projection().unwrap().game_id();
    let reservation = f.directory().confirm_reservation(game).unwrap();
    let revision = json(account(&f, ReadKind::Game, &f.token, &observed).unwrap())["game"]["game"]
        ["view_revision"]
        .as_u64()
        .unwrap();
    f.service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(110),
            revision,
            &reservation,
            &observed,
        )
        .unwrap();
    let boards = account(&f, ReadKind::Boards, &f.token, &observed).unwrap();
    assert!(boards.encode_json().unwrap().len() < 256 * 1024);
    assert_eq!(json(boards)["boards"].as_array().unwrap().len(), 20);
    assert_eq!(players.len(), 20);
}
#[test]
fn dedicated_reads_admin_purposes_success_rejection_failure_atomic_capture() {
    let f = Fixture::new();
    super::operator::saved(&f);
    let (admin_id, token) = super::operator::actor(&f, 60, AccountRole::Admin);
    for kind in [
        ReadKind::Game,
        ReadKind::Players,
        ReadKind::Calls,
        ReadKind::RemainingValues,
        ReadKind::Boards,
        ReadKind::Qualification,
    ] {
        account(&f, kind, &token, &[]).unwrap();
    }
    assert_eq!(
        account(&f, ReadKind::Result, &token, &[]).err(),
        Some(GameError::ResultNotCommitted)
    );
    let rows=f.game.query("SELECT actor_account_id,target_game_id,operation,outcome FROM game_admin_audit ORDER BY audit_id",&[]).unwrap();
    assert_eq!(rows.len(), 7);
    for row in rows {
        assert_eq!(row[0], SqlValue::Text(admin_id.to_string()));
        assert_eq!(
            row[1],
            SqlValue::Text(
                select(&f, ReadKind::Game, ReadRole::Account)
                    .game_id
                    .to_string()
            )
        );
    }
    f.game.execute("CREATE TRIGGER block_read_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
    let before = protected_rows(&f);
    assert_eq!(
        account(&f, ReadKind::Game, &token, &[]).err(),
        Some(GameError::Storage)
    );
    assert_eq!(protected_rows(&f), before);
}
#[test]
fn dedicated_reads_recovery_epoch_revocation_fails_old_selected_player() {
    let f = Fixture::new();
    let code = f.open(&GameConfiguration::default());
    let p = f.player(&code, 20, "RecoverRead");
    let answer = zeroize::Zeroizing::new(brews_backend::security::new_token(&f.rt).unwrap());
    f.service()
        .manage_recovery_answer(f.command(31), p.token(), Some(&answer))
        .unwrap();
    let new = f
        .service()
        .recover_player(f.command(32), &code, "RecoverRead", &answer, "192.0.2.77")
        .unwrap()
        .cookie
        .unwrap();
    for kind in [
        ReadKind::Game,
        ReadKind::Membership,
        ReadKind::Calls,
        ReadKind::OwnBoard,
        ReadKind::Qualification,
    ] {
        assert_eq!(
            player(&f, kind, p.token()).err(),
            Some(GameError::Unauthorized)
        );
        player(&f, kind, new.token()).unwrap();
    }
}

#[test]
fn dedicated_reads_full_thousand_actual_accepted_calls_remain_in_progress_and_exhausted() {
    let f = Fixture::new();
    let config = GameConfiguration {
        numeric_upper_bound: brews_domain::games::NUMERIC_UPPER_BOUND_MAX,
        board_side_length: 2,
        free_cells_enabled: false,
        free_cell_positions: vec![],
        player_capacity: 2,
        spectator_capacity: 0,
        ..GameConfiguration::default()
    };
    let code = f.open(&config);
    let first = f.player(&code, 20, "FullPoolFirst");
    let second = f.player(&code, 21, "FullPoolSecond");
    let observed = [
        id(&f.rt, 40).parse().unwrap(),
        id(&f.rt, 41).parse().unwrap(),
    ];
    for (p, c) in [(&first, observed[0]), (&second, observed[1])] {
        let g = f.service().prepare_player_connection(p.token(), c).unwrap();
        f.service().accept_player_connection(&g).unwrap();
    }
    let reservation = f
        .directory()
        .confirm_reservation(select(&f, ReadKind::Game, ReadRole::Account).game_id)
        .unwrap();
    let rev=json(account(&f,ReadKind::Game,&f.token,&observed).unwrap())["game"]["game"]["view_revision"].as_u64().unwrap();
    f.service()
        .start(
            f.auth().authorize_game_account(&f.token).unwrap(),
            f.command(30),
            rev,
            &reservation,
            &observed,
        )
        .unwrap();
    for n in 1..=brews_domain::games::NUMERIC_UPPER_BOUND_MAX {
        let rev = json(account(&f, ReadKind::Calls, &f.token, &observed).unwrap())["view_revision"]
            .as_u64()
            .unwrap();
        let mut b = *uuid::Uuid::parse_str(&id(&f.rt, 0)).unwrap().as_bytes();
        b[14..16].copy_from_slice(&(1000 + n as u16).to_be_bytes());
        let command = uuid::Uuid::from_bytes(b).to_string().parse().unwrap();
        f.service()
            .call_manual_value(
                f.auth().authorize_game_account(&f.token).unwrap(),
                command,
                &n.to_string(),
                rev,
            )
            .unwrap();
    }
    let calls = json(player(&f, ReadKind::Calls, first.token()).unwrap());
    assert_eq!(calls["calls"].as_array().unwrap().len(), 1000);
    assert_eq!(calls["latest"]["value"], "1000");
    let remaining = json(account(&f, ReadKind::RemainingValues, &f.token, &observed).unwrap());
    assert_eq!(remaining["remaining_values"], serde_json::json!([]));
    assert_eq!(remaining["exhausted"], true);
    assert!(matches!(
        f.service().player_view(first.token()).unwrap(),
        GameView::Player {
            state: GameState::InProgress,
            ..
        }
    ));
}

#[test]
fn dedicated_reads_history_purge_denies_deleted_original_player_grant() {
    let f = Fixture::new();
    let (_, first, _, _) = f.start_two_player_game();
    super::terminal::end_without_winner(&f, f.command(90));
    let expiry = match f
        .game
        .query("SELECT history_expires_at FROM game_record", &[])
        .unwrap()[0][0]
    {
        SqlValue::Integer(t) => t,
        _ => panic!("expiry"),
    };
    let selection = select(&f, ReadKind::Result, ReadRole::Player);
    f.rt.now.set(expiry);
    f.service().cleanup().unwrap();
    assert!(
        f.game
            .query("SELECT * FROM game_record", &[])
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.service()
            .dedicated_read(selection, first.token(), None, &[])
            .err(),
        Some(GameError::Unauthorized)
    );
}
