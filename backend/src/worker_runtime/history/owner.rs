//! Game-owner History adapter, independent of terminal view admission and grants.
use super::super::{WorkerRuntime, game_object::GameObject, game_peers};
use super::wire::{
    HISTORY_REPLY_MAX_BYTES, HistoryOwnerOutcome, HistoryOwnerReply, HistoryOwnerRequest,
};
use crate::game::GameError;
impl GameObject {
    pub(in crate::worker_runtime) async fn execute_history(
        &self,
        request: HistoryOwnerRequest,
    ) -> worker::Result<worker::Response> {
        let game_id = request.game_id;
        let identified_role = std::cell::Cell::new(None);
        let needs_capture = std::cell::Cell::new(true);
        let mut result = async {
            if !self.owns_game(game_id)? {
                return Err(GameError::Storage);
            }
            let db = self.database()?;
            let service = self.service(&db, &WorkerRuntime)?;
            let proof = game_peers::authorize_account(&self.env, request.token.as_str())
                .await
                .map_err(|e| match e {
                    game_peers::GamePeerError::Unauthorized => GameError::Unauthorized,
                    _ => GameError::Storage,
                })?;
            identified_role.set(Some(proof.role()));
            let observed = service.history(proof, game_id);
            if identified_role.get() == Some(brews_domain::accounts::AccountRole::Admin) {
                self.persist(&service, &db).await?;
            } else {
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GameError::Storage)?;
            }
            if observed.is_err() {
                needs_capture.set(false);
            }
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
        if let Err(error) = &result
            && error.is_execution_failure()
            && needs_capture.get()
            && identified_role.get() != Some(brews_domain::accounts::AccountRole::Host)
        {
            let captured = async {
                if !self.owns_game(game_id)? {
                    return Err(GameError::Storage);
                }
                let db = self.database()?;
                let service = self.service(&db, &WorkerRuntime)?;
                service.bind_operator_game_identity(game_id)?;
                self.capture_operator_peer_error(
                    &service,
                    &db,
                    request.token.as_str(),
                    crate::db::game::OperatorPurpose::HistoryRead,
                    *error,
                )
                .await
            }
            .await;
            if captured.is_err() {
                result = Err(GameError::Storage);
            }
        }
        let outcome = result.unwrap_or_else(|e| match e {
            GameError::NotFound | GameError::Expired => HistoryOwnerOutcome::NotFound {},
            GameError::Unauthorized => HistoryOwnerOutcome::Unauthorized {},
            _ => HistoryOwnerOutcome::Unavailable {},
        });
        let bytes = game_peers::encode_limit(
            &HistoryOwnerReply { game_id, outcome },
            HISTORY_REPLY_MAX_BYTES,
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
