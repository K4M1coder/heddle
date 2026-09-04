//! `prune` removes what Skein granted, and proves it removes nothing else
//! (spec 024 SC-006…SC-009).
//!
//! Every assertion about an ACE here is a real `GetNamedSecurityInfoW` read-back
//! through `tests/dacl`, never a claim about what the call intended. That is the
//! same discipline `profile.rs` applies to the grant, applied to its removal —
//! and it matters more here, because the failure mode of a wrong `prune` is a
//! permission an operator never gets back.
#![cfg(windows)]

mod dacl;

use dacl::{allow_aces, granted_sids};
use skein_sandbox::Sandbox;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, GENERIC_READ, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertStringSidToSidW, GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW,
    EXPLICIT_ACCESS_W, GRANT_ACCESS, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, TRUSTEE_IS_SID,
    TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SUB_CONTAINERS_AND_OBJECTS_INHERIT,
};

/// `ALL APPLICATION PACKAGES` — well known, present on `%SystemRoot%\System32`
/// on every Windows machine, and emphatically not a `skein-` profile. The
/// trustee `prune` must leave alone.
const ALL_APPLICATION_PACKAGES: &str = "S-1-15-2-1";

fn package_folder(profile: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA is set on Windows"))
        .join("Packages")
        .join(profile)
}

fn listed(profile: &str) -> bool {
    skein_sandbox::grants()
        .expect("the profiles on this machine are listable")
        .iter()
        .any(|grant| grant.profile == profile)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Puts an ACE for a **third party** on `dir`, through the same Win32 path
/// `profile::grant` uses.
///
/// Written by the test rather than by the code under test on purpose: the claim
/// is that `prune` leaves alone an ACE it has no idea exists.
fn grant_to(dir: &Path, string_sid: &str) {
    let name = wide(&dir.to_string_lossy());
    let sid_text = wide(string_sid);
    unsafe {
        let mut sid = windows::Win32::Security::PSID::default();
        ConvertStringSidToSidW(PCWSTR(sid_text.as_ptr()), &mut sid)
            .expect("a well-known SID parses");

        let mut existing: *mut ACL = std::ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        GetNamedSecurityInfoW(
            PCWSTR(name.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut existing),
            None,
            &mut descriptor,
        )
        .ok()
        .expect("the fixture directory's DACL is readable");

        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: GENERIC_READ.0,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: PWSTR(sid.0 as *mut u16),
            },
        };
        let mut merged: *mut ACL = std::ptr::null_mut();
        SetEntriesInAclW(Some(&[entry]), Some(existing), &mut merged)
            .ok()
            .expect("the third party's permissions build");
        SetNamedSecurityInfoW(
            PCWSTR(name.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(merged),
            None,
        )
        .ok()
        .expect("the fixture directory's DACL is writable");

        LocalFree(Some(HLOCAL(merged as *mut c_void)));
        LocalFree(Some(HLOCAL(descriptor.0)));
        // `ConvertStringSidToSidW` allocates with `LocalAlloc`, so this is
        // `LocalFree` and **not** `FreeSid` — the same distinction `launch.rs`
        // records for its own use of the function.
        LocalFree(Some(HLOCAL(sid.0)));
    }
}

/// The acceptance test: a real profile, real ACEs, a real removal, and every
/// claim read back off the object rather than off the return value.
#[test]
fn a_real_grant_is_listed_then_pruned_and_the_ace_is_gone_from_the_dacl() {
    let root = TempDir::new().expect("a temp root");
    let toolbin = TempDir::new().expect("a temp run directory");

    let sandbox = Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
        .expect("the profile and both grants");
    let profile = sandbox.profile().to_string();
    let sid = sandbox.string_sid().to_string();

    assert!(
        granted_sids(root.path()).contains(&sid),
        "the root must carry the ACE before it can be shown to lose it"
    );
    assert!(
        granted_sids(toolbin.path()).contains(&sid),
        "the run directory must carry the ACE before it can be shown to lose it"
    );
    assert!(
        listed(&profile),
        "the profile must be listed before pruning"
    );

    let pruned = skein_sandbox::prune(&profile).expect("a profile skein created is prunable");
    assert!(!pruned.unrecorded, "this profile carries a record");
    assert_eq!(
        pruned.revoked.len(),
        2,
        "both directories were granted, so both are revoked: {pruned:?}"
    );

    assert!(
        !granted_sids(root.path()).contains(&sid),
        "the root's DACL must no longer name the identity, got {:?}",
        allow_aces(root.path())
    );
    assert!(
        !granted_sids(toolbin.path()).contains(&sid),
        "the run directory's DACL must no longer name the identity, got {:?}",
        allow_aces(toolbin.path())
    );
    // The record lives in this folder, so its removal is what couples the
    // record's lifetime to the profile's — the whole reason the record is filed
    // here rather than somewhere skein chose.
    assert!(
        !package_folder(&profile).exists(),
        "DeleteAppContainerProfile must remove the package folder, or the record outlives it"
    );
    assert!(!listed(&profile), "a pruned profile is no longer listed");
}

/// The refusal test, and it is deliberately made on a directory `prune` **does**
/// rewrite. Proving that a DACL it never opened is unchanged would prove
/// nothing; proving that the one it rewrote lost exactly one trustee is the
/// claim `REVOKE_ACCESS` on a single `TRUSTEE_IS_SID` is supposed to support.
#[test]
fn pruning_leaves_every_ace_it_did_not_write() {
    let root = TempDir::new().expect("a temp root");
    let toolbin = TempDir::new().expect("a temp run directory");

    let before = allow_aces(root.path());
    grant_to(toolbin.path(), ALL_APPLICATION_PACKAGES);

    let sandbox = Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
        .expect("the profile and both grants");
    let profile = sandbox.profile().to_string();
    skein_sandbox::prune(&profile).expect("a profile skein created is prunable");

    assert_eq!(
        allow_aces(root.path()),
        before,
        "the root's access list must be exactly what it was before skein touched it"
    );
    assert!(
        granted_sids(toolbin.path()).contains(&ALL_APPLICATION_PACKAGES.to_string()),
        "a trustee skein did not write must survive a directory prune rewrote, got {:?}",
        allow_aces(toolbin.path())
    );
}

/// The name gate, which runs before any Win32 call at all. The calculator's
/// package folder still existing afterwards is the control: if the gate leaked,
/// `DeleteAppContainerProfile` would have been reached with a name Skein never
/// created.
#[test]
fn prune_refuses_a_name_it_could_not_have_created() {
    let calculator = "Microsoft.WindowsCalculator_8wekyb3d8bbwe";
    let existed = package_folder(calculator).exists();

    for name in [
        calculator,
        "skein-notahexstring",
        "skein-ABCDEF0123456789",
        "skein-0123",
    ] {
        let refused =
            skein_sandbox::prune(name).expect_err("only a skein profile name is prunable");
        assert!(
            refused.contains("16 lowercase hexadecimal"),
            "the refusal must name the shape it requires, got {refused}"
        );
    }

    assert_eq!(
        package_folder(calculator).exists(),
        existed,
        "a refused prune must leave the machine exactly as it found it"
    );
}

/// The shape of every profile made before this slice. Deleting it while saying
/// plainly what was *not* done beats both alternatives: refusing would leave a
/// thousand profiles permanently unremovable, and guessing at directories would
/// be a destructive command acting on a hash it cannot invert.
#[test]
fn an_unrecorded_profile_is_deleted_and_says_its_aces_are_unknown() {
    let root = TempDir::new().expect("a temp root");
    let sandbox = Sandbox::create(root.path(), &[]).expect("the profile and the grant");
    let profile = sandbox.profile().to_string();
    let sid = sandbox.string_sid().to_string();

    std::fs::remove_file(package_folder(&profile).join("skein-grants"))
        .expect("the record is removable");

    let pruned = skein_sandbox::prune(&profile).expect("a recordless profile is still prunable");
    assert!(
        pruned.unrecorded,
        "the caller must be told the directories were unknown: {pruned:?}"
    );
    assert!(
        pruned.revoked.is_empty() && pruned.clear.is_empty() && pruned.missing.is_empty(),
        "nothing may be reported about directories that were never known: {pruned:?}"
    );
    assert!(
        !package_folder(&profile).exists(),
        "the profile itself is removable without its record"
    );
    assert!(
        granted_sids(root.path()).contains(&sid),
        "the ACE survives, and the honest report above is the only thing standing in for it"
    );
}
