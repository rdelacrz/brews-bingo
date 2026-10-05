//! Cloudflare entry points; private adapters, no listening TCP server.
mod database;
mod edge;
mod runtime;
mod wire;

use crate::config::get_backend_config;
use crate::limits::{OWNER_CALLER_IDENTITY_MAX_BYTES, OWNER_REQUEST_MAX_BYTES};
use crate::{
    auth::{AuthError, AuthPolicy, AuthService, RequestContext, Runtime},
    storage,
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

#[event(fetch)]
pub async fn fetch(
    request: HttpRequest,
    env: Env,
    _context: Context,
) -> Result<http::Response<Body>> {
    let router = Router::new()
        .route("/api/auth/login", any(edge::handle))
        .route("/api/session", any(edge::handle))
        .route("/api/auth/enrollment/redeem", any(edge::handle))
        .route("/api/auth/enrollment/complete", any(edge::handle))
        .route("/api/auth/password-reset/redeem", any(edge::handle))
        .route("/api/auth/password-reset/complete", any(edge::handle))
        .route("/api/auth/logout", any(edge::handle))
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
    fn service<'a>(
        &'a self,
        db: &'a OwnerDatabase,
        rt: &'a WorkerRuntime,
    ) -> std::result::Result<AuthService<'a, OwnerDatabase, WorkerRuntime>, AuthError> {
        let cfg = get_backend_config(&self.env).map_err(|_| AuthError::Crypto)?;
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
        storage::migrate(&db)?;
        let rt = WorkerRuntime;
        let service = self.service(&db, &rt)?;
        let outcome = OwnerResponse::outcome(service.execute(
            message.command,
            RequestContext {
                command_id: message.command_id,
                caller_identity: message.caller_identity,
            },
        ));
        self.schedule(service.next_deadline()?)
            .await
            .map_err(|_| AuthError::Storage)?;
        // Await the output gate before the private response can cause Set-Cookie.
        self.state
            .storage()
            .sync()
            .await
            .map_err(|_| AuthError::Storage)?;
        Ok(outcome)
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
        let response = self
            .execute(request)
            .await
            .unwrap_or_else(|error| OwnerResponse::outcome(Err(error)));
        Response::from_json(&response).map(|r| r.with_status(200))
    }
    async fn alarm(&self) -> Result<Response> {
        let db = OwnerDatabase::new(self.state.storage());
        storage::migrate(&db)
            .map_err(|_| worker::Error::RustError("storage initialization failed".to_owned()))?;
        let rt = WorkerRuntime;
        let service = self
            .service(&db, &rt)
            .map_err(|_| worker::Error::RustError("configuration unavailable".to_owned()))?;
        service
            .cleanup()
            .map_err(|_| worker::Error::RustError("cleanup failed".to_owned()))?;
        let deadline = service
            .next_deadline()
            .map_err(|_| worker::Error::RustError("deadline lookup failed".to_owned()))?;
        self.schedule(deadline).await?;
        self.state.storage().sync().await?;
        Response::empty()
    }
}
