//! Pure logic: no Tauri, no filesystem, no network. Everything here is
//! unit-testable with `cargo test` and no window on screen.

pub mod agents;
pub mod games;
pub mod hooks;
pub mod picker;
pub mod tracker;
