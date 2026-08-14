#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ui::AppResult {
    ui::run_native()
}

#[cfg(target_arch = "wasm32")]
fn main() {
    ui::run_web();
}
