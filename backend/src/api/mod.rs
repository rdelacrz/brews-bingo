//! Bounded HTTP transport parsing.
mod decode;
pub mod error;
pub use decode::{BODY_LIMIT, Decoded, Operation, Payload, decode};
pub use error::ApiError;
