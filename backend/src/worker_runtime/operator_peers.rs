//! Closed private operator messages. Matching metadata is never reusable account authority.
use super::game_accounts::AccountAuthorityWire;
use super::game_peers::{self, GamePeerError, object, positive_integer, safe_integer};
#[cfg(target_arch = "wasm32")]
use crate::game::{HostTransferAck, HostTransferGrant, HostTransferProof};
use crate::game::{HostTransferWork, TransferPhase};
use brews_domain::ids::{AccountId, CommandId, GameId, OperationId};
use serde::{Deserialize, Serialize};
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(super) struct OperatorChallenge([u8; 32]);
impl Drop for OperatorChallenge {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.0.zeroize();
    }
}
impl<'de> Deserialize<'de> for OperatorChallenge {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        decode_challenge(d)
    }
}
pub(super) fn decode_challenge<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<OperatorChallenge, D::Error> {
    let value = zeroize::Zeroizing::new(<[u8; 32]>::deserialize(d)?);
    if *value == [0; 32] {
        return Err(serde::de::Error::custom("invalid operator challenge"));
    }
    Ok(OperatorChallenge(*value))
}
impl OperatorChallenge {
    #[cfg(any(test, target_arch = "wasm32"))]
    fn new(runtime: &impl crate::auth::Runtime) -> Result<Self, GamePeerError> {
        let mut value = zeroize::Zeroizing::new([0; 32]);
        runtime
            .fill_random(value.as_mut())
            .map_err(|_| GamePeerError::Unavailable)?;
        if *value == [0; 32] {
            return Err(GamePeerError::Unavailable);
        }
        Ok(Self(*value))
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) fn fresh_challenge() -> Result<OperatorChallenge, GamePeerError> {
    OperatorChallenge::new(&super::WorkerRuntime)
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransferWire {
    operation: OperationId,
    game: GameId,
    actor: AccountId,
    command: CommandId,
    fingerprint: [u8; 32],
    old_host: AccountId,
    target: AccountId,
    #[serde(deserialize_with = "safe_integer")]
    expected_assignment: i64,
    #[serde(deserialize_with = "safe_integer")]
    prepared_revision: i64,
    nonce: [u8; 32],
    phase: String,
    #[serde(deserialize_with = "safe_integer")]
    next: i64,
    #[serde(deserialize_with = "safe_integer")]
    retry_attempts: i64,
}
impl From<&HostTransferWork> for TransferWire {
    fn from(w: &HostTransferWork) -> Self {
        Self {
            operation: w.operation,
            game: w.game,
            actor: w.actor,
            command: w.command,
            fingerprint: w.fingerprint,
            old_host: w.old_host,
            target: w.target,
            expected_assignment: w.expected_assignment,
            prepared_revision: w.prepared_revision,
            nonce: w.nonce,
            phase: w.phase.tag().into(),
            next: w.next,
            retry_attempts: i64::from(w.retry_attempts),
        }
    }
}
impl TransferWire {
    pub(super) fn work(&self) -> Result<HostTransferWork, GamePeerError> {
        let phase = TransferPhase::parse(&self.phase).map_err(|_| GamePeerError::Unavailable)?;
        if self.old_host == self.target
            || !(0..=i64::from(crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT))
                .contains(&self.retry_attempts)
        {
            return Err(GamePeerError::Unavailable);
        }
        Ok(HostTransferWork {
            operation: self.operation,
            game: self.game,
            actor: self.actor,
            command: self.command,
            fingerprint: self.fingerprint,
            old_host: self.old_host,
            target: self.target,
            expected_assignment: self.expected_assignment,
            prepared_revision: self.prepared_revision,
            nonce: self.nonce,
            phase,
            next: self.next,
            retry_attempts: u32::try_from(self.retry_attempts)
                .map_err(|_| GamePeerError::Unavailable)?,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OperatorAccountsRequest {
    Authorize {
        token: game_peers::SecretToken,
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
    },
    Recipient {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        token: game_peers::SecretToken,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
    },
    Targets {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        token: game_peers::SecretToken,
        #[serde(deserialize_with = "object")]
        query: brews_contracts::games::HostTransferTargetsQuery,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OperatorAccountsReply {
    Authorized {
        #[serde(deserialize_with = "object")]
        actor: AccountAuthorityWire,
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
    },
    Recipient {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        actor: AccountAuthorityWire,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
        #[serde(deserialize_with = "safe_integer")]
        epoch: i64,
        #[serde(deserialize_with = "positive_integer")]
        checked_at: i64,
    },
    Targets {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        actor: AccountAuthorityWire,
        #[serde(deserialize_with = "object")]
        query: brews_contracts::games::HostTransferTargetsQuery,
        targets: Vec<brews_contracts::games::HostTransferTarget>,
    },
    Rejected {
        code: super::game_accounts::AccountRejection,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OperatorDirectoryRequest {
    ReleaseTargets {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        token: game_peers::SecretToken,
        game: GameId,
        #[serde(deserialize_with = "object")]
        query: brews_contracts::games::HostTransferTargetsQuery,
        targets: Vec<AccountId>,
    },
    Prepare {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
    },
    Settle {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
        #[serde(deserialize_with = "object")]
        projection: super::game_directory::ProjectionWire,
    },
    Complete {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
    },
    Targets {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        game: GameId,
        targets: Vec<AccountId>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OperatorDirectoryReply {
    ReleasedTargets {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        game: GameId,
        requested: Vec<AccountId>,
        #[serde(deserialize_with = "object")]
        actor: AccountAuthorityWire,
        targets: Vec<brews_contracts::games::HostTransferTarget>,
    },
    Completed {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
    },
    Prepared {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
    },
    Settled {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        #[serde(deserialize_with = "object")]
        work: TransferWire,
    },
    Targets {
        #[serde(deserialize_with = "decode_challenge")]
        challenge: OperatorChallenge,
        game: GameId,
        requested: Vec<AccountId>,
        eligible: Vec<AccountId>,
    },
    Rejected {
        code: super::game_directory::DirectoryGameRejection,
    },
}
#[cfg(target_arch = "wasm32")]
pub(super) enum OperatorAccountCut {
    Current(crate::auth::GameAccountAuthority),
    Expired {
        proof: crate::auth::GameAccountAuthority,
        observed_at: i64,
    },
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn authorize(
    env: &worker::Env,
    token: &str,
) -> Result<OperatorAccountCut, GamePeerError> {
    use crate::auth::Runtime;
    let challenge = fresh_challenge()?;
    let request = OperatorAccountsRequest::Authorize {
        token: game_peers::SecretToken::new(token),
        challenge: challenge.clone(),
    };
    let reply: OperatorAccountsReply = game_peers::call(
        env,
        "ACCOUNTS",
        "accounts",
        "/operator-accounts",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?;
    match reply {
        OperatorAccountsReply::Authorized {
            actor,
            challenge: actual,
        } if actual == challenge => {
            let proof = actor.proof()?;
            let now = super::WorkerRuntime.now_ms();
            if now >= proof.expires_at() {
                return Ok(OperatorAccountCut::Expired {
                    proof,
                    observed_at: now,
                });
            }
            Ok(OperatorAccountCut::Current(proof))
        }
        OperatorAccountsReply::Rejected { code } => Err(code.into()),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn recipient(
    env: &worker::Env,
    token: &str,
    work: &HostTransferWork,
) -> Result<
    (
        crate::auth::GameAccountAuthority,
        crate::auth::HostRecipientAuthority,
    ),
    GamePeerError,
> {
    let challenge = fresh_challenge()?;
    let expected = TransferWire::from(work);
    let request = OperatorAccountsRequest::Recipient {
        challenge: challenge.clone(),
        token: game_peers::SecretToken::new(token),
        work: expected.clone(),
    };
    match game_peers::call(
        env,
        "ACCOUNTS",
        "accounts",
        "/operator-accounts",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?
    {
        OperatorAccountsReply::Recipient {
            challenge: actual_challenge,
            actor,
            work: actual,
            epoch,
            checked_at,
        } if actual_challenge == challenge && actual == expected => Ok((
            actor.proof()?,
            crate::auth::HostRecipientAuthority::from_trusted_peer(work.target, epoch, checked_at)
                .and_then(|p| p.bind_transfer(work))
                .map_err(|_| GamePeerError::Unavailable)?,
        )),
        OperatorAccountsReply::Rejected { code } => Err(code.into()),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn targets(
    env: &worker::Env,
    token: &str,
    query: &brews_contracts::games::HostTransferTargetsQuery,
) -> Result<
    (
        crate::auth::GameAccountAuthority,
        Vec<brews_contracts::games::HostTransferTarget>,
    ),
    GamePeerError,
> {
    let challenge = fresh_challenge()?;
    let request = OperatorAccountsRequest::Targets {
        challenge: challenge.clone(),
        token: game_peers::SecretToken::new(token),
        query: query.clone(),
    };
    match game_peers::call(
        env,
        "ACCOUNTS",
        "accounts",
        "/operator-accounts",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?
    {
        OperatorAccountsReply::Targets {
            challenge: actual_challenge,
            actor,
            query: actual,
            targets,
        } if actual_challenge == challenge
            && actual.limit == query.limit
            && actual.cursor == query.cursor
            && targets.len() <= query.limit as usize + 1
            && !targets
                .windows(2)
                .any(|p| p[0].account_id >= p[1].account_id)
            && !targets
                .iter()
                .any(|t| query.cursor.is_some_and(|c| t.account_id <= c)) =>
        {
            Ok((actor.proof()?, targets))
        }
        OperatorAccountsReply::Rejected { code } => Err(code.into()),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn prepare(
    env: &worker::Env,
    work: &HostTransferWork,
) -> Result<HostTransferGrant, GamePeerError> {
    let challenge = fresh_challenge()?;
    let wire = TransferWire::from(work);
    let request = OperatorDirectoryRequest::Prepare {
        challenge: challenge.clone(),
        work: wire.clone(),
    };
    match game_peers::call(
        env,
        "GAME_DIRECTORY",
        "directory",
        "/host-transfer",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?
    {
        OperatorDirectoryReply::Prepared {
            work: actual,
            challenge: actual_challenge,
        } if actual_challenge == challenge && actual == wire => Ok(HostTransferGrant {
            work: actual.work()?,
        }),
        OperatorDirectoryReply::Rejected { code } => Err(directory_error(code)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn settle(
    env: &worker::Env,
    proof: &HostTransferProof,
) -> Result<HostTransferAck, GamePeerError> {
    let challenge = fresh_challenge()?;
    let wire = TransferWire::from(&proof.work);
    let request = OperatorDirectoryRequest::Settle {
        challenge: challenge.clone(),
        work: wire.clone(),
        projection: (&proof.projection).into(),
    };
    match game_peers::call(
        env,
        "GAME_DIRECTORY",
        "directory",
        "/host-transfer",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?
    {
        OperatorDirectoryReply::Settled {
            work: actual,
            challenge: actual_challenge,
        } if actual_challenge == challenge && actual == wire => Ok(HostTransferAck {
            work: actual.work()?,
        }),
        OperatorDirectoryReply::Rejected { code } => Err(directory_error(code)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn complete(
    env: &worker::Env,
    work: &HostTransferWork,
) -> Result<(), GamePeerError> {
    let challenge = fresh_challenge()?;
    let wire = TransferWire::from(work);
    let request = OperatorDirectoryRequest::Complete {
        challenge: challenge.clone(),
        work: wire.clone(),
    };
    match game_peers::call(
        env,
        "GAME_DIRECTORY",
        "directory",
        "/host-transfer",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?
    {
        OperatorDirectoryReply::Completed {
            challenge: actual,
            work,
        } if actual == challenge && work == wire => Ok(()),
        OperatorDirectoryReply::Rejected { code } => Err(directory_error(code)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn release_targets(
    env: &worker::Env,
    token: &str,
    game: GameId,
    query: &brews_contracts::games::HostTransferTargetsQuery,
    requested: &[brews_contracts::games::HostTransferTarget],
) -> Result<crate::auth::GameAccountAuthority, GamePeerError> {
    let challenge = fresh_challenge()?;
    let ids = requested.iter().map(|t| t.account_id).collect::<Vec<_>>();
    let request = OperatorDirectoryRequest::ReleaseTargets {
        challenge: challenge.clone(),
        token: game_peers::SecretToken::new(token),
        game,
        query: query.clone(),
        targets: ids.clone(),
    };
    match game_peers::call(
        env,
        "GAME_DIRECTORY",
        "directory",
        "/host-transfer",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?
    {
        OperatorDirectoryReply::ReleasedTargets {
            challenge: actual,
            game: actual_game,
            requested: actual_ids,
            actor,
            targets,
        } if actual == challenge
            && actual_game == game
            && actual_ids == ids
            && targets.len() == requested.len()
            && targets
                .iter()
                .zip(requested)
                .all(|(a, b)| a.account_id == b.account_id && a.username == b.username) =>
        {
            Ok(actor.proof()?)
        }
        OperatorDirectoryReply::Rejected { code } => Err(directory_error(code)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
pub(super) async fn filter_targets(
    env: &worker::Env,
    game: GameId,
    targets: &[AccountId],
) -> Result<Vec<AccountId>, GamePeerError> {
    let challenge = fresh_challenge()?;
    let request = OperatorDirectoryRequest::Targets {
        challenge: challenge.clone(),
        game,
        targets: targets.to_vec(),
    };
    match game_peers::call(
        env,
        "GAME_DIRECTORY",
        "directory",
        "/host-transfer",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await?
    {
        OperatorDirectoryReply::Targets {
            challenge: actual_challenge,
            game: actual,
            requested,
            eligible,
        } if actual_challenge == challenge
            && actual == game
            && requested == targets
            && !eligible.windows(2).any(|p| p[0] >= p[1])
            && eligible.iter().all(|id| targets.contains(id)) =>
        {
            Ok(eligible)
        }
        OperatorDirectoryReply::Rejected { code } => Err(directory_error(code)),
        _ => Err(GamePeerError::Unavailable),
    }
}
#[cfg(target_arch = "wasm32")]
impl super::AccountsObject {
    pub(super) async fn execute_operator_accounts(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        if request.method() != worker::Method::Post || request.url()?.query().is_some() {
            return worker::Response::error("invalid private operator request", 400);
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            axum::body::to_bytes(
                axum::body::Body::new(http.into_body()),
                game_peers::GAME_PEER_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("operator request bound".into()))?
            .to_vec(),
        );
        let request: OperatorAccountsRequest = super::decode_private_json(&bytes)
            .map_err(|_| worker::Error::RustError("invalid operator envelope".into()))?;
        let db = super::OwnerDatabase::new(self.state.storage());
        crate::db::migrate(&db)
            .map_err(|_| worker::Error::RustError("operator storage unavailable".into()))?;
        let rt = super::WorkerRuntime;
        let service = self
            .service(&db, &rt)
            .map_err(|_| worker::Error::RustError("operator service unavailable".into()))?;
        let result = match &request {
            OperatorAccountsRequest::Authorize { token, challenge } => service
                .authorize_game_account(token.as_str())
                .map(|actor| OperatorAccountsReply::Authorized {
                    actor: (&actor).into(),
                    challenge: challenge.clone(),
                }),
            OperatorAccountsRequest::Recipient {
                token,
                work,
                challenge,
            } => work
                .work()
                .map_err(|_| crate::auth::AuthError::InvalidInput)
                .and_then(|binding| {
                    service.authorize_host_transfer_recipient(token.as_str(), &binding)
                })
                .map(|(actor, target)| OperatorAccountsReply::Recipient {
                    challenge: challenge.clone(),
                    actor: (&actor).into(),
                    work: work.clone(),
                    epoch: target.credential_epoch(),
                    checked_at: target.checked_at(),
                }),
            OperatorAccountsRequest::Targets {
                token,
                query,
                challenge,
            } => service
                .eligible_host_targets(token.as_str(), query.cursor, query.limit)
                .map(|(actor, targets)| OperatorAccountsReply::Targets {
                    challenge: challenge.clone(),
                    actor: (&actor).into(),
                    query: query.clone(),
                    targets,
                }),
        };
        self.game_accounts_output_gate(&service)
            .await
            .map_err(|_| worker::Error::RustError("operator authority unavailable".into()))?;
        self.state.storage().sync().await?;
        // Derive fresh protected metadata after scheduling/durability awaits.
        let result = match (&request, result) {
            (OperatorAccountsRequest::Authorize { token, challenge }, Ok(_)) => service
                .authorize_game_account(token.as_str())
                .map(|actor| OperatorAccountsReply::Authorized {
                    actor: (&actor).into(),
                    challenge: challenge.clone(),
                }),
            (
                OperatorAccountsRequest::Recipient {
                    token,
                    work,
                    challenge,
                },
                Ok(_),
            ) => work
                .work()
                .map_err(|_| crate::auth::AuthError::InvalidInput)
                .and_then(|binding| {
                    service.authorize_host_transfer_recipient(token.as_str(), &binding)
                })
                .map(|(actor, target)| OperatorAccountsReply::Recipient {
                    challenge: challenge.clone(),
                    actor: (&actor).into(),
                    work: work.clone(),
                    epoch: target.credential_epoch(),
                    checked_at: target.checked_at(),
                }),
            (
                OperatorAccountsRequest::Targets {
                    token,
                    query,
                    challenge,
                },
                Ok(_),
            ) => service
                .eligible_host_targets(token.as_str(), query.cursor, query.limit)
                .map(|(actor, targets)| OperatorAccountsReply::Targets {
                    challenge: challenge.clone(),
                    actor: (&actor).into(),
                    query: query.clone(),
                    targets,
                }),
            (_, result) => result,
        };
        worker::Response::from_json(
            &result.unwrap_or_else(|e| OperatorAccountsReply::Rejected { code: e.into() }),
        )
    }
}
#[cfg(target_arch = "wasm32")]
impl super::directory::GameDirectoryObject {
    pub(super) async fn execute_operator_directory(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        if request.method() != worker::Method::Post || request.url()?.query().is_some() {
            return worker::Response::error("invalid private operator request", 400);
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = axum::body::to_bytes(
            axum::body::Body::new(http.into_body()),
            game_peers::GAME_PEER_MAX_BYTES,
        )
        .await
        .map_err(|_| worker::Error::RustError("operator request bound".into()))?;
        let request: OperatorDirectoryRequest = super::decode_private_json(&bytes)
            .map_err(|_| worker::Error::RustError("invalid operator envelope".into()))?;
        let db = super::OwnerDatabase::new(self.state.storage());
        crate::db::directory::migrate_directory(&db)
            .map_err(|_| worker::Error::RustError("operator storage unavailable".into()))?;
        let rt = super::WorkerRuntime;
        let service = crate::db::directory::DirectoryService::new(&db, &rt)
            .map_err(|_| worker::Error::RustError("operator service unavailable".into()))?;
        if let OperatorDirectoryRequest::ReleaseTargets {
            challenge,
            token,
            game,
            query,
            targets: requested,
        } = &request
        {
            // Complete scheduling/durability before the final Accounts yield.
            service
                .filter_host_transfer_targets(*game, requested)
                .map_err(|_| worker::Error::RustError("target cut unavailable".into()))?;
            self.schedule_cleanup().await?;
            self.state.storage().sync().await?;
            let result = async {
                let (actor, candidates) = targets(&self.env, token.as_str(), query).await?;
                let eligible = service
                    .filter_host_transfer_targets(*game, requested)
                    .map_err(|_| GamePeerError::Unavailable)?;
                if eligible != *requested
                    || requested
                        .iter()
                        .any(|id| !candidates.iter().any(|c| c.account_id == *id))
                {
                    return Err(GamePeerError::Unauthorized);
                }
                Ok(OperatorDirectoryReply::ReleasedTargets {
                    challenge: challenge.clone(),
                    game: *game,
                    requested: requested.clone(),
                    actor: (&actor).into(),
                    targets: candidates
                        .into_iter()
                        .filter(|c| requested.contains(&c.account_id))
                        .collect(),
                })
            }
            .await;
            return worker::Response::from_json(&result.unwrap_or_else(|e| {
                OperatorDirectoryReply::Rejected {
                    code: match e {
                        GamePeerError::Unauthorized | GamePeerError::Conflict => {
                            super::game_directory::DirectoryGameRejection::Conflict
                        }
                        _ => super::game_directory::DirectoryGameRejection::Unavailable,
                    },
                }
            }));
        }
        let result = match &request {
            OperatorDirectoryRequest::ReleaseTargets { .. } => {
                Err(crate::directory::DirectoryError::ProofMismatch)
            }
            OperatorDirectoryRequest::Prepare { work, challenge } => work
                .work()
                .map_err(|_| crate::directory::DirectoryError::ProofMismatch)
                .and_then(|w| service.prepare_host_transfer(&w))
                .map(|g| OperatorDirectoryReply::Prepared {
                    challenge: challenge.clone(),
                    work: (&g.work).into(),
                }),
            OperatorDirectoryRequest::Settle {
                work,
                projection,
                challenge,
            } => work
                .work()
                .map_err(|_| crate::directory::DirectoryError::ProofMismatch)
                .and_then(|w| {
                    projection.proof().map(|p| HostTransferProof {
                        work: w,
                        projection: p,
                    })
                })
                .and_then(|p| service.settle_host_transfer(&p))
                .map(|ack| OperatorDirectoryReply::Settled {
                    challenge: challenge.clone(),
                    work: (&ack.work).into(),
                }),
            OperatorDirectoryRequest::Complete { work, challenge } => work
                .work()
                .map_err(|_| crate::directory::DirectoryError::ProofMismatch)
                .and_then(|work| service.confirm_host_transfer_completion(&work))
                .map(|()| OperatorDirectoryReply::Completed {
                    work: work.clone(),
                    challenge: challenge.clone(),
                }),
            OperatorDirectoryRequest::Targets {
                game,
                targets,
                challenge,
            } => service
                .filter_host_transfer_targets(*game, targets)
                .map(|eligible| OperatorDirectoryReply::Targets {
                    challenge: challenge.clone(),
                    game: *game,
                    requested: targets.clone(),
                    eligible,
                }),
        };
        self.schedule_cleanup().await?;
        self.state.storage().sync().await?;
        let result = match (&request, result) {
            (
                OperatorDirectoryRequest::Targets {
                    game,
                    targets,
                    challenge,
                },
                Ok(_),
            ) => service
                .filter_host_transfer_targets(*game, targets)
                .map(|eligible| OperatorDirectoryReply::Targets {
                    game: *game,
                    requested: targets.clone(),
                    eligible,
                    challenge: challenge.clone(),
                }),
            (OperatorDirectoryRequest::Prepare { work, challenge }, Ok(_)) => work
                .work()
                .map_err(|_| crate::directory::DirectoryError::ProofMismatch)
                .and_then(|w| service.prepare_host_transfer(&w))
                .map(|g| OperatorDirectoryReply::Prepared {
                    work: (&g.work).into(),
                    challenge: challenge.clone(),
                }),
            (_, result) => result,
        };
        worker::Response::from_json(
            &result.unwrap_or_else(|e| OperatorDirectoryReply::Rejected { code: e.into() }),
        )
    }
}

#[cfg(target_arch = "wasm32")]
fn directory_error(code: super::game_directory::DirectoryGameRejection) -> GamePeerError {
    use super::game_directory::DirectoryGameRejection as Code;
    match code {
        Code::Conflict | Code::AssignmentBlocked | Code::Completed | Code::UnknownGame => {
            GamePeerError::Conflict
        }
        Code::InvalidProof => GamePeerError::InvalidInput,
        Code::Unavailable => GamePeerError::Unavailable,
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransferRecoveryRequest {
    #[serde(deserialize_with = "decode_challenge")]
    pub(super) challenge: OperatorChallenge,
    #[serde(deserialize_with = "object")]
    pub(super) work: TransferWire,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransferRecoveryReply {
    #[serde(deserialize_with = "decode_challenge")]
    pub(super) challenge: OperatorChallenge,
    #[serde(deserialize_with = "object")]
    pub(super) requested: TransferWire,
    #[serde(deserialize_with = "object")]
    pub(super) work: TransferWire,
    #[serde(deserialize_with = "object")]
    pub(super) projection: super::game_directory::ProjectionWire,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransferRecoveryAck {
    #[serde(deserialize_with = "decode_challenge")]
    pub(super) challenge: OperatorChallenge,
    #[serde(deserialize_with = "object")]
    pub(super) work: TransferWire,
}
#[cfg(target_arch = "wasm32")]
impl super::directory::GameDirectoryObject {
    pub(super) async fn recover_operator_transfers(
        &self,
        service: &crate::db::directory::DirectoryService<
            '_,
            super::OwnerDatabase,
            super::WorkerRuntime,
        >,
    ) -> worker::Result<()> {
        for work in service
            .due_host_transfers()
            .map_err(|_| worker::Error::RustError("transfer recovery unavailable".into()))?
        {
            let request = TransferRecoveryRequest {
                challenge: fresh_challenge().map_err(|_| {
                    worker::Error::RustError("transfer challenge unavailable".into())
                })?,
                work: (&work).into(),
            };
            let result = async {
                if work.phase != TransferPhase::Prepared {
                    let ack: TransferRecoveryAck = game_peers::call(
                        &self.env,
                        "GAMES",
                        &work.game.to_string(),
                        "/operator-transfer-ack",
                        &request,
                        game_peers::GAME_PEER_MAX_BYTES,
                    )
                    .await?;
                    if ack.challenge != request.challenge || ack.work != request.work {
                        return Err(GamePeerError::Unavailable);
                    }
                    service
                        .confirm_host_transfer_completion(&work)
                        .map_err(|_| GamePeerError::Unavailable)?;
                    return Ok(());
                }
                let reply: TransferRecoveryReply = game_peers::call(
                    &self.env,
                    "GAMES",
                    &work.game.to_string(),
                    "/operator-transfer-probe",
                    &request,
                    game_peers::GAME_PEER_MAX_BYTES,
                )
                .await?;
                if reply.challenge != request.challenge || reply.requested != request.work {
                    return Err(GamePeerError::Unavailable);
                }
                let current = reply.work.work()?;
                if !current.same_binding(&work) || current.phase == TransferPhase::Prepared {
                    return Err(GamePeerError::Unavailable);
                }
                let proof = HostTransferProof {
                    work: current,
                    projection: reply
                        .projection
                        .proof()
                        .map_err(|_| GamePeerError::Unavailable)?,
                };
                service
                    .settle_host_transfer(&proof)
                    .map_err(|_| GamePeerError::Unavailable)?;
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| GamePeerError::Unavailable)?;
                let request = TransferRecoveryRequest {
                    challenge: fresh_challenge()?,
                    work: (&proof.work).into(),
                };
                let ack: TransferRecoveryAck = game_peers::call(
                    &self.env,
                    "GAMES",
                    &work.game.to_string(),
                    "/operator-transfer-ack",
                    &request,
                    game_peers::GAME_PEER_MAX_BYTES,
                )
                .await?;
                if ack.challenge != request.challenge || ack.work != request.work {
                    return Err(GamePeerError::Unavailable);
                }
                service
                    .confirm_host_transfer_completion(&proof.work)
                    .map_err(|_| GamePeerError::Unavailable)?;
                Ok::<(), GamePeerError>(())
            }
            .await;
            if result.is_err() {
                service
                    .retry_host_transfer(&work)
                    .map_err(|_| worker::Error::RustError("transfer retry unavailable".into()))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Closed private operator fixtures fail fast."
)]
mod tests {
    use super::*;
    #[test]
    fn operator_fresh_challenges_differ_at_identical_clock_and_entropy_failure_denies() {
        let rt = crate::auth::test_support::TestRuntime::new();
        let first = OperatorChallenge::new(&rt).unwrap();
        let second = OperatorChallenge::new(&rt).unwrap();
        assert!(first != second);
        rt.fail.set(true);
        assert!(OperatorChallenge::new(&rt).is_err());
    }
    #[test]
    fn operator_challenge_and_authority_envelope_are_closed_and_fixed_width() {
        let body =
            serde_json::json!({"action":"authorize","token":"fixture","challenge":vec![1u8;32]});
        assert!(
            game_peers::decode::<OperatorAccountsRequest>(&serde_json::to_vec(&body).unwrap())
                .is_ok()
        );
        for challenge in [
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!(vec![0u8; 32]),
            serde_json::json!(vec![1u8; 31]),
            serde_json::json!(vec![1u8; 33]),
            serde_json::json!({"0":1}),
        ] {
            let mut bad = body.clone();
            bad["challenge"] = challenge;
            assert!(
                game_peers::decode::<OperatorAccountsRequest>(&serde_json::to_vec(&bad).unwrap())
                    .is_err()
            );
        }
        let mut missing = body.clone();
        missing.as_object_mut().unwrap().remove("challenge");
        assert!(
            game_peers::decode::<OperatorAccountsRequest>(&serde_json::to_vec(&missing).unwrap())
                .is_err()
        );
        let mut extra = body;
        extra["actor"] = serde_json::json!("host");
        assert!(
            game_peers::decode::<OperatorAccountsRequest>(&serde_json::to_vec(&extra).unwrap())
                .is_err()
        );
        assert!(
            game_peers::decode::<OperatorAccountsRequest>(
                br#"{"action":"authorize","token":"fixture","challenge":null,"challenge":null}"#
            )
            .is_err()
        );
    }
}
