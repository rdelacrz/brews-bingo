//! Cloudflare account authentication.
#![forbid(unsafe_code)]

pub mod api;
pub mod auth;
pub mod db;
pub mod directory;
pub mod game;
mod limits;
#[cfg(any(target_arch = "wasm32", test))]
mod observability;
pub mod security;

#[cfg(target_arch = "wasm32")]
mod config;
#[cfg(target_arch = "wasm32")]
mod worker_runtime;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(
    dead_code,
    reason = "Native tests exercise exact private-peer sources; SDK adapters are Wasm-only."
)]
mod worker_runtime {
    mod game_accounts;
    mod game_directory;
    mod game_peers;
    mod game_recovery;
    mod game_recovery_wire;
    mod game_wire;
}
