# Design Completeness Policy — the guiding thread ("fil conducteur")

**Date**: 2026-07-15 · **Status**: adopted · Answers: "must the design be 100% before implementation, or should we leave margin?"

## Verdict

**No — 100% up-front design is an anti-pattern (waterfall).** But *some* things must be right before the first line of code. We split every design decision into three buckets by **reversibility cost**, and we only require completeness where it is expensive-to-change-later.

This is **loop discipline applied to the design phase** (Constitution VIII): the design loop must have an explicit exit — we stop elaborating when the *next slice* is implementation-ready and the *one-way doors* are resolved, then let early implementation feed ground truth back into the still-open decisions.

## The three buckets

### A. Must be 100% before ANY implementation (one-way doors)
Expensive or impossible to change once code and data exist. Get these right first.
- **Constitution** (principles, incl. VIII loop discipline) and **architecture invariants** (ADs).
- **Event-sourcing schema** (Step identity, correlation, effect classification, loop event types) — the Ledger is the project's principal one-way door; migrating persisted history later is costly.
- **Silo isolation model** and the **config resolution + security-floor rule** (value-vs-lock semantics) — stamped into every table and every authz check.
- **Loop ownership** (Heddle owns the reason→act→observe loop; Goose is a turn/tool executor) — determines whether LoopController, per-step Ledger, and resume are even possible.
- **Egress enforcement boundary** (Local = no egress as a network boundary, not self-declaration).
- **GDPR erasure mechanism** (crypto-shredding key model) — the encryption/keying must exist from the first stored record.

### B. Must be 100% for the CURRENT slice only (the next feature to build)
Complete just-in-time, per feature, via the Spec-Kit gates.
- The feature's `spec.md` → `clarify` → `plan.md` → `tasks.md` → `checklist` → `analyze`, passing the **Constitution Check** gate.
- For Phase 0 (Epic 1): the ADR 0001 Goose spike is the gate; the feature is ready once its gates pass.

### C. Deliberately deferred — elaborate at a defined trigger (leave margin)
Cheap to change; specifying now would be gold-plating and likely wrong before we learn from implementation. Each carries an explicit **unfreeze trigger**.

| Deferred item | Unfreeze trigger |
|---|---|
| Detailed `plan.md`/`tasks.md` for Epics 2–7 (workflow 002, loop-controls, v1 axes) | when its predecessor epic is implemented and its spike (if any) is done |
| Multimodal v2→v8 detailed specs | when v1 is shipped; each version specced just-in-time |
| Loop budgets & no-progress **default values/heuristics** for a dev agent | after Phase 0/Epic 2 gives real iteration data (calibrate on ground truth, not guess) |
| Networked leader/follower election/quorum + ledger replication | when Server/Remote mode is scheduled (not needed for Local-first Phase 0) |
| External IdP (LDAP/OIDC/Entra/Google) + advanced RBAC | enterprise track, when team adoption requires it |
| `contracts/`, formal per-story BMAD files | when we switch to `bmad-dev-story` execution; CLI surface is the contract until then |
| Additional secret backends (SOPS/1Password/OpenBao/Infisical) | when the first real cloud secret appears (post-Phase-0) |

## Rule of thumb (Bezos doors)
- **One-way door** (irreversible/expensive) → resolve to 100% now (bucket A).
- **Two-way door** (reversible/cheap) → decide the *interface*, defer the *implementation detail*, unfreeze at trigger (bucket C).
- When unsure which, treat it as one-way (bias to getting the schema/boundary right).

## Consequence for "is conception done?"
Conception is **"done enough to start"** when: bucket A is resolved (see ADR 0002 for the hardening decisions), and the *current slice* (Phase 0) has passed its bucket-B gates. It is **never** "100% done" for the whole product before implementation — by design.
