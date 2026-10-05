#![cfg(feature = "backend")]

use brews_config::env::{ConfigError, EnvSource, backend};

mod support;

#[test]
fn get_initializes_lazily_and_never_acquires_again() -> Result<(), ConfigError> {
    let source = support::NativeSource::new();
    assert!(source.requested_keys()?.is_empty());
    let first: &'static backend::BackendConfig = backend::get_backend_config(&source)?;
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    assert_eq!(first.application_origin, "https://example.invalid");
    assert!(std::ptr::eq(first, backend::get_backend_config(&source)?));
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    assert!(std::ptr::eq(
        first,
        backend::get_backend_config(&NeverRead)?
    ));
    let replacement =
        support::NativeSource::new().with(backend::APP_ORIGIN, "https://replacement.invalid");
    assert!(std::ptr::eq(
        first,
        backend::get_backend_config(&replacement)?
    ));
    assert!(replacement.requested_keys()?.is_empty());
    Ok(())
}

struct NeverRead;

impl EnvSource for NeverRead {
    fn value(&self, _: &'static str) -> Result<Option<String>, ConfigError> {
        panic!("cached configuration must not reacquire input")
    }
}
