//! The in-process connector (spec 016 SC-004, spec 017 SC-007): a real rmcp
//! client and a real `EmbeddedServer` over a duplex, behind `heddle-core`'s
//! synchronous `ToolTransport`.
//!
//! Plain `#[test]`, never `#[tokio::test]`, for the reason
//! `crates/heddle-mcp/tests/rmcp_gateway.rs` records: the connector owns a
//! runtime and blocks on it, and `Runtime::block_on` inside a runtime panics.

#[cfg(windows)]
mod guard;

use git2::{Repository, Signature};
use heddle_connectors::{local_connector, FsRoot, LocalConnector};
use heddle_core::{ToolCall, ToolTransport};
use std::path::Path;
use tempfile::TempDir;

/// Nothing in this file cancels anything. A flag nobody keeps a second
/// reference to is what "no cancel channel" looks like — the same thing
/// `heddle chat` passes.
fn uncancelled() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))
}

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

/// `HeddleAgent` requires `T: ToolTransport + Send + 'static`. Without this the
/// first thing to notice would be `heddle acp-agent` failing to compile, with
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

/// The cap a model plans around and the cap the handler enforces are one
/// number, and this is what says so.
///
/// `git_log`'s refusal message already interpolates [`LOG_COUNT_CAP`]
/// (`server.rs`), but its `#[tool(description = ...)]` states the same ceiling
/// as a hand-typed literal — an attribute takes a string literal, so there is
/// no way to interpolate it there. Nothing but this assertion stops the
/// constant from moving while the prose a model reads stays behind, which would
/// have a model either under-asking silently or being told a ceiling that then
/// refuses it.
#[test]
fn the_advertised_log_cap_is_the_constant_that_enforces_it() {
    use heddle_connectors::LOG_COUNT_CAP;

    let (_repo_dir, mut over_a_repository) = connector_over_a_repository();

    let catalogue = over_a_repository
        .list()
        .expect("tools/list reaches the embedded server");
    let description = &catalogue
        .iter()
        .find(|s| s.name == "git_log")
        .expect("git_log is advertised over a repository")
        .description;

    let stated = format!("between 1 and {LOG_COUNT_CAP}");
    assert!(
        description.contains(&stated),
        "the advertised ceiling must be `{stated}`: {description}"
    );
}

#[test]
fn a_git_tool_whose_route_is_disabled_is_not_callable_by_name() {
    let (_dir, mut connector) = connector();

    // `Err`, and **not** the `Ok`-carrying-`isError` shape the containment
    // refusal above arrives in. A disabled route is not found, which rmcp
    // reports as a protocol-level `invalid_params` and `RmcpToolTransport`
    // maps to `HeddleError::Tool` — and `NativeLoop::mediate` survives only
    // `HeddleError::ToolDenied`, so this would **end the run**.
    //
    // That is the entire reason `heddle-cli`'s allowlist has to omit these two
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

// ---- the shell connector (spec 019) ----

/// The Windows-side gate: run access is **off** unless it is asked for, so the
/// catalogue is byte-identical to the one `dev` advertises (SC-007).
///
/// This is what keeps every pre-existing advertisement assertion in the
/// workspace green on the Windows leg — none of their fixtures asks for run
/// access. If one of them ever needs an assertion changed, the gate is wrong.
#[cfg(windows)]
#[test]
fn the_connector_does_not_list_proc_run_unless_run_access_is_asked_for() {
    use heddle_connectors::{local_connector_with_run, RunAccess};

    let dir = TempDir::new().expect("a temp dir");
    let root = FsRoot::new(dir.path()).expect("a canonicalizable root");
    let mut denied = local_connector_with_run(root, RunAccess::Denied, uncancelled())
        .expect("a denied launcher still serves the other tools");

    assert_eq!(names(&mut denied), vec!["fs_list", "fs_read", "fs_write"]);
}

/// And with run access asked for, the tool is there, describing the two numbers
/// a model has to plan around.
#[cfg(windows)]
#[test]
fn the_connector_lists_proc_run_with_its_caps_stated_when_run_access_is_allowed() {
    use heddle_connectors::{
        local_connector_with_run, RunAccess, RunDirs, RUN_OUTPUT_BYTE_CAP, RUN_TIMEOUT,
    };

    let dir = TempDir::new().expect("a temp dir");
    let root = FsRoot::new(dir.path()).expect("a canonicalizable root");
    let _pruned = guard::PrunedOnDrop::of_root(dir.path());
    let mut allowed =
        local_connector_with_run(root, RunAccess::Allowed(RunDirs::none()), uncancelled())
            .expect("the sandbox builds and the server serves");

    assert_eq!(
        names(&mut allowed),
        vec!["fs_list", "fs_read", "fs_write", "proc_run"]
    );

    let catalogue = allowed.list().expect("tools/list reaches the server");
    let spec = catalogue
        .iter()
        .find(|s| s.name == "proc_run")
        .expect("proc_run is advertised");
    // The description **is** the contract the model reads, so the caps it has
    // to work within have to be in it rather than only in a Rust constant.
    // Matched as the whole phrase, not as a bare number: the description also
    // names `%SystemRoot%\System32`, so a bare `RUN_TIMEOUT.as_secs()` of 32
    // would pass on a coincidence while the sentence still said thirty.
    let killed_after = format!("killed after {} seconds", RUN_TIMEOUT.as_secs());
    let truncated_at = format!("truncated at {RUN_OUTPUT_BYTE_CAP} bytes");
    assert!(
        spec.description.contains(&killed_after) && spec.description.contains(&truncated_at),
        "the wall clock and the output cap must be stated as `{killed_after}` and \
         `{truncated_at}`: {}",
        spec.description
    );
    assert!(
        spec.description.contains("PATH is not searched"),
        "and so must the resolution rule, or a model will keep naming `cargo`: {}",
        spec.description
    );
    // Derived by `schemars` from `RunParams`, not written here.
    let properties = spec.parameters["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("{}", spec.parameters));
    assert!(properties.contains_key("command"), "{}", spec.parameters);
    assert!(properties.contains_key("args"), "{}", spec.parameters);
}

/// The platform gate, on the two legs that are not Windows (SC-007).
///
/// **Fails loudly, never silently degrades.** An operator who asks for a
/// launcher on Linux or macOS gets a message naming the platform, not a server
/// that quietly serves one tool fewer than they asked for. This test is the
/// only one in the slice that runs on two of the three CI legs.
#[cfg(not(windows))]
#[test]
fn there_is_no_launcher_and_no_proc_tool_off_windows() {
    use heddle_connectors::{local_connector_with_run, EmbeddedServer, RunAccess, RunDirs};
    use heddle_sandbox::Sandbox;

    // `expect_err` is unavailable here and that is not an accident: it needs
    // `T: Debug`, and off Windows `Sandbox` is an uninhabited type with no
    // business deriving anything. Matching says the same thing without asking
    // the product for a trait only a test wants.
    fn refusal<T, E>(outcome: Result<T, E>, expectation: &str) -> E {
        match outcome {
            Ok(_) => panic!("{expectation}"),
            Err(e) => e,
        }
    }

    let dir = TempDir::new().expect("a temp dir");

    let refused = refusal(
        Sandbox::create(dir.path(), &[]),
        "there is no launcher backend on this platform",
    );
    assert!(
        refused.contains("Windows-only"),
        "the refusal must name the platform rule: {refused}"
    );

    let root = FsRoot::new(dir.path()).expect("a canonicalizable root");
    let carried = refusal(
        EmbeddedServer::with_run(root, RunAccess::Allowed(RunDirs::none()), uncancelled()),
        "asking for a launcher here must fail rather than serve a missing tool",
    );
    assert!(
        carried.to_string().contains("Windows-only"),
        "and it must carry the same reason up: {carried}"
    );

    // Denied is still a perfectly ordinary configuration here, and it must
    // advertise the three tools this platform really does have.
    let root = FsRoot::new(dir.path()).expect("a canonicalizable root");
    let mut denied = local_connector_with_run(root, RunAccess::Denied, uncancelled())
        .expect("a denied launcher still serves");
    let advertised = names(&mut denied);
    assert!(
        !advertised.iter().any(|name| name.starts_with("proc_")),
        "no `proc_` tool may be advertised off Windows: {advertised:?}"
    );
    assert_eq!(advertised, vec!["fs_list", "fs_read", "fs_write"]);
}

/// The tool description is the **only** channel that reaches the model with
/// this information (spec 020 SC-007).
///
/// `RmcpToolTransport::list` maps name, description and parameters into a
/// `ToolSpec` and drops the server's `instructions`, so a model shown
/// `proc_run` cannot otherwise tell a reachable `cargo` from an unreachable
/// one. Leaving it to the refusal to teach costs a wasted turn and an `isError`
/// round trip to learn something the operator already decided at launch.
///
/// With no run directory the advertisement must gain **nothing** — that is the
/// half that keeps every assertion slice 019 pinned true.
#[cfg(windows)]
#[test]
fn the_advertised_description_names_the_allowlisted_directories() {
    use heddle_connectors::{local_connector_with_run, RunAccess, RunDirs};

    fn proc_run_description(connector: &mut LocalConnector) -> String {
        connector
            .list()
            .expect("tools/list reaches the server")
            .into_iter()
            .find(|s| s.name == "proc_run")
            .expect("proc_run is advertised")
            .description
    }

    let dir = TempDir::new().expect("a temp dir");
    let toolbin = TempDir::new().expect("a temp run directory");
    let dirs = RunDirs::new(&[toolbin.path().to_path_buf()]).expect("a real directory");
    let named = dirs.paths()[0].to_string_lossy().replace(r"\\?\", "");

    let _pruned = guard::PrunedOnDrop::of_root(dir.path());
    let mut listed = local_connector_with_run(
        FsRoot::new(dir.path()).expect("a canonicalizable root"),
        RunAccess::Allowed(dirs),
        uncancelled(),
    )
    .expect("the sandbox builds and the server serves");
    let with_dirs = proc_run_description(&mut listed);

    assert!(
        with_dirs.contains(&named),
        "a model cannot ask for what it is not told it can reach: {with_dirs}"
    );
    // The static string stays the single home of the rule and the caps; the
    // appended sentence only enumerates.
    assert!(
        with_dirs.contains("PATH is not searched"),
        "and the rule the operator did not change must survive the edit: {with_dirs}"
    );

    let mut bare = local_connector_with_run(
        FsRoot::new(dir.path()).expect("a canonicalizable root"),
        RunAccess::Allowed(RunDirs::none()),
        uncancelled(),
    )
    .expect("a launcher with no run directory still serves");
    let without_dirs = proc_run_description(&mut bare);

    assert!(
        !without_dirs.contains("also looked for in"),
        "with nothing allowlisted there is nothing to enumerate: {without_dirs}"
    );
    assert!(
        with_dirs.starts_with(&without_dirs),
        "the appended sentence must be an addition and not a rewrite:\n{without_dirs}\n{with_dirs}"
    );
}
