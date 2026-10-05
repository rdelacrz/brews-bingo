#![cfg(feature = "backend")]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use brews_config::env::{
    ConfigError, InvalidValueKind,
    backend::{self, BackendConfig},
};

mod support;
use support::NativeSource;

#[test]
fn missing_required_keys_are_identified_without_fallbacks() {
    for key in [backend::APP_ORIGIN, backend::RATE_LIMIT_KEY] {
        assert_eq!(
            BackendConfig::new(&NativeSource::new().without(key)).err(),
            Some(ConfigError::MissingKey { key })
        );
    }
    let source = NativeSource::new()
        .without(backend::APP_ORIGIN)
        .without(backend::RATE_LIMIT_KEY);
    assert_eq!(
        BackendConfig::new(&source).err(),
        Some(ConfigError::MissingKey {
            key: backend::APP_ORIGIN
        })
    );
}

#[test]
fn explicit_argon2_costs_are_deserialized_as_numbers() -> Result<(), ConfigError> {
    let source = NativeSource::new()
        .with(backend::ARGON2_M_COST, "65536")
        .with(backend::ARGON2_T_COST, "6")
        .with(backend::ARGON2_P_COST, "1");
    let config = BackendConfig::new(&source)?;
    assert_eq!(
        (
            config.argon2_m_cost,
            config.argon2_t_cost,
            config.argon2_p_cost
        ),
        (65_536, 6, 1)
    );
    Ok(())
}

#[test]
fn kdf_costs_cannot_weaken_or_exceed_the_approved_bounds() {
    for (key, values) in [
        (
            backend::ARGON2_M_COST,
            ["0", "19455", "65537", "4294967295"],
        ),
        (backend::ARGON2_T_COST, ["0", "1", "7", "4294967295"]),
        (backend::ARGON2_P_COST, ["0", "2", "3", "4294967295"]),
    ] {
        for value in values {
            assert_eq!(
                BackendConfig::new(&NativeSource::new().with(key, value)).err(),
                Some(ConfigError::InvalidValue {
                    key,
                    kind: InvalidValueKind::OutOfRange
                })
            );
        }
    }
}

#[test]
fn unusable_supplied_numeric_values_never_become_defaults_or_leak() {
    for key in [
        backend::ARGON2_M_COST,
        backend::ARGON2_T_COST,
        backend::ARGON2_P_COST,
    ] {
        for value in [
            "",
            " ",
            "-1",
            "4294967296",
            "2.0",
            "1e5",
            "PRIVATE_VALUE_MARKER",
        ] {
            let error = BackendConfig::new(&NativeSource::new().with(key, value)).err();
            assert_eq!(error, Some(ConfigError::Deserialization));
            if let Some(error) = error {
                assert_redacted(error);
            }
        }
    }
}

#[test]
fn unsafe_or_noncanonical_origins_fail_closed() {
    for value in [
        "",
        "http://localhost:8787",
        "//example.invalid",
        "https://example.invalid/path",
        "https://example.invalid/path/..",
        "https://example.invalid//",
        "https://example.invalid?",
        "https://example.invalid?token=PRIVATE_VALUE_MARKER",
        "https://example.invalid#",
        "https://user:PRIVATE_VALUE_MARKER@example.invalid",
        "https://@example.invalid",
        "https://EXAMPLE.invalid",
        "HTTPS://example.invalid",
        "https://example.invalid:443",
        " https://example.invalid",
        "https://example.invalid\n",
        "https://example.invalid/./",
        "https://example.invalid/%2e/",
        "https:\\example.invalid",
        "data:PRIVATE_VALUE_MARKER",
    ] {
        let error = BackendConfig::new(&NativeSource::new().with(backend::APP_ORIGIN, value)).err();
        assert_eq!(
            error,
            Some(ConfigError::InvalidValue {
                key: backend::APP_ORIGIN,
                kind: InvalidValueKind::Origin
            })
        );
        if let Some(error) = error {
            assert_redacted(error);
        }
    }
}

#[test]
fn valid_origins_are_stored_without_an_optional_root_slash() -> Result<(), ConfigError> {
    for value in [
        "https://example.invalid",
        "https://example.invalid/",
        "https://localhost:8787",
        "https://localhost:8787/",
        "https://[::1]:8787/",
    ] {
        let config = BackendConfig::new(&NativeSource::new().with(backend::APP_ORIGIN, value))?;
        assert_eq!(config.application_origin, value.trim_end_matches('/'));
    }
    Ok(())
}

#[test]
fn origin_input_size_is_bounded_before_url_parsing() {
    let source = NativeSource::new().with(
        backend::APP_ORIGIN,
        format!("https://{}.invalid", "x".repeat(2048)),
    );
    assert_eq!(
        BackendConfig::new(&source).err(),
        Some(ConfigError::InvalidValue {
            key: backend::APP_ORIGIN,
            kind: InvalidValueKind::TooLong
        })
    );
}

#[test]
fn keys_require_canonical_unpadded_base64url_for_exactly_32_bytes() {
    let canonical = URL_SAFE_NO_PAD.encode([0u8; 32]);
    let mut trailing_bits = canonical.clone();
    trailing_bits.pop();
    trailing_bits.push('B');
    for value in [
        String::new(),
        format!("{canonical}="),
        format!(" {canonical}"),
        format!("{canonical}\n"),
        trailing_bits,
        URL_SAFE_NO_PAD.encode([0u8; 31]),
        URL_SAFE_NO_PAD.encode([0u8; 33]),
        base64::engine::general_purpose::STANDARD_NO_PAD.encode([0xffu8; 32]),
        "PRIVATE_VALUE_MARKER".to_owned(),
    ] {
        let error =
            BackendConfig::new(&NativeSource::new().with(backend::RATE_LIMIT_KEY, value)).err();
        assert_eq!(error, Some(ConfigError::Deserialization));
        if let Some(error) = error {
            assert_redacted(error);
        }
    }
}

#[test]
fn acquisition_can_only_request_the_declared_inventory() -> Result<(), ConfigError> {
    let source = NativeSource::new()
        .with("UNDECLARED_VALUE_MARKER", "PRIVATE_VALUE_MARKER")
        .with("PUBLIC_APP_NAME", "PRIVATE_VALUE_MARKER")
        .with("app_origin", "PRIVATE_VALUE_MARKER");
    BackendConfig::new(&source)?;
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    let unique: std::collections::BTreeSet<_> = backend::BACKEND_KEYS.into_iter().collect();
    assert_eq!(unique.len(), backend::BACKEND_KEYS.len());
    Ok(())
}

#[test]
fn validation_failure_still_acquires_each_declared_key_once() -> Result<(), ConfigError> {
    let source = NativeSource::new().with(backend::APP_ORIGIN, "PRIVATE_VALUE_MARKER");
    assert!(BackendConfig::new(&source).is_err());
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    Ok(())
}

fn assert_redacted(error: ConfigError) {
    assert!(!format!("{error:?} {error}").contains("PRIVATE_VALUE_MARKER"));
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn an_unusable_optional_binding_is_an_error_not_a_missing_default() {
    use brews_config::env::EnvSource;
    struct UnusableOptional(NativeSource);
    impl EnvSource for UnusableOptional {
        fn value(&self, key: &'static str) -> Result<Option<String>, ConfigError> {
            if key == backend::ARGON2_M_COST {
                return Err(ConfigError::Deserialization);
            }
            self.0.value(key)
        }
    }
    assert_eq!(
        BackendConfig::new(&UnusableOptional(NativeSource::new())).err(),
        Some(ConfigError::Deserialization)
    );
}
