# Tasks: `skein-gateway` — the first real `ModelClient`, and `skein chat` (v0 slice)

**Spec:** `specs/012-model-gateway/spec.md` · TDD (red→green), product code in
`crates/skein-gateway` and `crates/skein-cli` plus one additive error variant in
`crates/skein-core`, branch `012-model-gateway` cut from `dev` after slice 011 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the gateway is a library behind `ModelClient`, and `skein chat` is its complete
  client — the first command that runs the governed loop. The one place the CLI needed something the
  API lacked (a name for "the engine stopped this run") is closed by *adding to `SkeinError`*, not
  by reaching around it · II Local-first ✅ **two independent locks**: `LocalEndpoint::parse` refuses
  anything but loopback before a socket exists, and `ureq` with no TLS feature cannot speak HTTPS at
  all (measured: `TlsRequired`). Foreign host names are refused **without** being resolved, so no
  name leaves the machine in a DNS query
- III Test-First ✅ (T1 pins the `ureq` surface against the vendored source **and** a compiled probe
  before any product code; T3's red observed before T4, T5/T6/T7 each red before green, T8's before
  its green, T10's before T11) · IV Inverted coupling ✅ (`skein-gateway` is the only crate naming
  HTTP or the OpenAI wire format; `skein-core` does not depend on it and its dependency list is
  unchanged at five crates)
- V Traceability ✅ (`NativeLoop` appends `LlmRequest`/`LlmResponse`/`BudgetSpent` unchanged; the
  `BudgetSpent` payload is the provider's **real** metering, asserted against the stub's own number.
  The gap — the chain holds the *translated* request/response, not raw wire bytes — is stated in the
  spec as first-class prose, not a footnote)
- VI Security ✅ (no credential path at all in this slice, and the deferral carries its constraint
  pre-written: a token MUST arrive as a `SecretRef` through `SecretProvider`. The spec records that a
  real conversation can now carry a secret into the raw `LlmRequest` payload, which slice 011 had
  already put on the backlog and this slice makes reachable)
- VII Neutrality ✅ (one crate, one client, one subcommand, one error variant. **No `acp-agent`, no
  streaming, no auth, no REPL, no `--json`, no config file, no retry policy, no LiteLLM
  requirement.** Seven new packages, and no new dev-dependency: the wire tests are `std` sockets)
- VIII Loop discipline ✅ NON-NEGOTIABLE and load-bearing here for the first time: `tokens_used`
  comes from real provider metering or the turn **fails loudly** rather than metering zero, because a
  silent zero would disable `LoopController`'s token budget; `finish_reason: "length"` is **not**
  `final_output`, so a truncation cannot launder itself past the controller; and the `ProgressProbe`
  returns `false` always, because a tool-less chat has no external ground truth and VIII(b) forbids
  substituting the model's own judgment for one
- Cross-platform ✅ (no `#[cfg]` in either new file; the stub server binds `127.0.0.1:0` so no fixed
  port is assumed; the connection-refused test asserts the message *shape*, not the OS's own
  wording. `core.yml`'s `paths:` already covers `crates/**` and `Cargo.toml`, and
  `members = ["crates/*"]` already covers a new crate — confirmed by reading, not edited).

## Tasks
- [x] **T0** `specs/012-model-gateway/{spec.md,plan.md,tasks.md}`; branch `012-model-gateway` cut
      from `dev` with slice 011 merged
- [x] **T1** pinned the `ureq` surface against the vendored `ureq 3.4.0` / `ureq-proto 0.6.1` source
      and a compiled probe crate **outside** the repository, *before* any product code; measured,
      not copied — see below
- [x] **T2** control baseline: `cargo test --workspace` before any edit — **82**
- [x] **T3** RED — `crates/skein-gateway/tests/openai_compat.rs` with the `std::net::TcpListener`
      stub helper, against the not-yet-existing `skein_gateway::{LocalEndpoint, OpenAiCompatClient}`
- [x] **T4** GREEN — `crates/skein-gateway/` (`Cargo.toml`, `src/lib.rs`) + two root
      `[workspace.dependencies]` lines
- [x] **T5** RED→GREEN — the loopback allowlist (`LocalEndpoint::parse`)
- [x] **T6** RED→GREEN — token accounting (D8's three cases)
- [x] **T7** RED→GREEN — provider failure modes, `finish_reason: "length"`, `tool_calls`
- [x] **T8** RED→GREEN — end to end through `NativeLoop` against a two-response stub, still with no
      Ollama
- [x] **T9** the optional `#[ignore]`d live test against a real Ollama
- [x] **T10** RED — `crates/skein-cli/tests/cli_chat.rs`, process tests of the real binary
- [x] **T11** GREEN — `crates/skein-cli/src/chat.rs`, the `Chat` arm, and the `main.rs` docstring
      correction
- [x] **T12** `SkeinError::Unfinished` with its own test
- [x] **T13** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`, `cargo build --workspace`, + run `skein chat --help` and
      `skein chat` against a real local Ollama
- [x] **T14** control diff, dependency drift, close-out

## Pinned `ureq` surface (T1)

Measured on 2026-09-03 on this Windows host at the pinned 1.97, from the **vendored** source in the
registry cache and from a throwaway probe crate outside this repository (built, run, deleted;
`git status` clean before and after). The advisory plan's figures were **not** copied — they were
re-derived, and they hold, with two corrections noted below.

| Item | Pinned spelling / measured value |
|---|---|
| Declared dependency | `ureq = { version = "3", default-features = false }` |
| Resolved versions | `ureq 3.4.0`, `ureq-proto 0.6.1`, `http 1.5.0`, `httparse 1.10.1`, `percent-encoding 2.3.2`, `utf8-zero 0.8.1`, `base64 0.23.1` |
| License | `MIT OR Apache-2.0` (both `ureq-3.4.0/Cargo.toml` and `ureq-proto-0.6.1/Cargo.toml`) |
| MSRV | **1.85** for `ureq` and `ureq-proto` (`rust-version` in each vendored `Cargo.toml`), under the 1.97 pin — **no toolchain change** |
| `ureq`'s own default feature set | `["rustls", "gzip"]` (`[features] default`) — ours is empty, so `rustls`, `flate2`, `webpki-roots`, `ring` and the rest of the TLS graph are never compiled |
| `cargo tree -e normal` (probe, `ureq` + `http` + `serde_json`) | `ureq 3.4.0` → `base64 0.23.1`, `log 0.4.34`, `percent-encoding 2.3.2`, `utf8-zero 0.8.1`, `ureq-proto 0.6.1` → `base64 0.23.1`, `http 1.5.0`, `httparse 1.10.1`, `log 0.4.34`; `http 1.5.0` → `bytes 1.12.1`, `itoa 1.0.18` |
| Packages **new to this workspace** | **7**: `ureq`, `ureq-proto`, `http`, `httparse`, `percent-encoding`, `utf8-zero`, `base64 0.23.1`. `bytes 1.12.1`, `log 0.4.34` and `itoa 1.0.18` are already in the root `Cargo.lock`; `base64` **is** new as a second major beside the existing `base64 0.22.1` |
| Body read limit | `Body::read_to_string()` applies `MAX_BODY_SIZE = 10 MiB` (`ureq-3.4.0/src/body/mod.rs:30`), so no explicit `.limit()` is needed for a chat-completions body |

**Seven behaviours were measured in the probe, not assumed.**

1. **The exact bytes on the wire.** A `POST` with a `content-type: application/json` header and a
   `String` body produced, verbatim:

   ```
   POST /v1/chat/completions HTTP/1.1
   content-length: 35
   user-agent: ureq/3.4.0
   accept: */*
   host: 127.0.0.1:63076
   content-type: application/json

   {"model":"llama3.1","stream":false}
   ```

   The body is sent verbatim and the request line, headers and blank-line separator are fully
   observable from a `std` socket — which is what makes T3's byte assertions possible without a
   mocking crate.
2. **`https://` is unreachable without a TLS feature.** `a.post("https://api.openai.com/v1/chat/completions").send("{}")`
   returned `Err(ureq::Error::TlsRequired)`, `Debug` = `TlsRequired`, `Display` =
   `TLS required, but transport is unsecured`. `matches!(e, ureq::Error::TlsRequired)` is `true`, so
   the floor beneath `LocalEndpoint::parse` is machine-assertable.
3. **A dead loopback port is a clean error, not a hang.** `Err(Io(Custom { kind: ConnectionRefused,
   error: "Connection refused" }))`, `Display` = `io: Connection refused`, returned immediately.
4. **`timeout_global` fires.** With a 300 ms budget against a server that slept 2 s before replying:
   `Err(Timeout(Global))`, `Display` = `timeout: global`, after a measured **310.164 ms**.
5. **`http_status_as_error(false)` returns a readable non-2xx.** A stub 404 with an Ollama-shaped
   body came back as `Ok` with `status().as_u16() == 404` and
   `body_mut().read_to_string()` = `{"error":{"message":"model \"nope\" not found","type":"api_error"}}`
   — the provider's own message, which is what the operator needs to see.
6. **`http::Uri` on the D5 table**, all seven inputs:

   | Input | `scheme_str()` | `host()` | `port_u16()` |
   |---|---|---|---|
   | `http://127.0.0.1:11434/v1` | `Some("http")` | `Some("127.0.0.1")` | `Some(11434)` |
   | `http://[::1]:11434/v1` | `Some("http")` | `Some("[::1]")` — **with brackets** | `Some(11434)` |
   | `http://localhost:11434/v1` | `Some("http")` | `Some("localhost")` | `Some(11434)` |
   | `http://192.168.1.10:11434/v1` | `Some("http")` | `Some("192.168.1.10")` | `Some(11434)` |
   | `http://ollama.example.com/v1` | `Some("http")` | `Some("ollama.example.com")` | **`None`** |
   | `https://api.openai.com/v1` | `Some("https")` | `Some("api.openai.com")` | `None` |
   | `http://localhost/v1` | `Some("http")` | `Some("localhost")` | **`None`** |

   `IpAddr::from_str` parses `"127.0.0.1"` (`is_loopback() == true`), `"::1"` (`true`) and
   `"192.168.1.10"` (`false`). The bracket form must be stripped before `from_str`, and **a base URL
   may legitimately omit the port**, so the resolution check needs an explicit default rather than
   `port_u16().unwrap()`.
7. **`("localhost", 11434).to_socket_addrs()`** resolved to `[[::1]:11434, 127.0.0.1:11434]` on this
   host, `all(|a| a.ip().is_loopback()) == true` — so requiring *all* resolved addresses to be
   loopback accepts the documented Ollama URL while closing the hostile-`hosts`-entry hole.
8. **`Connection: close` makes multi-turn stubs deterministic.** Two turns driven through **one**
   `ureq::Agent` against a stub that closes each response produced exactly **2 accepts**. Without it,
   connection reuse would make T8's two-response stub racy.

**Two corrections to the advisory plan's figures.**

- The advisory plan quoted the timeout's `Display` as `timeout: global after 312ms`. The measured
  `Display` is **`timeout: global`** with no duration in it; `312ms` was the advisory run's *wall
  clock*, not part of the message. This matters, because a test asserting the quoted string would
  fail. The claim it supported — that a hanging server yields a clean error rather than a hang — is
  unchanged, and the wall clock re-measured at 310 ms for a 300 ms budget.
- The advisory plan's D5 table implied every base URL carries a port. Measured,
  `http://localhost/v1` and `http://ollama.example.com/v1` both give `port_u16() == None`, so
  `LocalEndpoint::parse` defaults the port to 80 for the resolution check. Recorded here rather than
  discovered at runtime.

## Control baseline (T2)

`cargo test --workspace` on `012-model-gateway` @ `188540a` (identical to `dev`), working tree
clean, 2026-09-03, before any edit: **82 passing**, 0 failed, 0 ignored — `acp_session` 13,
`cli_ledger` 8, `cli_secret` 2, `core` 13, `native_loop` 18, `tool_gateway` 9, `rmcp_gateway` 7,
`silo_ledger` 7, `silo_secret` 5. The five `src/lib.rs`/`src/main.rs` unit-test targets and the four
doc-test targets each contribute `0 passed`. This is the number T13 diffs against, and it matches
slice 011's recorded gate figure exactly.

## Observed red (Constitution III)

Recorded below as each step lands.

## Gate run (T13)

Recorded below.

## Control diff (T14)

Recorded below.

## Drift (T14)

Recorded below.

## Out of scope

Deliberately not done, so no one helpfully does it:

- **`skein acp-agent`** — the next slice. Needs a stdio `ConnectTo<Agent>`, a `tokio` runtime inside
  `skein-cli`, a `SessionParts` factory and a subprocess ACP end-to-end test.
- **Cloud providers, of any kind.** Structurally impossible in this build (no TLS), and it stays
  that way until an egress-policy layer exists.
- **LAN-reachable local providers** (`http://192.168.…`). Refused per ADR-0002 D4's loopback
  allowlist; needs the egress policy and the socket-deny boundary first.
- **LiteLLM's actual feature set** — multi-provider routing, cost/quota tracking, load balancing,
  guardrails, model capability descriptors, policy routing (design §4.5's fuller vision). This slice
  ships one provider path that happens to be OpenAI-compatible, which is what makes a LiteLLM
  sidecar a drop-in `--base-url` later rather than a rewrite.
- **Streaming (SSE).** `"stream": false` is sent explicitly. Streaming changes the Ledger capture
  shape and the ACP `AgentMessageChunk` story, and belongs with `acp-agent`.
- **Tool advertisement in the request.** Needs tool *discovery*, which `ToolGateway` defers and
  `TurnRequest` cannot express.
- **Raw-wire-byte capture in the Ledger.** The chain records the translated
  `TurnRequest`/`TurnResponse`; byte-exact capture (Spike 1's criterion C1, design §4.5's "exact
  model I/O") needs a new `StepKind` and a change to `NativeLoop`.
- **Redaction on the model-I/O path.** Already on slice 011's backlog; this slice makes it more
  urgent, and says so, but fixing it means changing the governed loop.
- **Provider authentication.** No caller yet; recorded with its Principle VI constraint pre-written.
- **An interactive chat REPL, multi-turn sessions, `--json` output, a config file** holding the base
  URL and model. One prompt, one run, plain stdout.
- **A retry policy.** A local provider that refuses twice refuses for a reason, and a silent retry
  hides it.
- **`spikes/`** — untouched (ADR-0004 D2). `spikes/runtime-loop/opt-a-native/` is read as precedent
  for the dependency decision and left exactly as it is.

## Next slice (not this feature)
- [ ] **`skein acp-agent`** — the stdio ACP server, now that a real `ModelClient` exists to put
      behind it. `SkeinAgent::new(factory)` already requires `C: ModelClient + Send + 'static` and
      `OpenAiCompatClient` satisfies it; what is missing is the transport, the `tokio` runtime in
      `skein-cli`, a `SessionParts` factory and a subprocess end-to-end test.
- [ ] **redaction on the `LlmRequest`/`LlmResponse` path**, so `skein ledger show` cannot print a
      conversation secret. Carried from slice 011 and now *reachable*, because a real conversation
      exists: `ToolCall`/`ToolResult` payloads pass through the `Redactor` in
      `ToolGateway::call_captured`, but `NativeLoop::run` appends model I/O **raw**. The fix belongs
      to the governed loop.
- [ ] **raw-wire-byte capture** — a `StepKind` for the provider's literal request and response
      bytes, which is what design §4.5's "exact model I/O" and Spike 1's criterion C1 actually ask
      for. Today the chain holds the translated `TurnRequest`/`TurnResponse`.
- [ ] **provider authentication**, when a local gateway needs one: an `Authorization: Bearer` whose
      value arrives as a `SecretRef` resolved through `SecretProvider` (Principle VI), never a
      literal and never a plaintext config value. About five lines, deliberately not written yet.
- [ ] **tool advertisement** — a `tools` field on `TurnRequest`, which needs tool *discovery* from
      the Tool Gateway first. Until it lands, a `skein chat` run produces no tool calls and the
      loop's tool mediation stays proven by the `ScriptedModel` suites.
- [ ] **streaming (SSE)**, together with ACP `AgentMessageChunk` notifications.
- [ ] **sampling parameters** — temperature, top-p, seed. `TurnRequest` cannot express them and no
      caller needs them yet.
- [ ] a config file holding the base URL, the model and a default silo root, so `--base-url`/
      `--model`/`--root` are not the only way to name them — the same config `Redactor::resolve` has
      been waiting for since slice 010.
- [ ] the egress-policy layer and ADR-0002 D4's **process-level socket-deny boundary**, which is
      what would close `LocalEndpoint`'s `localhost` re-resolution residual and what LAN-reachable
      providers need first.
