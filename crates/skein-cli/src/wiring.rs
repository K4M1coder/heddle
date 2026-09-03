//! What every command that runs the loop needs: the model flags, the loopback
//! guard, the budget, and the two honest stand-ins `NativeLoop` requires.
//!
//! `skein chat` and `skein acp-agent` reach the same provider under the same
//! Principle II rule. A second copy of that rule is a second thing to keep
//! right, and the one it guards — "a local provider is the only thing this can
//! talk to" — is NON-NEGOTIABLE, so there is exactly one.

use clap::Args;
use skein_core::{
    LoopBudget, ProgressProbe, Result, SkeinError, ToolCall, ToolOutcome, ToolTransport,
};
use skein_gateway::{LocalEndpoint, OpenAiCompatClient};
use std::time::Duration;

/// Ollama's own OpenAI-compatible endpoint, which `scripts/bootstrap.ps1
/// -WithOllama` installs. A LiteLLM sidecar is a different `--base-url` and no
/// code change.
const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";

/// Which model, where, and under what budget. Every budget flag maps onto one
/// `LoopBudget` field, so the CLI names the engine's policy and does not invent
/// its own.
#[derive(Args)]
pub struct ModelArgs {
    /// Model name as the local provider knows it. Required: defaulting to a
    /// model the machine may not have produces a 404 that looks like a bug.
    #[arg(long, value_name = "NAME")]
    pub model: String,
    /// OpenAI-compatible base URL. Defaults to $SKEIN_MODEL_BASE_URL, else
    /// http://localhost:11434/v1. Loopback only.
    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,
    #[arg(long, value_name = "N", default_value_t = 8)]
    pub max_iters: u32,
    #[arg(long, value_name = "N", default_value_t = 100_000)]
    pub max_tokens: u64,
    #[arg(long, value_name = "N", default_value_t = 8)]
    pub no_progress_limit: u32,
    /// Whole-request budget for one turn.
    #[arg(long, value_name = "S", default_value_t = 120)]
    pub timeout_secs: u64,
}

impl ModelArgs {
    /// `--base-url`, else `$SKEIN_MODEL_BASE_URL`, else Ollama's default, run
    /// through the loopback guard.
    ///
    /// Call this **before** opening a silo: an endpoint that cannot be built is
    /// an endpoint no socket was opened to, and a chain holding a one-step run
    /// would be a misleading record of an attempt that never left the process.
    pub fn endpoint(&self) -> Result<LocalEndpoint> {
        let base_url = match &self.base_url {
            Some(url) => url.clone(),
            None => {
                std::env::var("SKEIN_MODEL_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.into())
            }
        };
        LocalEndpoint::parse(&base_url)
    }

    pub fn client(&self, endpoint: LocalEndpoint) -> OpenAiCompatClient {
        OpenAiCompatClient::new(
            endpoint,
            &self.model,
            Duration::from_secs(self.timeout_secs),
        )
    }

    pub fn budget(&self) -> LoopBudget {
        LoopBudget::new(self.max_iters, self.max_tokens, self.no_progress_limit)
    }
}

/// A conversation with no tools has **no external ground truth**, and
/// Constitution VIII(b) forbids substituting the model's own judgment for one.
/// So every iteration is stale, and a model that never finishes is stopped by
/// the no-progress budget.
///
/// In the normal case this never bites: `should_exit` checks `final_output`
/// first, and a tool-less turn returns `finish_reason: "stop"` on iteration 1.
pub struct NoGroundTruth;

impl ProgressProbe for NoGroundTruth {
    fn observe(&mut self) -> bool {
        false
    }
}

/// Unreachable by construction: paired with an empty [`ToolPolicy`], every tool
/// name is refused before the transport is consulted. It exists because
/// `NativeLoop` is generic over a transport, not because a tool could run.
///
/// [`ToolPolicy`]: skein_core::ToolPolicy
pub struct NoTools;

impl ToolTransport for NoTools {
    fn call(&mut self, call: &ToolCall) -> Result<ToolOutcome> {
        Err(SkeinError::Tool(format!(
            "no tool server is configured in this command: {} was not called",
            call.tool
        )))
    }
}
