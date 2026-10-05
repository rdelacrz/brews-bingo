//! Restricted CLI adapter; principals are derived here, never deserialized from clients.
use super::{AccountsObject, OwnerDatabase, WorkerRuntime, edge};
use crate::{
    api::{ApiError, BODY_LIMIT, authenticate_cli, decode_cli},
    auth::{ManagementError, ManagementPrincipal},
    config::get_backend_config,
    limits::{MANAGEMENT_RESPONSE_MAX_BYTES, OWNER_REQUEST_MAX_BYTES},
    observability::{self, Boundary, Delivery, Failure},
    storage,
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
            observability::api_failure(Boundary::CliIngress, error);
            return edge::failure(error);
        }
    };
    let message = OwnerManagementRequest {
        command: decoded.command,
        command_id: decoded.command_id,
    };
    match call_accounts(env, &message).await {
        Ok(response) => response,
        Err(_) => {
            observability::failure(Boundary::AccountsPeer, Failure::Unavailable);
            edge::failure(ApiError::Unavailable)
        }
    }
}

async fn call_accounts(env: &Env, message: &OwnerManagementRequest) -> Result<AxumResponse> {
    let namespace = env.durable_object("ACCOUNTS")?;
    let stub = namespace.id_from_name("accounts")?.get_stub()?;
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
    let request = Request::new_with_init("https://accounts.internal/management", &init)?;
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
        let pending = matches!(typed, ManagementResponse::Pending { .. });
        if pending != (status == 202) {
            return Err(worker::Error::RustError("invalid management status".into()));
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
        storage::migrate(&db)?;
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
                )
                .await
            }
            result => result,
        };
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
