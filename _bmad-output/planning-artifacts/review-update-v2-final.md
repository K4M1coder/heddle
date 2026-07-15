# Final Pre-Promotion BMAD PRD Review — Skein v2 Update Draft

**Review date:** 2026-07-16  
**Review intent:** BMAD PRD reviewer gate, read-only  
**Primary draft:** `_bmad-output/planning-artifacts/PRD.update-draft.md`  
**Companion:** `_bmad-output/planning-artifacts/addendum.update-draft.md`  
**Promotion verdict:** **PROMOTE**  
**Product build authorization:** **NOT_READY**

## Overall Verdict

The v2 PRD draft is ready to replace the canonical PRD through the controlled BMAD update/promotion step. It now preserves the durable product intent, separates product outcomes from implementation mechanisms, closes the prior four critical reconciliation defects at the normative requirement level, gives every prior high-severity concern an authoritative destination and closure gate, and states unambiguously that promotion does not authorize product implementation.

Promotion must preserve the draft content and stable IDs. It must not mark architecture, ADR-0003, G0–G6, any blocking decision, or any product slice ready. The accepted next state is therefore: **canonical PRD promoted; architecture and downstream artifacts stale by design; `build_authorization: NOT_READY`; no product-code implementation swarm authorized.**

## Finding Counts

| Severity | Count |
| --- | ---: |
| Critical | 0 |
| High | 0 |
| Medium | 3 |
| Low | 2 |

The medium and low findings are post-promotion planning obligations or promotion-hygiene notes. None makes the PRD unsafe to promote because each is explicitly fail-closed behind the Build Authorization Gate and the normative quality-gate policy.

## Seven Rubric Dimensions

### 1. Decision-readiness — strong

The draft states the product thesis, release hypotheses, de-scope order, non-negotiable guarantees, twelve owned blocking decisions, required closure evidence, and ten explicit READY conditions. It makes the immediate decision actionable: promote the PRD, continue evidence and architecture work, and do not implement product behavior. Trade-offs are explicit rather than phrased as neutral considerations, especially for platform support, worker assurance, connector mutation, strict Local support, and optional capability breadth.

### 2. Substance over theater — strong

The PRD replaces universal-provider, universal-compliance, universal-reversibility, and single-model “omni” claims with release-specific capability declarations, qualified effect semantics, measurable evidence, and compliance-support language. Personas and journeys are load-bearing risk scenarios rather than decorative profiles. The addendum correctly carries stack hypotheses, component candidates, mechanism questions, rejected postures, and spike design outside the normative product requirements.

### 3. Strategic coherence — strong

The product has a clear thesis: a local-first governed control plane that provides a coherent chat, code, cowork, and future omni experience while owning policy, workflow, context, evidence, and completion independently of models and workers. Phase 0, Local Alpha, Team Alpha, V1, and v2–v8 milestones follow that thesis. `SM-011`, `SM-012`, and `CM-007` now test user value and adoption rather than safety activity alone; `CM-001`–`CM-008` prevent success metrics from rewarding policy bypass, hidden labor, unsafe context growth, or resource overcommit.

### 4. Done-ness clarity — adequate

All 22 functional requirements provide positive behavior, negative behavior, and expected evidence. Critical NFRs require pre-registered matrices, 100% critical-event evidence, explicit blocked outcomes, and release-specific platform and resource declarations. Exact schemas, fixtures, numeric thresholds, and test commands correctly remain downstream contracts governed by `BD-001`–`BD-012` and G0–G5. This dimension is adequate rather than strong because those contracts and the machine-readable gate manifest do not yet exist; that is an implementation-readiness blocker, not a PRD-promotion blocker.

### 5. Scope honesty — strong

The draft is explicit about what each release hypothesis includes and excludes, labels computer control and multimodal composition as post-V1 gated capabilities, preserves the non-calendar v2–v8 dependency sequence, and defines the order in which breadth must be removed before guarantees are weakened. Assumptions are labeled, owned, test-triggered, and paired with failure responses. The PRD no longer presents a platform-complete MVP or a mature enterprise product as near-term scope.

### 6. Downstream usability — adequate

The PRD provides stable contiguous namespaces, a self-contained glossary, named journeys, evidence outcomes, assumption and blocking-decision registers, and explicit BMAD–Spec-Kit handoff criteria. It is suitable as the new chain-top source. Architecture, epics/stories, and existing Spec-Kit artifacts are deliberately stale and must be regenerated only after promotion; the PRD explicitly blocks G0 and build authorization until this happens. Downstream usability becomes strong only after that mechanical reconciliation reports zero stale or orphaned IDs.

### 7. Shape fit — strong

The shape fits a high-stakes, multi-surface, multi-stakeholder, chain-top platform PRD. Product outcomes remain in the PRD; implementation candidates and spike mechanisms remain in the addendum; repository governance and quality-gate authority live in dedicated normative policies. The document is long because isolation, identity, tools, evidence, cross-platform packaging, enterprise integration, and autonomous work are genuinely load-bearing concerns.

## Retest of the Four Prior Critical Findings

| Prior ID | Disposition | Retest evidence |
| --- | --- | --- |
| C-01 — Local-backend lifecycle absent | **Closed for PRD promotion** | FR-006 now requires the local backend to remain installed, functional, and local-only by default; explicit Server exposure; exactly one active execution backend per client session; Remote stand-down; named detachment/reactivation; no implicit fallback, migration, merge, concurrent execution, or replay. UJ-04 and BD-002 carry journey and closure evidence. |
| C-02 — Local hard no-egress weakened | **Closed for PRD promotion** | Principle 3.1, FR-003, FR-005, FR-020, A-003, NFR-002, BD-003, SP-005, and G2 preserve hard no-egress. Local IPC/loopback exceptions must be pre-registered TCB endpoints. A platform that cannot prove enforcement is unsupported for strict Local; the invariant is not weakened. The stale architecture mechanism remains blocked from implementation. |
| C-03 — Hierarchical MCP authority/defaults dropped | **Closed for PRD promotion** | FR-011 now separates discovery, installation, trust, activation, grants, invocation, update, revocation, and removal; defaults connectors to disabled except for a declared least-privilege local-only base; binds authority and bounded delegation to Silo/Team/Project/Conversation; separates read and mutation; preserves ancestor locks/security floors and descendant revocation. PROJECT-GOVERNANCE makes these rules authoritative; BD-007 and G2 require formal tests. |
| C-04 — API/CLI/UI authority inconsistent | **Closed for PRD promotion** | Vision and FR-001 define `ApplicationProtocol` as normative, CLI as the complete reference client and E2E oracle, and graphical interfaces as non-privileged protocol peers that never shell through the CLI. This is compatible with Constitution Principle I's authoritative headless API and complete CLI client. Architecture remains a draft requiring post-promotion revalidation under BD-011 and cannot authorize a conflicting implementation. |

## Retest of the Eleven Prior High Findings

| Prior ID | Disposition | Retest evidence |
| --- | --- | --- |
| H-01 — Ownership, non-affiliation, and language lacked a durable destination | **Closed** | `docs/PROJECT-GOVERNANCE.md` normatively establishes independent ownership, forbids implied affiliation without verified authority, and requires English for all persistent repository artifacts while allowing French owner conversation. The PRD identifies Cédric Thedrez, `kamicoder`, and `cethgame`. |
| H-02 — Computer-control safe defaults absent | **Closed** | FR-021 enumerates keyboard, mouse, screen, window, browser, clipboard, and accessibility primitives as separate grants; defaults to project scope; treats folder/application/screen/full-computer widening as new visible time-bounded grants; makes full-computer access exceptional; and requires interruption and negative-boundary evidence. |
| H-03 — Accepted v2–v8 sequence lost | **Closed** | Section 5.5 restores v2 perception, v3 cowork, v4 creation, v5 motion, v6 omni, v7 real-time audio, and v8 native-language collaboration as a non-calendar dependency sequence with independent gates. |
| H-04 — Native-language team experience omitted | **Closed** | UJ-11 defines writing, reading, speaking, and hearing in each participant's chosen language, including per-direction failure, consent, identity, confidence, latency, and team-isolation behavior. FR-022 retains the gated capability. |
| H-05 — Local omni resource brokerage absent | **Closed** | FR-022 requires admission control for CPU, GPU, RAM, VRAM, storage, model residency, load/unload cost, eviction, concurrency, priority, and cancellation, with visible queue/degradation/refusal and no silent cloud fallback. `CM-008` prevents unsafe optimization. |
| H-06 — Advanced ACL-aware RAG and memory omitted | **Closed** | FR-016 now retains multimodal ingestion, source versioning/deletion, lexical/vector retrieval, reranking, graph/temporal/code-symbol indexes, separated memory scopes, and ACL enforcement through ingestion, retrieval, reranking, transformation, promotion, and output. |
| H-07 — Named enterprise connector targets diluted | **Closed** | FR-012 explicitly names Jira, Bitbucket, Confluence, Microsoft 365, Google Workspace, and local MCP options where feasible, while correctly limiting support to registry-declared operations and versions. |
| H-08 — Named identity profiles diluted | **Closed** | FR-009 explicitly targets local users, LDAP, OIDC, Entra groups, and Google Workspace groups and defines assurance, issuer/tenant, reconciliation, deprovisioning, session, and fail-closed expectations. |
| H-09 — JIT root of trust/provider/recovery unresolved | **Closed as a promotion finding; remains a build blocker** | FR-019 makes provider selection, local root of trust, open/no-cost baseline, optional commercial providers, bootstrap, rotation, recovery, backup, and circular dependencies mandatory before implementation. BD-009 and G2 own closure; the addendum names candidates without falsely accepting one. |
| H-10 — Bootstrap lacked an authoritative pre-code contract | **Closed as a promotion finding; remains a build blocker** | PROJECT-GOVERNANCE defines which substrate changes are permitted during `NOT_READY`; QUALITY-GATES G1 defines the tri-OS, idempotent, from-clone, profile-based, pinned, secret-free, offline-aware bootstrap contract. BD-010 and SP-006 require reproducible evidence before product code. |
| H-11 — CI/CD and staging were only a list | **Closed as a promotion finding; remains a build blocker** | QUALITY-GATES G3 defines named CI evidence classes, owners, entry/exit semantics, immutable digest promotion, branch protection, retry/failure ownership, and supply-chain controls. G6 defines autonomous-role separation, task envelopes, serialized integration, staging evidence, and contradiction stop conditions. |

## Mechanical Checks

### Identifier continuity and uniqueness

Mechanical extraction found the following unique contiguous sets:

- `FR-001`–`FR-022` — 22 IDs.
- `NFR-001`–`NFR-008` — 8 IDs.
- `SM-001`–`SM-012` — 12 IDs.
- `CM-001`–`CM-008` — 8 IDs.
- `CS-001`–`CS-006` — 6 IDs.
- `A-001`–`A-010` — 10 IDs.
- `BD-001`–`BD-012` — 12 IDs.
- `P-01`–`P-06` — 6 IDs.
- `UJ-01`–`UJ-11` — 11 IDs.

No duplicate or missing ID was found inside these namespaces.

### Assumption checks

All ten assumptions appear exactly once as `[ASSUMPTION A-###]` records and include owner, validation event, and failure response. The v2 draft correctly names Section 10 an **Assumption Register**, no longer falsely claiming that the current document alone is a round-trip index. It requires downstream artifacts to generate the bidirectional assumption-to-claim-to-test index and makes that index a G0 condition.

### Cross-reference checks

Internal section references and named document references in the v2 drafts resolve. References to `docs/PROJECT-GOVERNANCE.md`, `docs/QUALITY-GATES.md`, ADR-0003, and BD-001–BD-012 are coherent. Existing architecture mappings use old `FR-1`-style identifiers and old mechanisms; the PRD explicitly classifies architecture as unaccepted and requires revalidation before G0/build authorization. Existing epics, stories, and Spec-Kit artifacts remain stale and are excluded from implementation retrieval until regenerated.

### Memlog and conversation reconciliation

The v2 draft or its authoritative companion policies preserve the durable decisions in `.memlog.md`: ownership, single-product modular architecture, headless authority, local/server/remote isolation, local-backend lifecycle, hierarchical governance, hard Local egress, JIT secrets, Ledger/effects, bounded ground-truth loops, smallest-sufficient context, omni composition, BMAD–Spec-Kit sequencing, reuse, and explicit `NOT_READY`. The four prior reconciliation omissions and all eleven prior high findings now have a normative destination and a fail-closed closure path.

## Remaining Findings

### Medium (3)

1. **Downstream architecture is intentionally stale.** `architecture.md` still uses prior FR IDs and weaker mechanisms such as adapter metadata for Local egress and three-scope RBAC. This does not block PRD promotion because architecture is `draft`, ADR-0003 is `Proposed`, BD-011 is blocked, and G0 requires revalidation. It does block implementation.
2. **The machine-readable gate manifest does not yet exist.** QUALITY-GATES defines its mandatory schema and fail-closed semantics, and the PRD references it as the closure register. Creating and validating it is required before task generation and implementation, not before canonicalizing the requirements it will trace.
3. **Generated cross-artifact and assumption traceability does not yet exist.** The PRD now defines stable source IDs and requires the generated matrix at G0. Architecture, BMAD epics/stories, and Spec-Kit artifacts must be regenerated after promotion, so their current staleness is expected and explicitly blocking.

### Low (2)

1. **Promotion metadata needs an atomic update.** The controlled promotion should replace `remediation-draft-v2`/`canonical_prd_unchanged: true` with canonical final metadata without changing requirement content or IDs, and should preserve `build_authorization: NOT_READY`.
2. **Legacy mode aliases remain in historical artifacts.** The PRD consistently uses Local, Server, and Remote while the memlog contains Online-Server and Online-Remote. Downstream glossary/migration work should record the old names as historical aliases; this is not a semantic conflict.

## Promotion Conditions

The verdict is **PROMOTE**, subject to an atomic BMAD update action that:

1. promotes `PRD.update-draft.md` as the canonical PRD without substantive rewriting;
2. promotes the companion addendum as the PRD's non-normative mechanism companion;
3. retains stable IDs and `build_authorization: NOT_READY`;
4. records the promotion in the BMAD memlog;
5. does not mark architecture, ADR-0003, epics/stories, Spec-Kit artifacts, G0–G6, or any `BD-*` item complete;
6. immediately schedules source reconciliation of architecture and downstream planning artifacts before any implementation-readiness review.

## Final Verdict

**PROMOTE.** The v2 draft is ready to become the canonical product-requirements source.

**DO NOT IMPLEMENT.** Product implementation readiness remains **NOT_READY** because the canonical contracts, architecture decisions, security and privacy proofs, tri-OS bootstrap, gate manifest, CI/staging evidence, regenerated BMAD artifacts, complete Spec-Kit feature package, and BMAD implementation-readiness gate are still incomplete.
