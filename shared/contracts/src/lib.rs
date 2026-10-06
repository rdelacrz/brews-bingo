//! Safe public account/session projections; no transport credentials.
#![forbid(unsafe_code)]
#[cfg(feature = "games")]
pub mod games;
#[cfg(feature = "management")]
pub mod management;
#[cfg(feature = "users")]
pub mod users;
use brews_domain::{
    accounts::{AccountRole, AccountStatus, SessionScope},
    ids::AccountId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafeAccount {
    pub account_id: AccountId,
    pub username: String,
    pub role: AccountRole,
    pub status: AccountStatus,
    pub disabled: bool,
    pub created_at: i64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SessionKind {
    Account,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionView {
    pub kind: SessionKind,
    pub scope: SessionScope,
    pub expires_at: i64,
    pub account_id: AccountId,
    pub role: AccountRole,
}
