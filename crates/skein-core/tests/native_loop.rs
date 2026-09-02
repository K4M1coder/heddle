//! Acceptance tests for the native turn loop (specs 004 and 006).
//! Every exit is driven through real wiring: a `NativeLoop` calling a scripted
//! `ModelClient`, mediating the tools that model asks for through a real
//! `ToolGateway`, and writing into a real hash-chained `Ledger`.

use serde_json::json;
use skein_core::{
    Exit, Ledger, LoopBudget, LoopController, Message, ModelClient, NativeLoop, ProgressProbe,
    Redactor, Result, Role, SkeinError, StepKind, ToolCall, ToolGateway, ToolOutcome, ToolPolicy,
    ToolTransport, TurnRequest, TurnResponse,
};

const SECRET: &str = "sk-SECRET-abc123";

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

/// What a transport did, and what it was asked to do. `Forbidden` is the point
/// of the tool-free fixture: a script that names no tool must never reach one.
enum TransportMode {
    Reply(String),
    Fail,
    Forbidden,
}

struct RecordingTransport {
    calls: usize,
    seen: Vec<ToolCall>,
    mode: TransportMode,
}

impl RecordingTransport {
    fn new(reply: &str) -> Self {
        RecordingTransport {
            calls: 0,
            seen: Vec::new(),
            mode: TransportMode::Reply(reply.to_string()),
        }
    }

    fn failing() -> Self {
        RecordingTransport {
            mode: TransportMode::Fail,
            ..RecordingTransport::new("")
        }
    }

    fn forbidden() -> Self {
        RecordingTransport {
            mode: TransportMode::Forbidden,
            ..RecordingTransport::new("")
        }
    }
}

impl ToolTransport for RecordingTransport {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        self.calls += 1;
        self.seen.push(call.clone());
        match &self.mode {
            TransportMode::Reply(reply) => Ok(ToolOutcome {
                content: reply.clone(),
            }),
            TransportMode::Fail => Err(SkeinError::Tool("downstream exploded".into())),
            TransportMode::Forbidden => panic!("a tool-free script must never reach a transport"),
        }
    }
}

fn gateway(transport: RecordingTransport, approved: &[&str]) -> ToolGateway<RecordingTransport> {
    ToolGateway::new(
        transport,
        ToolPolicy::new(
            vec!["fs_write".into()],
            approved.iter().map(|s| s.to_string()).collect(),
        ),
        Redactor::new(vec![SECRET.into()]),
    )
}

/// The gateway the tool-free tests of spec 004 carry: it governs nothing because
/// nothing ever asks it to, and it proves that by exploding if it is asked.
fn no_tools() -> ToolGateway<RecordingTransport> {
    ToolGateway::new(
        RecordingTransport::forbidden(),
        ToolPolicy::new(Vec::new(), Vec::new()),
        Redactor::new(Vec::new()),
    )
}

fn reply(text: &str, tokens_used: u64, final_output: bool) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text(text),
        tokens_used,
        final_output,
        tool_calls: Vec::new(),
    }
}

fn reply_with_tools(
    text: &str,
    tokens_used: u64,
    final_output: bool,
    tool_calls: Vec<ToolCall>,
) -> TurnResponse {
    TurnResponse {
        tool_calls,
        ..reply(text, tokens_used, final_output)
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![false]), no_tools());
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
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![false, true, false]),
        no_tools(),
    );
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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
    let mut lp = NativeLoop::new(model, ScriptedProbe::new(vec![true]), no_tools());
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

// ---- mid-loop tool mediation (spec 006) ----

#[test]
fn model_requested_tool_reaches_the_gateway_and_its_transport() {
    let model = ScriptedModel::new(vec![
        reply_with_tools(
            "let me look",
            1,
            false,
            vec![ToolCall::new("read_file", json!({ "path": "a" }))],
        ),
        reply("here is the answer", 1, true),
    ]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::new("file contents"), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    let run = lp
        .run("run-tool", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::FinalOutput);
    assert_eq!(lp.gateway.transport.calls, 1);
    assert_eq!(lp.gateway.transport.seen[0].tool, "read_file");
    assert_eq!(
        lp.gateway.transport.seen[0].args,
        json!({ "path": "a" }),
        "the transport receives the raw arguments the model asked for"
    );
}

#[test]
fn tool_result_is_fed_back_as_data_into_the_next_request() {
    let model = ScriptedModel::new(vec![
        reply_with_tools(
            "checking",
            1,
            false,
            vec![ToolCall::new("read_file", json!({}))],
        ),
        reply("done", 1, true),
    ]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::new("what the tool itself said"), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    lp.run("run-feed", Message::user_text("start"), &mut led, &mut ctl)
        .unwrap();

    let requests: Vec<TurnRequest> = led
        .log("run-feed")
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).unwrap())
        .collect();

    assert_eq!(requests[1].messages.len(), 3);
    assert_eq!(requests[1].messages[0].text(), "start");
    assert_eq!(requests[1].messages[1], Message::assistant_text("checking"));

    let fed_back = &requests[1].messages[2];
    assert_eq!(
        fed_back.role,
        Role::User,
        "tool output is external data: not a system instruction, and not the words of the model"
    );
    assert!(
        fed_back
            .text()
            .starts_with("[tool_result tool=read_file status=ok]"),
        "the tool result is labelled as tool data: {}",
        fed_back.text()
    );
    assert!(fed_back.text().contains("what the tool itself said"));
}

#[test]
fn denied_tool_does_not_crash_the_loop_and_is_on_the_ledger() {
    let model = ScriptedModel::new(vec![
        reply_with_tools(
            "writing",
            1,
            false,
            vec![ToolCall::new("fs_write", json!({ "path": "a" }))],
        ),
        reply("fine, no write then", 1, true),
    ]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::new("wrote file"), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    let run = lp
        .run("run-deny", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::FinalOutput, "a refusal is not a failed run");
    assert_eq!(lp.gateway.transport.calls, 0);
    let k = kinds(&led, "run-deny");
    assert!(k.contains(&StepKind::ToolCall) && k.contains(&StepKind::Approval));
    assert!(
        !k.contains(&StepKind::ToolResult),
        "a refused call produced no result to capture"
    );
    let approval = led
        .log("run-deny")
        .into_iter()
        .find(|s| s.kind == StepKind::Approval)
        .expect("the refusal is on the record");
    assert!(approval.payload.contains("denied"), "{}", approval.payload);

    let requests: Vec<TurnRequest> = led
        .log("run-deny")
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).unwrap())
        .collect();
    let notice = requests[1].messages[2].text();
    assert!(
        notice.starts_with("[tool_result tool=fs_write status=denied]"),
        "the model is told plainly that the tool was refused: {notice}"
    );
}

#[test]
fn a_tool_call_does_not_buy_an_extra_iteration() {
    let model = ScriptedModel::new(vec![reply_with_tools(
        "one turn only",
        1,
        false,
        vec![ToolCall::new("read_file", json!({}))],
    )]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::new("contents"), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(1, 1_000_000, 10));

    let run = lp
        .run("run-budget", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(run.exit, Exit::MaxIters);
    assert_eq!(lp.client.calls, 1);
    assert_eq!(ctl.iters(), 1);
    assert_eq!(lp.gateway.transport.calls, 1);
    assert_eq!(led.log("run-budget").last().unwrap().kind, StepKind::Exit);

    let model = ScriptedModel::new(vec![reply_with_tools(
        "never sent",
        1,
        false,
        vec![ToolCall::new("read_file", json!({}))],
    )]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::new("contents"), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(0, 1_000_000, 10));

    lp.run("run-nobudget", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(lp.client.calls, 0);
    assert_eq!(
        lp.gateway.transport.calls, 0,
        "an exhausted budget spends no tool call either"
    );
}

#[test]
fn interleaved_turn_and_tool_steps_verify_on_one_chain() {
    let model = ScriptedModel::new(vec![
        reply_with_tools(
            "looking",
            1,
            false,
            vec![ToolCall::new("read_file", json!({}))],
        ),
        reply("done", 1, true),
    ]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::new("contents"), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    lp.run("run-chain", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    led.verify_chain("run-chain")
        .expect("turn steps and tool steps share one chain");
    assert_eq!(
        kinds(&led, "run-chain"),
        vec![
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::ToolCall,
            StepKind::Approval,
            StepKind::ToolResult,
            StepKind::IterationBoundary,
            StepKind::LlmRequest,
            StepKind::LlmResponse,
            StepKind::BudgetSpent,
            StepKind::Exit,
        ]
    );
    let log = led.log("run-chain");
    let seqs: Vec<u64> = log.iter().map(|s| s.seq).collect();
    assert_eq!(seqs, (0..log.len() as u64).collect::<Vec<_>>());
}

#[test]
fn two_tool_calls_in_one_turn_run_in_declaration_order() {
    let model = ScriptedModel::new(vec![
        reply_with_tools(
            "two things",
            1,
            false,
            vec![
                ToolCall::new("read_first", json!({})),
                ToolCall::new("read_second", json!({})),
            ],
        ),
        reply("done", 1, true),
    ]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::new("contents"), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    lp.run("run-order", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    assert_eq!(
        lp.gateway
            .transport
            .seen
            .iter()
            .map(|c| c.tool.as_str())
            .collect::<Vec<_>>(),
        vec!["read_first", "read_second"]
    );

    let captured: Vec<String> = led
        .log("run-order")
        .into_iter()
        .filter(|s| s.kind == StepKind::ToolResult)
        .map(|s| s.payload.clone())
        .collect();
    assert_eq!(captured.len(), 2);
    assert!(captured[0].contains("read_first") && captured[1].contains("read_second"));

    let requests: Vec<TurnRequest> = led
        .log("run-order")
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).unwrap())
        .collect();
    assert_eq!(requests[1].messages.len(), 4);
    assert!(requests[1].messages[2].text().contains("read_first"));
    assert!(requests[1].messages[3].text().contains("read_second"));
}

#[test]
fn a_secret_from_a_tool_never_enters_the_chain_through_the_history() {
    let model = ScriptedModel::new(vec![
        reply_with_tools(
            "reading config",
            1,
            false,
            vec![ToolCall::new("read_secret", json!({}))],
        ),
        reply("done", 1, true),
    ]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(
            RecordingTransport::new(&format!("config: api_key={SECRET}")),
            &[],
        ),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    lp.run("run-secret", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap();

    // The second LlmRequest carries the whole history, so this is the assertion
    // that fails if the loop feeds back the raw outcome instead of the capture.
    let payloads: Vec<String> = led
        .log("run-secret")
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET)),
        "no payload of the run may contain the secret: {payloads:?}"
    );
    assert!(payloads.iter().any(|p| p.contains("***")));

    let requests: Vec<TurnRequest> = led
        .log("run-secret")
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).unwrap())
        .collect();
    assert!(requests[1].messages[2].text().contains("***"));
}

#[test]
fn tool_transport_failure_propagates_and_leaves_the_chain_verifiable() {
    let model = ScriptedModel::new(vec![reply_with_tools(
        "trying",
        1,
        false,
        vec![ToolCall::new("read_file", json!({}))],
    )]);
    let mut lp = NativeLoop::new(
        model,
        ScriptedProbe::new(vec![true]),
        gateway(RecordingTransport::failing(), &[]),
    );
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    let err = lp
        .run("run-toolerr", Message::user_text("go"), &mut led, &mut ctl)
        .unwrap_err();

    assert!(matches!(err, SkeinError::Tool(_)), "got {err:?}");
    assert!(
        !kinds(&led, "run-toolerr").contains(&StepKind::ToolResult),
        "no ToolResult may be fabricated for a call that produced none"
    );
    led.verify_chain("run-toolerr").expect("chain verifies");
}
