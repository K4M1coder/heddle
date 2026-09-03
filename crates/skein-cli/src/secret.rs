//! Placeholder for T8's red: the secret commands land in the next step.

use skein_core::{Result, SkeinError};

fn unimplemented(reference: &str) -> Result<()> {
    Err(SkeinError::Secret(format!("{reference}: not implemented")))
}

pub fn set(reference: &str) -> Result<()> {
    unimplemented(reference)
}

pub fn delete(reference: &str) -> Result<()> {
    unimplemented(reference)
}
