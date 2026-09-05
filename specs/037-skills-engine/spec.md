# Feature Specification: `heddle-skills` — recipes compiled onto the native workflow engine

**Slice**: 037
**Status**: implemented
**Depends on**: [002-workflow-engine](../002-workflow-engine)

Design §4.4 names a skills/recipes engine that "loads BMAD, Spec-Kit, powerskills/superpowers and project-defined skills through Heddle's canonical skill/workflow contracts", and gives the canonical shape in one line:

```
Recipe = { name, description, instructions, required_extensions[], params[], prompt }
```

Nothing in the tree implements it. `crates/heddle-workflow` (slice 002) executes a `Workflow` value, but the only way to obtain one is to write it as Rust source, so an operator with a skill file has no path into the product at all. This slice adds `crates/heddle-skills`: a loader for that shape and a **compiler** from it to `heddle_workflow::Workflow`.

It adds **no execution capability**. Every run still happens in `WorkflowEngine::run`, unchanged.

---

## What this slice changes for a user

Before: to run a multi-step agent/tool sequence you edit Rust and rebuild.

After: you write a `.toml` file naming your steps, your required tools and your parameters, and it runs on the same engine — durable, resumable, and recorded on the same hash-chained Ledger — as any hand-built workflow. The chain a compiled recipe leaves is byte-for-byte the chain the hand-built equivalent leaves.

---

## Five things a reader must know up front

1. **This is a compiler, not an engine.** `compile` returns a `heddle_workflow::Workflow` and stops. Scheduling, resume, Ledger writing, approval semantics and the governed tool triple are `heddle-workflow`'s and `heddle-core`'s, reused as-is. A recipe-specific interpreter would have been a parallel core to keep in sync (Constitution IV) and would have had to re-earn every guarantee the Ledger already provides. `tests/end_to_end.rs` is deliberately a near-copy of `crates/heddle-workflow/tests/sequential.rs` — same engine, same doubles, same asserted chain — because "the graph came from a file" is the *only* difference the slice is entitled to make.

2. **`RecipeStep` has three variants, where `Node` has seven.** `WorkflowEngine::run` refuses `Subagent`, `Condition`, `Parallel` and `Loop` today (`crates/heddle-workflow/src/engine.rs:184-187`), before appending anything. A recipe author who could write `kind = "loop"` would be writing a recipe guaranteed to error at run. The narrowing is a consequence of the engine's current surface, not a judgement about the design's vocabulary — see `plan.md` D2.

3. **TOML, not YAML.** The workspace already parses TOML for exactly this job — an operator-authored config file, read and never written (`crates/heddle-gateway/src/route.rs:174-193`). Reusing it adds no crate to the tree. See `plan.md` D1.

4. **`{{param}}` substitution is a single scan, and an unresolvable placeholder refuses to compile.** Both "declared but unsupplied with no default" and "names no declared param at all" are `SkillError::MissingParam`, because the operator's symptom is the same either way — literal braces reaching a model. See `plan.md` D4.

5. **Nothing under `.claude/skills/`, `.agents/skills/`, `_bmad/` or `_bmad-output/` is read.** One made-up fixture recipe is the entire proof surface, and a registry or a CLI verb is named in "Out of scope" so it reads as scope rather than as a gap.

---

## Functional requirements

### FR-001 — `Recipe` is design §4.4's shape, deserializable from TOML

`Recipe { name, description, prompt, params, required_extensions, instructions }` — all six fields the design names. `params` and `required_extensions` are `#[serde(default)]`, so a recipe that takes neither does not have to say so. `name`, `description`, `prompt` and `instructions` are required: a recipe with no name is a run nobody can find again on the chain, and a missing `instructions` key is almost always mis-nested TOML.

### FR-002 — A recipe step is one of exactly three kinds

`RecipeStep::{Agent, Tool, Approval}`, tagged `#[serde(tag = "kind", rename_all = "snake_case")]` — the same attribute and therefore the same `kind = "agent"` vocabulary `heddle_workflow::Node` uses. Every variant carries an `id`, read back through `RecipeStep::id()` off the same `match` that decides the variant, mirroring `Node::id()`.

### FR-003 — Loading is two functions: one that touches disk, one that parses

`Recipe::from_path(&Path)` and `Recipe::from_toml_str(&str)`, mirroring `ProviderTable`'s split. A wrong path is `SkillError::Io` and a wrong file is `SkillError::Parse` — different errors because the remedies differ — and both name the path, because an operator who mistyped one needs to see which path was tried.

### FR-004 — `compile` emits exactly one `Node` per `RecipeStep`, in order

```rust
pub fn compile(
    recipe: &Recipe,
    params: &HashMap<String, String>,
    available_tools: &[&str],
) -> Result<Workflow>
```

| Recipe step | Compiled node |
|---|---|
| `Agent { id, prompt }` | `Node::Agent { id, prompt: Message::user_text(persona + "\n\n" + prompt) }` |
| `Tool { id, tool, args }` | `Node::Tool { id, call: ToolCall::new(tool, args) }` |
| `Approval { id, message }` | `Node::Approval { id, message }` |

The graph order is the `instructions` order. Nothing is reordered, merged or elided. `Workflow::new` is the constructor, so `params` stays `Value::Null`: the engine does not interpret it, every parameter is already baked into the graph by then, and a second copy nothing reads could only ever disagree with the nodes.

### FR-005 — The recipe-level `prompt` is a persona, folded into every agent step

A `Node::Agent` carries exactly one message and the engine has no separate system-prompt slot, so "who you are" would otherwise have to be copied into every step by hand. `compile` joins the recipe's `prompt` to each agent step's own prompt with a blank line — and when the recipe states no persona, the step's prompt is used alone rather than being given a leading blank line.

### FR-006 — Required extensions are checked before anything is built

Every name in `required_extensions` must appear in `available_tools`, or `compile` returns `SkillError::MissingExtension` naming both the missing extension and what *was* available. This happens before parameter resolution and before the first `Node` is constructed, so a recipe that cannot run is refused with nothing built and nothing to undo — as against a `Node::Tool` naming an absent tool, which would fail inside the gateway mid-run, after whatever preceded it had already taken effect.

`available_tools` is passed in rather than discovered: this crate holds no gateway and names no connector (Constitution IV). The caller that owns a `ToolGateway` already has the policy-filtered answer from `advertise()`, which is the list that actually matters — a tool the gateway knows but the policy forbids is not available to the run.

### FR-007 — Parameters resolve to a caller value, then a default, then a refusal

A caller's value wins over a declared `default`; a parameter with neither is only an error if something references it, so a recipe carrying an unused optional parameter is not refused for it. Parameters the recipe does not declare are ignored rather than refused — a caller passing one map for several recipes is doing something reasonable, and a typo in a caller's key still surfaces as the `MissingParam` for the declared parameter it failed to supply.

### FR-008 — Substitution covers prompts, approval messages, and the string leaves of tool arguments

`{{name}}` is the entire grammar: no conditionals, no loops, no filters, no nesting, and no templating dependency (Constitution VII). Arguments are substituted recursively on string leaves at any depth; non-string leaves and object *keys* are carried untouched. Substituting prompts but not arguments would have been a trap — a path is the most obvious place a parameterized value belongs, and left literal it would reach a filesystem tool as a `{{project}}` directory.

### FR-009 — An unresolvable or unterminated placeholder refuses to compile

`SkillError::MissingParam` for a placeholder with no value from either source; `SkillError::UnterminatedPlaceholder` for a `{{` that is never closed. Neither is passed through as literal text.

### FR-010 — The crate depends on `heddle-workflow` and `heddle-core`, and nothing else in the tree

Verified structurally, not asserted: `cargo tree -p heddle-skills` shows exactly those two workspace crates. No connector, no gateway, no CLI, no protocol adapter (Constitution IV).

---

## Success criteria

| # | Criterion | Proven by |
|---|---|---|
| SC-001 | A recipe file parses into `Recipe` with all six design-named fields intact | `a_recipe_file_parses_into_the_shape_the_design_names` |
| SC-002 | A `param`'s `default` survives the round trip; a param without one is distinguishable | `a_params_default_survives_the_round_trip` |
| SC-003 | Each `[[instructions]]` entry deserializes to the step kind it names, in order | `each_instruction_deserializes_to_the_step_kind_it_names` |
| SC-004 | A wrong path is `Io`, a wrong file is `Parse`, and both name the path | `an_unreadable_path_is_an_io_error_that_names_the_path`, `malformed_toml_is_a_parse_error_that_names_where_it_came_from` |
| SC-005 | Compiling emits one node per step, in recipe order, with the recipe's name | `compiling_a_recipe_emits_one_node_per_step_in_recipe_order` |
| SC-006 | Each step kind compiles to its node, with substitution applied | `an_agent_step_becomes_an_agent_node_carrying_the_persona_and_its_own_prompt`, `a_tool_step_becomes_a_tool_node_carrying_the_call_the_gateway_will_govern`, `an_approval_step_becomes_an_approval_node_with_its_message_substituted` |
| SC-007 | The extension check runs **before** anything is built | `the_extension_check_happens_before_any_substitution_or_node_is_built` |
| SC-008 | A missing extension refuses and says what is available | `a_missing_required_extension_refuses_to_compile_and_says_what_is_available` |
| SC-009 | An omitted param with no default, and an undeclared placeholder, both refuse | `an_omitted_param_with_no_default_refuses_to_compile`, `a_placeholder_naming_no_declared_param_refuses_to_compile` |
| SC-010 | Placeholders in tool arguments are substituted at any depth | `a_placeholder_inside_a_tool_steps_arguments_is_substituted_too` |
| SC-011 | A compiled recipe run on the **real** engine reaches `WorkflowExit::Completed` with the last node's outcome | `a_recipe_loaded_from_a_file_runs_every_node_in_order_and_reaches_its_final_result` |
| SC-012 | One `StepKind::WorkflowNode` step per node, in graph order | `every_compiled_node_lands_exactly_one_workflow_node_step_in_graph_order` |
| SC-013 | The chain a compiled recipe leaves is the chain a hand-built workflow leaves, and verifies | `a_compiled_recipe_leaves_the_same_chain_a_hand_built_workflow_would` |
| SC-014 | A compiled `approval` step is a real gate that stops the run | `the_full_recipe_stops_at_its_human_gate` |
| SC-015 | `heddle-skills` depends on `heddle-workflow` and `heddle-core` only | `cargo tree -p heddle-skills` |

The acceptance tests run against the **real** `WorkflowEngine` with a scripted `ModelClient` and a recording `ToolTransport` standing in for a provider and a connector, so none of them needs a running model or a real filesystem tool.

---

## Assumptions and residuals

- **`toml` handles an internally-tagged enum inside an array-of-tables.** Unproven before this slice: the workspace's only other `toml` consumer (`ProviderTable`) uses a flat `[[provider]]` table with no tagged enum in it. `[[instructions]]` with a `kind` tag was therefore sequenced as the first test written, so a `toml` limitation would have surfaced before the compiler or the e2e test existed. It works (`toml` 1.1.5).
- **The test doubles are re-derived, not shared.** `crates/heddle-workflow/tests/common/mod.rs` is compiled into that crate's own test binaries and is not an exported item, so there is nothing to depend on; its own header records that every workflow-adjacent crate re-derives this shape. Keeping them local also keeps `heddle-skills` free of a dev-dependency on another crate's test internals. The duplication is real and accepted.
- **`crates/heddle-workflow/Cargo.toml` hardcodes `version = "0.0.0"`** where the other eight crates use `version.workspace = true`. This crate follows the majority. The outlier is left alone as unrelated to this slice.
- **The design doc is still named `docs/superpowers/specs/2026-07-15-skein-design.md`** on disk, a leftover from the Skein→Heddle rename (commit `03ecf19`), and `README.md`'s "Master design" link to `...-heddle-design.md` is consequently already broken on `dev`. This slice cites the real on-disk path throughout rather than compounding the dangling link, and does not fix it — an unrelated rename is not this slice's scope.
- **`params` on the compiled `Workflow` stays `Value::Null`.** Stated as a decision (FR-004) rather than left as an accident of calling `Workflow::new`.

---

## Out of scope

- **Importing any existing skill file.** `.claude/skills/`, `.agents/skills/`, `_bmad/`, `_bmad-output/` are not read. One made-up fixture recipe is the whole proof surface.
- **A recipe registry, catalogue, or directory scan.** `from_path` takes one path. Where recipes live and how they are discovered is a follow-up slice's question.
- **A CLI verb** (e.g. `heddle skill run <path>`). A natural next slice; it would add `heddle-cli` argument-parsing surface to a slice whose contract is "the loader plus one working example".
- **`RecipeStep` variants for the four deferred `Node` kinds.** They arrive when the engine implements them, for the reason `node.rs`'s own header gives about `Node`.
- **A Goose-recipe import/export adapter.** §4.4 scopes external formats as adapters over the canonical shape, explicitly not as the canonical representation.
- **YAML.** TOML only (`plan.md` D1). A follow-up with a load-bearing need for YAML can add an adapter without touching `Recipe`.
- **A templating engine.** `{{name}}` and nothing else.
- **`TaskTracker` integration (FR-014/FR-015) and `LoopController`-gated `Loop` nodes (FR-017).** Named in spec 002 alongside FR-013b, but separate requirements with their own acceptance criteria and their own absent engine support.
- **The approve-and-resume round trip.** `the_full_recipe_stops_at_its_human_gate` proves the compiled gate stops the run in one `run` call. Recording a decision and resuming past it is already proven by `heddle-workflow`'s own suite for a hand-built `Approval` node, and compiling changes nothing about it.
