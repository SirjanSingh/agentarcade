//! Pure logic: no Tauri, no filesystem, no network. Everything here is
//! unit-testable with `cargo test` and no window on screen.

pub mod hooks;
pub mod picker;
pub mod tiers;
pub mod tracker;
