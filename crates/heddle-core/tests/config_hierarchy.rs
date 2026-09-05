//! Spec 002 FR-015 / User Story 2, and design §5.5: config resolution through
//! the Silo ▸ Team ▸ Project ▸ Conversation hierarchy.
//!
//! Every test here is about the *resolver*, not about task tracking. That
//! separation is the point of the type being generic: design §5.5 says "this
//! single resolver governs harness, TaskTracker, egress, providers and
//! secrets", so a resolver that could only resolve a tracker would be the wrong
//! shape. The values below are tracker names only because that is the example
//! the spec works through.

use heddle_core::{HeddleError, Hierarchy, Lock, Scope};

fn jira() -> String {
    "jira".to_string()
}

fn vikunja() -> String {
    "vikunja".to_string()
}

fn local() -> String {
    "local".to_string()
}

// ---- the two halves of §5.5's rule ----

#[test]
fn with_no_lock_anywhere_the_most_specific_value_wins() {
    let mut h = Hierarchy::server();
    h.set(Scope::Silo, jira(), Lock::Unlocked).unwrap();
    h.set(Scope::Project, vikunja(), Lock::Unlocked).unwrap();

    assert_eq!(h.resolve(), Some(&vikunja()));
    assert_eq!(h.resolved_scope(), Some(Scope::Project));
}

#[test]
fn setting_a_value_is_not_the_same_as_locking_it() {
    // §5.5, verbatim: a silo that only *supplies* Jira as a default leaves a
    // project free to choose otherwise. This is the test that would pass just
    // as well if `locked` were ignored entirely, which is why it is paired with
    // the one below rather than standing alone.
    let mut h = Hierarchy::server();
    h.set(Scope::Silo, jira(), Lock::Unlocked).unwrap();

    h.set(Scope::Project, vikunja(), Lock::Unlocked)
        .expect("an unlocked default above must not bind a project");
    assert_eq!(h.resolve(), Some(&vikunja()));
}

#[test]
fn a_lock_at_the_silo_is_honoured_by_a_child_project() {
    // Spec 002 US2 acceptance scenario 1: TaskTracker=Jira locked at the silo,
    // and a conversation in a child project gets Jira.
    let mut h = Hierarchy::server();
    h.set(Scope::Silo, jira(), Lock::Locked).unwrap();

    assert_eq!(h.resolve(), Some(&jira()));
    assert_eq!(h.resolved_scope(), Some(Scope::Silo));
}

#[test]
fn a_lower_scope_that_tries_to_override_a_lock_above_is_refused_explicitly() {
    // `spec.md`'s Edge Cases: "a lower level attempts to override a setting
    // locked higher up → explicit refusal". A silent no-op would leave an
    // operator believing a value they set is in force.
    let mut h = Hierarchy::server();
    h.set(Scope::Silo, jira(), Lock::Locked).unwrap();

    let refusal = h
        .set(Scope::Project, vikunja(), Lock::Unlocked)
        .expect_err("a project may not override a silo lock");
    match refusal {
        HeddleError::ConfigLocked { scope, locked_at } => {
            assert_eq!(scope, "project");
            assert_eq!(locked_at, "silo");
        }
        other => panic!("expected ConfigLocked, got {other:?}"),
    }
    assert_eq!(
        h.resolve(),
        Some(&jira()),
        "and the refused write left the resolved value untouched"
    );
}

#[test]
fn a_project_with_no_setting_above_it_may_choose_its_own_tracker() {
    // Spec 002 US2 acceptance scenario 2.
    let mut h = Hierarchy::server();

    h.set(Scope::Project, vikunja(), Lock::Unlocked)
        .expect("nothing above the project constrains it");
    assert_eq!(h.resolve(), Some(&vikunja()));
    assert_eq!(h.resolved_scope(), Some(Scope::Project));
}

#[test]
fn the_scope_that_owns_a_lock_may_still_change_its_own_mind() {
    // A lock caps the scopes *below* it, not the one that set it. Refusing the
    // owner would make a lock unrevisable, which no part of §5.5 asks for.
    let mut h = Hierarchy::server();
    h.set(Scope::Silo, jira(), Lock::Locked).unwrap();

    h.set(Scope::Silo, vikunja(), Lock::Locked)
        .expect("the locking scope owns its own setting");
    assert_eq!(h.resolve(), Some(&vikunja()));
}

#[test]
fn the_highest_explicit_lock_wins_over_a_lower_one() {
    // §5.5: "the highest explicit lock wins". Reached here by locking the lower
    // scope first, so the ordering of the writes cannot be what decides it.
    let mut h = Hierarchy::server();
    h.set(Scope::Project, vikunja(), Lock::Locked).unwrap();
    h.set(Scope::Silo, jira(), Lock::Locked)
        .expect("a scope above a lock is not capped by it");

    assert_eq!(h.resolve(), Some(&jira()));
    assert_eq!(h.resolved_scope(), Some(Scope::Silo));
}

#[test]
fn a_lock_below_an_unlocked_default_still_binds_what_is_under_it() {
    let mut h = Hierarchy::server();
    h.set(Scope::Silo, jira(), Lock::Unlocked).unwrap();
    h.set(Scope::Team, vikunja(), Lock::Locked).unwrap();

    assert_eq!(h.resolve(), Some(&vikunja()));
    let refusal = h.set(Scope::Conversation, local(), Lock::Unlocked);
    assert!(
        refusal.is_err(),
        "a team lock caps the conversation beneath it"
    );
}

#[test]
fn setting_the_same_scope_twice_replaces_rather_than_accumulates() {
    let mut h = Hierarchy::server();
    h.set(Scope::Project, jira(), Lock::Unlocked).unwrap();
    h.set(Scope::Project, vikunja(), Lock::Unlocked).unwrap();

    assert_eq!(h.resolve(), Some(&vikunja()));
    assert_eq!(
        h.settings().len(),
        1,
        "one setting per scope, not a history"
    );
}

#[test]
fn an_unconfigured_hierarchy_resolves_to_nothing() {
    // Not an error: "nobody has chosen" is a legitimate state, and the caller
    // that must then fall back to the always-available local tracker is the one
    // that knows what "local" means (Constitution IV).
    let h: Hierarchy<String> = Hierarchy::server();
    assert_eq!(h.resolve(), None);
    assert_eq!(h.resolved_scope(), None);
}

// ---- Local mode: the same rule, without a Team level ----

#[test]
fn local_mode_resolves_the_hierarchy_with_no_team_level() {
    // Spec 002 US2 acceptance scenario 3, and §5.5's "Local mode: Silo(local) ▸
    // Project ▸ Conversation".
    let mut h = Hierarchy::local();
    h.set(Scope::Silo, jira(), Lock::Unlocked).unwrap();
    h.set(Scope::Project, vikunja(), Lock::Unlocked).unwrap();
    h.set(Scope::Conversation, local(), Lock::Unlocked).unwrap();

    assert_eq!(h.resolve(), Some(&local()));

    let mut locked = Hierarchy::local();
    locked.set(Scope::Silo, jira(), Lock::Locked).unwrap();
    assert_eq!(
        locked.resolve(),
        Some(&jira()),
        "a silo lock binds a local project exactly as it binds a server one"
    );
    assert!(locked
        .set(Scope::Project, vikunja(), Lock::Unlocked)
        .is_err());
}

#[test]
fn local_mode_refuses_a_team_scoped_setting_outright() {
    // The Team level does not merely resolve to nothing in Local mode — it does
    // not exist (§5.5). Accepting the write and ignoring it would make
    // `resolve` lie about which scopes were consulted.
    let mut h = Hierarchy::local();

    let refusal = h
        .set(Scope::Team, jira(), Lock::Unlocked)
        .expect_err("Local mode has no Team level");
    match refusal {
        HeddleError::Config(detail) => assert!(
            detail.contains("team"),
            "the refusal names the offending scope: {detail}"
        ),
        other => panic!("expected Config, got {other:?}"),
    }
    assert_eq!(h.resolve(), None);
}

#[test]
fn scopes_are_ordered_from_least_to_most_specific() {
    // The ordering is what both halves of the resolution rule are expressed in,
    // so it is asserted directly rather than only through its consequences.
    assert!(Scope::Silo < Scope::Team);
    assert!(Scope::Team < Scope::Project);
    assert!(Scope::Project < Scope::Conversation);
}
