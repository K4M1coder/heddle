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
//! The record only ever says where to look; the name gate ([`is_skein_profile`])
//! says what may be acted on, and it is the same gate on both sides of the
//! module.

use crate::profile::{string_sid, wide};
use crate::{record, win32_path, Grant, GrantKind, GrantState, GrantedDir, Pruned};
use std::ffi::c_void;
use std::path::Path;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows::Win32::Security::Isolation::DeriveAppContainerSidFromAppContainerName;
use windows::Win32::Security::{
    FreeSid, GetAce, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};

/// The length of `sha256`'s first `NAME_HASH_BYTES` rendered as hex, which is
/// what `profile::profile_name` appends.
const NAME_HASH_CHARS: usize = 16;

/// The whole ownership claim, in one predicate.
///
/// `skein-` plus 16 lowercase hex characters is a namespace nothing else on a
/// Windows machine produces, and it is the *only* thing either public function
/// will act on. Nothing here consults the record, which is why a tampered
/// record could not widen what [`prune`] is able to delete.
fn is_skein_profile(name: &str) -> bool {
    name.strip_prefix("skein-").is_some_and(|hash| {
        hash.len() == NAME_HASH_CHARS
            && hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

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

pub(crate) fn prune(_profile: &str) -> Result<Pruned, String> {
    todo!("T5")
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
