//! Closed known-ID Game recovery metadata. Never browser authority.
use super::{
    game_directory::{ProjectionWire, ReadyWire, WorkWire},
    game_peers::{GamePeerError, object},
};
use brews_domain::games::GameCode;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GameRecoveryRequest {
    #[serde(deserialize_with = "object")]
    pub(super) work: WorkWire,
    #[serde(deserialize_with = "Option::<GameCode>::deserialize")]
    pub(super) game_code: Option<GameCode>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GameRecoveryReply {
    #[serde(deserialize_with = "object")]
    pub(super) request: GameRecoveryRequest,
    #[serde(deserialize_with = "object")]
    pub(super) outcome: GameRecoveryOutcome,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GameRecoveryOutcome {
    Ready {
        #[serde(deserialize_with = "object")]
        ready: ReadyWire,
        #[serde(deserialize_with = "object")]
        projection: ProjectionWire,
    },
    Committed {
        #[serde(deserialize_with = "object")]
        projection: ProjectionWire,
    },
    Absent {},
    Terminal {
        #[serde(deserialize_with = "object")]
        projection: ProjectionWire,
    },
    Unavailable {},
}
impl GameRecoveryReply {
    pub(super) fn validate_for(
        &self,
        request: &GameRecoveryRequest,
    ) -> Result<GameRecoveryOutcome, GamePeerError> {
        if self.request != *request {
            return Err(GamePeerError::Unavailable);
        }
        let work = request.work.work()?;
        if request.game_code.is_some() && work.ready_revision() != Some(0) {
            return Err(GamePeerError::Unavailable);
        }
        if matches!(
            self.outcome,
            GameRecoveryOutcome::Absent {} | GameRecoveryOutcome::Unavailable {}
        ) {
            return Ok(self.outcome.clone());
        }
        if let GameRecoveryOutcome::Terminal { projection } = &self.outcome {
            use brews_domain::games::GameState;
            let p = projection.proof().map_err(|_| GamePeerError::Unavailable)?;
            let times = p
                .ended_at()
                .is_some_and(|end| end >= p.started_at().unwrap_or(p.created_at()))
                && p.started_at().is_none_or(|start| start >= p.created_at());
            let shape = match p.state() {
                GameState::Cancelled if p.started_at().is_none() => {
                    p.history_expires_at().is_none()
                }
                GameState::Cancelled | GameState::Resolved => {
                    p.started_at().is_some()
                        && p.game_code().is_some()
                        && p.history_expires_at()
                            .is_some_and(|expiry| p.ended_at().is_some_and(|end| expiry > end))
                }
                _ => false,
            };
            let uncommitted_code = p.state() == GameState::Cancelled
                && p.started_at().is_none()
                && p.history_expires_at().is_none()
                && p.game_code().is_none()
                && request.game_code.is_some()
                && work.ready_revision() == Some(0);
            if p.game_id() != work.game_id()
                || p.designated_host_id() != work.account_id()
                || p.fingerprint() != work.fingerprint()
                || p.created_at() != work.created_at()
                || p.source_revision() <= 0
                || (p.game_code() != request.game_code.as_ref() && !uncommitted_code)
                || !times
                || !shape
            {
                return Err(GamePeerError::Unavailable);
            }
            return Ok(self.outcome.clone());
        }
        if let GameRecoveryOutcome::Committed { projection } = &self.outcome {
            use brews_domain::games::GameState;
            let p = projection.proof().map_err(|_| GamePeerError::Unavailable)?;
            let valid_state = match p.state() {
                GameState::AwaitingPlayers => p.started_at().is_none(),
                GameState::InProgress => p.started_at().is_some_and(|at| at >= p.created_at()),
                _ => false,
            };
            if p.game_id() != work.game_id()
                || p.designated_host_id() != work.account_id()
                || p.fingerprint() != work.fingerprint()
                || p.created_at() != work.created_at()
                || p.source_revision() <= 0
                || work.ready_revision() != Some(0)
                || request.game_code.is_none()
                || p.game_code() != request.game_code.as_ref()
                || !valid_state
                || p.ended_at().is_some()
                || p.history_expires_at().is_some()
            {
                return Err(GamePeerError::Unavailable);
            }
            return Ok(self.outcome.clone());
        }
        let GameRecoveryOutcome::Ready { ready, projection } = &self.outcome else {
            return Err(GamePeerError::Unavailable);
        };
        let ready = ready.proof().map_err(|_| GamePeerError::Unavailable)?;
        let projection = projection.proof().map_err(|_| GamePeerError::Unavailable)?;
        if ready.game_id() != work.game_id()
            || ready.account_id() != work.account_id()
            || ready.command_id() != work.command_id()
            || ready.fingerprint() != work.fingerprint()
            || ready.created_at() != work.created_at()
            || ready.source_revision() != 0
            || projection.game_id() != work.game_id()
            || projection.designated_host_id() != work.account_id()
            || projection.fingerprint() != work.fingerprint()
            || projection.created_at() != work.created_at()
            || projection.state() != brews_domain::games::GameState::New
            || projection.source_revision() != 0
            || projection.game_code().is_some()
            || projection.started_at().is_some()
            || projection.ended_at().is_some()
            || projection.history_expires_at().is_some()
        {
            return Err(GamePeerError::Unavailable);
        }
        Ok(self.outcome.clone())
    }
}

#[cfg(test)]
use crate::auth::test_support;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real owner fixtures fail fast."
)]
mod tests {
    use super::*;
    use crate::{
        auth::{AuthPolicy, GameAccountAuthority},
        db::{
            directory::{DirectoryService, migrate_directory},
            game::{GameService, migrate_game},
        },
        game::creation_fingerprint,
    };
    use brews_domain::{accounts::AccountRole, games::GameConfiguration};
    use test_support::{Sqlite, TestRuntime};

    pub(super) fn id<T: std::str::FromStr>(rt: &TestRuntime, n: u8) -> T
    where
        T::Err: std::fmt::Debug,
    {
        let mut b = [0; 16];
        b[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
        b[6] = 0x70;
        b[8] = 0x80;
        b[15] = n;
        uuid::Uuid::from_bytes(b).to_string().parse().unwrap()
    }
    fn genuine_ready() -> (GameRecoveryRequest, GameRecoveryReply) {
        let directory = Sqlite::new();
        let game = Sqlite::new();
        let rt = TestRuntime::new();
        migrate_directory(&directory).unwrap();
        migrate_game(&game).unwrap();
        let config = GameConfiguration::default();
        let work = DirectoryService::new(&directory, &rt)
            .unwrap()
            .claim_game(
                id(&rt, 1),
                id(&rt, 2),
                creation_fingerprint(&config).unwrap(),
            )
            .unwrap();
        let owner = GameService::new(&game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let created = owner
            .initialize(
                &work,
                GameAccountAuthority::from_trusted_peer(
                    work.account_id(),
                    id(&rt, 3),
                    AccountRole::Host,
                    0,
                    rt.now.get() + 100_000,
                )
                .unwrap(),
                &config,
            )
            .unwrap();
        let request = GameRecoveryRequest {
            work: (&work).into(),
            game_code: None,
        };
        let reply = GameRecoveryReply {
            request: request.clone(),
            outcome: GameRecoveryOutcome::Ready {
                ready: (&created.ready).into(),
                projection: (&owner.directory_projection().unwrap()).into(),
            },
        };
        (request, reply)
    }
    #[test]
    fn exact_negative_recovery_outcomes_are_not_initializer_authority() {
        let (request, mut reply) = genuine_ready();
        for outcome in [
            GameRecoveryOutcome::Absent {},
            GameRecoveryOutcome::Unavailable {},
        ] {
            reply.outcome = outcome.clone();
            assert_eq!(reply.validate_for(&request), Ok(outcome));
        }
    }
    #[test]
    fn recovery_original_bytes_require_maps_nullable_fields_and_closed_shapes() {
        use super::super::game_peers::decode;
        let (request, reply) = genuine_ready();
        let bytes = serde_json::to_string(&reply).unwrap();
        assert!(decode::<GameRecoveryReply>(bytes.as_bytes()).is_ok());
        for bad in [
            bytes.replace("\"game_code\":null", "\"game_code\":null,\"unknown\":1"),
            bytes.replacen("\"game_code\":null,", "", 1),
            bytes.replacen(
                "\"source_revision\":0",
                "\"source_revision\":9007199254740992",
                1,
            ),
            bytes.replacen(
                "\"source_revision\":0",
                "\"source_revision\":0,\"\\u0073ource_revision\":0",
                1,
            ),
            bytes.replacen(
                "\"result\":\"ready\"",
                "\"result\":\"ready\",\"result\":\"ready\"",
                1,
            ),
            bytes.replacen(
                "\"outcome\":{",
                "\"outcome\":{\"browser_principal\":\"Admin\",",
                1,
            ),
        ] {
            assert!(
                decode::<GameRecoveryReply>(bad.as_bytes()).is_err(),
                "Accepted invalid closed recovery shape"
            );
        }
        let mut value = serde_json::to_value(&reply).unwrap();
        for path in ["request", "outcome"] {
            let original = value[path].clone();
            value[path] =
                serde_json::Value::Array(original.as_object().unwrap().values().cloned().collect());
            assert!(decode::<GameRecoveryReply>(&serde_json::to_vec(&value).unwrap()).is_err());
            value[path] = original;
        }
        for path in ["ready", "projection"] {
            let original = value["outcome"][path].clone();
            value["outcome"][path] =
                serde_json::Value::Array(original.as_object().unwrap().values().cloned().collect());
            assert!(decode::<GameRecoveryReply>(&serde_json::to_vec(&value).unwrap()).is_err());
            value["outcome"][path] = original;
        }
        let mut work = serde_json::to_value(&request).unwrap();
        let original = work["work"].clone();
        work["work"] =
            serde_json::Value::Array(original.as_object().unwrap().values().cloned().collect());
        assert!(decode::<GameRecoveryRequest>(&serde_json::to_vec(&work).unwrap()).is_err());
    }
    #[test]
    fn ready_proof_and_reply_echo_bind_every_initializer_identity() {
        let (request, reply) = genuine_ready();
        let alternate = id::<brews_domain::ids::GameId>(&TestRuntime::new(), 9).to_string();
        for (path, field, replacement) in [
            ("ready", "game_id", serde_json::json!(alternate)),
            ("ready", "account_id", serde_json::json!(alternate)),
            ("ready", "command_id", serde_json::json!(alternate)),
            (
                "ready",
                "fingerprint",
                serde_json::to_value([8u8; 32]).unwrap(),
            ),
            ("ready", "created_at", serde_json::json!(1)),
            ("ready", "source_revision", serde_json::json!(1)),
            ("projection", "game_id", serde_json::json!(alternate)),
            (
                "projection",
                "designated_host_id",
                serde_json::json!(alternate),
            ),
            (
                "projection",
                "fingerprint",
                serde_json::to_value([8u8; 32]).unwrap(),
            ),
            ("projection", "created_at", serde_json::json!(1)),
            ("projection", "source_revision", serde_json::json!(1)),
            ("projection", "game_code", serde_json::json!("ABCD1234")),
        ] {
            let mut value = serde_json::to_value(&reply).unwrap();
            value["outcome"][path][field] = replacement;
            let bad: GameRecoveryReply = serde_json::from_value(value).unwrap();
            assert!(bad.validate_for(&request).is_err());
        }
        let mut wrong_echo = reply;
        wrong_echo.request.game_code = Some("ABCD1234".parse().unwrap());
        assert!(wrong_echo.validate_for(&request).is_err());
    }
    #[test]
    fn hidden_code_association_requires_a_ready_creation_snapshot() {
        let (mut request, mut reply) = genuine_ready();
        request.game_code = Some("ABCD1234".parse().unwrap());
        reply.request = request.clone();
        assert!(reply.validate_for(&request).is_err());
    }
    #[test]
    fn actual_new_owner_evidence_can_ack_the_exact_unready_initializer() {
        let (request, reply) = genuine_ready();
        assert!(matches!(
            reply.validate_for(&request),
            Ok(GameRecoveryOutcome::Ready { .. })
        ));
    }
}
