//! Private Directory peer transport; never routed at the public Worker boundary.
use super::{OwnerDatabase, WorkerRuntime};
use crate::{
    auth::Runtime,
    db::directory::{DirectoryService, migrate_directory},
    directory::DirectoryError,
    limits::OWNER_REQUEST_MAX_BYTES,
    observability::{self, Boundary, Failure},
};
use axum::body::{Body, to_bytes};
use brews_domain::ids::{AccountId, OperationId};
use serde::{Deserialize, Serialize};
use worker::{DurableObject, Env, Request, Response, Result, State, durable_object};

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DirectoryRequest {
    Acquire {
        operation_id: OperationId,
        account_id: AccountId,
    },
    Release {
        operation_id: OperationId,
        account_id: AccountId,
    },
    Reconcile {
        operation_id: OperationId,
        account_id: AccountId,
    },
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DirectoryResponse {
    Acquired {
        operation_id: OperationId,
        account_id: AccountId,
    },
    Released {
        operation_id: OperationId,
        account_id: AccountId,
    },
    Rejected {
        code: DirectoryRejection,
    },
}
#[derive(Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DirectoryRejection {
    HostedGame,
    Busy,
    Completed,
    StaleOperation,
    UnknownOperation,
    InvalidOperation,
    Unavailable,
}

#[durable_object]
pub struct GameDirectoryObject {
    state: State,
}
impl DurableObject for GameDirectoryObject {
    fn new(state: State, _env: Env) -> Self {
        Self { state }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        observability::init();
        let result = self.execute(request).await.unwrap_or_else(|_| {
            observability::failure(Boundary::DirectoryRequest, Failure::Unavailable);
            DirectoryResponse::Rejected {
                code: DirectoryRejection::Unavailable,
            }
        });
        self.schedule_cleanup().await.inspect_err(|_| {
            observability::failure(Boundary::DirectoryRequest, Failure::Storage);
        })?;
        self.state.storage().sync().await.inspect_err(|_| {
            observability::failure(Boundary::Durability, Failure::Storage);
        })?;
        Response::from_json(&result)
    }
    async fn alarm(&self) -> Result<Response> {
        observability::init();
        let result = self.cleanup_alarm().await;
        if result.is_err() {
            observability::failure(Boundary::DirectoryAlarm, Failure::Storage);
        }
        result
    }
}
impl GameDirectoryObject {
    async fn cleanup_alarm(&self) -> Result<Response> {
        let db = OwnerDatabase::new(self.state.storage());
        migrate_directory(&db)
            .map_err(|_| worker::Error::RustError("Directory initialization failed".into()))?;
        let rt = WorkerRuntime;
        let service = DirectoryService::new(&db, &rt)
            .map_err(|_| worker::Error::RustError("Directory unavailable".into()))?;
        service
            .cleanup()
            .map_err(|_| worker::Error::RustError("Directory cleanup failed".into()))?;
        self.schedule_cleanup().await?;
        self.state.storage().sync().await?;
        Response::empty()
    }

    async fn schedule_cleanup(&self) -> Result<()> {
        let storage = self.state.storage();
        let db = OwnerDatabase::new(self.state.storage());
        migrate_directory(&db)
            .map_err(|_| worker::Error::RustError("Directory initialization failed".into()))?;
        let rt = WorkerRuntime;
        let service = DirectoryService::new(&db, &rt)
            .map_err(|_| worker::Error::RustError("Directory unavailable".into()))?;
        let current = storage.get_alarm().await?;
        match service
            .next_deadline()
            .map_err(|_| worker::Error::RustError("Directory deadline unavailable".into()))?
        {
            Some(deadline) => {
                let deadline = deadline.max(rt.now_ms().saturating_add(1));
                if current != Some(deadline) {
                    storage
                        .set_alarm(worker::ScheduledTime::new(js_sys::Date::new(
                            &wasm_bindgen::JsValue::from_f64(deadline as f64),
                        )))
                        .await?;
                }
            }
            None if current.is_some() => storage.delete_alarm().await?,
            None => {}
        }
        Ok(())
    }
    async fn execute(&self, request: Request) -> Result<DirectoryResponse> {
        if request.method() != worker::Method::Post
            || request.path() != "/removal"
            || request.url()?.query().is_some()
        {
            return Ok(DirectoryResponse::Rejected {
                code: DirectoryRejection::InvalidOperation,
            });
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = to_bytes(Body::new(http.into_body()), OWNER_REQUEST_MAX_BYTES)
            .await
            .map_err(|_| worker::Error::RustError("Directory request bound exceeded".into()))?;
        let message: DirectoryRequest = super::decode_private_json(&bytes).map_err(|_| {
            observability::failure(Boundary::DirectoryRequest, Failure::InvalidInput);
            worker::Error::RustError("Directory request rejected".into())
        })?;
        let db = OwnerDatabase::new(self.state.storage());
        migrate_directory(&db)
            .map_err(|_| worker::Error::RustError("Directory initialization failed".into()))?;
        let rt = WorkerRuntime;
        let service = DirectoryService::new(&db, &rt)
            .map_err(|_| worker::Error::RustError("Directory unavailable".into()))?;
        let result = match message {
            DirectoryRequest::Acquire {
                operation_id,
                account_id,
            } => service
                .acquire_removal(operation_id, account_id)
                .map(|grant| DirectoryResponse::Acquired {
                    operation_id: grant.operation_id,
                    account_id: grant.account_id,
                }),
            DirectoryRequest::Release {
                operation_id,
                account_id,
            } => service
                .release_removal(operation_id, account_id)
                .map(|ack| DirectoryResponse::Released {
                    operation_id: ack.operation_id,
                    account_id: ack.account_id,
                }),
            DirectoryRequest::Reconcile {
                operation_id,
                account_id,
            } => service
                .reconcile_removal(operation_id, account_id)
                .map(|ack| DirectoryResponse::Released {
                    operation_id: ack.operation_id,
                    account_id: ack.account_id,
                }),
        };
        Ok(result.unwrap_or_else(|error| {
            match error {
                DirectoryError::Storage | DirectoryError::Clock => {
                    observability::failure(Boundary::DirectoryRequest, Failure::Storage);
                }
                DirectoryError::OperationMismatch => {
                    observability::failure(Boundary::DirectoryRequest, Failure::ProofMismatch);
                }
                _ => {}
            }
            DirectoryResponse::Rejected {
                code: match error {
                    DirectoryError::HostedGame => DirectoryRejection::HostedGame,
                    DirectoryError::Busy => DirectoryRejection::Busy,
                    DirectoryError::Completed => DirectoryRejection::Completed,
                    DirectoryError::StaleOperation => DirectoryRejection::StaleOperation,
                    DirectoryError::UnknownOperation => DirectoryRejection::UnknownOperation,
                    DirectoryError::OperationMismatch => DirectoryRejection::InvalidOperation,
                    _ => DirectoryRejection::Unavailable,
                },
            }
        }))
    }
}
