#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("storage operation failed")]
pub struct StorageError;
