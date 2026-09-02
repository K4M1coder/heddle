//! Acceptance tests for the native turn loop (spec 004).
//! Every exit is driven through real wiring: a `NativeLoop` calling a scripted
//! `ModelClient` and writing into a real hash-chained `Ledger`.

use skein_core::{
    Exit, Ledger, LoopBudget, LoopController, Message, ModelClient, NativeLoop, ProgressProbe,
    Result, SkeinError, StepKind, TurnRequest, TurnResponse,
};

/// A model whose every turn is decided in advance. Counts calls so tests can
/// prove the loop never spent a turn it had no budget for.
struct ScriptedModel {
    script: Vec<TurnResponse>,
    calls: usize,
    fail_at: Option<usize>,
}

impl ScriptedModel {
    fn new(script: Vec<TurnResponse>) -> Self {
        ScriptedModel {
            script,
            calls: 0,
            fail_at: None,
        }
    }
    fn failing_at(script: Vec<TurnResponse>, fail_at: usize) -> Self {
        ScriptedModel {
            script,
            calls: 0,
            fail_at: Some(fail_at),
        }
    }
}

impl ModelClient for ScriptedModel {
    fn turn(&mut self, _req: &TurnRequest) -> Result<TurnResponse> {
        let i = self.calls;
        self.calls += 1;
        if self.fail_at == Some(i) {
            return Err(SkeinError::Model(format!("scripted failure on turn {i}")));
        }
        Ok(self
            .script
            .get(i)
            .unwrap_or_else(|| panic!("script exhausted: the loop asked for turn {i}"))
            .clone())
    }
}

/// Ground truth the test controls directly — never the model.
struct ScriptedProbe {
    verdicts: Vec<bool>,
    idx: usize,
}

impl ScriptedProbe {
    fn new(verdicts: Vec<bool>) -> Self {
        ScriptedProbe { verdicts, idx: 0 }
    }
}

impl ProgressProbe for ScriptedProbe {
    fn observe(&mut self) -> bool {
        let v = self.verdicts[self.idx.min(self.verdicts.len() - 1)];
        self.idx += 1;
        v
    }
}

fn reply(text: &str, tokens_used: u64, final_output: bool) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text(text),
        tokens_used,
        final_output,
    }
}

fn kinds(led: &Ledger, run_id: &str) -> Vec<StepKind> {
    led.log(run_id).iter().map(|s| s.kind.clone()).collect()
}

// ---- exit-variant coverage, through real turns (SC-002) ----

#[test]
fn loop_reaches_final_output_through_real_turns() {
    let model = ScriptedModel::new(vec![
        reply("still working", 10, false),
        reply("here is the answer", 10, true),
    ]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    let run = lp
        .run("run-final", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::FinalOutput);
    assert_eq!(lp.client.calls, 2);
    assert_eq!(run.final_message.unwrap().text(), "here is the answer");
    assert_eq!(led.log("run-final").last().unwrap().kind, StepKind::Exit);
    assert_eq!(ctl.iters(), 2);
}

#[test]
fn loop_reaches_max_iters_through_real_turns() {
    let model = ScriptedModel::new(vec![
        reply("turn 1", 1, false),
        reply("turn 2", 1, false),
        reply("turn 3", 1, false),
    ]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(3, 1_000_000, 10));

    let run = lp
        .run("run-iters", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::MaxIters);
    // The engine stopped a model that wanted to keep going (Constitution VIII(a)).
    assert_eq!(lp.client.calls, 3);
    assert_eq!(run.final_message, None);
    assert_eq!(ctl.iters(), 3);
}

#[test]
fn loop_reaches_max_tokens_through_real_turns() {
    let model = ScriptedModel::new(vec![reply("expensive", 60, false)]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 50, 10));

    let run = lp
        .run("run-tokens", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::MaxTokens);
    assert_eq!(lp.client.calls, 1);
    assert_eq!(ctl.tokens(), 60);
    let spent = led
        .log("run-tokens")
        .into_iter()
        .find(|s| s.kind == StepKind::BudgetSpent)
        .expect("the turn's cost is in the ledger");
    assert_eq!(spent.payload, "60");
}

#[test]
fn loop_reaches_no_progress_on_external_ground_truth() {
    // The model's text is upbeat and plausible; only the probe decides.
    let model = ScriptedModel::new(vec![
        reply("great progress!", 1, false),
        reply("almost there!", 1, false),
    ]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![false]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 2));

    let run = lp
        .run("run-stale", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::NoProgress);
    assert_eq!(lp.client.calls, 2);

    // A single true signal resets staleness *through the loop*: without the
    // reset the run would have ended NoProgress on turn 2.
    let model = ScriptedModel::new(vec![
        reply("great progress!", 1, false),
        reply("almost there!", 1, false),
        reply("done", 1, true),
    ]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![false, true, false]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 2));

    let run = lp
        .run("run-reset", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::FinalOutput);
    assert_eq!(lp.client.calls, 3);
}

// ---- ledger fidelity (Constitution V, design §4.11) ----

#[test]
fn every_turn_is_recorded_in_the_hash_chained_ledger() {
    let model = ScriptedModel::new(vec![reply("one", 5, false), reply("two", 5, true)]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    lp.run(
        "run-led",
        Message::user_text("the prompt"),
        &mut led,
        &mut ctl,
    )
    .unwrap();

    led.verify_chain("run-led").unwrap();
    assert_eq!(
        kinds(&led, "run-led"),
        vec![
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::Exit,
        ]
    );

    let log = led.log("run-led");
    let seqs: Vec<u64> = log.iter().map(|s| s.seq).collect();
    assert_eq!(seqs, (0..log.len() as u64).collect::<Vec<_>>());

    // Exact I/O capture, not a lossy summary.
    let first_req_id = log
        .iter()
        .find(|s| s.kind == StepKind::LlmRequest)
        .expect("the request is captured")
        .id
        .clone();
    let req: TurnRequest = serde_json::from_str(&led.show(&first_req_id).unwrap().payload).unwrap();
    assert_eq!(req.run_id, "run-led");
    assert_eq!(req.messages[0].text(), "the prompt");
}

#[test]
fn two_runs_on_one_ledger_stay_isolated() {
    let model = ScriptedModel::new(vec![reply("a", 1, true), reply("b", 1, true)]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();

    let mut ctl_a = LoopController::new(LoopBudget::new(10, 1_000_000, 10));
    lp.run("run-a", Message::user_text("a?"), &mut led, &mut ctl_a)
        .unwrap();
    let mut ctl_b = LoopController::new(LoopBudget::new(10, 1_000_000, 10));
    lp.run("run-b", Message::user_text("b?"), &mut led, &mut ctl_b)
        .unwrap();

    for run_id in ["run-a", "run-b"] {
        let log = led.log(run_id);
        assert_eq!(log.len(), 5, "{run_id}: one turn plus its exit");
        assert!(log.iter().all(|s| s.run_id == run_id));
        led.verify_chain(run_id).unwrap();
    }
}

// ---- loop mechanics ----

#[test]
fn assistant_turn_is_fed_back_into_the_next_request() {
    let model = ScriptedModel::new(vec![
        reply("first reply", 1, false),
        reply("second reply", 1, false),
        reply("last reply", 1, true),
    ]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    lp.run("run-hist", Message::user_text("start"), &mut led, &mut ctl)
        .unwrap();

    let requests: Vec<TurnRequest> = led
        .log("run-hist")
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).unwrap())
        .collect();

    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].messages.len(), 1);
    assert_eq!(requests[1].messages.len(), 2);
    assert_eq!(requests[1].messages[0].text(), "start");
    assert_eq!(
        requests[1].messages[1],
        Message::assistant_text("first reply")
    );
    assert_eq!(requests[2].messages.len(), 3);
    assert_eq!(
        requests[2].messages[2],
        Message::assistant_text("second reply")
    );
}

#[test]
fn exhausted_budget_prevents_any_model_call() {
    let model = ScriptedModel::new(vec![reply("never sent", 1, true)]);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(0, 1_000_000, 10));

    let run = lp
        .run("run-broke", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::MaxIters);
    assert_eq!(
        lp.client.calls, 0,
        "the budget is checked before it is spent"
    );
    assert_eq!(run.final_message, None);
    assert_eq!(kinds(&led, "run-broke"), vec![StepKind::Exit]);
}

// ---- failure containment ----

#[test]
fn provider_error_leaves_the_chain_verifiable() {
    let model = ScriptedModel::failing_at(vec![reply("turn 1 ok", 1, false)], 1);
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]));
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    let err = lp
        .run("run-err", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap_err();

    assert!(matches!(err, SkeinError::Model(_)), "got {err:?}");
    led.verify_chain("run-err").unwrap();
    // The request was captured before the call, so the ledger names what killed the run.
    let log = led.log("run-err");
    assert_eq!(log.last().unwrap().kind, StepKind::LlmRequest);
    let req: TurnRequest = serde_json::from_str(&log.last().unwrap().payload).unwrap();
    assert_eq!(req.messages.len(), 2, "the failed turn carried the history");
}
