//! Secret handling (design §7.13, Constitution VI): the core holds *references*
//! and discovers values through a trait, so no credential store is named here
//! and no secret is ever configuration.

use crate::error::Result;
use zeroize::Zeroizing;

/// A URI naming a secret, never its value: `keychain://<service>/<account>`
/// today, `sops://` / `op://` / `bao://` when those backends land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef(pub String);

/// A resolved secret. Zeroized on drop, and its `Debug` is written by hand, so a
/// *derived* formatter on any struct holding one renders `SecretValue(***)`
/// rather than the secret. Reading the value takes a deliberate [`expose`].
///
/// [`expose`]: SecretValue::expose
pub struct SecretValue(Zeroizing<String>);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        SecretValue(Zeroizing::new(value.into()))
    }

    /// The one explicit place a caller opts into seeing the value.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretValue(***)")
    }
}

/// How the core discovers a secret's value (Constitution IV): just-in-time, in
/// memory, never persisted.
///
/// Read-only by construction. Provisioning lives on the concrete backend, so a
/// provider handed to a [`crate::tool::ToolGateway`] has no expressible way to
/// write a secret.
pub trait SecretProvider {
    fn resolve(&self, secret: &SecretRef) -> Result<SecretValue>;

    /// Governs availability under the egress policy (design §7.3): in Local
    /// mode, with egress OFF, only offline backends are usable.
    fn requires_network(&self) -> bool;
}
