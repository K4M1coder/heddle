# Feasibility and Engineering Strategy Review — Packaging, Stack, Bootstrap, and Context

**Date:** 2026-07-16  
**Reviewer role:** Feasibility and engineering-strategy reviewer  
**Scope:** All current Skein planning artifacts, feature specifications, architecture decisions, development guidance, and project research  
**Change constraint:** Read-only review; no product implementation was performed  
**Verdict:** **CONDITIONALLY FEASIBLE — NOT READY FOR IMPLEMENTATION**

## 1. Executive Assessment

Skein is feasible as a single user-facing product and distribution, but not as one executable, one process, one language, or one model context. The technically coherent target is already stated in ADR-0003: a **modular monolith with supervised optional sidecars and workers**, delivered from one monorepo, one version, one installer, one CLI/API/UI contract, and one default local backend.

The product vision is substantially larger than a coding-agent clone. A minimal clone can be built quickly because the core tool loop is small and commodity libraries provide most infrastructure. Skein additionally requires durable workflows, exact evidence, reversible effects, policy enforcement outside the model, silo and team isolation, cross-platform desktop control, enterprise identity and connectors, multimodal orchestration, and compliance-supporting controls. Those requirements make a one-pass autonomous implementation unsafe and unrealistic.

The current strategy is directionally sound, but implementation must remain blocked because:

1. ADR-0003 is still Proposed and its five mandatory evidence spikes have not been completed.
2. Phase 0 `plan.md` and `tasks.md` explicitly require regeneration.
3. The architecture stack is not yet compatibility-pinned or proven on Windows, Linux, and macOS.
4. No CI workflow, root language manifests, lockfiles, or pinned Rust toolchain currently exist.
5. The bootstrap scripts are useful prototypes, but do not yet establish reproducible, secure, from-scratch setup.
6. The canonical contracts named as one-way doors are not accompanied by versioned schemas, examples, and contract tests.
7. The context strategy is correctly stated, but has not yet been benchmarked or translated into measurable acceptance gates.

## 2. Single Product and Package vs. Modular Processes

### Decision

Adopt the following precise promise:

> Skein is one product, repository, release train, installer, application identity, CLI/API contract, and local data root. Internally it is a modular control plane that may supervise separately versioned processes for inference, browser automation, provider normalization, ML retrieval, media generation, and optional agent workers.

This satisfies the desired simple desktop experience without forcing incompatible ecosystems into one binary.

### Why one process is unsuitable

- LiteLLM and many retrieval or media components are Python-native.
- Tauri uses a Rust host with a TypeScript/JavaScript web frontend.
- Ollama, llama.cpp, vLLM, ComfyUI, browsers, office tools, and optional workers already have independent lifecycle and GPU requirements.
- Browser and computer-control components need stronger sandbox and permission boundaries than ordinary in-process libraries.
- A remote team instance needs authenticated network exposure while a local instance must retain a hard no-egress mode.
- Crashes, upgrades, licensing, resource limits, and security policies are easier to isolate with supervised processes.

### Required process model

The Rust backend must be the supervisor and source of truth. Sidecars must use authenticated local IPC or loopback with per-launch credentials, explicit capability declarations, health checks, bounded restart policies, version negotiation, resource limits, and complete Ledger correlation. A sidecar may compute; it may not own authorization, canonical workflow state, context selection policy, evidence, or completion verdicts.

The packaging spike must prove optional-component behavior: the base desktop remains useful offline without Python, Node, cloud credentials, or a preinstalled inference server; optional packs can be installed, verified, upgraded, disabled, and removed independently.

## 3. Language and Runtime Strategy

### Rust — retain for the control plane

Rust is appropriate for the long-lived local backend, CLI, workflow/loop controller, Ledger, policy enforcement points, local IPC, process supervision, filesystem boundaries, and cross-platform privileged adapters. It offers predictable resource use, strong data contracts, and good distribution characteristics.

However, `Rust 1.79` must not be accepted merely because it was written into the artifacts. In 2026 it is an unusually old baseline for a new Tauri 2 application and may conflict with current dependency MSRVs and security updates. The stack gate must select either:

- a current pinned stable toolchain with a defined support/update policy; or
- a deliberately older MSRV proven against every direct dependency and all three operating systems.

The chosen toolchain, target triples, and components must be materialized in `rust-toolchain.toml`, not only documented.

### TypeScript — retain, but confine it to surfaces and justified adapters

TypeScript is appropriate for the Tauri frontend, web UI, generated client SDKs, and possibly compatibility tooling for TypeScript-native ecosystems such as Archon. It should not become a second owner of workflow, policy, Ledger, or context semantics. UI actions must use the same versioned API exposed to the CLI and external clients; the UI should not literally shell out to CLI text parsing when a typed API/event protocol is available.

Node must be a build-time dependency for the desktop UI, not necessarily a runtime prerequisite for end users. The package manager, Node version, lockfile policy, frontend framework, generated API strategy, linting, formatting, unit tests, accessibility tests, and E2E framework remain undecided and require an explicit UI stack ADR before UI implementation.

### Python — use only as a supervised capability sidecar

Python is justified for LiteLLM, embeddings, reranking, document extraction, and multimodal pipelines where the ecosystem advantage is decisive. It should not be required for the minimal local core. Python dependencies must be isolated into one or more locked environments with hashes, explicit model/runtime compatibility, and an IPC contract. `Python 3.11+` is too open-ended for reproducible distribution; select supported minor versions and lock them with `uv.lock` or equivalent.

### Recommendation

Use a **Rust core + TypeScript UI + Python capability sidecars**. Avoid implementing the same domain behavior in multiple languages. Every cross-language boundary requires a versioned schema, compatibility tests, timeout/cancellation semantics, structured errors, trace propagation, and a security classification.

## 4. Reuse vs. Build

The current adopt/adapt/inspire/worker rubric is strong and should become a mandatory dependency decision record for each major component.

### Skein must build and own

- canonical artifact graph and BMAD–Spec-Kit projections;
- event-sourced workflow and governed loop semantics;
- Ledger, replay/effect rules, evidence bundles, and traceability;
- silo/team/project/conversation scope resolution and security floors;
- policy enforcement and approval integration;
- MCP/Tool Gateway mediation;
- ContextManifest, data classification, ACL-aware selection, and retrieval policy;
- capability registry, routing policy, and stable CLI/API/UI contracts.

### Skein should reuse behind replaceable adapters

- official MCP SDKs and transports;
- provider SDKs and LiteLLM where it passes the spike;
- SQLite/PostgreSQL libraries;
- OpenTelemetry;
- Playwright and native OS automation libraries;
- Ollama, llama.cpp, vLLM, ComfyUI, FFmpeg, STT/TTS engines, Blender;
- standard identity, secret, and policy systems when team/enterprise deployment requires them.

### Reuse gate

No dependency or borrowed code may enter the product without license and trademark review, update cadence, security history, maintenance health, supported-platform matrix, transitive-dependency/SBOM impact, offline behavior, data-egress analysis, resource footprint, API stability, and an exit strategy. Optional workers must pass observable-turn, tool-mediation, approval, cancellation, budget, and Ledger-correlation contract tests.

## 5. Cross-Platform Feasibility

Windows, Linux, and macOS are feasible as first-class targets for the headless core, CLI, local persistence, model gateways, and most MCP connectors. Desktop and computer control are materially harder and must be treated as platform adapters with common semantic contracts rather than identical implementations.

Required platform-specific work includes:

- Windows UI Automation, input injection, UAC/session boundaries, Authenticode, installer and Defender behavior;
- macOS Accessibility and Screen Recording permissions, hardened runtime, entitlements, Developer ID signing, and notarization;
- Linux X11/Wayland differences, desktop portals, packaging fragmentation, sandbox policy, and input/screen-capture permissions;
- keychain adapters for Windows Credential Manager, macOS Keychain, and Linux Secret Service, with a documented headless fallback;
- filesystem paths, case sensitivity, line endings, PTYs, process trees, signals, sockets, and browser installation differences.

Tri-OS CI is necessary but insufficient. Before cowork implementation, maintain physical or hosted E2E test environments for permission prompts, screen capture, keyboard/mouse control, process termination, signing, installers, upgrades, rollback, and uninstall. Computer scope (`project`, `directory`, or `whole machine`) must be enforced below the model at filesystem, process, desktop, and network boundaries.

## 6. From-Scratch Bootstrap Review

The two bootstrap scripts are a good early skeleton, but the repository cannot currently be reconstructed and verified from scratch as claimed.

Observed gaps:

- `rust-toolchain.toml`, root `Cargo.toml`, `package.json`, `pyproject.toml`, lockfiles, `.pre-commit-config.yaml`, and `.github/workflows/` are absent.
- The Windows script assumes `winget`; the Unix script requires the user to install Node manually and conflates macOS and diverse Linux distributions.
- Tool downloads are not checksum/signature verified and framework installations use moving `latest` or Git HEAD references.
- BMAD and Spec-Kit are considered valid when directories merely exist; versions and integrity are not checked.
- Goose remains a manual installation despite being named by the verification path.
- MCP setup delegates authentication to `claude mcp`, which cannot be the portable bootstrap contract for an independent Skein project.
- No clean-machine/container test currently executes the scripts.
- No offline cache, proxy, air-gapped setup, uninstall, update, or recovery strategy exists.
- No GPU/driver capability detection or optional-pack compatibility matrix exists.

### Bootstrap acceptance gate

Bootstrap is ready only when clean Windows, macOS, and Linux runners can clone a pinned commit, run one documented command, verify pinned tool/framework versions, execute planning validators and quality tools, and produce an identical environment report. Installation must be separated into minimal core, contributor development, local inference, enterprise connectors, and multimodal packs. All downloaded executables and archives need provenance verification. CI must test idempotence and a second run must perform no destructive or unnecessary changes.

## 7. CI/CD Strategy

No effective CI/CD implementation exists yet. Before product code, define and instantiate the pipeline contract.

Minimum pull-request gates:

1. artifact/schema validation and requirement traceability;
2. formatting, linting, type checks, and documentation checks per language;
3. Rust, TypeScript, and Python unit tests when those workspaces exist;
4. contract tests across language/process boundaries;
5. integration tests with hermetic fakes for models, MCP, identity, secrets, and trackers;
6. silo isolation, egress-deny, approval, redaction, replay, idempotency, and effect-safety tests;
7. SAST, dependency audit, license policy, secret scan, SBOM, provenance, and reproducible-build checks;
8. tri-OS build/test matrix with explicit allowlisted exceptions;
9. signed staging artifacts and clean-install/upgrade/uninstall smoke tests;
10. terminal acceptance evidence linked back to requirements.

Use trunk-based development with short-lived branches/worktrees, protected main, required independent review, Conventional Commits, generated changelogs, immutable release artifacts, and staged promotion. Releases must be signed per platform and accompanied by checksums, SBOM, provenance attestations, migration/rollback instructions, and compatibility metadata.

## 8. One-Million-Token Context Constraint

### Repository size is not active context

These are separate quantities:

- **Repository size** is the complete durable corpus: source, tests, documentation, generated schemas, workflows, assets, histories, and connector packages. A mature Skein repository will probably exceed one million tokens, and this is acceptable.
- **Active context** is the bounded material supplied to one model call: instructions, task contract, selected code, retrieved evidence, tool outputs, trajectory state, and output headroom. It must normally be much smaller than the repository and smaller than the model's advertised maximum.

One million tokens is approximately 3–5 MB of source text or roughly 50,000–100,000 mixed lines, depending heavily on language and formatting. This estimate is neither a code-quality target nor a platform feasibility limit. Rust types, tests, generated schemas, documentation, and configuration change the ratio substantially.

Long context does not eliminate context engineering. Relevant material can be lost by position, effective context can be lower than advertised context, cost and latency increase, and space is still required for tools, diffs, errors, intermediate state, and output. A 1M window must be treated as **overflow capacity**, never as default working memory.

### Mandatory smallest-sufficient context model

Every model call must use and persist a reproducible `ContextManifest` containing source identifiers and hashes, classification and ACL decision, selection method and rationale, token allocation, pinned requirements/policies, transformations/summaries, and links to prior evidence.

Selection must use, at minimum:

- repository maps;
- symbol indexes and references;
- dependency, import, call, and artifact relationship maps;
- lexical search and hybrid semantic retrieval;
- ACL and silo filtering before and after retrieval;
- lazy loading and progressive disclosure;
- source-hashed module summaries;
- trajectory compression that preserves decisions, failures, evidence, and provenance;
- pinned acceptance criteria, security invariants, and active constraints;
- reserved budget for tool results, repair iterations, and final output.

Whole-repository loading must be explicit, justified, recorded, and benchmark-gated. The normal target should be tens of thousands of high-value tokens, adjusted by task and model capability.

### Context quality gate

The ADR-0003 context spike must compare smallest-sufficient retrieval with full-context loading across representative small, medium, and larger repositories. Measure task success, regression rate, evidence recall, middle-position recall, latency, token/cost use, context stability across retries, and contamination from irrelevant or unauthorized material. Include code navigation, cross-module change, bug repair, architecture question, and security-sensitive tasks. No context architecture is accepted without these results.

## 9. Can a Solo Agent Swarm Build Skein?

### Feasibility

A single experienced owner using multiple coding agents can plausibly build a useful local alpha and much of the modular core by aggressively reusing mature components. The same arrangement cannot credibly deliver the complete enterprise, multimodal, real-time, tri-OS, compliance-supporting product in one uninterrupted campaign with dependable quality.

Agents increase implementation throughput; they do not remove integration complexity, platform access, external API tenants, signing certificates, security review, usability validation, legal responsibility, or operational maintenance. Parallel agents also amplify specification drift, duplicated abstractions, merge conflicts, unverified assumptions, and common-mode model errors.

### Required swarm operating rules

- Decompose only from approved BMAD epics/stories and fully gated Spec-Kit features.
- Give every task an isolated worktree, explicit file ownership, bounded context manifest, acceptance oracle, allowed tools, risk class, and loop budget.
- Separate author, reviewer, adversarial challenger, test/evaluation agent, integration owner, and final human approver.
- Do not count multiple agents using the same model/prompt lineage as independent evidence.
- Require external ground truth every iteration: compiler, tests, linters, contract probes, real API/tool results, or platform E2E evidence.
- Enforce action, iteration, and terminal verification.
- Stop on no progress, budget breach, unclear authority, security-boundary change, or contradiction with an accepted artifact.
- Merge only through a serialized integration queue after requirements traceability and full regression gates pass.

## 10. Staged Gates Before Autonomous Implementation

### Gate 0 — Planning integrity

- BMAD PRD validation passes.
- Architecture, UX obligations, epics/stories, NFRs, risks, and traceability are complete enough for the selected slice.
- BMAD implementation-readiness report has no critical finding.
- Spec-Kit clarify, plan, research, data model, contracts/CLI schemas, quickstart, tasks, checklist, and analyze artifacts exist for Phase 0 or carry an explicitly approved, evidence-backed waiver.
- No feature is marked both blocked and in progress.

### Gate 1 — One-way-door contracts

- Versioned schemas plus examples and contract tests exist for ArtifactModel, WorkflowDefinition, WorkerAdapter, CapabilityDescriptor, PolicyDecision, EvidenceBundle, ContextManifest, and Ledger events/folds.
- Silo isolation, crypto-shredding, egress boundary, effect replay, idempotency, approval, and config/security-floor semantics are threat-modeled.

### Gate 2 — Five ADR-0003 spikes

- Runtime ownership comparison.
- Archon workflow mapping.
- Context retrieval benchmark.
- Local and remote MCP governance proxy.
- Tri-OS single-package/offline installation proof.

Each spike needs a fixed budget, objective measurements, adversarial review, retained evidence, and an explicit accept/reject decision. ADR-0003 may become Accepted only afterward.

### Gate 3 — Reproducible engineering substrate

- Pinned toolchains, manifests, lockfiles, quality configuration, and tri-OS CI exist.
- Bootstrap succeeds and is idempotent on clean machines.
- Dependency, license, SBOM, provenance, and secret scanning gates run.
- Test fakes and staging infrastructure are available without production credentials.

### Gate 4 — Phase 0 regeneration and rehearsal

- Regenerate `specs/001-phase0-walking-skeleton/plan.md` and `tasks.md` from accepted evidence.
- Generate concrete BMAD story files and Spec-Kit checklists.
- Run a dry planning rehearsal: every task has inputs, outputs, dependencies, owner role, context budget, tests, rollback, and terminal criterion.
- Independent contradiction and security reviews report no critical issue.

### Gate 5 — Controlled implementation swarm

- Start with one vertical slice and low parallelism.
- Prove Ledger correlation, context manifests, loop budgets, tool governance, isolation, and CI evidence before increasing concurrency.
- Promote from local development to integration staging, then signed tri-OS staging; no production or enterprise connector writes during the initial campaign.

### Gate 6 — Capability-by-capability expansion

Add code agent, connectors, team mode, UI, cowork, multimodal, real-time voice, and translation as separate gated capabilities. Each capability receives its own threat model, platform matrix, resource benchmark, permissions model, evaluation suite, packaging proof, and rollback path.

## 11. Required Corrections to Current Artifacts

1. Change `sprint-status.yaml` Epic 1 from `in-progress` to a planning/spike-compatible state until readiness gates pass.
2. Remove residual Goose-specific acceptance wording where runtime selection is unresolved; use the governed WorkerAdapter contract and let the spike select an implementation.
3. Reconcile the claim that the UI is an overlay of the CLI with a typed shared API: the CLI and UI should be peer clients of the headless API, with CLI behavior authoritative, not UI-to-shell coupling.
4. Replace the unverified Rust 1.79 assumption with a toolchain-selection gate and dependency compatibility evidence.
5. Make TypeScript and frontend quality/build choices explicit before UI work.
6. Define exact Python sidecar boundaries, lock policy, IPC, and optional-install behavior.
7. Turn the bootstrap claims into tested clean-machine evidence and remove dependency on Claude-specific MCP setup.
8. Create actual CI/CD workflows before the first product code commit.
9. Execute the five spikes and formally accept or revise ADR-0003.
10. Preserve the 1M-token distinction in every agent-development workflow: repository size is unconstrained by a single call; active context is smallest-sufficient, selected, compressed, and evidenced.

## 12. Final Recommendation

Proceed with design completion and evidence spikes, not product implementation. The best underlying strategy is the current mixed stack—Rust control plane, TypeScript UI, Python optional capability sidecars—provided that language ownership is sharply bounded and cross-process contracts are versioned.

The project can be built incrementally by a solo owner with an agent swarm, but only as a sequence of bounded, independently verified vertical slices. The user may issue one high-level campaign command in the future; internally that campaign must remain a durable graph of specifications, approvals, isolated tasks, ground-truth loops, checkpoints, independent reviews, staged integration, and explicit terminal gates. A nominal one-million-token model context does not change that engineering requirement.
