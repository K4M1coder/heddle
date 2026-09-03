//! `skein secret set|delete` — the operator's way into the platform credential
//! store (design §7.13), and the second caller of `OsKeychain::store`/`delete`.
//!
//! Nothing here may put the value into a message, a format string or a stream.
//! Only the reference is ever printed: it names the secret and never carries it,
//! which is the point of holding references in the first place (Constitution VI).

use skein_core::{Result, SecretRef, SkeinError};
use skein_silo::OsKeychain;
use std::io::{IsTerminal, Read};

/// The value comes from stdin and from nowhere else. There is deliberately no
/// `--value` flag: it would land in shell history and in `ps`/Task Manager
/// process listings, which is exactly the leak Constitution VI names.
pub fn set(reference: &str) -> Result<()> {
    let secret = SecretRef(reference.to_string());
    OsKeychain::new()?.store(&secret, &read_value()?)?;
    println!("set {reference}");
    Ok(())
}

pub fn delete(reference: &str) -> Result<()> {
    let secret = SecretRef(reference.to_string());
    OsKeychain::new()?.delete(&secret)?;
    println!("deleted {reference}");
    Ok(())
}

/// Reads stdin to EOF, dropping one trailing line ending so the everyday
/// `echo`/heredoc idioms do not silently store a newline.
///
/// An interactive stdin is **refused** rather than prompted for. A prompt — even
/// a non-echoing one — can still put the value into terminal scrollback, and it
/// is a path no automated test can exercise without a PTY. Refusing closes the
/// leak completely and costs no dependency.
fn read_value() -> Result<String> {
    if std::io::stdin().is_terminal() {
        return Err(SkeinError::Secret(
            "refusing to read a secret from a terminal: pipe it instead, e.g. \
             `printf %s \"$TOKEN\" | skein secret set <REFERENCE>`"
                .into(),
        ));
    }

    let mut value = String::new();
    std::io::stdin()
        .read_to_string(&mut value)
        .map_err(|e| SkeinError::Secret(format!("could not read the value from stdin: {e}")))?;

    if let Some(trimmed) = value.strip_suffix('\n') {
        value.truncate(trimmed.strip_suffix('\r').unwrap_or(trimmed).len());
    }

    // `Redactor::from_values` drops empty secrets, so an empty credential is
    // silently useless downstream — better to refuse it at write time.
    if value.is_empty() {
        return Err(SkeinError::Secret(
            "the value read from stdin is empty; nothing was stored".into(),
        ));
    }
    Ok(value)
}
