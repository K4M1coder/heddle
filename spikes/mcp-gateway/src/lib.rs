//! SPIKE 4 — MCP tool governance (quarantined per ADR-0004 D2, throwaway).
//! Proves Skein can proxy a REAL rmcp MCP server through: (C2) policy/approval,
//! (redaction) secret scrubbing before capture, (capture) Ledger-shaped record,
//! (replay) answer from the record without re-invoking the downstream tool.
//! Ground truth = the four governance tests in tests/gateway.rs.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolRequestParam, CallToolResult, ServerCapabilities, ServerInfo};
use rmcp::service::{RoleClient, RunningService};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ---------- Downstream MCP server (the "real" tool holder) ----------

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Empty {}

#[derive(Clone)]
pub struct DownstreamServer {
    /// Shared counter: proves whether a tool actually executed downstream.
    pub invocations: Arc<AtomicUsize>,
    tool_router: ToolRouter<Self>,
}

impl DownstreamServer {
    pub fn new(invocations: Arc<AtomicUsize>) -> Self {
        Self { invocations, tool_router: Self::tool_router() }
    }
}

#[tool_router]
impl DownstreamServer {
    /// Returns content that embeds a secret — the gateway must redact it on capture.
    #[tool(description = "Read a config value (contains a secret token)")]
    fn read_secret(&self, _p: Parameters<Empty>) -> String {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        "config: api_key=sk-SECRET-abc123 endpoint=https://x".to_string()
    }

    /// A mutating tool the gateway policy denies unless explicitly approved.
    #[tool(description = "Write a file (mutating)")]
    fn fs_write(&self, _p: Parameters<Empty>) -> String {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        "wrote file".to_string()
    }
}

#[tool_handler]
impl ServerHandler for DownstreamServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("spike downstream")
    }
}

// ---------- Skein-side governed gateway ----------

#[derive(Debug, Clone, PartialEq)]
pub enum GatewayEvent {
    Denied { run_id: String, seq: u32, tool: String, reason: String },
    Executed { run_id: String, seq: u32, tool: String, redacted_result: String },
}

pub struct GatewayConfig {
    /// Tools that require explicit approval before running (deny-by-default for these).
    pub mutating: Vec<String>,
    /// Approvals granted this run (tool names).
    pub approved: Vec<String>,
    /// Literal secrets to redact from any captured output.
    pub secrets: Vec<String>,
}

pub struct Gateway {
    client: RunningService<RoleClient, ()>,
    cfg: GatewayConfig,
    run_id: String,
    seq: u32,
    ledger: Vec<GatewayEvent>,
}

impl Gateway {
    pub fn new(client: RunningService<RoleClient, ()>, cfg: GatewayConfig, run_id: &str) -> Self {
        Self { client, cfg, run_id: run_id.to_string(), seq: 0, ledger: Vec::new() }
    }

    pub fn ledger(&self) -> &[GatewayEvent] {
        &self.ledger
    }

    fn redact(&self, s: &str) -> String {
        let mut out = s.to_string();
        for secret in &self.cfg.secrets {
            if !secret.is_empty() {
                out = out.replace(secret, "***");
            }
        }
        out
    }

    /// The governed path: policy → (approval) → downstream call → redact → capture.
    pub async fn call(&mut self, tool: &str) -> Result<CallToolResult, String> {
        let needs_approval = self.cfg.mutating.iter().any(|m| m == tool);
        let approved = self.cfg.approved.iter().any(|a| a == tool);

        if needs_approval && !approved {
            let seq = self.seq;
            self.seq += 1;
            self.ledger.push(GatewayEvent::Denied {
                run_id: self.run_id.clone(),
                seq,
                tool: tool.to_string(),
                reason: "requires approval".into(),
            });
            return Err("denied by policy".into());
        }

        let result = self
            .client
            .call_tool(CallToolRequestParam::new(tool.to_string()))
            .await
            .map_err(|e| e.to_string())?;

        let raw = serde_json::to_string(&result.content).unwrap_or_default();
        let redacted = self.redact(&raw);
        let seq = self.seq;
        self.seq += 1;
        self.ledger.push(GatewayEvent::Executed {
            run_id: self.run_id.clone(),
            seq,
            tool: tool.to_string(),
            redacted_result: redacted,
        });
        Ok(result)
    }

    /// Replay: reconstruct the recorded (redacted) outputs WITHOUT touching the
    /// downstream server — proves the capture is a durable, replayable record.
    pub fn replay(&self) -> Vec<String> {
        self.ledger
            .iter()
            .filter_map(|e| match e {
                GatewayEvent::Executed { redacted_result, .. } => Some(redacted_result.clone()),
                _ => None,
            })
            .collect()
    }
}
