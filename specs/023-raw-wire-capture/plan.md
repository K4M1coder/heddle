# Plan — slice 023: raw wire-byte capture on the Ledger

**Target artifacts:** `specs/023-raw-wire-capture/{spec.md,plan.md,tasks.md}` plus the code changes
below. **Branch:** `023-raw-wire-capture`, cut from `dev`. **No PR.** Conventional Commits. Strict
TDD (Constitution III).

---

## 0. Read this first: what the tree actually is, versus what the request assumes

Everything below was verified this session against **`dev` at `12c14f5`** (`git show dev:<path>`),
not against the worktree checkout. Four corrections the implementer must not inherit blind:

| Claim in the request | What the tree says |
|---|---|
| "read `crates/heddle-gateway/src/lib.rs` and `crates/heddle-core/src/…` [in the worktree]" | **The worktree `023-raw-wire-capture` is checked out at `d364405`, which is behind `dev` (`12c14f5`).** Slices 021 and 022 are on `dev` and *absent* from this checkout — `specs/` here stops at `020`, and `ChatMessage` here has no `tool_calls`/`tool_call_id`. **First action of the implementation run: rebase/fast-forward the branch onto `dev` at `12c14f5` and re-measure the control baseline**, exactly as slice 022's T0 did. Every anchor in this plan is a `dev` anchor. |
| "per AGENTS.md's data-and-compatibility rules" | **There is no `AGENTS.md` anywhere in this repository**, tracked or untracked (checked `git ls-tree -r dev` and `find`). The governing document is `.specify/memory/constitution.md` (Principle V) plus the house practice visible in slices 012/015/022: every new payload field is `#[serde(default)]` + `skip_serializing_if`, and old chains must still deserialize. The additive-only rule is real; its cited source is not. This plan follows the rule and cites the real source. |
| "does ureq expose the exact bytes sent, or must heddle-gateway serialize once for capture and again for the actual send… decide and justify" | **The dilemma does not arise.** `turn` already serializes exactly once into `let body: String`, passes `&body` to `post`, which calls `.send(body)`. ureq 3.4.0's `impl AsSendBody for &str` (`send_body.rs:300-306`) wraps the slice as `BodyInner::ByteSlice` with `Some(self.len())` as content-length — no copy, no transformation, no re-encode. Capturing `body` *is* capturing the transmitted buffer. See D2. |
| "the existing Redactor must cover this new StepKind's payload exactly as it covers LlmRequest/LlmResponse" | **"Exactly as" is insufficient and would leak.** `Redactor::redact_json` serializes *then* scrubs, specifically because a secret containing `"`/`\`/newline is JSON-escaped during serialization. A raw wire body arrives **already escaped**, so the literal needle is absent from it. See D4 — this needs one new `Redactor` method and a test with a quote-bearing secret. |

Two further things verified rather than assumed:

- **`specs/012-model-gateway/tasks.md` really does say what the request quotes.** Confirmed verbatim
  in both *Out of scope* and *Next slice*. Repeated as a residual in `specs/022-…/spec.md`
  (*Assumptions and residuals*, *Out of scope*, and the rejected-alternatives register, where a
  raw-wire `StepKind` is explicitly deferred to "the standing separate item"). The request's history
  is accurate.
- **Spike 1's C1 is narrower than the residual implies**, and this matters for scoping.
  `docs/superpowers/spikes/runtime-loop-evidence.md` records C1 as PASS for the *response*
  ("captured byte-exact") and, after adversarial review, softened for the *request* to "exact
  pre-serialization payload… **Headers (auth) are not captured**; one `received_requests()` byte
  assertion would close the request-side gap." So the gap Spike 1 itself named is the **request
  body**, and headers were already out of scope there. This plan closes the body gap and records
  headers as a named residual (D6).

---

## 1. Problem

`crates/heddle-core/src/native_loop.rs`'s `NativeLoop::run` appends `StepKind::LlmRequest` from
`&req: &TurnRequest` and `StepKind::LlmResponse` from `&resp: &TurnResponse`. Both are **heddle-core's
own types** — the translated, structured representation on this side of the `ModelClient` port. The
bytes that actually crossed the socket are built and consumed entirely inside
`crates/heddle-gateway/src/lib.rs`'s `impl ModelClient for OpenAiCompatClient::turn`, and are dropped
when the function returns.

The consequence is precisely the one Constitution V exists to prevent: **every existing test and
every operator inspecting a chain reads a record that cannot disagree with itself.** A bug in
`ChatRequest`'s `Serialize` derive, a provider that answers a shape `ChatResponse` silently ignores,
a mistranslation in `impl From<&Message> for ChatMessage` — none of them is visible anywhere in the
chain, because the chain records the input to the translation and the output of it, never the wire
between them. `crates/heddle-gateway/tests/governed_run.rs` states this in a comment on a live
assertion:

> *"The chain holds the **translated** TurnRequest/TurnResponse, not the provider's raw wire bytes —
> the gap spec 012 states plainly and defers."*

Design §4.5 names the Gateway the "traceability chokepoint" that "captures model inputs/outputs to
the Ledger (§4.11)"; §4.11 names "exact model I/O" as the Ledger's first capture obligation. Spike 1
made byte-exact capture the deciding criterion (C1) for owning the loop at all. This slice makes the
claim true.

---

## 2. Approach

Add one `StepKind` variant, one payload struct, one **defaulted** `ModelClient` trait method, one
`Redactor` method, and four fields of storage on `OpenAiCompatClient`. Nothing else.

### D1 — one `StepKind::WireExchange` step per exchange, not two steps

`crates/heddle-core/src/ledger.rs`'s `enum StepKind` gains a single variant `WireExchange`
(serde name `"wire_exchange"`, from the existing `#[serde(rename_all = "snake_case")]`).

Payload: a new `WireExchange` struct in `crates/heddle-core/src/model.rs`, beside `TurnRequest` /
`TurnResponse`, re-exported from `lib.rs`:

```rust
pub struct WireExchange {
    pub url: String,      // which endpoint answered
    pub status: u16,      // the one wire fact in neither body
    pub request: String,  // the literal request-body bytes
    pub response: String, // the literal response-body bytes
}
```

`Serialize + Deserialize + Debug + Clone + PartialEq + Eq`, mirroring `TurnRequest`.

**Why one step and not a `WireRequest`/`WireResponse` pair.** The request bytes only become
*available to the core* after `turn` returns, so a two-step shape would append a "request" step
strictly *after* its own response had already happened — a chain whose ordering lies. One step is
atomic: it exists exactly when a full exchange completed, and the pairing needs no correlation key.
`ToolCall`/`ToolResult` are a pair because they genuinely bracket an unbounded interval during which
other steps (`Approval`) land; a single HTTP round trip has no such interior.

**Why this is additive-safe, verified rather than assumed:**

- `ledger.rs`'s `fn hash` feeds `serde_json::to_string(kind)` — the variant's own name. Adding a
  variant does not change any existing variant's name, so no existing step's id moves and
  `verify_chain` on a pre-023 chain is bit-for-bit unaffected.
- `crates/heddle-silo/src/ledger_store.rs`'s `SCHEMA` stores `kind TEXT NOT NULL` with **no CHECK
  constraint and no enum table**; `load` does `serde_json::from_str::<StepKind>(&kind)`. New names
  round-trip; old names are untouched.
- `crates/heddle-cli/src/ledger.rs`'s `kind_name` derives the column from `serde_json::to_value(kind)`
  rather than matching the enum, so `heddle ledger log|show` renders the new kind with **no CLI
  change at all**.
- `crates/heddle-acp/src/lib.rs`'s `project_updates` matches four kinds and ends `_ => {}`, so the new
  kind is inert on the ACP transcript — no `SessionUpdate` is emitted for it. That is correct and
  deliberate: raw provider bytes are audit evidence, not something to stream to an editor.

**Placement in the chain:** appended between `LlmRequest` and `LlmResponse`. Per turn the chain
becomes `IterationBoundary, LlmRequest, WireExchange, LlmResponse, BudgetSpent`. That reads in
causal order — what we meant to send, what actually crossed, what we made of it — and each step
still lands before anything downstream can act on it.

### D2 — the gateway hands back the *same buffer* it sent; there is no re-serialization

`OpenAiCompatClient` gains a private `last_exchange: Option<WireExchange>`. In `turn`:

1. Clear it first (`self.last_exchange = None`), so a transport failure cannot leave a stale
   exchange from an earlier turn to be re-appended.
2. Build `body` exactly as today.
3. Call `self.post(&body)?` exactly as today.
4. On `Ok((status, text))`, move `body` and `text` **into** the `WireExchange` — `request: body`,
   `response: text`, no re-serialization and no second `to_string`.
5. Parse from `&self.last_exchange.as_ref().expect("just set").response`. Every remaining `self`
   method used in `turn` (`metered`, `unrecognised`, `endpoint.base_url`) takes `&self`, so this
   borrows cleanly. If borrowck fights, cloning `text` for the exchange is the accepted fallback —
   it is a copy of one buffer, not a second derivation of it, so the identity invariant holds either
   way.

**This is what discharges the request's first invariant.** The captured request bytes are not "a
re-serialization that could silently diverge": they are the identical `String` whose `&str` was
handed to `ureq::RequestBuilder::send`, which (verified in `ureq-3.4.0/src/send_body.rs`) transmits
`BodyInner::ByteSlice(self.as_ref())` with `content-length = self.len()` and no transformation.
Divergence is not merely unlikely — there is only one buffer, so it is unrepresentable.

**Honest limits, both pre-existing and unchanged by this slice** (state these in `spec.md` rather
than claiming more than is true):

- The **request** capture is byte-identical to the transmitted body. Headers and the request line are
  not captured (D6).
- The **response** capture is byte-identical to the string the parser consumed. That string is
  ureq's decoding of the response body: `Body::read_to_string` (`ureq-3.4.0/src/body/mod.rs:296`)
  applies `.limit(MAX_BODY_SIZE)` (10 MB) and `.lossy_utf8(true)`, so a non-UTF-8 byte becomes
  U+FFFD and a >10 MB body is cut. **Both properties already govern what `ChatResponse` is parsed
  from today**; this slice neither introduces nor worsens them, and the captured bytes are exactly
  the bytes heddle-core acted on — which is the auditable claim that matters. `gzip` is not a factor:
  `heddle-gateway`'s `Cargo.toml` declares `ureq = { workspace = true }` against a workspace entry of
  `default-features = false`, so no content-encoding is decoded.

### D3 — the port grows a **defaulted** `take_wire_exchange`, not a changed `turn` signature

`crates/heddle-core/src/model.rs`'s `trait ModelClient`:

```rust
pub trait ModelClient {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse>;

    /// The literal bytes of the exchange the last `turn` performed, if this
    /// client has a wire at all. Taken, not borrowed: a captured exchange
    /// belongs to exactly one turn, so a client that fails before reaching a
    /// socket cannot re-offer the previous turn's bytes.
    fn take_wire_exchange(&mut self) -> Option<WireExchange> { None }
}
```

In `NativeLoop::run`, replacing the single `let resp = self.client.turn(&req)?;`:

```rust
let resp = self.client.turn(&req);
if let Some(exchange) = self.client.take_wire_exchange() {
    ledger.append(run_id, StepKind::WireExchange, /* redacted, see D4 */)?;
}
let resp = resp?;
ledger.append(run_id, StepKind::LlmResponse, self.redactor.redact_json(&resp)?)?;
```

Three properties fall out of this shape, each worth an FR:

- **A failed turn still records the wire.** A provider that answers HTTP 500, or answers a body
  `ChatResponse` cannot parse, currently produces `Err` and leaves nothing but the translated
  `LlmRequest`. Now the exchange that caused the failure is on the chain. This is the single most
  valuable case the slice buys and it must not be lost by writing `turn(&req)?` on one line.
- **A turn that never reached a socket records nothing.** `post` returning `Err` (connection
  refused, timeout) leaves `last_exchange` at `None`, so no `WireExchange` step is appended. The
  chain never claims bytes crossed when none did.
- **Every existing `ModelClient` implementation compiles and behaves identically.** The default body
  returns `None`. `ScriptedModel` (`heddle-core/tests/native_loop.rs`), and every stub across
  `heddle-connectors`, `heddle-mcp`, `heddle-silo` and `heddle-acp` tests, is a client with **no wire** —
  `None` is not a convenience default, it is the true answer for those types. Consequently *no*
  `StepKind` sequence asserted by a stub-model test changes.

**Rejected: widening `turn` to `turn(&mut self, req, sink: &mut dyn WireSink)`.** It rewrites every
implementation and every call site to thread a parameter that all but one implementation ignores,
and it puts a capture concern into the signature of the port's only real method (Constitution VII).

**Rejected: an observer closure installed on `OpenAiCompatClient`.** The closure would need
`&mut Ledger`, which `NativeLoop::run` borrows for the whole call; the workaround is a shared
`RefCell` buffer that the loop drains afterwards — i.e. exactly `take_wire_exchange`, with interior
mutability and a runtime borrow panic added. Strictly worse.

**Rejected: carrying the bytes on `TurnResponse`.** `TurnResponse` is itself serialized into the
`LlmResponse` payload, so the raw bytes would be duplicated inside it (or need
`skip_serializing_if`, which is a lie about the type); and an `Err` return carries no `TurnResponse`
at all, losing the failure case that is the point.

### D4 — redaction: `Redactor::redact_wire`, because `redact` alone leaks here

`redact_json`'s own doc states the rule it was built on:

> *"Serialize **then** scrub, never the other way round: a secret containing a quote, a backslash or
> a newline is JSON-escaped inside a serialized payload, so the literal needle would not appear in
> it and the secret would be missed entirely."*

A raw wire body inverts the premise: it arrives at the `Redactor` **already serialized and already
escaped**. A configured secret `pa"ss` present in a user prompt is transmitted as `pa\"ss` inside the
request body, so `redact`'s `str::replace(needle, "***")` finds nothing and the secret lands on the
chain in cleartext. Reusing `redact_json` does not help — `to_value` of a `String` field yields that
same escaped text as one `Value::String`, and the scrub still misses.

Add to `crates/heddle-core/src/tool.rs`'s `impl Redactor`:

```rust
/// Scrubs a payload that is **already serialized JSON text**, where a secret
/// containing a quote, backslash or newline appears in its escaped form rather
/// than literally. Each secret is matched twice: as written, and as
/// `serde_json` would have escaped it.
pub fn redact_wire(&self, text: &str) -> String { … }
```

Implementation: for each secret, replace the literal `secret.expose()`; then derive the escaped
needle as `serde_json::Value::String(secret.expose().into()).to_string()` minus its surrounding
quotes (infallible — a `Value::String` always serializes) and replace that too when it differs from
the literal. Order literal-first; the two cannot alias.

Then in `NativeLoop::run`, mirroring `ToolGateway::call_captured`'s field-by-field shape rather than
`redact_json`'s whole-value shape:

```rust
let scrubbed = WireExchange {
    url: self.redactor.redact(&exchange.url),
    status: exchange.status,
    request: self.redactor.redact_wire(&exchange.request),
    response: self.redactor.redact_wire(&exchange.response),
};
ledger.append(run_id, StepKind::WireExchange, serde_json::to_string(&scrubbed)?)?;
```

**Rejected: teaching `redact` itself about escaped forms.** It would change behaviour at three
existing call sites (`redact_call`'s tool name, `call_captured`'s outcome content, `mediate`'s
denial wording) for no need in this slice — Constitution VII. A separate method keeps the blast
radius at one new call site.

**Rejected: parsing the raw body and scrubbing it as a `Value`.** It would re-serialize the body,
destroying the exact property this slice exists to establish.

**Discovered, out of scope, recorded as a residual:** the same escape hole exists **today** on
`StepKind::ToolResult` whenever a tool's `content` is itself JSON text carrying a quote-bearing
secret. `redact_wire` is the fix, and pointing `call_captured` at it is a one-line change — but it
is a different payload with different tests, and slicing it here would widen this slice. Name it in
`spec.md`'s residuals and in `tasks.md`'s *Next slice*.

### D5 — capture is unconditional; there is no flag

No `--capture-wire`, no config key, no `#[cfg]`. Reasons, in order:

1. The gap is a *traceability* gap. A flag makes "no evidence" the default, which reinstates the
   defect under a different name. Constitution V says traceability "cannot be bypassed"; the
   existing `Redactor`-is-required argument in `NativeLoop::new`'s doc comment is the same argument
   for the same reason.
2. Constitution VII cuts *against* the flag here, not for it: a flag is a capability with no caller.
   Nothing in the tree wants to turn this off.
3. The cost is bounded and known: roughly one extra copy of the request and response JSON per turn,
   in the chain and in memory for the duration of `turn`. The chain already stores the translated
   forms of both. If chain size ever becomes a real constraint it is a retention concern for the
   silo (design §7's retention policy), not a per-run switch.
4. **Secrecy is not a reason to gate it**, because redaction is applied identically to both the
   translated and the raw payloads (D4). A run whose `LlmRequest` is safe to show has a
   `WireExchange` that is safe to show.

### D6 — bodies only; not headers, not the request line, not framing

`WireExchange` records the two entity bodies plus `url` and `status`. It does not record HTTP
headers.

This is scoped, not lazy. It is exactly where Spike 1's own adversarial correction drew the line
("Headers (auth) are not captured"), and it keeps the slice clear of the separately-named
**provider-authentication** residual, which the request explicitly forbids touching: the moment
headers are captured, an `Authorization: Bearer` becomes a chain payload and this slice inherits
that residual's entire Principle VI design. Today `post` sets exactly one header,
`content-type: application/json`, which is a constant and carries no information. Record
header capture as a residual, to be revisited *in the same slice that adds authentication*.

---

## 3. Steps

Ordered; each independently verifiable. Anchored to named items, never line numbers.

**T0 — branch and baseline.** Fast-forward `023-raw-wire-capture` onto `dev` at `12c14f5`
(the branch carries no commits of its own). Record the control baseline verbatim in `tasks.md`, in
slice 022's format: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `cargo test --workspace` pass/fail/ignored counts. Write
`specs/023-raw-wire-capture/{spec.md,plan.md,tasks.md}` in the slices 020–022 format, including the
`## Constitution Check (ADR-0004 D1 solo-v0 bar)` table with all eight principles plus
*Cross-platform*.

**T1 — RED, the headline claim.** In `crates/heddle-gateway/tests/governed_run.rs`, add a test beside
`an_end_to_end_run_against_a_stub_provider_lands_on_the_chain`. Drive the existing `StubProvider`
(the `std::net::TcpListener` stub already in that file, whose `read_request` already returns the raw
request text including the body). Then assert, for the run's first `StepKind::WireExchange` step:
its payload parses as `WireExchange`; `exchange.request` is **string-equal** to the body the stub
actually received (split the stub's raw capture at the blank line — note `StubProvider::request_body`
currently parses to a `Value`, so add a raw-body accessor beside it rather than reusing that one);
`exchange.response` is **string-equal** to the exact reply body the stub was scripted to write;
`exchange.status == 200`; `exchange.url` ends `/chat/completions`. Fails to compile — `WireExchange`
does not exist. That *is* the red, recorded as such.

**T2 — RED, redaction, with a quote-bearing secret.** In `crates/heddle-core/tests/native_loop.rs`,
beside `the_redacted_llm_payloads_are_still_parseable_turn_request_and_response`. This test needs a
real wire, so it belongs in `heddle-gateway/tests/governed_run.rs` instead — build a `NativeLoop`
with `Redactor::new(vec![SECRET.into()])` where `SECRET` **contains a `"`** (e.g. `pa"ss-w0rd`),
prompt with text embedding it, and assert: no payload of any kind in the run contains `SECRET`;
the `WireExchange` payload contains `***`; the payload still deserializes into `WireExchange`;
`verify_chain` passes; and — the control that proves the test is real — the stub's captured raw
request **does** contain the escaped secret, i.e. the model was sent the truth and only the record
was scrubbed. Add a sibling with a plain alphanumeric secret so both needle forms are covered.

**T3 — RED, backward compatibility from a shipped schema shape.** In
`crates/heddle-silo/tests/silo_ledger.rs` (which already owns the persist/reopen tests). Build a
chain containing **only pre-023 kinds** (`LlmRequest`, `LlmResponse`, `ToolCall`, `Exit`), persisted
through `SqliteLedgerStore`; drop it; reopen; assert `verify_chain` passes, `log` returns the same
kinds and payloads, and the step **ids are unchanged** from the ones recorded before reopening —
that last assertion is what proves the new variant did not perturb `hash`. Then append a
`StepKind::WireExchange` step onto that same run and assert the chain still verifies and the new
step's `parent` is the old chain's last id. This is the upgrade-shape test, not a fresh-chain test.

**T4 — RED, the two negative properties.**
(a) Extend `a_provider_failure_ends_the_run_with_the_request_already_on_the_chain`: its asserted
kinds stay `[IterationBoundary, LlmRequest]` — **no `WireExchange`**, because nothing crossed the
wire.
(b) A new test in `governed_run.rs`: a stub that answers `500` with a provider error body, or `200`
with a body `ChatResponse` cannot parse. `run` returns `Err(HeddleError::Model(_))`, and the chain
nevertheless holds a `WireExchange` whose `response` is that exact unparseable body. This is the
case with no other witness anywhere in the product.

**T5 — GREEN, `heddle-core`.** `ledger.rs`: the `WireExchange` variant. `model.rs`: the
`WireExchange` struct and the defaulted `ModelClient::take_wire_exchange`. `lib.rs`: re-export
`WireExchange` from the `model` line. `tool.rs`: `Redactor::redact_wire`. `native_loop.rs`: the
`turn` / take / append / `?` sequence in `NativeLoop::run` per D3 and D4, with a comment saying why
the `?` is deferred past the append.

**T6 — GREEN, `heddle-gateway`.** `OpenAiCompatClient` gains `last_exchange: Option<WireExchange>`
(initialised `None` in `new`); `turn` clears, captures and parses per D2; the `take_wire_exchange`
override returns `self.last_exchange.take()`.

**T7 — the existing assertions that genuinely change.** Only tests that drive the **real**
`OpenAiCompatClient` through `NativeLoop` are affected, because D3's default keeps every stub client
at `None`. Verified by grep, the set is exactly three files:

- `crates/heddle-gateway/tests/governed_run.rs` — the two-iteration `kinds` vector in
  `an_end_to_end_run_against_a_stub_provider_lands_on_the_chain` (`WireExchange` after each
  `LlmRequest`).
- `crates/heddle-cli/tests/cli_chat.rs` — **5** kind vectors containing `"llm_request"`, and **4**
  `"… ok\t<n> steps"` counts (currently 5, 9, 12, 12).
- `crates/heddle-cli/tests/cli_acp_agent.rs` — **10** kind vectors containing `"llm_request"`, and
  **5** step-count assertions (currently 5+5, 12, 11, 12, 11).

Counts are from `dev`; recount after rebase rather than trusting them. Every other test file that
names `StepKind` uses a stub model and must **not** change — if one does, D3's default was
mis-implemented.

**T8 — live verification against local Ollama.** Add one `#[ignore]`d test in
`crates/heddle-gateway/tests/openai_compat.rs`, following the house convention exactly
(`#[ignore = "needs a real local provider; set HEDDLE_LIVE_MODEL to run"]`, model read from
`HEDDLE_LIVE_MODEL`, doc comment showing the PowerShell invocation), asserting the captured
`request`/`response` parse as JSON and that `response` carries the provider's own `usage` object —
something no stub can vouch for. Then hand-verify per the acceptance criteria: run
`heddle chat --root … --silo … --base-url http://localhost:11434/v1 --model … --prompt …`, then
`heddle ledger log` to find the `wire_exchange` step and `heddle ledger show <id>` to read it, and
record verbatim in `tasks.md`'s `## Live verification (T8)` that (i) the captured `request.messages`
matches the `LlmRequest` step's `messages`, (ii) `response.usage.total_tokens` equals the
`BudgetSpent` payload, and (iii) `heddle ledger verify` reports `ok`. Cross-checking (i) and (ii)
against two *independently produced* steps is the practical substitute for a packet capture and is
strictly stronger than eyeballing the bytes alone.

**T9 — close-out.** Drop "raw-wire-byte capture" from the *Next slice* lists it has been carried in
since 012, and add the two residuals this slice creates or discovers: header/auth capture (D6, to be
taken with provider authentication) and the `ToolResult` JSON-escape hole (D4).

---

## 4. Validation

**Project gates, unchanged and all three must be green:**
`cargo fmt --all --check` · `cargo clippy --workspace --all-targets -- -D warnings` ·
`cargo test --workspace`. Baseline recorded at T0, close recorded at T9 with the delta explained.

**New tests, and the property each one proves:**

| Test | Home | Proves |
|---|---|---|
| exchange bytes equal the stub's observed bytes | `heddle-gateway/tests/governed_run.rs` | acceptance 2 — capture is byte-identical to a real socket exchange, both directions |
| quote-bearing secret is scrubbed from the exchange | `heddle-gateway/tests/governed_run.rs` | acceptance 3 + D4 — the escaped-needle hole is closed, not merely the literal one |
| plain secret is scrubbed from the exchange | `heddle-gateway/tests/governed_run.rs` | acceptance 3 — parity with `LlmRequest`/`LlmResponse` |
| pre-023 chain persisted, reopened, ids unchanged, then extended | `heddle-silo/tests/silo_ledger.rs` | acceptance 4 — upgrade shape, not fresh-chain |
| unreachable provider appends no exchange | `heddle-gateway/tests/governed_run.rs` (existing test, extended) | the chain never claims bytes crossed when none did |
| non-2xx / unparseable body still records the exchange | `heddle-gateway/tests/governed_run.rs` | the failure case that has no other witness |
| live provider exchange carries real `usage` | `heddle-gateway/tests/openai_compat.rs`, `#[ignore]`d | acceptance 5 |

No test asserts the *absence* of the variant from stub-driven runs beyond the sequences already
asserted in `native_loop.rs` and the connector suites — those are the regression net for D3's
default, and they pass unchanged or the design is wrong.

---

## 5. Risks and rollback

**Blast radius.** Five source files (`ledger.rs`, `model.rs`, `lib.rs`, `tool.rs`, `native_loop.rs`
in `heddle-core`; `lib.rs` in `heddle-gateway`) and three test files updated for genuinely-moved
assertions. `heddle-silo`, `heddle-cli`, `heddle-acp`, `heddle-mcp`, `heddle-connectors` and
`heddle-sandbox` **sources** are untouched. No new dependency, no `Cargo.toml` change, no `#[cfg]`,
no new CLI surface.

| Risk | Mitigation |
|---|---|
| **The implementer writes `self.client.turn(&req)?` on one line**, losing the failure-path capture — the highest-value case. | T4(b) is a red test specifically for it, written before T5. |
| **Redaction misses the escaped form** and a secret ships in cleartext on the chain. This is the one *silent* failure mode in the slice. | T2 uses a secret containing `"` and asserts both the scrubbed chain and the *unscrubbed* wire, so a `redact`-only implementation fails loudly. |
| **A stub-driven test's `kinds` vector changes**, meaning `take_wire_exchange`'s default was overridden somewhere it should not be. | The full-workspace suite is the detector; T7 names the exact three files allowed to change. |
| Chain and memory growth roughly doubles for model I/O. | Accepted (D5), bounded by ureq's existing 10 MB body limit, and stated in `spec.md` rather than discovered later. |
| The `dev`-vs-worktree drift (§0) causes work against stale `ChatMessage`/`ChatRequest` shapes. | T0 rebases and re-measures before any edit; this plan's anchors are `dev` anchors throughout. |

**Rollback.** `git revert` of the slice's commits restores the previous behaviour completely. There
is no migration to undo: the schema change is a new enum variant, and a chain written *with*
`wire_exchange` steps and then read by pre-023 code would fail its `serde_json::from_str::<StepKind>`
in `SqliteLedgerStore::load` — loudly, at load, never silently. State that direction explicitly in
`spec.md`: **forward compatibility is not claimed, backward compatibility is** (old chains read by
new code), which is the direction the additive-only rule actually governs.

---

## 6. Out of scope

Deliberately not done, so no one helpfully does it:

- **HTTP headers, the request line, and transport framing.** D6. Revisit with provider
  authentication, not before.
- **Provider authentication.** Explicitly forbidden by the request, and D6 is written the way it is
  to keep this slice from touching it.
- **MCP / tool-transport wire bytes.** A separate concern with its own capture via
  `ToolCall`/`ToolResult`. Constitution VII.
- **Any general network-tracing subsystem**, any on/off flag, any config key, any sampling or
  retention policy for the new step. D5.
- **Fixing the `ToolResult` JSON-escape redaction hole** discovered in D4. Recorded as a residual
  with its one-line fix named; a different payload with different tests.
- **`replay_tool_calls`-style replay from `WireExchange`**, a `heddle ledger diff` comparing raw and
  translated payloads, or any ACP `SessionUpdate` for the new kind. No caller.
- **Streaming (SSE).** Would change the capture shape entirely and is a standing separate residual.
- **`spikes/`** (ADR-0004 D2) — read as evidence for C1's actual wording, left byte-identical.
  `.github/`, `rust-toolchain.toml`, `Cargo.toml` likewise.
- **A PR.** No real remote; the bare mirror exists only for Archon's worktree isolation.
