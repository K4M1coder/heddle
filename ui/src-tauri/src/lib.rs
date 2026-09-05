//! The Tauri shell's library half: everything worth testing, and nothing that
//! needs a window.
//!
//! `main.rs` is the Tauri binary and this is what it wires up. The split exists
//! so `tests/chat_session.rs` can drive [`session::SessionHandle`] against the
//! real `heddle` binary with no `AppHandle` and no webview — the same reason
//! every product crate keeps its protocol adapter out of `heddle-core`.
//!
//! Nothing in `crates/` depends on this crate, and nothing may: the UI is the
//! outermost layer (Constitution I and IV).

pub mod code;
pub mod config;
pub mod session;
pub mod settings;
