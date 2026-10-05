//! Private Directory coordination authority, not an app or gameplay API.
//! Allocation-conscious: bounded SQL result sets own values at the database port.
use crate::db::StorageError;
use brews_domain::ids::{AccountId, OperationId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DirectoryError {
    #[error("Directory storage unavailable")]
    Storage,
    #[error("Account removal already in progress")]
    Busy,
    #[error("Account hosts a nonterminal game")]
    HostedGame,
    #[error("Removal operation target does not match")]
    OperationMismatch,
    #[error("Removal operation already completed")]
    Completed,
    #[error("Removal operation is unknown")]
    UnknownOperation,
    #[error("Trusted Directory clock unavailable")]
    Clock,
    #[error("Removal operation is outside the admission window")]
    StaleOperation,
    #[cfg(test)]
    #[error("Account assignment is blocked")]
    AssignmentBlocked,
}
impl From<StorageError> for DirectoryError {
    fn from(_: StorageError) -> Self {
        Self::Storage
    }
}

/// Proof of the matching durable Directory gate, not independent account authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemovalGrant {
    pub operation_id: OperationId,
    pub account_id: AccountId,
}

/// Secret-free release/absence acknowledgement, scoped to an exact intent.
/// Never actor authority, a no-hosted-games grant, or proof of account mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemovalReleaseAck {
    pub operation_id: OperationId,
    pub account_id: AccountId,
}
