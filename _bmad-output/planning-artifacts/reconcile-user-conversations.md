---
title: Heddle User Conversation Reconciliation Extract
document_type: source-extraction-report
status: draft-for-bmad-input
created: 2026-07-16
sources:
  - complete parent conversation in the forked context
  - referenced ChatGPT conversation "1 million de tokens"
canonical: false
---

# Heddle User Conversation Reconciliation Extract

## 1. Purpose and Method

This report extracts durable product input from the complete parent conversation and the referenced ChatGPT conversation. It is an input-reconciliation artifact, not a PRD, architecture, plan, or implementation authorization. It does not normalize away the user's qualitative intent.

Status meanings:

- **Covered**: explicitly represented in the current PRD with substantially equivalent intent.
- **Partial**: represented, but material scope, semantics, acceptance detail, or qualitative intent is absent.
- **Missing**: no explicit equivalent was found in the current PRD.
- **Contradictory**: the current PRD conflicts with, narrows, or ambiguously interprets the stated input.

Source labels:

- **P**: complete parent conversation.
- **R**: referenced ChatGPT conversation.
- **P+R**: stated or materially reinforced in both.

## 2. Durable Requirement Reconciliation

### 2.1 Product identity, ownership, language, and positioning

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| ID-01 | P | The project name/codename is **Heddle**. | Title; §1 | Covered | Preserve the name unless the owner explicitly reopens naming. |
| ID-02 | P | Heddle is an independent open-source project owned by Cédric Thedrez. | None | Missing | Ownership and provenance must not be inferred from repository location or prior employers. |
| ID-03 | P | Owner identity: Cédric Thedrez, GitHub alias `K4M1coder`, alias `cethgame` elsewhere. | None | Missing | Needed for project metadata, copyright, governance, and release ownership. |
| ID-04 | P | Heddle has no relationship with SodiusWillert or any of its entities. | None | Missing | Explicit prohibition: do not mention or imply affiliation. |
| ID-05 | P | All source code and persistent documentation must be in English. | None | Missing | Applies to code, comments, schemas, examples, scripts, operational docs, and generated repository artifacts. |
| ID-06 | P | Conversation with the owner may be in French. | None | Missing | Interaction preference, not a product localization requirement by itself. |
| ID-07 | P | BMAD and Spec-Kit artifacts should also be in English. | None | Missing | The final preference favored English for all persistent artifacts. |
| ID-08 | P+R | Deliver one coherent product/package/repository/install experience, while retaining modular internals and supervised auxiliary processes where needed. | §1; §5 “No separate server product” | Partial | “Single product” must not be normalized into “single binary” or “single process.” |
| ID-09 | P+R | Position Heddle between or beyond Claude Code, GitHub Copilot, OpenCode, OpenClaw, Goose, Hermes, Open WebUI, Archon, and related tools. | §1; FR-18 | Partial | This is a capability and experience benchmark, not a requirement to clone branding or internals. |
| ID-10 | P+R | Prefer integration, reuse, inspiration, compatible libraries/crates, or controlled forks over rebuilding commodity components; implement internally only where existing components fail the requirements. | §5 first non-goal | Covered | Preserve replaceability and license review. |
| ID-11 | P+R | The tool must feel simple and complete despite the breadth of the platform. | None | Missing | **Qualitative intent at risk:** architectural completeness must not leak as operational complexity. |
| ID-12 | P+R | The product must adapt to the user's environment rather than require one fixed enterprise or cloud topology. | §1; FR-3; FR-6; FR-8 | Partial | **Qualitative intent at risk:** adaptability spans hardware, connectivity, identity, providers, tools, and deployment. |

### 2.2 Core product surfaces and execution model

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| SUR-01 | P+R | Unify chat, coding, and cowork/computer-assistance functions in one interface. | §1; FR-12; §6 | Covered | Cowork is roadmap-staged, but remains part of the product identity. |
| SUR-02 | P | The UI must be a layer over the CLI/headless core. | §1 “headless core”; FR-1 | Partial | The PRD does not explicitly require UI operations to use the same public command/application contracts. |
| SUR-03 | P | The entire application must function from the command line. | FR-1; SM-1 | Covered | “Entire” includes administration and configuration, not only chat/code journeys. |
| SUR-04 | P | The entire application must function through an API. | FR-1; SM-1 | Covered | The API must not be a reduced secondary surface. |
| SUR-05 | P | The desktop UI, CLI, and API must expose equivalent core capabilities. | SM-1 | Partial | SM-1 proves one journey, not comprehensive surface parity. |
| SUR-06 | P | Desktop is the primary packaged experience, with an always-functional backend. | §1; §5 | Partial | The PRD does not explicitly state desktop packaging or backend lifecycle. |
| SUR-07 | P | The backend always exists locally; only its network exposure is configurable. | §5 “team backend = exposed instance” | Partial | Must distinguish service availability from network publication. |
| SUR-08 | P | When connected to another exposed Heddle instance, the local backend stops serving as the active backend. | None | Missing | Requires lifecycle, handoff, local capability behavior, and failure-recovery semantics. |
| SUR-09 | P+R | Heddle must run usefully on a single ordinary desktop with no team or cloud dependency. | §1; §6.1 | Partial | **Qualitative intent at risk:** local independence must remain a first-class acceptance property. |
| SUR-10 | P | Cross-platform support is required for Windows, Linux, and macOS. | Assumption §4.2 only references Ollama | Missing | Applies to product, bootstrap, packaging, connectors, and tests—not merely inference. |
| SUR-11 | P+R | A single user command may launch a long autonomous engineering campaign, but internally it must be decomposed into durable, gated, resumable work. | FR-13; FR-16 | Partial | One user initiation is acceptable; one unbounded model trajectory is not. |

### 2.3 Connectivity modes, silos, teams, and isolation

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| MOD-01 | P | Detect online/offline conditions and propose an appropriate mode. | FR-6 | Covered | Detection must inform, not silently override user intent. |
| MOD-02 | P | The user can easily switch among Local, Online-Server, and Online-Remote modes. | FR-6 | Covered | Mode transitions require explicit state and understandable consequences. |
| MOD-03 | P | Mode switching is proposed, never imposed. | FR-6 | Covered | Preserve user control even when connectivity changes. |
| MOD-04 | P | Local mode is the default and must support fully local operation. | FR-3; §6.1 | Partial | “Default” is not explicit in the PRD. |
| MOD-05 | P | Local mode must prohibit unintended network egress. | FR-3 | Covered | Requires enforcement outside the model. |
| MOD-06 | P | Data and information must not be shared between sessions belonging to different modes. | FR-6; SM-2 | Covered | Isolation applies to context, memory, indexes, secrets, logs, and artifacts. |
| MOD-07 | P | Remote-mode sharing is restricted to members of the same team. | FR-6 | Covered | Team membership must be policy-enforced, not prompt-enforced. |
| MOD-08 | P | Local, Server, and Remote sessions require sealed, non-cross-contaminating silos. | Glossary; FR-6 | Covered | Preserve explicit session/mode boundary semantics. |
| MOD-09 | P | Remote mode connects to an exposed backend from another application instance. | Glossary; FR-6 | Partial | Discovery, trust establishment, failover, and reversion are not specified. |
| MOD-10 | P | Online-Server mode exposes the instance backend to authorized team users. | Glossary; FR-6 | Partial | Network scope, discovery, authentication, TLS, and owner consent are absent. |
| MOD-11 | P | Team collaboration must remain possible on a self-hosted local instance without enterprise SaaS. | §6.1; FR-6 | Partial | **Qualitative intent at risk:** small-team collaboration must not depend on external identity or task services. |
| MOD-12 | P | Silo/project/conversation owners may authorize connectors subject to hierarchical authority. | FR-15 | Partial | Connector authorization and inheritance semantics are not explicit. |
| MOD-13 | P | Connectors are denied or local-only by default unless authorized at the proper scope. | FR-8 deny-by-default; FR-15 | Partial | Default MCP availability and authorization inheritance need explicit requirements. |

### 2.4 Harness, context, skills, tools, agents, and workflow

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| HAR-01 | P+R | Provide an advanced agentic harness comparable in ambition to Claude Code, OpenClaw, Hermes, Goose, and OpenCode. | §1; FR-18 | Partial | Benchmark includes permissions, sessions, context, tools, skills, hooks, workers, and ergonomics. |
| HAR-02 | P | The harness must provide advanced context management. | §1; FR-17 | Covered | Context selection must be reproducible and inspectable. |
| HAR-03 | P | The harness must provide tool lifecycle, discovery, selection, authorization, and execution management. | FR-2; FR-15; FR-18 | Partial | A complete tool registry/lifecycle requirement is not explicit. |
| HAR-04 | P | The harness must provide native skills management. | FR-5 | Partial | Packaging recipes is narrower than skill discovery, scope, versioning, precedence, trust, and governance. |
| HAR-05 | P | Team leads and project leads can modify team/project harness behavior. | FR-7; FR-15 | Covered | Project-lead authority should be explicit in RBAC mappings. |
| HAR-06 | P | A local user can modify the local harness. | FR-7 | Covered | Locks and security floors still apply. |
| HAR-07 | P | Hierarchical authority determines which harness settings, tools, connectors, and skills can be enabled or overridden. | FR-7; FR-15 | Partial | Tools/skills/connector authorization is not fully bound to hierarchy. |
| HAR-08 | P | The tool must natively sequence multi-agent actions across connected tools through workflows. | FR-13 | Covered | Workflow is a core capability, not a thin integration. |
| HAR-09 | P+R | Workflows must support agents, tools, subagents, approvals, conditions, parallelism, loops, retries, and human gates. | FR-13; FR-16 | Covered | Preserve deterministic boundaries around model-directed work. |
| HAR-10 | P+R | Use an Archon-like approach where useful, or integrate an existing durable workflow component if it satisfies Heddle's contracts. | FR-13 | Partial | The current wording says “inspired” but does not preserve the evaluate-before-build decision. |
| HAR-11 | P+R | Support interchangeable external agent workers such as Claude Code, Codex, OpenCode, Goose, and Hermes when governable. | FR-18 | Covered | External workers must not own authorization or canonical state. |
| HAR-12 | P+R | Heddle must guarantee the harness and workflow independently of the selected model or worker. | FR-16; FR-18 | Partial | **Qualitative intent at risk:** consistency is the product value, not just worker compatibility. |
| HAR-13 | P+R | Every loop must have external termination criteria and bounded iteration/token/cost/time or resource budgets. | FR-16 | Covered | The user specifically rejected unbounded “one-shot” autonomy. |
| HAR-14 | P+R | Verification and retry must consume ground-truth feedback such as tests, compiler results, tool results, or human judgment. | FR-16 | Covered | Model self-confidence is not an acceptance oracle. |
| HAR-15 | P+R | Detect stagnation/no progress and escalate or stop. | FR-16 | Covered | Terminal states must be explicit. |
| HAR-16 | P+R | Support plan-act-observe, ReAct, Reflexion, Self-Refine, and evaluator-optimizer patterns as controlled workflow patterns, not hype labels. | FR-16 | Covered | “Loop engineering” is an engineering umbrella, not a settled academic standard. |
| HAR-17 | P+R | Separate implementer and independent evaluator/reviewer roles for consequential work. | None | Missing | Needed for the future autonomous engineering swarm and quality gates. |
| HAR-18 | P+R | Use isolated workspaces/worktrees and controlled runtimes for parallel implementation tasks. | None | Missing | Mentioned as a required safe campaign pattern in the referenced conversation. |
| HAR-19 | P+R | Long workflows must survive crashes and resume from durable checkpoints. | FR-13 | Covered | Replay alone is insufficient without deterministic state restoration. |
| HAR-20 | P+R | A workflow/task is complete only with requirement, implementation, automated test, evidence, and independent review. | None | Missing | This is a development and product workflow invariant. |

### 2.5 BMAD, Spec-Kit, powerskills, and development sequencing

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| MET-01 | P | Heddle itself must master and expose BMAD, Spec-Kit, and powerskills by default. | FR-5 | Covered | Product capability. |
| MET-02 | P | Heddle's own conception and implementation must follow official BMAD usage. | None | Missing | Process requirement for this project, distinct from FR-5. |
| MET-03 | P | Heddle's own conception and implementation must follow official Spec-Kit usage. | None | Missing | Process requirement for this project, distinct from FR-5. |
| MET-04 | P | BMAD and Spec-Kit must be bridged through a real, operational artifact flow, not merely co-installed or represented by prompts. | FR-5 | Partial | Current PRD does not define bridge completeness. |
| MET-05 | P | Verify whether an established BMAD–Spec-Kit bridge exists and use official conventions rather than claiming completion prematurely. | None | Missing | Requires sourced evaluation and bridge acceptance evidence. |
| MET-06 | P | Complete conception before launching implementation. | None | Missing | Explicit sequencing gate. |
| MET-07 | P | Do not launch a swarm of implementation agents until specs, architecture, clarifications, checklists, examples, snippets, quality gates, and validation are complete. | None | Missing | Explicit prohibition and readiness definition. |
| MET-08 | P | Before implementation, run agents for contradiction, review, validation, and design challenge where useful. | None | Missing | Review agents are permitted during conception; coding agents are not. |
| MET-09 | P | The eventual swarm must cover implementation, review, testing, contradiction, validation, and staging as separate governed responsibilities. | None | Missing | Requires role separation and evidence-producing handoffs. |
| MET-10 | P | Risks and open questions must be explicitly addressed, not deferred without triggers and owners. | §8 | Partial | Listing four questions is not a closure process. |
| MET-11 | P | Spec-Kit must include clarification, plan, research, data model, contracts, quickstart, tasks, checklists, and cross-artifact analysis as applicable. | None | Missing | Existing feature artifacts are incomplete according to the user's stated acceptance standard. |
| MET-12 | P | BMAD must include the required planning documents, validation, implementation-readiness assessment, epics/stories, and missing official artifacts. | None | Missing | The user explicitly challenged claims of perfect BMAD implementation. |
| MET-13 | P | The project development process must itself adopt loop-engineering discipline. | FR-16 only covers product loops | Partial | Process loops need criteria, budgets, evidence, review, and termination. |
| MET-14 | P | Claude CLI may be used as an optional worker if it improves execution, but it is not required and must not displace the governed process. | FR-18 | Covered | Worker choice remains replaceable. |
| MET-15 | P | Computer-control MCP capabilities may be used during development when appropriately authorized. | None | Missing | This grants optional capability, not blanket authorization for destructive or external actions. |

### 2.6 Ledger, transparency, replay, revision, and evidence

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| LED-01 | P+R | Every step must be reversible, replayable, or revisable like commits in code. | FR-10 | Covered | The exact available operation may depend on whether an external effect is inherently reversible. |
| LED-02 | P | Users must see everything sent to every model, not only the produced result. | FR-10 | Covered | Includes system/developer instructions, selected context, attachments, tool schemas, and transformed input. |
| LED-03 | P | Users must see everything returned by every model, including intermediate output, not only final artifacts. | FR-10 | Covered | Provider limitations must be recorded honestly. |
| LED-04 | P+R | Tool calls, policy decisions, approvals, state transitions, tests, and evidence must be correlated with model I/O. | FR-10; FR-13 | Partial | FR-10 says tools/state but does not enumerate complete evidence correlation. |
| LED-05 | P+R | Preserve provenance, hashes, model/provider identity, timing, and execution outcomes for audit and reproduction. | FR-9; FR-10; FR-17 | Partial | Exact canonical evidence contract remains absent. |
| LED-06 | P+R | Reversal of external or irreversible effects requires compensating actions or prior approval; replay must not duplicate side effects. | FR-10 | Partial | “Reversible” currently overpromises without effect/idempotency semantics. |
| LED-07 | P+R | Ledger/audit data may contain personal data and secrets; retention, encryption, access, redaction, and erasure semantics must reconcile transparency with GDPR. | Compliance; FR-11 | Partial | Current PRD does not resolve exact-history versus erasure tension. |
| LED-08 | P+R | Outputs of an engineering campaign must include evidence bundles, not code alone. | None | Missing | Evidence includes diffs, test results, quality/security scans, policy decisions, traces, and approvals. |
| LED-09 | P+R | Maintain automatic requirements traceability from requirement to design, code, test, evidence, and status. | None | Missing | Central protection against silent requirement loss. |
| LED-10 | P | Full transparency is a core product promise, not an optional debug mode. | §1; UJ-3; FR-10 | Covered | **Qualitative intent at risk:** summaries must never replace access to raw captured records. |

### 2.7 Models, inference, multimodality, and omni behavior

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| AI-01 | P+R | Connect through APIs to all practical cloud AI providers. | §1; FR-3 | Covered | “All” should be implemented through capability-based adapters, not hard-coded equivalence. |
| AI-02 | P+R | Connect to local inference tools including LM Studio, Ollama, and vLLM. | FR-3 mentions local; §4.2 mentions Ollama/llama.cpp/vLLM | Partial | LM Studio is absent. |
| AI-03 | P+R | Include an inference server or a managed local inference capability. | §4.2 | Covered | Embedded may mean bundled/supervised adapter, not necessarily rewritten inference. |
| AI-04 | P+R | Route by modality, capability, data classification, locality, cost, latency, residency, and resource availability. | FR-3 | Partial | Current provider selection is much narrower. |
| AI-05 | P+R | The omni experience may combine several specialist models in parallel or sequence; it does not require one universal model. | FR-12; roadmap v6 | Partial | **Qualitative intent at risk:** the seamless “omni illusion” is orchestration behavior. |
| AI-06 | P+R | Local generation must eventually cover text and code. | FR-1; FR-3 | Covered | Core capability. |
| AI-07 | P+R | Local generation must eventually cover images, audio, video, textures, and 3D objects through integrated engines/tools. | FR-12 | Partial | Textures and 3D objects are not explicit in the PRD. |
| AI-08 | P+R | The system must eventually support local STT, TTS, translation, and real-time speech pipelines. | FR-12; §6.2 | Covered | Real-time latency criteria are not yet captured. |
| AI-09 | P+R | Resource-broker behavior should schedule local models according to GPU/CPU/RAM/VRAM, residency, loading cost, priority, interactive versus batch work, and hardware capability. | None | Missing | Required for practical multi-engine local omni operation. |
| AI-10 | P+R | Cloud use is optional and policy-controlled; some connected SaaS actions necessarily communicate remotely. | FR-3; FR-4 | Partial | Must distinguish local processing from remote business-system access. |
| AI-11 | R | The product should support advanced ACL-aware multimodal RAG and memory rather than vector-only document search. | FR-17 | Partial | Ingestion, hybrid retrieval, reranking, provenance, deletion propagation, temporal/graph/code indexes, and ACL trimming are not fully stated. |
| AI-12 | R | Distinguish session, personal, project, team, and organizational memory; promotion to durable memory must be explicit, traceable, revocable, and retained by policy. | FR-15; FR-17 | Partial | Memory classes and promotion semantics are missing. |

### 2.8 Requested release sequence and modalities

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| REL-01 | P | Reorder the originally proposed MVP numbering into a coherent dependency-based roadmap. | §6.2 | Covered | The normalized v1–v8 sequence is accepted as current direction. |
| REL-02 | P | v1: agentic core, providers/inference, connectors, methodologies, workflow, modes/silos, Chat+Code, identity/RBAC baseline, observability, Ledger, secrets, compliance-by-design. | §6.1 | Covered | v1 remains broad and must be sliced internally. |
| REL-03 | P | v2: multimodal perception inputs—images, documents, memory, web content, screenshots, and audio. | §6.2 says “Perception (multimodal inputs)” | Partial | The explicit modality/source list is only in FR-12 shorthand/design reference. |
| REL-04 | P | v3: cowork/computer control. | §6.2 | Covered | This differs from the user's earliest numbering and reflects requested rearrangement. |
| REL-05 | P | v4: generate Office files, text, audio, and images/media. | §6.2 says “Media generation” | Partial | Office files and text should remain explicit. |
| REL-06 | P | v5: animated images and video. | §6.2 says “Video” | Partial | Animated images are absent. |
| REL-07 | P | v6: omni input/output experience through coordinated specialist models and tools. | §6.2; FR-12 | Covered | Omni is an experience layer, not a single-model mandate. |
| REL-08 | P | v7: real-time audio interaction. | §6.2 | Covered | Requires duplex/streaming behavior and latency acceptance later. |
| REL-09 | P | v8: real-time multilingual translation for Teams or team chat so each participant can write, read, speak, and hear in their native language. | §6.2 says “Multilingual translation”; FR-12 | Partial | The collaboration scenario and four-direction language experience are missing. |
| REL-10 | R | Eventually generate complete applications, large applications, games, websites, dashboards, monthly/annual reports, and creative assets through governed workflows. | §1; UJ-1; FR-12/13 | Partial | These are durable outcome examples and future acceptance journeys, not a promise that one model does everything. |
| REL-11 | R | Eventually assist with email, calendar, shopping lists, appointments, planning, and task tracking locally where possible and through authorized connected services where necessary. | FR-4; FR-14 | Partial | Personal productivity breadth is not represented. |

### 2.9 MCP, integrations, computer access, and task tracking

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| INT-01 | P+R | Integrate MCP clients/connectors and support local MCP servers. | FR-2; FR-4 | Covered | MCP transport alone is not the policy boundary. |
| INT-02 | P | Provide Atlassian Jira, Bitbucket, and Confluence connectivity, preferably with local MCP server options where feasible. | FR-4 | Partial | Local-server versus remote-service adapter topology is not explicit. |
| INT-03 | P+R | Provide Microsoft 365 connectivity including Outlook, SharePoint, Teams, and relevant Graph-backed services. | FR-4 | Covered | Read and mutation capabilities require separate policies. |
| INT-04 | P+R | Provide Google Workspace connectivity. | FR-8 mentions Google identity only | Missing | Drive, Gmail, Calendar, collaboration, and administration connectors are not in functional scope. |
| INT-05 | P+R | Support common development, productivity, browser, creative, office, and enterprise tools through replaceable adapters/plugins. | §5; FR-12; FR-18 | Partial | No explicit capability/plugin registry requirement. |
| INT-06 | P | MCP connectors may be embedded/bundled when safe and license-compatible. | None | Missing | Bundling must not imply automatic authorization or activation. |
| INT-07 | P | Connector availability and activation are controlled by silo/project/conversation owners according to hierarchy; default is full local/minimal exposure. | FR-15 | Partial | Scope ownership and default connector set are missing. |
| INT-08 | P+R | Place a Heddle policy gateway in front of MCP/tool execution for schema validation, authorization, approvals, isolation, redaction, rate limits, and audit. | FR-8/9/11/13 | Partial | The composed gateway behavior is not an explicit FR. |
| INT-09 | P+R | Separate read/search capability from mutation/action capability and require stronger controls for consequential actions. | FR-2 confirmation | Partial | Applies to every connector, not only destructive shell/file actions. |
| INT-10 | P+R | Provide real browser automation using a controlled browser engine and evidence capture. | FR-12 | Partial | “Browser companion” does not explicitly cover profiles, sessions, downloads, domains, screenshots, and traces. |
| INT-11 | P | Provide computer control via virtual keyboard/mouse plus screen and window capture. | FR-12 | Partial | Exact control and perception primitives are not enumerated. |
| INT-12 | P | Computer access can be restricted to a project, a selected folder, or the entire computer. | None | Missing | Full-computer access must be explicit, exceptional, visible, and revocable. |
| INT-13 | P | Computer/tool scope is selected and authorized by the proper owner according to hierarchy. | FR-8; FR-15 | Partial | Scope grants and inheritance need explicit contracts. |
| INT-14 | P | Task tracking may use Jira or an embedded OSS equivalent in the backend. | FR-14 | Covered | Local/Vikunja/Jira fulfills the stated direction. |
| INT-15 | P | Workflow progress should synchronize with the configured task tracker. | FR-14 | Covered | The task tracker must not become canonical workflow state. |

### 2.10 Identity, authorization, secrets, security, and compliance

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| SEC-01 | P | Support a local user database. | FR-8 | Covered | Required for independent/offline operation. |
| SEC-02 | P | Support LDAP directories. | FR-8 | Covered | AD-compatible LDAP behavior should be clarified later. |
| SEC-03 | P | Support OIDC identity providers. | FR-8 | Covered | Includes self-hosted and enterprise IdPs. |
| SEC-04 | P | Support Entra ID groups. | FR-8 | Covered | Group-to-role mapping and synchronization remain unspecified. |
| SEC-05 | P | Support Google Workspace groups/identity. | FR-8 | Covered | Group lifecycle details remain unspecified. |
| SEC-06 | P+R | Provide advanced RBAC by roles and permissions across the application, silos, and functions/settings inside a silo. | FR-8 | Covered | The current three scopes preserve the requirement at a high level. |
| SEC-07 | P+R | Complement RBAC with attribute-, relationship-, and risk-aware policy where needed. | FR-8 only says RBAC | Missing | Needed for project ownership, data classes, environment, action risk, and resource relationships. |
| SEC-08 | P+R | Authorization and policy decisions must be enforced outside the model; an agent never authorizes itself. | FR-8 deny-by-default | Partial | Enforcement boundary should be explicit. |
| SEC-09 | P+R | Content from web, documents, RAG, tools, and MCP is untrusted data, never system instruction. | None | Missing | Prompt-injection boundary and provenance requirement. |
| SEC-10 | P+R | Integrate 1Password CLI (`op`) for secrets. | FR-11 | Covered | JIT reference resolution is required. |
| SEC-11 | P | Provide a reliable open-source or free alternative to 1Password. | FR-11 lists SOPS+age/OpenBao/Infisical/keychain | Covered | Choice remains pluggable. |
| SEC-12 | P+R | Resolve secrets just in time for command execution or authentication. | FR-11 | Covered | Avoid persistent plaintext expansion. |
| SEC-13 | P+R | Secrets should be passed by reference and must not enter model context, logs, Ledger, or ordinary environment capture. | FR-11 says reference-not-value/log redaction | Partial | Model-context prohibition and safe process injection need explicit wording. |
| SEC-14 | P | Secrets management belongs in Phase 0/foundation. | §6.1 | Covered | Foundation may precede full provider breadth. |
| SEC-15 | P+R | Default-deny tools, egress, identities, connectors, and sensitive actions. | FR-8; FR-3 | Partial | Default-deny should be a cross-cutting invariant. |
| SEC-16 | P+R | Sensitive, privileged, externally impactful, or irreversible actions require approval, stronger authentication, or separation of duties according to risk. | FR-2 confirmation | Partial | Risk-tier policy is missing. |
| SEC-17 | P+R | Preserve source ACLs through ingestion, indexing, retrieval, reranking, and output; filter before and after retrieval. | None | Missing | Essential for enterprise RAG and silo isolation. |
| SEC-18 | P+R | Include observability for traces, metrics, logs, cost, model calls, tool authorization/execution, workflows, approvals, and evaluations. | FR-9 | Partial | OpenTelemetry is present, but agent-specific telemetry scope is not. |
| SEC-19 | P+R | Observability data itself requires redaction, encryption, retention, and access control. | Compliance; FR-11 | Partial | Telemetry cannot become an uncontrolled copy of prompts and secrets. |
| SEC-20 | P | Design for GDPR compatibility. | Compliance | Covered | Product controls facilitate compliance; they do not certify an organization. |
| SEC-21 | P | Design controls and evidence for ISO/IEC 27001. | Compliance | Covered | Avoid claims that software alone is ISO certified. |
| SEC-22 | P | Design controls and evidence for SOC 2. | Compliance | Covered | Avoid claiming an application itself has an organizational attestation. |
| SEC-23 | P | Design for EU AI Act obligations and evidence. | Compliance | Covered | Actual risk category depends on intended and deployed use. |
| SEC-24 | P | Design for NIS2-related security, incident, continuity, supply-chain, and governance support. | Compliance | Covered | Applicability depends on the deploying entity. |
| SEC-25 | P+R | Compliance support must produce evidence and controls, not marketing claims of automatic certification. | Compliance final sentence | Covered | Preserve precise language in all public materials. |
| SEC-26 | P+R | Include threat modeling, data classification, privacy impact analysis triggers, retention, erasure, incident handling, supplier/license risk, and secure SDLC evidence. | Compliance | Partial | Current compliance paragraph is too compressed for these durable needs. |

### 2.11 Development environment, quality, testing, CI/CD, and reproducibility

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| DEV-01 | P | Follow software-development best practices. | None | Missing | Must become concrete, testable engineering policy rather than a slogan. |
| DEV-02 | P | Follow CI/CD best practices. | None | Missing | CI/CD architecture and gates are outside the current PRD. |
| DEV-03 | P | Provide complete development-environment preparation documentation. | None | Missing | Must cover supported OSes, languages, tools, quality, tests, MCP, frameworks, and credentials setup. |
| DEV-04 | P | Provide from-scratch bootstrap scripts immediately after clone, ideally before the first product-code commit. | None | Missing | Explicit sequencing requirement. |
| DEV-05 | P | Bootstrap must install or verify languages, package managers, quality tools, test tools, MCP components/connections, BMAD, Spec-Kit, powerskills, and loop-engineering support. | None | Missing | Scripts should be idempotent and avoid embedding secrets. |
| DEV-06 | P | A contributor must be able to resume development from any supported machine after cloning the repository. | None | Missing | Includes deterministic versions, environment checks, and documented recovery. |
| DEV-07 | P | Bootstrap and development workflows must work across Windows, Linux, and macOS. | None | Missing | Platform-specific adapters/scripts may exist behind a common entry point. |
| DEV-08 | P+R | Establish quality gates, checklists, examples, snippets, clarifications, and acceptance criteria before implementation. | None | Missing | User explicitly rejected beginning implementation without them. |
| DEV-09 | P+R | Use automated unit, integration, contract, end-to-end, security, performance, accessibility, packaging, and clean-install tests proportionate to each slice. | None | Missing | Exact thresholds may be decided later, but test classes are durable. |
| DEV-10 | P+R | Include linting, formatting, static analysis, dependency/license checks, SAST/SCA, secret scanning, and generated evidence in CI. | None | Missing | Required by stated best-practice and compliance goals. |
| DEV-11 | P+R | Test installation and operation in a clean/virgin environment, offline where applicable. | None | Missing | A local-first product is not proven by a developer workstation build. |
| DEV-12 | P+R | Use a modular monolith first rather than premature distributed microservices, with extractable process boundaries. | §5; §8 worker strategy | Partial | This is primarily architectural input and should be reconciled there. |
| DEV-13 | P+R | Reassess the technical stack and integration strategy through deep comparative analysis and spikes before locking it. | §8 | Partial | Only worker/context questions are listed; broader stack validation remains implicit. |
| DEV-14 | P+R | Preferred provisional mix: Rust for trusted control/host functions, TypeScript for UI, Python for optional AI/media pipelines, subject to spikes. | None | Missing | Not a final lock; preserve as a hypothesis to test. |
| DEV-15 | P+R | Keep core domain contracts independent of external frameworks and services so implementations remain replaceable. | §5; FR-18 | Partial | Applies to models, workflow, tools, identity, storage, RAG, and policy—not only workers. |
| DEV-16 | P+R | Review third-party licenses, notices, trademarks, update risks, and supply-chain posture before embedding/forking components. | Compliance mentions MCP supply chain | Partial | Open-source status does not remove compatibility and trademark obligations. |

### 2.12 One-million-token feasibility and context engineering

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| CTX-01 | R | A million tokens is roughly a context-capacity unit, not a sensible repository-size ceiling. | FR-17 | Covered | Do not optimize product scope merely to fit the whole mature repository into one prompt. |
| CTX-02 | R | A serious local MVP may fit below roughly one million source tokens, while a mature enterprise platform with tests, docs, connectors, packaging, and compliance evidence will probably exceed it. | None | Missing | Feasibility planning insight, not an FR. |
| CTX-03 | P+R | Repository size and active model context are different engineering concerns. | FR-17 | Covered | This distinction must guide both Heddle and its own development process. |
| CTX-04 | P+R | Each model call should receive the smallest sufficient context rather than the entire repository. | FR-17 | Covered | **Qualitative intent at risk:** more context is not automatically better context. |
| CTX-05 | P+R | Build repo maps, symbol indexes, dependency graphs, lexical/semantic retrieval, lazy loading, and trajectory compression. | FR-17 | Covered | Selection and compression must remain traceable. |
| CTX-06 | P+R | Preserve room in the context window for instructions, task input, tool output, diffs, tests, errors, and final output. | FR-17 | Partial | Token allocation is recorded, but budget categories are not explicit. |
| CTX-07 | P+R | Treat million-token windows as overflow capacity and benchmark them against retrieval, including middle-position degradation. | FR-17; §8 question 2 | Covered | Must be validated empirically. |
| CTX-08 | P+R | Every call requires a reproducible manifest of selected sources, hashes, classifications, budget, and rationale. | FR-17 | Covered | Supports replay, audit, and debugging. |
| CTX-09 | R | The core harness can be relatively compact because model reasoning is external, but enterprise reliability complexity lies in governance, connectors, tests, compatibility, and operations. | None | Missing | Planning insight that counters misleading “clone built in days” comparisons. |
| CTX-10 | R | Rapid solo clones reproduce visible behavior using existing infrastructure and AI-generated code; they do not prove enterprise maturity, security, compatibility, or compliance. | None | Missing | Feasibility and expectation-management input. |
| CTX-11 | P+R | A highly autonomous build is feasible as a campaign of many bounded tasks, checkpoints, tests, reviews, and repairs—not as a single unreviewed pass. | FR-13; FR-16 | Partial | Applies directly to Heddle's own implementation authorization. |
| CTX-12 | R | Human judgment remains required at product, legal, security, subjective-quality, and irreversible decision boundaries. | FR-13 approvals; FR-16 escalation | Partial | Human accountability boundaries should be explicit. |

### 2.13 Explicit prohibitions and non-negotiable boundaries

| ID | Source | Durable user input | Current PRD mapping | Status | Preservation note |
|---|---|---|---|---|---|
| PRO-01 | P | Do not implement product code before conception and readiness gates are complete and the owner has been warned/asked. | None | Missing | Current binding project-sequencing prohibition. |
| PRO-02 | P | Do not claim BMAD, Spec-Kit, or their bridge is complete without the required artifacts and official workflow evidence. | None | Missing | Claims must match verifiable artifact state. |
| PRO-03 | P | Do not share information across mode silos. | FR-6 | Covered | Includes derived and cached data. |
| PRO-04 | P | In Remote mode, do not share outside the same team. | FR-6 | Covered | Deny by default. |
| PRO-05 | P+R | Do not expose the local backend to the network unless explicitly configured. | §5; FR-6 | Partial | Default bind/address and discovery behavior are not explicit. |
| PRO-06 | P+R | Do not send local/restricted data to cloud models or remote tools contrary to mode and policy. | FR-3; FR-8 | Covered | Enforcement must occur before context construction and tool execution. |
| PRO-07 | P+R | Do not place resolved secrets in model context, Ledger, logs, or durable config. | FR-11 | Partial | Model context and Ledger are not explicitly named in FR-11. |
| PRO-08 | P+R | Do not let an agent or model grant itself permission, waive policy, or decide alone that sensitive work is acceptable. | FR-8; FR-16 | Partial | Independent policy and acceptance authorities need explicit contracts. |
| PRO-09 | P+R | Do not treat untrusted retrieved/web/tool content as higher-priority instructions. | None | Missing | Core prompt-injection defense. |
| PRO-10 | P+R | Do not promise automatic GDPR/ISO 27001/SOC 2/AI Act/NIS2 compliance or certification. | Compliance | Covered | State “supports controls/evidence,” subject to deployment and organization. |
| PRO-11 | P+R | Do not rewrite commodity infrastructure without a demonstrated requirements gap. | §5 | Covered | Internal differentiation remains harness, workflow, governance, context, and evidence. |
| PRO-12 | P | Do not mention or associate SodiusWillert with Heddle. | None | Missing | Applies to repository and public project materials. |
| PRO-13 | P | Do not write persistent project code or documentation in French. | None | Missing | English-only repository policy. |
| PRO-14 | P+R | Do not use a single model, provider, agent worker, IdP, secret manager, task tracker, or workflow dependency as an irreplaceable control-plane authority. | §5; FR-18 | Partial | Current non-goal names provider/IdP/secrets but not every listed dependency class. |
| PRO-15 | P+R | Do not equate one package with one process or force all optional heavy inference/media dependencies onto a basic desktop install. | None | Missing | Packaging must preserve a simple baseline and optional capability packs. |

## 3. Qualitative Intent That Must Survive Normalization

These intents are cross-cutting and can be lost even when individual requirements appear covered:

1. **Simplicity:** Heddle should present one understandable product and one coherent interaction model. Users should not need to understand its internal sidecars, adapters, models, or workflow graph to obtain ordinary results.
2. **Adaptability:** Heddle should discover and adapt to hardware, operating system, connectivity, available local engines, enterprise identity, existing tools, and organizational policy. Adaptation is user-visible and user-controlled.
3. **Omni illusion:** the user experiences one capable assistant even when multiple specialist models and tools execute in parallel or sequence. Internal specialization must not fragment the interaction.
4. **Local independence:** a single user on an ordinary desktop can install, operate, develop with, and recover Heddle without a cloud account, enterprise service, or another Heddle server.
5. **Team collaboration:** the same product can expose its backend to a small trusted team, preserve team-only sharing, and later connect to enterprise identity and work systems without replacing the local-first core.
6. **Full transparency:** users can inspect raw model inputs and outputs, context selection, tool calls, policies, approvals, tests, and state—not merely summaries or final files.
7. **Governed autonomy:** broad autonomous execution is desirable only when bounded by explicit policies, budgets, ground-truth verification, checkpoints, evidence, independent review, and human authority at consequential boundaries.
8. **Harness consistency:** the primary product value is a stable harness and workflow contract across changing models, workers, connectors, modes, and deployment environments.

## 4. Reconciliation Summary

This extract intentionally preserves both product requirements and project-process constraints. Items marked Missing or Partial are not automatically candidates for direct insertion into the PRD: identity facts may belong in project governance, stack hypotheses in research/ADRs, process gates in the constitution or development policy, and detailed contracts in Spec-Kit artifacts. They nevertheless remain durable user input and require an explicit destination, disposition, or owner-approved rejection before implementation readiness can pass.
