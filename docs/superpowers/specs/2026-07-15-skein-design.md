# Heddle — Design Document (Spec)

- **Codename**: Heddle *("a heddle of geese" = a flight of geese, a nod to Goose; and a heddle of intertwined threads = the connectors/models woven together)*
- **Date**: 2026-07-15
- **Status**: Design validated — awaiting review before implementation plan
- **Author and project owner**: Cédric Thedrez (`K4M1coder` on GitHub, `cethgame` elsewhere)
- **Project status**: independent open-source project
- **Method**: written in the **Spec-Kit** style (Spec → Plan → Tasks → Implement); verifiable artifacts in the **BMAD** style.

> ⚠️ This document describes **what** to build and **why**. The **detailed how** (tasks, sequencing) will be the subject of a separate implementation plan, produced after review of this spec.

> 🔧 **Hardened by adversarial review — read `docs/superpowers/adr/0002-design-hardening.md` alongside this spec.** ADR 0002 is authoritative where it corrects sections below (loop ownership D1, event-schema D2, config resolution D3, egress D4, GDPR erasure D5, ledger redaction D6, ground-truth enforcement D7, loop budgets D8). Design completeness is governed by `docs/DESIGN-COMPLETENESS-POLICY.md` (not 100% up-front by design).

---

## 1. Vision & objectives

### 1.1 Problem
Individuals and teams currently use siloed AI tools (chat, code assistants, automations) without a unified harness, native integration with their working environment, or consistent control over model choice (cloud, local, or sovereign).

### 1.2 Vision
A **single agentic tool**, local-first, bringing together **chat**, **code** and **cowork** (PC control), equipped with a powerful harness (context management, tools, skills), natively integrating business connectors via **MCP**, able to plug into **all AI providers** (cloud and local) and embedding its own inference, with the **BMAD / Spec-Kit / powerskills** methods as first-class skills.

### 1.3 Objectives (v1 / MVP)
1. **Agentic code assistant** (read/edit/execute, TDD, subagents).
2. **Multi-provider + local inference** (cloud + Ollama/vLLM/llama.cpp/LM Studio).
3. **Atlassian + M365 connectors** via MCP, usable within workflows.
4. **BMAD / Spec-Kit / powerskills frameworks** integrated as invocable skills.

### 1.4 Out of scope for v1 (later versions)
v1 is **text**. Multimodal and collaborative evolution is planned across versions v2→v8 + a parallel enterprise track — see **§8 Evolution roadmap**. Logical progression: *perceive → act → generate → animate → unify → speak → translate*.
- **v2 — Perception** (inputs): typed content abstraction + documents/images (OCR+vision) + audio (STT) + visual grounding + web ingestion/memory.
- **v3 — Action** (cowork): PC control (local / Computer Use) + browser companion (Chrome/Edge) + real-time web navigation.
- **v4 — Generation** of media: image + audio/TTS + Office files.
- **v5 — Temporal**: animated images + video.
- **v6 — Omni**: **multi-model orchestration** (parallel/sequential in the background) giving the illusion of a single model; a true omni model is a special case plugged in via the Gateway.
- **v7 — Real-time voice**: low-latency duplex streaming audio.
- **v8 — Translation** real-time multilingual (Teams / team chat, native language per member).
- **Track ⟂**: team/enterprise hardening (external IdPs LDAP/OIDC/Entra/Google + advanced RBAC §7.9-7.10, advanced audit, advanced RAG, vLLM GPU, recipe catalog, certifications), paced by team adoption. *Local identity + basic RBAC + observability + compliance-by-design are present from v1.*

### 1.5 Non-objectives (YAGNI principles)
- No rewrite of an agentic harness from scratch (we adopt a neutral base).
- No separate server product (the "team backend" is an exposed instance of the app).
- No dependency on a single provider (multi-provider neutrality).

---

## 2. Founding decisions (and rationale)

| Decision | Chosen option | Rationale |
|---|---|---|
| **Strategy** | Build on a **neutral open-source base** | Avoids vendor lock-in + reuses a proven harness. |
| **Core foundation / language** | **Heddle-owned Rust control plane** + optional Python/TypeScript sidecars | Rust owns policy, workflow, Ledger, context and stable API contracts. Existing agents and language-specific engines remain replaceable workers/adapters (ADR 0003). |
| **Model gateway** | **LiteLLM** | OpenAI-compatible entry point to 100+ cloud **and** local providers; cost/quotas/guardrails. |
| **Frameworks** | BMAD / Spec-Kit / powerskills packaged as **recipes/skills** | Integration = packaging + orchestration, not rewrite. |
| **PC control** | **Abstract hybrid** `Controller` interface | Interchangeable back-ends (Computer Use API **or** local enigo/xcap). No lock-in. |
| **Deployment** | **Local-first**, team backend **enableable** | The desktop is autonomous; team mode is a layer on top. |
| **Platforms** | **First-class cross-platform: Windows + macOS + Linux** (on equal footing) | Rust/Tauri/Goose/LiteLLM/SQLite/keyring are all multi-OS; CI on all three; per-OS signing. |
| **Surfaces** | **Headless core → CLI (reference) → UI (layer)** | Automatable, testable; the UI adds no capability of its own. |
| **Agent-runtime strategy** | Native Heddle loop by default; Goose/OpenCode/Cline/Hermes/Claude Code and others are optional `WorkerAdapter`s or sources of compatible components | Guarantees turn-level governance and prevents an external agent from owning policy, workflow state or evidence (ADR 0003). |
| **Packaging** | One product/repository/version/installer/command; modular monolith with optional supervised sidecars/workers | Simple local use without forcing every inference, browser or ML engine into one process. |
| **Context strategy** | Smallest-sufficient, reproducible `ContextManifest`; 1M-token windows are overflow capacity, not default working memory | Long-context models still degrade with position and complexity; selection, provenance and compression are core capabilities (§4.15). |
| **Editable harness** | **Layered** config: **team** base (leads, lockable) + **local** overrides (user) — see §5.4 | Team governance + local freedom, without breaking silo isolation. |
| **Identity** | **Pluggable** provider: local store (default) / LDAP-AD / OIDC / Entra ID / Google Workspace — §7.9 | Local-first offline; enterprise IdP + groups in Server/Remote mode. |
| **Authorization** | **RBAC** roles+permissions at **3 scopes** (global / silos / intra-silo) — §7.10 | Fine-grained control of usage, silo access, and functions/settings. |
| **Observability** | **OpenTelemetry** + immutable audit, from v1 — §7.11 | Exportable standard; cross-cutting foundation of compliance. |
| **Compliance** | **Compliance-by-design**: GDPR / ISO 27001 / SOC 2 / EU AI Act / NIS2 — §7.12 | The software provides the controls; certification remains organizational. |
| **Traceability** | **Git-style event-sourced Ledger**: every step (model I/O, tools, state) immutable, inspectable, replayable, reversible — §4.11 | Full transparency (all model in/out, not just results); captured at the Gateway. |
| **Secrets** | **Pluggable `SecretProvider`, JIT resolution**: SOPS+age / 1Password / OpenBao / Infisical (+ OS keychain) — §7.13 | References never values; simple default; compliance recommendation; native (hot-path) + optional MCP. |
| **Workflow** | **Native engine** (inspired by Archon) **event-sourced on the Ledger**; optional durable back-end (Temporal/Windmill) — §4.12 | Native multi-agentic sequencing across the whole SDLC chain; durability/replay free via the Ledger. |
| **Task tracking** | **Pluggable `TaskTracker`**: local (silo) / **Vikunja** (embedded OSS) / **Jira** (MCP) — §4.13 | Build on Jira OR an embeddable OSS; bound by the config hierarchy. |
| **Hierarchy** | **Silo ▸ Team ▸ Project ▸ Conversation** (Local: without Team); config "highest locks lowest" — §5.5 | A single resolution/lock mechanism for harness, tracker, egress, providers. |
| **Loop engineering** | **Engine-enforced loop control** (§4.14): external termination/budgets + ground-truth-anchored reflect/retry; node types ReAct/Reflexion/Self-Refine/evaluator-optimizer | Verified patterns (`docs/research/loop-engineering.md`); never trust the model to stop or self-judge. |
| **Component enablement** | **Embedded but disabled by default** (connectors, IdPs, trackers, controllers); enabling = **scope-owner authorization** via the hierarchy; **default posture = full local** — §4.3 | One enablement mechanism for every pluggable component; shipped ≠ enabled. |
| **Computer access** | Scoped grants: **Project (default) ▸ Folder ▸ FullComputer** — §4.9 | Least privilege by default; widening is owner-granted, hierarchy-capped, audited. |

### 2.1 Sources (state verified as of 2026-07-15)
- Goose: https://github.com/aaif-goose/goose · https://block-goose.mintlify.app/
- LiteLLM: https://github.com/BerriAI/litellm · https://docs.litellm.ai/docs/providers
- Spec-Kit: https://github.com/github/spec-kit
- BMAD-METHOD: https://docs.bmad-method.org/

---

## 3. Overall architecture

Guiding principle: **headless core**, the CLI is the complete reference client, the UI is a layer. Every UI capability exists in the CLI; every CLI capability is exposed by the API.

```
        ┌────────────── HEDDLE HEADLESS CONTROL PLANE (Rust) ────────────┐
        │  Complete programmatic API (JSON-RPC / local HTTP)             │
        │  Agentic runtime · Context · Tool/skill/provider dispatch      │
        └───────────────────────────┬───────────────────────────────────┘
                                     │ (same contract for all)
        ┌──────────────┬─────────────┴─────────────┬────────────────────┐
        ▼              ▼                             ▼
   API (socket/    CLI (COMPLETE client,       Tauri UI (layer,
   HTTP)           test reference)              zero own capability)
                                     │
   ┌─────────────────────────────────┼────────────────────────────────┐
   ▼                 ▼               ▼                ▼                 ▼
 Python          LiteLLM         MCP connectors   Backend +          Controller
 sidecar         Gateway         (Atlassian,      Mode               cowork
 (embeddings/    (100+ prov.     M365, fs, git,   Supervisor         (v2)
 RAG)            cloud+local)    shell, …)        (silos)
                     │
              Inference: Ollama / vLLM / llama.cpp / LM Studio (+ cloud)
```

**Languages**: Rust (core + Tauri UI) · Python (AI/inference sidecar) · TypeScript (UI front-end). Polyglot by design, each language on its strong ground.

---

## 4. Components & interfaces

Each component: **one role**, an **explicit interface**, **testable in isolation**. Inverted coupling (the core discovers connectors/providers/control; it does not depend on them).

### 4.1 Access surfaces
- **Headless core**: exposes the `Agent` via a local API (JSON-RPC + optional HTTP). Single surface.
- **CLI** (`heddle …`): complete client, authoritative; 100% scriptable; basis of E2E tests.
- **API**: same surface, for automation/CI/third parties; subject to the mode's exposure/authz.
- **UI (Tauri)**: only emits CLI/API commands, displays events. No business logic.

### 4.2 Agentic core (`core/`, Rust — Heddle-owned loop)
```rust
enum Content { Text(..), Image(..), Audio(..), Doc(..), Video(..) }  // typed abstraction (from v2)
struct Message { role: Role, parts: Vec<Content> }

trait Agent {
  fn run(&self, session: SessionId, input: Message) -> EventStream;
  fn register_extension(&mut self, ext: McpServer);   // = un connecteur
  fn register_skill(&mut self, skill: Recipe);         // = BMAD/Spec-Kit/…
}
```
Depends on: `ModelGateway`, `Backend`, governed Tool/MCP Gateway, skill engine, `LoopController`, and optional `WorkerAdapter`s. A worker never owns canonical workflow state, policy decisions, evidence or completion.

**Typed content abstraction** (introduced in v2, cross-cutting): a `Message` carries `parts` of types `text | image | audio | doc | video`. This is the **only core addition** required by the entire multimodal roadmap; concrete modalities are then provider capabilities (Gateway) or specialized tools — never a rewrite of the agentic loop.

### 4.3 Connectors (`connectors/`, MCP servers)
Each connector = one MCP server (Jira/Bitbucket/Confluence, Outlook/SharePoint/Teams, `fs`, `git`, `shell`). Added by **config**, no core code. Protocol: MCP (`tools/list`, `tools/call`, `resources/*`).

**Embedded connectors + hierarchical enablement (default: full local).** Heddle **ships a curated set of MCP connectors embedded** (bundled binaries/configs from the trust registry §7.6) so no external install is needed. But **shipped ≠ enabled**:
- **Default posture = full local**: out of the box, only offline connectors (`fs`, `git`, `shell`) are active; every network connector is present but **disabled**.
- **Enablement is an authorization** made by the **owner of the scope**, resolved through the hierarchy (§5.5): silo owner ▸ project owner ▸ conversation owner — a connector enabled/locked at a higher level binds lower levels; security stays a **monotonic floor** (a lower level can disable, never enable beyond what's allowed above; ADR 0002 D3).
- Enabling a network connector is subject to the **egress boundary** (ADR 0002 D4: `requires_network()` on every connector, checked at enable-time, enforced by the network sandbox in Local mode).
- This same **enablement policy applies to every pluggable component**: identity providers (§7.9), secret backends (§7.13), task trackers (§4.13), model providers/routes (§4.5), and controllers (§4.9). One mechanism, all components.

### 4.4 Skills / recipes engine (`skills/`)
Loads BMAD, Spec-Kit, powerskills/superpowers and project-defined skills through Heddle's canonical skill/workflow contracts. Goose recipes and other external formats are import/export adapters, not the canonical representation.
```
Recipe = { name, description, instructions, required_extensions[], params[], prompt }
```

### 4.5 Model gateway (`gateway/`, LiteLLM)
OpenAI-compatible entry point (`POST /v1/chat/completions`) → cloud/local providers; routing, cost/quotas, load-balancing and guardrails. LiteLLM is the initial replaceable adapter; Heddle owns model capability descriptors and policy routing.
**Traceability chokepoint**: all model I/O passes through here → the Gateway **captures model inputs/outputs to the Ledger (§4.11)**, whatever the emitting runtime.

### 4.6 Inference layer (`inference/`)
Local models (Ollama / llama.cpp / vLLM / LM Studio) exposed as OpenAI-compat endpoints, registered in LiteLLM. "Embedded inference server" = Ollama/llama.cpp packaged (+ optional GPU vLLM).

### 4.7 Python sidecar (`sidecar/`)
Embeddings, RAG/indexing, advanced inference orchestration, eval. Separate process (gRPC/local HTTP): `embed()`, `index()`, `search()`. Index storage is routed by silo/mode.

### 4.8 Backend & silos (`backend/`) + Mode Supervisor
```rust
trait Backend { fn store(&self, mode: Mode, team: Option<TeamId>) -> Silo; }
impl EmbeddedBackend  // SQLite + fichiers locaux, par namespace
impl RemoteBackend    // leader client, team partition
struct ModeSupervisor { fn detect()->NetState; fn switch(Mode); fn heartbeat(); }
```

### 4.9 Cowork Controller (`controller/`, interface set in v1; capture v2, control v3)
A single abstraction for controlling an external surface (capture + actions), with **several interchangeable channels**:
```rust
trait Controller {
  fn screenshot(&self) -> Frame;
  fn click(&self, target: Point); fn type_text(&self, s: &str);
}
impl ComputerUseController   // API Anthropic (grounding fourni)
impl LocalController         // desktop : enigo (clavier/souris) + xcap (capture)
impl BrowserController       // compagnon navigateur (extension Chrome/Edge, type "Claude for Chrome")
```
All three uses — captures as input (grounding, v2), PC control, and browser companion (v3) — reuse **the same visual grounding building block**. These are three implementations of one trait, not three separate developments.
**Cross-platform**: `enigo`/`xcap` cover Windows/macOS/Linux. On **macOS**, the `LocalController` requires the system permissions **Accessibility** and **Screen Recording** (requested explicitly from the user, never bypassed); on Linux, handle X11 **and** Wayland (portals).

**Computer-access scopes (hierarchically granted, default = narrowest).** Access to the machine is itself a scoped, owner-granted authorization (same enablement policy as §4.3):
```rust
enum AccessScope {
  Project,            // default: the project's working directory only
  Folder(PathBuf),    // an explicitly chosen folder tree
  FullComputer,       // whole PC: filesystem + Controller (screen/keyboard/mouse)
}
```
- **Default = `Project`** (the conversation's project directory). Widening to `Folder` or `FullComputer` is granted by the scope owner, resolved through the hierarchy (§5.5) — a silo/project lock caps what any conversation may request; security remains a monotonic floor.
- `FullComputer` gates both broad filesystem access **and** the cowork `Controller`; it always keeps per-action confirmations for destructive/irreversible operations (§7.4) and is audited (§7.11).

### 4.10 Generative modalities, omni orchestration & real-time streams (v4+)
- **Generation** (v4/v5): specialized tools/connectors — image (model via Gateway), **TTS** (audio), **Office files** (docx/pptx/xlsx via mature libraries), **video** (v5). Each output is a typed `Content` produced by a tool; the core does not depend on it.
- **Omni orchestrator** (v6): a layer between `Agent` and `Gateway` that **decomposes** a multimodal request, routes each sub-task to the appropriate **specialized model** (vision, ASR, TTS, image, LLM) — **in parallel** when sub-tasks are independent, **sequentially** when they depend on one another — then **recomposes** a unified response. Gives the **illusion of a single omni model while remaining multi-provider**. A true omni model = one route among others.
  ```rust
  trait OmniOrchestrator {
    fn plan(&self, input: Message) -> Vec<SubTask>;         // decomposition
    fn dispatch(&self, tasks: Vec<SubTask>) -> Vec<Content>; // parallel/sequential via Gateway
    fn compose(&self, parts: Vec<Content>) -> Message;       // recomposition
  }
  ```
- **Duplex streaming channel** (v7, *new to the execution model*): real-time audio requires a **continuous bidirectional stream** (simultaneous in and out), distinct from the request→response loop. Introduces a `RealtimeSession` interface (WebRTC/streaming or omni-realtime API) — it is the **only milestone that modifies the core's execution model** (cf. risks §10).
- **Team translation** (v8): composition STT→translate→TTS (voice) + text translation, **per participant** according to a "native language" profile carried by the member in the **team partition** (§5), via the Teams/chat connector.

### 4.11 Execution Ledger (event-sourced, git-style) — cross-cutting, from v1
**Each step is an immutable revision.** Heddle records, in an **append-only, hash-addressed and chained (parent→child)** journal, *everything* that makes up an execution — not just the produced results:
- **Model inputs**: the **exact** context/prompt sent to each model.
- **Model outputs**: the **raw** response of each model (before post-processing).
- **Tool-calls**: the call (name + arguments) **and** the result.
- **State changes**: session/file mutations (with pre-mutation snapshot where reversible).

```rust
struct StepId(String);                 // hash de contenu (comme un SHA de commit)
struct Step {
  id: StepId, parent: Option<StepId>,  // chain/DAG
  ts: i64, principal: PrincipalId, silo: SiloRef,
  kind: StepKind,                      // LlmRequest | LlmResponse | ToolCall | ToolResult | StateChange
  payload: Content,                    // the full content (in or out)
}
trait Ledger {
  fn append(&self, step: Step) -> StepId;        // append-only
  fn history(&self, session: SessionId) -> Vec<Step>;   // "git log"
  fn show(&self, id: StepId) -> Step;            // inspecter in/out exacts
  fn replay(&self, from: StepId) -> EventStream; // rejouer depuis un point
  fn revert(&self, to: StepId) -> Result<()>;    // undo (reversible effects) + restore snapshot
  fn branch(&self, from: StepId) -> SessionId;   // explorer une alternative
}
```
- **Capture point**: model inputs/outputs are captured at the **Gateway (§4.5)** — a single chokepoint traversed by every runtime → no model I/O escapes the journal. ⚠️ **Corrected by ADR 0002 (D1/D2)**: valid only if **Heddle owns the loop** and Goose is a per-turn executor (goosed/embedded) with a propagated `trace_id`; a `goose run` *subprocess* hides the exact prompt and tool calls. Tool ground truth is captured via a **Heddle MCP proxy**. Step identity = surrogate id + content-hash integrity field (not hash-as-PK), with effect-class + idempotency key for safe resume/replay/branch, plus loop event kinds (Reflection/Evaluation/IterationBoundary/BudgetSpent/Exit/Approval).
- **Honest reversibility**: internal effects (files/session) **undoable** via snapshot; irreversible external effects (email sent, ticket created) **recorded and flagged** as non-undoable (compensating action proposed, never automatic).
- **Isolation & security**: the journal lives **within the silo** (§5.3); it contains potentially sensitive prompts → subject to the **keychain/redaction, egress and retention** (§7). It is also the centerpiece of GDPR/AI Act traceability (§7.11-7.12).
- **Surfaces**: `heddle ledger log|show|replay|revert|branch` (reference CLI); the UI is merely a view of this journal.

### 4.12 Workflow engine (native, inspired by Archon, Ledger-synced)
The harness can **natively sequence multi-agentic actions** across its connected tools. A **workflow** = a graph of **typed nodes** executed by the core and **event-sourced on the Ledger** (§4.11) → durability, replay and **crash recovery for free**.
```rust
enum Node {
  Agent(Prompt), Tool(McpCall), Subagent(WorkflowRef),
  Approval(Prompt),           // human-in-the-loop
  Cond(Expr), Parallel(Vec<Node>), Loop { until: Expr, body: Box<Node> },
}
struct Workflow { name: String, params: Vec<Param>, graph: Vec<Node> }
trait WorkflowEngine {
  fn run(&self, wf: &Workflow, ctx: RunCtx) -> EventStream; // each step → Step in the Ledger
  fn resume(&self, run: RunId) -> EventStream;              // resume from the last Step
}
```
- **Archon-inspired model** (deterministic harness + tasks + knowledge + MCP), running on the **local backend** by default.
- **Goose recipes + BMAD/Spec-Kit flows = workflows** (a recipe is a declarative `Workflow`).
- **SDLC chain**: workflows orchestrate the whole chain — design, dev, tests, packaging, deployment — via the corresponding MCP connectors (git, CI/CD, tests, registries, cloud), and can drive Jira/tracker (§4.13).
- **Optional durable back-end** (Temporal Rust-core / Windmill) behind `WorkflowEngine` for enterprise scale; default = native engine on the Ledger.

### 4.13 Task tracking (pluggable `TaskTracker`)
Workflows and the user create/track tasks via an interchangeable tracker:
```rust
trait TaskTracker { fn create(&self, t: Task)->TaskId; fn update(&self, id: TaskId, s: Status); fn list(&self, q: Query)->Vec<Task>; fn requires_network(&self)->bool; }
impl LocalTracker    // silo-backed, hors-ligne (toujours disponible)
impl Vikunja         // lightweight embeddable OSS (API-first), local or server
impl JiraTracker     // via the Jira MCP connector (cloud/enterprise)
// (Plane possible later for team scale)
```
The **active back-end is resolved by the config hierarchy (§5.5)**: choice between Vikunja (local/embedded) and Jira cloud, settable at the silo/project/conversation level. Workflow progress is reflected in the tracker.

### 4.14 Loop engineering (agent-loop control) — transversal
Loop control is a **first-class, engine-enforced layer** (not prompt text), sitting around both the core agent loop (§4.2) and each Workflow node (§4.12). Grounded in verified patterns — see `docs/research/loop-engineering.md`. Two load-bearing constraints drive the design:
1. **Externally-enforced termination** — the engine, not the model, decides when to stop.
2. **Ground-truth-anchored reflection** — reflect/retry is anchored to external feedback (MCP tool results, code execution, tests, linters, type-checkers), never model self-judgment (intrinsic self-correction is unreliable and can degrade output).

```rust
struct LoopBudget { max_iters: u32, max_tokens: u64, max_cost: Cents, wall_clock: Duration }
enum Exit { FinalOutput, MaxIters, NoProgress, Error, HumanReject, BudgetExceeded }
trait LoopController {                              // middleware/hooks around each step
  fn before_step(&self, s: &LoopState) -> Directive;   // proceed | inject | short-circuit
  fn observe(&self, out: &StepResult) -> GroundTruth;  // tool/test/compiler feedback
  fn evaluate(&self, gt: &GroundTruth) -> Verdict;     // pass | retry | reflect | escalate
  fn should_exit(&self, s: &LoopState) -> Option<Exit>;// budget/termination, engine-enforced
}
```
- **Loop node types** (Workflow §4.12): `ReAct` (plan-act-observe), `Reflexion` (act→evaluate→reflect→retry, reflections persisted in the **Ledger** as episodic memory), `SelfRefine` (single-model generate→critique→refine), `EvaluatorOptimizer` (separate evaluator LLM in the loop).
- **Three verification levels**: **action** (each step), **iteration** (each turn vs criteria), **terminal** (full acceptance before "done").
- **Loop budgets & no-progress detection** are engine primitives (`LoopBudget`, `Exit`), persisted to the Ledger → resumable/inspectable.
- **HITL escalation**: on budget/failure-threshold breach or `needsApproval` tools → pause as a durable Ledger event, await human approve/reject (reuses §7.4 confirmations).
- **Simplicity gate** (Anthropic): prefer the simplest solution; add loop autonomy only when it earns measurable value.

### 4.15 Context manager and million-token operating model

Heddle MUST NOT equate an advertised context window with reliable working memory. A one-million-token window is useful overflow capacity, but repository-wide injection is neither required nor desirable. The context manager builds a versioned `ContextManifest` for every model call, recording selected sources, hashes, classifications, token allocation and selection rationale.

The manager combines repository and symbol maps, lexical and semantic search, dependency graphs, artifact relations, ACL-aware retrieval, lazy loading, source-linked summaries and trajectory compression. Requirements, security policy and acceptance criteria may be pinned; incidental history may be compacted or omitted.

Normal operation targets the **smallest sufficient context**, commonly tens of thousands of tokens. Output/tool-result reserve and loop headroom are protected from input expansion. Full-context loading is an explicit diagnostic strategy and MUST be benchmarked against retrieval-based selection before becoming a workflow default.

Research and feasibility baseline: `docs/research/agent-platform-landscape.md` (including *Lost in the Middle*, RULER and LongCodeBench).

---

## 5. Connectivity, modes & isolation

### 5.1 Embedded backend, always functional
Every instance embeds an always-active backend. What is configurable = its **network exposure**. No separate server product.

### 5.2 Three modes (auto-detected, switch proposed)

| Mode | Local backend | Exposure | Role |
|---|---|---|---|
| **Local** | Active | OFF | Autonomous, off-network |
| **Online — Server** | Active + shared | **ON** | *Leader* (serves other instances) |
| **Online — Remote** | On **standby** | client | *Follower* (uses a remote leader) |

- **Mode supervisor**: detects connectivity + leader presence; proposes the switch (never forced); **automatic local fallback** if offline or leader lost.
- **Auto-elected leader/follower model**: at any moment, exactly **one** active backend per instance (no split-brain).

### 5.3 Data isolation
- **Inter-mode**: **watertight** silos (namespace per mode: DB/schema + directory + separate keychain). No session crosses the boundary. Changing mode = changing silo, never merging.
- **Intra-Remote**: **team-partitioned** sharing — a follower only accesses `team:<its own>` on the leader. Two teams on the same leader remain invisible to each other.
- **Tested invariants**: write to a silo → prove invisibility in the other silos and other teams.

### 5.4 Harness governance & configuration (editable local + team)
The harness is **configurable and versioned** (config-as-code: system instructions, enabled tools + permissions, skills/recipes, context parameters, model routing, security/egress policies, guardrails).

**Roles** (within the team partition, §5.3): `member`, `team lead`, `project lead`, `admin`. Only leads/admin edit the team layer.

**Two-layer configuration, merged at resolution:**

| Layer | Edited by | Storage (silo) | Effect |
|---|---|---|---|
| **Team** | team lead / project lead / admin | team partition (Remote mode) | common base; **lockable** settings |
| **Local** | the user | local silo | overrides/complements the base; only layer in pure Local mode |

**Resolution rules:**
- Precedence: **local overrides team**, *except* settings marked **locked** by a lead/admin (non-overridable — governance).
- Isolation respected: team config in the team partition, local config in the local silo. In **pure Local mode**, no team layer (consistent with §5.3).
- **Versioned**: historized, reviewed, reversible (edited like code).
- **Security (link §7)**: security settings (egress, guardrails, forbidden connectors) are **lockable** by lead/admin; a local user **cannot loosen** a constraint imposed by the team. Any change to security config is **audited**.

### 5.5 Organizational hierarchy & config resolution
Data and config are organized in a **hierarchy**, different depending on the mode:

```
  Server/Remote modes :   Silo ▸ Team ▸ Project ▸ Conversation
  Local mode          :   Silo(local) ▸ Project ▸ Conversation      (no teams)
```

**Config resolution (harness, TaskTracker, egress, providers, secrets…):**
- **Setting a value is not the same as locking it.** Without an explicit lock, the most specific configured value wins (Conversation > Project > Team > Silo).
- An explicit lock caps lower scopes; the highest explicit lock wins.
- Security policy is a **monotonic floor**: lower scopes may tighten restrictions but never weaken a higher-scope security constraint.
- This single resolver governs harness, TaskTracker, egress, providers and secrets.

**Example (TaskTracker)**: if the silo explicitly locks "Jira", all teams/projects/conversations below use Jira; if it only supplies Jira as an unlocked default, a project or conversation may choose local Vikunja. In **Local mode**, the same rule applies without the Team level.

- **Isolation preserved**: the hierarchy lives *within* a silo; it never crosses the silo boundary (§5.3). Team membership remains the authorization boundary (§7.10).

---

## 6. Data flow (nominal)

```
Input (UI/CLI/API)
 └─1─ Supervisor : resolve the silo (local / server / remote+team)
 └─2─ Context : load memory/history OF THE SILO
 └─3─ Agentic loop :
        a) LLM → LiteLLM Gateway → provider (cloud/local)
        b) tool request → MCP dispatch → connector → result
        c) evaluate, repeat
 └─4─ Side effects via connectors (subject to §7 rules)
 └─5─ Persistence IN THE CURRENT SILO only
 └──► Typed event stream (token / tool-call / diff / error) → identical CLI & UI
```

- Isolation is enforced at the **boundaries** (steps 1 and 5).
- **Default policy**: Local mode ⇒ **no network output** ⇒ **local providers only**.
- RAG sidecar: `embed(query)` (local model by default) → `search()` in the silo's index → passages re-injected at step 2.

---

## 7. Security, identity, observability & compliance

1. **AuthN (Remote/leader)**: identity verified at attach via a **pluggable identity provider** (§7.9). **Deny-by-default** access. **Mandatory TLS** as soon as exposed.
2. **Secrets**: managed via a **pluggable `SecretProvider` with just-in-time resolution** (§7.13) — config stores only **references**, never values. OS vault by default. **Never in plaintext**, **one keychain per silo**, **Ledger/audit redaction**. The agent never enters credentials into a form.
3. **Egress by mode**: Local = no output (local only); Server/Remote = output per **explicit policy** (endpoint allow-list).
4. **Execution guardrails**: destructive/irreversible actions → **confirmation** (or allow-list in CI); **sandbox** for code/shell; audit log of tool-calls (what/when/which silo).
5. **Prompt-injection defense**: any content reported by a tool (email, page, issue, capture) is **data, not instruction**. Instructions found in external content → flagged, never executed.
6. **Supply chain (MCP & recipes)**: trust registry, version pinning, review before activation. No silent loading.
7. **Cowork / browser / voice safety (v2+)**: confirmations before irreversible actions; no credential entry; screenshots, web content and audio confined to the silo + egress policy; the browser companion never acts on instructions found *within* a page (boundary §7.5).
8. **Harness governance (§5.4)**: security settings are **lockable** by team/project lead/admin and **non-overridable** locally; any config edit (especially security) is **audited and versioned**. Editing the harness is itself a governed action, not a bypass of the rules above.
9. **Execution Ledger (§4.11)**: contains the entirety of model prompts/responses → **sensitive data**. Confined to the **silo**, subject to **egress**, to the **keychain** (secret redaction before persistence), to a configurable **retention policy**, and to **RBAC access** (who can read the journal). The ledger is the source of truth for traceability (§7.11-7.12) — it cannot be bypassed.

### 7.13 Secret management (pluggable `SecretProvider`, just-in-time resolution)
Principle: **store *references*, never values**. The secret is resolved **JIT** at the precise moment a command is executed or an authentication occurs, injected into memory (subprocess env or auth header), **never persisted**, **redacted from the Ledger/audit/logs**, and wiped from memory after use (`zeroize`-on-drop type).

```rust
struct SecretRef(String);   // URI: keychain:// | sops:// | op:// | bao:// | infisical://
trait SecretProvider {
  fn resolve(&self, r: &SecretRef) -> Result<SecretValue>; // JIT, in memory
  fn requires_network(&self) -> bool;                       // pour la politique egress
}
impl OsKeychain   // zero-config default, offline (Windows Credential Manager/DPAPI, Keychain, secret-service)
impl SopsAge      // versionable encrypted files, offline (simple portable default)
impl OnePassword  // op CLI / SDK / MCP — op:// references
impl OpenBao      // serveur Vault-compatible (OSS, Linux Foundation)
impl Infisical    // CLI `infisical run` / MCP
```

- **User choice** among: **SOPS+age**, **1Password**, **OpenBao**, **Infisical** (+ OS keychain). **Default**: OS keychain (zero-config); **simple portable default**: SOPS+age.
- **Integration**: **native adapter by default** for the JIT hot-path (faster, smaller attack surface — a secret does not pass through a tool-call); **optional MCP** (1Password, Infisical).
- **Egress consistency (§7.3)**: `requires_network()` governs availability. In **Local mode** (egress OFF), only **offline** back-ends (OS keychain, SOPS+age) work; online back-ends (1Password cloud, OpenBao server, Infisical) require Server/Remote + explicit egress.
- **Compliance recommendation**:

| Need | Recommendation |
|---|---|
| Local / individual, GDPR minimization, offline | **SOPS+age** or OS keychain |
| Team/enterprise with **centralized rotation, revocation, audit** (ISO 27001 / SOC 2 / NIS2) | **OpenBao** (self-hosted, full control) or **1Password Business** |
| Dev-friendly self-hostable compromise | **Infisical** |

### 7.9 Identity (pluggable providers)
A single abstraction, several interchangeable back-ends — same inverted-coupling pattern as the rest:
```rust
trait IdentityProvider {
  fn authenticate(&self, cred: Credential) -> Principal;      // qui es-tu
  fn groups(&self, p: &Principal) -> Vec<Group>;              // tes groupes
}
impl LocalUserStore     // local user store (default, offline)
impl LdapProvider       // annuaire LDAP/AD
impl OidcProvider       // generic OIDC
impl EntraIdProvider    // Microsoft Entra ID (+ groupes)
impl GoogleWorkspace    // Google Workspace (+ groupes)
```
- **Group → role mapping**: Entra/Google/LDAP/OIDC groups are mapped to RBAC roles (§7.10) via a correspondence table managed by an admin.
- **Local-first default**: `LocalUserStore` works without network; external IdPs are enabled in Server/Remote mode (enterprise track §8).

### 7.10 RBAC (roles + permissions, at three scopes)
**Deny-by-default** authorization, evaluated at three nested levels:

| Scope | Control | Permission examples |
|---|---|---|
| **Global (tool)** | general tool usage | connect, create a session, use cowork, expose a backend |
| **Silo access** | which silos a principal can see/use | read/write `team:alpha`, deny `team:beta` |
| **Intra-silo** | functions & settings within a silo | enable a given connector, edit the harness, change egress, invoke a given skill, use a given provider |

- **Roles** (composable): `member`, `team lead`, `project lead`, `admin` (+ custom roles); each role = a set of permissions.
- **Consistency with §5.4**: the harness "lockable settings" are expressed as intra-silo permissions (e.g. `harness.egress.edit` reserved for leads/admin).
- **Delivered early** in a local version (local store + basic roles); advanced RBAC + external IdPs on the **enterprise track**.

### 7.11 Observability
- **Traces / metrics / logs** via **OpenTelemetry** (standard, exportable to enterprise tooling). Integrated **from v1** (cheap early, painful to retrofit).
- **Immutable audit log**: authN/authZ, tool-calls, harness/security edits, silo access — timestamped, attributed to the principal, bounded to the silo. Complementary to the **execution Ledger (§4.11)** which captures the *content* (model I/O): the audit says "who did what", the ledger says "exactly what was sent/received".
- **Product metrics**: cost/tokens per provider (via LiteLLM), latency, tool failure rate; **respect the egress policy** (no export outside policy).

### 7.12 Compliance (compliance-by-design)
> The software **provides the controls** that *enable* compliance; **certification** (ISO 27001, SOC 2) remains an **organizational** process. Heddle is designed not to be the blocking link.

| Framework | What Heddle provides |
|---|---|
| **GDPR** | Minimization (Local mode without egress), **right to erasure** & export/portability by admin, data residency (local/on-prem), legal basis/consent, processing register, encryption at rest & in transit. |
| **ISO 27001** | Access control (RBAC §7.10), secret management (§7.2), audit (§7.11), change management (versioned config-as-code §5.4), supply chain (§7.6). |
| **SOC 2** | *Security/Confidentiality/Availability* criteria: RBAC, immutable audit, encryption, silo isolation, local fallback (§5.2). |
| **EU AI Act** | Transparency ("AI-generated content" disclosure), **human oversight** (confirmations §7.4), traceability of AI decisions (audit §7.11), model/routing documentation (via Gateway), risk classification of uses. |
| **NIS2** | Technical measures (encryption, MFA via IdP, hardening), **logging & incident reporting**, supply-chain security (§7.6), governance (roles/responsibilities). |

- **Retention & residency**: per-silo retention policies; data stays where the mode requires (Local = never any output).
- **Traceability**: the audit (§7.11) is the cross-cutting piece that serves GDPR, ISO, SOC 2, AI Act and NIS2 all at once.

---

## 8. Phasing & milestones (verifiable exit criteria)

### Phase 0 — A skeleton that works (vertical slice)
Headless core + frozen API/event contract; minimal CLI; 1 provider via LiteLLM; `filesystem` connector; Local silo persistence; **Ledger** (step-level capture); **`SecretProvider` foundation** (OS keychain + JIT resolution of the Gateway key + redaction) — the other secret back-ends arrive with the cloud providers/connectors.
**Exit**: at the terminal, a conversation that reads/writes a file, persisted & reloaded; Gateway key resolved from the keychain (never in plaintext); journal inspectable via `heddle ledger`.

### Phase 1 — MVP (4 axes)
- **1a** Agentic code (`fs`/`git`/`shell` sandbox, edit+diff, TDD, subagents).
- **1b** Multi-provider + local inference (LiteLLM + embedded Ollama/llama.cpp, switching, Local egress).
- **1c** Atlassian + M365 connectors (MCP).
- **1d** BMAD + Spec-Kit + powerskills frameworks (recipes/skills).
- **1e** **Native workflow engine** (§4.12) event-sourced on the Ledger + **TaskTracker** (§4.13: local/Vikunja/Jira) + **loop-engineering controls** (§4.14: `LoopController`, budgets, ground-truth reflect/retry, 3 verification levels) — multi-agentic sequencing across the SDLC chain.
- **Cross-cutting** Modes & silos + **Silo▸Team▸Project▸Conversation hierarchy** & config resolution (§5.5) (supervisor, strict isolation, complete Local; basic Server/Remote + team authz).
- **UI** Tauri (Chat + Code).
**Exit**: from UI *and* CLI *and* API, a real scenario — "read Confluence spec → Spec-Kit plan → TDD code → Bitbucket PR → Jira ticket", switching cloud↔local, silo isolation verified by test.

> From here on, the **multimodal & collaborative evolution roadmap**. Logical progression: *perceive → act → generate → animate → unify → speak → translate*. Each version remains local-first and respects silos/egress/authz, and builds on the previous one. Technical pivot: *visual grounding* (v2), reused by captures/cowork/browser.

### v2 — Perception (multimodal inputs)
First the **typed content abstraction** (`Content = text|image|audio|doc|video`, §4.2) — the only core addition of the whole roadmap — then:
- **Documents + images**: parsing/OCR + vision.
- **Audio input**: speech-to-text.
- **Visual grounding** (anchoring on a capture) — *pivot building block* reused in v3.
- **Web**: ingestion/memorization of web content into the silo's RAG.
**Exit**: summarize, within a single request, a PDF + an image + an audio excerpt + a web page, persisted without type loss.

### v3 — Action (cowork & control)
Reuses v2 grounding to **act** on external surfaces:
- **PC control**: `LocalController` (enigo/xcap) + `ComputerUseController` (API).
- **Browser companion**: `BrowserController` (Chrome/Edge extension) + **real-time web navigation**.
**Exit**: drive a third-party app **and** a web page on a scripted task, with confirmations on irreversible actions.

### v4 — Media generation (outputs)
Image (via Gateway), audio/**TTS**, **Office files** (docx/pptx/xlsx). Independent of v3 — may overlap.
**Exit**: produce a .docx + an image + an audio clip from a prompt, artifacts persisted to the silo.

### v5 — Temporal (animation & video)
Animated images + **video** (depends on v4 image generation).
**Exit**: generate a short video clip from an instruction + assets.

### v6 — Omni (illusion of a single model)
**Omni orchestrator** (§4.10): decomposes a multimodal request, routes to specialized models (parallel/sequential in the background), recomposes. Illusion of a single model **without dependence on a proprietary omni model**; a true omni is one route among others.
**Exit**: a single conversation mixing text/image/audio input and output, served by several transparently orchestrated models.

### v7 — Real-time voice (duplex streaming audio)
`RealtimeSession` interface (continuous bidirectional channel) — **modifies the core execution model** (cf. §10). Builds on v6 orchestration.
**Exit**: a live voice conversation, low-latency, interruptible.

### v8 — Real-time multilingual translation
Everyone **writes/reads/speaks/hears in their native language** in Teams / a team chat. Composition STT→translation→TTS + text translation, per participant (language profile carried by the team partition §5).
**Exit**: two members of different languages converse (text + voice), each in their own language, via the Teams/chat connector.

### Track ⟂ — Team/enterprise hardening (parallel)
External IdPs (LDAP/OIDC/Entra/Google) + **advanced RBAC** (§7.9-7.10), advanced audit, advanced RAG, vLLM GPU, additional connectors, recipe catalog, preparation for **certifications** (ISO 27001 / SOC 2). **Paced by team adoption**, not by modalities. *NB: local identity + basic RBAC + observability (OpenTelemetry) + compliance-by-design are integrated from v1.*
**Exit**: multi-workstation deployment (1 leader, N followers, 2 isolated teams) with enterprise IdP + validated 3-scope RBAC.

---

## 9. CI/CD & best practices

- **Polyglot monorepo**: `core/` (Cargo), `sidecar/` (uv), `ui/` (pnpm), `connectors/`, `skills/`, `docs/`. Trunk-based, Conventional Commits, PR + review.
- **Tests**: pyramid (unit per boundary + fake MCP integration + **E2E via CLI** golden path) + dedicated **isolation tests**; TDD on the core.
- **Per-language quality**: Rust (`fmt`/`clippy -D warnings`/`cargo audit`/`cargo deny`) · Python (`ruff`/`mypy`/`pytest`/`pip-audit`) · TS (`eslint`/`prettier`/`vitest`/`playwright`/`tsc`). Unified pre-commit.
- **Pipeline**: lint → build 3 languages → unit → integration → E2E CLI → **security scans (SAST, deps, secrets, SBOM)** → artifacts. **First-class cross-platform**: CI matrix **Windows + macOS + Linux** (all three treated equally; green tests required on all three before merge).
- **Release**: SemVer, auto changelog, **per-OS code signing** — Authenticode (Windows) **and** Developer ID + notarization (macOS) — essential for an agent that controls the PC; per-OS artifacts (Tauri); nightly/stable channels.
- **Method**: dogfooding via the **BMAD × Spec-Kit bridge** (BMAD plans → Spec-Kit executes), both **actually installed** (`.specify/`, `_bmad/`, `_bmad-output/`). Conformant artifacts: PRD/architecture/epics/sprint-status (BMAD) + constitution + `specs/001-*/spec|plan|tasks` (Spec-Kit). See `docs/METHODOLOGY.md`. This design remains the exhaustive reference; ADRs for architecture decisions (`docs/superpowers/adr/`).

---

## 10. Risks & open questions

| Risk / question | Impact | Approach |
|---|---|---|
| Agent-runtime composition | **High** | ADR 0003: Heddle owns the loop; bounded spikes compare native Rust, Goose, OpenCode and Cline integration surfaces before accepting a worker path. |
| Effective use of very long contexts | **High** | `ContextManifest` + repo/symbol maps + hybrid retrieval + position-sensitive benchmark; 1M context is overflow, not a substitute for context engineering. |
| Reliability of cowork *grounding* (click anchoring) | Medium | Computer Use hybrid first, local afterward. |
| Multi-OS local inference packaging (llama.cpp/vLLM; vLLM poorly suited to Windows/macOS) | Medium | Ollama as robust and cross-platform default; optional GPU vLLM (especially Linux). |
| Per-OS signing/notarization (Authenticode + macOS Developer ID) | Medium | Integrate signing into the release CI from the start; Apple/Windows developer accounts required. |
| macOS cowork permissions (Accessibility + Screen Recording) | Medium | Detect missing permission and guide the user; never bypass. |
| License compatibility (Apache/MIT/…) for commercial distribution | Medium | License audit in CI (`cargo deny`, equivalents). |
| Broad MVP scope (4 axes) | Medium | Phase 0 de-risks the architecture; the exit scenario forces integration. |
| Complexity of 3-scope RBAC × multiple IdPs | **High** | Single, tested permission model; deny-by-default; local store first, external IdPs afterward; dedicated authorization test suite. |
| GDPR erasure vs blocking destructive deletions (§7.4) | Medium | The **agent** does not hard-delete; **GDPR erasure** is a governed + audited *admin* function — not a contradiction, two distinct paths. |
| EU AI Act classification depending on use (may become "high risk") | Medium | Transparency + human oversight + audit by default; document the uses; let the org classify its deployment. |
| **Duplex streaming channel (v7)** modifies the core execution model | **High** | Isolate in `RealtimeSession`; rely first on an omni-realtime API before a home-grown WebRTC stack. |
| Complexity of the omni orchestrator (v6): composition latency, coherence | Medium | Simple router first (rules per `Content` type); parallelize the independent; measure recomposition latency. |
| Cost/latency of video generation (v5) | Medium | Cloud providers first; optional local GPU; asynchronous jobs. |
| Browser extension publication & security (v3) | Medium | Minimal permission scope; Chrome/Edge store review; strict anti-injection boundary. |
| Real-time translation quality/latency (v8) | Medium | Dedicated models + text fallback; explicit language profile per member. |
| Ledger growth (§4.11): volume of stored prompts/responses | Medium | Configurable retention, compaction/archival, hash addressing (dedup), large-blob storage outside the DB. |
| Secrets present in prompts captured by the Ledger | **High** | Redaction/masking before persistence; encryption at rest; RBAC access; never any output outside egress. |
| Event sourcing = founding architecture decision (costly to retrofit) | Medium | Capture from v1 (Phase 0) even if advanced revert/branch come later — see Phase 0 plan. |

---

## 11. Glossary
- **Silo**: watertight data partition associated with a mode (and a team in Remote).
- **Leader / Follower**: instance exposing its backend / instance attached to a leader.
- **Recipe**: an external declarative task/skill bundle (for example Goose YAML) imported into Heddle's canonical skill/workflow representation.
- **Controller**: abstraction of PC control (capture + keyboard/mouse).
- **Sidecar**: auxiliary Python process (embeddings/RAG/eval).
- **Principal**: authenticated entity (user/service) carrying an identity and groups.
- **IdP**: pluggable identity provider (local, LDAP/AD, OIDC, Entra ID, Google Workspace).
- **RBAC**: role+permission access control, at 3 scopes (global / silos / intra-silo).
- **Omni orchestrator**: layer that composes several specialized models to simulate a single model.
- **Harness**: configuration of the agent's behavior (instructions, tools, skills, context, policies) — editable in team/local layers.
- **Execution Ledger**: append-only, hash-addressed and chained journal (git-commit style), capturing each step (model I/O, tool-calls, state changes); inspectable, replayable, reversible, revisable.
- **Event sourcing**: state is derived from a journal of immutable events rather than mutated in place — enables history, replay and time-travel.
- **SecretProvider**: pluggable secret back-end (SOPS+age, 1Password, OpenBao, Infisical, OS keychain) resolving *references* into values **just-in-time**, without ever persisting the value.
- **JIT resolution (secrets)**: the secret is decrypted/retrieved only at the moment of its use (execution/auth), kept in ephemeral memory, redacted from logs.
- **Workflow**: graph of typed nodes (agent/tool/subagent/approval/condition/parallel/loop) sequencing multi-agentic actions, event-sourced on the Ledger (durable, replayable, resumable).
- **Organizational hierarchy**: Silo ▸ Team ▸ Project ▸ Conversation (Local mode: without Team); unit of config resolution/lock, "highest wins".
- **TaskTracker**: pluggable task-tracking back-end (local silo / embedded Vikunja OSS / Jira via MCP), bound by the hierarchy.
- **Loop engineering**: deliberate design/control/instrumentation of the agent's reason→act→observe→reflect→retry loop; engine-enforced termination + ground-truth-anchored reflection (see §4.14).
- **Ground truth (loop)**: external feedback (tool result, code execution, tests, linters, type-checkers) the loop evaluates against — the antidote to unreliable intrinsic self-correction.
- **Loop budget**: engine-enforced limits (max iterations, tokens, cost, wall-clock) plus no-progress detection; the model never decides when to stop.
