//! Fixture hygiene: the profile a test created does not outlive the test.
//!
//! Every `Sandbox::create` over a `TempDir` mints an AppContainer profile that
//! nothing used to remove, so one `cargo test --workspace` left a measured 27
//! behind in `%LOCALAPPDATA%\Packages` — the leak this slice exists to close,
//! reproduced by the suite that proves the slice works.
//!
//! This is `cli_secret.rs`'s `TestRef` applied to a different piece of machine
//! state, and it is deliberately **not** product behaviour: `Sandbox` gains no
//! `Drop`. A profile is reused by root on purpose, and removing it when a
//! session ends would defeat that and would race two sessions over one
//! workspace.

use skein_sandbox::Sandbox;

pub struct PrunedOnDrop(String);

impl PrunedOnDrop {
    /// Declare it **after** the sandbox and after the `TempDir`s, so it drops
    /// before the sandbox and while the directories still exist: a prune of a
    /// directory already deleted reports `missing` and revokes nothing, which
    /// would still leak nothing but would stop proving that it revoked.
    pub fn of(sandbox: &Sandbox) -> PrunedOnDrop {
        PrunedOnDrop(sandbox.profile().to_string())
    }
}

impl Drop for PrunedOnDrop {
    fn drop(&mut self) {
        // Tolerant by design, for `TestRef`'s reason: a test that already pruned
        // is a normal case, and a cleanup that panicked would mask the real
        // failure it was cleaning up after.
        let _ = skein_sandbox::prune(&self.0);
    }
}
