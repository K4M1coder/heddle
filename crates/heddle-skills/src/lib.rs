//! Heddle's skills/recipes engine (spec 037, design §4.4).
//!
//! A **recipe** is a skill as its author writes it: a small TOML file naming
//! its steps, the tools it cannot run without, and its parameters. This crate
//! loads one and **compiles** it into a [`heddle_workflow::Workflow`] — the
//! exact type spec 002's native engine already executes.
//!
//! **There is no second execution path here, and that is the point.** Design
//! §4.12 states it as an architectural commitment — *"Goose recipes +
//! BMAD/Spec-Kit flows = workflows (a recipe is a declarative `Workflow`)"* —
//! and spec 002's FR-013b as a requirement. A recipe-specific interpreter would
//! satisfy neither: it would be a parallel core to keep in sync (Constitution
//! IV), and everything the Ledger guarantees about a workflow run — durability,
//! resume, replay, the governed tool triple — would have to be re-earned in a
//! second place. Compiling instead means a recipe inherits all of it by
//! construction, and `crates/heddle-skills/tests/end_to_end.rs` is the same
//! assertion `crates/heddle-workflow/tests/sequential.rs` makes about a
//! hand-built graph, differing only in where the graph came from.
//!
//! This crate depends on `heddle-workflow`, `heddle-core`, `serde`,
//! `serde_json`, `toml` and `thiserror`, and names no provider, protocol or
//! connector (Constitution IV).

pub mod compile;
pub mod error;
pub mod loader;
pub mod recipe;

pub use compile::compile;
pub use error::{Result, SkillError};
pub use recipe::{Recipe, RecipeParam, RecipeStep};
