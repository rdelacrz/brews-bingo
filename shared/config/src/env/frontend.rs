//! Public build inputs and their isolate-local singleton.

use std::sync::OnceLock;

use serde::Deserialize;

use super::{ConfigError, InvalidValueKind};

pub const API_PATH: &str = "/api";
pub const PUBLIC_APP_NAME_MAX_BYTES: usize = 128;
/// Optional public display text: nonblank, control-free and at most 128 bytes.
pub const PUBLIC_APP_NAME: &str = "PUBLIC_APP_NAME";
/// Optional compatibility assertion; `/api` is the only valid value and default.
pub const PUBLIC_API_PATH: &str = "PUBLIC_API_PATH";
pub const FRONTEND_KEYS: [&str; 2] = [PUBLIC_APP_NAME, PUBLIC_API_PATH];

static CONFIG: OnceLock<Result<PublicAppConfig, ConfigError>> = OnceLock::new();

pub fn get_frontend_config() -> Result<&'static PublicAppConfig, ConfigError> {
    CONFIG
        .get_or_init(PublicAppConfig::new)
        .as_ref()
        .map_err(|error| *error)
}

/// Public-only settings; frontend-only builds cannot import backend bindings.
#[cfg_attr(
    not(feature = "backend"),
    doc = r#"
```compile_fail
use brews_config::env::backend::BackendConfig;
```
"#
)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicAppConfig {
    #[serde(rename = "public_app_name")]
    pub application_name: Option<String>,
    #[serde(rename = "public_api_path", default = "default_api_path")]
    pub api_path: String,
}

impl PublicAppConfig {
    pub fn new() -> Result<Self, ConfigError> {
        Self::parse_from_pairs(
            [
                (PUBLIC_APP_NAME, option_env!("PUBLIC_APP_NAME")),
                (PUBLIC_API_PATH, option_env!("PUBLIC_API_PATH")),
            ]
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned()))),
        )
    }

    fn parse_from_pairs(
        input: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, ConfigError> {
        let config: Self = envy::from_iter(input)
            .map_err(|error| super::deserialize_error(error, &FRONTEND_KEYS))?;
        if let Some(name) = config.application_name.as_deref() {
            if name.len() > PUBLIC_APP_NAME_MAX_BYTES {
                return Err(ConfigError::InvalidValue {
                    key: PUBLIC_APP_NAME,
                    kind: InvalidValueKind::TooLong,
                });
            }
            if name.trim().is_empty() || name.chars().any(char::is_control) {
                return Err(ConfigError::InvalidValue {
                    key: PUBLIC_APP_NAME,
                    kind: InvalidValueKind::PublicText,
                });
            }
        }
        if config.api_path != API_PATH {
            return Err(ConfigError::InvalidValue {
                key: PUBLIC_API_PATH,
                kind: InvalidValueKind::ApiPath,
            });
        }
        Ok(config)
    }
}

fn default_api_path() -> String {
    API_PATH.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_public_settings_use_serde_defaults() -> Result<(), ConfigError> {
        let config = PublicAppConfig::parse_from_pairs([])?;
        assert_eq!(config.application_name, None);
        assert_eq!(config.api_path, API_PATH);
        Ok(())
    }

    #[test]
    fn explicit_public_values_are_preserved() -> Result<(), ConfigError> {
        let config = PublicAppConfig::parse_from_pairs([
            (PUBLIC_APP_NAME.to_owned(), " Public name ☕ ".to_owned()),
            (PUBLIC_API_PATH.to_owned(), API_PATH.to_owned()),
        ])?;
        assert_eq!(config.application_name.as_deref(), Some(" Public name ☕ "));
        assert_eq!(config.api_path, API_PATH);
        Ok(())
    }

    #[test]
    fn maximum_display_name_is_measured_in_utf8_bytes() -> Result<(), ConfigError> {
        let name = "é".repeat(64);
        let config =
            PublicAppConfig::parse_from_pairs([(PUBLIC_APP_NAME.to_owned(), name.clone())])?;
        assert_eq!(config.application_name, Some(name));
        assert!(
            PublicAppConfig::parse_from_pairs([(PUBLIC_APP_NAME.to_owned(), "é".repeat(65))])
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn supplied_display_names_cannot_be_blank_control_text_or_unbounded() {
        for value in [
            String::new(),
            "   ".to_owned(),
            "\t".to_owned(),
            "\u{2003}".to_owned(),
            "name\0PRIVATE_VALUE_MARKER".to_owned(),
            "line\nname".to_owned(),
            "x".repeat(129),
        ] {
            let error =
                PublicAppConfig::parse_from_pairs([(PUBLIC_APP_NAME.to_owned(), value)]).err();
            assert!(matches!(
                error,
                Some(ConfigError::InvalidValue {
                    key: PUBLIC_APP_NAME,
                    ..
                })
            ));
            if let Some(error) = error {
                assert_redacted(error);
            }
        }
    }

    #[test]
    fn supplied_api_path_must_be_exactly_the_locked_relative_root() {
        for value in [
            "",
            "https://other.invalid/api",
            "//other.invalid/api",
            "/api/",
            "/other",
            "api",
            "/api?key=PRIVATE_VALUE_MARKER",
            "/api#fragment",
            "/%61pi",
            " /api",
            "/api\n",
            "\\other.invalid\\api",
        ] {
            let error =
                PublicAppConfig::parse_from_pairs([(PUBLIC_API_PATH.to_owned(), value.to_owned())])
                    .err();
            assert_eq!(
                error,
                Some(ConfigError::InvalidValue {
                    key: PUBLIC_API_PATH,
                    kind: InvalidValueKind::ApiPath
                })
            );
            if let Some(error) = error {
                assert_redacted(error);
            }
        }
    }

    #[test]
    fn serde_rejects_unknown_keys_with_redacted_errors() {
        for key in ["RATE_LIMIT_KEY", "APP_ORIGIN", "PUBLIC_UNREGISTERED_MARKER"] {
            let error = PublicAppConfig::parse_from_pairs([(
                key.to_owned(),
                "PRIVATE_VALUE_MARKER".to_owned(),
            )])
            .err();
            assert_eq!(error, Some(ConfigError::Deserialization));
            if let Some(error) = error {
                assert_redacted(error);
                assert!(!format!("{error:?} {error}").contains(key));
            }
        }
    }

    fn assert_redacted(error: ConfigError) {
        assert!(!format!("{error:?} {error}").contains("PRIVATE_VALUE_MARKER"));
        assert!(std::error::Error::source(&error).is_none());
    }
}
