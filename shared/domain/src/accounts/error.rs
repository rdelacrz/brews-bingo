#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("invalid account input")]
pub struct ValidationError;
