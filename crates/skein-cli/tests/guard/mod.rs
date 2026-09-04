//! Fixture hygiene: the app container profile a test caused does not outlive
//! the test.
//!
//! `cli_secret.rs`'s `TestRef` applied to a second kind of machine state. A
//! `--allow-run` session mints an AppContainer profile for its `--fs-root` and
//! leaves an ACE on that directory, and until this slice nothing removed
//! either.
//!
//! Keyed on the **root directory** rather than on a profile name, because the
//! session that created it was a subprocess and no `Sandbox` was ever visible
//! here. The lookup is `skein_sandbox::grants()` — the same capability
//! `skein sandbox list` renders, used on itself.

use skein_sandbox::GrantKind;
use std::path::{Path, PathBuf};

pub struct PrunedOnDrop(PathBuf);

impl PrunedOnDrop {
    /// Declare it **after** the `TempDir` it names, so it drops first and the
    /// prune it performs is a real revoke rather than a `missing`.
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
        let Ok(grants) = skein_sandbox::grants() else {
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
                let _ = skein_sandbox::prune(&grant.profile);
            }
        }
    }
}
