# Feature Specification: `skein-gateway` — the first real `ModelClient`, and `skein chat` (v0 slice)

**Feature Branch:** `012-model-gateway` · **Created:** 2026-09-03 · **Status:** Implemented (v0 slice)
**Input:** `specs/011-skein-cli/tasks.md` "Next slice" — *"**`skein acp-agent` and `skein chat`,
together with the real `ModelClient`** (BMAD Story 1.4)"* · Constitution I (**the CLI is the core's
complete, authoritative client**), II (**local-first, NON-NEGOTIABLE**), III (**test-first**),
IV (**inverted coupling**), V (**traceability**), VII (**no capability without a real need**),
VIII (**loop discipline, NON-NEGOTIABLE**) · design §4.5 (Model Gateway), §4.2, §4.11 ·
ADR-0002 D4 (Local mode enforces a **loopback allowlist**) · ADR-0004 D3 (*"one local model path
(Ollama via gateway)"*).

Eight merged slices (004–011) built a governed, ACP-reachable, persistently-storing agentic loop
with real secret resolution and a real CLI. **None of it had ever spoken to a model.** Before this
slice, `grep -rn "impl ModelClient" crates/` returned four hits, all in `tests/` — a `ScriptedModel`
each in `skein-acp/tests/acp_session.rs`, `skein-core/tests/native_loop.rs`,
`skein-mcp/tests/rmcp_gateway.rs` and `skein-silo/tests/silo_ledger.rs`. The only `ModelClient` in
product code was `skein-acp`'s `CancellableModel`, a decorator generic over an inner client with no
real inner client anywhere. `grep -rn "reqwest\|hyper\|ureq\|axum" crates/*/Cargo.toml Cargo.toml`
returned nothing: the workspace made no HTTP call of any kind.

This slice ships the first one. `crates/skein-gateway` holds one `ModelClient` speaking the
OpenAI-compatible chat-completions wire format to a **loopback-only** local provider, and
`skein chat` is its caller — the first command in the product that runs the governed loop.

## What this slice lets a user do, and what it does not

**It does:**

```
skein chat --silo <ID> [--root <PATH>] --model <NAME> [--base-url <URL>]
           [--prompt <TEXT>] [--run-id <ID>]
           [--max-iters N] [--max-tokens N] [--no-progress-limit N] [--timeout-secs S]
```

One prompt, one run. The prompt comes from `--prompt` or, absent it, from stdin read to EOF. The
run is appended to the **real silo chain**, so `skein ledger log|show|verify` from slice 011 become
newly meaningful against a real conversation. **stdout carries the assistant's answer and nothing
else**; the run id goes to stderr, so stdout stays the scriptable contract slice 011 established.

**It does not serve ACP.** There is no `skein acp-agent` in this slice. That is a different concern
with its own machinery — a stdio `ConnectTo<Agent>` transport, a `tokio` runtime inside `skein-cli`
(which still has **no** async dependency at all), a `SessionParts` factory and a subprocess ACP
end-to-end test. It is on `tasks.md`'s `## Next slice` list, which is this repository's way of
recording a deferral.

**It does not reach a cloud provider, and structurally cannot.** `ureq` is declared with
`default-features = false`, which excludes every TLS feature, so an `https://` URL fails at the
transport with `ureq::Error::TlsRequired` ("TLS required, but transport is unsecured") before any
socket is opened to it. Every cloud provider endpoint is HTTPS. Principle II is therefore a
property of the build, not of a code review.

**It does not stream, advertise tools, authenticate, or hold a REPL.** See `## Out of scope` in
`tasks.md`.

## Five things a reader must know up front

These are load-bearing and are stated here rather than in a footnote.

1. **The test suite needs no Ollama; the feature does.** Every automated test in this slice runs
   against a `std::net::TcpListener` stub server speaking real HTTP/1.1 bytes inside the test
   process. `cargo test --workspace` is therefore green on a machine — and on a CI runner — with
   no local model installed. `skein chat` itself is useful only against a real local provider.
2. **When nothing is listening, `skein chat` fails loudly and fast.** `ureq` surfaces
   `io: Connection refused` on a closed loopback port in well under a second; the client maps it to
   `SkeinError::Model` naming the endpoint and asking whether a local provider is listening. Not a
   hang, not a panic, not an empty answer.
3. **The chain records the translated `TurnRequest`/`TurnResponse`, not the provider's raw wire
   bytes.** `NativeLoop::run` appends `serde_json::to_string(&req)`/`(&resp)`. Design §4.5's
   "exact model I/O" and Spike 1's byte-exact criterion C1 are therefore **not** met by this slice;
   byte-exact capture needs a new `StepKind` and a change to the governed loop, which this slice's
   invariants exclude. Recorded on the next-slice list.
4. **No `tools` field is sent.** `TurnRequest` carries only `run_id` and `messages`, and `model.rs`'s
   own comment says tool *advertisement* awaits tool discovery. A run driven by this client will not
   normally produce tool calls, so the loop's tool mediation stays proven by the `ScriptedModel`
   suites until advertisement lands. A `tool_calls` array is nevertheless *parsed* if a provider
   sends one — see FR-009.
5. **`localhost` resolution is checked, and the TOCTOU residual is real.** `LocalEndpoint::parse`
   resolves the literal host name `localhost` and requires **every** resolved address to be
   loopback, closing the hostile-`hosts`-entry hole. But `ureq` re-resolves at request time, so a
   hostile resolver could in principle answer differently between the check and the connection. The
   real fix is ADR-0002 D4's process-level socket-deny boundary, which does not exist in this
   workspace. This check is the policy layer above it.

## User Scenarios & Testing

### User Story 1 — An agent finally answers, and the answer is on the chain (P1)
As an operator, I ask a question and get the model's answer on stdout, with the whole exchange
inspectable afterwards.
**Acceptance:**
1. **Given** a local OpenAI-compatible provider listening on loopback, **When**
   `skein chat --root <root> --silo <id> --model <name> --prompt "…"` is invoked, **Then** the exit
   code is 0, **stdout is exactly the assistant's answer**, and stderr names the run id.
2. **Given** that invocation completed, **When** `skein ledger log --root <root> --silo <id>` is
   invoked as a **second process**, **Then** the run's steps are listed —
   `iteration_boundary`, `llm_request`, `llm_response`, `budget_spent`, `exit` — and
   `skein ledger verify` reports the run `ok`.

### User Story 2 — The wire is the OpenAI chat-completions contract, exactly (P1)
As a maintainer, the request the client puts on the socket must be the documented contract, not
something that merely happens to work against one provider.
**Acceptance:**
1. **Given** a client pointed at a stub server, **When** one turn is taken, **Then** the stub
   observes `POST /v1/chat/completions HTTP/1.1`, a `content-type: application/json` header, and a
   body whose `model` is the configured name, whose `messages` are the conversation's roles and
   texts **in order**, and whose `stream` is `false`.
2. **Given** a conversation holding system, user and assistant messages, **When** it is sent,
   **Then** the three `skein_core::Role`s map to `"system"`, `"user"` and `"assistant"`, in order.

### User Story 3 — A local provider is the only thing this can talk to (P1)
As a security reviewer, "local-first" must be checkable, not asserted.
**Acceptance:**
1. **Given** a base URL naming a public host (`http://ollama.example.com/v1`), **When**
   `LocalEndpoint::parse` is called, **Then** it fails with `SkeinError::Model` **before any socket
   or DNS query exists** — the name is refused without being resolved.
2. **Given** a base URL naming a private-LAN literal (`http://192.168.1.10:11434/v1`), **When**
   it is parsed, **Then** it fails: a valid IP that is not loopback is refused (ADR-0002 D4).
3. **Given** an `https://` base URL, **When** it is parsed, **Then** it fails on the scheme with a
   message naming local-only operation.
4. **Given** `skein chat --base-url http://192.168.1.10:11434/v1`, **When** it is invoked against a
   silo, **Then** the exit code is 1 and **the silo's ledger holds no run** — the refusal happens
   before the loop starts.

### User Story 4 — Token accounting is real, or the turn fails (P1)
As an operator, the token budget is what stops a runaway loop, so a fabricated token count is worse
than an error.
**Acceptance:**
1. **Given** a response carrying `usage.total_tokens`, **When** it is parsed, **Then**
   `tokens_used` is that number.
2. **Given** a response carrying `usage.prompt_tokens` and `usage.completion_tokens` but no
   `total_tokens`, **When** it is parsed, **Then** `tokens_used` is their sum.
3. **Given** a response with **no** `usage` object at all, **When** it is parsed, **Then** the turn
   fails with `SkeinError::Model` naming the missing metering. It does **not** meter zero.

### User Story 5 — A truncated answer is not a completed answer (P1)
As a maintainer, only the engine may decide a run is done (Principle VIII(a)).
**Acceptance:**
1. **Given** a response whose `finish_reason` is `"length"`, **When** it is parsed, **Then**
   `final_output` is `false` — the provider truncated the model mid-thought, and calling that a
   completed answer would let a truncation launder itself past `LoopController`.
2. **Given** a response carrying `tool_calls`, **When** it is parsed, **Then** the calls are
   translated into `skein_core::ToolCall`s and `final_output` is `false`.

### User Story 6 — A provider failure is legible, and never silent (P1)
As an operator, when the model does not answer I need to know why, in one line.
**Acceptance:**
1. **Given** nothing listening on the configured port, **When** a turn is taken, **Then**
   `SkeinError::Model` names the base URL and asks whether a local provider is listening.
2. **Given** a provider answering `404` with `{"error":{"message":"model \"nope\" not found"}}`,
   **When** a turn is taken, **Then** `SkeinError::Model` carries **the provider's own message**.
3. **Given** a provider that accepts the request and never replies, **When** a turn is taken,
   **Then** the turn fails on a timeout rather than blocking the run.
4. **Given** a body that is not a recognisable chat-completions response, **When** a turn is taken,
   **Then** `SkeinError::Model` says so and names the endpoint.

### User Story 7 — A run the engine stopped does not look like an answer (P1)
As an operator, an empty answer with exit 0 is slice 011's User Story 4 failure.
**Acceptance:**
1. **Given** a provider that never returns `finish_reason: "stop"`, **When** `skein chat` runs with
   a budget it exhausts, **Then** the exit code is 1, **stdout is empty**, and stderr names the exit
   (`MaxIters`, `MaxTokens` or `NoProgress`) via `SkeinError::Unfinished`.

## Requirements

- **FR-001**: A new crate `crates/skein-gateway` MUST hold the only `ModelClient` implementation in
  product code, and MUST be the only crate in the product naming HTTP or the OpenAI wire format.
  `skein-core` MUST NOT depend on it (Principle IV).
- **FR-002**: `skein-core`'s dependency list MUST remain `serde`, `serde_json`, `thiserror`, `sha2`,
  `zeroize`.
- **FR-003**: `ureq` MUST be declared with `default-features = false`, so **no TLS backend is
  compiled in** and an `https://` endpoint is unreachable at the transport. This is a structural
  security property and MUST NOT be relaxed for convenience.
- **FR-004**: `LocalEndpoint::parse(&str)` MUST be the only way to obtain the address
  `OpenAiCompatClient` posts to, and MUST reject, **before any socket is opened**:
  a non-`http` scheme; a host that parses as an IP address and is not loopback; and any host name
  other than the literal `localhost`. A foreign host name MUST be refused **without being
  resolved** — resolving it would itself be egress, since the name would leave the machine in a DNS
  query.
- **FR-005**: `LocalEndpoint::parse` MUST accept `localhost` only if **every** address it resolves
  to is loopback.
- **FR-006**: The request body MUST be
  `{"model": …, "messages": [{"role": …, "content": …}], "stream": false}`. `"stream": false` MUST
  be explicit: a provider defaulting to SSE would break the parse silently.
- **FR-007**: `skein_core::Role::{User, Assistant, System}` MUST map to `"user"`, `"assistant"`,
  `"system"`, and message order MUST be preserved.
- **FR-008**: `final_output` MUST be `finish_reason == "stop"` **and** `tool_calls` empty. A
  `finish_reason` of `"length"` MUST NOT be final.
- **FR-009**: A `tool_calls` array in the response MUST be translated into
  `Vec<skein_core::ToolCall>` with `tool = function.name` and `args` parsed from
  `function.arguments`. Silently dropping a model intent would weaken Principle V, because the
  chain records the `TurnResponse` and not the raw body.
- **FR-010**: `tokens_used` MUST come from `usage.total_tokens`, else from
  `usage.prompt_tokens + usage.completion_tokens`, else the turn MUST fail with
  `SkeinError::Model`. A `0` fallback is forbidden: `LoopController::should_exit` stops on
  `tokens >= max_tokens`, so a silent zero would disable the token budget while looking like it
  worked (Principle VIII, NON-NEGOTIABLE).
- **FR-011**: Every provider failure MUST surface as `SkeinError::Model` whose message names the
  endpoint. A non-2xx MUST carry the provider's own body, truncated.
- **FR-012**: Connect and global timeouts MUST be configured explicitly, and the global timeout MUST
  be settable from `skein chat --timeout-secs` (default 120s).
- **FR-013**: `skein-core` MUST gain exactly one additive `SkeinError` variant,
  `Unfinished { run_id, exit }`, and nothing else. No existing signature may change.
- **FR-014**: `skein chat` MUST require `--model` with no default. Defaulting to a model the machine
  may not have would produce a 404 that looks like a bug.
- **FR-015**: The base URL MUST come from `--base-url`, else `$SKEIN_MODEL_BASE_URL`, else
  `http://localhost:11434/v1`, mirroring the `--root`/`$SKEIN_ROOT` precedent.
- **FR-016**: `skein chat` MUST write the assistant's answer to **stdout and nothing else**, and the
  run id to stderr.
- **FR-017**: An exit other than `Exit::FinalOutput` MUST produce exit code 1 with **empty stdout**.
  `LoopRun.final_message` is `None` for every non-`FinalOutput` exit, so there is nothing to print,
  and printing nothing with exit 0 would be a silently wrong answer.
- **FR-018**: `skein chat` MUST supply `NativeLoop`'s two structurally-required collaborators
  without shipping a fake: a `ToolTransport` that is **unreachable by construction** (paired with
  `ToolPolicy::new(vec![], vec![])`, so deny-by-default refuses every name before the transport is
  consulted) and a `ProgressProbe` that returns `false` always, because a chat with no tools has
  **no external ground truth** and Principle VIII(b) forbids substituting the model's own judgment
  for one.
- **FR-019**: No automated test may require a running Ollama. The one live test MUST be `#[ignore]`d
  and MUST skip cleanly when its environment variable is unset.
- **FR-020**: `crates/skein-mcp`, `crates/skein-acp` and `crates/skein-silo` MUST be unchanged.

## Success Criteria

- **SC-001**: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` and `cargo build --workspace` all clean; the suite is 82 pre-existing +
  the slice's new tests, with the measured number recorded in `tasks.md`.
- **SC-002**: `skein chat --help` succeeds, and a hand-run `skein chat` against a real local Ollama
  is recorded with its transcript and the resulting `skein ledger log`/`verify` output — the
  slice's headline claim, checked by running it.
- **SC-003**: Every wire test asserts **bytes on a real socket** served by `std::net::TcpListener`.
  No HTTP-mocking dependency (`wiremock`, `mockito`) is added, and no test asserts an intent where
  it could assert a byte.
- **SC-004**: Every `skein chat` test is a **process invocation of the real binary**, following
  slice 011's SC-003.
- **SC-005**: `git diff dev -- crates/skein-mcp/ crates/skein-acp/ crates/skein-silo/ spikes/
  .github/ rust-toolchain.toml` is empty.
- **SC-006**: `git diff dev -- crates/skein-core/` is one added error variant plus its test; all 82
  pre-existing tests stay live controls with their bodies unchanged.
- **SC-007**: `cargo tree -e normal -p skein-gateway` shows **no TLS crate** — no `rustls`, no
  `native-tls`, no `webpki`. FR-003 is asserted by measurement, not by the manifest's intent.
- As in specs 004–011, the macOS and Linux legs of `core.yml` are unobserved until the repository
  has a remote; only the Windows leg is run locally.

## Assumptions

- **`localhost` re-resolution is a TOCTOU residual, not a closed hole.** Stated in point 5 above.
  The mitigation is that foreign names are refused *without* resolution, so no name ever leaves the
  machine; the full fix is a process-level socket-deny boundary that does not exist yet.
- **A conversation can now carry a secret into the chain, and this slice makes that more urgent.**
  `NativeLoop::run` appends `LlmRequest`/`LlmResponse` payloads **raw** — `ToolCall`/`ToolResult`
  pass through the `Redactor`, model I/O does not. Slice 011 recorded this on its backlog; a real
  conversation makes it reachable. The fix belongs to the governed loop, not to the gateway or the
  CLI, so it stays on the next-slice list rather than being papered over here.
- **No LiteLLM sidecar is required.** Ollama's own endpoint is OpenAI-compatible, so
  `--base-url http://localhost:11434/v1` speaks the same contract design §4.5 specifies. Pointing
  `--base-url` at a LiteLLM sidecar (`http://localhost:4000/v1`) works with **no code change**;
  requiring a Python process to speak a protocol Ollama already speaks would be a prerequisite with
  no capability behind it (Principle VII).
- **No provider authentication.** Ollama's OpenAI-compatible endpoint requires no credential, so an
  `Authorization: Bearer` path would be a capability with no caller. It is deferred **with its
  constraint pre-written**: when a local gateway needs a token, it MUST arrive as a `SecretRef`
  resolved through the existing `SecretProvider` (Principle VI), never a literal and never a
  plaintext config value.
- **`--base-url`'s trailing path is preserved.** `<base>/chat/completions` is formed by appending,
  so `http://localhost:11434/v1` yields `/v1/chat/completions`. A trailing slash on the base URL is
  normalised away; anything else is the operator's own path prefix and is respected.
- **A tool-less chat's `no_progress_limit` is what stops a model that never finishes.** Every
  iteration is stale by construction (FR-018), so `NoProgress` is the backstop. In the normal case
  it never bites: `should_exit` checks `final_output` first, and a tool-less turn returns
  `finish_reason: "stop"` on iteration 1.
