//! Browser entrypoint; shared UI owns application logic.
#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(brews_ui::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Build the browser app with: python3 platform/web/tools/build.py");
}
