# Feature Specification: Native workflow engine + TaskTracker + hierarchy

**Feature Branch**: `002-workflow-engine`

**Created**: 2026-07-15

**Status**: Draft

**Input**: Epic 6 (`_bmad-output/planning-artifacts/epics.md`); design §4.12, §4.13, §5.5.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Sequence a resumable multi-agent chain (Priority: P1)
As a user, I define a workflow that chains several agents/tools across the SDLC (e.g. read spec → code → test → package); if it is interrupted, it **resumes** where it left off.

**Why this priority**: this is the requested capability — native multi-agent sequencing through the harness; resumption proves Ledger synchronization.

**Independent Test**: launch a workflow with ≥3 nodes, interrupt it, then resume it; verify that no already-logged step is re-executed.

**Acceptance Scenarios**:
1. **Given** a workflow with sequential nodes, **When** I run it, **Then** each step produces a `Step` in the Ledger and the final result is reached.
2. **Given** a workflow interrupted after node 2, **When** I `resume`, **Then** execution resumes at node 3 (idempotence of logged steps).
3. **Given** an `Approval` node, **When** execution reaches it, **Then** it waits for human validation before continuing.

### User Story 2 - Choose the task tracker through the hierarchy (Priority: P1)
As a project manager, I set the TaskTracker (local Vikunja or cloud Jira) at the silo/project level; lower levels inherit it, and a lock set higher up takes precedence.

**Independent Test**: set Jira at the silo → a child project uses it; set nothing → a project can choose Vikunja.

**Acceptance Scenarios**:
1. **Given** TaskTracker=Jira set at the silo, **When** a conversation in a child project creates a task, **Then** it is created in Jira (the silo setting locks it).
2. **Given** no setting above the project, **When** the project chooses Vikunja, **Then** its conversations use Vikunja.
3. **Given** Local mode, **When** config is resolved, **Then** the hierarchy applies without a Team level.

### User Story 3 - Progress reflected in the tracker (Priority: P2)
As a user, a workflow's progress creates/updates tasks in the active tracker.

**Acceptance Scenarios**:
1. **Given** a running workflow, **When** a node completes, **Then** the corresponding task moves to the appropriate status in the resolved TaskTracker.

### Edge Cases
- Resume after crash: state is rebuilt from the Ledger (no double effect on idempotent steps; non-idempotent external effects are flagged and not replayed without confirmation).
- Tracker backend unavailable (Jira offline in Local mode): explicit fallback/error; the local tracker remains available.
- Lock conflict: a lower level attempts to override a setting locked higher up → explicit refusal.

## Requirements *(mandatory)*

### Functional Requirements
- **FR-013**: The system MUST execute workflows (agent/tool/subagent/approval/condition/parallel/loop nodes) through a `WorkflowEngine`.
- **FR-013a**: Every workflow step MUST be logged as a Ledger `Step`; a workflow MUST be **resumable** from the last Step.
- **FR-013b**: Goose recipes and BMAD/Spec-Kit flows MUST be executable as workflows.
- **FR-014**: The system MUST provide a pluggable `TaskTracker`: local (silo), Vikunja (embedded), Jira (via MCP).
- **FR-015**: Config (including the TaskTracker) MUST be resolved according to the Silo▸Team▸Project▸Conversation hierarchy, a setting fixed at one level **locking** the lower levels.
- **FR-016**: Workflows MUST be able to orchestrate the SDLC through MCP connectors (design, dev/git, tests, packaging, deployment) and the TaskTracker.

### Key Entities
- **Workflow**: `{name, params, graph: [Node]}`; **Node**: agent/tool/subagent/approval/cond/parallel/loop.
- **WorkflowRun**: executed instance, addressed by `RunId`, derived from the Ledger.
- **Task**: tracking unit (`{id, title, status, links}`) in a TaskTracker.
- **ConfigScope**: resolution level (Silo/Team/Project/Conversation) + `locked` flag.

## Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001**: a workflow with ≥3 nodes that is interrupted then resumed does not re-execute any logged step (US1).
- **SC-002**: hierarchical resolution of the TaskTracker honors "the highest level locks" (US2), tested across all 4 levels and in Local mode (3 levels).
- **SC-003**: a workflow's progress is visible in the resolved tracker (US3).
- **SC-004**: a workflow orchestrates at least one real SDLC chain (e.g. code → test → PR) through connectors (US1/FR-016).

## Assumptions
- The native engine (event-sourced Ledger) is the default; Temporal/Windmill are optional backends behind `WorkflowEngine`.
- Vikunja is the default embedded OSS tracker; Jira via the existing MCP connector.
- The hierarchy lives within a silo (never cross-silo); team membership remains the authorization boundary (§7.10).
- In Local mode, the Team level does not exist.
