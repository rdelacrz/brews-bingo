//! Directory game coordination: metadata-only DTOs, owned core is the sole SQL authority.
use super::game_peers::{GamePeerError, object, optional_integer, optional_object, safe_integer};
use crate::{
    auth::Runtime,
    db::{Database, directory::DirectoryService},
    directory::{DirectoryError, games::*},
};
use brews_domain::{
    games::{GameCode, GameState},
    ids::{AccountId, CommandId, GameId},
};
use serde::{Deserialize, Serialize};

fn checked_revision(value: i64) -> Result<SourceRevision, DirectoryError> {
    if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&value) {
        return Err(DirectoryError::ProofMismatch);
    }
    value.try_into()
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadyWire {
    game_id: GameId,
    account_id: AccountId,
    command_id: CommandId,
    fingerprint: [u8; 32],
    #[serde(deserialize_with = "safe_integer")]
    created_at: i64,
    #[serde(deserialize_with = "safe_integer")]
    source_revision: i64,
}
impl From<&CreationReadyProof> for ReadyWire {
    fn from(p: &CreationReadyProof) -> Self {
        Self {
            game_id: p.game_id(),
            account_id: p.account_id(),
            command_id: p.command_id(),
            fingerprint: *p.fingerprint().digest(),
            created_at: p.created_at(),
            source_revision: p.source_revision(),
        }
    }
}
impl ReadyWire {
    pub(super) fn proof(&self) -> Result<CreationReadyProof, DirectoryError> {
        Ok(CreationReadyProof::new(
            self.game_id,
            self.account_id,
            self.command_id,
            CreationFingerprint::from_digest(self.fingerprint),
            self.created_at,
            checked_revision(self.source_revision)?,
        ))
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectionWire {
    game_id: GameId,
    designated_host_id: AccountId,
    fingerprint: [u8; 32],
    state: GameState,
    #[serde(deserialize_with = "safe_integer")]
    source_revision: i64,
    #[serde(deserialize_with = "Option::<GameCode>::deserialize")]
    game_code: Option<GameCode>,
    #[serde(deserialize_with = "safe_integer")]
    created_at: i64,
    #[serde(deserialize_with = "optional_integer")]
    started_at: Option<i64>,
    #[serde(deserialize_with = "optional_integer")]
    ended_at: Option<i64>,
    #[serde(deserialize_with = "optional_integer")]
    history_expires_at: Option<i64>,
}
impl From<&GameProjection> for ProjectionWire {
    fn from(p: &GameProjection) -> Self {
        Self {
            game_id: p.game_id(),
            designated_host_id: p.designated_host_id(),
            fingerprint: *p.fingerprint().digest(),
            state: p.state(),
            source_revision: p.source_revision(),
            game_code: p.game_code().cloned(),
            created_at: p.created_at(),
            started_at: p.started_at(),
            ended_at: p.ended_at(),
            history_expires_at: p.history_expires_at(),
        }
    }
}
impl ProjectionWire {
    pub(super) fn proof(&self) -> Result<GameProjection, DirectoryError> {
        Ok(GameProjection::new(
            self.game_id,
            self.designated_host_id,
            CreationFingerprint::from_digest(self.fingerprint),
            self.state,
            checked_revision(self.source_revision)?,
            self.game_code.clone(),
            self.created_at,
            self.started_at,
            self.ended_at,
            self.history_expires_at,
        ))
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkWire {
    account_id: AccountId,
    command_id: CommandId,
    game_id: GameId,
    fingerprint: [u8; 32],
    #[serde(deserialize_with = "safe_integer")]
    created_at: i64,
    #[serde(deserialize_with = "safe_integer")]
    deadline: i64,
    #[serde(deserialize_with = "optional_integer")]
    next_retry_at: Option<i64>,
    attempts: u32,
    #[serde(deserialize_with = "optional_integer")]
    ready_revision: Option<i64>,
}
impl From<&CreationWork> for WorkWire {
    fn from(w: &CreationWork) -> Self {
        Self {
            account_id: w.account_id(),
            command_id: w.command_id(),
            game_id: w.game_id(),
            fingerprint: *w.fingerprint().digest(),
            created_at: w.created_at(),
            deadline: w.deadline(),
            next_retry_at: w.next_retry_at(),
            attempts: w.attempts(),
            ready_revision: w.ready_revision(),
        }
    }
}
impl WorkWire {
    pub(super) fn work(&self) -> Result<CreationWork, GamePeerError> {
        if self.created_at.checked_add(CREATION_LIFETIME_MS) != Some(self.deadline)
            || self.ready_revision.is_some_and(|r| r != 0)
            || self.next_retry_at.is_some_and(|t| t < self.created_at)
        {
            return Err(GamePeerError::Unavailable);
        }
        Ok(CreationWork::new(
            self.account_id,
            self.command_id,
            self.game_id,
            CreationFingerprint::from_digest(self.fingerprint),
            self.created_at,
            self.deadline,
            self.next_retry_at,
            self.attempts,
            self.ready_revision,
        ))
    }
    fn same_creation(&self, other: &Self) -> bool {
        self.account_id == other.account_id
            && self.command_id == other.command_id
            && self.game_id == other.game_id
            && self.fingerprint == other.fingerprint
            && self.created_at == other.created_at
            && self.deadline == other.deadline
            && self.ready_revision == other.ready_revision
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DirectoryGameRequest {
    Claim {
        account_id: AccountId,
        command_id: CommandId,
        fingerprint: [u8; 32],
    },
    Acknowledge {
        #[serde(deserialize_with = "object")]
        ready: ReadyWire,
    },
    AllocateCode {
        #[serde(deserialize_with = "object")]
        ready: ReadyWire,
    },
    Publish {
        #[serde(deserialize_with = "object")]
        projection: ProjectionWire,
    },
    ConfirmReservation {
        game_id: GameId,
    },
    Release {
        #[serde(deserialize_with = "object")]
        projection: ProjectionWire,
    },
    PendingProbe {
        game_id: GameId,
    },
    GameCode {
        game_id: GameId,
    },
    LookupCode {
        game_code: GameCode,
    },
    DueWork {
        limit: u32,
    },
    Retry {
        #[serde(deserialize_with = "object")]
        work: WorkWire,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DirectoryGameRejection {
    Conflict,
    AssignmentBlocked,
    Completed,
    UnknownGame,
    InvalidProof,
    Unavailable,
}
impl From<DirectoryError> for DirectoryGameRejection {
    fn from(e: DirectoryError) -> Self {
        match e {
            DirectoryError::ReservationOccupied
            | DirectoryError::CommandConflict
            | DirectoryError::Busy => Self::Conflict,
            DirectoryError::AssignmentBlocked | DirectoryError::HostedGame => {
                Self::AssignmentBlocked
            }
            DirectoryError::Completed | DirectoryError::StaleOperation => Self::Completed,
            DirectoryError::UnknownGame => Self::UnknownGame,
            DirectoryError::ProofMismatch | DirectoryError::OperationMismatch => Self::InvalidProof,
            _ => Self::Unavailable,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DirectoryGameOutcome {
    Work {
        #[serde(deserialize_with = "object")]
        work: WorkWire,
    },
    Acknowledged {
        #[serde(deserialize_with = "object")]
        ready: ReadyWire,
    },
    CodeAllocated {
        #[serde(deserialize_with = "object")]
        ready: ReadyWire,
        game_code: GameCode,
    },
    Published {
        game_id: GameId,
        #[serde(deserialize_with = "safe_integer")]
        source_revision: i64,
        published: bool,
    },
    Reserved {
        game_id: GameId,
    },
    Released {
        game_id: GameId,
    },
    Pending {
        game_id: GameId,
        #[serde(deserialize_with = "optional_object")]
        work: Option<WorkWire>,
    },
    Code {
        game_id: GameId,
        #[serde(deserialize_with = "Option::<GameCode>::deserialize")]
        game_code: Option<GameCode>,
    },
    Lookup {
        game_code: GameCode,
        #[serde(deserialize_with = "Option::<GameId>::deserialize")]
        game_id: Option<GameId>,
    },
    Due {
        #[serde(deserialize_with = "super::game_peers::bounded_objects")]
        work: Vec<WorkWire>,
    },
    Rejected {
        code: DirectoryGameRejection,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DirectoryGameReply {
    #[serde(deserialize_with = "object")]
    request: DirectoryGameRequest,
    #[serde(deserialize_with = "object")]
    outcome: DirectoryGameOutcome,
}
pub(super) fn dispatch<D: Database, R: Runtime>(
    service: &DirectoryService<'_, D, R>,
    request: &DirectoryGameRequest,
) -> DirectoryGameReply {
    let result =
        match request {
            DirectoryGameRequest::Claim {
                account_id,
                command_id,
                fingerprint,
            } => service
                .claim_game(
                    *account_id,
                    *command_id,
                    CreationFingerprint::from_digest(*fingerprint),
                )
                .map(|w| DirectoryGameOutcome::Work { work: (&w).into() }),
            DirectoryGameRequest::Acknowledge { ready } => ready
                .proof()
                .and_then(|p| service.acknowledge_creation(p))
                .map(|a| DirectoryGameOutcome::Acknowledged {
                    ready: (&a.proof()).into(),
                }),
            DirectoryGameRequest::AllocateCode { ready } => ready
                .proof()
                .and_then(|p| service.allocate_game_code(CreationAck::new(p)))
                .map(|g| DirectoryGameOutcome::CodeAllocated {
                    ready: (&g.creation().proof()).into(),
                    game_code: g.game_code().clone(),
                }),
            DirectoryGameRequest::ConfirmReservation { game_id } => service
                .confirm_reservation(*game_id)
                .map(|p| DirectoryGameOutcome::Reserved {
                    game_id: p.game_id(),
                }),
            DirectoryGameRequest::Publish { projection } => projection
                .proof()
                .and_then(|p| service.publish_game(p))
                .map(|a| DirectoryGameOutcome::Published {
                    game_id: a.game_id(),
                    source_revision: a.source_revision(),
                    published: a.published(),
                }),
            DirectoryGameRequest::Release { projection } => projection
                .proof()
                .and_then(|p| service.release_game(TerminalProof::new(p)))
                .map(|a| DirectoryGameOutcome::Released {
                    game_id: a.game_id(),
                }),
            DirectoryGameRequest::PendingProbe { game_id } => {
                service
                    .creation_work(*game_id)
                    .map(|w| DirectoryGameOutcome::Pending {
                        game_id: *game_id,
                        work: w.as_ref().map(WorkWire::from),
                    })
            }
            DirectoryGameRequest::GameCode { game_id } => {
                service
                    .game_code(*game_id)
                    .map(|game_code| DirectoryGameOutcome::Code {
                        game_id: *game_id,
                        game_code,
                    })
            }
            DirectoryGameRequest::LookupCode { game_code } => service
                .lookup_game_code(game_code)
                .map(|game_id| DirectoryGameOutcome::Lookup {
                    game_code: game_code.clone(),
                    game_id,
                }),
            DirectoryGameRequest::DueWork { limit } => {
                service
                    .due_creation_work(*limit)
                    .map(|work| DirectoryGameOutcome::Due {
                        work: work.iter().map(WorkWire::from).collect(),
                    })
            }
            DirectoryGameRequest::Retry { work } => work
                .work()
                .map_err(|_| DirectoryError::ProofMismatch)
                .and_then(|w| service.retry_creation(w))
                .map(|w| DirectoryGameOutcome::Work { work: (&w).into() }),
        };
    DirectoryGameReply {
        request: request.clone(),
        outcome: result.unwrap_or_else(|e| DirectoryGameOutcome::Rejected { code: e.into() }),
    }
}
pub(super) fn verify_reply(
    request: &DirectoryGameRequest,
    reply: DirectoryGameReply,
) -> Result<DirectoryGameOutcome, GamePeerError> {
    if reply.request != *request {
        return Err(GamePeerError::Unavailable);
    }
    let valid = match (request, &reply.outcome) {
        (
            DirectoryGameRequest::Claim {
                account_id,
                command_id,
                fingerprint,
            },
            DirectoryGameOutcome::Work { work },
        ) => {
            work.account_id == *account_id
                && work.command_id == *command_id
                && work.fingerprint == *fingerprint
                && work.work().is_ok()
        }
        (
            DirectoryGameRequest::Acknowledge { ready },
            DirectoryGameOutcome::Acknowledged { ready: found },
        )
        | (
            DirectoryGameRequest::AllocateCode { ready },
            DirectoryGameOutcome::CodeAllocated { ready: found, .. },
        ) => ready == found && ready.source_revision == 0,
        (
            DirectoryGameRequest::ConfirmReservation { game_id },
            DirectoryGameOutcome::Reserved { game_id: found },
        ) => game_id == found,
        (
            DirectoryGameRequest::Publish { projection },
            DirectoryGameOutcome::Published {
                game_id,
                source_revision,
                ..
            },
        ) => projection.game_id == *game_id && *source_revision >= projection.source_revision,
        (
            DirectoryGameRequest::Release { projection },
            DirectoryGameOutcome::Released { game_id },
        ) => projection.game_id == *game_id,
        (
            DirectoryGameRequest::PendingProbe { game_id },
            DirectoryGameOutcome::Pending {
                game_id: found,
                work,
            },
        ) => {
            game_id == found
                && work
                    .as_ref()
                    .is_none_or(|w| w.game_id == *game_id && w.work().is_ok())
        }
        (
            DirectoryGameRequest::GameCode { game_id },
            DirectoryGameOutcome::Code { game_id: found, .. },
        ) => game_id == found,
        (
            DirectoryGameRequest::LookupCode { game_code },
            DirectoryGameOutcome::Lookup {
                game_code: found, ..
            },
        ) => game_code == found,
        (DirectoryGameRequest::DueWork { limit }, DirectoryGameOutcome::Due { work }) => {
            *limit > 0
                && *limit <= super::game_peers::GAME_PEER_WORK_LIMIT
                && work.len() <= *limit as usize
                && work.iter().all(|w| w.work().is_ok())
        }
        (DirectoryGameRequest::Retry { work }, DirectoryGameOutcome::Work { work: found }) => {
            work.same_creation(found)
                && found.work().is_ok()
                && found.attempts >= work.attempts
                && found.next_retry_at >= work.next_retry_at
        }
        (_, DirectoryGameOutcome::Rejected { code }) => {
            return Err(match code {
                DirectoryGameRejection::Conflict
                | DirectoryGameRejection::AssignmentBlocked
                | DirectoryGameRejection::Completed
                | DirectoryGameRejection::UnknownGame => GamePeerError::Conflict,
                _ => GamePeerError::Unavailable,
            });
        }
        _ => false,
    };
    if valid {
        Ok(reply.outcome)
    } else {
        Err(GamePeerError::Unavailable)
    }
}

#[cfg(target_arch = "wasm32")]
impl super::directory::GameDirectoryObject {
    pub(super) async fn execute_games_request(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        use super::{OwnerDatabase, WorkerRuntime};
        use axum::body::{Body, to_bytes};
        if request.method() != worker::Method::Post
            || request.path() != "/games"
            || request.url()?.query().is_some()
        {
            return worker::Response::error("invalid private game request", 400);
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            to_bytes(
                Body::new(http.into_body()),
                super::game_peers::GAME_PEER_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("private request bound".into()))?
            .to_vec(),
        );
        let message: DirectoryGameRequest = match super::game_peers::decode(&bytes) {
            Ok(m) => m,
            Err(_) => return worker::Response::error("invalid private game request", 400),
        };
        let db = OwnerDatabase::new(self.state.storage());
        crate::db::directory::migrate_directory(&db)
            .map_err(|_| worker::Error::RustError("Directory initialization failed".into()))?;
        let rt = WorkerRuntime;
        let service = DirectoryService::new(&db, &rt)
            .map_err(|_| worker::Error::RustError("Directory unavailable".into()))?;
        let result = dispatch(&service, &message);
        self.schedule_cleanup().await?;
        self.state.storage().sync().await?;
        let limit = if matches!(message, DirectoryGameRequest::DueWork { .. }) {
            super::game_peers::GAME_PEER_BATCH_MAX_BYTES
        } else {
            super::game_peers::GAME_PEER_MAX_BYTES
        };
        let encoded = super::game_peers::encode_limit(&result, limit)
            .map_err(|_| worker::Error::RustError("private response bound".into()))?;
        let mut response = worker::Response::from_bytes(encoded.to_vec())?;
        response
            .headers_mut()
            .set("Content-Type", "application/json")?;
        response.headers_mut().set("Cache-Control", "no-store")?;
        Ok(response)
    }
}

#[cfg(test)]
use crate::auth::test_support;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Wire fixtures fail fast."
)]
mod tests {
    use super::super::game_peers::decode;
    use super::*;
    fn id<T: std::str::FromStr>(n: u8) -> T
    where
        T::Err: std::fmt::Debug,
    {
        format!("01900000-0000-7000-8000-{n:012}").parse().unwrap()
    }
    #[test]
    fn directory_proof_reconstruction_requires_safe_typed_revision_and_closed_maps() {
        let ready = ReadyWire {
            game_id: id(3),
            account_id: id(1),
            command_id: id(2),
            fingerprint: [7; 32],
            created_at: 1000,
            source_revision: 0,
        };
        let request = DirectoryGameRequest::Acknowledge {
            ready: ready.clone(),
        };
        let wire = serde_json::to_string(&request).unwrap();
        assert!(decode::<DirectoryGameRequest>(wire.as_bytes()).is_ok());
        for bad in [
            wire.replace("\"source_revision\":0", "\"source_revision\":-1"),
            wire.replace(
                "\"source_revision\":0",
                "\"source_revision\":9007199254740992",
            ),
            wire.replace(
                "\"source_revision\":0",
                "\"source_revision\":0,\"\\u0073ource_revision\":0",
            ),
            wire.replace(
                "\"source_revision\":0",
                "\"source_revision\":0,\"principal\":\"Admin\"",
            ),
            wire.replace("\"source_revision\":0", "\"source_revision\":0.0"),
        ] {
            assert!(decode::<DirectoryGameRequest>(bad.as_bytes()).is_err());
        }
        let mut unsafe_ready = ready;
        unsafe_ready.source_revision = i64::MAX;
        assert!(unsafe_ready.proof().is_err());
        let projection = ProjectionWire {
            game_id: id(3),
            designated_host_id: id(1),
            fingerprint: [7; 32],
            state: GameState::AwaitingPlayers,
            source_revision: i64::MAX,
            game_code: Some("ABCD1234".parse().unwrap()),
            created_at: 1000,
            started_at: None,
            ended_at: None,
            history_expires_at: None,
        };
        assert!(projection.proof().is_err());
    }
    #[test]
    fn directory_due_work_rejects_sequences_and_more_than_one_hundred() {
        let work = WorkWire {
            account_id: id(1),
            command_id: id(2),
            game_id: id(3),
            fingerprint: [0; 32],
            created_at: 1000,
            deadline: 1000 + CREATION_LIFETIME_MS,
            next_retry_at: Some(1000),
            attempts: 0,
            ready_revision: None,
        };
        let reply = DirectoryGameReply {
            request: DirectoryGameRequest::DueWork { limit: 100 },
            outcome: DirectoryGameOutcome::Due {
                work: vec![work.clone()],
            },
        };
        let good = serde_json::to_string(&reply).unwrap();
        assert!(decode::<DirectoryGameReply>(good.as_bytes()).is_ok());
        let object = serde_json::to_string(&work).unwrap();
        let sequence = format!(
            r#"["{}","{}","{}",{},1000,{},1000,0,null]"#,
            work.account_id,
            work.command_id,
            work.game_id,
            serde_json::to_string(&[0u8; 32]).unwrap(),
            work.deadline
        );
        assert!(
            serde_json::from_str::<DirectoryGameReply>(&good.replace(&object, &sequence)).is_err()
        );
        let too_many = DirectoryGameReply {
            request: DirectoryGameRequest::DueWork { limit: 100 },
            outcome: DirectoryGameOutcome::Due {
                work: vec![work; 101],
            },
        };
        assert!(
            serde_json::from_slice::<DirectoryGameReply>(&serde_json::to_vec(&too_many).unwrap())
                .is_err()
        );
    }
    #[test]
    fn directory_publication_recovery_and_release_use_real_core() {
        let db = test_support::Sqlite::new();
        let rt = test_support::TestRuntime::new();
        crate::db::directory::migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let mut b = [0; 16];
        b[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
        b[6] = 0x70;
        b[8] = 0x80;
        b[15] = 2;
        let work = service
            .claim_game(
                id(1),
                uuid::Uuid::from_bytes(b).to_string().parse().unwrap(),
                CreationFingerprint::from_digest([9; 32]),
            )
            .unwrap();
        let ready = CreationReadyProof::new(
            work.game_id(),
            work.account_id(),
            work.command_id(),
            work.fingerprint(),
            work.created_at(),
            SourceRevision::INITIAL,
        );
        let ack = service.acknowledge_creation(ready).unwrap();
        let grant = service.allocate_game_code(ack).unwrap();
        let check = |request: &DirectoryGameRequest| {
            verify_reply(request, dispatch(&service, request)).unwrap()
        };
        let due = DirectoryGameRequest::DueWork { limit: 100 };
        let pending = match check(&due) {
            DirectoryGameOutcome::Due { work } => {
                assert_eq!(work.len(), 1);
                work[0].clone()
            }
            _ => panic!("Expected due work"),
        };
        let retry = DirectoryGameRequest::Retry { work: pending };
        assert!(matches!(check(&retry), DirectoryGameOutcome::Work { .. }));
        let probe = DirectoryGameRequest::PendingProbe {
            game_id: work.game_id(),
        };
        assert!(matches!(
            check(&probe),
            DirectoryGameOutcome::Pending { work: Some(_), .. }
        ));
        let missing = DirectoryGameRequest::PendingProbe { game_id: id(8) };
        assert!(matches!(
            check(&missing),
            DirectoryGameOutcome::Pending { work: None, .. }
        ));
        let code = DirectoryGameRequest::GameCode {
            game_id: work.game_id(),
        };
        assert!(matches!(
            check(&code),
            DirectoryGameOutcome::Code {
                game_code: Some(_),
                ..
            }
        ));
        let lookup = DirectoryGameRequest::LookupCode {
            game_code: grant.game_code().clone(),
        };
        assert!(matches!(
            check(&lookup),
            DirectoryGameOutcome::Lookup { game_id: None, .. }
        ));
        let projection = GameProjection::new(
            work.game_id(),
            work.account_id(),
            work.fingerprint(),
            GameState::AwaitingPlayers,
            1.try_into().unwrap(),
            Some(grant.game_code().clone()),
            work.created_at(),
            None,
            None,
            None,
        );
        let publish = DirectoryGameRequest::Publish {
            projection: (&projection).into(),
        };
        assert!(matches!(
            check(&publish),
            DirectoryGameOutcome::Published {
                source_revision: 1,
                published: true,
                ..
            }
        ));
        assert!(
            matches!(check(&lookup),DirectoryGameOutcome::Lookup{game_id:Some(found),..} if found==work.game_id())
        );
        let mut wrong = dispatch(&service, &publish);
        if let DirectoryGameOutcome::Published { game_id, .. } = &mut wrong.outcome {
            *game_id = id(8);
        }
        assert!(verify_reply(&publish, wrong).is_err());
        let terminal = GameProjection::new(
            work.game_id(),
            work.account_id(),
            work.fingerprint(),
            GameState::Cancelled,
            2.try_into().unwrap(),
            Some(grant.game_code().clone()),
            work.created_at(),
            None,
            Some(rt.now.get()),
            None,
        );
        let release = DirectoryGameRequest::Release {
            projection: (&terminal).into(),
        };
        assert!(matches!(
            check(&release),
            DirectoryGameOutcome::Released { .. }
        ));
        assert!(matches!(
            check(&lookup),
            DirectoryGameOutcome::Lookup { game_id: None, .. }
        ));
        assert!(matches!(check(&due),DirectoryGameOutcome::Due{work} if work.is_empty()));
    }
    #[test]
    fn directory_claim_ack_and_code_use_real_owner_core_and_exact_binding() {
        let db = test_support::Sqlite::new();
        let rt = test_support::TestRuntime::new();
        crate::db::directory::migrate_directory(&db).unwrap();
        let service = DirectoryService::new(&db, &rt).unwrap();
        let request = DirectoryGameRequest::Claim {
            account_id: id(1),
            command_id: {
                let mut b = [0; 16];
                b[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
                b[6] = 0x70;
                b[8] = 0x80;
                b[15] = 2;
                uuid::Uuid::from_bytes(b).to_string().parse().unwrap()
            },
            fingerprint: [9; 32],
        };
        let response = dispatch(&service, &request);
        let work = match verify_reply(&request, response).unwrap() {
            DirectoryGameOutcome::Work { work } => work,
            _ => panic!("Expected work"),
        };
        let ready = ReadyWire {
            game_id: work.game_id,
            account_id: work.account_id,
            command_id: work.command_id,
            fingerprint: work.fingerprint,
            created_at: work.created_at,
            source_revision: 0,
        };
        let ack = DirectoryGameRequest::Acknowledge {
            ready: ready.clone(),
        };
        assert!(matches!(
            verify_reply(&ack, dispatch(&service, &ack)).unwrap(),
            DirectoryGameOutcome::Acknowledged { .. }
        ));
        let allocate = DirectoryGameRequest::AllocateCode { ready };
        let reply = dispatch(&service, &allocate);
        assert!(matches!(
            verify_reply(&allocate, reply.clone()).unwrap(),
            DirectoryGameOutcome::CodeAllocated { .. }
        ));
        assert!(verify_reply(&request, reply).is_err());
        let confirm = DirectoryGameRequest::ConfirmReservation { game_id: id(8) };
        assert!(verify_reply(&confirm, dispatch(&service, &confirm)).is_err());
        let wire = serde_json::to_vec(&dispatch(&service, &request)).unwrap();
        assert!(decode::<DirectoryGameReply>(&wire).is_ok());
    }
}
