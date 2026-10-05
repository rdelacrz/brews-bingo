//! Browser admin Users transport; the Accounts owner derives live session authority.
use super::{AccountsObject, OwnerDatabase, WorkerRuntime, edge};
use crate::{
    api::{self, ApiError, BODY_LIMIT},
    auth::{ManagementError, ManagementPrincipal},
    config::get_backend_config,
    db,
    limits::{MANAGEMENT_RESPONSE_MAX_BYTES, OWNER_REQUEST_MAX_BYTES},
    observability::{self, Boundary, Delivery, Failure, RemovalEvent},
};
use axum::{
    body::{Body, to_bytes},
    extract::{Request as AxumRequest, State},
    response::Response as AxumResponse,
};
use brews_contracts::{management::ManagementCommand, users::UserResponse};
use brews_domain::ids::CommandId;
use serde::{Deserialize, Serialize};
use worker::{Env, Headers, Method, Request, RequestInit, Response, send::SendWrapper};
use zeroize::{Zeroize, Zeroizing};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerUsersRequest {
    command: ManagementCommand,
    command_id: Option<CommandId>,
    session_token: String,
}
impl Drop for OwnerUsersRequest {
    fn drop(&mut self) {
        self.session_token.zeroize();
    }
}

pub(super) async fn handle(
    State(env): State<SendWrapper<Env>>,
    request: AxumRequest,
) -> AxumResponse {
    worker::send::SendFuture::new(async move {
        match dispatch(&env, request).await {
            Ok(response) => response,
            Err(error) => {
                observability::api_failure(Boundary::UsersIngress, error);
                edge::failure(error)
            }
        }
    })
    .await
}

async fn dispatch(env: &Env, request: AxumRequest) -> Result<AxumResponse, ApiError> {
    let cfg = get_backend_config(env).map_err(|_| ApiError::Unavailable)?;
    if request.uri().scheme_str() != Some("https") {
        return Err(ApiError::Forbidden);
    }
    let (parts, body) = request.into_parts();
    let bytes = Zeroizing::new(
        to_bytes(body, BODY_LIMIT)
            .await
            .map_err(|_| ApiError::PayloadTooLarge)?
            .to_vec(),
    );
    let mut decoded = api::decode_users(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        &bytes,
        &cfg.application_origin,
    )?;
    let message = OwnerUsersRequest {
        command: decoded.command.clone(),
        command_id: decoded.command_id,
        session_token: decoded.session_token.take().unwrap_or_default(),
    };
    let body = Zeroizing::new(serde_json::to_string(&message).map_err(|_| ApiError::Unavailable)?);
    let namespace = env
        .durable_object("ACCOUNTS")
        .map_err(|_| ApiError::Unavailable)?;
    let stub = namespace
        .id_from_name("accounts")
        .and_then(|id| id.get_stub())
        .map_err(|_| ApiError::Unavailable)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(body.as_str().into()));
    let headers = Headers::new();
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| ApiError::Unavailable)?;
    init.with_headers(headers);
    let request = Request::new_with_init("https://accounts.internal/users", &init)
        .map_err(|_| ApiError::Unavailable)?;
    let response = stub.fetch_with_request(request).await.map_err(|_| {
        observability::failure(Boundary::AccountsPeer, Failure::PeerTransport);
        ApiError::Unavailable
    })?;
    let status = response.status_code();
    let http: worker::HttpResponse = response.try_into().map_err(|_| ApiError::Unavailable)?;
    let content = Zeroizing::new(
        to_bytes(Body::new(http.into_body()), MANAGEMENT_RESPONSE_MAX_BYTES)
            .await
            .map_err(|_| ApiError::Unavailable)?
            .to_vec(),
    );
    if matches!(status, 200 | 202) {
        let typed = UserResponse::decode_json(&content).map_err(|_| ApiError::Unavailable)?;
        if typed.status() != status {
            return Err(ApiError::Unavailable);
        }
    } else {
        validate_error(&content, status)?;
    }
    http::Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .header("Referrer-Policy", "no-referrer")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(content.as_slice().to_vec()))
        .map_err(|_| ApiError::Unavailable)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorEnvelope {
    #[serde(deserialize_with = "deserialize_error_fields")]
    error: ErrorFields,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorFields {
    code: String,
    message: String,
}
fn deserialize_error_fields<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ErrorFields, D::Error> {
    struct ObjectVisitor;
    impl<'de> serde::de::Visitor<'de> for ObjectVisitor {
        type Value = ErrorFields;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a JSON object")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<ErrorFields, A::Error> {
            ErrorFields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor)
}

fn validate_error(bytes: &[u8], status: u16) -> Result<(), ApiError> {
    let envelope: ErrorEnvelope =
        super::decode_private_json(bytes).map_err(|_| ApiError::Unavailable)?;
    let error = match envelope.error.code.as_str() {
        "invalid_input" => ManagementError::InvalidInput,
        "unauthorized" => ManagementError::Unauthorized,
        "forbidden" => ManagementError::Forbidden,
        "not_found" => ManagementError::NotFound,
        "conflict" => ManagementError::Conflict,
        "stale_command" => ManagementError::StaleCommand,
        "last_admin" => ManagementError::LastAdmin,
        "hosted_game" => ManagementError::HostedGame,
        "busy" => ManagementError::Busy,
        "unavailable" => ManagementError::Storage,
        _ => return Err(ApiError::Unavailable),
    };
    if error.status() != status || envelope.error.message != error.to_string() {
        return Err(ApiError::Unavailable);
    }
    Ok(())
}

impl AccountsObject {
    pub(super) async fn execute_users_request(&self, request: Request) -> worker::Result<Response> {
        match self.execute_users_inner(request).await {
            Ok(response) => Ok(response),
            Err(error) => {
                observability::management_failure(Boundary::AccountsManagement, error);
                let mut response = super::management::management_error(error)?;
                safe_response_headers(&mut response)?;
                Ok(response)
            }
        }
    }
    async fn execute_users_inner(&self, request: Request) -> Result<Response, ManagementError> {
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
        let mut message: OwnerUsersRequest =
            super::decode_private_json(&bytes).map_err(|_| ManagementError::InvalidInput)?;
        let principal =
            ManagementPrincipal::AdminSession(std::mem::take(&mut message.session_token));
        let db = OwnerDatabase::new(self.state.storage());
        db::migrate(&db)?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let cfg = get_backend_config(&self.env).map_err(|_| ManagementError::Crypto)?;
        let outcome = service.execute_users(
            message.command.clone(),
            &principal,
            message.command_id,
            &cfg.application_origin,
        );
        let operation_id = match &outcome {
            Ok(UserResponse::Pending { operation_id }) => Some(*operation_id),
            _ => None,
        };
        let outcome = match outcome {
            Ok(UserResponse::Pending { operation_id }) => {
                let result = self
                    .progress_removal(
                        &service,
                        operation_id,
                        &message.command,
                        message.command_id.ok_or(ManagementError::InvalidInput)?,
                        &principal,
                    )
                    .await;
                match result {
                    Ok(result) => {
                        service.users_removal_response(&message.command, &principal, result)
                    }
                    Err(error) => Err(error),
                }
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
        service.authorize_users_release(&principal)?;
        let mut response = Response::from_json(&outcome)
            .map_err(|_| ManagementError::Storage)?
            .with_status(outcome.status());
        safe_response_headers(&mut response).map_err(|_| ManagementError::Storage)?;
        if let Some(operation_id) = operation_id
            && matches!(
                outcome,
                UserResponse::Disabled { .. } | UserResponse::Deleted { .. }
            )
        {
            observability::removal(RemovalEvent::Completed, operation_id);
        }
        log_delivery(&outcome, &message);
        Ok(response)
    }
}
fn safe_response_headers(response: &mut Response) -> worker::Result<()> {
    for (name, value) in [
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Content-Type-Options", "nosniff"),
    ] {
        response.headers_mut().set(name, value)?;
    }
    Ok(())
}
fn log_delivery(response: &UserResponse, request: &OwnerUsersRequest) {
    let (receipt, delivery) = match response {
        UserResponse::Created { receipt, .. }
        | UserResponse::EnrollmentLink { receipt, .. }
        | UserResponse::PasswordReset { receipt, .. } => (Some(receipt), Delivery::Issued),
        UserResponse::Disabled { receipt, .. }
        | UserResponse::Deleted { receipt, .. }
        | UserResponse::Enabled { receipt, .. }
        | UserResponse::Committed { receipt } => (Some(receipt), Delivery::Committed),
        _ => (None, Delivery::Pending),
    };
    if let Some(receipt) = receipt {
        observability::management_delivery(receipt.operation, receipt.command_id, delivery);
    } else if matches!(response, UserResponse::Pending { .. })
        && let Some(command_id) = request.command_id
    {
        use brews_contracts::management::ReceiptOperation;
        let operation = match &request.command {
            ManagementCommand::DisableAccount { .. } => ReceiptOperation::DisableAccount,
            ManagementCommand::DeleteAccount { .. } => ReceiptOperation::DeleteAccount,
            _ => return,
        };
        observability::management_delivery(operation, command_id, Delivery::Pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_payload_rejects_positional_fields() {
        let error = ManagementError::Unauthorized;
        let bytes = format!(r#"{{"error":["{}","{error}"]}}"#, error.code());
        assert!(matches!(
            validate_error(bytes.as_bytes(), error.status()),
            Err(ApiError::Unavailable)
        ));
        assert!(serde_json::from_slice::<ErrorEnvelope>(bytes.as_bytes()).is_err());
    }

    #[test]
    fn error_payload_accepts_only_matching_management_errors() {
        for error in [
            ManagementError::InvalidInput,
            ManagementError::Unauthorized,
            ManagementError::Forbidden,
            ManagementError::NotFound,
            ManagementError::Conflict,
            ManagementError::StaleCommand,
            ManagementError::LastAdmin,
            ManagementError::HostedGame,
            ManagementError::Busy,
            ManagementError::Storage,
        ] {
            let bytes = format!(
                r#"{{"error":{{"code":"{}","message":"{error}"}}}}"#,
                error.code()
            );
            assert_eq!(validate_error(bytes.as_bytes(), error.status()), Ok(()));
            assert_eq!(
                validate_error(bytes.as_bytes(), 200),
                Err(ApiError::Unavailable)
            );
        }
        assert_eq!(
            validate_error(
                br#"{"error":{"code":"unauthorized","message":"wrong message"}}"#,
                401
            ),
            Err(ApiError::Unavailable)
        );
    }

    #[test]
    fn error_payload_preserves_duplicate_and_unknown_field_rejection() {
        for bytes in [
            br#"{"error":{"code":"unauthorized","code":"unauthorized","message":"Authentication required."}}"#.as_slice(),
            br#"{"error":{"code":"unauthorized","co\u0064e":"unauthorized","message":"Authentication required."}}"#,
            br#"{"error":{"code":"unauthorized","message":"Authentication required.","message":"Authentication required."}}"#,
            br#"{"error":{"code":"unauthorized","message":"Authentication required.","mess\u0061ge":"Authentication required."}}"#,
            br#"{"error":{"code":"unauthorized","message":"Authentication required.","unknown":null}}"#,
            br#"{"error":{"code":"unauthorized","message":"Authentication required."},"unknown":null}"#,
            br#"{"error":{"code":"unauthorized","message":"Authentication required."},"err\u006fr":{"code":"unauthorized","message":"Authentication required."}}"#,
        ] {
            assert_eq!(validate_error(bytes, 401), Err(ApiError::Unavailable));
            assert!(serde_json::from_slice::<ErrorEnvelope>(bytes).is_err());
        }
    }

    #[test]
    fn error_payload_rejects_other_nonobject_shapes() {
        for shape in ["null", "true", "42", r#""object""#, "[]", "{}"] {
            let bytes = format!(r#"{{"error":{shape}}}"#);
            assert_eq!(
                validate_error(bytes.as_bytes(), 401),
                Err(ApiError::Unavailable)
            );
        }
    }
}
