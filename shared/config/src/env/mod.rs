//! Independent backend/private and frontend/public typed environment settings.

#[cfg(feature = "backend")]
pub mod backend;
#[cfg(all(feature = "cli", not(target_arch = "wasm32")))]
pub mod cli;
pub mod error;
#[cfg(feature = "frontend")]
pub mod frontend;
#[cfg(any(feature = "backend", feature = "cli"))]
mod utility;

pub use error::{ConfigError, InvalidValueKind};

/// Provider adapters return absence separately from unusable bindings.
#[cfg(feature = "backend")]
pub trait EnvSource {
    fn value(&self, key: &'static str) -> Result<Option<String>, ConfigError>;
}

#[cfg(any(feature = "backend", feature = "frontend", feature = "cli"))]
fn deserialize_error(error: envy::Error, keys: &[&'static str]) -> ConfigError {
    match error {
        envy::Error::MissingValue(missing) => keys
            .iter()
            .copied()
            .find(|key| key.eq_ignore_ascii_case(missing))
            .map(|key| ConfigError::MissingKey { key })
            .unwrap_or(ConfigError::Deserialization),
        // Envy's custom errors may include submitted values; never retain them.
        envy::Error::Custom(_) => ConfigError::Deserialization,
    }
}

#[cfg(all(test, any(feature = "backend", feature = "frontend")))]
mod tests {
    use super::*;

    #[test]
    fn missing_values_only_identify_known_keys() {
        assert_eq!(
            deserialize_error(envy::Error::MissingValue("known_key"), &["KNOWN_KEY"]),
            ConfigError::MissingKey { key: "KNOWN_KEY" }
        );
        let error = deserialize_error(
            envy::Error::MissingValue("PRIVATE_VALUE_MARKER"),
            &["KNOWN_KEY"],
        );
        assert_eq!(error, ConfigError::Deserialization);
        assert!(!format!("{error:?} {error}").contains("PRIVATE_VALUE_MARKER"));
    }

    #[test]
    fn third_party_deserialization_errors_are_discarded() {
        let error = deserialize_error(
            envy::Error::Custom("PRIVATE_VALUE_MARKER".to_owned()),
            &["KNOWN_KEY"],
        );
        assert_eq!(error, ConfigError::Deserialization);
        assert!(!format!("{error:?} {error}").contains("PRIVATE_VALUE_MARKER"));
        assert!(std::error::Error::source(&error).is_none());
    }
}
