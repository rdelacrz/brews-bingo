//! Public History reads: bounded routing index, authoritative owner hydration.
use super::{
    edge,
    game_directory::{DirectoryGameOutcome, DirectoryGameRequest},
    game_peers,
    history_wire::{self, HistoryOwnerOutcome, HistoryOwnerReply, HistoryOwnerRequest},
};
use crate::{
    api::{self, ApiError, HistoryCommand, HistoryCursor},
    auth::{GameAccountAuthority, Runtime},
    config::get_backend_config,
};
use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    response::Response,
};
use brews_contracts::history::{HistoryDetail, HistoryPage};
use http::{HeaderValue, StatusCode};
use worker::{
    Env,
    send::{SendFuture, SendWrapper},
};
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
enum HistoryError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid History request")]
    Transport(#[from] ApiError),
}
fn failure(error: HistoryError) -> Response {
    match error {
        HistoryError::Transport(error) => edge::failure(error),
        HistoryError::Unauthorized => {
            let mut response = output(
                &serde_json::json!({"error":{"code":"unauthorized","message":"Unauthorized."}}),
            )
            .unwrap_or_else(|_| edge::failure(ApiError::Unavailable));
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            response
        }
    }
}
pub(super) async fn handle(State(env): State<SendWrapper<Env>>, request: Request) -> Response {
    SendFuture::new(async move { dispatch(&env, request).await.unwrap_or_else(failure) }).await
}
fn peer_error(e: game_peers::GamePeerError) -> HistoryError {
    match e {
        game_peers::GamePeerError::Unauthorized => HistoryError::Unauthorized,
        _ => ApiError::Unavailable.into(),
    }
}
async fn current(env: &Env, token: &str) -> Result<GameAccountAuthority, HistoryError> {
    game_peers::authorize_account(env, token)
        .await
        .map_err(peer_error)
}
fn same_authority(a: &GameAccountAuthority, b: &GameAccountAuthority) -> bool {
    a.account_id() == b.account_id()
        && a.session_id() == b.session_id()
        && a.role() == b.role()
        && a.credential_epoch() == b.credential_epoch()
        && a.expires_at() == b.expires_at()
}
async fn owner(
    env: &Env,
    request: &HistoryOwnerRequest,
) -> Result<HistoryOwnerOutcome, HistoryError> {
    let reply: HistoryOwnerReply = game_peers::call(
        env,
        "GAMES",
        &request.game_id.to_string(),
        "/history",
        request,
        history_wire::HISTORY_REPLY_MAX_BYTES,
    )
    .await
    .map_err(peer_error)?;
    history_wire::verify_owner(request, reply, super::WorkerRuntime.now_ms()).map_err(peer_error)
}
async fn confirm_expiry_denial(
    env: &Env,
    token: &str,
    game_id: brews_domain::ids::GameId,
) -> Result<(), HistoryError> {
    // A release-time expiry hole is omitted only after the owning Game durably denies it.
    // A rolled-back owner clock, storage failure, or unexpected success is unavailable.
    match owner(
        env,
        &HistoryOwnerRequest {
            game_id,
            token: game_peers::SecretToken::new(token),
            summary: true,
        },
    )
    .await?
    {
        HistoryOwnerOutcome::NotFound {} => Ok(()),
        HistoryOwnerOutcome::Unauthorized {} => Err(HistoryError::Unauthorized),
        _ => Err(ApiError::Unavailable.into()),
    }
}
async fn dispatch(env: &Env, request: Request) -> Result<Response, HistoryError> {
    let cfg = get_backend_config(env).map_err(|_| ApiError::Unavailable)?;
    if request.uri().scheme_str() != Some("https")
        || request
            .uri()
            .authority()
            .is_some_and(|a| a.as_str().contains('@'))
    {
        return Err(ApiError::Forbidden.into());
    }
    let (parts, body) = request.into_parts();
    let bytes = Zeroizing::new(
        to_bytes(body, api::BODY_LIMIT)
            .await
            .map_err(|_| ApiError::PayloadTooLarge)?
            .to_vec(),
    );
    let decoded = api::decode_history(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        &bytes,
        &cfg.application_origin,
    )?;
    let token = decoded
        .session_token
        .as_deref()
        .ok_or(HistoryError::Unauthorized)?;
    let initial = current(env, token).await?;
    match decoded.command {
        HistoryCommand::Detail(game_id) => {
            let outcome = owner(
                env,
                &HistoryOwnerRequest {
                    game_id,
                    token: game_peers::SecretToken::new(token),
                    summary: false,
                },
            )
            .await?;
            let proof = current(env, token).await?;
            if !same_authority(&initial, &proof) {
                return Err(HistoryError::Unauthorized);
            }
            match outcome {
                HistoryOwnerOutcome::Detail { history } => {
                    let now = super::WorkerRuntime.now_ms();
                    history_wire::validate_history(&history, now).map_err(peer_error)?;
                    if history.expires_at <= now {
                        confirm_expiry_denial(env, token, game_id).await?;
                        let proof = current(env, token).await?;
                        if !same_authority(&initial, &proof) {
                            return Err(HistoryError::Unauthorized);
                        }
                        return Err(ApiError::NotFound.into());
                    }
                    output(&HistoryDetail { history })
                }
                HistoryOwnerOutcome::NotFound {} => Err(ApiError::NotFound.into()),
                HistoryOwnerOutcome::Unauthorized {} => Err(HistoryError::Unauthorized),
                _ => Err(ApiError::Unavailable.into()),
            }
        }
        HistoryCommand::List(query) => {
            let DirectoryGameOutcome::HistoryIndexes { entries } = game_peers::directory_reply(
                env,
                DirectoryGameRequest::HistoryIndexes {
                    after: query.after,
                    limit: query.limit,
                    outcome: query.outcome,
                },
            )
            .await
            .map_err(peer_error)?
            else {
                return Err(ApiError::Unavailable.into());
            };
            let mut indexes = Vec::with_capacity(entries.len());
            let observed = super::WorkerRuntime.now_ms();
            for e in entries {
                let p = e.proof().map_err(|_| ApiError::Unavailable)?;
                let (Some(start), Some(end), Some(expiry), Some(_)) = (
                    p.started_at(),
                    p.ended_at(),
                    p.history_expires_at(),
                    p.game_code(),
                ) else {
                    return Err(ApiError::Unavailable.into());
                };
                if !matches!(
                    p.state(),
                    brews_domain::games::GameState::Resolved
                        | brews_domain::games::GameState::Cancelled
                ) || !(1..=start).contains(&p.created_at())
                    || !(start..=observed).contains(&end)
                    || crate::db::game::add_three_calendar_months(end).ok() != Some(expiry)
                {
                    return Err(ApiError::Unavailable.into());
                }
                indexes.push(p);
            }
            let more = indexes.len() > query.limit as usize;
            indexes.truncate(query.limit as usize);
            let next_cursor = if more {
                indexes.last().map(|p| {
                    HistoryCursor {
                        ended_at: p.ended_at().unwrap_or(0),
                        game_id: p.game_id(),
                    }
                    .encode(query.outcome)
                })
            } else {
                None
            };
            let mut games = Vec::with_capacity(indexes.len());
            // Concurrency one bounds owner fanout and never scans to fill expiry holes.
            for p in indexes {
                let outcome = owner(
                    env,
                    &HistoryOwnerRequest {
                        game_id: p.game_id(),
                        token: game_peers::SecretToken::new(token),
                        summary: true,
                    },
                )
                .await?;
                match outcome {
                    HistoryOwnerOutcome::Summary { summary } => {
                        if summary.game_id != p.game_id()
                            || Some(&summary.game_code) != p.game_code()
                            || summary.designated_host_id != p.designated_host_id()
                            || summary.outcome != p.state()
                            || Some(summary.started_at) != p.started_at()
                            || Some(summary.ended_at) != p.ended_at()
                            || Some(summary.expires_at) != p.history_expires_at()
                        {
                            return Err(ApiError::Unavailable.into());
                        }
                        games.push(summary);
                    }
                    HistoryOwnerOutcome::NotFound {}
                        if p.history_expires_at()
                            .is_some_and(|t| t <= super::WorkerRuntime.now_ms()) => {}
                    HistoryOwnerOutcome::Unauthorized {} => return Err(HistoryError::Unauthorized),
                    _ => return Err(ApiError::Unavailable.into()),
                }
            }
            loop {
                let proof = current(env, token).await?;
                if !same_authority(&initial, &proof) {
                    return Err(HistoryError::Unauthorized);
                }
                let now = super::WorkerRuntime.now_ms();
                for s in &games {
                    history_wire::validate_summary(s, now).map_err(peer_error)?;
                }
                let expired = games
                    .iter()
                    .filter(|s| s.expires_at <= now)
                    .map(|s| s.game_id)
                    .collect::<Vec<_>>();
                if expired.is_empty() {
                    return output(&HistoryPage { games, next_cursor });
                }
                for game_id in expired {
                    confirm_expiry_denial(env, token, game_id).await?;
                }
                games.retain(|s| s.expires_at > now);
                // Each retry removes a selected row: bounded by the page, never fills holes.
                // Recheck Accounts/time after denial awaits before disclosing remaining games.
            }
        }
    }
}
fn output(value: &impl serde::Serialize) -> Result<Response, HistoryError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ApiError::Unavailable)?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    for (name, value) in [
        ("content-type", "application/json"),
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    Ok(response)
}
