//! Naming an executable, and rendering what it did.
//!
//! **This is the only module in the workspace that names `skein-sandbox`,** the
//! same boundary `src/git.rs` keeps around `git2` — and for a stronger reason:
//! that crate holds every `unsafe` block in the product.
//!
//! Nothing here interprets shell syntax, and nothing here decides containment.
//! Containment is the AppContainer's DACL, the Job Object and the per-call
//! human approval; what this module decides is which executable a `command`
//! string may name at all, and how a finished run reads to a model.

use crate::fs::FsRoot;
use crate::server::{RUN_OUTPUT_BYTE_CAP, RUN_TIMEOUT};
use skein_sandbox::{Captured, Run, Sandbox};
use std::path::PathBuf;

/// The executable a `command` names: System32, then `%SystemRoot%`, then a path
/// inside the configured root.
///
/// **`%PATH%` is deliberately not searched.** It is ambient, per-process and
/// influenced by anything that has ever written the user's environment;
/// resolving through it would make the reachable executable set undecidable
/// from the configuration. A fixed list plus root-relative paths is decidable
/// and deny-by-default.
///
/// The stated cost, which the refusal message carries so a model meets it
/// rather than guessing: `cargo`, `node`, `python` and everything else under
/// the user profile is **not reachable**, and would not launch even if this
/// found it — no directory there carries an `ALL APPLICATION PACKAGES` ACE.
pub(crate) fn resolve_exe(
    root: &FsRoot,
    _run_dirs: &[PathBuf],
    command: &str,
) -> Result<PathBuf, String> {
    // A separator means the model is naming a path, and a path is only ever
    // resolved against the root — which is what refuses an absolute one,
    // however real the file behind it is.
    if command.contains('/') || command.contains('\\') {
        return root.resolve(command);
    }

    let name = if command.to_ascii_lowercase().ends_with(".exe") {
        command.to_string()
    } else {
        format!("{command}.exe")
    };
    let system_root = PathBuf::from(
        std::env::var_os("SystemRoot")
            .ok_or_else(|| "this Windows installation does not name its own root".to_string())?,
    );
    let system32 = system_root.join("System32");
    for directory in [&system32, &system_root] {
        let candidate = directory.join(&name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "{name} is in neither {} nor {}; %PATH% is deliberately not searched, so name an \
         executable in one of those two directories or a path relative to the configured root",
        system32.display(),
        system_root.display()
    ))
}

/// One bounded launch, rendered the way the line-oriented tools next door
/// render themselves.
pub(crate) fn execute(
    sandbox: &Sandbox,
    root: &FsRoot,
    command: &str,
    args: &[String],
) -> Result<String, String> {
    let exe = resolve_exe(root, sandbox.run_dirs(), command)?;
    let run = sandbox.run(&exe, args, RUN_OUTPUT_BYTE_CAP, RUN_TIMEOUT)?;
    Ok(report(&run))
}

/// `exit <n>`, then each stream under its own header.
///
/// A nonzero exit is inside an `Ok` and reads as `exit <n>`: the process ran,
/// the result is true, and the model needs the output. An `Err` would discard
/// both.
fn report(run: &Run) -> String {
    let mut rendered = format!("exit {}\n", run.exit_code);
    rendered.push_str("--- stdout ---\n");
    push_stream(&mut rendered, "stdout", &run.stdout);
    rendered.push_str("--- stderr ---\n");
    push_stream(&mut rendered, "stderr", &run.stderr);
    rendered
}

/// The captured text, then the drop label **immediately under it**.
///
/// Truncated and labelled, following `STATUS_ENTRY_CAP`'s reasoning rather than
/// `READ_BYTE_CAP`'s: the process has already run and cannot be un-run, and
/// there is no smaller call for the model to make instead — it cannot ask for
/// fewer bytes. A silent truncation would be a wrong answer in a right answer's
/// shape; a refusal would throw away a side effect a human approved.
fn push_stream(rendered: &mut String, name: &str, stream: &Captured) {
    if !stream.text.is_empty() {
        rendered.push_str(&stream.text);
        if !stream.text.ends_with('\n') {
            rendered.push('\n');
        }
    }
    if stream.dropped_bytes > 0 {
        rendered.push_str(&format!(
            "# {} bytes of {name} not shown, over the {RUN_OUTPUT_BYTE_CAP}-byte cap\n",
            stream.dropped_bytes
        ));
    }
}
