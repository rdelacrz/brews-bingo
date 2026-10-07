//! Real SQLite index pagination: minimal routing metadata, not copied snapshots.
use super::*;
use brews_domain::ids::GameId;
#[test]
fn history_index_uses_bounded_descending_keyset_across_hosts_and_pending_publication() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let now = rt.now.get();
    let games: Vec<GameId> = (1..=4)
        .map(|n| command(&rt, n).to_string().parse().unwrap())
        .collect();
    for (i, game) in games.iter().enumerate() {
        db.execute("INSERT INTO directory_game_index(game_id,designated_host_id,fingerprint,state,source_revision,game_code,created_at,started_at,ended_at,history_expires_at,publication_state) VALUES(?,?,?,?,?,?,?,?,?,?,?)", &[
            SqlValue::Text(game.to_string()),SqlValue::Text(account(i as u8+1).to_string()),SqlValue::Blob(vec![7;32]),SqlValue::Text(GameState::Cancelled.to_string()),SqlValue::Integer(3),SqlValue::Text(format!("GAME000{}",i+1)),SqlValue::Integer(now-10),SqlValue::Integer(now-5),SqlValue::Integer(now),SqlValue::Integer(now+10000),SqlValue::Text("pending".into())]).unwrap();
    }
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let first = svc.history_indexes(None, 2, None).unwrap();
    assert_eq!(first.len(), 3, "page must include exact bounded lookahead");
    assert_eq!(
        first.iter().map(|p| p.game_id()).collect::<Vec<_>>(),
        vec![games[3], games[2], games[1]]
    );
    let second = svc
        .history_indexes(Some((now, games[2])), 2, Some(GameState::Cancelled))
        .unwrap();
    assert_eq!(
        second.iter().map(|p| p.game_id()).collect::<Vec<_>>(),
        vec![games[1], games[0]]
    );
    assert!(
        svc.history_indexes(None, 2, Some(GameState::Resolved))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn history_index_maximum_is_fifty_and_expiry_filters_beyond_first_page() {
    let db = Sqlite::new();
    let rt = TestRuntime::new();
    migrate_directory(&db).unwrap();
    let now = rt.now.get();
    for n in 1..=55 {
        let game: GameId = command(&rt, n).to_string().parse().unwrap();
        db.execute("INSERT INTO directory_game_index(game_id,designated_host_id,fingerprint,state,source_revision,game_code,created_at,started_at,ended_at,history_expires_at,publication_state) VALUES(?,?,?,?,?,?,?,?,?,?,?)",&[SqlValue::Text(game.to_string()),SqlValue::Text(account(n).to_string()),SqlValue::Blob(vec![7;32]),SqlValue::Text(GameState::Cancelled.to_string()),SqlValue::Integer(3),SqlValue::Text(format!("GAME00{n:02}")),SqlValue::Integer(now-10),SqlValue::Integer(now-5),SqlValue::Integer(now),SqlValue::Integer(if n<=4 {now}else{now+10000}),SqlValue::Text("published".into())]).unwrap();
    }
    let svc = DirectoryService::new(&db, &rt).unwrap();
    let first = svc.history_indexes(None, 50, None).unwrap();
    assert_eq!(first.len(), 51);
    let cursor = first[49].game_id();
    let rest = svc.history_indexes(Some((now, cursor)), 50, None).unwrap();
    assert_eq!(rest.len(), 1);
    assert!(svc.history_indexes(None, 51, None).is_err());
    assert!(svc.history_indexes(None, 0, None).is_err());
    assert!(
        svc.history_indexes(None, 1, Some(GameState::InProgress))
            .is_err()
    );
    db.conn.borrow().execute_batch("CREATE TRIGGER ignore_history_index_clock BEFORE UPDATE ON directory_metadata BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    rt.now.set(now + 1);
    assert_eq!(
        svc.history_indexes(None, 50, None).err(),
        Some(directory::DirectoryError::Storage)
    );
}
