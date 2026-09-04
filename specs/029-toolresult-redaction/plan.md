# Plan — slice 029: a `ToolResult` capture is scrubbed as wire bytes

**Target artifacts:** `specs/029-toolresult-redaction/{spec.md,plan.md,tasks.md}` plus the code
changes below. **Branch:** `029-toolresult-redaction`, cut from `dev`. **No PR.** Conventional
Commits. Strict TDD (Constitution III): red before green, and a red-by-revert to prove the green is
load-bearing.

---

## 0. Read this first — the tree, and why a one-line fix needs a plan

### 0.1 The worktree was stale, and the stale state invites the wrong fix

`HEAD` here was `d364405`, which predates slice 023 **entirely**:
`git show d364405:crates/skein-core/src/tool.rs | grep -c redact_wire` returns `0`. The whole premise
of this slice is that `redact_wire` already exists and is already the right tool for an
already-serialized body. An implementer starting from `d364405` would find no such method and would
be strongly tempted to write a second, divergent one from scratch. **S0 of this run: fast-forward
`029-toolresult-redaction` onto `dev` at `fe13c73` and re-measure the control baseline.** Verified
after the fast-forward: `git show HEAD:crates/skein-core/src/tool.rs` has `redact_wire` at line 237.
Every anchor below is a `dev` anchor.

### 0.2 Anchors verified on `dev` at `fe13c73`

| anchor | file | fact |
|---|---|---|
| the defect | `skein-core/src/tool.rs:410` | `content: self.redactor.redact(&outcome.content)` |
| `Redactor::redact` | `skein-core/src/tool.rs:213` | `text.replace(secret.expose(), "***")` — the literal needle only |
| `Redactor::redact_json` | `skein-core/src/tool.rs:224` | scrub-**then**-serialize; its doc names the escaping hazard |
| `Redactor::redact_wire` | `skein-core/src/tool.rs:237` | literal **and** `Value::String`-escaped needle; the exact premise a tool result satisfies |
| the source of the escaping | `skein-mcp/src/lib.rs:57` | `content: serde_json::to_string(&result)?` — the whole `CallToolResult`, serialized |
| the only other `ToolOutcome` construction in the product | — | there is none: `grep -rn "ToolOutcome {" crates/*/src` returns `tool.rs:76` (the definition) and `skein-mcp/src/lib.rs:57` |
| the propagation | `skein-core/src/native_loop.rs:186` | `Ok((_, captured)) => captured.content` — the capture is what goes back to the model |
| the two existing `redact_wire` callers | `skein-core/src/native_loop.rs:115-116` | `exchange.request` / `exchange.response`, with the comment stating the premise |
| the unit-level control | `skein-core/tests/tool_gateway.rs:226` | `secret_is_redacted_from_args_and_result_before_capture`, over `SECRET = "sk-SECRET-abc123"` — no escapable character, so it is green either way |
| the integration control | `skein-connectors/tests/governed_fs_run.rs:652` | `a_secret_in_a_files_contents_is_scrubbed_from_the_chain`, over `SECRET_ON_DISK = "sk-from-disk-SECRET-abc123"` — likewise |
| the assertion shape that cannot catch this | `skein-connectors/tests/governed_fs_run.rs:685` | `payloads.iter().all(|p| !p.contains(SECRET_ON_DISK))` |
| the helper that is *still* not enough | `skein-connectors/tests/governed_fs_run.rs:290` | `escaped(text)` — single-escaped; the `ToolResult` payload is doubly escaped |
| `replay_tool_calls` | `skein-core/src/tool.rs:418` | parses each `ToolResult` payload into a `CapturedResult`; the parsed form every assertion here uses |

### 0.3 What leaks today, measured

Measured this session with a temporary test at both levels, then reverted before S1. Configured
secret: `sk-"awkward"-SECRET-abc123`, in a real file, read by the real `fs_read` tool through the
real embedded MCP server, in the real `NativeLoop` (`governed_fs_run.rs`'s own harness).

| # | where | what is on it | leaked? |
|---|---|---|---|
| 1 | `ToolResult` step | `{"tool":"fs_read","content":"{\"content\":[{\"type\":\"text\",\"text\":\"api_key=sk-\\\"awkward\\\"-SECRET-abc123\\n…\"}],\"isError\":false}"}` | **yes** |
| 2 | turn 2's `LlmRequest` step | the same bytes, inside the `role:"tool"` message | **yes** |
| 3 | turn 2's `WireExchange` step | the same bytes again, escaped once more | **yes** |
| 4 | the HTTP request body the provider received | `…\"content\":\"{\\\"content\\\":[…api_key=sk-\\\\\\\"awkward\\\\\\\"-SECRET-abc123…` | **yes — the secret left the machine** |

And the two assertions a reasonable test would use, over all payloads of that run:

```text
naive contains over all payloads:         false
escaped-form contains over all payloads:  false
```

**Both false, with the secret present four times over.** Row 1's payload holds `sk-\\\"awkward…`
(backslash, backslash, backslash, quote): the literal needle `sk-"awkward…` is absent, and the
single-escaped needle `sk-\"awkward…` is not a substring of `sk-\\\"awkward…` either. This is why
D4 makes the parsed capture the only assertion surface in this slice.

The unit-level measurement isolated the same thing without any of the network machinery, and pinned
D2 at the same time:

```text
STEP ToolCall:   {"id":"","tool":"read_secret","args":{"token":"***"}}
STEP ToolResult: {"tool":"read_secret","content":"{\"content\":[{\"text\":\"api_key=pa\\\"ss\\\\wo\\nrd\",…}]…}"}
DECODED TEXT:    "api_key=pa\"ss\\wo\nrd"        ← the secret, whole
naive contains(AWKWARD) over raw ToolResult payload: false
```

`args.token` is `"***"`; the result is the secret. One line is wrong and the other is right, in the
same function, eight lines apart.

---

## 1. Problem

`ToolGateway::call_captured` scrubs an already-serialized JSON body with the scrubber whose needle is
the secret as written. A configured secret containing `"`, `\` or a newline is therefore captured in
cleartext, fed back to the model, and sent to the provider.

## 2. Approach

### D1 — `redact` becomes `redact_wire` at `tool.rs:410`, and nothing else in production

```rust
let captured = CapturedResult {
    tool,
    content: self.redactor.redact_wire(&outcome.content),
};
```

`ToolOutcome.content` has exactly one producer in the product — `skein-mcp/src/lib.rs:57`,
`serde_json::to_string(&result)?` — so "already-serialized JSON" is not a convention this line hopes
for, it is the only thing the port is ever handed. That is `redact_wire`'s documented premise
verbatim (`tool.rs:231-236`), and it was written one slice ago for the two other bodies satisfying
it. The comment above the line changes with it: it currently says nothing about escaping, and after
the fix it must say why this scrubber and not the neighbouring one.

**Fixing here rather than downstream is forced, not preferred.** Rows 2–4 of §0.3 are all *copies* of
row 1 made after the fact, each one escaping level deeper. `redact_wire` carries two needles —
literal and singly-escaped — and row 4 needs a third. Scrubbing the source is the only point where
the existing two needles suffice.

### D2 — `ToolCall`'s capture is correct; confirmed, not assumed

`call_captured` scrubs the attempt eight lines earlier:

```rust
let attempt = self.redactor.redact_call(call);          // scrubs Value::String contents
ledger.append(run_id, StepKind::ToolCall, serde_json::to_string(&attempt)?)?;  // serializes after
```

`redact_call` (`tool.rs:259`) walks a `serde_json::Value`, whose `Value::String`s hold **decoded**
Rust strings — `call.args` is deserialized from the provider's response, so a secret containing a
quote is in there as `pa"ss`, not `pa\"ss`. The literal needle matches; serialization then escapes
the already-substituted `***`. This is `redact_json`'s order, and `redact_json`'s doc explains it.

Measured rather than reasoned about: with `AWKWARD = "pa\"ss\\wo\nrd"` in a call's arguments, the
`ToolCall` step captured `{"token":"***"}` (§0.3). **The request that produced this slice assumed
both lines needed the same fix. They do not.** Changing this one would be a change with no failing
test behind it, and FR-008 pins its current behaviour instead.

### D3 — the audit: five plain-`redact` sites, one wrong

`grep -rn "\.redact(" crates/*/src` — the whole product:

| site | argument | already-serialized? | verdict |
|---|---|---|---|
| `skein-acp/src/stream.rs:72` | a model's streamed text delta | no | correct; its comment at `:54-57` already states this |
| `skein-core/src/native_loop.rs:113` | `exchange.url` | no — ours, plain | correct |
| `skein-core/src/native_loop.rs:192` | the tool name out of a `ToolDenied` | no | correct |
| `skein-core/src/tool.rs:262` | `call.tool`, from a parsed response | no | correct |
| `skein-core/src/tool.rs:410` | `outcome.content` | **yes** | **the defect** |

Recorded so this slice's scope is a measurement rather than a claim, and so a later reader does not
re-audit four correct lines.

### D4 — every assertion runs against the parsed capture

This is the decision that makes the slice testable at all. §0.3 measured both obvious assertion
shapes passing on a leaking tree. The `ToolResult` step payload is
`serde_json::to_string(&CapturedResult)` over a `content` field that is *itself* a serialized JSON
document, so a secret in it is escaped twice and matches no needle spelled the way an author thinks
of the secret.

So each test:

1. takes the capture through `replay_tool_calls` (or `serde_json::from_str::<CapturedResult>`), which
   undoes the outer level — this is also what a real replay consumer does;
2. parses `captured.content` as a `serde_json::Value`, which undoes the inner level and simultaneously
   discharges FR-004;
3. asserts on the decoded string at `content[0].text` — the one place the secret appears as written.

`assert_eq!` on that string, not `contains`: it pins both halves of FR-002/FR-005 at once and cannot
be satisfied by an unrelated payload.

### D5 — the awkward secret covers all three escapes at unit level, one at integration level

Unit (`tool_gateway.rs`): `"pa\"ss\\wo\nrd"` — quote, backslash **and** newline, the three characters
`serde_json` escapes in a string. `redact_wire` derives its second needle from
`Value::String(..).to_string()`, so one secret exercising all three proves the derivation rather than
one character of it.

Integration (`governed_fs_run.rs`): `sk-"awkward"-SECRET-abc123` — the quote only, and long and
distinctive per that file's existing convention for `SECRET_ON_DISK`. A backslash and a newline in a
file fixture buy nothing the unit test has not already covered, and the integration test's job is
FR-003 — that the escaping really arises from a **real** server rather than from a double's
`format!`.

### D6 — nothing else changes

No port, no `StepKind`, no payload shape, no CLI argument, no `Cargo.toml`, no `Redactor` signature,
no new method. One identifier, one comment, and tests. Rollback is the inverse one-line revert —
which is precisely what S3 performs on purpose.

---

## 3. Steps

- **S0** fast-forward onto `dev` at `fe13c73`; verify `redact_wire` is present at `HEAD`; measure the
  control baseline; measure the leak at both levels (§0.3) and revert the measurement; write
  `specs/029-toolresult-redaction/{spec.md,plan.md}`. *(`tasks.md` is S6's close-out.)*
- **S1** RED, forward — `tool_gateway.rs`: an awkward secret in a wire-shaped tool result must be
  absent from the **parsed** capture (FR-001/002/004/005), while the raw outcome and the transport's
  call still carry it (FR-006). Red recorded verbatim.
- **S2** GREEN — the one line at `tool.rs:410`, plus its comment (D1).
- **S3** RED-by-revert — temporarily restore `redact` at `tool.rs:410`, run S4's integration test,
  record the failure verbatim, restore `redact_wire`. This is the proof that the one line is
  load-bearing: a one-identifier change is exactly the kind a reviewer cannot distinguish from a
  no-op by reading it.
- **S4** integration — `governed_fs_run.rs`: the same property with a real file, the real embedded
  MCP server and the real loop (FR-003), asserted on the parsed capture per D4. This is the test S3
  reverts against.
- **S5** controls — the properties D1 puts at risk and nothing else would catch: a secret with no
  escapable character captures byte-identically (FR-007); an awkward secret in a call's *arguments*
  still captures as `***` (FR-008, D2's measurement made an assertion); the two existing controls at
  `tool_gateway.rs:226` and `governed_fs_run.rs:652` unmodified; and the unconfigured half of
  `governed_fs_run.rs`'s existing test unweakened (FR-009).
- **S6** close-out: `tasks.md` with the reds verbatim, the deviations and the residuals; the three
  gates green.

## 4. Validation

| gate | command |
|---|---|
| format | `cargo fmt --all -- --check` |
| lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| tests | `cargo test --workspace` |

The control baseline is the same three gates measured immediately after the fast-forward, before any
edit.

## 5. Risks and rollback

- **A green test over a leaking tree.** The realised risk, not a hypothetical one: §0.3 measured two
  reasonable assertion shapes passing while the secret was present four times. D4 is the mitigation
  and S3 is the proof that the mitigation works — a test that cannot fail when the line is reverted
  is not a test of the line.
- **Scrubbing a body into invalid JSON.** `redact_wire` replaces a whole escaped substring, so the
  surrounding escapes stay balanced; FR-004 asserts parseability rather than trusting it, and D4's
  step 2 makes every assertion depend on it.
- **Fixing the second line too.** D2 measured that line correct. FR-008 pins it, so a future
  mechanical "make them consistent" edit fails a test.
- **The double-escaped copies downstream.** Closed by fixing the source; explicitly *not* closed by
  adding needles (spec rejected alternative 1). If a third serialized-body site appears later, it
  needs `redact_wire` at its own source, and D3's audit table is where it is recorded.
- **Rollback** is one identifier.

## 6. Out of scope

- **Unconfigured secrets** (FR-009).
- **`Redactor`'s public surface** — no new method, no signature change.
- **`redact_call` / the `ToolCall` capture** (D2).
- **`skein-acp`'s streamed deltas** — plain text; D3.
