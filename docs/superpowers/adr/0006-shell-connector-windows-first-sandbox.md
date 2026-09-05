# ADR-0006: `shell` ships Windows-first, gated off elsewhere until each OS earns its own backend

**Status:** Accepted
**Date:** 2026-09-03
**Decider:** Cédric Thedrez (`kamicoder`)
**Supersedes/amends:** supersedes ADR-0005's blanket deferral of `shell`. Does not reopen
ADR-0004 D3's other items.

## Context

ADR-0005 deferred `shell` out of v0 entirely, reasoning that no crate covers process sandboxing
across Windows, Linux, and macOS at once, and that hand-rolling three platform backends behind one
trait is a subsystem, not a slice. That research stands. The conclusion drawn from it does not: a
v0 MVP does not need cross-OS parity on day one, and this project's own dev machine, and its own
CI, already resolve the practical objection that follows from picking one OS first.

Corrected premises:

- **Tri-OS CI is already free and already running.** `.github/workflows/core.yml` and
  `spikes.yml` both run `windows-latest`, `macos-latest`, `ubuntu-latest` via GitHub Actions —
  free for a public repository. There is no need to source a personal Mac to keep the existing
  gates green; a Windows-only addition simply needs its own tests gated to the Windows leg, the
  same way any `#[cfg(windows)]` code in any Rust project is tested.
- **The dev machine is Windows.** The most-exercised, most-debuggable platform for a first
  capability-sandboxed shell tool is the one the operator can actually run and inspect locally.
  Linux (via WSL, or the Linux CI leg) and macOS (CI leg only, for now) remain untouched by this
  connector until they get their own backend.

ADR-0005's actual load-bearing point survives: a *single* trait implemented identically on all
three OSes on day one is still not a slice. What this ADR changes is the unit of delivery — one OS
at a time, each gated, rather than all three or none.

## Decision

- **`shell` ships for Windows first.** The connector is compiled and its MCP tools are advertised
  only when the host OS is Windows (`#[cfg(windows)]` at the connector boundary, verified by a test
  that a non-Windows build advertises no `shell` tools at all — not a silent no-op tool that
  answers "unsupported," an absent one, matching the deny-by-default posture `fs`/`git` already
  established). Linux and macOS get their own backend in a later, separately-scoped slice each;
  until then the tool is simply not there, which is what Principle VI (Fail clearly, never a silent
  fallback) asks for here.
- **Dependency choice, verified this session:**
  - **`win32job`** (Job Objects: kill-on-close, memory/CPU/process-count limits) — narrow, mature,
    does exactly one well-understood thing. Adopted for process lifetime/resource bounds.
  - **`rappct`** (AppContainer/LPAC capability restriction) was considered and **rejected**: ~5,400
    all-time downloads, single maintainer, pre-1.0. This is the same trust profile this project
    already rejected once this session for third-party MCP filesystem servers, and the reasoning
    transfers directly — a low-adoption crate sitting in the tool-execution path is a direct
    code-execution supply-chain risk for a governed agent, not a convenience trade worth making.
  - **Restricted token + AppContainer construction is hand-rolled directly against `windows-rs`**
    (the official Microsoft crate; not yet a Heddle dependency, added by this work) instead of
    `rappct`. More implementation work, same trust bar this project already held for `git2` over a
    smaller wrapper, and for an in-house MCP server over a third-party one.
  - Job Objects alone (`win32job`) bound resource usage but not filesystem or network capability —
    that is not a jail by itself. The `windows-rs` restricted-token/AppContainer layer is what
    actually bounds what a spawned process can touch; both are required, not either/or.
- **Scope for the first shell slice**: a Windows-only sandboxed process launcher (restricted token,
  AppContainer SID scoped to the same root a connector's `fs`/`git` tools already use, no network
  capability granted, Job Object for lifetime/resource bounds), then exactly one MCP tool built on
  it, mirroring `fs`/`git`'s existing shape (bounded output, no arbitrary passthrough beyond the
  command and arguments a model supplies as *values*, never as shell syntax — the same fixed-argv
  discipline slice 017 already established for `git`). The design question ADR-0005 flagged — one
  trait, per-OS backends, bucket-A review before product code depends on it — still applies; it is
  now scoped to designing that trait for its first (Windows) implementation, not for three at once.
- **Linux and macOS remain deferred**, each its own future ADR-authorized slice when it is planned,
  not implied by this one. Landlock (Linux) and Seatbelt or a hand-rolled sandbox profile (macOS)
  are the leading candidates per ADR-0005's research and are not re-litigated here.

## Consequences

- ADR-0005's "v0 closes without `shell`" conclusion is superseded: v0 now includes a Windows-only
  `shell` connector as its own slice, sized and TDD-provable the way `fs` and `git` were.
- `windows-rs` becomes a new workspace dependency, scoped to the connector or a sibling
  platform-specific module — not spread across crates that do not need it.
- CI's Linux and macOS legs continue to build and test the full workspace; they simply never
  compile or exercise `shell`'s tool code, exactly as any other `#[cfg(windows)]` module in the
  ecosystem behaves under `cargo test --workspace` on a non-Windows runner.
- The next planning brief for this connector must verify `windows-rs`'s actual API shape for
  restricted tokens and AppContainer profile creation before assuming it (this project's standing
  rule: research the real API, do not guess it) and must state its own security test list the way
  slice 017 proved containment for `git` — at minimum, a test that a sandboxed process cannot write
  outside its configured root and cannot reach the network.
