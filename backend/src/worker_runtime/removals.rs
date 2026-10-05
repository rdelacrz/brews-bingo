//! Accounts-owned recovery of private Directory gate calls.
use super::{
    AccountsObject, OwnerDatabase, WorkerRuntime,
    directory::{DirectoryRejection, DirectoryRequest, DirectoryResponse},
};
use crate::{
    auth::{
        AuthService, ManagementError, ManagementPrincipal, RemovalGateGrant, RemovalPhase,
        RemovalReleaseAck, RemovalWork,
    },
    limits::OWNER_RESPONSE_MAX_BYTES,
    observability::{self, Boundary, Failure, RemovalEvent},
};
use brews_contracts::management::{
    AuditActor, ManagementCommand, ManagementResponse, ReceiptOperation,
};
use brews_domain::ids::{CommandId, OperationId};
use worker::{Headers, Method, Request, RequestInit};

impl AccountsObject {
    pub(super) async fn progress_removal(
        &self,
        service: &AuthService<'_, OwnerDatabase, WorkerRuntime>,
        operation_id: OperationId,
        command: &ManagementCommand,
        command_id: CommandId,
        principal: &ManagementPrincipal,
    ) -> std::result::Result<ManagementResponse, ManagementError> {
        let result = self
            .progress_work(service, operation_id, command, command_id, principal)
            .await;
        // A concurrent handler may have finished while this handler awaited a peer.
        let resolution = service.removal_resolution(operation_id, command, principal, command_id);
        if matches!(principal, ManagementPrincipal::AdminSession(_))
            && let Err(error @ (ManagementError::Unauthorized | ManagementError::Forbidden)) =
                &resolution
        {
            // Cleanup uses the already-authorized intent, never the denied issuer's bearer.
            if let Some(work) = service.removal_operation(operation_id)? {
                let cleanup = self.recover_session_removal(service, &work).await;
                if let Err(failure @ (ManagementError::Storage | ManagementError::Crypto)) = cleanup
                {
                    observability::management_failure(Boundary::AccountsManagement, failure);
                    if service.removal_operation(operation_id)?.is_some() {
                        service.retry_removal(operation_id)?;
                    }
                }
            }
            return Err(*error);
        }
        if !matches!(resolution, Ok(ManagementResponse::Pending { .. })) {
            if resolution.is_err()
                && matches!(
                    result,
                    Err(ManagementError::HostedGame
                        | ManagementError::StaleCommand
                        | ManagementError::LastAdmin
                        | ManagementError::Forbidden)
                )
            {
                return result;
            }
            return resolution;
        }
        if matches!(
            result,
            Ok(ManagementResponse::Pending { .. })
                | Err(ManagementError::Storage | ManagementError::Crypto | ManagementError::Busy)
        ) {
            if let Err(error) = &result {
                observability::management_failure(Boundary::AccountsManagement, *error);
            }
            service.retry_removal(operation_id)?;
            observability::removal(RemovalEvent::RetryScheduled, operation_id);
            return Ok(ManagementResponse::Pending { operation_id });
        }
        result
    }
    async fn progress_work(
        &self,
        service: &AuthService<'_, OwnerDatabase, WorkerRuntime>,
        operation_id: OperationId,
        command: &ManagementCommand,
        command_id: CommandId,
        principal: &ManagementPrincipal,
    ) -> std::result::Result<ManagementResponse, ManagementError> {
        let mut rejection = ManagementError::Conflict;
        loop {
            let resolution =
                service.removal_resolution(operation_id, command, principal, command_id)?;
            let Some(work) = service.removal_operation(operation_id)? else {
                return Ok(resolution);
            };
            let request = if work.phase == RemovalPhase::Prepared {
                DirectoryRequest::Acquire {
                    operation_id,
                    account_id: work.target_account_id,
                }
            } else {
                DirectoryRequest::Reconcile {
                    operation_id,
                    account_id: work.target_account_id,
                }
            };
            let reply = self.call_directory(service, request).await;
            // No snapshot or late grant may outlive a phase transition or completion.
            let resolution =
                service.removal_resolution(operation_id, command, principal, command_id)?;
            let Some(current) = service.removal_operation(operation_id)? else {
                return Ok(resolution);
            };
            if current.phase != work.phase {
                continue;
            }
            let reply = reply?;
            match (current.phase, reply) {
                (
                    RemovalPhase::Prepared,
                    DirectoryResponse::Acquired {
                        operation_id: found,
                        account_id,
                    },
                ) if found == operation_id && account_id == current.target_account_id => {
                    let grant = RemovalGateGrant::verified(operation_id, account_id);
                    if let Err(error) =
                        service.commit_removal_with_principal(operation_id, principal, &grant)
                    {
                        if matches!(error, ManagementError::Storage | ManagementError::Crypto) {
                            return Err(error);
                        }
                        service.begin_removal_abort(operation_id)?;
                        observability::removal(RemovalEvent::AbortStarted, operation_id);
                        rejection = error;
                    }
                }
                (RemovalPhase::Prepared, DirectoryResponse::Rejected { code }) => {
                    rejection = match code {
                        DirectoryRejection::HostedGame => ManagementError::HostedGame,
                        DirectoryRejection::StaleOperation => ManagementError::StaleCommand,
                        DirectoryRejection::Completed => ManagementError::Conflict,
                        DirectoryRejection::Busy => return Err(ManagementError::Busy),
                        _ => return Err(ManagementError::Storage),
                    };
                    service.begin_removal_abort(operation_id)?;
                    observability::removal(RemovalEvent::AbortStarted, operation_id);
                }
                (
                    RemovalPhase::Committed,
                    DirectoryResponse::Released {
                        operation_id: found,
                        account_id,
                    },
                ) if found == operation_id && account_id == current.target_account_id => {
                    let outcome = service.finish_removal(
                        operation_id,
                        &RemovalReleaseAck::verified(operation_id, account_id),
                    )?;
                    if matches!(outcome, ManagementResponse::Pending { .. }) {
                        observability::removal(
                            RemovalEvent::AwaitingAcknowledgements,
                            operation_id,
                        );
                    }
                    return Ok(outcome);
                }
                (
                    RemovalPhase::Aborting,
                    DirectoryResponse::Released {
                        operation_id: found,
                        account_id,
                    },
                ) if found == operation_id && account_id == current.target_account_id => {
                    service.abort_removal(
                        operation_id,
                        &RemovalReleaseAck::verified(operation_id, account_id),
                    )?;
                    observability::removal(RemovalEvent::Aborted, operation_id);
                    return Err(rejection);
                }
                (_, DirectoryResponse::Rejected { .. }) => {
                    observability::failure(Boundary::DirectoryPeer, Failure::Unavailable);
                    return Err(ManagementError::Storage);
                }
                _ => {
                    observability::failure(Boundary::DirectoryPeer, Failure::ProofMismatch);
                    return Err(ManagementError::Storage);
                }
            }
        }
    }
    async fn call_directory(
        &self,
        service: &AuthService<'_, OwnerDatabase, WorkerRuntime>,
        message: DirectoryRequest,
    ) -> std::result::Result<DirectoryResponse, ManagementError> {
        let namespace = self
            .env
            .durable_object("GAME_DIRECTORY")
            .map_err(|_| ManagementError::Storage)?;
        let stub = namespace
            .id_from_name("directory")
            .and_then(|id| id.get_stub())
            .map_err(|_| ManagementError::Storage)?;
        let body = serde_json::to_string(&message).map_err(|_| ManagementError::Storage)?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(body.into()));
        let headers = Headers::new();
        headers
            .set("Content-Type", "application/json")
            .map_err(|_| ManagementError::Storage)?;
        init.with_headers(headers);
        let request = Request::new_with_init("https://directory.internal/removal", &init)
            .map_err(|_| ManagementError::Storage)?;
        // Intent and its recovery alarm must survive a crash during the peer call.
        self.schedule(Self::combined_deadline(service).map_err(|_| ManagementError::Storage)?)
            .await
            .map_err(|_| ManagementError::Storage)?;
        self.state.storage().sync().await.map_err(|_| {
            observability::failure(Boundary::Durability, Failure::Storage);
            ManagementError::Storage
        })?;
        if let DirectoryRequest::Reconcile { operation_id, .. } = message
            && let Ok(Some(work)) = service.removal_operation(operation_id)
            && work.phase == RemovalPhase::Committed
        {
            observability::removal(RemovalEvent::Committed, operation_id);
        }
        let response = stub.fetch_with_request(request).await.map_err(|_| {
            observability::failure(Boundary::DirectoryPeer, Failure::PeerTransport);
            ManagementError::Storage
        })?;
        if response.status_code() != 200 {
            observability::failure(Boundary::DirectoryPeer, Failure::PeerProtocol);
            return Err(ManagementError::Storage);
        }
        let http: worker::HttpResponse =
            response.try_into().map_err(|_| ManagementError::Storage)?;
        let content = axum::body::to_bytes(
            axum::body::Body::new(http.into_body()),
            OWNER_RESPONSE_MAX_BYTES,
        )
        .await
        .map_err(|_| {
            observability::failure(Boundary::DirectoryPeer, Failure::PeerProtocol);
            ManagementError::Storage
        })?;
        super::decode_private_json(&content).map_err(|_| {
            observability::failure(Boundary::DirectoryPeer, Failure::PeerProtocol);
            ManagementError::Storage
        })
    }
    pub(super) async fn recover_removals(
        &self,
        service: &AuthService<'_, OwnerDatabase, WorkerRuntime>,
    ) -> std::result::Result<(), ManagementError> {
        for work in service.removal_work(crate::limits::REMOVAL_RECOVERY_BATCH_SIZE)? {
            let command = match work.operation {
                ReceiptOperation::DisableAccount => ManagementCommand::DisableAccount {
                    account_id: work.target_account_id,
                },
                ReceiptOperation::DeleteAccount => ManagementCommand::DeleteAccount {
                    account_id: work.target_account_id,
                },
                _ => return Err(ManagementError::Storage),
            };
            let result = if work.actor == AuditActor::DeveloperCli {
                self.progress_removal(
                    service,
                    work.operation_id,
                    &command,
                    work.command_id,
                    &ManagementPrincipal::DeveloperCli,
                )
                .await
            } else {
                self.recover_session_removal(service, &work).await
            };
            match result {
                Ok(_)
                | Err(
                    ManagementError::Conflict
                    | ManagementError::StaleCommand
                    | ManagementError::HostedGame
                    | ManagementError::NotFound,
                ) => {}
                Err(_) => return Err(ManagementError::Storage),
            }
        }
        Ok(())
    }
    async fn recover_session_removal(
        &self,
        service: &AuthService<'_, OwnerDatabase, WorkerRuntime>,
        work: &RemovalWork,
    ) -> std::result::Result<ManagementResponse, ManagementError> {
        let Some(current) = service.removal_operation(work.operation_id)? else {
            return Err(ManagementError::Conflict);
        };
        if current.phase == RemovalPhase::Prepared {
            service.begin_removal_abort(work.operation_id)?;
            observability::removal(RemovalEvent::AbortStarted, work.operation_id);
        }
        let reply = self
            .call_directory(
                service,
                DirectoryRequest::Reconcile {
                    operation_id: work.operation_id,
                    account_id: work.target_account_id,
                },
            )
            .await;
        let Some(current) = service.removal_operation(work.operation_id)? else {
            return Err(ManagementError::Conflict);
        };
        match reply {
            Ok(DirectoryResponse::Released {
                operation_id,
                account_id,
            }) if operation_id == current.operation_id
                && account_id == current.target_account_id =>
            {
                let ack = RemovalReleaseAck::verified(operation_id, account_id);
                if current.phase == RemovalPhase::Committed {
                    let outcome = service.finish_removal(operation_id, &ack)?;
                    if matches!(outcome, ManagementResponse::Pending { .. }) {
                        service.retry_removal(operation_id)?;
                        observability::removal(RemovalEvent::RetryScheduled, operation_id);
                    }
                    Ok(outcome)
                } else {
                    service.abort_removal(operation_id, &ack)?;
                    Err(ManagementError::Conflict)
                }
            }
            _ => {
                service.retry_removal(current.operation_id)?;
                observability::removal(RemovalEvent::RetryScheduled, current.operation_id);
                Ok(ManagementResponse::Pending {
                    operation_id: current.operation_id,
                })
            }
        }
    }
}
