# Plan: slice 018 — prove the ACP permission gate end to end with an answering client

**Target artifacts:** `specs/018-acp-permission-gate/{spec.md,plan.md,tasks.md}`
**Branch:** `018-acp-permission-gate`, cut from `dev` at `4eeea42` (verified: `git rev-parse --short HEAD` = `4eeea42`, `git remote -v` empty — no PR, as the request states)
**Product code touched:** none. This slice is tests plus Spec-Kit artifacts. See *Approach D1*.

---

## Problem

Slice 013 wired `AcpPermissionTransport` so that a mutating tool call made through
`heddle acp-agent` asks the connected ACP client before executing. Slices 016 (`fs_write`) and
017 (git, read-only only) left the gate *reachable* but incompletely proven, and both name the
same residual without closing it:

- `specs/016-fs-connector/tasks.md`, `## Next slice`: *"the ACP permission gate, exercised …
  `cli_acp_agent.rs`'s client registers no permission handler, and building one is a slice of its
  own. Until then the gate is wired and unproven end to end."*
- `specs/017-git-connector/tasks.md`, `## Next slice`: *"Carried unchanged from slice 016: the ACP
  permission gate exercised end to end …"*

Constitution VI (deny-by-default; a mutating tool requires approval) is the safety mechanism at
stake. A gate that has never been driven by an answering client against a tool with a real effect
is exactly the "confident, well-formed answer to the wrong question" this project's discipline
warns about.

---

## What was verified before planning (and where the request's premises need correcting)

Everything below was read this session in the working tree at `4eeea42`. Two of the request's
premises are wrong in a way that changes the slice's shape, and the spec must say so rather than
inherit them.

### ❗ Correction 1 — a real ACP client answering a real permission request is **already** tested

The request says *"every existing test either uses only read-only tools (never reaching the gate)
or never exercises the branch where a real client is asked and actually answers."* The second half
is false. `crates/heddle-acp/tests/acp_session.rs` contains, under `// Unit level.`:

- helper `ask_permission(outcome, tool_calls)` — builds a **real** `Client` with
  `.on_receive_request(async move |request: RequestPermissionRequest, responder, _cx| …)` that finds
  the offered option by `PermissionOptionKind` and answers with
  `RequestPermissionResponse::new(RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option.option_id.clone())))`,
  connected to a real `Agent` over a real `ByteStreams` / `tokio::io::duplex` transport, driving
  `heddle_acp::AcpPermissionTransport::new(CountingTransport{…}, cx, SessionId::new("unit"))`.
- `p1_an_allow_answer_reaches_the_inner_transport`
- `p2_a_reject_answer_denies_without_reaching_the_transport`
- `p3_a_cancelled_answer_denies_without_reaching_the_transport`

So the ACP request/response round trip, all three outcomes, and "the inner transport is not
reached on a refusal" are proven. **The client-side API question the request asks us to research is
already answered in-repo**, by working code that compiles against `agent-client-protocol 2.0.0`.

### ✅ What is genuinely unproven (this slice's actual reason to exist)

Cross-checking `p1`–`p3` against the invariants the request lists, four things are missing, and all
four sit in the same place — the layer where the binary, the policy, the connector, the disk and the
chain are all real:

1. **No real mutating tool.** `p1`/`p2` use a `CountingTransport` double whose `call` returns
   `"file contents"` and bumps an `AtomicUsize`. Nothing on disk is at stake, so
   "a Deny answer means no side effect occurred" is not proven — only "a counter did not move".
2. **No Ledger.** `ask_permission` drives `AcpPermissionTransport::call` **directly**, bypassing
   `ToolGateway::call_captured`. No `ToolCall`, no `Approval`, no `ToolResult` step exists in those
   tests and `verify_chain` is never called. Constitution V is unproven for this path.
3. **No real process.** `p1`–`p3` are in-process over a duplex. The proven-good path
   (`an_acp_client_drives_the_real_binary_and_the_session_lands_on_the_chain`) never makes a tool
   call at all.
4. **No test anywhere makes a tool call through `heddle acp-agent`.** Verified by reading every test
   in `crates/heddle-cli/tests/cli_acp_agent.rs`: all eight either drive a text-only turn, or assert
   the *advertised* tool list off the wire
   (`acp_agent_accepts_an_fs_root_and_still_serves_a_session`,
   `acp_agent_over_a_git_repository_advertises_the_git_tools_too` — both assert
   `body["tools"]` and stop there), or refuse before the handshake. The `StubProvider` in that file
   is only ever given plain `reply(...)` bodies; there is **no `tool_call_reply` helper in that
   file** (it exists in `cli_chat.rs` only).

That also explains why nobody has hit a failure: because no ACP test ever provokes a tool call, the
missing permission handler in that file's client has never mattered. Add a tool call without adding
the handler and the request would go unhandled → `ask` returns `Err` → `HeddleError::Tool` →
`NativeLoop::mediate` treats it as fatal → the prompt answers with an internal error. Worth stating
in the spec, because it is the reason the residual could sit open through two slices.

### ✅ Confirmed premises

- **`fs_write` is still the only `Mutating` tool.** `grep -rn "Mutating" crates/ --include=*.rs`
  outside tests yields `crates/heddle-core/src/tool.rs` (the enum and the two `decide` arms) and
  `crates/heddle-cli/src/wiring.rs:214` (`allowed.push(("fs_write".to_string(), ToolAccess::Mutating))`).
  `git_status` and `git_log` are `ToolAccess::ReadOnly` in `ToolArgs::git_tools`.
- **`ToolArgs::agent_policy`** allowlists `fs_write` **and** puts it in `approved`, so
  `ToolPolicy::decide` returns `Decision::Allow` and the call reaches `AcpPermissionTransport`.
  `ToolArgs::chat_policy` omits it entirely. Both gated on `--fs-root` being present by
  `ToolArgs::policy`.
- **`crates/heddle-cli/src/acp.rs`'s `serve`** builds one `SessionParts` per session with
  `transport: tools.transport()?`, `policy: tools.agent_policy()`, and
  `ledger: Silo::open(&root,&id)?.ledger()?`. One session → run id `heddle-1#1`.
- **`AcpPermissionTransport::ask`** sends `RequestPermissionRequest::new(session_id,
  ToolCallUpdate::new(ToolCallId::new(tool), ToolCallUpdateFields::new().title(tool)), vec![…])`
  with exactly two options: `("heddle.allow-once", AllowOnce)` and `("heddle.reject-once", RejectOnce)`
  (constants `ALLOW_ONCE` / `REJECT_ONCE`). It blocks the calling thread on
  `std::sync::mpsc::Receiver::recv()` — **no timeout**. Only `option_id == "heddle.allow-once"`
  reaches the inner transport; any other `Selected` yields
  `HeddleError::ToolDenied { reason: format!("acp client declined permission ({})", selected.option_id) }`,
  and `Cancelled` yields `"acp permission request cancelled"`.
- **`fs_write`'s parameters** are `WriteParams { path: String, content: String }`
  (`crates/heddle-connectors/src/server.rs`); it calls `self.root.resolve_new(&arg)` then
  `std::fs::write`, returning `format!("wrote {} bytes to {arg}", content.len())`. The parent
  directory must already exist — writing at the root of `--fs-root` satisfies that.
- **The concurrency question is already settled by working code.** `agent-client-protocol`'s
  `src/concepts/ordering.rs` states that `on_receive_request` callbacks run *inside* the dispatch
  loop and that `block_task()` is for tasks *outside* it — a foreground `connect_with` future
  qualifies explicitly. `Builder::connect_with` (`src/jsonrpc.rs`) drives transport, dispatch and
  `main_fn` as one composed future, and the existing headline test already receives
  `SessionNotification`s while its `main_fn` awaits `PromptRequest`'s response under
  `futures::executor::block_on`. On the agent side, `HeddleAgent::serve`'s `PromptRequest` handler
  spawns an OS thread and returns immediately with the recorded comment *"the dispatch task must
  stay free to deliver the permission answers the loop thread waits on."* So a permission handler
  that only records and responds cannot deadlock either side.
- **`Builder::on_receive_request` and `on_receive_notification`** both return
  `Builder<Host, impl HandleDispatchFrom<…>, Runner, Close>` — chainable. `name()` returns `Self`
  and must stay first, as it already is in `cli_acp_agent.rs`.
- **`RequestPermissionRequest` derives `Clone`** (`agent-client-protocol-schema-1.5.0/src/v1/client.rs`),
  is `#[non_exhaustive]`, and its fields are `session_id`, `tool_call: ToolCallUpdate`
  (`tool_call_id`, flattened `fields`), `options`, `meta`. `PermissionOptionKind` is `Copy`.
- **The established deny shape on the chain.** `ToolGateway::call_captured` appends `ToolCall`
  (redacted attempt) → `Approval` (`ApprovalRecord { tool, decision, reason }`) → *then* calls the
  transport → `ToolResult` only on success. `crates/heddle-connectors/tests/governed_fs_run.rs`'s
  `an_unlisted_write_never_reaches_the_server` pins that shape for a policy denial: the file is
  absent from disk, the fed-back message starts with `[tool_result tool=fs_write status=denied]`,
  the filtered chain is `vec![StepKind::ToolCall, StepKind::Approval]`, and `verify_chain` passes.
- **`heddle ledger log`** prints four tab-separated columns `run_id \t seq \t kind \t step_id` — no
  payload. `heddle ledger show <id>` prints the payload. `heddle ledger verify` prints
  `<run_id>\tok\t<n> steps`. The existing `logged_kinds` helper in `cli_acp_agent.rs` reads column
  index 2.

### 🔍 Two residuals discovered while verifying (recorded, not fixed here)

- **A permission request cannot be correlated to its tool call by a client.**
  `AcpPermissionTransport::ask` uses `ToolCallId::new(tool)` — the tool *name* — while
  `heddle_acp::project_updates` uses `step.id`, the chain hash, as the `ToolCallId` for the
  `SessionUpdate::ToolCall`. The two ids never match, so an editor cannot join the prompt it showed
  to the tool call it later sees. Fixing it needs the chain step id inside the transport, which the
  transport does not have; that is a design change, not this slice's.
- **An ACP-denied call is projected as `Pending` forever.** `project_updates` maps
  `Approval.decision == "allowed"` → `ToolCallStatus::Pending` and only a `ToolResult` step →
  `Completed`. On the ACP-deny path the `Approval` says `allowed` (the *policy* allowed it) and no
  `ToolResult` is written, so the client's last word on that tool call is `Pending`. This is
  observable and mildly wrong; changing it would mean changing what the chain records, which
  Principle VII puts outside this slice.

---

## Approach

### D1 — Prove the gate at the CLI-acceptance layer only. No product code changes.

Everything the slice needs already exists and is verified above: the transport, the policy, the
connector, the silo-backed chain, the client-side answering API. The gap is a *test*, in one file.
So this slice adds tests to `crates/heddle-cli/tests/cli_acp_agent.rs` and changes no `src/`.

**Rejected: add a `StepKind` (or a second `Approval` step) recording the client's answer.** It is
the tempting reading of the traceability invariant, and it loses on three counts. (a) The request
itself instructs: *"with the refusal recorded in the Ledger exactly as an unlisted-tool denial
already is (verify how the existing deny path records this, and match that shape rather than
inventing a new one)"* — and the verified shape is `ToolCall` + `Approval` + no `ToolResult`.
(b) `AcpPermissionTransport` holds no `Ledger` and is constructed *inside* `ToolGateway`; giving it
one inverts the gateway/transport relationship that Principle IV's decorator design rests on.
(c) A new `StepKind` changes `hash()`'s input space and every chain-shape assertion in the tree —
enormous blast radius to record something already derivable. The answer **is** on the chain twice
over: as the presence or absence of `ToolResult`, and verbatim inside the *next* `LlmRequest`
payload, because `NativeLoop::mediate` feeds `[tool_result tool=fs_write status=denied]\nacp client
declined permission (heddle.reject-once)` back into the conversation the next request records.

**Rejected: extend `crates/heddle-acp/tests/acp_session.rs` instead.** It already has the answering
client, so it looks cheaper. It loses because the two things the slice must prove are precisely the
two it cannot host: a real `fs_write` effect on disk needs `heddle-connectors`, which `heddle-acp`
does not depend on and must not (Principle IV — `heddle-acp` is the ACP-only crate), and the
governed chain needs `ToolGateway`, which `ask_permission` deliberately bypasses. `heddle-cli` is
the one crate that already depends on both `heddle-acp` and `heddle-connectors` and already runs the
real binary.

**Rejected: a live-model variant (the `#[ignore]`d T11 pattern of slices 016/017).** A live model
cannot be made to answer Deny, and cannot be relied on to call `fs_write` at all; the whole point of
the `StubProvider` here is that both branches are deterministic. Stated in *Out of scope*.

### D2 — One parameterised harness, two tests, one answer each.

The allow and deny runs differ only in which offered option the client selects. So: one helper
taking a `PermissionOptionKind`, two `#[test]`s. Both use a fresh `TempDir` fs root and a fresh
silo, so `session_id` is deterministically `heddle-1` (the facade's `AtomicU64` starts at 1 in a
fresh process) exactly as the existing headline test relies on.

**Rejected: one test with both answers in sequence.** Two prompts in one session would make the run
ids `heddle-1#1` and `heddle-1#2` and share one fs root, so "no file was written" would depend on the
order of two runs rather than on the denial. Two independent tests keep each assertion's ground
truth its own.

### D3 — The disk is the ground truth for Deny, not the tool result.

The deny assertion is `!fs_root.join("planted.txt").exists()`, on the same fixture where the allow
test proves the same call *does* create that file. This is `governed_fs_run.rs`'s recorded reasoning
applied to a new refusing layer: *"its **absence on disk** is the ground truth that nothing
downstream of the policy ran. Not a counter in the server: an effect the server would have had."*
That the tool result text says `denied` is asserted too, but it is corroboration, not the proof.

### D4 — Assert the request the agent sent, from the client's side.

The client's handler records each `RequestPermissionRequest` it receives (it derives `Clone`) into an
`Arc<Mutex<Vec<_>>>`, and both tests assert on it: exactly one request, `session_id == "heddle-1"`,
`tool_call.tool_call_id == "fs_write"`, `tool_call.fields.title == Some("fs_write")`, and the two
options in order with their ids and kinds. This is what makes "the request is visible" a *checked*
claim about the wire and not an inference from the agent's own source, and it pins the two option-id
string constants that `AcpPermissionTransport::call` matches on — today they are asserted nowhere,
so a typo in either would silently turn every Allow into a denial.

---

## Steps

Strict TDD (Constitution III): each step's red is observed and recorded verbatim in `tasks.md`
under `## Observed red` **before** its green. Ordered so each is independently verifiable.

- **T0 — artifacts and branch.** `specs/018-acp-permission-gate/{spec.md,plan.md,tasks.md}`,
  mirroring slice 017's format and its `## Constitution Check` table shape (the eight principles
  plus the `Cross-platform` row). Branch `018-acp-permission-gate` cut from `dev` at `4eeea42`.
  `spec.md` must carry the two premise corrections and the two discovered residuals from *What was
  verified* above — a spec that repeats the "never exercised by an answering client" claim ships a
  wrong claim.
- **T1 — control baseline.** `cargo test --workspace` before any edit, recorded verbatim per
  target. `specs/017-git-connector/tasks.md` records **191 passed, 0 failed, 3 ignored** at its
  close, with `cli_acp_agent` at 8; `dev` is one documentation-only commit later (`4eeea42`), so
  the figure is expected to be identical and must be **re-measured, not quoted**.
- **T2 — helpers, no assertions yet.** Append to `crates/heddle-cli/tests/cli_acp_agent.rs`, under a
  new `// ---- the ACP permission gate (spec 018) ----` section:
  - `fn tool_call_reply(tool: &str, arguments: serde_json::Value) -> String` — **copied verbatim**
    from `cli_chat.rs`, for the reason that file's own header already records: `heddle-cli` has no
    `lib` target, integration-test binaries share nothing, and copying keeps the other file's tests
    as this slice's controls.
  - `fn last_message(request: &serde_json::Value) -> String` — likewise copied from `cli_chat.rs`.
  - `struct Answered { stop: StopReason, asked: Vec<RequestPermissionRequest>, updates: Vec<SessionUpdate> }`
    — **new**.
  - `fn run_answering(root: &Path, silo: &str, fs_root: &Path, base_url: &str, answer: PermissionOptionKind) -> Answered`
    — **new**. Builds the `AcpAgent`/`AcpAgentConfig` transport with
    `["acp-agent", "--root", …, "--silo", …, "--model", "llama3.1", "--base-url", …, "--fs-root", …, "--timeout-secs", "10"]`
    (the existing tests' flag set plus nothing), then
    `Client.builder().name("test-client").on_receive_notification(…).on_receive_request(…, agent_client_protocol::on_receive_request!()).connect_with(transport, async move |cx: ConnectionTo<Agent>| …)`
    inside `run_with_timeout(…)`, doing `InitializeRequest` → `NewSessionRequest` → one
    `PromptRequest`. `name()` stays first; the notification handler is the one already in this file;
    the request handler pushes `request.clone()` and answers with the option whose `kind == answer`.
    New imports: `PermissionOption`/`PermissionOptionKind`, `RequestPermissionOutcome`,
    `RequestPermissionRequest`, `RequestPermissionResponse`, `SelectedPermissionOutcome`,
    `ToolCallId` from `agent_client_protocol::schema::v1`.
- **T3 — RED→GREEN — the Allow path.** `an_acp_client_that_allows_lets_a_real_fs_write_execute`.
  Red first: write the test and observe it fail. It must fail *for the right reason* — record which.
  Green needs no `src/` change; if it does, the plan is wrong and that is a stop condition, not a
  thing to patch.
- **T4 — RED→GREEN — the Deny path.** `an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives`,
  same fixture, `PermissionOptionKind::RejectOnce`.
- **T5 — no pre-existing assertion changes.** `git diff dev -- crates/heddle-cli/tests/cli_acp_agent.rs`
  must be **append-only apart from the `use` block**. If any of the eight existing tests needs an
  assertion changed, something is wrong with the harness, not with them — stop.
- **T6 — gates, control diff, close-out.** The four gates (below). `cargo test --workspace` must be
  the T1 figure **+2** with `cli_acp_agent` 8 → 10 and **every other target's count unchanged** —
  slice 017's SC-012 discipline, stated as a number.
  `git diff dev --stat -- crates/heddle-core/ crates/heddle-acp/ crates/heddle-connectors/ crates/heddle-gateway/ crates/heddle-mcp/ crates/heddle-silo/ spikes/ .github/ rust-toolchain.toml Cargo.toml Cargo.lock`
  must be **empty** — for this slice the blast radius is one test file, and an empty diff over
  `crates/heddle-cli/src/` too is the stronger claim worth recording.
  `tasks.md`'s `## Next slice` must carry forward every residual slice 017 listed, **strike the ACP
  permission gate item** (this slice closes it), and add the two discovered residuals from *What was
  verified*.

---

## Validation

### The project's own gates (ADR-0004 D1(c)/(d), `docs/QUALITY-GATES.md`)

`cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo build --workspace`; `cargo test --workspace`. Tri-OS CI (`.github/workflows/core.yml`) needs
no edit — the workspace is `members = ["crates/*"]` and the workflow's `paths:` already reads
`crates/**`. No `#[cfg]` anywhere: the fixture uses `TempDir` and `Path::join`, and `FsRoot`
canonicalizes both sides of its containment check already. The tri-OS caveat of slices 004–017
stands unamended — the Windows leg is observed locally, macOS and Linux remain unobserved until
this repository has a remote.

### New tests — both in `crates/heddle-cli/tests/cli_acp_agent.rs`

**`an_acp_client_that_allows_lets_a_real_fs_write_execute`**

`StubProvider::serving(vec![tool_call_reply("fs_write", json!({"path": "planted.txt", "content": "planted by the model"})), reply("written", "stop", 7)])`,
a fresh `TempDir` as `--fs-root`, silo `"phi"`, answer `PermissionOptionKind::AllowOnce`. Asserts:

- `stop == StopReason::EndTurn`, and `chunks(&updates)` contains `"written"`.
- `asked.len() == 1`; `asked[0].session_id == SessionId::new("heddle-1")`;
  `asked[0].tool_call.tool_call_id == ToolCallId::new("fs_write")`;
  `asked[0].tool_call.fields.title.as_deref() == Some("fs_write")`; and the options are exactly
  `[("heddle.allow-once", AllowOnce), ("heddle.reject-once", RejectOnce)]` in that order (D4).
- **The write is on disk**: `std::fs::read_to_string(fs_root.join("planted.txt")) == "planted by the model"`.
- The model was told the truth: `last_message(&provider.request_body())` on the **second** request
  starts with `[tool_result tool=fs_write status=ok]` and contains `wrote 20 bytes to planted.txt`
  (the exact rmcp JSON wrapper around that string is whatever `governed_fs_run.rs`'s `ok` case
  already shows — match it, do not invent it).
- `logged_kinds(&root, "phi", "heddle-1#1")` == `["iteration_boundary", "llm_request",
  "llm_response", "budget_spent", "tool_call", "approval", "tool_result", "iteration_boundary",
  "llm_request", "llm_response", "budget_spent", "exit"]` — the same twelve
  `chat_with_an_fs_root_advertises_the_read_tools_and_reads_a_real_file` pins for `heddle chat`.
- `heddle ledger verify --run heddle-1#1` exits 0 with `heddle-1#1\tok\t12 steps\n` (Principle V,
  verified the way every prior slice verifies it — through the chain, in a second process).

**`an_acp_client_that_rejects_stops_the_fs_write_and_the_run_survives`**

Identical fixture and script, silo `"chi"`, answer `PermissionOptionKind::RejectOnce`. Asserts:

- `asked.len() == 1` with the same request shape — a refusal is still an ask.
- **The Constitution VI proof**: `!fs_root.join("planted.txt").exists()`, with the comment that the
  allow test above proves the very same call *does* create it, so the absence is the effect the
  server would have had (D3).
- `last_message(&provider.request_body())` on the second request starts with
  `[tool_result tool=fs_write status=denied]` and contains `acp client declined permission` and
  `heddle.reject-once` — the client's answer, verbatim, inside the payload the next `llm_request`
  step records.
- `logged_kinds(&root, "chi", "heddle-1#1")` == the allow list **with `"tool_result"` removed**, and
  `heddle ledger verify --run heddle-1#1` == `heddle-1#1\tok\t11 steps\n`. Same shape as
  `an_unlisted_write_never_reaches_the_server`'s `[ToolCall, Approval]`, at a different refusing
  layer — matched, not reinvented (D1).
- `stop == StopReason::EndTurn`: a governed refusal is history the run survives, not an error.
- `updates` contains a `SessionUpdate::ToolCall` and **no** `ToolCallUpdate` carrying
  `ToolCallStatus::Completed`. Assert the absence only — the `Pending`-forever projection is a
  recorded residual, and asserting it positively would freeze behaviour the slice does not endorse.

### Success criteria for `spec.md`

- **SC-001** An ACP client answering `AllowOnce` over the real protocol to the real `heddle acp-agent`
  binary lets `fs_write` execute; the file exists on disk with the model's exact content.
- **SC-002** An ACP client answering `RejectOnce` under the identical fixture leaves **no file on
  disk**.
- **SC-003** Both runs' chains verify with `verify_chain` (via `heddle ledger verify`), at 12 and 11
  steps respectively; the deny chain differs from the allow chain by the absence of `tool_result`
  and nothing else.
- **SC-004** The permission request observed **by the client** carries the session id, the tool name
  as its `tool_call_id` and title, and exactly the two documented option ids and kinds.
- **SC-005** The model is told `status=ok` with the byte count on Allow and `status=denied` with the
  selected option id on Deny; the run reaches `StopReason::EndTurn` in both.
- **SC-006** No file under `crates/*/src/` changes. `cargo test --workspace` is T1 **+2**, with
  every target except `cli_acp_agent` unchanged.

---

## Risks and rollback

**Blast radius: one test file.** Nothing in `src/` changes, no manifest changes, no new dependency.
Rollback is `git checkout dev -- crates/heddle-cli/tests/cli_acp_agent.rs`, or deleting the branch.

| Risk | Mitigation |
|---|---|
| **The client's permission handler deadlocks the dispatch loop**, so `ask` never gets an answer and `AcpPermissionTransport::ask`'s untimed `rx.recv()` hangs the child forever. | Verified impossible for a handler that only records and responds: `src/concepts/ordering.rs` states `on_receive_request` runs *in* the loop and must not `block_task()` — this handler does neither, it responds synchronously. On the agent side `HeddleAgent::serve` already spawns the prompt onto an OS thread for exactly this reason. Belt and braces: the file's existing `run_with_timeout` fails at 60s instead of hanging CI on three OSes, and an orphaned child is reaped when the test binary exits. |
| **The two `Selected` arms of `AcpPermissionTransport::call` are keyed on string constants** (`"heddle.allow-once"`), so a client that answers with a hand-built id rather than one of the offered options silently denies. | The harness selects `request.options.iter().find(\|o\| o.kind == answer)` — the offered option, never a literal — which is `acp_session.rs`'s existing pattern and is what a real editor does. SC-004 pins the literals separately, so a drift in either constant fails loudly instead of turning every Allow into a denial. |
| **The stub's two-body script desynchronises** because a denied first turn changes how many provider requests happen. | Verified from `NativeLoop::mediate`: a `ToolDenied` becomes a fed-back `Message` and the loop continues, so both branches make exactly two provider requests. Both tests therefore use the same two-body script, and `request_body()` is read in order — the second read is the assertion. |
| **`fs_write` fails for a reason unrelated to permission** (missing parent directory) and the deny test passes vacuously. | The allow test is the control: same fixture, same path, and it asserts the file *does* appear. `WriteParams`' path resolves via `FsRoot::resolve_new` against the `--fs-root` root itself, whose parent trivially exists. |
| **A pre-existing test in the file breaks**, meaning the harness perturbed shared state. | T5 makes it a stop condition. The two new tests use their own silos (`phi`, `chi`) and their own `TempDir`s, and `#[test]`s in one binary run in separate threads with no shared fixture in this file. |
| **`heddle-1` is not the session id** because Rust ran another test in the same *process* that opened a session first. | Not a risk: each test spawns its **own** `heddle` child process, and the facade's `AtomicU64` starts at 1 per process — the reasoning the existing headline test already records and relies on. |

---

## Out of scope

- **Any change under `crates/*/src/`.** Including the two residuals discovered while verifying (the
  `ToolCallId` correlation mismatch, and the `Pending`-forever projection of an ACP-denied call).
  Both are recorded in `tasks.md`'s `## Next slice`; neither is fixed here.
- **A new `StepKind` or a second `Approval` step** recording the client's answer explicitly — see
  D1's rejected alternative and the request's own "match that shape rather than inventing a new one".
- **The `Cancelled` outcome.** `p3_a_cancelled_answer_denies_without_reaching_the_transport` already
  covers it at unit level, and the request scopes this slice to two outcomes.
- **`AllowAlways` / `RejectAlways`, or any "remember my answer" persistence.** The transport offers
  neither kind; adding one is a feature, not a proof.
- **New mutating tools, new `ToolAccess` classification, any UI.**
- **A live-model test.** A live model cannot be made to answer Deny and cannot be relied on to call
  `fs_write`; determinism in both branches is the whole point of the `StubProvider`. Slice 017's
  T13 hand-verification pattern does not transfer.
- **The other named residuals**, all carried forward untouched: the `canonicalize`-to-open TOCTOU
  fix, `role: "tool"` / `tool_call_id` conversation replay, raw wire-byte capture, streaming (SSE),
  provider authentication, a config file, `--json` output, and the slices-008-vs-014
  `serde_json/preserve_order` reconciliation.
- **`spikes/`** (ADR-0004 D2 quarantine), `crates/heddle-silo/`, `.github/`, `rust-toolchain.toml` —
  all asserted empty in T6's control diff.
- **A PR.** This repository has no git remote (verified); the branch is merged locally like every
  prior slice. Conventional Commits throughout.
