# Plan — slice 014: redact secrets on the `LlmRequest`/`LlmResponse` Ledger path

**Target artifacts:** `specs/014-ledger-redaction/spec.md`, `specs/014-ledger-redaction/plan.md`,
`specs/014-ledger-redaction/tasks.md` · **Branch:** `014-ledger-redaction` cut from `dev` · **No PR**
(no git remote) · **Strict TDD** (Constitution III) · **Conventional Commits**.

Everything named below was read in the working tree on branch `dev` this session. Where the request's
description and the source disagree, the source wins and the disagreement is recorded.

---

## Problem

`NativeLoop::run` (`crates/skein-core/src/native_loop.rs`) appends the model conversation to the
Ledger raw:

```rust
ledger.append(run_id, StepKind::LlmRequest,  serde_json::to_string(&req)?)?;
let resp = self.client.turn(&req)?;
ledger.append(run_id, StepKind::LlmResponse, serde_json::to_string(&resp)?)?;
```

`ToolGateway::call_captured` (`crates/skein-core/src/tool.rs`) does the opposite for its own steps:
it holds a `Redactor`, scrubs `call.args` through `Redactor::redact_value` before the `ToolCall`
step, and scrubs `outcome.content` through `Redactor::redact` before the `ToolResult` step — while
handing the **raw** `ToolOutcome` back to the trusted caller. Two collaborators writing the same
chain, one governed and one not.

Since slices 012 and 013 shipped, `skein chat` and `skein acp-agent` drive real conversations through
a real provider, so the payloads that land unredacted are real user prompts and real model output,
written to a durable SQLite-backed silo (slice 009) and readable forever after via
`skein ledger show`. Constitution VI ("secrets by reference, never by value… redacted from logs") and
Principle V's replayability promise together turn this from a transient leak into a permanent one.

The gap is named honestly in `specs/011-skein-cli/spec.md`'s Assumptions and repeated on 012's and
013's "Next slice" lists; 013's Constitution Check carries it forward verbatim under V ("the chain
holds the *translated* `TurnRequest`/`TurnResponse`, and model I/O is **not** redacted").

### A second leak found while reading, not in the request

`ToolGateway::call_captured` redacts `call.args` but copies `call.tool` **verbatim** into the
`ToolCall` step's `attempt.tool`, into `ApprovalRecord.tool` for the `Approval` step, and into
`CapturedResult.tool` for the `ToolResult` step. The tool name is model-chosen text (013's spec
says so explicitly: *"the tool name is model-chosen, so omission may not mean permission"*), so it is
exactly as capable of carrying an echoed secret as the arguments are. This is in scope: the slice's
own invariant is *"one configured secret is scrubbed everywhere in the chain"*, and leaving it would
make that claim false. It is three lines and one test.

### Full `StepKind` audit (the invariant asks for it)

Enumerated against `crates/skein-core/src/ledger.rs` and every `ledger.append` call site in product
code:

| `StepKind` | Payload today | Verdict |
|---|---|---|
| `LlmRequest` | `serde_json::to_string(&TurnRequest)` — full conversation | **leaks; fixed here** |
| `LlmResponse` | `serde_json::to_string(&TurnResponse)` — model output + `tool_calls` | **leaks; fixed here** |
| `ToolCall` | `{tool: raw, args: redacted}` | **`tool` leaks; fixed here** |
| `Approval` | `{tool: raw, decision, reason}` — `reason` is policy-authored constant text | **`tool` leaks; fixed here** |
| `ToolResult` | `{tool: raw, content: redacted}` | **`tool` leaks; fixed here** |
| `IterationBoundary` | `(ctl.iters()+1).to_string()` | safe (integer) |
| `BudgetSpent` | `resp.tokens_used.to_string()` | safe (integer) |
| `Exit` | `format!("{exit:?}")` on a fieldless enum | safe |
| `StateChange`, `Reflection` | **never appended by product code** — only `crates/skein-silo/tests/silo_ledger.rs` appends a `StateChange`, with a literal | nothing to redact today |

No other Ledger payload carries model- or user-authored text.

---

## Approach

### D1 — `NativeLoop` gets its own `Redactor`, injected as a required constructor argument

`NativeLoop::new` becomes `new(client, probe, gateway, redactor)` and `NativeLoop` gains a **private**
`redactor: Redactor` field. `client`, `probe` and `gateway` stay `pub` (a caller inspects the
collaborators it injected — every existing test does, e.g. `lp.client.calls`, `lp.gateway.transport`);
the redactor is not something a caller reads back, so it stays private.

`run` then writes:

```rust
ledger.append(run_id, StepKind::LlmRequest,  self.redactor.redact_json(&req)?)?;
let resp = self.client.turn(&req)?;
ledger.append(run_id, StepKind::LlmResponse, self.redactor.redact_json(&resp)?)?;
```

`req` is still built raw from `messages` and `&req` is still what `self.client.turn` receives; `resp`
is still returned raw through `LoopRun.final_message`. Only the two `append` arguments change. This
is the exact shape `call_captured` already has: *"The tool needs the real secret; only the record must
not have it."*

**Why a required argument rather than a default.** Constitution VI is deny-by-default. A
`with_redactor(self, r)` builder, or a `Default`-empty redactor, makes "no redaction" the silent
default — which is precisely the bug being fixed, reintroduced as an API affordance. Making it the
fourth positional argument means a caller cannot forget it, and the compiler enumerates every site.
This mirrors slice 006's justification for `ToolGateway` being a concrete constructor argument on
`NativeLoop` rather than something the loop discovers.

**Rejected: `NativeLoop` borrows `ToolGateway`'s redactor** (e.g. a new `ToolGateway::redactor()`
accessor). It couples two collaborators that Principle IV says must each be independently injectable,
it makes the loop's redaction silently depend on how the gateway was built, and it forces the gateway
to exist for a reason unrelated to tools.

**Rejected: put the `Redactor` on `Ledger`** — one constructor change, zero call-site churn, and it
would cover every `StepKind` at once. It loses on three counts. (a) `Ledger` is constructed in crates
that have no business knowing about secrets: `Silo::ledger()` in `skein-silo`, `Ledger::new()`,
`Ledger::open(store)`; and it is constructed on the **read** path too (`skein ledger show`), where a
redactor is meaningless. (b) `Ledger::append` computes the hash chain; making redaction implicit
there means the chain's content depends on invisible constructor state, so the same run replayed
through a differently-constructed `Ledger` hashes differently — a direct hit on Principle V. (c) The
Ledger is a *record*, not a policy point; redaction is governance and belongs beside the other
governance (`ToolPolicy`, `Redactor`) rather than inside the thing being governed.

### D2 — `Redactor::redact_json`, a new public method; `redact_value` stays private

```rust
/// Serializes `value` and scrubs the strings inside it, leaving its shape
/// intact — so the captured payload stays parseable for replay.
pub fn redact_json<T: Serialize + ?Sized>(&self, value: &T) -> Result<String> {
    Ok(self.redact_value(&serde_json::to_value(value)?).to_string())
}
```

This reuses the existing, already-tested `redact_value` recursion (`Value::String` → `redact`,
arrays and objects recursed, object **keys** redacted too, other scalars cloned) rather than
inventing a second mechanism. `redact_value` itself stays private: `ToolGateway` is in the same
module and the only external need is the serialize-then-scrub shape.

**Rejected: string-level redaction** (`self.redactor.redact(&serde_json::to_string(&req)?)`). It
would preserve key order and touch no new API, and it is wrong twice over. A secret containing `"`,
`\`, or a newline is JSON-**escaped** inside the serialized payload, so the literal needle does not
appear and the secret is **missed entirely**; and a replacement that does land can straddle an escape
sequence and produce unparseable JSON. This is the failure mode the slice invariant names.

**Known, verified consequence — object key order changes.** `serde_json::to_value` produces a
`serde_json::Map` which is a `BTreeMap` (the workspace declares plain `serde_json = "1"`; the
`preserve_order` feature is **not** enabled — confirmed in `Cargo.toml` and every crate manifest), so
`LlmRequest`/`LlmResponse` payloads become alphabetically keyed: `{"messages":…,"run_id":…}` rather
than `{"run_id":…,"messages":…}`. Every consumer in the tree was checked and is order-independent:

- `skein-acp`'s `project_updates` — `serde_json::from_str::<TurnResponse>`
- `crates/skein-acp/tests/acp_session.rs` — `from_str::<TurnResponse>`
- `crates/skein-gateway/tests/governed_run.rs` — `from_str::<TurnResponse>`, plus one
  `payload(...).contains("anyone home?")` substring assertion
- `crates/skein-core/tests/native_loop.rs` — every LlmRequest assertion goes through
  `from_str::<TurnRequest>`
- `skein ledger show` prints the payload verbatim; no test pins llm-step payload text
- No test hardcodes a step id, so the changed hashes are invisible

State this in the spec as a deliberate, checked consequence rather than letting an implementer
discover it.

### D3 — `Redactor: Clone`, hand-written; `SecretValue` stays non-`Clone`

One run must configure **one** secret set used by both collaborators. `Redactor` is not `Clone`
today and `SecretValue` deliberately is not either. Add only:

```rust
impl Clone for Redactor {
    fn clone(&self) -> Self {
        Redactor { secrets: self.secrets.iter().map(|s| SecretValue::new(s.expose())).collect() }
    }
}
```

Both copies are `Zeroizing` and both zeroize on drop; `SecretValue`'s hand-written `Debug` and its
`expose()` opt-in are untouched, and `secret.rs`'s public API does not widen. The empty-secret filter
in `from_values` does not need re-applying: the source vector is already filtered.

**Rejected: `Arc<Redactor>` on both constructors.** One copy of the plaintext instead of two, but it
changes `ToolGateway::new`'s signature as well — ten more call sites — and puts an `Arc` in a public
constructor signature purely to save one small allocation of material that is already resident in
process memory. **Rejected: build two independent `Redactor`s at each wiring site.** No new API at
all, but two independently-configured redactors on the same run can silently diverge, which defeats
"one configured secret is scrubbed everywhere in the chain".

### D4 — `ToolGateway` redacts the tool name too

In `call_captured`, `attempt.tool`, `ApprovalRecord.tool` and `CapturedResult.tool` become
`self.redactor.redact(&call.tool)`. The **decision** still uses the raw name
(`self.policy.decide(&call.tool)`) and the transport still receives the raw `call` — only the three
recorded copies change. `CapturedResult.tool` also feeds `tool_message` back into the conversation,
so the model sees `***` where it emitted a secret-bearing name; that is consistent with
`captured.content`, which the loop already feeds back redacted for exactly this reason.

### D5 — `skein-acp`'s public API does not change; `SkeinSession::new` clones

`SessionParts` already has a single `redactor: Redactor` field. `SkeinSession::new` currently moves it
into `ToolGateway::new`; it will instead clone it into the gateway and pass the original to
`skein_core::NativeLoop::new`. No field added, no bound changed, no caller of `SessionParts` touched
beyond what already exists. This is the smallest possible change to that crate and keeps the
"operator supplies the undecorated ports" contract intact.

**Consequence, documented not hidden:** `project_updates` derives `AgentMessageChunk` text from the
`LlmResponse` **Ledger payload**, so an ACP client's transcript now shows `***` where a configured
secret appeared — the same property `ToolResult` content already has, and which that function's own
comment already states ("Straight from the chain, so it is redacted for the same reason the chain
is"). `skein chat`'s stdout is **not** affected: it prints `run.final_message`, which is the raw
`resp.message`. The two commands therefore differ, and the spec says so plainly rather than leaving a
reader to find out.

### D6 — the CLI gains a real way to configure a secret, **in this slice**

Decided: **included, not deferred.** Without it `Redactor::new(vec![])` remains the only thing
`skein chat`/`skein acp-agent` construct, the fix has no caller in the shipped binary, and the slice
would close a gap only in principle. "No capability without a current caller" (Constitution VII) cuts
in favour of shipping the caller. The cost is ~30 lines and two tests, because every piece already
exists: `SecretRef`, `SecretProvider`, `Redactor::resolve`, `skein-silo`'s `OsKeychain` (which
implements `SecretProvider` and reports `requires_network() == false`), and `skein secret set` as the
provisioning path.

New in `crates/skein-cli/src/wiring.rs`, beside `ModelArgs`:

```rust
/// Which secrets this run must never write into its chain. References only:
/// there is no `--redact-value`, for the reason `skein secret set` has no
/// `--value` (shell history, `ps`).
#[derive(Args)]
pub struct RedactArgs {
    /// keychain://<service>/<account>. Repeatable.
    #[arg(long = "redact", value_name = "REFERENCE")]
    pub redact: Vec<String>,
}

impl RedactArgs {
    pub fn redactor(&self) -> Result<Redactor> { … }
}
```

`redactor()` returns `Redactor::new(vec![])` **without opening the credential store** when `--redact`
is absent — otherwise every `skein chat` would acquire a runtime keychain dependency, and the nine
existing `cli_chat`/`cli_acp_agent` tests (which run headless) would start depending on a platform
credential store. With references present it resolves them through `OsKeychain::new()?` via
`Redactor::resolve`, whose documented all-or-nothing failure ("a `Redactor` built from a
misconfigured reference would scrub nothing, and would do it silently") is exactly the behaviour
wanted.

`RedactArgs` is flattened into `ChatArgs` and into the `AcpAgent` subcommand in
`crates/skein-cli/src/main.rs`. It is deliberately **not** added to `ModelArgs`: redaction is
run-governance, not a model knob, and `ModelArgs`'s docstring says what it is for.

**Ordering** in both commands: `model.endpoint()?` first (the Principle II guard, unchanged), then
`redact.redactor()?`, then `Silo::open`. An unresolvable reference is therefore exit 1 with **no
chain opened** — the same rule both commands already document for a non-loopback `--base-url`, and
the reason is identical: a one-step run in a silo would be a misleading record of an attempt that
never left the process.

Both commands pass the same `Redactor` value to the gateway and the loop (`chat.rs` clones it into
`ToolGateway::new`; `acp.rs` resolves once before `serve` and clones per session inside the factory
closure, which needs `Redactor: Clone` — D3).

### D7 — call sites that must change (enumerated before the fix, slice-007 style)

`NativeLoop::new` — **26 sites**, all verified by grep this session:

| File | Count | Shape of the mechanical edit |
|---|---|---|
| `crates/skein-acp/src/lib.rs` (`SkeinSession::new`) | 1 | clone into gateway, original into the loop (D5) |
| `crates/skein-cli/src/chat.rs` | 1 | `redact.redactor()?`, cloned into the gateway (D6) |
| `crates/skein-core/tests/native_loop.rs` | **20** | add a 4th argument |
| `crates/skein-gateway/tests/governed_run.rs` | 2 | `Redactor::new(vec![])` |
| `crates/skein-mcp/tests/rmcp_gateway.rs` | 1 | `Redactor::new(vec![SECRET.into()])` |
| `crates/skein-silo/tests/silo_ledger.rs` | 1 | `Redactor::new(vec![])` |

For `native_loop.rs`'s twenty: the eighteen built with the `no_tools()` helper take
`Redactor::new(Vec::new())`; the ones built with the `gateway(...)` helper (which already carries
`Redactor::new(vec![SECRET.into()])`) take `Redactor::new(vec![SECRET.into()])`, so the loop and the
gateway share the run's secret set. **No assertion in any of the twenty changes** — they stay live
controls on the signature change, exactly as slice 013 kept `acp_session.rs`'s thirteen.

`ToolGateway::new` — **signature unchanged**; its ten call sites (including
`crates/skein-silo/tests/silo_secret.rs` and `crates/skein-core/tests/tool_gateway.rs`) are untouched
except where a caller now clones the redactor it is handing over.

One shared test fixture changes additively: `ScriptedModel` in `crates/skein-core/tests/native_loop.rs`
currently discards its `_req`. It gains a `seen: Vec<TurnRequest>` field so a test can assert the
model received the **raw** value. Additive; no existing assertion touched.

---

## Steps

Ordered; each is independently verifiable. Red observed and recorded before every green
(Constitution III), in `tasks.md`'s `## Observed red` section, mirroring slice 013's format.

- **T0** — `specs/014-ledger-redaction/{spec.md,plan.md,tasks.md}`; branch `014-ledger-redaction` cut
  from `dev` with slice 013 merged. Constitution Check table in `tasks.md` in the exact shape used by
  `specs/013-acp-agent/tasks.md` (one line per principle, I–VIII plus Cross-platform). Principle V's
  line must now read as **closed** rather than carried forward, and Principle VI must name the
  `--redact` caller (D6) so the entry is not a promise.
- **T1** — control baseline: `cargo test --workspace` before any edit. Expected **110 passed, 0
  failed, 1 ignored** (slice 013's recorded gate figure). Record the per-target breakdown; this is
  the number T9 diffs against.
- **T2 (RED→GREEN)** — `Redactor::redact_json` (D2) and `impl Clone for Redactor` (D3), with a test
  in `crates/skein-core/tests/core.rs` beside the existing `redactor_resolves_from_a_provider`:
  a nested `serde_json::Value` round-trips through `redact_json` with its structure intact and its
  secret-bearing strings scrubbed, and a clone of a `Redactor` scrubs what the original scrubs.
  First because nothing else compiles without it.
- **T3 (RED→GREEN)** — `NativeLoop`'s fourth constructor argument and the two `redact_json` calls in
  `run` (D1). Its red is T4's first test, written before this step. Update all 26 sites mechanically
  in the same commit — the workspace does not compile in between, so this is one atomic change.
- **T4 (RED, written before T3's green)** — the three new tests in
  `crates/skein-core/tests/native_loop.rs` (see Validation).
- **T5 (RED→GREEN)** — the tool-name redaction in `ToolGateway::call_captured` (D4), with its test in
  `crates/skein-core/tests/tool_gateway.rs` beside
  `secret_is_redacted_from_args_and_result_before_capture`.
- **T6 (GREEN)** — `SkeinSession::new` clones the injected redactor into both collaborators (D5).
  Covered by T7's test; `skein-acp`'s public API is otherwise unchanged.
- **T7 (RED→GREEN)** — one test in `crates/skein-acp/tests/acp_session.rs` proving a session's chain
  is redacted **and** pinning the `project_updates` consequence (D5).
- **T8 (RED→GREEN)** — `wiring::RedactArgs` and its `redactor()`; `main.rs` flattens it into
  `ChatArgs` and `AcpAgent`; `chat.rs` and `acp.rs` resolve it after the endpoint guard and before
  `Silo::open`, and hand the same value to the gateway and the loop (D6). Its red is the two new
  `cli_chat.rs` tests, written first. `Redactor::new(vec![])` must no longer appear anywhere in
  `crates/skein-cli/src`.
- **T9** — gates (below), control diff, dependency drift, close-out. `git diff dev --
  crates/skein-silo/ spikes/ .github/ rust-toolchain.toml` must be empty except
  `crates/skein-silo/tests/silo_ledger.rs`'s one mechanical `NativeLoop::new` argument — state that
  exception explicitly rather than claiming an empty diff. Expected dependency drift: **zero new
  packages, zero new edges** (`skein-cli` already depends on `skein-silo`; `Arc` was rejected in D3
  and `std` needs no declaration).

---

## Validation

### Existing gates (all four must be clean, Windows leg observed locally; macOS and Linux legs unobserved until the repo has a remote — same standing caveat as specs 004–013)

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` — **110 baseline + 8 new = 118 passed, 1 ignored**, with all 110 existing
  bodies unchanged
- `cargo build --workspace`
- `skein chat --help` and `skein acp-agent --help` both list `--redact <REFERENCE>`

### New tests

**`crates/skein-core/tests/native_loop.rs`** — the acceptance core, deliberately shaped like
`tool_gateway.rs`'s `secret_is_redacted_from_args_and_result_before_capture`:

1. `a_secret_in_the_conversation_is_redacted_from_the_llm_payloads` — prompt text contains `SECRET`
   and the scripted reply text contains `SECRET`; the loop is built with
   `Redactor::new(vec![SECRET.into()])`. Asserts: (a) **no** payload of the run contains `SECRET` —
   scanning every step, not just the two, so a future step type leaking is caught; (b) at least one
   payload contains `***`; (c) `lp.client.seen[0].messages[0].text()` contains the **raw** `SECRET`,
   i.e. the model genuinely received it; (d) `run.final_message.unwrap().text()` contains the raw
   `SECRET`, i.e. the loop's own return value is unaffected and only the record is scrubbed;
   (e) `led.verify_chain(...)` still passes.
2. `the_redacted_llm_payloads_are_still_parseable_turn_request_and_response` — deserializes the
   `LlmRequest` payload into `TurnRequest` and the `LlmResponse` payload into `TurnResponse` and
   asserts the structure survived: `run_id`, `messages.len()`, each message's `role`,
   `tokens_used`, `final_output`, and that the message text is the original with the secret replaced
   by `***` (not truncated, not empty).
3. `a_tool_call_arriving_with_a_secret_in_its_name_is_redacted_from_the_llm_response_too` — a scripted
   `TurnResponse` whose `tool_calls` name embeds `SECRET`. The `LlmResponse` payload contains the
   whole `tool_calls` array, so this is the assertion that fails if only the `ToolCall` step is fixed
   and the response payload is not.

**`crates/skein-core/tests/tool_gateway.rs`**:

4. `a_secret_in_a_tool_name_is_redacted_from_the_attempt_and_the_approval` — a call to a
   secret-bearing name is denied by the empty/unlisted-name path; asserts no payload of the run
   contains `SECRET`, the `Approval` payload contains both `***` and `denied` (so the policy still
   saw the **raw** name and refused it), and `gw.transport.calls == 0`.

**`crates/skein-acp/tests/acp_session.rs`**:

5. `a_secret_is_redacted_from_a_sessions_chain_and_from_the_client_transcript` — a session wired with
   `redactor: Redactor::new(vec![SECRET.into()])` and a scripted model that echoes it; asserts no
   payload on the session's chain contains `SECRET`, and that the `AgentMessageChunk` produced by
   `project_updates` shows `***`. Pins D5's consequence as intended behaviour, not an accident.

**`crates/skein-cli/tests/cli_chat.rs`** — the proof the shipped binary has a caller:

6. `chat_redacts_a_configured_secret_from_the_chain_but_not_from_stdout` — stores a value in the real
   platform credential store under a per-process, per-test reference removed by a `Drop` guard
   (the established `TestRef` pattern from `crates/skein-cli/tests/cli_secret.rs`, which already runs
   green on all three CI legs); the existing `StubProvider` echoes that value; runs
   `skein chat --redact <ref> …`; asserts the value appears on **stdout** (the operator still gets the
   real answer) and that the silo's chain contains `***` and never the value.
7. `chat_refuses_an_unresolvable_redaction_reference_before_opening_a_chain` — exit code 1, stdout
   empty, stderr names the reference, and the silo's ledger file **does not exist**.

**`crates/skein-cli/tests/cli_acp_agent.rs`**:

8. `acp_agent_refuses_an_unresolvable_redaction_reference_before_serving` — exit 1, stdout empty
   (stdout is the protocol), no chain opened. The redaction-on-chain behaviour itself is already
   proven by tests 1–5 and 6; this one only pins the ordering guarantee for the second command.

No test here requires a running Ollama or an installed editor.

---

## Risks and rollback

**Blast radius.** Five of seven crates: `skein-core` (product code, `native_loop.rs` + `tool.rs`),
`skein-acp` (one function body), `skein-cli` (product code, three files), and test files in
`skein-gateway`, `skein-mcp`, `skein-silo`. `skein-silo`'s and `skein-gateway`'s **product** code is
untouched.

- **A mechanical edit that is not mechanical.** Twenty-six call sites is enough for a slip to hide in.
  Mitigation: the compiler rejects every missed site (a required positional argument, D1), and every
  one of the 110 existing tests must pass with a **byte-identical body** — verify with
  `git diff dev -- crates/skein-core/tests/native_loop.rs` showing only added arguments and the
  additive `ScriptedModel.seen` field.
- **Key reordering breaks something unexamined.** Mitigated by the D2 audit; the residual risk is a
  consumer outside `crates/` — there is none, `skein ledger show` prints verbatim, and no test pins a
  step id. If it does bite, `cargo test --workspace` catches it at T3.
- **The ACP transcript change surprises a user.** A configured secret now renders as `***` in the
  editor's agent message. This is intended (D5) and pinned by test 5, but it is a user-visible
  behaviour change and belongs in the spec's "What this slice changes for a user" section, not only
  here.
- **Credential-store flakiness in tests 6–8.** Bounded: only tests 6 and 7 touch the keychain, they
  follow `cli_secret.rs`'s already-green `Drop`-guarded pattern, and `RedactArgs::redactor()` never
  opens the store when `--redact` is absent, so the nine pre-existing CLI tests keep running headless.
- **A second plaintext copy of each secret** (D3's `Clone`). Both copies are `Zeroizing`; the material
  is already resident. Accepted, and named in the spec's Assumptions rather than left implicit.
- **Not a defence against a secret the operator never configured.** Redaction only scrubs values in
  the run's `Redactor`. A credential pasted into a prompt that was never registered via
  `skein secret set` + `--redact` still lands in cleartext. This slice makes redaction *possible and
  wired*; it does not make it automatic, and the spec must say so plainly instead of claiming the
  class of leak is closed.

**Rollback.** The whole slice is one branch off `dev` with no migration and no persisted-format
change beyond payload key order in **new** steps. `git branch -D 014-ledger-redaction` reverts
everything. Chains written before the slice remain readable and verifiable afterwards: `verify_chain`
recomputes from the stored payload, so pre-existing steps still hash correctly, and
`TurnResponse.tool_calls` is already `#[serde(default)]` for old payloads. Partial rollback is
available at a seam: T8 (the CLI `--redact` wiring) can be dropped on its own, leaving the core fix
in place with `Redactor::new(vec![])` at the wiring sites — but that is exactly the "no caller"
outcome D6 rejects, so it is a rollback option, not a fallback plan.

---

## Out of scope

- **Raw wire-byte capture** (the HTTP request/response bodies `skein-gateway` exchanges). A separate,
  already-named "Next slice" item.
- **Provider authentication / a provider token as a `SecretRef`.** 013's Constitution Check pre-wrote
  that constraint for a later slice; this slice adds no auth path.
- **Automatic secret detection** — entropy heuristics, `sk-`-prefix matching, or any redaction of
  values the operator did not configure. `Redactor` is an exact-value scrubber and stays one.
- **Redacting `SkeinError` messages, stderr, or `skein chat`'s stdout.** The invariant is about
  *Ledger payloads*. `chat`'s stdout carrying the raw answer is the intended contract (test 6).
- **A config file for secret references.** v0 has none (`SiloArgs`' docstring says so); `--redact` is
  repeatable flags and `$SKEIN_ROOT`-style environment fallback is not added for it.
- **Changing `ToolGateway::new`'s signature**, adding `Arc` anywhere, or making `SecretValue: Clone`.
- **`spikes/`** — untouched (ADR-0004 D2).
- **Widening the ACP surface**, adding streaming, tool advertisement, or a `--json` output mode.
