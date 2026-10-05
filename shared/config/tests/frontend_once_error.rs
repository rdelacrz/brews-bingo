#![cfg(feature = "frontend")]

use brews_config::env::{ConfigError, frontend};

#[test]
fn public_singleton_caches_the_constructor_outcome() -> Result<(), ConfigError> {
    match frontend::PublicAppConfig::new() {
        Ok(_) => assert!(std::ptr::eq(
            frontend::get_frontend_config()?,
            frontend::get_frontend_config()?
        )),
        Err(expected) => {
            for _ in 0..3 {
                assert_eq!(frontend::get_frontend_config().err(), Some(expected));
            }
            assert!(std::error::Error::source(&expected).is_none());
            assert!(!format!("{expected:?} {expected}").contains("PRIVATE_VALUE_MARKER"));
        }
    }
    Ok(())
}
