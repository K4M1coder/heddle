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

use std::fs::OpenOptions;
use std::io::{Read, Write};
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
    match read_locked(&path) {
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

/// The record's whole text, read under a **shared** lock.
///
/// The lock is not optional once [`append`] takes an exclusive one: a Windows
/// byte-range lock does not merely order writers, it makes an unlocked read of
/// the locked range *fail*. So a plain read here would turn a concurrent
/// session's append into an error out of `list` or `prune`. Shared against
/// shared it does not wait, and it also means a reader never sees the record
/// part-written.
fn read_locked(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    file.lock_shared()?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
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
///
/// The read and the write happen under **one exclusive lock on this file**, and
/// they have to: nothing sequences two sessions over one `--fs-root`, they
/// derive the same profile name and so share this one record, and an unlocked
/// read-modify-write drops whichever run directory was written first. The DACL
/// grant merges idempotently per-SID and so survives, which is what makes the
/// dropped line a live ACE that neither `list` nor `prune` can see — the same
/// leak, one level down.
///
/// Only the new lines are written, never the whole file again. The dedup below
/// already makes the two equivalent — the result is always the old content plus
/// whatever was not in it — and it means a reader cannot catch the record
/// *empty*, only short of its last line. `read` collapses an empty record to
/// "no record at all", which `prune` reports as `unrecorded` and revokes
/// nothing for.
pub(crate) fn append(profile: &str, dirs: &[&Path]) -> Result<(), String> {
    let path = record_path(profile)?;
    let unwritable =
        |e: std::io::Error| format!("{}: the grant record is not writable: {e}", path.display());

    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&path)
        .map_err(unwritable)?;
    // Blocking, not `try_lock`: the other holder is another session doing this
    // same short read-and-append, so waiting for it is the point. The lock is
    // released when the handle closes, which every path out of here does.
    file.lock().map_err(unwritable)?;

    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| format!("{}: the grant record is unreadable: {e}", path.display()))?;
    let mut known = parse(&text);

    let mut added = String::new();
    // A record this function wrote always ends in a newline. One a killed
    // process left half-written may not, and appending to that would splice two
    // directories into a single path naming neither.
    if !text.is_empty() && !text.ends_with('\n') {
        added.push('\n');
    }
    for dir in dirs {
        if known.iter().any(|seen| seen == *dir) {
            continue;
        }
        added.push_str(&dir.to_string_lossy());
        added.push('\n');
        known.push(dir.to_path_buf());
    }

    file.write_all(added.as_bytes()).map_err(unwritable)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch package folder, removed however the test ends.
    ///
    /// Deliberately **not** a name the cleanup gate accepts — `skein-` plus 16
    /// lowercase hex characters is what `is_skein_profile` requires, and this
    /// is not that — so a `grants()` or `prune()` running in a parallel test
    /// cannot see it, and one left behind could never be acted on.
    struct Scratch(String);

    impl Scratch {
        fn new() -> Scratch {
            let name = format!("skein-record-test-{}", std::process::id());
            std::fs::create_dir_all(packages_dir().expect("LOCALAPPDATA is set").join(&name))
                .expect("a scratch package folder");
            Scratch(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ =
                std::fs::remove_dir_all(packages_dir().expect("LOCALAPPDATA is set").join(&self.0));
        }
    }

    /// Two ACP sessions over one workspace start independently, so nothing
    /// sequences their two `append`s onto the one deterministic profile. An
    /// unlocked read-modify-write drops whichever run directory was written
    /// first, and its DACL grant — which merges per-SID and so survives — is
    /// then an ACE that neither `list` nor `prune` can see.
    ///
    /// Driven at this function rather than through `Sandbox::create`, because
    /// this is where the defect is: `CreateAppContainerProfile` has contention
    /// of its own on the same scenario, and a test going through it would be
    /// reporting on two races at once.
    #[test]
    fn concurrent_appends_to_one_record_all_survive() {
        const SESSIONS: usize = 8;

        let scratch = Scratch::new();
        let root = PathBuf::from(r"C:\workspace");
        append(&scratch.0, &[root.as_path()]).expect("the root is recorded first");

        let run_dirs: Vec<PathBuf> = (0..SESSIONS)
            .map(|i| PathBuf::from(format!(r"C:\tools\session-{i}")))
            .collect();
        std::thread::scope(|scope| {
            for dir in &run_dirs {
                scope.spawn(|| {
                    append(&scratch.0, &[dir.as_path()]).expect("every session records its run dir")
                });
            }
        });

        let recorded = read(&scratch.0)
            .expect("the record is readable")
            .expect("eight sessions wrote to it");
        for dir in &run_dirs {
            assert!(
                recorded.contains(dir),
                "{} lost its line to a concurrent append, and its ACE is now unprunable: {recorded:?}",
                dir.display()
            );
        }
        assert_eq!(
            recorded.len(),
            SESSIONS + 1,
            "the root once and each run directory once, got {recorded:?}"
        );
        assert_eq!(
            recorded.first(),
            Some(&root),
            "the root written first stays first, got {recorded:?}"
        );
    }
}
