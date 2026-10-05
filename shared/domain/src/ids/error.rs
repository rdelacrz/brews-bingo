#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("invalid UUID-v7 identifier")]
pub struct IdError;
