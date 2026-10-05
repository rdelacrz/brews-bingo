#![cfg(feature = "backend")]

use brews_config::env::{ConfigError, EnvSource, backend};

mod support;

#[test]
fn failed_get_caches_a_safe_error_without_retrying() -> Result<(), ConfigError> {
    let source = support::NativeSource::new().with(backend::RATE_LIMIT_KEY, "PRIVATE_VALUE_MARKER");
    let expected = ConfigError::Deserialization;
    assert!(source.requested_keys()?.is_empty());
    assert_eq!(backend::get_backend_config(&source).err(), Some(expected));
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    assert_eq!(backend::get_backend_config(&source).err(), Some(expected));
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    let repaired = support::NativeSource::new();
    assert_eq!(backend::get_backend_config(&repaired).err(), Some(expected));
    assert!(repaired.requested_keys()?.is_empty());
    assert_eq!(
        backend::get_backend_config(&NeverRead).err(),
        Some(expected)
    );
    assert!(!format!("{expected:?} {expected}").contains("PRIVATE_VALUE_MARKER"));
    assert!(std::error::Error::source(&expected).is_none());
    Ok(())
}

struct NeverRead;

impl EnvSource for NeverRead {
    fn value(&self, _: &'static str) -> Result<Option<String>, ConfigError> {
        panic!("cached configuration failure must not reacquire input")
    }
}
