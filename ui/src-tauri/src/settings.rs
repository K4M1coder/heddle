//! What the running session's connectors really are, reported and not decided.
//!
//! **This module changes nothing.** It is a projection of one already-made
//! decision — the `ResolvedLaunch` the child process was actually spawned from
//! — into rows a screen can paint. There is no toggle here, and that is
//! structural rather than a first-slice shortcut: enabling a connector is
//! `heddle acp-agent`'s flags, so a switch in this window would be the UI
//! serving a capability the CLI does not (Constitution I).
//!
//! Two consequences worth stating in one place, because both look like
//! omissions and neither is:
//!
//! 1. **`git` is derived, not configured.** There is no `--git` flag; the CLI
//!    offers `git_status`/`git_log` exactly when `--fs-root` names a real
//!    repository (`ToolArgs::git_tools`). This module calls the same
//!    [`is_git_repository`] against the same root rather than re-deriving what
//!    "is a repository" means.
//! 2. **`atlassian` and `m365` are neither on nor off.** They exist in
//!    `heddle-connectors` (specs 039 and 040) and no `heddle` subcommand accepts
//!    a flag that wires either to a session — specs/039's own "Out of scope"
//!    defers that enablement policy deliberately. So they are reported as
//!    [`ConnectorState::NotWiredToSession`], which is the true status, instead
//!    of as a disabled switch that implies an operator could flip it.
//!
//! **No Tauri type appears here**, for `session.rs`'s reason, and `std::env` is
//! never read here either: the snapshot arrives as an argument so that what the
//! screen reports can never drift from what the running child was launched
//! with.

use crate::config::ResolvedLaunch;
use heddle_connectors::{is_git_repository, FsRoot};
use serde::Serialize;

/// Whether a connector is serving this session, and the third answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectorState {
    /// The session offers this connector's tools.
    Enabled,
    /// The session could offer them and does not: a flag was not passed, or a
    /// condition the flag depends on is not met.
    Disabled,
    /// The connector is compiled in and **nothing can turn it on for a
    /// session**, because no CLI flag reaches it yet. Not a disabled switch.
    NotWiredToSession,
}

/// One connector, as the Settings screen reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorStatus {
    /// The connector's own name, as `docs/` and the specs already call it.
    pub name: String,
    pub state: ConnectorState,
    /// The tool names this session really offers for it — the names
    /// `ToolArgs::agent_policy` allowlists, not a paraphrase. Empty whenever
    /// the state is not [`ConnectorState::Enabled`].
    pub tools: Vec<String>,
    /// Why it is in that state, in the operator's terms. Never empty: a row
    /// that is merely off tells an operator nothing they can act on.
    pub detail: String,
}

/// The whole of what the Settings screen shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSettings {
    /// The one directory this session may work in, as the operator named it.
    pub fs_root: Option<String>,
    pub connectors: Vec<ConnectorStatus>,
}

fn status(name: &str, state: ConnectorState, tools: &[&str], detail: &str) -> ConnectorStatus {
    ConnectorStatus {
        name: name.to_string(),
        state,
        tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
        detail: detail.to_string(),
    }
}

/// The running session's connector status, derived from the one resolution the
/// child was launched from.
///
/// A `--fs-root` that cannot be opened is reported as *off with the reason*
/// rather than as an error for the whole screen: `main.rs` has already refused
/// to start a session over an unopenable root, so reaching this arm at all
/// means the directory changed under a running window, and an operator meeting
/// that deserves the other four rows as well as this one.
pub fn session_settings(resolved: &ResolvedLaunch) -> SessionSettings {
    let root = resolved
        .fs_root
        .as_ref()
        .and_then(|path| FsRoot::new(path).ok());
    let repository = root.as_ref().is_some_and(is_git_repository);

    let fs = match &root {
        Some(_) => status(
            "fs",
            ConnectorState::Enabled,
            // `agent_policy`'s list, `fs_write` included: `heddle acp-agent`
            // approves it so the decision reaches the human behind the editor.
            // This window's own Code view is read-only regardless — it is a
            // view of the session's reach, not the whole of it.
            &["fs_read", "fs_list", "fs_write"],
            "The session may read and write inside HEDDLE_UI_FS_ROOT, and nowhere else.",
        ),
        None => status(
            "fs",
            ConnectorState::Disabled,
            &[],
            "HEDDLE_UI_FS_ROOT is not set, so this session has no tools at all: no root, no tools.",
        ),
    };

    let git = if repository {
        status(
            "git",
            ConnectorState::Enabled,
            &["git_status", "git_log"],
            "HEDDLE_UI_FS_ROOT is a git repository, so the read-only git tools are offered.",
        )
    } else if root.is_some() {
        status(
            "git",
            ConnectorState::Disabled,
            &[],
            "HEDDLE_UI_FS_ROOT is not a git repository. There is no flag for this: the CLI \
             offers the git tools exactly when the root it was given is one.",
        )
    } else {
        status(
            "git",
            ConnectorState::Disabled,
            &[],
            "HEDDLE_UI_FS_ROOT is not set, so there is no repository to read.",
        )
    };

    let shell = if resolved.allow_run {
        status(
            "shell",
            ConnectorState::Enabled,
            &["proc_run"],
            "HEDDLE_UI_ALLOW_RUN is on, so the sandboxed process tool is offered over the root. \
             Windows-only in v0; elsewhere the agent refuses the flag rather than ignoring it.",
        )
    } else if root.is_some() {
        status(
            "shell",
            ConnectorState::Disabled,
            &[],
            "HEDDLE_UI_ALLOW_RUN is not set. Running a process is a second opt-in on top of the \
             root, and it is off until it is taken.",
        )
    } else {
        status(
            "shell",
            ConnectorState::Disabled,
            &[],
            "HEDDLE_UI_FS_ROOT is not set, and the process tool is offered over the root or not \
             at all.",
        )
    };

    // The one sentence this screen exists to be able to say honestly. Both rows
    // carry it because both are true for the same reason.
    let deferred = |connector: &str, spec: &str| {
        format!(
            "The {connector} connector is compiled in ({spec}), but no flag on any `heddle` \
             subcommand wires it to a session yet, so it is neither on nor off here. Enabling it \
             is CLI work, not window work."
        )
    };

    SessionSettings {
        fs_root: resolved
            .fs_root
            .as_ref()
            .map(|path| path.display().to_string()),
        connectors: vec![
            fs,
            git,
            shell,
            status(
                "atlassian",
                ConnectorState::NotWiredToSession,
                &[],
                &deferred("Atlassian", "specs/039-atlassian-connector"),
            ),
            status(
                "m365",
                ConnectorState::NotWiredToSession,
                &[],
                &deferred("Microsoft 365", "specs/040-m365-connector"),
            ),
        ],
    }
}
