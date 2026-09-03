//! The in-process connector (spec 016 SC-004, spec 017 SC-007): a real rmcp
//! client and a real `EmbeddedServer` over a duplex, behind `skein-core`'s
//! synchronous `ToolTransport`.
//!
//! Plain `#[test]`, never `#[tokio::test]`, for the reason
//! `crates/skein-mcp/tests/rmcp_gateway.rs` records: the connector owns a
//! runtime and blocks on it, and `Runtime::block_on` inside a runtime panics.

use git2::{Repository, Signature};
use skein_connectors::{local_connector, FsRoot, LocalConnector};
use skein_core::{ToolCall, ToolTransport};
use std::path::Path;
use tempfile::TempDir;

fn connector() -> (TempDir, LocalConnector) {
    let dir = TempDir::new().expect("a temp dir");
    std::fs::write(dir.path().join("notes.txt"), "in the root").expect("a file in the root");
    let root = FsRoot::new(dir.path()).expect("a canonicalizable root");
    let connector =
        local_connector(root).expect("the embedded server starts and the client connects");
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

/// The same connector over a **real repository with a real commit**, which is
/// the only thing that turns the git routes on.
fn connector_over_a_repository() -> (TempDir, LocalConnector) {
    let dir = TempDir::new().expect("a temp dir");
    let repo = Repository::init(dir.path()).expect("a repository is initialised");
    std::fs::write(dir.path().join("notes.txt"), "in the root").expect("a file to commit");
    let mut index = repo.index().expect("the index opens");
    index
        .add_path(Path::new("notes.txt"))
        .expect("the path is staged");
    index.write().expect("the index is written");
    let tree = repo
        .find_tree(index.write_tree().expect("the index writes a tree"))
        .expect("the tree is found");
    let who = Signature::now("Fixture Author", "fixture@example.invalid").expect("a signature");
    repo.commit(Some("HEAD"), &who, &who, "the only commit", &tree, &[])
        .expect("the commit is written");

    let root = FsRoot::new(dir.path()).expect("a canonicalizable root");
    let connector =
        local_connector(root).expect("the embedded server starts and the client connects");
    (dir, connector)
}

fn names(connector: &mut LocalConnector) -> Vec<String> {
    let mut catalogue = connector
        .list()
        .expect("tools/list reaches the embedded server");
    catalogue.sort_by(|a, b| a.name.cmp(&b.name));
    catalogue.into_iter().map(|s| s.name).collect()
}

#[test]
fn the_connector_lists_the_git_tools_only_when_the_root_is_a_repository() {
    let (_plain_dir, mut over_a_plain_directory) = connector();
    let (_repo_dir, mut over_a_repository) = connector_over_a_repository();

    // The server advertising what it can actually do, which is all `tools/list`
    // has ever been. This is also what keeps every pre-existing advertisement
    // assertion in the workspace green untouched: each one's fixture root is a
    // plain `TempDir` (SC-012).
    assert_eq!(
        names(&mut over_a_plain_directory),
        vec!["fs_list", "fs_read", "fs_write"]
    );
    assert_eq!(
        names(&mut over_a_repository),
        vec!["fs_list", "fs_read", "fs_write", "git_log", "git_status"]
    );

    let catalogue = over_a_repository
        .list()
        .expect("tools/list reaches the embedded server");
    let schema_of = |name: &str| {
        catalogue
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name} must be advertised"))
            .parameters
            .clone()
    };
    // Empty `properties` and not a missing key: rmcp derives
    // `{"type":"object","properties":{}}` for a `#[tool]` method with no
    // `Parameters<T>` argument, and that emptiness *is* the injection argument
    // — there is nothing for a model to put anything in.
    assert_eq!(
        schema_of("git_status")["properties"]
            .as_object()
            .expect("an object")
            .len(),
        0,
        "{}",
        schema_of("git_status")
    );
    assert!(
        schema_of("git_log")["properties"]
            .as_object()
            .expect("an object")
            .contains_key("count"),
        "{}",
        schema_of("git_log")
    );
}

#[test]
fn a_git_tool_whose_route_is_disabled_is_not_callable_by_name() {
    let (_dir, mut connector) = connector();

    // `Err`, and **not** the `Ok`-carrying-`isError` shape the containment
    // refusal above arrives in. A disabled route is not found, which rmcp
    // reports as a protocol-level `invalid_params` and `RmcpToolTransport`
    // maps to `SkeinError::Tool` — and `NativeLoop::mediate` survives only
    // `SkeinError::ToolDenied`, so this would **end the run**.
    //
    // That is the entire reason `skein-cli`'s allowlist has to omit these two
    // names in the same case rather than leaning on this gate: an allowlisted
    // name whose route is disabled is fatal, where a name absent from the
    // allowlist is a survivable `denied` with a reason.
    let error = connector
        .call(&ToolCall::new("git_status", serde_json::json!({})))
        .expect_err("a disabled route must not be callable by name");

    assert!(
        error.to_string().contains("git_status") || error.to_string().contains("not found"),
        "the error must say what was not found: {error}"
    );
}
