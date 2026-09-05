# Feature Specification: `heddle acp-agent` — the ACP facade as a running process (v0 slice)

**Feature Branch:** `013-acp-agent` · **Created:** 2026-09-03 · **Status:** Implemented (v0 slice)
**Input:** `specs/012-model-gateway/tasks.md` "Next slice" — *"**`heddle acp-agent`** — the stdio ACP
server, now that a real `ModelClient` exists to put behind it"* · Constitution I (**the CLI is the
core's complete, authoritative client**), II (**local-first, NON-NEGOTIABLE**), III
(**test-first**), IV (**inverted coupling**), V (**traceability**), VII (**no capability without a
real need**), VIII (**loop discipline, NON-NEGOTIABLE**) · design §4.1, §4.5 · ADR-0003 D2 and
ADR-0004 D3 (**ACP is the client boundary**).

Slice 008 built a complete, tested ACP facade in `crates/heddle-acp` — `HeddleAgent`, `HeddleSession`,
`SessionParts`, `serve`, `project_updates`, `AcpPermissionTransport`, `CancellableModel` — and it is
exercised only by its own test binary over an in-process `tokio::io::duplex`. Slice 012 built the
first real `ModelClient` and wired it into `heddle chat`. Both halves are real; **nothing in the
workspace served ACP as a running process with a real model behind it.**

This slice ships the process. `heddle acp-agent` serves the slice-008 facade on the executable's own
stdin/stdout, with `heddle-gateway`'s loopback-only client in front of the model and `heddle-silo`'s
hash-chained ledger behind every session.

## What this slice lets a user do, and what it does not

**It does:**

```
heddle acp-agent --silo <ID> [--root <PATH>] --model <NAME> [--base-url <URL>]
                [--max-iters N] [--max-tokens N] [--no-progress-limit N] [--timeout-secs S]
```

Serves ACP on stdin/stdout until the client disconnects, then exits 0. An ACP-speaking editor
configured to launch that command gets a Heddle agent whose every session is a governed `NativeLoop`
run against a loopback-only local model, recorded on the silo's hash-chained ledger and verifiable
afterwards with `heddle ledger log|show|verify`.

**It does not add a tokio runtime, and does not need one.** `crates/heddle-acp` lists `tokio` under
`[dev-dependencies]` only, and its product code uses `std::thread`, `std::sync::mpsc`, `AtomicBool`
and `Mutex`. `agent-client-protocol 2.0.0` is likewise runtime-agnostic — its own manifest lists
`tokio` under `[dev-dependencies]`, and its `Stdio` transport wraps `std::io::stdin()`/`stdout()` in
`blocking::Unblock`, which owns its threads. One `futures::executor::block_on` of the connection
future is therefore a complete runtime for this path. Slice 012's spec predicted a tokio runtime
here; that prediction was wrong, and this document supersedes it.

**It does not grow the ACP surface.** `initialize`, `session/new`, `session/prompt`,
`session/cancel` — exactly what slice 008 implemented, with no new method, no new capability field
and no change to `project_updates`.

**It does not advertise tools, and cannot reach one.** See point 3 below.

## Four things a reader must know up front

These are load-bearing and are stated here rather than in a footnote.

1. **The test suite needs no editor and no Ollama.** The end-to-end test is the real `heddle` binary
   spawned as a **subprocess** by the real `agent-client-protocol` client (`AcpAgent` /
   `AcpAgentConfig`, the same transport an editor uses), against a `std::net::TcpListener` stub
   serving real HTTP/1.1 chat-completions bytes — slice 012's SC-003 precedent, and no
   HTTP-mocking dependency.
2. **stdout is the protocol.** A single stray byte on stdout corrupts the JSON-RPC stream, so
   `heddle acp-agent` writes nothing there but ACP frames. Its stderr is one startup line naming the
   silo and the endpoint, plus `main`'s single error line — bounded on purpose, because an ACP
   client is not obliged to drain the child's stderr and a full pipe would block the agent.
3. **`AcpPermissionTransport` is not reachable through this command, and the reason is the policy.**
   `heddle_core::TurnRequest` is `{ run_id, messages }`, so no `tools` field goes on the wire, and
   `heddle-cli` does not depend on `heddle-mcp`, so there is no tool server to reach. The session is
   wired with `ToolPolicy::new(vec![], vec![])`, and deny-by-default refuses every name in
   `ToolGateway::call_captured` before the transport is consulted. A model that nevertheless invents
   a `tool_calls` array — `OpenAiCompatClient` parses one if a provider sends it (slice 012 FR-009)
   — produces a `ToolCall` step, an `Approval` step with `decision: "denied"`, and the run
   *continues*; through `project_updates` the client sees a `SessionUpdate::ToolCall` followed by a
   `ToolCallUpdate` with `ToolCallStatus::Failed`. The permission round-trip itself stays proven by
   slice 008's in-process suite.
4. **A session's read model is a snapshot taken at its own `session/new`.** `Ledger::open` calls
   `store.load()` once, so a session does not see rows another session appends later. Concurrent
   sessions each hold their own SQLite connection to the same file; the store is rollback-journal
   with `synchronous = FULL`, i.e. single-writer, and `rusqlite` sets a 5s busy timeout on every
   `Connection::open`, so a simultaneous write waits rather than failing. Distinct sessions never
   share a `run_id`, so no two chains interleave. This is invisible to ACP — `project_updates` only
   reads the run just appended — and is the same property `heddle chat` has.

## User Scenarios & Testing

### User Story 1 — An editor drives Heddle, and the conversation is on the chain (P1)
As an operator, I point an ACP-speaking editor at `heddle acp-agent` and get a governed agent whose
every turn is recorded and verifiable afterwards.
**Acceptance:**
1. **Given** a local OpenAI-compatible provider on loopback and a temp silo root, **When** a real
   ACP client spawns `heddle acp-agent --root <root> --silo alpha --model <name> --base-url <url>`
   as a subprocess and sends `initialize`, `session/new`, then two `session/prompt`s, **Then** the
   `InitializeResponse` comes back, `session/new` yields `SessionId("heddle-1")`, each
   `PromptResponse.stop_reason` is `EndTurn`, and the `AgentMessageChunk` notifications carry the
   provider's two answers.
2. **Given** that connection has closed, **When** `heddle ledger log --root <root> --silo alpha` is
   invoked as a **second process**, **Then** runs `heddle-1#1` and `heddle-1#2` are listed, each with
   the steps `iteration_boundary`, `llm_request`, `llm_response`, `budget_spent`, `exit`, and
   `heddle ledger verify` reports both `ok`.

### User Story 2 — A local provider is the only thing this can talk to, here too (P1)
As a security reviewer, the Principle II guard must be enforced on every command that reaches a
model, with one implementation rather than two.
**Acceptance:**
1. **Given** `heddle acp-agent --base-url http://192.168.1.10:11434/v1`, **When** it is invoked,
   **Then** the exit code is 1, stdout is empty, stderr contains `not a loopback address`, and the
   silo's ledger file **does not exist** — the refusal happens before any chain is opened and before
   any ACP handshake.
2. **Given** the same resolution rule, **When** `--base-url` is absent, **Then** the endpoint comes
   from `$HEDDLE_MODEL_BASE_URL`, else `http://localhost:11434/v1` — identical to `heddle chat`,
   because both commands call the same code.

### User Story 3 — Closing the editor is not an error (P1)
As an operator, quitting my editor must not leave a failure in the log.
**Acceptance:**
1. **Given** `heddle acp-agent` with a valid endpoint and silo, **When** its stdin reaches EOF
   without a handshake, **Then** the exit code is 0 and **stdout is empty**.

### User Story 4 — A silo that cannot be opened fails before the handshake, not during it (P1)
As an operator, a disk problem must be legible, and must never become a silently in-memory chain.
**Acceptance:**
1. **Given** a `--root` that cannot hold a silo, **When** `heddle acp-agent` starts, **Then** it
   exits 1 with a message, having served nothing.
2. **Given** a session factory that fails at `session/new` time, **When** a client calls
   `session/new`, **Then** the client receives a JSON-RPC error, the connection **stays usable**, and
   a subsequent successful `session/new` still returns a session id. Not a panic, and not a silent
   `Ledger::new()`.

## Requirements

- **FR-001**: `heddle acp-agent` MUST serve exactly the ACP surface slice 008 implemented —
  `initialize`, `session/new`, `session/prompt`, `session/cancel` — with no new method, no new
  capability field, and no change to `project_updates`.
- **FR-002**: The model MUST be reached through `heddle_gateway::{LocalEndpoint, OpenAiCompatClient}`.
  The endpoint MUST be parsed **before** the silo is opened and before the handshake, so an
  off-machine `--base-url` is exit code 1 with no chain and no ACP session.
- **FR-003**: Base-URL resolution MUST be `--base-url` → `$HEDDLE_MODEL_BASE_URL` →
  `http://localhost:11434/v1`, identical to `heddle chat`, and MUST have exactly one implementation
  shared by both commands.
- **FR-004**: Each session's `SessionParts.ledger` MUST be `Silo::open(root, id)?.ledger()?` — a
  durable, silo-backed chain. `Ledger::new()` MUST NOT appear in `heddle-cli`.
- **FR-005**: A silo that cannot be opened MUST fail the process **before serving** (exit 1); a
  session-time failure MUST surface as a JSON-RPC error on `session/new`, never as a panic and never
  as a silent in-memory chain.
- **FR-006**: `heddle-acp` MUST gain exactly two things: `HeddleAgent::serve_stdio` and a fallible
  factory bound. `serve` itself, `SessionParts`'s fields, `HeddleSession`, `project_updates`,
  `AcpPermissionTransport` and `CancellableModel` MUST be otherwise unchanged.
- **FR-007**: `agent_client_protocol` MUST NOT be named outside `crates/heddle-acp` in product code.
  (`heddle-cli`'s **test** binary names it deliberately, because the acceptance criterion is a real
  ACP client.)
- **FR-008**: The process MUST write nothing to stdout except the ACP protocol stream, and MUST
  bound its stderr to a startup line plus `main`'s single error line.
- **FR-009**: A normal client disconnect (stdin EOF) MUST exit 0.
- **FR-010**: `heddle-core` MUST gain exactly one additive variant, `Protocol(String)`, and nothing
  else.
- **FR-011**: The tool path MUST stay deny-by-default: `ToolPolicy::new(vec![], vec![])` with an
  unreachable-by-construction `ToolTransport`, shared with `heddle chat`.
- **FR-012**: No automated test may require a running Ollama or an installed editor.
- **FR-013**: `crates/heddle-mcp`, `crates/heddle-silo` and `crates/heddle-gateway` MUST be unchanged.

## Success Criteria

- **SC-001**: All four gates clean; test count = 105 + new, recorded in `tasks.md`.
- **SC-002**: The end-to-end test drives the **real binary as a subprocess** with the **real
  `agent-client-protocol` client**, over the child's actual stdio — the one thing no prior slice has
  proven (slice 008's suite uses an in-process `tokio::io::duplex`).
- **SC-003**: The ledger assertions run in a **second process** (`heddle ledger log|verify`), so
  persistence is proven across a process boundary, not by reading an in-memory `Ledger`.
- **SC-004**: No test requires a live model; the provider is a `std::net::TcpListener` stub serving
  real HTTP/1.1 bytes (slice 012's SC-003 precedent), and **no HTTP-mocking dependency is added**.
- **SC-005**: `git diff dev -- crates/heddle-mcp/ crates/heddle-silo/ crates/heddle-gateway/ spikes/
  .github/ rust-toolchain.toml` is empty; `git diff dev -- crates/heddle-core/` is one variant plus
  its test; every one of the 105 pre-existing tests stays a live control with an unchanged body.
- **SC-006**: Dependency drift measured per target and recorded: expected **zero new packages**
  (`futures`/`futures-executor` are already in the graph via `agent-client-protocol`), new *edges*
  only — `heddle-acp → futures`, `heddle-cli → heddle-acp`, and `heddle-cli` dev →
  `agent-client-protocol` + `futures`.
- **SC-007**: `heddle acp-agent --help` succeeds and `heddle chat --help` still lists the same flags it
  listed on `dev`.
- **SC-008**: The hand-run against a real editor and a real Ollama is recorded with its transcript
  and `heddle ledger verify` output — or explicitly recorded as unobserved, with the reason.
- As in specs 004–012, the macOS and Linux legs of `core.yml` are unobserved until the repository
  has a remote; only the Windows leg is run locally.

## Assumptions

- **`futures::executor::block_on` is a complete runtime for this path.** Measured, not assumed: a
  two-binary probe outside the repository ran the exact shape `heddle-acp` uses — a handler that
  moves its `Responder` onto a plain `std::thread`, sends a notification from that thread and
  responds from it — with single-threaded `block_on` on both the agent and the client side, over a
  real child process and real pipes. `stop_reason=EndTurn` and the notification both arrived. The
  documented fallback, if this ever surprises us, is one line inside `serve_stdio`:
  `tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(…)`.
- **The factory has to be fallible.** `Silo::open(root, id)?.ledger()?` is a SQLite file open plus a
  full `load()` of the existing chain — fallible on permissions, a locked file, or a corrupt store.
  With an infallible factory the only options are a panic inside `session/new`, which poisons the
  factory mutex and kills every other session on a recoverable disk error, or a fallback to
  `Ledger::new()`, which runs the session on an **in-memory** chain and persists nothing. The second
  is a direct Principle V violation and invisible to the operator. So the bound changes.
- **`NewSessionRequest.cwd` is received and ignored.** No tool in this slice can touch a filesystem,
  so honouring a working directory would be a claim with nothing behind it.
- **`AgentCapabilities`' `prompt_capabilities` defaults to all-false, and that is correct here.** In
  ACP those flags gate *image/audio/embedded-context* prompts; plain text needs no advertisement,
  and this agent accepts text blocks only (`user_message` refuses a non-text block with
  `invalid_params`, unchanged from slice 008).
- **The model-I/O redaction gap is carried forward unchanged.** The chain holds the *translated*
  `TurnRequest`/`TurnResponse` and `NativeLoop::run` appends them raw. Carried from slices 011 and
  012; the fix belongs to the governed loop.
- **One process, one silo.** Sessions live only as long as the connection, and there is no
  `session/load`, no session persistence across restarts, and no per-client silo mapping.
