//! Minimal immutable serving-version identity; never returns runtime settings or secrets.
use crate::api::ApiError;
use axum::{
    body::to_bytes,
    extract::{Request, State},
    response::Response,
};
use serde::{Deserialize, Serialize};
use worker::{
    Env,
    send::{SendFuture, SendWrapper},
};

#[derive(Deserialize)]
struct CloudflareVersion {
    id: String,
}

#[derive(Serialize)]
struct DeploymentIdentity {
    version: u8,
    version_id: String,
}

fn identity(env: &Env) -> Result<DeploymentIdentity, ApiError> {
    let metadata: CloudflareVersion = env
        .object_var("CF_VERSION_METADATA")
        .map_err(|_| ApiError::Unavailable)?;
    let id = metadata
        .id
        .parse::<uuid::Uuid>()
        .map_err(|_| ApiError::Unavailable)?;
    if id.is_nil() || id.to_string() != metadata.id {
        return Err(ApiError::Unavailable);
    }
    Ok(DeploymentIdentity {
        version: 1,
        version_id: metadata.id,
    })
}

pub(super) async fn handle(State(env): State<SendWrapper<Env>>, request: Request) -> Response {
    SendFuture::new(async move {
        let result = async {
            if request.uri().scheme_str() != Some("https") {
                return Err(ApiError::Forbidden);
            }
            if request.method() != http::Method::GET {
                return Err(ApiError::MethodNotAllowed);
            }
            if request.uri().query().is_some() {
                return Err(ApiError::InvalidInput);
            }
            let body = to_bytes(request.into_body(), 0)
                .await
                .map_err(|_| ApiError::InvalidInput)?;
            if !body.is_empty() {
                return Err(ApiError::InvalidInput);
            }
            let bytes = serde_json::to_vec(&identity(&env)?).map_err(|_| ApiError::Unavailable)?;
            super::response::json_bytes(200, bytes)
        }
        .await;
        result.unwrap_or_else(super::edge::failure)
    })
    .await
}
