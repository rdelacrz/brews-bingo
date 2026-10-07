//! Game-owner History adapter, independent of terminal view admission and grants.
use super::{
    WorkerRuntime,
    game_object::GameObject,
    game_peers,
    history_wire::{HistoryOwnerOutcome, HistoryOwnerReply, HistoryOwnerRequest},
};
use crate::game::GameError;
impl GameObject {
    pub(super) async fn execute_history(
        &self,
        request: HistoryOwnerRequest,
    ) -> worker::Result<worker::Response> {
        let game_id = request.game_id;
        let result = async {
            let db = self.database()?;
            let service = self.service(&db, &WorkerRuntime)?;
            let proof = game_peers::authorize_account(&self.env, request.token.as_str())
                .await
                .map_err(|e| match e {
                    game_peers::GamePeerError::Unauthorized => GameError::Unauthorized,
                    _ => GameError::Storage,
                })?;
            let observed = service.history(proof, game_id);
            self.state
                .storage()
                .sync()
                .await
                .map_err(|_| GameError::Storage)?;
            observed?;
            let current = game_peers::authorize_account(&self.env, request.token.as_str())
                .await
                .map_err(|e| match e {
                    game_peers::GamePeerError::Unauthorized => GameError::Unauthorized,
                    _ => GameError::Storage,
                })?;
            let history = match service.history_for_response(current, game_id) {
                Ok(history) => history,
                Err(GameError::NotFound) => {
                    // Only denial may await here; success must use the final Accounts proof.
                    self.state
                        .storage()
                        .sync()
                        .await
                        .map_err(|_| GameError::Storage)?;
                    return Err(GameError::NotFound);
                }
                Err(error) => return Err(error),
            };
            Ok::<_, GameError>(if request.summary {
                HistoryOwnerOutcome::Summary {
                    summary: history.summary(),
                }
            } else {
                HistoryOwnerOutcome::Detail { history }
            })
        }
        .await;
        let outcome = result.unwrap_or_else(|e| match e {
            GameError::NotFound | GameError::Expired => HistoryOwnerOutcome::NotFound {},
            GameError::Unauthorized => HistoryOwnerOutcome::Unauthorized {},
            _ => HistoryOwnerOutcome::Unavailable {},
        });
        let bytes = game_peers::encode_limit(
            &HistoryOwnerReply { game_id, outcome },
            super::history_wire::HISTORY_REPLY_MAX_BYTES,
        )
        .map_err(|_| worker::Error::RustError("History response bound".into()))?;
        let mut response = worker::Response::from_bytes(bytes.to_vec())?;
        response
            .headers_mut()
            .set("Content-Type", "application/json")?;
        response.headers_mut().set("Cache-Control", "no-store")?;
        Ok(response)
    }
}
