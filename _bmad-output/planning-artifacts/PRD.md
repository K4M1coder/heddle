---
title: Skein
created: 2026-07-15
updated: 2026-07-15
---

# PRD: Skein
*Local-first AI agentic tool, unifying chat, code and cowork.*

## 0. Document Purpose
This PRD is intended for the project owner, open-source contributors, product stakeholders, and downstream workflows (architecture, epics/stories). It defines the **what** and the **why**; the **how** lives in `architecture.md`. The exhaustive design reference is `docs/superpowers/specs/2026-07-15-skein-design.md`. Vocabulary is anchored in §3 Glossary; features contain nested FRs; assumptions are indexed in §9.

## 1. Vision
Skein is a **single, local-first AI agentic tool**, bringing together **chat**, **code** and **cowork** (PC control) behind a headless core equipped with an advanced harness (context, tools, skills). It connects to **all AI providers** (cloud and local), embeds its own inference, natively integrates business connectors (Atlassian, M365) via MCP, and masters the **BMAD / Spec-Kit / powerskills** methods. It gives the user **full transparency and reversibility** (each step is an inspectable/replayable "commit") and **enterprise compliance** (identity, RBAC, audit, GDPR/ISO/SOC2/AI Act/NIS2).

## 2. Target User

### 2.1 Jobs To Be Done
- Develop/assist on code in an agentic way, using TDD, with subagents.
- Drive Jira/Bitbucket/Confluence and M365 from a single tool.
- Freely choose the model (sovereign cloud or offline local) depending on sensitivity.
- Automate PC (cowork) and web tasks.
- Stay in control: see *everything* sent to the models, and be able to undo/replay.
- Work as a team with governance (roles, shared config, compliance).

### 2.2 Non-Users (v1)
Users looking for a simple web chatbot without local execution; use cases without a real workstation (cowork requires a workstation).

### 2.3 Key User Journeys
- **UJ-1. The engineer chains spec → code → PR → ticket.** From the CLI or the UI, they read a Confluence spec, generate a Spec-Kit plan, code in TDD, open a Bitbucket PR, and create a Jira ticket — switching cloud↔local. **Climax:** the PR and the ticket exist, and the session is persisted. **Edge:** offline, the tool falls back to Local mode (local model).
- **UJ-2. The project manager governs the team.** They edit the team harness layer, lock security settings, and assign roles; members inherit the baseline and locally override the rest.
- **UJ-3. The user audits an agent decision.** Via `skein ledger`, they inspect the exact prompt sent to the model and the raw response, then replay or undo the step.

## 3. Glossary
- **Silo** — sealed data partition tied to a mode (and a team in Remote).
- **Mode** — Local / Online-Server (leader) / Online-Remote (follower).
- **Harness** — configuration of the agent's behavior (instructions, tools, skills, context, policies), editable in team/local layers.
- **Ledger** — append-only, hash-chained journal (model I/O, tools, state), git-style.
- **Connector** — MCP server exposing a tool/resource (Jira, M365, fs, git…).
- **Gateway** — OpenAI-compatible LiteLLM gateway to 100+ providers.
- **Controller** — abstraction for driving an external surface (PC, browser).
- **Principal / RBAC / SecretProvider / IdP** — see `architecture.md` and design §7.

## 4. Features

### 4.1 Agentic code assistant
**Description:** read/edit files, run commands (sandbox), agentic loop, subagents, TDD. Realizes UJ-1.
**Functional Requirements:**
#### FR-1: Headless agentic loop
The user can launch a task (CLI/API/UI) that runs plan→tools→eval through to completion. Realizes UJ-1.
- **Consequences (testable):** a session reads/writes a file and is persisted, then reloaded from the silo.
#### FR-2: fs/git/shell connectors (MCP)
The agent can manipulate files, git and shell through MCP connectors, with destructive actions requiring confirmation.

### 4.2 Multi-provider & local inference
**Description:** switch cloud↔local through the Gateway; embedded inference server (Ollama/llama.cpp; vLLM optional).
#### FR-3: Provider selection
The user can route to a cloud OR local provider; in Local mode, only local providers are allowed (egress OFF).

### 4.3 Atlassian & M365 connectors
#### FR-4: Jira/Bitbucket/Confluence + Outlook/SharePoint/Teams via MCP, usable in workflows.

### 4.4 BMAD / Spec-Kit / powerskills frameworks
#### FR-5: These methods are packaged as invocable recipes/skills (`/spec`, `/bmad`, …).

### 4.5 Modes, silos & harness governance
#### FR-6: 3 auto-detected modes, switching proposed (never imposed), local fallback; sealed silos; sharing partitioned by team in Remote.
#### FR-7: Harness editable in a team layer (leads, lockable) + a local layer (override except locks).

### 4.6 Identity, RBAC, observability, compliance
#### FR-8: Pluggable IdP (local/LDAP/OIDC/Entra/Google) + RBAC with 3 scopes (global/silos/intra-silo), deny-by-default.
#### FR-9: OpenTelemetry observability + immutable audit from v1.

### 4.7 Traceability & reversibility (Ledger)
#### FR-10: Every step (exact model I/O, tools, state) is captured, inspectable, replayable, reversible (`skein ledger log|show|replay|revert|branch`).

### 4.8 Secrets management
#### FR-11: Pluggable SecretProvider (SOPS+age/1Password/OpenBao/Infisical/OS keychain), **JIT resolution**, reference-not-value, log redaction, offline-only in Local mode.

### 4.9 Cowork & multimodal (v2+)
#### FR-12: PC control + browser companion (hybrid Controller); perception (doc/image/audio/grounding); generation (image/TTS/Office/video); omni (orchestration); real-time voice; multilingual translation. Detail: roadmap §6 + design §8.

### 4.10 Workflow engine (native, Archon-inspired)
**Description:** the harness natively sequences multi-agent actions across the connected tools, over the entire SDLC (design→dev→tests→packaging→deployment). Realizes UJ-1.
#### FR-13: Event-sourced workflow
The user/agent can define and execute a workflow (agent/tool/subagent/approval/condition/parallel/loop nodes); each step is logged in the Ledger (durable, replayable, resumable). Goose recipes and BMAD/Spec-Kit flows are external formats projected into the canonical workflow/artifact model.
- **Consequences (testable):** an interrupted workflow can be resumed from the last Ledger Step.
#### FR-16: Engine-enforced loop control (loop engineering)
Every agent loop and loop node is governed by a `LoopController`: externally-enforced termination (iteration/token/cost budget + no-progress detection), ground-truth-anchored reflect/retry (tests/compiler/tools, not self-judgment), three verification levels (action/iteration/terminal), and human escalation on threshold breach. Node types: ReAct, Reflexion, Self-Refine, evaluator-optimizer. See design §4.14, research `docs/research/loop-engineering.md`.
- **Consequences (testable):** a loop stops at its budget even if the model would continue; a reflect/retry step consumes external ground truth (e.g. a failing test) rather than model self-assessment.

### 4.11 Task tracking & hierarchy
#### FR-14: Pluggable TaskTracker
The user can track tasks through an interchangeable backend: local (silo), **Vikunja** (embedded OSS) or **Jira** (via MCP). Workflows reflect their progress there.
#### FR-15: Hierarchy & config resolution
Data/config is organized into **Silo ▸ Team ▸ Project ▸ Conversation** (Local mode: without Team). Values and locks are separate: without a lock, the most specific value wins; the highest explicit lock caps lower scopes. Security settings form a monotonic floor, so lower scopes may tighten but never weaken them.
- **Consequences (testable):** an explicitly locked TaskTracker at silo scope applies below; an unlocked default may be overridden at project or conversation scope.

### 4.12 Context management and agent workers
#### FR-17: Reproducible smallest-sufficient context
Every model call has a `ContextManifest` that records selected sources, source hashes, classifications, token allocation and selection rationale. Repository maps, symbol/dependency indexes, hybrid retrieval, lazy loading and trajectory compression are used before whole-repository loading. Million-token windows are overflow capacity, not default working memory.

#### FR-18: Replaceable governed workers
Skein owns the agent loop and may delegate bounded work to compatible workers (native, Goose, OpenCode, Cline, Hermes, Claude Code or others). A worker is eligible only when its contract exposes the model/tool/approval/termination events required by policy and Ledger capture.

## 5. Non-Goals (Explicit)
- No wholesale rewrite of commodity infrastructure. Skein implements its differentiating control plane and reuses model, MCP, browser, storage, inference and observability components behind adapters.
- No separate server product (the team backend = an exposed instance).
- No dependence on a single provider (AI, IdP, secrets).
- No simple web chatbot without local execution.

## 6. MVP Scope

### 6.1 In Scope (v1)
Agentic code assistant · multi-provider + local inference · Atlassian+M365 connectors · BMAD/Spec-Kit/powerskills frameworks · **native workflow engine (event-sourced Ledger) + TaskTracker (local/Vikunja/Jira)** · **Silo▸Team▸Project▸Conversation hierarchy & config resolution** · modes & silos (full Local, baseline Server/Remote + team authz) · Chat+Code UI · local identity + baseline RBAC · observability · Ledger · SecretProvider foundation · compliance-by-design.

### 6.2 Out of Scope for MVP (roadmap)
- **v2** Perception (multimodal inputs) · **v3** Cowork/control · **v4** Media generation · **v5** Video · **v6** Omni · **v7** Real-time voice · **v8** Multilingual translation.
- External IdPs + advanced RBAC + certifications: enterprise track (in parallel).

## 7. Success Metrics
**Primary**
- **SM-1**: complete UJ-1 end-to-end (spec→PR→ticket) from CLI *and* UI *and* API, switching cloud↔local. Validates FR-1..FR-5.
- **SM-2**: silo isolation proven by test (a write invisible outside the silo/team). Validates FR-6.
**Secondary**
- **SM-3**: any model I/O inspectable via `skein ledger`. Validates FR-10.
**Counter-metrics (do not optimize)**
- **SM-C1**: do not gain speed by bypassing confirmations/redaction/egress — security takes precedence over latency.

## 8. Open Questions
1. Worker strategy — native Rust loop is the baseline; bounded spikes determine whether Goose, OpenCode, Cline or other workers satisfy the turn-level governance contract.
2. Context quality — benchmark smallest-sufficient retrieval against full-context loading, including million-token and middle-position cases.
3. Exact token-level capture format via Gateway→Ledger ingestion.
4. Local-first identity model (key pairs) → transition to enterprise OIDC.

## 9. Assumptions Index
- [ASSUMPTION §2] Cowork requires a real workstation (no pure thin client).
- [ASSUMPTION §4.2] Ollama is the default local inference engine, cross-platform.
- [ASSUMPTION §6] v1 is text; multimodal is strictly v2+.

## Compliance & Regulatory *(adapt-in: regulated domain)*
GDPR (minimization via Local mode, admin right to erasure, residency, retention), ISO 27001 & SOC 2 (RBAC, audit, encryption, config-as-code change management), EU AI Act (transparency of "AI content", human oversight, Ledger traceability), NIS2 (logging/incident, MCP supply chain, governance). The software provides the controls; certification is organizational.

## Audit Trail / Decision Provenance *(adapt-in)*
Two complementary journals: **audit** (who/when) and **Ledger** (exactly what, in/out). Cross-cutting foundation of compliance.
