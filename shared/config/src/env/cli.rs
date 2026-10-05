//! Native developer-client inputs; no backend application configuration.
use super::ConfigError;
use serde::Deserialize;
use std::{path::PathBuf, sync::OnceLock};
use zeroize::Zeroizing;
pub const CLI_ORIGIN_MAX_BYTES: usize = 2048;
pub use super::utility::SecretKey;

pub const BREWS_API_ORIGIN: &str = "BREWS_API_ORIGIN";
pub const BREWS_DEV_CLI_KEY: &str = "BREWS_DEV_CLI_KEY";
pub const BREWS_TLS_CA_FILE: &str = "BREWS_TLS_CA_FILE";
pub const CLI_KEYS: [&str; 3] = [BREWS_API_ORIGIN, BREWS_DEV_CLI_KEY, BREWS_TLS_CA_FILE];
static CONFIG: OnceLock<Result<CliConfig, ConfigError>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliConfig {
    #[serde(rename = "brews_api_origin")]
    pub api_origin: String,
    #[serde(rename = "brews_dev_cli_key")]
    pub dev_cli_key: SecretKey,
    #[serde(rename = "brews_tls_ca_file")]
    pub tls_ca_file: Option<PathBuf>,
}
impl CliConfig {
    pub fn new() -> Result<Self, ConfigError> {
        let mut pairs = Vec::with_capacity(CLI_KEYS.len());
        for key in CLI_KEYS {
            match std::env::var(key) {
                Ok(value) => pairs.push((key.to_owned(), Zeroizing::new(value))),
                Err(std::env::VarError::NotPresent) => {}
                Err(std::env::VarError::NotUnicode(_)) => return Err(ConfigError::Deserialization),
            }
        }
        let mut config: Self = envy::from_iter(
            pairs
                .into_iter()
                .map(|(key, mut value)| (key, std::mem::take(&mut *value))),
        )
        .map_err(|error| super::deserialize_error(error, &CLI_KEYS))?;
        config.api_origin = super::utility::validate_origin(
            &config.api_origin,
            BREWS_API_ORIGIN,
            CLI_ORIGIN_MAX_BYTES,
        )?;
        Ok(config)
    }
}
pub fn get_cli_config() -> Result<&'static CliConfig, ConfigError> {
    CONFIG
        .get_or_init(CliConfig::new)
        .as_ref()
        .map_err(|error| *error)
}
