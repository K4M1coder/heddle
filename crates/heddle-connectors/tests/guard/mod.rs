//! Fixture hygiene: the app container profile a test caused does not outlive
//! the test.
//!
//! `EmbeddedServer::with_run` mints one per `--fs-root` it is given, and until
//! this slice nothing removed them — one `cargo test --workspace` left a
//! measured 27 behind in `%LOCALAPPDATA%\Packages`. This is `cli_secret.rs`'s
//! `TestRef` applied to that state, and it is deliberately not product
//! behaviour: a profile is reused by root on purpose, and dropping it with the
//! server would defeat that and race two sessions over one workspace.
//!
//! Keyed on the **root directory** rather than on a profile name, because a
//! test here never holds the `Sandbox` — the server owns it privately. The
//! lookup is `heddle_sandbox::grants()`, the same operator-facing capability
//! `heddle sandbox list` renders.

use heddle_sandbox::GrantKind;
use std::path::{Path, PathBuf};

pub struct PrunedOnDrop(PathBuf);

impl PrunedOnDrop {
    /// Declare it **after** the `TempDir` and before the server, so it drops
    /// with the server already gone and the directory still there: a prune of a
    /// directory already deleted revokes nothing and reports `missing`.
    pub fn of_root(root: &Path) -> PrunedOnDrop {
        PrunedOnDrop(
            root.canonicalize()
                .expect("a fixture root exists when its guard is made"),
        )
    }
}

impl Drop for PrunedOnDrop {
    fn drop(&mut self) {
        // Tolerant throughout, for `TestRef`'s reason: a cleanup that panicked
        // would mask the real failure it was cleaning up after.
        let Ok(grants) = heddle_sandbox::grants() else {
            return;
        };
        for grant in grants {
            let mine = grant
                .dirs
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|dir| dir.kind == GrantKind::Root && dir.path == self.0);
            if mine {
                let _ = heddle_sandbox::prune(&grant.profile);
            }
        }
    }
}
