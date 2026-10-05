//! Safe transport errors; never retain request/header/parser values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("invalid request")]
    InvalidInput,
    #[error("origin rejected")]
    Forbidden,
    #[error("route not found")]
    NotFound,
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("request body too large")]
    PayloadTooLarge,
    #[error("JSON content type required")]
    UnsupportedMediaType,
    #[error("service unavailable")]
    Unavailable,
}
