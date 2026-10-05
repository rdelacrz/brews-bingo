#![allow(dead_code)]

use std::{collections::BTreeMap, sync::Mutex};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use brews_config::env::{ConfigError, EnvSource, backend};
use zeroize::Zeroizing;

pub struct NativeSource {
    values: BTreeMap<&'static str, Zeroizing<String>>,
    reads: Mutex<Vec<&'static str>>,
}

impl NativeSource {
    pub fn new() -> Self {
        let bytes: [u8; 32] = std::array::from_fn(|index| index as u8);
        Self {
            values: BTreeMap::from([
                (
                    backend::APP_ORIGIN,
                    Zeroizing::new("https://example.invalid".to_owned()),
                ),
                (
                    backend::RATE_LIMIT_KEY,
                    Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes)),
                ),
            ]),
            reads: Mutex::new(Vec::new()),
        }
    }

    pub fn with(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.values.insert(key, Zeroizing::new(value.into()));
        self
    }

    pub fn without(mut self, key: &'static str) -> Self {
        self.values.remove(key);
        self
    }

    pub fn requested_keys(&self) -> Result<Vec<&'static str>, ConfigError> {
        self.reads
            .lock()
            .map(|reads| reads.clone())
            .map_err(|_| ConfigError::Deserialization)
    }
}

impl EnvSource for NativeSource {
    fn value(&self, key: &'static str) -> Result<Option<String>, ConfigError> {
        self.reads
            .lock()
            .map_err(|_| ConfigError::Deserialization)?
            .push(key);
        Ok(self.values.get(key).map(|value| value.to_string()))
    }
}
