//! Fresh clock and fail-closed platform CSPRNG; no ambient OS environment.
use crate::auth::{AuthError, Runtime};
use crate::limits::JS_SAFE_INTEGER_MAX;

const WEB_CRYPTO_RANDOM_MAX_BYTES: usize = 65_536;
use js_sys::{Function, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};

pub(super) struct WorkerRuntime;
impl Runtime for WorkerRuntime {
    fn now_ms(&self) -> i64 {
        let now = js_sys::Date::now();
        if now.is_finite() && (0.0..=JS_SAFE_INTEGER_MAX as f64).contains(&now) {
            now as i64
        } else {
            -1
        }
    }
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), AuthError> {
        if bytes.is_empty() || bytes.len() > WEB_CRYPTO_RANDOM_MAX_BYTES {
            return Err(AuthError::Crypto);
        }
        let global = js_sys::global();
        let crypto =
            Reflect::get(&global, &JsValue::from_str("crypto")).map_err(|_| AuthError::Crypto)?;
        let random = Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
            .map_err(|_| AuthError::Crypto)?
            .dyn_into::<Function>()
            .map_err(|_| AuthError::Crypto)?;
        let array = Uint8Array::new_with_length(bytes.len() as u32);
        random
            .call1(&crypto, array.as_ref())
            .map_err(|_| AuthError::Crypto)?;
        array.copy_to(bytes);
        array.fill(0, 0, bytes.len() as u32);
        Ok(())
    }
}
