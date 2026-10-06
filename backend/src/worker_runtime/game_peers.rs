//! Closed private game peer protocol. Never accepted at public ingress.
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub(super) const GAME_PEER_MAX_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum GamePeerError {
    #[error("invalid private game request")]
    InvalidInput,
    #[error("game peer unavailable")]
    Unavailable,
    #[error("unauthorized")]
    Unauthorized,
    #[error("game coordination conflict")]
    Conflict,
}

pub(super) fn encode<T: Serialize>(value: &T) -> Result<Zeroizing<Vec<u8>>, GamePeerError> {
    encode_limit(value, GAME_PEER_MAX_BYTES)
}
pub(super) fn encode_limit<T: Serialize>(
    value: &T,
    limit: usize,
) -> Result<Zeroizing<Vec<u8>>, GamePeerError> {
    struct BoundedWriter {
        bytes: Zeroizing<Vec<u8>>,
        limit: usize,
    }
    impl std::io::Write for BoundedWriter {
        fn write(&mut self, part: &[u8]) -> std::io::Result<usize> {
            if part.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("private payload bound"));
            }
            self.bytes.extend_from_slice(part);
            Ok(part.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = BoundedWriter {
        bytes: Zeroizing::new(Vec::with_capacity(limit)),
        limit,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| GamePeerError::Unavailable)?;
    Ok(writer.bytes)
}
pub(super) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, GamePeerError> {
    decode_limit(bytes, GAME_PEER_MAX_BYTES)
}
pub(super) fn decode_limit<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    limit: usize,
) -> Result<T, GamePeerError> {
    if bytes.len() > limit || bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err(GamePeerError::InvalidInput);
    }
    serde_json::from_slice(bytes).map_err(|_| GamePeerError::InvalidInput)
}

pub(super) fn object<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<T, D::Error> {
    struct MapVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for MapVisitor<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a JSON object")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    d.deserialize_map(MapVisitor(std::marker::PhantomData))
}
pub(super) fn optional_object<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    struct OptionalMap<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for OptionalMap<T> {
        type Value = Option<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an object or null")
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            object(d).map(Some)
        }
    }
    d.deserialize_option(OptionalMap(std::marker::PhantomData))
}
pub(super) fn safe_integer<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let value = i64::deserialize(d)?;
    if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&value) {
        return Err(serde::de::Error::custom("unsafe integer"));
    }
    Ok(value)
}
pub(super) fn positive_integer<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let value = safe_integer(d)?;
    if value == 0 {
        return Err(serde::de::Error::custom("nonpositive integer"));
    }
    Ok(value)
}
pub(super) fn optional_integer<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<i64>, D::Error> {
    let value = Option::<i64>::deserialize(d)?;
    if value.is_some_and(|v| !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&v)) {
        return Err(serde::de::Error::custom("unsafe integer"));
    }
    Ok(value)
}

pub(super) const GAME_PEER_WORK_LIMIT: u32 = 100;
pub(super) const GAME_PEER_BATCH_MAX_BYTES: usize = 128 * 1024;
pub(super) fn bounded_objects<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    struct Element<T>(T);
    impl<'de, T: Deserialize<'de>> Deserialize<'de> for Element<T> {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            object(d).map(Self)
        }
    }
    struct List<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for List<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 100 metadata objects")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut a: A,
        ) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(value) = a.next_element::<Element<T>>()? {
                if out.len() >= GAME_PEER_WORK_LIMIT as usize {
                    return Err(serde::de::Error::custom("work bound exceeded"));
                }
                out.push(value.0);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(List(std::marker::PhantomData))
}

pub(super) struct SecretToken(Zeroizing<String>);
impl Serialize for SecretToken {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.as_str().serialize(s)
    }
}
impl<'de> Deserialize<'de> for SecretToken {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(|value| Self(Zeroizing::new(value)))
    }
}
impl std::fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}
impl SecretToken {
    pub(super) fn new(value: &str) -> Self {
        Self(Zeroizing::new(value.to_owned()))
    }
    pub(super) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(target_arch = "wasm32")]
use super::{
    game_accounts::{
        self, AccountCloseReply, AccountCloseRequest, AccountConnectionIdentity, AccountReply,
        AccountRequest,
    },
    game_directory::{self, DirectoryGameOutcome, DirectoryGameReply, DirectoryGameRequest},
};
#[cfg(target_arch = "wasm32")]
use crate::{
    auth::{GameAccountAuthority, Runtime},
    directory::games::*,
};
#[cfg(target_arch = "wasm32")]
use brews_domain::ids::{CommandId, ConnectionId, GameId};

#[cfg(target_arch = "wasm32")]
async fn call<T: serde::de::DeserializeOwned>(
    env: &worker::Env,
    binding: &str,
    name: &str,
    path: &str,
    message: &impl Serialize,
    response_limit: usize,
) -> Result<T, GamePeerError> {
    use axum::body::{Body, to_bytes};
    use worker::{Headers, Method, Request, RequestInit};
    let namespace = env
        .durable_object(binding)
        .map_err(|_| GamePeerError::Unavailable)?;
    let stub = namespace
        .id_from_name(name)
        .and_then(|id| id.get_stub())
        .map_err(|_| GamePeerError::Unavailable)?;
    let body = encode(message)?;
    let body = std::str::from_utf8(&body).map_err(|_| GamePeerError::Unavailable)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(body.into()));
    let headers = Headers::new();
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| GamePeerError::Unavailable)?;
    init.with_headers(headers);
    let request = Request::new_with_init(&format!("https://peer.internal{path}"), &init)
        .map_err(|_| GamePeerError::Unavailable)?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| GamePeerError::Unavailable)?;
    if response.status_code() != 200 {
        return Err(GamePeerError::Unavailable);
    }
    let http: worker::HttpResponse = response
        .try_into()
        .map_err(|_| GamePeerError::Unavailable)?;
    let bytes = Zeroizing::new(
        to_bytes(Body::new(http.into_body()), response_limit)
            .await
            .map_err(|_| GamePeerError::Unavailable)?
            .to_vec(),
    );
    decode_limit(&bytes, response_limit).map_err(|_| GamePeerError::Unavailable)
}
#[cfg(target_arch = "wasm32")]
async fn account_reply(
    env: &worker::Env,
    request: &AccountRequest,
) -> Result<AccountReply, GamePeerError> {
    call(
        env,
        "ACCOUNTS",
        "accounts",
        "/game-authority",
        request,
        GAME_PEER_MAX_BYTES,
    )
    .await
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn authorize_account(
    env: &worker::Env,
    token: &str,
) -> Result<GameAccountAuthority, GamePeerError> {
    let request = AccountRequest::Authorize {
        token: SecretToken::new(token),
    };
    let reply = account_reply(env, &request).await?;
    game_accounts::verified_authority(&request, reply, super::WorkerRuntime.now_ms())
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn register_account_connection(
    env: &worker::Env,
    token: &str,
    game_id: GameId,
    connection_id: ConnectionId,
) -> Result<GameAccountAuthority, GamePeerError> {
    let request = AccountRequest::Register {
        token: SecretToken::new(token),
        game_id,
        connection_id,
    };
    let reply = account_reply(env, &request).await?;
    game_accounts::verified_authority(&request, reply, super::WorkerRuntime.now_ms())
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn authorize_account_connection(
    env: &worker::Env,
    identity: &AccountConnectionIdentity,
) -> Result<GameAccountAuthority, GamePeerError> {
    let request = AccountRequest::AuthorizeConnection {
        identity: *identity,
    };
    let reply = account_reply(env, &request).await?;
    game_accounts::verified_authority(&request, reply, super::WorkerRuntime.now_ms())
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn unregister_account_connection(
    env: &worker::Env,
    identity: &AccountConnectionIdentity,
) -> Result<(), GamePeerError> {
    let request = AccountRequest::Unregister {
        identity: *identity,
    };
    let reply = account_reply(env, &request).await?;
    game_accounts::verified_unregistered(&request, reply)
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn call_game_close(
    env: &worker::Env,
    request: &AccountCloseRequest,
) -> Result<AccountCloseReply, GamePeerError> {
    // Exact existing work GameId is the namespace name; no code lookup creates an object.
    call(
        env,
        "GAMES",
        &request.target.game_id.to_string(),
        "/close-account",
        request,
        GAME_PEER_MAX_BYTES,
    )
    .await
}
#[cfg(target_arch = "wasm32")]
async fn directory_reply(
    env: &worker::Env,
    request: DirectoryGameRequest,
) -> Result<DirectoryGameOutcome, GamePeerError> {
    let limit = if matches!(request, DirectoryGameRequest::DueWork { .. }) {
        GAME_PEER_BATCH_MAX_BYTES
    } else {
        GAME_PEER_MAX_BYTES
    };
    let reply: DirectoryGameReply = call(
        env,
        "GAME_DIRECTORY",
        "directory",
        "/games",
        &request,
        limit,
    )
    .await?;
    game_directory::verify_reply(&request, reply)
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn claim_game(
    env: &worker::Env,
    authority: &GameAccountAuthority,
    command_id: CommandId,
    fingerprint: CreationFingerprint,
) -> Result<CreationWork, GamePeerError> {
    match directory_reply(
        env,
        DirectoryGameRequest::Claim {
            account_id: authority.account_id(),
            command_id,
            fingerprint: *fingerprint.digest(),
        },
    )
    .await?
    {
        DirectoryGameOutcome::Work { work } => work.work(),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn acknowledge_creation(
    env: &worker::Env,
    proof: &CreationReadyProof,
) -> Result<CreationAck, GamePeerError> {
    match directory_reply(
        env,
        DirectoryGameRequest::Acknowledge {
            ready: proof.into(),
        },
    )
    .await?
    {
        DirectoryGameOutcome::Acknowledged { ready } => ready
            .proof()
            .map(CreationAck::new)
            .map_err(|_| GamePeerError::Unavailable),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn allocate_game_code(
    env: &worker::Env,
    ack: CreationAck,
) -> Result<CodeGrant, GamePeerError> {
    match directory_reply(
        env,
        DirectoryGameRequest::AllocateCode {
            ready: (&ack.proof()).into(),
        },
    )
    .await?
    {
        DirectoryGameOutcome::CodeAllocated { ready, game_code } => ready
            .proof()
            .map(|p| CodeGrant::new(CreationAck::new(p), game_code))
            .map_err(|_| GamePeerError::Unavailable),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn publish_game(
    env: &worker::Env,
    projection: &GameProjection,
) -> Result<ProjectionAck, GamePeerError> {
    match directory_reply(
        env,
        DirectoryGameRequest::Publish {
            projection: projection.into(),
        },
    )
    .await?
    {
        DirectoryGameOutcome::Published {
            game_id,
            source_revision,
            published,
        } => Ok(ProjectionAck::new(game_id, source_revision, published)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn confirm_reservation(
    env: &worker::Env,
    game_id: GameId,
) -> Result<ReservationProof, GamePeerError> {
    match directory_reply(env, DirectoryGameRequest::ConfirmReservation { game_id }).await? {
        DirectoryGameOutcome::Reserved { game_id } => Ok(ReservationProof::new(game_id)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn release_game(
    env: &worker::Env,
    proof: &TerminalProof,
) -> Result<ReleaseAck, GamePeerError> {
    match directory_reply(
        env,
        DirectoryGameRequest::Release {
            projection: proof.projection().into(),
        },
    )
    .await?
    {
        DirectoryGameOutcome::Released { game_id } => Ok(ReleaseAck::new(game_id)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn creation_work(
    env: &worker::Env,
    game_id: GameId,
) -> Result<Option<CreationWork>, GamePeerError> {
    match directory_reply(env, DirectoryGameRequest::PendingProbe { game_id }).await? {
        DirectoryGameOutcome::Pending { work, .. } => work.map(|w| w.work()).transpose(),
        _ => Err(GamePeerError::Unavailable),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Private wire fixtures fail fast."
)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TokenRequest {
        token: SecretToken,
    }

    #[test]
    fn private_encoder_is_bounded_before_appending() {
        #[derive(Serialize)]
        struct Payload {
            value: String,
        }
        let small = Payload {
            value: "x".repeat(GAME_PEER_MAX_BYTES - 12),
        };
        let encoded = encode(&small).unwrap();
        assert_eq!(encoded.len(), GAME_PEER_MAX_BYTES);
        assert!(
            encode(&Payload {
                value: "x".repeat(GAME_PEER_MAX_BYTES - 11)
            })
            .is_err()
        );
    }
    #[test]
    fn private_original_bytes_accept_closed_object_not_sequence() {
        let wire: TokenRequest = decode(br#"{"token":"native-test-placeholder"}"#).unwrap();
        assert_eq!(wire.token.as_str(), "native-test-placeholder");
        for bytes in [
            br#"["native-test-placeholder"]"#.as_slice(),
            br#"{"token":"x","unknown":1}"#,
            br#"{"token":"x","token":"y"}"#,
            br#"{"token":"x","\u0074oken":"y"}"#,
        ] {
            assert!(decode::<TokenRequest>(bytes).is_err());
        }
        assert!(!format!("{:?}", wire.token).contains("native-test-placeholder"));
        assert!(decode::<TokenRequest>(&vec![b' '; GAME_PEER_MAX_BYTES + 1]).is_err());
    }
}
