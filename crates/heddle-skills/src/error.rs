//! Error type for the skills/recipes compiler.

use thiserror::Error;

/// Why a recipe could not be loaded or compiled.
///
/// A type of its own rather than a reach into `heddle_core::HeddleError`, for
/// Constitution IV's reason: a recipe's parse and validation failures are this
/// crate's vocabulary, and borrowing the core's error type would have made
/// `HeddleError` grow a variant for every format this layer ever learns to
/// read. The shape and the one-variant-per-failure-class discipline are
/// `HeddleError`'s, deliberately — a reader moving between the two files should
/// recognize the house style.
#[derive(Debug, Error)]
pub enum SkillError {
    /// The file could not be read. Carries the path because an operator who
    /// mistyped one needs to see which path was tried; a bare `io::Error` says
    /// the system cannot find the file specified without saying which file.
    #[error("could not read the recipe at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The file was read and is not a valid recipe. Distinct from
    /// [`SkillError::Io`] because the remedy is different: one is a wrong path,
    /// the other a wrong file.
    #[error("the recipe at {path} is not valid: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    /// The recipe names a tool the target gateway does not advertise.
    ///
    /// Raised at compile time, before a single node is built, rather than being
    /// left for the run to discover: a `Node::Tool` naming an absent tool would
    /// fail somewhere inside the gateway, mid-run, having already executed
    /// whatever came before it. Refusing to compile leaves nothing to undo.
    #[error(
        "recipe {recipe} requires the extension {extension:?}, which the target gateway does not \
         advertise; it advertises: {available}"
    )]
    MissingExtension {
        recipe: String,
        extension: String,
        /// What *was* available, so the message answers "then what may I use?"
        /// in the same breath as refusing.
        available: String,
    },
    /// A `{{placeholder}}` in the recipe has no value to substitute.
    ///
    /// One variant covers both ways that happens — a declared param the caller
    /// omitted with no default, and a placeholder naming no declared param at
    /// all — because the operator's symptom is identical either way: a prompt
    /// carrying literal braces to a model. See `plan.md` D4.
    #[error(
        "recipe {recipe} references {{{{{param}}}}}, which has no supplied value and no default"
    )]
    MissingParam { recipe: String, param: String },
    /// A `{{` was opened and never closed. Refused rather than passed through,
    /// so an unbalanced brace is a named mistake instead of a prompt that
    /// silently keeps its punctuation.
    #[error("recipe {recipe} has an unterminated placeholder in {context}")]
    UnterminatedPlaceholder { recipe: String, context: String },
}

/// This crate's result type, named as `heddle-core`'s is.
pub type Result<T> = std::result::Result<T, SkillError>;
