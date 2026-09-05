// A release build must not open a console window behind the app on Windows;
// a debug build must, because that is where the child's stderr goes.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! `heddle-ui` — the Tauri Chat window, as a running process.
//!
//! Wiring only, like every `heddle` subcommand: the ACP client is
//! `session::SessionHandle`'s, the launch flags are `config`'s, the containment
//! is `heddle_connectors::FsRoot`'s, and the loop, the tools and the chain are
//! the child `heddle acp-agent`'s. This file owns exactly six commands and two
//! events, and every one of the six is a call the CLI already serves —
//! `docs/UI.md` holds the table.
//!
//! The frontend can therefore do nothing the CLI cannot: it cannot choose a
//! model, name a directory, spawn a process, or reach a provider. It can send
//! text, cancel, browse and read inside the one directory the *operator* named,
//! and render what came back (Constitution I).

use heddle_connectors::FsRoot;
use heddle_ui::code::{self, Entry};
use heddle_ui::config::{launch_from_env, ResolvedLaunch};
use heddle_ui::session::SessionHandle;
use heddle_ui::settings::{self, SessionSettings};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use agent_client_protocol::schema::v1::StopReason;

/// What every command needs before it can answer anything.
const NOT_STARTED: &str = "no session: the agent has not started";

/// The one session this window drives, and two read-only projections of the
/// launch it was started from. Multi-session is a later slice; a `HeddleAgent`
/// already supports it, this window does not expose it.
///
/// The `fs_root` and `settings` slots are filled **once**, in `start_session`,
/// from the same `ResolvedLaunch` the child was spawned with. Re-deriving
/// either later from `std::env` would let the Code view browse a directory the
/// running agent was never given, and the Settings screen report a capability
/// it does not have — the same class of drift `FsRoot` pins a directory handle
/// to prevent.
#[derive(Default)]
struct AppState {
    session: Mutex<Option<SessionHandle>>,
    fs_root: Mutex<Option<Arc<FsRoot>>>,
    settings: Mutex<Option<SessionSettings>>,
}

impl AppState {
    /// A clone of the live session, with the lock released before the caller
    /// awaits anything: a `MutexGuard` must never be held across an `.await`.
    fn session(&self) -> Result<SessionHandle, String> {
        self.session
            .lock()
            .expect("the session slot")
            .clone()
            .ok_or_else(|| NOT_STARTED.to_string())
    }

    /// The session's root, or `None` when it was launched without one.
    ///
    /// Cloned out and the lock released here, so no command holds the mutex
    /// while it touches the filesystem.
    fn fs_root(&self) -> Option<Arc<FsRoot>> {
        self.fs_root.lock().expect("the fs-root slot").clone()
    }
}

/// Where `heddle` is expected to be: beside this executable.
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn launch() -> Result<ResolvedLaunch, String> {
    launch_from_env(|name| std::env::var(name).ok(), &exe_dir())
}

/// Spawns `heddle acp-agent` and opens one session: ACP `initialize`, then
/// `session/new`. Returns the session id the chain will record runs under.
#[tauri::command]
async fn start_session(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    if let Ok(existing) = state.session() {
        return Ok(existing.session_id().to_string());
    }

    let updates = app.clone();
    let exited = app.clone();
    let resolved = launch()?;
    // Opened before the child is spawned and held for the session's lifetime,
    // so the Code view browses the very directory the agent was handed. A root
    // that cannot be opened is a **loud** refusal here rather than a per-click
    // one later, matching `ToolArgs::verify_root`'s own ordering: an operator
    // who mistyped `HEDDLE_UI_FS_ROOT` hears about it before a model does.
    let fs_root = match &resolved.fs_root {
        Some(path) => Some(Arc::new(FsRoot::new(path).map_err(|e| e.to_string())?)),
        None => None,
    };
    let settings = settings::session_settings(&resolved);

    let handle = SessionHandle::start(
        resolved.launch.clone(),
        move |notification| {
            // Relayed 1:1 and untransformed: the projection that produced this
            // update is `heddle-acp`'s `project_updates`, reading the chain. The
            // window is a view of that view, and adds nothing to it
            // (Constitution V).
            let _ = updates.emit("session-update", notification.update);
        },
        move |reason| {
            let _ = exited.emit("agent-exited", reason);
        },
    )?;

    let session_id = handle.session_id().to_string();
    *state.fs_root.lock().expect("the fs-root slot") = fs_root;
    *state.settings.lock().expect("the settings slot") = Some(settings);
    *state.session.lock().expect("the session slot") = Some(handle);
    Ok(session_id)
}

/// Sends one ACP `session/prompt` and resolves with its `StopReason`.
///
/// Every `session-update` event for the turn has already been emitted by the
/// time this returns: `heddle-acp` sends the run's whole batch before it answers
/// the prompt, so there is no token-level stream to subscribe to.
#[tauri::command]
async fn send_prompt(text: String, state: State<'_, AppState>) -> Result<StopReason, String> {
    state.session()?.prompt(&text).await
}

/// Sends one ACP `session/cancel`.
///
/// Takes effect at the next turn boundary: a model call already in flight
/// completes (`crates/heddle-acp/src/cancel.rs`).
#[tauri::command]
async fn cancel_run(state: State<'_, AppState>) -> Result<(), String> {
    state.session()?.cancel()
}

/// One directory inside the session's `--fs-root`, listed.
///
/// The same read `fs_list` performs for the agent, through the same `FsRoot`
/// handle, with the same containment. Synchronous rather than `async`: there is
/// no protocol round trip to await here, only a handle-relative `read_dir`.
#[tauri::command]
fn list_directory(path: String, state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    code::list_directory(state.fs_root().as_deref(), &path)
}

/// One file inside the session's `--fs-root`, read as text.
///
/// The same read `fs_read` performs for the agent, with the same byte cap and
/// the same refusal for anything that is not UTF-8 text.
#[tauri::command]
fn read_file(path: String, state: State<'_, AppState>) -> Result<String, String> {
    code::read_file(state.fs_root().as_deref(), &path)
}

/// What the running session's connectors are — reported, never changed.
///
/// No environment variable is read here: this is the snapshot `start_session`
/// took from the `ResolvedLaunch` the child was really spawned from.
#[tauri::command]
fn session_settings(state: State<'_, AppState>) -> Result<SessionSettings, String> {
    state
        .settings
        .lock()
        .expect("the settings slot")
        .clone()
        .ok_or_else(|| NOT_STARTED.to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_session,
            send_prompt,
            cancel_run,
            list_directory,
            read_file,
            session_settings
        ])
        // One boundary turns the session into a closed one, exactly like
        // `crates/heddle-cli/src/main.rs` turns an error into a message: the
        // child's stdin closes, `heddle acp-agent` exits zero, nothing leaks.
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(state) = window.try_state::<AppState>() {
                    if let Some(session) = state.session.lock().expect("the session slot").take() {
                        session.close();
                    }
                    // Dropped with the session rather than left behind: `FsRoot`
                    // pins its directory for as long as it lives, and a closed
                    // window must stop holding the operator's folder open.
                    state.fs_root.lock().expect("the fs-root slot").take();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("the Heddle window starts");
}
