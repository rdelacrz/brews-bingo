//! Environment parsing helpers, separate from declared settings.
mod origin;
mod secret;
pub(super) use origin::validate_origin;
pub use secret::SecretKey;
