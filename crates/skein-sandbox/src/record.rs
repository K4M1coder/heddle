//! The one thing a profile cannot tell you about itself: which directories
//! carry its ACEs.
//!
//! The profile name is `sha256(root)` truncated, which is one-way, and
//! `Win32::Security::Isolation` has no API that maps a profile to a directory.
//! So the directories are written down at grant time or they are lost. This
//! module is that file, and nothing else: one absolute path per line, UTF-8, the
//! fs-root first. A Windows path cannot contain a newline, so the format needs
//! no escaping, no delimiter and no dependency.
//!
//! It lives in `%LOCALAPPDATA%\Packages\<profile>\`, the folder Windows itself
//! created for the profile, for two measured reasons. Its DACL names SYSTEM,
//! Administrators and the user and **not** the AppContainer SID, so the
//! sandboxed child cannot read or rewrite what a later prune will act on — its
//! own `AC` subfolder is the writable one, which is why the record is not put
//! there. And `DeleteAppContainerProfile` removes that folder, so the record
//! cannot outlive what it describes.

use std::path::{Path, PathBuf};

/// The file, inside each profile's own package folder.
pub(crate) const RECORD_NAME: &str = "skein-grants";

/// `%LOCALAPPDATA%\Packages`, the one derivation of that path in this crate.
///
/// `GetAppContainerFolderPath` would answer a *different* question — it returns
/// the `AC` subfolder — and having two derivations that can disagree is worse
/// than having one that can be wrong.
pub(crate) fn packages_dir() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(|local| PathBuf::from(local).join("Packages"))
        .ok_or_else(|| {
            "LOCALAPPDATA is unset, so the app container profile directory cannot be located"
                .to_string()
        })
}

pub(crate) fn record_path(profile: &str) -> Result<PathBuf, String> {
    Ok(packages_dir()?.join(profile).join(RECORD_NAME))
}

/// The directories recorded for `profile`, or `None` if it has no record.
///
/// `None` is not an error: every profile created before this slice exists
/// without one, and a listing that hid them would hide a thousand profiles.
pub(crate) fn read(profile: &str) -> Result<Option<Vec<PathBuf>>, String> {
    let path = record_path(profile)?;
    match std::fs::read_to_string(&path) {
        // An empty or truncated file collapses to `None` rather than to an
        // empty list: a record naming no directory says exactly what no record
        // says, and letting the two shapes differ would put a profile in a
        // listing with no line of its own.
        Ok(text) => Ok(Some(parse(&text)).filter(|paths: &Vec<PathBuf>| !paths.is_empty())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!(
            "{}: the grant record is unreadable: {e}",
            path.display()
        )),
    }
}

fn parse(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Adds `dirs` to `profile`'s record, keeping what is already there.
///
/// Cumulative rather than overwriting, because two sessions over one workspace
/// may name different `--run-dir`s and both grants persist on the one profile.
/// Replacing the record would leave the first session's ACE with nothing
/// pointing at it — precisely the leak this module exists to close. Order is
/// first-seen, so the fs-root written by the first `create` stays first.
pub(crate) fn append(profile: &str, dirs: &[&Path]) -> Result<(), String> {
    let path = record_path(profile)?;
    let mut known = read(profile)?.unwrap_or_default();
    for dir in dirs {
        if !known.iter().any(|seen| seen == *dir) {
            known.push(dir.to_path_buf());
        }
    }

    let mut text = String::new();
    for dir in &known {
        text.push_str(&dir.to_string_lossy());
        text.push('\n');
    }
    std::fs::write(&path, text)
        .map_err(|e| format!("{}: the grant record is not writable: {e}", path.display()))
}
