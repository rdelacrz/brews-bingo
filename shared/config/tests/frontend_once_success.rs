#![cfg(feature = "frontend")]

use brews_config::env::{ConfigError, frontend};

#[test]
fn get_initializes_the_public_config_and_returns_the_same_instance() -> Result<(), ConfigError> {
    let first: &'static frontend::PublicAppConfig = frontend::get_frontend_config()?;
    assert_eq!(first.api_path, frontend::API_PATH);
    assert!(std::ptr::eq(first, frontend::get_frontend_config()?));
    Ok(())
}
