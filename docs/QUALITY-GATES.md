---
title: Skein Planning, Bootstrap, CI/CD, and Staging Quality Gates
status: remediation-draft-v2
updated: 2026-07-16
build_authorization: NOT_READY
---

# Skein Planning, Bootstrap, CI/CD, and Staging Quality Gates

## Gate Authority

This document is normative under `PROJECT-GOVERNANCE.md`. It governs design completion and the engineering substrate before product implementation. It does not authorize product code. Current authorization remains `NOT_READY` until every mandatory gate is independently approved and the owner records explicit authorization.

## Versioned Gate Manifest

The repository shall maintain a stable, machine-readable gate manifest before implementation tasks are generated. Every gate entry shall contain: stable gate and decision IDs; applicable requirement, risk, platform, and release IDs; expected artifact paths and schema versions; validation commands; pre-registered acceptance oracles, datasets, fixtures, denominators, exclusions, confidence/error treatment, and thresholds; accountable owner; independent reviewer and approver roles; evidence hashes and locations; freshness/expiry rules; waiver authority; status; and blocking dependencies.

Gate evaluation shall fail closed for missing, stale, ambiguous, unavailable, unverifiable, or orphaned critical evidence. Critical security, isolation, authorization, effect, secret, Ledger, mode-lifecycle, and build-provenance events require 100% evidence completeness. Lower operational telemetry SLOs may never substitute for critical evidence completeness.

## G0 — Planning and Traceability

Entry requires a remediation draft only. Exit requires validated BMAD PRD, accepted architecture and one-way-door ADRs, regenerated epics/stories, complete Spec-Kit clarification/research/data-model/contracts/quickstart/tasks/checklists/analyze artifacts, zero critical contradictions, zero stale or orphaned requirement IDs, and a requirement-to-design-to-task-to-test-to-evidence matrix. The assumptions register shall map each assumption to dependent claims and tests. Cross-artifact promotion is blocked until the report is mechanically clean.

## G1 — Pre-Code Bootstrap Contract

Before the first product-code commit, a contributor or agent on each declared Windows, macOS, and Linux candidate shall be able to clone a pinned revision and run one documented idempotent setup entry point. The contract shall define minimal-planning/core, contributor, local-inference, enterprise-connector, and multimodal profiles; pinned language/tool/framework versions; lockfiles and provenance; quality/test tools; BMAD, Spec-Kit, power-skill, and loop-engineering tooling; MCP development connections; secret-free fake identity/model/connector/secret systems; environment report schema; offline behavior; upgrade/rollback/uninstall; and recovery from a partially failed setup.

Bootstrap scripts, lockfiles, test fixtures, CI definitions, and environment documentation are permitted engineering-substrate changes under `NOT_READY`; they shall not contain product runtime behavior. Clean-machine evidence must be reproducible from the clone without pre-existing cloud credentials.

## G2 — Contract and Security Proof

Exit requires versioned schemas, positive/negative examples, compatibility rules, and independent conformance fixtures for every pre-implementation canonical contract. It also requires data-flow and threat models, authorization and delegation truth tables, identity profiles, MCP trust rules, computer grants, JIT root-of-trust and recovery decisions, strict-Local egress proof, mode/backend lifecycle proof, Ledger/effect/privacy reconciliation, and crash-boundary recovery evidence.

Strict Local permits only pre-registered local IPC/loopback TCB endpoints. Tests cover direct sockets, DNS, updates, discovery, child processes, remote tools/MCP, telemetry, identity, model, secret provider, and tracker traffic. If hard enforcement cannot be proven on a platform, strict Local is marked unsupported there.

Isolation release evidence shall execute a declared matrix across mode, silo, team, project, conversation, principal lifecycle, cache/index, storage, process, browser, telemetry, backup/export, replay, and client surface. “No known leak” is insufficient without 100% execution of the applicable matrix and zero disclosure.

## G3 — CI Quality Model

The CI contract shall define named stages, owners, entry/exit rules, immutable outputs, and promotion dependencies for formatting; linting; compilation/type checks; documentation; unit; property/mutation; integration; contract; E2E; accessibility; performance/resource; security and privacy; isolation; crash/recovery; migration/compatibility; SAST; dependency/license/secret scans; SBOM; provenance; signing; reproducible build; packaging; install/upgrade/rollback/uninstall; and capability-registry validation.

Branch protection shall require the gate-manifest jobs, independent review, no unresolved critical/high finding, and no evidence waiver for non-waivable invariants. Failure ownership and retry limits are explicit. Artifacts promote by digest through development, integration, security validation, staging, and release-candidate states; rebuilding an artifact resets downstream evidence.

## G4 — User Value and Acceptance Oracles

Acceptance oracles and baselines shall be frozen before final implementation evaluation. Local Alpha measures setup effort, time to first governed local value, completion rate for representative code/workflow journeys, recovery success, operator interventions, and task time against a documented baseline using the underlying tools separately. V1 additionally measures successful cross-surface task completion, policy consistency, connector-task burden, sustained voluntary use, and user trust/control outcomes. Counter-metrics include setup regressions, hidden manual work, approval fatigue, unsafe workarounds, resource exhaustion, cost, latency, and abandonment.

Numeric thresholds may be spike outputs only when the protocol, task classes, baseline freeze point, sample method, confidence treatment, and approving role were pre-registered before results were observed.

## G5 — Worker Assurance

Governed workers require complete capture of observable model I/O, context, tools, budgets, cancellation, policy, approvals, and effects. Reduced-assurance workers are visibly labeled; may not access protected or restricted context; may not perform mutation, external communication, secret use, privileged computer action, or terminal verification; and their outputs are untrusted suggestions requiring governed re-ingestion and independent verification.

When exact content is unavailable because of provider-side hidden reasoning, lawful erasure, encryption/key destruction, proprietary internals, or capture loss, the UI and evidence API shall show an explicit typed unavailable-evidence marker with reason, scope, time, consequence, and replay limitation. It shall never imply that hidden reasoning was captured or that an incomplete record is exact.

## G6 — Staging and Autonomous Work

No autonomous implementation swarm may start before G0–G5 pass and explicit build authorization changes from `NOT_READY`. After authorization, each task requires approved BMAD and Spec-Kit references, exclusive ownership, isolated workspace, allowed tools/destinations/credentials/effect classes, reproducible ContextManifest, loop budgets, pre-registered action/iteration/terminal oracles, rollback/recovery, and expected EvidenceBundle.

Author, reviewer, adversarial challenger, test/evaluation agent, integration owner, and final human approver remain distinct. Shared model/prompt lineage is not independent evidence. Integration is serialized. Staging requires traceability, regression, security, isolation, recovery, provenance, install/rollback, user-value, and release-capability evidence. Any contradiction in authority or security boundaries stops promotion.

## Feasibility Guardrails

A mature Skein repository may exceed one million source tokens. A million-token model window is overflow capacity, not normal working memory and not a repository-size target. Acceptance requires smallest-sufficient context, repository and symbol maps, dependency graphs, ACL-aware hybrid retrieval, lazy loading, and traceable compression. Rapidly produced agent or coding-tool clones demonstrate prototype feasibility only; they are not evidence of enterprise maturity, cross-platform reliability, security, governance, maintainability, or compliance-support readiness.
