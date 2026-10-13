//! Closed private discovery messages; tokens are zeroized and never part of replies.
use super::super::game_peers::{
    GamePeerError, SecretToken, object, optional_integer, optional_object, positive_integer,
};
use brews_contracts::discovery::{CurrentGameSummary, GameAvailability, GameEntry, GameSlot};
use brews_domain::{
    games::{GameCode, GameState},
    ids::GameId,
};
use serde::{Deserialize, Serialize};

pub(in crate::worker_runtime) const DISCOVERY_REPLY_MAX_BYTES: usize = 4096;
#[derive(Serialize, Deserialize)]
#[serde(tag = "view", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::worker_runtime) enum DiscoveryAction {
    Summary { token: SecretToken },
    Entry { game_code: GameCode },
    Account { token: SecretToken },
    Player { token: SecretToken },
    Spectator { token: SecretToken },
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::worker_runtime) struct ObservedDiscoveryExpiry {
    #[serde(deserialize_with = "positive_integer")]
    pub deadline: i64,
    #[serde(deserialize_with = "positive_integer")]
    pub observed_at: i64,
}
/// Only the private edge chooses whether this invocation is the primary read.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::worker_runtime) enum DiscoveryReadPhase {
    Primary,
    Recut,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::worker_runtime) struct DiscoveryOwnerRequest {
    pub game_id: GameId,
    pub phase: DiscoveryReadPhase,
    #[serde(deserialize_with = "object")]
    pub action: DiscoveryAction,
    #[serde(default, deserialize_with = "optional_object")]
    pub expiry: Option<ObservedDiscoveryExpiry>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::worker_runtime) enum DiscoveryOutcome {
    Current {
        slot: GameSlot,
        #[serde(deserialize_with = "optional_object")]
        game: Option<CurrentGameSummary>,
    },
    Entry {
        #[serde(deserialize_with = "object")]
        entry: GameEntry,
    },
    Availability {
        #[serde(deserialize_with = "object")]
        availability: GameAvailability,
    },
    ExpiryFenced {
        #[serde(deserialize_with = "object")]
        expiry: ObservedDiscoveryExpiry,
    },
    NotFound {},
    Unauthorized {},
    Unavailable {},
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::worker_runtime) struct DiscoveryOwnerReply {
    pub game_id: GameId,
    #[serde(deserialize_with = "positive_integer")]
    pub checked_at: i64,
    #[serde(deserialize_with = "optional_integer")]
    pub valid_until: Option<i64>,
    #[serde(deserialize_with = "object")]
    pub outcome: DiscoveryOutcome,
}
fn live(state: GameState) -> bool {
    matches!(state, GameState::AwaitingPlayers | GameState::InProgress)
}
fn flags(state: GameState, player: bool, _spectator: bool) -> bool {
    !player || state == GameState::AwaitingPlayers
}
pub(in crate::worker_runtime) fn verify(
    request: &DiscoveryOwnerRequest,
    reply: &DiscoveryOwnerReply,
    now: i64,
) -> Result<(), GamePeerError> {
    if reply.game_id != request.game_id
        || reply.checked_at > now
        || !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&reply.checked_at)
        || reply.valid_until.is_some_and(|t| t <= reply.checked_at)
    {
        return Err(GamePeerError::Unavailable);
    }
    if let Some(expected) = request.expiry {
        return if (1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&expected.deadline)
            && (expected.deadline..=crate::limits::JS_SAFE_INTEGER_MAX)
                .contains(&expected.observed_at)
            && reply.valid_until.is_none()
            && (matches!(&reply.outcome, DiscoveryOutcome::ExpiryFenced { expiry } if *expiry == expected)
                || matches!(reply.outcome, DiscoveryOutcome::Unavailable {}))
        {
            Ok(())
        } else {
            Err(GamePeerError::Unavailable)
        };
    }
    let valid = match (&request.action, &reply.outcome) {
        (DiscoveryAction::Summary { .. }, DiscoveryOutcome::Current { slot, game }) => {
            reply.valid_until.is_some()
                && match (slot, game) {
                    (GameSlot::Occupied, Some(g)) => {
                        g.game_id == request.game_id
                            && matches!(
                                g.state,
                                GameState::New | GameState::AwaitingPlayers | GameState::InProgress
                            )
                            && (1..=reply.checked_at).contains(&g.created_at)
                            && g.started_at
                                .is_none_or(|t| (g.created_at..=reply.checked_at).contains(&t))
                            && (g.state == GameState::InProgress) == g.started_at.is_some()
                            && (g.state == GameState::InProgress) == g.idle_due.is_none()
                            && g.idle_due.is_none_or(|t| {
                                t > reply.checked_at && t <= crate::limits::JS_SAFE_INTEGER_MAX
                            })
                            && g.view_revision <= brews_contracts::games::MAX_SAFE_REVISION
                            && (g.state != GameState::New || g.game_code.is_none())
                    }
                    (GameSlot::Preparing | GameSlot::Releasing, None) => true,
                    _ => false,
                }
        }
        (DiscoveryAction::Entry { game_code }, DiscoveryOutcome::Entry { entry }) => {
            entry.game_id == request.game_id
                && &entry.game_code == game_code
                && live(entry.state)
                && flags(entry.state, entry.can_join_player, entry.can_join_spectator)
        }
        (
            DiscoveryAction::Account { .. },
            DiscoveryOutcome::Availability {
                availability:
                    GameAvailability::Account {
                        state,
                        can_join_player,
                        can_join_spectator,
                        player_count,
                        connected_player_count,
                        player_capacity,
                        spectator_count,
                        spectator_capacity,
                        can_start,
                    },
            },
        ) => {
            reply.valid_until.is_some()
                && live(*state)
                && flags(*state, *can_join_player, *can_join_spectator)
                && (2..=brews_domain::games::PLAYER_CAPACITY_MAX).contains(player_capacity)
                && player_count <= player_capacity
                && connected_player_count <= player_count
                && spectator_count <= spectator_capacity
                && *can_join_spectator == (spectator_count < spectator_capacity)
                && *spectator_capacity <= brews_domain::games::SPECTATOR_CAPACITY_MAX
                && (!*can_start
                    || (*state == GameState::AwaitingPlayers
                        && *connected_player_count >= crate::game::START_CONNECTED_PLAYER_MIN))
                && *can_join_player
                    == (*state == GameState::AwaitingPlayers && player_count < player_capacity)
        }
        (
            DiscoveryAction::Spectator { .. },
            DiscoveryOutcome::Availability {
                availability:
                    GameAvailability::Spectator {
                        state,
                        can_join_player,
                        can_join_spectator,
                    },
            },
        ) => {
            reply.valid_until.is_some()
                && live(*state)
                && flags(*state, *can_join_player, *can_join_spectator)
        }
        (
            DiscoveryAction::Player { .. },
            DiscoveryOutcome::Availability {
                availability:
                    GameAvailability::Player {
                        state,
                        can_join_player,
                        can_join_spectator,
                    },
            },
        ) => {
            reply.valid_until.is_some()
                && live(*state)
                && flags(*state, *can_join_player, *can_join_spectator)
        }
        (
            _,
            DiscoveryOutcome::NotFound {}
            | DiscoveryOutcome::Unauthorized {}
            | DiscoveryOutcome::Unavailable {},
        ) => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(GamePeerError::Unavailable)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::worker_runtime) struct DiscoveryDirectoryRequest {
    pub token: SecretToken,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::worker_runtime) enum DiscoveryDirectoryReply {
    Route {
        #[serde(deserialize_with = "optional_object")]
        route: Option<crate::directory::games::DiscoveryRoute>,
        #[serde(deserialize_with = "positive_integer")]
        checked_at: i64,
        #[serde(deserialize_with = "positive_integer")]
        valid_until: i64,
    },
    Unauthorized {},
    Unavailable {},
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Closed bounded fixture messages fail fast."
)]
mod tests {
    use super::*;
    fn request() -> DiscoveryOwnerRequest {
        DiscoveryOwnerRequest {
            game_id: "01900000-0000-7000-8000-000000000001".parse().unwrap(),
            phase: DiscoveryReadPhase::Primary,
            action: DiscoveryAction::Entry {
                game_code: "AB12CD34".parse().unwrap(),
            },
            expiry: None,
        }
    }
    #[test]
    fn discovery_private_original_bytes_reject_unknown_duplicate_positional_and_unsafe_fields() {
        let request = request();
        let original = serde_json::to_string(&request).unwrap();
        assert!(
            super::super::super::game_peers::decode::<DiscoveryOwnerRequest>(original.as_bytes())
                .is_ok()
        );
        for bad in [
            original.replace(
                "\"view\":\"entry\"",
                "\"view\":\"entry\",\"\\u0076iew\":\"entry\"",
            ),
            original.replace("\"game_code\":", "\"unknown\":1,\"game_code\":"),
            original.replace("AB12CD34", "ab12cd34"),
            original.replace(
                "\"action\":{\"view\":\"entry\",\"game_code\":\"AB12CD34\"}",
                "\"action\":[\"entry\",\"AB12CD34\"]",
            ),
        ] {
            assert!(
                super::super::super::game_peers::decode::<DiscoveryOwnerRequest>(bad.as_bytes())
                    .is_err()
            );
        }
        let reply = DiscoveryOwnerReply {
            game_id: request.game_id,
            checked_at: 1000,
            valid_until: Some(2000),
            outcome: DiscoveryOutcome::Entry {
                entry: GameEntry {
                    game_id: request.game_id,
                    game_code: "AB12CD34".parse().unwrap(),
                    state: GameState::AwaitingPlayers,
                    can_join_player: true,
                    can_join_spectator: false,
                },
            },
        };
        let wire = serde_json::to_string(&reply).unwrap();
        assert!(verify(&request, &reply, 1000).is_ok());
        for bad in [
            wire.replace("\"checked_at\":1000", "\"checked_at\":1000.0"),
            wire.replace("\"checked_at\":1000", "\"checked_at\":9007199254740992"),
            wire.replace("\"valid_until\":2000,", ""),
            wire.replace(
                "\"state\":\"awaiting_players\"",
                "\"state\":\"awaiting_players\",\"players\":[]",
            ),
            wire.replace(
                "\"can_join_player\":true",
                "\"can_join_player\":true,\"\\u0063an_join_player\":false",
            ),
        ] {
            assert!(
                super::super::super::game_peers::decode::<DiscoveryOwnerReply>(bad.as_bytes())
                    .is_err()
            );
        }
    }
    #[test]
    fn discovery_reply_identity_lifecycle_admission_and_deadlines_are_bound() {
        let request = request();
        let mut reply = DiscoveryOwnerReply {
            game_id: request.game_id,
            checked_at: 1000,
            valid_until: Some(2000),
            outcome: DiscoveryOutcome::Entry {
                entry: GameEntry {
                    game_id: request.game_id,
                    game_code: "AB12CD34".parse().unwrap(),
                    state: GameState::AwaitingPlayers,
                    can_join_player: true,
                    can_join_spectator: false,
                },
            },
        };
        assert!(verify(&request, &reply, 999).is_err());
        reply.valid_until = Some(1000);
        assert!(verify(&request, &reply, 1000).is_err());
        reply.valid_until = Some(2000);
        if let DiscoveryOutcome::Entry { entry } = &mut reply.outcome {
            entry.state = GameState::InProgress;
        }
        assert!(verify(&request, &reply, 1000).is_err());
        if let DiscoveryOutcome::Entry { entry } = &mut reply.outcome {
            entry.can_join_player = false;
        }
        assert!(verify(&request, &reply, 1000).is_ok());
        if let DiscoveryOutcome::Entry { entry } = &mut reply.outcome {
            entry.can_join_spectator = true;
        }
        assert!(verify(&request, &reply, 1000).is_ok());
    }

    #[test]
    fn discovery_expiry_ack_is_bound_and_cannot_replace_a_read_projection() {
        let mut request = request();
        let expiry = ObservedDiscoveryExpiry {
            deadline: 2000,
            observed_at: 2007,
        };
        request.expiry = Some(expiry);
        let mut reply = DiscoveryOwnerReply {
            game_id: request.game_id,
            checked_at: 1999,
            valid_until: None,
            outcome: DiscoveryOutcome::ExpiryFenced { expiry },
        };
        assert!(verify(&request, &reply, 1999).is_ok());
        request.expiry = None;
        assert!(verify(&request, &reply, 1999).is_err());
        request.expiry = Some(expiry);
        reply.outcome = DiscoveryOutcome::ExpiryFenced {
            expiry: ObservedDiscoveryExpiry {
                observed_at: 2006,
                ..expiry
            },
        };
        assert!(verify(&request, &reply, 1999).is_err());
        reply.outcome = DiscoveryOutcome::NotFound {};
        assert!(verify(&request, &reply, 1999).is_err());
        reply.outcome = DiscoveryOutcome::Unavailable {};
        assert!(verify(&request, &reply, 1999).is_ok());
        let wire = serde_json::to_string(&request).unwrap();
        for bad in [
            wire.replace("2007", "2007.0"),
            wire.replace("2007", "9007199254740992"),
            wire.replace(
                "\"observed_at\":2007",
                "\"observed_at\":2007,\"\\u006fbserved_at\":2007",
            ),
        ] {
            assert!(
                super::super::super::game_peers::decode::<DiscoveryOwnerRequest>(bad.as_bytes())
                    .is_err()
            );
        }
    }
}
