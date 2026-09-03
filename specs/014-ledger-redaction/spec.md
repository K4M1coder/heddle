# Feature Specification: redaction on the `LlmRequest`/`LlmResponse` Ledger path (v0 slice)

**Feature Branch:** `014-ledger-redaction` · **Created:** 2026-09-03 · **Status:** Implemented (v0
slice) **Input:** `specs/013-acp-agent/tasks.md` "Next slice" — *"**redaction on the
`LlmRequest`/`LlmResponse` path**, so `skein ledger show` cannot print a conversation secret"*,
carried unchanged from slices 011 and 012 · Constitution III (**test-first**), IV (**inverted
coupling**), V (**traceability**), VI (**secrets by reference, redacted from logs**), VII (**no
capability without a real need**) · design §4.11, §7.13.

Since slice 012, `skein chat` drives a real conversation through a real provider; since slice 013,
`skein acp-agent` does the same from an editor. Both write every turn into a durable SQLite-backed
silo (slice 009), readable forever after with `skein ledger show`. `ToolGateway::call_captured`
scrubs the tool payloads it records; `NativeLoop::run` appended the model conversation **raw**. Two
collaborators writing the same chain, one governed and one not.

This slice closes that, and ships the operator-facing way to configure it, so the fix has a caller in
the shipped binary rather than existing only in principle.

## What this slice changes for a user

```
skein chat      --silo <ID> [--root <PATH>] --model <NAME> [--base-url <URL>] [--redact <REFERENCE>]…
skein acp-agent --silo <ID> [--root <PATH>] --model <NAME> [--base-url <URL>] [--redact <REFERENCE>]…
```

`--redact` is repeatable and takes a **reference** — `keychain://<service>/<account>`, the same shape
`skein secret set` provisions — never a value. There is no `--redact-value`, for exactly the reason
`skein secret set` has no `--value`: a secret in a flag lands in shell history and in process
listings.

With one or more references configured, every value they resolve to is replaced by `***` in every
Ledger payload of that run, on both commands.

Three user-visible consequences, stated here rather than left to be discovered:

1. **`skein chat`'s stdout is unaffected.** It prints `run.final_message`, which is the raw
   `resp.message`. The operator still gets the real answer; only the record is scrubbed.
2. **An ACP client's transcript now shows `***`.** `project_updates` derives its
   `AgentMessageChunk` text from the `LlmResponse` **Ledger payload**, so an editor sees the redacted
   text. This is the same property `ToolResult` content already had, and it is intended: the two
   commands therefore differ, and that difference is deliberate.
3. **`LlmRequest`/`LlmResponse` payload key order depends on the build graph, and every consumer
   parses rather than pattern-matches, so it does not matter.** This slice's own manifest declares
   plain `serde_json = "1"` with no `preserve_order` feature, but Cargo unifies features per build
   graph: `agent-client-protocol` declares `serde_json = { features = ["preserve_order", …] }`, so
   any binary whose graph includes `skein-acp` — `skein-cli`, since it depends on `skein-acp` —
   compiles `serde_json::Map` as an insertion-ordered `IndexMap`, not the `BTreeMap` this slice
   assumed. **Correction:** the original text here claimed payloads are "alphabetically keyed"
   because `preserve_order` "is not enabled" — false in the shipped binary, first measured by slice
   008 (its `## Observed` risk R1) and pointed out against this file specifically by slice 015
   (`specs/015-tool-advertisement/tasks.md`). No behaviour was ever wrong: every consumer in the
   tree parses rather than pattern-matches (plan D2's audit), so the actual key order never
   mattered, which is why this went unnoticed through two more slices. Recorded here rather than
   left for a reader to trip over.

An unresolvable `--redact` reference is **exit 1 with no chain opened** — the same rule both commands
already document for a non-loopback `--base-url`, and for the same reason: a one-step run in a silo
would be a misleading record of an attempt that never left the process.

## Four things a reader must know up front

1. **The invariant is per-payload, not per-step-kind.** The acceptance tests scan *every* payload of
   a run for the secret, not only the two new ones, so a future step type that leaks is caught by a
   test that already exists.
2. **Redaction is serialize-then-scrub, not scrub-the-serialized-string.** A secret containing `"`,
   `\` or a newline is JSON-**escaped** inside a serialized payload, so a string-level replace would
   miss the literal needle entirely; and a replacement that did land could straddle an escape
   sequence and produce unparseable JSON. `Redactor::redact_json` serializes to a
   `serde_json::Value`, scrubs the strings inside it, and re-serializes — so the payload stays
   parseable for replay.
3. **The tool *name* is redacted too.** It is model-chosen text (`ToolPolicy`'s own docstring says
   so), so it can carry an echoed secret exactly as arguments can. All three recorded copies — the
   `ToolCall` attempt, the `ApprovalRecord`, the `CapturedResult` — are scrubbed, while the policy
   decision and the transport call still use the **raw** name.
4. **This is not a defence against a secret the operator never configured.** `Redactor` is an
   exact-value scrubber. A credential pasted into a prompt that was never registered with
   `skein secret set` and named with `--redact` still lands in cleartext. This slice makes redaction
   possible and wired; it does not make it automatic.

## Functional requirements

- **FR-001** `NativeLoop::new` takes a `Redactor` as a required fourth argument. Not a builder, not a
  `Default`: "no redaction" must not be a silent default, and the compiler must enumerate every call
  site.
- **FR-002** `NativeLoop::run` writes the `LlmRequest` and `LlmResponse` payloads through
  `Redactor::redact_json`. The request handed to `ModelClient::turn` and the response returned in
  `LoopRun.final_message` stay **raw**.
- **FR-003** `Redactor::redact_json` is public and reuses the existing `redact_value` recursion.
  `redact_value` stays private.
- **FR-004** `Redactor` is `Clone`, hand-written, so one run configures one secret set shared by the
  loop and the gateway. `SecretValue` stays non-`Clone` and its public API does not widen.
- **FR-005** `ToolGateway::call_captured` redacts the tool name in the `ToolCall` attempt, the
  `ApprovalRecord` and the `CapturedResult`. `ToolPolicy::decide` and `ToolTransport::call` still
  receive the raw name, and `SkeinError::ToolDenied` still names it raw to the caller.
- **FR-006** `SkeinSession::new` clones the injected redactor into the gateway and passes the
  original to the loop. `skein-acp`'s public API is otherwise unchanged.
- **FR-007** `skein chat` and `skein acp-agent` accept a repeatable `--redact <REFERENCE>`, resolved
  through `skein-silo`'s `OsKeychain` as a `SecretProvider`, after the endpoint guard and before the
  silo is opened.
- **FR-008** With no `--redact`, the credential store is **not opened**. A run that configures no
  secret must acquire no runtime keychain dependency.
- **FR-009** `ToolGateway::new`'s signature is unchanged; no `Arc` is added anywhere.

## Success criteria

- **SC-001** No payload of a run whose prompt and reply both carry a configured secret contains that
  secret; at least one contains `***`.
- **SC-002** The redacted `LlmRequest`/`LlmResponse` payloads still deserialize into `TurnRequest`
  and `TurnResponse` with their structure intact.
- **SC-003** The model genuinely received the raw prompt, and `LoopRun.final_message` genuinely
  carries the raw reply.
- **SC-004** A tool call whose *name* embeds a secret leaks it into no payload, is still refused by
  the policy on its raw name, and reaches no transport.
- **SC-005** An ACP session's chain is redacted and its client transcript shows `***`.
- **SC-006** `skein chat --redact <ref>` prints the real answer on stdout and writes `***` to the
  chain; an unresolvable reference exits 1 with no ledger file created, on both commands.
- **SC-007** All 110 pre-existing tests pass with **unchanged bodies**. `git diff dev` on the test
  files shows only added arguments and the additive `ScriptedModel.seen` field.
- **SC-008** `git diff dev -- crates/skein-silo/ spikes/ .github/ rust-toolchain.toml` is empty
  except `crates/skein-silo/tests/silo_ledger.rs`'s one mechanical `NativeLoop::new` argument.
  `skein-silo`'s and `skein-gateway`'s **product** code is untouched.
- **SC-009** Zero new packages and zero new dependency edges.

## Assumptions

- **Two plaintext copies of each secret exist per run** — the loop's `Redactor` and the gateway's,
  because `Redactor: Clone` deep-copies its `SecretValue`s. Both are `Zeroizing` and both zeroize on
  drop, and the material is already resident in process memory. Accepted deliberately over an `Arc`
  in a public constructor signature.
- **The Windows leg is observed locally; the macOS and Linux legs are unobserved** until the
  repository has a remote — the standing caveat of specs 004–013.
- **`skein secret set` is the provisioning path.** `--redact` resolves references; it does not create
  them.

## Out of scope

- **Raw wire-byte capture** — the HTTP bodies `skein-gateway` exchanges. A separate, already-named
  next-slice item.
- **Provider authentication / a provider token as a `SecretRef`.**
- **Automatic secret detection** — entropy heuristics, `sk-` prefix matching, or any redaction of a
  value the operator did not configure.
- **Redacting `SkeinError` messages, stderr, or `skein chat`'s stdout.** The invariant is about
  Ledger payloads; `chat`'s stdout carrying the raw answer is the intended contract.
- **A config file for secret references**, or a `$SKEIN_…` environment fallback for `--redact`.
- **Changing `ToolGateway::new`'s signature**, adding `Arc` anywhere, or making `SecretValue: Clone`.
- **`spikes/`** — untouched (ADR-0004 D2).
- **Widening the ACP surface**, streaming, tool advertisement, or a `--json` output mode.
