//! Backend bindings, typed configuration and its isolate-local singleton.

use std::sync::OnceLock;

use serde::Deserialize;
use zeroize::Zeroizing;

pub use super::utility::SecretKey;
use super::{ConfigError, EnvSource, InvalidValueKind};

/// Required canonical HTTPS origin; no default.
pub const APP_ORIGIN: &str = "APP_ORIGIN";
/// Required private binding: canonical base64url for 32 bytes; no default.
pub const RATE_LIMIT_KEY: &str = "RATE_LIMIT_KEY";
/// Optional private management secret; absence disables the developer interface.
pub const DEV_CLI_KEY: &str = "DEV_CLI_KEY";
/// Optional memory cost in KiB; default 19456, range 19456..=65536.
pub const ARGON2_M_COST: &str = "ARGON2_M_COST";
/// Optional iterations; default 2, range 2..=6.
pub const ARGON2_T_COST: &str = "ARGON2_T_COST";
/// Optional parallelism; default and only supported value 1.
pub const ARGON2_P_COST: &str = "ARGON2_P_COST";
pub const ARGON2_MIN_MEMORY_KIB: u32 = 19_456;
pub const ARGON2_MAX_MEMORY_KIB: u32 = 65_536;
pub const ARGON2_DEFAULT_MEMORY_KIB: u32 = ARGON2_MIN_MEMORY_KIB;
pub const ARGON2_MIN_ITERATIONS: u32 = 2;
pub const ARGON2_MAX_ITERATIONS: u32 = 6;
pub const ARGON2_DEFAULT_ITERATIONS: u32 = ARGON2_MIN_ITERATIONS;
pub const ARGON2_PARALLELISM: u32 = 1;
pub const RATE_LIMIT_KEY_BYTES: usize = 32;
pub const RATE_LIMIT_KEY_ENCODED_LEN: usize = (RATE_LIMIT_KEY_BYTES * 8).div_ceil(6);
pub const APP_ORIGIN_MAX_BYTES: usize = 2048;

pub const BACKEND_KEYS: [&str; 6] = [
    APP_ORIGIN,
    RATE_LIMIT_KEY,
    DEV_CLI_KEY,
    ARGON2_M_COST,
    ARGON2_T_COST,
    ARGON2_P_COST,
];

static CONFIG: OnceLock<Result<BackendConfig, ConfigError>> = OnceLock::new();

/// The first call acquires input; later calls borrow the config or cached error.
pub fn get_backend_config(source: &impl EnvSource) -> Result<&'static BackendConfig, ConfigError> {
    CONFIG
        .get_or_init(|| BackendConfig::new(source))
        .as_ref()
        .map_err(|error| *error)
}

/// Secret-bearing settings cannot be printed or serialized.
/// ```compile_fail
/// use brews_config::env::backend::BackendConfig;
/// fn requires_debug<T: std::fmt::Debug>() {}
/// requires_debug::<BackendConfig>();
/// ```
/// ```compile_fail
/// use brews_config::env::backend::BackendConfig;
/// fn requires_serialize<T: serde::Serialize>() {}
/// requires_serialize::<BackendConfig>();
/// ```
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    #[serde(rename = "app_origin")]
    pub application_origin: String,
    pub rate_limit_key: SecretKey,
    pub dev_cli_key: Option<SecretKey>,
    #[serde(default = "default_m_cost")]
    pub argon2_m_cost: u32,
    #[serde(default = "default_t_cost")]
    pub argon2_t_cost: u32,
    #[serde(default = "default_p_cost")]
    pub argon2_p_cost: u32,
}

impl BackendConfig {
    pub fn new(source: &impl EnvSource) -> Result<Self, ConfigError> {
        let mut pairs = Vec::with_capacity(BACKEND_KEYS.len());
        for key in BACKEND_KEYS {
            if let Some(value) = source.value(key)? {
                pairs.push((key.to_owned(), Zeroizing::new(value)));
            }
        }
        let mut config: Self = envy::from_iter(
            pairs
                .into_iter()
                .map(|(key, mut value)| (key, std::mem::take(&mut *value))),
        )
        .map_err(|error| super::deserialize_error(error, &BACKEND_KEYS))?;
        config.application_origin = super::utility::validate_origin(
            &config.application_origin,
            APP_ORIGIN,
            APP_ORIGIN_MAX_BYTES,
        )?;
        for (key, valid) in [
            (
                ARGON2_M_COST,
                (ARGON2_MIN_MEMORY_KIB..=ARGON2_MAX_MEMORY_KIB).contains(&config.argon2_m_cost),
            ),
            (
                ARGON2_T_COST,
                (ARGON2_MIN_ITERATIONS..=ARGON2_MAX_ITERATIONS).contains(&config.argon2_t_cost),
            ),
            (ARGON2_P_COST, config.argon2_p_cost == ARGON2_PARALLELISM),
        ] {
            if !valid {
                return Err(ConfigError::InvalidValue {
                    key,
                    kind: InvalidValueKind::OutOfRange,
                });
            }
        }
        if config
            .dev_cli_key
            .as_ref()
            .is_some_and(|key| key.expose_secret() == config.rate_limit_key.expose_secret())
        {
            return Err(ConfigError::InvalidValue {
                key: DEV_CLI_KEY,
                kind: InvalidValueKind::SecretKey,
            });
        }
        Ok(config)
    }
}

fn default_m_cost() -> u32 {
    ARGON2_DEFAULT_MEMORY_KIB
}

fn default_t_cost() -> u32 {
    ARGON2_DEFAULT_ITERATIONS
}

fn default_p_cost() -> u32 {
    ARGON2_PARALLELISM
}
