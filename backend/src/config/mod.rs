//! Cloudflare binding adapter; schema and one-time initialization are shared.

use brews_config::env::{
    ConfigError, EnvSource,
    backend::{self, BackendConfig, RATE_LIMIT_KEY},
};
use js_sys::Reflect;
use wasm_bindgen::JsValue;
use worker::Env;

pub(super) fn get_backend_config(env: &Env) -> Result<&'static BackendConfig, ConfigError> {
    backend::get_backend_config(&Bindings(env))
}

struct Bindings<'a>(&'a Env);

impl EnvSource for Bindings<'_> {
    fn value(&self, key: &'static str) -> Result<Option<String>, ConfigError> {
        let present = Reflect::has(self.0.as_ref(), &JsValue::from_str(key))
            .map_err(|_| ConfigError::Deserialization)?;
        if !present {
            return Ok(None);
        }
        let value = if key == RATE_LIMIT_KEY || key == backend::DEV_CLI_KEY {
            self.0.secret(key).map(|secret| secret.to_string())
        } else {
            self.0.var(key).map(|value| value.to_string())
        }
        .map_err(|_| ConfigError::Deserialization)?;
        Ok(Some(value))
    }
}
