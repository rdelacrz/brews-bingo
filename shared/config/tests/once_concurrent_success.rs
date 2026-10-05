#![cfg(any(feature = "backend", feature = "frontend"))]

use brews_config::env::ConfigError;
use std::sync::Barrier;

#[cfg(feature = "backend")]
mod support;

#[cfg(feature = "backend")]
#[test]
fn racing_backend_get_calls_share_one_acquisition() -> Result<(), ConfigError> {
    use brews_config::env::backend;
    let source = support::NativeSource::new();
    let start = Barrier::new(8);
    std::thread::scope(|scope| -> Result<(), ConfigError> {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    backend::get_backend_config(&source)
                })
            })
            .collect();
        for thread in threads {
            let config = match thread.join() {
                Ok(result) => result?,
                Err(payload) => std::panic::resume_unwind(payload),
            };
            assert!(std::ptr::eq(backend::get_backend_config(&source)?, config));
        }
        Ok(())
    })?;
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    assert!(std::ptr::eq(
        backend::get_backend_config(&source)?,
        backend::get_backend_config(&source)?
    ));
    assert_eq!(source.requested_keys()?, backend::BACKEND_KEYS);
    Ok(())
}

#[cfg(feature = "frontend")]
#[test]
fn racing_frontend_get_calls_share_one_instance() -> Result<(), ConfigError> {
    use brews_config::env::frontend;
    let start = Barrier::new(8);
    std::thread::scope(|scope| -> Result<(), ConfigError> {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    frontend::get_frontend_config()
                })
            })
            .collect();
        for thread in threads {
            let config = match thread.join() {
                Ok(result) => result?,
                Err(payload) => std::panic::resume_unwind(payload),
            };
            assert!(std::ptr::eq(frontend::get_frontend_config()?, config));
        }
        Ok(())
    })
}
