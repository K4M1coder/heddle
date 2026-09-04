//! Acceptance tests for `skein sandbox list|prune` (spec 024).
//!
//! Driving the **real** binary as a subprocess, following `cli_secret.rs`'s
//! convention that a test which creates machine state removes it from a `Drop`
//! guard — there it is a credential, here it is an AppContainer profile and the
//! ACEs it left on two directories, so a failing assertion cannot leave either
//! behind on the developer's machine.
//!
//! There is deliberately **no Win32 here**. Whether the ACE really left the
//! directory's security descriptor is `skein-sandbox`'s `tests/prune.rs`,
//! reading it back off the object; what this file asks is the narrower question
//! a CLI test can answer honestly — that the binary is a real client of that
//! capability, and that its interface is what an operator was promised.

#[cfg(windows)]
mod guard;

use std::process::{Command, Output};

fn skein(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_skein"))
        .args(args)
        .output()
        .expect("the skein binary runs")
}

fn both_streams(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Present on all three platforms, and no `#[cfg]` here is the point of the
/// test. A tool advertisement must be absent where it cannot work, because a
/// model calling a disabled name gets a fatal run; a subcommand is read by a
/// human, and one that is simply missing is indistinguishable from a stale
/// binary or a typo.
#[test]
fn sandbox_list_and_prune_are_documented_on_every_platform() {
    let group = skein(&["sandbox", "--help"]);
    assert!(group.status.success(), "{}", both_streams(&group));
    for named in ["list", "prune"] {
        assert!(
            both_streams(&group).contains(named),
            "`skein sandbox --help` must name {named}: {}",
            both_streams(&group)
        );
    }

    let prune = skein(&["sandbox", "prune", "--help"]);
    assert!(prune.status.success(), "{}", both_streams(&prune));
    for flag in ["--profile", "--all"] {
        assert!(
            both_streams(&prune).contains(flag),
            "`skein sandbox prune --help` must name {flag}: {}",
            both_streams(&prune)
        );
    }
}

/// The confirmation this command has instead of a prompt. `secret delete` — the
/// closest destructive precedent — asks nothing either; what stands in for it is
/// that the operator had to name *what* to remove, so a bare `prune` is a usage
/// error rather than a machine-wide delete.
#[test]
fn prune_without_a_selector_is_a_usage_error() {
    let bare = skein(&["sandbox", "prune"]);
    assert_eq!(bare.status.code(), Some(2), "{}", both_streams(&bare));
    for flag in ["--profile", "--all"] {
        assert!(
            both_streams(&bare).contains(flag),
            "the refusal must name the selectors it wanted: {}",
            both_streams(&bare)
        );
    }

    // The other half of the same gate: `--all` is not a widening of `--profile`,
    // it is the other choice, so naming both is as much a usage error as naming
    // neither.
    let both = skein(&[
        "sandbox",
        "prune",
        "--profile",
        "skein-0000000000000000",
        "--all",
    ]);
    assert_eq!(both.status.code(), Some(2), "{}", both_streams(&both));
}

#[cfg(windows)]
mod windows {
    use super::{both_streams, skein};
    use tempfile::TempDir;

    #[test]
    fn a_real_grant_is_listed_and_pruned_through_the_binary() {
        let root = TempDir::new().expect("a temp root");
        let toolbin = TempDir::new().expect("a temp run directory");
        let sandbox = skein_sandbox::Sandbox::create(root.path(), &[toolbin.path().to_path_buf()])
            .expect("the profile and both grants");
        let profile = sandbox.profile().to_string();
        // The passing path prunes through the binary, so this guard is for the
        // failing one: a panicking assertion must not leave a profile and two
        // ACEs behind on the developer's machine.
        let _pruned = crate::guard::PrunedOnDrop::of_root(root.path());

        let listed = skein(&["sandbox", "list"]);
        assert!(listed.status.success(), "{}", both_streams(&listed));
        let root_line = String::from_utf8_lossy(&listed.stdout)
            .lines()
            .find(|line| line.starts_with(&format!("{}\t", profile)) && line.contains("root"))
            .map(str::to_string)
            .unwrap_or_else(|| {
                panic!(
                    "the profile just created must be listed: {}",
                    both_streams(&listed)
                )
            });
        let columns: Vec<&str> = root_line.split('\t').collect();
        assert_eq!(columns.len(), 5, "five columns, always: {root_line:?}");
        assert_eq!(columns[1], sandbox.string_sid());
        assert_eq!(columns[3], "granted");
        assert!(
            columns[4].contains(
                root.path()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .expect("a temp dir has a name")
            ),
            "the last column is the directory: {root_line:?}"
        );

        let pruned = skein(&["sandbox", "prune", "--profile", &profile]);
        assert!(pruned.status.success(), "{}", both_streams(&pruned));
        assert!(
            both_streams(&pruned).contains("deleted profile"),
            "the operator is told what was removed: {}",
            both_streams(&pruned)
        );

        let again = skein(&["sandbox", "list"]);
        assert!(again.status.success(), "{}", both_streams(&again));
        assert!(
            !String::from_utf8_lossy(&again.stdout).contains(&profile),
            "a pruned profile is gone from the listing: {}",
            both_streams(&again)
        );
    }
}

/// Fail clearly, never silently degrade: the subcommand is present here, and
/// what it does is refuse with the reason.
#[cfg(not(windows))]
#[test]
fn sandbox_list_refuses_with_a_reason() {
    let listed = skein(&["sandbox", "list"]);
    assert_eq!(listed.status.code(), Some(1), "{}", both_streams(&listed));
    assert!(
        both_streams(&listed).contains("Windows-only"),
        "the refusal must name the platform: {}",
        both_streams(&listed)
    );
}
