//! The Code view's two reads, and no third thing.
//!
//! **This module adds no capability.** Every path it is handed is resolved by
//! [`FsRoot`] — the handle-relative containment primitive the CLI's own
//! `fs_read` and `fs_list` tools resolve every model-supplied path through
//! (`crates/heddle-connectors/src/fs.rs`) — so the Code view can reach exactly
//! what the running session's agent can reach, and nothing else. The window
//! does not re-derive "is this path inside the root": there is one answer to
//! that question in this product and this is not where it lives (Constitution
//! I and VII).
//!
//! The caps and refusals below are `fs_read`'s own, deliberately: a file the
//! agent is refused for being oversize or non-textual is refused to the
//! operator's Code view for the same reason and in the same words. A view that
//! could read what the tool cannot would be a second, wider filesystem surface
//! wearing a read-only label.
//!
//! **No Tauri type appears here**, for `session.rs`'s reason: `tests/
//! code_commands.rs` drives these functions against a real `TempDir` with no
//! `AppHandle` and no window.

use heddle_connectors::{FsRoot, READ_BYTE_CAP};
use serde::Serialize;
use std::io::{ErrorKind, Read};

/// What the window says when the session was launched with no `--fs-root`.
///
/// A distinct, pinned message rather than an empty listing: absent a root the
/// session has no tools at all (`crates/heddle-cli/src/wiring.rs`'s "no root,
/// no tools"), and rendering that as an empty directory would be the window
/// reporting on a directory it never looked at.
pub const NO_FS_ROOT: &str =
    "this session has no fs-root: there is nothing to browse, and the agent has no tools either";

/// One row of a directory listing.
///
/// `camelCase` on the wire for `chatState.ts`'s reason: the frontend types are
/// the wire shapes, so a rename here breaks a test rather than a running app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The entry's own name, never a path: the frontend joins it onto the
    /// directory it asked for, so nothing here can carry a traversal.
    pub name: String,
    /// Whether listing *this* entry would show more entries.
    pub directory: bool,
}

/// The root itself, for a frontend that has not navigated anywhere yet.
///
/// `FsRoot` refuses an empty path — correctly, for a model that supplied
/// nothing — but the Code view's initial state genuinely means "the root", so
/// it is spelled as such here rather than surfaced as a refusal.
fn or_root(path: &str) -> &str {
    if path.trim().is_empty() {
        "."
    } else {
        path
    }
}

/// One directory's real children, sorted directories-first and then by name.
///
/// Not recursive, exactly as `fs_list` is not: the frontend lists a
/// subdirectory when the operator opens it, so one click is one read.
pub fn list_directory(root: Option<&FsRoot>, path: &str) -> Result<Vec<Entry>, String> {
    let root = root.ok_or_else(|| NO_FS_ROOT.to_string())?;
    let arg = or_root(path);
    let mut entries = Vec::new();
    for entry in root.read_dir(arg)? {
        let entry = entry.map_err(|e| format!("{arg}: {e}"))?;
        entries.push(Entry {
            name: entry.file_name().to_string_lossy().into_owned(),
            directory: entry.file_type().is_ok_and(|kind| kind.is_dir()),
        });
    }
    // Sorted so the same directory reads the same way twice — `fs_list`'s own
    // reason, one layer out: an operator given a different order on every
    // redraw cannot tell a change from a shuffle. Directories lead because that
    // is what a tree is expected to look like, not because of the tab order
    // `fs_list`'s `dir`/`file` prefixes happen to produce.
    entries.sort_by(|left, right| {
        right
            .directory
            .cmp(&left.directory)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(entries)
}

/// One file's real content as text, or a stated reason it cannot be shown.
///
/// The size and the bytes come off the **same** open handle, for `fs_read`'s
/// recorded reason: the file measured against the cap must provably be the file
/// returned.
pub fn read_file(root: Option<&FsRoot>, path: &str) -> Result<String, String> {
    let root = root.ok_or_else(|| NO_FS_ROOT.to_string())?;
    let arg = path.trim();
    let mut file = root.open_file(arg)?;
    let size = file.metadata().map_err(|e| format!("{arg}: {e}"))?.len();
    if size > READ_BYTE_CAP as u64 {
        return Err(format!(
            "{arg} is {size} bytes, over the {READ_BYTE_CAP}-byte read cap that `fs_read` applies \
             to the same file; open a smaller one"
        ));
    }
    let mut contents = String::new();
    match Read::read_to_string(&mut file, &mut contents) {
        Ok(_) => Ok(contents),
        // A stated refusal, not a lossy decode: showing replacement characters
        // where bytes were would be the window inventing content, and this
        // screen's whole claim is that it displays the file that is on disk.
        Err(e) if e.kind() == ErrorKind::InvalidData => Err(format!(
            "{arg} is not UTF-8 text and cannot be shown; this view reads text files only"
        )),
        Err(e) => Err(format!("{arg}: {e}")),
    }
}
