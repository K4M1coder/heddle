//! The AppContainer profile and the ACL grant (spec 019 SC-001).
//!
//! `#![cfg(windows)]` on the file rather than on each test: there is no
//! `Sandbox` to make on the other two platforms, so the whole file has nothing
//! to say there. The absence gates that *do* run there live in
//! `skein-connectors`' `tests/connector.rs`, where the catalogue is.
#![cfg(windows)]

use skein_sandbox::Sandbox;
use std::ffi::c_void;
use tempfile::TempDir;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    GetAce, MapGenericMask, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, GENERIC_MAPPING,
    PSECURITY_DESCRIPTOR, PSID,
};
use windows::Win32::Storage::FileSystem::{
    FILE_ALL_ACCESS, FILE_APPEND_DATA, FILE_EXECUTE, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, FILE_READ_DATA, FILE_WRITE_DATA, WRITE_DAC,
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Every allow-ACE in the directory's DACL, as its trustee's string SID and
/// its access mask **normalised through `MapGenericMask`**.
///
/// Read back through the **real** `GetNamedSecurityInfoW` rather than by
/// trusting what `Sandbox::create` says it wrote: the grant is the load-bearing
/// containment mechanism, so its ground truth has to be the object's own
/// security descriptor.
///
/// The normalisation is not optional, and it was measured rather than assumed.
/// A generic mask written on a directory with `CONTAINER_INHERIT |
/// OBJECT_INHERIT` **splits into two ACEs**: writing `0xA0000000`
/// (`GENERIC_READ | GENERIC_EXECUTE`) reads back as `0x1200A9` with no inherit
/// flags *plus* `0xA0000000` flagged inherit-only. A test comparing against the
/// constant it wrote would be right about one of them and wrong about the
/// other. `MapGenericMask` with the file mapping resolves both to the same
/// specific rights, and is a no-op on a mask that is already specific.
fn allow_aces(dir: &std::path::Path) -> Vec<(String, u32)> {
    let name = wide(&dir.to_string_lossy());
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let mut aces = Vec::new();
    unsafe {
        let status = GetNamedSecurityInfoW(
            PCWSTR(name.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut descriptor,
        );
        assert!(status.is_ok(), "the DACL must be readable: {status:?}");
        assert!(!dacl.is_null(), "a directory must carry a DACL");
        for index in 0..(*dacl).AceCount as u32 {
            let mut ace: *mut c_void = std::ptr::null_mut();
            if GetAce(dacl, index, &mut ace).is_err() {
                continue;
            }
            let allowed = ace as *const ACCESS_ALLOWED_ACE;
            // `SidStart` is the first `u32` of the SID, laid out inline at the
            // end of the ACE, so its address *is* the `PSID`.
            let sid = PSID(std::ptr::addr_of!((*allowed).SidStart) as *mut c_void);
            let mut text = PWSTR::null();
            if ConvertSidToStringSidW(sid, &mut text).is_ok() {
                let mut mask = (*allowed).Mask;
                MapGenericMask(&mut mask, &FILE_MAPPING);
                aces.push((text.to_string().expect("a SID renders as UTF-16"), mask));
                let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
            }
        }
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    aces
}

/// The file `GENERIC_MAPPING`, which is what turns a `GENERIC_*` bit into the
/// specific rights it actually confers on a file object.
const FILE_MAPPING: GENERIC_MAPPING = GENERIC_MAPPING {
    GenericRead: FILE_GENERIC_READ.0,
    GenericWrite: FILE_GENERIC_WRITE.0,
    GenericExecute: FILE_GENERIC_EXECUTE.0,
    GenericAll: FILE_ALL_ACCESS.0,
};

fn granted_sids(dir: &std::path::Path) -> Vec<String> {
    allow_aces(dir).into_iter().map(|(sid, _)| sid).collect()
}

/// Every normalised mask this directory's DACL grants `sid`, and nothing it
/// grants anyone else.
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
    // The second call over the same root meets
    // `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)` and must fall through to
    // deriving the SID from the name rather than failing. Without that, a
    // second ACP session over one workspace could not start.
    let again = Sandbox::create(one.path(), &[]).expect("the same profile is reused, not refused");
    let elsewhere =
        Sandbox::create(other.path(), &[]).expect("a different root gets its own profile");

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
