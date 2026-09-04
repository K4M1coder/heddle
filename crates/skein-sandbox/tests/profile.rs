//! The AppContainer profile and the ACL grant (spec 019 SC-001).
//!
//! `#![cfg(windows)]` on the file rather than on each test: there is no
//! `Sandbox` to make on the other two platforms, so the whole file has nothing
//! to say there. The absence gates that *do* run there live in
//! `skein-connectors`' `tests/connector.rs`, where the catalogue is.
#![cfg(windows)]

mod dacl;
mod guard;

use dacl::{allow_aces, granted_sids};
use skein_sandbox::Sandbox;
use tempfile::TempDir;
use windows::Win32::Storage::FileSystem::{
    FILE_APPEND_DATA, FILE_EXECUTE, FILE_READ_DATA, FILE_WRITE_DATA, WRITE_DAC,
};

/// Every normalised mask this directory's DACL grants `sid`, and nothing it
/// grants anyone else.
///
/// Only this file compares masks — `prune.rs` asks whether a trustee is named
/// at all — so it stays here rather than in the shared `dacl` module.
fn granted_masks(dir: &std::path::Path, sid: &str) -> Vec<u32> {
    allow_aces(dir)
        .into_iter()
        .filter(|(trustee, _)| trustee == sid)
        .map(|(_, mask)| mask)
        .collect()
}

#[test]
fn a_sandbox_derives_an_appcontainer_sid_and_grants_it_the_root() {
    let dir = TempDir::new().expect("a temp dir");

    let sandbox =
        Sandbox::create(dir.path(), &[]).expect("the profile is created and the root granted");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    // S-1-15-2-* is the AppContainer SID authority, and nothing else has it.
    // Asserting the prefix rather than the whole string is the point: the hash
    // suffix is machine- and path-derived and pinning it would pin the fixture.
    assert!(
        sandbox.string_sid().starts_with("S-1-15-2-"),
        "an AppContainer SID must carry the package authority, got {}",
        sandbox.string_sid()
    );

    // The grant, read off the directory's own security descriptor. This is the
    // mechanism the whole containment claim rests on — `FsRoot` is a path check
    // inside this process and a child never passes through it.
    assert!(
        granted_sids(dir.path()).contains(&sandbox.string_sid().to_string()),
        "the root's DACL must name the AppContainer SID, got {:?}",
        granted_sids(dir.path())
    );
}

#[test]
fn the_same_root_reuses_one_profile_and_two_roots_do_not() {
    let one = TempDir::new().expect("a temp dir");
    let other = TempDir::new().expect("a second temp dir");

    let first = Sandbox::create(one.path(), &[]).expect("the first profile");
    let _pruned_first = guard::PrunedOnDrop::of(&first);
    // The second call over the same root meets
    // `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)` and must fall through to
    // deriving the SID from the name rather than failing. Without that, a
    // second ACP session over one workspace could not start.
    let again = Sandbox::create(one.path(), &[]).expect("the same profile is reused, not refused");
    let _pruned_again = guard::PrunedOnDrop::of(&again);
    let elsewhere =
        Sandbox::create(other.path(), &[]).expect("a different root gets its own profile");
    let _pruned_elsewhere = guard::PrunedOnDrop::of(&elsewhere);

    assert_eq!(
        first.string_sid(),
        again.string_sid(),
        "one root must mean one identity, or every run would leave a new profile behind"
    );
    assert_ne!(
        first.string_sid(),
        elsewhere.string_sid(),
        "two roots must not share an identity, or a grant on one would reach the other"
    );
}

/// The narrower grant, measured against the object's own descriptor rather than
/// asserted about intent (spec 020 SC-001).
///
/// A run directory is for *reaching an executable*, and a toolchain directory
/// does not need to be writable by the sandboxed child — a child that could
/// overwrite `cargo.exe` would leave a side effect outliving the run. The
/// fs-root is the opposite case and keeps its full access, so this test pins
/// both halves: it would pass just as loudly if the two masks were swapped.
///
/// The mask itself is not invented here. `%SystemRoot%\System32` — the
/// directory every AppContainer on this machine already executes from — carries
/// `ALL APPLICATION PACKAGES` at exactly `FILE_GENERIC_READ |
/// FILE_GENERIC_EXECUTE`, measured with `Get-Acl`. This is Windows' own answer
/// to the question, copied.
#[test]
fn a_run_dir_is_granted_read_and_execute_and_the_root_is_not() {
    let root = TempDir::new().expect("a temp root");
    let toolbin = TempDir::new().expect("a temp run directory");

    let sandbox = Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
        .expect("the profile, the root's grant and the run directory's");
    let _pruned_sandbox = guard::PrunedOnDrop::of(&sandbox);

    let sid = sandbox.string_sid();
    let run_masks = granted_masks(toolbin.path(), sid);
    assert!(
        !run_masks.is_empty(),
        "the run directory's DACL must name the AppContainer SID at all, got {:?}",
        allow_aces(toolbin.path())
    );
    for mask in &run_masks {
        assert_eq!(
            mask & (FILE_READ_DATA.0 | FILE_EXECUTE.0),
            FILE_READ_DATA.0 | FILE_EXECUTE.0,
            "the image loader must be able to read and execute the PE file: {mask:#x}"
        );
        // Three separate bits, because they fail differently: `FILE_WRITE_DATA`
        // would let the child overwrite `cargo.exe`, `FILE_APPEND_DATA` would
        // let it corrupt one, and `WRITE_DAC` would let it widen its own grant.
        assert_eq!(
            mask & (FILE_WRITE_DATA.0 | FILE_APPEND_DATA.0 | WRITE_DAC.0),
            0,
            "a run directory must not become writable by the thing it launches: {mask:#x}"
        );
    }

    // The control, in the same test and for `escape.rs`'s recorded reason: if
    // the root were narrowed too, the assertion above would pass while the
    // whole grant did nothing.
    let root_masks = granted_masks(root.path(), sid);
    assert!(
        root_masks
            .iter()
            .any(|mask| mask & FILE_WRITE_DATA.0 == FILE_WRITE_DATA.0),
        "the fs-root keeps its full access — an agent's workspace is writable: {root_masks:?}"
    );
}

/// The grant **adds**; it does not replace. This is what says so.
///
/// `grant` reads the directory's existing DACL and merges through
/// `SetEntriesInAclW`. Dropping that read and writing an ACL holding only the
/// new entry is a shorter function that passes every other test in this file —
/// each one filters the DACL down to its own AppContainer SID and never looks
/// at who else is on it.
///
/// The pre-existing trustee here is **another sandbox's**, which is not a
/// contrived fixture: it is one workspace serving as a second session's run
/// directory, and it is the case where a replace is worst — the surviving
/// session keeps running against a directory it silently no longer reaches.
///
/// It has to be an *explicit* ACE to prove anything, and that is the whole
/// reason this test builds one rather than trusting a bare `TempDir`. A fresh
/// temp directory carries nothing but ACEs inherited from `%TEMP%`, and
/// `SetNamedSecurityInfoW` without `PROTECTED_DACL_SECURITY_INFORMATION`
/// rewrites only the explicit half — so inherited entries survive a replace too
/// and could not tell the two implementations apart.
///
/// Compared as a multiset: an ACE that appeared twice before and once after is
/// a loss too, and the inheritance split `dacl::allow_aces` documents means
/// duplicate-looking pairs are normal here.
#[test]
fn the_grant_leaves_every_trustee_the_directory_already_had() {
    let shared = TempDir::new().expect("a temp dir two sessions both want");
    let elsewhere = TempDir::new().expect("the first session's own root");

    let first = Sandbox::create(elsewhere.path(), &[shared.path().to_path_buf()])
        .expect("the first session's profile, and its grant on the shared directory");
    let _pruned_first = guard::PrunedOnDrop::of(&first);

    let before = allow_aces(shared.path());
    assert!(
        before.iter().any(|(trustee, _)| trustee == first.string_sid()),
        "the first session must hold an explicit ACE here, or this test proves nothing:          {before:?}"
    );

    let second =
        Sandbox::create(shared.path(), &[]).expect("the second session takes it as a root");
    let _pruned_second = guard::PrunedOnDrop::of(&second);

    let mut after = allow_aces(shared.path());
    for pair in &before {
        let found = after
            .iter()
            .position(|candidate| candidate == pair)
            .unwrap_or_else(|| panic!("the grant must not evict {pair:?}; after {after:?}"));
        after.remove(found);
    }
    assert!(
        after
            .iter()
            .any(|(trustee, _)| trustee == second.string_sid()),
        "and it must add the second identity on top of them, got {:?}",
        allow_aces(shared.path())
    );
}
