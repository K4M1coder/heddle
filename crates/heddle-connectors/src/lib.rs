//! Heddle's embedded connectors (design §4.3, Constitution IV): the only crate
//! in the product that names the MCP protocol as a **server**, the way
//! `heddle-mcp` is the only one naming it as a **client**.
//!
//! `heddle-core` reaches a connector through the `ToolTransport` port it defines
//! and never depends on this crate, exactly as it never depends on `heddle-mcp`.

mod atlassian;
mod connector;
mod fs;
mod git;
#[cfg(windows)]
mod run;
mod server;

pub use atlassian::{AtlassianConfig, AtlassianServer};
pub use connector::{
    atlassian_connector, local_connector, local_connector_with_run, LocalConnector,
};
pub use fs::{FsRoot, RunDirs};
pub use git::is_git_repository;
pub use server::{
    EmbeddedServer, ListParams, LogParams, ReadParams, RunAccess, RunParams, WriteParams,
    LOG_COUNT_CAP, READ_BYTE_CAP, RUN_OUTPUT_BYTE_CAP, RUN_TIMEOUT, STATUS_ENTRY_CAP,
};
