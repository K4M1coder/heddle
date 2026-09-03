//! Skein's embedded connectors (design §4.3, Constitution IV): the only crate
//! in the product that names the MCP protocol as a **server**, the way
//! `skein-mcp` is the only one naming it as a **client**.
//!
//! `skein-core` reaches a connector through the `ToolTransport` port it defines
//! and never depends on this crate, exactly as it never depends on `skein-mcp`.

mod connector;
mod fs;
mod git;
mod server;

pub use connector::{local_connector, LocalConnector};
pub use fs::FsRoot;
pub use git::is_git_repository;
pub use server::{
    EmbeddedServer, ListParams, LogParams, ReadParams, WriteParams, LOG_COUNT_CAP, READ_BYTE_CAP,
    STATUS_ENTRY_CAP,
};
