# Implementation Plan: `heddle-gateway` — the first real `ModelClient`, and `heddle chat` (v0 slice)

**Branch**: `012-model-gateway` | **Date**: 2026-09-03 | **Spec**: `specs/012-model-gateway/spec.md`

## Summary

A new workspace member `crates/heddle-gateway` holds the one `ModelClient` implementation in product
code, the OpenAI chat-completions wire translation, and the loopback guard:

```rust
pub struct LocalEndpoint { /* … */ }
impl LocalEndpoint { pub fn parse(base_url: &str) -> Result<LocalEndpoint>; }

pub struct OpenAiCompatClient { /* … */ }
impl OpenAiCompatClient { pub fn new(endpoint: LocalEndpoint, model: impl Into<String>) -> Self; }
impl ModelClient for OpenAiCompatClient { fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse>; }
```

`heddle-core` gains **one** additive error variant. `heddle-cli` gains one subcommand, `heddle chat`,
and one dependency, `heddle-gateway`. Nothing existing is rewritten: `ModelClient`, `NativeLoop`,
`LoopController`, `Ledger`, `ToolGateway` and both protocol crates are untouched, so the 82-test
baseline stays a live control throughout.

## Decisions

### D1 — Isolate the protocol in its own crate

`heddle-core` must not learn HTTP (Principle IV; its dependency list is `serde`, `serde_json`,
`thiserror`, `sha2`, `zeroize` and stays that way). `heddle-mcp` isolates MCP and `heddle-acp`
isolates ACP the same way, each stating it in its module docstring. `heddle-gateway` is named for
design §4.5's `gateway/` component.

*Rejected:* putting the client in `heddle-silo` (which is storage) or `heddle-cli` (which has **no
`lib` target** by deliberate design, so an implementation there would be unreachable from
`heddle-acp` when `acp-agent` lands).

### D2 — `ureq 3` with **no default features**, not `reqwest`

Two decisive reasons, both re-measured in T1:

1. **It matches the `ModelClient::turn` boundary exactly.** `turn` is `&mut self -> Result<…>`,
   synchronous. `heddle-mcp` owns a `tokio::runtime::Runtime` and calls `block_on` *because `rmcp` is
   async-only* — there was no choice. Here there is one, and taking it removes the hazard
   `RmcpToolTransport`'s docstring has to warn about (*"no method of this type may be called from
   inside an async context: `Runtime::block_on` panics when a runtime is already entered"*).
   `heddle-acp` calls `ModelClient::turn` from a spawned OS thread inside an async program, so a
   `block_on`-based client would be a live panic risk there. A blocking client is not.
2. **No TLS ⇒ no cloud provider, structurally.** With `default-features = false`, an `https://` POST
   fails at the transport with `ureq::Error::TlsRequired`. Every cloud provider endpoint is HTTPS.
   That makes Principle II a property of the build, not of a code review.

Plus: `MIT OR Apache-2.0`, MSRV 1.85 for `ureq`/`ureq-proto`, under the 1.97 pin, and **7 packages
new to this workspace** against roughly 65 for `reqwest` + `wiremock`.

*Rejected — `reqwest` + `wiremock`*, the `spikes/runtime-loop/opt-a-native/` stack and therefore the
"safe" choice: it forces a `tokio` runtime into a synchronous boundary that does not need one, at
roughly 9× the dependency cost (Principle VII, "start simple").

*Rejected — hand-rolled HTTP/1.1 over `std::net::TcpStream` in product code*: Principle VII says we
**reuse** proven existing tools rather than rewrite them, and chunked transfer-encoding, keep-alive
and header parsing are exactly what an audited crate should own. A *test* stub is a different
matter — see D3.

### D3 — Test the wire against a `std::net::TcpListener` stub, with **no new dev-dependency**

The client is exercised against a real socket speaking real HTTP/1.1 bytes, served by a small `std`
helper inside the test file: bind `127.0.0.1:0`, accept, read headers + `Content-Length` body, reply
with a canned body and `Connection: close`. The tests assert **the exact bytes the client sent**.

`Connection: close` makes each turn a fresh accept, measured: a two-turn loop through one
`ureq::Agent` produced exactly 2 accepts. Without it, connection reuse would make a multi-turn stub
racy.

*Rejected — `wiremock 0.6`* (the spike's choice): it is `async`, so a synchronous client under test
would need a `tokio` runtime created purely to start the mock — reintroducing in the test suite the
dependency D2 removed from the product, for less fidelity than a socket that shows the literal
bytes. *Rejected — `mockito`*: sync-friendly but still tokio-backed internally, and still a new
dependency where `std` suffices. This mirrors slice 011's T1 finding, which established that this
workspace tests a process/protocol boundary without a harness crate.

### D4 — Talk to Ollama's own OpenAI-compatible endpoint; no LiteLLM sidecar

`heddle chat` defaults to `http://localhost:11434/v1` and POSTs to `<base>/chat/completions`. That is
the same wire contract design §4.5 specifies, so pointing `--base-url` at a LiteLLM sidecar works
with no code change — but requiring a Python process to speak a protocol Ollama already speaks would
be a prerequisite with no capability behind it (Principle VII). `scripts/bootstrap.ps1 -WithOllama`
already installs Ollama and pulls `llama3.1`.

### D5 — Loopback allowlist, enforced at construction, before any socket is opened

`LocalEndpoint::parse(&str)` is the guard, and `OpenAiCompatClient` cannot be constructed without
one. Rules, all measured against `http::Uri`:

| Input | Verdict |
|---|---|
| `http://127.0.0.1:11434/v1` | accept — `host()` = `"127.0.0.1"`, parses as `IpAddr`, `is_loopback()` |
| `http://[::1]:11434/v1` | accept — `host()` returns `"[::1]"` **with brackets**; stripped before `IpAddr::from_str` |
| `http://localhost:11434/v1` | accept **only if every resolved address is loopback** — measured `[[::1]:11434, 127.0.0.1:11434]`, `all(is_loopback) == true` |
| `http://192.168.1.10:11434/v1` | **reject** — a valid IP that is not loopback (ADR-0002 D4) |
| `http://ollama.example.com/v1` | **reject without resolving it** — resolving would itself be egress: the name would leave the machine in a DNS query |
| `https://…` | **reject** on the scheme — and `ureq` without TLS is the hard floor underneath if the check were ever removed |

*Why resolve `localhost` rather than trust the name:* a `hosts` entry can point `localhost`
anywhere. Requiring **all** resolved addresses to be loopback closes that. The TOCTOU residual is in
the spec's Assumptions rather than hidden.

*Rejected — accept IP literals only:* removes DNS entirely, but `http://localhost:11434` is Ollama's
documented URL and `localhost` is exactly how users paper over the IPv4/IPv6 difference Windows and
Linux disagree on. Refusing it would generate confusing bug reports for a hole the resolution check
already closes.

### D6 — No authentication in this slice

Ollama's OpenAI-compatible endpoint needs no credential, so an `Authorization: Bearer` path would be
a capability with **no caller** — the Principle VII argument slice 011 used to reject the placeholder
model. Deferred **with its constraint already written**: a token MUST arrive as a `SecretRef`
resolved through the existing `SecretProvider` (Principle VI), never a literal.

### D7 — Wire translation, stated exactly

**Request**, built with `serde_json` and sent as a `String` so the bytes are ours:

```json
{"model": "<--model>", "messages": [{"role": "user|assistant|system", "content": "<Message::text()>"}], "stream": false}
```

`Message::text()` concatenates the message's `Content::Text` parts; `Content` has exactly one
variant today, so nothing is lost. **No `tools` field** — `TurnRequest` cannot express one.
`"stream": false` is explicit, not implied.

**Response**, from `choices[0]`:

- `message.content` (may be `null`) → `Message::assistant_text(…)`.
- `message.tool_calls[]`, if present, → `Vec<ToolCall>` with `tool = function.name` and
  `args = serde_json::from_str(function.arguments)`. We advertise no tools, so this should not
  occur — but silently dropping a model intent would weaken Principle V, because the chain records
  the `TurnResponse`, not the raw body.
- `finish_reason` → `final_output = (finish_reason == "stop" && tool_calls.is_empty())`.
  **`"length"` is NOT final**: it means the provider truncated the model mid-thought, and calling
  that a completed answer would let a truncation launder itself past `LoopController`, which
  Principle VIII(a) reserves to the engine.
- `usage` → `tokens_used`, per D8.

### D8 — `tokens_used` is real, or the turn fails

Order: `usage.total_tokens` if present; else `usage.prompt_tokens + usage.completion_tokens` if both
present; else `Err(HeddleError::Model(…))` naming the missing metering.

A `0` fallback is the tempting third option and is rejected: `LoopController::should_exit` stops on
`self.tokens >= budget.max_tokens`, so a silent `0` would disable the token budget while looking
like it worked. Principle VIII is NON-NEGOTIABLE, and refusing loudly is this project's established
answer to "I cannot honestly produce this value" (`heddle-cli`'s `kind_name` refusing rather than
printing a blank column; `secret set` refusing an empty value).

### D9 — Error mapping: every failure is a `HeddleError::Model` naming the endpoint

`ureq::Agent` is configured with `http_status_as_error(false)`, so a provider error carries the
provider's own message rather than being flattened into a status code.

| Failure | Message shape |
|---|---|
| connect refused / io | `model provider: POST <base>/chat/completions failed: <ureq error>; is a local provider listening at <base>?` |
| non-2xx | `model provider: <base> returned <status>: <body, truncated>` |
| unparseable body / missing `choices[0]` | `model provider: <base> returned an unrecognised chat-completions response: <detail>` |
| missing `usage` | D8's message |

Timeouts are configured explicitly: `timeout_connect(Some(5s))` and
`timeout_global(Some(<--timeout-secs>, default 120s))`.

### D10 — `heddle chat`, and the two honest stand-ins it needs

```
heddle chat --silo <ID> [--root <PATH>] --model <NAME> [--base-url <URL>]
           [--prompt <TEXT>] [--run-id <ID>]
           [--max-iters N] [--max-tokens N] [--no-progress-limit N] [--timeout-secs S]
```

`--model` is required with no default. `--base-url` → `$HEDDLE_MODEL_BASE_URL` →
`http://localhost:11434/v1`. The prompt comes from `--prompt`, else stdin to EOF. One prompt, one
run — no interactive REPL. The run lands on the **real silo chain** via
`Silo::open(root, id)?.ledger()?`. `run_id` defaults to `chat-{unix_millis}-{pid}`; `--run-id`
overrides. stdout carries the answer and nothing else; the run id goes to stderr. An exit other
than `FinalOutput` is exit code 1 with empty stdout.

Two collaborators `NativeLoop` structurally requires, supplied without shipping a fake:

- **`ToolTransport`** — a private `NoTools` whose `call` returns `HeddleError::Tool`, paired with
  `ToolPolicy::new(vec![], vec![])`. Deny-by-default means **no name can reach it**: the policy
  refuses every tool before the transport is consulted (`ToolGateway::call_captured` decides first).
  It is unreachable by construction, not a stub that pretends to work.
- **`ProgressProbe`** — a private `NoGroundTruth` returning `false` always: a chat with no tools has
  **no external ground truth**, and Principle VIII(b) forbids substituting the model's own judgment
  for one. Every iteration is therefore stale and a model that never finishes is stopped by the
  no-progress budget.

### D11 — One additive change to `heddle-core`

```rust
/// A run ended on a budget rather than with an answer. Not a provider failure:
/// the engine stopped the model, which is Principle VIII working.
#[error("run {run_id} ended without a final answer: {exit}")]
Unfinished { run_id: String, exit: String },
```

`heddle chat` needs to fail with exit 1 when the engine stops a run, and the alternatives are worse:
`HeddleError::Model` would print `model provider:` for a budget decision no provider made, and
`HeddleError::Storage` is a lie. Additive, no signature changes, mirroring slice 011's precedent of
closing a CLI's gap *by adding to the API* rather than reaching around it (`Ledger::runs()`).
**`ModelClient` itself is unchanged** — this slice supplies an implementation behind the trait,
which is the whole point of Principle IV.

## Complexity Tracking

| Addition | Callers today | Why it is not speculative |
|---|---|---|
| `crates/heddle-gateway` | `heddle-cli`'s `chat` | The trait has had no implementation for eight slices; this is the one. |
| `LocalEndpoint` | `OpenAiCompatClient::new` | Principle II's enforcement point. Not constructible around. |
| `HeddleError::Unfinished` | `heddle-cli`'s `chat` | Demanded by the budget-exit test, added when that test asked for it. |
| `--timeout-secs` | the client's `timeout_global` | The one knob whose absence is a hang, which is the failure the operator cannot diagnose. |

Not added, though each was considered: an `Authorization` header (no caller — D6), a `tools` field
(`TurnRequest` cannot express one), a streaming path, a `--json` flag, a config file, a retry policy
(a local provider that refuses twice refuses for a reason, and a silent retry hides it).
