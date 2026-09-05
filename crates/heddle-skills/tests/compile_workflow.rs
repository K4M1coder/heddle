//! Spec 037 acceptance (b): compiling a [`Recipe`] produces the node graph the
//! recipe describes — one [`Node`] per step, in order, with every
//! `{{placeholder}}` resolved.
//!
//! These assertions are equalities against whole `Node` values rather than
//! field-by-field checks, which `Node`'s own `PartialEq` derive
//! (`crates/heddle-workflow/src/node.rs:20`) makes possible. The point is that
//! the compiled graph is pinned *exactly*: the persona/step-prompt join format
//! is a decision an operator's prompts depend on, and it should not be able to
//! drift without a test going red.

use heddle_core::{Message, ToolCall};
use heddle_skills::{compile, Recipe, SkillError};
use heddle_workflow::Node;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

fn fixture() -> Recipe {
    Recipe::from_path(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plan_and_package.toml"),
    )
    .expect("the shipped fixture recipe must parse")
}

/// Both of the fixture's params, supplied. `kind` has a default, so supplying it
/// also covers "a caller's value wins over a default".
fn both_params() -> HashMap<String, String> {
    HashMap::from([
        ("project".to_string(), "Heddle".to_string()),
        ("kind".to_string(), "plan".to_string()),
    ])
}

/// Only the param that has no default. What the caller may legally omit is the
/// other one.
fn required_param_only() -> HashMap<String, String> {
    HashMap::from([("project".to_string(), "Heddle".to_string())])
}

#[test]
fn compiling_a_recipe_emits_one_node_per_step_in_recipe_order() {
    let workflow = compile(&fixture(), &both_params(), &["read_file"])
        .expect("a fully parameterized recipe whose tools are available must compile");

    assert_eq!(
        workflow.name, "plan-and-package",
        "the recipe's name becomes the workflow's, which is what a reader of the chain sees"
    );
    assert_eq!(workflow.graph.len(), 4);
    assert_eq!(
        workflow.graph.iter().map(Node::id).collect::<Vec<_>>(),
        vec!["plan", "read-spec", "package", "ship"],
        "step order is graph order; the compiler reorders nothing"
    );
}

#[test]
fn an_agent_step_becomes_an_agent_node_carrying_the_persona_and_its_own_prompt() {
    let workflow = compile(&fixture(), &both_params(), &["read_file"]).expect("must compile");

    assert_eq!(
        workflow.graph[0],
        Node::Agent {
            id: "plan".into(),
            prompt: Message::user_text(
                "You are drafting a plan for the project named Heddle.\n\nDraft a plan."
            ),
        },
        "the recipe-level prompt is the persona, joined to the step's own prompt by a blank line"
    );
    assert_eq!(
        workflow.graph[2],
        Node::Agent {
            id: "package".into(),
            prompt: Message::user_text(
                "You are drafting a plan for the project named Heddle.\n\nPackage it."
            ),
        },
        "every agent step gets the persona, not just the first"
    );
}

#[test]
fn a_tool_step_becomes_a_tool_node_carrying_the_call_the_gateway_will_govern() {
    let workflow = compile(&fixture(), &both_params(), &["read_file"]).expect("must compile");

    assert_eq!(
        workflow.graph[1],
        Node::Tool {
            id: "read-spec".into(),
            call: ToolCall::new("read_file", json!({ "path": "spec.md" })),
        },
        "a tool step compiles to the same ToolCall a hand-built workflow would carry"
    );
}

#[test]
fn an_approval_step_becomes_an_approval_node_with_its_message_substituted() {
    let workflow = compile(&fixture(), &both_params(), &["read_file"]).expect("must compile");

    assert_eq!(
        workflow.graph[3],
        Node::Approval {
            id: "ship".into(),
            message: "Ready to ship the package for Heddle?".into(),
        },
        "the human reading an approval gate sees resolved values, not the recipe's braces"
    );
}

#[test]
fn a_params_default_is_used_when_the_caller_omits_it() {
    let workflow =
        compile(&fixture(), &required_param_only(), &["read_file"]).expect("kind has a default");

    match &workflow.graph[0] {
        Node::Agent { prompt, .. } => assert_eq!(
            prompt.text(),
            "You are drafting a plan for the project named Heddle.\n\nDraft a plan.",
            "the default for `kind` is substituted exactly as a supplied value would be"
        ),
        other => panic!("expected an agent node, got {other:?}"),
    }
}

#[test]
fn a_callers_value_wins_over_a_default() {
    let params = HashMap::from([
        ("project".to_string(), "Heddle".to_string()),
        ("kind".to_string(), "packaging checklist".to_string()),
    ]);
    let workflow = compile(&fixture(), &params, &["read_file"]).expect("must compile");

    match &workflow.graph[0] {
        Node::Agent { prompt, .. } => assert!(
            prompt.text().starts_with(
                "You are drafting a packaging checklist for the project named Heddle."
            ),
            "a default is a fallback, not a fixed value: {}",
            prompt.text()
        ),
        other => panic!("expected an agent node, got {other:?}"),
    }
}

#[test]
fn workflow_params_stay_opaque_rather_than_restating_what_is_already_substituted() {
    let workflow = compile(&fixture(), &both_params(), &["read_file"]).expect("must compile");

    // `Workflow::params` is documented as carried opaquely and uninterpreted by
    // the engine (`crates/heddle-workflow/src/lib.rs:36-38`). By the time a
    // recipe is compiled, every parameter is already baked into the graph, so
    // repeating them here would create a second, non-authoritative copy that
    // nothing reads and that could disagree with the nodes.
    assert!(workflow.params.is_null());
}

#[test]
fn an_omitted_param_with_no_default_refuses_to_compile() {
    let err = compile(&fixture(), &HashMap::new(), &["read_file"])
        .expect_err("`project` has no default, so omitting it cannot be papered over");

    match err {
        SkillError::MissingParam { recipe, param } => {
            assert_eq!(recipe, "plan-and-package");
            assert_eq!(param, "project");
        }
        other => panic!("expected MissingParam, got {other}"),
    }
}

#[test]
fn a_placeholder_naming_no_declared_param_refuses_to_compile() {
    // The other half of `MissingParam`: not "declared but unsupplied" but
    // "never declared at all" — a typo. Left as literal text it would reach a
    // model as `{{projct}}`, and the only symptom would be a confused answer.
    // See `plan.md` D4.
    let recipe = Recipe::from_toml_str(
        r#"
        name = "typo"
        description = "A recipe with a misspelled placeholder."
        prompt = "Work on {{projct}}."

        [[params]]
        name = "project"
        default = "Heddle"

        [[instructions]]
        kind = "agent"
        id = "only"
        prompt = "Go."
        "#,
    )
    .expect("the recipe itself is valid TOML; the mistake is semantic");

    match compile(&recipe, &HashMap::new(), &[]).expect_err("an undeclared placeholder refuses") {
        SkillError::MissingParam { param, .. } => assert_eq!(param, "projct"),
        other => panic!("expected MissingParam, got {other}"),
    }
}

#[test]
fn a_missing_required_extension_refuses_to_compile_and_says_what_is_available() {
    let err = compile(&fixture(), &both_params(), &[])
        .expect_err("the recipe requires read_file and the gateway advertises nothing");

    match err {
        SkillError::MissingExtension {
            recipe,
            extension,
            available,
        } => {
            assert_eq!(recipe, "plan-and-package");
            assert_eq!(extension, "read_file");
            assert_eq!(
                available, "none",
                "refusing without saying what may be used instead leaves the operator guessing"
            );
        }
        other => panic!("expected MissingExtension, got {other}"),
    }
}

#[test]
fn the_extension_check_happens_before_any_substitution_or_node_is_built() {
    // `plan.md` D3 as a tested property rather than a comment. This recipe is
    // broken twice over: it requires a tool the gateway does not advertise
    // *and* references a param with no value. If the extension check did not
    // run first, the param failure would be reported instead — so the variant
    // of the returned error is evidence of the order the checks ran in.
    let recipe = Recipe::from_toml_str(
        r#"
        name = "doubly-broken"
        description = "Requires an absent tool and an unsupplied param."
        prompt = "Work on {{project}}."
        required_extensions = ["write_file"]

        [[params]]
        name = "project"

        [[instructions]]
        kind = "agent"
        id = "only"
        prompt = "Go."
        "#,
    )
    .expect("valid TOML");

    match compile(&recipe, &HashMap::new(), &["read_file"]).expect_err("both checks would refuse") {
        SkillError::MissingExtension { extension, .. } => assert_eq!(
            extension, "write_file",
            "a missing connector is reported before anything is built, so nothing was built"
        ),
        other => panic!("the extension check must run first; got {other}"),
    }
}

#[test]
fn a_duplicated_required_extension_is_not_a_failure_when_it_is_available() {
    // Naming the same tool twice is redundant, not wrong: the check asks "is
    // every named tool available", and asking twice about an available tool has
    // the same answer. It must not be mistaken for a conflict.
    let recipe = Recipe::from_toml_str(
        r#"
        name = "redundant"
        description = "Names the same required extension twice."
        prompt = "Go."
        required_extensions = ["read_file", "read_file"]
        instructions = []
        "#,
    )
    .expect("valid TOML");

    assert!(compile(&recipe, &HashMap::new(), &["read_file"]).is_ok());
}

#[test]
fn a_recipe_with_no_steps_compiles_to_an_empty_graph_rather_than_refusing() {
    // The engine's own `run` walks the graph and falls through with no
    // iterations, returning `Completed`
    // (`crates/heddle-workflow/src/engine.rs:194-197`). A recipe that declares
    // no steps therefore has a well-defined meaning — do nothing, successfully
    // — and refusing to compile it would be this layer inventing a restriction
    // the engine does not have.
    let recipe = Recipe::from_toml_str(
        r#"
        name = "empty"
        description = "A recipe with no steps."
        prompt = "Go."
        instructions = []
        "#,
    )
    .expect("valid TOML");

    let workflow = compile(&recipe, &HashMap::new(), &[]).expect("an empty recipe is legal");
    assert!(workflow.graph.is_empty());
}

#[test]
fn a_placeholder_inside_a_tool_steps_arguments_is_substituted_too() {
    // Substituting prompts but not arguments would be a trap: a path is the one
    // place a parameterized value most obviously belongs, and left unsubstituted
    // it would reach a filesystem tool as a literal `{{project}}` directory.
    // See `plan.md` D4.
    let recipe = Recipe::from_toml_str(
        r#"
        name = "parameterized-args"
        description = "A tool step whose arguments carry a placeholder."
        prompt = "Go."

        [[params]]
        name = "project"

        [[instructions]]
        kind = "tool"
        id = "read"
        tool = "read_file"
        args = { path = "{{project}}/spec.md", depth = 2, nested = { also = "{{project}}" } }
        "#,
    )
    .expect("valid TOML");

    let params = HashMap::from([("project".to_string(), "heddle".to_string())]);
    let workflow = compile(&recipe, &params, &["read_file"]).expect("must compile");

    assert_eq!(
        workflow.graph[0],
        Node::Tool {
            id: "read".into(),
            call: ToolCall::new(
                "read_file",
                json!({ "path": "heddle/spec.md", "depth": 2, "nested": { "also": "heddle" } })
            ),
        },
        "string leaves are substituted at any depth; non-string leaves are carried untouched"
    );
}

#[test]
fn an_empty_recipe_prompt_leaves_an_agent_step_with_only_its_own_prompt() {
    // A recipe with no persona should not produce a message that opens with a
    // blank line. The join is between two things, and with one of them absent
    // there is nothing to join.
    let recipe = Recipe::from_toml_str(
        r#"
        name = "no-persona"
        description = "A recipe that states no persona."
        prompt = ""

        [[instructions]]
        kind = "agent"
        id = "only"
        prompt = "Just this."
        "#,
    )
    .expect("valid TOML");

    let workflow = compile(&recipe, &HashMap::new(), &[]).expect("must compile");
    assert_eq!(
        workflow.graph[0],
        Node::Agent {
            id: "only".into(),
            prompt: Message::user_text("Just this."),
        }
    );
}

#[test]
fn an_unterminated_placeholder_refuses_rather_than_being_carried_through() {
    let recipe = Recipe::from_toml_str(
        r#"
        name = "unbalanced"
        description = "A placeholder that is never closed."
        prompt = "Work on {{project."

        [[params]]
        name = "project"
        default = "Heddle"

        [[instructions]]
        kind = "agent"
        id = "only"
        prompt = "Go."
        "#,
    )
    .expect("valid TOML");

    match compile(&recipe, &HashMap::new(), &[]).expect_err("an unbalanced brace refuses") {
        SkillError::UnterminatedPlaceholder { recipe, .. } => assert_eq!(recipe, "unbalanced"),
        other => panic!("expected UnterminatedPlaceholder, got {other}"),
    }
}
