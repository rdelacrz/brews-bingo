//! Browser UI; authoritative state remains on the server.
#![forbid(unsafe_code)]

mod app;
mod client;
mod components;
mod features;
mod layouts;
mod routes;
mod session;

pub use app::App;
