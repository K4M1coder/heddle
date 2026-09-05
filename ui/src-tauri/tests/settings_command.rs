//! Acceptance tests for the Settings screen's one command (spec 041).
//!
//! The screen's claim is that it reports the **running session's** real
//! connector status, so every fixture here goes through the same
//! `config::launch_from_env` the child was launched from, with a real `TempDir`
//! standing in for `--fs-root` — including a really `git init`ed one, because
//! "is this a repository" is answered by `heddle_connectors::is_git_repository`
//! walking the directory and not by a boolean this test could set.
//!
//! No window and no `AppHandle`: `settings.rs` names no Tauri type.
//!
//! Written before `ui/src-tauri/src/settings.rs` existed (Constitution III).

use heddle_ui::config::launch_from_env;
use heddle_ui::settings::{session_settings, ConnectorState, ConnectorStatus, SessionSettings};
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |name: &str| map.get(name).cloned()
}

/// The settings a window would report for this environment, resolved exactly
/// once — the same single resolution the child process was handed.
fn settings(pairs: &[(&str, &str)]) -> SessionSettings {
    let resolved = launch_from_env(env_of(pairs), Path::new("/app")).expect("a launch");
    session_settings(&resolved)
}

fn connector<'a>(settings: &'a SessionSettings, name: &str) -> &'a ConnectorStatus {
    settings
        .connectors
        .iter()
        .find(|status| status.name == name)
        .unwrap_or_else(|| panic!("the screen must report {name}"))
}

fn base() -> Vec<(&'static str, &'static str)> {
    vec![("HEDDLE_ROOT", "/silos"), ("HEDDLE_UI_MODEL", "llama3.1")]
}

/// `base()` plus a real directory as `--fs-root`.
fn rooted(dir: &TempDir) -> Vec<(&'static str, &str)> {
    let mut pairs = base();
    pairs.push((
        "HEDDLE_UI_FS_ROOT",
        dir.path().to_str().expect("a utf-8 temp path"),
    ));
    pairs
}

#[test]
fn every_connector_the_product_ships_is_named_so_none_reads_as_forgotten() {
    let settings = settings(&base());
    let names: Vec<&str> = settings
        .connectors
        .iter()
        .map(|status| status.name.as_str())
        .collect();
    assert_eq!(names, vec!["fs", "git", "shell", "atlassian", "m365"]);
}

#[test]
fn with_no_fs_root_the_session_has_no_tools_and_the_screen_says_so() {
    let settings = settings(&base());
    assert_eq!(settings.fs_root, None);
    for name in ["fs", "git", "shell"] {
        let status = connector(&settings, name);
        assert_eq!(
            status.state,
            ConnectorState::Disabled,
            "{name} must be off when the session was given no root"
        );
        assert!(
            status.tools.is_empty(),
            "{name} must offer no tool when the session was given no root"
        );
        assert!(
            !status.detail.is_empty(),
            "{name} must say why it is off, not merely be off"
        );
    }
}

#[test]
fn a_configured_root_turns_the_filesystem_tools_on_and_names_them() {
    let dir = TempDir::new().expect("a temp root");
    let settings = settings(&rooted(&dir));

    assert_eq!(
        settings.fs_root.as_deref(),
        Some(dir.path().to_str().expect("a utf-8 temp path"))
    );
    let fs = connector(&settings, "fs");
    assert_eq!(fs.state, ConnectorState::Enabled);
    // The names `ToolArgs::agent_policy` really allowlists for a session with a
    // root, not a prose paraphrase of them.
    assert_eq!(fs.tools, vec!["fs_read", "fs_list", "fs_write"]);
}

#[test]
fn git_is_on_only_when_the_configured_root_is_really_a_repository() {
    let plain = TempDir::new().expect("a temp root");
    let off = settings(&rooted(&plain));
    assert_eq!(connector(&off, "git").state, ConnectorState::Disabled);
    assert!(connector(&off, "git").tools.is_empty());
    assert!(
        !connector(&off, "git").detail.is_empty(),
        "a disabled connector must say why, not just be off"
    );

    // A real repository, made the way one really is: the answer comes from
    // `is_git_repository` walking this directory — the same call
    // `ToolArgs::git_tools` makes to decide whether the CLI offers the tools.
    let repo = TempDir::new().expect("a temp root");
    git2::Repository::init(repo.path()).expect("a real git repository");
    let on = settings(&rooted(&repo));
    assert_eq!(connector(&on, "git").state, ConnectorState::Enabled);
    assert_eq!(connector(&on, "git").tools, vec!["git_status", "git_log"]);
}

#[test]
fn shell_follows_allow_run_and_is_off_by_default() {
    let dir = TempDir::new().expect("a temp root");
    let mut pairs = rooted(&dir);

    assert_eq!(
        connector(&settings(&pairs), "shell").state,
        ConnectorState::Disabled,
        "proc_run is a second opt-in on top of a root, and it stays off until it is taken"
    );

    pairs.push(("HEDDLE_UI_ALLOW_RUN", "true"));
    let on = settings(&pairs);
    assert_eq!(connector(&on, "shell").state, ConnectorState::Enabled);
    assert_eq!(connector(&on, "shell").tools, vec!["proc_run"]);
}

#[test]
fn atlassian_and_m365_report_that_no_flag_wires_them_to_a_session_yet() {
    // Not a toggle the operator forgot to flip: `heddle acp-agent` accepts no
    // flag that enables either connector for a session at all (specs/039's own
    // "Out of scope"), so a disabled-looking switch here would be the window
    // inventing a capability the CLI does not serve (Constitution I).
    let dir = TempDir::new().expect("a temp root");
    let mut rich = rooted(&dir);
    rich.push(("HEDDLE_UI_ALLOW_RUN", "true"));

    for settings in [settings(&base()), settings(&rich)] {
        for name in ["atlassian", "m365"] {
            let status = connector(&settings, name);
            assert_eq!(
                status.state,
                ConnectorState::NotWiredToSession,
                "{name} is neither on nor off: no flag reaches it"
            );
            assert!(status.tools.is_empty());
            assert!(
                status.detail.contains("no flag"),
                "{name} must say why it is neither"
            );
        }
    }
}

#[test]
fn the_screen_serialises_to_the_camel_case_wire_shape_settings_state_ts_reads() {
    let wire = serde_json::to_value(settings(&base())).expect("the settings serialise");
    assert!(
        wire.get("fsRoot").is_some(),
        "the frontend types are the wire shapes, and the wire is camelCase"
    );
    assert_eq!(wire["connectors"][0]["name"], "fs");
    assert_eq!(wire["connectors"][0]["state"], "disabled");
    assert_eq!(wire["connectors"][3]["state"], "notWiredToSession");
}
