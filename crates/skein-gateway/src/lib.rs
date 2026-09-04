//! The Model Gateway (design §4.5, Constitution IV): the **only** crate in the
//! product that names HTTP or the OpenAI chat-completions wire format, the way
//! `skein-mcp` is the only one naming MCP and `skein-acp` the only one naming
//! ACP. `skein-core` does not depend on it — it discovers a provider through
//! `ModelClient` and never learns that a socket is involved.
//!
//! Two properties are structural rather than reviewed:
//!
//! - **`ureq` is declared with no default features**, so no TLS backend is
//!   compiled in and an `https://` endpoint fails at the transport with
//!   `ureq::Error::TlsRequired`. Every cloud provider endpoint is HTTPS, so
//!   "local providers only" (Constitution II, NON-NEGOTIABLE) is a property of
//!   the build.
//! - **[`OpenAiCompatClient`] cannot be constructed without a
//!   [`LocalEndpoint`]**, and `LocalEndpoint::parse` refuses anything but
//!   loopback before a socket is opened.
//!
//! The client is synchronous, matching `ModelClient::turn`. `skein-acp` calls
//! `turn` from a spawned OS thread inside an async program, where a
//! `block_on`-based client would panic; a blocking one cannot.

use serde::{Deserialize, Serialize};
use skein_core::{
    Message, ModelClient, Result, SkeinError, ToolCall, ToolSpec, TurnRequest, TurnResponse,
};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

/// Connect budget, separate from the global one so a wrong port fails fast even
/// when the operator has allowed a long generation.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How much of a provider's error body reaches the operator's terminal. Enough
/// for Ollama's `{"error":{"message":…}}`, short of pasting a whole HTML page.
const ERROR_BODY_CHARS: usize = 400;

/// The port a loopback base URL is assumed to use when it names none, for the
/// resolution check only. `http::Uri::port_u16` returns `None` for
/// `http://localhost/v1`.
const DEFAULT_HTTP_PORT: u16 = 80;

/// A base URL that has been proved to address this machine.
///
/// The proof happens here, at construction, and not at request time: an
/// endpoint that cannot be built is an endpoint no socket was ever opened to.
/// [`OpenAiCompatClient`] takes one by value and has no other constructor, so
/// there is no path from a string to a request that skips this check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEndpoint {
    base_url: String,
}

impl LocalEndpoint {
    /// Accepts `http://<loopback-ip-or-localhost>[:port][/path]` and nothing
    /// else (ADR-0002 D4: Local mode enforces a loopback allowlist).
    ///
    /// A host name other than the literal `localhost` is refused **without
    /// being resolved**: resolving it would itself be egress, because the name
    /// would leave this machine in a DNS query. `localhost` *is* resolved,
    /// because a `hosts` entry can point it anywhere, and every address it
    /// resolves to must be loopback.
    ///
    /// Residual, recorded in `specs/012-model-gateway/spec.md`: `ureq`
    /// re-resolves at request time, so a hostile resolver could answer
    /// differently between this check and the connection. Closing that needs
    /// ADR-0002 D4's process-level socket-deny boundary, which this workspace
    /// does not have; this is the policy layer above it.
    pub fn parse(base_url: &str) -> Result<LocalEndpoint> {
        let refuse = |why: String| -> SkeinError {
            SkeinError::Model(format!(
                "base URL {base_url:?} is not a local provider: {why}"
            ))
        };

        let uri: http::Uri = base_url
            .parse()
            .map_err(|e| refuse(format!("not a URL: {e}")))?;

        match uri.scheme_str() {
            Some("http") => {}
            Some(scheme) => {
                return Err(refuse(format!(
                    "scheme {scheme:?} is refused; Skein v0 talks to local providers over http \
                     only, and no TLS backend is compiled in"
                )))
            }
            None => return Err(refuse("no scheme; expected http://…".into())),
        }

        let host = uri
            .host()
            .ok_or_else(|| refuse("no host".into()))?
            // `Uri::host` keeps the brackets of an IPv6 literal.
            .trim_start_matches('[')
            .trim_end_matches(']');

        match host.parse::<IpAddr>() {
            Ok(ip) if ip.is_loopback() => {}
            Ok(ip) => {
                return Err(refuse(format!(
                    "{ip} is not a loopback address; reaching a provider off this machine needs \
                     the egress policy layer, which does not exist yet"
                )))
            }
            Err(_) if host.eq_ignore_ascii_case("localhost") => {
                let port = uri.port_u16().unwrap_or(DEFAULT_HTTP_PORT);
                let addrs: Vec<SocketAddr> = (host, port)
                    .to_socket_addrs()
                    .map_err(|e| refuse(format!("localhost does not resolve: {e}")))?
                    .collect();
                if addrs.is_empty() {
                    return Err(refuse("localhost resolves to no address".into()));
                }
                if let Some(off) = addrs.iter().find(|a| !a.ip().is_loopback()) {
                    return Err(refuse(format!(
                        "localhost resolves to {off}, which is not loopback"
                    )));
                }
            }
            Err(_) => {
                return Err(refuse(format!(
                    "host name {host:?} is refused without being resolved, because the query \
                     would itself leave this machine; use a loopback address or localhost"
                )))
            }
        }

        Ok(LocalEndpoint {
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// The base, with any trailing slash removed. Whatever path prefix the
    /// operator gave is kept: `http://localhost:11434/v1` posts to
    /// `/v1/chat/completions`.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

/// A `ModelClient` speaking OpenAI chat-completions to a local provider.
///
/// Ollama's own endpoint is OpenAI-compatible, so this reaches it directly; a
/// LiteLLM sidecar is a different `--base-url` and no code change.
pub struct OpenAiCompatClient {
    endpoint: LocalEndpoint,
    model: String,
    agent: ureq::Agent,
}

impl OpenAiCompatClient {
    /// `timeout` is the whole-request budget, and is required rather than
    /// defaulted: the failure it prevents — a provider that accepts a request
    /// and never answers — is the one an operator cannot diagnose, so the
    /// caller states its own number.
    ///
    /// `http_status_as_error(false)` is what lets a provider's own error body
    /// reach the operator instead of being flattened into a status code.
    pub fn new(endpoint: LocalEndpoint, model: impl Into<String>, timeout: Duration) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(timeout))
            .build()
            .into();
        OpenAiCompatClient {
            endpoint,
            model: model.into(),
            agent,
        }
    }

    fn post(&self, body: &str) -> Result<(u16, String)> {
        let url = self.endpoint.chat_completions_url();
        let mut response = self
            .agent
            .post(&url)
            .header("content-type", "application/json")
            .send(body)
            .map_err(|e| {
                SkeinError::Model(format!(
                    "POST {url} failed: {e}; is a local provider listening at {}?",
                    self.endpoint.base_url
                ))
            })?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().map_err(|e| {
            SkeinError::Model(format!("POST {url} returned an unreadable body: {e}"))
        })?;
        Ok((status, text))
    }

    fn unrecognised(&self, detail: impl std::fmt::Display) -> SkeinError {
        SkeinError::Model(format!(
            "{} returned an unrecognised chat-completions response: {detail}",
            self.endpoint.base_url
        ))
    }

    /// Real provider metering, or a refusal.
    ///
    /// A `0` fallback is the tempting third option and is forbidden:
    /// `LoopController::should_exit` stops on `tokens >= max_tokens`, so a
    /// silent zero would disable the token budget while looking like it worked
    /// (Constitution VIII, NON-NEGOTIABLE).
    fn metered(&self, usage: Option<Usage>) -> Result<u64> {
        let missing = || {
            SkeinError::Model(format!(
                "{} answered without token metering (no usage.total_tokens and no \
                 usage.prompt_tokens + usage.completion_tokens); the loop's token budget cannot \
                 be enforced against a fabricated count",
                self.endpoint.base_url
            ))
        };
        let usage = usage.ok_or_else(missing)?;
        if let Some(total) = usage.total_tokens {
            return Ok(total);
        }
        match (usage.prompt_tokens, usage.completion_tokens) {
            (Some(prompt), Some(completion)) => Ok(prompt + completion),
            _ => Err(missing()),
        }
    }
}

impl ModelClient for OpenAiCompatClient {
    fn turn(&mut self, req: &TurnRequest) -> Result<TurnResponse> {
        let body = serde_json::to_string(&ChatRequest {
            model: &self.model,
            messages: req.messages.iter().map(ChatMessage::from).collect(),
            stream: false,
            tools: req.tools.iter().map(ChatTool::from).collect(),
        })?;

        let (status, text) = self.post(&body)?;
        if !(200..300).contains(&status) {
            return Err(SkeinError::Model(format!(
                "{} returned {status}: {}",
                self.endpoint.base_url,
                truncated(&text)
            )));
        }

        let parsed: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| self.unrecognised(format!("{e}: {}", truncated(&text))))?;
        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| self.unrecognised("no choices[0]"))?;

        let tool_calls = choice
            .message
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                // Normalized here and nowhere else, so every `ToolCall` leaving
                // this crate has a non-empty id and the loop that echoes them
                // needs no fallback of its own. Ollama supplies ids; the
                // OpenAI-compat ecosystem does not guarantee one, and an empty
                // id reaching the echo would produce a request answering a call
                // it never made.
                let id = c.id.unwrap_or_else(|| format!("call_{i}"));
                serde_json::from_str(&c.function.arguments)
                    .map(|args| ToolCall::with_id(id, c.function.name, args))
                    .map_err(|e| {
                        self.unrecognised(format!(
                            "tool call arguments are not JSON: {e}: {}",
                            truncated(&c.function.arguments)
                        ))
                    })
            })
            .collect::<Result<Vec<ToolCall>>>()?;

        Ok(TurnResponse {
            message: Message::assistant_text(choice.message.content.unwrap_or_default()),
            tokens_used: self.metered(parsed.usage)?,
            // The provider's `"stop"` is a *claim* that the model is done, and
            // it is only believed when the model asked for nothing further.
            // `"length"` is deliberately not final: it means the provider
            // truncated the model mid-thought, and treating that as a completed
            // answer would let a truncation launder itself past
            // `LoopController`, which Constitution VIII(a) reserves to the
            // engine.
            final_output: choice.finish_reason.as_deref() == Some("stop") && tool_calls.is_empty(),
            tool_calls,
        })
    }
}

fn truncated(text: &str) -> String {
    match text.char_indices().nth(ERROR_BODY_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// The request, as a struct rather than a `json!` literal so that **field order
/// is the wire order**: `serde_json`'s `Map` is a `BTreeMap` here, which would
/// sort a literal's keys alphabetically. The bytes are ours, and the tests
/// assert them.
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    /// Explicit, not implied: a provider that defaulted to SSE would break the
    /// parse silently, and streaming is out of scope for this slice.
    stream: bool,
    /// Skipped when empty, so a run that advertises nothing puts exactly the
    /// bytes on the wire it put there before this field existed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatTool<'a>>,
}

/// The two tool fields serialize **last** and are skipped when empty, so a
/// message that involves no tool puts exactly the bytes on the wire it put
/// there before they existed.
#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
    /// What the assistant asked for on this turn, echoed so the ids the
    /// following `tool` messages name have something to answer.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ChatToolCall<'a>>,
    /// Which call a `tool` message answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
}

/// The request-side mirror of [`ResponseToolCall`]: the same envelope, sent
/// back so a provider sees the turn it produced.
#[derive(Serialize)]
struct ChatToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatCallFunction<'a>,
}

#[derive(Serialize)]
struct ChatCallFunction<'a> {
    name: &'a str,
    /// A JSON *string* holding JSON, per the wire format, exactly as
    /// [`ToolFunction::arguments`] arrives.
    arguments: String,
}

/// OpenAI's function-tool envelope. `type` is the wire's discriminator and
/// `"function"` is the only kind v0 sends.
#[derive(Serialize)]
struct ChatTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatFunction<'a>,
}

/// `strict` is deliberately absent. It is an OpenAI structured-outputs
/// extension; Ollama documents its own compatibility layer as experimental
/// while listing `tools` as supported, so an unrecognised key buys a local
/// provider nothing. `parameters` is borrowed straight from the [`ToolSpec`] —
/// the schema on the wire is the one the server derived, never a copy.
#[derive(Serialize)]
struct ChatFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

impl<'a> From<&'a ToolSpec> for ChatTool<'a> {
    fn from(spec: &'a ToolSpec) -> ChatTool<'a> {
        ChatTool {
            kind: "function",
            function: ChatFunction {
                name: &spec.name,
                description: &spec.description,
                parameters: &spec.parameters,
            },
        }
    }
}

impl<'a> From<&'a Message> for ChatMessage<'a> {
    fn from(message: &'a Message) -> ChatMessage<'a> {
        ChatMessage {
            role: match message.role {
                skein_core::Role::User => "user",
                skein_core::Role::Assistant => "assistant",
                skein_core::Role::System => "system",
                skein_core::Role::Tool => "tool",
            },
            content: message.text(),
            tool_calls: message
                .tool_calls
                .iter()
                .map(|call| ChatToolCall {
                    id: &call.id,
                    kind: "function",
                    function: ChatCallFunction {
                        name: &call.tool,
                        // Serializing an owned `Value` back to text cannot
                        // fail: there is no writer to error and no key a
                        // `Value` can hold that is not already a string. A
                        // fallback here is forbidden for this slice's own
                        // reason — an empty object would silently erase the
                        // arguments the model chose, which is the exact
                        // information loss this shape exists to stop.
                        arguments: serde_json::to_string(&call.args)
                            .expect("a serde_json::Value re-serializes"),
                    },
                })
                .collect(),
            tool_call_id: message.tool_call_id.as_deref(),
        }
    }
}

/// Unknown fields are ignored, which is why a real provider's `id`, `object`,
/// `created` and vendor extensions cost nothing here.
#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    /// `None` when the provider sent no metering at all, which is a refusal —
    /// see [`OpenAiCompatClient::metered`].
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    /// Explicitly nullable: a tool-calling turn carries `"content": null`.
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ResponseToolCall>,
}

#[derive(Deserialize)]
struct ResponseToolCall {
    /// Absent on compat layers that do not synthesize one; see `turn`.
    #[serde(default)]
    id: Option<String>,
    function: ToolFunction,
}

#[derive(Deserialize)]
struct ToolFunction {
    name: String,
    /// A JSON *string* holding JSON, per the OpenAI wire format.
    arguments: String,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    completion_tokens: Option<u64>,
}
