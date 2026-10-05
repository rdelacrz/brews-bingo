#![cfg(all(feature = "backend", feature = "frontend"))]

use brews_config::env::{ConfigError, backend, frontend};
mod support;

#[test]
fn backend_failure_does_not_poison_the_public_singleton() -> Result<(), ConfigError> {
    let source = support::NativeSource::new().without(backend::APP_ORIGIN);
    let expected = ConfigError::MissingKey {
        key: backend::APP_ORIGIN,
    };
    assert_eq!(backend::get_backend_config(&source).err(), Some(expected));
    let public = frontend::get_frontend_config()?;
    assert_eq!(public.api_path, frontend::API_PATH);
    assert!(std::ptr::eq(public, frontend::get_frontend_config()?));
    assert_eq!(backend::get_backend_config(&source).err(), Some(expected));
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    Ok(())
}
