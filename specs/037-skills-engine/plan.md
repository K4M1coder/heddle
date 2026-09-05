# Implementation Plan: slice 037 — `heddle-skills`, recipes compiled onto the native engine

## Problem

Design §4.12 states a commitment: *"Goose recipes + BMAD/Spec-Kit flows = workflows (a recipe is a declarative `Workflow`)"*. Spec 002 turns it into a requirement, FR-013b: *"Goose recipes and BMAD/Spec-Kit flows MUST be executable as workflows."* The engine that would execute them has been merged since slice 002. The thing that turns a recipe into something it can execute does not exist, so design §8's Phase-1 axis 1d has no implementation anywhere in the tree.

The risk in building one is not that it is hard. It is that it quietly becomes a second engine. A recipe runner that did its own sequencing, kept its own record of what had run, or decided its own approval semantics would be a parallel core to keep in sync (Constitution IV) — and every guarantee the Ledger already provides (durability, resume, replay, the governed tool triple) would have to be re-earned in a second place, where the first mistake would be silent. So the design question for this slice is not "how do we run a recipe" but **"how little may this crate do"**.

The answer: parse, validate, and emit `Vec<Node>`. Nothing else.

## What was verified before planning

Everything the source plan cited was checked against the working tree before a line was written; fifteen of fifteen patterns matched. Three things worth recording because they changed a decision:

| Checked | Finding | Consequence |
|---|---|---|
| `crates/*/Cargo.toml` version convention | Eight crates use `version.workspace = true`; `heddle-workflow` alone hardcodes `version = "0.0.0"` | The new manifest follows the majority. Had it been copied from the nearest neighbour it would have followed the outlier. |
| `toml`'s handling of `#[serde(tag = "kind")]` inside `[[array-of-tables]]` | Unproven in this workspace — `ProviderTable` is the only `toml` consumer and its `[[provider]]` table holds no tagged enum | The parse test was sequenced **first**, so a `toml` limitation would have surfaced before the compiler or the e2e test existed. It works (`toml` 1.1.5). |
| `crates/heddle-workflow/tests/common/mod.rs` visibility | `tests/`-only, not an exported item, so not importable from another crate | The e2e test re-derives the doubles locally. The duplication is stated in `spec.md`'s residuals rather than discovered by a reviewer. |

Also checked and left alone: `README.md:22`'s "Master design" link points at `docs/superpowers/specs/2026-07-15-heddle-design.md`, which does not exist — the file is still `...-skein-design.md` after the rename in commit `03ecf19`. Pre-existing on `dev`, unrelated to this slice, and cited by its real on-disk name throughout these documents so the new docs are not built on the same dangling link.

## Approach

### D1 — TOML, not YAML

The workspace already depends on `toml` (root `Cargo.toml:77`, `default-features = false, features = ["parse", "serde", "std"]`) for precisely this job: reading a file an operator hand-authored, never writing one. `crates/heddle-gateway/src/route.rs:174-193` is the existing consumer and the pattern this crate's loader mirrors.

Choosing TOML therefore adds **no crate to the dependency tree** and reuses a pattern the workspace has already settled once. YAML would add a parser for a need already solved, and would bring indentation-sensitivity and anchors to a format whose whole audience is humans hand-converting BMAD and Spec-Kit flows. `Recipe`'s shape is flat enough that TOML's array-of-tables maps onto `instructions` cleanly, which the parse test confirms.

Rejected alternative: JSON. It is already in the tree via `serde_json`, but a config file with no comments is the wrong artifact for something whose steps need explaining — and the fixture recipe's own header comment is the demonstration.

### D2 — `RecipeStep` is a three-variant authoring vocabulary, not a re-use of `Node`

Two alternatives were available and both were rejected.

**Deserialize `Node` directly**, making a recipe a serialized `Workflow`. Rejected because `Node::Agent` carries a `Message`, so an author would have to hand-write `prompt = { role = "user", parts = [{ type = "text", text = "..." }] }` in TOML to say "ask the model this" — a wire format's shape imposed on a human for zero benefit. The recipe-level persona of D5 would also be impossible to express, since a `Message` has no slot to fold it into.

**Mirror all seven `Node` kinds.** Rejected because `WorkflowEngine::run` refuses `Subagent`, `Condition`, `Parallel` and `Loop` (`crates/heddle-workflow/src/engine.rs:184-187`) before appending anything. A recipe author who could write `kind = "loop"` would be writing a recipe guaranteed to error at run, and a compiler that emitted one would be building a graph it knows cannot execute.

So `RecipeStep` has exactly the three variants the engine executes. Note that this is the *opposite* of the reasoning `node.rs`'s own header gives for `Node` having all seven from the first slice — and deliberately so. `Node` is the serialized shape of a stored `Workflow`, so growing it variant-by-variant would force migration of every workflow an earlier slice wrote. `RecipeStep` has no such stored history to protect: recipes are read from source files that their authors can edit, so it can grow when the engine grows.

What it *does* borrow is `Node`'s `#[serde(tag = "kind", rename_all = "snake_case")]`, so an author writes `kind = "agent"` in both places and the two vocabularies read alike even though the types differ.

### D3 — Every required extension is checked before a single node is built

`compile` validates `required_extensions` against `available_tools` first, then resolves parameters, then constructs nodes. The order is the decision.

The alternative — build the graph and let the run discover the missing tool — fails inside `ToolGateway::call_captured`, mid-run, *after* every node preceding the tool step has already executed and taken effect. Refusing at compile time leaves nothing built and nothing to undo, and the error can say what the gateway *does* advertise, which a failure deep in the gateway cannot.

This is tested as an ordering property rather than asserted as a comment: `the_extension_check_happens_before_any_substitution_or_node_is_built` feeds `compile` a recipe broken *twice over* — an absent tool and an unsupplied parameter — and asserts which error comes back. The variant is the evidence of the order.

`available_tools` is a caller-supplied `&[&str]`, not something this crate discovers. It holds no gateway and must name no connector (Constitution IV), and the caller that owns a `ToolGateway` already has the *policy-filtered* answer from `advertise()` — which is the list that matters, since a tool the gateway knows but the policy forbids is not available to the run.

### D4 — Substitution is a single scan of one grammar, and it refuses what it cannot resolve

`{{name}}` is the entire grammar. No conditionals, no loops, no filters, no nesting, and no `handlebars`/`tera` dependency (Constitution VII).

The source plan proposed a `str::replace` per parameter. **This implementation uses a single forward scan instead**, and the reason is the error case rather than efficiency: `replace` cannot distinguish a placeholder that names no declared parameter from one it has already substituted. It would leave a typo — `{{projct}}` — in the prompt as literal braces, and the only symptom would be a confused model answer. A scan sees every placeholder the author wrote and can refuse the ones it cannot resolve. It is still not a template engine; the grammar is unchanged.

Two questions the source plan left open, resolved here:

**An undeclared placeholder is `SkillError::MissingParam`**, the same variant as a declared-but-unsupplied parameter with no default. One variant for both because the operator's symptom is identical — literal braces reaching a model — and only an error names the culprit. An unterminated `{{` gets its own variant, `UnterminatedPlaceholder`, because the remedy is different: one is a wrong name, the other a missing brace.

**Tool arguments *are* substituted**, recursively, on string leaves at any depth. The source plan offered "or, simpler, do not substitute inside args at all". Rejected: a path is the single most obvious place a parameterized value belongs, and it is the one place a recipe can put one. The asymmetry — prompts expand, arguments do not — would be a trap that hands a filesystem tool a literal `{{project}}` directory. Object *keys* are left alone: parameterizing an argument's name would be reshaping the tool's schema rather than filling it in, which is not what `{{param}}` promises.

Parameters the recipe does not declare are ignored rather than refused, so one caller map can serve several recipes; a typo in a caller's key still surfaces as the `MissingParam` for the declared parameter it failed to supply.

### D5 — The recipe-level `prompt` is a persona folded into every agent step

Design §4.4 lists `prompt` alongside `instructions` without saying how they relate. `Node::Agent` carries exactly one `Message` and the engine has no separate system-prompt slot, so an author who wanted a consistent persona would otherwise have to paste the same paragraph into every step.

`compile` joins them: `persona + "\n\n" + step prompt`. The join format is pinned by an equality assertion on a whole `Node::Agent` value, because operators' prompts will depend on it and it should not be able to drift silently. When a recipe states no persona the step's prompt is used alone — a message whose first instruction to the model is a blank line is a bug, not a formatting detail.

### D6 — The compiled `Workflow`'s `params` stays `Value::Null`

`Workflow::new` is the constructor, rather than a struct literal, and it sets `params: Value::Null`. Kept, as a decision: the engine documents `params` as carried opaquely and uninterpreted (`crates/heddle-workflow/src/lib.rs:36-38`), and by the time a recipe is compiled every parameter is already baked into the graph. Echoing them there would create a second, non-authoritative copy that nothing reads and that could disagree with the nodes. Asserted, so it reads as intent.

### D7 — The e2e test is a near-copy of `sequential.rs`, and the resemblance is the argument

`tests/end_to_end.rs` uses the same engine, the same double shapes, the same scripted turns and the same expected `Vec<StepKind>` as `crates/heddle-workflow/tests/sequential.rs`. The only difference is that its `Workflow` came out of `compile` instead of a `Workflow::new(...)` literal.

That is what makes the claim checkable. If both files pass, compiling a recipe demonstrably changes nothing about how a workflow behaves — which is what "a recipe is a declarative `Workflow`" has to mean to be worth stating. A bespoke e2e test asserting bespoke things would have proven the compiler does *something*, not that it does *nothing extra*.

The fixture's fourth step is an `approval`, and the engine returns `AwaitingApproval` at an undecided gate. The three `Completed`-shape tests therefore truncate the recipe to its first three steps, reproducing `sequential.rs` exactly. One further test runs the full four steps and asserts the gate stops the run — one `run` call, not an approve-and-resume round trip, since resuming past a decided gate is already proven by `heddle-workflow`'s own suite and compiling changes nothing about it.

### D8 — The test doubles are re-derived locally

`crates/heddle-workflow/tests/common/mod.rs` is compiled into that crate's test binaries and is not an exported item, so there is nothing to depend on. Its own header records that every workflow-adjacent crate in this workspace re-derives this shape, and that it lives in `tests/common/` there only because that crate has four test files needing it.

The alternative — a `dev-dependency` on `heddle-workflow`'s test internals, or promoting them to a published test-support module — would couple this crate to another crate's fixtures for one test file (Constitution IV). The duplication is real, is roughly 100 lines, and is stated in `spec.md`'s residuals rather than left for a reviewer to notice.

## Steps

1. `crates/heddle-skills/Cargo.toml` — `version.workspace = true` per the verified majority; deps on `heddle-workflow`, `heddle-core`, `serde`, `serde_json`, `toml`, `thiserror`, all workspace-inherited.
2. `crates/heddle-skills/tests/fixtures/plan_and_package.toml` — the one made-up Spec-Kit-style recipe, written before any source so the rest of the slice has a concrete target.
3. `tests/recipe_parsing.rs` red → `src/recipe.rs`, `src/error.rs`, `src/loader.rs`, `src/lib.rs` green.
4. `tests/compile_workflow.rs` red → `src/compile.rs` green.
5. `tests/end_to_end.rs` with local doubles → green against the real engine.
6. `specs/037-skills-engine/{spec,plan,tasks}.md`.
7. `README.md` "Current status".
8. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
