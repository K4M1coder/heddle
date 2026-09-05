// A release build must not open a console window behind the app on Windows;
// a debug build must, because that is where the child's stderr goes.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! `heddle-ui` — the Tauri Chat window, as a running process.
//!
//! Wiring only, like every `heddle` subcommand: the ACP client is
//! `session::SessionHandle`'s, the launch flags are `config`'s, and the loop,
//! the tools and the chain are the child `heddle acp-agent`'s. This file owns
//! exactly three commands and one event, and each of the three commands is one
//! ACP call the CLI already serves — `docs/UI.md` holds the table.
//!
//! The frontend can therefore do nothing the CLI cannot: it cannot choose a
//! model, name a directory, spawn a process, or reach a provider. It can send
//! text, cancel, and render what came back (Constitution I).

use heddle_ui::config::launch_from_env;
use heddle_ui::session::{AgentLaunch, SessionHandle};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

use agent_client_protocol::schema::v1::StopReason;

/// The one session this window drives. Multi-session is a later slice; a
/// `HeddleAgent` already supports it, this window does not expose it.
#[derive(Default)]
struct AppState {
    session: Mutex<Option<SessionHandle>>,
}

impl AppState {
    /// A clone of the live session, with the lock released before the caller
    /// awaits anything: a `MutexGuard` must never be held across an `.await`.
    fn session(&self) -> Result<SessionHandle, String> {
        self.session
            .lock()
            .expect("the session slot")
            .clone()
            .ok_or_else(|| "no session: the agent has not started".to_string())
    }
}

/// Where `heddle` is expected to be: beside this executable.
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn launch() -> Result<AgentLaunch, String> {
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
    let handle = SessionHandle::start(
        launch()?,
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

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_session,
            send_prompt,
            cancel_run
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
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("the Heddle window starts");
}
