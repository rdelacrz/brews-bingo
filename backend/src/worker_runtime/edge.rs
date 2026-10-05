//! Axum HTTP boundary. Only the private AccountsObject creates authority.
use super::wire::{CookieWire, OwnerRequest, OwnerResponse};
use crate::config::get_backend_config;
use crate::limits::{
    MILLISECONDS_PER_SECOND, OWNER_RESPONSE_MAX_BYTES, RATE_LIMIT_BLOCK_MS, SESSION_LIFETIME_MS,
};
use crate::observability::{self, Boundary, Failure};
use crate::{
    api::{self, ApiError, BODY_LIMIT, Decoded, Operation, Payload},
    auth::AuthCommand,
    security::token_digest,
};
use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    response::Response,
};
use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
use http::{HeaderValue, StatusCode};
use std::net::IpAddr;
use wasm_bindgen::JsValue;
use worker::{
    Env, RequestInit,
    send::{SendFuture, SendWrapper},
};
use zeroize::Zeroizing;

pub(super) async fn handle(State(env): State<SendWrapper<Env>>, request: Request) -> Response {
    SendFuture::new(async move {
        match dispatch(&env, request).await {
            Ok(response) => response,
            Err(error) => {
                observability::api_failure(Boundary::AuthIngress, error);
                failure(error)
            }
        }
    })
    .await
}

async fn dispatch(env: &Env, request: Request) -> Result<Response, ApiError> {
    let config = get_backend_config(env).map_err(|_| {
        observability::failure(Boundary::AuthIngress, Failure::Configuration);
        ApiError::Unavailable
    })?;
    if request.uri().scheme_str() != Some("https") {
        return Err(ApiError::Forbidden);
    }
    let (parts, body) = request.into_parts();
    let body = Zeroizing::new(
        to_bytes(body, BODY_LIMIT)
            .await
            .map_err(|_| ApiError::PayloadTooLarge)?
            .to_vec(),
    );
    let decoded = api::decode(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        &body,
        &config.application_origin,
    )?;
    // Trust only Cloudflare ingress metadata; missing IP shares one bucket.
    let caller_identity = parts
        .headers
        .get("cf-connecting-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<IpAddr>().ok())
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unavailable".to_owned());
    let command_id = decoded.command_id.clone();
    let command = command(decoded)?;
    let payload = Zeroizing::new(
        serde_json::to_string(&OwnerRequest {
            command,
            command_id,
            caller_identity,
        })
        .map_err(|_| ApiError::Unavailable)?,
    );
    let namespace = env
        .durable_object("ACCOUNTS")
        .map_err(|_| ApiError::Unavailable)?;
    let stub = namespace
        .id_from_name("accounts")
        .and_then(|id| id.get_stub())
        .map_err(|_| ApiError::Unavailable)?;
    let mut init = RequestInit::new();
    init.with_method(worker::Method::Post)
        .with_body(Some(JsValue::from_str(&payload)));
    init.headers
        .set("Content-Type", "application/json")
        .map_err(|_| ApiError::Unavailable)?;
    let request = worker::Request::new_with_init("https://accounts.internal/auth", &init)
        .map_err(|_| ApiError::Unavailable)?;
    let mut response = stub.fetch_with_request(request).await.map_err(|_| {
        observability::failure(Boundary::AccountsPeer, Failure::PeerTransport);
        ApiError::Unavailable
    })?;
    if response.status_code() != 200 {
        observability::failure(Boundary::AccountsPeer, Failure::PeerProtocol);
        return Err(ApiError::Unavailable);
    }
    let data = Zeroizing::new(response.bytes().await.map_err(|_| ApiError::Unavailable)?);
    if data.len() > OWNER_RESPONSE_MAX_BYTES {
        return Err(ApiError::Unavailable);
    }
    let result: OwnerResponse = serde_json::from_slice(&data).map_err(|_| {
        observability::failure(Boundary::AccountsPeer, Failure::PeerProtocol);
        ApiError::Unavailable
    })?;
    let mut response = json_response(result.status, result.body)?;
    match &result.cookie {
        CookieWire::None => {}
        CookieWire::Clear => {
            response.headers_mut().insert("set-cookie",HeaderValue::from_static("__Host-brews_session=; Secure; HttpOnly; Path=/; SameSite=Lax; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"));
        }
        CookieWire::Set { token, expires_at } => {
            token_digest(token).map_err(|_| ApiError::Unavailable)?;
            let now = js_sys::Date::now() as i64;
            let remaining = expires_at
                .checked_sub(now)
                .filter(|n| *n > 0 && *n <= SESSION_LIFETIME_MS)
                .ok_or(ApiError::Unavailable)?;
            let date = js_sys::Date::new(&JsValue::from_f64(*expires_at as f64))
                .to_utc_string()
                .as_string()
                .ok_or(ApiError::Unavailable)?;
            let cookie = Zeroizing::new(format!(
                "__Host-brews_session={token}; Secure; HttpOnly; Path=/; SameSite=Lax; Max-Age={}; Expires={date}",
                remaining / MILLISECONDS_PER_SECOND
            ));
            response.headers_mut().insert(
                "set-cookie",
                HeaderValue::from_str(&cookie).map_err(|_| ApiError::Unavailable)?,
            );
        }
    }
    if result.status == 429 {
        response.headers_mut().insert(
            "retry-after",
            HeaderValue::from_str(&(RATE_LIMIT_BLOCK_MS / MILLISECONDS_PER_SECOND).to_string())
                .map_err(|_| ApiError::Unavailable)?,
        );
    }
    Ok(response)
}
fn command(decoded: Decoded) -> Result<AuthCommand, ApiError> {
    let Decoded {
        operation,
        payload,
        session_token,
        ..
    } = decoded;
    Ok(match (operation, payload) {
        (Operation::Login, Payload::Login { username, password }) => {
            AuthCommand::Login { username, password }
        }
        (Operation::Current, Payload::Empty) => AuthCommand::Current {
            token: session_token,
        },
        (Operation::Logout, Payload::Empty) => AuthCommand::Logout {
            token: session_token,
        },
        (Operation::RedeemEnrollment, Payload::Redeem { token }) => AuthCommand::Redeem {
            purpose: AccessLinkPurpose::Enrollment,
            token,
        },
        (Operation::RedeemReset, Payload::Redeem { token }) => AuthCommand::Redeem {
            purpose: AccessLinkPurpose::PasswordReset,
            token,
        },
        (Operation::CompleteEnrollment, Payload::Complete { new_password }) => {
            AuthCommand::Complete {
                scope: SessionScope::EnrollmentOnly,
                token: session_token.ok_or(ApiError::InvalidInput)?,
                new_password,
            }
        }
        (Operation::CompleteReset, Payload::Complete { new_password }) => AuthCommand::Complete {
            scope: SessionScope::PasswordResetOnly,
            token: session_token.ok_or(ApiError::InvalidInput)?,
            new_password,
        },
        _ => return Err(ApiError::InvalidInput),
    })
}
fn json_response(status: u16, body: serde_json::Value) -> Result<Response, ApiError> {
    let mut response = Response::new(Body::from(
        serde_json::to_vec(&body).map_err(|_| ApiError::Unavailable)?,
    ));
    *response.status_mut() = StatusCode::from_u16(status).map_err(|_| ApiError::Unavailable)?;
    response.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
pub(super) fn failure(error: ApiError) -> Response {
    let status = match error {
        ApiError::InvalidInput => 400,
        ApiError::Forbidden => 403,
        ApiError::NotFound => 404,
        ApiError::MethodNotAllowed => 405,
        ApiError::PayloadTooLarge => 413,
        ApiError::UnsupportedMediaType => 415,
        ApiError::Unavailable => 503,
    };
    let code = match error {
        ApiError::InvalidInput => "invalid_input",
        ApiError::Forbidden => "forbidden",
        ApiError::NotFound => "not_found",
        ApiError::MethodNotAllowed => "method_not_allowed",
        ApiError::PayloadTooLarge => "payload_too_large",
        ApiError::UnsupportedMediaType => "unsupported_media_type",
        ApiError::Unavailable => "service_unavailable",
    };
    // Serialization failure must still return a safe response.
    json_response(
        status,
        serde_json::json!({"error":{"code":code,"message":error.to_string()}}),
    )
    .unwrap_or_else(|_| {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
        response
    })
}
