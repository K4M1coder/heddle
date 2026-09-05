# Adversarial Reconciliation Review — PRD Update Draft and Addendum

**Date:** 2026-07-16  
**Review type:** Adversarial, read-only source reconciliation  
**Primary artifacts:** `PRD.update-draft.md`, `addendum.update-draft.md`  
**Verdict:** **FAIL — NOT READY FOR CANONICAL PROMOTION OR IMPLEMENTATION**

## 1. Executive Verdict

The update draft is materially stronger than the canonical PRD: it introduces stable requirements, negative outcomes, evidence expectations, honest capability declarations, staged release hypotheses, explicit build authorization, and a substantially better separation between product requirements and implementation mechanisms.

It is not yet a complete reconciliation of durable user intent. Four blocking contradictions or omissions remain, principally around the local-backend lifecycle, the normative Local no-egress guarantee, hierarchical MCP authorization/defaults, and conflicting authority semantics between API, CLI, and UI. Eleven additional major findings leave important intent underspecified or parked only as non-normative questions. No product implementation or implementation swarm is authorized.

### Finding counts

| Severity | Count |
|---|---:|
| Critical | 4 |
| High | 11 |
| Medium | 7 |
| Low | 2 |
| **Total** | **24** |

## 2. Critical Findings

### C-01 — The required local-backend lifecycle is still not a normative product outcome

**Evidence:** The durable decision states that the local backend always exists, network exposure is configurable, and local execution stands down while the application is attached to another exposed Heddle instance (`.memlog.md:14-15`; `reconcile-user-conversations.md:59-61`). The PRD defines explicit modes and isolated transitions (`PRD.update-draft.md:177-183`) and defines Server and Remote terms (`PRD.update-draft.md:464-465`), but never requires the backend to remain installed/functional, bind locally by default, cease being the active execution backend on Remote attachment, or reactivate safely after detachment. The addendum leaves all of this as open architecture questions (`addendum.update-draft.md:121-133`).

**Impact:** An implementation could satisfy the draft while destroying or disabling the local backend in Remote mode, exposing it by default, running local and remote execution concurrently, or failing over silently. This drops a direct user decision and affects process ownership, split-brain safety, mode isolation, and recovery.

**Required disposition:** Add a normative requirement and state-machine acceptance outcomes for backend existence, default bind/exposure, active-backend selection, stand-down, detach/reactivation, failure, and explicit user choice. Then bind it to BD-002 and architecture.

### C-02 — The PRD weakens the adopted Local hard no-egress invariant

**Evidence:** The constitution requires Local mode to have network egress off (`constitution.md:10-12`), the design-completeness policy identifies the hard network boundary as a one-way door (`DESIGN-COMPLETENESS-POLICY.md:17-20`), and the memlog records it as decided (`.memlog.md:20`). The PRD principle says “offline-capable by default” (`PRD.update-draft.md:32`), but its glossary defines Local as allowing “declared local processing and destinations” and explicitly says it is not necessarily network-disconnected (`PRD.update-draft.md:457`). A-003 treats strict no-egress as a hypothesis that may narrow (`PRD.update-draft.md:390`), while FR-020 assumes Local telemetry export creates no external traffic (`PRD.update-draft.md:291-295`). Existing architecture AD-4 still relies on adapter `requires_network()` metadata (`architecture.md:40-43`), which the validation report already rejected as insufficient.

**Impact:** The authoritative artifacts can produce incompatible implementations: hard network isolation, policy-only denial, loopback-only access, or merely “no cloud inference.” A one-way security boundary cannot remain semantically optional.

**Required disposition:** Preserve hard no-egress as the normative Local invariant and make platform support conditional on proof. If an OS cannot enforce it, that OS/capability is unsupported for strict Local rather than silently weakening the meaning of Local. Define loopback, update, discovery, DNS, child-process, remote tool, telemetry, identity, and secret-provider behavior.

### C-03 — Hierarchical MCP authorization and local/disabled defaults remain dropped

**Evidence:** The durable decisions require MCP connectors to be local-only or disabled by default and authorized by the applicable silo/team/project/conversation owner (`.memlog.md:18`; `reconcile-user-conversations.md:81-82,94,186-188`). FR-011 provides a good lifecycle and separates read/mutation (`PRD.update-draft.md:217-223`), while FR-008 defines value/lock resolution (`PRD.update-draft.md:193-199`). Neither binds connector enablement/grants to owner authority at each scope, specifies inheritance/delegation, nor requires a minimal local-only default. The addendum defines `ToolGrant/MCPTrust` fields but omits hierarchical grant authority (`addendum.update-draft.md:79`) and tests one local and one remote server without first defining who may authorize each (`addendum.update-draft.md:101-103`).

**Impact:** An administrator, project lead, conversation owner, installer, or worker could all plausibly become the effective connector authority. Bundled connectors could be interpreted as enabled connectors. This is a direct security and product-governance gap.

**Required disposition:** Add explicit product outcomes for default state, discovery versus activation, scope ownership, delegation, higher-scope denial/locks, lower-scope tightening, read/mutation separation, and revocation propagation. Include a truth table and negative cross-surface tests.

### C-04 — API/CLI/UI authority is internally inconsistent

**Evidence:** The user required the UI to be an overlay and the product to function entirely by CLI and API. The constitution says the headless API exposes every capability, the CLI is its complete authoritative client, and the UI adds none (`constitution.md:7-8`). The architecture says the CLI is authoritative and the UI emits CLI/API commands (`architecture.md:25-28,84-85`). The PRD instead calls GUI surfaces peer clients and requires equivalent API/CLI lifecycle events (`PRD.update-draft.md:24-26,137-143`). The addendum calls `ApplicationProtocol` canonical but does not resolve whether API semantics or CLI behavior is the conformance authority (`addendum.update-draft.md:68`).

**Impact:** This ambiguity affects E2E oracles, protocol versioning, authentication, streaming, error semantics, whether UI may call CLI subprocesses, and which surface resolves disagreements.

**Required disposition:** Adopt one precise hierarchy. Recommended: the versioned headless application protocol is normative; CLI is the complete reference client and E2E oracle; UI is another non-privileged client and never shells through the CLI unless explicitly treated as an adapter. Amend constitution, PRD, and architecture together.

## 3. High Findings

### H-01 — Ownership is present, but the explicit non-affiliation and repository-language prohibitions have no durable destination

The PRD correctly identifies Cédric Thedrez, `kamicoder`, and `cethgame` and calls Heddle independent/open source (`PRD.update-draft.md:20-24`). The forbidden organizational association does not appear in either draft, which is correct, but no governance requirement prevents its future introduction. English-only persistent code/documentation is absent from both drafts despite `reconcile-user-conversations.md:281-282`. These are project-governance rules rather than product FRs, but they still require explicit placement before readiness.

**Required disposition:** Put ownership, non-affiliation, English-only repository content, and French-only user conversation policy in project governance/contribution/release metadata, referenced by the build gate. Do not insert the forbidden organization name into normal project materials.

### H-02 — Computer-control scopes lack the required safe default and explicit full-computer semantics

FR-021 supports project, directory, application/window, or broader sessions (`PRD.update-draft.md:297-303`) but does not state that project scope is the default, enumerate virtual keyboard/mouse and screen/window capture, define whole-computer scope as exceptional, or bind scope-grant authority to the hierarchy. The durable decisions require project default and explicit widening (`.memlog.md:19`; `reconcile-user-conversations.md:191-193`).

### H-03 — The multimodal roadmap loses the accepted dependency-based v2–v8 mapping

The PRD compresses all post-V1 work into an unnumbered sequence (`PRD.update-draft.md:116-118`) and FR-022 aggregates every modality (`PRD.update-draft.md:305-311`). The reconciled intent records an accepted sequence: v2 perception inputs, v3 cowork, v4 Office/text/audio/image generation, v5 animation/video, v6 omni, v7 real-time audio, and v8 multilingual real-time collaboration (`reconcile-user-conversations.md:165-173`). Avoiding premature release promises is sound, but deleting the accepted ordering loses a product decision.

**Required disposition:** Preserve the sequence as roadmap stages or capability milestones, explicitly non-calendar and subject to independent gates. Detailed specs may remain deferred.

### H-04 — Real-time translation omits the defining team-collaboration experience

FR-022 mentions real-time speech and translation but not the required Teams/team-chat scenario in which each participant writes, reads, speaks, and hears in their native language (`reconcile-user-conversations.md:173`). “Declared quality/latency limits” is not a user journey or four-direction acceptance model.

### H-05 — Practical local omni scheduling/resource brokerage is absent

The draft supports multi-model orchestration but lacks the resource broker needed to schedule CPU/GPU/RAM/VRAM, model residency/load cost, priority, and interactive versus batch work (`reconcile-user-conversations.md:156`). FR-004 covers endpoint capability selection but not local resource admission, eviction, concurrency, or degradation.

### H-06 — Advanced ACL-aware multimodal RAG and memory are reduced to generic context selection

FR-016 provides an excellent `ContextManifest`, but the product intent also requires ingestion/versioning, hybrid lexical/vector retrieval, reranking, graph/temporal/code indexes, source ACL propagation, deletion propagation, and distinct memory scopes (`reconcile-user-conversations.md:158`). The draft does not explicitly retain those outcomes or disposition them to a future feature.

### H-07 — Enterprise integration scope is too generic to preserve named minimums

FR-012 names suites but not the durable minimum of Jira, Bitbucket, Confluence, M365, and Google Workspace capability families or local MCP server options where feasible (`reconcile-user-conversations.md:181-187`). “Selected” connectors can legally mean none of several explicitly requested systems. Exact operations should remain registry-driven, but minimum target families need roadmap disposition.

### H-08 — Identity-source requirements are abstracted beyond verifiable target profiles

FR-009 says “versioned external identity profiles” without preserving local users, LDAP, OIDC, Entra ID groups, and Google Workspace groups as named targets. Abstraction is appropriate for the contract, but the roadmap needs explicit target profiles and release/non-release disposition.

### H-09 — JIT secrets are strong at product level, but bootstrap and provider selection remain incomplete

FR-019 correctly excludes values from models, prompts, arguments, logs, telemetry, and persisted results (`PRD.update-draft.md:281-287`). The addendum names 1Password and OpenBao (`addendum.update-draft.md:141`), but does not decide the reliable free/open-source baseline, local bootstrap root-of-trust, emergency/recovery path, or how credentials reach MCP/identity/provider adapters without circular secret dependencies.

### H-10 — Development bootstrap is described but not yet an acceptance artifact or executable pre-code gate

The addendum lists a strong substrate (`addendum.update-draft.md:155-170`) and SP-006 defines a clean-machine spike (`addendum.update-draft.md:109-111`). However, no named bootstrap contract, command, environment manifest, support matrix, generated connection verification, or evidence location exists. The Build Authorization Gate says bootstrap and CI/CD must work before product source changes merge (`PRD.update-draft.md:435`) but does not distinguish permitted bootstrap/tooling commits from prohibited product code.

**Required disposition:** Create a planning-slice deliverable for idempotent tri-OS bootstrap before the first product-code commit, with profiles, lockfiles, environment report, secret-free connector checks, and recovery from clone.

### H-11 — CI/CD and staging controls are a list, not a traceable gate model

The addendum enumerates test and supply-chain classes (`addendum.update-draft.md:164-168`) and swarm staging constraints (`addendum.update-draft.md:172-185`), but lacks stage names, entry/exit criteria, branch protections, artifact promotion rules, failure ownership, evidence schemas, and mapping from each current-slice requirement to required jobs. The user explicitly rejected implementation without checklists and quality gates.

## 4. Medium Findings

### M-01 — Several acceptance outcomes are not independently measurable

Terms such as “useful local behavior,” “one coherent result,” “actionable setup information,” “minimum required secret,” and “appropriate to supported hardware” lack named measurement methods or calibration ownership (`PRD.update-draft.md:32,169-175,281-287,305-311,335-337`). Phase-specific thresholds may be deferred, but the oracle type and threshold-setting gate must be named.

### M-02 — SM-004 permits missing evidence while FR-014 implies complete reconstruction

SM-004 allows only 99.9% event correlation for successful conformance runs (`PRD.update-draft.md:356`), while FR-014 and the Ledger promise reconstruction of all supplied/returned/decided/applied material (`PRD.update-draft.md:241-247`). The missing 0.1% may contain a protected effect. Define critical-event completeness as 100% and use a lower operational SLO only for non-critical telemetry.

### M-03 — “Zero known disclosure” is weaker than the stated isolation release oracle

NFR-002 says zero *known* disclosure (`PRD.update-draft.md:319-321`), which can pass through insufficient testing. SM-006 is stronger but depends on an undefined matrix (`PRD.update-draft.md:358`). The release gate should require completion and coverage of the declared matrix, not merely absence of known incidents.

### M-04 — Reduced-assurance workers create a scope leak

FR-017 permits a reduced-assurance mode for opaque workers (`PRD.update-draft.md:265-271`) without defining whether model I/O still enters the Ledger, whether such workers may read protected context, or how users avoid confusing their outputs with governed execution. This can undermine the product's central transparency promise even if protected effects are blocked.

### M-05 — “Every input/output” transparency is qualified without an explicit unavailable-content UX

FR-014 sensibly permits lawful erasure and secret exclusion, but no outcome specifies what users see when provider-side hidden reasoning, encrypted/erased payload, proprietary worker internals, or streaming loss prevents exact capture. The distinction between exact observable I/O and unavailable internal reasoning must be explicit to avoid overclaim.

### M-06 — The feasibility implications of a repository exceeding 1M tokens are not retained as planning constraints

The active-context distinction is correctly preserved in principle, FR-016, SP-003, metrics, and ADR-0003. The drafts omit the expectation-management conclusions that a mature repository will likely exceed 1M source tokens and that rapid clones do not evidence enterprise maturity (`reconcile-user-conversations.md:253-264`). These belong in feasibility/development strategy, not the product FRs, but need a deliberate destination.

### M-07 — The addendum partially becomes a process policy without a governance/precedence rule

Sections 9 and 10 contain mandatory development and swarm policy while the addendum declares itself non-normative mechanism detail (`addendum.update-draft.md:11-13,155-185`). If non-normative, those gates can be ignored; if mandatory, they belong in the constitution, development policy, CI contract, or BMAD/Spec-Kit bridge governance.

## 5. Low Findings

### L-01 — Terminology drifts between Local/Server/Remote and Online-Server/Online-Remote

The memlog records Online-Server and Online-Remote (`.memlog.md:12`), while the PRD normalizes them to Server and Remote. This is acceptable if declared as canonical terminology, but no alias/deprecation statement exists.

### L-02 — “One installer experience” may overstate optional-pack portability

The addendum presents one installer experience while also requiring optional heavy capability packs (`addendum.update-draft.md:17,170`). Until SP-006 passes, describe this as the target packaging hypothesis and avoid implying one identical artifact across all operating systems.

## 6. Explicit Verification Matrix

| Required verification | Status | Adversarial conclusion |
|---|---|---|
| Identity and ownership | Pass | Cédric Thedrez, `kamicoder`, `cethgame`, independent open-source identity are explicit in the PRD. |
| Prohibited organizational association | Partial | Forbidden association is absent, as required, but no durable governance prohibition prevents later introduction. |
| English-only persistent code and documentation | Fail | Not stated or assigned to a governance artifact. |
| Local/Server/Remote modes | Partial | Explicit selection and isolation exist; lifecycle, discovery, loss, and active-backend semantics remain unresolved. |
| No cross-mode sharing | Pass at requirement level | FR-006/FR-007 cover direct and derived domains with negative tests; architecture contract remains pending. |
| Remote sharing only inside one team | Pass at requirement level | Explicit in principles, journeys, FR-007, and metrics; membership lifecycle still needs contracts. |
| CLI/API/UI relationship | Fail | User intent is present but authority hierarchy conflicts across PRD, constitution, and architecture. |
| Local backend always functional; exposure configurable; stand-down on Remote | Fail | Only an open architecture question, not a product requirement. |
| MCP authorization hierarchy | Fail | Lifecycle is strong; owner/delegation/inheritance and local/disabled defaults are missing. |
| Computer access scopes | Partial | Scope types exist; project default, exact primitives, full-computer exception, and hierarchical grant authority are missing. |
| JIT secrets | Pass with design gap | Product outcome and leakage boundaries are explicit; provider/bootstrap root-of-trust remains unresolved. |
| Multimodal roadmap and omni composition | Partial | Omni composition is explicit; accepted dependency-based v2–v8 milestones and several concrete modalities/use cases are compressed away. |
| Real-time audio and translation | Partial | Named in FR-022; duplex team communication in each participant's native language lacks a journey and oracle. |
| Development bootstrap | Partial | Strong candidate checklist and spike exist; no executable contract/evidence package exists yet. |
| CI/CD quality gates | Partial | Gate classes are listed; pipeline semantics, traceability, checklists, and staging contract remain absent. |
| 1M repository/context distinction | Pass | Correctly treats 1M as overflow, not working memory or a repository limit; SP-003 provides a suitable evidence direction. |

## 7. Product-versus-Process Placement Review

| Concern | Correct durable destination |
|---|---|
| User-visible modes, backend lifecycle, connector authority/defaults, computer grants, modality journeys | Canonical PRD requirements and journeys |
| Event, workflow, worker, policy, silo, secret, context, and effect semantics | Accepted architecture, ADRs, versioned contracts, examples, conformance fixtures |
| Ownership, non-affiliation, English-only repository policy | Project governance, contribution guide, release metadata |
| BMAD/Spec-Kit handoff, no-code-before-readiness, role separation | Constitution and bridge process policy |
| Bootstrap, quality tools, CI stages, staging and promotion | Development-environment specification, CI contract, runbooks, checklists |
| v2–v8 capability ordering | Product roadmap with per-stage Spec-Kit elaboration triggers |
| Clone-versus-enterprise feasibility and repository/context sizing | Feasibility and engineering-strategy document |

The current addendum mixes all four latter classes while disclaiming normative authority. Each item needs an explicit destination and traceability link before canonical promotion.

## 8. Required Reconciliation Gate

The drafts may proceed to another update cycle only after:

1. C-01 through C-04 are resolved consistently across PRD, constitution, architecture, and decision records.
2. Every High finding has a destination, owner, closure artifact, and acceptance oracle or an explicit owner-approved rejection.
3. Product intent is not hidden solely in non-normative “questions” or spike prose.
4. Governance and development-process requirements are moved to authoritative policies and referenced by the Build Authorization Gate.
5. A new adversarial reconciliation reports zero Critical findings.

**Final verdict: FAIL — the update is a credible remediation draft, but it is not yet a lossless or contradiction-free BMAD PRD update and must not authorize implementation, staging, or an autonomous implementation swarm.**
