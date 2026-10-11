//! Directory final account cut: an honest free slot after the Accounts await.
use super::super::{OwnerDatabase, WorkerRuntime, directory::GameDirectoryObject, game_peers};
use super::wire::{DISCOVERY_REPLY_MAX_BYTES, DiscoveryDirectoryReply, DiscoveryDirectoryRequest};
use crate::{
    auth::Runtime,
    db::directory::{DirectoryService, migrate_directory},
};
impl GameDirectoryObject {
    pub(in crate::worker_runtime) async fn execute_discovery_directory(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        if request.method() != worker::Method::Post || request.url()?.query().is_some() {
            return worker::Response::error("invalid discovery route request", 400);
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            axum::body::to_bytes(
                axum::body::Body::new(http.into_body()),
                game_peers::GAME_PEER_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("discovery route bound".into()))?
            .to_vec(),
        );
        let request: DiscoveryDirectoryRequest = match game_peers::decode(&bytes) {
            Ok(r) => r,
            Err(_) => return worker::Response::error("invalid discovery route request", 400),
        };
        let db = OwnerDatabase::new(self.state.storage());
        migrate_directory(&db)
            .map_err(|_| worker::Error::RustError("Directory initialization failed".into()))?;
        // Drain migration writes before the final Accounts proof and reservation cut.
        self.state.storage().sync().await?;
        let reply = match game_peers::authorize_discovery_account(&self.env, request.token.as_str())
            .await
        {
            Ok(proof) => {
                let now = WorkerRuntime.now_ms();
                if proof.expires_at() <= now {
                    match game_peers::fence_discovery_account_expiry(
                        &self.env,
                        request.token.as_str(),
                        &proof,
                        now,
                    )
                    .await
                    {
                        Ok(()) => DiscoveryDirectoryReply::Unauthorized {},
                        Err(_) => DiscoveryDirectoryReply::Unavailable {},
                    }
                } else {
                    match DirectoryService::new(&db, &WorkerRuntime)
                        .and_then(|s| s.discovery_route())
                    {
                        Ok(route) => DiscoveryDirectoryReply::Route {
                            route,
                            checked_at: now,
                            valid_until: proof.expires_at(),
                        },
                        Err(_) => DiscoveryDirectoryReply::Unavailable {},
                    }
                }
            }
            Err(game_peers::GamePeerError::Unauthorized) => {
                DiscoveryDirectoryReply::Unauthorized {}
            }
            Err(_) => DiscoveryDirectoryReply::Unavailable {},
        };
        let bytes = game_peers::encode_limit(&reply, DISCOVERY_REPLY_MAX_BYTES)
            .map_err(|_| worker::Error::RustError("discovery route reply bound".into()))?;
        worker::Response::from_bytes(bytes.to_vec())
    }
}
