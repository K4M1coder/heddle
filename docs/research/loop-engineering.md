# Loop Engineering — Research Report

**Date**: 2026-07-15 · **Status**: verified (deep-research, 6 angles, 24 sources fetched, 25 claims adversarially verified 2/3-to-refute, 25 confirmed / 0 refuted)

> Purpose: ground Skein's agent-loop and workflow engine (and our own development process) in verified loop-engineering patterns, separating established concepts from hype.

## 1. What "loop engineering" is (and isn't)

**"Loop engineering" is a practitioner umbrella term, not an academic term of art.** It was popularized around June 2026 (attributed to Addy Osmani; blog-sourced, *not* peer-reviewed) to name the deliberate **design, control, and instrumentation of the iterative feedback loop** at the heart of LLM agents: the *reason → act → observe → reflect → retry* cycle, run until an exit condition.

A common practitioner framing places it as the outermost of **four nested engineering layers** (blog-sourced, useful as a mental model, not canonical):

```
prompt engineering   → optimize a single instruction
context engineering  → assemble what the model sees
harness engineering  → the runtime/tools/memory around the model   (≈ Skein's core)
loop engineering     → make the multi-turn, self-iterating loop reliable
```

**Caveat (verified):** the umbrella term and Cole Medin's "PIV loop" attribution are single-source/blog-level. What *is* solid and primary-sourced are the constituent patterns below. Skein should introduce "loop engineering" as a *synthesizing frame* and cite the underlying patterns — which is what this document does.

## 2. Established patterns (primary sources)

| Pattern | What it is | Source |
|---|---|---|
| **Agent loop** | An LLM using tools on environmental feedback in a loop until an exit condition. Agents (model directs its own loop) vs workflows (predefined code paths). | [Anthropic](https://www.anthropic.com/engineering/building-effective-agents), [OpenAI](https://cdn.openai.com/business-guides-and-resources/a-practical-guide-to-building-agents.pdf), [LangChain](https://docs.langchain.com/oss/python/langchain/middleware/overview) |
| **ReAct** | Interleave reasoning traces + actions (Thought→Action→Observation); grounding in external retrieval cuts hallucination vs pure chain-of-thought. | [Yao et al., ICLR 2023](https://arxiv.org/abs/2210.03629) |
| **Reflexion** | Actor→Evaluator→Self-reflection; write a verbal reflection on failure, store in an **episodic memory buffer**, retry with reflections. No weight updates. | [Shinn et al., NeurIPS 2023](https://arxiv.org/abs/2303.11366) |
| **Self-Refine** | One frozen LLM as generator+critic+refiner, iterating at test time. No training. | [Madaan et al., NeurIPS 2023](https://arxiv.org/abs/2303.17651) |
| **Evaluator-optimizer** | One LLM generates, a **separate** LLM evaluates+feeds back in a loop. Use when eval criteria are clear and iteration adds measurable value. | [Anthropic](https://www.anthropic.com/engineering/building-effective-agents) |
| **Evaluator / reflect-refine as control loop** | The reflect-refine cycle implemented as an event-driven feedback control loop (control-theory lineage). | [AWS Prescriptive Guidance](https://docs.aws.amazon.com/prescriptive-guidance/latest/agentic-ai-patterns/evaluator-reflect-refine-loop-patterns.html) |
| **Middleware / hooks** | Composable interceptors around each loop step (before_model / after_model / modify_request) for retries, fallbacks, early termination, guardrails. | [LangChain](https://docs.langchain.com/oss/python/langchain/middleware/overview) |
| **HITL gates / approvals** | Exceeding retry/action limits escalates to a human; tools marked `needsApproval` halt the run until approved/rejected. | [OpenAI](https://developers.openai.com/api/docs/guides/agents/guardrails-approvals) |

## 3. The two load-bearing constraints (design-critical)

1. **Intrinsic self-correction is unreliable** — LLMs struggle to self-correct reasoning without external feedback, and performance *can degrade* after a self-correction pass ([Huang et al., DeepMind, ICLR 2024](https://arxiv.org/abs/2310.01798), replicated across GPT-3.5/4/4-Turbo/Llama-2; corroborated by Kamoi et al. TACL survey). ⇒ **Anchor every reflect/retry to ground-truth external feedback** (tool results, code execution, tests, linters, type-checkers), never model self-judgment. (RL-trained reasoning models self-correct via *training-time* instillation — a different regime, not intrinsic prompting.)
2. **Termination must be externally enforced** — do not trust the model to decide when to stop. Use iteration/turn caps, token/cost budgets, and no-progress detection; exit conditions (OpenAI): tool calls, structured output, errors, or max turns. ([Anthropic](https://www.anthropic.com/engineering/building-effective-agents), [OpenAI](https://cdn.openai.com/business-guides-and-resources/a-practical-guide-to-building-agents.pdf))

**Corollary (Anthropic, verbatim):** "it's crucial for the agents to gain 'ground truth' from the environment at each step (such as tool call results or code execution) to assess its progress." And: prefer the simplest solution — only add loop-based autonomy when it earns its keep.

## 4. Three levels of verification (practitioner, actionable)

- **Action verification** — validate each tool/step result as it happens.
- **Iteration verification** — after each loop turn, check progress against criteria (tests/linters).
- **Terminal verification** — before declaring done, run the full acceptance check.

## 5. PIV loop (Cole Medin) — noted, not canonical

Plan → Implement → Validate: define task/constraints/acceptance criteria, let the agent implement, validate output against criteria before proceeding. Real and directly usable as a dev-loop shape, but **blog/single-source** — adopt as a convention, not an authority. ([workshop repo](https://github.com/coleam00/ai-transformation-workshop))

## 6. How this maps onto Skein

See design §4.14. Summary: loop control is a **first-class, engine-enforced layer** (not prompt text); reflections and loop state live in the **Ledger** (§4.11); reflect/retry is anchored to **MCP tool / test / compiler ground truth**; node types (ReAct / Reflexion / Self-Refine / evaluator-optimizer) are **Workflow nodes** (§4.12); HITL gates reuse existing confirmations (§7.4).

## 7. Open questions (carried into design)
- Exact durable event types for Actor/Evaluator/Reflection/observation/approval/termination on the Ledger, and loop-state reconstruction on resume.
- Which ground-truth signals to wire into every dev loop (compiler/tests/linters/type-checkers/MCP results) to keep reflect-and-retry net-positive.
- Default loop budgets and no-progress heuristics tuned for a *software-development* agent vs a chat agent.
