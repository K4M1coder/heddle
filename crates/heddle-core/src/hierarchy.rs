//! Config resolution through the organizational hierarchy (design §5.5, spec
//! 002 FR-015): Silo ▸ Team ▸ Project ▸ Conversation, or the same without Team
//! in Local mode.
//!
//! [`Hierarchy`] is generic over the value it resolves, and that is not
//! speculative generality: §5.5 says in as many words that "this single resolver
//! governs harness, TaskTracker, egress, providers and secrets". A resolver that
//! could only resolve a tracker would have to be written four more times, and
//! the fifth copy is where the lock rule would quietly differ from the first.
//!
//! Spec 002's Key Entity `ConfigScope` — "resolution level + `locked` flag" — is
//! [`Setting`]'s `scope` and `lock` fields; the value it carries is the third
//! field, since a level with no value to resolve is not a thing anyone sets.
//!
//! The whole rule is two sentences of §5.5, and both are load-bearing:
//!
//! 1. Setting a value is not the same as locking it. **Without a lock, the most
//!    specific value wins** (Conversation > Project > Team > Silo).
//! 2. An explicit lock caps everything below it, and **the highest explicit lock
//!    wins**.
//!
//! Security policy's monotonic floor (§5.5's third bullet) is deliberately *not*
//! encoded here. "Tighten" has no meaning for an arbitrary `T` — it needs an
//! ordering on values that only a security-typed value can supply — so a
//! resolver that claimed to enforce it for `Hierarchy<String>` would be claiming
//! something it cannot check.

use crate::error::{HeddleError, Result};
use serde::{Deserialize, Serialize};

/// A level of the hierarchy, ordered from least to most specific.
///
/// The derived `Ord` **is** the specificity order, and both halves of the
/// resolution rule are expressed in terms of it, so the declaration order below
/// is behaviour rather than style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Silo,
    /// Absent in Local mode (§5.5). Present in the enum regardless, for the
    /// reason `Node`'s deferred variants are: this is a serialized shape, and a
    /// Local-mode build that could not even *parse* a server-mode config would
    /// fail to explain why it was refusing it.
    Team,
    Project,
    Conversation,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Silo => "silo",
            Scope::Team => "team",
            Scope::Project => "project",
            Scope::Conversation => "conversation",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether a setting binds the scopes beneath it.
///
/// A named pair rather than a `bool` because every call site reads
/// `Lock::Locked` at a glance and `true` at a guess — and the guess is about the
/// one flag in this module that changes who is allowed to configure what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lock {
    Unlocked,
    Locked,
}

impl Lock {
    pub fn is_locked(&self) -> bool {
        matches!(self, Lock::Locked)
    }
}

/// One scope's configured value. Spec 002's `ConfigScope`, plus what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setting<T> {
    pub scope: Scope,
    pub value: T,
    pub lock: Lock,
}

/// Which levels exist (§5.5): Local mode has no Team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Local,
    Server,
}

impl Mode {
    fn has(&self, scope: Scope) -> bool {
        !matches!((self, scope), (Mode::Local, Scope::Team))
    }
}

/// The settings of one hierarchy, and the rule that picks the winner.
///
/// At most one [`Setting`] per [`Scope`]: a hierarchy is a *configuration*, not
/// an audit trail. (§5.5's "any change to security config is audited" is the
/// Ledger's job, and the Ledger is where a change would be recorded.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hierarchy<T> {
    mode: Mode,
    settings: Vec<Setting<T>>,
}

impl<T> Hierarchy<T> {
    /// Silo ▸ Project ▸ Conversation.
    pub fn local() -> Self {
        Hierarchy::new(Mode::Local)
    }

    /// Silo ▸ Team ▸ Project ▸ Conversation.
    pub fn server() -> Self {
        Hierarchy::new(Mode::Server)
    }

    pub fn new(mode: Mode) -> Self {
        Hierarchy {
            mode,
            settings: Vec::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Every configured setting, in the order it was first set. Read-only: the
    /// only way in is [`Hierarchy::set`], which is where the lock is enforced.
    pub fn settings(&self) -> &[Setting<T>] {
        &self.settings
    }

    /// Configure `scope`, replacing whatever that scope held before.
    ///
    /// Refuses, rather than silently ignoring, in the two cases §5.5 and spec
    /// 002's Edge Cases name — a scope that does not exist in this mode, and a
    /// scope capped by a lock above it. Silence would leave an operator
    /// believing a value they set is in force, which is the failure mode a
    /// lock exists to prevent in the first place.
    ///
    /// A lock does **not** cap the scope that set it, nor any scope above it:
    /// it caps what is beneath. Otherwise a lock would be unrevisable by its own
    /// owner, which no part of §5.5 asks for.
    pub fn set(&mut self, scope: Scope, value: T, lock: Lock) -> Result<()> {
        if !self.mode.has(scope) {
            return Err(HeddleError::Config(format!(
                "scope {scope} does not exist in {:?} mode",
                self.mode
            )));
        }
        if let Some(above) = self.lock_above(scope) {
            return Err(HeddleError::ConfigLocked {
                scope: scope.to_string(),
                locked_at: above.to_string(),
            });
        }

        let setting = Setting { scope, value, lock };
        match self.settings.iter_mut().find(|s| s.scope == scope) {
            Some(existing) => *existing = setting,
            None => self.settings.push(setting),
        }
        Ok(())
    }

    /// The value in force, or `None` if nobody has configured one.
    ///
    /// `None` is not an error: "nobody has chosen" is a legitimate state, and
    /// the caller that must then fall back to an always-available default is the
    /// one that knows what that default is (Constitution IV).
    pub fn resolve(&self) -> Option<&T> {
        self.winner().map(|s| &s.value)
    }

    /// Which scope [`Hierarchy::resolve`] took its answer from — the difference
    /// between "the project chose this" and "the silo made the project take it".
    pub fn resolved_scope(&self) -> Option<Scope> {
        self.winner().map(|s| s.scope)
    }

    /// §5.5's rule, whole: the highest lock if there is one, otherwise the most
    /// specific value.
    fn winner(&self) -> Option<&Setting<T>> {
        self.settings
            .iter()
            .filter(|s| s.lock.is_locked())
            .min_by_key(|s| s.scope)
            .or_else(|| self.settings.iter().max_by_key(|s| s.scope))
    }

    /// The strictly-higher scope whose lock caps `scope`, if any. Strictly
    /// higher, so a scope is never capped by its own lock.
    fn lock_above(&self, scope: Scope) -> Option<Scope> {
        self.settings
            .iter()
            .filter(|s| s.lock.is_locked() && s.scope < scope)
            .map(|s| s.scope)
            .min()
    }
}

impl<T> Default for Hierarchy<T> {
    /// Local mode, because Constitution II makes full-local the default posture
    /// of the product and a `Default` that assumed a Team level would contradict
    /// it.
    fn default() -> Self {
        Hierarchy::local()
    }
}
