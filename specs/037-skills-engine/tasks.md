# Tasks: `heddle-skills` — recipes compiled onto the native engine (slice 037)

## Constitution Check

| Principle | How this slice satisfies it |
|---|---|
| **I — Headless core, CLI as reference, UI as a thin layer** | The capability is a library API in the headless core: `Recipe::from_path` and `compile`. No UI surface, and — stated as scope, not as a gap — no CLI verb yet either, so the slice adds no surface the API does not expose. Whatever CLI verb lands next calls exactly these two functions. |
| **II — Local-first, silo isolation** | Nothing here opens a socket or can. The crate's whole dependency tree is `heddle-workflow`, `heddle-core`, `serde`, `serde_json`, `toml` (parse-only, no writer) and `thiserror`; no HTTP client, no TLS backend, no transport. A recipe is read from the local filesystem and compiled in-process. The run it produces is governed by whatever gateway and policy the caller already had, in the silo the caller already opened. |
| **III — Test-First** | Every increment was observed red before it was made green — evidence below. `tests/recipe_parsing.rs` was written before `recipe.rs`/`error.rs`/`loader.rs` existed; `tests/compile_workflow.rs` before `compile.rs`. The fixture recipe was authored before any source file, so the tests had a concrete target rather than being retrofitted to whatever the code turned out to do. Both interface boundaries have a double behind them: the model is a `ScriptedModel`, the connector a `RecordingTransport`. |
| **IV — Inverted coupling & explicit boundaries** | Checked structurally, not asserted: `cargo tree -p heddle-skills` shows exactly two workspace crates, `heddle-core` and `heddle-workflow`. Nothing in `src/` names a connector, a provider, a protocol or a transport. `compile` takes the available tool names as a caller-supplied `&[&str]` precisely so it need not hold a gateway to ask (`plan.md` D3). The dependency direction is one-way — `heddle-workflow` does not know this crate exists. |
| **V — Traceability & reversibility** | The slice adds **no record of its own**, which is its central claim. Every step of a compiled recipe is written to the Ledger by `WorkflowEngine::run` and `ToolGateway::call_captured`, unchanged and unwrapped. `a_compiled_recipe_leaves_the_same_chain_a_hand_built_workflow_would` asserts the resulting `Vec<StepKind>` equals `sequential.rs`'s exactly — including the governed tool triple — and then calls `verify_chain`. A recipe cannot become a way to run something the chain does not hold. |
| **VI — Security & secrets by reference** | A recipe is external content, and it is treated as data: it names a tool and supplies arguments, and the *gateway's* policy still decides whether that call happens. Compiling cannot widen what a run may do — `read_file` in a recipe is refused at compile time unless the caller's own policy-filtered `advertise()` already allowed it (FR-006), so a recipe may narrow the available surface and never widen it. No secret is read, resolved, or stored here; `{{param}}` substitution is textual over caller-supplied values, and a recipe has no syntax for a `SecretRef`. Deny-by-default holds: an absent required extension refuses the compile rather than proceeding hopefully. |
| **VII — Neutrality & reuse (YAGNI)** | This is the principle the slice is mostly about. The engine is reused rather than rewritten — the crate is ~300 lines of compiler and no scheduler, no resume logic, no Ledger writing. The TOML parser is the one the workspace already had (`plan.md` D1), adding no crate to the tree. No templating engine for a `{{name}}` grammar (D4). No registry, no catalogue, no CLI verb, no YAML, no Goose adapter — each named in `spec.md`'s "Out of scope" as a thing with no demonstrated need yet. `RecipeStep` has three variants and not seven, because four of them have nothing to compile to (D2). |
| **VIII — Loop discipline** | This slice hosts no loop. `compile` is a single pass over a finite `Vec<RecipeStep>` and cannot iterate; substitution is a single forward scan that consumes its input monotonically and so terminates on the string's length. `{{name}}` deliberately does not nest, which is what keeps expansion non-recursive — there is no fixpoint to reach and no expansion budget to enforce. Termination, budgets and no-progress detection remain `heddle-core`'s `LoopController`'s, externally enforced, where they were. A recipe cannot ask for an unbounded loop: the `Loop` node kind has no `RecipeStep` to reach it, which is the same conclusion FR-017 reaches from the other direction. |
| **Cross-platform** | No OS-specific code and no `#[cfg]`. Paths are `std::path::Path` throughout, and the fixture is located from `CARGO_MANIFEST_DIR` rather than a relative path, so the tests do not depend on the working directory a runner happens to use. The join format `"\n\n"` is explicit rather than platform-dependent. Covered by the tri-OS matrix as an ordinary workspace member (`Cargo.toml:8`'s `crates/*` glob picks it up with no root edit). |
| **English-only content** | All source, comments, doc comments, tests, fixtures and specs are English. |

**Complexity Tracking**: no departure from a principle. Two costs worth naming, both accepted deliberately and recorded in `spec.md`'s residuals rather than left implicit:

1. **~100 lines of duplicated test doubles** (`plan.md` D8). `heddle-workflow`'s `tests/common/mod.rs` is not an exported item and cannot be imported. The alternative — a dev-dependency on another crate's test internals for the sake of one test file — trades a real coupling for a notional saving, against Constitution IV.
2. **A recipe vocabulary narrower than the design's `Node` list** (`plan.md` D2). This is a consequence of the engine's current surface, not a decision to diverge from design §4.12, and it reverses on its own when the deferred node kinds land.

---

## Tasks

| # | Task | Status |
|---|---|---|
| T1 | `crates/heddle-skills/Cargo.toml` — workspace-inherited version per the verified majority | done |
| T2 | `tests/fixtures/plan_and_package.toml` — the one made-up Spec-Kit-style recipe, authored first | done |
| T3 | `tests/recipe_parsing.rs` observed red, then `src/{recipe,error,loader,lib}.rs` green — 8 tests | done |
| T4 | `tests/compile_workflow.rs` observed red, then `src/compile.rs` green — 16 tests | done |
| T5 | `tests/end_to_end.rs` with local `ScriptedModel`/`RecordingTransport` doubles, against the real `WorkflowEngine` — 7 tests | done |
| T6 | `specs/037-skills-engine/{spec,plan,tasks}.md` | done |
| T7 | `README.md` "Current status" | done |
| T8 | Full local validation (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`) | done |

---

## Observed red

**T3** — `tests/recipe_parsing.rs` before any source file existed:

```
error[E0432]: unresolved import `heddle_skills`
  --> crates\heddle-skills\tests\recipe_parsing.rs:10:5
   |
10 | use heddle_skills::{Recipe, RecipeStep, SkillError};
   |     ^^^^^^^^^^^^^ use of unresolved module or unlinked crate `heddle_skills`
error: could not compile `heddle-skills` (test "recipe_parsing") due to 1 previous error
```

and green after `recipe.rs`, `error.rs`, `loader.rs` and `lib.rs`:

```
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**T4** — `tests/compile_workflow.rs` before `compile.rs` existed:

```
error[E0432]: unresolved import `heddle_skills::compile`
  --> crates\heddle-skills\tests\compile_workflow.rs:13:21
   |
13 | use heddle_skills::{compile, Recipe, SkillError};
   |                     ^^^^^^^ no `compile` in the root
error: could not compile `heddle-skills` (test "compile_workflow") due to 1 previous error
```

and green after `compile.rs`:

```
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

To keep that red honest rather than staged, `lib.rs` was written first *without* its `pub mod compile;` line and the line added as part of T4 — so the failing import was the real absence of the module, not a module deliberately hidden from a test that could otherwise have seen it.

---

## Finding: the parse test had to come first, and it retired the slice's one real risk

`toml`'s handling of an internally-tagged enum (`#[serde(tag = "kind")]`) inside an array-of-tables (`[[instructions]]`) was the one part of the design nothing in this workspace had exercised. The only existing `toml` consumer, `ProviderTable`, reads a flat `[[provider]]` table with no tagged enum in it, and TOML's table semantics are not JSON's.

Had it not worked, the whole recipe shape would have needed rethinking — `instructions` is the field the compiler is entirely built around. Sequencing that parse as the *first* test written meant the answer arrived before `compile.rs` or the e2e test existed, at a cost of one test file rather than a rewrite. It works on `toml` 1.1.5.

This is the ordinary payoff of Principle III, worth recording because the risk was identified in advance and priced into the task order rather than discovered.

---

## Finding: "validates before it builds" needed an ordering test, not a comment

`plan.md` D3's claim is that a missing extension is refused *before* any node is constructed. A comment saying so proves nothing, and the function's return type cannot express it — an `Err` looks the same whichever check produced it.

The test that does prove it feeds `compile` a recipe broken **twice over**: it requires a tool the caller does not advertise *and* references a parameter with no supplied value and no default. Either fault alone would refuse. Because both are present, the *variant* of the returned error is evidence of which check ran first — `MissingExtension` can only come back if the extension check preceded parameter resolution.

The same shape is worth reaching for whenever a guarantee is about order rather than outcome: give the function two ways to fail and let it tell you which one it found.

---

## Deviation from the source plan

| Item | Planned | Done | Why |
|---|---|---|---|
| `{{param}}` substitution mechanism | "plain `str::replace` over literal `{{name}}` tokens" | a single forward scan of the same grammar | `replace` cannot distinguish a placeholder naming no declared parameter from one it has already substituted, so a typo would survive into a prompt as literal braces with a confused model answer as the only symptom. The scan is what makes the source plan's own open question ("is an undeclared placeholder an error or literal text?") answerable as *error*. Same grammar, still no template engine. `plan.md` D4. |
| Substitution inside a tool step's `args` | offered as either "string leaves, recursively" or "not at all — simpler and sufficient" | string leaves, recursively, at any depth | The asymmetry would be a trap: a path is the one place a recipe can put a parameterized value, and left literal it reaches a filesystem tool as a `{{project}}` directory. Object keys are still untouched. `plan.md` D4. |
| `SkillError` variants | four (`Io`, `Parse`, `MissingExtension`, `MissingParam`) | five, plus `available` on `MissingExtension` | `UnterminatedPlaceholder` was added because an unbalanced `{{` has a different remedy from a wrong name, and reporting it as `MissingParam` would name a "parameter" the author never wrote. `MissingExtension` carries what *was* available so the refusal answers "then what may I use?" in the same breath — `ProviderTable::get`'s own house style for an unknown-name refusal. |
| e2e coverage of the fixture's `approval` step | explicitly deferred: "compile only the first three instructions; the fuller proof is a natural but separate follow-up" | the three-step `Completed` proof **plus** one test that the full four-step recipe returns `AwaitingApproval { node_id: "ship" }` | Kept inside the plan's stated risk boundary. What the plan warned against was the two-call approve-and-resume round trip inflating the slice; this is one `run` call and no new machinery, and without it the compile test's `Node::Approval` assertion is only structural. The approve-and-resume proof remains out of scope (`spec.md`, "Out of scope"). |
| Spec folder slug | `037-skills-recipes-engine` (`plan-context.md`) / `037-skills-engine` (`plan.md`) | `037-skills-engine` | The two setup artifacts disagreed; the confirmation artifact verified the second, and it matches the crate name. |
| Test count | 3 test files described by acceptance criterion | 3 test files, 31 tests | The edge cases the source plan listed as a checklist (empty `instructions`, absent `params`/`required_extensions`, duplicate extension names, argless tool step, undeclared placeholder) were each written as a test rather than reasoned about in prose. |

---

## Close-out (T8)

All three CI gates (`.github/workflows/core.yml:57,59,61`) run green locally on Windows:

- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — green, `heddle-skills` contributing 31 tests across three integration binaries.

`cargo tree -p heddle-skills` confirms the Constitution IV claim structurally: `heddle-core` and `heddle-workflow`, and no other workspace crate.
