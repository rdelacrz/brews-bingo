//! Canonical private 32-byte configuration keys.
use super::super::InvalidValueKind;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use zeroize::Zeroizing;

pub(super) const SECRET_KEY_BYTES: usize = 32;
pub(super) const SECRET_KEY_ENCODED_LEN: usize = (SECRET_KEY_BYTES * 8).div_ceil(6);

/// Zeroizing key storage, deliberately without `Debug` or `Serialize`.
/// ```compile_fail
/// use brews_config::env::backend::SecretKey;
/// fn requires_debug<T: std::fmt::Debug>() {}
/// requires_debug::<SecretKey>();
/// ```
/// ```compile_fail
/// use brews_config::env::backend::SecretKey;
/// fn requires_serialize<T: serde::Serialize>() {}
/// requires_serialize::<SecretKey>();
/// ```
pub struct SecretKey(Zeroizing<[u8; SECRET_KEY_BYTES]>);

impl SecretKey {
    pub fn expose_secret(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<'de> Deserialize<'de> for SecretKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let encoded = Zeroizing::new(String::deserialize(deserializer)?);
        let invalid = || serde::de::Error::custom(InvalidValueKind::SecretKey);
        if encoded.len() != SECRET_KEY_ENCODED_LEN {
            return Err(invalid());
        }
        let mut decoded = Zeroizing::new([0u8; SECRET_KEY_BYTES]);
        let written = URL_SAFE_NO_PAD
            .decode_slice(encoded.as_bytes(), &mut *decoded)
            .map_err(|_| invalid())?;
        if written != SECRET_KEY_BYTES {
            return Err(invalid());
        }
        Ok(Self(decoded))
    }
}
