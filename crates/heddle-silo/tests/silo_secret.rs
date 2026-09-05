//! Acceptance tests for the OS-keychain `SecretProvider` (spec 010).
//!
//! Every assertion is against the **real** platform credential store. A secret
//! backend proved against an in-memory stand-in proves nothing: the claim is
//! that Heddle can read the vault the operator already trusts.
//!
//! Test credentials are named per process and per test, and every one of them is
//! removed by a `Drop` guard, so a failing assertion cannot leave a credential
//! behind on the developer's machine.

use heddle_core::{
    HeddleError, Ledger, Redactor, Result, SecretProvider, SecretRef, StepKind, ToolAccess,
    ToolCall, ToolGateway, ToolOutcome, ToolPolicy, ToolTransport,
};
use heddle_silo::OsKeychain;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};

/// A transport that hands back whatever it was built with — here, a payload that
/// embeds the secret, because the interesting case is a tool that legitimately
/// needs one and echoes it.
struct EchoTransport {
    reply: String,
}

impl ToolTransport for EchoTransport {
    fn call(&mut self, _call: &ToolCall) -> Result<ToolOutcome> {
        Ok(ToolOutcome {
            content: self.reply.clone(),
        })
    }
}

/// A credential in the real store, unique to this process and this test, deleted
/// on every exit path including a panic.
struct TestSecret {
    keychain: OsKeychain,
    reference: SecretRef,
}

impl TestSecret {
    fn store(value: &str) -> TestSecret {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let service = format!(
            "heddle-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let reference = SecretRef(format!("keychain://{service}/redactor"));
        let keychain = OsKeychain::new().expect("the platform credential store opens");
        keychain
            .store(&reference, value)
            .expect("the platform credential store accepts a write");
        TestSecret {
            keychain,
            reference,
        }
    }
}

impl Drop for TestSecret {
    fn drop(&mut self) {
        // Tolerant by design: a test that already deleted the credential is the
        // normal case, and a cleanup that panics would mask the real failure.
        let _ = self.keychain.delete(&self.reference);
    }
}

#[test]
fn k1_keychain_round_trips_a_secret() {
    let held = TestSecret::store("hunter2");

    let resolved = held
        .keychain
        .resolve(&held.reference)
        .expect("what was stored comes back");
    assert_eq!(resolved.expose(), "hunter2");

    held.keychain
        .delete(&held.reference)
        .expect("the credential is removable");

    let err = held
        .keychain
        .resolve(&held.reference)
        .expect_err("a deleted credential no longer resolves");
    assert!(matches!(err, HeddleError::Secret(_)), "got {err}");
}

#[test]
fn k2_a_missing_reference_fails_loudly() {
    let keychain = OsKeychain::new().unwrap();
    let never_stored = SecretRef(format!(
        "keychain://heddle-test-{}-never/redactor",
        std::process::id()
    ));

    let err = keychain
        .resolve(&never_stored)
        .expect_err("an unstored reference must not resolve");

    assert!(
        matches!(err, HeddleError::Secret(_)),
        "a missing secret is an error, never an empty value: {err}"
    );
}

#[test]
fn k3_a_non_keychain_scheme_is_rejected() {
    let keychain = OsKeychain::new().unwrap();

    for uri in [
        "op://vault/item",
        "sops://file/key",
        "keychain://",
        "keychain:///redactor",
        "keychain://service",
        "keychain://service/",
        "just-a-string",
    ] {
        let err = keychain
            .resolve(&SecretRef(uri.into()))
            .err()
            .unwrap_or_else(|| panic!("{uri:?} must be refused"));
        assert!(
            matches!(err, HeddleError::Secret(_)),
            "{uri:?} refused as a secret error, got {err}"
        );
    }
}

#[test]
fn k4_a_governed_call_redacts_a_provider_resolved_secret() {
    let held = TestSecret::store("hunter2");
    let mut ledger = Ledger::new();

    // The config holds the reference; the Redactor is what turns it into a value.
    let redactor = Redactor::resolve(&held.keychain, std::slice::from_ref(&held.reference))
        .expect("the stored reference resolves");
    let mut gateway = ToolGateway::new(
        EchoTransport {
            reply: "Authorization: Bearer hunter2".into(),
        },
        ToolPolicy::new(
            vec![("read_secret".into(), ToolAccess::ReadOnly)],
            Vec::new(),
        ),
        redactor,
    );

    let outcome = gateway
        .call(
            "run-k4",
            &ToolCall::new("read_secret", json!({})),
            &mut ledger,
        )
        .expect("an allowlisted read-only tool runs");

    assert!(
        outcome.content.contains("hunter2"),
        "the trusted caller still gets the real secret: it is what the tool returned"
    );
    let payloads: Vec<String> = ledger
        .log("run-k4")
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains("hunter2")),
        "no captured payload may contain the secret: {payloads:?}"
    );
    let result = ledger
        .log("run-k4")
        .into_iter()
        .find(|s| s.kind == StepKind::ToolResult)
        .expect("the executed call was recorded")
        .payload
        .clone();
    assert!(result.contains("***"), "{result}");
    ledger
        .verify_chain("run-k4")
        .expect("redaction does not break the chain");
}

#[test]
fn k5_the_os_keychain_needs_no_network() {
    assert!(
        !OsKeychain::new().unwrap().requires_network(),
        "the OS keychain is what makes Local mode with egress OFF usable (design §7.3)"
    );
}
