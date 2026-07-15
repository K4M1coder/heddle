# PRD Quality Review — Skein

## Overall verdict

The PRD has a distinctive thesis, an unusually serious governance model, and strong supporting design research, but it is **not decision-ready as a build authorization**. The principal risks are an unresolved runtime/composition decision that changes the Phase 0 path, a platform-sized MVP whose priorities are not reconciled with its stated outcomes, and functional requirements that mostly lack testable completion criteria; these defects would propagate ambiguity into architecture, stories, and autonomous implementation.

## Decision-readiness — broken

The PRD does state meaningful decisions in §§1, 5, and 6: Skein owns the control plane, remains local-first, exposes one backend selectively, isolates modes into silos, and treats the CLI/API as authoritative. It also names four open questions in §8. However, the most consequential runtime decision is presented as both a baseline decision and an unresolved experiment: FR-18 says Skein owns the loop, while Open Question 1 and ADR-0003 leave the worker/runtime route behind five uncompleted spikes. The supporting architecture is still `draft`, ADR-0003 is `Proposed`, and the epic breakdown makes Phase 0 depend on that evidence.

The PRD does not expose the sacrifice attached to the broad v1 bundle. It selects agent runtime, workflows, Ledger, local/team modes, UI, identity/RBAC, observability, secrets, enterprise connectors, and framework integration together, but does not state what will be removed if schedule, packaging, or security evidence fails. For a chain-top PRD that is intended to authorize autonomous downstream execution (§0), this is a blocking decision gap.

### Findings

- **critical** Runtime ownership is asserted but the executable runtime choice is not approved (§4.12 FR-18; §8 Q1; ADR-0003 “Quality gate”) — Phase 0 cannot be planned reliably while the default/native path and eligible worker surfaces remain spike-dependent. *Fix:* complete the five bounded evidence spikes, accept or reject ADR-0003, then record the selected Phase 0 runtime contract and rejected alternatives directly in the PRD decision log.
- **critical** No explicit product green-light gate exists (§6.1; §8) — the PRD can be mistaken for authorization to implement despite unresolved one-way-door evidence, missing per-feature acceptance, and a draft architecture. *Fix:* add a “Build authorization” section with named mandatory artifacts, owners, evidence, zero-critical threshold, and an explicit `NOT READY / READY` state.
- **high** MVP trade-offs are hidden behind an additive scope (§6.1; design §8 Phase 1) — the document chooses nearly every platform foundation for v1 without saying what is sacrificed or which capability is the minimum market/user proof. *Fix:* rank v1 capabilities as must/should/could, define a smallest independently valuable release, and name the de-scope order.
- **high** Open questions have no owners, deadlines, or decision criteria (§8) — all four affect architecture or evidence capture, yet none identifies who closes it or what evidence is sufficient. *Fix:* convert each question into a decision record entry with owner, due gate, options, acceptance criteria, and blocking/non-blocking status.
- **medium** The PRD has no `[NOTE FOR PM]` callouts at genuine tensions (§§6–8) — tensions such as full Local mode versus bundled enterprise connectors, immutable full-content traceability versus minimization, and one installer versus optional sidecars are only resolved in external documents. *Fix:* add concise PM notes at these decision boundaries and link the authoritative ADR.

## Substance over theater — adequate

Most of the content is earned. Silo isolation, the Ledger, hierarchical locks, loop budgets, external ground truth, JIT secrets, and `ContextManifest` are product-specific and backed by architecture or research. The three journeys drive actual architecture decisions, and the compliance text correctly says that software supplies controls rather than certification.

The weak area is breadth language. “All AI providers,” “enterprise compliance,” connector coverage, embedded inference, and “masters” several development methods read as universal promises without capability boundaries. FR-9 and the compliance appendix list important controls but do not define operational thresholds, evidence outputs, or a control baseline, which makes part of the NFR content ceremonial rather than testable.

### Findings

- **high** Universal capability claims are not bounded (§1; FR-3–FR-5) — “all AI providers,” native business integrations, and framework “mastery” cannot be verified as written and obscure the adapter/certification limits described elsewhere. *Fix:* replace absolutes with a versioned capability registry and name the providers, protocols, framework conformance suites, and support tiers required for each release.
- **high** Compliance requirements are control lists rather than product contracts (§4.6 FR-9; Compliance & Regulatory) — no evidence package, retention bounds, data-subject workflow, audit access controls, or control-to-test mapping is required by the PRD. *Fix:* define compliance-support outcomes and trace each to FRs, tests, evidence, and explicit organizational responsibilities.
- **medium** Observability is asserted without service-level meaning (FR-9) — “OpenTelemetry observability” does not specify mandatory spans, redaction behavior, offline export behavior, retention, or acceptable telemetry loss. *Fix:* add minimum semantic conventions and measurable privacy/reliability criteria.
- **low** The vision phrase “masters the BMAD / Spec-Kit / powerskills methods” is marketing language (§1) — it has no conformance definition. *Fix:* use “provides versioned, tested workflow profiles conforming to named upstream versions” and define the conformance checks.

## Strategic coherence — thin

The thesis is coherent: a Skein-owned, local-first control plane can unify chat, code, and cowork while retaining policy, context, evidence, and workflow ownership. FR-6, FR-10, FR-13, FR-16, FR-17, and FR-18 reinforce that thesis, and the roadmap correctly stages perception, action, generation, omni composition, real-time voice, and translation.

The prioritization and success metrics do not fully validate it. SM-1 is a feature integration demonstration, not evidence that the harness is more reliable, governable, or context-efficient than optional workers or existing tools. SM-2 and SM-3 cover isolation and inspectability, but no metric measures governed-loop success, recovery, policy enforcement, smallest-sufficient context quality, user control, or time-to-value. The broad platform MVP therefore resembles a capability backlog under a strong thesis rather than a thesis-driven first release.

### Findings

- **critical** Success metrics do not validate Skein’s core differentiation (§7) — there is no measure for workflow reliability, bounded-loop completion, recovery, policy correctness, context selection quality, or evidence completeness. *Fix:* add outcome metrics and counter-metrics tied to the control-plane thesis, with baselines against at least one native and one external-worker path.
- **high** SM-1 over-couples MVP success to three surfaces and two enterprise suites (§7 SM-1) — a CLI/API/UI + Confluence + Bitbucket + Jira scenario can fail for integration breadth even if the core thesis is proven, or pass while governance quality is poor. *Fix:* split core-platform proof from connector/surface expansion and assign separate release gates.
- **high** Feature priority does not follow an explicit minimum-value argument (§6.1) — UI, M365/Atlassian, workflow, task tracking, modes, RBAC, secrets, observability, and framework support are all in scope with no dependency/value ranking. *Fix:* define an ordered release hypothesis and a dependency-aware cut line for Phase 0, local alpha, team alpha, and v1.
- **medium** Only one counter-metric is defined (§7 SM-C1) — cost, latency, false approvals, context bloat, replay safety, and operator intervention could be optimized harmfully. *Fix:* add counter-metrics for policy bypass, secret/PII leakage, irreversible-effect duplication, context utilization, cost, and human escalation burden.

## Done-ness clarity — broken

Only FR-1, FR-13, FR-15, and FR-16 include explicit consequences, and even those cover only one path each. FR-2 through FR-12, FR-14, FR-17, and FR-18 primarily state capabilities. Terms such as “usable in workflows,” “packaged,” “auto-detected,” “baseline RBAC,” “immutable audit,” “compliance-by-design,” “omni,” and “eligible” do not identify observable pass/fail behavior. The roadmap exit criteria help at milestone level but cannot substitute for requirement-level acceptance.

This is the dimension most likely to cause autonomous agents to generate plausible implementations and self-authored tests that satisfy their own interpretation. Ground-truth loop discipline requires independent test oracles before task creation, not during implementation.

### Findings

- **critical** Most FRs lack an acceptance oracle (§4 FR-2–FR-12, FR-14, FR-17, FR-18) — downstream stories cannot determine completion without inventing requirements. *Fix:* give every FR at least one Given/When/Then acceptance criterion, error-path criterion, authorization criterion, and evidence artifact where applicable.
- **high** Security-critical behavior is underspecified (FR-2, FR-3, FR-6–FR-11) — confirmation classes, hard egress enforcement, deny-by-default outcomes, redaction residual risk, replay semantics, and JIT secret lifetime are not bounded in the PRD. *Fix:* add normative security acceptance criteria linked to ADR-0002 D2–D8 and the threat model.
- **high** FR-17 has no measurable context-quality contract (§4.12) — `ContextManifest` fields are named, but relevance, reproducibility, reserved headroom, ACL trimming, degradation, and comparison with full-context are not acceptance-tested. *Fix:* define benchmark datasets, quality/latency/cost thresholds, deterministic manifest checks, and failure behavior when budget is exceeded.
- **high** FR-18 eligibility is circular (§4.12) — a worker is “eligible” when required events are exposed, but the event contract, completeness threshold, and degraded modes are not specified in the PRD. *Fix:* reference a versioned `WorkerAdapter` contract and require conformance tests for model I/O, tools, approvals, correlation, effects, cancellation, and terminal verdicts.
- **medium** Milestone exits omit negative and recovery cases (§6.2; design §8) — success paths do not cover crashes, partial effects, denied actions, offline transitions, split-brain prevention, or replay safety. *Fix:* add failure/recovery acceptance scenarios to every milestone.

## Scope honesty — thin

The PRD has explicit non-goals and a clear v2–v8 roadmap, which is a meaningful strength. It also separates an enterprise track and acknowledges four open questions. Nevertheless, the v1 scope still combines a desktop agent, a backend, a workflow engine, a policy plane, enterprise connectors, local inference, identity, secrets, observability, compliance controls, and three access surfaces. The document does not state capacity, schedule, staffing, supported-provider limits, or a de-scope trigger.

The Assumptions Index does not round-trip to inline assumptions. The indexed entries are not tagged at the claimed sections, and several consequential assumptions remain unmarked: LiteLLM suitability as the initial gateway, feasibility of packaging local inference across three operating systems, usability of an embedded connector set, and viability of crypto-shredding for all Ledger payload relationships.

### Findings

- **high** The MVP is platform-complete rather than honestly minimal (§6.1) — the breadth is inconsistent with the absence of capacity and release constraints. *Fix:* distinguish Phase 0, local developer alpha, small-team alpha, and v1; state explicit non-goals and support limits for each.
- **high** Assumption tracking is mechanically broken (§9 versus §§2, 4.2, 6) — the index cites inline assumptions that do not exist at those locations, so assumptions cannot be reviewed or retired. *Fix:* add exact inline tags with stable IDs and make the index bidirectional.
- **medium** Important feasibility assumptions are presented as decisions (§§1, 4, 6) — cross-platform inference packaging, bundled MCP connectors, broad provider coverage, and team backend switching lack assumption markers or evidence state. *Fix:* tag each as validated, unvalidated, or rejected and attach a spike/evidence reference.
- **medium** Deferral boundaries conflict across documents (§6.2; Design Completeness Policy bucket C) — the PRD puts baseline Server/Remote and team authz in v1 while the policy defers election/quorum/replication until Server/Remote scheduling, without defining the v1 subset. *Fix:* specify the exact v1 Server/Remote semantics and declare excluded distributed guarantees.

## Downstream usability — thin

This PRD explicitly feeds architecture and epics/stories (§0), so downstream usability is load-bearing. It has a glossary, stable FR/UJ/SM prefixes, three role-based journeys, and useful links to architecture and research. However, the glossary delegates core terms to another document, feature-level acceptance is sparse, and cross-artifact coverage is already stale: the architecture front matter binds only FR-1 through FR-16 while the PRD includes FR-17 and FR-18.

The journeys are too few for the declared stakeholder and risk surface. There is no named journey for local first run/offline recovery, team join/leave and mode switching, connector authorization, secret setup, denied/destructive action, harness conflict resolution, GDPR export/erasure, incident investigation, or workflow failure/resume. “The engineer,” “the project manager,” and “the user” are roles rather than stable named personas, but they do carry enough inline context to avoid floating journeys.

### Findings

- **high** Cross-artifact requirement coverage is stale (§4 FR-17/FR-18; architecture front matter and capability map) — architecture metadata does not bind FR-17 or FR-18, and the epic map details only a subset. *Fix:* regenerate the traceability matrix from the canonical FR inventory and fail validation on unmapped or stale IDs.
- **high** User journeys do not cover the highest-risk behaviors (§2.3) — the PRD lacks source journeys for mode transitions, team isolation, authorization, irreversible effects, recovery, privacy rights, and administration. *Fix:* add journeys and edge cases for each security/operational boundary before architecture readiness.
- **medium** The glossary is not self-contained (§3) — `Principal`, `RBAC`, `SecretProvider`, and `IdP` are delegated to architecture/design, so extracted requirements do not carry stable definitions. *Fix:* define all normative domain nouns in the PRD and use external documents only for implementation detail.
- **low** FR ordering is non-sequential (§4: FR-13, FR-16, FR-14, FR-15, FR-17, FR-18) — IDs are unique but the order complicates review and generated coverage checks. *Fix:* order by ID or explicitly group by capability while providing a canonical sorted inventory.

## Shape fit — adequate

The PRD is correctly shaped as a multi-stakeholder platform PRD rather than a single-operator capability note. User journeys, functional requirements, non-goals, roadmap, open questions, and compliance constraints all belong. The chain-top role justifies more rigor than an ordinary open-source hobby project, particularly because the output is intended to drive architecture and autonomous story execution.

The shape becomes under-formalized where risk is highest. A platform spanning desktop control, enterprise data, identity, secrets, event sourcing, and regulated evidence needs explicit constraint traceability, trust boundaries, and operational journeys. Conversely, detailed implementation nouns such as LiteLLM, Ollama, SQLite, Vikunja, and candidate workers appear in the PRD without a clean separation between product requirement and replaceable architectural default.

### Findings

- **high** Regulatory and security constraint traceability is absent (Compliance & Regulatory; FR-8–FR-11) — a regulated-capability PRD needs a control/requirement/evidence mapping, not only a narrative appendix. *Fix:* add stable constraint IDs and trace them to FRs, risks, tests, evidence, and accountable owner.
- **medium** Product requirements and implementation defaults are mixed (§§3–6) — naming LiteLLM, Ollama, SQLite-adjacent concepts, Vikunja, and specific worker candidates in normative requirements makes replaceable choices look contractual. *Fix:* state capability requirements in the PRD and move defaults/options to architecture or clearly label them non-normative.
- **low** Persona depth is insufficient for the B2B/team shape (§2) — roles are present, but administrators, security/privacy reviewers, and non-technical cowork users have no decision-driving context. *Fix:* add only the personas that change authorization, deployment, audit, or UX decisions and map each to journeys/FRs.

## Mechanical notes

- **ID continuity:** FR IDs 1–18 are unique and numerically complete, but are not presented in sequence. UJ-1–UJ-3 and SM-1–SM-3 are contiguous; `SM-C1` is a clearly differentiated counter-metric.
- **Cross-references:** Architecture metadata and maps are stale for FR-17/FR-18. FR ranges such as “FR-1..FR-5” in SM-1 mask which individual requirements are actually demonstrated. The PRD points to design sections and research files, but acceptance depends on information outside the PRD.
- **Assumptions Index:** The three index entries do not have corresponding inline `[ASSUMPTION: …]` tags at §§2, 4.2, and 6. The round-trip check therefore fails.
- **Glossary drift:** “Online-Server”/“Server,” “Online-Remote”/“Remote,” “team layer”/“local layer” versus the four-level hierarchy, and “Gateway” as both a LiteLLM product choice and a general model boundary should be normalized. “powerskills” and “superpowers” are used across project documents and need one canonical term or an explicit distinction.
- **UJ protagonists:** Each journey has a role-carrying protagonist, but no stable persona IDs or names. This is acceptable mechanically, though inadequate for the declared stakeholder breadth.
- **Required sections:** Vision, target users, journeys, glossary, features, non-goals, scope, metrics, open questions, assumptions, compliance, and audit are present. Missing for the stakes: explicit prioritization, decision/build gate, constraints/traceability matrix, and requirement-level acceptance criteria.
