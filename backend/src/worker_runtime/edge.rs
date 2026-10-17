//! Axum HTTP boundary. Only the private AccountsObject creates authority.
use super::wire::{CookieWire, OwnerRequest, OwnerResponse};
use crate::config::get_backend_config;
use crate::limits::{
    MILLISECONDS_PER_SECOND, OWNER_RESPONSE_MAX_BYTES, RATE_LIMIT_BLOCK_MS, SESSION_LIFETIME_MS,
};
use crate::observability::{self, Boundary, Failure};
use crate::{
    api::{
        self, ApiError, AuthenticationOperation, AuthenticationPayload, AuthenticationRequest,
        BODY_LIMIT,
    },
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
    let decoded = api::decode_authentication(
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
    let current_token = decoded
        .session_token
        .as_deref()
        .map(|value| Zeroizing::new(value.to_owned()));
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
    let response = stub.fetch_with_request(request).await.map_err(|_| {
        observability::failure(Boundary::AccountsPeer, Failure::PeerTransport);
        ApiError::Unavailable
    })?;
    if response.status_code() != 200 {
        observability::failure(Boundary::AccountsPeer, Failure::PeerProtocol);
        return Err(ApiError::Unavailable);
    }
    let http: worker::HttpResponse = response.try_into().map_err(|_| ApiError::Unavailable)?;
    let data = Zeroizing::new(
        to_bytes(Body::new(http.into_body()), OWNER_RESPONSE_MAX_BYTES)
            .await
            .map_err(|_| ApiError::Unavailable)?
            .to_vec(),
    );
    let result: OwnerResponse = super::decode_private_json(&data).map_err(|_| {
        observability::failure(Boundary::AccountsPeer, Failure::PeerProtocol);
        ApiError::Unavailable
    })?;
    let token = match &result.cookie {
        CookieWire::Set { token, .. } => Some(token.as_str()),
        _ => current_token.as_deref().map(|value| value.as_str()),
    };
    let protected = result
        .body
        .get("session")
        .is_some_and(|value| !value.is_null())
        || matches!(result.cookie, CookieWire::Set { .. });
    let mut release_sample = None;
    if protected {
        let authority = result.authority.as_ref().ok_or(ApiError::Unavailable)?;
        let token = token.ok_or(ApiError::Unavailable)?;
        if let CookieWire::Set { expires_at, .. } = &result.cookie
            && *expires_at != authority.expires_at
        {
            return Err(ApiError::Unavailable);
        }
        let observed_at = js_sys::Date::now() as i64;
        if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&observed_at) {
            return Err(ApiError::Unavailable);
        }
        if observed_at >= authority.expires_at {
            super::auth_output::fence(env, token, authority, observed_at).await?;
            return Err(ApiError::Unavailable);
        }
        super::auth_output::check(env, token, authority, &result.body).await?;
        let observed_at = js_sys::Date::now() as i64;
        if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&observed_at) {
            return Err(ApiError::Unavailable);
        }
        if observed_at >= authority.expires_at {
            super::auth_output::fence(env, token, authority, observed_at).await?;
            return Err(ApiError::Unavailable);
        }
        release_sample = Some(observed_at);
    } else if result.authority.is_some() {
        return Err(ApiError::Unavailable);
    }
    let mut response = super::response::json_bytes(
        result.status,
        serde_json::to_vec(&result.body).map_err(|_| ApiError::Unavailable)?,
    )?;
    match &result.cookie {
        CookieWire::None => {}
        CookieWire::Clear => {
            response.headers_mut().insert("set-cookie",HeaderValue::from_static("__Host-brews_session=; Secure; HttpOnly; Path=/; SameSite=Lax; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"));
        }
        CookieWire::Set { token, expires_at } => {
            token_digest(token).map_err(|_| ApiError::Unavailable)?;
            let now = release_sample.ok_or(ApiError::Unavailable)?;
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
fn command(decoded: AuthenticationRequest) -> Result<AuthCommand, ApiError> {
    let AuthenticationRequest {
        operation,
        payload,
        session_token,
        ..
    } = decoded;
    Ok(match (operation, payload) {
        (AuthenticationOperation::Login, AuthenticationPayload::Login { username, password }) => {
            AuthCommand::Login { username, password }
        }
        (AuthenticationOperation::Current, AuthenticationPayload::Empty) => AuthCommand::Current {
            token: session_token,
        },
        (AuthenticationOperation::Logout, AuthenticationPayload::Empty) => AuthCommand::Logout {
            token: session_token,
        },
        (AuthenticationOperation::RedeemEnrollment, AuthenticationPayload::Redeem { token }) => {
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::Enrollment,
                token,
            }
        }
        (AuthenticationOperation::RedeemReset, AuthenticationPayload::Redeem { token }) => {
            AuthCommand::Redeem {
                purpose: AccessLinkPurpose::PasswordReset,
                token,
            }
        }
        (
            AuthenticationOperation::CompleteEnrollment,
            AuthenticationPayload::Complete { new_password },
        ) => AuthCommand::Complete {
            scope: SessionScope::EnrollmentOnly,
            token: session_token.ok_or(ApiError::InvalidInput)?,
            new_password,
        },
        (
            AuthenticationOperation::CompleteReset,
            AuthenticationPayload::Complete { new_password },
        ) => AuthCommand::Complete {
            scope: SessionScope::PasswordResetOnly,
            token: session_token.ok_or(ApiError::InvalidInput)?,
            new_password,
        },
        _ => return Err(ApiError::InvalidInput),
    })
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
    serde_json::to_vec(&serde_json::json!({"error":{"code":code,"message":error.to_string()}}))
        .map_err(|_| ApiError::Unavailable)
        .and_then(|bytes| super::response::json_bytes(status, bytes))
        .unwrap_or_else(|_| {
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            response
        })
}
