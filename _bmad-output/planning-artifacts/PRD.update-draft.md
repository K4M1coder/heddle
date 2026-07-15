---
title: Skein Product Requirements Document — Update Draft
status: remediation-draft-v2
created: 2026-07-16
updated: 2026-07-16
canonical_prd_unchanged: true
build_authorization: NOT_READY
---

# Skein Product Requirements Document — Update Draft

## 1. Purpose and Status

This draft remediates the critical and high findings raised against the canonical Skein PRD. It preserves the long-term vision while defining honest release hypotheses, observable product outcomes, risk journeys, assumptions, and the evidence required before implementation may begin.

This file is not the canonical PRD and does not authorize product implementation. The canonical PRD remains `_bmad-output/planning-artifacts/PRD.md` until this draft is reviewed, reconciled, and deliberately promoted through the BMAD PRD update workflow.

**Current decision: NOT READY.** Product implementation, implementation-agent swarms, and production-facing connector work remain blocked until the Build Authorization Gate in Section 11 passes. Time-boxed design and evidence spikes are permitted only when they cannot become production code by accident and produce reproducible evidence for a named blocking decision.

## 2. Product Vision

Skein is an independent open-source, local-first agentic work platform created by Cédric Thedrez (`kamicoder` on GitHub and `cethgame` elsewhere). It presents one coherent experience across chat, software development, and computer-assisted work while retaining user and team control over models, tools, workflows, context, permissions, evidence, and data location.

Skein's differentiation is not a claim to contain every model or replace every existing tool. It is a governed control plane that can compose supported local and remote capabilities behind versioned contracts. A capability is supported only when its version, platform, modality, trust level, limitations, and conformance evidence appear in the release's capability registry.

The versioned headless `ApplicationProtocol` is the normative application boundary and source of client-visible semantics. The CLI is its complete reference client and end-to-end conformance oracle. Graphical interfaces are non-privileged peer clients of the protocol, never shell through the CLI, and may not create behavior unavailable to other authorized protocol clients.

The long-term experience may appear “omni” to users by coordinating multiple specialized models and tools in parallel or sequence. It does not require one universal model and must not conceal which providers, tools, data, or transformations contributed to an outcome.

## 3. Product Principles and Boundaries

1. **Strict Local by default.** A fresh base installation provides useful local behavior without a cloud account and enforces hard no-egress. Only versioned, explicitly enumerated local IPC and loopback members of the trusted computing base are eligible exceptions. If a platform cannot prove this boundary, strict Local is unsupported there; the invariant is never weakened into a policy-only promise.
2. **One control plane.** Skein owns canonical workflow state, loop termination, policy decisions, approvals, context manifests, evidence, and completion verdicts. External agents may be bounded workers, never implicit authorities.
3. **Isolation before sharing.** Local, Server, and Remote sessions use separate silos. No information crosses mode sessions. Remote sharing is restricted to authorized members of the same team.
4. **No agent self-authorization.** Authorization and completion are decided outside model output.
5. **Replayable history, qualified effects.** Inputs, outputs, decisions, and observations are inspectable and revisionable. External effects are separately classified as replay-safe, reversible, compensatable, or irreversible; Skein never promises universal reversal.
6. **Smallest-sufficient context.** A million-token window is overflow capacity, not normal working memory and not a repository-size limit. Each model call receives a reproducible, policy-filtered context selected for the task.
7. **Evidence over confidence.** Agent loops are bounded and checked against external ground truth at action, iteration, and terminal levels.
8. **Compliance support, not certification.** Skein provides configurable controls and evidence that can support organizational compliance. The software does not certify an organization or guarantee that a deployment is compliant.
9. **Progressive capability.** High-risk and enterprise capabilities ship only after their own threat model, permissions, negative tests, packaging proof, and rollback path pass.

## 4. Personas and Risk Journeys

### P-01 — Alex, independent developer

Alex develops on one workstation, often offline, and wants an agent that can understand a repository, edit within an approved project, run tests, and preserve a complete history without silently contacting cloud services.

**UJ-01 — Offline first run.** Alex installs Skein on a clean workstation, starts in Local mode, selects a project directory, chooses an available local model, and completes a read-only repository explanation without creating a cloud account. If no compatible local model is available, Skein explains the missing capability and offers setup guidance; it does not silently switch to a remote provider.

**UJ-02 — Governed code change and recovery.** Alex requests a change. Skein shows the planned scope and permissions, performs bounded edits and tests, records model/tool evidence, then survives an interruption and resumes without duplicating a prior effect. If tests do not improve within the loop budget, the run stops with a diagnostic and requests human direction.

### P-02 — Maya, project lead

Maya manages a small team and owns project-level workflows, approved tools, quality gates, and non-security harness defaults. She needs local autonomy without allowing members to weaken team safeguards.

**UJ-03 — Harness governance.** Maya publishes a project workflow and locks selected settings. A member can tighten security or customize unlocked preferences but cannot weaken a security floor or override an explicit higher-scope lock. Skein shows the effective value, source, lock, and denial reason.

**UJ-04 — Remote team session.** Maya exposes her instance to an authenticated team. A member deliberately switches from a Local session to a Remote session and sees only that team's authorized projects and conversations. Local-session memory and artifacts remain absent. If the remote instance disappears, Skein does not merge or copy the remote session into Local; it offers a safe mode choice and preserves isolation.

### P-03 — Sam, team contributor

Sam uses CLI, API, and desktop surfaces interchangeably. Sam may use approved enterprise connectors but has no authority to install or widen them.

**UJ-05 — Connector denial and approval.** Sam asks Skein to read a permitted issue and then post a comment. Read and mutation permissions are evaluated separately. If mutation is not granted, Skein denies the action without leaking credentials or attempting a side effect. If approval is required, the workflow suspends and resumes only after an authorized decision.

**UJ-06 — Computer access boundary.** Sam grants access to one project directory and one application window. An attempted path escape, symlink/junction escape, unrelated window capture, clipboard read, or privilege widening is denied and audited. Expanding to a directory or whole-computer session requires a separate, time-bounded grant and visible indication.

### P-04 — Priya, security and platform administrator

Priya manages identity sources, team boundaries, capability trust, policy, secrets, audit access, and incident response. She needs deny-by-default behavior and evidence that cannot be bypassed through another surface.

**UJ-07 — Identity lifecycle.** Priya connects an approved identity source, maps groups to scoped roles, and verifies tenant/issuer binding. Removing a member revokes new access within the declared propagation objective and invalidates stale sessions. A duplicate email from another issuer never links identities automatically.

**UJ-08 — Incident investigation.** Priya traces an action from user request through policy decision, context selection, model traffic, tool call, approval, and effect outcome. Standard telemetry contains no prompt, secret, or personal-data payload by default. Privileged evidence access is itself authorized and audited.

### P-05 — Elena, privacy and compliance lead

Elena configures purposes, retention, export, and data-subject workflows and needs evidence without product certification claims.

**UJ-09 — Rights request and erasure.** Elena locates a subject's eligible data across primary and derived stores, exports permitted data, applies legal-hold exceptions, and executes erasure. Skein reports completed, excepted, pending, and unverifiable locations. Shared or multi-subject artifacts are not destroyed indiscriminately, and the audit record does not retain erased plaintext.

### P-06 — Jordan, non-technical cowork user (post-V1)

Jordan wants Skein to assist with documents and desktop applications while retaining clear previews, a stop control, and visible limits.

**UJ-10 — Safe desktop action.** Jordan asks Skein to update a document. Skein identifies the target application and window, previews sensitive or external actions, verifies focus before input, and confirms the resulting state. Loss of focus or stale visual state pauses rather than guessing.

**UJ-11 — Native-language team collaboration.** Jordan joins an authorized team chat or meeting in which participants use different languages. For each participant, Skein independently supports writing in their chosen language and being read in each recipient's chosen language, reading received text in their chosen language, speaking in their chosen language and being heard in each listener's chosen language, and hearing speech in their chosen language. The interface identifies translated content, source language, confidence or unavailable segments, latency state, and whether text, speech, or both are active. Consent loss, identity ambiguity, unsupported language direction, or quality below the pre-registered threshold pauses the affected direction without fabricating content or exposing another team's communication.

## 5. Release Hypothesis and Scope

### 5.1 Phase 0 — Design and evidence gate, not a product release

**Hypothesis:** Skein's one-way-door contracts and local-first packaging claims can be made coherent and testable before persistent product behavior is created.

Phase 0 contains only planning remediation, normative contracts, threat models, fixtures, and bounded spikes. It produces no supported end-user product.

Exit requires the Build Authorization Gate in Section 11. If a spike fails, the associated claim is narrowed or deferred; failure is not converted into hidden implementation scope.

### 5.2 Local Alpha — Strict-local developer core

**Hypothesis:** one developer receives independent value from a strict-local CLI/API agent with governed project access, bounded loops, recoverable workflow state, inspectable evidence, and one proven local inference path.

Local Alpha includes one supported operating path per target OS, one local model route, project-scoped files and commands, local identity, local silos, a minimal workflow, policy/approval enforcement, context manifests, and Ledger/evidence inspection. A graphical client, enterprise connectors, team networking, full-computer control, external IdPs, and multimodal generation are excluded.

### 5.3 Team Alpha — Small authenticated team

**Hypothesis:** a small team can share only explicitly team-scoped remote sessions while preserving local/remote isolation and centrally governed policy.

Team Alpha adds authenticated Server and Remote operation, team membership lifecycle, project/conversation authorization, harness locks and security floors, one governed remote model route, one read-oriented enterprise connector followed by one separately approved mutation path, and administrative/audit workflows. It does not promise high availability, automatic leader election, offline merge, multi-region operation, or enterprise certification.

### 5.4 V1 — Governed extensible work platform

**Hypothesis:** users will adopt Skein as their control plane when chat, code, workflow, and selected enterprise operations share consistent policy, context, evidence, and recovery semantics across CLI, API, and desktop UI.

V1 adds a non-technical desktop experience, a documented extension and capability registry, tested BMAD/Spec-Kit/power-skill profiles, selected Atlassian and Microsoft 365 capabilities, supported external identity profiles, and operational compliance-support evidence. Exact support is declared in the release registry; unsupported products or versions are not implied.

### 5.5 Long-term capability sequence

The accepted non-calendar dependency sequence is preserved as capability milestones, not delivery-date promises:

1. **Milestone v2 — Perception:** image, documents, authorized memory, web material, screen/window capture, and audio inputs.
2. **Milestone v3 — Cowork:** constrained browser and computer assistance with separately granted perception and action.
3. **Milestone v4 — Creation:** Office files, text, audio, and image outputs.
4. **Milestone v5 — Motion:** animated images and video.
5. **Milestone v6 — Omni:** coordinated multimodal input/output through attributable parallel or sequential composition.
6. **Milestone v7 — Real-time audio:** bounded-latency streaming speech interactions.
7. **Milestone v8 — Native-language collaboration:** team chat and meeting communication in which each participant can write, read, speak, and hear in their chosen language.

Every milestone has an independent feature specification, safety and quality gate, capability declaration, and de-scope decision. Passing one milestone does not imply support for every modality, language, provider, or operation.

### 5.6 De-scope order

When schedule, security evidence, maintainability, or packaging constraints conflict, remove scope in this order before weakening core guarantees:

1. additional providers, workers, connectors, and operating-system variants;
2. connector mutation operations before connector read operations;
3. desktop UI breadth before CLI/API behavior;
4. Team Alpha breadth and external identity profiles;
5. browser/desktop control and multimodal capabilities;
6. convenience automation and optimization.

Never de-scope silo isolation, deny-by-default policy, external loop termination, evidence integrity, secret exclusion, replay/effect safety, or explicit egress control to meet a release date.

## 6. Functional Requirements

The IDs below are canonical within this draft and intentionally sorted. Acceptance outcomes describe product behavior, including negative paths. Detailed mechanisms and implementation candidates belong in the addendum and downstream contracts.

### FR-001 — Headless task execution

Skein shall define a normative, versioned headless `ApplicationProtocol` covering commands, typed events, authentication, authorization, streaming, cancellation, approvals, errors, and compatibility. The CLI shall expose every supported protocol capability as the complete reference client and E2E oracle. Graphical clients shall be non-privileged peers that call the protocol directly and never shell through the CLI.

- **Positive outcome:** given an authorized local task, API and CLI clients receive equivalent typed lifecycle events and the same terminal result.
- **Negative outcome:** malformed, unsupported-version, unauthenticated, or unauthorized requests fail before model or tool execution and return a stable error category.
- **Evidence:** protocol conformance results and correlated run record.

### FR-002 — Governed agent loops

Every agentic run shall have externally enforced iteration, time, token/resource, and no-progress limits plus declared terminal criteria.

- **Positive outcome:** progress is evaluated using declared external observations, and a run ends only in a named terminal state.
- **Negative outcome:** a model request to continue cannot exceed a limit; missing ground truth for a required retry causes stop or human escalation.
- **Evidence:** budget, observation, evaluation, escalation, and exit records.

### FR-003 — Governed files, commands, and source control

Users shall grant separate, time-bounded capabilities for file read, file write, command execution, and source-control effects within an approved scope.

- **Positive outcome:** an authorized project-scoped change can be inspected, tested, and committed without access outside the scope.
- **Negative outcome:** traversal, link escape, unapproved process/network use, destructive operation, or scope widening is denied before effect.
- **Evidence:** grant, policy decision, tool intent/result, and changed-artifact record.

### FR-004 — Model capability selection

Skein shall route only to model endpoints whose declared modality, locality, data-class permission, tool support, context limit, and health satisfy the current policy.

- **Positive outcome:** the user can choose among eligible local or approved remote routes and see why a route was selected.
- **Negative outcome:** no eligible route produces a clear capability failure; Skein never silently downgrades locality, residency, privacy, or required features.
- **Evidence:** capability snapshot, policy decision, routing decision, and actual provider/model identity.

### FR-005 — Local inference availability

Local Alpha shall support at least one documented local text/code inference path on each declared target platform, subject to published hardware limits.

- **Positive outcome:** a compatible clean machine completes the Local Alpha journey without external inference.
- **Negative outcome:** missing hardware, model, license acceptance, or runtime produces actionable setup information and no covert cloud fallback.
- **Evidence:** environment report and clean-machine acceptance run.

### FR-006 — Explicit connectivity modes

The local backend shall remain installed, functional, and locally usable in every installation. It shall bind only to a local-only endpoint by default; network exposure requires an explicit authorized Server-mode configuration. Skein shall detect relevant connectivity and instance availability, explain consequences, and let the user explicitly select Local, Server, or Remote mode. Exactly one execution backend is active for each client session. While attached in Remote mode, the local backend remains available but stands down as that session's execution backend.

- **Positive outcome:** attachment selects the remote backend only after explicit confirmation; detachment enters a named detached state and requires an explicit choice before a new or existing Local/Server session reactivates the local backend. The active backend, bind/exposure state, silo, storage location, and processing location remain visible.
- **Negative outcome:** Remote loss, reconnect, conflicting discovery, client restart, or local backend availability never causes concurrent execution, implicit fallback, data movement, state merge, session migration, or effect replay. An uncertain transition enters a blocked state.
- **Evidence:** versioned backend-lifecycle state-machine conformance, single-active-backend tests, bind/exposure tests, detach/reactivation tests, and cross-mode no-movement fixtures.

### FR-007 — Silo and team isolation

All data and effects shall be scoped by an authenticated, system-derived security context covering storage, files, indexes, caches, processes, tools, telemetry, secrets, browser state, temporary data, backups, and exports.

- **Positive outcome:** authorized members can access only objects belonging to their selected silo and, in Remote mode, their team.
- **Negative outcome:** forged identifiers, stale membership, cross-team search, replay, cache, export, or telemetry access returns no protected content and creates an audit event where appropriate.
- **Evidence:** cross-domain negative suite and membership-revocation test.

### FR-008 — Hierarchical harness governance

Skein shall resolve values, explicit locks, and security floors across Silo, Team, Project, and Conversation scopes and show effective provenance.

- **Positive outcome:** the most specific unlocked value applies; the highest applicable explicit lock caps lower scopes; lower scopes may tighten but never weaken security.
- **Negative outcome:** an unauthorized editor or lower scope cannot bypass a lock or reduce a security floor through CLI, API, UI, import, replay, or background execution.
- **Evidence:** resolution explanation and exhaustive truth-table/property tests.

### FR-009 — Identity and session lifecycle

Skein shall target local user accounts, LDAP directories, OIDC providers, Microsoft Entra group mappings, and Google Workspace group mappings through versioned identity profiles. A profile is supported only when its assurance, tenant/issuer binding, group reconciliation, session, deprovisioning, break-glass recovery, and failure behavior are declared and tested.

- **Positive outcome:** a principal has a stable identity independent of mutable email and receives only current scoped relationships and roles.
- **Negative outcome:** issuer/tenant mismatch, ambiguous account linking, removed membership, expired assurance, or revoked session fails closed.
- **Evidence:** identity-profile conformance and lifecycle records.

### FR-010 — Authorization and obligations

Every protected action shall be decided outside the model using subject, action, resource, scope, relationship, attributes, environment, data class, destination, and risk, with outcomes deny, allow, or allow subject to obligations.

- **Positive outcome:** an allowed action carries an auditable decision identifier and all obligations, such as approval or redaction, are enforced at the effect boundary.
- **Negative outcome:** missing policy data, policy failure, bypass through another surface, or unsatisfied obligation denies the action.
- **Evidence:** policy decision and cross-surface default-deny tests.

### FR-011 — Tool and MCP lifecycle governance

Skein shall distinguish connector discovery, installation, trust, enablement, configuration, authentication, grant, invocation, update, revocation, and removal. Bundled or discovered does not mean enabled. The base state is disabled, except for a release-declared minimal local-only set with no external destination and least privilege. Read and mutation grants are always separate.

- **Positive outcome:** the applicable owner at Silo, Team, Project, or Conversation scope may grant or delegate only authority they hold. Higher-scope denials, locks, and security floors cap descendants; lower scopes may narrow grants. Each invocation is limited by resource, destination, data class, effect class, duration, quota, and separately resolved read/mutate authority.
- **Negative outcome:** lower-scope widening, grant laundering through another client, implicit enablement, mutation under a read grant, stale delegation, revoked ancestor grant, untrusted identity/version/schema, tool-description injection, oversized output, undeclared destination, or secret-return attempt is denied or quarantined without changing policy. Revocation propagates to descendant grants and new invocations within the declared objective.
- **Evidence:** owner/delegation/grant records, hierarchical authorization truth table, lock/security-floor property tests, cross-surface negative suite, read-versus-mutate tests, schema identity, sanitized result, and revocation propagation test.

### FR-012 — Enterprise work connectors

Jira, Bitbucket, Confluence, Microsoft 365, and Google Workspace are named roadmap connector families, including local MCP-server options where feasible. Each release shall registry-declare the exact product/version, resource, read operation, mutation operation, authentication profile, and limitation actually supported; naming a family never implies suite-wide support.

- **Positive outcome:** a declared connector operation preserves source identity, authorization, classification, and provenance in a workflow.
- **Negative outcome:** unsupported operation/version, insufficient delegated permission, wrong tenant, rate limit, partial failure, or revoked credential terminates safely and reports whether any effect occurred.
- **Evidence:** connector capability declaration and sandbox/staging contract tests.

### FR-013 — Workflow definition and durable execution

Users shall define, inspect, validate, run, suspend, resume, cancel, and revise workflows containing deterministic, agent, tool, approval, condition, parallel, and bounded-loop steps.

- **Positive outcome:** after interruption at any declared boundary, state reconstruction produces the same next decision and does not duplicate a completed external effect.
- **Negative outcome:** incompatible definition version, ambiguous effect state, failed fold, missing approval, or unsafe resume enters a named blocked state rather than guessing.
- **Evidence:** definition validation, crash-boundary suite, fold result, and terminal evidence bundle.

### FR-014 — Evidence Ledger and audit

Skein shall preserve attributable model inputs/outputs, transformations, context selections, tool observations, policy decisions, approvals, effects, and outcomes with integrity and access controls, while maintaining a distinct operational audit trail.

- **Positive outcome:** an authorized reviewer can reconstruct what was supplied, returned, decided, and applied, including routing and redaction stages.
- **Negative outcome:** credentials and secret values are excluded; unauthorized evidence access, integrity failure, unavailable payload after lawful erasure, or unsupported replay is reported explicitly.
- **Evidence:** integrity verification, privileged-access audit, redaction tests, and reconstruction fixture.

### FR-015 — Replay, revision, and effect safety

Skein shall allow history inspection, branching, recorded-result replay, and revision while re-evaluating current authorization before any new effect.

- **Positive outcome:** pure steps replay deterministically where supported; reversible or compensatable effects identify their separate authorized action.
- **Negative outcome:** irreversible effects are never automatically re-fired, old approvals do not authorize new effects, and ambiguous prior effect state blocks execution.
- **Evidence:** idempotency and effect-recovery test results.

### FR-016 — Context management

Every model call shall have a reproducible context manifest recording source identities and versions, hashes, authorization/classification decisions, selection rationale, transformations, model/tokenizer identity, budget allocation, and reserved output/loop headroom.

- **Positive outcome:** the selector retrieves the smallest sufficient authorized context and preserves pinned requirements, policies, and acceptance criteria.
- **Negative outcome:** unauthorized, deleted, stale beyond policy, untraceable, or over-budget material is excluded; if sufficient context cannot be assembled, the task degrades explicitly or stops.
- **Evidence:** manifest reconstruction, ACL-leakage suite, and retrieval-versus-full-context benchmark.

The context subsystem shall retain roadmap contracts for ACL-aware multimodal ingestion, source versioning and deletion propagation, lexical and vector retrieval, reranking, graph/temporal/code-symbol indexes, and separately governed session, personal, project, team, and organizational memory. Source ACLs shall constrain ingestion, indexing, retrieval, reranking, transformation, memory promotion, and output—not only final display.

### FR-017 — Replaceable governed workers

Skein may delegate only bounded work to workers that pass the release's governed-worker conformance profile.

- **Positive outcome:** Skein observes and correlates every required model turn, tool request/result, approval, effect, cancellation, budget, and terminal signal for the selected profile.
- **Negative outcome:** opaque or incomplete workers are rejected for governed execution or exposed only in a clearly labeled reduced-assurance mode that cannot make protected effects.
- **Evidence:** worker conformance report and capability declaration.

### FR-018 — Skills and engineering-method profiles

Skein shall package versioned, testable workflow profiles for supported BMAD, Spec-Kit, and power-skill practices and preserve traceability between planning artifacts, executable feature artifacts, tasks, tests, and evidence.

- **Positive outcome:** a supported profile validates required artifacts, stable requirement IDs, gates, and handoffs against a named upstream version.
- **Negative outcome:** missing artifacts, stale mappings, unresolved critical contradictions, or unsupported upstream versions block implementation authorization rather than being silently waived.
- **Evidence:** profile conformance report and round-trip traceability check.

### FR-019 — Just-in-time secrets

Skein shall use secret references and resolve them only for an authorized purpose at the execution boundary through a supported provider profile.

- **Positive outcome:** the target receives the minimum required secret for the minimum lifetime, while models, prompts, command arguments, ordinary logs, telemetry, and persisted tool results receive no secret value.
- **Negative outcome:** provider unavailability, revoked lease, offline incompatibility, destination mismatch, or redaction uncertainty fails closed without plaintext fallback.
- **Evidence:** secret-use metadata, revocation test, and leakage test corpus.

Before implementation authorization, the JIT baseline decision shall select and threat-model a default local root of trust, one reliable open-source or no-cost provider path, optional commercial providers, bootstrap and rotation ceremonies, break-glass recovery, backup/restore boundaries, and circular-dependency handling for identity, MCP, model, and secret-provider credentials. Failure to recover trust without plaintext fallback blocks the affected profile.

### FR-020 — Observability and incident evidence

Skein shall emit correlated health, performance, reliability, and security telemetry with content suppressed by default and export governed by mode, silo, destination, retention, and policy.

- **Positive outcome:** operators can diagnose run, workflow, model, context, tool, policy, approval, and recovery behavior without reading protected payloads.
- **Negative outcome:** Local mode or policy-denied export creates no external traffic; exporter failure cannot block local evidence integrity or leak buffered data across silos.
- **Evidence:** semantic-convention conformance, privacy scan, exporter-deny, and buffer-isolation tests.

### FR-021 — Computer and browser control (post-V1 gated capability)

Skein shall expose virtual keyboard, virtual mouse, screen capture, window capture, browser, clipboard, and accessibility/automation primitives as separate grants. Project scope is the default. Widening to an explicit folder, application/window, screen, or full-computer session requires a new visible, time-bounded grant from an authorized owner under Silo/Team/Project/Conversation locks and security floors; full-computer access is exceptional and never inferred.

- **Positive outcome:** the user sees the active target and scope, can interrupt immediately, and receives post-action verification.
- **Negative outcome:** stale frame, focus change, secure surface, permission loss, unsupported platform behavior, or target ambiguity pauses or denies the action.
- **Evidence:** platform-specific permission, targeting, interruption, and negative-boundary suites.

### FR-022 — Multimodal and omni composition (post-V1 gated capabilities)

Skein shall represent supported text, code, document, image, audio, video, animation, and 3D inputs/outputs through versioned capability declarations and attributable workflows, including real-time speech and translation when separately released.

- **Positive outcome:** the user receives one coherent result with visible provenance for each model/tool contribution and declared quality/latency limits.
- **Negative outcome:** unsupported modality, unsafe transformation, missing consent, resource exhaustion, or real-time quality below the declared threshold degrades or stops transparently.
- **Evidence:** modality-specific evaluation, resource, provenance, safety, and latency reports.

Local composition shall use an admission-controlled resource broker that accounts for CPU, GPU, RAM, VRAM, storage, model residency, load/unload cost, eviction safety, concurrency, interactive versus batch priority, and cancellation. Resource exhaustion shall yield a visible queue, declared degradation, or refusal—not uncontrolled overcommit or silent remote fallback.

## 7. Non-Functional Requirements

### NFR-001 — Security and privacy defaults

Protected actions and external destinations are deny-by-default. Security-relevant failures fail closed. Ordinary logs and telemetry contain no raw prompt, secret, tool payload, personal data, or customer content by default.

### NFR-002 — Isolation

Release gates require 100% execution of the applicable pre-registered isolation matrix and zero cross-mode, cross-silo, cross-team, cross-project, or cross-conversation disclosure. The matrix covers identity lifecycle, storage, retrieval, indexes, caches, processes, tools, browser state, telemetry, temporary data, backups, exports, replay, and every client surface. Missing or unavailable matrix evidence fails the gate; absence of a known incident is not proof. Any confirmed isolation bypass blocks release.

### NFR-003 — Recovery and effect integrity

Crash testing at every supported workflow/effect boundary must produce no silent loss, no duplicate irreversible effect, and no guessed effect state. Ambiguity must be visible and require authorized resolution.

### NFR-004 — Cross-platform honesty

Every release publishes an exact operating-system, architecture, hardware, installer, and capability matrix. A capability not proven on a platform is labeled unavailable or experimental rather than implied by the product name.

### NFR-005 — Reproducibility and supply chain

Supported builds use pinned toolchains and dependencies, verified source provenance, license policy, SBOMs, signed release artifacts, checksums, and documented upgrade, rollback, uninstall, and data-retention behavior.

### NFR-006 — Performance and resource budgets

Each release declares and tests startup, idle memory, interactive event latency, cancellation response, Ledger growth, context construction, and model/tool timeout budgets appropriate to its supported hardware. Exact thresholds are release acceptance data, not universal promises.

### NFR-007 — Accessibility and user control

Graphical surfaces target WCAG 2.2 AA where applicable. Sensitive actions expose clear scope, consequence, approval, progress, cancellation, and terminal status without requiring users to interpret raw model reasoning.

### NFR-008 — Compatibility

Versioned application, artifact, workflow, evidence, context, worker, connector, and capability contracts declare compatibility and migration behavior. Unsupported versions fail explicitly.

## 8. Success Metrics and Counter-Metrics

Thresholds below are release gates unless a Phase 0 spike is explicitly responsible for establishing the baseline.

### Core outcome metrics

- **SM-001 — Bounded-loop enforcement:** 100% of conformance runs terminate in a named state within declared hard budgets; zero model-controlled budget bypasses.
- **SM-002 — Ground-truth use:** 100% of retry/reflect decisions in development workflows cite an eligible external observation; unsupported self-judgment cannot satisfy a terminal gate.
- **SM-003 — Policy consistency:** the authorization mutation suite detects every seeded bypass across API, CLI, UI, worker, replay, connector, and background paths; no protected effect lacks a policy decision ID.
- **SM-004 — Evidence completeness:** 100% of critical security, policy, approval, effect, mode, secret, Ledger, and terminal-verdict events in conformance runs are correlated into a reconstructable evidence bundle, with zero secret values in the leakage corpus. A separate lower operational SLO may apply only to explicitly non-critical telemetry.
- **SM-005 — Context quality:** before final benchmark execution, the gate freezes representative task classes, baselines, non-inferiority margin, ACL/deletion cases, sample method, confidence treatment, and approving role. Smallest-sufficient context must satisfy the frozen quality threshold, improve declared resource measures, and have zero unauthorized-context recall.
- **SM-006 — Isolation:** 100% of the applicable pre-registered mode/silo/team/project/conversation and storage/retrieval/cache/process/tool/telemetry/backup/replay/export/client-surface matrix executes with zero protected disclosure.
- **SM-007 — Recovery:** 100% of supported crash-boundary cases resume deterministically or enter an explicit blocked state; zero duplicate irreversible effects.
- **SM-008 — User control:** 100% of high-risk conformance actions show scope and consequence and require the declared approval/step-up before effect.
- **SM-009 — Local value:** a clean supported workstation completes UJ-01 and UJ-02 without cloud credentials or external inference.
- **SM-010 — Team isolation and lifecycle:** Team Alpha completes UJ-04 and UJ-07, including membership revocation and remote-loss behavior, within declared objectives.
- **SM-011 — Local user value:** on a pre-registered representative journey set, Local Alpha meets frozen thresholds for setup effort, time to first governed value, task completion, recovery, operator intervention, and task time relative to using the underlying tools separately.
- **SM-012 — V1 adoption value:** V1 meets frozen thresholds for cross-surface task completion, connector-task burden, sustained voluntary use, and perceived control/trust relative to the documented baseline, without degrading CM-001–CM-008.

### Counter-metrics

- **CM-001:** do not improve completion time by increasing policy bypass, approval omission, secret/PII exposure, or irreversible-effect risk.
- **CM-002:** do not improve task success by routinely loading whole repositories or exceeding context budgets; track context size, irrelevant-context ratio, latency, and cost.
- **CM-003:** do not reduce user intervention by concealing uncertainty; track unsafe auto-approval attempts, blocked ambiguity, and incorrect terminal verdicts.
- **CM-004:** do not improve throughput by increasing recovery ambiguity, duplicate effects, flaky tests, or unreviewed dependency risk.
- **CM-005:** do not claim connector/provider breadth from untested adapters; track supported versus detected versus experimental capabilities separately.
- **CM-006:** do not optimize audit completeness by retaining unnecessary personal or secret-bearing content; track minimization, redaction, retention, and erasure exceptions.
- **CM-007:** do not improve apparent adoption by hiding setup labor, manual recovery, approval fatigue, unsafe workarounds, abandonment, or unavailable evidence.
- **CM-008:** do not improve interactive responsiveness by uncontrolled CPU/GPU/RAM/VRAM overcommit, starvation of cancellation or safety work, or silent remote fallback.

## 9. Compliance-Support Constraints

Skein shall provide technical controls and evidence that may support GDPR, EU AI Act, ISO/IEC 27001, SOC 2, and NIS2 obligations for a configured deployment. Applicability, lawful basis, organizational controls, risk classification, certification, attestation, and legal interpretation remain the deployer's responsibility with qualified advisers.

- **CS-001 — Data governance:** processing inventory, purpose, classification, location, recipient, retention, deletion method, and controller/processor role can be recorded for governed data paths.
- **CS-002 — Rights support:** authorized workflows support access, correction metadata, restriction, objection handling, portability, erasure, exceptions, and evidence without promising that every source system can satisfy every request automatically.
- **CS-003 — AI governance:** deployments can inventory capabilities and use cases, record model/provider identity, disclose AI mediation, configure human oversight, restrict prohibited or unapproved use cases, and retain required evidence.
- **CS-004 — Security management evidence:** policy changes, access reviews, incidents, vulnerabilities, suppliers, continuity tests, release provenance, and control operation can be evidenced and exported according to authorization and retention.
- **CS-005 — No certification overclaim:** product copy, UI, documentation, and capability registries use “supports,” “enables,” or “provides evidence for”; they do not state that installing Skein makes an organization GDPR-, AI-Act-, ISO-27001-, SOC-2-, or NIS2-compliant.
- **CS-006 — Qualified review gates:** privacy impact, high-risk AI, employment/monitoring, biometric, critical infrastructure, cross-border transfer, and certification claims require organizational legal/security/privacy review outside automated product approval.

## 10. Assumption Register and Dependency Requirement

Every assumption is tagged inline below and has an owner, validation trigger, and failure response.

- **[ASSUMPTION A-001]** A modular monolith with supervised optional components can provide one-product installation without making every capability a single process. **Owner:** Architecture Lead. **Validate:** package/bootstrap spike. **If false:** split optional packs while preserving one versioned product experience.
- **[ASSUMPTION A-002]** A Skein-owned control loop is feasible with acceptable maintenance cost using reusable provider and tool protocols. **Owner:** Runtime Lead. **Validate:** runtime ownership spike. **If false:** narrow supported worker assurance; do not surrender policy or evidence ownership.
- **[ASSUMPTION A-003]** Strict no-egress behavior can be enforced honestly on one or more declared Local Alpha platforms. **Owner:** Security Lead. **Validate:** adversarial egress spike. **If false for a platform:** mark strict Local unsupported on that platform; never weaken the invariant or rely only on adapter metadata.
- **[ASSUMPTION A-004]** At least one local inference path can be packaged or reliably detected for each Local Alpha platform. **Owner:** Release Lead. **Validate:** clean-machine installation spike. **If false:** reduce the platform matrix or require an explicitly installed local engine.
- **[ASSUMPTION A-005]** Smallest-sufficient context can match full-context task quality on representative work while reducing cost/latency and preserving ACLs. **Owner:** Context Lead. **Validate:** context benchmark. **If false:** revise selection and allow justified larger contexts; do not abandon manifest or ACL requirements.
- **[ASSUMPTION A-006]** Shared and derived personal data can be reconciled with integrity-preserving evidence and lawful erasure. **Owner:** Privacy Lead. **Validate:** data model, DPIA, and erasure fixtures. **If false:** narrow retained payloads and replay claims before storing user data.
- **[ASSUMPTION A-007]** Team Alpha can provide useful authenticated sharing without high availability, offline merge, automatic election, or multi-region replication. **Owner:** Product Owner. **Validate:** team journeys and architecture review. **If false:** defer Team Alpha until the required distributed contract is designed.
- **[ASSUMPTION A-008]** Selected external workers and connectors can expose sufficient governed events and delegated identity. **Owner:** Integration Lead. **Validate:** worker and MCP spikes. **If false:** classify them as reduced-assurance/read-only/unsupported.
- **[ASSUMPTION A-009]** BMAD, Spec-Kit, and power-skill profiles can be versioned and round-trip stable requirement IDs and gate status. **Owner:** Methodology Lead. **Validate:** conformance profile and feature rehearsal. **If false:** support fewer named versions and remove “conformant” claims.
- **[ASSUMPTION A-010]** A solo owner assisted by bounded agents can deliver Local Alpha by reusing commodity components and limiting concurrency. **Owner:** Project Owner. **Validate:** Phase 0 planning rehearsal and first vertical slice. **If false:** reduce scope or recruit maintainers; do not weaken gates.

Downstream feature, architecture, gate-manifest, and test artifacts shall cite every assumption on which they depend and generate a bidirectional assumption-to-claim-to-test index. This section is only the register until that generated index passes G0. An assumption is removed only by replacing it with an accepted decision/evidence reference or a rejected-assumption scope change.

## 11. Build Authorization Gate

### 11.1 Current status

**NOT READY — PRODUCT IMPLEMENTATION BLOCKED.** The project may continue PRD remediation, architecture work, threat modeling, contract examples, and bounded evidence spikes. It may not start production source implementation or launch autonomous implementation, review, testing, contradiction, validation, or staging swarms against product code.

### 11.2 Owned blocking decisions

| Decision | Accountable owner | Required closure evidence | Current state |
| --- | --- | --- | --- |
| BD-001 Canonical Ledger, event, effect, integrity, encryption, retention, erasure, and compatibility contract | Architecture + Privacy Leads | versioned contract, examples, folds, golden replay/erasure fixtures, adversarial review | Blocked |
| BD-002 Local/Server/Remote state machine and silo security-domain contract | Architecture + Security Leads | state diagrams, trust boundaries, split/loss/reconnect behavior, negative isolation matrix | Blocked |
| BD-003 Enforceable Local egress claim per target OS | Security + Release Leads | reproducible socket/DNS/child-process tests and declared limitations | Blocked |
| BD-004 Headless application protocol and WorkerAdapter conformance | Runtime Lead | versioned async/stream/cancel/approval/effect contract, examples, candidate spike report | Blocked |
| BD-005 Durable workflow folds and effect recovery | Workflow Lead | crash-at-boundary suite, idempotency rules, deterministic/blocked outcomes | Blocked |
| BD-006 Authorization, configuration, approval, and silo capability model | Security Lead | formal decision inputs/outcomes, permission matrix, truth tables, mutation tests | Blocked |
| BD-007 MCP/tool trust and delegated authorization | Integration + Security Leads | lifecycle contract, local/remote spike, schema pinning, redaction/injection tests | Blocked |
| BD-008 Context lifecycle and benchmark | Context Lead | ContextManifest contract, ACL/deletion lifecycle, retrieval/full-context evidence | Blocked |
| BD-009 Identity, secrets, audit, and privacy lifecycle | Security + Privacy Leads | threat/data models, provider profiles, rights/retention/erasure fixtures | Blocked |
| BD-010 Single-package tri-OS bootstrap and supply chain | Release Lead | clean-machine, offline-first, idempotence, signing/provenance plan and evidence | Blocked |
| BD-011 Platform composition decision | Architecture Lead + Project Owner | all ADR-0003 spikes complete; ADR accepted/revised; architecture revalidated | Blocked |
| BD-012 BMAD–Spec-Kit implementation handoff | Product + Methodology Leads | validated PRD; accepted architecture; regenerated epics/stories; complete Spec-Kit feature package and analysis | Blocked |

### 11.3 READY criteria

The Project Owner may change the gate to READY for one named vertical slice only when all of the following are true:

1. this PRD update is reconciled into the canonical PRD and BMAD validation reports zero critical findings;
2. all one-way-door decisions affecting the slice are accepted, versioned, exemplified, threat-modeled, and independently reviewed;
3. required ADR-0003 and additional security/recovery spikes have reproducible evidence and explicit accept/reject decisions;
4. architecture is accepted and its requirement coverage is current;
5. BMAD epics and concrete story artifacts are regenerated from stable FR IDs;
6. a real Spec-Kit feature branch contains clarify, research, data model, contracts, quickstart, plan, tasks, quality checklists, and cross-artifact analysis with no unresolved critical contradiction;
7. BMAD implementation-readiness reports zero critical findings and every task has ownership, allowed files/tools, risk, context budget, external test oracle, rollback, and terminal evidence;
8. pinned development bootstrap and CI/CD gates work on the declared platform matrix before product source changes are merged;
9. the implementation swarm starts at low concurrency with separate author, reviewer, adversarial challenger, test/evaluation, integration, and human approval roles;
10. the Project Owner records explicit build authorization naming the feature, commit, contract versions, maximum concurrency, loop budgets, and staging boundary.

The versioned gate manifest required by `docs/QUALITY-GATES.md` is the mechanical closure register for BD-001–BD-012. Missing, stale, expired, unavailable, or non-independent critical evidence evaluates as failure. No prose assertion or waived check can substitute for a passing manifest.

## 12. Glossary

- **Application protocol:** the normative versioned headless commands, events, errors, streaming, cancellation, approval, authentication, authorization, and compatibility contract. The CLI is its complete reference client/E2E oracle; graphical clients are non-privileged peers and never shell through the CLI.
- **Approval:** an authorization obligation requiring a permitted human decision before execution may continue; it is not model consent.
- **Artifact:** a versioned planning, design, implementation, test, evidence, or user-output object with identity and provenance.
- **Audit trail:** the security/operational record of who attempted or changed what, when, under which identity and policy. It is distinct from content-rich execution evidence.
- **Capability:** a versioned declaration of what a model, worker, tool, connector, platform adapter, or workflow profile can do and under which limits.
- **Capability registry:** the release-specific inventory of supported, experimental, detected, disabled, and unsupported capabilities with versions and conformance evidence.
- **Connector:** an adapter to an external or local system. An MCP server may implement a connector, but MCP does not by itself supply Skein authorization or trust.
- **Context manifest:** the reproducible record of what information was selected for one model call, why it was selected, how it was transformed, and which authorization and token budgets applied.
- **Control plane:** Skein-owned policy, workflow/loop state, context, evidence, approval, capability, and completion logic.
- **Cowork:** governed assistance that can perceive or act through desktop, browser, or application capabilities in addition to text and code.
- **Effect:** an externally observable change. Effects are classified as replay-safe, reversible, compensatable, irreversible, or ambiguous according to a versioned contract.
- **Evidence bundle:** the requirement-linked package of inputs, outputs, decisions, observations, tests, effects, provenance, and approvals used to support a completion verdict.
- **Ground truth:** an externally produced observation eligible to evaluate progress, such as a test, compiler, linter, tool result, or verified environment state; model self-opinion is not ground truth.
- **Harness:** the governed combination of instructions, policies, context rules, tools, skills, workflows, budgets, approvals, and evaluations around models and workers.
- **Identity provider (IdP):** a source that authenticates principals and may supply attributes or group memberships under a declared assurance profile.
- **Ledger:** the integrity-protected execution history used to reconstruct model, context, workflow, decision, observation, and effect records subject to access, retention, and erasure rules.
- **Local mode:** a session bound to the local backend and local silo under a hard no-egress boundary. Only explicitly enumerated local IPC/loopback TCB endpoints are permitted. A platform that cannot prove this invariant does not support strict Local.
- **Loop engineering:** the deliberate external design, bounding, instrumentation, evaluation, and termination of iterative agent behavior using ground truth and named terminal states.
- **MCP:** Model Context Protocol, a protocol for exposing tools, resources, and prompts. MCP transport or compatibility does not imply trust, authorization, isolation, or safe output.
- **Omni:** a unified user experience created by attributable orchestration of one or more modality-specific models and tools, not necessarily one model.
- **Policy decision:** a non-model result that denies, allows, or conditionally allows an action with obligations and a stable evidence identifier.
- **Power-skill profile:** Skein's versioned packaging and conformance description for a supported reusable agentic skill practice; the term does not imply universal mastery.
- **Principal:** a stable authenticated person, service, or device identity bound to one or more scoped relationships and attributes.
- **Remote mode:** a client session attached to another Skein instance and its selected team/silo; it is isolated from sessions in Local and Server modes.
- **Server mode:** an explicitly network-exposed Skein instance serving authorized clients while retaining its own local backend and silos.
- **Silo:** a security and lifecycle domain that scopes data, keys, indexes, caches, tools, processes, telemetry, backups, exports, and sessions; it is more than a database namespace.
- **Skill:** a versioned instruction and resource package invoked under harness, tool, context, and policy controls.
- **Worker:** a native or external execution component delegated a bounded task. A worker never owns Skein's policy, canonical state, evidence, or final authorization.
- **Workflow:** a versioned graph of deterministic, agent, tool, approval, condition, parallel, and bounded-loop steps with durable state and explicit terminal outcomes.
