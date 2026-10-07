//! Cloudflare entry points; private adapters, no listening TCP server.
mod database;
pub(super) mod directory;
mod edge;
mod game_accounts;
mod game_directory;
mod game_object;
mod game_peers;
mod game_recovery;
mod game_recovery_wire;
mod game_sockets;
mod game_wire;
mod games;
mod history;
mod history_owner;
mod history_wire;
mod management;
mod removals;
mod runtime;
mod users;
mod wire;

use crate::config::get_backend_config;
use crate::limits::{OWNER_CALLER_IDENTITY_MAX_BYTES, OWNER_REQUEST_MAX_BYTES};
use crate::observability::{self, Boundary, Failure};
use crate::{
    auth::{AuthError, AuthPolicy, AuthService, RequestContext, Runtime},
    db,
};
use axum::{Router, body::Body, routing::any};
use database::OwnerDatabase;
use runtime::WorkerRuntime;
use tower::ServiceExt;
use wasm_bindgen::JsValue;
use wire::{OwnerRequest, OwnerResponse};
use worker::{
    Context, DurableObject, Env, HttpRequest, Request, Response, Result, ScheduledTime, State,
    durable_object, event, send::SendWrapper,
};
use zeroize::Zeroizing;

/// Parse typed bytes directly so duplicate fields remain visible to Serde.
fn decode_private_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> std::result::Result<T, ()> {
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(());
    }
    serde_json::from_slice(bytes).map_err(|_| ())
}

#[event(fetch)]
pub async fn fetch(
    request: HttpRequest,
    env: Env,
    _context: Context,
) -> Result<http::Response<Body>> {
    crate::observability::init();
    let router = Router::new()
        .route("/api/auth/login", any(edge::handle))
        .route("/api/session", any(edge::handle))
        .route("/api/auth/enrollment/redeem", any(edge::handle))
        .route("/api/auth/enrollment/complete", any(edge::handle))
        .route("/api/auth/password-reset/redeem", any(edge::handle))
        .route("/api/auth/password-reset/complete", any(edge::handle))
        .route("/api/auth/logout", any(edge::handle))
        .route("/_dev/commands", any(management::handle))
        .route("/api/users", any(users::handle))
        .route("/api/users/{*path}", any(users::handle))
        .route("/api/history", any(history::handle))
        .route("/api/history/{*path}", any(history::handle))
        .route("/api/games", any(games::handle))
        .route("/api/games/{*path}", any(games::handle))
        .fallback(edge::handle)
        .with_state(SendWrapper::new(env));
    router
        .oneshot(request.map(Body::new))
        .await
        .map_err(|_| worker::Error::RustError("HTTP routing failed".to_owned()))
}

/// Singleton account authority per environment. There is no public admin/seed API.
#[durable_object]
pub struct AccountsObject {
    state: State,
    env: Env,
}
impl AccountsObject {
    async fn cleanup_alarm(&self) -> Result<Response> {
        let db = OwnerDatabase::new(self.state.storage());
        db::migrate(&db)
            .map_err(|_| worker::Error::RustError("storage initialization failed".to_owned()))?;
        let rt = WorkerRuntime;
        let service = self
            .service(&db, &rt)
            .map_err(|_| worker::Error::RustError("configuration unavailable".to_owned()))?;
        self.dispatch_game_socket_closes(&service)
            .await
            .map_err(|_| worker::Error::RustError("game socket close dispatch failed".into()))?;
        self.recover_removals(&service)
            .await
            .map_err(|_| worker::Error::RustError("removal recovery failed".to_owned()))?;
        service
            .cleanup()
            .map_err(|_| worker::Error::RustError("cleanup failed".to_owned()))?;
        service
            .cleanup_management()
            .map_err(|_| worker::Error::RustError("management cleanup failed".to_owned()))?;
        let deadline = Self::combined_deadline(&service)
            .map_err(|_| worker::Error::RustError("deadline lookup failed".to_owned()))?;
        self.schedule(deadline).await?;
        self.state.storage().sync().await?;
        Response::empty()
    }

    fn service<'a>(
        &'a self,
        db: &'a OwnerDatabase,
        rt: &'a WorkerRuntime,
    ) -> std::result::Result<AuthService<'a, OwnerDatabase, WorkerRuntime>, AuthError> {
        let cfg = get_backend_config(&self.env).map_err(|_| {
            observability::failure(Boundary::AccountsAuth, Failure::Configuration);
            AuthError::Crypto
        })?;
        AuthService::new(
            db,
            rt,
            AuthPolicy {
                m_cost: cfg.argon2_m_cost,
                t_cost: cfg.argon2_t_cost,
                p_cost: cfg.argon2_p_cost,
            },
            cfg.rate_limit_key.expose_secret(),
        )
    }
    async fn execute(&self, request: Request) -> std::result::Result<OwnerResponse, AuthError> {
        if request.method() != worker::Method::Post
            || request.path() != "/auth"
            || request
                .url()
                .map_err(|_| AuthError::InvalidInput)?
                .query()
                .is_some()
        {
            return Err(AuthError::InvalidInput);
        }
        let http: HttpRequest = request.try_into().map_err(|_| AuthError::InvalidInput)?;
        let bytes = Zeroizing::new(
            axum::body::to_bytes(Body::new(http.into_body()), OWNER_REQUEST_MAX_BYTES)
                .await
                .map_err(|_| AuthError::InvalidInput)?
                .to_vec(),
        );
        let message: OwnerRequest =
            serde_json::from_slice(&bytes).map_err(|_| AuthError::InvalidInput)?;
        if message.caller_identity.len() > OWNER_CALLER_IDENTITY_MAX_BYTES {
            return Err(AuthError::InvalidInput);
        }
        let db = OwnerDatabase::new(self.state.storage());
        db::migrate(&db)?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let is_completion = matches!(&message.command, crate::auth::AuthCommand::Complete { .. });
        let result = service.execute(
            message.command,
            RequestContext {
                command_id: message.command_id,
                caller_identity: message.caller_identity,
            },
        );
        if let Err(error) = &result {
            if is_completion && *error == AuthError::Unauthorized {
                observability::failure(Boundary::AccountsAuth, Failure::Forbidden);
            } else {
                observability::auth_failure(Boundary::AccountsAuth, *error);
            }
        }
        let outcome = OwnerResponse::outcome(result);
        self.dispatch_game_socket_closes(&service).await?;
        self.schedule(Self::combined_deadline(&service)?)
            .await
            .map_err(|_| AuthError::Storage)?;
        // Await the output gate before the private response can cause Set-Cookie.
        self.state.storage().sync().await.map_err(|_| {
            observability::failure(Boundary::Durability, Failure::Storage);
            AuthError::Storage
        })?;
        Ok(outcome)
    }
    fn combined_deadline(
        service: &AuthService<'_, OwnerDatabase, WorkerRuntime>,
    ) -> std::result::Result<Option<i64>, AuthError> {
        let auth = service.next_deadline()?;
        let management = service
            .next_management_deadline()
            .map_err(|_| AuthError::Storage)?;
        Ok(auth.into_iter().chain(management).min())
    }
    async fn schedule(&self, deadline: Option<i64>) -> Result<()> {
        let storage = self.state.storage();
        let current = storage.get_alarm().await?;
        match deadline {
            Some(deadline) => {
                let now = WorkerRuntime.now_ms();
                let deadline = deadline.max(now.saturating_add(1));
                if current != Some(deadline) {
                    storage
                        .set_alarm(ScheduledTime::new(js_sys::Date::new(&JsValue::from_f64(
                            deadline as f64,
                        ))))
                        .await?;
                }
            }
            None if current.is_some() => storage.delete_alarm().await?,
            None => {}
        }
        Ok(())
    }
}
impl DurableObject for AccountsObject {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        crate::observability::init();
        if request.path() == "/game-authority" {
            return self.execute_game_authority_request(request).await;
        }
        if request.path() == "/users" {
            return self.execute_users_request(request).await;
        }
        if request.path() == "/management" {
            return self.execute_management_request(request).await;
        }
        let response = self.execute(request).await.unwrap_or_else(|error| {
            observability::auth_failure(Boundary::AccountsAuth, error);
            OwnerResponse::outcome(Err(error))
        });
        Response::from_json(&response).map(|r| r.with_status(200))
    }
    async fn alarm(&self) -> Result<Response> {
        observability::init();
        let result = self.cleanup_alarm().await;
        if result.is_err() {
            observability::failure(Boundary::AccountsAlarm, Failure::Storage);
        }
        result
    }
}
