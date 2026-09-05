---
title: Heddle PRD Update Addendum — Implementation Candidates and Decision Detail
status: remediation-draft-v2
created: 2026-07-16
updated: 2026-07-16
canonical_prd_unchanged: true
build_authorization: NOT_READY
---

# Heddle PRD Update Addendum

## 1. Purpose

This addendum preserves implementation candidates, mechanisms, rejected alternatives, and evidence-spike detail that should not appear as normative product requirements. It accompanies `PRD.update-draft.md`; it is not architecture approval and does not authorize product implementation. Mandatory engineering-process rules in Sections 9 and 10 are normative only through `docs/PROJECT-GOVERNANCE.md` and `docs/QUALITY-GATES.md`, which take precedence over this addendum.

## 2. Recommended Composition Hypothesis

The current best hypothesis is one product, one repository, one version, one installer experience, and a modular monolith supervised by a Heddle-owned backend. “Single package” does not require one process. Optional language-specific, inference, browser, connector, or media components may run as authenticated supervised processes if they cannot own canonical policy, workflow state, context selection, evidence, or completion.

Recommended language ownership remains:

- **Rust:** long-lived backend, CLI, workflow and loop control, Ledger/evidence boundaries, policy enforcement points, process supervision, local IPC, and privileged cross-platform adapters.
- **TypeScript:** desktop/web presentation, generated clients, and narrowly justified compatibility adapters; never a second policy or workflow source of truth.
- **Python:** optional locked sidecars for provider normalization, retrieval/ML, document/media processing, and other ecosystem-heavy capabilities; never required for the minimal local core.

This remains a hypothesis until the toolchain, package, IPC, lifecycle, and tri-OS spikes pass. Exact versions and frameworks belong in accepted architecture ADRs and locked manifests.

## 3. Reuse Strategy

### Build and own

- canonical ArtifactModel and BMAD–Spec-Kit projections;
- workflow and loop semantics;
- PolicyDecision and approval enforcement;
- Tool/MCP Gateway mediation;
- Ledger, effect model, EvidenceBundle, and traceability;
- ContextManifest and ACL-aware context policy;
- silo/team/project/conversation resolution and security floors;
- CapabilityRegistry and routing policy;
- application protocol and worker conformance contracts.

### Reuse behind replaceable adapters

- official MCP SDKs and protocol transports;
- model provider SDKs and an optional provider-normalization gateway;
- SQLite for local persistence and PostgreSQL where team requirements justify it;
- OpenTelemetry formats and exporters under Heddle privacy/egress policy;
- Playwright and OS-native accessibility/automation APIs;
- established local inference servers and libraries;
- established media, document, audio, video, and 3D engines;
- standard identity, secret, and policy systems where deployment warrants them.

### Candidate posture

- **Goose, OpenCode, Cline, Hermes, Claude Code:** optional workers or sources of compatible libraries and behavioral inspiration; never the control plane.
- **Archon:** workflow concepts and possible import/export compatibility; no direct runtime ownership before mapping evidence and license review.
- **Aider:** repository maps and edit/test-loop inspiration.
- **OpenClaw:** local gateway and assistant UX inspiration; not a team-security foundation.
- **LibreChat/Open WebUI:** UX references; embedding depends on architecture and license review.
- **LangGraph/Temporal:** reference semantics or later optional backends only after Heddle's native workflow contract exists.
- **LiteLLM:** replaceable initial provider adapter candidate, not Heddle's capability/policy model or guaranteed exact-I/O boundary.

Every dependency decision requires a versioned adopt/adapt/inspire/worker/reject record covering license and trademark, maintenance, security history, platform matrix, transitive footprint, offline behavior, egress, resource use, API stability, provenance, and exit strategy.

## 4. Canonical Contracts Required Before Product Implementation

The following artifacts are mechanisms and must be designed outside the PRD. Each requires a versioned schema, positive and negative examples, compatibility rules, and conformance fixtures.

1. **ApplicationProtocol:** async commands/events, streaming, cancellation, backpressure, authentication, errors, approvals, and version negotiation.
2. **SiloContext:** system-derived, unforgeable scope capability required by every store, cache, index, tool, process, telemetry, secret, browser profile, temporary path, backup, and export.
3. **ArtifactModel:** stable IDs, versions, status, relations, provenance, requirement links, and projections to BMAD, Spec-Kit, Markdown, task trackers, and APIs.
4. **LedgerEvent and AuditEvent:** surrogate event identity, ordering, correlation, canonical serialization, integrity, payload protection, retention, migration, and erasure semantics.
5. **EffectProtocol:** intent/applied/failed/ambiguous state, effect classification, idempotency, compensation, approval, and recovery.
6. **WorkflowDefinition and Fold:** node types, deterministic state transitions, parallelism, suspension, resume, cancellation, terminal outcomes, and incompatible-version behavior.
7. **LoopContract:** mandatory budgets, eligible ground truth, action/iteration/terminal verification, no-progress detection, escalation, and external termination.
8. **WorkerAdapter:** bounded-task/turn events, actual model traffic visibility, tool mediation, correlation, approvals, effects, cancellation, budget enforcement, and reduced-assurance profiles.
9. **CapabilityDescriptor:** modality, locality, data classes, destinations, tool/stream/structured-output support, context/resource limits, health, trust, support tier, and conformance evidence.
10. **PolicyDecision:** subject/action/resource/scope/relationship/attributes/environment/risk inputs; deny/allow/obligations output; decision ID and explanation.
11. **ConfigurationResolution:** value source, explicit lock, monotonic security lattice, effective value, provenance, and denial explanation.
12. **ToolGrant/MCPTrust:** server identity, digest/signature, transport, schema pin, requested/granted capabilities, delegated identity, expiry, quota, destination, data/effect class, update, and revocation.
13. **ContextManifest:** immutable source versions, hashes, ACL snapshots, selector/tokenizer/model versions, summaries and lineage, budget, rationale, pinned items, deletion/index freshness, and replay limits.
14. **EvidenceBundle:** requirement, decision, context, model, tool, effect, test, review, approval, artifact, and terminal-verdict links.
15. **SecretHandle:** opaque resolve/use/renew/revoke lifecycle, authorized destination and purpose, lease, injection channel, failure, and audit metadata without value serialization.
16. **ContentEnvelope/BlobReference:** extensible modality, content hash, classification, size, storage, encryption, retention, transformation, provenance, and streaming metadata without implementing future modalities now.

## 5. Blocking Spike Portfolio

Spikes are disposable evidence work, not covert product implementation. Every spike has a fixed time/resource budget, scriptable setup, raw evidence, an adversarial reviewer, an explicit accept/reject/narrow decision, and a cleanup/archive rule.

### SP-001 — Runtime ownership and worker visibility

Compare a minimal Heddle-owned loop with candidate Goose embedded/service surfaces and selected OpenCode/Cline integration paths. Prove actual request/response streaming, tool interception, policy pause, cancellation, correlation, mandatory budgets, and terminal control. Failure means the candidate remains opaque or reduced-assurance; it does not inherit control-plane ownership.

### SP-002 — Workflow compatibility and durability

Map one representative Archon-style workflow losslessly into the canonical graph. Independently crash at each effect boundary and prove deterministic resume or explicit blocked state with no duplicate irreversible effect.

### SP-003 — Context quality and the 1M-window claim

Compare repository map plus hybrid retrieval against full-context loading on small, medium, and large representative repositories. Include code navigation, cross-module change, repair, architecture, security, middle-position recall, unauthorized-content contamination, deletion propagation, latency, token/cost, and retry stability. One million tokens is an overflow test point, not the default.

### SP-004 — Tool/MCP governance

Proxy one local stdio server and one remote OAuth server through identity, policy, grant, approval, schema pinning, destination restriction, prompt-injection defenses, redaction, output classification, quotas, cancellation, and Ledger evidence. Failure removes remote or mutation support from the next release.

### SP-005 — Strict-Local hard egress boundary

Attempt direct sockets, DNS, subprocess network access, remote MCP, model, telemetry, update, discovery, identity, tracker, and secret-provider egress on each declared OS. Prove the versioned local IPC/loopback TCB allowlist and honest platform limitations. Failure marks strict Local unsupported for that platform or capability; it never narrows or weakens the meaning of strict Local.

### SP-006 — Single-package clean-machine bootstrap

On clean Windows, macOS, and Linux environments, clone a pinned commit and run one supported setup command. Verify pinned tool/framework versions, provenance, idempotence, offline first run, optional pack lifecycle, upgrade/rollback/uninstall, and an identical environment report.

### SP-007 — Privacy, Ledger, and erasure

Model single-subject, multi-subject, shared, derived, indexed, backed-up, branched, exported, and legally held data. Prove integrity behavior after selective erasure without retaining plaintext or destroying unrelated subjects' rights. Failure narrows stored payload and replay features.

### SP-008 — Desktop control feasibility (before cowork scheduling)

Prove target window identity, scaling, permission lifecycle, accessibility-first targeting, focus checks, stale-frame handling, multi-monitor behavior, interruption, and post-action verification per OS. Failure narrows platform/scope or defers full-PC control.

## 6. Mode and Deployment Mechanism Constraints and Questions

Architecture must implement the normative backend lifecycle in FR-006: the local backend always remains installed and functional, binds local-only by default, exposes itself only under explicit Server authorization, and is not the active execution backend for a client session attached in Remote mode. Exactly one backend executes for a client session. Detachment requires explicit reactivation or selection; loss never causes implicit fallback, state movement, merge, or replay.

Architecture must still decide mechanisms for:

- connectivity-state detection versus security mode selection;
- local backend process supervision while it stands down for a Remote client session;
- explicit detached, blocked, reactivating, active-local, active-server, and active-remote transitions;
- authenticated discovery and prevention of malicious instance advertisement;
- remote leader loss, reconnect, stale sessions, cached metadata, and split-brain behavior;
- whether Team Alpha excludes all offline remote writes and reconciliation;
- replication, election, leases, quorum, and migration triggers for later releases.

The required Team Alpha baseline is no automatic local/remote merge, no offline remote writes, no leader election, and an explicit blocked/disconnected state until the user chooses another isolated session.

## 7. Security and Governance Mechanism Questions

Required design outputs include system and feature data-flow diagrams, STRIDE-style and privacy threat models, abuse cases, data inventory, authorization matrix, relationship model, identity assurance profiles, MCP trust registry, computer capability taxonomy, incident evidence model, and control-to-requirement-to-test mappings.

Candidate policy implementation may be an embedded evaluator or a standard engine behind a Heddle-owned schema. Simple roles remain an administrative UX; enforcement must include attributes, relationships, environment, purpose, destination, data class, assurance, and risk.

Secrets should be resolved through native provider adapters or tightly governed tool boundaries. The pre-implementation decision gate must select a default local root of trust, a reliable open-source or no-cost baseline, optional commercial providers such as 1Password CLI/service accounts, and recovery behavior; OpenBao and operating-system keychains are candidates, not accepted choices. Values should be injected directly into target transports/processes where possible, never returned to model-visible context. Provider profiles must define bootstrap ceremony, circular dependencies, leases, renewal, revocation, rotation, backup/restore, break-glass recovery, crash behavior, child-process propagation, offline failure, and credentials for MCP, identity, model, and secret-provider adapters.

Tool/MCP mechanisms must encode owner and delegation authority at Silo, Team, Project, and Conversation scope; inherited denials, explicit locks, monotonic security floors, separately resolved read/mutate grants, descendant revocation, and cross-surface consistency. Discovery and bundling never activate a connector. The base profile is disabled except for a registry-declared least-privilege local-only set.

Computer-control mechanisms must begin at project scope and model folder, application/window, screen, and full-computer widening as independent time-bounded grants. Keyboard, mouse, capture, clipboard, browser, and accessibility primitives remain separately authorizable.

## 8A. Context, RAG, Memory, and Resource Mechanisms

The context architecture must preserve ACL-aware multimodal ingestion, source versioning, tombstone/deletion propagation, hybrid lexical/vector retrieval, reranking, graph/temporal/code-symbol indexes, and separate session, personal, project, team, and organizational memory scopes. ACL evaluation applies before ingestion and retrieval and again before transformation/output. Memory promotion is explicit, attributable, revocable, and retention-governed.

Local omni scheduling requires a resource broker that inventories CPU, GPU, RAM, VRAM, storage, model residency, load latency, eviction constraints, concurrency, and health. It must reserve interactive/cancellation/safety capacity ahead of batch work, expose queue/degradation decisions, and never use remote capacity as an implicit fallback.

## 8. Compliance-Support Design Work

The product should generate evidence usable by an organization's governance program, but formal mappings must avoid legal conclusions. Required mappings include:

- GDPR processing purposes, minimization, rights, retention, security, processors/subprocessors, transfer records, breach support, and DPIA triggers;
- EU AI Act role/use-case inventory, prohibited/high-risk gating, transparency, human oversight, logs, literacy support, monitoring, and deployer obligations;
- ISO/IEC 27001 control ownership and evidence references within an organizational ISMS;
- SOC 2 trust-services control description, evidence cadence, and operating-effectiveness ownership;
- NIS2 risk management, vulnerability handling, supplier assurance, incident/continuity evidence, and management responsibility.

Qualified legal, privacy, security, and audit review is external to automated completion. Product documentation must never use certification badges or unconditional compliance language without an actual scoped organizational certification or attestation.

## 9. Development Environment and CI/CD Substrate

This section is a normative planning/substrate obligation through `docs/PROJECT-GOVERNANCE.md` and `docs/QUALITY-GATES.md`; it does not authorize product implementation. Environment automation, lockfiles, fakes, quality configuration, CI/staging definitions, and evidence scripts are permitted before build authorization only when they contain no product runtime behavior and are linked to a named gate.

Before the first product source implementation commit, the repository needs tested from-scratch preparation for contributors and agents:

- pinned Rust toolchain and target policy;
- pinned Node/package-manager choice and frontend quality stack if UI work is in scope;
- pinned Python minor version and hashed lock environments for optional sidecars;
- pinned BMAD, Spec-Kit, power-skill, and loop-engineering/conformance tooling;
- portable MCP development server setup and connection verification independent of proprietary CLIs;
- formatting, linting, type checking, documentation, unit, integration, contract, E2E, mutation/property, security, and performance tooling;
- hermetic fakes for models, MCP, identity, secrets, trackers, and external effects;
- tri-OS CI, clean-install and idempotent-bootstrap tests;
- SAST, dependency audit, license policy, secret scanning, SBOM, provenance, signing, and reproducible-build checks;
- staged promotion with immutable artifacts, checksums, compatibility metadata, migration, rollback, and uninstall evidence.

The bootstrap should expose separate profiles for minimal planning/core, contributor development, local inference, enterprise connectors, and multimodal packs. It must not require cloud credentials or Python/Node at end-user runtime unless the selected capability pack declares them.

## 10. Future Implementation Swarm Harness

No implementation, review, testing, contradiction, validation, integration, or staging swarm against product code may start until all gates in `docs/QUALITY-GATES.md` pass and explicit authorization changes from `NOT_READY`. Once authorized, each task must carry:

- approved BMAD epic/story and complete Spec-Kit feature/task references;
- isolated worktree and exclusive file ownership;
- allowed tools, destinations, credentials, and effect class;
- a reproducible ContextManifest and explicit context/token budget;
- iteration, wall-time, token/cost, and no-progress budgets;
- action, iteration, and terminal external test oracles;
- rollback/recovery procedure and expected EvidenceBundle;
- stop conditions for contradiction, authority ambiguity, security-boundary change, or budget breach.

Author, reviewer, adversarial challenger, test/evaluation agent, integration owner, and final human approver are distinct roles. Agents sharing the same model/prompt lineage are not independent evidence. Integration is serialized through a gated queue. Staging begins only after terminal evidence, traceability, regression, security, isolation, recovery, and provenance checks pass.

## 11. Known Superseded or Unsafe Planning Material

Existing Phase 0 plans that encode a Goose CLI subprocess, hash-as-primary-key Ledger identity, waived foundational contracts, or obsolete requirement mappings must remain blocked and excluded from implementation-agent retrieval. Warning banners alone are insufficient. They should later be regenerated or moved to a non-executable archive after the canonical PRD and architecture are accepted.

ADR-0002 is accepted direction but does not replace the missing normative schemas and fixtures. ADR-0003 remains Proposed until its evidence conditions pass. The Design Completeness Policy remains valid only with the corrected interpretation that every one-way-door contract—not merely a prose decision—is complete before product persistence or security-boundary implementation begins.
