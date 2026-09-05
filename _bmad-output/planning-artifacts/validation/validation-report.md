# Validation Report — Heddle PRD

- **PRD:** `_bmad-output/planning-artifacts/PRD.md`
- **Rubric:** `.claude/skills/bmad-prd/assets/prd-validation-checklist.md`
- **Run at:** 2026-07-16T00:18:59+02:00
- **Grade:** Poor
- **Gate:** NOT READY FOR IMPLEMENTATION

## Overall verdict

Heddle has a differentiated and technically credible thesis: a local-first, Heddle-owned control plane with governed workflows, replaceable workers, explicit context selection, MCP mediation, and evidence-rich execution. The current PRD is not decision-ready as build authorization because its runtime and one-way-door contracts remain unresolved, most functional requirements lack independent acceptance oracles, and the stated MVP combines too many platform foundations without an explicit de-scope order.

The architecture, security, and feasibility reviews strengthen this verdict. They identify unresolved canonical contracts for the Ledger, effects, workflows, workers, policy, silos, context, and mode transitions; insufficient cross-platform proof for hard offline egress and desktop control; incomplete identity/MCP/privacy governance; and a BMAD–Spec-Kit bridge that is documented but not yet operationally complete.

## Dimension verdicts

- Decision-readiness — broken
- Substance over theater — adequate
- Strategic coherence — thin
- Done-ness clarity — broken
- Scope honesty — thin
- Downstream usability — thin
- Shape fit — adequate

## Findings by severity

### Critical

1. **[Decision readiness] Runtime and build authorization are unresolved.** ADR-0003 is still proposed, its five evidence spikes are incomplete, and no explicit READY/NOT READY build gate exists.
2. **[Strategic coherence] Success metrics do not prove the control-plane thesis.** They omit governed-loop reliability, recovery, policy correctness, evidence completeness, and context quality.
3. **[Done-ness clarity] Most FRs lack independent acceptance oracles.** Autonomous implementers would need to invent completion criteria and tests.
4. **[Architecture] The canonical Ledger/event/effect/privacy contract does not exist.** Persistence must not be implemented before versioned schemas, folds, effect states, replay rules, encryption, erasure, and golden fixtures exist.
5. **[Architecture] Local, Server, and Remote mode semantics contradict the isolation promise.** Connectivity state, silo selection, leader loss, fallback, caching, split-brain, and reconnection behavior are not normatively defined.
6. **[Architecture] Hard offline egress has no proven tri-OS enforcement design.** Adapter metadata is not a network security boundary.
7. **[Architecture] WorkerAdapter and durable workflow execution contracts are undefined.** Event sourcing alone does not guarantee safe recovery or prevent duplicate external effects.
8. **[Privacy] Exact history, immutable evidence, replay, and GDPR erasure are not reconciled at the data-model level.**
9. **[Security] No complete threat model, enforceable silo security-domain model, RBAC+ABAC+ReBAC policy contract, or MCP trust/authorization contract exists.**

### High

1. Replace universal provider/framework/compliance claims with versioned capability and conformance registries.
2. Split the current platform-sized MVP into architecture spikes, strict-local core, durable workflow, UI, governed remote connector, and team mode releases.
3. Define a single versioned headless application protocol; CLI and UI should be peer clients, with CLI behavior authoritative, not UI-to-shell coupling.
4. Make silo context an unforgeable capability required by stores, files, caches, workers, tools, telemetry, secrets, browser profiles, and temporary data.
5. Specify MCP install, trust, enable, grant, invoke, revoke, schema pinning, delegated identity, output classification, quotas, redaction, and audit semantics.
6. Define worker, model, tool, workflow, policy, evidence, context, and capability contracts with examples and conformance tests.
7. Complete identity assurance, provisioning/deprovisioning, group reconciliation, tenant binding, session, and break-glass semantics.
8. Complete JIT secret lifetime, injection channel, child-process propagation, redaction, revocation, rotation, and crash behavior.
9. Produce control-to-requirement-to-test-to-evidence mappings for GDPR, EU AI Act enablement, ISO 27001 enablement, SOC 2 enablement, and NIS2 enablement without claiming product certification.
10. Prove clean-machine bootstrap, locked toolchains, tri-OS CI/CD, package provenance, offline operation, and optional component lifecycle before product source implementation.

### Medium

1. Normalize terminology and stable requirement IDs across PRD, architecture, epics, and specs.
2. Separate replayable history from effect-specific reversible, compensatable, or irreversible actions.
3. Add async, streaming, cancellation, backpressure, approval suspension, and terminal-state semantics before freezing interfaces.
4. Define privacy-safe observability conventions, retention, local buffering, exporter policy, and erasure behavior.
5. Add startup, latency, memory, Ledger growth, cancellation, and resource budgets.
6. Treat one million tokens as overflow capacity, not working memory or a repository-size limit; require smallest-sufficient reproducible context.

### Low

1. Replace “masters BMAD / Spec-Kit / powerskills” with versioned, tested conformance profiles.
2. Make the glossary self-contained and normalize mode, gateway, power-skills, and hierarchy terms.
3. Order FRs canonically or generate a sorted requirement registry.

## Mechanical notes

- FR identifiers are unique but not presented in order; architecture and epic traceability is stale for FR-17 and FR-18.
- The Assumptions Index does not round-trip to inline assumption tags.
- The current Feature 001 plan and tasks remain blocked and encode superseded Goose- and hash-ID-specific choices.
- Spec Kit cannot currently select an active feature officially because the repository is on `master`, and required feature design artifacts, checklists, and analysis reports are missing.
- BMAD implementation readiness cannot run to PASS until the PRD, architecture, epics/stories, and Spec-Kit feature package are regenerated and validated.

## Required gate sequence

1. Update and revalidate the PRD until there are no critical findings.
2. Create the canonical requirement registry, threat models, one-way-door contracts, examples, and decision records.
3. Execute and review the bounded ADR-0003 evidence spikes; accept or revise the ADR.
4. Update and validate architecture; regenerate epics and concrete BMAD story artifacts.
5. On a real Spec-Kit feature branch, run clarify, plan, research, data model, contracts, quickstart, tasks, requirements-quality checklists, and analyze.
6. Run BMAD implementation-readiness and require zero critical findings.
7. Only then authorize a low-concurrency implementation swarm, followed by independent review, testing, contradiction, validation, and signed staging.

## Reviewer files

- `review-rubric.md`
- `review-architecture-adversarial.md`
- `review-security-governance.md`
- `review-feasibility-context.md`
