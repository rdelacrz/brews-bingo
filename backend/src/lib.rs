//! Cloudflare account authentication.
#![forbid(unsafe_code)]

pub mod api;
pub mod auth;
mod limits;
pub mod security;
pub mod storage;

#[cfg(target_arch = "wasm32")]
mod config;
#[cfg(target_arch = "wasm32")]
mod worker_runtime;
