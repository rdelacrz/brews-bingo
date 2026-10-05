#![cfg(feature = "backend")]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use brews_config::env::{
    ConfigError,
    backend::{BACKEND_KEYS, BackendConfig},
};

mod support;

#[test]
fn constructor_acquires_every_declared_key_once() -> Result<(), ConfigError> {
    let source = support::NativeSource::new();
    assert!(source.requested_keys()?.is_empty());
    let config = BackendConfig::new(&source)?;
    assert_eq!(source.requested_keys()?, BACKEND_KEYS);
    assert_eq!(config.application_origin, "https://example.invalid");
    assert_eq!(config.argon2_m_cost, 19_456);
    Ok(())
}

#[test]
fn serde_deserializes_the_backend_struct_with_optional_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes: [u8; 32] = std::array::from_fn(|index| index as u8);
    let config: BackendConfig = envy::from_iter([
        (
            "APP_ORIGIN".to_owned(),
            "https://example.invalid".to_owned(),
        ),
        ("RATE_LIMIT_KEY".to_owned(), URL_SAFE_NO_PAD.encode(bytes)),
    ])?;
    assert_eq!(config.application_origin, "https://example.invalid");
    assert!(config.rate_limit_key.expose_secret() == bytes);
    assert_eq!(config.argon2_m_cost, 19_456);
    assert_eq!(config.argon2_t_cost, 2);
    assert_eq!(config.argon2_p_cost, 1);
    Ok(())
}

#[test]
fn serde_itself_reports_missing_required_fields() {
    let bytes: [u8; 32] = std::array::from_fn(|index| index as u8);
    for (missing, field) in [
        ("APP_ORIGIN", "app_origin"),
        ("RATE_LIMIT_KEY", "rate_limit_key"),
    ] {
        let pairs = [
            (
                "APP_ORIGIN".to_owned(),
                "https://example.invalid".to_owned(),
            ),
            ("RATE_LIMIT_KEY".to_owned(), URL_SAFE_NO_PAD.encode(bytes)),
        ];
        let error = envy::from_iter::<_, BackendConfig>(
            pairs.into_iter().filter(|(key, _)| key != missing),
        )
        .err();
        assert!(matches!(error, Some(envy::Error::MissingValue(actual)) if actual == field));
    }
}
