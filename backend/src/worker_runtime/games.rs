//! Public game ingress; all authority and game commits stay in private owners.
use super::{
    edge, game_peers,
    game_wire::{GameAction, GameCookieWire, GameIngress, GameOwnerResponse, GameRejection},
};
use crate::auth::Runtime as _;
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
    if (request.method() == http::Method::GET && request.uri().path() == "/api/games")
        || request.uri().path().ends_with("/availability")
    {
        return super::discovery::handle(State(env), request).await;
    }
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
    let bytes = match to_bytes(body, api::BODY_LIMIT).await {
        Ok(bytes) => Zeroizing::new(bytes.to_vec()),
        Err(_) => {
            if let Ok(context) = api::operator_rejection_context(
                &parts.method,
                parts.uri.path(),
                &parts.headers,
                &cfg.application_origin,
            ) {
                super::operator_runtime::audit_rejected_transport(env, context).await?;
            }
            return Err(ApiError::PayloadTooLarge);
        }
    };
    let decoded = api::decode_games(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        &bytes,
        &cfg.application_origin,
    );
    let mut decoded = match decoded {
        Ok(decoded) => decoded,
        Err(error) => {
            if let Ok(context) = api::operator_rejection_context(
                &parts.method,
                parts.uri.path(),
                &parts.headers,
                &cfg.application_origin,
            ) {
                super::operator_runtime::audit_rejected_transport(env, context).await?;
            }
            return Err(error);
        }
    };
    let mut ingress =
        GameIngress::from_decoded(&mut decoded, &bytes).map_err(|_| ApiError::InvalidInput)?;
    if ingress.operation == GameAction::RecoverPlayer {
        let GamePayload::RecoverPlayer(input) =
            ingress.payload().map_err(|_| ApiError::InvalidInput)?
        else {
            return Err(ApiError::InvalidInput);
        };
        let code = match brews_domain::games::GameCode::from_lookup(&input.game_code) {
            Ok(code) => code,
            Err(_) => return rejected(GameRejection::InvalidRecovery),
        };
        let caller = parts
            .headers
            .get("cf-connecting-ip")
            .and_then(|h| h.to_str().ok())
            .filter(|s| s.len() <= 45)
            .and_then(|s| s.parse::<std::net::IpAddr>().ok())
            .ok_or(ApiError::Unavailable)?;
        ingress.recovery_caller = Some(caller.to_string());
        match game_peers::directory_reply(
            env,
            super::game_directory::DirectoryGameRequest::DiscoveryCode { game_code: code },
        )
        .await
        .map_err(|_| ApiError::Unavailable)?
        {
            super::game_directory::DirectoryGameOutcome::Lookup {
                game_id: Some(game_id),
                ..
            } => ingress.game_id = Some(game_id),
            super::game_directory::DirectoryGameOutcome::Lookup { game_id: None, .. } => {
                return rejected(GameRejection::InvalidRecovery);
            }
            _ => return Err(ApiError::Unavailable),
        }
    }
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
        GameOwnerResponse::Operator { response, .. } => {
            if let Some(failure) =
                super::operator_runtime::release_edge(env, &ingress, &result).await?
            {
                return rejected(failure);
            }
            json_response(
                200,
                response.encode_json().map_err(|_| ApiError::Unavailable)?,
            )
        }
        GameOwnerResponse::HostTransferTargets { response, .. } => {
            if let Some(failure) =
                super::operator_runtime::release_edge(env, &ingress, &result).await?
            {
                return rejected(failure);
            }
            json_response(
                200,
                serde_json::to_vec(response).map_err(|_| ApiError::Unavailable)?,
            )
        }
        GameOwnerResponse::AudienceSynced {
            response,
            authority,
        } => {
            let release = super::game_wire::AudienceReleaseRequest::Sync {
                token: ingress
                    .session_token
                    .as_deref()
                    .ok_or(ApiError::Unavailable)?
                    .to_owned(),
                authority: authority.clone(),
            };
            if let Some(failure) =
                release_audience_output(env, &ingress, release, authority.valid_until).await?
            {
                return Ok(failure);
            }
            json_response(
                200,
                response.encode_json().map_err(|_| ApiError::Unavailable)?,
            )
        }
        GameOwnerResponse::Audience {
            response,
            cookie,
            valid_until,
            clear_player_cookie,
            clear_spectator_cookie,
        } => {
            if let Some(deadline) = valid_until {
                let release =
                    if let brews_contracts::games::GameResponse::AdmissionContext { expires_at } =
                        response
                    {
                        let GamePayload::AdmissionContext(input) =
                            ingress.payload().map_err(|_| ApiError::Unavailable)?
                        else {
                            return Err(ApiError::Unavailable);
                        };
                        let token = match cookie {
                            GameCookieWire::SpectatorAdmission { token, .. } => token.as_str(),
                            _ => ingress
                                .session_token
                                .as_deref()
                                .or(ingress.admission_token.as_deref())
                                .ok_or(ApiError::Unavailable)?,
                        };
                        super::game_wire::AudienceReleaseRequest::Context {
                            token: token.to_owned(),
                            game_code: input.game_code,
                            game_id,
                            expires_at: *expires_at,
                        }
                    } else {
                        let receipt = match response {
                            brews_contracts::games::GameResponse::ParticipantChanged {
                                receipt,
                            }
                            | brews_contracts::games::GameResponse::Committed { receipt } => {
                                receipt
                            }
                            _ => return Err(ApiError::Unavailable),
                        };
                        let current = match cookie {
                            GameCookieWire::Spectator { token, .. }
                            | GameCookieWire::Player { token, .. } => Some(token.clone()),
                            _ => ingress.session_token.clone(),
                        };
                        super::game_wire::AudienceReleaseRequest::Receipt {
                            token: current,
                            context: ingress.admission_token.clone(),
                            receipt: receipt.clone(),
                        }
                    };
                if let Some(failure) =
                    release_audience_output(env, &ingress, release, *deadline).await?
                {
                    return Ok(failure);
                }
            }
            let mut output = json_response(
                200,
                response.encode_json().map_err(|_| ApiError::Unavailable)?,
            )?;
            match cookie {
                GameCookieWire::Spectator { token, expires_at } => set_cookie(
                    &mut output,
                    api::SPECTATOR_COOKIE_NAME,
                    game_id,
                    token,
                    *expires_at,
                )?,
                GameCookieWire::SpectatorAdmission { token, expires_at } => set_cookie(
                    &mut output,
                    api::SPECTATOR_ADMISSION_COOKIE_NAME,
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
                GameCookieWire::None => {}
                _ => return Err(ApiError::Unavailable),
            }
            for (clear, name) in [
                (*clear_player_cookie, api::PLAYER_COOKIE_NAME),
                (*clear_spectator_cookie, api::SPECTATOR_COOKIE_NAME),
            ] {
                if clear {
                    output.headers_mut().append(
                        "set-cookie",
                        HeaderValue::from_str(&format!(
                            "{name}=; Secure; HttpOnly; Path={}; SameSite=Strict; Max-Age=0",
                            api::game_cookie_path(game_id)
                        ))
                        .map_err(|_| ApiError::Unavailable)?,
                    );
                }
            }
            Ok(output)
        }
        GameOwnerResponse::Membership {
            response,
            valid_until,
            clear_player_cookie,
        } => {
            if *clear_player_cookie {
                let mut output = json_response(
                    200,
                    response.encode_json().map_err(|_| ApiError::Unavailable)?,
                )?;
                output.headers_mut().insert(
                    "set-cookie",
                    HeaderValue::from_str(&format!(
                        "{}=; Secure; HttpOnly; Path={}; SameSite=Strict; Max-Age=0",
                        api::PLAYER_COOKIE_NAME,
                        api::game_cookie_path(game_id)
                    ))
                    .map_err(|_| ApiError::Unavailable)?,
                );
                return Ok(output);
            }
            let original_deadline = valid_until.ok_or(ApiError::Unavailable)?;
            let observed_at = super::runtime::WorkerRuntime.now_ms();
            if observed_at >= original_deadline {
                fence_recovery_output(env, &ingress, original_deadline, observed_at).await?;
                return rejected(GameRejection::Unauthorized);
            }
            ingress.operation = match ingress.operation {
                GameAction::RenameAlias
                    if matches!(
                        response,
                        brews_contracts::games::GameResponse::Committed { .. }
                    ) =>
                {
                    GameAction::RenameAliasReceiptOutput
                }
                GameAction::RenameAlias => GameAction::RenameAliasOutput,
                GameAction::LeavePlayer => GameAction::LeavePlayerOutput,
                _ => return Err(ApiError::Unavailable),
            };
            let checked: GameOwnerResponse = game_peers::call(
                env,
                "GAMES",
                &game_id.to_string(),
                "/command",
                &ingress,
                brews_contracts::games::GAME_FRAME_MAX_BYTES
                    + super::game_wire::GAME_WIRE_MAX_BYTES,
            )
            .await
            .map_err(|_| ApiError::Unavailable)?;
            // Expiry at this receiving cut binds the original owner deadline and sample.
            let observed_at = super::runtime::WorkerRuntime.now_ms();
            if observed_at >= original_deadline {
                fence_recovery_output(env, &ingress, original_deadline, observed_at).await?;
                return rejected(GameRejection::Unauthorized);
            }
            checked
                .validate_for(&ingress)
                .map_err(|_| ApiError::Unavailable)?;
            match &checked {
                GameOwnerResponse::Membership {
                    response: current,
                    valid_until: Some(deadline),
                    clear_player_cookie: false,
                } => {
                    if *deadline > original_deadline
                        || !same_receipt(
                            membership_receipt(response)?,
                            membership_receipt(current)?,
                        )?
                    {
                        return Err(ApiError::Unavailable);
                    }
                    if observed_at >= *deadline {
                        fence_recovery_output(env, &ingress, *deadline, observed_at).await?;
                        return rejected(GameRejection::Unauthorized);
                    }
                    json_response(
                        200,
                        response.encode_json().map_err(|_| ApiError::Unavailable)?,
                    )
                }
                GameOwnerResponse::Rejected { code } => rejected(*code),
                _ => Err(ApiError::Unavailable),
            }
        }
        GameOwnerResponse::RecoveryAnswer {
            response,
            valid_until,
        } => {
            let observed_at = super::runtime::WorkerRuntime.now_ms();
            if observed_at >= *valid_until {
                fence_recovery_output(env, &ingress, *valid_until, observed_at).await?;
                return rejected(GameRejection::Unauthorized);
            }
            let checked: GameOwnerResponse = game_peers::call(
                env,
                "GAMES",
                &game_id.to_string(),
                "/command",
                &ingress,
                brews_contracts::games::GAME_FRAME_MAX_BYTES
                    + super::game_wire::GAME_WIRE_MAX_BYTES,
            )
            .await
            .map_err(|_| ApiError::Unavailable)?;
            checked
                .validate_for(&ingress)
                .map_err(|_| ApiError::Unavailable)?;
            match &checked {
                GameOwnerResponse::RecoveryAnswer {
                    response: current,
                    valid_until,
                } => {
                    if !same_receipt(answer_receipt(response)?, answer_receipt(current)?)? {
                        return Err(ApiError::Unavailable);
                    }
                    let observed_at = super::runtime::WorkerRuntime.now_ms();
                    if observed_at >= *valid_until {
                        fence_recovery_output(env, &ingress, *valid_until, observed_at).await?;
                        return rejected(GameRejection::Unauthorized);
                    }
                    json_response(
                        200,
                        response.encode_json().map_err(|_| ApiError::Unavailable)?,
                    )
                }
                GameOwnerResponse::Rejected { code } => rejected(*code),
                _ => Err(ApiError::Unavailable),
            }
        }
        GameOwnerResponse::Recovered {
            receipt,
            game,
            cookie,
            valid_until,
        } => {
            if let GameCookieWire::Player { token, expires_at } = cookie {
                ingress.operation = GameAction::RecoveryOutput;
                ingress.session_token = Some(token.to_owned());
                let observed_at = super::runtime::WorkerRuntime.now_ms();
                if let Some(deadline) = valid_until.filter(|deadline| *deadline <= observed_at) {
                    fence_recovery_output(env, &ingress, deadline, observed_at).await?;
                    return rejected(GameRejection::InvalidRecovery);
                }
                // A new owner cut follows each await; an expired initial cut is never retried.
                let checked: GameOwnerResponse = game_peers::call(
                    env,
                    "GAMES",
                    &game_id.to_string(),
                    "/command",
                    &ingress,
                    brews_contracts::games::GAME_FRAME_MAX_BYTES
                        + super::game_wire::GAME_WIRE_MAX_BYTES,
                )
                .await
                .map_err(|_| ApiError::Unavailable)?;
                checked
                    .validate_for(&ingress)
                    .map_err(|_| ApiError::Unavailable)?;
                match checked {
                    GameOwnerResponse::Recovered {
                        receipt: current_receipt,
                        game,
                        valid_until: Some(valid_until),
                        cookie:
                            GameCookieWire::Player {
                                ref token,
                                expires_at: current_expiry,
                            },
                    } => {
                        let observed_at = super::runtime::WorkerRuntime.now_ms();
                        if current_expiry != *expires_at
                            || !same_receipt(receipt, &current_receipt)?
                        {
                            return Err(ApiError::Unavailable);
                        }
                        if observed_at >= valid_until {
                            fence_recovery_output(env, &ingress, valid_until, observed_at).await?;
                            return rejected(GameRejection::InvalidRecovery);
                        }
                        recovery_output(
                            game_id,
                            &current_receipt,
                            game.as_ref(),
                            Some((token, current_expiry)),
                        )
                    }
                    GameOwnerResponse::Rejected { code } => rejected(code),
                    _ => Err(ApiError::Unavailable),
                }
            } else {
                recovery_output(game_id, receipt, game.as_ref(), None)
            }
        }
        GameOwnerResponse::Rejected { code } => rejected_spectator_entry(&ingress, *code),
        GameOwnerResponse::Synced {
            response,
            valid_until,
            authority,
            player_authority,
        } => {
            let valid_until = valid_until.ok_or(ApiError::Unavailable)?;
            let token = ingress
                .session_token
                .as_deref()
                .ok_or(ApiError::Unavailable)?;
            // Preserve expiry observed at receipt before any account reauthorization await.
            let observed_at = super::WorkerRuntime.now_ms();
            if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&observed_at) {
                return Err(ApiError::Unavailable);
            }
            if observed_at >= valid_until {
                fence_recovery_output(env, &ingress, valid_until, observed_at).await?;
                if let Some(authority) = authority {
                    let proof = authority
                        .denial_authority()
                        .map_err(|_| ApiError::Unavailable)?;
                    if observed_at >= proof.expires_at() {
                        game_peers::fence_discovery_account_expiry(env, token, &proof, observed_at)
                            .await
                            .map_err(|_| ApiError::Unavailable)?;
                    }
                }
                return rejected(GameRejection::Unauthorized);
            }
            let proof = if ingress.view == Some(super::game_wire::ViewSelector::Account) {
                let proof = game_peers::authorize_account(env, token)
                    .await
                    .map_err(|_| ApiError::Unavailable)?;
                if authority.as_ref() != Some(&super::game_wire::SyncAccountAuthority::from(&proof))
                {
                    return Err(ApiError::Unavailable);
                }
                Some(proof)
            } else {
                if authority.is_some() {
                    return Err(ApiError::Unavailable);
                }
                let binding = player_authority.as_ref().ok_or(ApiError::Unavailable)?;
                let release = super::game_wire::PlayerSyncReleaseRequest {
                    token: token.to_owned(),
                    authority: binding.clone(),
                };
                let checked: super::game_wire::PlayerSyncReleaseReply = game_peers::call(
                    env,
                    "GAMES",
                    &game_id.to_string(),
                    "/sync-player-release",
                    &release,
                    super::game_wire::GAME_WIRE_MAX_BYTES,
                )
                .await
                .map_err(|_| ApiError::Unavailable)?;
                // Sample the original deadline before inspecting the release ACK.
                let observed_at = super::WorkerRuntime.now_ms();
                if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&observed_at) {
                    return Err(ApiError::Unavailable);
                }
                if observed_at >= valid_until {
                    fence_recovery_output(env, &ingress, valid_until, observed_at).await?;
                    return rejected(GameRejection::Unauthorized);
                }
                checked
                    .validate_for(binding)
                    .map_err(|_| ApiError::Unavailable)?;
                if let super::game_wire::PlayerSyncReleaseReply::Rejected { code } = checked {
                    return rejected(code);
                }
                None
            };
            let observed_at = super::WorkerRuntime.now_ms();
            if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&observed_at) {
                return Err(ApiError::Unavailable);
            }
            if observed_at >= valid_until {
                fence_recovery_output(env, &ingress, valid_until, observed_at).await?;
                if let Some(proof) = proof.filter(|p| p.expires_at() <= observed_at) {
                    game_peers::fence_discovery_account_expiry(env, token, &proof, observed_at)
                        .await
                        .map_err(|_| ApiError::Unavailable)?;
                }
                return rejected(GameRejection::Unauthorized);
            }
            json_response(
                200,
                response.encode_json().map_err(|_| ApiError::Unavailable)?,
            )
        }
        GameOwnerResponse::Success { response, cookie } => {
            let bytes = response.encode_json().map_err(|_| ApiError::Unavailable)?;
            let mut output = json_response(response.status(), bytes)?;
            match cookie {
                GameCookieWire::Spectator { .. } | GameCookieWire::SpectatorAdmission { .. } => {
                    return Err(ApiError::Unavailable);
                }
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
fn same_receipt(
    first: &brews_contracts::games::GameReceipt,
    second: &brews_contracts::games::GameReceipt,
) -> Result<bool, ApiError> {
    Ok(first.encode_json().map_err(|_| ApiError::Unavailable)?
        == second.encode_json().map_err(|_| ApiError::Unavailable)?)
}
fn answer_receipt(
    response: &brews_contracts::games::GameResponse,
) -> Result<&brews_contracts::games::GameReceipt, ApiError> {
    match response {
        brews_contracts::games::GameResponse::RecoveryAnswerChanged { receipt, .. }
        | brews_contracts::games::GameResponse::Committed { receipt } => Ok(receipt),
        _ => Err(ApiError::Unavailable),
    }
}
fn membership_receipt(
    response: &brews_contracts::games::GameResponse,
) -> Result<&brews_contracts::games::GameReceipt, ApiError> {
    match response {
        brews_contracts::games::GameResponse::AliasRenamed { receipt, .. }
        | brews_contracts::games::GameResponse::PlayerLeft { receipt, .. }
        | brews_contracts::games::GameResponse::Committed { receipt } => Ok(receipt),
        _ => Err(ApiError::Unavailable),
    }
}
fn recovery_output(
    game_id: GameId,
    receipt: &brews_contracts::games::GameReceipt,
    game: Option<&brews_contracts::games::GameView>,
    cookie: Option<(&str, i64)>,
) -> Result<Response, ApiError> {
    let value = if let Some(game) = game {
        let brews_contracts::games::GameView::Player {
            player_id,
            alias,
            board,
            ..
        } = game
        else {
            return Err(ApiError::Unavailable);
        };
        serde_json::json!({"receipt":receipt,"membership":{"player_id":player_id,"alias":alias},"game":game,"board":board})
    } else {
        serde_json::json!({"receipt":receipt})
    };
    let mut response = json_response(
        200,
        serde_json::to_vec(&value).map_err(|_| ApiError::Unavailable)?,
    )?;
    if let Some((token, expires)) = cookie {
        set_cookie(
            &mut response,
            api::PLAYER_COOKIE_NAME,
            game_id,
            token,
            expires,
        )?;
    }
    Ok(response)
}
async fn release_audience_output(
    env: &Env,
    ingress: &GameIngress,
    request: super::game_wire::AudienceReleaseRequest,
    deadline: i64,
) -> Result<Option<Response>, ApiError> {
    let game = ingress.game_id.ok_or(ApiError::Unavailable)?;
    let before = super::WorkerRuntime.now_ms();
    if before < 0 {
        return Err(ApiError::Unavailable);
    }
    if before >= deadline {
        fence_recovery_output(env, ingress, deadline, before).await?;
        return rejected(GameRejection::Unauthorized).map(Some);
    }
    let attempt =
        super::game_wire::AudienceReleaseAttempt::new(request, &super::WorkerRuntime, before)
            .map_err(|_| ApiError::Unavailable)?;
    let binding = attempt.binding().map_err(|_| ApiError::Unavailable)?;
    let checked: super::game_wire::AudienceReleaseReply = game_peers::call(
        env,
        "GAMES",
        &game.to_string(),
        "/audience-release",
        &attempt,
        super::game_wire::GAME_WIRE_MAX_BYTES,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    match &checked {
        super::game_wire::AudienceReleaseReply::Released { binding: ack, .. }
        | super::game_wire::AudienceReleaseReply::Rejected { binding: ack, .. } => {
            if ack != &binding {
                return Err(ApiError::Unavailable);
            }
        }
    }
    let now = super::WorkerRuntime.now_ms();
    if now < 0 {
        return Err(ApiError::Unavailable);
    }
    if now >= deadline {
        fence_recovery_output(env, ingress, deadline, now).await?;
        return rejected(GameRejection::Unauthorized).map(Some);
    }
    match checked {
        super::game_wire::AudienceReleaseReply::Rejected { code, .. } => rejected(code).map(Some),
        super::game_wire::AudienceReleaseReply::Released {
            valid_until,
            checked_at,
            binding: ack,
        } => {
            if ack != binding
                || checked_at > now
                || valid_until <= 0
                || valid_until > deadline
                || checked_at < 0
                || checked_at >= valid_until
            {
                return Err(ApiError::Unavailable);
            }
            if now >= valid_until {
                fence_recovery_output(env, ingress, valid_until, now).await?;
                return rejected(GameRejection::Unauthorized).map(Some);
            }
            Ok(None)
        }
    }
}
async fn fence_recovery_output(
    env: &Env,
    ingress: &GameIngress,
    deadline: i64,
    observed_at: i64,
) -> Result<(), ApiError> {
    let game = ingress.game_id.ok_or(ApiError::Unavailable)?;
    let request = super::discovery::wire::DiscoveryOwnerRequest {
        game_id: game,
        action: if ingress.view == Some(super::game_wire::ViewSelector::Account) {
            super::discovery::wire::DiscoveryAction::Account {
                token: game_peers::SecretToken::new(
                    ingress
                        .session_token
                        .as_deref()
                        .ok_or(ApiError::Unavailable)?,
                ),
            }
        } else {
            super::discovery::wire::DiscoveryAction::Player {
                token: game_peers::SecretToken::new(
                    ingress
                        .session_token
                        .as_deref()
                        .or(ingress.admission_token.as_deref())
                        .unwrap_or(""),
                ),
            }
        },
        expiry: Some(super::discovery::wire::ObservedDiscoveryExpiry {
            deadline,
            observed_at,
        }),
    };
    let reply: super::discovery::wire::DiscoveryOwnerReply = game_peers::call(
        env,
        "GAMES",
        &game.to_string(),
        "/discovery",
        &request,
        super::discovery::wire::DISCOVERY_REPLY_MAX_BYTES,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    super::discovery::wire::verify(&request, &reply, super::runtime::WorkerRuntime.now_ms())
        .map_err(|_| ApiError::Unavailable)?;
    if !matches!(
        reply.outcome,
        super::discovery::wire::DiscoveryOutcome::ExpiryFenced { .. }
    ) {
        return Err(ApiError::Unavailable);
    }
    Ok(())
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
fn rejected_spectator_entry(
    ingress: &GameIngress,
    code: GameRejection,
) -> Result<Response, ApiError> {
    let mut response = rejected(code)?;
    if matches!(code, GameRejection::Unauthorized)
        && matches!(
            ingress.operation,
            GameAction::SpectatorContext | GameAction::JoinSpectator
        )
        && ingress.session_token.is_some()
    {
        // An expired grace seat leaves a longer-lived HttpOnly browser cookie.
        // Clear only that selected credential; retry still requires fresh code proof.
        let game = ingress.game_id.ok_or(ApiError::Unavailable)?;
        response.headers_mut().insert(
            "set-cookie",
            HeaderValue::from_str(&format!(
                "{}=; Secure; HttpOnly; Path={}; SameSite=Strict; Max-Age=0",
                api::SPECTATOR_COOKIE_NAME,
                api::game_cookie_path(game)
            ))
            .map_err(|_| ApiError::Unavailable)?,
        );
    }
    Ok(response)
}

pub(super) fn rejected(code: GameRejection) -> Result<Response, ApiError> {
    let (status, code, message) = match code {
        GameRejection::InvalidInput => (400, "invalid_input", "invalid game command"),
        GameRejection::Unauthorized => (401, "unauthorized", "game authority required"),
        GameRejection::Forbidden => (403, "forbidden", "game permission denied"),
        GameRejection::NotFound => (404, "not_found", "game not found"),
        GameRejection::Conflict => (409, "conflict", "game command conflict"),
        GameRejection::StaleCommand => (409, "stale_command", "stale game command"),
        GameRejection::InvalidRecovery => {
            (401, "invalid_recovery", "invalid player recovery proof")
        }
        GameRejection::RecoveryBlocked => (429, "recovery_blocked", "player recovery blocked"),
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
