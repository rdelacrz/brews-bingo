//! Slice5 orchestration and protected output cuts. No cross-owner atomicity claim.
use super::{
    OwnerDatabase, WorkerRuntime,
    game_object::{GameObject, peer_error},
    game_peers,
    game_wire::{GameAction, GameIngress, GameOwnerResponse, SyncAccountAuthority},
    operator_peers,
};
use crate::{
    api::GamePayload,
    auth::Runtime,
    db::game::{GameService, OperatorPurpose, TransferPreparation},
    game::{GameError, HostTransferWork},
};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperatorReleaseRequest {
    #[serde(deserialize_with = "operator_peers::decode_challenge")]
    challenge: operator_peers::OperatorChallenge,
    #[serde(deserialize_with = "super::game_peers::object")]
    ingress: GameIngress,
    #[serde(deserialize_with = "super::game_peers::object")]
    response: GameOwnerResponse,
    #[serde(deserialize_with = "super::game_peers::positive_integer")]
    valid_until: i64,
    #[serde(deserialize_with = "super::game_peers::positive_integer")]
    observed_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperatorReleaseReply {
    #[serde(deserialize_with = "operator_peers::decode_challenge")]
    challenge: operator_peers::OperatorChallenge,
    game_id: brews_domain::ids::GameId,
    digest: OperatorReleaseBinding,
    valid_until: i64,
    checked_at: i64,
    rejection: Option<super::game_wire::GameRejection>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperatorTransportRejectionRequest {
    game_id: brews_domain::ids::GameId,
    purpose: OperatorPurpose,
    token: game_peers::SecretToken,
    #[serde(deserialize_with = "operator_peers::decode_challenge")]
    challenge: operator_peers::OperatorChallenge,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperatorTransportRejectionReply {
    game_id: brews_domain::ids::GameId,
    purpose: OperatorPurpose,
    #[serde(deserialize_with = "operator_peers::decode_challenge")]
    challenge: operator_peers::OperatorChallenge,
}
pub(super) async fn audit_rejected_transport(
    env: &worker::Env,
    context: (brews_domain::ids::GameId, OperatorPurpose, String),
) -> Result<(), crate::api::ApiError> {
    let (game_id, purpose, token) = context;
    let token = zeroize::Zeroizing::new(token);
    let request = OperatorTransportRejectionRequest {
        game_id,
        purpose,
        token: game_peers::SecretToken::new(&token),
        challenge: operator_peers::fresh_challenge()
            .map_err(|_| crate::api::ApiError::Unavailable)?,
    };
    let reply: OperatorTransportRejectionReply = game_peers::call(
        env,
        "GAMES",
        &game_id.to_string(),
        "/operator-transport-rejection",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await
    .map_err(|_| crate::api::ApiError::Unavailable)?;
    if reply.game_id != game_id || reply.purpose != purpose || reply.challenge != request.challenge
    {
        return Err(crate::api::ApiError::Unavailable);
    }
    Ok(())
}
#[derive(Serialize, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
struct OperatorReleaseBinding([u8; 32]);
impl Drop for OperatorReleaseBinding {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OperatorExpiryDenialRequest {
    #[serde(deserialize_with = "super::game_peers::object")]
    ingress: GameIngress,
    #[serde(deserialize_with = "super::game_peers::object")]
    response: GameOwnerResponse,
    #[serde(deserialize_with = "super::game_peers::positive_integer")]
    valid_until: i64,
    #[serde(deserialize_with = "super::game_peers::positive_integer")]
    observed_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperatorExpiryDenialReply {
    binding: OperatorReleaseBinding,
}
fn denial_binding(
    request: &OperatorExpiryDenialRequest,
) -> Result<OperatorReleaseBinding, GameError> {
    use sha2::{Digest, Sha256};
    let encoded = game_peers::encode_limit(
        request,
        brews_contracts::games::GAME_FRAME_MAX_BYTES + super::game_wire::GAME_WIRE_MAX_BYTES,
    )
    .map_err(|_| GameError::Storage)?;
    let mut hash = Sha256::new();
    hash.update(b"brews-operator-expiry-denial\0");
    hash.update(encoded.as_slice());
    Ok(OperatorReleaseBinding(hash.finalize().into()))
}
async fn fence_edge_operator_expiry(
    env: &worker::Env,
    request: OperatorExpiryDenialRequest,
) -> Result<(), crate::api::ApiError> {
    let game = request
        .ingress
        .game_id
        .ok_or(crate::api::ApiError::Unavailable)?;
    let expected = denial_binding(&request).map_err(|_| crate::api::ApiError::Unavailable)?;
    let reply: OperatorExpiryDenialReply = game_peers::call(
        env,
        "GAMES",
        &game.to_string(),
        "/operator-expiry-denial",
        &request,
        brews_contracts::games::GAME_FRAME_MAX_BYTES + super::game_wire::GAME_WIRE_MAX_BYTES,
    )
    .await
    .map_err(|_| crate::api::ApiError::Unavailable)?;
    if reply.binding != expected {
        return Err(crate::api::ApiError::Unavailable);
    }
    Ok(())
}
fn request_digest(request: &OperatorReleaseRequest) -> Result<OperatorReleaseBinding, GameError> {
    use sha2::{Digest, Sha256};
    let encoded = game_peers::encode_limit(
        request,
        brews_contracts::games::GAME_FRAME_MAX_BYTES + super::game_wire::GAME_WIRE_MAX_BYTES,
    )
    .map_err(|_| GameError::Storage)?;
    let mut hash = Sha256::new();
    hash.update(b"brews-operator-release-attempt\0");
    hash.update(encoded.as_slice());
    Ok(OperatorReleaseBinding(hash.finalize().into()))
}
fn original_authority(
    response: &GameOwnerResponse,
) -> Result<(&SyncAccountAuthority, i64), GameError> {
    match response {
        GameOwnerResponse::Operator {
            authority,
            valid_until,
            ..
        }
        | GameOwnerResponse::HostTransferTargets {
            authority,
            valid_until,
            ..
        } => Ok((authority, *valid_until)),
        _ => Err(GameError::InvalidInput),
    }
}
impl GameObject {
    pub(super) async fn authorize_operator_account(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        token: &str,
    ) -> Result<crate::auth::GameAccountAuthority, game_peers::GamePeerError> {
        match operator_peers::authorize(&self.env, token).await? {
            operator_peers::OperatorAccountCut::Current(proof) => Ok(proof),
            operator_peers::OperatorAccountCut::Expired { proof, observed_at } => {
                service
                    .fence_operator_output_expiry(observed_at)
                    .map_err(|_| game_peers::GamePeerError::Unavailable)?;
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| game_peers::GamePeerError::Unavailable)?;
                game_peers::fence_discovery_account_expiry(&self.env, token, &proof, observed_at)
                    .await?;
                Err(game_peers::GamePeerError::Unauthorized)
            }
        }
    }
    pub(super) async fn execute_operator_transport_rejection(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            axum::body::to_bytes(
                axum::body::Body::new(http.into_body()),
                game_peers::GAME_PEER_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("operator rejection bound".into()))?
            .to_vec(),
        );
        let request: OperatorTransportRejectionRequest = super::decode_private_json(&bytes)
            .map_err(|_| worker::Error::RustError("invalid operator rejection".into()))?;
        if !self
            .owns_game(request.game_id)
            .map_err(|_| worker::Error::RustError("operator owner unavailable".into()))?
        {
            return worker::Response::error("operator owner mismatch", 400);
        }
        let db = self
            .database()
            .map_err(|_| worker::Error::RustError("operator storage unavailable".into()))?;
        let rt = WorkerRuntime;
        let service = self
            .service(&db, &rt)
            .map_err(|_| worker::Error::RustError("operator unavailable".into()))?;
        service
            .bind_operator_game_identity(request.game_id)
            .map_err(|_| worker::Error::RustError("operator identity unavailable".into()))?;
        match self
            .authorize_operator_account(&service, request.token.as_str())
            .await
        {
            Ok(actor) => {
                if service
                    .capture_operator_rejection(&actor, request.purpose)
                    .is_err()
                {
                    return worker::Response::error("operator audit unavailable", 503);
                }
            }
            Err(game_peers::GamePeerError::Unauthorized) => {}
            Err(_) => return worker::Response::error("operator authority unavailable", 503),
        }
        self.persist(&service, &db).await.map_err(|_| {
            worker::Error::RustError("operator audit durability unavailable".into())
        })?;
        worker::Response::from_json(&OperatorTransportRejectionReply {
            game_id: request.game_id,
            purpose: request.purpose,
            challenge: request.challenge.clone(),
        })
    }
    pub(super) async fn recover_host_transfer(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        work: &HostTransferWork,
        abort: bool,
    ) -> Result<(), GameError> {
        let result = async {
            let proof = service.host_transfer_proof(work, abort)?;
            self.persist(service, db).await?;
            let proof = service.host_transfer_proof(&proof.work, false)?;
            let ack = operator_peers::settle(&self.env, &proof)
                .await
                .map_err(peer_error)?;
            service.acknowledge_host_transfer(&proof, ack)?;
            self.persist(service, db).await?;
            operator_peers::complete(&self.env, &proof.work)
                .await
                .map_err(peer_error)
        }
        .await;
        if result.is_err() {
            service.retry_host_transfer(work)?;
            self.persist(service, db).await?;
        }
        result
    }
    async fn operator_output(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        response: brews_contracts::games::GameResponse,
        purpose: OperatorPurpose,
    ) -> Result<GameOwnerResponse, GameError> {
        let result = async {
            self.persist(service, db).await?;
            let _ = self.broadcast(service, db).await;
            self.persist(service, db).await?;
            let authority = self
                .authorize_operator_account(service, token)
                .await
                .map_err(peer_error)?;
            let valid_until = service.authorize_operator_output(&authority, &response)?;
            Ok(GameOwnerResponse::Operator {
                response,
                authority: (&authority).into(),
                valid_until,
            })
        }
        .await;
        if let Err(error) = &result {
            self.capture_operator_peer_error(service, db, token, purpose, *error)
                .await?;
        }
        result
    }
    async fn capture_operator_peer_error(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        token: &str,
        purpose: OperatorPurpose,
        error: GameError,
    ) -> Result<(), GameError> {
        // Only a fresh authenticated actor supplies audit attribution. If the
        // authority owner cannot accept that check, protected output stays denied.
        if let Ok(actor) = self.authorize_operator_account(service, token).await {
            if error == GameError::Storage {
                service.capture_operator_failure(&actor, purpose)?;
            } else {
                service.capture_operator_rejection(&actor, purpose)?;
            }
            self.persist(service, db).await?;
        }
        Ok(())
    }
    pub(super) async fn execute_operator(
        &self,
        ingress: &GameIngress,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
    ) -> Result<GameOwnerResponse, GameError> {
        service.bind_operator_game_identity(ingress.game_id.ok_or(GameError::InvalidInput)?)?;
        let token = ingress
            .session_token
            .as_deref()
            .ok_or(GameError::Unauthorized)?;
        let authority = self
            .authorize_operator_account(service, token)
            .await
            .map_err(peer_error)?;
        match (
            ingress.operation,
            ingress.payload().map_err(|_| GameError::InvalidInput)?,
        ) {
            (GameAction::Configuration, GamePayload::Configuration(patch)) => {
                let response = service.update_configuration(
                    authority,
                    ingress.command_id.ok_or(GameError::InvalidInput)?,
                    &patch,
                )?;
                self.operator_output(service, db, token, response, OperatorPurpose::Configuration)
                    .await
            }
            (GameAction::Resume, GamePayload::Empty) => {
                let response = service.resume_game(
                    authority,
                    ingress.command_id.ok_or(GameError::InvalidInput)?,
                )?;
                self.operator_output(service, db, token, response, OperatorPurpose::Resume)
                    .await
            }
            (GameAction::HostTransfer, GamePayload::HostTransfer(input)) => {
                let work = match service.prepare_host_transfer(
                    authority,
                    ingress.command_id.ok_or(GameError::InvalidInput)?,
                    &input,
                )? {
                    TransferPreparation::Committed(receipt) => {
                        return self
                            .operator_output(
                                service,
                                db,
                                token,
                                brews_contracts::games::GameResponse::Committed { receipt },
                                OperatorPurpose::HostTransfer,
                            )
                            .await;
                    }
                    TransferPreparation::Prepared(work) => work,
                };
                self.persist(service, db).await?;
                let final_attempt = std::cell::Cell::new(false);
                let result = async {
                    let grant = operator_peers::prepare(&self.env, &work)
                        .await
                        .map_err(peer_error)?;
                    // The last cross-owner check jointly refreshes actor and recipient;
                    // the following Game CAS has no await, credential replay or positive cache.
                    let (actor, recipient) = operator_peers::recipient(&self.env, token, &work)
                        .await
                        .map_err(peer_error)?;
                    final_attempt.set(true);
                    service.commit_host_transfer(actor, recipient, &grant)
                }
                .await;
                match result {
                    Ok(response) => {
                        // After Game commit, failures leave the exact receipt/assignment
                        // and pending cleanup durable. They cannot repeat the transfer.
                        let _ = self.recover_host_transfer(service, db, &work, false).await;
                        self.operator_output(
                            service,
                            db,
                            token,
                            response,
                            OperatorPurpose::HostTransfer,
                        )
                        .await
                    }
                    Err(error) => {
                        let fresh = self.authorize_operator_account(service, token).await;
                        if !final_attempt.get()
                            && let Ok(actor) = fresh
                        {
                            if error == GameError::Storage {
                                service.capture_operator_failure(
                                    &actor,
                                    OperatorPurpose::HostTransfer,
                                )?;
                            } else {
                                service.capture_operator_rejection(
                                    &actor,
                                    OperatorPurpose::HostTransfer,
                                )?;
                            }
                        }
                        let _ = self.recover_host_transfer(service, db, &work, true).await;
                        self.persist(service, db).await?;
                        Err(error)
                    }
                }
            }
            (GameAction::HostTransferTargets, GamePayload::HostTransferTargets(query)) => {
                let binding = service.authorize_host_target_discovery(&authority)?;
                let capture_peer_error = std::cell::Cell::new(true);
                let result = async {
                    let (actor, candidates) = operator_peers::targets(&self.env, token, &query)
                        .await
                        .map_err(peer_error)?;
                    capture_peer_error.set(false);
                    let fresh_binding = service.authorize_host_target_discovery(&actor)?;
                    capture_peer_error.set(true);
                    if fresh_binding != binding {
                        return Err(GameError::Conflict);
                    }
                    let requested = candidates
                        .iter()
                        .map(|target| target.account_id)
                        .collect::<Vec<_>>();
                    let eligible = operator_peers::filter_targets(&self.env, binding.0, &requested)
                        .await
                        .map_err(peer_error)?;
                    let (actor, fresh) = operator_peers::targets(&self.env, token, &query)
                        .await
                        .map_err(peer_error)?;
                    if serde_json::to_vec(&fresh).map_err(|_| GameError::Storage)?
                        != serde_json::to_vec(&candidates).map_err(|_| GameError::Storage)?
                    {
                        return Err(GameError::Conflict);
                    }
                    capture_peer_error.set(false);
                    service.audit_host_target_discovery(&actor, binding)?;
                    capture_peer_error.set(true);
                    let more = candidates.len() > query.limit as usize;
                    let next_cursor = if more {
                        candidates
                            .get(query.limit as usize - 1)
                            .map(|t| t.account_id)
                    } else {
                        None
                    };
                    let targets = candidates
                        .into_iter()
                        .take(query.limit as usize)
                        .filter(|target| {
                            target.account_id != binding.1 && eligible.contains(&target.account_id)
                        })
                        .collect();
                    let response = brews_contracts::games::HostTransferTargets {
                        game_id: binding.0,
                        host_assignment_revision: binding.2,
                        targets,
                        next_cursor,
                    };
                    self.persist(service, db).await?;
                    let actor = self
                        .authorize_operator_account(service, token)
                        .await
                        .map_err(peer_error)?;
                    capture_peer_error.set(false);
                    let fresh_binding = service.authorize_host_target_discovery(&actor)?;
                    capture_peer_error.set(true);
                    if fresh_binding != binding {
                        return Err(GameError::Conflict);
                    }
                    let valid_until = service.operator_discovery_deadline(&actor)?;
                    Ok(GameOwnerResponse::HostTransferTargets {
                        response,
                        authority: (&actor).into(),
                        valid_until,
                    })
                }
                .await;
                if capture_peer_error.get()
                    && let Err(error) = &result
                {
                    self.capture_operator_peer_error(
                        service,
                        db,
                        token,
                        OperatorPurpose::HostTransferTargets,
                        *error,
                    )
                    .await?;
                }
                result
            }
            _ => Err(GameError::InvalidInput),
        }
    }
    pub(super) async fn execute_operator_expiry_denial(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            axum::body::to_bytes(
                axum::body::Body::new(http.into_body()),
                brews_contracts::games::GAME_FRAME_MAX_BYTES
                    + super::game_wire::GAME_WIRE_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("operator denial bound".into()))?
            .to_vec(),
        );
        let request: OperatorExpiryDenialRequest = super::decode_private_json(&bytes)
            .map_err(|_| worker::Error::RustError("invalid operator denial".into()))?;
        let game = request
            .ingress
            .game_id
            .ok_or_else(|| worker::Error::RustError("invalid denial game".into()))?;
        if request.observed_at < request.valid_until
            || !self
                .owns_game(game)
                .map_err(|_| worker::Error::RustError("denial owner unavailable".into()))?
        {
            return worker::Response::error("invalid expiry denial", 400);
        }
        request
            .response
            .validate_for(&request.ingress)
            .map_err(|_| worker::Error::RustError("invalid denial context".into()))?;
        let (original, deadline) = original_authority(&request.response)
            .map_err(|_| worker::Error::RustError("invalid denial cap".into()))?;
        if deadline != request.valid_until {
            return worker::Response::error("denial cap mismatch", 400);
        }
        let db = self
            .database()
            .map_err(|_| worker::Error::RustError("denial storage unavailable".into()))?;
        let rt = WorkerRuntime;
        let service = self
            .service(&db, &rt)
            .map_err(|_| worker::Error::RustError("denial unavailable".into()))?;
        service
            .fence_operator_output_expiry(request.observed_at)
            .map_err(|_| worker::Error::RustError("denial fence unavailable".into()))?;
        self.state.storage().sync().await?;
        let proof = original
            .denial_authority()
            .map_err(|_| worker::Error::RustError("denial authority unavailable".into()))?;
        if request.observed_at >= proof.expires_at() {
            let token = request
                .ingress
                .session_token
                .as_deref()
                .ok_or_else(|| worker::Error::RustError("denial token unavailable".into()))?;
            game_peers::fence_discovery_account_expiry(
                &self.env,
                token,
                &proof,
                request.observed_at,
            )
            .await
            .map_err(|_| worker::Error::RustError("denial Accounts fence unavailable".into()))?;
        }
        self.state.storage().sync().await?;
        worker::Response::from_json(&OperatorExpiryDenialReply {
            binding: denial_binding(&request)
                .map_err(|_| worker::Error::RustError("denial binding unavailable".into()))?,
        })
    }
    pub(super) async fn execute_operator_release(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            axum::body::to_bytes(
                axum::body::Body::new(http.into_body()),
                brews_contracts::games::GAME_FRAME_MAX_BYTES
                    + super::game_wire::GAME_WIRE_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("operator release bound".into()))?
            .to_vec(),
        );
        let request: OperatorReleaseRequest = super::decode_private_json(&bytes)
            .map_err(|_| worker::Error::RustError("invalid operator release".into()))?;
        let game = request
            .ingress
            .game_id
            .ok_or_else(|| worker::Error::RustError("invalid operator game".into()))?;
        let digest = request_digest(&request)
            .map_err(|_| worker::Error::RustError("invalid operator output".into()))?;
        let result = self.release_operator(&request).await;
        if self
            .capture_operator_release_error(&request, &result)
            .await
            .is_err()
        {
            return worker::Response::error("operator release unavailable", 503);
        }
        let reply = OperatorReleaseReply {
            challenge: request.challenge.clone(),
            game_id: game,
            digest,
            valid_until: request.valid_until,
            checked_at: WorkerRuntime.now_ms(),
            rejection: result.err().map(super::game_object::rejection),
        };
        self.state.storage().sync().await?;
        // Denials need durable clock fences; success gets one final fresh Accounts
        // cut and synchronous Game authorization after the last storage await.
        if reply.rejection.is_some() {
            return worker::Response::from_json(&OperatorReleaseReply {
                checked_at: WorkerRuntime.now_ms(),
                ..reply
            });
        }
        let result = self.release_operator_cut(&request).await;
        if self
            .capture_operator_release_error(&request, &result)
            .await
            .is_err()
        {
            return worker::Response::error("operator release unavailable", 503);
        }
        if result.is_err() {
            self.state.storage().sync().await?;
        }
        worker::Response::from_json(&OperatorReleaseReply {
            rejection: result.err().map(super::game_object::rejection),
            checked_at: WorkerRuntime.now_ms(),
            ..reply
        })
    }
    async fn capture_operator_release_error(
        &self,
        request: &OperatorReleaseRequest,
        result: &Result<(), GameError>,
    ) -> worker::Result<()> {
        if !matches!(result, Err(GameError::Storage)) {
            return Ok(());
        }
        let purpose = match request.ingress.operation {
            GameAction::Configuration => OperatorPurpose::Configuration,
            GameAction::Resume => OperatorPurpose::Resume,
            GameAction::HostTransfer => OperatorPurpose::HostTransfer,
            GameAction::HostTransferTargets => OperatorPurpose::HostTransferTargets,
            _ => return Ok(()),
        };
        let db = self
            .database()
            .map_err(|_| worker::Error::RustError("release audit storage unavailable".into()))?;
        let rt = WorkerRuntime;
        let service = self
            .service(&db, &rt)
            .map_err(|_| worker::Error::RustError("release audit unavailable".into()))?;
        let token = request.ingress.session_token.as_deref().ok_or_else(|| {
            worker::Error::RustError("release audit authority unavailable".into())
        })?;
        self.capture_operator_peer_error(&service, &db, token, purpose, GameError::Storage)
            .await
            .map_err(|_| worker::Error::RustError("release audit capture unavailable".into()))
    }
    async fn release_operator(&self, request: &OperatorReleaseRequest) -> Result<(), GameError> {
        request
            .response
            .validate_for(&request.ingress)
            .map_err(|_| GameError::InvalidInput)?;
        let (original, deadline) = original_authority(&request.response)?;
        if deadline != request.valid_until || request.observed_at >= deadline {
            let db = self.database()?;
            let rt = WorkerRuntime;
            let service = self.service(&db, &rt)?;
            service.fence_operator_output_expiry(request.observed_at)?;
            let token = request
                .ingress
                .session_token
                .as_deref()
                .ok_or(GameError::Unauthorized)?;
            let proof = original
                .denial_authority()
                .map_err(|_| GameError::Storage)?;
            if request.observed_at >= proof.expires_at() {
                game_peers::fence_discovery_account_expiry(
                    &self.env,
                    token,
                    &proof,
                    request.observed_at,
                )
                .await
                .map_err(peer_error)?;
            }
            return Err(GameError::Unauthorized);
        }
        self.release_operator_cut(request).await
    }
    async fn release_operator_cut(
        &self,
        request: &OperatorReleaseRequest,
    ) -> Result<(), GameError> {
        let game = request.ingress.game_id.ok_or(GameError::InvalidInput)?;
        if !self.owns_game(game)? {
            return Err(GameError::Unauthorized);
        }
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let (original, deadline) = original_authority(&request.response)?;
        let observed_at = rt.now_ms();
        if observed_at >= deadline {
            service.fence_operator_output_expiry(observed_at)?;
            return Err(GameError::Unauthorized);
        }
        let token = request
            .ingress
            .session_token
            .as_deref()
            .ok_or(GameError::Unauthorized)?;
        if let GameOwnerResponse::HostTransferTargets { response, .. } = &request.response {
            let GamePayload::HostTransferTargets(query) = request
                .ingress
                .payload()
                .map_err(|_| GameError::InvalidInput)?
            else {
                return Err(GameError::InvalidInput);
            };
            let (actor, candidates) = operator_peers::targets(&self.env, token, &query)
                .await
                .map_err(peer_error)?;
            let binding = service.authorize_host_target_discovery(&actor)?;
            if binding.0 != response.game_id
                || binding.2 != response.host_assignment_revision
                || response.targets.iter().any(|t| {
                    t.account_id == binding.1
                        || !candidates
                            .iter()
                            .any(|c| c.account_id == t.account_id && c.username == t.username)
                })
            {
                return Err(GameError::Unauthorized);
            }
            let ids = response
                .targets
                .iter()
                .map(|t| t.account_id)
                .collect::<Vec<_>>();
            if operator_peers::filter_targets(&self.env, game, &ids)
                .await
                .map_err(peer_error)?
                != ids
            {
                return Err(GameError::Unauthorized);
            }
            let (actor, candidates) = operator_peers::targets(&self.env, token, &query)
                .await
                .map_err(peer_error)?;
            if service.authorize_host_target_discovery(&actor)? != binding
                || response.targets.iter().any(|t| {
                    !candidates
                        .iter()
                        .any(|c| c.account_id == t.account_id && c.username == t.username)
                })
            {
                return Err(GameError::Unauthorized);
            }
        }
        let actor = self
            .authorize_operator_account(&service, token)
            .await
            .map_err(peer_error)?;
        let actor =
            if let GameOwnerResponse::HostTransferTargets { response, .. } = &request.response {
                let GamePayload::HostTransferTargets(query) = request
                    .ingress
                    .payload()
                    .map_err(|_| GameError::InvalidInput)?
                else {
                    return Err(GameError::InvalidInput);
                };
                operator_peers::release_targets(&self.env, token, game, &query, &response.targets)
                    .await
                    .map_err(peer_error)?
            } else {
                actor
            };
        if &SyncAccountAuthority::from(&actor) != original {
            return Err(GameError::Unauthorized);
        }
        let observed_at = rt.now_ms();
        if observed_at >= deadline {
            service.fence_operator_output_expiry(observed_at)?;
            if observed_at >= actor.expires_at() {
                game_peers::fence_discovery_account_expiry(&self.env, token, &actor, observed_at)
                    .await
                    .map_err(peer_error)?;
            }
            return Err(GameError::Unauthorized);
        }
        match &request.response {
            GameOwnerResponse::Operator { response, .. } => {
                let (purpose, fp) = match request
                    .ingress
                    .payload()
                    .map_err(|_| GameError::InvalidInput)?
                {
                    GamePayload::Configuration(patch) => (
                        OperatorPurpose::Configuration,
                        crate::db::game::patch_fingerprint(&patch)?,
                    ),
                    GamePayload::HostTransfer(input) => (
                        OperatorPurpose::HostTransfer,
                        crate::db::game::transfer_fingerprint(&input)?,
                    ),
                    GamePayload::Empty if request.ingress.operation == GameAction::Resume => (
                        OperatorPurpose::Resume,
                        crate::db::game::resume_fingerprint(),
                    ),
                    _ => return Err(GameError::InvalidInput),
                };
                service.authorize_operator_request_output(&actor, response, purpose, fp)?;
            }
            GameOwnerResponse::HostTransferTargets { response, .. } => {
                let (id, _, assignment) = service.authorize_host_target_discovery(&actor)?;
                if id != response.game_id || assignment != response.host_assignment_revision {
                    return Err(GameError::Conflict);
                }
            }
            _ => return Err(GameError::InvalidInput),
        }
        Ok(())
    }
}
pub(super) async fn release_edge(
    env: &worker::Env,
    ingress: &GameIngress,
    response: &GameOwnerResponse,
) -> Result<Option<super::game_wire::GameRejection>, crate::api::ApiError> {
    let (_, valid_until) =
        original_authority(response).map_err(|_| crate::api::ApiError::Unavailable)?;
    let observed_at = WorkerRuntime.now_ms();
    let encoded = zeroize::Zeroizing::new(
        serde_json::to_vec(ingress).map_err(|_| crate::api::ApiError::Unavailable)?,
    );
    let ingress: GameIngress =
        serde_json::from_slice(&encoded).map_err(|_| crate::api::ApiError::Unavailable)?;
    let encoded = zeroize::Zeroizing::new(
        serde_json::to_vec(response).map_err(|_| crate::api::ApiError::Unavailable)?,
    );
    let copy: GameOwnerResponse =
        serde_json::from_slice(&encoded).map_err(|_| crate::api::ApiError::Unavailable)?;
    let game = ingress.game_id.ok_or(crate::api::ApiError::Unavailable)?;
    if observed_at >= valid_until {
        fence_edge_operator_expiry(
            env,
            OperatorExpiryDenialRequest {
                ingress,
                response: copy,
                valid_until,
                observed_at,
            },
        )
        .await?;
        return Ok(Some(super::game_wire::GameRejection::Unauthorized));
    }
    let request = OperatorReleaseRequest {
        challenge: operator_peers::fresh_challenge()
            .map_err(|_| crate::api::ApiError::Unavailable)?,
        ingress,
        response: copy,
        valid_until,
        observed_at,
    };
    let reply: OperatorReleaseReply = game_peers::call(
        env,
        "GAMES",
        &game.to_string(),
        "/operator-release",
        &request,
        brews_contracts::games::GAME_FRAME_MAX_BYTES + super::game_wire::GAME_WIRE_MAX_BYTES,
    )
    .await
    .map_err(|_| crate::api::ApiError::Unavailable)?;
    if reply.challenge != request.challenge
        || reply.game_id != game
        || reply.digest
            != request_digest(&request).map_err(|_| crate::api::ApiError::Unavailable)?
        || reply.valid_until != valid_until
        || !(observed_at..=WorkerRuntime.now_ms()).contains(&reply.checked_at)
    {
        return Err(crate::api::ApiError::Unavailable);
    }
    if let Some(code) = reply.rejection {
        return Ok(Some(code));
    }
    let final_now = WorkerRuntime.now_ms();
    if final_now >= valid_until {
        // A final-cut denial is sent back to the owner with the exact sample and
        // awaited; no protected output can be released after rollback.
        fence_edge_operator_expiry(
            env,
            OperatorExpiryDenialRequest {
                ingress: request.ingress,
                response: request.response,
                valid_until,
                observed_at: final_now,
            },
        )
        .await?;
        return Ok(Some(super::game_wire::GameRejection::Unauthorized));
    }
    Ok(None)
}

impl GameObject {
    pub(super) async fn execute_transfer_recovery(
        &self,
        request: worker::Request,
        acknowledge: bool,
    ) -> worker::Result<worker::Response> {
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = axum::body::to_bytes(
            axum::body::Body::new(http.into_body()),
            game_peers::GAME_PEER_MAX_BYTES,
        )
        .await
        .map_err(|_| worker::Error::RustError("transfer recovery bound".into()))?;
        let request: operator_peers::TransferRecoveryRequest =
            super::decode_private_json(&bytes)
                .map_err(|_| worker::Error::RustError("invalid transfer recovery".into()))?;
        let work = request
            .work
            .work()
            .map_err(|_| worker::Error::RustError("invalid transfer proof".into()))?;
        if !self
            .owns_game(work.game)
            .map_err(|_| worker::Error::RustError("transfer owner unavailable".into()))?
        {
            return worker::Response::error("transfer owner mismatch", 400);
        }
        let db = self
            .database()
            .map_err(|_| worker::Error::RustError("transfer storage unavailable".into()))?;
        let rt = WorkerRuntime;
        let service = self
            .service(&db, &rt)
            .map_err(|_| worker::Error::RustError("transfer unavailable".into()))?;
        if acknowledge
            && service
                .host_transfer_work()
                .map_err(|_| worker::Error::RustError("transfer storage unavailable".into()))?
                .iter()
                .all(|pending| pending.operation != work.operation)
        {
            if work.phase == crate::game::TransferPhase::Prepared {
                return worker::Response::error("invalid settled transfer", 400);
            }
            self.state.storage().sync().await?;
            return worker::Response::from_json(&operator_peers::TransferRecoveryAck {
                challenge: request.challenge.clone(),
                work: request.work,
            });
        }
        let proof = service
            .host_transfer_proof(&work, !acknowledge)
            .map_err(|_| worker::Error::RustError("transfer proof unavailable".into()))?;
        if acknowledge {
            if proof.work.phase != work.phase {
                return worker::Response::error("transfer phase mismatch", 400);
            }
            service
                .acknowledge_host_transfer(
                    &proof,
                    crate::game::HostTransferAck { work: work.clone() },
                )
                .map_err(|_| worker::Error::RustError("transfer ack unavailable".into()))?;
            self.persist(&service, &db)
                .await
                .map_err(|_| worker::Error::RustError("transfer durability unavailable".into()))?;
            worker::Response::from_json(&operator_peers::TransferRecoveryAck {
                challenge: request.challenge.clone(),
                work: request.work,
            })
        } else {
            self.persist(&service, &db)
                .await
                .map_err(|_| worker::Error::RustError("transfer durability unavailable".into()))?;
            let proof = service
                .host_transfer_proof(&proof.work, false)
                .map_err(|_| worker::Error::RustError("transfer final proof unavailable".into()))?;
            worker::Response::from_json(&operator_peers::TransferRecoveryReply {
                challenge: request.challenge.clone(),
                requested: request.work,
                work: (&proof.work).into(),
                projection: (&proof.projection).into(),
            })
        }
    }
}
