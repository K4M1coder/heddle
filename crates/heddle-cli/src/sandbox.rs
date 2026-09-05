//! `heddle sandbox list|prune` — a rendering of `heddle-sandbox`'s two cleanup
//! functions, and nothing more.
//!
//! `Sandbox::create` grants an AppContainer identity an ACE on the `--fs-root`
//! and on every `--run-dir`, and leaves an AppContainer profile behind. Both
//! outlive the process that made them, and until this command existed nothing
//! removed either. This file is the operator's way to see what is there and to
//! take it back; the proof that a removal removes only what heddle granted is
//! structural and lives in the library.

use heddle_core::{HeddleError, Result};
use heddle_sandbox::{Grant, GrantKind, GrantState, Pruned};

/// Five tab-separated columns, unconditionally — `ledger::log`'s rule, and for
/// its reason: the set does not change with a profile's state, so a script's
/// field offsets never shift.
///
/// ```text
/// <profile>\t<sid>\t<root|run-dir|unrecorded>\t<granted|clear|missing|->\t<path|->
/// ```
pub fn list() -> Result<()> {
    for grant in heddle_sandbox::grants().map_err(refused)? {
        for (kind, state, path) in columns(&grant) {
            println!("{}\t{}\t{kind}\t{state}\t{path}", grant.profile, grant.sid);
        }
    }
    Ok(())
}

fn columns(grant: &Grant) -> Vec<(&'static str, &'static str, String)> {
    match &grant.dirs {
        // One line rather than none: a profile that cannot say where its ACEs
        // are is precisely the one an operator most needs to see.
        None => vec![("unrecorded", "-", "-".to_string())],
        Some(dirs) => dirs
            .iter()
            .map(|dir| {
                (
                    match dir.kind {
                        GrantKind::Root => "root",
                        GrantKind::RunDir => "run-dir",
                    },
                    match dir.state {
                        GrantState::Granted => "granted",
                        GrantState::Clear => "clear",
                        GrantState::Missing => "missing",
                    },
                    dir.path.display().to_string(),
                )
            })
            .collect(),
    }
}

/// One named profile, or every profile heddle made. clap makes the choice
/// exclusive and mandatory, so there is no third case here and no default.
pub fn prune(profile: Option<&str>, all: bool) -> Result<()> {
    let selected: Vec<String> = match (profile, all) {
        (Some(named), _) => vec![named.to_string()],
        _ => heddle_sandbox::grants()
            .map_err(refused)?
            .into_iter()
            .map(|grant| grant.profile)
            .collect(),
    };

    // Reported as it goes rather than at the end: a prune that fails part way
    // through has still removed everything printed above the error, and an
    // operator needs to know which.
    for name in selected {
        report(heddle_sandbox::prune(&name).map_err(refused)?);
    }
    Ok(())
}

fn report(pruned: Pruned) {
    for path in &pruned.revoked {
        println!("revoked {}", path.display());
    }
    for path in &pruned.clear {
        println!("clear {}", path.display());
    }
    for path in &pruned.missing {
        println!("missing {}", path.display());
    }
    if pruned.unrecorded {
        println!(
            "deleted profile {} (no record: any directories it was granted are unknown and were \
             not touched)",
            pruned.profile
        );
    } else {
        println!("deleted profile {}", pruned.profile);
    }
}

/// `heddle-sandbox` is a leaf that depends on no Heddle crate, so it refuses with
/// a `String` and the mapping happens here.
///
/// `Storage` because this is durable local machine state that could not be read
/// or removed — the nearest thing `HeddleError` has. A `Sandbox` variant would
/// read better and is deliberately not added: it would mean editing
/// `heddle-core` for a rendering concern at the outermost layer.
fn refused(reason: String) -> HeddleError {
    HeddleError::Storage(reason)
}
