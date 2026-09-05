//! SPIKE Option A — native Heddle-owned agent loop (quarantined, throwaway).
//! Proves the five pre-registered criteria of docs/superpowers/spikes/spike-protocol.md
//! against an OpenAI-compatible endpoint: exact I/O capture, tool interception,
//! external termination, run correlation. MCP wiring (rmcp) is a follow-up probe;
//! the in-process tool proves the mediation point exists in a loop we own.

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

/// Ledger-shaped event log entry (criterion 4: every event carries run_id + seq).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Criterion 1: the byte-exact JSON payload sent to the model.
    LlmRequest { run_id: String, seq: u32, payload: Value },
    /// Criterion 1: the raw, unparsed response body.
    LlmResponse { run_id: String, seq: u32, raw: String },
    /// Criterion 2: emitted BEFORE the tool executes (mediation point).
    ToolIntercepted { run_id: String, seq: u32, name: String, args: Value },
    ToolExecuted { run_id: String, seq: u32, name: String, result: Value },
    ToolDenied { run_id: String, seq: u32, name: String },
    /// Criterion 3: loop stopped by the harness, process still alive.
    Terminated { run_id: String, seq: u32, reason: String },
    Done { run_id: String, seq: u32, final_text: String },
}

impl Event {
    pub fn run_id(&self) -> &str {
        match self {
            Event::LlmRequest { run_id, .. }
            | Event::LlmResponse { run_id, .. }
            | Event::ToolIntercepted { run_id, .. }
            | Event::ToolExecuted { run_id, .. }
            | Event::ToolDenied { run_id, .. }
            | Event::Terminated { run_id, .. }
            | Event::Done { run_id, .. } => run_id,
        }
    }
    pub fn seq(&self) -> u32 {
        match self {
            Event::LlmRequest { seq, .. }
            | Event::LlmResponse { seq, .. }
            | Event::ToolIntercepted { seq, .. }
            | Event::ToolExecuted { seq, .. }
            | Event::ToolDenied { seq, .. }
            | Event::Terminated { seq, .. }
            | Event::Done { seq, .. } => *seq,
        }
    }
}

/// Policy verdict returned by the mediator BEFORE a tool runs (criterion 2).
pub enum Verdict {
    Allow,
    Deny,
}

pub type Mediator = Box<dyn Fn(&str, &Value) -> Verdict + Send + Sync>;

#[derive(Debug, PartialEq)]
pub enum Exit {
    FinalOutput,
    Cancelled,
    MaxTurns,
    Error(String),
}

pub struct LoopOutcome {
    pub exit: Exit,
    pub events: Vec<Event>,
}

struct EventLog {
    run_id: String,
    seq: u32,
    events: Vec<Event>,
}

impl EventLog {
    fn new(run_id: &str) -> Self {
        EventLog { run_id: run_id.to_string(), seq: 0, events: Vec::new() }
    }
    fn push(&mut self, f: impl FnOnce(String, u32) -> Event) {
        let ev = f(self.run_id.clone(), self.seq);
        self.seq += 1;
        self.events.push(ev);
    }
}

/// The only tool in the spike: echoes its arguments (in-process; the mediation
/// point in front of it is what matters, not the tool itself).
fn echo_tool(args: &Value) -> Value {
    json!({ "echoed": args })
}

/// Run a Heddle-owned agent loop against an OpenAI-compatible `endpoint`.
pub async fn run_loop(
    endpoint: &str,
    user_prompt: &str,
    run_id: &str,
    max_turns: u32,
    mediator: Mediator,
    cancel: CancellationToken,
) -> LoopOutcome {
    let client = reqwest::Client::new();
    let mut log = EventLog::new(run_id);
    let mut messages = vec![json!({ "role": "user", "content": user_prompt })];

    for _turn in 0..max_turns {
        let payload = json!({
            "model": "spike-model",
            "messages": messages,
            "tools": [{
                "type": "function",
                "function": { "name": "echo", "description": "echo args back", "parameters": { "type": "object" } }
            }]
        });

        // Criterion 1 (request side): capture the exact payload before sending.
        log.push(|run_id, seq| Event::LlmRequest { run_id, seq, payload: payload.clone() });

        // Criterion 3: the harness owns termination — the model call is raced
        // against an external cancellation token, mid-turn.
        let send = client.post(format!("{endpoint}/chat/completions")).json(&payload).send();
        let resp = tokio::select! {
            r = send => r,
            _ = cancel.cancelled() => {
                log.push(|run_id, seq| Event::Terminated { run_id, seq, reason: "external-cancel".into() });
                return LoopOutcome { exit: Exit::Cancelled, events: log.events };
            }
        };

        let raw = match resp {
            Ok(r) => match r.text().await {
                Ok(t) => t,
                Err(e) => return LoopOutcome { exit: Exit::Error(e.to_string()), events: log.events },
            },
            Err(e) => return LoopOutcome { exit: Exit::Error(e.to_string()), events: log.events },
        };

        // Criterion 1 (response side): capture the raw body before parsing.
        log.push(|run_id, seq| Event::LlmResponse { run_id, seq, raw: raw.clone() });

        let parsed: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => return LoopOutcome { exit: Exit::Error(format!("bad json: {e}")), events: log.events },
        };
        let message = &parsed["choices"][0]["message"];

        if let Some(tool_calls) = message["tool_calls"].as_array() {
            // Echo the assistant turn back into history (OpenAI convention).
            messages.push(message.clone());
            for tc in tool_calls {
                let name = tc["function"]["name"].as_str().unwrap_or("?").to_string();
                let args: Value = serde_json::from_str(tc["function"]["arguments"].as_str().unwrap_or("{}"))
                    .unwrap_or(json!({}));
                let call_id = tc["id"].as_str().unwrap_or("call_0").to_string();

                // Criterion 2: interception BEFORE execution — the mediator decides.
                log.push(|run_id, seq| Event::ToolIntercepted {
                    run_id, seq, name: name.clone(), args: args.clone(),
                });
                match mediator(&name, &args) {
                    Verdict::Allow => {
                        let result = echo_tool(&args);
                        log.push(|run_id, seq| Event::ToolExecuted {
                            run_id, seq, name: name.clone(), result: result.clone(),
                        });
                        messages.push(json!({
                            "role": "tool", "tool_call_id": call_id, "content": result.to_string()
                        }));
                    }
                    Verdict::Deny => {
                        log.push(|run_id, seq| Event::ToolDenied { run_id, seq, name: name.clone() });
                        messages.push(json!({
                            "role": "tool", "tool_call_id": call_id, "content": "{\"error\":\"denied by policy\"}"
                        }));
                    }
                }
            }
            continue; // next turn
        }

        // No tool calls → final output.
        let final_text = message["content"].as_str().unwrap_or("").to_string();
        log.push(|run_id, seq| Event::Done { run_id, seq, final_text: final_text.clone() });
        return LoopOutcome { exit: Exit::FinalOutput, events: log.events };
    }

    LoopOutcome { exit: Exit::MaxTurns, events: log.events }
}
