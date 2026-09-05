//! Acceptance tests for `heddle secret set|delete` (spec 011).
//!
//! Against the **real** platform credential store, driving the **real** binary,
//! following `crates/heddle-silo/tests/silo_secret.rs`'s conventions: test
//! credentials are named per process and per test, and every one is removed by a
//! `Drop` guard, so a failing assertion cannot leave a credential behind on the
//! developer's machine.
//!
//! The load-bearing assertion is a negative one — that the value appears in
//! neither output stream. Constitution VI is only met if that is *checked*, not
//! merely intended.

use heddle_core::{SecretProvider, SecretRef};
use heddle_silo::OsKeychain;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const VALUE: &str = "hunter2";

/// A reference unique to this process and this test, whose credential is removed
/// on every exit path including a panic.
struct TestRef {
    keychain: OsKeychain,
    reference: SecretRef,
}

impl TestRef {
    fn unused() -> TestRef {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let service = format!(
            "heddle-cli-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        TestRef {
            keychain: OsKeychain::new().expect("the platform credential store opens"),
            reference: SecretRef(format!("keychain://{service}/cli")),
        }
    }

    fn uri(&self) -> &str {
        &self.reference.0
    }
}

impl Drop for TestRef {
    fn drop(&mut self) {
        // Tolerant by design: a test that already deleted the credential is the
        // normal case, and a cleanup that panics would mask the real failure.
        let _ = self.keychain.delete(&self.reference);
    }
}

fn heddle_with_stdin(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_heddle"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the heddle binary runs");
    child
        .stdin
        .as_mut()
        .expect("stdin was piped")
        .write_all(stdin)
        .expect("the value is piped in");
    child.wait_with_output().expect("the binary exits")
}

fn both_streams(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn e8_secret_set_then_delete_round_trips_without_printing_the_value() {
    let held = TestRef::unused();

    let set = heddle_with_stdin(&["secret", "set", held.uri()], b"hunter2\n");
    assert!(set.status.success(), "{}", both_streams(&set));
    assert!(
        !both_streams(&set).contains(VALUE),
        "the value must not reach either stream: {}",
        both_streams(&set)
    );
    assert!(
        both_streams(&set).contains(held.uri()),
        "the reference names the secret and never carries it, so it is safe to print"
    );

    let stored = held
        .keychain
        .resolve(&held.reference)
        .expect("the value really landed in the platform store");
    assert_eq!(stored.expose(), VALUE, "the trailing newline is stripped");

    let deleted = heddle_with_stdin(&["secret", "delete", held.uri()], b"");
    assert!(deleted.status.success(), "{}", both_streams(&deleted));
    assert!(!both_streams(&deleted).contains(VALUE));
    assert!(
        held.keychain.resolve(&held.reference).is_err(),
        "a deleted credential no longer resolves"
    );
}

#[test]
fn e9_secret_set_has_no_value_flag_and_refuses_an_empty_secret() {
    let held = TestRef::unused();

    let flagged = heddle_with_stdin(&["secret", "set", held.uri(), "--value", VALUE], b"");
    assert_eq!(
        flagged.status.code(),
        Some(2),
        "a secret passed as a flag lands in shell history and in process listings, so the \
         flag must not exist: {}",
        both_streams(&flagged)
    );
    assert!(
        both_streams(&flagged).contains("unexpected argument '--value' found"),
        "{}",
        both_streams(&flagged)
    );

    let empty = heddle_with_stdin(&["secret", "set", held.uri()], b"");
    assert_eq!(empty.status.code(), Some(1));
    assert!(
        both_streams(&empty).contains("secret:"),
        "{}",
        both_streams(&empty)
    );
    assert!(
        held.keychain.resolve(&held.reference).is_err(),
        "a refused set stores nothing"
    );
}
