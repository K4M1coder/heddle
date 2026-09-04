//! The record `Sandbox::create` leaves beside its profile, and the listing
//! built on it (spec 024 SC-001…SC-005).
//!
//! `#![cfg(windows)]` on the file rather than on each test, for
//! `tests/profile.rs`'s stated reason: there is no `Sandbox` to make on the
//! other two platforms, so the whole file has nothing to say there. The absence
//! gate that *does* run there is `tests/absent.rs`.
#![cfg(windows)]

use skein_sandbox::Sandbox;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The record's path, derived here the way `list` derives it and **not** by
/// asking the crate under test — a test that reused the implementation's own
/// helper would pass even if that helper pointed at the wrong directory.
fn record_of(profile: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA is set on Windows"))
        .join("Packages")
        .join(profile)
        .join("skein-grants")
}

fn recorded(profile: &str) -> Vec<PathBuf> {
    std::fs::read_to_string(record_of(profile))
        .unwrap_or_else(|e| panic!("{}: {e}", record_of(profile).display()))
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Both spellings of one directory, because `TempDir::path` is not canonical
/// and `Sandbox::create` records what it was handed.
fn holds(recorded: &[PathBuf], dir: &Path) -> bool {
    let canonical = dir.canonicalize().ok();
    recorded
        .iter()
        .any(|seen| seen == dir || Some(seen.as_path()) == canonical.as_deref())
}

#[test]
fn a_created_sandbox_records_its_root_and_run_dirs_beside_its_profile() {
    let root = TempDir::new().expect("a temp root");
    let toolbin = TempDir::new().expect("a temp run directory");

    let sandbox = Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
        .expect("the profile, the root's grant and the run directory's");

    let lines = recorded(sandbox.profile());
    assert!(
        holds(&lines, root.path()),
        "the record must name the fs-root, got {lines:?}"
    );
    assert!(
        holds(&lines, toolbin.path()),
        "the record must name the run directory, got {lines:?}"
    );
    // Root first is not cosmetic: it is the only thing that distinguishes the
    // full-access grant from the read-and-execute ones when the record is read
    // back, so `list` labels each directory off this order.
    assert!(
        holds(&lines[..1], root.path()),
        "the fs-root must be the first line, got {lines:?}"
    );
}

/// A second session over one workspace may name different `--run-dir`s, and
/// both grants persist on the one deterministic profile. Overwriting the record
/// would leave the first session's ACE with nothing pointing at it — which is
/// the exact leak this slice exists to close, reintroduced one level down.
#[test]
fn a_second_create_over_one_root_unions_rather_than_replaces() {
    let root = TempDir::new().expect("a temp root");
    let first = TempDir::new().expect("the first run directory");
    let second = TempDir::new().expect("the second run directory");

    let one = Sandbox::create(root.path(), &[first.path().to_path_buf()])
        .expect("the first session's profile");
    let two = Sandbox::create(root.path(), &[second.path().to_path_buf()])
        .expect("the second session reuses the profile");
    assert_eq!(
        one.profile(),
        two.profile(),
        "one root must mean one profile, or this test is not about a union at all"
    );

    let lines = recorded(two.profile());
    assert!(
        holds(&lines, root.path()),
        "the root survives a second create, got {lines:?}"
    );
    assert!(
        holds(&lines, first.path()),
        "the first session's run directory must not be dropped, got {lines:?}"
    );
    assert!(
        holds(&lines, second.path()),
        "the second session's run directory must be added, got {lines:?}"
    );
    assert_eq!(lines.len(), 3, "no path is recorded twice, got {lines:?}");
}

/// `grants()` must report the **directory's** state, not the record's claim
/// about it, which is why every assertion here about `Granted` is worth making
/// on a sandbox that was only just created: if the state were copied off the
/// record it would read `Granted` even where an `icacls` had already removed
/// the ACE, and an operator would prune something that was not there.
#[test]
fn a_created_profile_is_listed_with_its_directories_and_their_live_state() {
    let root = TempDir::new().expect("a temp root");
    let toolbin = TempDir::new().expect("a temp run directory");

    let sandbox = Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
        .expect("the profile and both grants");

    let listed = skein_sandbox::grants().expect("the profiles on this machine are listable");
    let mine = listed
        .iter()
        .find(|grant| grant.profile == sandbox.profile())
        .unwrap_or_else(|| panic!("the profile just created must be listed, got {listed:?}"));

    assert_eq!(
        mine.sid,
        sandbox.string_sid(),
        "the listed identity must be the one the sandbox launches under"
    );

    let dirs = mine
        .dirs
        .as_ref()
        .expect("a profile created by this slice carries a record");
    let root_entry = dirs
        .iter()
        .find(|dir| holds(std::slice::from_ref(&dir.path), root.path()))
        .unwrap_or_else(|| panic!("the fs-root must be listed, got {dirs:?}"));
    assert_eq!(root_entry.kind, skein_sandbox::GrantKind::Root);
    assert_eq!(root_entry.state, skein_sandbox::GrantState::Granted);

    let run_entry = dirs
        .iter()
        .find(|dir| holds(std::slice::from_ref(&dir.path), toolbin.path()))
        .unwrap_or_else(|| panic!("the run directory must be listed, got {dirs:?}"));
    assert_eq!(run_entry.kind, skein_sandbox::GrantKind::RunDir);
    assert_eq!(run_entry.state, skein_sandbox::GrantState::Granted);
}

/// The shape of every profile made before this slice — a thousand of them on
/// this machine. Listing them with no directories is the whole point: an
/// operator who cannot see them cannot remove them.
#[test]
fn a_profile_with_no_record_is_listed_with_no_directories() {
    let root = TempDir::new().expect("a temp root");
    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");

    std::fs::remove_file(record_of(sandbox.profile())).expect("the record is removable");

    let listed = skein_sandbox::grants().expect("the profiles on this machine are listable");
    let mine = listed
        .iter()
        .find(|grant| grant.profile == sandbox.profile())
        .unwrap_or_else(|| panic!("a recordless profile must still be listed, got {listed:?}"));
    assert_eq!(
        mine.dirs, None,
        "a profile with no record has no known directories, not an empty list of them"
    );
}

/// A machine-wide assertion, and not a formality: `%LOCALAPPDATA%\Packages`
/// holds hundreds of unrelated Store packages, and a scan that reported them
/// would be offering an operator a `prune` on `Microsoft.WindowsCalculator`.
#[test]
fn nothing_but_a_skein_hash_name_is_listed() {
    let listed = skein_sandbox::grants().expect("the profiles on this machine are listable");
    assert!(
        !listed.is_empty(),
        "this machine has created profiles, so an empty listing means the scan found nothing"
    );
    for grant in &listed {
        let hash = grant
            .profile
            .strip_prefix("skein-")
            .unwrap_or_else(|| panic!("{} is not a skein profile", grant.profile));
        assert!(
            hash.len() == 16
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{} is not skein- plus 16 lowercase hex characters",
            grant.profile
        );
    }
}
