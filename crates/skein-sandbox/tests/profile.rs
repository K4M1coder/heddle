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
    GetAce, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Every SID named by an allow-ACE in the directory's DACL, in string form.
///
/// Read back through the **real** `GetNamedSecurityInfoW` rather than by
/// trusting what `Sandbox::create` says it wrote: the grant is the load-bearing
/// containment mechanism, so its ground truth has to be the object's own
/// security descriptor.
fn granted_sids(dir: &std::path::Path) -> Vec<String> {
    let name = wide(&dir.to_string_lossy());
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let mut sids = Vec::new();
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
                sids.push(text.to_string().expect("a SID renders as UTF-16"));
                let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
            }
        }
        let _ = LocalFree(Some(HLOCAL(descriptor.0)));
    }
    sids
}

#[test]
fn a_sandbox_derives_an_appcontainer_sid_and_grants_it_the_root() {
    let dir = TempDir::new().expect("a temp dir");

    let sandbox = Sandbox::create(dir.path()).expect("the profile is created and the root granted");

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

    let first = Sandbox::create(one.path()).expect("the first profile");
    // The second call over the same root meets
    // `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)` and must fall through to
    // deriving the SID from the name rather than failing. Without that, a
    // second ACP session over one workspace could not start.
    let again = Sandbox::create(one.path()).expect("the same profile is reused, not refused");
    let elsewhere = Sandbox::create(other.path()).expect("a different root gets its own profile");

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
