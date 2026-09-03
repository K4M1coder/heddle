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
use windows::Win32::Foundation::{LocalFree, GENERIC_ALL, GENERIC_EXECUTE, GENERIC_READ, HLOCAL};
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

/// What a `--run-dir` gets, and it is Windows' own answer rather than a guess.
///
/// `%SystemRoot%\System32` and `C:\Program Files` — the directories every
/// AppContainer on a Windows machine already executes from — each carry
/// `ALL APPLICATION PACKAGES` at an effective `FILE_GENERIC_READ |
/// FILE_GENERIC_EXECUTE` plus an inherit-only `GENERIC_READ | GENERIC_EXECUTE`,
/// measured with `Get-Acl`. Writing this mask with
/// `SUB_CONTAINERS_AND_OBJECTS_INHERIT` reproduces that pair exactly.
///
/// Read is there because `FILE_GENERIC_EXECUTE` alone does not carry
/// `FILE_READ_DATA` and the image loader has to read the PE file. Write is
/// absent because a toolchain directory does not need to be writable by the
/// thing it launches, and a child that could overwrite `cargo.exe` would leave
/// a side effect outliving the run.
const RUN_DIR_ACCESS: u32 = GENERIC_READ.0 | GENERIC_EXECUTE.0;

pub(crate) fn create(root: &Path, run_dirs: &[std::path::PathBuf]) -> Result<Sandbox, String> {
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
            Err(e) if e.code() == already_exists() => DeriveAppContainerSidFromAppContainerName(
                PCWSTR(wide_name.as_ptr()),
            )
            .map_err(|e| {
                format!(
                    "the app container profile {name} exists but its identity is unreadable: {e}"
                )
            })?,
            Err(e) => {
                return Err(format!(
                    "the app container profile {name} could not be created: {e}"
                ))
            }
        }
    };

    // Every grant happens while the `PSID` is live, and the `FreeSid` below runs
    // on every path out — including the error ones, which is why none of these
    // uses `?` directly. The whole construction fails if any one grant does: a
    // sandbox that could not re-permission a directory the operator named must
    // be an exit code before a model is shown a tool, not a per-call surprise.
    let identity = unsafe {
        string_sid(sid).and_then(|text| {
            grant(root, sid, GENERIC_ALL.0)
                .and_then(|()| {
                    run_dirs
                        .iter()
                        .try_for_each(|dir| grant(dir, sid, RUN_DIR_ACCESS).map_err(unwritable))
                })
                .map(|()| text)
        })
    };
    unsafe { FreeSid(sid) };

    Ok(Sandbox {
        root: root.to_path_buf(),
        run_dirs: run_dirs.to_vec(),
        sid: identity?,
    })
}

/// The two ways out of a `--run-dir` this user cannot re-permission, appended
/// to the Win32 error that named the directory.
///
/// Reachable in practice, not theoretical: `C:\Program Files\nodejs` does not
/// inherit its parent's AppContainer ACEs, its DACL is protected and its owner
/// is `NT AUTHORITY\SYSTEM`, so naming it from a non-elevated shell fails with
/// `ERROR_ACCESS_DENIED`. An operator meeting that at startup needs to be told
/// what to do about it, not handed an error code.
fn unwritable(reason: String) -> String {
    format!(
        "{reason}; a directory's permissions are writable only by its owner or an \
         administrator, so run skein elevated once or name a directory you own"
    )
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

/// Merges one inheritable ACE for `sid`, carrying `access`, into `dir`'s DACL.
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
unsafe fn grant(dir: &Path, sid: PSID, access: u32) -> Result<(), String> {
    let name = wide(&win32_path(dir));
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
    .map_err(|e| format!("{}: its permissions are unreadable: {e}", dir.display()))?;

    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: access,
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
        .map_err(|e| format!("{}: the new permissions do not build: {e}", dir.display()));
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
        .map_err(|e| format!("{}: its permissions are not writable: {e}", dir.display()))
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
