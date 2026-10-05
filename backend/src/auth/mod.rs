//! Account auth authority (allocation-conscious, bounded KDF/SQL work).
use crate::storage::Database;
use brews_config::env::backend::{
    ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_PARALLELISM,
};
use std::fmt;
mod command;
pub use command::AuthCommand;

pub struct RequestContext {
    pub command_id: Option<String>,
    pub caller_identity: String,
}
impl fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RequestContext([redacted])")
    }
}
pub enum CookieEffect {
    None,
    Set { token: String, expires_at: i64 },
    Clear,
}
impl fmt::Debug for CookieEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Set { .. } => "Set([redacted])",
            Self::Clear => "Clear",
        })
    }
}
pub struct AuthOutcome {
    pub body: serde_json::Value,
    pub cookie: CookieEffect,
}
impl fmt::Debug for AuthOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthOutcome")
            .field("body", &self.body)
            .field("cookie", &self.cookie)
            .finish()
    }
}
pub struct AuthService<'a, D: Database, R: Runtime> {
    db: &'a D,
    runtime: &'a R,
    policy: AuthPolicy,
    rate_key: &'a [u8],
}
mod management;
mod rate;
mod receipts;
mod records;
mod service;
pub use management::{
    ManagementError, ManagementPrincipal, RemovalGateGrant, RemovalPhase, RemovalReleaseAck,
    RemovalWork,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthPolicy {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}
impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            m_cost: ARGON2_DEFAULT_MEMORY_KIB,
            t_cost: ARGON2_DEFAULT_ITERATIONS,
            p_cost: ARGON2_PARALLELISM,
        }
    }
}

pub trait Runtime {
    fn now_ms(&self) -> i64;
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError>;
}
mod error;
pub use error::AuthError;

#[cfg(test)]
mod tests;
