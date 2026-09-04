# Tasks: tool advertisement on the `TurnRequest` path (v0 slice)

**Spec:** `specs/015-tool-advertisement/spec.md` · **Plan:** `specs/015-tool-advertisement/plan.md` ·
TDD (red→green), product code in `crates/skein-core`, `crates/skein-gateway` and `crates/skein-acp`,
branch `015-tool-advertisement` cut from `dev` after slice 014 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the whole mechanism lives in `skein-core` — a type, a defaulted trait method,
  one gateway method and one loop call. `skein-gateway` gains only the translation of that type onto
  its own wire format, and `skein-acp` only a one-line forward. No CLI change and no new capability ·
  II Local-first ✅ NON-NEGOTIABLE and unchanged: no new egress path, no new dependency, the loopback
  guard and the no-TLS build property are untouched. Advertisement adds bytes to a request that was
  already going to the same local endpoint
- III Test-First ✅ every step's red observed and recorded in `## Observed red` before its green ·
  IV Inverted coupling ✅ `skein-core` still names no protocol: `advertise` asks a `ToolTransport`,
  which is the port, and `ToolSpec.parameters` is an opaque `serde_json::Value` because the schema is
  the server's document and the core never interprets it. `skein-gateway` remains the only crate
  naming the OpenAI wire format, `skein-mcp` the only one naming MCP
- V Traceability ✅ no new `StepKind` and none needed: the advertisement travels inside `TurnRequest`,
  which `run` already captures as `LlmRequest` through `Redactor::redact_json`, so tool descriptions
  and schemas are scrubbed by `redact_value`'s existing recursion. A run's captured request now shows
  exactly what the model was told it could do
- VI Security ✅ deny-by-default, and the one silent default in the slice is argued rather than
  assumed: an un-overridden `ToolTransport::list` advertises **nothing**. The policy filter lives
  inside `ToolGateway`, so "you cannot advertise what the policy forbids" is structural. An
  unapproved `Mutating` tool is still advertised **on purpose** — denying it at advertisement would
  disconnect `AcpPermissionTransport`, which is the only path to a human
- VII Neutrality ✅ one new type, one defaulted trait method, one gateway method, two serde
  attributes. Zero new packages, zero new dependency edges, no new crate, no config, no flag. No
  `ToolCatalog` trait and no hand-written schema anywhere
- VIII Loop discipline ✅ NON-NEGOTIABLE and untouched: `advertise` runs **after** the pre-flight
  `should_exit(false)` check, so a zero-budget run makes no round trip and the budget is still
  enforced before it is spent. The exits, the probe and the controller are unchanged
- Cross-platform ✅ no `#[cfg]` in any new code, no filesystem and no process work. No workspace
  member is added, so `core.yml`'s `paths: crates/**` needs no change

## Tasks
- [x] **T0** `specs/015-tool-advertisement/{spec.md,plan.md,tasks.md}`; branch
      `015-tool-advertisement` cut from `dev` with slice 014 merged
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **120 passed, 1 ignored**
- [x] **T2** RED→GREEN — `ToolSpec` in `crates/skein-core/src/tool.rs`, re-exported from `lib.rs`,
      with its round-trip test in `crates/skein-core/tests/core.rs`
- [x] **T3** RED→GREEN — `ToolTransport::list` with its defaulted body and the docstring arguing why
      *this* silent default is the safe one
- [x] **T4** RED→GREEN — `ToolGateway::advertise`: list, filter to the allowlist, in allowlist order
- [x] **T5** RED→GREEN — `TurnRequest.tools` and every literal construction site, one atomic commit
- [x] **T6** RED→GREEN — `NativeLoop::run` advertises once per run and stamps the specs into every
      `TurnRequest`; a `list` failure is fatal
- [x] **T7** RED→GREEN — `AcpPermissionTransport::list` forwards to its inner transport (the slice's
      highest-risk line, its own test)
- [x] **T8** RED→GREEN — `ChatRequest.tools` in `crates/skein-gateway/src/lib.rs`, byte-exact
- [x] **T9** gates, control diff, dependency drift, close-out

## Control baseline (T1)

`cargo test --workspace` on `015-tool-advertisement` @ `8860a01` (identical to `dev`), working tree
clean apart from this slice's three spec files, 2026-09-03, before any code edit: **120 passed, 0
failed, 1 ignored** — `acp_session` 15, `cli_acp_agent` 4, `cli_chat` 8, `cli_ledger` 8, `cli_secret`
2, `core` 17, `native_loop` 21, `tool_gateway` 10, `governed_run` 2, `openai_compat` 14 (+1 ignored,
the optional live-Ollama test), `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5. The five
`src/lib.rs`/`src/main.rs` unit-test targets and the five doc-test targets each contribute
`0 passed`. This matches slice 014's recorded gate figure exactly, and it is the number T9 diffs
against.

## Observed red (Constitution III)

All on 2026-09-03.

- **T2** `cargo test -p skein-core --test core` with the round-trip test written against a type that
  did not exist — **1 compile error**, and the file did not build:
  - `error[E0432]: unresolved import skein_core::ToolSpec` at `crates/skein-core/tests/core.rs:5:79`
    — `no ToolSpec in the root`
  - `error: could not compile skein-core (test "core") due to 1 previous error`
  - Green: **18 passed** where 17 had passed, with the seventeen unchanged.

- **T3** `cargo test -p skein-core --test tool_gateway` with the defaulted-body test written against
  a trait that had only `call` — **1 compile error**:
  - `error[E0599]: no method named list found for struct UnlistedTransport in the current scope` at
    `crates/skein-core/tests/tool_gateway.rs:70:27`
  - `error: could not compile skein-core (test "tool_gateway") due to 1 previous error`
  - Green: **11 passed** where 10 had passed, with the ten unchanged — the proof that a defaulted
    method left all nine pre-existing `impl ToolTransport` sites compiling untouched.

- **T4** `cargo test -p skein-core --test tool_gateway` with the three advertisement tests written
  against a gateway that had no such method — **3 compile errors**, one per new test:
  - `error[E0599]: no method named advertise found for struct ToolGateway<T> in the current scope`,
    at `crates/skein-core/tests/tool_gateway.rs:127`, `:152` and `:171`
  - `error: could not compile skein-core (test "tool_gateway") due to 3 previous errors`
  - Green: **14 passed** where 11 had passed, with the eleven unchanged. `CountingTransport` gained
    an additive `catalogue` field and an `offering` constructor; no existing body moved.

- **T5** `cargo test -p skein-core --test core` with the no-`tools`-key serialization test written
  against a two-field `TurnRequest` — **1 compile error**:
  - `error[E0560]: struct TurnRequest has no field named tools` at
    `crates/skein-core/tests/core.rs:361:9`
  - `error: could not compile skein-core (test "core") due to 1 previous error`
  - Then a **second, behavioural red** once the field existed, because the test's byte-exact literal
    guessed serde's default externally-tagged enum shape for `Content` rather than reading it:
    `left: "…\"parts\":[{\"type\":\"text\",\"text\":\"hello\"}]…"` /
    `right: "…\"parts\":[{\"Text\":{\"text\":\"hello\"}}]…"`. `Content` carries
    `#[serde(tag = "type", rename_all = "snake_case")]`. The expectation was corrected to the tree's
    actual wire shape; the claim under test — that **no `tools` key appears** — was never weakened.
  - Green: **19 passed** in `core` where 18 had passed, and `cargo test --workspace` reached
    **126 passed, 1 ignored**. The three literal construction sites gained `tools: Vec::new()` and
    nothing else; `openai_compat.rs`'s byte-exact no-tools assertion is still green with an
    **unchanged body**, which is what D5's skip-when-empty exists to achieve.

- **T6** `cargo test -p skein-core --test native_loop` with the four advertisement tests written
  against a `run` that never listed anything — **2 failed, 23 passed**:
  - `the_advertised_catalogue_reaches_every_turn_of_the_run` —
    `assertion left == right failed: once per run, not once per turn: the catalogue does not change
    mid-run / left: 0 / right: 1`, at `crates/skein-core/tests/native_loop.rs:1131`
  - `a_catalogue_that_cannot_be_read_ends_the_run_before_it_starts` — panicked at `:1163` on
    `expect_err("an inventory we could not read leaves the run's capabilities unknown")`: the run
    completed happily, because nothing had asked the transport anything.
  - The other two new tests (`a_zero_budget_run_never_asks_for_a_catalogue` and
    `a_run_with_an_empty_catalogue_captures_no_tools_key`) were **green from the start**, and that is
    recorded rather than dressed up: they pin behaviour the tree already had and this step must not
    break. The zero-budget one is what would fail if `advertise` were ever moved above the pre-flight
    `should_exit` check; the empty-catalogue one is what would fail if the `skip_serializing_if` were
    dropped.
  - Green: **25 passed** where 21 had passed, with the twenty-one unchanged.

- **T7** `cargo test -p skein-acp --test acp_session p4` before the override existed — **1 failed**,
  and the failure is the exact bug the plan called the slice's highest-risk line:
  - `assertion left == right failed: the inner transport's catalogue, in its own order / left: [] /
    right: ["fs_read", "fs_list"]`, at `crates/skein-acp/tests/acp_session.rs:1118`
  - Worth stating plainly: this red **compiled cleanly**. Inheriting a defaulted trait method is not
    a compile error, so nothing but this test stands between the tree and a `skein acp-agent` that
    silently advertises nothing while `skein chat` works.
  - Green: **16 passed** where 15 had passed, with the fifteen unchanged. `CataloguedTransport` is a
    new double rather than a field on `CountingTransport`, so the three pre-existing permission
    tests stay controls.

- **T8** `cargo test -p skein-gateway --test openai_compat advertised` before `ChatRequest` had the
  field — **1 failed**, the client having silently dropped the whole array:
  - `left: "{\"model\":\"llama3.1\",\"messages\":[…],\"stream\":false}"` /
    `right: "…,\"stream\":false,\"tools\":[{\"type\":\"function\",\"function\":{\"name\":\"fs_read\",…}}]}"`,
    at `crates/skein-gateway/tests/openai_compat.rs:256`
  - Green: **15 passed, 1 ignored** where 14 had passed, and
    `turn_sends_an_openai_chat_completions_request`'s byte-exact literal is **unchanged** — the
    control D5 exists to keep.
  - One comment corrected while in the file: `tool_calls_are_translated_and_are_not_a_final_answer`
    said *"This client advertises no tools"*, which stopped being true of the client the moment
    `ChatRequest` grew the field. It now says *"This **request** advertises no tools"*, which is what
    the test actually arranges. No assertion moved.

## Gate run (T9)

2026-09-03, Windows leg observed locally; macOS and Linux legs unobserved until the repository has a
remote (the standing caveat of specs 004–014).

- `cargo fmt --all -- --check` — clean. It was **not** clean on first ask: rustfmt rewrapped one
  import in `crates/skein-core/tests/core.rs` and one iterator chain in
  `crates/skein-acp/tests/acp_session.rs`. `cargo fmt --all` applied both; no logic moved.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no warning on any of the six
  crates. `ToolPolicy::advertisable` takes `&[ToolSpec]` rather than `Vec<ToolSpec>` because it only
  reads.
- `cargo test --workspace` — **132 passed, 0 failed, 1 ignored**: the 120 baseline plus twelve, and
  every one of the 120 with an unchanged body. `core` 17→**19**, `native_loop` 21→**25**,
  `tool_gateway` 10→**14**, `acp_session` 15→**16**, `openai_compat` 14→**15** (+1 ignored).
  Unchanged: `cli_acp_agent` 4, `cli_chat` 8, `cli_ledger` 8, `cli_secret` 2, `governed_run` 2,
  `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5.
  - **Twelve new tests, not the ten the plan predicted.** Two extra, both because a claim the plan
    folded into another test has its own failure mode:
    `a_catalogue_that_cannot_be_read_is_an_error_not_an_empty_advertisement` (the gateway must not
    turn an unreadable inventory into an empty one, which is a different bug from the loop
    mishandling the error) and `a_zero_budget_run_never_asks_for_a_catalogue` (the plan states D4's
    ordering as a rationale and tested only the "advertise once" half). 120 + 10 + 2 = 132.
- `cargo build --workspace` — clean.

## A build-graph hazard this slice ran into, recorded because it will recur

`advertised_tools_are_sent_in_openais_function_shape` **passed** under
`cargo test -p skein-gateway` and **failed** under `cargo test --workspace`, on nothing but the key
order inside the `parameters` schema — insertion order in the workspace build, sorted order in the
single-crate build.

The cause is the one slice 008 recorded under its risk R1 and verified as *"real, and inert"*:
`agent-client-protocol` declares `serde_json = { features = ["preserve_order", …] }`, and **Cargo
unifies features per build graph**. A graph containing `skein-acp` compiles `serde_json` with
`preserve_order`, so `Map` is an insertion-ordered `IndexMap`; a graph without it gets a sorted
`BTreeMap`. Slice 008's note held only because no assertion in the tree depended on object ordering.
A byte-exact assertion over a multi-key `serde_json::Value` is the first thing in the tree that does.

The fix is in the test and needs no product change: the schema literal's keys are written in
**alphabetical order**, so insertion order and sorted order are the same string and the assertion
holds under either resolution. The reason lives in a comment at the literal, because it is invisible
otherwise and the next author to add a tool schema will hit it. The envelope keys around it
(`type`/`function`, `name`/`description`/`parameters`) are **struct fields**, so their order is ours
and this does not touch them.

Two of slice 014's statements are false in the light of this, and are **left unedited** here rather
than quietly corrected in a closed slice: `specs/014-ledger-redaction/spec.md:44` and `plan.md:137`
both tell readers `preserve_order` is not enabled, and that Ledger payloads are therefore
alphabetically keyed. In the shipped binary — `skein-cli` depends on `skein-acp` — it is enabled and
they are not. No behaviour is wrong; the prose is. Recorded as a discovery for review to consolidate,
and put on the next-slice list below.

## Control diff (T9)

`git diff dev --stat -- crates/skein-cli/ crates/skein-silo/ spikes/ .github/ rust-toolchain.toml` is
**empty** — a genuinely empty control diff, where slice 014 had to declare one mechanical exception.
`spikes/` is untouched (ADR-0004 D2), and so are `.github/` and `rust-toolchain.toml`.

Over the whole branch, **1167 insertions and 13 deletions** across 14 files, three of them this
slice's spec artifacts. Every one of the 13 deletions is accounted for, and **not one is an
assertion**:

- five `use skein_core::{…}` import lines that grew a name and were rewrapped — in `tests/core.rs`,
  `tests/native_loop.rs`, `tests/tool_gateway.rs`, `tests/acp_session.rs` and
  `tests/openai_compat.rs`;
- one comment in `tests/openai_compat.rs`, corrected from *"This client advertises no tools"* to
  *"This **request** advertises no tools"*, because the first became false the moment `ChatRequest`
  grew the field;
- the rest are in product code this slice exists to change: `model.rs`'s docstring saying
  advertisement was deferred, `tool.rs`'s and `native_loop.rs`'s edited lines, and `lib.rs`'s
  re-export list.

`git diff dev` over the three touched test directories shows **no deleted assertion and no changed
test body** anywhere. The three `TurnRequest { … }` literals gained `tools: Vec::new()` and nothing
else. That is what D5's skip-when-empty bought.

## Drift (T9)

**Zero new packages and zero new dependency edges — by construction, not by measurement.**
`git diff dev -- Cargo.toml crates/*/Cargo.toml Cargo.lock` is **empty**: no manifest and no lockfile
in the workspace changed, so the graph `cargo` resolves is the same graph it resolved on `dev`.
Everything the slice needed — `serde`, `serde_json`, and the crates' existing edges to `skein-core` —
was already declared.

No toolchain change and no new build prerequisite: no crate entered the graph, so nothing can have
raised the MSRV, and `rust-toolchain.toml`, `workspace.package.rust-version` and
`.github/workflows/core.yml` are untouched. No workspace member was added, so `core.yml`'s
`paths: crates/**` needs no edit.

## Deviations from the plan

Four, all recorded rather than absorbed:

1. **No `ToolPolicy` accessor over the allowlist.** The plan's T3 says *"`ToolPolicy` gains an
   accessor over its allowlist names, in allowlist order."* It did not, because it does not need one:
   `ToolPolicy` and `ToolGateway` live in the same module, so `advertise` reaches the allowlist with
   no public API growth. What the policy gained instead is a **private**
   `advertisable(&[ToolSpec])`, which also puts the filtering rule in the type that owns the
   allowlist rather than in the type that owns the transport. A public accessor with one in-module
   caller would have been surface for nothing.
2. **`ToolSpec` derives `Eq`, not merely `PartialEq`.** The plan's T2 specifies
   `Serialize + Deserialize + PartialEq`. `TurnRequest` derives `Eq`, so a `Vec<ToolSpec>` field
   forces `ToolSpec: Eq`; `serde_json::Value` is `Eq`, which is why `ToolCall` already derives it, so
   the derive is sound. The code decided this, not preference.
3. **`ChatRequest.tools` carries only `skip_serializing_if`, not `#[serde(default)]`.** The plan's D5
   gives both fields both attributes. `ChatRequest` derives `Serialize` alone — it is a request the
   client writes and never reads — so `default` would be inert. `TurnRequest.tools` has both, because
   it *is* deserialized out of the Ledger and the default is what lets an old chain replay.
4. **Four `TurnRequest` construction sites in the plan, three in the tree.** The plan's T5 names
   *"`native_loop.rs`'s `run`, `crates/skein-acp/tests/acp_session.rs`, and
   `crates/skein-gateway/tests/openai_compat.rs`'s `ask` helper"* as four; that list is three, and
   `grep` finds exactly those three literal constructions in the workspace. Nothing was missed —
   every other use of `TurnRequest` in the tree is by reference.

Also worth stating, since Constitution III is the point: **two of T6's four tests were green on
arrival**, and they are labelled as such in `## Observed red` rather than presented as driven. They
guard D4's placement instead of driving it.

## Out of scope

Deliberately not done, so no one helpfully does it. Identical to the spec's list, and in particular:

- **Every part of slice 016** — `crates/skein-connectors`, `FsRoot`, the `fs` server, `--fs-root`,
  and all `skein-cli` wiring. This slice ships advertisement machinery with **no caller in the
  shipped binary**, exactly as slice 005 shipped the whole `ToolGateway` before `skein chat` existed.
  Both loop-running commands still carry `NoTools` and an empty policy, so `advertise` returns empty
  and no `tools` key is serialized anywhere.
- **`git` and `shell` tools.** ADR-0004 D3's sixth item stays open for both.
- **Deriving `ToolAccess` from MCP tool annotations**; **denying advertisement to an unapproved
  `Mutating` tool**; **`role: "tool"`/`tool_call_id` replay**; **`strict`, `tool_choice`, parallel
  tool calls, streaming**; **a new `StepKind`**; **per-turn re-listing**; **a `ToolCatalog` trait**.
- **`crates/skein-cli/`, `crates/skein-silo/`, `spikes/`, `.github/`, `rust-toolchain.toml`** — all
  verified empty in the control diff above.

## Next slice (not this feature)
- [ ] **the `fs` connector (slice 016)** — a new `crates/skein-connectors` holding a root-bounded
      embedded rmcp filesystem server, opt-in via `--fs-root`, wired into both commands with named
      allowlists. This is what gives this slice a caller, and where the governed tool path is finally
      exercised against something real. ADR-0004 D3 closes for `fs` there and **remains open for
      `git` and `shell`** — say so in its close-out rather than claiming the item done.
- [ ] **raw-wire-byte capture** — a `StepKind` for the provider's literal request and response bytes.
      Carried unchanged from slices 011, 012, 013 and 014.
- [x] **`role: "tool"` / `tool_call_id` conversation replay**, which would reopen
      `native_loop.rs`'s anti-injection decision deliberately rather than by accident. Recorded as a
      residual in this slice's spec: the current user-role feedback produces a valid OpenAI sequence
      precisely because no `tool_calls` are ever sent, so there is no dangling id to satisfy.
      **Done in slice 022**, deliberately as this item asked — and the dangling-id argument is
      discharged by a test over the serialized request rather than by the absence of the array.
- [ ] **reconcile slices 008 and 014 on `serde_json/preserve_order`.** 008 is right and 014 is wrong;
      the tree should not carry both. See the gate-run note above.
- [ ] **streaming (SSE)** with incremental ACP `AgentMessageChunk` notifications.
- [ ] **provider authentication**, a config file, the egress-policy layer and ADR-0002 D4's
      process-level socket-deny boundary.
