//! Public game ingress; all authority and game commits stay in private owners.
use super::{
    edge, game_peers,
    game_wire::{GameAction, GameCookieWire, GameIngress, GameOwnerResponse, GameRejection},
};
use crate::{
    api::{self, ApiError, GamePayload},
    config::get_backend_config,
    game::creation_fingerprint,
};
use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    response::Response,
};
use brews_domain::ids::GameId;
use http::{HeaderValue, StatusCode};
use worker::{
    Env, Headers, Method, RequestInit,
    send::{SendFuture, SendWrapper},
};
use zeroize::Zeroizing;

pub(super) async fn handle(State(env): State<SendWrapper<Env>>, request: Request) -> Response {
    SendFuture::new(async move {
        match dispatch(&env, request).await {
            Ok(response) => response,
            Err(error) => edge::failure(error),
        }
    })
    .await
}
fn peer_error(error: game_peers::GamePeerError) -> GameRejection {
    match error {
        game_peers::GamePeerError::Unauthorized => GameRejection::Unauthorized,
        game_peers::GamePeerError::Conflict => GameRejection::Conflict,
        game_peers::GamePeerError::InvalidInput => GameRejection::InvalidInput,
        game_peers::GamePeerError::Unavailable => GameRejection::Unavailable,
    }
}
async fn dispatch(env: &Env, request: Request) -> Result<Response, ApiError> {
    let cfg = get_backend_config(env).map_err(|_| ApiError::Unavailable)?;
    if request.uri().scheme_str() != Some("https") {
        return Err(ApiError::Forbidden);
    }
    let (parts, body) = request.into_parts();
    let bytes = Zeroizing::new(
        to_bytes(body, api::BODY_LIMIT)
            .await
            .map_err(|_| ApiError::PayloadTooLarge)?
            .to_vec(),
    );
    let mut decoded = api::decode_games(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        &bytes,
        &cfg.application_origin,
    )?;
    let mut ingress =
        GameIngress::from_decoded(&mut decoded, &bytes).map_err(|_| ApiError::InvalidInput)?;
    if ingress.operation == GameAction::Create {
        let GamePayload::Create(input) = ingress.payload().map_err(|_| ApiError::InvalidInput)?
        else {
            return Err(ApiError::InvalidInput);
        };
        let Some(token) = ingress.session_token.as_deref() else {
            return rejected(GameRejection::Unauthorized);
        };
        let proof = match game_peers::authorize_account(env, token).await {
            Ok(proof) => proof,
            Err(error) => return rejected(peer_error(error)),
        };
        let fingerprint =
            creation_fingerprint(&input.configuration).map_err(|_| ApiError::InvalidInput)?;
        let work = match game_peers::claim_game(
            env,
            &proof,
            ingress.command_id.ok_or(ApiError::InvalidInput)?,
            fingerprint,
        )
        .await
        {
            Ok(work) => work,
            Err(error) => return rejected(peer_error(error)),
        };
        ingress.game_id = Some(work.game_id());
    }
    let game_id = ingress.game_id.ok_or(ApiError::InvalidInput)?;
    let payload = game_peers::encode(&ingress).map_err(|_| ApiError::Unavailable)?;
    let namespace = env
        .durable_object("GAMES")
        .map_err(|_| ApiError::Unavailable)?;
    let stub = namespace
        .id_from_name(&game_id.to_string())
        .and_then(|id| id.get_stub())
        .map_err(|_| ApiError::Unavailable)?;
    let headers = Headers::new();
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| ApiError::Unavailable)?;
    if ingress.operation == GameAction::Stream {
        headers
            .set("Upgrade", "websocket")
            .map_err(|_| ApiError::Unavailable)?;
        headers
            .set("Connection", "Upgrade")
            .map_err(|_| ApiError::Unavailable)?;
    }
    let mut init = RequestInit::new();
    let private_body = std::str::from_utf8(&payload).map_err(|_| ApiError::Unavailable)?;
    if ingress.operation == GameAction::Stream {
        headers
            .set(super::game_wire::STREAM_INGRESS_HEADER, private_body)
            .map_err(|_| ApiError::Unavailable)?;
        init.with_method(Method::Get).with_headers(headers);
    } else {
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(private_body.into()));
    }
    let owner_request = worker::Request::new_with_init("https://game.internal/command", &init)
        .map_err(|_| ApiError::Unavailable)?;
    let response = stub
        .fetch_with_request(owner_request)
        .await
        .map_err(|_| ApiError::Unavailable)?;
    if response.status_code() == 101 {
        if ingress.operation != GameAction::Stream {
            return Err(ApiError::Unavailable);
        }
        let http: worker::HttpResponse = response.try_into().map_err(|_| ApiError::Unavailable)?;
        if http.extensions().get::<worker::WebSocket>().is_none() {
            return Err(ApiError::Unavailable);
        }
        return Ok(http.map(Body::new));
    }
    if response.status_code() != 200 {
        return Err(ApiError::Unavailable);
    }
    let http: worker::HttpResponse = response.try_into().map_err(|_| ApiError::Unavailable)?;
    let bytes = Zeroizing::new(
        to_bytes(
            Body::new(http.into_body()),
            brews_contracts::games::GAME_FRAME_MAX_BYTES + super::game_wire::GAME_WIRE_MAX_BYTES,
        )
        .await
        .map_err(|_| ApiError::Unavailable)?
        .to_vec(),
    );
    let result = GameOwnerResponse::decode_json(&bytes).map_err(|_| ApiError::Unavailable)?;
    result
        .validate_for(&ingress)
        .map_err(|_| ApiError::Unavailable)?;
    match &result {
        GameOwnerResponse::Rejected { code } => rejected(*code),
        GameOwnerResponse::Synced { response } => json_response(
            200,
            response.encode_json().map_err(|_| ApiError::Unavailable)?,
        ),
        GameOwnerResponse::Success { response, cookie } => {
            let bytes = response.encode_json().map_err(|_| ApiError::Unavailable)?;
            let mut output = json_response(response.status(), bytes)?;
            match cookie {
                GameCookieWire::None => {}
                GameCookieWire::Admission { token, expires_at } => set_cookie(
                    &mut output,
                    api::ADMISSION_COOKIE_NAME,
                    game_id,
                    token,
                    *expires_at,
                )?,
                GameCookieWire::Player { token, expires_at } => set_cookie(
                    &mut output,
                    api::PLAYER_COOKIE_NAME,
                    game_id,
                    token,
                    *expires_at,
                )?,
            }
            Ok(output)
        }
    }
}
fn set_cookie(
    response: &mut Response,
    name: &str,
    game: GameId,
    token: &str,
    expires: i64,
) -> Result<(), ApiError> {
    use crate::auth::Runtime;
    crate::security::token_digest(token).map_err(|_| ApiError::Unavailable)?;
    let now = super::WorkerRuntime.now_ms();
    if expires <= now {
        return Err(ApiError::Unavailable);
    }
    let seconds = expires
        .saturating_sub(now)
        .div_euclid(crate::limits::MILLISECONDS_PER_SECOND);
    let text = Zeroizing::new(format!(
        "{name}={token}; Secure; HttpOnly; Path={}; SameSite=Strict; Max-Age={seconds}",
        api::game_cookie_path(game)
    ));
    response.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_str(&text).map_err(|_| ApiError::Unavailable)?,
    );
    Ok(())
}
fn json_response(status: u16, bytes: Vec<u8>) -> Result<Response, ApiError> {
    http::Response::builder()
        .status(status)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Cache-Control", "no-store")
        .header("Referrer-Policy", "no-referrer")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes))
        .map_err(|_| ApiError::Unavailable)
}
pub(super) fn rejected(code: GameRejection) -> Result<Response, ApiError> {
    let (status, code, message) = match code {
        GameRejection::InvalidInput => (400, "invalid_input", "invalid game command"),
        GameRejection::Unauthorized => (401, "unauthorized", "game authority required"),
        GameRejection::Forbidden => (403, "forbidden", "game permission denied"),
        GameRejection::NotFound => (404, "not_found", "game not found"),
        GameRejection::Conflict => (409, "conflict", "game command conflict"),
        GameRejection::StaleCommand => (409, "stale_command", "stale game command"),
        GameRejection::Unavailable => (503, "service_unavailable", "game service unavailable"),
    };
    let mut response = json_response(
        status,
        serde_json::to_vec(&serde_json::json!({"error":{"code":code,"message":message}}))
            .map_err(|_| ApiError::Unavailable)?,
    )?;
    *response.status_mut() = StatusCode::from_u16(status).map_err(|_| ApiError::Unavailable)?;
    Ok(response)
}
