//! The AppContainer identity for one directory, and the ACE that lets it in.
//!
//! Two Win32 acts, in this order and for this reason: derive a **deterministic**
//! identity from the root's path, then grant that identity access to the root.
//! Deterministic because a fresh profile per run would leave one behind per
//! run, and because two concurrent ACP sessions over one workspace must agree
//! on who they are.

use crate::{win32_path, Sandbox};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::path::Path;
use windows::core::{HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW,
    EXPLICIT_ACCESS_W, GRANT_ACCESS, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, TRUSTEE_IS_SID,
    TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
};
use windows::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows::Win32::Security::{
    FreeSid, ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT,
};

/// An AppContainer name may be 64 characters; this is 22, so the derivation has
/// room and never needs a length check. Eight bytes of SHA-256 is far more
/// collision margin than a per-machine set of workspace directories needs.
const NAME_HASH_BYTES: usize = 8;

pub(crate) fn create(root: &Path) -> Result<Sandbox, String> {
    let name = profile_name(root);
    let wide_name = wide(&name);

    // `pcapabilities: None` **is** the no-network decision, at the profile
    // level: the profile is created with zero capability SIDs, so
    // `internetClient` (S-1-15-3-1), `internetClientServer` (S-1-15-3-2) and
    // `privateNetworkClientServer` (S-1-15-3-3) are all absent, and the
    // Windows Filtering Platform has no permit filter whose condition this
    // identity satisfies.
    let sid = unsafe {
        match CreateAppContainerProfile(
            PCWSTR(wide_name.as_ptr()),
            PCWSTR(wide_name.as_ptr()),
            PCWSTR(wide_name.as_ptr()),
            None,
        ) {
            Ok(sid) => sid,
            // The profile from an earlier run over this same root. Deriving the
            // SID from the name is the documented way back to it; failing here
            // would mean a second session over one workspace could not start.
            Err(e) if e.code() == already_exists() => {
                DeriveAppContainerSidFromAppContainerName(PCWSTR(wide_name.as_ptr()))
                    .map_err(|e| format!("the app container profile {name} exists but its identity is unreadable: {e}"))?
            }
            Err(e) => {
                return Err(format!(
                    "the app container profile {name} could not be created: {e}"
                ))
            }
        }
    };

    // Both fallible steps happen while the `PSID` is live, and the `FreeSid`
    // below runs on every path out — including the error ones, which is why
    // neither uses `?` directly.
    let identity = unsafe { string_sid(sid).and_then(|text| grant(root, sid).map(|()| text)) };
    unsafe { FreeSid(sid) };

    Ok(Sandbox {
        root: root.to_path_buf(),
        sid: identity?,
    })
}

/// `HRESULT_FROM_WIN32(ERROR_ALREADY_EXISTS)`, which windows-rs 0.61 does not
/// generate as a constant.
fn already_exists() -> HRESULT {
    HRESULT::from_win32(windows::Win32::Foundation::ERROR_ALREADY_EXISTS.0)
}

/// `skein-` plus 16 hex characters of `sha256(root path)`.
///
/// Derived from the path the caller gives, which for every shipped call site is
/// already canonical — `FsRoot` canonicalizes once in its constructor — so two
/// spellings of one directory reach this with the same bytes.
fn profile_name(root: &Path) -> String {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let mut name = String::from("skein-");
    for byte in &digest[..NAME_HASH_BYTES] {
        name.push_str(&format!("{byte:02x}"));
    }
    name
}

/// # Safety
/// `sid` must be a valid `PSID` for the duration of the call.
unsafe fn string_sid(sid: PSID) -> Result<String, String> {
    let mut text = PWSTR::null();
    ConvertSidToStringSidW(sid, &mut text)
        .map_err(|e| format!("the app container identity does not render as a string SID: {e}"))?;
    let rendered = text.to_string();
    LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
    rendered.map_err(|e| format!("the app container identity is not valid UTF-16: {e}"))
}

/// Merges one inheritable full-access ACE for `sid` into `root`'s DACL.
///
/// **This is the containment mechanism.** A spawned process never passes
/// through `FsRoot::resolve`; what stops it writing outside the root is that no
/// directory outside the root carries an ACE naming this SID. A user-profile
/// subtree — where `TempDir` and most workspaces live — carries no
/// `ALL APPLICATION PACKAGES` ACE, so without this the child could not read its
/// own workspace at all.
///
/// Idempotent: re-granting the same trustee the same access under
/// `GRANT_ACCESS` merges rather than duplicating, which is what makes the
/// deterministic profile name worth having.
///
/// # Safety
/// `sid` must be a valid `PSID` for the duration of the call.
unsafe fn grant(root: &Path, sid: PSID) -> Result<(), String> {
    let name = wide(&win32_path(root));
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
    .map_err(|e| format!("{}: its permissions are unreadable: {e}", root.display()))?;

    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: windows::Win32::Foundation::GENERIC_ALL.0,
        grfAccessMode: GRANT_ACCESS,
        // Inheritable, so files and directories created inside the root after
        // this call are reachable too — otherwise the child could read the
        // workspace as it stood at launch and nothing it made itself.
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
            // `ptstrName` is typed `PWSTR` even when the form is
            // `TRUSTEE_IS_SID`; casting the `PSID` into it is the documented
            // Win32 idiom and the single most transposable field here.
            ptstrName: PWSTR(sid.0 as *mut u16),
        },
    };

    let mut merged: *mut ACL = std::ptr::null_mut();
    let entries = SetEntriesInAclW(Some(&[entry]), Some(existing), &mut merged)
        .ok()
        .map_err(|e| format!("{}: the new permissions do not build: {e}", root.display()));
    let written = entries.and_then(|()| {
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
        .map_err(|e| format!("{}: its permissions are not writable: {e}", root.display()))
    });

    if !merged.is_null() {
        LocalFree(Some(HLOCAL(merged as *mut c_void)));
    }
    LocalFree(Some(HLOCAL(descriptor.0)));
    written
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
