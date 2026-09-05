//! The silo's secret backend (design §7.13): the platform's own credential
//! store, offline and zero-config.
//!
//! This is the only module in the product that names a credential store, so
//! `heddle-core` discovers secrets through `SecretProvider` and never through an
//! OS API. The store is owned by the `OsKeychain` value rather than registered
//! as `keyring-core`'s process-global default, which is what keeps design §7.2's
//! "one keychain per silo" expressible.

use heddle_core::{HeddleError, Result, SecretProvider, SecretRef, SecretValue};
use keyring_core::{CredentialStore, Entry, Error};
use std::sync::Arc;

const SCHEME: &str = "keychain://";

/// The platform credential store — Windows Credential Manager, macOS Keychain
/// Services, or the Linux kernel session keyring.
pub struct OsKeychain {
    backend: Arc<CredentialStore>,
}

impl OsKeychain {
    pub fn new() -> Result<OsKeychain> {
        Ok(OsKeychain {
            backend: native_store()?,
        })
    }

    /// Writes a secret. Inherent rather than a `SecretProvider` method: the
    /// product only ever reads, so a provider handed to a `ToolGateway` must
    /// have no expressible way to write one.
    pub fn store(&self, secret: &SecretRef, value: &str) -> Result<()> {
        self.entry(secret)?
            .set_password(value)
            .map_err(|e| refused(secret, e))
    }

    /// Removes a secret. Inherent for the same reason as [`OsKeychain::store`].
    pub fn delete(&self, secret: &SecretRef) -> Result<()> {
        self.entry(secret)?
            .delete_credential()
            .map_err(|e| refused(secret, e))
    }

    fn entry(&self, secret: &SecretRef) -> Result<Entry> {
        let (service, account) = parse(secret)?;
        self.backend
            .build(service, account, None)
            .map_err(|e| refused(secret, e))
    }
}

impl SecretProvider for OsKeychain {
    fn resolve(&self, secret: &SecretRef) -> Result<SecretValue> {
        self.entry(secret)?
            .get_password()
            .map(SecretValue::new)
            .map_err(|e| refused(secret, e))
    }

    /// The whole point of this backend: it is what makes Local mode with egress
    /// OFF usable (design §7.3).
    fn requires_network(&self) -> bool {
        false
    }
}

/// Splits `keychain://<service>/<account>`, refusing anything else.
///
/// The validation has to be ours: at least one platform store accepts an empty
/// service name, and this backend must not silently serve a scheme it does not
/// implement.
fn parse(secret: &SecretRef) -> Result<(&str, &str)> {
    let refuse = |why: &str| HeddleError::Secret(format!("secret reference {:?} {why}", secret.0));
    let rest = secret
        .0
        .strip_prefix(SCHEME)
        .ok_or_else(|| refuse("is not a keychain:// reference"))?;
    let (service, account) = rest
        .split_once('/')
        .ok_or_else(|| refuse("has no /<account>"))?;
    if service.is_empty() || account.is_empty() {
        return Err(refuse("has an empty service or account"));
    }
    Ok((service, account))
}

/// The reference names the secret and never carries it, so it is safe in the
/// message — which is the point of holding references in the first place.
fn refused(secret: &SecretRef, err: Error) -> HeddleError {
    HeddleError::Secret(format!("{}: {err}", secret.0))
}

#[cfg(target_os = "windows")]
fn native_store() -> Result<Arc<CredentialStore>> {
    let store: Arc<CredentialStore> =
        windows_native_keyring_store::Store::new().map_err(unopened)?;
    Ok(store)
}

#[cfg(target_os = "macos")]
fn native_store() -> Result<Arc<CredentialStore>> {
    let store: Arc<CredentialStore> =
        apple_native_keyring_store::keychain::Store::new().map_err(unopened)?;
    Ok(store)
}

/// The kernel session keyring, not Secret Service: it needs no D-Bus, no
/// `gnome-keyring` and no graphical session, so it works on a headless host.
#[cfg(target_os = "linux")]
fn native_store() -> Result<Arc<CredentialStore>> {
    let store: Arc<CredentialStore> =
        linux_keyutils_keyring_store::Store::new().map_err(unopened)?;
    Ok(store)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
compile_error!("OsKeychain supports Windows, macOS and Linux; this target has no native store");

fn unopened(err: Error) -> HeddleError {
    HeddleError::Secret(format!("the platform credential store did not open: {err}"))
}
