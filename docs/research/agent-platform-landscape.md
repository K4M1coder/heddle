# Agent Platform Landscape and Reuse Strategy

**Status:** Research baseline  
**Date:** 2026-07-15  
**Owner:** Cédric Thedrez (`kamicoder` on GitHub, `cethgame` elsewhere)  
**Project:** Heddle — independent open-source project

## Purpose

This report evaluates existing coding agents, personal assistants, workflow engines, chat platforms, and long-context research to decide what Heddle should adopt, adapt, learn from, or implement internally.

The target is a single installable, local-first product that works for one person or a small team, can connect to existing enterprise systems, and guarantees its own harness, policy, workflow, evidence, and context semantics.

## Decision rubric

Each candidate is classified as:

- **Adopt** — use as a replaceable component behind a Heddle-owned contract.
- **Adapt** — reuse compatible code or packages with attribution and a narrow adapter.
- **Inspire** — copy proven behavior and architecture, not code or runtime ownership.
- **Worker** — invoke as an optional external agent through a stable protocol.
- **Reject for embedding** — unsuitable because of license, coupling, security model, or deployment weight.

No external product may own Heddle's canonical artifacts, authorization decisions, workflow state, Ledger, or completion verdict.

## Capability matrix

| Product | Strongest reusable ideas | Strategy for Heddle | Main reason |
|---|---|---|---|
| Claude Code / Agent SDK | Lifecycle hooks, permission grammar, subagents, skills, plugins, MCP, worktrees, checkpoints, stream-JSON SDK surface | **Inspire + optional Worker** | Excellent harness UX; core runtime is proprietary and model-coupled |
| GitHub Copilot | IDE-native agent mode, async issue-to-PR workers, review, enterprise MCP controls, mission-control UX | **Inspire + enterprise connector** | Important user journeys; not an embeddable neutral runtime |
| OpenCode | Open server/CLI/TUI architecture, multi-provider agent, custom agents, SDK-like surfaces | **Adapt selectively + optional Worker** | MIT and architecturally useful; TypeScript/Bun runtime must not own Heddle state |
| Cline | Plan/Act separation, approvals, checkpoints, browser, headless CLI, worktree Kanban, SDK | **Adapt selectively + optional Worker** | Apache-2.0 and broad surfaces; IDE-centric implementation |
| Goose | Rust provider/extension traits, MCP-native tools, recipes, desktop/server components, custom distributions | **Adapt crates if suitable + optional Worker** | Apache-2.0 and Rust fit; only usable as core executor if turn-level control is exposed |
| Hermes Agent | Procedural memory, self-created skills, messaging gateway, terminal backends, trajectory compression, local browser | **Inspire + optional Worker** | Rich assistant behavior; Python runtime and its learning loop are not Heddle's policy boundary |
| Archon | Deterministic YAML workflows, AI/deterministic nodes, worktrees, validation loops, approvals, multi-surface execution | **Adapt workflow concepts/code after license review** | Closest workflow reference; current MIT TypeScript/Bun engine is highly relevant |
| Aider | Tree-sitter repository map, architect/editor split, lint/test repair loop, compact diff editing | **Adapt concepts/libraries** | Mature context-selection and edit-loop patterns |
| OpenClaw | Always-on local gateway, channels, routing, cron, device nodes, pairing | **Inspire only** | Valuable assistant UX, but its single-trusted-operator model and security history do not satisfy Heddle team/RBAC requirements |
| LibreChat | Multi-provider chat, MCP UX, per-user credentials, LDAP/OIDC, artifacts, RAG and multi-user patterns | **Inspire; optionally reuse isolated UI packages after review** | MIT and feature-rich, but its Mongo/Node/Python deployment is too heavy as Heddle's core |
| Open WebUI | Offline model/RAG UX, knowledge spaces, admin and model permissions | **Inspire; reject embedding** | Current branding-restricted license is unsuitable for an independent distribution |
| LangGraph | Durable state graphs, checkpoints, HITL, inspection | **Reference or optional backend** | Strong semantics, but Python coupling conflicts with the minimal local Rust core |
| Temporal | Durable long-running execution and replay | **Optional enterprise backend later** | Excellent durability; too operationally heavy for the default desktop |
| LiteLLM | Provider normalization, routing, budgets, fallback, OpenAI-compatible gateway | **Adopt as replaceable adapter initially** | Avoids reimplementing provider breadth; Heddle retains its own capability model |

## Architecture conclusion

Heddle should be a **modular monolith with optional sidecars and workers**:

1. The local desktop starts one embedded backend and works fully offline.
2. Modules run in-process by default when safe and practical.
3. Heavy or language-specific components may run as supervised sidecars.
4. Existing agents may run as optional workers through a `WorkerAdapter` contract.
5. Team mode exposes the same backend over an authenticated network boundary; it does not introduce a different product architecture.

The proprietary or third-party worker is never the control plane. Heddle owns:

- canonical artifacts and requirement traceability;
- workflow and loop state;
- policy decisions and approvals;
- MCP/tool mediation;
- context selection and data classification;
- evidence and the event-sourced Ledger;
- completion criteria.

## Canonical contracts Heddle must own

```text
ArtifactModel       BMAD, Spec-Kit, Jira and Markdown are projections
WorkflowDefinition  deterministic graph, loops, gates and approvals
WorkerAdapter       one-turn or bounded-task execution with typed events
CapabilityRegistry  models, tools and workers described by capabilities
PolicyDecision      deny / allow / require approval / restrict scope
EvidenceBundle      requirement + change + test + policy + provenance
ContextManifest     selected sources, token budget, hashes and rationale
```

## Single package does not mean single process

The distribution goal is:

- one repository;
- one version;
- one installer;
- one application and CLI;
- one default local data directory;
- one command to bootstrap development.

It does **not** require every inference server, browser, office suite, connector, or Python ML library to be linked into one binary. Optional components are downloaded or enabled explicitly and supervised by the embedded backend.

## Feasibility and the 1-million-token constraint

One million tokens is roughly 3–5 MB of plain source text and commonly represents approximately 50,000–100,000 lines of mixed code. This is an estimate, not a project-size guarantee: Rust and generated schemas tend to use more tokens per line, while configuration may use fewer.

A serious local-first core can plausibly remain below one million source tokens if connectors, UIs, generated files, vendored dependencies, workflows, and test assets are modular. The complete mature platform, including tri-OS desktop code, enterprise connectors, multimodal pipelines, security tests, packaging, and documentation, will probably exceed it.

More importantly, **fitting a repository into a nominal context window is not the design objective**. Long-context research shows that usable context is smaller than advertised context and that retrieval and reasoning quality can degrade with position and task complexity:

- *Lost in the Middle* reports lower performance when relevant information appears in the middle of long inputs.
- RULER distinguishes advertised context length from effective context length.
- LongCodeBench evaluates realistic code understanding and repair at up to million-token scales, confirming that long-context coding needs dedicated evaluation.

Heddle therefore treats 1M context as overflow capacity, not working memory.

### Context operating model

For each model call, Heddle builds a reproducible `ContextManifest`:

```text
immutable instructions and policy       5–10%
task specification and acceptance       5–15%
selected code and dependency slices    30–50%
retrieved docs and prior evidence       5–15%
tool results, diffs and errors          10–20%
reserved output and loop headroom       20–30%
```

The percentages are policy defaults, not hard constants. The context manager uses:

- repository maps and symbol indexes;
- lexical search (`ripgrep`/BM25) and semantic retrieval;
- dependency and call graphs;
- artifact relationships and provenance;
- per-module summaries with source hashes;
- lazy loading and progressive disclosure;
- trajectory compression that preserves evidence links;
- context pinning for requirements, security rules and acceptance criteria;
- a recorded explanation of why each source entered the context.

The target is normally the **smallest sufficient context**, often tens of thousands of tokens, even when a 1M-token model is available.

## Implementation economics

Rapid solo clones are credible because a minimal agent loop is small and mature libraries already provide model APIs, MCP, terminal execution, Git, diffs, parsing, TUI frameworks, databases, and browsers. Those clones reproduce visible behavior quickly; they generally do not reproduce years of security hardening, compatibility, evaluation, governance, and enterprise operations.

Heddle should expect to implement roughly the differentiating control plane while reusing commodity infrastructure:

### Implement internally

- canonical artifact graph and BMAD–Spec-Kit bridge;
- governed loop/workflow engine;
- policy and scope resolution;
- Tool/MCP Gateway;
- Ledger and evidence model;
- context manager and ACL-aware retrieval semantics;
- capability registry and routing policy;
- unified API/CLI/UI event contracts.

### Reuse behind adapters

- official MCP SDK and protocol transports;
- LiteLLM or direct provider SDKs;
- SQLite/PostgreSQL;
- Playwright and OS automation libraries;
- OpenTelemetry;
- local inference servers such as llama.cpp/Ollama/vLLM;
- media engines such as ComfyUI, FFmpeg, Whisper-family STT, Piper-family TTS and Blender;
- identity and policy systems where deployment warrants them.

## Required validation spikes

Before production implementation, run bounded spikes with objective exit criteria:

1. **Agent runtime ownership:** compare native Rust loop, embedded Goose crates, goosed, OpenCode SDK/server and Cline SDK. Reject any path that hides turn-level model/tool events.
2. **Workflow reuse:** prototype one Archon-compatible YAML workflow and map it losslessly to Heddle's canonical workflow graph.
3. **Context quality:** benchmark repo-map + hybrid retrieval against full-context loading on representative repositories, including middle-position tests.
4. **Tool governance:** proxy one local MCP server and one remote OAuth MCP server through policy, approval, redaction and Ledger capture.
5. **Single-package UX:** prove fresh install and offline first run on Windows, macOS and Linux.

## Rust core building blocks and protocol convergence (runtime-spike inputs)

An independent parallel research pass (four segment reviewers: CLI coding agents, IDE/Copilot, assistant harnesses, chat UIs) **cross-confirmed the capability matrix above** (same verdicts: OpenCode/Cline reuse under permissive licenses, Aider Apache-2.0 repo-map, Goose Apache-2.0 crates, Open WebUI license-blocked, LibreChat MIT-inspire, Archon as workflow reference). It also surfaced one decision-critical fact missing above:

**Protocol convergence — the field is standardizing the client↔agent boundary.** Claude Code exposes a newline-delimited **stream-JSON over stdio** SDK surface; OpenCode exposes an **OpenAPI 3.1 + SSE** server; **Goose is pivoting to ACP (Agent Client Protocol) over HTTP/WebSocket** (`goose serve`). All three are the same shape: *headless core + protocol boundary + thin clients* — exactly Heddle's AD-1.

**Implication for Spike 1 (runtime ownership).** The three concrete options are now precise:

| Option | Rust building blocks | Trade-off |
|---|---|---|
| **A. Native Heddle loop** | `rmcp` (official MCP Rust SDK, `docs.rs/rmcp`) for tools + `async-openai`/`reqwest` for the OpenAI-compatible Gateway + `ratatui` for the TUI | Full turn-level ownership (satisfies Ledger/LoopController); most code, but bounded (LiteLLM + rmcp carry the heavy lifting) |
| **B. Embed Goose crates** | `goose-sdk` / `goose-sdk-types` / `goose-providers` (Apache-2.0) as libraries | Reuse provider breadth; must verify the SDK exposes per-turn events, not just batch runs |
| **C. ACP worker** | `agent-client-protocol` Rust SDK (+ its `rmcp` bridge) to drive goosed / OpenCode / Cline as `WorkerAdapter`s | Free interop (Zed, Goose clients); depends on ACP exposing every governed event Heddle needs |

**Recommendation to carry into the spike:** adopt **ACP as Heddle's own client↔core boundary** (standard, has a Rust SDK, gives free multi-client interop) and evaluate A vs B vs C for the *execution* tier behind it. Port **Aider's repo-map** (tree-sitter crates) for context selection (Spike 3). Use **`rmcp`** for the Tool/MCP Gateway rather than a bespoke MCP client (Spike 4). This reframes ADR-0003's Spike 1 from "which product do we wrap" to "we own an ACP core; which executor sits behind it".

## Primary sources

- Claude Code extension model and hooks: https://code.claude.com/docs/en/features-overview and https://code.claude.com/docs/en/hooks
- OpenCode: https://github.com/anomalyco/opencode
- Cline: https://github.com/cline/cline
- Goose custom distributions: https://github.com/aaif-goose/goose/blob/main/CUSTOM_DISTROS.md
- Hermes Agent: https://github.com/NousResearch/hermes-agent
- Archon: https://github.com/coleam00/Archon
- Aider: https://github.com/Aider-AI/aider
- LibreChat: https://github.com/danny-avila/LibreChat
- Open WebUI license: https://github.com/open-webui/open-webui/blob/main/LICENSE
- LangGraph: https://github.com/langchain-ai/langgraph
- Lost in the Middle: https://arxiv.org/abs/2307.03172
- RULER: https://arxiv.org/abs/2404.06654
- LongCodeBench: https://arxiv.org/abs/2505.07897
- MCP Rust SDK (rmcp): https://docs.rs/rmcp
- Agent Client Protocol (ACP) Rust SDK: https://github.com/agentclientprotocol/rust-sdk
- Agentic AI Foundation (Goose governance): https://aaif.io/projects/goose/

