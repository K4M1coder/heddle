//! The native Skein-owned turn loop (design §4.2/§4.14, ADR-0003 decision A).
//! The engine, not the model, decides when a run ends, and every turn lands in
//! the hash-chained Ledger before it can influence anything else.

use crate::content::Message;
use crate::error::Result;
use crate::ledger::{Ledger, StepKind};
use crate::loop_ctl::{Exit, LoopController};
use crate::model::{ModelClient, TurnRequest};

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

/// Drives turns against a provider until the controller says stop.
/// The collaborators are public so a caller can inspect the client it injected.
pub struct NativeLoop<C: ModelClient, P: ProgressProbe> {
    pub client: C,
    pub probe: P,
}

impl<C: ModelClient, P: ProgressProbe> NativeLoop<C, P> {
    pub fn new(client: C, probe: P) -> Self {
        NativeLoop { client, probe }
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
        let mut messages = vec![prompt];

        loop {
            ledger.append(
                run_id,
                StepKind::IterationBoundary,
                (ctl.iters() + 1).to_string(),
            );

            let req = TurnRequest {
                run_id: run_id.to_string(),
                messages: messages.clone(),
            };
            // Captured before the call, so a client that errors still leaves the
            // exact request in the chain.
            ledger.append(run_id, StepKind::LlmRequest, serde_json::to_string(&req)?);

            let resp = self.client.turn(&req)?;
            ledger.append(run_id, StepKind::LlmResponse, serde_json::to_string(&resp)?);
            ledger.append(
                run_id,
                StepKind::BudgetSpent,
                resp.tokens_used.to_string(),
            );

            let made_progress = self.probe.observe();
            ctl.record_iteration(resp.tokens_used, made_progress);

            if let Some(exit) = ctl.should_exit(resp.final_output) {
                return Ok(terminate(ledger, run_id, exit, Some(resp.message)));
            }
            messages.push(resp.message);
        }
    }
}

/// The single place a run is closed out, so "every terminated run ends with
/// exactly one Exit step" holds on all return paths.
fn terminate(
    ledger: &mut Ledger,
    run_id: &str,
    exit: Exit,
    last_message: Option<Message>,
) -> LoopRun {
    ledger.append(run_id, StepKind::Exit, format!("{exit:?}"));
    let final_message = match exit {
        Exit::FinalOutput => last_message,
        _ => None,
    };
    LoopRun {
        exit,
        final_message,
    }
}
