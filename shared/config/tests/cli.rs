#![cfg(all(feature = "cli", not(target_arch = "wasm32")))]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::process::Command;

#[test]
fn constructor_requires_private_cli_inputs() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "cli_config_probe", "--nocapture"])
        .env_clear()
        .env("CONFIG_PROBE", "missing")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn cli_rejects_noncanonical_or_insecure_origins() {
    for origin in [
        "http://localhost:8787",
        "https://example.invalid/path",
        "https://example.invalid?key=PRIVATE_VALUE_MARKER",
        "https://user:PRIVATE_VALUE_MARKER@example.invalid",
        "https://EXAMPLE.invalid",
        "https://example.invalid:443",
        "https://example.invalid/./",
        " https://example.invalid",
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "cli_config_probe", "--nocapture"])
            .env_clear()
            .env("CONFIG_PROBE", "invalid_origin")
            .env("BREWS_API_ORIGIN", origin)
            .env("BREWS_DEV_CLI_KEY", URL_SAFE_NO_PAD.encode([3u8; 32]))
            .output()
            .unwrap();
        assert!(output.status.success());
    }
}

#[test]
fn cli_config_probe() {
    use brews_config::env::{
        ConfigError,
        cli::{BREWS_API_ORIGIN, CliConfig},
    };
    match std::env::var("CONFIG_PROBE").as_deref() {
        Ok("missing") => {
            assert_eq!(
                CliConfig::new().err(),
                Some(ConfigError::MissingKey {
                    key: BREWS_API_ORIGIN
                })
            );
        }
        Ok("invalid_origin") => {
            let error = CliConfig::new().err().expect("unsafe origin accepted");
            assert!(!format!("{error:?} {error}").contains("PRIVATE_VALUE_MARKER"));
            assert!(std::error::Error::source(&error).is_none());
        }
        _ => {}
    }
}
