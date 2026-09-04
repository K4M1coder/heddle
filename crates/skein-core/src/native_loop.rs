//! The native Skein-owned turn loop (design §4.2/§4.14, ADR-0003 decision A).
//! The engine, not the model, decides when a run ends, and every turn lands in
//! the hash-chained Ledger before it can influence anything else.

use crate::content::Message;
use crate::error::{Result, SkeinError};
use crate::ledger::{Ledger, StepKind};
use crate::loop_ctl::{Exit, LoopController};
use crate::model::{ModelClient, TurnRequest, WireExchange};
use crate::tool::{Redactor, ToolCall, ToolGateway, ToolTransport};

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
///
/// The redactor is private: unlike the other three it is not something a caller
/// reads back, only something it configures.
pub struct NativeLoop<C: ModelClient, P: ProgressProbe, T: ToolTransport> {
    pub client: C,
    pub probe: P,
    pub gateway: ToolGateway<T>,
    redactor: Redactor,
}

impl<C: ModelClient, P: ProgressProbe, T: ToolTransport> NativeLoop<C, P, T> {
    /// The redactor is a required argument rather than a builder step or a
    /// `Default`, because Constitution VI is deny-by-default: an optional one
    /// would make "this run records its conversation in cleartext" the silent
    /// default, which is the bug this exists to prevent.
    pub fn new(client: C, probe: P, gateway: ToolGateway<T>, redactor: Redactor) -> Self {
        NativeLoop {
            client,
            probe,
            gateway,
            redactor,
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

        // Once per run, and after the budget check above so a run with no budget
        // makes no round trip. The catalogue does not change mid-run, so every
        // turn is told the same thing.
        //
        // A failure here ends the run, exactly as `mediate` treats any
        // non-`ToolDenied` transport error: an inventory we could not read
        // leaves the run's capabilities unknown, and a model told "no tools"
        // because a server was unreachable would answer as if it had none.
        let tools = self.gateway.advertise()?;

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
                tools: tools.clone(),
            };
            // Captured before the call, so a client that errors still leaves the
            // request in the chain. Scrubbed on the way in and nowhere else:
            // `&req` below is the raw value, exactly as `ToolGateway` hands the
            // raw call to its transport.
            ledger.append(
                run_id,
                StepKind::LlmRequest,
                self.redactor.redact_json(&req)?,
            )?;

            let resp = self.client.turn(&req);
            if let Some(exchange) = self.client.take_wire_exchange() {
                // Field by field, and `redact_wire` for the two bodies: they
                // are already-serialized JSON, so a secret containing a quote
                // is on them in escaped form and `redact_json`'s whole-value
                // scrub would miss it. The url is ours and plain text.
                let scrubbed = WireExchange {
                    url: self.redactor.redact(&exchange.url),
                    status: exchange.status,
                    request: self.redactor.redact_wire(&exchange.request),
                    response: self.redactor.redact_wire(&exchange.response),
                };
                ledger.append(
                    run_id,
                    StepKind::WireExchange,
                    serde_json::to_string(&scrubbed)?,
                )?;
            }
            let resp = resp?;
            ledger.append(
                run_id,
                StepKind::LlmResponse,
                self.redactor.redact_json(&resp)?,
            )?;
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
            // The echo and the answers are pushed together, in that order, so
            // "every echoed id is answered by exactly one following tool
            // message, and no tool message answers an id nothing asked for" is
            // a property of this control flow rather than of review. The calls
            // are redacted for the same reason their results are: the history
            // is replayed into the next request's payload, and the wire and the
            // chain's `LlmRequest` capture must stay the same bytes.
            let echoed = resp.tool_calls.iter().map(|c| self.redactor.redact_call(c));
            messages.push(resp.message.with_tool_calls(echoed.collect()));
            messages.extend(feedback);
        }
    }

    /// Runs the turn's requested calls sequentially, in the order the model
    /// declared them, and returns what the model is told about each.
    ///
    /// This is the only place a [`Role::Tool`](crate::Role) message is made.
    /// Tool output is external content, and here it is distinguishable from the
    /// model's own words and from operator instruction by its role rather than
    /// by a marker any of them could equally have typed. What that does **not**
    /// buy is a model's *obedience*: one may still follow instructions it reads
    /// in tool content, under this shape exactly as under the text-marker one
    /// it replaced, and design §7 item 5's other half stays open.
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
            let body = match self.gateway.call_captured(run_id, call, ledger) {
                // The redacted capture, never the raw outcome: the history is
                // replayed into the next request's payload, so feeding back the
                // real secret would put it straight back on the chain. It goes
                // back unwrapped: an MCP `CallToolResult` already carries
                // whether it is an error.
                Ok((_, captured)) => captured.content,
                // The one outcome that genuinely needs words, because the
                // refusal is the gateway's and no tool ran to produce a payload
                // explaining it. The name is redacted for the same reason
                // `call_captured` redacts it.
                Err(SkeinError::ToolDenied { tool, reason }) => {
                    let tool = self.redactor.redact(&tool);
                    format!("the {tool} tool call was refused: {reason}")
                }
                Err(e) => return Err(e),
            };
            feedback.push(Message::tool_result(call.id.clone(), body));
        }
        Ok(feedback)
    }
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
