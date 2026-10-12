//! Dedicated passive reads: fresh per-invocation and per-release challenges, original caps.
use super::{WorkerRuntime, game_object::GameObject, game_peers, games, operator_peers};
use crate::{
    api::{self, ApiError, ReadRole, ReadSelection},
    auth::{GameAccountAuthority, Runtime},
    db::game::{GameService, ReadBinding, ReadCut, ReadIdentity, read_purpose},
    game::GameError,
};
use brews_contracts::games::{GAME_FRAME_MAX_BYTES, reads::ReadProjection};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};
const READ_PEER_MAX_BYTES: usize = GAME_FRAME_MAX_BYTES + super::game_wire::GAME_WIRE_MAX_BYTES;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRequest {
    #[serde(deserialize_with = "game_peers::object")]
    selection: ReadSelection,
    token: String,
    challenge: operator_peers::OperatorChallenge,
}
impl Drop for ReadRequest {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct ReadDigest([u8; 32]);
impl Drop for ReadDigest {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
fn digest<T: Serialize>(v: &T) -> Result<ReadDigest, GameError> {
    use sha2::{Digest, Sha256};
    let bytes =
        game_peers::encode_limit(v, READ_PEER_MAX_BYTES * 2).map_err(|_| GameError::Storage)?;
    let mut h = Sha256::new();
    h.update(b"brews-dedicated-read-cut\0");
    h.update(bytes.as_slice());
    Ok(ReadDigest(h.finalize().into()))
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReadRejection {
    Unauthorized,
    Forbidden,
    NotFound,
    InvalidInput,
    ResultNotCommitted,
    RemainingValuesUnavailable,
    Unavailable,
}
impl From<GameError> for ReadRejection {
    fn from(e: GameError) -> Self {
        match e {
            GameError::Unauthorized | GameError::Expired => Self::Unauthorized,
            GameError::Forbidden => Self::Forbidden,
            GameError::NotFound => Self::NotFound,
            GameError::InvalidInput => Self::InvalidInput,
            GameError::ResultNotCommitted => Self::ResultNotCommitted,
            GameError::RemainingValuesUnavailable => Self::RemainingValuesUnavailable,
            _ => Self::Unavailable,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "read_result", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    clippy::large_enum_variant,
    reason = "The bounded owner reply fits the Worker stack; retaining it inline avoids a separate DTO allocation."
)]
enum ReadReply {
    Ready {
        challenge: operator_peers::OperatorChallenge,
        request_digest: ReadDigest,
        #[serde(deserialize_with = "game_peers::object")]
        projection: ReadProjection,
        #[serde(deserialize_with = "game_peers::object")]
        binding: ReadBinding,
        checked_at: i64,
    },
    Rejected {
        challenge: operator_peers::OperatorChallenge,
        request_digest: ReadDigest,
        code: ReadRejection,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseRequest {
    #[serde(deserialize_with = "game_peers::object")]
    request: ReadRequest,
    #[serde(deserialize_with = "game_peers::object")]
    projection: ReadProjection,
    #[serde(deserialize_with = "game_peers::object")]
    binding: ReadBinding,
    challenge: operator_peers::OperatorChallenge,
    observed_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseReply {
    challenge: operator_peers::OperatorChallenge,
    request_digest: ReadDigest,
    checked_at: i64,
    valid_until: i64,
    #[serde(deserialize_with = "required_nullable")]
    code: Option<ReadRejection>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpiryRequest {
    #[serde(deserialize_with = "game_peers::object")]
    request: ReadRequest,
    #[serde(deserialize_with = "game_peers::object")]
    binding: ReadBinding,
    observed_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpiryReply {
    request_digest: ReadDigest,
}
fn required_nullable<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
fn failure(code: ReadRejection) -> Result<axum::response::Response, ApiError> {
    if matches!(code, ReadRejection::ResultNotCommitted) {
        return games::json_response(
            409,
            br#"{"error":{"code":"result_not_committed","message":"game result not committed"}}"#
                .to_vec(),
        );
    }
    if matches!(code, ReadRejection::RemainingValuesUnavailable) {
        return games::json_response(409,br#"{"error":{"code":"remaining_values_unavailable","message":"remaining values unavailable in final view"}}"#.to_vec());
    }
    games::rejected(match code {
        ReadRejection::Unauthorized => super::game_wire::GameRejection::Unauthorized,
        ReadRejection::Forbidden => super::game_wire::GameRejection::Forbidden,
        ReadRejection::NotFound => super::game_wire::GameRejection::NotFound,
        ReadRejection::InvalidInput => super::game_wire::GameRejection::InvalidInput,
        _ => super::game_wire::GameRejection::Unavailable,
    })
}
fn validate_cut(
    selection: &ReadSelection,
    projection: &ReadProjection,
    binding: &ReadBinding,
    checked_at: i64,
) -> Result<(), ApiError> {
    if selection.kind != projection.kind()
        || checked_at < 0
        || checked_at >= binding.valid_until
        || binding.valid_until > crate::limits::JS_SAFE_INTEGER_MAX
        || binding.host_assignment_revision < 0
        || binding.host_assignment_revision > crate::limits::JS_SAFE_INTEGER_MAX
    {
        return Err(ApiError::Unavailable);
    }
    let valid = match (&binding.identity, selection.role) {
        (
            ReadIdentity::Account {
                expires_at, epoch, ..
            },
            ReadRole::Account,
        )
        | (
            ReadIdentity::Player {
                expires_at, epoch, ..
            },
            ReadRole::Player,
        )
        | (
            ReadIdentity::Spectator {
                expires_at, epoch, ..
            },
            ReadRole::Spectator,
        ) => {
            *epoch >= 0
                && *epoch <= crate::limits::JS_SAFE_INTEGER_MAX
                && binding.valid_until <= *expires_at
                && *expires_at <= crate::limits::JS_SAFE_INTEGER_MAX
        }
        _ => false,
    };
    if !valid {
        return Err(ApiError::Unavailable);
    }
    projection
        .encode_json()
        .map_err(|_| ApiError::Unavailable)?;
    Ok(())
}
async fn fence_edge_expiry(
    env: &worker::Env,
    request: &ReadRequest,
    binding: &ReadBinding,
    observed_at: i64,
) -> Result<(), ApiError> {
    let expiry = ExpiryRequest {
        request: clone_request(request),
        binding: binding.clone(),
        observed_at,
    };
    let expected = digest(&expiry).map_err(|_| ApiError::Unavailable)?;
    let reply: ExpiryReply = game_peers::call(
        env,
        "GAMES",
        &request.selection.game_id.to_string(),
        "/read-expiry-denial",
        &expiry,
        READ_PEER_MAX_BYTES,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    if reply.request_digest != expected {
        return Err(ApiError::Unavailable);
    }
    Ok(())
}
fn clone_request(r: &ReadRequest) -> ReadRequest {
    ReadRequest {
        selection: r.selection,
        token: r.token.clone(),
        challenge: r.challenge.clone(),
    }
}
pub(super) async fn dispatch(
    env: &worker::Env,
    parts: &http::request::Parts,
    bytes: &[u8],
    origin: &str,
) -> Result<axum::response::Response, ApiError> {
    let decoded = match api::decode_game_read(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        bytes,
        origin,
    ) {
        Ok(v) => v,
        Err(e) => {
            if let Ok(Some((game, kind, token))) = api::read_rejection_context(
                &parts.method,
                parts.uri.path(),
                parts.uri.query(),
                &parts.headers,
            ) {
                super::operator_runtime::audit_rejected_transport(
                    env,
                    (game, read_purpose(kind), token),
                )
                .await?;
            }
            return Err(e);
        }
    };
    let request = ReadRequest {
        selection: decoded.selection,
        token: decoded.token.as_deref().unwrap_or("").to_owned(),
        challenge: operator_peers::fresh_challenge().map_err(|_| ApiError::Unavailable)?,
    };
    let expected = digest(&request).map_err(|_| ApiError::Unavailable)?;
    let reply: ReadReply = game_peers::call(
        env,
        "GAMES",
        &request.selection.game_id.to_string(),
        "/read",
        &request,
        READ_PEER_MAX_BYTES,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    let (projection, binding, checked_at) = match reply {
        ReadReply::Rejected {
            challenge,
            request_digest,
            code,
        } => {
            if challenge != request.challenge || request_digest != expected {
                return Err(ApiError::Unavailable);
            }
            return failure(code);
        }
        ReadReply::Ready {
            challenge,
            request_digest,
            projection,
            binding,
            checked_at,
        } => {
            if challenge != request.challenge || request_digest != expected {
                return Err(ApiError::Unavailable);
            }
            (projection, binding, checked_at)
        }
    };
    validate_cut(&request.selection, &projection, &binding, checked_at)?;
    // Every edge await is followed by an owner release. No renewal or entry is involved.
    let mut cap = binding.valid_until;
    for _ in 0..2 {
        let now = WorkerRuntime.now_ms();
        if now >= cap {
            fence_edge_expiry(
                env,
                &request,
                &ReadBinding {
                    valid_until: cap,
                    ..binding.clone()
                },
                now,
            )
            .await?;
            return failure(ReadRejection::Unauthorized);
        }
        if request.selection.role == ReadRole::Account {
            let authority = match operator_peers::authorize(env, &request.token).await {
                Err(game_peers::GamePeerError::Unauthorized) => {
                    return failure(ReadRejection::Unauthorized);
                }
                Err(_) => return Err(ApiError::Unavailable),
                Ok(operator_peers::OperatorAccountCut::Current(a)) => a,
                Ok(operator_peers::OperatorAccountCut::Expired { proof, observed_at }) => {
                    fence_edge_expiry(env, &request, &binding, observed_at).await?;
                    game_peers::fence_discovery_account_expiry(
                        env,
                        &request.token,
                        &proof,
                        observed_at,
                    )
                    .await
                    .map_err(|_| ApiError::Unavailable)?;
                    return failure(ReadRejection::Unauthorized);
                }
            };
            if account_identity(&authority) != binding.identity {
                return failure(ReadRejection::Unauthorized);
            }
        }
        let observed_at = WorkerRuntime.now_ms();
        // Persist an already observed expiry before attempting entropy for the next cut.
        if observed_at >= cap {
            fence_edge_expiry(
                env,
                &request,
                &ReadBinding {
                    valid_until: cap,
                    ..binding.clone()
                },
                observed_at,
            )
            .await?;
            return failure(ReadRejection::Unauthorized);
        }
        let release = ReleaseRequest {
            request: clone_request(&request),
            projection: projection.clone(),
            binding: ReadBinding {
                valid_until: cap,
                ..binding.clone()
            },
            challenge: operator_peers::fresh_challenge().map_err(|_| ApiError::Unavailable)?,
            observed_at,
        };
        let expected = digest(&release).map_err(|_| ApiError::Unavailable)?;
        let reply: ReleaseReply = game_peers::call(
            env,
            "GAMES",
            &request.selection.game_id.to_string(),
            "/read-release",
            &release,
            READ_PEER_MAX_BYTES,
        )
        .await
        .map_err(|_| ApiError::Unavailable)?;
        if reply.challenge != release.challenge
            || reply.request_digest != expected
            || reply.checked_at < observed_at
            || reply.checked_at > crate::limits::JS_SAFE_INTEGER_MAX
            || reply.valid_until <= 0
            || reply.valid_until > cap
        {
            return Err(ApiError::Unavailable);
        }
        cap = cap.min(reply.valid_until);
        let after = WorkerRuntime.now_ms();
        if after >= cap {
            fence_edge_expiry(
                env,
                &request,
                &ReadBinding {
                    valid_until: cap,
                    ..binding.clone()
                },
                after,
            )
            .await?;
            return failure(ReadRejection::Unauthorized);
        }
        if let Some(code) = reply.code {
            return failure(code);
        }
        if reply.checked_at >= cap {
            return Err(ApiError::Unavailable);
        }
    }
    games::json_response(
        200,
        projection
            .encode_json()
            .map_err(|_| ApiError::Unavailable)?,
    )
}
fn account_identity(a: &GameAccountAuthority) -> ReadIdentity {
    ReadIdentity::Account {
        account_id: a.account_id(),
        session_id: a.session_id(),
        account_role: a.role(),
        epoch: a.credential_epoch(),
        expires_at: a.expires_at(),
    }
}
impl GameObject {
    async fn check_read_deadline(
        &self,
        service: &GameService<'_, super::OwnerDatabase, WorkerRuntime>,
        binding: &ReadBinding,
    ) -> Result<(), GameError> {
        let now = service.read_clock()?;
        if now >= binding.valid_until {
            service.fence_operator_output_expiry(now)?;
            self.state
                .storage()
                .sync()
                .await
                .map_err(|_| GameError::Storage)?;
            return Err(GameError::Unauthorized);
        }
        Ok(())
    }

    async fn read_account(
        &self,
        service: &GameService<'_, super::OwnerDatabase, WorkerRuntime>,
        request: &ReadRequest,
    ) -> Result<Option<GameAccountAuthority>, GameError> {
        if request.selection.role == ReadRole::Account {
            self.authorize_operator_account(service, &request.token)
                .await
                .map(Some)
                .map_err(super::game_object::peer_error)
        } else {
            Ok(None)
        }
    }
    async fn read_initial(&self, request: &ReadRequest) -> Result<ReadCut, GameError> {
        if !self.owns_game(request.selection.game_id)? {
            return Err(GameError::NotFound);
        }
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        service.bind_operator_game_identity(request.selection.game_id)?;
        let actor = self.read_account(&service, request).await?;
        let initial = match self.observed(&service) {
            Ok(observed) => service.capture_read(
                request.selection,
                &request.token,
                actor.as_ref(),
                &observed,
                true,
            ),
            Err(error) => {
                self.capture_read_failure(&service, actor.as_ref(), request, error)?;
                Err(error)
            }
        };
        self.state
            .storage()
            .sync()
            .await
            .map_err(|_| GameError::Storage)?;
        let cut = initial?;
        self.check_read_deadline(&service, &cut.binding).await?;
        let actor = self.read_account(&service, request).await?;
        let released = self.observed(&service).and_then(|observed| {
            service.release_read(
                request.selection,
                &request.token,
                actor.as_ref(),
                &observed,
                &cut.binding,
                &cut.projection,
            )
        });
        match released {
            Ok((checked_at, valid_until)) => Ok(ReadCut {
                binding: ReadBinding {
                    valid_until,
                    ..cut.binding
                },
                checked_at,
                projection: cut.projection,
            }),
            Err(e) => {
                self.capture_read_failure(&service, actor.as_ref(), request, e)?;
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
                Err(e)
            }
        }
    }
    fn capture_read_failure(
        &self,
        service: &GameService<'_, super::OwnerDatabase, WorkerRuntime>,
        actor: Option<&GameAccountAuthority>,
        request: &ReadRequest,
        error: GameError,
    ) -> Result<(), GameError> {
        if let Some(a) = actor {
            if error == GameError::Storage {
                service.capture_operator_failure(a, read_purpose(request.selection.kind))?;
            } else {
                service.capture_operator_rejection(a, read_purpose(request.selection.kind))?;
            }
        }
        Ok(())
    }
    async fn read_release(&self, request: &ReleaseRequest) -> Result<(i64, i64), GameError> {
        if !self.owns_game(request.request.selection.game_id)?
            || request.projection.kind() != request.request.selection.kind
        {
            return Err(GameError::InvalidInput);
        }
        let db = self.database()?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        if request.observed_at >= request.binding.valid_until {
            service.fence_operator_output_expiry(request.observed_at)?;
            self.state
                .storage()
                .sync()
                .await
                .map_err(|_| GameError::Storage)?;
            return Err(GameError::Unauthorized);
        }
        self.check_read_deadline(&service, &request.binding).await?;
        let actor = self.read_account(&service, &request.request).await?;
        let initial = self.observed(&service).and_then(|observed| {
            service.release_read(
                request.request.selection,
                &request.request.token,
                actor.as_ref(),
                &observed,
                &request.binding,
                &request.projection,
            )
        });
        if let Err(error) = initial {
            self.capture_read_failure(&service, actor.as_ref(), &request.request, error)?;
        }
        self.state
            .storage()
            .sync()
            .await
            .map_err(|_| GameError::Storage)?;
        initial?;
        self.check_read_deadline(&service, &request.binding).await?;
        let actor = self.read_account(&service, &request.request).await?;
        let final_cut = self.observed(&service).and_then(|observed| {
            service.release_read(
                request.request.selection,
                &request.request.token,
                actor.as_ref(),
                &observed,
                &request.binding,
                &request.projection,
            )
        });
        match final_cut {
            Ok(c) => Ok(c),
            Err(e) => {
                self.capture_read_failure(&service, actor.as_ref(), &request.request, e)?;
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
                Err(e)
            }
        }
    }
    pub(super) async fn execute_read_transport(
        &self,
        request: worker::Request,
        path: &str,
    ) -> worker::Result<worker::Response> {
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = Zeroizing::new(
            axum::body::to_bytes(axum::body::Body::new(http.into_body()), READ_PEER_MAX_BYTES)
                .await
                .map_err(|_| worker::Error::RustError("read request bound".into()))?
                .to_vec(),
        );
        let result = match path {
            "/read" => {
                let r: ReadRequest = super::decode_private_json(&bytes)
                    .map_err(|_| worker::Error::RustError("invalid read request".into()))?;
                let request_digest = digest(&r)
                    .map_err(|_| worker::Error::RustError("read binding unavailable".into()))?;
                let result = self.read_initial(&r).await;
                let reply = match result {
                    Ok(c) => ReadReply::Ready {
                        challenge: r.challenge.clone(),
                        request_digest,
                        projection: c.projection,
                        binding: c.binding,
                        checked_at: c.checked_at,
                    },
                    Err(e) => ReadReply::Rejected {
                        challenge: r.challenge.clone(),
                        request_digest,
                        code: e.into(),
                    },
                };
                game_peers::encode_limit(&reply, READ_PEER_MAX_BYTES)
            }
            "/read-release" => {
                let r: ReleaseRequest = super::decode_private_json(&bytes)
                    .map_err(|_| worker::Error::RustError("invalid read release".into()))?;
                let request_digest = digest(&r)
                    .map_err(|_| worker::Error::RustError("read binding unavailable".into()))?;
                let result = self.read_release(&r).await;
                let (checked_at, valid_until, code) = match result {
                    Ok((t, d)) => (t, d, None),
                    Err(e) => {
                        if matches!(e, GameError::Storage | GameError::RandomUnavailable) {
                            return worker::Response::error("read cut unavailable", 503);
                        }
                        let db = self.database().map_err(|_| {
                            worker::Error::RustError("read denial unavailable".into())
                        })?;
                        let rt = WorkerRuntime;
                        let service = self.service(&db, &rt).map_err(|_| {
                            worker::Error::RustError("read denial unavailable".into())
                        })?;
                        let now = service.read_clock().map_err(|_| {
                            worker::Error::RustError("read denial clock unavailable".into())
                        })?;
                        service.fence_operator_output_expiry(now).map_err(|_| {
                            worker::Error::RustError("read denial unavailable".into())
                        })?;
                        self.state.storage().sync().await?;
                        (now, r.binding.valid_until, Some(e.into()))
                    }
                };
                let reply = ReleaseReply {
                    challenge: r.challenge.clone(),
                    request_digest,
                    checked_at,
                    valid_until,
                    code,
                };
                game_peers::encode_limit(&reply, READ_PEER_MAX_BYTES)
            }
            "/read-expiry-denial" => {
                let r: ExpiryRequest = super::decode_private_json(&bytes)
                    .map_err(|_| worker::Error::RustError("invalid read denial".into()))?;
                if !self
                    .owns_game(r.request.selection.game_id)
                    .map_err(|_| worker::Error::RustError("read owner unavailable".into()))?
                    || r.observed_at < r.binding.valid_until
                {
                    return worker::Response::error("invalid read denial", 400);
                }
                let db = self
                    .database()
                    .map_err(|_| worker::Error::RustError("read storage unavailable".into()))?;
                let rt = WorkerRuntime;
                let service = self
                    .service(&db, &rt)
                    .map_err(|_| worker::Error::RustError("read service unavailable".into()))?;
                service
                    .fence_operator_output_expiry(r.observed_at)
                    .map_err(|_| worker::Error::RustError("read denial unavailable".into()))?;
                self.state.storage().sync().await?;
                if let ReadIdentity::Account {
                    account_id,
                    session_id,
                    account_role,
                    epoch,
                    expires_at,
                } = r.binding.identity
                    && r.observed_at >= expires_at
                {
                    let a = GameAccountAuthority::from_trusted_peer(
                        account_id,
                        session_id,
                        account_role,
                        epoch,
                        expires_at,
                    )
                    .map_err(|_| worker::Error::RustError("read account unavailable".into()))?;
                    game_peers::fence_discovery_account_expiry(
                        &self.env,
                        &r.request.token,
                        &a,
                        r.observed_at,
                    )
                    .await
                    .map_err(|_| {
                        worker::Error::RustError("read account fence unavailable".into())
                    })?;
                }
                game_peers::encode_limit(
                    &ExpiryReply {
                        request_digest: digest(&r).map_err(|_| {
                            worker::Error::RustError("read denial binding unavailable".into())
                        })?,
                    },
                    READ_PEER_MAX_BYTES,
                )
            }
            _ => return worker::Response::error("read path unavailable", 404),
        }
        .map_err(|_| worker::Error::RustError("read response bound".into()))?;
        worker::Response::from_json(
            &serde_json::from_slice::<serde_json::Value>(&result)
                .map_err(|_| worker::Error::RustError("read output unavailable".into()))?,
        )
    }
}
