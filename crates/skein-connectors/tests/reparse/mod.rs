//! Planting a reparse point after a root or a server already exists.
//!
//! Spec 021's containment tests need this on three sides of the crate — the
//! `FsRoot` walk itself, the three fs tools over a server, and a whole governed
//! run — and each integration test is its own crate, so it was written out three
//! times. It is a `tests/reparse/mod.rs` directory module rather than a
//! `tests/reparse.rs` file for `tests/guard/mod.rs`'s reason: that is what makes
//! cargo treat it as a shared module instead of a test binary of its own.
//!
//! Only what **all three** callers use lives here. Each test binary compiles
//! this module for itself, so a helper one of them does not call is dead code in
//! that binary, and `-D warnings` is right to say so — which is why
//! `reparse_file`, whose one caller is `fs_root.rs`, stays there.

use std::path::Path;

/// A reparse point at `link` leading to the **directory** `target`.
///
/// A junction on Windows rather than a symlink, because a junction needs no
/// privilege and `symlink_dir` needs `SeCreateSymbolicLinkPrivilege` — which
/// this project's own developer machines do not have, so every symlink test
/// written against it has silently skipped since slice 016.
/// `std::os::windows::fs::junction_point` would be the direct route and is
/// nightly-only, so `mklink /J` it is.
pub fn reparse_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let ok = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?
            .success();
        ok.then_some(())
            .ok_or_else(|| std::io::Error::other("mklink /J refused"))
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)
}
