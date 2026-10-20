//! Allocation-conscious Game owner adapter; all domain SQL belongs to db::game.
use super::{
    database::OwnerDatabase,
    game_accounts::{AccountCloseReply, AccountCloseRequest, CloseResult},
    game_peers,
    game_recovery::wire::{GameRecoveryOutcome, GameRecoveryReply, GameRecoveryRequest},
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

pub(super) fn privileged_purpose(
    ingress: &GameIngress,
) -> Option<crate::db::game::OperatorPurpose> {
    use crate::db::game::OperatorPurpose as Purpose;
    Some(match ingress.operation {
        GameAction::Create => Purpose::Creation,
        GameAction::Configuration => Purpose::Configuration,
        GameAction::Resume => Purpose::Resume,
        GameAction::HostTransfer => Purpose::HostTransfer,
        GameAction::HostTransferTargets => Purpose::HostTransferTargets,
        GameAction::Stream if ingress.view == Some(super::game_wire::ViewSelector::Account) => {
            Purpose::AccountStream
        }
        GameAction::Lobby => Purpose::Lobby,
        GameAction::Start => Purpose::Start,
        GameAction::CallRandom => Purpose::CallRandom,
        GameAction::CallManual => Purpose::CallManual,
        GameAction::Winner => Purpose::Winner,
        GameAction::Cancel => Purpose::Cancel,
        GameAction::Sync if ingress.view == Some(super::game_wire::ViewSelector::Account) => {
            Purpose::AccountContext
        }
        GameAction::Exit if ingress.view == Some(super::game_wire::ViewSelector::Account) => {
            Purpose::AccountExit
        }
        _ => return None,
    })
}
enum RegistryAvailability {
    Available,
    Unavailable,
}

#[durable_object]
pub struct GameObject {
    pub(super) state: State,
    pub(super) env: Env,
    pub(super) snapshot_send: futures_util::lock::Mutex<()>,
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
        self.persist_recovery_with_gate(service, db, || Ok(()))
            .await
    }
    async fn persist_recovery_with_gate(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        gate: impl Fn() -> Result<(), GameError>,
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
        gate()?;
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
                    gate()?;
                }
            }
            None if alarm.is_some() => {
                storage
                    .delete_alarm()
                    .await
                    .map_err(|_| GameError::Storage)?;
                gate()?;
            }
            None => {}
        }
        storage.sync().await.map_err(|_| GameError::Storage)?;
        gate()?;
        Ok(registry)
    }
    async fn audience_output(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        ingress: &GameIngress,
        out: crate::db::game::AudienceMutation,
    ) -> Result<GameOwnerResponse, GameError> {
        let gate = || {
            service
                .audience_mutation_deadline(
                    &out,
                    ingress.admission_token.as_deref(),
                    ingress.session_token.as_deref(),
                )
                .map(|_| ())
        };
        let _ = self.broadcast(service, db).await;
        let persisted = async {
            gate()?;
            self.persist_recovery_with_gate(service, db, gate).await?;
            service.audience_mutation_deadline(
                &out,
                ingress.admission_token.as_deref(),
                ingress.session_token.as_deref(),
            )
        }
        .await;
        let valid_until = match persisted {
            Ok(deadline) => deadline,
            Err(error) => {
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
                return Err(error);
            }
        };
        let cookie = out
            .participant
            .cookie
            .as_ref()
            .map_or(GameCookieWire::None, |cookie| {
                if out.clears_spectator_cookie {
                    GameCookieWire::Player {
                        token: cookie.token().to_owned(),
                        expires_at: cookie.expires_at(),
                    }
                } else {
                    GameCookieWire::Spectator {
                        token: cookie.token().to_owned(),
                        expires_at: cookie.expires_at(),
                    }
                }
            });
        Ok(GameOwnerResponse::Audience {
            response: out.participant.response,
            cookie,
            valid_until,
            clear_player_cookie: out.clears_player_cookie,
            clear_spectator_cookie: out.clears_spectator_cookie,
        })
    }
    async fn release_audience(
        &self,
        attempt: super::game_wire::AudienceReleaseAttempt,
    ) -> Result<super::game_wire::AudienceReleaseReply, GameError> {
        use super::game_wire::AudienceReleaseRequest;
        let request = &attempt.request;
        let game = match request {
            AudienceReleaseRequest::Sync { authority, .. } => authority.game_id,
            AudienceReleaseRequest::Receipt { receipt, .. } => receipt.game_id,
            AudienceReleaseRequest::Context { game_id, .. } => *game_id,
        };
        if !self.owns_game(game)? {
            return Err(GameError::Unauthorized);
        }
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let gate = || match request {
            AudienceReleaseRequest::Sync { token, authority } => service
                .release_spectator_output(token, &authority.binding())
                .map(|cut| (cut.valid_until, cut.checked_at)),
            AudienceReleaseRequest::Receipt {
                token,
                context,
                receipt,
            } => service
                .release_audience_receipt(receipt, token.as_deref(), context.as_deref())
                .map(|deadline| (deadline, rt.now_ms())),
            AudienceReleaseRequest::Context {
                token,
                game_code,
                game_id,
                expires_at,
            } => service
                .release_spectator_context(token, *game_id, game_code, *expires_at)
                .map(|deadline| (deadline, rt.now_ms())),
        };
        let initial = gate();
        self.state
            .storage()
            .sync()
            .await
            .map_err(|_| GameError::Storage)?;
        let (original_cap, _) = initial?;
        let (deadline, checked_at) = match gate() {
            Ok(d) => d,
            Err(e) => {
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
                return Err(e);
            }
        };
        Ok(super::game_wire::AudienceReleaseReply::Released {
            binding: attempt.binding().map_err(|_| GameError::Storage)?,
            valid_until: deadline.min(original_cap),
            checked_at,
        })
    }
    async fn release_player_sync(
        &self,
        request: super::game_wire::PlayerSyncReleaseRequest,
    ) -> Result<super::game_wire::PlayerSyncReleaseReply, GameError> {
        request
            .authority
            .validate()
            .map_err(|_| GameError::InvalidInput)?;
        if !self.owns_game(request.authority.game_id)? {
            return Err(GameError::Unauthorized);
        }
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let binding = request.authority.binding();
        let initial = service.release_player_sync(&request.token, &binding);
        self.state
            .storage()
            .sync()
            .await
            .map_err(|_| GameError::Storage)?;
        initial?;
        let checked_at = match service.release_player_sync(&request.token, &binding) {
            Ok(now) => now,
            Err(error) => {
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
                return Err(error);
            }
        };
        Ok(super::game_wire::PlayerSyncReleaseReply::Released {
            authority: request.authority.clone(),
            checked_at,
        })
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
    fn attempt_recovery_socket_close(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        close: &crate::db::game::RecoverySocketClose,
        ws: &worker::WebSocket,
    ) -> Result<bool, GameError> {
        let hint = Attachment::read(ws)?;
        if hint.connection_id != close.connection
            || hint.session_id != close.session
            || hint.authentication_epoch != close.epoch
            || !match hint.viewer {
                Viewer::Player(p) => p == close.player,
                Viewer::Spectator(s) => s.to_string() == close.player.to_string(),
                Viewer::Account(_) => false,
            }
        {
            return Err(GameError::Storage);
        }
        if ready_state(ws)? == 3 {
            return Ok(true);
        }
        // Recheck the durable snapshot before physical attempt, including its retry schedule.
        if !service.due_recovery_socket_closes()?.contains(close) {
            return Ok(false);
        }
        if close_checked(ws, CloseCode::Policy).is_err() {
            service.retry_recovery_socket_close(close)?;
            return Ok(false);
        }
        Ok(true)
    }

    /// Presence and broadcast paths may dispatch a known fenced socket through this gate.
    pub(super) fn dispatch_fenced_player_socket_close(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        ws: &worker::WebSocket,
    ) -> Result<(), GameError> {
        let hint = Attachment::read(ws)?;
        let close = service
            .pending_recovery_socket_closes()?
            .into_iter()
            .find(|close| close.connection == hint.connection_id);
        let Some(close) = close else {
            // A later bounded batch owns this fenced identity; absence here is never an ACK.
            return Ok(());
        };
        if self.attempt_recovery_socket_close(service, &close, ws)? {
            service.acknowledge_recovery_socket_close(&close)?;
        }
        Ok(())
    }

    async fn close_recovered_sockets(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
    ) -> Result<(), GameError> {
        let sockets = self.sockets()?;
        for close in service.pending_recovery_socket_closes()? {
            let mut pending = false;
            for ws in &sockets {
                if Attachment::read(ws)?.connection_id == close.connection {
                    pending |= !self.attempt_recovery_socket_close(service, &close, ws)?;
                }
            }
            if !pending {
                service.acknowledge_recovery_socket_close(&close)?;
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
        if service.command_game_missing()?
            && service.pending_work(1)?.is_empty()
            && service.host_transfer_work()?.is_empty()
            && self.sockets()?.is_empty()
        {
            // An audit-only owner has no game view to broadcast or command to recover.
            return self.persist(&service, &db).await;
        }
        // Required SQL/peer recovery is independent of best-effort transport.
        self.persist_recovery(&service, &db).await?;
        let socket_result = self.broadcast(&service, &db).await;
        let close_result = self.close_recovered_sockets(&service).await;
        for work in service.host_transfer_work()? {
            if work.next_attempt_at() <= rt.now_ms() || terminal.is_some() {
                let _ = self.recover_host_transfer(&service, &db, &work, true).await;
            }
        }
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
        close_result.and(socket_result)
    }
    async fn execute(&self, ingress: &GameIngress) -> Result<GameOwnerResponse, GameError> {
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let id = ingress.game_id.ok_or(GameError::InvalidInput)?;
        if !self.owns_game(id)? {
            return Err(GameError::Conflict);
        }
        let token = ingress.session_token.as_deref().unwrap_or("");
        if let Some(purpose) = privileged_purpose(ingress) {
            service.bind_operator_game_identity(id)?;
            let cleanup = service.cleanup();
            self.privileged_operation_result(&service, &db, token, purpose, cleanup)
                .await?;
        } else {
            service.cleanup()?;
        }
        if matches!(
            ingress.operation,
            GameAction::Configuration
                | GameAction::Resume
                | GameAction::HostTransfer
                | GameAction::HostTransferTargets
        ) {
            return self.execute_operator(ingress, &service, &db).await;
        }
        // Leave and Recovery capture current/prepared predecessors before best-effort transport.
        if !matches!(
            ingress.operation,
            GameAction::LeavePlayer
                | GameAction::RecoverPlayer
                | GameAction::LeaveSpectator
                | GameAction::SwitchToPlayer
                | GameAction::SwitchToSpectator
        ) {
            let result = self.broadcast(&service, &db).await;
            if let Some(purpose) = privileged_purpose(ingress) {
                self.privileged_operation_result(&service, &db, token, purpose, result)
                    .await?;
            } else {
                result?;
            }
        }
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
            (GameAction::SpectatorContext, GamePayload::AdmissionContext(input)) => {
                let outcome = service.spectator_admission_context(
                    &input.game_code,
                    ingress.admission_token.as_deref(),
                    ingress.session_token.as_deref(),
                )?;
                self.persist(&service, &db).await?;
                let expires = match outcome.response {
                    GameResponse::AdmissionContext { expires_at } => expires_at,
                    _ => return Err(GameError::Storage),
                };
                let selected = outcome
                    .cookie
                    .as_ref()
                    .map(crate::game::SecretCookie::token)
                    .or(ingress.session_token.as_deref())
                    .or(ingress.admission_token.as_deref())
                    .ok_or(GameError::Unauthorized)?;
                let deadline = match service.release_spectator_context(
                    selected,
                    id,
                    &input.game_code,
                    expires,
                ) {
                    Ok(deadline) => deadline,
                    Err(error) => {
                        self.state
                            .storage()
                            .sync()
                            .await
                            .map_err(|_| GameError::Storage)?;
                        return Err(error);
                    }
                };
                let cookie = outcome
                    .cookie
                    .as_ref()
                    .map_or(GameCookieWire::None, |cookie| {
                        GameCookieWire::SpectatorAdmission {
                            token: cookie.token().to_owned(),
                            expires_at: cookie.expires_at(),
                        }
                    });
                Ok(GameOwnerResponse::Audience {
                    response: outcome.response,
                    cookie,
                    valid_until: Some(deadline),
                    clear_player_cookie: false,
                    clear_spectator_cookie: false,
                })
            }
            (GameAction::JoinSpectator, GamePayload::AdmissionContext(input)) => {
                let out = service.join_spectator(
                    command.ok_or(GameError::InvalidInput)?,
                    ingress.admission_token.as_deref().unwrap_or(""),
                    ingress.session_token.as_deref(),
                    &input.game_code,
                )?;
                self.audience_output(&service, &db, ingress, out).await
            }
            (GameAction::SwitchToSpectator, GamePayload::Empty) => {
                let out =
                    service.switch_to_spectator(command.ok_or(GameError::InvalidInput)?, token)?;
                self.audience_output(&service, &db, ingress, out).await
            }
            (GameAction::SwitchToPlayer, GamePayload::JoinPlayer(input)) => {
                let out = service.switch_to_player(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    input,
                )?;
                self.audience_output(&service, &db, ingress, out).await
            }
            (GameAction::LeaveSpectator, GamePayload::LeavePlayer(input)) => {
                let out = service.leave_spectator(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    input.expected_state,
                )?;
                self.audience_output(&service, &db, ingress, out).await
            }
            (GameAction::Sync, GamePayload::Empty)
                if ingress.view == Some(super::game_wire::ViewSelector::Spectator) =>
            {
                let (response, mut authority) =
                    service.sync_spectator(token, ingress.known_revision)?;
                let initial = service.release_spectator_output(token, &authority);
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
                authority.valid_until = authority.valid_until.min(initial?.valid_until);
                match service.release_spectator_output(token, &authority) {
                    Ok(cut) => authority.valid_until = authority.valid_until.min(cut.valid_until),
                    Err(error) => {
                        self.state
                            .storage()
                            .sync()
                            .await
                            .map_err(|_| GameError::Storage)?;
                        return Err(error);
                    }
                }
                Ok(GameOwnerResponse::AudienceSynced {
                    response,
                    authority: authority.into(),
                })
            }
            (GameAction::Create, GamePayload::Create(input)) => {
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Creation,
                )
                .await?;
                let work = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Creation,
                        game_peers::creation_work(&self.env, id).await,
                    )
                    .await?
                    .ok_or(GameError::NotFound)?;
                if Some(work.command_id()) != command
                    || work.fingerprint()
                        != crate::game::creation_fingerprint(&input.configuration)?
                {
                    return Err(GameError::Conflict);
                }
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Creation,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Creation,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
                let result = service.initialize(&work, authority, &input.configuration)?;
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Creation,
                )
                .await?;
                self.privileged_peer_result(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Creation,
                    game_peers::acknowledge_creation(&self.env, &result.ready).await,
                )
                .await?;
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Creation,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Creation,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
                service.account_view(authority, &self.observed(&service)?)?;
                Ok(success(result.response))
            }
            (GameAction::Lobby, GamePayload::Revision(input)) => {
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Lobby,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Lobby,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
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
                    self.persist_privileged(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Lobby,
                    )
                    .await?;
                    let creation = self
                        .privileged_peer_result(
                            &service,
                            &db,
                            token,
                            crate::db::game::OperatorPurpose::Lobby,
                            game_peers::creation_work(&self.env, id).await,
                        )
                        .await?
                        .ok_or(GameError::Conflict)?;
                    let projection = service.directory_projection()?;
                    if creation.game_id() != projection.game_id()
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
                    self.persist_privileged(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Lobby,
                    )
                    .await?;
                    let ack = self
                        .privileged_peer_result(
                            &service,
                            &db,
                            token,
                            crate::db::game::OperatorPurpose::Lobby,
                            game_peers::acknowledge_creation(&self.env, &ready).await,
                        )
                        .await?;
                    self.persist_privileged(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Lobby,
                    )
                    .await?;
                    let grant = self
                        .privileged_peer_result(
                            &service,
                            &db,
                            token,
                            crate::db::game::OperatorPurpose::Lobby,
                            game_peers::allocate_game_code(&self.env, ack).await,
                        )
                        .await?;
                    self.persist_privileged(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Lobby,
                    )
                    .await?;
                    let authority = self
                        .privileged_peer_result(
                            &service,
                            &db,
                            token,
                            crate::db::game::OperatorPurpose::Lobby,
                            game_peers::authorize_account(&self.env, token).await,
                        )
                        .await?;
                    let current = reload(&service, &work)?;
                    service.commit_lobby(&current, authority, &grant)?;
                }
                let current = reload(&service, &work)?;
                let response = self
                    .privileged_operation_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Lobby,
                        self.publish(&service, &db, &current).await,
                    )
                    .await?;
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Lobby,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Lobby,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
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
            (GameAction::RecoverPlayer, GamePayload::RecoverPlayer(input)) => {
                let code = brews_domain::games::GameCode::from_lookup(&input.game_code)
                    .map_err(|_| GameError::InvalidRecovery)?;
                let outcome = service.recover_player(
                    command.ok_or(GameError::InvalidInput)?,
                    &code,
                    &input.alias,
                    &input.recovery_answer,
                    ingress
                        .recovery_caller
                        .as_deref()
                        .ok_or(GameError::Storage)?,
                )?;
                // Persist the close ledger and alarm before transport or output awaits.
                self.persist(&service, &db).await?;
                let _ = self.close_recovered_sockets(&service).await;
                self.persist(&service, &db).await?;
                service.validate_recovery_outcome(&outcome)?;
                let (game, cookie) = match outcome.cookie.as_ref() {
                    Some(cookie) => (
                        Some(service.recovered_player_view(cookie.token())?),
                        GameCookieWire::Player {
                            token: cookie.token().to_owned(),
                            expires_at: cookie.expires_at(),
                        },
                    ),
                    None => (None, GameCookieWire::None),
                };
                Ok(GameOwnerResponse::Recovered {
                    valid_until: outcome
                        .cookie
                        .as_ref()
                        .map(|cookie| service.recovered_player_deadline(cookie.token()))
                        .transpose()?,
                    receipt: outcome.receipt,
                    game,
                    cookie,
                })
            }
            (GameAction::RecoveryOutput, GamePayload::RecoverPlayer(_)) => {
                self.persist(&service, &db).await?;
                let game = service.recovered_player_view(token)?;
                let receipt = service
                    .recovery_output_receipt(command.ok_or(GameError::InvalidInput)?, token)?;
                let brews_contracts::games::GameOutcome::PlayerRecovered {
                    session_expires_at, ..
                } = receipt.outcome
                else {
                    return Err(GameError::Storage);
                };
                Ok(GameOwnerResponse::Recovered {
                    valid_until: Some(service.recovered_player_deadline(token)?),
                    receipt,
                    game: Some(game),
                    cookie: GameCookieWire::Player {
                        token: token.to_owned(),
                        expires_at: session_expires_at,
                    },
                })
            }
            (GameAction::RenameAlias, GamePayload::RenameAlias(input)) => {
                let outcome = service.rename_my_alias(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    &input.alias,
                )?;
                self.complete_membership(&service, &db, token, outcome)
                    .await
            }
            (GameAction::LeavePlayer, GamePayload::LeavePlayer(input)) => {
                let outcome = service.leave_player(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    input.expected_state,
                )?;
                self.complete_membership(&service, &db, token, outcome)
                    .await
            }
            (GameAction::RenameAliasOutput, GamePayload::RenameAlias(input)) => {
                let outcome = service.membership_output(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    Some(&input.alias),
                    None,
                )?;
                self.complete_membership(&service, &db, token, outcome)
                    .await
            }
            (GameAction::RenameAliasReceiptOutput, GamePayload::RenameAlias(input)) => {
                let outcome = service.membership_receipt_output(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    &input.alias,
                )?;
                self.complete_membership(&service, &db, token, outcome)
                    .await
            }
            (GameAction::LeavePlayerOutput, GamePayload::LeavePlayer(input)) => {
                let outcome = service.membership_output(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    None,
                    Some(input.expected_state),
                )?;
                self.complete_membership(&service, &db, token, outcome)
                    .await
            }
            (GameAction::PutRecoveryAnswer, GamePayload::RecoveryAnswer(input)) => {
                let response = service.manage_recovery_answer(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    Some(&input.recovery_answer),
                )?;
                self.persist(&service, &db).await?;
                Ok(GameOwnerResponse::RecoveryAnswer {
                    response,
                    valid_until: service.recovered_player_deadline(token)?,
                })
            }
            (GameAction::DeleteRecoveryAnswer, GamePayload::Empty) => {
                let response = service.manage_recovery_answer(
                    command.ok_or(GameError::InvalidInput)?,
                    token,
                    None,
                )?;
                self.persist(&service, &db).await?;
                Ok(GameOwnerResponse::RecoveryAnswer {
                    response,
                    valid_until: service.recovered_player_deadline(token)?,
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
                if let Some(purpose) = privileged_purpose(ingress) {
                    self.persist_privileged(&service, &db, token, purpose)
                        .await?;
                } else {
                    self.persist(&service, &db).await?;
                }
                let initial = match ingress.view {
                    Some(super::game_wire::ViewSelector::Player) => {
                        service.sync_player(token, ingress.known_revision)
                    }
                    Some(super::game_wire::ViewSelector::Account) => {
                        let authority = self
                            .privileged_peer_result(
                                &service,
                                &db,
                                token,
                                privileged_purpose(ingress).ok_or(GameError::Storage)?,
                                game_peers::authorize_account(&self.env, token).await,
                            )
                            .await?;
                        service.sync_account(
                            authority,
                            ingress.known_revision,
                            &self.observed(&service)?,
                        )
                    }
                    Some(super::game_wire::ViewSelector::Spectator) | None => {
                        return Err(GameError::InvalidInput);
                    }
                };
                if let Some(purpose) = privileged_purpose(ingress) {
                    self.persist_privileged(&service, &db, token, purpose)
                        .await?;
                } else {
                    self.persist(&service, &db).await?;
                }
                initial?.encode_json().map_err(|_| GameError::Storage)?;
                // Flush maintenance, then derive the protected response without another success await.
                let (response, valid_until, authority, player_authority) = match ingress.view {
                    Some(super::game_wire::ViewSelector::Player) => {
                        let cut = service.sync_player_output(token, ingress.known_revision);
                        let (response, deadline) = match cut {
                            Ok(cut) => cut,
                            Err(error) => {
                                self.state
                                    .storage()
                                    .sync()
                                    .await
                                    .map_err(|_| GameError::Storage)?;
                                return Err(error);
                            }
                        };
                        // Make the cut durable, then recheck its exact original binding.
                        let gate = service.release_player_sync(token, &deadline);
                        self.state
                            .storage()
                            .sync()
                            .await
                            .map_err(|_| GameError::Storage)?;
                        gate?;
                        if let Err(error) = service.release_player_sync(token, &deadline) {
                            self.state
                                .storage()
                                .sync()
                                .await
                                .map_err(|_| GameError::Storage)?;
                            return Err(error);
                        }
                        (response, deadline.valid_until, None, Some(deadline.into()))
                    }
                    Some(super::game_wire::ViewSelector::Account) => {
                        let authority = self
                            .privileged_peer_result(
                                &service,
                                &db,
                                token,
                                privileged_purpose(ingress).ok_or(GameError::Storage)?,
                                game_peers::authorize_account(&self.env, token).await,
                            )
                            .await?;
                        let wire = super::game_wire::SyncAccountAuthority::from(&authority);
                        let cut = service.sync_account_output(
                            authority,
                            ingress.known_revision,
                            &self.observed(&service)?,
                        );
                        let (response, deadline) = match cut {
                            Ok(cut) => cut,
                            Err(error) => {
                                self.state
                                    .storage()
                                    .sync()
                                    .await
                                    .map_err(|_| GameError::Storage)?;
                                return Err(error);
                            }
                        };
                        (response, deadline, Some(wire), None)
                    }
                    Some(super::game_wire::ViewSelector::Spectator) | None => {
                        return Err(GameError::InvalidInput);
                    }
                };
                response.encode_json().map_err(|_| GameError::Storage)?;
                Ok(GameOwnerResponse::Synced {
                    response,
                    valid_until: Some(valid_until),
                    authority,
                    player_authority,
                })
            }
            (GameAction::Exit, GamePayload::Empty) => {
                let command = command.ok_or(GameError::InvalidInput)?;
                if let Some(purpose) = privileged_purpose(ingress) {
                    self.persist_privileged(&service, &db, token, purpose)
                        .await?;
                } else {
                    self.persist(&service, &db).await?;
                }
                let (response, account) = match ingress.view {
                    Some(super::game_wire::ViewSelector::Player) => {
                        (service.exit_player(token, command)?, None)
                    }
                    Some(super::game_wire::ViewSelector::Account) => {
                        let authority = self
                            .privileged_peer_result(
                                &service,
                                &db,
                                token,
                                privileged_purpose(ingress).ok_or(GameError::Storage)?,
                                game_peers::authorize_account(&self.env, token).await,
                            )
                            .await?;
                        let account = authority.account_id();
                        (service.exit_account(authority, command)?, Some(account))
                    }
                    Some(super::game_wire::ViewSelector::Spectator) | None => {
                        return Err(GameError::InvalidInput);
                    }
                };
                // The owner has revoked every same-principal binding before physical closure.
                if let Some(purpose) = privileged_purpose(ingress) {
                    self.persist_privileged(&service, &db, token, purpose)
                        .await?;
                } else {
                    self.persist(&service, &db).await?;
                }
                let _ = self.broadcast(&service, &db).await;
                if let Some(purpose) = privileged_purpose(ingress) {
                    self.persist_privileged(&service, &db, token, purpose)
                        .await?;
                } else {
                    self.persist(&service, &db).await?;
                }
                if let Some(account) = account {
                    let current = self
                        .privileged_peer_result(
                            &service,
                            &db,
                            token,
                            crate::db::game::OperatorPurpose::AccountExit,
                            game_peers::authorize_account(&self.env, token).await,
                        )
                        .await?;
                    if current.account_id() != account || current.expires_at() <= rt.now_ms() {
                        return Err(GameError::Unauthorized);
                    }
                }
                // A player Exit intentionally retires its token; no post-Exit private data is released.
                Ok(success(response))
            }
            (GameAction::CallRandom, GamePayload::Revision(input)) => {
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::CallRandom,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::CallRandom,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
                let response = service.call_random(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    input.expected_revision,
                )?;
                self.complete_gameplay(
                    &service,
                    &db,
                    token,
                    response,
                    false,
                    privileged_purpose(ingress).ok_or(GameError::InvalidInput)?,
                )
                .await
            }
            (GameAction::CallManual, GamePayload::CallManual(input)) => {
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::CallManual,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::CallManual,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
                let response = service.call_manual_value(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    &input.value,
                    input.expected_revision,
                )?;
                self.complete_gameplay(
                    &service,
                    &db,
                    token,
                    response,
                    false,
                    privileged_purpose(ingress).ok_or(GameError::InvalidInput)?,
                )
                .await
            }
            (GameAction::Winner, GamePayload::Winner(input)) => {
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Winner,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Winner,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
                let response = service.submit_winner(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    input.player_id,
                    input.expected_revision,
                )?;
                self.complete_gameplay(
                    &service,
                    &db,
                    token,
                    response,
                    true,
                    privileged_purpose(ingress).ok_or(GameError::InvalidInput)?,
                )
                .await
            }
            (GameAction::Cancel, GamePayload::Cancel(input)) => {
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Cancel,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Cancel,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
                let response = service.cancel_game(
                    authority,
                    command.ok_or(GameError::InvalidInput)?,
                    input.expected_state,
                    input.confirmed,
                )?;
                self.complete_gameplay(
                    &service,
                    &db,
                    token,
                    response,
                    true,
                    privileged_purpose(ingress).ok_or(GameError::InvalidInput)?,
                )
                .await
            }
            (GameAction::Start, GamePayload::Revision(input)) => {
                let command = command.ok_or(GameError::InvalidInput)?;
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Start,
                )
                .await?;
                let reservation = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Start,
                        game_peers::confirm_reservation(&self.env, id).await,
                    )
                    .await?;
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Start,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Start,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
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
                    match self
                        .privileged_operation_result(
                            &service,
                            &db,
                            token,
                            crate::db::game::OperatorPurpose::Start,
                            self.publish(&service, &db, &work).await,
                        )
                        .await?
                    {
                        GameResponse::Pending { operation_id } => {
                            GameResponse::Pending { operation_id }
                        }
                        _ => initial,
                    }
                } else {
                    initial
                };
                self.privileged_operation_result(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Start,
                    self.broadcast(&service, &db).await,
                )
                .await?;
                self.persist_privileged(
                    &service,
                    &db,
                    token,
                    crate::db::game::OperatorPurpose::Start,
                )
                .await?;
                let authority = self
                    .privileged_peer_result(
                        &service,
                        &db,
                        token,
                        crate::db::game::OperatorPurpose::Start,
                        game_peers::authorize_account(&self.env, token).await,
                    )
                    .await?;
                service.account_view(authority, &self.observed(&service)?)?;
                Ok(success(response))
            }
            _ => Err(GameError::InvalidInput),
        }
    }
    async fn privileged_operation_result<T>(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        purpose: crate::db::game::OperatorPurpose,
        result: Result<T, GameError>,
    ) -> Result<T, GameError> {
        if let Err(error) = &result {
            self.capture_operator_peer_error(service, db, token, purpose, *error)
                .await?;
        }
        result
    }
    async fn privileged_peer_result<T>(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        purpose: crate::db::game::OperatorPurpose,
        result: Result<T, game_peers::GamePeerError>,
    ) -> Result<T, GameError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let error = peer_error(error);
                self.capture_operator_peer_error(service, db, token, purpose, error)
                    .await?;
                Err(error)
            }
        }
    }
    async fn persist_privileged(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        purpose: crate::db::game::OperatorPurpose,
    ) -> Result<(), GameError> {
        if let Err(error) = self.persist(service, db).await {
            self.capture_operator_peer_error(service, db, token, purpose, error)
                .await?;
            return Err(error);
        }
        Ok(())
    }
    async fn complete_membership(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        outcome: crate::db::game::MembershipOutcome,
    ) -> Result<GameOwnerResponse, GameError> {
        self.persist_membership(service, db, token, &outcome)
            .await?;
        service.membership_output_deadline(token, &outcome)?;
        self.close_recovered_sockets(service).await?;
        service.membership_output_deadline(token, &outcome)?;
        self.persist_membership(service, db, token, &outcome)
            .await?;
        service.membership_output_deadline(token, &outcome)?;
        self.broadcast(service, db).await?;
        service.membership_output_deadline(token, &outcome)?;
        self.persist_membership(service, db, token, &outcome)
            .await?;
        let valid_until = service.membership_output_deadline(token, &outcome)?;
        let clear_player_cookie = outcome.clears_player_cookie();
        Ok(GameOwnerResponse::Membership {
            response: outcome.response,
            valid_until,
            clear_player_cookie,
        })
    }
    async fn persist_membership(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        outcome: &crate::db::game::MembershipOutcome,
    ) -> Result<(), GameError> {
        match self
            .persist_recovery_with_gate(service, db, || {
                service
                    .membership_output_deadline(token, outcome)
                    .map(|_| ())
            })
            .await?
        {
            RegistryAvailability::Available => Ok(()),
            RegistryAvailability::Unavailable => Err(GameError::Storage),
        }
    }
    async fn complete_gameplay(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        response: GameResponse,
        terminal: bool,
        purpose: crate::db::game::OperatorPurpose,
    ) -> Result<GameOwnerResponse, GameError> {
        let result = async {
            // Terminal cleanup/release proceed even when native socket enumeration is unavailable.
            if terminal {
                self.persist_recovery(service, db).await?;
            } else {
                self.persist(service, db).await?;
            }
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
            if let Err(error) = service.authorize_gameplay_response(authority) {
                // Expiry rejection may write a denial fence; finish durability before rejection.
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
                return Err(error);
            }
            Ok(success(response))
        }
        .await;
        self.privileged_operation_result(service, db, token, purpose, result)
            .await
    }
    async fn finish_terminal_release(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        response: GameResponse,
    ) -> Result<GameResponse, GameError> {
        for work in service.host_transfer_work()? {
            self.recover_host_transfer(service, db, &work, true).await?;
        }
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
pub(super) fn rejection(error: GameError) -> GameRejection {
    match error {
        GameError::InvalidRecovery => GameRejection::InvalidRecovery,
        GameError::RecoveryBlocked => GameRejection::RecoveryBlocked,
        GameError::Unauthorized => GameRejection::Unauthorized,
        GameError::Forbidden => GameRejection::Forbidden,
        GameError::InvalidInput => GameRejection::InvalidInput,
        GameError::NotFound | GameError::Expired => GameRejection::NotFound,
        GameError::ResultNotCommitted
        | GameError::RemainingValuesUnavailable
        | GameError::Conflict
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
            snapshot_send: Default::default(),
            transport_retry: Default::default(),
        }
    }
    async fn fetch(&self, request: Request) -> worker::Result<Response> {
        let path = request.path();
        if matches!(
            path.as_str(),
            "/operator-transfer-probe" | "/operator-transfer-ack"
        ) && request.method() == Method::Post
            && request.url()?.query().is_none()
        {
            return self
                .execute_transfer_recovery(request, path == "/operator-transfer-ack")
                .await;
        }
        if matches!(
            path.as_str(),
            "/read" | "/read-release" | "/read-expiry-denial"
        ) {
            if request.method() != worker::Method::Post || request.url()?.query().is_some() {
                return worker::Response::error("invalid passive read", 400);
            }
            return self.execute_read_transport(request, &path).await;
        }
        if path == "/operator-expiry-denial" {
            if request.method() != worker::Method::Post || request.url()?.query().is_some() {
                return worker::Response::error("invalid operator denial", 400);
            }
            return self.execute_operator_expiry_denial(request).await;
        }
        if matches!(
            path.as_str(),
            "/operator-transport-rejection" | "/operator-transport-failure"
        ) {
            if request.method() != worker::Method::Post || request.url()?.query().is_some() {
                return worker::Response::error("invalid operator rejection", 400);
            }
            let outcome = if path == "/operator-transport-failure" {
                super::operator_runtime::TransportAuditOutcome::Failed
            } else {
                super::operator_runtime::TransportAuditOutcome::Rejected
            };
            return self
                .execute_operator_transport_capture(request, outcome)
                .await;
        }
        if path == "/operator-release"
            && request.method() == Method::Post
            && request.url()?.query().is_none()
        {
            return self.execute_operator_release(request).await;
        }
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
            || !matches!(
                path.as_str(),
                "/command"
                    | "/recovery"
                    | "/close-account"
                    | "/developer-audit"
                    | "/developer-audit-expiry"
                    | "/developer-audit-rejection"
                    | "/developer-audit-failure"
                    | "/history"
                    | "/discovery"
                    | "/sync-player-release"
                    | "/audience-release"
            )
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
        if path == "/audience-release" {
            let request =
                match game_peers::decode::<super::game_wire::AudienceReleaseAttempt>(&bytes) {
                    Ok(r) => r,
                    Err(_) => return Response::error("invalid audience release", 400),
                };
            let binding = request
                .binding()
                .map_err(|_| worker::Error::RustError("invalid audience release".into()))?;
            let reply = match self.release_audience(request).await {
                Ok(r) => r,
                Err(e) => super::game_wire::AudienceReleaseReply::Rejected {
                    binding,
                    code: rejection(e),
                },
            };
            return Response::from_json(&reply);
        }
        if path == "/sync-player-release" {
            let request =
                match game_peers::decode::<super::game_wire::PlayerSyncReleaseRequest>(&bytes) {
                    Ok(request) => request,
                    Err(_) => return Response::error("invalid private Sync release request", 400),
                };
            let reply = match self.release_player_sync(request).await {
                Ok(reply) => reply,
                Err(error) => super::game_wire::PlayerSyncReleaseReply::Rejected {
                    code: rejection(error),
                },
            };
            return Response::from_json(&reply);
        }
        if path == "/developer-audit" {
            return self.execute_developer_audit(&bytes).await;
        }
        if matches!(
            path.as_str(),
            "/developer-audit-rejection" | "/developer-audit-failure"
        ) {
            let outcome = if path == "/developer-audit-failure" {
                super::operator_runtime::TransportAuditOutcome::Failed
            } else {
                super::operator_runtime::TransportAuditOutcome::Rejected
            };
            return self
                .execute_developer_audit_transport(&bytes, outcome)
                .await;
        }
        if path == "/developer-audit-expiry" {
            return self.execute_developer_audit_expiry(&bytes).await;
        }
        if path == "/discovery" {
            let request =
                match game_peers::decode::<super::discovery::wire::DiscoveryOwnerRequest>(&bytes) {
                    Ok(request) => request,
                    Err(_) => return Response::error("invalid private discovery request", 400),
                };
            return self.execute_discovery(request).await;
        }
        if path == "/history" {
            let request =
                match game_peers::decode::<super::history::wire::HistoryOwnerRequest>(&bytes) {
                    Ok(request) => request,
                    Err(_) => return Response::error("invalid private History request", 400),
                };
            return self.execute_history(request).await;
        }
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
                        Err(error) => {
                            if ingress.view == Some(super::game_wire::ViewSelector::Spectator)
                                && self.state.storage().sync().await.is_err()
                            {
                                return Response::from_json(&GameOwnerResponse::Rejected {
                                    code: GameRejection::Unavailable,
                                });
                            }
                            GameOwnerResponse::Rejected {
                                code: rejection(error),
                            }
                        }
                    }
                } else {
                    let result = self.execute(&ingress).await;
                    if result.is_err()
                        && (matches!(
                            ingress.operation,
                            GameAction::SpectatorContext
                                | GameAction::JoinSpectator
                                | GameAction::SwitchToSpectator
                                | GameAction::SwitchToPlayer
                                | GameAction::LeaveSpectator
                        ) || ingress.view == Some(super::game_wire::ViewSelector::Spectator))
                        && self.state.storage().sync().await.is_err()
                    {
                        return Response::from_json(&GameOwnerResponse::Rejected {
                            code: GameRejection::Unavailable,
                        });
                    }
                    result.unwrap_or_else(|error| {
                        use crate::observability::{
                            PlayerLifecycleOperation, player_lifecycle_failure,
                        };
                        match ingress.operation {
                            GameAction::RenameAlias
                            | GameAction::RenameAliasOutput
                            | GameAction::RenameAliasReceiptOutput => player_lifecycle_failure(
                                PlayerLifecycleOperation::RenameAlias,
                                error,
                            ),
                            GameAction::LeavePlayer | GameAction::LeavePlayerOutput => {
                                player_lifecycle_failure(
                                    PlayerLifecycleOperation::LeavePlayer,
                                    error,
                                )
                            }
                            _ => {}
                        }
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
        let replaced_account = if let Ok(hint) = Attachment::read(&ws)
            && matches!(hint.viewer, Viewer::Account(_))
        {
            match service.current_account_connection(hint.session_id) {
                Ok(current) => {
                    let closed = hint.account_grant().map_err(|_| {
                        worker::Error::RustError("game transport unavailable".into())
                    })?;
                    current.as_ref() != Some(&closed)
                }
                Err(GameError::NotFound) => true,
                Err(_) => {
                    return Err(worker::Error::RustError(
                        "game transport unavailable".into(),
                    ));
                }
            }
        } else {
            false
        };
        self.retire_socket(&ws, &service, &db, CloseCode::Normal)
            .await
            .map_err(|_| worker::Error::RustError("game transport unavailable".into()))?;
        // A fenced predecessor changes no view; its broadcast would race successor output.
        if replaced_account {
            return Ok(());
        }
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
