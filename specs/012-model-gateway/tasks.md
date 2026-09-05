# Tasks: `heddle-gateway` — the first real `ModelClient`, and `heddle chat` (v0 slice)

**Spec:** `specs/012-model-gateway/spec.md` · TDD (red→green), product code in
`crates/heddle-gateway` and `crates/heddle-cli` plus one additive error variant in
`crates/heddle-core`, branch `012-model-gateway` cut from `dev` after slice 011 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the gateway is a library behind `ModelClient`, and `heddle chat` is its complete
  client — the first command that runs the governed loop. The one place the CLI needed something the
  API lacked (a name for "the engine stopped this run") is closed by *adding to `HeddleError`*, not
  by reaching around it · II Local-first ✅ **two independent locks**: `LocalEndpoint::parse` refuses
  anything but loopback before a socket exists, and `ureq` with no TLS feature cannot speak HTTPS at
  all (measured: `TlsRequired`). Foreign host names are refused **without** being resolved, so no
  name leaves the machine in a DNS query
- III Test-First ✅ (T1 pins the `ureq` surface against the vendored source **and** a compiled probe
  before any product code; T3's red observed before T4, T10's before T11, T12's before its variant.
  T5/T6/T7's guards are preconditions of T4's tests compiling, so their reds were observed by
  **mutation** instead — see `## Observed red`, which records that deviation and why a surviving
  mutant is a stronger check than an absent name) · IV Inverted coupling ✅ (`heddle-gateway` is the only crate naming
  HTTP or the OpenAI wire format; `heddle-core` does not depend on it and its dependency list is
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
- [x] **T3** RED — `crates/heddle-gateway/tests/openai_compat.rs` with the `std::net::TcpListener`
      stub helper, against the not-yet-existing `heddle_gateway::{LocalEndpoint, OpenAiCompatClient}`
- [x] **T4** GREEN — `crates/heddle-gateway/` (`Cargo.toml`, `src/lib.rs`) + two root
      `[workspace.dependencies]` lines
- [x] **T5** the loopback allowlist (`LocalEndpoint::parse`), red observed by mutation
- [x] **T6** token accounting (D8's three cases), red observed by mutation
- [x] **T7** provider failure modes, `finish_reason: "length"`, `tool_calls`; red by mutation
- [x] **T8** end to end through `NativeLoop` against a two-response stub, still with no Ollama; no
      product code, so non-vacuity checked by mutation
- [x] **T9** the optional `#[ignore]`d live test against a real Ollama
- [x] **T10** RED — `crates/heddle-cli/tests/cli_chat.rs`, process tests of the real binary
- [x] **T11** GREEN — `crates/heddle-cli/src/chat.rs`, the `Chat` arm, and the `main.rs` docstring
      correction
- [x] **T12** RED→GREEN — `HeddleError::Unfinished` with its own test; landed **before** T11, which
      cannot compile without it
- [x] **T13** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`, `cargo build --workspace`, + run `heddle chat --help` and
      `heddle chat` against a real local Ollama
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

**Eight behaviours were measured in the probe, not assumed.**

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

All on 2026-09-03. **A deviation from the plan's step order is recorded first, because it changes
what "red" could mean for three of these steps.**

The plan's T4 puts `LocalEndpoint`, `OpenAiCompatClient`, the request builder *and* the response
parser in `src/lib.rs`, and its T5/T6/T7 then ask for a fresh red on the loopback allowlist, the
token accounting and the failure mapping. For a single-file crate those two instructions conflict:
`OpenAiCompatClient::new` takes a `LocalEndpoint` **by value** and has no other constructor, and
`turn` must return a `tokens_used`, so all three behaviours are preconditions of T3's two tests
compiling at all. Writing T5–T7's tests after T4 therefore produced tests that **passed on first
run**, and slice 011's own tasks.md states the standard: *"A test that cannot fail proves
nothing."*

Rather than dress that up, each of T5/T6/T7 observed its red by **mutating the guard under test**
into its natural wrong implementation and running the new tests against it. That is a strictly
stronger observation than an absent function: an unresolved-import red proves only that a name is
missing, whereas a surviving mutant would prove the test does not depend on the behaviour it
claims to pin. In every case the product code was then restored byte for byte —
`git diff --stat` on `src/lib.rs` against the preceding commit is **empty** for all three, so
those commits are tests only, which is checkable from the history.

- **T3** `cargo test -p heddle-gateway --test openai_compat` against a `Cargo.toml` + empty
  `src/lib.rs` skeleton — the genuine absent-name red, the same shape as slices 007–010:
  - `error[E0432]: unresolved imports heddle_gateway::LocalEndpoint,
    heddle_gateway::OpenAiCompatClient` — *no `LocalEndpoint` in the root*, *no
    `OpenAiCompatClient` in the root*
  - `error: could not compile heddle-gateway (test "openai_compat") due to 1 previous error`
- **T5** mutation: the scheme and host checks deleted from `LocalEndpoint::parse`, the URL parse
  left in place. **3 passed, 2 failed.**
  - `an_https_base_url_is_refused` — `https://api.openai.com/v1 was accepted as
    "https://api.openai.com/v1"`
  - `a_non_loopback_base_url_is_refused` — `http://ollama.example.com/v1 was accepted as
    "http://ollama.example.com/v1"`
  - `loopback_base_urls_are_accepted` still passed, which is the control: the mutant is
    permissive, not broken.
- **T6** mutation: `metered()` replaced by the exact zero fallback D8 rejects
  (`usage.and_then(|u| u.total_tokens).unwrap_or(0)`). **6 passed, 2 failed.**
  - `tokens_used_falls_back_to_prompt_plus_completion_when_total_is_absent` — `left: 0,
    right: 42`
  - `a_response_without_usage_is_refused_rather_than_metered_as_zero` — `expected a refusal, got
    TurnResponse { message: … "unmetered" …, tokens_used: 0, final_output: true, tool_calls: [] }`
  - `tokens_used: 0` **with** `final_output: true` is precisely the failure D8 exists to prevent:
    `LoopController::should_exit` stops on `tokens >= max_tokens`, and a zero never trips it, so
    the budget would be silently disabled while looking like it worked.
- **T7** mutation: three guards at once — believe any `finish_reason`
  (`choice.finish_reason.is_some()`), drop the model's tool intent, and trust every status.
  **11 passed, 3 failed.**
  - `finish_reason_length_is_not_a_final_answer` — `a truncated answer is not a final answer`
  - `tool_calls_are_translated_and_are_not_a_final_answer` — `left: []`, `right: [ToolCall { tool:
    "read_file", args: Object {"path": String("README.md")} }]`
  - `a_provider_error_status_carries_the_providers_own_message` — `got: http://127.0.0.1:50521/v1
    returned an unrecognised chat-completions response: no choices[0]`
  - **That third failure is worth reading twice, and it justifies the ordering inside `turn`.**
    `ChatResponse::choices` is `#[serde(default)]`, so an error body like
    `{"error":{"message":…}}` *parses successfully* as a response with no choices. Without the
    status check running first, a 404 therefore reaches the operator as "unrecognised
    chat-completions response" instead of as the provider's own `model "nope" not found`. Two
    individually reasonable decisions — lenient deserialization, and checking the status before
    parsing — compose into a diagnosability property that only the ordering preserves. No
    assertion count would have surfaced that; the mutation did.
- **T8** introduced **no product code** — it is a control on the composition of units that are
  already green — so it had no red available at all. Its non-vacuity was checked the same way, by
  sending only the latest message instead of the accumulated history:
  - `an_end_to_end_run_against_a_stub_provider_lands_on_the_chain` — `left: [{assistant:
    "thinking out lou"}]`, `right: [{user: "what is the answer?"}, {assistant: "thinking out
    lou"}]`
- **T10** `cargo test -p heddle-cli --test cli_chat` before `heddle chat` existed: **6 failed,
  0 passed** — the same shape as slice 011's T5, a red on output and exit codes rather than on a
  compile error, because clap rejects an unknown subcommand at runtime.
  - `chat_answers_from_a_local_provider_and_records_the_run` — `left: ""`, `right: "the answer is
    42\n"`
  - `chat_fails_loudly_when_no_provider_is_listening` — `left: Some(2), right: Some(1)`: clap
    exits **2** on `unrecognized subcommand 'chat'` where the command's own contract is 1.
- **T12** `cargo test -p heddle-core --test core`:
  - `error[E0599]: no variant named Unfinished found for enum HeddleError`
  - T12 consequently landed **before** T11 rather than after it as the plan ordered:
    `chat.rs` cannot compile without the variant, so "added when T10's budget-exit test demands
    it" and "after T11" cannot both hold. The plan's own wording — *"added when T10's budget-exit
    test demands it and not before"* — is the instruction that was followed.

## Gate run (T13)

2026-09-03, Windows leg observed locally; macOS and Linux legs unobserved until the repository has
a remote (SC-001).

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no lint raised on the new crate
  or on `heddle-cli`'s new module.
- `cargo test --workspace` — **105 passing, 1 ignored, 0 failed**: 82 pre-existing + 23 new. Per
  binary: `acp_session` 13, `cli_chat` **6**, `cli_ledger` 8, `cli_secret` 2, `core` **14**,
  `native_loop` 18, `tool_gateway` 9, `governed_run` **2**, `openai_compat` **14 + 1 ignored**,
  `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5. The new crate's `src/lib.rs` unit-test target
  and its doc-test target each contribute `0 passed`, so neither inflates the count.
- **105, not the plan's projected ≈99.** The plan enumerated 17 new tests; 23 landed. The six extra
  are named and justified rather than glossed:
  1. `loopback_base_urls_are_accepted` — the plan's D5 table has three **accept** rows and T5 says
     "add D5's table as tests"; the plan's numbered list only covered the refusals. Without it the
     allowlist is pinned in one direction, and a guard that refuses everything would pass.
     It also covers the trailing-slash normalisation the spec's Assumptions promise.
  2. `an_unrecognised_response_body_is_refused` — D9's **third** row (unparseable body / missing
     `choices[0]`). T7 says "D9's four mappings"; the plan's numbered list had only three of them.
  3. `a_provider_failure_ends_the_run_with_the_request_already_on_the_chain` — `NativeLoop`'s own
     comment claims the request is captured *before* the call so a failing client still leaves the
     exact request in the chain. That claim had never been exercised against a client that can
     really fail, and now can be.
  4. `chat_reads_the_prompt_from_stdin_when_no_flag_is_given` — the prompt's second source. The
     plan specifies it as behaviour (D10) and lists no test for it.
  5. `the_base_url_falls_back_to_the_environment_and_the_local_default` — FR-015's precedence.
     Exactly slice 011's own deviation, for the same reason: shipping a documented resolution rule
     with no test, where the failure mode (silently defaulting somewhere) is invisible and a
     process test costs four lines, is the worse of the two deviations.
  6. `an_unfinished_run_names_the_run_and_the_exit_that_stopped_it` — the `HeddleError::Unfinished`
     message, which the plan's validation list anticipated conditionally ("if T12 lands").

  None is one of the three shapes the plan excludes as padding: there is no test of `ureq` itself,
  no test of `--help` text, and no unit test of an inner formatter standing in for a behaviour
  test.
- `cargo build --workspace` — clean; `target/debug/heddle.exe` exists (8.9 MB) and runs.
  `heddle chat --help` prints `Usage: heddle chat [OPTIONS] --silo <ID> --model <NAME>` with all ten
  options — **`heddle chat`, not `heddle.exe chat`**, the `bin_name` property slice 011's T1 pinned,
  still holding with a new subcommand.
- **SC-007, the structural TLS claim, measured rather than asserted.**
  `cargo tree -e normal,build,dev --prefix none` over the whole workspace matches **zero**
  occurrences of `rustls`, `native-tls`, `webpki`, `openssl` or `ring`. There is no TLS
  implementation in this binary to reach a cloud provider with.

### The headline claim, checked by running it

`heddle chat` against the **real** Ollama on this host (`lfm2.5:latest`, one of three installed
models), then read back with slice 011's commands — two binaries, two slices, one chain:

```
$ heddle chat --root <tmp> --silo live --model lfm2.5:latest \
    --prompt "In one short sentence: what is a hash chain?"
EXITCODE=0
stdout: A hash chain is a sequence of data blocks where each block's input includes the hash of
        the previous block, forming a linked cryptographic proof.
stderr: run chat-1788422518058-68972

$ heddle ledger log --root <tmp> --silo live
chat-1788422518058-68972	0	iteration_boundary	a9a57189f94fae27463a7de818267d5cd89fadcd23995e453e959d7a22886dfc
chat-1788422518058-68972	1	llm_request	07954f4aa51d870c5dece3dafd7b971c14cb6186c11cc0af2dd5b6d2ba634ca8
chat-1788422518058-68972	2	llm_response	820ae542b8ee7d6ca18ffad1cb74aa65770d8e009b6d529efdd30889d6895d1f
chat-1788422518058-68972	3	budget_spent	388fe0bf2a3850c816977432938717514d2e3187a9afebd439316733cee86bfc
chat-1788422518058-68972	4	exit	1f594a9c3a6f63983c8688e634950191290c999c30a236bb268f0f9326677748

$ heddle ledger verify --root <tmp> --silo live
chat-1788422518058-68972	ok	5 steps

$ heddle ledger show --root <tmp> --silo live 388fe0bf…86bfc
kind	budget_spent
payload
202
```

**202 is Ollama's own `usage.total_tokens`**, not a constant and not a zero — so the risk the plan
flagged as the one thing a mocked test cannot prove is settled: this provider does send metering,
and D8's loud refusal is a guard against a provider that does not, never a path the normal case
takes.

The T9 live test was also run by hand and recorded at its commit:

```
live lfm2.5:latest @ http://localhost:11434/v1
  content      = "pong"
  tokens_used  = 103
  final_output = true
```

and, with the variable unset, `HEDDLE_LIVE_MODEL is unset; skipping the live provider test` — so the
`#[ignore]` gate degrades to a skip rather than a failure on a machine with no model.

### The Principle II refusals, also run by hand

Three invocations of the shipped binary, all exit **1**:

```
$ heddle chat … --base-url https://api.openai.com/v1
error: model provider: base URL "https://api.openai.com/v1" is not a local provider: scheme
"https" is refused; Heddle v0 talks to local providers over http only, and no TLS backend is
compiled in

$ heddle chat … --base-url http://192.168.1.10:11434/v1
error: model provider: base URL "http://192.168.1.10:11434/v1" is not a local provider:
192.168.1.10 is not a loopback address; reaching a provider off this machine needs the egress
policy layer, which does not exist yet

$ heddle chat … --base-url http://ollama.example.com/v1
error: model provider: base URL "http://ollama.example.com/v1" is not a local provider: host name
"ollama.example.com" is refused without being resolved, because the query would itself leave this
machine; use a loopback address or localhost
```

All three named the silo `probe`, and afterwards **`<root>/probe/` does not exist**: the endpoint
is parsed before `Silo::open`, so a refused base URL leaves no directory and no chain. That is
stronger than the spec's US3.4 wording ("the ledger holds no run") and is what the automated test
asserts.

## Control diff (T14)

`git diff dev --stat -- crates/heddle-mcp/ crates/heddle-acp/ crates/heddle-silo/ spikes/ .github/
rust-toolchain.toml` is **empty** (SC-005), `spikes/` included per ADR-0004 D2 — so specs 005, 008,
009 and 010's suites, 32 of the 82 baseline tests, are live controls run against this slice's
`heddle-core`.

`git diff dev --stat -- crates/heddle-core/` is `src/error.rs | 6 +` and `tests/core.rs | 18 +` —
**one added error variant and one added test, 24 insertions and 0 deletions** (SC-006). No existing
variant, signature or test body changed, so all 82 baseline tests stayed live controls on the core
addition.

`git diff dev -- Cargo.toml` is **exactly two added `[workspace.dependencies]` lines**, `ureq` and
`http`. `members = ["crates/*"]` already covered the new crate and `core.yml`'s `paths:` already
covered `crates/**` and `Cargo.toml` — both confirmed by reading, neither edited.

`git diff dev --stat` over the branch is **2724 insertions and 12 deletions** across 15 files.
Unlike slice 011, this slice **does** delete, and all twelve lines are accounted for:

- `crates/heddle-cli/src/main.rs` — the two-line docstring claiming *"v0 has no `chat` and no
  `acp-agent` because the workspace has no real `ModelClient` to put behind them"*. That sentence
  is now false, so it is rewritten rather than left; plus the `use heddle_core::Result` line, now
  importing `HeddleError` too, and one doc comment reworded from "a ledger command reads" to "a
  command reads or writes", which `SiloArgs` now also is.
- `crates/heddle-cli/src/ledger.rs` — the seven-line `--root`/`$HEDDLE_ROOT` resolution, lifted out
  of the private `open_ledger` and onto `SiloArgs::root()` where `chat` and the three `ledger`
  commands share one implementation. `chat` needs the identical rule and a second copy would be
  the drift `kind_name`'s own docstring warns about; the eight `cli_ledger` tests are the
  regression control on the move, and stayed green with their bodies unchanged.

## Drift (T14)

Measured against a detached worktree at the branch point (`188540a`), so both sides come from a
real resolution rather than from the previous slice's note.

As in slices 010 and 011, a handful of package versions differ between the two trees purely as
resolution noise, because `Cargo.lock` is `.gitignore`d in this repository: the freshly resolved
**base** worktree picked up `serde`/`serde_core`/`serde_derive` 1.0.229, `serde_json` 1.0.151,
`proc-macro2` 1.0.107, `quote` 1.0.47 and (on Linux/macOS) `libc` 0.2.189, one or two patches ahead
of the working tree's cached lock. Those six-to-seven are excluded below.

- **`ureq` adds exactly seven external crates, and the same seven on every target** — `ureq 3.4.0`,
  `ureq-proto 0.6.1`, `http 1.5.0`, `httparse 1.10.1`, `percent-encoding 2.3.2`, `utf8-zero 0.8.1`,
  `base64 0.23.1` — plus the new `heddle-gateway` workspace member itself.
  `cargo tree -e normal,build,dev [--target …] --prefix none | sort -u | wc -l`:

  | Target | before | after | added |
  |---|---|---|---|
  | `x86_64-pc-windows-msvc` (host) | 141 | 149 | the seven above + `heddle-gateway` |
  | `x86_64-unknown-linux-gnu` | 140 | 148 | the seven above + `heddle-gateway` |
  | `aarch64-apple-darwin` | 142 | 150 | the seven above + `heddle-gateway` |

  Nothing was removed on any target. There is no `#[cfg]` in either new file and no target-gated
  dependency, which is why the added set is identical on all three legs.
- **T1's prediction of seven held exactly.** `bytes 1.12.1`, `log 0.4.34` and `itoa 1.0.18` appear
  in `ureq`'s standalone probe graph but are already in the workspace, so they cost nothing — the
  same lesson slice 011 recorded, that a standalone probe overstates cost because it cannot see
  what the workspace already resolves. The direct `http` edge in `heddle-gateway/Cargo.toml` adds a
  *name*, not a package: `ureq-proto` already pulls `http 1.5.0`.
- **`base64 0.23.1` joins the existing `base64 0.22.1`**, confirmed present in the same graph. A
  second major is not unprecedented here: slice 011 measured `syn 2.0.119` and `syn 3.0.4` already
  coexisting on `dev`.
- **No toolchain change.** The highest MSRV among the added crates is **1.85** (`ureq`,
  `ureq-proto`; `base64` declares 1.71.0, `http` 1.57.0, `percent-encoding` 1.51, and `httparse`
  and `utf8-zero` declare none), all below the 1.97 pin. So `rust-toolchain.toml` and
  `workspace.package.rust-version` are unchanged and `.github/workflows/core.yml` needs no edit.
  Every added crate is `MIT OR Apache-2.0`.
- **No new build prerequisite.** Six of the seven have `build = false`. `httparse` has a
  `build.rs`, and it was read: it shells out to `rustc --version` to enable SIMD paths above its
  MSRV and does nothing else. No crate ships a `.c` file, so there is no C amalgamation and no
  `pkg-config`. `docs/DEVELOPMENT.md`'s "Machine prerequisites" is unchanged by this slice; the
  one edit T14 makes to that file corrects the stale "Local inference" row.
- **`heddle-gateway` has five direct dependencies** — `heddle-core`, `serde`, `serde_json`, `ureq`,
  `http` — and **no dev-dependencies at all**: the wire tests are `std` sockets, so the slice adds
  no test harness. **`heddle-cli` gains exactly one**, `heddle-gateway`, bringing it to five. Neither
  crate takes `tokio`: every path in this slice is synchronous, which is the whole reason `ureq`
  was chosen over `reqwest`.

## Out of scope

Deliberately not done, so no one helpfully does it:

- **`heddle acp-agent`** — the next slice. Needs a stdio `ConnectTo<Agent>`, a `tokio` runtime inside
  `heddle-cli`, a `SessionParts` factory and a subprocess ACP end-to-end test.
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
- [ ] **`heddle acp-agent`** — the stdio ACP server, now that a real `ModelClient` exists to put
      behind it. `HeddleAgent::new(factory)` already requires `C: ModelClient + Send + 'static` and
      `OpenAiCompatClient` satisfies it; what is missing is the transport, the `tokio` runtime in
      `heddle-cli`, a `SessionParts` factory and a subprocess end-to-end test.
- [ ] **redaction on the `LlmRequest`/`LlmResponse` path**, so `heddle ledger show` cannot print a
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
      the Tool Gateway first. Until it lands, a `heddle chat` run produces no tool calls and the
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
