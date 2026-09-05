//! The native workflow engine (spec 002 FR-013/FR-013a, design §4.12).
//!
//! `run` replays the run's Ledger, skips every node the chain already records as
//! complete, and executes the remainder in graph order. That single mechanism is
//! all of "resume": there is no second API, no cursor, and no state kept outside
//! the chain — which is what keeps `spec.md`'s "`WorkflowRun` … derived from the
//! Ledger" literally true rather than aspirational.

use crate::node::Node;
use crate::Workflow;
use heddle_core::{
    HeddleError, Ledger, Message, ModelClient, NewTask, NoTracker, Redactor, Result, StepKind,
    TaskId, TaskStatus, TaskTracker, ToolCall, ToolGateway, ToolTransport, TurnRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The three decisions a workflow-level approval can be in. Spelled as
/// constants because they are written by `run`/`decide` and read back by the
/// scan, and a typo between the two would silently turn "approved" into "not
/// decided yet" — a gate that opens by accident.
const PENDING: &str = "pending";
const APPROVED: &str = "approved";
const REJECTED: &str = "rejected";

/// The `WorkflowNode` step's payload: which node completed, and what it
/// produced. Private to this crate, exactly as `ApprovalRecord` is private to
/// `heddle-core`'s `tool.rs` — the `StepKind` is the core's vocabulary, the
/// payload belongs beside the code that writes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeRecord {
    node_id: String,
    outcome: String,
}

/// The `StateChange` step's payload: which node opened which task.
///
/// The binding is written to the chain rather than derived from `node_id`
/// because a tracker assigns its own ids — Jira answers `PROJ-123`, the local
/// one answers a row id — so there is nothing to derive from. Recording it is
/// also what makes it *survive*: a resumed run in a different process learns
/// which task is already open by reading the chain, which is the same way it
/// learns which nodes are already done (Constitution V).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskBinding {
    node_id: String,
    task_id: String,
}

/// The workflow-level `Approval` step's payload.
///
/// It shares `StepKind::Approval` with the gateway's `ApprovalRecord` — both
/// mean "a decision was required and recorded" — and shares no *field* with it.
/// That is what lets the scan below tell them apart by parse rather than by
/// convention: `ApprovalRecord` is `{tool, decision, reason}`, so it has no
/// `node_id` and cannot deserialize into this type. The two deciders are
/// different (a policy there, a human here) and so the two payloads stay
/// different types.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowApproval {
    node_id: String,
    decision: String,
}

/// How a `run` call ended.
///
/// `AwaitingApproval` is not an error and neither is `Rejected`: a workflow that
/// stops on a human is doing its job, and a human who says no has answered the
/// question the node asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowExit {
    Completed,
    AwaitingApproval { node_id: String },
    Rejected { node_id: String },
}

/// The outcome of one `run` call. `final_outcome` is the last node's recorded
/// outcome — read off the chain for a node this call skipped, so a run that
/// resumes with nothing left to do still reports the same result the original
/// call would have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    pub exit: WorkflowExit,
    pub final_outcome: Option<String>,
}

/// Executes a [`Workflow`]'s graph against a Ledger.
///
/// Generic over the same two ports [`NativeLoop`] is, and for the same reason
/// (Constitution IV): the engine never names a provider or a protocol. There is
/// no `Agent` trait to box here and none anywhere in the workspace — an agent
/// *is* the pairing of a [`ModelClient`] with a [`ToolGateway`], which is what
/// this struct holds.
///
/// The redactor is private, unlike the other two: it is not something a caller
/// reads back, only something it configures. It is a required constructor
/// argument for [`NativeLoop::new`]'s stated reason — an optional one would make
/// "this workflow records its conversation in cleartext" the silent default,
/// which is the bug it exists to prevent (Constitution VI).
///
/// [`NativeLoop`]: heddle_core::NativeLoop
/// [`NativeLoop::new`]: heddle_core::NativeLoop::new
pub struct WorkflowEngine<C: ModelClient, T: ToolTransport, K: TaskTracker = NoTracker> {
    pub client: C,
    pub gateway: ToolGateway<T>,
    redactor: Redactor,
    /// The resolved tracker (design §4.13), or `None` for a run nobody asked to
    /// track. *Which* tracker this is was decided by
    /// [`Hierarchy`](heddle_core::Hierarchy) before the engine was built — the
    /// engine receives the answer and never the question, so it names no
    /// backend and holds no config (Constitution IV).
    ///
    /// `NoTracker` is uninhabited, so the default type parameter makes
    /// `WorkflowEngine<C, T>` a type in which "tracked" is not merely unset but
    /// unrepresentable.
    tracker: Option<K>,
}

impl<C: ModelClient, T: ToolTransport> WorkflowEngine<C, T, NoTracker> {
    /// An engine that tracks nothing. The chain it writes is exactly the chain
    /// this crate wrote before task tracking existed — no binding steps, since
    /// there is nothing to bind to.
    pub fn new(client: C, gateway: ToolGateway<T>, redactor: Redactor) -> Self {
        WorkflowEngine {
            client,
            gateway,
            redactor,
            tracker: None,
        }
    }
}

impl<C: ModelClient, T: ToolTransport, K: TaskTracker> WorkflowEngine<C, T, K> {
    /// An engine that reflects each node's progress into `tracker` (spec 002
    /// User Story 3).
    pub fn with_tracker(
        client: C,
        gateway: ToolGateway<T>,
        redactor: Redactor,
        tracker: K,
    ) -> Self {
        WorkflowEngine {
            client,
            gateway,
            redactor,
            tracker: Some(tracker),
        }
    }

    /// The tracker this engine was given, if it was given one.
    pub fn tracker(&self) -> Option<&K> {
        self.tracker.as_ref()
    }

    /// Execute every node of `workflow` that the Ledger does not already record
    /// as complete, in graph order.
    ///
    /// Returns as soon as the graph is exhausted, an approval is undecided, or
    /// an approval was refused. It never blocks: "the process was killed" and "a
    /// human has not answered" are answered by the same scan, so a run
    /// interrupted by a kill is recoverable by a *new* process opening the same
    /// Ledger — which a parked thread would not be.
    pub fn run(
        &mut self,
        run_id: &str,
        workflow: &Workflow,
        ledger: &mut Ledger,
    ) -> Result<WorkflowRun> {
        let completed = completed_nodes(ledger, run_id)?;
        let decisions = last_decisions(ledger, run_id);
        let bound = task_bindings(ledger, run_id);
        let mut final_outcome = None;

        for node in &workflow.graph {
            // The whole of resume (SC-001): a node the chain already records is
            // not re-executed and not re-logged. Its outcome still counts as the
            // run's result so far, read back off the chain rather than
            // recomputed.
            if let Some(outcome) = completed.get(node.id()) {
                final_outcome = Some(outcome.clone());
                continue;
            }

            // Opened *before* the executor, so a run that dies inside a node
            // leaves that node's task visibly in progress rather than absent.
            // A node the chain already binds to a task reuses it, which is what
            // keeps polling a slow human from opening a task per poll.
            let task = self.open_task(run_id, workflow, node, &bound, ledger)?;

            let outcome = match node {
                Node::Agent { prompt, .. } => self.run_agent(run_id, prompt, ledger)?,
                Node::Tool { call, .. } => self.run_tool(run_id, call, ledger)?,
                Node::Approval { id, .. } => {
                    match decisions.get(id.as_str()).map(String::as_str) {
                        // Nobody has been asked yet: ask, once.
                        None => {
                            self.append_decision(run_id, id, PENDING, ledger)?;
                            self.move_task(&task, TaskStatus::Blocked)?;
                            return Ok(WorkflowRun {
                                exit: WorkflowExit::AwaitingApproval {
                                    node_id: id.clone(),
                                },
                                final_outcome,
                            });
                        }
                        // Asked and unanswered. Nothing is appended — otherwise
                        // every poll of a slow human would grow the chain.
                        Some(PENDING) => {
                            self.move_task(&task, TaskStatus::Blocked)?;
                            return Ok(WorkflowRun {
                                exit: WorkflowExit::AwaitingApproval {
                                    node_id: id.clone(),
                                },
                                final_outcome,
                            });
                        }
                        Some(REJECTED) => {
                            // Cancelled, not Blocked: a human who said no has
                            // answered the question the node asked, and a task
                            // left blocked would claim they still owe one.
                            self.move_task(&task, TaskStatus::Cancelled)?;
                            return Ok(WorkflowRun {
                                exit: WorkflowExit::Rejected {
                                    node_id: id.clone(),
                                },
                                final_outcome,
                            });
                        }
                        // Decided: the gate completes and the walk continues in
                        // *this* call. An approval already given should not need
                        // a further round trip to take effect.
                        Some(APPROVED) => APPROVED.to_string(),
                        Some(other) => {
                            return Err(HeddleError::LedgerIntegrity {
                                seq: 0,
                                detail: format!(
                                    "approval node {id} carries an unknown decision {other:?}"
                                ),
                            })
                        }
                    }
                }
                // Reserved by `spec.md`'s node vocabulary, implemented by a
                // later slice. The refusal happens **here**, before any step is
                // appended for this node, so the chain carries no partial record
                // to mislead a reader and a retry after that slice lands resumes
                // cleanly at this same node.
                Node::Subagent { .. }
                | Node::Condition { .. }
                | Node::Parallel { .. }
                | Node::Loop { .. } => return Err(unsupported(node)),
            };

            self.append_completion(run_id, node.id(), &outcome, ledger)?;
            // Spec 002 User Story 3, whole: "when a node completes, the
            // corresponding task moves to the appropriate status". After the
            // completion step, so the chain is the thing that is true first and
            // the tracker is the projection of it.
            self.move_task(&task, TaskStatus::Done)?;
            final_outcome = Some(outcome);
        }

        Ok(WorkflowRun {
            exit: WorkflowExit::Completed,
            final_outcome,
        })
    }

    /// Record a human decision for an `Approval` node.
    ///
    /// It does not itself resume: the caller calls [`WorkflowEngine::run`]
    /// again, which is one Ledger scan away from continuing. That mirrors
    /// `Ledger::append`'s own append-then-let-the-reader-act shape rather than
    /// inventing a callback, and it is what lets the decision be recorded by a
    /// different process from the one that will act on it.
    pub fn decide(
        &mut self,
        run_id: &str,
        node_id: &str,
        approved: bool,
        ledger: &mut Ledger,
    ) -> Result<()> {
        let decision = if approved { APPROVED } else { REJECTED };
        self.append_decision(run_id, node_id, decision, ledger)?;
        Ok(())
    }

    /// One bounded turn plus mediation of the tools that turn asked for — not an
    /// open-ended ReAct loop, and so not something a `LoopController` has
    /// anything to terminate (`plan.md` D4).
    ///
    /// The request is captured *before* the call, so a client that errors still
    /// leaves the request on the chain, and it is scrubbed on the way in and
    /// nowhere else — `&req` below is the raw value. Both are `NativeLoop`'s
    /// rules, followed here rather than restated.
    fn run_agent(&mut self, run_id: &str, prompt: &Message, ledger: &mut Ledger) -> Result<String> {
        let req = TurnRequest {
            run_id: run_id.to_string(),
            messages: vec![prompt.clone()],
            tools: self.gateway.advertise()?,
        };
        ledger.append(
            run_id,
            StepKind::LlmRequest,
            self.redactor.redact_json(&req)?,
        )?;

        let resp = self.client.turn(&req)?;
        ledger.append(
            run_id,
            StepKind::LlmResponse,
            self.redactor.redact_json(&resp)?,
        )?;

        for call in &resp.tool_calls {
            match self.gateway.call_captured(run_id, call, ledger) {
                Ok(_) => {}
                // A refusal is a governance decision the run is designed to
                // survive, exactly as in `NativeLoop::mediate`: the attempt and
                // the verdict are already on the chain. Unlike there, this slice
                // has no second turn to tell the model about it — a stated
                // consequence of the one-turn node definition, not an oversight.
                Err(HeddleError::ToolDenied { .. }) => {}
                // Any other tool error leaves the tool's effect unknown, so it
                // ends the node exactly as a provider failure does.
                Err(e) => return Err(e),
            }
        }

        Ok(resp.message.text())
    }

    /// One governed call. The whole of the policy/record/execute path already
    /// lives in `ToolGateway::call_captured`; this wraps it rather than
    /// re-implementing any part of it, so a workflow cannot become a way around
    /// the governor (Constitution VI).
    fn run_tool(&mut self, run_id: &str, call: &ToolCall, ledger: &mut Ledger) -> Result<String> {
        let (_, captured) = self.gateway.call_captured(run_id, call, ledger)?;
        Ok(captured.content)
    }

    /// Ensure this node has a task, and return its id.
    ///
    /// Three cases, and only the middle one costs anything: no tracker at all
    /// (`None`, and the caller's status changes become no-ops); a node the chain
    /// already binds to a task (re-asserted, not re-created); and a node seen
    /// for the first time (created, and the binding appended).
    ///
    /// The binding step is appended **after** the tracker has answered, so a
    /// tracker that refuses leaves no chain entry pointing at a task that was
    /// never opened.
    fn open_task(
        &mut self,
        run_id: &str,
        workflow: &Workflow,
        node: &Node,
        bound: &HashMap<String, String>,
        ledger: &mut Ledger,
    ) -> Result<Option<TaskId>> {
        let Some(tracker) = self.tracker.as_mut() else {
            return Ok(None);
        };

        if let Some(task_id) = bound.get(node.id()) {
            let id = TaskId::new(task_id);
            tracker.update(&id, TaskStatus::InProgress)?;
            return Ok(Some(id));
        }

        // The title is the caller's only handle on this task in a tracker that
        // knows nothing about workflows; the links are what let that same
        // caller ask for "this run's tasks" without the tracker learning what a
        // run is.
        let id = tracker.create(
            NewTask::new(format!("{}: {}", workflow.name, node.id()))
                .with_status(TaskStatus::InProgress)
                .with_link(run_id)
                .with_link(node.id()),
        )?;
        let binding = TaskBinding {
            node_id: node.id().to_string(),
            task_id: id.to_string(),
        };
        ledger.append(
            run_id,
            StepKind::StateChange,
            serde_json::to_string(&binding)?,
        )?;
        Ok(Some(id))
    }

    /// Move a task, if there is one. Both `None`s — no tracker, or a node the
    /// tracker never saw — mean the same thing here, so the caller writes one
    /// unconditional line at each exit instead of guarding every one of them.
    fn move_task(&mut self, task: &Option<TaskId>, status: TaskStatus) -> Result<()> {
        if let (Some(tracker), Some(id)) = (self.tracker.as_mut(), task.as_ref()) {
            tracker.update(id, status)?;
        }
        Ok(())
    }

    fn append_completion(
        &mut self,
        run_id: &str,
        node_id: &str,
        outcome: &str,
        ledger: &mut Ledger,
    ) -> Result<()> {
        let record = NodeRecord {
            node_id: node_id.to_string(),
            outcome: self.redactor.redact(outcome),
        };
        ledger.append(
            run_id,
            StepKind::WorkflowNode,
            serde_json::to_string(&record)?,
        )?;
        Ok(())
    }

    fn append_decision(
        &mut self,
        run_id: &str,
        node_id: &str,
        decision: &str,
        ledger: &mut Ledger,
    ) -> Result<()> {
        let record = WorkflowApproval {
            node_id: node_id.to_string(),
            decision: decision.to_string(),
        };
        ledger.append(run_id, StepKind::Approval, serde_json::to_string(&record)?)?;
        Ok(())
    }
}

/// The one place a deferred node kind refuses, so the message cannot drift
/// between variants.
fn unsupported(node: &Node) -> HeddleError {
    HeddleError::Unsupported(format!(
        "workflow node kind {} is not implemented until a follow-up slice (node {})",
        node.kind_name(),
        node.id()
    ))
}

/// Every node the chain records as complete, with what it produced.
///
/// A malformed payload is an error rather than a skip: a `WorkflowNode` step
/// this build cannot read means the engine cannot tell whether the node ran, and
/// guessing "it did not" would re-execute it — the exact double effect
/// `spec.md`'s Edge Cases forbid.
fn completed_nodes(ledger: &Ledger, run_id: &str) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for step in ledger.log(run_id) {
        if step.kind != StepKind::WorkflowNode {
            continue;
        }
        let record: NodeRecord = serde_json::from_str(&step.payload)?;
        out.insert(record.node_id, record.outcome);
    }
    Ok(out)
}

/// The **last** decision recorded per approval node.
///
/// Last, not first: `decide` appends, and an append-only chain records a change
/// of mind as a later step rather than by editing an earlier one.
///
/// A payload that does not parse is skipped rather than refused, and that is the
/// opposite of `completed_nodes` above on purpose: `StepKind::Approval` is
/// shared with the gateway, so *most* steps of this kind in a real run are
/// `ApprovalRecord`s about tool calls and are correctly none of this scan's
/// business.
/// Every `node_id -> task_id` binding the chain records.
///
/// Lenient like [`last_decisions`] and for the same reason: `StepKind::StateChange`
/// is the core's vocabulary and this crate is not its only writer, so a payload
/// that does not parse is somebody else's state change rather than a corrupt
/// one. The last binding for a node wins, matching the append-only chain's way
/// of recording a change.
fn task_bindings(ledger: &Ledger, run_id: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for step in ledger.log(run_id) {
        if step.kind != StepKind::StateChange {
            continue;
        }
        if let Ok(binding) = serde_json::from_str::<TaskBinding>(&step.payload) {
            out.insert(binding.node_id, binding.task_id);
        }
    }
    out
}

fn last_decisions(ledger: &Ledger, run_id: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for step in ledger.log(run_id) {
        if step.kind != StepKind::Approval {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<WorkflowApproval>(&step.payload) {
            out.insert(record.node_id, record.decision);
        }
    }
    out
}
