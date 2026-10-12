//! Authoritative read cuts after Directory and Accounts awaits, without view admission.
use super::super::{WorkerRuntime, game_object::GameObject, game_peers};
use super::wire::{
    DISCOVERY_REPLY_MAX_BYTES, DiscoveryAction, DiscoveryOutcome, DiscoveryOwnerReply,
    DiscoveryOwnerRequest,
};
use crate::{
    auth::{GameAccountAuthority, Runtime},
    db::game::{DiscoveryValue, DiscoveryViewer},
    game::GameError,
};
use brews_contracts::discovery::GameSlot;

fn peer_error(error: game_peers::GamePeerError) -> GameError {
    match error {
        game_peers::GamePeerError::Unauthorized => GameError::Unauthorized,
        _ => GameError::Storage,
    }
}
pub(super) async fn route(
    env: &worker::Env,
) -> Result<Option<crate::directory::games::DiscoveryRoute>, GameError> {
    match game_peers::directory_reply(
        env,
        super::super::game_directory::DirectoryGameRequest::Discovery {},
    )
    .await
    .map_err(peer_error)?
    {
        super::super::game_directory::DirectoryGameOutcome::Discovery { route } => Ok(route),
        _ => Err(GameError::Storage),
    }
}
pub(super) fn same_authority(a: &GameAccountAuthority, b: &GameAccountAuthority) -> bool {
    a.account_id() == b.account_id()
        && a.session_id() == b.session_id()
        && a.role() == b.role()
        && a.credential_epoch() == b.credential_epoch()
        && a.expires_at() == b.expires_at()
}
impl GameObject {
    pub(in crate::worker_runtime) async fn execute_discovery(
        &self,
        request: DiscoveryOwnerRequest,
    ) -> worker::Result<worker::Response> {
        let result = self.discovery_read(&request).await;
        let reply = result.unwrap_or_else(|error| DiscoveryOwnerReply {
            game_id: request.game_id,
            checked_at: WorkerRuntime.now_ms(),
            valid_until: None,
            outcome: match error {
                GameError::Unauthorized | GameError::Forbidden => DiscoveryOutcome::Unauthorized {},
                GameError::NotFound | GameError::Expired => DiscoveryOutcome::NotFound {},
                _ => DiscoveryOutcome::Unavailable {},
            },
        });
        let bytes = game_peers::encode_limit(&reply, DISCOVERY_REPLY_MAX_BYTES)
            .map_err(|_| worker::Error::RustError("discovery response bound".into()))?;
        let mut response = worker::Response::from_bytes(bytes.to_vec())?;
        response
            .headers_mut()
            .set("Content-Type", "application/json")?;
        response.headers_mut().set("Cache-Control", "no-store")?;
        Ok(response)
    }
    async fn discovery_read(
        &self,
        request: &DiscoveryOwnerRequest,
    ) -> Result<DiscoveryOwnerReply, GameError> {
        if !self.owns_game(request.game_id)? {
            return Err(GameError::Storage);
        }
        let db = self.database()?;
        let service = self.service(&db, &WorkerRuntime)?;
        if let Some(expiry) = request.expiry {
            service.fence_discovery_expiry(request.game_id, expiry.deadline, expiry.observed_at)?;
            self.state
                .storage()
                .sync()
                .await
                .map_err(|_| GameError::Storage)?;
            return Ok(DiscoveryOwnerReply {
                game_id: request.game_id,
                checked_at: WorkerRuntime.now_ms(),
                valid_until: None,
                outcome: DiscoveryOutcome::ExpiryFenced { expiry },
            });
        }
        let account_token = match &request.action {
            DiscoveryAction::Summary { token } | DiscoveryAction::Account { token } => {
                Some(token.as_str())
            }
            _ => None,
        };
        let initial = match account_token {
            Some(token) => Some(
                game_peers::authorize_discovery_account(&self.env, token)
                    .await
                    .map_err(peer_error)?,
            ),
            None => None,
        };
        // Hydrate before proving the reservation. This first cut is never released.
        let first_view = match &request.action {
            DiscoveryAction::Summary { .. } | DiscoveryAction::Account { .. } => {
                DiscoveryViewer::Summary(initial.as_ref().ok_or(GameError::Unauthorized)?)
            }
            DiscoveryAction::Entry { game_code } => DiscoveryViewer::Entry(game_code),
            DiscoveryAction::Player { token } => DiscoveryViewer::Player(token.as_str()),
            DiscoveryAction::Spectator { token } => DiscoveryViewer::Spectator(token.as_str()),
        };
        let first_code = match &request.action {
            DiscoveryAction::Entry { game_code } => Some(game_code),
            _ => None,
        };
        let first = service.discovery(request.game_id, first_view, first_code, true, &[]);
        // The output gate makes observed denial fences durable; no timers/grants are changed.
        self.state
            .storage()
            .sync()
            .await
            .map_err(|_| GameError::Storage)?;
        if let Err(error) = first
            && !matches!(error, GameError::Expired | GameError::NotFound)
        {
            return Err(error);
        }
        // At most one recut after a final-cut denial needs a durability await.
        for attempt in 0..2 {
            let routing = route(&self.env).await?;
            let selected = routing
                .as_ref()
                .filter(|r| r.game_id == request.game_id)
                .ok_or(
                    if matches!(request.action, DiscoveryAction::Summary { .. }) {
                        GameError::Storage
                    } else {
                        GameError::NotFound
                    },
                )?;
            let current = match account_token {
                Some(token) => {
                    let proof = game_peers::authorize_discovery_account(&self.env, token)
                        .await
                        .map_err(peer_error)?;
                    if !initial.as_ref().is_some_and(|a| same_authority(a, &proof)) {
                        return Err(GameError::Unauthorized);
                    }
                    Some(proof)
                }
                None => None,
            };
            let view = match &request.action {
                DiscoveryAction::Summary { .. } => {
                    DiscoveryViewer::Summary(current.as_ref().ok_or(GameError::Unauthorized)?)
                }
                DiscoveryAction::Account { .. } => {
                    DiscoveryViewer::Account(current.as_ref().ok_or(GameError::Unauthorized)?)
                }
                DiscoveryAction::Entry { game_code } => DiscoveryViewer::Entry(game_code),
                DiscoveryAction::Player { token } => DiscoveryViewer::Player(token.as_str()),
                DiscoveryAction::Spectator { token } => DiscoveryViewer::Spectator(token.as_str()),
            };
            let observed = if matches!(request.action, DiscoveryAction::Account { .. }) {
                self.discovery_observed()?
            } else {
                Vec::new()
            };
            let final_cut = service.discovery(
                request.game_id,
                view,
                selected.published_code.as_ref(),
                !selected.releasing,
                &observed,
            );
            match final_cut {
                Ok(cut) => {
                    if cut.requires_sync {
                        self.state
                            .storage()
                            .sync()
                            .await
                            .map_err(|_| GameError::Storage)?;
                        if attempt == 0 {
                            continue;
                        }
                        return Err(GameError::Storage);
                    }
                    let outcome = match cut.value {
                        DiscoveryValue::Summary(game) => {
                            let slot = if selected.releasing || cut.releasing {
                                GameSlot::Releasing
                            } else if !selected.initialized {
                                GameSlot::Preparing
                            } else if game.is_none() {
                                GameSlot::Releasing
                            } else {
                                GameSlot::Occupied
                            };
                            DiscoveryOutcome::Current {
                                slot,
                                game: (slot == GameSlot::Occupied).then_some(game).flatten(),
                            }
                        }
                        DiscoveryValue::Entry(entry) => DiscoveryOutcome::Entry { entry },
                        DiscoveryValue::Availability(availability) => {
                            DiscoveryOutcome::Availability { availability }
                        }
                    };
                    return Ok(DiscoveryOwnerReply {
                        game_id: request.game_id,
                        checked_at: cut.checked_at,
                        valid_until: cut.valid_until,
                        outcome,
                    });
                }
                Err(error @ (GameError::Expired | GameError::Unauthorized)) => {
                    self.state
                        .storage()
                        .sync()
                        .await
                        .map_err(|_| GameError::Storage)?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(GameError::Storage)
    }
}
