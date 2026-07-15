# Skein

Skein is an independent open-source, local-first agentic platform designed to unify chat, coding, cowork, governed workflows, multi-agent execution, tools, MCP connectors, local/cloud inference, memory, evidence and team collaboration in one adaptable product.

## Project ownership

Skein is created and owned by **Cédric Thedrez**:

- GitHub: **[`kamicoder`](https://github.com/kamicoder)**
- Other public identity: **`cethgame`**

## Current status

The project is in architecture and specification hardening. Product implementation has not started. The design is developed through a BMAD × Spec-Kit bridge, adversarial architecture review and evidence-based loop engineering.

Key documents:

- [Master design](docs/superpowers/specs/2026-07-15-skein-design.md)
- [BMAD PRD](_bmad-output/planning-artifacts/PRD.md)
- [BMAD architecture](_bmad-output/planning-artifacts/architecture.md)
- [Spec-Kit constitution](.specify/memory/constitution.md)
- [Design completeness policy](docs/DESIGN-COMPLETENESS-POLICY.md)
- [Platform landscape and reuse strategy](docs/research/agent-platform-landscape.md)
- [Development environment](docs/DEVELOPMENT.md)

## Development language policy

All persistent project content is written in English: source code, comments, documentation, specifications, architecture artifacts, examples, tests and commit messages.

## Development bootstrap

After cloning:

```powershell
# Windows
.\scripts\bootstrap.ps1
```

```bash
# macOS / Linux
./scripts/bootstrap.sh
```

These scripts prepare the languages, quality tools, BMAD, Spec-Kit and local development dependencies required by the current project stage.
