//! The platform gate on the cleanup surface (spec 024 SC-010).
//!
//! `#![cfg(not(windows))]` on the file rather than on each test, the mirror of
//! `tests/{profile,prune,record}.rs`'s whole-file `#![cfg(windows)]`: there is
//! no profile to find on the other two platforms, so this file is the only one
//! of the four with anything to say there.
//!
//! It runs on two of three CI legs and **cannot be executed on the Windows
//! machine this slice was written on**, which is the standing caveat slice 019
//! recorded for its own absence gates. What is verified here is that it
//! compiles for the Linux target and that the refusal it asserts is the one the
//! crate actually holds.
#![cfg(not(windows))]

/// Both halves of the surface refuse, and the refusal is a *reason* rather than
/// an empty listing.
///
/// An `Ok(vec![])` would be the tempting shape — nothing here, nothing to do —
/// and it is the wrong one: it is indistinguishable from a Windows machine that
/// has never run heddle, so an operator could not tell "this platform makes no
/// profiles" from "you have none". Fail clearly, never silently degrade.
#[test]
fn grants_and_prune_refuse_off_windows() {
    let unlistable = heddle_sandbox::grants().expect_err("no platform but Windows has profiles");
    let unprunable = heddle_sandbox::prune("heddle-0000000000000000")
        .expect_err("a well-formed name is still unprunable where nothing creates profiles");

    // The name is deliberately well formed, so this cannot pass by way of the
    // name gate: what refuses it is the platform, and the message has to say so.
    for refusal in [&unlistable, &unprunable] {
        assert!(
            refusal.contains("Windows-only"),
            "the refusal must name the platform, got {refusal}"
        );
        assert!(
            refusal.contains("list or prune"),
            "the refusal must be about cleanup rather than about launching, got {refusal}"
        );
    }
}
