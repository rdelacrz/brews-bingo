#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AuthError {
    #[error("invalid input")]
    InvalidInput,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("invalid link")]
    InvalidLink,
    #[error("unauthorized")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited,
    #[error("command conflict")]
    Conflict,
    #[error("stale command")]
    StaleCommand,
    #[error("storage failure")]
    Storage,
    #[error("cryptographic operation failed")]
    Crypto,
}

impl From<crate::db::StorageError> for AuthError {
    fn from(_: crate::db::StorageError) -> Self {
        Self::Storage
    }
}
