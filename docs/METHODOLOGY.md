# Méthodologie Skein — Bridge BMAD × Spec-Kit (dogfooding)

Skein est **conçu avec les méthodes qu'il intègre** (dogfooding). On applique le **bridge** BMAD × Spec-Kit : **BMAD planifie**, **Spec-Kit exécute**. Les deux frameworks sont réellement installés dans ce dépôt.

## Le flux (bridge)

```mermaid
graph LR
  subgraph BMAD["BMAD — Planification (agentic)"]
    PRD[PRD.md] --> ARCH[architecture.md] --> EP[epics.md] --> SS[sprint-status.yaml]
  end
  subgraph SK["Spec-Kit — Exécution (gated)"]
    CONST[constitution.md] --> SPEC[spec.md] --> PLAN[plan.md] --> TASKS[tasks.md] --> IMPL[/implement/]
  end
  EP -->|une epic = une feature| SPEC
  CONST -.gate.-> PLAN
```

- **BMAD** produit des artefacts de planification haute-fidélité (analyste/PM/architecte) : `_bmad-output/planning-artifacts/{PRD.md, architecture.md, epics.md}` + `_bmad-output/implementation-artifacts/sprint-status.yaml`.
- **Spec-Kit** exécute chaque epic comme une *feature* gated : `.specify/memory/constitution.md` (principes immuables, gate) → `specs/[###-feature]/{spec.md, plan.md, tasks.md}` → `/speckit-implement`.
- Règle BMAD respectée : **contexte propre par phase**, mémoire dans les fichiers (pas le chat).

## Outillage installé (réel)

| Framework | Installé via | Emplacement |
|---|---|---|
| Spec-Kit | `uvx … specify init --here --integration claude` | `.specify/` + skills `.claude/skills/speckit-*` |
| BMAD-METHOD v6.10 | `npx bmad-method@latest install --modules bmm --tools claude-code` | `_bmad/`, `_bmad-output/` + skills `.claude/skills/bmad-*` |

Commandes disponibles (dans un agent) : Spec-Kit `/speckit-constitution|specify|plan|tasks|implement` ; BMAD `bmad-create-prd`, `bmad-create-architecture`, `bmad-create-epics-and-stories`, agents `bmad-agent-{analyst,pm,architect,dev}`, etc.

## Correspondance des artefacts

| Contenu | BMAD (planning) | Spec-Kit (exécution) | Référence exhaustive (superpowers) |
|---|---|---|---|
| Principes immuables | (dans PRD/arch) | `.specify/memory/constitution.md` | design §1-§7 |
| Quoi / pourquoi | `PRD.md` | `specs/001-*/spec.md` | design §1, §8 |
| Comment (archi) | `architecture.md` | `specs/001-*/plan.md` | design §3-§7 |
| Découpage | `epics.md` + `sprint-status.yaml` | `specs/001-*/tasks.md` | plan Phase 0 (bite-sized) |

**Note de conformité honnête :** les documents `docs/superpowers/specs|plans/` (issus du flux brainstorming→writing-plans) restent la **référence exhaustive** (code TDD complet par étape). Les artefacts BMAD/Spec-Kit ci-dessus sont la **vue conforme** aux deux frameworks, dérivée de cette référence. En cas de divergence, la constitution + le PRD priment sur l'intention ; le plan superpowers prime sur le détail d'implémentation.

## Bridge de référence
Pattern hybride documenté (BMAD planifie → Spec-Kit exécute) et implémentation concrète : `oimiragieo/BMAD-SPEC-KIT`. Voir aussi les docs officielles : `github/spec-kit`, `docs.bmad-method.org`.
