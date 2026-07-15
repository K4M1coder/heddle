# Skein Constitution

Immutable principles governing the transformation of specifications into code for Skein. Every spec, plan, task, and implementation must conform to them; any violation must be justified under "Complexity Tracking" or rejected.

## Core Principles

### I. Headless core — CLI as the reference, UI as a thin layer
Every capability lives in the headless core and is exposed through a **programmatic API**; the **CLI is its complete, authoritative client** (the basis for E2E tests); the **UI adds no capability of its own**. Everything the UI does, the CLI does; everything the CLI does, the API exposes.

### II. Local-first, silo isolation (NON-NEGOTIABLE)
Every capability has a **local implementation by default**. Data is partitioned into **airtight silos per mode** (Local / Server / Remote) and, in Remote, **per team**. No data crosses a silo boundary. In Local mode, **no network egress** (egress OFF) — local providers only.

### III. Test-First (NON-NEGOTIABLE)
Strict TDD: test written → fails (red) → minimal implementation → passes (green) → refactor. Every interface boundary is testable with a mock behind it. A dedicated **isolation test** guards each silo invariant.

### IV. Inverted coupling & explicit boundaries
The core **discovers** connectors (MCP), providers (Gateway), identity, secrets, and control (Controller) through **traits/interfaces**; it never depends on them directly. Adding a capability = adding an implementation behind an interface, never rewriting the core.

### V. Traceability & reversibility (event sourcing)
Every step (exact model I/O, tool-calls, state changes) is captured in an **append-only, hash-chained Ledger** — inspectable, replayable, reversible (git-style). Complemented by an **immutable audit trail** (who/when). Traceability cannot be bypassed.

### VI. Security & secrets by reference
Deny-by-default. **Secrets by reference, never by value**, resolved **just-in-time**, redacted from logs. RBAC across 3 scopes (global / silos / intra-silo). Destructive/irreversible actions → confirmation. External content = data, never instruction (anti-injection).

### VII. Neutrality & reuse (YAGNI)
Multi-provider, multi-IdP, multi-secret-backend: no vendor lock-in. We **reuse** proven existing tools (Goose, LiteLLM, MCP, BMAD, Spec-Kit) rather than rewrite them. Start simple; no capability without a real need.

### VIII. Loop discipline (NON-NEGOTIABLE)
Every agentic loop — in the product **and** in how we build it — must: (a) have **externally-enforced termination** (iteration/token/cost budget + no-progress detection; the model never decides when to stop); (b) **anchor every reflect/retry to ground-truth external feedback** (tests, compiler, linters, type-checkers, tool results) — never model self-judgment, because intrinsic self-correction is unreliable and can degrade output; (c) verify at three levels — **action, iteration, terminal**; (d) **escalate to a human** on budget/failure-threshold breach. See §4.14 and `docs/research/loop-engineering.md`.

## Additional Constraints (Stack & Compliance)

- **First-class cross-platform**: Windows + macOS + Linux as equals (tri-OS CI matrix, green required before merge). No OS-specific call without `#[cfg]` + an equivalent.
- **Stack**: Rust core (Goose as an upstream dependency; hybrid fork/patch with an upstream PR when needed); Python sidecar; Tauri/TS UI; LiteLLM Gateway; SQLite persistence; OpenTelemetry observability from v1.
- **Compliance by-design**: GDPR / ISO 27001 / SOC 2 / EU AI Act / NIS2 — the software provides the controls; certification remains an organizational matter.
- **Per-OS code signing** (Authenticode + Developer ID/macOS notarization) — an agent that drives the PC must be signed.

## Development Workflow (BMAD × Spec-Kit Bridge)

- **Planning = BMAD**: PRD → architecture → epics/stories (verifiable artifacts in `_bmad-output/planning-artifacts/`).
- **Execution = Spec-Kit**: `specs/[###-feature]/` with `spec.md` → `plan.md` → `tasks.md` → gated implementation, each phase passing the **Constitution Check**.
- **Conventional Commits**, trunk-based, PR + review. Pipeline: lint → build (3 languages) → tests (unit/integration/E2E CLI/isolation) → security scans (SAST/deps/secrets/SBOM) → signed artifacts.

## Governance

This constitution **takes precedence** over other practices. Every PR/review verifies conformance to it. Any complexity that departs from a principle must be justified (the plan's "Complexity Tracking" table) or rejected. Amendment = documentation + version + date.

**Version**: 1.1.0 | **Ratified**: 2026-07-15 | **Last Amended**: 2026-07-15
<!-- 1.1.0: added Principle VIII (Loop discipline) from loop-engineering research. -->
