//! Fresh Accounts output checks and exact expiry acknowledgements.
use super::{AccountsObject, OwnerDatabase, WorkerRuntime, game_peers};
use crate::{
    api::ApiError,
    auth::{AuthError, AuthOutputAuthority, Runtime},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputRequest {
    token: game_peers::SecretToken,
    #[serde(deserialize_with = "game_peers::object")]
    authority: AuthOutputAuthority,
    #[serde(deserialize_with = "game_peers::optional_object")]
    body: Option<serde_json::Value>,
    observed_at: Option<i64>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
enum OutputReply {
    Current {
        #[serde(deserialize_with = "game_peers::object")]
        authority: AuthOutputAuthority,
    },
    ExpiryFenced {
        #[serde(deserialize_with = "game_peers::object")]
        authority: AuthOutputAuthority,
        observed_at: i64,
    },
    Denied {},
}
impl AccountsObject {
    pub(super) async fn execute_auth_output_request(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        if request.method() != worker::Method::Post || request.url()?.query().is_some() {
            return worker::Response::error("invalid output request", 400);
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            axum::body::to_bytes(
                axum::body::Body::new(http.into_body()),
                crate::limits::OWNER_REQUEST_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("output request bound".into()))?
            .to_vec(),
        );
        let request: OutputRequest = super::decode_private_json(&bytes)
            .map_err(|_| worker::Error::RustError("invalid output request".into()))?;
        let result = async {
            let db = OwnerDatabase::new(self.state.storage());
            crate::db::migrate(&db)?;
            let service = self.service(&db, &WorkerRuntime)?;
            if let Some(observed_at) = request.observed_at {
                if request.body.is_some() {
                    return Err(AuthError::InvalidInput);
                }
                service.fence_auth_output_expiry(
                    request.token.as_str(),
                    &request.authority,
                    observed_at,
                )?;
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| AuthError::Storage)?;
                return Ok(OutputReply::ExpiryFenced {
                    authority: request.authority.clone(),
                    observed_at,
                });
            }
            self.state
                .storage()
                .sync()
                .await
                .map_err(|_| AuthError::Storage)?;
            let observed_at = WorkerRuntime.now_ms();
            if observed_at >= request.authority.expires_at {
                service.fence_auth_output_expiry(
                    request.token.as_str(),
                    &request.authority,
                    observed_at,
                )?;
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| AuthError::Storage)?;
                return Err(AuthError::Unauthorized);
            }
            let release = service.validate_auth_output(
                request.token.as_str(),
                &request.authority,
                request.body.as_ref().ok_or(AuthError::InvalidInput)?,
            );
            if let Err(error) = release {
                self.state
                    .storage()
                    .sync()
                    .await
                    .map_err(|_| AuthError::Storage)?;
                return Err(error);
            }
            Ok(OutputReply::Current {
                authority: request.authority.clone(),
            })
        }
        .await;
        worker::Response::from_json(&result.unwrap_or(OutputReply::Denied {}))
    }
}
pub(super) async fn check(
    env: &worker::Env,
    token: &str,
    authority: &AuthOutputAuthority,
    body: &serde_json::Value,
) -> Result<(), ApiError> {
    let request = OutputRequest {
        token: game_peers::SecretToken::new(token),
        authority: authority.clone(),
        body: Some(body.clone()),
        observed_at: None,
    };
    let reply: OutputReply = game_peers::call(
        env,
        "ACCOUNTS",
        "accounts",
        "/auth-output",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    if !matches!(reply, OutputReply::Current { authority: found } if found == *authority) {
        return Err(ApiError::Unavailable);
    }
    Ok(())
}
pub(super) async fn fence(
    env: &worker::Env,
    token: &str,
    authority: &AuthOutputAuthority,
    observed_at: i64,
) -> Result<(), ApiError> {
    let request = OutputRequest {
        token: game_peers::SecretToken::new(token),
        authority: authority.clone(),
        body: None,
        observed_at: Some(observed_at),
    };
    let reply: OutputReply = game_peers::call(
        env,
        "ACCOUNTS",
        "accounts",
        "/auth-output",
        &request,
        game_peers::GAME_PEER_MAX_BYTES,
    )
    .await
    .map_err(|_| ApiError::Unavailable)?;
    if matches!(reply, OutputReply::ExpiryFenced { authority: found, observed_at: sample } if found == *authority && sample == observed_at)
    {
        Ok(())
    } else {
        Err(ApiError::Unavailable)
    }
}
