//! Reading a directory's DACL back off its own security descriptor.
//!
//! Shared by `profile.rs` and `prune.rs` — one copy of a `GetAce` loop whose
//! normalisation rule is subtle enough that two copies would be a hazard. It is
//! a `tests/dacl/mod.rs` directory module rather than a `tests/dacl.rs` file so
//! cargo treats it as a shared module instead of a test binary of its own.
//!
//! Every read here goes through the **real** `GetNamedSecurityInfoW` rather than
//! trusting what `Sandbox::create` or `prune` says it wrote: the grant is the
//! load-bearing containment mechanism, so its ground truth has to be the
//! object's own security descriptor.

use std::ffi::c_void;
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
    FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
};

pub fn wide(text: &str) -> Vec<u16> {
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
pub fn allow_aces(dir: &std::path::Path) -> Vec<(String, u32)> {
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
pub const FILE_MAPPING: GENERIC_MAPPING = GENERIC_MAPPING {
    GenericRead: FILE_GENERIC_READ.0,
    GenericWrite: FILE_GENERIC_WRITE.0,
    GenericExecute: FILE_GENERIC_EXECUTE.0,
    GenericAll: FILE_ALL_ACCESS.0,
};

pub fn granted_sids(dir: &std::path::Path) -> Vec<String> {
    allow_aces(dir).into_iter().map(|(sid, _)| sid).collect()
}

/// Every normalised mask this directory's DACL grants `sid`, and nothing it
/// grants anyone else.
pub fn granted_masks(dir: &std::path::Path, sid: &str) -> Vec<u32> {
    allow_aces(dir)
        .into_iter()
        .filter(|(trustee, _)| trustee == sid)
        .map(|(_, mask)| mask)
        .collect()
}
