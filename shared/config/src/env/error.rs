//! Value-redacted configuration errors.

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidValueKind {
    #[error("expected nonblank display text without control characters")]
    PublicText,
    #[error("expected the fixed same-origin /api path")]
    ApiPath,
    #[error("value exceeds its byte limit")]
    TooLong,
    #[error("expected canonical unpadded base64url for a 32-byte key")]
    SecretKey,
    #[error("expected a canonical HTTPS origin")]
    Origin,
    #[error("outside approved bounds")]
    OutOfRange,
}

/// Submitted values and third-party error sources are never retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid configuration for {key}: {kind}")]
    InvalidValue {
        key: &'static str,
        kind: InvalidValueKind,
    },
    #[error("missing required configuration key: {key}")]
    MissingKey { key: &'static str },
    #[error("configuration deserialization failed")]
    Deserialization,
}
