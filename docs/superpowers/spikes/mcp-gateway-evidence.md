# Spike 4 Evidence — Tool governance (`spikes/mcp-gateway/`)

**Date:** 2026-09-03 (note written) · **Status:** COMPLETE
**Method:** pre-registered criteria (spike-protocol.md §Spike 4); ground truth = observable
assertions over a **live in-process rmcp client↔server pair**, not a mocked protocol. Nothing
was patched to "make it pass".

This note was written retroactively: the spike itself landed with `spikes/mcp-gateway/`, but
the evidence note ADR-0004 D2 requires ("an evidence note in `docs/superpowers/spikes/`" per
spike) was never produced, so ADR-0003's Accepted line cited a file that did not exist. Slice
`005-tool-gateway` closes that gap. The results below come from a fresh re-run, not from
quoting the original session.

## Question and pre-registered exit criteria

**Question:** can Heddle proxy MCP servers through policy/approval/redaction/Ledger capture
using `rmcp`?

**Exit:** a tool call is (1) blocked by policy, (2) allowed after approval, (3) logged with the
secret redacted, (4) replayed from the captured record.

## Criteria matrix

| Criterion | Test | Result | What the test actually observes |
|---|---|---|---|
| C1 blocked by policy | `c1_blocked_by_policy` | **PASS** | `fs_write` is classified mutating and unapproved. The call returns `Err`, the shared `AtomicUsize` the downstream tool bumps stays at **0** — the server really never ran it — and the record's last event is `Denied { tool: "fs_write" }`. |
| C2 allowed after approval | `c2_allowed_after_approval` | **PASS** | The same tool, now in `approved`, returns `Ok`; the counter is **exactly 1** (executed once, not zero and not twice); the last event is `Executed`. |
| C3 logged with redacted secret | `c3_capture_redacts_secret` | **PASS** | `read_secret` really returns `sk-SECRET-abc123` (asserted as a sanity check on the live result), while a dump of the whole record contains no occurrence of it and does contain `***`. The scan is over the entire record, not only the result field. |
| C4 replayed from the record | `c4_replay_from_record` | **PASS** | After one live call the counter is 1; `replay()` returns the redacted output and the counter is **unchanged** — replay made no downstream call. |

The spike covers **one** in-process server over a `tokio::io::duplex` pipe. The protocol
question in spike-protocol.md mentions "1 local stdio + 1 remote OAuth"; neither a spawned
stdio process nor a remote OAuth server was exercised. The governance properties are proven
against a real MCP implementation; multi-server and remote-auth transport remain unproven.

## Command and observed output

```console
$ cargo test --manifest-path spikes/mcp-gateway/Cargo.toml
     Running tests\gateway.rs (target\debug\deps\gateway-521da3d98dfc1817.exe)

running 4 tests
test c1_blocked_by_policy ... ok
test c3_capture_redacts_secret ... ok
test c4_replay_from_record ... ok
test c2_allowed_after_approval ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Exit code 0. Toolchain: `stable` (rustc 1.97.1), as `.github/workflows/spikes.yml` uses.

## Findings that shaped the product slice

- **`rmcp` 2.2.0 is `edition = "2024"`**, so it cannot be compiled by any rustc below 1.85.
  The workspace pinned 1.79 in three enforcing places, which made promoting this spike
  impossible without a toolchain decision. Slice 005 pins 1.97 — see
  `specs/005-tool-gateway/plan.md` Complexity Tracking.
- **The spike calls the deprecated `CallToolRequestParam`** (`rmcp-2.2.0/src/model.rs`:
  `#[deprecated(since = "0.13.0", note = "Use CallToolRequestParams instead")]`). It survives
  only because `spikes.yml` runs no clippy; product code under `clippy -D warnings` must use
  `CallToolRequestParams` with `.with_arguments(..)`.
- **`transport-async-rw` suffices** for an in-process duplex; the spike's `transport-io` is
  that plus `tokio/io-std`, i.e. stdio, which nothing here needs.
- **The spike's `GatewayEvent` enum is a spike-only stand-in.** The product writes through
  `Ledger::append` with the existing `StepKind::{ToolCall, Approval, ToolResult}`; no new
  variant and no parallel event log.
- **The spike redacts only results**, because it never captures arguments. §4.11 requires the
  call's name *and* arguments, so the product redacts both — capturing arguments unredacted
  would have introduced a leak the spike did not have.

## Decision

**Promote the design, not the code.** `spikes/mcp-gateway/` stays quarantined and unimported
per ADR-0004 D2; slice `005-tool-gateway` reimplements the governed path in
`crates/heddle-core/src/tool.rs` (no new dependency) with the `rmcp` adapter isolated in
`crates/heddle-mcp/`. All four criteria are re-proven there against a live embedded rmcp
server, plus a fifth property the spike could not express: the governed run's hash chain
verifies across a denial and an execution.
