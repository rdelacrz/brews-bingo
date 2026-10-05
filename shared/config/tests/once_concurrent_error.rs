#![cfg(feature = "backend")]

use brews_config::env::{ConfigError, backend};
use std::sync::Barrier;
mod support;

#[test]
fn racing_backend_get_calls_share_one_cached_failure() -> Result<(), ConfigError> {
    let source = support::NativeSource::new().without(backend::APP_ORIGIN);
    let expected = ConfigError::MissingKey {
        key: backend::APP_ORIGIN,
    };
    let start = Barrier::new(8);
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    backend::get_backend_config(&source)
                })
            })
            .collect();
        for thread in threads {
            let result = match thread.join() {
                Ok(result) => result,
                Err(payload) => std::panic::resume_unwind(payload),
            };
            assert_eq!(result.err(), Some(expected));
        }
    });
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    assert_eq!(backend::get_backend_config(&source).err(), Some(expected));
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    Ok(())
}
