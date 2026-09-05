//! Spec 037 acceptance (a): a recipe file parses into a [`Recipe`] with its
//! `required_extensions` and `params` — defaults included — intact.
//!
//! The fixture is read from disk rather than inlined, because `from_path` is the
//! entry point an operator actually reaches and the `[[instructions]]`
//! array-of-tables carrying an internally-tagged enum is the one part of the
//! shape that TOML could plausibly disagree with `serde_json` about. Parsing the
//! real file is what settles that.

use heddle_skills::{Recipe, RecipeStep, SkillError};
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plan_and_package.toml")
}

fn fixture() -> Recipe {
    Recipe::from_path(&fixture_path()).expect("the shipped fixture recipe must parse")
}

#[test]
fn a_recipe_file_parses_into_the_shape_the_design_names() {
    let recipe = fixture();

    // Design §4.4: `Recipe = { name, description, instructions,
    // required_extensions[], params[], prompt }` — every one of the six fields,
    // asserted here so the type cannot quietly drop one.
    assert_eq!(recipe.name, "plan-and-package");
    assert!(recipe
        .description
        .starts_with("Draft a plan from a spec file"));
    assert_eq!(
        recipe.prompt,
        "You are drafting a {{kind}} for the project named {{project}}.",
        "the recipe-level prompt is carried verbatim; substitution is the compiler's job, not the loader's"
    );
    assert_eq!(recipe.required_extensions, vec!["read_file"]);
    assert_eq!(recipe.params.len(), 2);
    assert_eq!(recipe.instructions.len(), 4);
}

#[test]
fn a_params_default_survives_the_round_trip() {
    let recipe = fixture();

    assert_eq!(recipe.params[0].name, "project");
    assert_eq!(
        recipe.params[0].default, None,
        "a param with no default is the case that makes an omitted value a compile error"
    );
    assert_eq!(recipe.params[1].name, "kind");
    assert_eq!(
        recipe.params[1].default.as_deref(),
        Some("plan"),
        "a default is the whole reason a caller may omit a param"
    );
}

#[test]
fn each_instruction_deserializes_to_the_step_kind_it_names() {
    let recipe = fixture();

    // `id()` mirrors `heddle_workflow::Node::id()`: every variant carries one,
    // and it is read off the same `match` that decides the variant, so a step
    // kind added later cannot be named in one place and forgotten in the other.
    assert_eq!(
        recipe
            .instructions
            .iter()
            .map(RecipeStep::id)
            .collect::<Vec<_>>(),
        vec!["plan", "read-spec", "package", "ship"],
        "instruction order is graph order; the loader preserves it"
    );

    match &recipe.instructions[0] {
        RecipeStep::Agent { prompt, .. } => assert_eq!(prompt, "Draft a plan."),
        other => panic!("instruction 0 declares kind = \"agent\", got {other:?}"),
    }
    match &recipe.instructions[1] {
        RecipeStep::Tool { tool, args, .. } => {
            assert_eq!(tool, "read_file");
            assert_eq!(args["path"], "spec.md");
        }
        other => panic!("instruction 1 declares kind = \"tool\", got {other:?}"),
    }
    match &recipe.instructions[2] {
        RecipeStep::Agent { prompt, .. } => assert_eq!(prompt, "Package it."),
        other => panic!("instruction 2 declares kind = \"agent\", got {other:?}"),
    }
    match &recipe.instructions[3] {
        RecipeStep::Approval { message, .. } => {
            assert_eq!(message, "Ready to ship the package for {{project}}?")
        }
        other => panic!("instruction 3 declares kind = \"approval\", got {other:?}"),
    }
}

#[test]
fn a_recipe_with_no_params_and_no_required_extensions_still_parses() {
    // Both fields are `#[serde(default)]`, so the simplest possible recipe — a
    // name, a description, a prompt and one step — is legal. A recipe author
    // should not have to write `params = []` to say "this takes none".
    let recipe = Recipe::from_toml_str(
        r#"
        name = "minimal"
        description = "The smallest legal recipe."
        prompt = "Do the thing."

        [[instructions]]
        kind = "agent"
        id = "only"
        prompt = "Go."
        "#,
    )
    .expect("params and required_extensions are both optional");

    assert!(recipe.params.is_empty());
    assert!(recipe.required_extensions.is_empty());
    assert_eq!(recipe.instructions.len(), 1);
}

#[test]
fn a_tool_step_with_no_arguments_parses_rather_than_being_rejected() {
    // `args` defaults to `Value::Null`, which is what a tool taking no
    // arguments serializes to anyway. Requiring `args = {}` would make the
    // common shape the awkward one.
    let recipe = Recipe::from_toml_str(
        r#"
        name = "argless"
        description = "A tool step that needs no arguments."
        prompt = "Do the thing."

        [[instructions]]
        kind = "tool"
        id = "ping"
        tool = "read_file"
        "#,
    )
    .expect("a tool step's args are optional");

    match &recipe.instructions[0] {
        RecipeStep::Tool { args, .. } => assert!(args.is_null()),
        other => panic!("expected a tool step, got {other:?}"),
    }
}

#[test]
fn malformed_toml_is_a_parse_error_that_names_where_it_came_from() {
    let err = Recipe::from_toml_str("name = \"broken\"\nthis is not toml")
        .expect_err("malformed TOML must not parse");

    assert!(
        matches!(err, SkillError::Parse { .. }),
        "malformed content is a Parse error, not an Io one: {err}"
    );
}

#[test]
fn an_unreadable_path_is_an_io_error_that_names_the_path() {
    let missing = fixture_path().with_file_name("no-such-recipe.toml");
    let err = Recipe::from_path(&missing).expect_err("a missing recipe file must refuse");

    assert!(
        matches!(err, SkillError::Io { .. }),
        "a missing file is an Io error, not a Parse one: {err}"
    );
    assert!(
        err.to_string().contains("no-such-recipe.toml"),
        "an operator who mistyped a path needs to see which path was tried, not just \
         \"the system cannot find the file specified\": {err}"
    );
}

#[test]
fn a_recipe_missing_a_required_field_refuses_rather_than_defaulting() {
    // `name`, `description`, `prompt` and `instructions` are deliberately not
    // `#[serde(default)]`: a recipe with no steps to run, or no name to record
    // on the chain, is a mistake worth naming at load time.
    let err = Recipe::from_toml_str("description = \"no name, no prompt, no steps\"")
        .expect_err("a recipe without its required fields must refuse");

    assert!(matches!(err, SkillError::Parse { .. }), "got {err}");
}
