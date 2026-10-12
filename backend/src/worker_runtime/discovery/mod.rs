//! Slice 1 edge routing. The final successful owner reply is released without another await.
mod owner;
mod reservation_lookup;
pub(super) mod wire;

use self::wire::{
    DISCOVERY_REPLY_MAX_BYTES, DiscoveryAction, DiscoveryDirectoryReply, DiscoveryDirectoryRequest,
    DiscoveryOutcome, DiscoveryOwnerReply, DiscoveryOwnerRequest, ObservedDiscoveryExpiry,
};
use super::{edge, game_peers};
use crate::{
    api::{self, ApiError, AvailabilityView, DiscoveryCommand},
    auth::Runtime,
    config::get_backend_config,
};
use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    response::Response,
};
use brews_contracts::discovery::{CurrentGame, CurrentGames, GameAvailability, GameSlot};
use http::{HeaderValue, StatusCode};
use worker::{
    Env,
    send::{SendFuture, SendWrapper},
};
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
enum DiscoveryError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid discovery request")]
    Transport(#[from] ApiError),
}
fn peer_error(error: game_peers::GamePeerError) -> DiscoveryError {
    match error {
        game_peers::GamePeerError::Unauthorized => DiscoveryError::Unauthorized,
        _ => ApiError::Unavailable.into(),
    }
}
fn failure(error: DiscoveryError) -> Response {
    match error {
        DiscoveryError::Transport(e) => edge::failure(e),
        DiscoveryError::Unauthorized => {
            let mut r = output(
                &serde_json::json!({"error":{"code":"unauthorized","message":"Unauthorized."}}),
            )
            .unwrap_or_else(|_| edge::failure(ApiError::Unavailable));
            *r.status_mut() = StatusCode::UNAUTHORIZED;
            r
        }
    }
}
pub(super) async fn handle(State(env): State<SendWrapper<Env>>, request: Request) -> Response {
    SendFuture::new(async move { dispatch(&env, request).await.unwrap_or_else(failure) }).await
}
async fn owner(
    env: &Env,
    request: &DiscoveryOwnerRequest,
) -> Result<DiscoveryOwnerReply, DiscoveryError> {
    let reply = game_peers::call(
        env,
        "GAMES",
        &request.game_id.to_string(),
        "/discovery",
        request,
        DISCOVERY_REPLY_MAX_BYTES,
    )
    .await
    .map_err(peer_error)?;
    wire::verify(request, &reply, super::WorkerRuntime.now_ms()).map_err(peer_error)?;
    Ok(reply)
}
async fn dispatch(env: &Env, request: Request) -> Result<Response, DiscoveryError> {
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
    let decoded = api::decode_discovery(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        &bytes,
        &cfg.application_origin,
    )?;
    let account = matches!(
        decoded.command,
        DiscoveryCommand::Current
            | DiscoveryCommand::List { .. }
            | DiscoveryCommand::Availability {
                view: AvailabilityView::Account,
                ..
            }
    );
    let initial = if account {
        Some(
            game_peers::authorize_discovery_account(
                env,
                decoded
                    .session_token
                    .as_deref()
                    .ok_or(DiscoveryError::Unauthorized)?,
            )
            .await
            .map_err(peer_error)?,
        )
    } else {
        None
    };
    let (game_id, action) = match &decoded.command {
        DiscoveryCommand::List { .. } | DiscoveryCommand::Current => {
            let routing = owner::route(env).await.map_err(|_| ApiError::Unavailable)?;
            let token = decoded
                .session_token
                .as_deref()
                .ok_or(DiscoveryError::Unauthorized)?;
            let Some(route) = routing else {
                let proof: DiscoveryDirectoryReply = game_peers::call(
                    env,
                    "GAME_DIRECTORY",
                    "directory",
                    "/discovery",
                    &DiscoveryDirectoryRequest {
                        token: game_peers::SecretToken::new(token),
                    },
                    DISCOVERY_REPLY_MAX_BYTES,
                )
                .await
                .map_err(peer_error)?;
                match proof {
                    DiscoveryDirectoryReply::Route {
                        route: None,
                        checked_at,
                        valid_until,
                    } if checked_at <= super::WorkerRuntime.now_ms()
                        && valid_until > checked_at
                        && initial
                            .as_ref()
                            .is_some_and(|p| p.expires_at() == valid_until) =>
                    {
                        fence_expired_account(
                            env,
                            token,
                            initial.as_ref(),
                            super::WorkerRuntime.now_ms(),
                        )
                        .await?;
                    }
                    DiscoveryDirectoryReply::Unauthorized {} => {
                        return Err(DiscoveryError::Unauthorized);
                    }
                    _ => return Err(ApiError::Unavailable.into()),
                }
                return current_output(
                    &decoded.command,
                    CurrentGame {
                        slot: GameSlot::Free,
                        game: None,
                    },
                );
            };
            (
                route.game_id,
                DiscoveryAction::Summary {
                    token: game_peers::SecretToken::new(token),
                },
            )
        }
        DiscoveryCommand::Code(code) => {
            let game_id = lookup(env, code).await?.ok_or(ApiError::NotFound)?;
            (
                game_id,
                DiscoveryAction::Entry {
                    game_code: code.clone(),
                },
            )
        }
        DiscoveryCommand::Availability {
            game_id,
            view,
            game_code,
        } => {
            let action = match view {
                AvailabilityView::Entry => {
                    let code = game_code.as_ref().ok_or(ApiError::InvalidInput)?;
                    if lookup(env, code).await? != Some(*game_id) {
                        return Err(ApiError::NotFound.into());
                    }
                    DiscoveryAction::Entry {
                        game_code: code.clone(),
                    }
                }
                AvailabilityView::Account => DiscoveryAction::Account {
                    token: game_peers::SecretToken::new(
                        decoded
                            .session_token
                            .as_deref()
                            .ok_or(DiscoveryError::Unauthorized)?,
                    ),
                },
                AvailabilityView::Player => DiscoveryAction::Player {
                    token: game_peers::SecretToken::new(
                        decoded
                            .session_token
                            .as_deref()
                            .ok_or(DiscoveryError::Unauthorized)?,
                    ),
                },
            };
            (*game_id, action)
        }
    };
    let mut request = DiscoveryOwnerRequest {
        game_id,
        action,
        expiry: None,
    };
    let hydrated = owner(env, &request).await?;
    let hydration_time = super::WorkerRuntime.now_ms();
    if let Some(deadline) = hydrated.valid_until.filter(|t| *t <= hydration_time) {
        drop(hydrated);
        return Err(deny_observed_expiry(
            env,
            &mut request,
            ObservedDiscoveryExpiry {
                deadline,
                observed_at: hydration_time,
            },
            if account {
                decoded.session_token.as_deref()
            } else {
                None
            },
            initial.as_ref(),
        )
        .await);
    }
    if account {
        fence_expired_account(
            env,
            decoded
                .session_token
                .as_deref()
                .ok_or(DiscoveryError::Unauthorized)?,
            initial.as_ref(),
            super::WorkerRuntime.now_ms(),
        )
        .await?;
    }
    if matches!(hydrated.outcome, DiscoveryOutcome::Unavailable {}) {
        return Err(ApiError::Unavailable.into());
    }
    if matches!(hydrated.outcome, DiscoveryOutcome::Unauthorized {}) {
        return Err(DiscoveryError::Unauthorized);
    }
    if account {
        let proof = game_peers::authorize_discovery_account(
            env,
            decoded
                .session_token
                .as_deref()
                .ok_or(DiscoveryError::Unauthorized)?,
        )
        .await
        .map_err(peer_error)?;
        if !initial
            .as_ref()
            .is_some_and(|a| owner::same_authority(a, &proof))
        {
            return Err(DiscoveryError::Unauthorized);
        }
    }
    // Recut Game after edge reauthorization/hydration. The owner reauthorizes
    // again after its peer awaits; no successful projection is reused across cuts.
    let reply = owner(env, &request).await?;
    let observed_at = super::WorkerRuntime.now_ms();
    if let Some(deadline) = reply.valid_until.filter(|t| *t <= observed_at) {
        // Discard the projection and carry the original receiving-clock sample.
        // A normal retry at a rolled-back owner clock cannot establish denial.
        drop(reply);
        return Err(deny_observed_expiry(
            env,
            &mut request,
            ObservedDiscoveryExpiry {
                deadline,
                observed_at,
            },
            if account {
                decoded.session_token.as_deref()
            } else {
                None
            },
            initial.as_ref(),
        )
        .await);
    }
    if account {
        fence_expired_account(
            env,
            decoded
                .session_token
                .as_deref()
                .ok_or(DiscoveryError::Unauthorized)?,
            initial.as_ref(),
            observed_at,
        )
        .await?;
    }
    match reply.outcome {
        DiscoveryOutcome::Current { slot, game } => {
            current_output(&decoded.command, CurrentGame { slot, game })
        }
        DiscoveryOutcome::Entry { entry } => {
            if matches!(decoded.command, DiscoveryCommand::Availability { .. }) {
                output(&GameAvailability::Entry {
                    state: entry.state,
                    can_join_player: entry.can_join_player,
                    can_join_spectator: entry.can_join_spectator,
                })
            } else {
                output(&entry)
            }
        }
        DiscoveryOutcome::Availability { availability } => output(&availability),
        DiscoveryOutcome::NotFound {} => Err(ApiError::NotFound.into()),
        DiscoveryOutcome::Unauthorized {} => Err(DiscoveryError::Unauthorized),
        DiscoveryOutcome::Unavailable {} | DiscoveryOutcome::ExpiryFenced { .. } => {
            Err(ApiError::Unavailable.into())
        }
    }
}
async fn deny_observed_expiry(
    env: &Env,
    request: &mut DiscoveryOwnerRequest,
    expiry: ObservedDiscoveryExpiry,
    account_token: Option<&str>,
    proof: Option<&crate::auth::GameAccountAuthority>,
) -> DiscoveryError {
    request.expiry = Some(expiry);
    let denial = owner(env, request).await;
    let account_denial = if let Some(token) = account_token {
        fence_expired_account(env, token, proof, expiry.observed_at).await
    } else {
        Ok(())
    };
    if matches!(
        denial,
        Ok(DiscoveryOwnerReply {
            outcome: DiscoveryOutcome::ExpiryFenced { .. },
            ..
        })
    ) {
        account_denial.err().unwrap_or_else(|| {
            if matches!(request.action, DiscoveryAction::Entry { .. }) {
                ApiError::NotFound.into()
            } else {
                ApiError::Unavailable.into()
            }
        })
    } else {
        ApiError::Unavailable.into()
    }
}
async fn fence_expired_account(
    env: &Env,
    token: &str,
    proof: Option<&crate::auth::GameAccountAuthority>,
    observed_at: i64,
) -> Result<(), DiscoveryError> {
    if let Some(proof) = proof
        && proof.expires_at() <= observed_at
    {
        game_peers::fence_discovery_account_expiry(env, token, proof, observed_at)
            .await
            .map_err(peer_error)?;
        return Err(DiscoveryError::Unauthorized);
    }
    Ok(())
}
async fn lookup(
    env: &Env,
    code: &brews_domain::games::GameCode,
) -> Result<Option<brews_domain::ids::GameId>, DiscoveryError> {
    match game_peers::directory_reply(
        env,
        super::game_directory::DirectoryGameRequest::DiscoveryCode {
            game_code: code.clone(),
        },
    )
    .await
    .map_err(peer_error)?
    {
        super::game_directory::DirectoryGameOutcome::Lookup { game_id, .. } => Ok(game_id),
        _ => Err(ApiError::Unavailable.into()),
    }
}
fn current_output(
    command: &DiscoveryCommand,
    current: CurrentGame,
) -> Result<Response, DiscoveryError> {
    match command {
        DiscoveryCommand::Current => output(&current),
        DiscoveryCommand::List { state } => output(&CurrentGames {
            slot: current.slot,
            games: current
                .game
                .filter(|g| state.is_none_or(|s| g.state == s))
                .into_iter()
                .collect(),
            next_cursor: None,
        }),
        _ => Err(ApiError::Unavailable.into()),
    }
}
fn output(value: &impl serde::Serialize) -> Result<Response, DiscoveryError> {
    let bytes = game_peers::encode_limit(value, DISCOVERY_REPLY_MAX_BYTES).map_err(peer_error)?;
    let mut response = Response::new(Body::from(bytes.to_vec()));
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
