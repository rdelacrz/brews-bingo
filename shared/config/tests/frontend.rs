#![cfg(feature = "frontend")]

use brews_config::env::{
    ConfigError,
    frontend::{self, PublicAppConfig},
};

#[test]
fn serde_deserializes_the_public_struct_with_optional_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    let config: PublicAppConfig = envy::from_iter([])?;
    assert_eq!(config.application_name, None);
    assert_eq!(config.api_path, frontend::API_PATH);
    Ok(())
}

#[test]
fn constructor_is_zero_argument_and_uses_only_public_build_inputs() -> Result<(), ConfigError> {
    let config = PublicAppConfig::new()?;
    assert_eq!(config.api_path, frontend::API_PATH);
    let source = include_str!("../src/env/frontend.rs");
    let captures: Vec<_> = source
        .lines()
        .filter(|line| line.contains("option_env!"))
        .collect();
    assert_eq!(captures.len(), frontend::FRONTEND_KEYS.len());
    assert!(
        captures
            .iter()
            .any(|line| line.contains("option_env!(\"PUBLIC_APP_NAME\")"))
    );
    assert!(
        captures
            .iter()
            .any(|line| line.contains("option_env!(\"PUBLIC_API_PATH\")"))
    );
    for forbidden in ["std::env", "envy::from_env", "web_sys", "js_sys"] {
        assert!(!source.contains(forbidden));
    }
    Ok(())
}
