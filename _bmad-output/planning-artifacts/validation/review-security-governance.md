# Heddle Pre-Implementation Security, Privacy, Identity, and Compliance Review

**Review date:** 2026-07-16  
**Review scope:** BMAD planning artifacts, Spec-Kit feature artifacts, constitution, master design, architecture-hardening ADRs, methodology, and design-completeness policy  
**Reviewer posture:** Pre-implementation design gate; evidence-based and read-only review  
**Decision:** **FAIL — implementation must not start**

> This review is a product and engineering control assessment, not legal advice or a certification opinion. Regulatory applicability and organizational compliance must be confirmed by qualified legal, privacy, security, and audit professionals.

## 1. Executive Summary

Heddle has a credible security direction: default-local operation, silo and team partitioning, deny-by-default authorization, a hard egress boundary, just-in-time secret resolution, event-sourced evidence, explicit human approvals, and a correct statement that ISO 27001 and SOC 2 certification are organizational matters. ADR 0002 materially improves the design by addressing network enforcement, event identity, irreversible effects, crypto-shredding, defense-in-depth secret detection, dual control, and loop safety.

The design is not implementation-ready, however. Most governance controls remain architectural intentions rather than complete, testable contracts. The planning set lacks a system threat model, data-protection threat model, identity trust model, authorization decision model, data inventory and lifecycle model, MCP trust and supply-chain model, compliance control matrix, abuse cases, negative authorization tests, and release quality gates. ABAC and ReBAC are absent. External identity providers are named but their assurance, provisioning, deprovisioning, group reconciliation, tenant binding, session, and break-glass semantics are undefined. Computer and MCP scopes are described in the master design but are not carried into PRD requirements, feature specs, stories, or acceptance suites.

The gate therefore fails until the Critical and High findings below are closed or explicitly accepted by the project owner with documented residual risk and a time-bounded remediation owner. Critical security invariants may not be deferred to a later enterprise track when Phase 1 already exposes remote backends, MCP tools, exact model I/O, and baseline team authorization.

## 2. Reviewed Evidence

- `.specify/memory/constitution.md`
- `_bmad-output/planning-artifacts/PRD.md`
- `_bmad-output/planning-artifacts/architecture.md`
- `_bmad-output/planning-artifacts/epics.md`
- `_bmad-output/implementation-artifacts/sprint-status.yaml`
- `specs/001-phase0-walking-skeleton/spec.md`, `plan.md`, and `tasks.md`
- `specs/002-workflow-engine/spec.md`
- `docs/superpowers/specs/2026-07-15-heddle-design.md`
- `docs/superpowers/adr/0002-design-hardening.md`
- `docs/superpowers/adr/0003-platform-composition-and-worker-strategy.md`
- `docs/DESIGN-COMPLETENESS-POLICY.md`
- `docs/METHODOLOGY.md`
- `docs/research/loop-engineering.md`
- `docs/research/agent-platform-landscape.md`

## 3. Control Coverage Summary

| Control objective | Current state | Gate assessment |
| --- | --- | --- |
| Default-local and no local egress | Constitution and ADR 0002 define a hard boundary and loopback allowlist | Direction accepted; enforcement contract and adversarial tests missing |
| Silo isolation | Strong invariant; namespace tests proposed | Insufficient: logical namespace is not a complete security boundary |
| Team-only remote sharing | Explicitly stated | Missing tenant-binding, membership lifecycle, object-level authorization, and negative tests |
| RBAC | Three scopes and default deny defined | Incomplete permission model; no formal decision semantics or matrix |
| ABAC | Not specified | Missing |
| ReBAC | Only informal ownership/team relationships | Missing |
| Local/LDAP/OIDC/Entra/Google identity | Providers and group mapping named | Trust, assurance, lifecycle, tenant, token, and reconciliation contracts missing |
| JIT secrets | Good reference-not-value direction; 1Password and OpenBao named | Provider lifecycle, lease/revocation, process exposure, and failure behavior missing |
| MCP authorization hierarchy | Enablement hierarchy described in master design | Not carried into canonical requirements or testable authorization contracts |
| Computer access scopes | Project/Folder/FullComputer described | Not traced to PRD/spec/tasks; path, symlink, capture, clipboard, and OS privilege controls missing |
| Audit, replay, revision | Ledger and separate audit log are strong concepts | Privacy, access, integrity, keying, replay, legal hold, and redaction details incomplete |
| GDPR enablement | Crypto-shredding decision is promising | Data model, lawful-purpose controls, rights workflow, backups, retention, DPIA, and transfer controls missing |
| EU AI Act enablement | Transparency, oversight, traceability named | Role classification, prohibited/high-risk use controls, literacy, monitoring, and deployer obligations missing |
| ISO 27001 enablement | Selected product controls named | No Annex A/control mapping or evidence ownership model |
| SOC 2 enablement | Selected trust-service controls named | No criteria mapping, control ownership, evidence cadence, or operating-effectiveness plan |
| NIS2 enablement | Logging, incident, supply-chain, governance named | Incident timelines, vulnerability handling, continuity, supplier assurance, and responsibility model missing |

## 4. Findings

### SG-001 — Critical — No complete threat model or trust-boundary model

The artifacts contain security principles and a risk list, but no structured threat model covering the local process, remote leader/follower boundary, browser, desktop controller, model gateway, MCP proxy and servers, workers, identity providers, secrets providers, databases, telemetry exporters, update channel, plugins, and model/tool content. There is no asset inventory, attacker model, data-flow diagram with trust boundaries, STRIDE-style analysis, privacy threat analysis, abuse-case catalog, or required mitigations linked to acceptance tests.

**Required closure evidence:**

- System and per-feature data-flow diagrams with process, network, identity, storage, administrative, and third-party trust boundaries.
- Threat and abuse-case register covering prompt injection, tool poisoning, malicious MCP servers, confused deputy, SSRF, path traversal, symlink and junction escape, command injection, credential theft, screenshot/clipboard leakage, replay, race conditions, TOCTOU, unauthorized remote attachment, cross-team enumeration, telemetry leakage, update compromise, dependency compromise, and denial of wallet/service.
- Privacy threat model covering purpose expansion, over-collection, re-identification, inference, excessive retention, unauthorized correlation, and model-provider disclosure.
- Every High/Critical threat mapped to a preventive or detective control, an owner, and a negative test or explicit residual-risk acceptance.

### SG-002 — Critical — Silo and remote team isolation are asserted but not specified as enforceable security domains

The constitution requires airtight silos and team-only remote sharing. The architecture currently reduces Phase 0 isolation to a namespaced SQLite access pattern, while remote replication, leadership handoff, reconciliation, and authorization are deferred. A namespace parameter is vulnerable to missing-filter defects and does not establish database, key, filesystem, blob, cache, vector-index, telemetry, backup, or process isolation. Remote team membership is stated as the boundary, but no server-side subject-to-team-to-object binding contract exists.

**Required closure evidence:**

- Canonical `SiloId` and `TeamId` types that cannot be caller-supplied without authorization context; server-derived scope on every request.
- Isolation model for relational data, blobs, indexes, caches, logs, traces, temporary files, secrets, encryption keys, backups, workers, and MCP sessions.
- Per-silo/team key hierarchy and deletion/rotation semantics.
- Remote tenant binding, membership grant/revoke propagation, stale-session handling, object-level authorization, anti-enumeration behavior, and cache invalidation.
- Cross-silo and cross-team negative test matrix, including concurrency, backup/restore, export, search/RAG, branch/replay, and telemetry paths.

### SG-003 — Critical — RBAC is incomplete and ABAC/ReBAC are absent

The requested authorization model is RBAC/ABAC/ReBAC. Current artifacts define composable roles and three scopes, but no attributes, relationships, policy evaluation contract, precedence, conflict handling, resource taxonomy, action taxonomy, decision evidence, or complete permission matrix. “Harness locks = intra-silo permissions” conflates configuration inheritance with authorization. Ownership is informal rather than a first-class relationship.

**Required closure evidence:**

- Versioned authorization model with `Subject`, `Action`, `Resource`, `Scope`, `Relationship`, `Attributes`, `Environment`, `Risk`, `Decision`, and `Obligations`.
- Complete permission matrix for global, silo, team, project, conversation, connector, tool, skill, workflow, model, secret reference, Ledger, audit, computer scope, export, retention, and administration operations.
- ABAC rules for data classification, device trust, network/mode, provider residency, action risk, time, purpose, and assurance level.
- ReBAC graph for owner/member/lead/contributor/viewer/service relationships and delegated administration.
- Explicit deny/allow precedence, monotonic security-floor semantics, lock semantics, dual control, break-glass expiry, and auditable policy decision IDs.
- Property-based and mutation-tested authorization suite proving default deny and non-bypass across every API, CLI, UI, worker, MCP, replay, and background job path.

### SG-004 — Critical — MCP and tool governance is not a complete authorization and trust contract

The master design says embedded connectors are disabled by default and enabled by a scope owner, but canonical PRD requirements only state connector availability and generic destructive confirmation. There is no distinction between installing, trusting, enabling, configuring, authenticating, listing, invoking, delegating, and mutating through an MCP server. Tool schemas and server-returned instructions are untrusted, yet signing, provenance, update, capability attenuation, per-tool authorization, result sanitization, rate limits, and revocation are not specified.

**Required closure evidence:**

- MCP trust registry schema: origin, version, digest/signature, license, maintainer, transport, requested capabilities, network destinations, data classes, and review status.
- Separate permissions for install/trust/enable/configure/read/invoke/mutate/delegate/update/remove.
- Capability grants scoped by silo/team/project/conversation and optionally resource instance, with expiry and revocation.
- Read and mutation tools separated; external side effects use intent/applied records, idempotency keys, approval obligations, and compensation where possible.
- Server identity and transport authentication for local and remote MCP; no implicit trust in stdio child processes.
- Prompt-injection and tool-poisoning boundary tests; tool descriptions/results are data and cannot alter system policy.
- Connector sandbox, destination allowlist, filesystem/process/network limits, quotas, timeout, output-size bounds, and audit requirements.

### SG-005 — High — Computer access scopes are not promoted into requirements and omit escape controls

The master design defines `Project`, `Folder`, and `FullComputer`, with Project as default. This important control is absent from the PRD functional requirements, architecture spine mapping, epics, and feature specs. Folder semantics do not define canonical paths, symlink/junction handling, mount boundaries, network shares, removable media, process inheritance, clipboard, window capture, multi-monitor capture, or child-process access. “FullComputer” is too coarse for least privilege.

**Required closure evidence:**

- PRD requirement and dedicated feature specification with user stories, misuse cases, and acceptance tests.
- Separate capabilities for filesystem read/write, process execution, screen capture, window capture, keyboard, pointer, clipboard, microphone, camera, accessibility APIs, browser profiles, credentials, and elevation.
- Canonical path and handle-based enforcement resistant to traversal, symlinks, junctions, race conditions, and alternate data streams.
- Explicit target window/application and screen-region grants where supported; visible capture/action indicator and emergency stop.
- Time-bounded session grants, reauthentication for widening, OS permission detection, audited approvals, and no silent privilege escalation.

### SG-006 — High — Identity-provider support is a list, not an identity assurance design

Local, LDAP/AD, generic OIDC, Entra ID, and Google Workspace are named, and group-to-role mapping is mentioned. Missing are local credential policy, passwordless/key recovery, MFA, OIDC discovery and issuer pinning, PKCE/state/nonce, token audience and tenant validation, JWKS rotation, LDAP TLS and bind strategy, nested/dynamic group semantics, SCIM or JIT provisioning, deprovisioning latency, account linking, duplicate identities, service identities, session revocation, device trust, and break-glass ownership.

**Required closure evidence:**

- Canonical principal and external-identity-link model with stable, non-email subject identifiers and tenant/issuer binding.
- Authenticator assurance levels and step-up requirements by action risk.
- Provider-specific security profiles and conformance tests.
- Group synchronization/reconciliation rules, precedence, maximum staleness, deletion/deactivation behavior, and protection against group-name collisions across issuers/tenants.
- Local bootstrap admin, recovery, rotation, lockout, MFA/passkey strategy, and audited break-glass process.
- Session/token lifetime, refresh, revocation, reauthentication, CSRF, replay, and stolen-device controls.

### SG-007 — High — JIT secret management lacks end-to-end exposure and lifecycle semantics

Reference-not-value, JIT resolution, zeroization, redaction, and provider neutrality are sound principles. 1Password and OpenBao are appropriately identified as options. The design does not specify lease duration, caching, renewal, revocation, rotation, child-process injection, environment-variable and command-line exposure, crash dumps, swap, clipboard, provider authentication, offline behavior, audit metadata, or how an MCP-based secret provider is prevented from returning plaintext into model-visible context.

**Required closure evidence:**

- Native `SecretProvider` contract for resolve/use/revoke/renew with opaque handles and a strict prohibition on serializing secret values.
- Prefer direct injection into the target transport/process over returning values to agents; forbid secrets in argv, prompts, tool output, telemetry, errors, and persistent environment configuration.
- 1Password CLI/service-account and OpenBao authentication/lease models, including bootstrap-secret handling and rotation.
- Provider-specific offline/fail-closed rules; no fallback from a governed provider to plaintext configuration.
- Memory lifetime, zeroization limitations, subprocess inheritance, dump/swap mitigations, and redaction test corpus.
- Secret access authorization by scope, purpose, tool identity, destination, and data classification, with audited reference metadata but never values.

### SG-008 — High — Ledger transparency, replay, reversibility, privacy, and audit are internally incomplete

ADR 0002 correctly distinguishes recorded-result replay from re-executing effects and introduces pure/reversible/irreversible classes. The PRD still claims every step is “replayable, reversible,” which is false for irreversible external actions. Exact model I/O capture can include personal data, special-category data, source code, credentials, third-party data, and privileged communications. The separate audit journal is named but its append authority, tamper evidence, clock, retention, export, access, legal hold, and correlation are undefined.

**Required closure evidence:**

- Replace universal reversibility claims with precise semantics: deterministic replay where possible, recorded-result replay by default, compensation for reversible effects, and no reversal claim for irreversible effects.
- Versioned event and audit schemas, schema migration policy, canonical serialization, clock/ordering model, signatures or MACs, key rotation, integrity verification, and independent checkpointing.
- Ledger data classification, field-level protection, privileged `ledger.read`, redacted views, purpose-limited export, retention tiers, legal holds, and access alerts.
- Replay authorization re-evaluated against current policy; old authorization must never authorize a new side effect.
- Branch/restore/backup behavior under erasure, revocation, changed membership, changed connector credentials, and changed policy.
- Explicit audit events for authentication, authorization decision, policy/config changes, approvals, secret-reference use, exports, erasure, break-glass, MCP lifecycle, and computer grants.

### SG-009 — High — GDPR enablement is incomplete and crypto-shredding assumptions are unproven

ADR 0002’s subject-key crypto-shredding is a useful direction, but data is not always attributable to exactly one subject. Shared conversations, source repositories, messages involving several people, derived embeddings, summaries, model caches, screenshots, backups, and audit records create multi-subject and derived-data problems. Destroying a key may conflict with legal holds or erase other subjects’ data. The artifacts lack a processing inventory, controller/processor role model, lawful-purpose records, data subject rights workflow, retention schedule, DPIA trigger, international transfer controls, and processor/subprocessor governance.

**Required closure evidence:**

- Data inventory and classification by artifact, field, subject category, purpose, lawful basis, controller/processor role, recipient, location, retention, and deletion method.
- Key-envelope model for multi-subject/shared artifacts, derived data, indexes, backups, replicas, branches, caches, and exports; tested global erasure propagation.
- Rights workflow for access, rectification, restriction, objection, portability, and erasure with identity verification, exceptions, legal hold, deadlines, and evidence.
- Purpose limitation and data minimization controls, including provider-specific disclosure records and opt-in for external inference.
- DPIA screening and required DPIA for likely high-risk processing such as pervasive computer observation, workplace monitoring, biometrics, or high-risk AI use.
- Records of processing, breach workflow, processor terms, subprocessor inventory, data transfer mechanism and transfer impact assessment where applicable.

### SG-010 — High — EU AI Act enablement is too generic for an omni-purpose agent platform

Transparency, human oversight, model routing documentation, and risk classification are named, but Heddle can be configured for employment, education, access to services, biometrics, surveillance, and other regulated contexts. The design has no prohibited-use policy, deployment/use-case classification workflow, provider/deployer role assessment, high-risk feature gate, fundamental-rights impact support, AI literacy support, post-market monitoring, incident handling, accuracy/robustness/cybersecurity evidence, or generated-content marking policy by modality.

**Required closure evidence:**

- Use-case registration and risk-classification workflow before enabling regulated workflows or connectors.
- Prohibited-use policy enforced by product policy where technically possible and supported by governance documentation elsewhere.
- Deployment role and obligation matrix for provider, deployer, importer, distributor, and GPAI dependencies where applicable.
- Human-oversight design specifying competence, authority, information, intervention, stop, override, and automation-bias mitigation.
- Logging, instructions for use, model/capability documentation, evaluation, incident, monitoring, and retention requirements by risk class.
- Content provenance/labeling strategy for generated or manipulated text, image, audio, and video, aligned to applicable implementation timelines and standards.

### SG-011 — High — ISO 27001, SOC 2, and NIS2 are named without an auditable control framework

The documents correctly say certification is organizational, but listing RBAC, audit, encryption, and supply-chain scanning is not enough to establish readiness. There is no control catalog mapping, accountable owner, evidence source, operating frequency, exception process, risk-treatment linkage, incident response plan, continuity objectives, vulnerability disclosure/remediation policy, supplier assurance process, secure update policy, or evidence retention.

**Required closure evidence:**

- Product-control matrix mapped to relevant ISO/IEC 27001:2022 Annex A controls, SOC 2 Trust Services Criteria, and NIS2 risk-management/incident obligations, clearly distinguishing product controls from organizational controls.
- Control owner, implementation status, test method, evidence source, cadence, retention, exception, and residual risk for every mapped control.
- Secure SDLC, vulnerability management and disclosure, patch SLAs, signed release/update chain, SBOM/VEX, dependency provenance, and emergency revocation.
- Incident response and reporting runbooks, severity model, forensics preservation, customer notification support, and regulatory escalation inputs.
- Backup/restore, recovery objectives, continuity tests, capacity, availability, supplier risk, and privileged-access reviews.

### SG-012 — High — Security quality gates and checklists do not exist at the required level

The design names linting, tests, SAST, dependency scans, secret scans, SBOM, signing, and cross-platform CI. No mandatory gate specification defines pass/fail thresholds, blocking severity, exception approval, evidence format, artifact provenance, security test suites, privacy sign-off, threat-model review, authorization coverage, or release readiness. The current Spec-Kit feature 002 has only a draft `spec.md`; feature 001 is blocked and requires regeneration. There are no feature checklists or completed BMAD readiness evidence in the reviewed artifact set.

**Required closure evidence:**

- Pre-implementation gate checklist requiring approved threat model, data model, authorization matrix, misuse cases, privacy review, ADR acceptance, traceability, and no unresolved Critical/High contradictions.
- Per-PR gates: formatting, lint, tests, coverage thresholds, authorization negative tests, isolation tests, SAST, SCA, secrets, license, SBOM, reproducible build metadata, and policy-as-code tests.
- Pre-release gates: penetration test scope, fuzzing for parsers/protocols, sandbox escape testing, update/signing verification, backup/restore, erasure, incident exercise, performance/DoS budgets, tri-OS tests, and evidence bundle.
- Formal exception process with owner, rationale, compensating control, expiry, and approval; Critical exceptions require project-owner and security approval.
- BMAD implementation-readiness report and Spec-Kit clarify/plan/tasks/checklist/analyze artifacts for the current slice before code implementation.

### SG-013 — Medium — Default-local claims need precise mode-transition and local-service rules

The design differentiates Local, Online-Server, and Online-Remote and says switching is proposed rather than imposed. It does not fully define automatic detection inputs, spoofing resistance, consent, active-session behavior, silo selection, unsaved work, model/provider changes, connector shutdown, local backend standby, or rollback when a remote attachment fails. “Loopback allowlist” also needs a rule for local services that proxy externally.

**Required closure evidence:**

- Deterministic mode state machine with user-visible transition plan, confirmation, rollback, and no automatic data migration.
- Separate credentials, process state, stores, keys, indexes, caches, and histories for each mode and remote team.
- Local-service classification that follows effective egress, not destination address; a loopback proxy with cloud access is network-capable.
- Tests for mode flapping, spoofed discovery, partial attachment, revoked membership, offline fallback, and active workflows.

### SG-014 — Medium — Observability may become an uncontrolled secondary disclosure channel

OpenTelemetry is appropriate, but prompts, model responses, tool outputs, file paths, user identifiers, connector metadata, screenshots, and secrets can leak through span names, attributes, logs, baggage, or exporter queues. Silo-scoped audit conflicts with centralized monitoring unless security trimming and tenant attribution are explicit.

**Required closure evidence:**

- Telemetry data classification and allowlist schema; content capture off by default.
- Silo/team attribution, access control, local buffering, retention, deletion, sampling, redaction, and exporter egress policy.
- Baggage propagation rules preventing sensitive data and cross-tenant contamination.
- Tests proving Local mode cannot export telemetry and remote users cannot query another team’s telemetry.

### SG-015 — Medium — Compliance and capability wording contains overclaims

The PRD vision says Heddle provides “enterprise compliance,” “full transparency and reversibility,” connects to “all AI providers,” and “masters” BMAD/Spec-Kit/powerskills. Those are outcome or universal claims not supported by the current design or evidence. Later text correctly states that the software only provides controls and that certification is organizational, but the opening claim remains misleading.

**Required wording changes:**

- Replace “enterprise compliance” with “controls intended to support organizational compliance programs.”
- Replace “all AI providers” with “providers supported by tested adapters and declared capability contracts.”
- Replace universal reversibility with the effect-class semantics required by SG-008.
- Replace “masters” frameworks with “provides versioned, tested integrations” until conformance suites demonstrate official workflow fidelity.
- Never claim “GDPR compliant,” “EU AI Act compliant,” “NIS2 compliant,” “ISO 27001 certified,” or “SOC 2 compliant/certified” for the software alone. ISO 27001 certification applies to an organization’s ISMS scope; SOC 2 is an attestation on service-organization controls over a defined period.

## 5. Required Threat-Model Scope

The pre-implementation threat model must, at minimum, cover these security domains:

1. **Local host:** untrusted repositories/documents, malicious local users, filesystem races, child processes, shell, clipboard, screen, accessibility privileges, browser profiles, local IPC, model servers, and loopback proxies.
2. **Remote/team:** leader discovery, attachment, TLS, identity federation, tenant binding, authorization, session revocation, team isolation, replication, offline work, reconciliation, backup, and administrative delegation.
3. **Agent/LLM:** prompt injection, indirect injection, data exfiltration, excessive agency, goal hijacking, hallucinated authorization, model substitution, context poisoning, memory poisoning, and cost/availability attacks.
4. **MCP/tools/workers:** malicious server or worker, schema poisoning, capability escalation, unauthorized delegation, confused deputy, SSRF, result injection, oversized output, crashes, side effects, retries, and update compromise.
5. **Secrets/identity:** bootstrap, token theft, issuer/tenant confusion, stale groups, account linking, service identities, secret lease leakage, subprocess exposure, revocation, recovery, and break-glass.
6. **Data/Ledger/RAG:** cross-silo retrieval, derived data, embeddings, exact model I/O, redaction bypass, tampering, replay, erasure, legal hold, archival, export, and provenance.
7. **Distribution/supply chain:** dependency provenance, licenses, build runners, signing keys, SBOM/VEX, installers, auto-update, rollback, plugin registry, model files, and compromised upstreams.
8. **Operations/compliance:** telemetry disclosure, incident detection, forensics, retention, deletion, data transfers, processor dependencies, continuity, recovery, and evidence integrity.

## 6. Minimum Pre-Implementation Quality Gate

Implementation may begin only when all conditions below are evidenced:

- [ ] PRD wording is corrected to avoid compliance, universality, and reversibility overclaims.
- [ ] ADR 0003 is Accepted or replaced; all five required spikes have evidence bundles.
- [ ] Architecture status is Approved and includes security, privacy, identity, key-management, MCP, and computer-control views.
- [ ] System threat model and privacy threat model are approved with no unowned Critical/High risks.
- [ ] Canonical data inventory, classification, lifecycle, retention, residency, backup, erasure, and legal-hold model exists.
- [ ] RBAC/ABAC/ReBAC policy model and complete permission matrix are approved.
- [ ] Identity trust and lifecycle model covers local, LDAP, OIDC, Entra ID, and Google Workspace.
- [ ] MCP/tool/worker trust, authorization, sandbox, supply-chain, and update contracts are approved.
- [ ] Secret-provider and key-management contracts cover 1Password and an OSS/self-hosted alternative, with JIT use and no model-context exposure.
- [ ] Silo/team isolation model covers every storage, index, cache, telemetry, worker, backup, export, and replay path.
- [ ] Computer-access capability model and escape-resistant scope enforcement are specified.
- [ ] Ledger/audit schema, privacy controls, replay semantics, cryptographic integrity, key rotation, and erasure behavior are specified.
- [ ] GDPR, EU AI Act, ISO 27001, SOC 2, and NIS2 product-control mappings distinguish organizational obligations and name evidence owners.
- [ ] Current-slice BMAD readiness and Spec-Kit clarify/plan/tasks/checklist/analyze gates pass with no unresolved Critical/High contradiction.
- [ ] CI/CD and release gate policy defines blocking thresholds, evidence, exceptions, signing, SBOM, vulnerability management, tri-OS verification, and security test suites.

## 7. Recommended Closure Order

1. Freeze product implementation and correct the PRD overclaims.
2. Produce the system/data-flow and privacy threat models.
3. Define the canonical resource/action/relationship/attribute authorization model.
4. Define silo/team isolation and encryption/key domains across all persistence and observability paths.
5. Define identity federation, local bootstrap/recovery, sessions, group reconciliation, and break-glass.
6. Define MCP/tool/worker trust and computer-access capability contracts.
7. Complete the Ledger/audit/privacy/erasure lifecycle design.
8. Build the compliance control matrix and CI/CD quality-gate policy.
9. Regenerate the current feature through BMAD readiness and all Spec-Kit gates.
10. Run an independent adversarial review; implementation can start only if no Critical or High finding remains unowned or unaccepted.

## 8. Final Gate Decision

**FAIL / BLOCKED.** Heddle’s security principles are promising but are not yet expressed as sufficiently complete, traceable, testable, and auditable contracts for an agent platform that can control a computer, invoke MCP tools, connect to enterprise systems, expose a team backend, and retain exact model/tool content. No product code should be implemented from the current artifacts. The next permissible work is planning, threat modeling, formal control definition, clarification, quality-gate construction, and evidence-producing architecture spikes.
