//! Skein's embedded connectors (design §4.3, Constitution IV): the only crate
//! in the product that names the MCP protocol as a **server**, the way
//! `skein-mcp` is the only one naming it as a **client**.
//!
//! `skein-core` reaches a connector through the `ToolTransport` port it defines
//! and never depends on this crate, exactly as it never depends on `skein-mcp`.

mod fs;

pub use fs::FsRoot;
