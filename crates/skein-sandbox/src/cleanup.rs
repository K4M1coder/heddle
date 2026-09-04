//! Finding the AppContainer profiles Skein made, and taking them back.
//!
//! Discovery is a directory scan, because there is nothing else to use:
//! `Win32::Security::Isolation` exposes eleven functions and not one of them
//! enumerates profiles. The package folder Windows creates under
//! `%LOCALAPPDATA%\Packages` is the only machine-wide artifact a profile leaves
//! that names itself, and `launch.rs` already rests on that layout being where
//! Windows puts it.
//!
//! Nothing here consults the record to decide what it is *allowed* to touch.
//! The record only ever says where to look; the name gate
//! ([`profile::is_skein_profile`]) says what may be acted on, and it is the
//! same gate on both sides of the module.

use crate::profile::{is_skein_profile, string_sid, wide, NAME_HASH_BYTES};
use crate::{record, win32_path, Grant, GrantKind, GrantState, GrantedDir, Pruned};
use std::ffi::c_void;
use std::path::Path;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW,
    EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, REVOKE_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID,
    TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
};
use windows::Win32::Security::Isolation::{
    DeleteAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows::Win32::Security::{
    FreeSid, GetAce, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE,
    PSECURITY_DESCRIPTOR, PSID,
};

pub(crate) fn grants() -> Result<Vec<Grant>, String> {
    let packages = record::packages_dir()?;
    let entries = std::fs::read_dir(&packages).map_err(|e| {
        format!(
            "{}: the app container profiles are unlistable: {e}",
            packages.display()
        )
    })?;

    let mut found = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("{}: one entry is unreadable: {e}", packages.display()))?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| is_skein_profile(name)) else {
            continue;
        };
        found.push(describe(name)?);
    }
    // Sorted so two runs of `skein sandbox list` over an unchanged machine print
    // the same thing; `read_dir` order is the filesystem's, not an ordering.
    found.sort_by(|one, other| one.profile.cmp(&other.profile));
    Ok(found)
}

fn describe(profile: &str) -> Result<Grant, String> {
    let identity = AppContainerSid::derive(profile)?;
    let sid = identity.text()?;

    let dirs = match record::read(profile)? {
        None => None,
        Some(paths) => {
            let mut listed = Vec::with_capacity(paths.len());
            for (index, path) in paths.iter().enumerate() {
                listed.push(GrantedDir {
                    state: state_of(path, identity.0)?,
                    // The record's first line is the fs-root, because that is
                    // the order `Sandbox::create` writes it in. One fact, stored
                    // once.
                    kind: if index == 0 {
                        GrantKind::Root
                    } else {
                        GrantKind::RunDir
                    },
                    path: path.clone(),
                });
            }
            Some(listed)
        }
    };

    Ok(Grant {
        profile: profile.to_string(),
        sid,
        dirs,
    })
}

pub(crate) fn prune(profile: &str) -> Result<Pruned, String> {
    if !is_skein_profile(profile) {
        // The count comes off the constant the name is minted from, so the
        // sentence an operator is shown cannot describe a different shape than
        // the gate above enforces.
        return Err(format!(
            "{profile} is not a profile skein could have created: the name must be skein- followed by {} lowercase hexadecimal characters",
            NAME_HASH_BYTES * 2
        ));
    }

    let identity = AppContainerSid::derive(profile)?;
    let recorded = record::read(profile)?;
    let mut pruned = Pruned {
        profile: profile.to_string(),
        revoked: Vec::new(),
        clear: Vec::new(),
        missing: Vec::new(),
        unrecorded: recorded.is_none(),
    };

    // The ACEs first and the profile last. `DeleteAppContainerProfile` takes the
    // package folder — and the record inside it — with the profile, so failing
    // half way through the reverse order would leave every un-revoked ACE with
    // nothing left on the machine able to say where it is.
    for dir in recorded.unwrap_or_default() {
        match state_of(&dir, identity.0)? {
            GrantState::Missing => pruned.missing.push(dir),
            GrantState::Clear => pruned.clear.push(dir),
            GrantState::Granted => {
                revoke(&dir, identity.0)?;
                pruned.revoked.push(dir);
            }
        }
    }

    let wide_name = wide(profile);
    unsafe { DeleteAppContainerProfile(PCWSTR(wide_name.as_ptr())) }
        .map_err(|e| format!("the app container profile {profile} is not removable: {e}"))?;

    Ok(pruned)
}

/// Removes every ACE naming `sid` from `dir`'s DACL, and can remove no other.
///
/// One `EXPLICIT_ACCESS_W` whose mode is `REVOKE_ACCESS` and whose trustee is
/// the single `PSID` derived from a `skein-<hash>` name. The access mask and the
/// inheritance flags are `0` because `REVOKE_ACCESS` reads neither: it removes
/// *all* of that trustee's entries, which is what a directory grant needs, since
/// an inheritable generic mask splits into two ACEs on the way in.
///
/// # Safety
/// `sid` must be a valid `PSID` for the duration of the call.
fn revoke(dir: &Path, sid: PSID) -> Result<(), String> {
    let name = wide(&win32_path(dir));
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: 0,
        grfAccessMode: REVOKE_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
            // `ptstrName` is typed `PWSTR` even when the form is
            // `TRUSTEE_IS_SID`; casting the `PSID` into it is the documented
            // Win32 idiom, and the same one `profile::grant` writes.
            ptstrName: PWSTR(sid.0 as *mut u16),
        },
    };

    with_dacl(dir, |existing| {
        let mut stripped: *mut ACL = std::ptr::null_mut();
        let built = unsafe { SetEntriesInAclW(Some(&[entry]), Some(existing), &mut stripped) }
            .ok()
            .map_err(|e| {
                format!(
                    "{}: the permissions without this identity do not build: {e}",
                    dir.display()
                )
            });
        let written = built.and_then(|()| {
            unsafe {
                SetNamedSecurityInfoW(
                    PCWSTR(name.as_ptr()),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    None,
                    None,
                    Some(stripped),
                    None,
                )
            }
            .ok()
            .map_err(|e| format!("{}: {}", dir.display(), not_writable(e)))
        });
        if !stripped.is_null() {
            unsafe { LocalFree(Some(HLOCAL(stripped as *mut c_void))) };
        }
        written
    })
}

/// The way out of a directory whose permissions this user cannot rewrite,
/// which `profile::create` already meets on the way in.
///
/// Reported rather than swallowed, and the profile is then **not** deleted: the
/// record survives, so an elevated retry can finish exactly the work this run
/// could not.
fn not_writable(reason: windows::core::Error) -> String {
    format!(
        "its permissions are not writable: {reason}; a directory's permissions are writable only          by its owner or an administrator, so run skein elevated once or name a directory you own"
    )
}

/// A derived `PSID` that is freed on every path out, including the error ones.
///
/// `DeriveAppContainerSidFromAppContainerName` only *computes* the SID from the
/// name — it creates nothing — so calling it while listing mints no profile.
struct AppContainerSid(PSID);

impl AppContainerSid {
    fn derive(profile: &str) -> Result<AppContainerSid, String> {
        let wide_name = wide(profile);
        unsafe { DeriveAppContainerSidFromAppContainerName(PCWSTR(wide_name.as_ptr())) }
            .map(AppContainerSid)
            .map_err(|e| {
                format!("the identity of the app container profile {profile} is underivable: {e}")
            })
    }

    fn text(&self) -> Result<String, String> {
        unsafe { string_sid(self.0) }
    }
}

impl Drop for AppContainerSid {
    fn drop(&mut self) {
        unsafe { FreeSid(self.0) };
    }
}

/// What `dir`'s **own** security descriptor says about `sid`, right now.
///
/// # Safety
/// `sid` must be a valid `PSID` for the duration of the call.
fn state_of(dir: &Path, sid: PSID) -> Result<GrantState, String> {
    match dir.try_exists() {
        Ok(false) => return Ok(GrantState::Missing),
        Ok(true) => {}
        Err(e) => return Err(format!("{}: it cannot be looked up: {e}", dir.display())),
    }

    let wanted = unsafe { string_sid(sid) }?;
    let present = with_dacl(dir, |dacl| Ok(trustees(dacl).contains(&wanted)))?;
    Ok(if present {
        GrantState::Granted
    } else {
        GrantState::Clear
    })
}

/// Reads `dir`'s DACL, hands it to `read`, and frees the descriptor on every
/// path out.
///
/// One place holds the `GetNamedSecurityInfoW`/`LocalFree` pair, so neither
/// caller can leak it and neither can forget that the borrowed `*mut ACL` dies
/// with the descriptor.
fn with_dacl<T>(
    dir: &Path,
    read: impl FnOnce(*const ACL) -> Result<T, String>,
) -> Result<T, String> {
    let name = wide(&win32_path(dir));
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let opened = unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(name.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut descriptor,
        )
    }
    .ok()
    .map_err(|e| format!("{}: its permissions are unreadable: {e}", dir.display()));

    let outcome = opened.and_then(|()| {
        if dacl.is_null() {
            // A null DACL grants everyone everything and names no trustee, so
            // there is nothing of this profile's in it to find or remove.
            return Err(format!(
                "{}: it carries no discretionary access list at all",
                dir.display()
            ));
        }
        read(dacl)
    });
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    outcome
}

/// Every trustee the DACL names, as a string SID.
///
/// An ACE whose SID does not render is skipped rather than failing the read.
/// Precision here is not a safety boundary in either direction: a false positive
/// only ever leads to a `REVOKE_ACCESS` naming this profile's own SID, and a
/// false negative reports `clear` and writes nothing.
///
/// # Safety
/// `dacl` must point at a valid ACL that outlives the call.
fn trustees(dacl: *const ACL) -> Vec<String> {
    let mut found = Vec::new();
    unsafe {
        for index in 0..(*dacl).AceCount as u32 {
            let mut ace: *mut c_void = std::ptr::null_mut();
            if GetAce(dacl, index, &mut ace).is_err() {
                continue;
            }
            // `SidStart` is the first `u32` of the SID, laid out inline at the
            // end of the ACE, so its address *is* the `PSID`.
            let allowed = ace as *const ACCESS_ALLOWED_ACE;
            let sid = PSID(std::ptr::addr_of!((*allowed).SidStart) as *mut c_void);
            let mut text = PWSTR::null();
            if ConvertSidToStringSidW(sid, &mut text).is_ok() {
                if let Ok(rendered) = text.to_string() {
                    found.push(rendered);
                }
                LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
            }
        }
    }
    found
}
