# ADR-0005: v0's `shell` connector is deferred pending a sandbox foundation; v0 closes without it

**Status:** Superseded by [ADR-0006](0006-shell-connector-windows-first-sandbox.md)
**Date:** 2026-09-03
**Decider:** Cédric Thedrez (`kamicoder`)
**Supersedes/amends:** narrows ADR-0004 D3's "MCP tools (fs/git/shell)" item; does not reopen D3's other items.

> **Superseded:** this ADR's research (no crate covers Windows+Linux+macOS process sandboxing at
> once) still stands, but its conclusion — defer `shell` out of v0 entirely — was wrong to treat
> cross-OS parity as required on day one. ADR-0006 ships `shell` Windows-first instead, gated off
> on other OSes until each earns its own backend. Read ADR-0006 for the current decision; this
> document is kept for its research and for the historical record.

## Context

ADR-0004 D3 named three connector families as v0 build scope: `fs`, `git`, `shell`. Slice 016
(`specs/016-fs-connector`) closed `fs`: an embedded rmcp server, root-bounded, hand-verified live
against a real local Ollama model. Slice 017 (`specs/017-git-connector`) closed `git`: two
read-only tools (`git_status`, `git_log`) via `git2`, root-bounded to one configured repository, a
fixed argument surface with no subprocess and no arbitrary `git` passthrough, also hand-verified
live. Both slices independently rejected a general "run this shell command" tool as part of their
own scope, reasoning that an allowlist of command *names* is not an allowlist of *effects* — the
same binary can read a file, delete a tree, or reach the network depending on arguments no static
allowlist can fully anticipate.

`shell` is the one remaining named item. Before planning it as a slice, this ADR records the
research into whether it can be built with the same TDD-provable, single-slice discipline every
other v0 item used.

## Research

Surveyed the Rust ecosystem in September 2026 for a proven, actively-maintained crate that
sandboxes a child process's filesystem/network/execution capabilities across Windows, Linux, and
macOS — the three OSes Heddle's own CI already targets (ADR-0004 D1(d), tri-OS CI green):

- **`birdcage`** (Phylum) — cross-platform embeddable sandbox, but explicitly Linux (Landlock) and
  macOS (Seatbelt) only; no Windows support.
- **`rust-landlock`** — a safe wrapper over the Linux Landlock LSM. Linux-only by construction;
  Landlock is a Linux kernel feature.
- **`rappct`** — an AppContainer/LPAC toolkit for Windows (profile lifecycle, capability builders,
  Job Object integration). Windows-only by construction.
- **`openai/codex-windows-sandbox`** — a Windows-specific helper binary and library, merged into
  Codex's own tree; not a general-purpose dependency, and Windows-only.
- **`gaol`** (servo) — cross-platform application sandboxing; effectively unmaintained for years
  and pre-dates modern Landlock/Seatbelt/AppContainer APIs this project would want.
- macOS's own `sandbox-exec` is itself deprecated upstream guidance, used pragmatically by several
  projects (e.g. Codex) but with no first-party Rust wrapper at the maturity of the above.

No crate — and no small combination of crates — currently gives one coherent, safe, testable API
across all three OSes. The real options are: (a) hand-roll three platform-specific backends
(Landlock on Linux, Seatbelt or a manual sandbox profile on macOS, restricted token + Job Object or
AppContainer on Windows) behind one trait, or (b) ship an unsandboxed shell tool and rely on
allowlisting alone, which slices 016 and 017 already rejected as insufficient for exactly this
tool shape.

Option (a) is not a slice. It is a new subsystem: three independent OS-security integrations, each
with its own failure modes, each needing platform-specific tests that cannot run in the other two
OSes' CI legs, each carrying its own hardening iteration (a sandbox that "passes its tests" and
still leaks a capability is a distinct, well-known failure class — a green suite does not prove a
negative). Every prior v0 slice was sized to days of TDD work against a design that was fully
specified before implementation began (ADR-0004 D1(c): "Spec-Kit clarify → plan → tasks → analyze
green for the current slice only"). A cross-platform sandbox does not fit that shape: its design
question — which primitive on which OS, with which fallback when the primitive is unavailable
(e.g. an unprivileged container, a locked-down CI runner, an old kernel without Landlock) — is
itself the kind of one-way-door contract ADR-0004 D1(a) reserves for bucket-A review, not something
to settle inside a single implementation slice's TDD loop.

## Decision

- **`shell` is deferred out of v0**, not built as a fourth connector slice. This narrows ADR-0004
  D3's connector item to `fs` and `git`, both delivered (slices 016, 017), each hand-verified live
  against a real local model with real Ledger entries.
- **v0, as ADR-0004 D3 defined it, is otherwise complete**: Heddle-owned native loop with an
  ACP-shaped core boundary (ADR-0003, slices 003/004/008/013), MCP tools for `fs` and `git`
  (slices 005/015/016/017), one local model path via the Ollama-compatible gateway (slices
  006/012), silo `Local` + Ledger + `SecretProvider` foundation (slices 003/009/010/014), and the
  CLI surface (slices 011 onward). A `shell` connector was named in D3's list but its safe
  construction depends on infrastructure — a cross-platform process sandbox — that does not yet
  exist in this codebase or in the Rust ecosystem in a form mature enough to depend on.
- Building that infrastructure is a new, explicitly-scoped initiative, not a drift: a future
  **sandbox foundation** effort should (1) design one trait covering the capability this project
  actually needs (bound a child process's filesystem writes and network egress to what the tool
  call's arguments justify), (2) implement it per-OS behind that trait, gated by its own bucket-A
  contract review before any product code depends on it, and (3) only then admit a `shell`
  connector slice sized the way `fs` and `git` were. This ADR does not attempt that design; it
  records why the attempt does not belong inside the connector slices already shipped.
- This is an ADR-authorized scope narrowing, per ADR-0004 D3's own rule ("Scope additions to v0
  require an explicit ADR, not a conversation drift") — the symmetric case, a scope item that
  cannot be safely closed without new infrastructure, gets the same explicit treatment rather than
  silent abandonment or an unsafe shortcut.

## Consequences

- `specs/017-git-connector/tasks.md`'s "Next slice" section, which named this same deferral and
  reasoning, is the working note this ADR now makes durable at the architecture-decision level.
- No code changes accompany this ADR. `crates/heddle-connectors` gains no `shell.rs`; no subprocess
  execution tool exists anywhere in `crates/`.
- A future sandbox-foundation ADR, when it exists, supersedes the deferral recorded here for the
  specific question of *how* to build `shell` — not *whether* v0 needed it, which this ADR settles
  now.
