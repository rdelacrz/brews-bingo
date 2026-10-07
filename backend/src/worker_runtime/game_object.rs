//! Allocation-conscious Game owner adapter; all domain SQL belongs to db::game.
use super::{
    database::OwnerDatabase,
    game_accounts::{AccountCloseReply, AccountCloseRequest, CloseResult},
    game_peers,
    game_recovery_wire::{GameRecoveryOutcome, GameRecoveryReply, GameRecoveryRequest},
    game_sockets::{Attachment, CloseCode, Viewer, ready_state},
    game_wire::{GameAction, GameCookieWire, GameIngress, GameOwnerResponse, GameRejection},
    runtime::WorkerRuntime,
};
use crate::{
    api::GamePayload,
    auth::{AuthPolicy, Runtime},
    config::get_backend_config,
    db::{
        game::{GameService, migrate_game},
        game_delivery::{DeliveryService, migrate_game_delivery},
    },
    directory::games::{CreationReadyProof, SourceRevision},
    game::{GameError, LobbyPreparation, PendingWork, WorkKind, WorkPhase},
};
use brews_contracts::games::GameResponse;
use wasm_bindgen::JsValue;
use worker::{
    DurableObject, Env, HttpRequest, Method, Request, Response, ScheduledTime, State,
    durable_object,
};
use zeroize::Zeroizing;

enum RegistryAvailability {
    Available,
    Unavailable,
}

#[durable_object]
pub struct GameObject {
    pub(super) state: State,
    pub(super) env: Env,
    pub(super) sending:
        std::cell::RefCell<std::collections::BTreeSet<brews_domain::ids::ConnectionId>>,
    pub(super) transport_retry: std::cell::Cell<bool>,
}
impl GameObject {
    pub(super) fn database(&self) -> Result<OwnerDatabase, GameError> {
        let db = OwnerDatabase::new(self.state.storage());
        migrate_game(&db)?;
        migrate_game_delivery(&db).map_err(|_| GameError::Storage)?;
        Ok(db)
    }
    pub(super) fn service<'a>(
        &'a self,
        db: &'a OwnerDatabase,
        rt: &'a WorkerRuntime,
    ) -> Result<GameService<'a, OwnerDatabase, WorkerRuntime>, GameError> {
        let cfg = get_backend_config(&self.env).map_err(|_| GameError::Storage)?;
        GameService::new(
            db,
            rt,
            AuthPolicy {
                m_cost: cfg.argon2_m_cost,
                t_cost: cfg.argon2_t_cost,
                p_cost: cfg.argon2_p_cost,
            },
            cfg.rate_limit_key.expose_secret(),
        )
    }
    pub(super) async fn persist(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
    ) -> Result<(), GameError> {
        match self.persist_recovery(service, db).await? {
            RegistryAvailability::Available => Ok(()),
            RegistryAvailability::Unavailable => Err(GameError::Storage),
        }
    }
    async fn persist_recovery(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
    ) -> Result<RegistryAvailability, GameError> {
        let delivery = DeliveryService::new(db, &WorkerRuntime).map_err(|_| GameError::Storage)?;
        let core_due = service.next_deadline()?;
        let ledger_due = delivery.next_deadline().map_err(|_| GameError::Storage)?;
        let now = WorkerRuntime.now_ms();
        let socket_deadline = (|| {
            let mut socket_due = None;
            for ws in self.sockets()? {
                let due = match (ready_state(&ws)?, Attachment::read(&ws)) {
                    (1, Ok(hint)) => hint.session_expires_at,
                    _ => now.saturating_add(crate::game::RETRY_INITIAL_MS),
                };
                socket_due = Some(socket_due.map_or(due, |prior: i64| prior.min(due)));
            }
            Ok::<_, GameError>(socket_due)
        })();
        let (socket_due, registry) = match socket_deadline {
            Ok(due) => (due, RegistryAvailability::Available),
            // Uncertainty is not absence: persist a retry even after owner reconstruction.
            Err(_) => (
                Some(now.saturating_add(crate::game::RETRY_INITIAL_MS)),
                RegistryAvailability::Unavailable,
            ),
        };
        let retry_due = self
            .transport_retry
            .get()
            .then(|| now.saturating_add(crate::game::RETRY_INITIAL_MS));
        let deadline = core_due
            .into_iter()
            .chain(ledger_due)
            .chain(socket_due)
            .chain(retry_due)
            .min();
        let storage = self.state.storage();
        let alarm = storage.get_alarm().await.map_err(|_| GameError::Storage)?;
        match deadline {
            Some(due) => {
                let now = WorkerRuntime.now_ms();
                let due = if due <= now {
                    now.saturating_add(crate::game::RETRY_INITIAL_MS)
                } else {
                    due
                };
                if alarm != Some(due) {
                    storage
                        .set_alarm(ScheduledTime::new(js_sys::Date::new(&JsValue::from_f64(
                            due as f64,
                        ))))
                        .await
                        .map_err(|_| GameError::Storage)?;
                }
            }
            None if alarm.is_some() => storage
                .delete_alarm()
                .await
                .map_err(|_| GameError::Storage)?,
            None => {}
        }
        storage.sync().await.map_err(|_| GameError::Storage)?;
        Ok(registry)
    }
    async fn recover(
        &self,
        request: &GameRecoveryRequest,
    ) -> Result<GameRecoveryOutcome, GameError> {
        let work = request.work.work().map_err(peer_error)?;
        if !self.owns_game(work.game_id())? {
            return Err(GameError::Conflict);
        }
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let terminal = service.cleanup()?;
        if terminal.is_some() {
            // Pre-start grants are gone; started grants receive their permitted final view.
            let _ = self.broadcast(&service, &db).await;
        }
        let projection = match service.directory_projection() {
            Ok(projection) => projection,
            Err(GameError::NotFound) if rt.now_ms() < work.deadline() => {
                self.persist(&service, &db).await?;
                return Ok(GameRecoveryOutcome::Absent {});
            }
            Err(GameError::NotFound) => {
                let sealed = service.seal_abandoned_creation(&work)?;
                self.persist(&service, &db).await?;
                let reply = GameRecoveryReply {
                    request: request.clone(),
                    outcome: GameRecoveryOutcome::Terminal {
                        projection: sealed.projection().into(),
                    },
                };
                return reply.validate_for(request).map_err(peer_error);
            }
            Err(error) => return Err(error),
        };
        let outcome = if let Some(terminal) = terminal {
            GameRecoveryOutcome::Terminal {
                projection: terminal.projection().into(),
            }
        } else if projection.state() == brews_domain::games::GameState::New {
            let ready = service.creation_ready(&work)?;
            GameRecoveryOutcome::Ready {
                ready: (&ready).into(),
                projection: (&projection).into(),
            }
        } else {
            GameRecoveryOutcome::Committed {
                projection: (&projection).into(),
            }
        };
        let reply = GameRecoveryReply {
            request: request.clone(),
            outcome,
        };
        let outcome = reply.validate_for(request).map_err(peer_error)?;
        self.persist(&service, &db).await?;
        // Re-read after the output gate; expiry may have changed the owner meanwhile.
        if service.directory_projection()? != projection {
            return Err(GameError::Conflict);
        }
        Ok(outcome)
    }
    async fn close_account(&self, request: &AccountCloseRequest) -> Result<(), GameError> {
        let target = &request.target;
        if !self.owns_game(target.game_id)? {
            return Err(GameError::Conflict);
        }
        let work = crate::auth::GameSocketCloseWork::from_trusted_peer(
            target.account_id,
            target.session_id,
            target.game_id,
            target.connection_id,
            target.operation_id,
            target.epoch,
            target.expires,
            target.created_at,
        )
        .map_err(|_| GameError::InvalidInput)?;
        let db = self.database()?;
        let service = self.service(&db, &WorkerRuntime)?;
        // Local authority is fenced first. Do not call back into an awaiting Accounts owner.
        service.close_account_connection(&work)?;
        self.persist(&service, &db).await?;
        let delivery = DeliveryService::new(&db, &WorkerRuntime).map_err(|_| GameError::Storage)?;
        for ws in self.sockets()? {
            let hint = match Attachment::read(&ws) {
                Ok(hint) => hint,
                Err(_) => {
                    close_checked(&ws, CloseCode::Policy)?;
                    continue;
                }
            };
            if hint.viewer == Viewer::Account(target.account_id)
                && hint.game_id == target.game_id
                && hint.session_id == target.session_id
                && hint.connection_id == target.connection_id
                && hint.authentication_epoch == target.epoch
                && hint.session_expires_at == target.expires
            {
                close_checked(&ws, CloseCode::Policy)?;
                delivery
                    .retire_connection(hint.binding())
                    .map_err(|_| GameError::Storage)?;
            }
        }
        // Include already-absent targets only after native enumeration and local fencing.
        delivery
            .retire_connection(crate::db::game_delivery::DeliveryBinding {
                connection_id: target.connection_id,
                session_id: target.session_id,
                auth_epoch: target.epoch as u64,
                expires_at: target.expires,
            })
            .map_err(|_| GameError::Storage)?;
        self.persist(&service, &db).await?;
        for ws in self.sockets()? {
            let hint = Attachment::read(&ws)?;
            if hint.identity().is_ok_and(|identity| {
                identity.account_id == target.account_id
                    && identity.session_id == target.session_id
                    && identity.game_id == target.game_id
                    && identity.connection_id == target.connection_id
                    && identity.epoch == target.epoch
                    && identity.expires == target.expires
            }) && ready_state(&ws)? != 3
            {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }
    async fn wake(&self) -> Result<(), GameError> {
        const RECOVERY_BATCH_LIMIT: u32 = 10;
        self.transport_retry.set(false);
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let terminal = service.cleanup()?;
        DeliveryService::new(&db, &rt)
            .map_err(|_| GameError::Storage)?
            .cleanup_expired()
            .map_err(|_| GameError::Storage)?;
        // Required SQL/peer recovery is independent of best-effort transport.
        self.persist_recovery(&service, &db).await?;
        let socket_result = self.broadcast(&service, &db).await;
        let due = service.pending_work(RECOVERY_BATCH_LIMIT)?;
        for snapshot in due {
            if snapshot.next_attempt_at() > rt.now_ms()
                && !(terminal.is_some()
                    && snapshot.kind() == WorkKind::Release
                    && snapshot.attempt_count() == 0)
            {
                continue;
            }
            let current = match reload(&service, &snapshot) {
                Ok(current) if current == snapshot => current,
                Ok(_) | Err(GameError::Conflict) => continue,
                Err(error) => return Err(error),
            };
            if current.kind() == WorkKind::Release {
                let result = self.release_terminal_work(&service, &db, &current).await;
                if result.is_err()
                    && let Ok(current) = reload(&service, &current)
                {
                    service.retry_work(&current)?;
                }
            } else if current.phase() == WorkPhase::Prepared {
                // An actor ID in durable intent cannot recreate a fresh account credential.
                service.retry_work(&current)?;
            } else {
                let result = self.publish(&service, &db, &current).await;
                if result.is_err()
                    && let Ok(current) = reload(&service, &current)
                {
                    service.retry_work(&current)?;
                }
            }
            self.persist_recovery(&service, &db).await?;
        }
        self.persist(&service, &db).await?;
        socket_result
    }
    async fn execute(&self, ingress: &GameIngress) -> Result<GameOwnerResponse, GameError> {
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let id = ingress.game_id.ok_or(GameError::InvalidInput)?;
        if !self.owns_game(id)? {
            return Err(GameError::Conflict);
        }
        service.cleanup()?;
        self.broadcast(&service, &db).await?;
        // A caller cannot retarget an existing Game by supplying a different typed ID.
        if let Ok(projection) = service.directory_projection()
            && projection.game_id() != id
        {
            return Err(GameError::Conflict);
        }
        let token = ingress.session_token.as_deref().unwrap_or("");
        let command = ingress.command_id;
        let payload = ingress.payload().map_err(|_| GameError::InvalidInput)?;
        match (ingress.operation, payload) {
            (GameAction::Create, GamePayload::Create(input)) => {
                self.persist(&service, &db).await?;
                let work = game_peers::creation_work(&self.env, id)
                    .await
                    .map_err(peer_error)?
                    .ok_or(GameError::NotFound)?;
                if Some(work.command_id()) != command
                    || work.fingerprint()
                        != crate::game::creation_fingerprint(&input.configuration)?
                {
                    return Err(GameError::Conflict);
                }
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                let result = service.initialize(&work, authority, &input.configuration)?;
                self.persist(&service, &db).await?;
                game_peers::acknowledge_creation(&self.env, &result.ready)
                    .await
                    .map_err(peer_error)?;
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                service.account_view(authority, &self.observed(&service)?)?;
                Ok(success(result.response))
            }
            (GameAction::Lobby, GamePayload::Revision(input)) => {
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                let command = command.ok_or(GameError::InvalidInput)?;
                let prepared =
                    service.prepare_lobby(authority, command, input.expected_revision)?;
                let work = match prepared {
                    LobbyPreparation::Committed(receipt) => {
                        return Ok(success(GameResponse::Committed { receipt }));
                    }
                    LobbyPreparation::Ready(work) => work,
                };
                if work.phase() == WorkPhase::Prepared {
                    self.persist(&service, &db).await?;
                    let creation = game_peers::creation_work(&self.env, id)
                        .await
                        .map_err(peer_error)?
                        .ok_or(GameError::Conflict)?;
                    let projection = service.directory_projection()?;
                    if creation.game_id() != projection.game_id()
                        || creation.account_id() != projection.designated_host_id()
                        || creation.fingerprint() != projection.fingerprint()
                        || creation.created_at() != projection.created_at()
                    {
                        return Err(GameError::Conflict);
                    }
                    let ready = CreationReadyProof::new(
                        creation.game_id(),
                        creation.account_id(),
                        creation.command_id(),
                        creation.fingerprint(),
                        creation.created_at(),
                        SourceRevision::INITIAL,
                    );
                    self.persist(&service, &db).await?;
                    let ack = game_peers::acknowledge_creation(&self.env, &ready)
                        .await
                        .map_err(peer_error)?;
                    self.persist(&service, &db).await?;
                    let grant = game_peers::allocate_game_code(&self.env, ack)
                        .await
                        .map_err(peer_error)?;
                    self.persist(&service, &db).await?;
                    let authority = game_peers::authorize_account(&self.env, token)
                        .await
                        .map_err(peer_error)?;
                    let current = reload(&service, &work)?;
                    service.commit_lobby(&current, authority, &grant)?;
                }
                let current = reload(&service, &work)?;
                let response = self.publish(&service, &db, &current).await?;
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                service.account_view(authority, &self.observed(&service)?)?;
                Ok(success(response))
            }
            (GameAction::AdmissionContext, GamePayload::AdmissionContext(input)) => {
                let outcome = service.admission_context(
                    &input.game_code,
                    ingress.admission_token.as_deref(),
                    ingress.session_token.as_deref(),
                )?;
                self.persist(&service, &db).await?;
                if service.directory_projection()?.state()
                    == brews_domain::games::GameState::Cancelled
                {
                    return Err(GameError::Expired);
                }
                let cookie = outcome
                    .cookie
                    .as_ref()
                    .map_or(GameCookieWire::None, |cookie| GameCookieWire::Admission {
                        token: cookie.token().to_owned(),
                        expires_at: cookie.expires_at(),
                    });
                Ok(GameOwnerResponse::Success {
                    response: outcome.response,
                    cookie,
                })
            }
            (GameAction::JoinPlayer, GamePayload::JoinPlayer(input)) => {
                let outcome = service.join_player(
                    command.ok_or(GameError::InvalidInput)?,
                    ingress
                        .admission_token
                        .as_deref()
                        .ok_or(GameError::Unauthorized)?,
                    ingress.session_token.as_deref(),
                    input,
                )?;
                self.broadcast(&service, &db).await?;
                self.persist(&service, &db).await?;
                if let Some(cookie) = outcome.cookie.as_ref() {
                    service.player_view(cookie.token())?;
                }
                let cookie = outcome
                    .cookie
                    .as_ref()
                    .map_or(GameCookieWire::None, |cookie| GameCookieWire::Player {
                        token: cookie.token().to_owned(),
                        expires_at: cookie.expires_at(),
                    });
                Ok(GameOwnerResponse::Success {
                    response: outcome.response,
                    cookie,
                })
            }
            (GameAction::Sync, GamePayload::Empty) => {
                self.persist(&service, &db).await?;
                let response = match ingress.view {
                    Some(super::game_wire::ViewSelector::Player) => {
                        service.sync_player(token, ingress.known_revision)?
                    }
                    Some(super::game_wire::ViewSelector::Account) => {
                        let authority = game_peers::authorize_account(&self.env, token)
                            .await
                            .map_err(peer_error)?;
                        service.sync_account(
                            authority,
                            ingress.known_revision,
                            &self.observed(&service)?,
                        )?
                    }
                    None => return Err(GameError::InvalidInput),
                };
                response.encode_json().map_err(|_| GameError::Storage)?;
                self.persist(&service, &db).await?;
                let response = match ingress.view {
                    Some(super::game_wire::ViewSelector::Player) => {
                        service.sync_player(token, ingress.known_revision)?
                    }
                    Some(super::game_wire::ViewSelector::Account) => {
                        let authority = game_peers::authorize_account(&self.env, token)
                            .await
                            .map_err(peer_error)?;
                        service.sync_account(
                            authority,
                            ingress.known_revision,
                            &self.observed(&service)?,
                        )?
                    }
                    None => return Err(GameError::InvalidInput),
                };
                response.encode_json().map_err(|_| GameError::Storage)?;
                Ok(GameOwnerResponse::Synced { response })
            }
            (GameAction::Exit, GamePayload::Empty) => {
                let command = command.ok_or(GameError::InvalidInput)?;
                self.persist(&service, &db).await?;
                let (response, account) = match ingress.view {
                    Some(super::game_wire::ViewSelector::Player) => {
                        (service.exit_player(token, command)?, None)
                    }
                    Some(super::game_wire::ViewSelector::Account) => {
                        let authority = game_peers::authorize_account(&self.env, token)
                            .await
                            .map_err(peer_error)?;
                        let account = authority.account_id();
                        (service.exit_account(authority, command)?, Some(account))
                    }
                    None => return Err(GameError::InvalidInput),
                };
                // The owner has revoked every same-principal binding before physical closure.
                self.persist(&service, &db).await?;
                let _ = self.broadcast(&service, &db).await;
                self.persist(&service, &db).await?;
                if let Some(account) = account {
                    let current = game_peers::authorize_account(&self.env, token)
                        .await
                        .map_err(peer_error)?;
                    if current.account_id() != account || current.expires_at() <= rt.now_ms() {
                        return Err(GameError::Unauthorized);
                    }
                }
                // A player Exit intentionally retires its token; no post-Exit private data is released.
                Ok(success(response))
            }
            (GameAction::CallRandom, GamePayload::Revision(input)) => {
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                let response = service.call_random(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    input.expected_revision,
                )?;
                self.complete_gameplay(&service, &db, token, response, false)
                    .await
            }
            (GameAction::CallManual, GamePayload::CallManual(input)) => {
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                let response = service.call_manual_value(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    &input.value,
                    input.expected_revision,
                )?;
                self.complete_gameplay(&service, &db, token, response, false)
                    .await
            }
            (GameAction::Winner, GamePayload::Winner(input)) => {
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                let response = service.submit_winner(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    input.player_id,
                    input.expected_revision,
                )?;
                self.complete_gameplay(&service, &db, token, response, true)
                    .await
            }
            (GameAction::Cancel, GamePayload::Cancel(input)) => {
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                let response = service.cancel_game(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    input.expected_state,
                    input.confirmed,
                )?;
                self.complete_gameplay(&service, &db, token, response, true)
                    .await
            }
            (GameAction::Start, GamePayload::Revision(input)) => {
                let command = command.ok_or(GameError::InvalidInput)?;
                self.persist(&service, &db).await?;
                let reservation = game_peers::confirm_reservation(&self.env, id)
                    .await
                    .map_err(peer_error)?;
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                let initial = service.start(
                    authority,
                    command,
                    input.expected_revision,
                    &reservation,
                    &self.observed(&service)?,
                )?;
                let pending = service
                    .pending_work(100)?
                    .into_iter()
                    .find(|w| w.command_id() == command);
                let response = if let Some(work) = pending {
                    match self.publish(&service, &db, &work).await? {
                        GameResponse::Pending { operation_id } => {
                            GameResponse::Pending { operation_id }
                        }
                        _ => initial,
                    }
                } else {
                    initial
                };
                self.broadcast(&service, &db).await?;
                self.persist(&service, &db).await?;
                let authority = game_peers::authorize_account(&self.env, token)
                    .await
                    .map_err(peer_error)?;
                service.account_view(authority, &self.observed(&service)?)?;
                Ok(success(response))
            }
            _ => Err(GameError::InvalidInput),
        }
    }
    async fn complete_gameplay(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        response: GameResponse,
        terminal: bool,
    ) -> Result<GameOwnerResponse, GameError> {
        // Schedule durable recovery before any post-commit peer/output await.
        self.persist(service, db).await?;
        let delivery = self.broadcast(service, db).await;
        let response = if terminal {
            // Final delivery is best-effort; viewer ACK/Exit never gates release.
            self.finish_terminal_release(service, db, response).await?
        } else {
            delivery?;
            response
        };
        self.persist(service, db).await?;
        let authority = game_peers::authorize_account(&self.env, token)
            .await
            .map_err(peer_error)?;
        service.authorize_gameplay_response(authority)?;
        Ok(success(response))
    }
    async fn finish_terminal_release(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        response: GameResponse,
    ) -> Result<GameResponse, GameError> {
        let pending = service
            .pending_work(100)?
            .into_iter()
            .find(|work| work.kind() == WorkKind::Release);
        let Some(work) = pending else {
            return Ok(response);
        };
        match self.release_terminal_work(service, db, &work).await {
            Ok(()) => Ok(response),
            Err(_) => {
                if let Ok(current) = reload(service, &work) {
                    service.retry_work(&current)?;
                    self.persist(service, db).await?;
                    Ok(GameResponse::Pending {
                        operation_id: work.operation_id(),
                    })
                } else {
                    // A concurrent exact ACK can already have completed the intent.
                    if service
                        .pending_work(100)?
                        .iter()
                        .any(|pending| pending.kind() == WorkKind::Release)
                    {
                        return Err(GameError::Conflict);
                    }
                    Ok(response)
                }
            }
        }
    }
    async fn release_terminal_work(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        expected: &PendingWork,
    ) -> Result<(), GameError> {
        let proof = service.cleanup()?.ok_or(GameError::Conflict)?;
        self.persist_recovery(service, db).await?;
        let before = reload(service, expected)?;
        if before != *expected || before.kind() != WorkKind::Release {
            return Err(GameError::Conflict);
        }
        let ack = game_peers::release_game(&self.env, &proof)
            .await
            .map_err(peer_error)?;
        let after = reload(service, &before)?;
        if service
            .cleanup()?
            .as_ref()
            .map(|current| current.projection())
            != Some(proof.projection())
        {
            return Err(GameError::Conflict);
        }
        service.acknowledge_release(&after, ack)
    }
    pub(super) async fn publish(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        work: &PendingWork,
    ) -> Result<GameResponse, GameError> {
        let projection = service.directory_projection()?;
        self.persist(service, db).await?;
        let before = reload(service, work)?;
        if before.phase() != WorkPhase::AwaitingAcknowledgement
            || before.fence_revision() != projection.source_revision()
            || service.directory_projection()? != projection
        {
            return Err(GameError::Conflict);
        }
        let ack = match game_peers::publish_game(&self.env, &projection).await {
            Ok(ack) => ack,
            Err(_) => {
                if let Ok(current) = reload(service, work) {
                    service.retry_work(&current)?;
                }
                self.persist(service, db).await?;
                return Ok(GameResponse::Pending {
                    operation_id: work.operation_id(),
                });
            }
        };
        let current = reload(service, &before)?;
        if service.directory_projection()? != projection {
            return Err(GameError::Conflict);
        }
        let result = service.acknowledge_projection(&current, ack);
        self.persist(service, db).await?;
        result
    }
}
fn close_checked(ws: &worker::WebSocket, code: CloseCode) -> Result<(), GameError> {
    if ready_state(ws)? != 3 {
        ws.close(Some(code.value()), Some("session unavailable"))
            .map_err(|_| GameError::Storage)?;
    }
    if ready_state(ws)? != 3 {
        return Err(GameError::Storage);
    }
    Ok(())
}
fn private_response(value: &impl serde::Serialize) -> worker::Result<Response> {
    let bytes = game_peers::encode(value)
        .map_err(|_| worker::Error::RustError("private game response bound".into()))?;
    let mut response = Response::from_bytes(bytes.to_vec())?;
    response
        .headers_mut()
        .set("Content-Type", "application/json")?;
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}
pub(super) fn reload(
    service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
    expected: &PendingWork,
) -> Result<PendingWork, GameError> {
    service
        .pending_work(100)?
        .into_iter()
        .find(|work| work.operation_id() == expected.operation_id())
        .ok_or(GameError::Conflict)
}
pub(super) fn peer_error(error: game_peers::GamePeerError) -> GameError {
    match error {
        game_peers::GamePeerError::Unauthorized => GameError::Unauthorized,
        game_peers::GamePeerError::Conflict => GameError::Conflict,
        game_peers::GamePeerError::InvalidInput => GameError::InvalidInput,
        game_peers::GamePeerError::Unavailable => GameError::Storage,
    }
}
fn success(response: GameResponse) -> GameOwnerResponse {
    GameOwnerResponse::Success {
        response,
        cookie: GameCookieWire::None,
    }
}
fn rejection(error: GameError) -> GameRejection {
    match error {
        GameError::Unauthorized => GameRejection::Unauthorized,
        GameError::Forbidden => GameRejection::Forbidden,
        GameError::InvalidInput => GameRejection::InvalidInput,
        GameError::NotFound | GameError::Expired => GameRejection::NotFound,
        GameError::Conflict
        | GameError::StaleRevision
        | GameError::Capacity
        | GameError::Infeasible => GameRejection::Conflict,
        GameError::StaleCommand => GameRejection::StaleCommand,
        _ => GameRejection::Unavailable,
    }
}
impl DurableObject for GameObject {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            sending: Default::default(),
            transport_retry: Default::default(),
        }
    }
    async fn fetch(&self, request: Request) -> worker::Result<Response> {
        let path = request.path();
        if request.method() == Method::Get
            && path == "/command"
            && request.url()?.query().is_none()
            && request.headers().get("Upgrade")?.as_deref() == Some("websocket")
        {
            let encoded = request
                .headers()
                .get(super::game_wire::STREAM_INGRESS_HEADER)?
                .map(Zeroizing::new);
            let ingress = encoded
                .as_ref()
                .and_then(|text| GameIngress::decode_json(text.as_bytes()).ok())
                .filter(|ingress| ingress.operation == GameAction::Stream);
            let outcome = match ingress {
                Some(ingress) => match self.upgrade(&ingress).await {
                    Ok(response) => return Ok(response),
                    Err(error) => GameOwnerResponse::Rejected {
                        code: rejection(error),
                    },
                },
                None => GameOwnerResponse::Rejected {
                    code: GameRejection::InvalidInput,
                },
            };
            return Response::from_json(&outcome);
        }
        if request.method() != Method::Post
            || request.url()?.query().is_some()
            || !matches!(path.as_str(), "/command" | "/recovery" | "/close-account")
        {
            return Response::error("invalid private game request", 400);
        }
        let http: HttpRequest = request.try_into()?;
        let bytes = Zeroizing::new(
            axum::body::to_bytes(
                axum::body::Body::new(http.into_body()),
                super::game_wire::GAME_WIRE_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("invalid private game request".into()))?
            .to_vec(),
        );
        if path == "/recovery" {
            let request: GameRecoveryRequest = match game_peers::decode(&bytes) {
                Ok(request) => request,
                Err(_) => return Response::error("invalid private game request", 400),
            };
            let outcome = self
                .recover(&request)
                .await
                .unwrap_or(GameRecoveryOutcome::Unavailable {});
            return private_response(&GameRecoveryReply { request, outcome });
        }
        if path == "/close-account" {
            let request: AccountCloseRequest = match game_peers::decode(&bytes) {
                Ok(request) => request,
                Err(_) => return Response::error("invalid private game request", 400),
            };
            let result = if self.close_account(&request).await.is_ok() {
                CloseResult::Closed
            } else {
                CloseResult::Unavailable
            };
            return private_response(&AccountCloseReply { result, request });
        }
        let outcome = match GameIngress::decode_json(&bytes) {
            Ok(ingress) => {
                if ingress.operation == GameAction::Stream {
                    match self.upgrade(&ingress).await {
                        Ok(response) => return Ok(response),
                        Err(error) => GameOwnerResponse::Rejected {
                            code: rejection(error),
                        },
                    }
                } else {
                    self.execute(&ingress).await.unwrap_or_else(|error| {
                        GameOwnerResponse::Rejected {
                            code: rejection(error),
                        }
                    })
                }
            }
            Err(()) => GameOwnerResponse::Rejected {
                code: GameRejection::InvalidInput,
            },
        };
        Response::from_json(&outcome)
    }
    async fn alarm(&self) -> worker::Result<Response> {
        self.wake()
            .await
            .map_err(|_| worker::Error::RustError("game recovery unavailable".into()))?;
        Response::ok("game recovery complete")
    }
    async fn websocket_message(
        &self,
        ws: worker::WebSocket,
        message: worker::WebSocketIncomingMessage,
    ) -> worker::Result<()> {
        self.incoming(ws, message)
            .await
            .map_err(|_| worker::Error::RustError("game transport unavailable".into()))
    }
    async fn websocket_close(
        &self,
        ws: worker::WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> worker::Result<()> {
        let db = self
            .database()
            .map_err(|_| worker::Error::RustError("game transport unavailable".into()))?;
        let service = self
            .service(&db, &WorkerRuntime)
            .map_err(|_| worker::Error::RustError("game transport unavailable".into()))?;
        self.retire_socket(&ws, &service, &db, CloseCode::Normal)
            .await
            .map_err(|_| worker::Error::RustError("game transport unavailable".into()))?;
        self.broadcast(&service, &db)
            .await
            .map_err(|_| worker::Error::RustError("game transport unavailable".into()))
    }
    async fn websocket_error(
        &self,
        ws: worker::WebSocket,
        _error: worker::Error,
    ) -> worker::Result<()> {
        DurableObject::websocket_close(
            self,
            ws,
            usize::from(CloseCode::Resynchronize.value()),
            String::new(),
            false,
        )
        .await
    }
}
