# Spike 2 Evidence — Workflow reuse (`spikes/workflow-archon/`)

**Date:** 2026-07-16 · **Status:** COMPLETE · **Method:** pre-registered criteria (spike-protocol.md §Spike 2); ground truth = 3 passing tests.

## Question
Can one Archon-style YAML workflow map **losslessly** onto Heddle's canonical graph (nodes agent/tool/subagent/approval/cond/parallel/loop), execute as a stub graph, and round-trip back to YAML?

## Result — PASS

| Exit criterion | Proof |
|---|---|
| Real Archon-style workflow parsed | `parses_all_kinds_and_executes`: a `build-and-ship` workflow using Archon vocabulary (`steps`, `type: ai/deterministic/human/cond/parallel/loop/subagent`, `then/else/branches/body`) parses with **zero gaps** |
| All 7 canonical kinds representable | same test asserts every kind ∈ {agent, tool, subagent, approval, cond, parallel, loop} appears after translation |
| Executed as a stub graph | `execute_stub` pre-order walk reaches all nested ids (cond arms, parallel branches, loop body): plan→gate→code→fanout→unit→lint→retry→fix→replan→signoff |
| Round-trip lossless | `round_trip_is_lossless`: canonical → Archon YAML → canonical is **identity** (`wf1 == wf2`) |
| Gaps surfaced, not swallowed | `unknown_step_type_is_recorded_as_gap`: an unknown `type: quantum_step` is recorded in `gaps` and skipped (not panicked); mappable steps still translate |

`cargo test` → **3 passed / 0 failed**.

## Findings
- The **translation is a real vocabulary mapping**, not a serde round-trip of one type: `ArchonWorkflow`/`ArchonStep` (external shape) ↔ canonical `Workflow`/`Node` are distinct models with a mapping layer that also normalizes synonyms (`ai|agent`, `deterministic|tool`, `human|approval`, `conditional|cond`, `sub_workflow|subagent`).
- **Gap discipline works**: unmappable constructs are reported, matching QUALITY-GATES "no silent truncation".

## Caveats (honest scope)
- This validates the **structural graph contract**, not runtime semantics (retry limits, parallel join policy, cond evaluation) — those belong to the LoopController/engine (Epic 6), not this spike.
- The YAML shape is a **plausible reconstruction** of Archon's schema, not a byte-for-byte import of a real Archon export file (Archon's engine is TypeScript/Bun; a true fixture import is a follow-up before claiming import/export compatibility).
- `serde_yaml` 0.9 is used (unmaintained but functional); the product would pick a maintained YAML crate.

## Consequence for ADR-0003
Spike 2 of 5 complete: workflow-reuse route is viable — Heddle owns a canonical graph, Archon-style YAML is a projection onto it. Remaining: Spike 3 (context quality), Spike 5 (tri-OS install). Spikes 1 & 4 already done.
