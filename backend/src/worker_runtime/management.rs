//! Restricted CLI adapter; principals are derived here, never deserialized from clients.
use super::{AccountsObject, OwnerDatabase, WorkerRuntime, edge};
use crate::{
    api::{ApiError, BODY_LIMIT, authenticate_cli, decode_cli},
    auth::{ManagementError, ManagementPrincipal},
    config::get_backend_config,
    db,
    limits::{MANAGEMENT_RESPONSE_MAX_BYTES, OWNER_REQUEST_MAX_BYTES},
    observability::{self, Boundary, Delivery, Failure},
};
use axum::{
    body::{Body, to_bytes},
    extract::{Request as AxumRequest, State},
    response::Response as AxumResponse,
};
use brews_contracts::management::{ManagementCommand, ManagementResponse, ReceiptOperation};
use brews_domain::ids::CommandId;
use serde::{Deserialize, Serialize};
use worker::{Env, Headers, Method, Request, RequestInit, Response, Result, send::SendWrapper};
use zeroize::Zeroizing;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerManagementRequest {
    command: ManagementCommand,
    command_id: Option<CommandId>,
}

pub(super) async fn handle(
    State(env): State<SendWrapper<Env>>,
    request: AxumRequest,
) -> AxumResponse {
    worker::send::SendFuture::new(async move { dispatch(&env, request).await }).await
}

async fn dispatch(env: &Env, request: AxumRequest) -> AxumResponse {
    let cfg = match get_backend_config(env) {
        Ok(cfg) => cfg,
        Err(_) => {
            observability::failure(Boundary::CliIngress, Failure::Configuration);
            return edge::failure(ApiError::Unavailable);
        }
    };
    let key = cfg.dev_cli_key.as_ref().map(|key| key.expose_secret());
    if let Err(error) = authenticate_cli(request.headers(), key) {
        observability::api_failure(Boundary::CliIngress, error);
        return edge::failure(error);
    }
    if request.uri().scheme_str() != Some("https") {
        observability::failure(Boundary::CliIngress, Failure::Forbidden);
        return edge::failure(ApiError::Forbidden);
    }
    let (parts, body) = request.into_parts();
    let bytes = match to_bytes(body, BODY_LIMIT).await {
        Ok(bytes) => Zeroizing::new(bytes.to_vec()),
        Err(_) => {
            observability::failure(Boundary::CliIngress, Failure::PayloadTooLarge);
            return edge::failure(ApiError::PayloadTooLarge);
        }
    };
    let decoded = match decode_cli(
        &parts.method,
        parts.uri.query(),
        &parts.headers,
        &bytes,
        key,
    ) {
        Ok(decoded) => decoded,
        Err(error) => {
            // Only an unambiguous canonical target can route a rejected developer read.
            if let Ok(context) = super::decode_private_json::<GameAuditTransportContext>(&bytes) {
                let request = GameAuditTransportRejection {
                    game_id: context.game_id,
                };
                let result: Result<GameAuditTransportRejection> = super::game_peers::call(
                    env,
                    "GAMES",
                    &context.game_id.to_string(),
                    "/developer-audit-rejection",
                    &request,
                    super::game_peers::GAME_PEER_MAX_BYTES,
                )
                .await
                .map_err(|_| {
                    worker::Error::RustError("developer audit capture unavailable".into())
                });
                if !result.is_ok_and(|reply| reply.game_id == context.game_id) {
                    return edge::failure(ApiError::Unavailable);
                }
            }
            observability::api_failure(Boundary::CliIngress, error);
            return edge::failure(error);
        }
    };
    let message = OwnerManagementRequest {
        command: decoded.command,
        command_id: decoded.command_id,
    };
    match call_owner(env, &message).await {
        Ok(response) => response,
        Err(_) => {
            if let ManagementCommand::ListGameAudit { game_id, .. } = message.command {
                let request = GameAuditTransportRejection { game_id };
                let captured: std::result::Result<GameAuditTransportRejection, _> =
                    super::game_peers::call(
                        env,
                        "GAMES",
                        &game_id.to_string(),
                        "/developer-audit-failure",
                        &request,
                        super::game_peers::GAME_PEER_MAX_BYTES,
                    )
                    .await;
                if !captured.is_ok_and(|reply| reply.game_id == game_id) {
                    return edge::failure(ApiError::Unavailable);
                }
            }
            observability::failure(Boundary::AccountsPeer, Failure::Unavailable);
            edge::failure(ApiError::Unavailable)
        }
    }
}

async fn call_owner(env: &Env, message: &OwnerManagementRequest) -> Result<AxumResponse> {
    let game = match &message.command {
        ManagementCommand::ListGameAudit { game_id, .. } => Some(*game_id),
        _ => None,
    };
    let namespace = env.durable_object(if game.is_some() { "GAMES" } else { "ACCOUNTS" })?;
    let target = game.map_or_else(|| "accounts".into(), |id| id.to_string());
    let stub = namespace.id_from_name(&target)?.get_stub()?;
    let body = Zeroizing::new(
        serde_json::to_string(message)
            .map_err(|_| worker::Error::RustError("management serialization failed".into()))?,
    );
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(body.as_str().into()));
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    init.with_headers(headers);
    let request = Request::new_with_init(
        if game.is_some() {
            "https://game.internal/developer-audit"
        } else {
            "https://accounts.internal/management"
        },
        &init,
    )?;
    let response = stub.fetch_with_request(request).await?;
    let status = response.status_code();
    let http: worker::HttpResponse = response.try_into()?;
    let content = Zeroizing::new(
        to_bytes(Body::new(http.into_body()), MANAGEMENT_RESPONSE_MAX_BYTES)
            .await
            .map_err(|_| worker::Error::RustError("management response bound exceeded".into()))?
            .to_vec(),
    );
    if status == 200 || status == 202 {
        let typed = ManagementResponse::decode_json(&content)
            .map_err(|_| worker::Error::RustError("invalid management result".into()))?;
        if let ManagementCommand::ListGameAudit {
            game_id,
            after,
            limit,
        } = message.command
        {
            let ManagementResponse::GameAudit {
                game_id: actual,
                events,
                next_cursor,
            } = &typed
            else {
                return Err(worker::Error::RustError("invalid game audit result".into()));
            };
            if !brews_contracts::game_audit::validate_page(
                game_id,
                after,
                limit,
                *actual,
                events,
                *next_cursor,
            ) {
                return Err(worker::Error::RustError("invalid game audit page".into()));
            }
            let observed = crate::auth::Runtime::now_ms(&WorkerRuntime);
            if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&observed)
                || events.iter().any(|event| event.occurred_at > observed)
            {
                return Err(worker::Error::RustError("invalid game audit clock".into()));
            }
            if events.iter().any(|event| observed >= event.expires_at) {
                let denial = GameAuditExpiryFence {
                    game_id,
                    observed_at: observed,
                };
                let response: GameAuditExpiryFence = super::game_peers::call(
                    env,
                    "GAMES",
                    &game_id.to_string(),
                    "/developer-audit-expiry",
                    &denial,
                    super::game_peers::GAME_PEER_MAX_BYTES,
                )
                .await
                .map_err(|_| worker::Error::RustError("game audit expiry unavailable".into()))?;
                if response.game_id != game_id || response.observed_at != observed {
                    return Err(worker::Error::RustError("invalid game audit expiry".into()));
                }
                return Err(worker::Error::RustError("expired game audit result".into()));
            }
        }
        let pending = matches!(typed, ManagementResponse::Pending { .. });
        if pending != (status == 202) {
            return Err(worker::Error::RustError("invalid management status".into()));
        }
    } else if game.is_some() {
        let error = match status {
            400 => ManagementError::InvalidInput,
            403 => ManagementError::Forbidden,
            503 => ManagementError::Storage,
            _ => return Err(worker::Error::RustError("invalid game audit status".into())),
        };
        let result: GameAuditErrorReply = super::decode_private_json(&content)
            .map_err(|_| worker::Error::RustError("invalid game audit error".into()))?;
        if result.error.code != error.code() || result.error.message != error.to_string() {
            return Err(worker::Error::RustError("invalid game audit error".into()));
        }
    } else if !matches!(status, 400 | 401 | 403 | 404 | 409 | 503) {
        return Err(worker::Error::RustError("invalid management status".into()));
    }
    http::Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .body(Body::from(content.as_slice().to_vec()))
        .map_err(|_| worker::Error::RustError("management response failed".into()))
}

impl AccountsObject {
    pub(super) async fn execute_management_request(&self, request: Request) -> Result<Response> {
        match self.execute_management_inner(request).await {
            Ok(response) => Ok(response),
            Err(error) => {
                observability::management_failure(Boundary::AccountsManagement, error);
                management_error(error)
            }
        }
    }
    async fn execute_management_inner(
        &self,
        request: Request,
    ) -> std::result::Result<Response, ManagementError> {
        if request.method() != Method::Post
            || request
                .url()
                .map_err(|_| ManagementError::InvalidInput)?
                .query()
                .is_some()
        {
            return Err(ManagementError::InvalidInput);
        }
        let http: worker::HttpRequest = request
            .try_into()
            .map_err(|_| ManagementError::InvalidInput)?;
        let bytes = Zeroizing::new(
            to_bytes(Body::new(http.into_body()), OWNER_REQUEST_MAX_BYTES)
                .await
                .map_err(|_| ManagementError::InvalidInput)?
                .to_vec(),
        );
        let message: OwnerManagementRequest =
            super::decode_private_json(&bytes).map_err(|_| ManagementError::InvalidInput)?;
        let db = OwnerDatabase::new(self.state.storage());
        db::migrate(&db)?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let cfg = get_backend_config(&self.env).map_err(|_| ManagementError::Crypto)?;
        let outcome = service.execute_management(
            message.command.clone(),
            ManagementPrincipal::DeveloperCli,
            message.command_id,
            &cfg.application_origin,
        );
        let removal_operation_id = match &outcome {
            Ok(ManagementResponse::Pending { operation_id }) => Some(*operation_id),
            _ => None,
        };
        let outcome = match outcome {
            Ok(ManagementResponse::Pending { operation_id }) => {
                self.progress_removal(
                    &service,
                    operation_id,
                    &message.command,
                    message.command_id.ok_or(ManagementError::InvalidInput)?,
                    &ManagementPrincipal::DeveloperCli,
                )
                .await
            }
            result => result,
        };
        self.dispatch_game_socket_closes(&service)
            .await
            .map_err(|_| ManagementError::Storage)?;
        self.schedule(Self::combined_deadline(&service)?)
            .await
            .map_err(|_| ManagementError::Storage)?;
        self.state.storage().sync().await.map_err(|_| {
            observability::failure(Boundary::Durability, Failure::Storage);
            ManagementError::Storage
        })?;
        let outcome = outcome?;
        let response = management_result(&outcome).map_err(|_| ManagementError::Storage)?;
        if let Some(operation_id) = removal_operation_id
            && matches!(outcome, ManagementResponse::Committed { .. })
        {
            observability::removal(observability::RemovalEvent::Completed, operation_id);
        }
        match &outcome {
            ManagementResponse::Issued { receipt, .. } => observability::management_delivery(
                receipt.operation,
                receipt.command_id,
                Delivery::Issued,
            ),
            ManagementResponse::Committed { receipt } => observability::management_delivery(
                receipt.operation,
                receipt.command_id,
                Delivery::Committed,
            ),
            ManagementResponse::Pending { .. } => {
                if let (Some(id), Some(operation)) = (
                    message.command_id,
                    match message.command {
                        ManagementCommand::DisableAccount { .. } => {
                            Some(ReceiptOperation::DisableAccount)
                        }
                        ManagementCommand::DeleteAccount { .. } => {
                            Some(ReceiptOperation::DeleteAccount)
                        }
                        _ => None,
                    },
                ) {
                    observability::management_delivery(operation, id, Delivery::Pending);
                }
            }
            _ => {}
        }
        Ok(response)
    }
}
pub(super) fn management_result(outcome: &ManagementResponse) -> Result<Response> {
    let status = if matches!(outcome, ManagementResponse::Pending { .. }) {
        202
    } else {
        200
    };
    let mut response = Response::from_json(outcome)?.with_status(status);
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}
pub(super) fn management_error(error: ManagementError) -> Result<Response> {
    let mut response = Response::from_json(
        &serde_json::json!({"error":{"code":error.code(),"message":error.to_string()}}),
    )?
    .with_status(error.status());
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GameAuditExpiryFence {
    pub(super) game_id: brews_domain::ids::GameId,
    pub(super) observed_at: i64,
}
impl super::game_object::GameObject {
    pub(super) async fn execute_developer_audit(&self, bytes: &[u8]) -> Result<Response> {
        let message: OwnerManagementRequest = super::decode_private_json(bytes)
            .map_err(|_| worker::Error::RustError("invalid game audit request".into()))?;
        let ManagementCommand::ListGameAudit {
            game_id,
            after,
            limit,
        } = message.command
        else {
            return Response::error("invalid game audit command", 400);
        };
        if message.command_id.is_some()
            || !self
                .owns_game(game_id)
                .map_err(|_| worker::Error::RustError("game audit identity unavailable".into()))?
        {
            return Response::error("invalid game audit target", 400);
        }
        let result = async {
            let db = self.database()?;
            let service = self.service(&db, &WorkerRuntime)?;
            service.bind_operator_game_identity(game_id)?;
            let result = service.list_game_audit(game_id, after, limit);
            self.persist_developer_audit(&service, &db, game_id).await?;
            let result = result?;
            if let ManagementResponse::GameAudit { events, .. } = &result {
                let now = crate::auth::Runtime::now_ms(&WorkerRuntime);
                if events.iter().any(|event| now >= event.expires_at) {
                    service.fence_operator_output_expiry(now)?;
                    self.state
                        .storage()
                        .sync()
                        .await
                        .map_err(|_| crate::game::GameError::Storage)?;
                    return Err(crate::game::GameError::Storage);
                }
            }
            Ok::<_, crate::game::GameError>(result)
        }
        .await;
        match result {
            Ok(outcome) => management_result(&outcome),
            Err(error) => {
                let error = match error {
                    crate::game::GameError::InvalidInput => ManagementError::InvalidInput,
                    crate::game::GameError::Forbidden => ManagementError::Forbidden,
                    _ => ManagementError::Storage,
                };
                management_error(error)
            }
        }
    }
    async fn persist_developer_audit(
        &self,
        service: &crate::db::game::GameService<'_, OwnerDatabase, WorkerRuntime>,
        db: &OwnerDatabase,
        game: brews_domain::ids::GameId,
    ) -> std::result::Result<(), crate::game::GameError> {
        if let Err(error) = self.persist(service, db).await {
            service.capture_developer_audit_failure(game)?;
            self.persist(service, db).await?;
            return Err(error);
        }
        Ok(())
    }
    pub(super) async fn execute_developer_audit_expiry(&self, bytes: &[u8]) -> Result<Response> {
        let request: GameAuditExpiryFence = super::decode_private_json(bytes)
            .map_err(|_| worker::Error::RustError("invalid game audit fence".into()))?;
        if !self
            .owns_game(request.game_id)
            .map_err(|_| worker::Error::RustError("game audit identity unavailable".into()))?
        {
            return Response::error("invalid game audit target", 400);
        }
        let db = self
            .database()
            .map_err(|_| worker::Error::RustError("game audit unavailable".into()))?;
        let service = self
            .service(&db, &WorkerRuntime)
            .map_err(|_| worker::Error::RustError("game audit unavailable".into()))?;
        service
            .fence_operator_output_expiry(request.observed_at)
            .map_err(|_| worker::Error::RustError("game audit fence unavailable".into()))?;
        self.state.storage().sync().await?;
        Response::from_json(&request)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GameAuditErrorReply {
    error: GameAuditErrorFields,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GameAuditErrorFields {
    code: String,
    message: String,
}

// Purpose and target extraction rejects duplicate fields; submitted contents are discarded.
#[derive(Deserialize)]
struct GameAuditTransportContext {
    #[serde(rename = "operation")]
    _operation: GameAuditTransportPurpose,
    game_id: brews_domain::ids::GameId,
}
#[derive(Deserialize)]
enum GameAuditTransportPurpose {
    #[serde(rename = "list_game_audit")]
    ListGameAudit,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameAuditTransportRejection {
    game_id: brews_domain::ids::GameId,
}
impl super::game_object::GameObject {
    pub(super) async fn execute_developer_audit_transport(
        &self,
        bytes: &[u8],
        outcome: super::operator_runtime::TransportAuditOutcome,
    ) -> Result<Response> {
        let request: GameAuditTransportRejection = super::decode_private_json(bytes)
            .map_err(|_| worker::Error::RustError("invalid developer audit rejection".into()))?;
        if !self
            .owns_game(request.game_id)
            .map_err(|_| worker::Error::RustError("developer audit identity unavailable".into()))?
        {
            return Response::error("invalid developer audit target", 400);
        }
        let db = self
            .database()
            .map_err(|_| worker::Error::RustError("developer audit storage unavailable".into()))?;
        let service = self
            .service(&db, &WorkerRuntime)
            .map_err(|_| worker::Error::RustError("developer audit service unavailable".into()))?;
        service
            .bind_operator_game_identity(request.game_id)
            .map_err(|_| worker::Error::RustError("developer audit identity unavailable".into()))?;
        let captured = match outcome {
            super::operator_runtime::TransportAuditOutcome::Rejected => {
                service.capture_developer_audit_rejection(request.game_id)
            }
            super::operator_runtime::TransportAuditOutcome::Failed => {
                service.capture_developer_audit_failure(request.game_id)
            }
        };
        captured
            .map_err(|_| worker::Error::RustError("developer audit capture unavailable".into()))?;
        self.persist_developer_audit(&service, &db, request.game_id)
            .await
            .map_err(|_| {
                worker::Error::RustError("developer audit durability unavailable".into())
            })?;
        Response::from_json(&request)
    }
}
