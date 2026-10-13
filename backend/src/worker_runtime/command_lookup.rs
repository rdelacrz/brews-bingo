//! Exact current-account Directory intent lookup before Game initialization.
use super::game_reads::{
    READ_PEER_MAX_BYTES, ReadDigest, ReadRequest, account_identity, clone_request, digest,
};
use super::{OwnerDatabase, WorkerRuntime, game_object::GameObject, game_peers, operator_peers};
use crate::{
    api::ReadRole,
    auth::{GameAccountAuthority, Runtime},
    db::game::{GameService, ReadIdentity},
    game::GameError,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;
fn required_nullable<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreationLookupRequest {
    #[serde(deserialize_with = "game_peers::object")]
    request: ReadRequest,
    challenge: operator_peers::OperatorChallenge,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreationLookupReply {
    challenge: operator_peers::OperatorChallenge,
    request_digest: ReadDigest,
    #[serde(deserialize_with = "game_peers::object")]
    identity: ReadIdentity,
    #[serde(deserialize_with = "required_nullable")]
    created_at: Option<i64>,
}
pub(super) async fn creation_pending(
    env: &worker::Env,
    request: &ReadRequest,
    authority: &GameAccountAuthority,
) -> Result<Option<i64>, GameError> {
    let request = CreationLookupRequest {
        request: clone_request(request),
        challenge: operator_peers::fresh_challenge().map_err(|_| GameError::RandomUnavailable)?,
    };
    let expected = digest(&request)?;
    let reply: CreationLookupReply = game_peers::call(
        env,
        "GAME_DIRECTORY",
        "directory",
        "/command-creation",
        &request,
        READ_PEER_MAX_BYTES,
    )
    .await
    .map_err(super::game_object::peer_error)?;
    if reply.challenge != request.challenge
        || reply.request_digest != expected
        || reply.identity != account_identity(authority)
    {
        return Err(GameError::Storage);
    }
    Ok(reply.created_at)
}
impl GameObject {
    pub(super) async fn persist_creation_audit_alarm(
        &self,
        service: &GameService<'_, OwnerDatabase, WorkerRuntime>,
    ) -> Result<(), GameError> {
        // No game/presence reconciliation or sender drain: only arm retained owner cleanup.
        if let Some(deadline) = service.next_deadline()? {
            let storage = self.state.storage();
            let alarm = storage.get_alarm().await.map_err(|_| GameError::Storage)?;
            let due = deadline.max(
                WorkerRuntime
                    .now_ms()
                    .saturating_add(crate::game::RETRY_INITIAL_MS),
            );
            if alarm.is_none_or(|current| due < current) {
                storage
                    .set_alarm(worker::ScheduledTime::new(js_sys::Date::new(
                        &wasm_bindgen::JsValue::from_f64(due as f64),
                    )))
                    .await
                    .map_err(|_| GameError::Storage)?;
            }
            storage.sync().await.map_err(|_| GameError::Storage)?;
        }
        Ok(())
    }
}
impl super::directory::GameDirectoryObject {
    pub(super) async fn execute_command_creation_lookup(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        if request.method() != worker::Method::Post || request.url()?.query().is_some() {
            return worker::Response::error("invalid command lookup", 400);
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = Zeroizing::new(
            axum::body::to_bytes(axum::body::Body::new(http.into_body()), READ_PEER_MAX_BYTES)
                .await
                .map_err(|_| worker::Error::RustError("command lookup bound".into()))?
                .to_vec(),
        );
        let request: CreationLookupRequest = super::decode_private_json(&bytes)
            .map_err(|_| worker::Error::RustError("invalid command lookup".into()))?;
        let brews_contracts::games::reads::ReadKind::CommandLookup(command) =
            request.request.selection.kind
        else {
            return worker::Response::error("invalid command lookup", 400);
        };
        if request.request.selection.role != ReadRole::Account
            || request.request.selection.player_id.is_some()
        {
            return worker::Response::error("invalid command lookup", 400);
        }
        let db = super::OwnerDatabase::new(self.state.storage());
        crate::db::directory::migrate_directory(&db)
            .map_err(|_| worker::Error::RustError("command storage unavailable".into()))?;
        self.state.storage().sync().await?;
        let proof = match operator_peers::authorize(&self.env, &request.request.token)
            .await
            .map_err(|_| worker::Error::RustError("command authority unavailable".into()))?
        {
            operator_peers::OperatorAccountCut::Current(a) => a,
            operator_peers::OperatorAccountCut::Expired { proof, observed_at } => {
                game_peers::fence_discovery_account_expiry(
                    &self.env,
                    &request.request.token,
                    &proof,
                    observed_at,
                )
                .await
                .map_err(|_| worker::Error::RustError("command expiry unavailable".into()))?;
                return worker::Response::error("command authority expired", 401);
            }
        };
        let service = crate::db::directory::DirectoryService::new(&db, &WorkerRuntime)
            .map_err(|_| worker::Error::RustError("command storage unavailable".into()))?;
        let created = service
            .command_creation_pending(
                proof.account_id(),
                request.request.selection.game_id,
                command,
            )
            .map_err(|_| worker::Error::RustError("command storage unavailable".into()))?;
        self.state.storage().sync().await?;
        let fresh = match operator_peers::authorize(&self.env, &request.request.token)
            .await
            .map_err(|_| worker::Error::RustError("command authority unavailable".into()))?
        {
            operator_peers::OperatorAccountCut::Current(a) => a,
            operator_peers::OperatorAccountCut::Expired { proof, observed_at } => {
                game_peers::fence_discovery_account_expiry(
                    &self.env,
                    &request.request.token,
                    &proof,
                    observed_at,
                )
                .await
                .map_err(|_| worker::Error::RustError("command expiry unavailable".into()))?;
                return worker::Response::error("command authority expired", 401);
            }
        };
        if account_identity(&proof) != account_identity(&fresh) {
            return worker::Response::error("command authority changed", 401);
        }
        let current = service
            .command_creation_pending(
                fresh.account_id(),
                request.request.selection.game_id,
                command,
            )
            .map_err(|_| worker::Error::RustError("command storage unavailable".into()))?;
        if current != created {
            return worker::Response::error("command intent changed", 503);
        }
        // Checked clock maintenance is durable before the reply. The final return remains RUNTIME-FINAL-01.
        self.state.storage().sync().await?;
        let reply = CreationLookupReply {
            challenge: request.challenge.clone(),
            request_digest: digest(&request)
                .map_err(|_| worker::Error::RustError("command binding unavailable".into()))?,
            identity: account_identity(&fresh),
            created_at: current,
        };
        let bytes = game_peers::encode_limit(&reply, READ_PEER_MAX_BYTES)
            .map_err(|_| worker::Error::RustError("command response bound".into()))?;
        worker::Response::from_bytes(bytes.to_vec())
    }
}
