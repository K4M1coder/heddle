//! Acceptance tests for the rmcp transport (spec 005, SC-002). Every governance
//! property is proven end-to-end through the product gateway against a *live*
//! in-process MCP server, so "did the tool actually execute?" is answered by the
//! server's own counter rather than by a double's bookkeeping.
//!
//! These are plain `#[test]`, never `#[tokio::test]`: `RmcpToolTransport` owns a
//! runtime and blocks on it, and `Runtime::block_on` inside a runtime panics.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use serde_json::json;
use skein_core::{
    replay_tool_calls, Ledger, LoopBudget, LoopController, Message, ModelClient, NativeLoop,
    ProgressProbe, Redactor, SkeinError, StepKind, ToolAccess, ToolCall, ToolGateway, ToolPolicy,
    TurnRequest, TurnResponse,
};
use skein_mcp::RmcpToolTransport;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;

const SECRET: &str = "sk-SECRET-abc123";

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct Empty {}

/// The downstream tool holder. Both tools bump a shared counter, which is the
/// ground truth for whether the governor let a call through.
#[derive(Clone)]
struct DownstreamServer {
    invocations: Arc<AtomicUsize>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl DownstreamServer {
    fn new(invocations: Arc<AtomicUsize>) -> Self {
        DownstreamServer {
            invocations,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Read a config value (contains a secret token)")]
    fn read_secret(&self, _p: Parameters<Empty>) -> String {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        format!("config: api_key={SECRET} endpoint=https://x")
    }

    #[tool(description = "Write a file (mutating)")]
    fn fs_write(&self, _p: Parameters<Empty>) -> String {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        "wrote file".to_string()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for DownstreamServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("skein-mcp test downstream")
    }
}

/// The two tools `DownstreamServer` exposes, classified as an operator would.
fn live_server(approved: &[&str]) -> (Runtime, ToolGateway<RmcpToolTransport>, Arc<AtomicUsize>) {
    live_server_allowing(
        &[
            ("read_secret", ToolAccess::ReadOnly),
            ("fs_write", ToolAccess::Mutating),
        ],
        approved,
    )
}

/// A live client↔server pair over an in-process duplex. The returned runtime
/// must be bound to a named local for the whole test body: dropping it kills the
/// task serving the downstream side.
fn live_server_allowing(
    allowed: &[(&str, ToolAccess)],
    approved: &[&str],
) -> (Runtime, ToolGateway<RmcpToolTransport>, Arc<AtomicUsize>) {
    let server_rt = Runtime::new().expect("server runtime");
    let (server_t, client_t) = server_rt.block_on(async { tokio::io::duplex(8192) });
    let invocations = Arc::new(AtomicUsize::new(0));

    let server = DownstreamServer::new(invocations.clone());
    server_rt.spawn(async move {
        if let Ok(s) = server.serve(server_t).await {
            let _ = s.waiting().await;
        }
    });

    let transport = RmcpToolTransport::connect(client_t).expect("client connects to MCP server");
    let gateway = ToolGateway::new(
        transport,
        ToolPolicy::new(
            allowed
                .iter()
                .map(|(name, access)| (name.to_string(), *access))
                .collect(),
            approved.iter().map(|s| s.to_string()).collect(),
        ),
        Redactor::new(vec![SECRET.into()]),
    );
    (server_rt, gateway, invocations)
}

#[test]
fn c1_policy_denial_never_reaches_the_mcp_server() {
    let (_rt, mut gw, invocations) = live_server(&[]);
    let mut led = Ledger::new();

    let err = gw
        .call("run-m1", &ToolCall::new("fs_write", json!({})), &mut led)
        .expect_err("a mutating tool without approval must be denied");

    assert!(matches!(err, SkeinError::ToolDenied { .. }), "got {err:?}");
    assert_eq!(
        invocations.load(Ordering::SeqCst),
        0,
        "the downstream tool must never have executed"
    );
}

#[test]
fn c2_approved_tool_executes_once_downstream() {
    let (_rt, mut gw, invocations) = live_server(&["fs_write"]);
    let mut led = Ledger::new();

    gw.call("run-m2", &ToolCall::new("fs_write", json!({})), &mut led)
        .expect("an approved mutating tool runs");

    assert_eq!(invocations.load(Ordering::SeqCst), 1);
    assert!(led
        .log("run-m2")
        .iter()
        .any(|s| s.kind == StepKind::ToolResult));
}

#[test]
fn c3_secret_is_redacted_before_it_reaches_the_ledger() {
    let (_rt, mut gw, _invocations) = live_server(&[]);
    let mut led = Ledger::new();

    let out = gw
        .call("run-m3", &ToolCall::new("read_secret", json!({})), &mut led)
        .expect("read_secret runs");

    assert!(
        out.content.contains(SECRET),
        "sanity: the live server really returns the secret"
    );
    let payloads: Vec<String> = led
        .log("run-m3")
        .iter()
        .map(|s| s.payload.clone())
        .collect();
    assert!(
        payloads.iter().all(|p| !p.contains(SECRET)),
        "no captured payload may contain the secret: {payloads:?}"
    );
    assert!(payloads.iter().any(|p| p.contains("***")));
}

#[test]
fn c4_replay_makes_no_new_downstream_call() {
    let (_rt, mut gw, invocations) = live_server(&[]);
    let mut led = Ledger::new();

    gw.call("run-m4", &ToolCall::new("read_secret", json!({})), &mut led)
        .expect("read_secret runs");
    let calls_after_live = invocations.load(Ordering::SeqCst);
    assert_eq!(calls_after_live, 1);

    let replayed = replay_tool_calls(&led, "run-m4").expect("replay reads the record");

    assert_eq!(
        invocations.load(Ordering::SeqCst),
        calls_after_live,
        "replay must make no new downstream call"
    );
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].tool, "read_secret");
    let captured = led
        .log("run-m4")
        .iter()
        .find(|s| s.kind == StepKind::ToolResult)
        .expect("the executed call was captured")
        .payload
        .clone();
    assert!(
        captured.contains(&replayed[0].content.replace('"', "\\\"")),
        "the replayed content must be what was captured: {captured}"
    );
    assert!(replayed[0].content.contains("***") && !replayed[0].content.contains(SECRET));
}

#[test]
fn c5_governed_run_verifies_chain() {
    let (_rt, mut gw, _invocations) = live_server(&[]);
    let mut led = Ledger::new();

    gw.call("run-m5", &ToolCall::new("fs_write", json!({})), &mut led)
        .expect_err("denied");
    gw.call("run-m5", &ToolCall::new("read_secret", json!({})), &mut led)
        .expect("read_secret runs");

    led.verify_chain("run-m5")
        .expect("a run holding a denial and an execution still verifies");
    assert_eq!(
        led.log("run-m5")
            .iter()
            .map(|s| s.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            StepKind::ToolCall,
            StepKind::Approval,
            StepKind::ToolCall,
            StepKind::Approval,
            StepKind::ToolResult,
        ]
    );
}

/// A model whose every turn is decided in advance, so the only live thing in the
/// test is the MCP server.
struct ScriptedModel {
    script: Vec<TurnResponse>,
    calls: usize,
}

impl ModelClient for ScriptedModel {
    fn turn(&mut self, _req: &TurnRequest) -> skein_core::Result<TurnResponse> {
        let i = self.calls;
        self.calls += 1;
        Ok(self.script[i].clone())
    }
}

struct ScriptedProbe;

impl ProgressProbe for ScriptedProbe {
    fn observe(&mut self) -> bool {
        true
    }
}

fn scripted_reply(text: &str, final_output: bool, tool_calls: Vec<ToolCall>) -> TurnResponse {
    TurnResponse {
        message: Message::assistant_text(text),
        tokens_used: 1,
        final_output,
        tool_calls,
    }
}

#[test]
fn c6_native_loop_calls_a_live_mcp_tool_mid_run() {
    let (_rt, gateway, invocations) = live_server(&[]);
    let model = ScriptedModel {
        script: vec![
            scripted_reply(
                "reading config",
                false,
                vec![ToolCall::new("read_secret", json!({}))],
            ),
            scripted_reply("done", true, Vec::new()),
        ],
        calls: 0,
    };
    let mut lp = NativeLoop::new(model, ScriptedProbe, gateway);
    let mut led = Ledger::new();
    let mut ctl = LoopController::new(LoopBudget::new(10, 1_000_000, 10));

    lp.run("run-m6", Message::user_text("go"), &mut led, &mut ctl)
        .expect("the loop drives a live tool call");

    assert_eq!(
        invocations.load(Ordering::SeqCst),
        1,
        "the server itself is the ground truth for whether the tool ran"
    );

    let requests: Vec<TurnRequest> = led
        .log("run-m6")
        .into_iter()
        .filter(|s| s.kind == StepKind::LlmRequest)
        .map(|s| serde_json::from_str(&s.payload).unwrap())
        .collect();
    let fed_back = requests[1].messages[2].text();
    assert!(
        fed_back.starts_with("[tool_result tool=read_secret status=ok]"),
        "{fed_back}"
    );
    assert!(
        fed_back.contains("endpoint=https://x") && fed_back.contains("***"),
        "the real server output reaches the model, redacted: {fed_back}"
    );
    assert!(!fed_back.contains(SECRET));
}

#[test]
fn c7_unlisted_tool_never_reaches_the_live_server() {
    let (_rt, mut gw, invocations) =
        live_server_allowing(&[("fs_write", ToolAccess::Mutating)], &[]);
    let mut led = Ledger::new();

    let err = gw
        .call("run-m7", &ToolCall::new("read_secret", json!({})), &mut led)
        .expect_err("a tool the operator never allowlisted must be denied");

    assert!(matches!(err, SkeinError::ToolDenied { .. }), "got {err:?}");
    assert_eq!(
        invocations.load(Ordering::SeqCst),
        0,
        "the server really implements read_secret, so only the allowlist stopped it"
    );
}
