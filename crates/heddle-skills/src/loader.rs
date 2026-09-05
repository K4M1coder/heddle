//! Reading a recipe off disk.
//!
//! The two-function split is `ProviderTable::from_path`/`from_toml_str`'s
//! (`crates/heddle-gateway/src/route.rs:174-193`), and it is worth having for
//! the same reason there: disk I/O lives in one function and parsing in the
//! other, so the parser is testable against a `&str` without a filesystem, and
//! a wrong path is a different error from a wrong file.

use crate::error::{Result, SkillError};
use crate::recipe::Recipe;
use std::path::Path;

impl Recipe {
    /// Reads a recipe from a file.
    ///
    /// Both failure modes name the path. `from_toml_str` cannot know one — it is
    /// handed text — so the path is attached here, which is why its own error
    /// carries a placeholder origin that this function replaces.
    pub fn from_path(path: &Path) -> Result<Recipe> {
        let text = std::fs::read_to_string(path).map_err(|source| SkillError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Recipe::from_toml_str(&text).map_err(|e| match e {
            SkillError::Parse { source, .. } => SkillError::Parse {
                path: path.display().to_string(),
                source,
            },
            other => other,
        })
    }

    /// Parses a recipe from TOML text.
    ///
    /// Deliberately the whole schema, as `from_toml_str` is for the provider
    /// table: there is no include mechanism and no inheritance between recipes.
    /// A richer skill-packaging format, if one lands, should be able to
    /// *contain* this shape rather than having to reconcile with it.
    pub fn from_toml_str(text: &str) -> Result<Recipe> {
        toml::from_str(text).map_err(|source| SkillError::Parse {
            // Replaced by `from_path` when there is a real path to name. A
            // caller that parses a string it built itself has no path to
            // report, and inventing one would be a lie in the message.
            path: "<string>".to_string(),
            source,
        })
    }
}
