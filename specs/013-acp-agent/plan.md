# Implementation Plan: `heddle acp-agent` — the ACP facade as a running process (v0 slice)

**Spec:** `specs/013-acp-agent/spec.md` · branch `013-acp-agent` cut from `dev` after slice 012
merged. No PR (the repository has no remote).

## Summary

One new subcommand, one blocking `serve_stdio` helper inside the crate that already owns ACP, one
shared wiring module in the CLI, and one additive change each to `heddle-acp` and `heddle-core`.

`HeddleAgent::new` already requires `C: ModelClient + Send + 'static` and `OpenAiCompatClient`
satisfies it. What was missing is a binary: a transport, a `SessionParts` factory that produces a
real client and a real silo-backed chain, and a test that proves a real ACP client can drive the
real executable over real pipes.

Two of the request's inherited premises were wrong and are corrected here rather than carried:

1. **No tokio runtime is required, and none is added.** `crates/heddle-acp/Cargo.toml` lists `tokio`
   under `[dev-dependencies]` only, and its product code (`src/lib.rs`, `src/permission.rs`,
   `src/cancel.rs`) names no tokio type. `agent-client-protocol 2.0.0`'s own manifest likewise lists
   `tokio` under `[dev-dependencies]`; its runtime pieces are `async-io`, `async-process`,
   `blocking`, `futures`, `futures-concurrency`, and `src/stdio.rs`'s `Stdio` transport wraps
   `std::io::stdin()`/`stdout()` in `blocking::Unblock`, which owns its own threads.
2. **The `agent-client-protocol` crate already ships both sides of the subprocess story.**
   `src/stdio.rs::Stdio` is the agent-side transport; `src/acp_agent.rs::{AcpAgent, AcpAgentConfig}`
   is the client-side transport that *spawns a child process* and speaks ACP over its stdio. So the
   end-to-end test needs no hand-rolled stdio plumbing, and the acceptance criterion "a real ACP
   client from the crate, not a hand-rolled stand-in" is satisfiable directly.

## Decisions

### D1 — `heddle acp-agent` lives in `heddle-cli`; the stdio transport stays inside `heddle-acp`

`crates/heddle-acp/src/lib.rs`'s module doc claims: *"This is the only crate in the product that
names the Agent Client Protocol."* Naming `agent_client_protocol::Stdio` in `heddle-cli` would break
that claim (and Principle IV's boundary discipline) for one type. So `heddle-acp` gains **one**
public method on the existing `impl<C, P, T, F> HeddleAgent<C, P, T, F>` block, beside `serve`:

```rust
pub fn serve_stdio(self) -> heddle_core::Result<()>
```

implemented as `futures::executor::block_on(self.serve(Stdio::new())).map_err(…)`.

The precedent is exact: `crates/heddle-mcp/src/lib.rs`'s `RmcpToolTransport` owns its runtime and
blocks on it, and its doc says so. The protocol-adapter crate owning its executor is this
workspace's established shape, and it keeps both the ACP type names and the executor choice out of
the CLI.

*Alternative rejected:* put `Stdio::new()` and `block_on` in `heddle-cli/src/acp.rs`. It needs no
change to `heddle-acp`, but it makes the CLI name ACP types and forces `agent_client_protocol::Error`
into a CLI-level error mapping — two boundary leaks to avoid one five-line method.

### D2 — `futures::executor::block_on`, not a tokio runtime

`heddle-acp` gains `futures = { version = "0.3", default-features = false, features = ["std",
"executor"] }`, declared in the root `[workspace.dependencies]` as this workspace does for every
shared dependency.

The ACP crate is runtime-agnostic and uses the `async-io`/`blocking` family, so a tokio runtime
would be a reactor and timer wheel that nothing in this path uses. `block_on` parks one thread and
polls one future; the connection's own task actor drives whatever the handlers spawn, and the
*blocking* work is on a plain `std::thread` that `heddle-acp` already spawns. Zero new packages.

*Alternative rejected:* `tokio::runtime::Builder::new_current_thread()` — mirrors `heddle-mcp`
exactly and adds no workspace dependency line, but compiles a runtime nothing needs. It is the
documented one-line fallback if `block_on` ever surprises us.

### D3 — The factory becomes fallible: `F: FnMut() -> heddle_core::Result<SessionParts<C, P, T>>`

This is the one public-API change to `heddle-acp`, and it needs justifying (Principle IV).

Today `HeddleAgent::new(factory)` takes `F: FnMut() -> SessionParts<C, P, T>`. That was free while
every caller injected `Ledger::new()`. The first real caller must build, per session,
`Silo::open(root, id)?.ledger()?` — a SQLite file open plus a full `load()` of the existing chain,
both fallible. With an infallible factory the only options are:

- panic inside `session/new` → the `factory` mutex is poisoned and the whole agent process dies on a
  recoverable disk error, taking every other session with it; or
- fall back to `Ledger::new()` → the session silently runs with an **in-memory** chain and nothing is
  persisted. That is a direct Principle V violation, and invisible to the operator.

Neither is acceptable, so the signature changes. The change is small and fully contained:
`HeddleAgent::new`'s `F` bound; `HeddleAgent::open` returns `heddle_core::Result<SessionId>`; the
`NewSessionRequest` handler maps the error with `respond_with_internal_error`, which the
`PromptRequest` handler already uses with a `HeddleError`; and the construction sites in
`crates/heddle-acp/tests/acp_session.rs` gain `Ok(…)`. **No other crate constructs a `HeddleAgent` or
a `SessionParts`.**

The gain is behavioural, not cosmetic: a bad silo produces a JSON-RPC error on `session/new` that a
client can show its user, and the connection survives. Its test is
`a_factory_that_fails_makes_session_new_fail_and_leaves_the_connection_usable`.

### D4 — A shared `wiring` module in `heddle-cli`, so the loopback guard has exactly one copy

`heddle acp-agent` needs the same six knobs `heddle chat` has (`--model`, `--base-url`, `--max-iters`,
`--max-tokens`, `--no-progress-limit`, `--timeout-secs`), the same base-URL resolution, the same
`LocalEndpoint::parse` guard, and the same two honest stand-ins (`NoGroundTruth`, `NoTools`). All of
that lived in `crates/heddle-cli/src/chat.rs`.

New module `crates/heddle-cli/src/wiring.rs` holds `ModelArgs` (a clap `#[derive(Args)]` carrying
those six flags, moved verbatim from `ChatArgs` so every flag name, value name, default and help
string is byte-identical), `DEFAULT_BASE_URL`, `ModelArgs::{endpoint, client, budget}`, and
`NoGroundTruth`/`NoTools` with their comments carried over intact — those comments are the Principle
VIII(b) and deny-by-default arguments. `NoTools`'s message is generalised from "`heddle chat`" to
"this command", since it is now shared by two.

`ChatArgs` keeps `--prompt` and `--run-id` and gains `#[command(flatten)] model: ModelArgs`
**declared first**, so `heddle chat --help` keeps its current flag order.

The refactor's correctness proof is that **the five existing `cli_chat.rs` tests stay green with
their bodies unchanged** — including `the_base_url_falls_back_to_the_environment_and_the_local_default`
and `chat_refuses_a_non_loopback_base_url`, which are precisely the behaviours being moved.

*Alternative rejected:* duplicate the six flags and the resolution logic in a new `AcpAgentArgs`.
Fewer files touched, but it makes a **second copy of a NON-NEGOTIABLE Principle II guard path** that
can drift.

### D5 — One silo, one `Ledger` per session, opened at `session/new`

`SessionParts.ledger` is a `Ledger` by value and the factory runs once per `session/new`, so each
session gets its own `Silo::open(&root, &id)?.ledger()?`. Run ids stay `{session_id}#{n}` with
`session_id = heddle-{n}` minted from `HeddleAgent`'s own `AtomicU64` starting at 1, so the first
session's first prompt is run `heddle-1#1` — deterministic, and directly assertable in the test.

A **pre-flight** `Silo::open(root, &silo)?.ledger()?` runs (and is dropped) *before* serving starts,
so a bad `--root`/`--silo` is exit code 1 with a message, not a JSON-RPC error after a handshake.
The endpoint is parsed before that, so an off-machine `--base-url` opens no silo at all — the same
ordering `chat.rs` documents and `chat_refuses_a_non_loopback_base_url` asserts.

The concurrency and snapshot properties are stated in the spec's point 4 rather than here, because
an operator needs them more than a maintainer does.

### D6 — `HeddleError::Protocol(String)`: one additive variant in `heddle-core`

`serve_stdio` must return an error a CLI can print honestly. Every existing variant names a
different subsystem (`Storage`, `Secret`, `Model`, `Tool`, `ToolDenied`, `Json`, `NotFound`,
`LedgerIntegrity`, `Unfinished`), and rendering a broken ACP pipe as `tool transport: …` would
mislead:

```rust
#[error("protocol: {0}")]
Protocol(String),
```

Nothing else in `heddle-core` changes.

*Alternative rejected:* reuse `HeddleError::Tool`. It is what `AcpPermissionTransport` uses today —
but there it *is* a tool-transport failure. For `serve_stdio` it would be a lie in the operator's
terminal.

### D7 — The ACP surface does not grow (Principle VII)

`initialize`, `session/new`, `session/prompt`, `session/cancel` and
`InitializeResponse::new(request.protocol_version).agent_capabilities(AgentCapabilities::new())`
stay exactly as slice 008 wrote them. `NewSessionRequest`'s `cwd` is received and ignored, because
no tool in this slice can touch a filesystem.

### D8 — stdout is the protocol; stderr stays short

`heddle acp-agent` never writes to stdout: a single stray byte corrupts the JSON-RPC stream. It
writes exactly one stderr line at startup (naming the silo and the endpoint — the same courtesy
`heddle chat`'s `run {run_id}` line provides) and nothing per turn. An ACP client is not obliged to
drain the child's stderr, and a full stderr pipe would block the agent; `main.rs`'s existing error
boundary already owns the one-line failure message.

## Complexity Tracking

| Addition | Why it is not avoidable |
|---|---|
| `HeddleAgent::serve_stdio` (D1) | The alternative is naming `agent_client_protocol::Stdio` and `Error` in `heddle-cli`, breaking the crate's own "only crate that names ACP" claim. |
| A fallible factory bound (D3) | An infallible factory forces either a poisoned mutex on a recoverable disk error or a silent in-memory chain. Both are worse than a signature change. |
| `crates/heddle-cli/src/wiring.rs` (D4) | Two commands need the same NON-NEGOTIABLE Principle II guard. One implementation, with the five existing `cli_chat.rs` tests as the control. |
| `HeddleError::Protocol` (D6) | `serve_stdio`'s failure is neither a model failure nor a tool failure, and printing it as one would mislead an operator. |
| `futures` as a workspace dependency (D2) | `block_on` has to come from somewhere; `futures-executor` is already in the graph via `agent-client-protocol`, so the cost is an edge, not a package. |
