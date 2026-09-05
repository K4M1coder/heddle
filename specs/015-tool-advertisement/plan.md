# Plan — slice 015: tool advertisement on the `TurnRequest` path

**Spec:** `specs/015-tool-advertisement/spec.md` · **Tasks:** `specs/015-tool-advertisement/tasks.md`
· Branch `015-tool-advertisement` cut from `dev` (`8860a01`) after slice 014 merged.

**Source of the decisions below:** the run plan of 2026-09-03, whose D1–D10 were verified against
this tree rather than inherited. This document keeps the half that is *this* slice (D2–D6) and names
the half that is not (D1, D7–D10 → slice 016).

## Problem

ADR-0004 D3 names six things as v0's build scope. Slices 004–014 delivered five. The sixth — *"MCP
tools (fs/git/shell)"* — is unbuilt in the product although its entire supporting apparatus is built
and green. Two independent gaps produce that, and this slice closes exactly one:

1. **No advertisement.** `TurnRequest` has no `tools` field, so `OpenAiCompatClient` cannot send one,
   so no model ever learns a tool exists. The response path is built; the request path is not.
2. **No server.** Both loop-running commands wire `NoTools` behind an empty allowlist. Slice 016.

The consequence today is that `heddle chat` and `heddle acp-agent` are chatbots, and every governance
property the project has built is unexercised against anything real.

## Approach

### D2 — advertisement is **discovered** from the transport and filtered by the policy, never hand-written

`ToolGateway` gains `advertise()`: ask the transport for its catalogue (MCP `tools/list`), keep only
names the policy allowlists, in allowlist order.

*Rejected: hand-writing `ToolSpec`s next to the CLI allowlist.* Each tool's JSON Schema would then
exist twice — once derived by the server from its real parameter type, once typed by hand — and the
two would drift. Drift here is not cosmetic: the model sends arguments matching the **advertised**
schema and the server rejects them against the **real** one. Discovery makes drift unrepresentable.

Putting the filter **inside `ToolGateway`** — which owns both the policy and the transport — rather
than in the CLI makes "you cannot advertise what the policy forbids" structural rather than reviewed,
the same move slice 005 made for calls.

**Allowlist order, not catalogue order.** The operator's list is the authority; a server does not get
to influence the order a model reads its tools in. An allowlisted name the server does not offer is
simply absent — never fabricated from the policy, which would be the hand-written schema by another
route.

**A `Mutating` tool with no approval is still advertised.** It is visible, and denied at call time
with a reason the model is told. Denying at advertisement would make `AcpPermissionTransport`
permanently unreachable: `call_captured` consults the policy **before** the transport, so a
`Mutating` tool absent from `approved` never reaches the ACP permission prompt at all.
`crates/heddle-acp/src/permission.rs` states the design directly — *"the client can only further
restrict, never widen."*

**Tool annotations are deliberately not consulted.** `crates/heddle-core/src/tool.rs` already commits
to this: *"Access is configuration, not discovery: deriving it from a server's tool annotations is a
later slice."* A server that self-declares `readOnlyHint: true` would otherwise be trusted to
classify its own risk.

### D3 — `ToolTransport::list` is a **defaulted** trait method

```rust
fn list(&mut self) -> Result<Vec<ToolSpec>> { Ok(Vec::new()) }
```

Nine `impl ToolTransport` sites exist — `heddle-acp/src/permission.rs`, `heddle-cli/src/wiring.rs`,
`heddle-mcp/src/lib.rs`, and six test doubles across `heddle-core`, `heddle-acp`, `heddle-gateway`,
`heddle-silo`. A defaulted method leaves all nine compiling untouched, keeping the 120-test baseline a
live control.

The project distrusts silent defaults — `NativeLoop::new`'s required `Redactor` exists for exactly
that reason — so the distinction has to be written into the docstring rather than assumed: **there
the silent default was the unsafe one; here it is the safe one.** A transport that does not override
`list` advertises nothing, which is deny-by-default.

*Rejected: a separate `ToolCatalog` trait.* A transport that can call but not enumerate is not a
thing MCP models, and two traits would have to be injected and kept in sync at every wiring site for
no gained precision.

**`AcpPermissionTransport` must override `list` and forward to its inner transport.** If it inherits
the default, `heddle acp-agent` silently advertises nothing while `heddle chat` works — a bug no
compiler catches and only an ACP-level test finds. **This is the single highest-risk line in the
slice**, and T7 is a test dedicated to it alone.

### D4 — advertise once per run, before the first turn

In `NativeLoop::run`, after the pre-flight `ctl.should_exit(false)` check — so a zero-budget run makes
no round trip — and before the message vector is built. The resulting specs are reused for every
`TurnRequest` in the run.

*Rejected: per turn.* The catalogue does not change mid-run; per-turn listing is one extra round trip
per iteration for nothing.

A `list` failure is **fatal to the run**, matching how `mediate` treats any non-`ToolDenied`
transport error: a tool inventory we could not read leaves the run's capabilities unknown. It
propagates with `?` before the first `IterationBoundary`, so a failed run leaves no step at all —
exactly what a `ModelClient::turn` failure already does mid-loop.

**No new `StepKind`.** The advertisement is part of the model's input and travels inside
`TurnRequest`, which `run` already captures as `LlmRequest` through `Redactor::redact_json`.
Constitution V's "exact model I/O" is satisfied by the existing capture, and descriptions and schemas
are scrubbed for free by `redact_value`'s recursion.

### D5 — both new fields are skipped when empty

`TurnRequest.tools` carries `#[serde(default, skip_serializing_if = "Vec::is_empty")]`; the gateway's
`ChatRequest.tools` carries `#[serde(skip_serializing_if = "Vec::is_empty")]`.

Consequence, and the reason: **a run with no tools produces byte-identical wire bytes and
byte-identical `LlmRequest` payloads to today's.** That keeps `openai_compat.rs`'s
`turn_sends_an_openai_chat_completions_request` byte-exact assertion green with an unchanged body,
keeps every existing Ledger payload shape, and keeps old chains deserializing — the same
`#[serde(default)]` treatment `TurnResponse.tool_calls` already documents. Slice 014 had to announce
a payload-shape change to its readers; this slice must not need to.

### D6 — the OpenAI wire shape, verified rather than assumed

```json
"tools": [
  { "type": "function",
    "function": {
      "name": "fs_read",
      "description": "…",
      "parameters": { "type": "object", "properties": { }, "required": [ ] }
    } }
]
```

`strict` is **omitted**: it is an OpenAI structured-outputs extension, Ollama's own docs describe its
OpenAI compatibility layer as experimental while listing `tools` as a supported request field, and
sending an unrecognised key to a local provider buys nothing.

`tools` is serialized **last**, after `stream`. The position is arbitrary for a JSON object and is
fixed here only so the bytes are ours and the tests can assert them, the same reason `ChatRequest` is
a struct rather than a `json!` literal.

**The tool-result feedback path is deliberately left as it is.** Strict OpenAI wants
`{"role":"tool","tool_call_id":…}` replies. `NativeLoop::mediate` instead feeds results back as
user-role text via `tool_message`, and `ChatMessage` sends only `{role, content}` — it never echoes
`tool_calls` into the assistant history. **Because no `tool_calls` are ever sent, there is no
dangling call id to satisfy and the history stays a valid OpenAI message sequence** (`user`,
`assistant`, `user`). Changing this would reopen `native_loop.rs`'s explicit anti-injection decision,
which Constitution VI backs. Out of scope; recorded as a residual in the spec. **Closed by slice
022**, whose `spec.md` quotes that decision's origin in full: it was never "`role:"tool"` is
unsafe", but "`User` + a label is the cheapest thing safer than `System` or `Assistant`, and the
real fix is a typed variant, deferred". The dangling-id argument above is answered structurally
there.

### Deferred to slice 016 (recorded here so the split is legible)

D1 build-the-`fs`-server-in-tree with its supply-chain reasoning; D7 `git`/`shell` deferral; D8 root
containment (`FsRoot`, the `Path::join`-absolute footgun, the TOCTOU residual); D9
`crates/heddle-connectors` as a new workspace member and the amended `heddle-mcp` MCP invariant; D10
`--fs-root` and the per-command allowlist asymmetry.

## Steps

- **T0** `spec.md`, `plan.md`, `tasks.md`, including the Constitution Check table.
- **T1** Control baseline: `cargo test --workspace`, recorded verbatim. Expect **120 passed, 1
  ignored**.
- **T2 · RED→GREEN** `ToolSpec` in `crates/heddle-core/src/tool.rs`, re-exported from `lib.rs`, tested
  in `crates/heddle-core/tests/core.rs`.
- **T3 · RED→GREEN** `ToolTransport::list` with its defaulted body and its docstring on why *this*
  silent default is the safe one.
- **T4 · RED→GREEN** `ToolGateway::advertise` — list, filter, order — tested in
  `crates/heddle-core/tests/tool_gateway.rs` against a double whose catalogue is wider than the
  policy.
- **T5 · RED→GREEN** `TurnRequest.tools` with D5's attributes, and the three literal construction
  sites: `native_loop.rs`'s `run`, `crates/heddle-acp/tests/acp_session.rs`, and
  `crates/heddle-gateway/tests/openai_compat.rs`'s `ask` helper. Mechanical, one atomic commit.
- **T6 · RED→GREEN** `NativeLoop::run` advertises once (D4) and stamps the specs into every
  `TurnRequest` of the run.
- **T7 · RED→GREEN** `AcpPermissionTransport::list` forwards to its inner transport (D3's hazard).
- **T8 · RED→GREEN** `ChatRequest.tools` in `crates/heddle-gateway/src/lib.rs` producing D6's exact
  shape, with a byte-exact test.
- **T9** Gates, control diff, dependency drift, close-out.

## Validation

**The project's own gates**, per ADR-0004 D1(c)/(d) and `docs/QUALITY-GATES.md`: `cargo fmt --all
--check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`; `cargo
build --workspace`. Tri-OS CI needs no change: no workspace member is added and `core.yml`'s
`paths:` already covers `crates/**`. The Windows leg is observable locally; macOS and Linux remain
unobserved until the repository has a remote — the standing caveat of specs 004–014.

**Every pre-existing test body must be unchanged** except the mechanical `TurnRequest { … }` field
additions at two test sites. D5's skip-when-empty is what makes that achievable; if a pre-existing
assertion has to be edited, that is a signal the design drifted, not a test to fix.

**New tests**, one claim each:

1. `ToolSpec` round-trips through the Ledger payload shape.
2. A transport that does not override `list` advertises nothing.
3. `advertise` returns only allowlisted names, in allowlist order, from a wider catalogue.
4. `advertise` includes an unapproved `Mutating` tool — D2's deliberate choice, pinned.
5. `NativeLoop::run` puts the advertised specs in every `TurnRequest` of the run and lists once.
6. A `list` failure ends the run before any step is appended.
7. A run with an empty catalogue produces a `TurnRequest` payload with **no `tools` key**.
8. `AcpPermissionTransport::list` forwards rather than inheriting the empty default.
9. `ChatRequest` byte-exactness with one tool.
10. The existing no-tools byte-exact test passes with an unchanged body (asserted by `git diff`).

Expected **120 → ~130**. A prediction to be reconciled in the close-out, not a target: slice 014's
close-out recorded ten tests where its plan predicted eight, and said why.

## Risks and rollback

**Blast radius.** `heddle-core` (`tool.rs`, `model.rs`, `native_loop.rs`, `lib.rs`), `heddle-gateway`
(`lib.rs`), `heddle-acp` (`permission.rs`). No package added, no manifest changed. `heddle-cli`,
`heddle-silo` and `spikes/` untouched.

| Risk | Mitigation |
|---|---|
| **`AcpPermissionTransport` silently inherits the default `list`** — `acp-agent` advertises nothing while `chat` works, and nothing fails to compile. | T7 is a test for exactly this. Highest-value single test in the slice. |
| **A pre-existing byte-exact wire assertion changes.** | D5's skip-when-empty; test 10 asserts the body is unchanged by `git diff`, not merely that it passes. |
| **Advertising changes behaviour for existing users.** | Nothing is wired: both commands still carry `NoTools` and an empty policy, so `advertise` returns empty and no key is serialized. |
| **A model ignores or mangles a `tools` array.** | Out of this slice's reach — no model receives one until 016 wires a connector. |

**Rollback.** One commit range, cut from `dev`; no remote and no PR. `git revert` of the range
restores `TurnRequest` to `{run_id, messages}` and the wire to `{model, messages, stream}`, and the
literal construction sites revert with it. Old Ledger chains are unaffected in both directions:
`#[serde(default)]` on `tools` means chains written with the field still deserialize after a revert,
and chains written before it deserialize after the merge.

## Out of scope

Per the spec's own list, and identically: any connector, `crates/heddle-connectors`, `--fs-root`,
every `heddle-cli` change, `git`/`shell`, annotation-derived `ToolAccess`, `role: "tool"` replay,
`strict`/`tool_choice`/streaming, a new `StepKind`, per-turn re-listing, a `ToolCatalog` trait,
`crates/heddle-silo/`, `spikes/`, `.github/`, `rust-toolchain.toml`.
