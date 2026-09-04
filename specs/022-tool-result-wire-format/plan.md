# Implementation Plan: slice 022 — tool results reply as `role:"tool"` with a real `tool_call_id`

**Spec:** `specs/022-tool-result-wire-format/spec.md` · **Branch:**
`022-tool-result-wire-format`, cut from `dev` at `6873137` · **No PR** (this repository has no
remote) · Conventional Commits · Strict TDD (Constitution III).

## Problem

`crates/skein-core/src/native_loop.rs`'s `mediate` fed every tool result back through the free
function `tool_message`, which returned
`Message::user_text(format!("[tool_result tool={tool} status={status}]\n{body}"))`. The assistant
turn that *requested* those tools was pushed as `resp.message`, which
`crates/skein-gateway/src/lib.rs`'s `ModelClient for OpenAiCompatClient::turn` built as
`Message::assistant_text(choice.message.content.unwrap_or_default())` — and on a tool-calling turn a
provider sends `"content": null`, so that message was **empty**. The `tool_calls` array was carried
on `TurnResponse.tool_calls`, recorded in the Ledger's `LlmResponse` step, and then dropped:
`struct ChatMessage` serialized `{role, content}` and nothing else, and `struct ResponseToolCall`
did not even deserialize the provider's `id`.

So the conversation replayed to the provider on turn *N+1* was:

```
user      "…"
assistant ""                                        <- the tool-calling turn, emptied
user      "[tool_result tool=fs_read status=ok]\n…"
user      "[tool_result tool=fs_read status=ok]\n…"
```

Two facts the model itself produced were gone: **which tool calls it made**, and **which result
answers which call**. Correspondence survived only as message ordering, which nothing told the model
about. `spec.md`'s finding 4 measures what that costs: 0/6 vs 6/6 on two independent local models.

## What was verified before planning, rather than inherited from the comments

| Claim, as the prose had it | Verified against the tree at `6873137` |
|---|---|
| "changing this reopens an anti-injection decision Constitution VI backs" (slice 015) | **Partly false as implied.** The origin (slices 005/006) says the label is *"a marker, not a security boundary"* and names a typed variant as *"the real fix"*, deferred. Quoted in full in `spec.md`'s register. |
| "a stricter hosted provider may reject this" (the originating request) | **Refuted.** Ollama accepts every shape including malformed strict; and no TLS backend is compiled into `ureq`, so a hosted provider is unreachable. |
| `tool.rs` may import from `content.rs`, so a `ToolCall` in `content.rs` would cycle | **False, and the direction is the safe one.** `tool.rs` imports `error`, `ledger`, `secret` — not `content`. `content.rs` importing `ToolCall` from `tool.rs` is acyclic. |
| "seven test files assert the old label" | **Confirmed by grep**: seven files, eighteen sites — `native_loop.rs`, `governed_fs_run.rs`, `governed_git_run.rs`, `cli_chat.rs`, `cli_acp_agent.rs`, `acp_session.rs`, `rmcp_gateway.rs`. An eighth test file changes without ever having asserted the label: `openai_compat.rs`, whose `Message { role, parts }` struct literal gains the new fields. |
| `Role` is matched exhaustively in exactly one place outside `content.rs` | **Confirmed by grep**: `impl From<&Message> for ChatMessage`. The compiler finds every site. |
| `skein-acp/src/lib.rs` needs no change | **Confirmed.** `project_updates` reads `Step` payloads and `Message::text()`, both unchanged — but only because of D2's `#[serde(default)]`. |

## Decisions

### D1 — the trust boundary moves from a text label to `Role::Tool`

`crates/skein-core/src/content.rs`'s `enum Role` gains a `Tool` variant; `Message` gains two
serde-defaulted fields, `tool_calls: Vec<ToolCall>` (assistant only) and
`tool_call_id: Option<String>` (`Role::Tool` only), each with `skip_serializing_if`. `content.rs`
imports `ToolCall` from `tool.rs`; the direction is acyclic (verified above).

Constructors: `user_text` / `assistant_text` keep their signatures and default the new fields;
`Message::tool_result(tool_call_id, body)` sets `Role::Tool`; `Message::with_tool_calls(self, calls)`
is the combinator the loop uses on the echo. `Message::text()` is unchanged, so `skein-acp`'s
`project_updates` and the gateway's `content` field behave exactly as before.

**How this satisfies P-VI, and why it is strictly stronger than the label.** `Role::Tool` is
producible by exactly one constructor, reachable from exactly one call site
(`NativeLoop::mediate`). No operator prompt, no model output and no tool body can produce one: a
prompt containing the literal text `[tool_result tool=fs_write status=ok]` was **byte-identical on
the wire** to a real tool result before this slice, and is now unambiguously a `{"role":"user"}`
message. On the OpenAI wire a `tool` message's `content` is an opaque JSON-escaped string carrying
no instruction-bearing structure, so tool bytes cannot escape into message structure. That is what
Constitution VI's *"external content is data, never instruction"* can be given at this layer. What
it does **not** give — and `spec.md` says so plainly, because finding 2 proves it — is *obedience*
resistance.

**Rejected: a `Content::ToolResult { … }` / `Content::ToolCalls { … }` variant**, which is what
slices 005 and 006 literally named. Three reasons:

1. `Content`'s own doc comment scopes it to a **modality** axis (*"v0 carries Text; image/audio/doc/
   video land in v2 without changing the pipeline"*). The tool boundary is a **role/structure** axis.
2. It would force `Message::text()` — used by `skein-acp` to build the ACP transcript and by the
   gateway to build `content` — to decide whether a tool result is "text", changing the ACP
   transcript as a side effect of a wire-format slice.
3. It mismatches the only wire format this workspace speaks: OpenAI puts `tool_call_id` on the
   **message**, not on a content block.

Message-level fields map 1:1 to the wire, keep `Content` a pure modality enum, and leave the v2
media work untouched. The deferred *intent* — a typed boundary rather than a text marker — is
honoured; only its location moves.

### D2 — `ToolCall` gains a defaulted `id`; `ToolCall::new` keeps its signature

`ToolCall` gains `#[serde(default)] pub id: String`. `ToolCall::new(tool, args)` keeps its
two-argument shape and sets `id: String::new()`; `ToolCall::with_id(id, tool, args)` is added.

`ToolCall::new` has ~44 call sites across eight test files; a required third argument would rewrite
all of them for no behavioural reason and bury this slice's real assertions in churn.

`#[serde(default)]` is **load-bearing beyond ergonomics**: `skein-acp`'s `project_updates`
deserializes `ToolCall` out of old `StepKind::ToolCall` payloads inside a `let Ok(…) else
{ continue }`, so a non-defaulted field would make every pre-022 chain **silently** lose its
tool-call updates. Same reasoning as `TurnResponse::tool_calls`'s existing `#[serde(default)]`.

### D3 — the gateway parses the provider's id, and synthesizes one when the provider omits it

`struct ResponseToolCall` gains `#[serde(default)] id: Option<String>`. In `turn`, the `.enumerate()`d
mapping uses `c.id.unwrap_or_else(|| format!("call_{i}"))`. Ollama does supply ids (verified live),
but the OpenAI-compat ecosystem does not guarantee it, and a `""` id reaching the echo would produce
a self-inconsistent request. Normalizing at the port boundary means **every `ToolCall` that leaves
`skein-gateway` has a non-empty id**, so the loop needs no fallback and FR-005 has a single guard.

### D4 — the loop echoes the *redacted* calls and replies per id

In `NativeLoop::run`, `messages.push(resp.message)` becomes a push of
`resp.message.with_tool_calls(echoed)`, where `echoed` is each of `resp.tool_calls` through the
loop's own `Redactor`. In `mediate`, `tool_message(...)` becomes
`Message::tool_result(call.id.clone(), body)` with `body`:

- `Ok((_, captured))` → `captured.content` — the redacted capture, exactly as the existing comment
  requires (*"the history is replayed into the next request's payload, so feeding back the real
  secret would put it straight back on the chain"*).
- `Err(SkeinError::ToolDenied { tool, reason })` → `format!("the {tool} tool call was refused:
  {reason}")`, with `tool` through the redactor for the same reason `call_captured` redacts it.

The free function `tool_message` is **deleted** — a replacement, not an addition beside it. The
`[tool_result tool=… status=…]` prefix goes away entirely: the role carries the boundary, the
`tool_call_id` carries the identity, and MCP's `CallToolResult` already carries `"isError"` in the
body, so a hand-written `status=ok` restated the payload. The one case that genuinely needs words is
a *gateway* denial, which is why that branch keeps a sentence.

**Redacting the echo is a decision, not an oversight.** The provider already saw the raw call — it
produced it — so redacting costs no secrecy that was not already spent, and it keeps the wire and
the Ledger's `LlmRequest` capture byte-identical, which is what makes `governed_fs_run.rs`'s
existing *"the captured request and the wire must agree"* assertion still mean something.
`Redactor::redact_value` is private to `tool.rs`, so `pub fn redact_call(&self, call: &ToolCall) ->
ToolCall` is added and `ToolGateway::call_captured`'s `attempt` construction uses it — extracting a
duplication rather than widening the API for a new caller.

**Why the "dangling id" objection is answered structurally, not by care.** `mediate` iterates
`calls` — i.e. `resp.tool_calls`, the exact array being echoed — and pushes exactly one message per
element, in order, on every branch: `Ok` and `ToolDenied` both produce one, and the only other
branch (`Err(e) => return Err(e)`) ends the run before `messages` is extended at all. `run` then
does `messages.push(assistant_with_echo)` immediately followed by `messages.extend(feedback)`. So
FR-005 is a property of the control flow. That is the concrete, tested answer slice 015 asked for.

### D5 — the gateway serializes both directions

`struct ChatMessage` gains `#[serde(skip_serializing_if = "Vec::is_empty")] tool_calls:
Vec<ChatToolCall<'a>>` and `#[serde(skip_serializing_if = "Option::is_none")] tool_call_id:
Option<&'a str>`; `impl From<&Message> for ChatMessage` maps `Role::Tool => "tool"` and fills them.
A new `ChatToolCall { id, #[serde(rename="type")] kind: "function", function: ChatCallFunction
{ name, arguments: String } }` mirrors the response side, with `arguments` produced by
`serde_json::to_string(&call.args)` — a JSON *string* holding JSON, per the wire format and per
`ToolFunction::arguments`'s existing doc comment. Field order is the wire order (`ChatRequest`'s
existing struct-not-`json!` rule); the new fields serialize **last** so a message without them puts
exactly the bytes on the wire it put there before (FR-009).

## Validation

The project's own gates, exactly as `.github/workflows/core.yml` runs them:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

plus, by hand on the Windows machine (CI does not pass `--include-ignored`):

```
$env:SKEIN_LIVE_MODEL = "gemma4:latest"
cargo test -p skein-connectors --test governed_fs_run -- --ignored --nocapture
```

New tests, all behaviour-proving, no padding:

| test | file | proves |
|---|---|---|
| multi-call round-trip | `native_loop.rs` | two calls in one turn are echoed with distinct ids and answered by two `Role::Tool` messages in order — the defect finding 4 measured (SC-001) |
| forged-label prompt | `native_loop.rs` | a user prompt that *looks* like tool output is still `Role::User`; the boundary is structural (SC-002, P-VI) |
| no-dangling-id invariant | `openai_compat.rs` | every wire `role:"tool"` answers exactly one earlier echoed id — slice 015's objection, discharged (SC-003) |
| secret in echoed args | `native_loop.rs` | Principle VI survives the echo; `LlmRequest` still parses; `verify_chain` still passes (SC-004) |
| wire bytes | `openai_compat.rs` | the exact serialization, `arguments` as a JSON string (SC-005) |
| missing-id fallback | `openai_compat.rs` | D3's synthesized ids (SC-006) |
| multi-call through the real chain | `governed_fs_run.rs` | the same as the first row with a real `EmbeddedServer`, real `OpenAiCompatClient`, real socket |
| live (`#[ignore]`) | `governed_fs_run.rs` | a real local model completes a two-file tool-mediated run in the new shape (SC-007) |

**Not written, deliberately:** a test asserting a model *ignores* instructions in tool content.
Finding 2 shows that is a model property under both shapes, and asserting it would make the suite a
model benchmark that fails on model choice.

## Risks

- **Ledger payload shape changes.** `LlmRequest`/`LlmResponse` payloads gain fields. Mitigated by
  `#[serde(default)]` + `skip_serializing_if` everywhere, so a turn with no tools serializes
  byte-identically and old chains still deserialize — the same both-directions-across-a-revert
  argument `TurnRequest::tools` already makes. `verify_chain` hashes payload strings and is
  indifferent to their shape; asserted in the secret-in-args test rather than assumed.
- **A silently-skipped ACP update.** If `ToolCall::id` were not defaulted, `project_updates`'s
  `let Ok(…) else { continue }` would drop every pre-022 tool-call update **without erroring**. This
  is the one silent failure mode; it is why D2 is written as it is and why `acp_session.rs` is
  re-run even though its source does not change.
- **A model confused by `***` in its own echoed arguments** (D4). Only reachable when a configured
  secret literally appears in a model-authored argument, which is already the case for the fed-back
  *content*. Accepted, recorded in `spec.md`'s Assumptions.
- **A provider that rejects the strict shape.** Not observed (finding 1); by Constitution II the
  reachable provider set is loopback-only.

**Rollback** is a single `git revert` of the slice's commits: the change is additive at every serde
boundary and deletes exactly one private function (`tool_message`), so a revert restores the prior
wire bytes exactly.

## Assumptions and residuals

Carried in `spec.md`'s *Assumptions and residuals*, not duplicated here.

## Out of scope

Carried in `spec.md`'s *Out of scope*, not duplicated here.

## Next slice

- **Raw wire-byte capture** — a `StepKind` for the provider's literal bytes, which would make
  wire/chain agreement directly observable rather than argued. Carried since slice 011.
- **Model obedience to instructions found in tool content** — design §7 item 5's other half, which
  this slice explicitly does **not** close.
- **Streaming (SSE), `strict: true`, `tool_choice`, provider authentication, a config file.**
- **Annotation-derived `ToolAccess`.**
- Carried from slice 021: the `git2` residual, hard-link containment, the `ipnet` / `cap_std::net`
  dead weight.
