---
title: Heddle Project Governance
status: remediation-draft-v2
updated: 2026-07-16
build_authorization: NOT_READY
---

# Heddle Project Governance

## Identity and Independence

Heddle is an independent open-source project created and owned by Cédric Thedrez, known as `K4M1coder` on GitHub and `cethgame` elsewhere. Project metadata, contribution material, generated artifacts, releases, and public statements shall not assert or imply an organizational affiliation unless the owner deliberately amends this policy with verifiable authority.

## Repository Language

All persistent repository content shall be written in English, including source code, comments, identifiers, tests, commits, documentation, BMAD artifacts, Spec-Kit artifacts, workflow definitions, schemas, examples, generated reports, and release material. French is permitted in direct conversation with the owner, but conversational language shall not leak into persistent artifacts unless a separately approved localization feature explicitly requires translated fixtures or resources.

## Authority and Precedence

For planning and build authorization, precedence is:

1. applicable law and an approved security exception process;
2. the accepted project constitution;
3. this governance policy and `QUALITY-GATES.md`;
4. the canonical BMAD PRD and accepted architecture/ADRs;
5. approved Spec-Kit feature artifacts and their contracts;
6. release capability registries and implementation tasks;
7. non-normative research and mechanism notes.

Mandatory process rules in `addendum.update-draft.md` Sections 9 and 10 derive their authority from this policy and `QUALITY-GATES.md`. The addendum remains non-normative for product mechanisms; it cannot override a product requirement or authorize implementation. A lower-precedence artifact may tighten safety but may not weaken a higher-precedence rule.

## Hierarchical Product Governance

Runtime administration follows Silo, Team, Project, and Conversation scope. Delegation is explicit, bounded, revocable, and cannot convey authority the delegator does not hold. Higher-scope denials, explicit locks, and security floors cap lower scopes. Lower scopes may narrow grants or increase security. Connector read and mutation authority, computer capabilities, data access, model destinations, workflow changes, skills, and harness settings are independently grantable.

## Change Classes During NOT_READY

While build authorization is `NOT_READY`, permitted engineering-substrate changes are limited to planning artifacts, threat models, contracts and schemas without product runtime behavior, test fixtures, disposable bounded spikes, environment/bootstrap automation, quality tooling configuration, CI/staging definitions, documentation, and evidence capture needed to decide a named gate.

Prohibited changes include product runtime features, production connector implementations, persistent product schemas/migrations, deployable user-facing behavior, implementation-agent swarms, and spike code promoted or copied into product paths. Every permitted substrate change must name its gate, have a cleanup or promotion decision, and remain independently reviewable.

## Decision and Exception Control

One-way-door decisions require an owner, stable decision ID, pre-registered evidence, independent adversarial review, explicit disposition, and expiry/freshness rule. Exceptions require scope, rationale, risk owner, compensating controls, expiration, and audit evidence. No exception may redefine strict Local, weaken silo/team isolation, expose secrets to models, allow agent self-authorization, or bypass the Build Authorization Gate.
