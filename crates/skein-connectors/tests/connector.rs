//! The in-process connector (spec 016, SC-004): a real rmcp client and a real
//! `FsServer` over a duplex, behind `skein-core`'s synchronous `ToolTransport`.
//!
//! Plain `#[test]`, never `#[tokio::test]`, for the reason
//! `crates/skein-mcp/tests/rmcp_gateway.rs` records: the connector owns a
//! runtime and blocks on it, and `Runtime::block_on` inside a runtime panics.

use skein_connectors::{fs_connector, FsRoot, LocalConnector};
use skein_core::{ToolCall, ToolTransport};
use tempfile::TempDir;

fn connector() -> (TempDir, LocalConnector) {
    let dir = TempDir::new().expect("a temp dir");
    std::fs::write(dir.path().join("notes.txt"), "in the root").expect("a file in the root");
    let root = FsRoot::new(dir.path()).expect("a canonicalizable root");
    let connector = fs_connector(root).expect("the embedded server starts and the client connects");
    (dir, connector)
}

#[test]
fn the_connector_lists_the_three_tools_with_their_derived_schemas() {
    let (_dir, mut connector) = connector();

    let mut catalogue = connector
        .list()
        .expect("tools/list reaches the embedded server");
    catalogue.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(
        catalogue
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["fs_list", "fs_read", "fs_write"]
    );
    for spec in &catalogue {
        assert!(
            !spec.description.is_empty(),
            "{} must describe itself to the model",
            spec.name
        );
        // Derived by `schemars` from the real parameter struct, not written
        // here: every tool takes `path`, and only `fs_write` takes `content`.
        let properties = spec.parameters["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{}: {}", spec.name, spec.parameters));
        assert!(properties.contains_key("path"), "{}", spec.parameters);
        assert_eq!(
            properties.contains_key("content"),
            spec.name == "fs_write",
            "{}: {}",
            spec.name,
            spec.parameters
        );
    }
}

#[test]
fn the_connector_calls_a_tool_in_the_same_process() {
    let (_dir, mut connector) = connector();

    let outcome = connector
        .call(&ToolCall::new(
            "fs_read",
            serde_json::json!({"path": "notes.txt"}),
        ))
        .expect("the call reaches the embedded server");

    assert!(
        outcome.content.contains("in the root"),
        "the real file's contents must come back: {}",
        outcome.content
    );
}

#[test]
fn a_containment_refusal_arrives_as_a_tool_error_not_a_transport_failure() {
    let (_dir, mut connector) = connector();

    // `Ok`, deliberately: the transport succeeded. What failed is the tool, and
    // the model is told so — the whole reason the tools return
    // `Result<String, String>` instead of failing the call.
    let outcome = connector
        .call(&ToolCall::new(
            "fs_read",
            serde_json::json!({"path": "../escape.txt"}),
        ))
        .expect("a refused path must not fail the transport");

    assert!(
        outcome.content.contains("\"isError\":true"),
        "the refusal must arrive flagged as a tool error: {}",
        outcome.content
    );
}

/// `SkeinAgent` requires `T: ToolTransport + Send + 'static`. Without this the
/// first thing to notice would be `skein acp-agent` failing to compile, with
/// the error pointing at the wiring rather than at the connector.
#[test]
fn the_connector_is_send_so_an_acp_session_can_own_one() {
    fn requires_send<T: Send + 'static>() {}
    requires_send::<LocalConnector>();
}
