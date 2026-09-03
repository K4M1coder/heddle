//! The native Skein-owned turn loop (design §4.2/§4.14, ADR-0003 decision A).
//! The engine, not the model, decides when a run ends, and every turn lands in
//! the hash-chained Ledger before it can influence anything else.

use crate::content::Message;
use crate::error::{Result, SkeinError};
use crate::ledger::{Ledger, StepKind};
use crate::loop_ctl::{Exit, LoopController};
use crate::model::{ModelClient, TurnRequest};
use crate::tool::{ToolCall, ToolGateway, ToolTransport};

/// The ground-truth progress signal (Constitution VIII(b)).
/// Deliberately takes no model output: a probe that cannot see the model's words
/// cannot launder self-judgment into the budget.
pub trait ProgressProbe {
    fn observe(&mut self) -> bool;
}

/// How a run ended. `final_message` is populated only for [`Exit::FinalOutput`];
/// every other exit means the engine stopped the model mid-thought.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRun {
    pub exit: Exit,
    pub final_message: Option<Message>,
}

/// Drives turns against a provider until the controller says stop, mediating
/// every tool the model asks for through the gateway. The collaborators are
/// public so a caller can inspect the client, probe and gateway it injected.
/// The gateway is a concrete [`ToolGateway`], not a trait: the loop is generic
/// over the transport so it never names a protocol (Constitution IV), while the
/// governed step itself stays unsubstitutable (Constitution VI).
pub struct NativeLoop<C: ModelClient, P: ProgressProbe, T: ToolTransport> {
    pub client: C,
    pub probe: P,
    pub gateway: ToolGateway<T>,
}

impl<C: ModelClient, P: ProgressProbe, T: ToolTransport> NativeLoop<C, P, T> {
    pub fn new(client: C, probe: P, gateway: ToolGateway<T>) -> Self {
        NativeLoop {
            client,
            probe,
            gateway,
        }
    }

    /// Ledger and controller are borrowed, not owned: the caller inspects them
    /// after the run.
    pub fn run(
        &mut self,
        run_id: &str,
        prompt: Message,
        ledger: &mut Ledger,
        ctl: &mut LoopController,
    ) -> Result<LoopRun> {
        // Checked before the first call, so "the budget is enforced before it is
        // spent" is structural rather than a matter of turn ordering.
        if let Some(exit) = ctl.should_exit(false) {
            return terminate(ledger, run_id, exit, None);
        }

        let mut messages = vec![prompt];

        loop {
            ledger.append(
                run_id,
                StepKind::IterationBoundary,
                (ctl.iters() + 1).to_string(),
            )?;

            let req = TurnRequest {
                run_id: run_id.to_string(),
                messages: messages.clone(),
            };
            // Captured before the call, so a client that errors still leaves the
            // exact request in the chain.
            ledger.append(run_id, StepKind::LlmRequest, serde_json::to_string(&req)?)?;

            let resp = self.client.turn(&req)?;
            ledger.append(run_id, StepKind::LlmResponse, serde_json::to_string(&resp)?)?;
            ledger.append(run_id, StepKind::BudgetSpent, resp.tokens_used.to_string())?;

            // Before the probe: design §4.14 names tool results as a ground-truth
            // reflection anchor, so a probe that ran first could never see the
            // effect of this turn's own tool (Constitution VIII(b)).
            let feedback = self.mediate(run_id, &resp.tool_calls, ledger)?;

            let made_progress = self.probe.observe();
            ctl.record_iteration(resp.tokens_used, made_progress);

            if let Some(exit) = ctl.should_exit(resp.final_output) {
                return terminate(ledger, run_id, exit, Some(resp.message));
            }
            messages.push(resp.message);
            messages.extend(feedback);
        }
    }

    /// Runs the turn's requested calls sequentially, in the order the model
    /// declared them, and returns what the model is told about each.
    ///
    /// A refusal is a governance decision the run is designed to survive: the
    /// attempt and the verdict are already on the chain and the model is told
    /// plainly. Any other tool error leaves the tool's effect unknown, so it ends
    /// the run exactly as a provider failure does.
    fn mediate(
        &mut self,
        run_id: &str,
        calls: &[ToolCall],
        ledger: &mut Ledger,
    ) -> Result<Vec<Message>> {
        let mut feedback = Vec::with_capacity(calls.len());
        for call in calls {
            let message = match self.gateway.call_captured(run_id, call, ledger) {
                // The redacted capture, never the raw outcome: the history is
                // replayed into the next request's payload, so feeding back the
                // real secret would put it straight back on the chain.
                Ok((_, captured)) => tool_message(&captured.tool, "ok", &captured.content),
                Err(SkeinError::ToolDenied { tool, reason }) => {
                    tool_message(&tool, "denied", &reason)
                }
                Err(e) => return Err(e),
            };
            feedback.push(message);
        }
        Ok(feedback)
    }
}

/// Tool output is external content: it enters the conversation as user-role data
/// under a label, never as a system instruction and never as the model's own
/// words. The label is a marker, not an injection boundary — that needs a typed
/// content variant (design §7 item 5).
fn tool_message(tool: &str, status: &str, body: &str) -> Message {
    Message::user_text(format!("[tool_result tool={tool} status={status}]\n{body}"))
}

/// The single place a run is closed out, so "every terminated run ends with
/// exactly one Exit step" holds on all return paths.
fn terminate(
    ledger: &mut Ledger,
    run_id: &str,
    exit: Exit,
    last_message: Option<Message>,
) -> Result<LoopRun> {
    ledger.append(run_id, StepKind::Exit, format!("{exit:?}"))?;
    let final_message = match exit {
        Exit::FinalOutput => last_message,
        _ => None,
    };
    Ok(LoopRun {
        exit,
        final_message,
    })
}
