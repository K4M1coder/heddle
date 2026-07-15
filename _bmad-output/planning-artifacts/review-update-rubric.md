# PRD Quality Review — Skein Update Draft

## Overall verdict

**Verdict: CONDITIONAL PASS FOR PRD REMEDIATION; NOT READY FOR IMPLEMENTATION.** The update draft is materially stronger than the canonical PRD: it establishes an explicit build gate, bounded release hypotheses, a coherent de-scope order, named risk journeys, stable requirements with positive/negative outcomes and evidence, capability-based claims, and an honest separation between product requirements and implementation candidates. It is suitable to continue the BMAD PRD update cycle, but it is not yet suitable for final promotion or implementation authorization because quantitative release oracles, compliance traceability, assumption round-tripping, and cross-artifact traceability remain incomplete, and every one-way-door contract is deliberately still blocked.

No new critical defect was found in the two drafts. Five current high-severity defects remain after remediation; two are newly exposed or materially reframed, while three continue prior traceability and acceptance-quality findings. Eighteen previous critical/high findings are only partially fixed or remain open. The explicit `NOT_READY` state correctly prevents those open items from being mistaken for build approval.

## Summary counts

### Current findings

| Severity | Count |
| --- | ---: |
| Critical | 0 |
| High | 5 |
| Medium | 5 |
| Low | 1 |

### Previous critical/high disposition

| Status | Count |
| --- | ---: |
| Fixed | 10 |
| Partial | 16 |
| Open | 2 |
| Total reviewed | 28 |

The disposition table covers the eighteen critical/high findings in the prior official rubric and the ten additional high-level critical/high findings synthesized in the prior validation report. Overlapping findings are retained as separate prior findings because the request is to determine the status of each previous issue.

## Dimension grades

| Dimension | Prior | Current |
| --- | --- | --- |
| Decision-readiness | broken | adequate |
| Substance over theater | adequate | strong |
| Strategic coherence | thin | adequate |
| Done-ness clarity | broken | adequate |
| Scope honesty | thin | adequate |
| Downstream usability | thin | adequate |
| Shape fit | adequate | strong |

## Decision-readiness — adequate

The draft now makes the operative decision unambiguous: product implementation is blocked (§1, §11.1). It distinguishes permitted design/evidence work from product implementation, names twelve blocking decisions with accountable owners and closure evidence (§11.2), states ten READY conditions (§11.3), and provides a defensible release/de-scope sequence (§5). This fixes the previous absence of a green-light gate and the hidden additive-scope problem.

The dimension is not strong because the artifact is intentionally a remediation draft and the decisions most likely to create irreversible architecture remain unresolved. The addendum correctly lists required contracts and spikes, but listing closure work is not closure. The PRD is therefore decision-ready for the decision **not to implement**, not decision-ready to authorize a build.

### Findings

- **high** READY criteria rely on future artifacts without a single normative closure register (§11.2–§11.3; addendum §§4–5) — the gate identifies evidence classes, but it does not define stable artifact IDs, expected paths, approving roles, review independence, expiry/staleness rules, or how the gate is mechanically evaluated. *Fix:* create a versioned gate manifest mapping every `BD-*` item to required artifact IDs, validation commands, approvers, freshness rules, and machine-readable status; reference that contract from §11.
- **medium** Decision deadlines are gates rather than schedules (§11.2) — owners and evidence are present, but no target checkpoint or escalation rule exists for decisions that remain blocked indefinitely. *Fix:* add a decision cadence or milestone, escalation owner, and abandon/narrow trigger without inventing calendar promises that have not been agreed.

## Substance over theater — strong

The universal claims identified previously have been replaced with a release-specific capability registry (§2, FR-004, FR-012, FR-017, FR-018, FR-022). “Omni” is explicitly an attributable orchestration experience rather than a universal-model claim (§2, Glossary). Compliance language is constrained to support and evidence and explicitly rejects automatic certification (§3, §9). Loop, context, policy, evidence, recovery, secrets, isolation, and connector requirements are product-specific and tied to observable evidence.

The addendum appropriately carries mechanisms, candidate components, rejected postures, spikes, and stack hypotheses rather than allowing them to masquerade as product requirements. The six personas exceed the rubric's usual warning threshold, but each drives a distinct risk boundary; Jordan is explicitly post-V1, so this is not persona furniture.

### Findings

- **medium** Several NFRs describe a future declaration rather than a current product contract (NFR-004, NFR-006, NFR-008) — publishing thresholds or compatibility behavior later is honest, but it leaves these sections less substantive than the FRs. *Fix:* define the minimum mandatory fields and pass/fail semantics of each release acceptance declaration now, even where numeric values remain spike outputs.

## Strategic coherence — adequate

The thesis is now explicit and consistent: Skein is a governed local-first control plane that owns workflow state, policy, context, evidence, and completion while composing replaceable capabilities (§2–§3). Phase 0, Local Alpha, Team Alpha, V1, and post-V1 capabilities each have a hypothesis and clear boundary (§5). The de-scope order protects the thesis rather than maximizing feature count (§5.6). Core metrics now measure bounded loops, ground truth, policy consistency, evidence, context, isolation, recovery, user control, local value, and team lifecycle (§8), with six counter-metrics guarding against harmful optimization.

The remaining weakness is that V1's adoption hypothesis is not paired with an adoption or comparative user-value oracle. Most metrics establish safety and conformance, not whether Skein is meaningfully simpler or more useful than operating the underlying tools separately.

### Findings

- **high** The V1 adoption thesis has no user-value success metric (§5.4, §8) — `SM-009` proves two journeys can complete locally, but no metric tests setup effort, time-to-first-value, task success relative to a baseline, sustained use, or whether the unified control plane reduces operator burden. *Fix:* add stakes-appropriate user-value metrics and counter-metrics for Local Alpha and V1, with a stated comparison baseline and measurement method.
- **medium** `SM-005` delegates its threshold to the same spike it is intended to judge (§8, addendum SP-003) — without a predeclared non-inferiority margin or a documented baseline-freeze procedure, the acceptance threshold can be selected after observing results. *Fix:* define the protocol for setting and freezing the threshold before final benchmark evaluation, including representative task classes and confidence requirements.

## Done-ness clarity — adequate

All twenty-two FRs now include a positive outcome, negative outcome, and required evidence. Security-sensitive paths cover denial, stale identity, forged scope, ambiguous effects, secret leakage, egress, unsafe resume, and reduced-assurance workers. The requirements are substantially more usable as sources for independent test oracles than the previous capability-only statements.

The PRD deliberately leaves exact schemas, examples, fixtures, thresholds, and contract suites to downstream artifacts. That is proper separation of product and architecture, but the release-level NFRs and some success metrics still permit the acceptance threshold to be authored after implementation evidence exists. This prevents a strong grade.

### Findings

- **high** Several release-critical NFRs have no bounded minimum or independently fixed oracle (NFR-004–NFR-008; SM-004–SM-005) — “declares and tests,” “appropriate,” “target,” and “at least 99.9%” do not define sampling, exclusions, measurement windows, supported hardware, accessibility exceptions, or evidence-loss treatment. *Fix:* add a release-acceptance contract requiring pre-registered datasets, environments, denominators, exclusions, confidence/error treatment, and waiver authority before implementation tasks are generated.
- **medium** FR evidence labels are not yet acceptance contracts (§6) — phrases such as “conformance report,” “negative suite,” and “capability declaration” name outputs but not their mandatory content, independence, or pass threshold. *Fix:* assign each evidence class a stable contract ID in the downstream contract package and prohibit task authorization until those contracts exist.

## Scope honesty — adequate

The update fixes the platform-complete MVP problem by splitting Phase 0, Local Alpha, Team Alpha, V1, and long-term capabilities (§5). It states release-specific exclusions, declares which guarantees may never be traded for schedule, and treats platform, provider, connector, and modality breadth as removable before safety boundaries. The addendum clearly labels the mixed-language stack and reused components as hypotheses rather than accepted architecture.

Assumptions now have stable IDs, owners, validation triggers, and failure responses (§10). However, the section is called a round-trip index while each assumption appears only once in that section. Product statements that depend on the assumptions do not cite their IDs, so the claimed bidirectional traceability is not actually present.

### Findings

- **high** The Assumptions “Round-Trip Index” still does not round-trip (§10 versus §§2–6) — each `A-*` record exists once, but the claims it qualifies do not carry an inline `A-*` reference, and there is no separate index from assumption to dependent requirement/release hypothesis. *Fix:* cite assumption IDs at every dependent normative claim and add a generated dependency index; alternatively rename the section “Assumption Register” and stop claiming round-trip behavior until downstream links exist.
- **medium** Local Alpha platform scope remains internally tense (§5.2, §5.6, FR-005, NFR-004) — FR-005 requires one path on each declared target platform while operating-system variants are first in the de-scope order, but the initial declared target set is absent. *Fix:* name the Phase 0 candidate matrix and state that Local Alpha authorization selects a tested subset before FR-005 becomes applicable.

## Downstream usability — adequate

The draft now supplies contiguous `FR-001`–`FR-022`, named personas `P-01`–`P-06`, named journeys `UJ-01`–`UJ-10`, `NFR-*`, `SM-*`, `CM-*`, `CS-*`, `A-*`, and `BD-*` identifiers. The glossary is self-contained and normalizes Local, Server, Remote, Silo, Worker, MCP, Loop Engineering, Context Manifest, and Power-skill profile. Risk journeys now cover first run, recovery, harness governance, remote isolation, connector approval, computer scope, identity lifecycle, investigation, erasure, and desktop safety.

The remaining traceability gap is external: this draft intentionally has not updated architecture, epics, stories, or Spec-Kit artifacts. Section 11 correctly blocks implementation until regeneration, but the draft cannot yet be promoted as a chain-top artifact without a reconciled requirement graph.

### Findings

- **high** Cross-artifact traceability remains open (§11.3 items 4–6) — stable IDs now exist, but architecture, epics/stories, and Spec-Kit artifacts still point to the old canonical PRD and stale requirement inventory. *Fix:* after PRD promotion, regenerate mappings and require an automated zero-orphan/zero-stale-ID report before final BMAD implementation readiness.

## Shape fit — strong

The shape now fits a high-stakes, multi-stakeholder, chain-top platform PRD. Product vision, risk journeys, release hypotheses, FRs, NFRs, measurable outcomes, compliance-support constraints, assumptions, build authorization, and glossary are all load-bearing. Technical choices, component candidates, spike procedures, development substrate, and swarm mechanics are moved to the addendum, preserving useful depth without contaminating normative product requirements.

The post-V1 FRs are clearly labeled gated capabilities and preserve the long-term user vision without implying current release scope. The document is long, but the length is justified by the security, privacy, cross-platform, enterprise, and autonomous-development stakes.

### Findings

- **low** The PRD contains six personas despite the rubric's normal four-persona warning (§4) — all six currently drive decisions, but Jordan is post-V1 and could distract Local Alpha planning. *Fix:* preserve Jordan in the addendum or clearly filter persona/journey extraction by release when generating near-term stories.

## Previous critical/high finding disposition

### Prior official rubric findings

| ID | Previous finding | Status | Evidence and rationale |
| --- | --- | --- | --- |
| R-C1 | Runtime ownership asserted but executable runtime choice unapproved | Partial | §§3, 6 FR-017, 10 A-002/A-008 and 11 BD-004/BD-011 define ownership and block implementation; addendum SP-001 defines evidence. The runtime candidate decision and conformance contract are still not accepted. |
| R-C2 | No explicit product green-light gate | Fixed | §11 provides explicit `NOT READY`, owned blocking decisions, READY criteria, and explicit owner authorization. |
| R-H1 | MVP trade-offs hidden behind additive scope | Fixed | §5 separates Phase 0, Local Alpha, Team Alpha, V1, post-V1, exclusions, and a de-scope order. |
| R-H2 | Open questions lack owners and decision criteria | Fixed | §11.2 assigns accountable owners and closure evidence to `BD-001`–`BD-012`; addendum §§5–8 supplies bounded decision questions and spikes. |
| R-H3 | Universal capability claims unbounded | Fixed | §2 and FR-004/012/017/018/022 require release-specific capability declarations and conformance evidence. |
| R-H4 | Compliance controls are not product contracts | Partial | §9 adds stable `CS-*` constraints and FR/NFR evidence behaviors, but no control-to-FR/test/evidence/owner mapping exists yet. |
| R-C3 | Success metrics do not validate the control-plane thesis | Fixed | §8 adds ten thesis-aligned outcome metrics and six counter-metrics. A separate V1 user-value metric is still newly required. |
| R-H5 | MVP metric over-coupled core proof to surfaces/connectors | Fixed | §5 and §8 separate Local Alpha core proof, Team Alpha, and V1 expansion. |
| R-H6 | Feature priority lacks a minimum-value argument | Fixed | §5 states release hypotheses, boundaries, exclusions, and de-scope order. |
| R-C4 | Most FRs lack acceptance oracles | Fixed | Every FR in §6 has positive, negative, and evidence outcomes. Evidence contracts still need downstream formalization. |
| R-H7 | Security-critical behavior underspecified | Partial | FR-002/003/006–011/014/015/019/020 and NFR-001–003 now define meaningful negative behavior; exact threat models, schemas, tests, and thresholds remain blocked by §11. |
| R-H8 | Context-quality contract not measurable | Partial | FR-016, SM-005, A-005, BD-008 and SP-003 define dimensions and evidence; the non-inferiority threshold and freeze procedure remain unset. |
| R-H9 | Worker eligibility circular | Partial | FR-017 and addendum `WorkerAdapter` identify mandatory observable events and reduced-assurance behavior; the versioned contract and conformance fixtures do not yet exist. |
| R-H10 | MVP is platform-complete rather than minimal | Fixed | §5 establishes a strict-local Local Alpha and defers UI, team, enterprise, computer control, and multimodal breadth. |
| R-H11 | Assumption tracking mechanically broken | Partial | Stable assumptions, owners, triggers, and failure responses now exist, but dependent claims do not cite them and the “round-trip” claim is inaccurate. |
| R-H12 | Cross-artifact requirement coverage stale | Open | The update draft has stable IDs and a blocking gate, but architecture, epics/stories, and Spec-Kit artifacts have not yet been regenerated. |
| R-H13 | Highest-risk user journeys missing | Fixed | UJ-01–UJ-10 cover the previously missing operational and security boundaries. |
| R-H14 | Regulatory/security constraint traceability absent | Partial | `CS-001`–`CS-006` provide stable constraints and §11 requires evidence, but the actual constraint-to-FR-risk-test-evidence-owner map is absent. |

### Additional critical/high findings from the prior consolidated validation

| ID | Previous finding | Status | Evidence and rationale |
| --- | --- | --- | --- |
| V-C1 | Canonical Ledger/event/effect/privacy contract absent | Open | FR-014/015, BD-001 and addendum contracts/spike define required closure, but no normative schema, fold, compatibility policy, or fixture exists. Correctly blocks implementation. |
| V-C2 | Local/Server/Remote semantics contradict isolation | Partial | FR-006/007, UJ-04, A-007, BD-002 and addendum §6 resolve the product behavior and safest Team Alpha hypothesis; the normative state machine and security-domain contract remain absent. |
| V-C3 | Hard offline egress lacks tri-OS proof | Partial | FR-003/005/020, A-003, BD-003 and SP-005 define failure behavior and evidence; enforcement is not yet selected or proven. |
| V-C4 | WorkerAdapter and durable workflow contracts undefined | Partial | FR-013/017 and BD-004/005 plus addendum contracts/SP-001/002 define required semantics; schemas, folds, examples, and crash fixtures remain uncreated. |
| V-C5 | Exact history, replay, and GDPR erasure unreconciled | Partial | FR-014/015, UJ-09, A-006, BD-001/009 and SP-007 explicitly narrow the promise and define evidence; the data model and erasure fixtures remain absent. |
| V-C6 | Threat, silo, authorization, and MCP governance incomplete | Partial | FR-007–011, BD-006/007 and addendum §§4/7 define the required model; formal threat/data-flow models, matrices, and conformance tests remain absent. |
| V-H1 | Single versioned headless protocol missing | Partial | FR-001 and addendum `ApplicationProtocol` define product semantics; the versioned schema, examples, and tests do not exist. |
| V-H2 | Identity, JIT secrets, audit, and privacy lifecycle incomplete | Partial | FR-009/014/019, UJ-07–09 and BD-009 cover required behavior; provider profiles, lifecycle contracts, and fixtures remain absent. |
| V-H3 | Compliance-support mappings missing | Partial | §9 and addendum §8 improve terminology and constraints, but mappings and accountable operating-control ownership are still future work. |
| V-H4 | Clean bootstrap, CI/CD, provenance, and offline packaging unproven | Partial | NFR-004–006, BD-010 and addendum SP-006/§9 define the gate; no clean-machine evidence or instantiated CI/CD exists. |

## Mechanical notes

- **ID continuity:** `FR-001`–`FR-022`, `NFR-001`–`NFR-008`, `SM-001`–`SM-010`, `CM-001`–`CM-006`, `CS-001`–`CS-006`, `A-001`–`A-010`, and `BD-001`–`BD-012` are unique and contiguous within their namespaces.
- **Cross-references:** Internal section references resolve. External architecture, epic/story, and Spec-Kit references remain intentionally stale until the draft is promoted and those artifacts are regenerated.
- **Assumptions:** IDs are complete and owned, but the claimed round trip fails because dependent normative statements do not cite `A-*` IDs.
- **Glossary:** The glossary is self-contained and materially resolves prior drift. `Power-skill profile` is now a defined product term. Local/Server/Remote naming is consistent in the draft.
- **UJ protagonists:** Every journey has a named protagonist and carries decision-driving context inline.
- **Document roles:** Product behavior is predominantly in the PRD; mechanisms and candidate technologies are predominantly in the addendum. No product implementation authorization is implied.

## Gate recommendation

Promote neither draft yet. Resolve the five current high findings, rerun this rubric, reconcile the accepted text into the canonical PRD and memlog through the official BMAD Update workflow, then regenerate and validate downstream architecture and Spec-Kit artifacts. Bounded evidence spikes may proceed only under the constraints already stated in §1 and §11; product implementation and implementation-agent swarms remain blocked.
