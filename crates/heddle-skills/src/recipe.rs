//! The recipe vocabulary (design §4.4, spec 037).
//!
//! Design §4.4 gives the canonical shape in one line —
//! `Recipe = { name, description, instructions, required_extensions[],
//! params[], prompt }` — and leaves `instructions` and `params` to a spec to
//! pin down. Spec 037 pins them here.
//!
//! [`RecipeStep`] is **narrower** than [`heddle_workflow::Node`] on purpose,
//! and in two different ways. It has three variants where `Node` has seven,
//! because the other four refuse at
//! [`WorkflowEngine::run`](heddle_workflow::WorkflowEngine::run) today: a
//! recipe author who could write `kind = "loop"` would be writing a recipe
//! guaranteed to fail. And its fields are plain strings where `Node`'s are
//! [`Message`](heddle_core::Message) and [`ToolCall`](heddle_core::ToolCall),
//! because a recipe author should not have to hand-write a message's
//! `parts = [{ type = "text", text = "..." }]` in TOML to say "ask the model
//! this". Turning the one into the other is [`compile`](crate::compile)'s whole
//! job.

use serde::{Deserialize, Serialize};

/// A skill as its author writes it: what it is, what it needs, and what it does.
///
/// `name` becomes the compiled [`Workflow`](heddle_workflow::Workflow)'s name,
/// which is what a reader of the Ledger sees, so it is required rather than
/// defaulted — an unnamed run is a run nobody can find again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    pub description: String,
    /// The recipe-level persona, prepended to every [`RecipeStep::Agent`]'s own
    /// prompt at compile time. One statement of "who you are" for the whole
    /// recipe, rather than the same paragraph copied into every step — which is
    /// what an author would otherwise have to do, since a `Node::Agent` carries
    /// exactly one message and the engine has no separate system-prompt slot.
    pub prompt: String,
    /// Declared parameters. Defaulted, so a recipe that takes none does not
    /// have to write `params = []` to say so.
    #[serde(default)]
    pub params: Vec<RecipeParam>,
    /// The tools this recipe cannot run without, by the name a model would call
    /// them by. Checked against what the target gateway advertises before
    /// anything is built (`plan.md` D3).
    #[serde(default)]
    pub required_extensions: Vec<String>,
    /// The steps, in the order they run. Required and not defaulted: a recipe
    /// whose `instructions` key is missing entirely is almost always mis-nested
    /// TOML, whereas an author who genuinely means "no steps" can still write
    /// `instructions = []` and get an empty graph.
    pub instructions: Vec<RecipeStep>,
}

/// One declared parameter.
///
/// `default` is what makes a parameter optional; a parameter without one is
/// required, and omitting it is
/// [`SkillError::MissingParam`](crate::SkillError::MissingParam). Values are
/// `String` and not `serde_json::Value` because substitution is textual — a
/// placeholder always lands inside a prompt, a message, or a JSON string leaf,
/// and every one of those positions wants text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeParam {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default: Option<String>,
}

/// One step of a recipe: exactly the three things the engine executes.
///
/// The `#[serde(tag = "kind", rename_all = "snake_case")]` attribute is
/// [`heddle_workflow::Node`]'s own, copied deliberately rather than
/// coincidentally: a recipe author writes `kind = "agent"` and so does anyone
/// hand-writing a serialized `Workflow`, so the two vocabularies read the same
/// even though the types differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecipeStep {
    /// One bounded model turn. `prompt` is joined to the recipe-level persona
    /// at compile time.
    Agent { id: String, prompt: String },
    /// One governed tool call. `tool` is the advertised name; `args` is handed
    /// to the gateway as written, with its string leaves substituted.
    Tool {
        id: String,
        tool: String,
        /// Defaulted to `Value::Null`, which is what a tool taking no arguments
        /// serializes to anyway — requiring `args = {}` would make the common
        /// shape the awkward one.
        #[serde(default)]
        args: serde_json::Value,
    },
    /// A human gate. The engine stops here until a decision is recorded out of
    /// band, so a recipe cannot approve itself.
    Approval { id: String, message: String },
}

impl RecipeStep {
    /// The step's own identity, whatever its kind — and the id the compiled
    /// [`Node`](heddle_workflow::Node) carries, which is how a resumed run says
    /// which step it is pending on.
    ///
    /// Read off the same `match` that decides the behaviour, mirroring
    /// [`heddle_workflow::Node::id`], so a variant added later cannot be named
    /// in one place and forgotten in the other.
    pub fn id(&self) -> &str {
        match self {
            RecipeStep::Agent { id, .. }
            | RecipeStep::Tool { id, .. }
            | RecipeStep::Approval { id, .. } => id,
        }
    }
}
