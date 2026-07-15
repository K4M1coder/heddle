# Skein Adversarial Architecture Review

**Gate:** Pre-implementation BMAD architecture readiness  
**Date:** 2026-07-16  
**Reviewer posture:** Adversarial; evidence over intent  
**Verdict:** **FAIL — NOT READY FOR IMPLEMENTATION**

## Executive assessment

The architectural direction is credible: a Skein-owned Rust control plane, ports and adapters, a governed MCP boundary, an event-sourced execution record, and replaceable workers are appropriate for the stated product. The current artifact set does not yet turn that direction into an implementable architecture.

The gate fails because several acknowledged one-way doors remain contradictory, deferred, or specified only as prose corrections in ADR-0002. The executable Phase 0 plan still contains designs explicitly rejected by later ADRs. Server/Remote behavior is included in the MVP while its consistency, failover, identity, and replication model are deferred. The Ledger is expected to provide exact capture, replay, reversibility, privacy erasure, workflow recovery, and audit evidence, but no canonical event schema, fold, effect protocol, compatibility policy, or executable examples exist.

No autonomous implementation swarm should be launched. Spikes may be run only as bounded architecture experiments that produce evidence and make no product-code commitments.

## Severity model

- **Critical:** permits an unsafe or fundamentally incompatible implementation, or leaves a one-way door undecided.
- **High:** blocks reliable implementation of a major subsystem or creates a likely security/isolation failure.
- **Medium:** material ambiguity or maintainability risk that must be resolved in the relevant feature plan.
- **Low:** clarity, naming, or documentation debt that should be fixed before the gate is rerun.

## Critical findings

### C-01 — The canonical Ledger contract does not exist

**Evidence:** The design still presents `StepId` as a content hash and a minimal `Step`; ADR-0002 D2 replaces this with a surrogate identifier, integrity hash, correlation fields, effect classes, idempotency, two-phase effects, and loop events. Feature 001 still declares `Step {id(hash), parent, seq...}` and its superseded plan implements a hash as the primary key. Architecture AD-5 states only “append-only, hash-chained.”

**Why this blocks implementation:** The Ledger is the persistence root for sessions, workflows, replay, audit evidence, loop state, privacy erasure, and branching. Implementing any persisted schema before the canonical event contract is fixed creates the exact migration risk the completeness policy classifies as a one-way door. A prose ADR cannot substitute for a versioned schema and deterministic fold.

**Required fix:** Before Phase 0 planning, create a normative Ledger design package containing:

1. versioned event envelope and payload schemas;
2. identity, parentage, sequence, causation, correlation, tenant/silo and trace fields;
3. append/concurrency rules and transaction boundaries;
4. `Intent -> Applied|Failed|Unknown` effect state machine;
5. idempotency and replay policy by effect class;
6. branch semantics and recorded-result replay semantics;
7. deterministic folds for session, workflow, loop, approval, budget, and effect state;
8. encryption/blob-reference/retention/crypto-shredding rules;
9. golden fixtures proving repeat events, concurrent branches, crash between intent and effect, replay, schema upgrade, and erasure.

**Gate evidence:** reviewed schema, data model, state-transition diagrams, compatibility policy, and executable contract-test vectors.

### C-02 — Local/Server/Remote mode semantics contradict the isolation promise

**Evidence:** The backend is described as “always functional,” but in Remote mode it is “on standby” and only one backend is active. Loss of the leader triggers automatic Local fallback, while sessions may never cross or merge between modes. ADR-0002 D10 acknowledges that reconciliation and ledger replication are unavoidable but defers them. The MVP nevertheless includes baseline Server/Remote and team authorization.

**Why this blocks implementation:** The design does not define what happens to the active conversation when a remote leader disappears, whether a fallback creates an empty Local session, whether queued effects continue, how an instance later returns to Remote, or what “standby” means for an always-functional backend. Silent fallback can make data appear lost; automatic continuation can violate the no-sharing rule; replication can violate silo isolation.

**Required fix:** Define an explicit mode state machine and data ownership contract. At minimum:

- distinguish connectivity state from selected data silo;
- never switch a running session’s silo implicitly;
- on leader loss, pause the Remote run and offer either reconnect or start a new Local branch with an explicit, policy-governed export/import boundary;
- define whether Remote data is cached, and if so its encryption, ACL, expiry, and offline behavior;
- define leader authority, leases, split-brain handling, request idempotency, and effect ownership;
- either specify replication/reconciliation before MVP Remote implementation or remove Server/Remote from v1 and keep only a non-persistent connectivity spike.

**Gate evidence:** state diagram, failure matrix, sequence diagrams for connect/disconnect/fallback/reconnect, and isolation tests covering two teams and two modes.

### C-03 — Egress OFF is an invariant without a cross-platform enforcement architecture

**Evidence:** ADR-0002 D4 requires process socket denial with a loopback allowlist. The architecture merely checks `requires_network()` and names a “network sandbox.” No mechanism is selected for Windows, macOS, or Linux. Local components such as LiteLLM, Ollama, MCP servers, telemetry exporters, package managers, browsers, IdPs, secret providers, and workers may initiate their own network traffic.

**Why this blocks implementation:** Adapter metadata is not a security boundary. A compromised, misclassified, or simply auto-updating component can egress. The product makes a strong “no network output” claim that cannot be tested or guaranteed by the current design.

**Required fix:** Produce an egress threat model and one bounded spike per OS. Define the trusted computing base, process topology, DNS handling, loopback rules, child-process inheritance, IPv4/IPv6 behavior, proxy bypass prevention, and observable proof. If application-level enforcement is the only portable MVP option, narrow the product claim explicitly and expose a “strict offline sandbox unavailable” status rather than claiming hard isolation.

**Gate evidence:** adversarial tests in tri-OS CI showing blocked direct sockets, DNS, spawned-child egress, telemetry export, and undeclared MCP traffic.

### C-04 — Runtime ownership is decided in principle but not in contract

**Evidence:** ADR-0003 is still Proposed and requires five spikes. Feature 001 requires Goose in FR-002 while also stating that the runtime path is undecided. The task index still names `GooseRuntime` and a binary stub. The exhaustive plan contains a rejected batch subprocess implementation. `AgentRuntime`, `WorkerAdapter`, and `CapabilityDescriptor` are not normatively defined.

**Why this blocks implementation:** Different workers expose different granularity, cancellation, approvals, tool mediation, context ownership, and model I/O. A weak adapter will either leak control back to the worker or falsely claim exact evidence and termination.

**Required fix:** Replace product-specific Phase 0 requirements with a versioned governed-execution contract. Define bounded invocation, event ordering, backpressure, cancellation acknowledgement, context ownership, model-call visibility, tool-call mediation, approval suspension, budget accounting, effect identity, error taxonomy, and capability negotiation. Run the ADR-0003 spike matrix and accept one Phase 0 path. Regenerate all Phase 0 artifacts afterward; archive the superseded code snippets so they cannot be executed accidentally.

**Gate evidence:** accepted ADR-0003, adapter protocol examples, conformance tests, and spike evidence for native Rust plus at least the serious reuse candidates.

### C-05 — Workflow recovery is incorrectly treated as “free” from event sourcing

**Evidence:** The design says durability, replay, and crash recovery are obtained “for free” by logging nodes. It does not define scheduling decisions, deterministic expression evaluation, parallel join semantics, retries, timer behavior, external effect uncertainty, approval leases, cancellation, compensation, or versioning of a running workflow. Optional Temporal/Windmill backends are named despite materially different determinism and execution models.

**Why this blocks implementation:** An event log alone does not make an execution durable. A crash after an external action but before `Applied`, or a workflow-definition change during suspension, can duplicate effects or make the run unreconstructable.

**Required fix:** Specify the workflow execution semantics independently of any backend: node lifecycle, scheduler decisions as events, deterministic fold, expression language, parallel failure policy, cancellation propagation, timers, approvals, effect uncertainty resolution, definition snapshot/version, migration policy, and terminal states. Make optional backends conform to this contract rather than pretending the trait makes them equivalent.

**Gate evidence:** formal state machines and scenario fixtures for crash points, parallel branches, approvals, cancellation, definition upgrades, and uncertain external effects.

### C-06 — GDPR erasure, exact historical inspection, and immutable evidence are not reconciled at the data model level

**Evidence:** ADR-0002 proposes per-data-subject crypto-shredding while the product promises exact model input/output inspection and replay. A single prompt may contain data about multiple subjects, project secrets, and shared team context. Keys are said to be shared with backups/followers/branches, but subject extraction, key envelopes, derived artifacts, embeddings, summaries, and external provider copies are unspecified.

**Why this blocks implementation:** One payload cannot be selectively erased by destroying one subject key unless payload segmentation and key wrapping were designed before the first record. Destroying a shared payload key may erase other subjects’ evidence. Retaining plaintext in indexes, traces, blobs, or worker caches defeats shredding.

**Required fix:** Define data classification, payload segmentation, envelope encryption, subject-to-object indexing, derived-data lineage, deletion propagation, legal-hold exceptions, backup/key replication, and proof-of-erasure. State clearly that external provider retention is controlled by provider contracts and cannot be undone by Skein.

**Gate evidence:** privacy data-flow map and erasure test covering Ledger payload, blob, context manifest, index, summary, trace, backup, branch, and remote cache.

## High findings

### H-01 — The API/CLI/UI layering is internally inconsistent

**Evidence:** The constitution says the API exposes everything and the CLI is its authoritative complete client. Architecture diagrams make CLI and API peers over the core. AD-1 says the UI “only emits CLI/API commands,” while the UI section says it emits CLI/API commands through a single surface. No protocol or local security model is selected.

**Risk:** Three subtly different application surfaces and duplicated orchestration logic; unsafe unauthenticated localhost HTTP; UI behavior that cannot be reproduced through the CLI.

**Fix:** Establish one versioned application protocol owned by the headless service. CLI and UI are independent clients of that protocol; the UI never shells out to the CLI. Define transport profiles (in-process/local IPC/authenticated network), streaming, cancellation, errors, compatibility, authentication, CSRF/origin protections, and discovery. Generate or contract-test both clients against the same schema.

### H-02 — Silo isolation is reduced to namespace selection instead of a capability boundary

**Evidence:** AD-2 uses `Backend.store(mode, team)` and Phase 0 tests two namespace strings in one SQLite database. The design also includes files, blobs, keychains, indexes, telemetry, worker directories, model caches, browser profiles, task trackers, and temporary files.

**Risk:** Cross-silo leakage outside the relational store; confused-deputy bugs from caller-supplied mode/team identifiers.

**Fix:** Introduce an unforgeable `SiloContext`/capability established after authentication and mode selection. Every storage, tool, context, worker, telemetry, secret, cache, and filesystem operation must require it. Prefer separate encryption domains and roots over table namespaces. Build a resource inventory and negative isolation suite across all persistence and side effects.

### H-03 — MCP governance is a policy aspiration, not an architecture

**Evidence:** Connectors are described as embedded MCP servers from a trust registry, with hierarchical enablement and a Skein MCP proxy. Missing are server identity, transport trust, schema pinning, capability grants, tool-level authorization, OAuth token audience, delegated-user identity, output classification, quotas, timeouts, cancellation, prompt-injection handling, and update/revocation behavior.

**Risk:** Enabling one connector grants excessive authority; remote servers can change schemas or exfiltrate data; tool outputs contaminate model context; secrets enter model-visible arguments.

**Fix:** Define a Tool/MCP Gateway contract with separate install, trust, enable, grant, invoke, and revoke states. Authorization must be per principal, silo, tool, action, resource scope, data class, effect class, destination, and time. Pin server identity and schema versions; mediate OAuth; redact before both model return and persistence; log policy decisions and evidence; reject secret resolution through model-visible calls by default.

### H-04 — Hierarchical configuration semantics remain contradictory

**Evidence:** ADR-0002 D3 separates a value from a lock and defines most-specific-wins. Spec 002 says “a setting fixed at one level locking the lower levels,” and its Jira scenario treats merely setting Jira at silo scope as a lock. The design alternates between a two-layer Team/Local model and a four-level Silo/Team/Project/Conversation model. “Higher” is used ambiguously for hierarchy and strictness.

**Risk:** Authorization bypass, surprising inheritance, and irreproducible harness behavior.

**Fix:** Replace prose with typed configuration algebra: source scope, value, explicit lock, security lattice, effective decision, provenance, and denial reason. Define local overrides in Remote mode without crossing silos—prefer per-user preferences stored inside the Remote team partition or explicitly exclude them. Add truth-table examples and property-based tests. Correct Spec 002 acceptance scenarios.

### H-05 — Exact model I/O capture has no trustworthy chokepoint

**Evidence:** The design calls LiteLLM the universal capture point, but workers may call models internally, providers may stream, retries/fallbacks may transform requests, and local inference endpoints may be called directly. The Phase 0 plan proposes application-level prompt/response logging and later LiteLLM JSONL ingestion, which is not equivalent to exact wire-level capture.

**Risk:** The Ledger may show reconstructed rather than actual requests, omit provider retries or tool schemas, duplicate streams, or persist secrets and credentials.

**Fix:** Require all governed model traffic through a Skein-owned model mediation boundary or mark the worker non-conformant. Define canonical request, transformed provider request, streamed raw response, usage, retries, routing decision, redaction stage, and hashes as distinct evidence events. Explicitly exclude authorization headers and secret values from “exact I/O.”

### H-06 — Context management lacks lifecycle, ACL, and reproducibility semantics

**Evidence:** `ContextManifest` records hashes and rationale, but no canonical schema, selector version, tokenizer/model identity, summary lineage, index freshness, ACL snapshot, deletion propagation, retrieval evaluation, or reproducibility behavior is defined.

**Risk:** A replay cannot reconstruct context; stale or unauthorized chunks influence a model; compressed trajectories hide decisive evidence; long-context benchmarks optimize the wrong metric.

**Fix:** Specify the manifest and context pipeline: immutable source versions, authorization before and after retrieval, tokenizer/model versions, deterministic selection inputs, summary provenance, cache keying, index tombstones, budget reservation, and redaction. Define benchmark tasks and metrics for answer quality, recall, middle-position degradation, latency, cost, and leakage. Treat the one-million-token limit as capacity, never as a repository-size acceptance criterion.

### H-07 — Desktop control is underdesigned for the claimed safety boundary

**Evidence:** `Controller` has only screenshot/click/type methods and relies on `enigo`/`xcap` assertions. It does not model windows, display scaling, accessibility trees, focus, coordinate transforms, secure desktops, permission revocation, keyboard layouts, clipboard, multi-monitor behavior, Wayland portals, or user interruption.

**Risk:** Actions target the wrong window or user, credentials leak through screenshots/clipboard, and “FullComputer” becomes an effectively unbounded capability.

**Fix:** Split perception, targeting, and action ports; use typed window/display/frame identities and freshness checks; prefer accessibility/DOM semantics before coordinates; require foreground/focus verification, action previews, emergency stop, scoped sessions, and post-action visual confirmation. Produce OS-specific threat models and feasibility spikes before freezing a common trait.

### H-08 — Local inference packaging is a list of products, not a deployment design

**Evidence:** Ollama, llama.cpp, vLLM, LM Studio and LiteLLM are all named. “Embedded” alternately means bundled, supervised, or externally installed. vLLM is acknowledged as unsuitable for much of the tri-OS default. Model acquisition, licenses, checksums, disk/VRAM requirements, process lifecycle, port conflicts, offline bootstrap, and upgrades are undefined.

**Risk:** The one-installer/local-first promise fails on fresh machines, or the distribution silently downloads large/licensed components.

**Fix:** Define capability tiers: built-in minimal engine, detected external engines, and optional managed components. Select one Phase 0 local path through a packaging spike. Specify manifests, signatures/checksums, model licenses, hardware detection, resource scheduling, ports, health, upgrades, rollback, offline media, and uninstall/data retention.

### H-09 — The modular boundaries omit first-class security and orchestration components

**Evidence:** Architecture ports name runtime, gateway, backend, identity, secrets, controller, and Ledger, but not a Policy Decision Point, Tool Gateway, Context Manager, Workflow Scheduler, Approval service, Capability Registry, Evidence service, or cryptographic key manager. Some later appear only in capability maps or prose.

**Risk:** Policy and correlation logic spreads through adapters, making deny-by-default and evidence capture unenforceable.

**Fix:** Produce a component model and dependency rules for the control plane. Distinguish pure domain decisions from effectful adapters. Make policy, capability resolution, context selection, tool mediation, workflow scheduling, approvals, evidence, and key management explicit boundaries with owned state and contracts.

### H-10 — MVP scope defeats the walking-skeleton risk strategy

**Evidence:** v1 includes code assistant, multiple providers, inference packaging, Atlassian/M365, three methodologies, workflow engine, trackers, hierarchy, three modes, team authorization, UI, identity, RBAC, observability, Ledger, secrets, and compliance controls. Epics beyond Epic 1 are not decomposed, yet the PRD success metric requires the entire cross-enterprise journey.

**Risk:** Integration begins before foundational contracts stabilize; quality gates become ceremonial; a solo/open-source project cannot obtain meaningful acceptance feedback soon enough.

**Fix:** Define staged releasable slices. Recommended order: (0) architecture/evidence spikes; (1) strict-local CLI/API native loop with governed filesystem and canonical Ledger; (2) workflow/loop durability; (3) UI client; (4) one remote provider and one remote MCP connector; (5) team mode and identity. Keep the complete vision, but do not call all of it one MVP.

## Medium findings

### M-01 — Architecture metadata and requirement coverage are stale

The architecture front matter binds only through FR-16 although FR-17 and FR-18 are mapped later. PRD numbering is non-sequential, while Spec 002 invents FR-013a/b and maps loop control to FR-017 although the PRD uses FR-16. Fix one canonical requirement registry with stable IDs and generated traceability.

### M-02 — Security uses “RBAC” for a model that is actually RBAC + ABAC + ReBAC + risk policy

Tool scopes, data classification, hierarchy, effect class, destination, and relationship to a team/project cannot be represented safely by roles alone. Define a policy input/output schema and decide whether the implementation is an embedded policy engine or a Skein evaluator. Preserve simple role administration as UX, not as the full authorization model.

### M-03 — Audit and Ledger separation is not operationally defined

The documents call them complementary but do not define shared correlation, independent retention, immutability guarantees, access roles, clock source, signing/anchoring, or what remains after crypto-shredding. Create an evidence model and explicitly state which claims each record can prove.

### M-04 — “Reversible” is overused

Git-style terminology implies stronger guarantees than the design can provide for emails, tickets, deployments, desktop actions, and provider calls. Rename the property to `replayable history` plus effect-specific `reversible`, `compensatable`, or `irreversible`; require compensations to be separately authorized actions.

### M-05 — Async and streaming are absent from the core interfaces

The Rust examples use synchronous methods returning values despite streaming models, long tools, cancellations, backpressure, approvals, real-time events, and network calls. Do not freeze these snippets as contracts. Define async protocol semantics before selecting Rust trait shapes.

### M-06 — Optional workflow backends are premature

Temporal and Windmill are not interchangeable adapters to a generic `WorkflowEngine`; each imposes execution, determinism, worker, and deployment constraints. Keep them as research candidates until the native semantic contract exists and a real scaling requirement appears.

### M-07 — Vikunja is described as embedded without deployment evidence

An API-first server is not necessarily embeddable in a single desktop package. Compare a truly embedded local tracker with a supervised Vikunja service and Jira adapter. The local tracker should be sufficient for the default installation.

### M-08 — Supply-chain ownership and licensing are incomplete

The trust registry is undefined, and “embedded connectors” may carry incompatible update, trademark, transitive-license, or runtime requirements. Require SBOM, provenance, signature verification, license policy, vulnerability response, connector isolation, and revocation before bundling.

### M-09 — Observability may violate the same privacy and egress controls it is intended to prove

Define semantic conventions with default content suppression, trace/metric cardinality limits, local buffering, per-silo correlation, exporter policy, and erasure/retention behavior. Never put prompts, tool arguments, secrets, or personal data into ordinary OTel attributes.

### M-10 — The real-time and multimodal roadmap crosses core boundaries earlier than claimed

Typed multimodal content affects storage, hashing, redaction, quotas, streaming, tool schemas, API compatibility, and blob lifecycle. Real-time voice is not the only later change to the execution model. Freeze an extensible envelope/blob reference in the initial Ledger/API contract without implementing modalities.

### M-11 — The 1M-token discussion is directionally correct but not an architecture budget

Source-token size does not predict maintainability, build time, test cost, or context effectiveness. Establish module size/dependency budgets, generated/vendor exclusions, retrieval evals, and ownership boundaries instead of a repository-token ceiling.

### M-12 — Performance and resource goals are missing

Phase 0 declares performance N/A, but responsiveness, startup time, idle memory, Ledger growth, stream latency, and cancellation are architecture-shaping for a desktop tool. Add measurable budgets even if generous.

## Process and artifact integrity findings

### P-01 — The BMAD gate cannot pass with `architecture.md` still marked Draft

ADR-0003 explicitly requires architecture readiness and Spec-Kit analysis. Neither evidence set exists. The architecture must remain Draft, and implementation must remain blocked, until the critical findings are resolved and the spikes are complete.

### P-02 — The BMAD-to-Spec-Kit bridge is descriptive, not operationally complete

The methodology documents a mapping, but there is no validated PRD report, implementation-readiness report, architecture checklist, feature checklist, clarification record, Spec-Kit analysis report, or per-story BMAD artifact. Claiming conformity before these outputs exist is unsupported.

**Fix:** Run the installed official workflows in order and preserve their outputs. A bridge must carry stable requirement IDs, decisions, acceptance criteria, dependencies, and gate status—not merely copy prose between folders.

### P-03 — The current Phase 0 Spec-Kit plan deliberately waives required design artifacts at the exact point they are needed

`research.md`, `data-model.md`, `contracts/`, and `quickstart.md` are consolidated/waived, yet the missing canonical data model and contracts are the main blockers. The exhaustive plan contains obsolete code, so it cannot safely serve as their substitute.

**Fix:** Regenerate feature 001 with explicit research, canonical data model, API/event contracts, quickstart, checklists, and analysis. Do not waive them for a foundational vertical slice.

### P-04 — Superseded implementation snippets remain dangerously executable

The exhaustive plan contains complete code for the rejected Goose subprocess and hash-primary-key Ledger. Warning banners are insufficient for autonomous agents that retrieve snippets by relevance.

**Fix:** Move obsolete examples to a clearly non-executable archive or replace code bodies with references to the decisions that superseded them. Exclude archived plans from agent implementation context by policy.

## Decisions required before implementation

The following decisions must be recorded as accepted ADRs or normative contracts:

1. Canonical Ledger/event/effect/encryption schema and compatibility policy.
2. Headless service protocol and local/network transport security.
3. `WorkerAdapter` governed-execution contract and selected Phase 0 runtime.
4. Tool/MCP Gateway trust, authorization, identity delegation, and redaction contract.
5. Cross-platform Local egress guarantee or a narrowed, honest claim.
6. Silo capability model covering every store, cache, process, tool, trace, and filesystem path.
7. Local/Server/Remote state machine and the v1 decision on replication/reconciliation.
8. Workflow scheduler semantics, deterministic folds, and effect recovery.
9. Hierarchical configuration algebra and policy model.
10. Context manifest/retrieval/ACL/deletion lifecycle.
11. Phase 0 inference packaging path and optional-component lifecycle.
12. Privacy erasure model across primary and derived data.

## Required architecture spikes

Spikes are permitted before product implementation only if time-boxed, isolated, and discarded or promoted through review. Each must produce raw evidence, a decision, and a reproducible script/test.

| Spike | Required proof | Failure outcome |
|---|---|---|
| Runtime ownership | turn-level model/tool events, correlation, cancellation, approval pause, enforced budget | use native Rust loop; candidate remains optional opaque worker |
| MCP governance | local stdio + remote OAuth server through policy, scope, approval, redaction, Ledger | do not ship remote connector support |
| Egress boundary | tri-OS blocked socket/DNS/child traffic with loopback policy | narrow offline guarantee and expose limitation |
| Workflow durability | crash at every effect boundary; no duplicate action; deterministic resume | redesign event/effect protocol |
| Context strategy | retrieval vs full context, ACL leakage, middle-position cases, reproducibility | revise selector and benchmark before integration |
| Package bootstrap | clean Windows/macOS/Linux install and offline first run | split optional components or change default stack |
| Desktop control | window identity, scaling, permissions, interruption and post-action verification on three OSes | defer local full-PC control or use narrower backends |

## Minimum quality gates for a rerun

The architecture gate may be rerun only when all items below are present:

- [ ] PRD validation report with corrected stable requirement IDs.
- [ ] Accepted architecture ADRs for every decision listed above.
- [ ] Canonical component and deployment diagrams for Local, Server, and Remote.
- [ ] Threat models for egress, MCP/tool execution, desktop control, identity, and secrets.
- [ ] Canonical Ledger/data model plus golden replay/erasure fixtures.
- [ ] Versioned application, worker, workflow, tool, and context contracts with examples.
- [ ] Completed ADR-0003 evidence bundles and accepted decision.
- [ ] Regenerated Epic 1 stories with no stale Goose-specific requirement.
- [ ] Regenerated Feature 001 `research.md`, `data-model.md`, `contracts/`, `quickstart.md`, plan, tasks, and checklists.
- [ ] Spec-Kit clarification and analysis reports with zero unresolved critical findings.
- [ ] BMAD implementation-readiness report passing architecture, epics/stories, traceability, testability, and sequencing.
- [ ] Explicit loop-engineering budgets for the future implementation swarm: iteration, token/cost/time, no-progress rule, ground-truth oracle, escalation owner, and terminal acceptance suite.

## Recommended corrected dependency order

```text
Constitution and canonical terminology
  -> requirement registry and traceability
  -> threat models and one-way-door contracts
  -> bounded architecture spikes
  -> accepted architecture and ADR-0003
  -> regenerated BMAD epics/stories
  -> Spec-Kit clarify/research/data-model/contracts/plan
  -> checklists and cross-artifact analysis
  -> BMAD implementation-readiness gate
  -> only then: bounded implementation agents
  -> independent review/test/contradiction agents
  -> staging after terminal evidence passes
```

## Final verdict

**FAIL.** Skein has a strong vision and several sound principles, but it is not architecture-ready. The highest-risk contracts are still distributed across contradictory artifacts, and the current executable plan encodes rejected decisions. Implementation now would create irreversible persistence, isolation, and governance debt.

The correct next action is to close the twelve decisions above, run the bounded evidence spikes, regenerate the BMAD and Spec-Kit artifacts, and rerun this gate. Product source implementation and autonomous implementation swarms remain prohibited until that rerun passes.
