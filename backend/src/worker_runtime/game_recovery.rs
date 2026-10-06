//! Directory alarm coordination; only actual Game state is initializer/terminal evidence.
use super::{
    game_peers::GamePeerError,
    game_recovery_wire::{GameRecoveryReply, GameRecoveryRequest},
};
use crate::{
    auth::Runtime,
    db::{Database, directory::DirectoryService},
    directory::DirectoryError,
};

fn settle_probe<D: Database, R: Runtime>(
    service: &DirectoryService<'_, D, R>,
    request: &GameRecoveryRequest,
    reply: Result<GameRecoveryReply, GamePeerError>,
) -> Result<(), DirectoryError> {
    let snapshot = request
        .work
        .work()
        .map_err(|_| DirectoryError::ProofMismatch)?;
    let Some(current) = service.creation_work(snapshot.game_id())? else {
        return Ok(());
    };
    if current != snapshot || service.game_code(current.game_id())? != request.game_code {
        return Ok(());
    }
    let outcome = reply.and_then(|r| r.validate_for(request));
    if let Ok(super::game_recovery_wire::GameRecoveryOutcome::Terminal { projection }) = &outcome {
        match service.release_game(crate::directory::games::TerminalProof::new(
            projection.proof()?,
        )) {
            Ok(ack) => {
                if ack.game_id() != current.game_id()
                    || service.creation_work(current.game_id())?.is_some()
                {
                    return Err(DirectoryError::Storage);
                }
                return Ok(());
            }
            Err(DirectoryError::Storage | DirectoryError::Clock) => {
                return Err(DirectoryError::Storage);
            }
            Err(_) => {}
        }
    }
    if let Ok(super::game_recovery_wire::GameRecoveryOutcome::Committed { projection }) = &outcome {
        match service.publish_game(projection.proof()?) {
            Ok(ack) => {
                if !ack.published()
                    || ack.game_id() != current.game_id()
                    || service
                        .creation_work(current.game_id())?
                        .is_some_and(|w| w.next_retry_at().is_some())
                {
                    return Err(DirectoryError::Storage);
                }
                return Ok(());
            }
            Err(DirectoryError::Storage | DirectoryError::Clock) => {
                return Err(DirectoryError::Storage);
            }
            Err(_) => {}
        }
    }
    if let Ok(super::game_recovery_wire::GameRecoveryOutcome::Ready { ready, .. }) = outcome
        && current.ready_revision().is_none()
    {
        match service.acknowledge_creation(ready.proof()?) {
            Ok(_) => {
                let persisted = service.creation_work(current.game_id())?;
                if persisted
                    .is_none_or(|w| w.ready_revision() != Some(0) || w.next_retry_at().is_some())
                {
                    return Err(DirectoryError::Storage);
                }
                return Ok(());
            }
            Err(DirectoryError::Storage | DirectoryError::Clock) => {
                return Err(DirectoryError::Storage);
            }
            Err(_) => {}
        }
    }
    let retried = service.retry_creation(current)?;
    if service.creation_work(current.game_id())? != Some(retried) {
        return Err(DirectoryError::Storage);
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
impl super::directory::GameDirectoryObject {
    pub(super) async fn recover_game_creations(
        &self,
        service: &DirectoryService<'_, super::OwnerDatabase, super::WorkerRuntime>,
    ) -> worker::Result<()> {
        const RECOVERY_BATCH_LIMIT: u32 = 10;
        let due = service
            .due_creation_work(RECOVERY_BATCH_LIMIT)
            .map_err(|_| worker::Error::RustError("Directory recovery work unavailable".into()))?;
        for snapshot in due {
            let current = service.creation_work(snapshot.game_id()).map_err(|_| {
                worker::Error::RustError("Directory recovery work unavailable".into())
            })?;
            if current != Some(snapshot) {
                continue;
            }
            let code = service.game_code(snapshot.game_id()).map_err(|_| {
                worker::Error::RustError("Directory recovery association unavailable".into())
            })?;
            // A wake survives eviction or failed peer delivery before any cross-owner await.
            self.schedule_cleanup().await?;
            self.state.storage().sync().await?;
            // The output gate itself awaits: never send a superseded work snapshot.
            if service.creation_work(snapshot.game_id()).map_err(|_| {
                worker::Error::RustError("Directory recovery work unavailable".into())
            })? != Some(snapshot)
                || service.game_code(snapshot.game_id()).map_err(|_| {
                    worker::Error::RustError("Directory recovery association unavailable".into())
                })? != code
            {
                continue;
            }
            let request = GameRecoveryRequest {
                work: (&snapshot).into(),
                game_code: code,
            };
            let reply = self.call_game_recovery(&request).await;
            settle_probe(service, &request, reply).map_err(|_| {
                worker::Error::RustError("Directory recovery transition failed".into())
            })?;
            self.schedule_cleanup().await?;
            self.state.storage().sync().await?;
        }
        Ok(())
    }

    async fn call_game_recovery(
        &self,
        message: &GameRecoveryRequest,
    ) -> Result<GameRecoveryReply, GamePeerError> {
        use axum::body::{Body, to_bytes};
        use worker::{Headers, Method, Request, RequestInit};
        let work = message.work.work()?;
        let namespace = self
            .env
            .durable_object("GAMES")
            .map_err(|_| GamePeerError::Unavailable)?;
        let stub = namespace
            .id_from_name(&work.game_id().to_string())
            .and_then(|id| id.get_stub())
            .map_err(|_| GamePeerError::Unavailable)?;
        let bytes = super::game_peers::encode(message)?;
        let body = std::str::from_utf8(&bytes).map_err(|_| GamePeerError::Unavailable)?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(body.into()));
        let headers = Headers::new();
        headers
            .set("Content-Type", "application/json")
            .map_err(|_| GamePeerError::Unavailable)?;
        init.with_headers(headers);
        let request = Request::new_with_init("https://game.internal/recovery", &init)
            .map_err(|_| GamePeerError::Unavailable)?;
        let response = stub
            .fetch_with_request(request)
            .await
            .map_err(|_| GamePeerError::Unavailable)?;
        if response.status_code() != 200 {
            return Err(GamePeerError::Unavailable);
        }
        let http: worker::HttpResponse = response
            .try_into()
            .map_err(|_| GamePeerError::Unavailable)?;
        let bytes = zeroize::Zeroizing::new(
            to_bytes(
                Body::new(http.into_body()),
                super::game_peers::GAME_PEER_MAX_BYTES,
            )
            .await
            .map_err(|_| GamePeerError::Unavailable)?
            .to_vec(),
        );
        super::game_peers::decode(&bytes).map_err(|_| GamePeerError::Unavailable)
    }
}

#[cfg(test)]
use crate::auth::test_support;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real SQLite recovery fixtures fail fast."
)]
mod tests {
    use super::*;
    use crate::{db::directory::migrate_directory, directory::games::CreationFingerprint};
    use test_support::{Sqlite, TestRuntime};
    fn command<T: std::str::FromStr>(rt: &TestRuntime, n: u8) -> T
    where
        T::Err: std::fmt::Debug,
    {
        let mut b = [0; 16];
        b[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
        b[6] = 0x70;
        b[8] = 0x80;
        b[15] = n;
        uuid::Uuid::from_bytes(b).to_string().parse().unwrap()
    }
    struct Fixture {
        directory: Sqlite,
        game: Sqlite,
        rt: TestRuntime,
        work: crate::directory::games::CreationWork,
        ready: crate::directory::games::CreationReadyProof,
    }
    impl Fixture {
        fn new() -> Self {
            use crate::{
                auth::AuthPolicy,
                db::game::{GameService, migrate_game},
                game::creation_fingerprint,
            };
            let directory = Sqlite::new();
            let game = Sqlite::new();
            let rt = TestRuntime::new();
            migrate_directory(&directory).unwrap();
            migrate_game(&game).unwrap();
            let config = brews_domain::games::GameConfiguration::default();
            let work = DirectoryService::new(&directory, &rt)
                .unwrap()
                .claim_game(
                    command(&rt, 1),
                    command(&rt, 2),
                    creation_fingerprint(&config).unwrap(),
                )
                .unwrap();
            let created = GameService::new(&game, &rt, AuthPolicy::default(), &[9; 32])
                .unwrap()
                .initialize(&work, Self::authority(&rt, &work), &config)
                .unwrap();
            Self {
                directory,
                game,
                rt,
                work,
                ready: created.ready,
            }
        }
        fn authority(
            rt: &TestRuntime,
            work: &crate::directory::games::CreationWork,
        ) -> crate::auth::GameAccountAuthority {
            crate::auth::GameAccountAuthority::from_trusted_peer(
                work.account_id(),
                command(rt, 3),
                brews_domain::accounts::AccountRole::Host,
                0,
                rt.now.get() + 100_000,
            )
            .unwrap()
        }
        fn directory(&self) -> DirectoryService<'_, Sqlite, TestRuntime> {
            DirectoryService::new(&self.directory, &self.rt).unwrap()
        }
        fn game(&self) -> crate::db::game::GameService<'_, Sqlite, TestRuntime> {
            crate::db::game::GameService::new(
                &self.game,
                &self.rt,
                crate::auth::AuthPolicy::default(),
                &[9; 32],
            )
            .unwrap()
        }
        fn request(&self) -> GameRecoveryRequest {
            GameRecoveryRequest {
                work: (&self
                    .directory()
                    .creation_work(self.work.game_id())
                    .unwrap()
                    .unwrap())
                    .into(),
                game_code: self.directory().game_code(self.work.game_id()).unwrap(),
            }
        }
        fn ready_reply(&self, request: &GameRecoveryRequest) -> GameRecoveryReply {
            GameRecoveryReply {
                request: request.clone(),
                outcome: super::super::game_recovery_wire::GameRecoveryOutcome::Ready {
                    ready: (&self.ready).into(),
                    projection: (&self.game().directory_projection().unwrap()).into(),
                },
            }
        }
    }
    #[test]
    fn initializer_ready_reads_only_exact_persisted_new_identity() {
        use super::super::game_directory::WorkWire;
        use crate::db::Database;
        let f = Fixture::new();
        assert_eq!(f.game().creation_ready(&f.work).unwrap(), f.ready);
        assert_eq!(
            f.game
                .query("SELECT count(*) FROM game_receipts", &[])
                .unwrap(),
            vec![vec![crate::db::SqlValue::Integer(1)]]
        );
        let original = serde_json::to_value(WorkWire::from(&f.work)).unwrap();
        for (field, value) in [
            (
                "game_id",
                serde_json::json!(command::<brews_domain::ids::GameId>(&f.rt, 7)),
            ),
            (
                "account_id",
                serde_json::json!(command::<brews_domain::ids::AccountId>(&f.rt, 7)),
            ),
            (
                "command_id",
                serde_json::json!(command::<brews_domain::ids::CommandId>(&f.rt, 7)),
            ),
            ("fingerprint", serde_json::to_value([8; 32]).unwrap()),
        ] {
            let mut bad = original.clone();
            bad[field] = value;
            let wire: WorkWire = serde_json::from_value(bad).unwrap();
            assert_eq!(
                f.game().creation_ready(&wire.work().unwrap()),
                Err(crate::game::GameError::Conflict)
            );
        }
        let mut bad = original;
        bad["created_at"] = serde_json::json!(f.work.created_at() - 1);
        bad["deadline"] = serde_json::json!(f.work.deadline() - 1);
        let wire: WorkWire = serde_json::from_value(bad).unwrap();
        assert!(f.game().creation_ready(&wire.work().unwrap()).is_err());
        let ack = f.directory().acknowledge_creation(f.ready).unwrap();
        let grant = f.directory().allocate_game_code(ack).unwrap();
        let work = match f
            .game()
            .prepare_lobby(Fixture::authority(&f.rt, &f.work), command(&f.rt, 4), 0)
            .unwrap()
        {
            crate::game::LobbyPreparation::Ready(work) => work,
            _ => panic!("Expected actual lobby intent"),
        };
        f.game()
            .commit_lobby(&work, Fixture::authority(&f.rt, &f.work), &grant)
            .unwrap();
        assert!(f.game().creation_ready(&f.work).is_err());
    }
    #[test]
    fn initializer_ready_never_initializes_absence_or_revives_expired_game() {
        use crate::db::{Database, SqlValue};
        let f = Fixture::new();
        let empty = Sqlite::new();
        crate::db::game::migrate_game(&empty).unwrap();
        let absent = crate::db::game::GameService::new(
            &empty,
            &f.rt,
            crate::auth::AuthPolicy::default(),
            &[9; 32],
        )
        .unwrap();
        assert_eq!(
            absent.creation_ready(&f.work),
            Err(crate::game::GameError::NotFound)
        );
        assert!(
            empty
                .query("SELECT * FROM game_record", &[])
                .unwrap()
                .is_empty()
        );
        assert!(
            empty
                .query("SELECT * FROM game_configuration", &[])
                .unwrap()
                .is_empty()
        );
        f.rt.now.set(f.work.deadline());
        assert!(f.game().creation_ready(&f.work).is_err());
        assert_eq!(
            f.game.query("SELECT state FROM game_record", &[]).unwrap(),
            vec![vec![SqlValue::Text("cancelled".into())]]
        );
    }
    #[test]
    fn player_acceptance_preflight_rejects_a_superseded_preparation_without_presence() {
        let f = Fixture::new();
        let ack = f.directory().acknowledge_creation(f.ready).unwrap();
        let intent = match f
            .game()
            .prepare_lobby(Fixture::authority(&f.rt, &f.work), command(&f.rt, 4), 0)
            .unwrap()
        {
            crate::game::LobbyPreparation::Ready(work) => work,
            _ => panic!("Expected actual lobby preparation"),
        };
        let grant = f.directory().allocate_game_code(ack).unwrap();
        f.game()
            .commit_lobby(&intent, Fixture::authority(&f.rt, &f.work), &grant)
            .unwrap();
        let pending = f.game().pending_work(1).unwrap().remove(0);
        let published = f
            .directory()
            .publish_game(f.game().directory_projection().unwrap())
            .unwrap();
        f.game()
            .acknowledge_projection(&pending, published)
            .unwrap();
        let context = f
            .game()
            .admission_context(grant.game_code(), None, None)
            .unwrap()
            .cookie
            .unwrap();
        let player = f
            .game()
            .join_player(
                command(&f.rt, 5),
                context.token(),
                None,
                brews_contracts::games::JoinPlayer {
                    game_code: grant.game_code().clone(),
                    alias: "Alice".into(),
                    recovery_answer: None,
                },
            )
            .unwrap()
            .cookie
            .unwrap();
        let old = f
            .game()
            .prepare_player_connection(player.token(), command(&f.rt, 6))
            .unwrap();
        f.game().validate_prepared_player_connection(&old).unwrap();
        assert!(
            f.game()
                .player_connection(old.connection_id())
                .unwrap()
                .is_none()
        );
        let current = f
            .game()
            .prepare_player_connection(player.token(), command(&f.rt, 7))
            .unwrap();
        assert_eq!(
            f.game().validate_prepared_player_connection(&old),
            Err(crate::game::GameError::Unauthorized)
        );
        f.game()
            .validate_prepared_player_connection(&current)
            .unwrap();
        f.game().accept_player_connection(&current).unwrap();
        assert_eq!(
            f.game().validate_prepared_player_connection(&current),
            Err(crate::game::GameError::Unauthorized)
        );
        assert_eq!(
            f.game().player_connection(current.connection_id()).unwrap(),
            Some(current)
        );
    }
    #[test]
    fn genuine_initializer_evidence_acknowledges_only_current_unready_creation() {
        let f = Fixture::new();
        let request = f.request();
        settle_probe(&f.directory(), &request, Ok(f.ready_reply(&request))).unwrap();
        let current = f
            .directory()
            .creation_work(f.work.game_id())
            .unwrap()
            .unwrap();
        assert_eq!(current.ready_revision(), Some(0));
        assert_eq!(current.next_retry_at(), None);
        assert_eq!(current.deadline(), f.work.deadline());
    }
    #[test]
    fn genuine_committed_lobby_recovers_directory_publication_without_faking_game_ack() {
        use super::super::game_recovery_wire::GameRecoveryOutcome;
        use crate::game::LobbyPreparation;
        let f = Fixture::new();
        let ack = f.directory().acknowledge_creation(f.ready).unwrap();
        let intent = match f
            .game()
            .prepare_lobby(Fixture::authority(&f.rt, &f.work), command(&f.rt, 4), 0)
            .unwrap()
        {
            LobbyPreparation::Ready(work) => work,
            _ => panic!("Expected genuine lobby intent"),
        };
        let grant = f.directory().allocate_game_code(ack).unwrap();
        f.game()
            .commit_lobby(&intent, Fixture::authority(&f.rt, &f.work), &grant)
            .unwrap();
        let request = f.request();
        let reply = GameRecoveryReply {
            request: request.clone(),
            outcome: GameRecoveryOutcome::Committed {
                projection: (&f.game().directory_projection().unwrap()).into(),
            },
        };
        assert!(reply.validate_for(&request).is_ok());
        settle_probe(&f.directory(), &request, Ok(reply)).unwrap();
        assert_eq!(
            f.directory().lookup_game_code(grant.game_code()).unwrap(),
            Some(f.work.game_id())
        );
        assert_eq!(
            f.directory()
                .creation_work(f.work.game_id())
                .unwrap()
                .unwrap()
                .next_retry_at(),
            None
        );
        assert_eq!(f.game().pending_work(100).unwrap().len(), 1);
    }
    #[test]
    fn genuine_owner_terminal_evidence_releases_reservation_after_deadline() {
        use super::super::game_recovery_wire::GameRecoveryOutcome;
        let f = Fixture::new();
        f.rt.now.set(f.work.deadline());
        let terminal = f.game().cleanup().unwrap().unwrap();
        let request = f.request();
        let reply = GameRecoveryReply {
            request: request.clone(),
            outcome: GameRecoveryOutcome::Terminal {
                projection: terminal.projection().into(),
            },
        };
        assert!(reply.validate_for(&request).is_ok());
        settle_probe(&f.directory(), &request, Ok(reply)).unwrap();
        assert!(
            f.directory()
                .creation_work(f.work.game_id())
                .unwrap()
                .is_none()
        );
        assert!(f.directory().confirm_reservation(f.work.game_id()).is_err());
        assert_eq!(f.game().pending_work(100).unwrap().len(), 1);
    }
    #[test]
    fn ready_new_game_with_hidden_code_retains_publication_backoff_and_original_deadline() {
        let f = Fixture::new();
        let ack = f.directory().acknowledge_creation(f.ready).unwrap();
        let grant = f.directory().allocate_game_code(ack).unwrap();
        let request = f.request();
        settle_probe(&f.directory(), &request, Ok(f.ready_reply(&request))).unwrap();
        let work = f
            .directory()
            .creation_work(f.work.game_id())
            .unwrap()
            .unwrap();
        assert_eq!(work.ready_revision(), Some(0));
        assert_eq!(work.attempts(), 1);
        assert_eq!(work.next_retry_at(), Some(f.rt.now.get() + 1000));
        assert_eq!(work.deadline(), f.work.deadline());
        assert_eq!(
            f.directory().lookup_game_code(grant.game_code()).unwrap(),
            None
        );
    }
    #[test]
    fn late_probe_after_initializer_ack_cannot_overwrite_current_phase() {
        let f = Fixture::new();
        let snapshot = f.request();
        let reply = f.ready_reply(&snapshot);
        f.directory().acknowledge_creation(f.ready).unwrap();
        let current = f.directory().creation_work(f.work.game_id()).unwrap();
        settle_probe(&f.directory(), &snapshot, Ok(reply)).unwrap();
        settle_probe(&f.directory(), &snapshot, Err(GamePeerError::Unavailable)).unwrap();
        assert_eq!(
            f.directory().creation_work(f.work.game_id()).unwrap(),
            current
        );
    }
    #[test]
    fn late_probe_after_committed_terminal_release_cannot_recreate_work() {
        let f = Fixture::new();
        let snapshot = f.request();
        let reply = f.ready_reply(&snapshot);
        f.rt.now.set(f.work.deadline());
        f.directory()
            .release_game(f.game().cleanup().unwrap().unwrap())
            .unwrap();
        settle_probe(&f.directory(), &snapshot, Ok(reply)).unwrap();
        assert!(
            f.directory()
                .creation_work(f.work.game_id())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn absent_after_deadline_keeps_reservation_and_saturating_retry() {
        let f = Fixture::new();
        f.rt.now.set(f.work.deadline());
        for _ in 0..12 {
            let request = f.request();
            let reply = GameRecoveryReply {
                request: request.clone(),
                outcome: super::super::game_recovery_wire::GameRecoveryOutcome::Absent {},
            };
            settle_probe(&f.directory(), &request, Ok(reply)).unwrap();
            let current = f
                .directory()
                .creation_work(f.work.game_id())
                .unwrap()
                .unwrap();
            let delay = current.next_retry_at().unwrap() - f.rt.now.get();
            assert!((1000..=300_000).contains(&delay));
            f.rt.now.set(current.next_retry_at().unwrap());
        }
        assert!(f.directory().confirm_reservation(f.work.game_id()).is_ok());
    }
    #[test]
    fn retry_sql_failure_is_an_alarm_error_and_preserves_actual_work() {
        let f = Fixture::new();
        let request = f.request();
        f.directory.conn.borrow().execute_batch("CREATE TRIGGER fail_recovery BEFORE UPDATE OF attempts ON directory_game_creations BEGIN SELECT RAISE(ABORT,'fault'); END").unwrap();
        assert_eq!(
            settle_probe(&f.directory(), &request, Err(GamePeerError::Unavailable)),
            Err(DirectoryError::Storage)
        );
        assert_eq!(
            f.directory().creation_work(f.work.game_id()).unwrap(),
            Some(f.work)
        );
    }
    #[test]
    fn genuine_new_cancellation_releases_only_the_hidden_directory_association() {
        let f = Fixture::new();
        let ack = f.directory().acknowledge_creation(f.ready).unwrap();
        let grant = f.directory().allocate_game_code(ack).unwrap();
        f.rt.now.set(f.work.deadline());
        let terminal = f.game().cleanup().unwrap().unwrap();
        assert_eq!(terminal.projection().game_code(), None);
        let released = f.directory().release_game(terminal.clone()).unwrap();
        assert_eq!(released.game_id(), f.work.game_id());
        assert!(
            f.directory()
                .creation_work(f.work.game_id())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            f.directory().lookup_game_code(grant.game_code()).unwrap(),
            None
        );
        let next = f
            .directory()
            .claim_game(command(&f.rt, 8), command(&f.rt, 9), f.work.fingerprint())
            .unwrap();
        f.directory().release_game(terminal).unwrap();
        assert_eq!(
            f.directory()
                .confirm_reservation(next.game_id())
                .unwrap()
                .game_id(),
            next.game_id()
        );
    }
    #[test]
    fn recovery_accepts_actual_cancellation_without_inventing_the_hidden_code() {
        use super::super::game_recovery_wire::GameRecoveryOutcome;
        let f = Fixture::new();
        let ack = f.directory().acknowledge_creation(f.ready).unwrap();
        let grant = f.directory().allocate_game_code(ack).unwrap();
        f.rt.now.set(f.work.deadline());
        let terminal = f.game().cleanup().unwrap().unwrap();
        assert_eq!(terminal.projection().game_code(), None);
        let request = f.request();
        assert_eq!(request.game_code.as_ref(), Some(grant.game_code()));
        let reply = GameRecoveryReply {
            request: request.clone(),
            outcome: GameRecoveryOutcome::Terminal {
                projection: terminal.projection().into(),
            },
        };
        assert!(reply.validate_for(&request).is_ok());
        settle_probe(&f.directory(), &request, Ok(reply)).unwrap();
        assert!(
            f.directory()
                .creation_work(f.work.game_id())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            f.directory().lookup_game_code(grant.game_code()).unwrap(),
            None
        );
        assert!(f.directory().confirm_reservation(f.work.game_id()).is_err());
        assert_eq!(f.game().directory_projection().unwrap().game_code(), None);
    }
    #[test]
    fn hidden_code_release_preserves_exact_terminal_identity_and_publication_fences() {
        use super::super::game_directory::ProjectionWire;
        use crate::directory::games::TerminalProof;
        let f = Fixture::new();
        let ack = f.directory().acknowledge_creation(f.ready).unwrap();
        f.directory().allocate_game_code(ack).unwrap();
        f.rt.now.set(f.work.deadline());
        let terminal = f.game().cleanup().unwrap().unwrap();
        let original = serde_json::to_value(ProjectionWire::from(terminal.projection())).unwrap();
        for (field, value) in [
            (
                "game_id",
                serde_json::json!(command::<brews_domain::ids::GameId>(&f.rt, 7)),
            ),
            (
                "designated_host_id",
                serde_json::json!(command::<brews_domain::ids::AccountId>(&f.rt, 7)),
            ),
            ("fingerprint", serde_json::to_value([8; 32]).unwrap()),
            ("created_at", serde_json::json!(f.work.created_at() - 1)),
            ("source_revision", serde_json::json!(0)),
            ("game_code", serde_json::json!("WRONG123")),
            ("ended_at", serde_json::Value::Null),
            ("started_at", serde_json::json!(f.rt.now.get())),
        ] {
            let mut bad = original.clone();
            bad[field] = value;
            let wire: ProjectionWire = serde_json::from_value(bad).unwrap();
            let request = f.request();
            let reply = GameRecoveryReply {
                request: request.clone(),
                outcome: super::super::game_recovery_wire::GameRecoveryOutcome::Terminal {
                    projection: wire.clone(),
                },
            };
            assert!(
                reply.validate_for(&request).is_err(),
                "Accepted recovery mismatched {field}"
            );
            let result = f
                .directory()
                .release_game(TerminalProof::new(wire.proof().unwrap()));
            if field == "game_id" {
                // Unknown old targets may acknowledge absence, but never clear this slot.
                assert!(result.is_ok());
            } else {
                assert!(result.is_err(), "Accepted mismatched {field}");
            }
            assert!(f.directory().confirm_reservation(f.work.game_id()).is_ok());
        }
        let g = Fixture::new();
        let ack = g.directory().acknowledge_creation(g.ready).unwrap();
        let grant = g.directory().allocate_game_code(ack).unwrap();
        let intent = match g
            .game()
            .prepare_lobby(Fixture::authority(&g.rt, &g.work), command(&g.rt, 4), 0)
            .unwrap()
        {
            crate::game::LobbyPreparation::Ready(work) => work,
            _ => panic!("Expected actual lobby intent"),
        };
        g.game()
            .commit_lobby(&intent, Fixture::authority(&g.rt, &g.work), &grant)
            .unwrap();
        g.directory()
            .publish_game(g.game().directory_projection().unwrap())
            .unwrap();
        g.rt.now.set(g.work.deadline());
        let terminal = g.game().cleanup().unwrap().unwrap();
        let mut bad = serde_json::to_value(ProjectionWire::from(terminal.projection())).unwrap();
        bad["game_code"] = serde_json::Value::Null;
        let wire: ProjectionWire = serde_json::from_value(bad).unwrap();
        assert_eq!(
            g.directory()
                .release_game(TerminalProof::new(wire.proof().unwrap())),
            Err(DirectoryError::ProofMismatch)
        );
        assert!(g.directory().confirm_reservation(g.work.game_id()).is_ok());
        g.directory().release_game(terminal).unwrap();
    }
    #[test]
    fn ignored_hidden_code_release_writes_cannot_claim_a_durable_ack() {
        use crate::db::{Database, SqlValue};
        for operation in [
            "DELETE ON directory_game_creations",
            "DELETE ON directory_game_index",
            "DELETE ON directory_hosted_nonterminal_games",
            "UPDATE OF game_id ON directory_global_reservation",
            "INSERT ON directory_retired_creations",
        ] {
            let f = Fixture::new();
            let ack = f.directory().acknowledge_creation(f.ready).unwrap();
            let grant = f.directory().allocate_game_code(ack).unwrap();
            f.rt.now.set(f.work.deadline());
            let terminal = f.game().cleanup().unwrap().unwrap();
            let before = f.directory().creation_work(f.work.game_id()).unwrap();
            f.directory.conn.borrow().execute_batch(&format!("CREATE TRIGGER ignore_release BEFORE {operation} BEGIN SELECT RAISE(IGNORE); END")).unwrap();
            assert_eq!(
                f.directory().release_game(terminal),
                Err(DirectoryError::Storage),
                "Ignored {operation} returned success"
            );
            assert_eq!(
                f.directory().creation_work(f.work.game_id()).unwrap(),
                before
            );
            assert_eq!(
                f.directory().game_code(f.work.game_id()).unwrap().as_ref(),
                Some(grant.game_code())
            );
            assert!(f.directory().confirm_reservation(f.work.game_id()).is_ok());
            assert_eq!(
                f.directory
                    .query("SELECT game_id FROM directory_global_reservation", &[])
                    .unwrap(),
                vec![vec![SqlValue::Text(f.work.game_id().to_string())]]
            );
            assert!(
                f.directory
                    .query("SELECT * FROM directory_retired_creations", &[])
                    .unwrap()
                    .is_empty()
            );
        }
    }
    #[test]
    fn ignored_retry_write_cannot_fake_durable_alarm_backoff() {
        let f = Fixture::new();
        let request = f.request();
        f.directory.conn.borrow().execute_batch("CREATE TRIGGER ignore_recovery BEFORE UPDATE OF attempts ON directory_game_creations BEGIN SELECT RAISE(IGNORE); END").unwrap();
        assert_eq!(
            settle_probe(&f.directory(), &request, Err(GamePeerError::Unavailable)),
            Err(DirectoryError::Storage)
        );
        assert_eq!(
            f.directory().creation_work(f.work.game_id()).unwrap(),
            Some(f.work)
        );
    }
    #[test]
    fn unavailable_game_probe_durably_backs_off_the_current_work() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&db).unwrap();
        let directory = DirectoryService::new(&db, &rt).unwrap();
        let work = directory
            .claim_game(
                command(&rt, 1),
                command(&rt, 2),
                CreationFingerprint::from_digest([9; 32]),
            )
            .unwrap();
        let request = GameRecoveryRequest {
            work: (&work).into(),
            game_code: None,
        };
        settle_probe(&directory, &request, Err(GamePeerError::Unavailable)).unwrap();
        let current = directory.creation_work(work.game_id()).unwrap().unwrap();
        assert_eq!(current.attempts(), 1);
        assert_eq!(current.next_retry_at(), Some(rt.now.get() + 1000));
        assert_eq!(current.deadline(), work.deadline());
        assert_eq!(current.ready_revision(), None);
        assert!(directory.confirm_reservation(work.game_id()).is_ok());
    }
}
