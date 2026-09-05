# ADR 0002 — Design hardening from adversarial review

**Date**: 2026-07-15 · **Status**: accepted · **Supersedes/updates**: parts of the master design (§4.2, §4.11, §4.12, §4.14, §5.4, §5.5, §7.3, §7.9, §7.10, §7.12, §7.13) and ADR 0001 (Goose integration scope).

**Context**: Four parallel adversarial reviewers red-teamed the design (architecture, security/config, loop/workflow-vs-research, process/readiness). They found real, converging defects, primarily around loop ownership and the event-sourcing schema. This ADR records each confirmed finding and the decided resolution. Detail lives here; the master spec carries pointers.

## Decisions

### D1 — Loop ownership: **Heddle owns the loop** (was ambiguous / implicitly Goose)
The `goose run` **CLI subprocess** integration is incompatible with LoopController (§4.14), per-step Ledger capture (§4.11), termination enforcement, and "recipe = resumable workflow" (§4.12) — because Goose runs its own reason→act→observe loop opaquely.
**Decision**: Heddle's WorkflowEngine/LoopController own the iteration. Goose is used as a **single-turn / tool executor** via **goosed (HTTP/streaming API) or the embedded `goose` crate** — the ADR 0001 spike is re-scoped to evaluate *these*, not the CLI subprocess, and must confirm per-turn model I/O + a correlation ID. Goose's MCP tool traffic is routed through a **Heddle-hosted MCP proxy** so tool calls/results become Ledger ground truth. If a capability must be delegated to a Goose-internal loop, LoopController degrades honestly to **process-level supervision (wall-clock/kill) only**, and the per-step claims are explicitly waived for that path.
**Impact**: one-way door; blocks Phase 0 runtime design. ADR 0001 spike scope updated; Phase 0 plan Task 5 (`GooseRuntime`) must expose step-level events, not `output()`-batch.

### D2 — Event-sourcing schema corrections (one-way door)
1. **Step identity**: use a **surrogate id** (`ULID`/monotonic) as primary key; keep the **content hash as a separate integrity field**, and chain on `parent + seq`. (Content-hash-as-PK collides on legitimate repeats and breaks `branch`.)
2. **Correlation**: every Step carries `{session_id, run_id, step_seq, trace_id}`; the `trace_id` is propagated to the Gateway so captured model I/O attributes back to a step even under concurrency.
3. **Effect classification**: `StepKind` gains an effect class `{pure | reversible | irreversible}` + an **idempotency key** for external effects, and effect logging is **two-phase** (`Intent` → `Applied`). `resume`/`replay`/`branch` **replay recorded results by default** and re-execute effects only on explicit opt-in; irreversible effects are never auto-re-fired.
4. **Loop event types**: extend `StepKind` with `Reflection`, `Evaluation{verdict}`, `IterationBoundary{n}`, `BudgetSpent{tokens,cost}`, `Exit{reason}`, `Approval{decision}`; define the fold that reconstructs `LoopState` (iter, reflection buffer, budget) on resume.

### D3 — Unified config resolution + security floor (fixes §5.4↔§5.5 contradiction)
Two **orthogonal** axes:
- **Value resolution** — *most-specific wins*: the lowest level that sets a value wins (Conversation > Project > Team > Silo).
- **Lock** — an explicit flag; a locked value freezes all lower levels; **the highest lock wins**. *Setting a value ≠ locking it.*
Algorithm: walk highest→lowest; highest **locked** value wins; else lowest **set** value wins.
- **Security settings are a monotonic floor**: a higher level can only make egress/guardrails **stricter**; lower levels may tighten further, never loosen. A higher level can never *relax* a lower level's security.
- **RBAC scoping**: each role edits **its own scope and below** (project lead ⇒ Project▸Conversation only; team lead ⇒ Team and below). Silo-level **security** locks require **dual control** (four-eyes / break-glass, audited).

### D4 — Egress enforced at a network boundary (fixes "Local = no egress" holes)
`requires_network()` becomes a property of **every** network-capable pluggable interface — **including MCP connectors**, model routes, OTel exporters, IdP, secrets, trackers — checked at **enable-time**. Beneath the policy layer, Local mode enforces a **hard network boundary** (process socket-deny / loopback allowlist) so a mis-declared or hostile backend cannot egress. Egress is **deny-by-default with an allowlist even in Server/Remote**.

### D5 — GDPR erasure via crypto-shredding (fixes append-only↔erasure contradiction)
Personal-data payloads are stored **encrypted under a per-data-subject key** (subject-indexed keystore, mutable, **outside** the append-only log). The Ledger chains the **hash of the ciphertext**. Erasure = **destroy the subject's key(s)** → plaintext unrecoverable (Art. 17) while ciphertext + hash + chain integrity remain. Append a **tombstone Step** (who/when/legal basis). Keys are shared with backups/followers/branches so shredding is global. This is a **separate key domain** from secret redaction (D6).

### D6 — Ledger secret protection is defense-in-depth, not dictionary-only (softens overclaim)
Redaction cannot be value-dictionary-only. Add: (1) **pattern + entropy detectors** (`sk-`,`ghp-`,PEM,JWT, high-entropy), (2) **canonicalization** (normalize whitespace/encoding) before matching, (3) **treat all connector/tool results as untrusted** and scan them, (4) accept residual leakage ⇒ **encrypt Ledger at rest + RBAC-gate `ledger read` as an audited high-privilege action + retention limits**. Spec wording changes from "redacted" to "risk-reduced (defense-in-depth)".

### D7 — Ground-truth anchoring is enforced, not conventional (Principle VIII teeth)
`Verdict::Reflect|Retry` is **only constructible from a `GroundTruth` carrying an external-source tag** (tool/test/compiler/linter); the engine rejects reflect/retry verdicts lacking one. **`SelfRefine`** (model self-judgment) is **gated**: forbidden in dev loops where a compiler/test exists; allowed only with an explicit "no external ground truth available — output may degrade" acknowledgment. Every loop node **declares its ground-truth source at authoring time**.

### D8 — Mandatory loop budgets (fixes unbounded `Node::Loop`)
`LoopBudget` is a **mandatory field** of `Node::Loop` and of agent-loop nodes, with an **engine-injected non-optional default**. `until` is a convenience exit; the budget is the guaranteed one. A concrete **no-progress heuristic** is required (e.g. "no reduction in failing-test or compiler-error count across N iterations → `Exit::NoProgress`"); defaults are calibrated after Phase 0 (deferred per completeness policy bucket C).

### D9 — Hexagonal boundary: honest scoping for Phase 0
Phase 0 keeps `SiloStore`/`LedgerStore` concrete (rusqlite) but **behind the `Backend`/`Ledger` traits consumed by `ChatService`/CLI**; adapters move to their own crates when a second implementation appears (Remote backend). The "core depends only on traits" claim is scoped: *the core depends on the port traits; the default implementations ship in-crate for Phase 0 and are extracted at the second backend.* Recorded as a conscious deviation.

### D10 — Networked leader/follower (deferred, but constraints recorded)
Election/lease/quorum and **ledger replication** are unspecified and conflict with "never merge". **Deferred** to Server/Remote scheduling (completeness bucket C), with a recorded requirement: a reconciliation path for offline-Local work is unavoidable, and a leadership hand-off needs ledger replication.

### D11 — Native-loop fallback (stack de-risking)
The functional requirements (LoopController hooks per step, exact per-turn I/O capture, MCP proxying, resumable steps) may exceed what goosed/the embedded crate expose. **Fallback decided now**: if the T000 spike finds no turn-level API, Heddle implements its **own native loop** in the Rust core — direct Gateway (OpenAI-compat) calls + its own MCP client (using the official Rust MCP SDK / Goose's MCP crates as libraries). This is a bounded effort (LiteLLM does provider heavy-lifting; MCP does tool heavy-lifting), not a harness rewrite. Goose then remains a *component source and inspiration* (extensions, recipes format), not the runtime. Positioning: **a core between OpenClaw / Claude Code / Hermes / Goose** — the loop is ours, the ecosystem is reused. The spike's exit criteria: choose `goosed` / `embedded crate` / `native loop` with evidence.

## Phase 0 readiness fixes (bucket B — do now)
- **F1** `sprint-status.yaml` → BMAD-compatible `development_status:` schema (flat map, `epic-1`, `1-1-slug: backlog`, status vocab `backlog/ready-for-dev/in-progress/review/done`) — OR declare Spec-Kit tasks the sole tracker. **Decision**: adopt the BMAD schema (we committed to the bridge).
- **F2** plan001 Constitution Check: add **Principle VIII** and justify the Phase-0 loop-control deferral in **Complexity Tracking** ("Goose owns the loop in Phase 0 pending D1; LoopController lands with FR-16 in Epic 6").
- **F3** Add an **FR crosswalk** in spec001 (its FR-00x → PRD FR-x / AD-x).
- **F4** Fix FR numbering: contiguous PRD ids, reconcile FR-16/FR-017 provenance, add FR-16 to architecture `binds`.
- **F5** One-line deviation note in plan001: research/data-model/quickstart intentionally consolidated into `docs/superpowers/*`.
- **F6** ADR 0001 re-scoped per D1 (spike evaluates goosed/embedded + correlation, not CLI subprocess).

## Status of resolutions
Buckets A/one-way-door decisions (D1–D9) are **accepted and recorded here**; the master spec is updated with pointers. Full re-write of every affected section into prose is itself deferred to each feature's Spec-Kit pass (bucket B) — the ADR is the authoritative record until then.
