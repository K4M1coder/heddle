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
    Message, ModelClient, Result, SkeinError, TextSink, ToolCall, ToolSpec, TurnRequest,
    TurnResponse, WireExchange,
};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

/// Connect budget, separate from the global one so a wrong port fails fast even
/// when the operator has allowed a long generation.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The stream reader's ceiling, restored deliberately rather than inherited.
/// `Body::read_to_string` applied ureq's own `MAX_BODY_SIZE`; the reader this
/// crate now uses is documented "not limited by default", so without this line a
/// provider looping forever would grow memory without bound. Same number as
/// ureq's, because the property being preserved is ureq's.
const MAX_STREAM_BODY: u64 = 10 * 1024 * 1024;

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
    /// The last completed round trip, waiting for the loop to take it. `None`
    /// until a turn reaches a socket *and* gets an answer, so a client that
    /// never connected has nothing to hand over.
    last_exchange: Option<WireExchange>,
    /// Where each content delta goes as it comes off the socket. `None` for a
    /// caller with nothing to show before the turn ends — `skein chat`, whose
    /// contract is one answer on stdout, is exactly that caller.
    sink: Option<Box<dyn TextSink>>,
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
            last_exchange: None,
            sink: None,
        }
    }

    /// Sends the request and reads the whole answer, as the two shapes the
    /// provider actually produces: a success is an event stream, a refusal is
    /// one plain JSON body under a non-2xx status. Both are measured, not
    /// assumed.
    ///
    /// Nothing here raises the stream's own faults. They are carried out, so the
    /// caller can record the bytes that arrived *before* propagating — a
    /// mid-stream failure leaves evidence rather than nothing.
    fn send(&mut self, url: &str, body: &str) -> Result<(u16, Answer)> {
        let mut response = self
            .agent
            .post(url)
            .header("content-type", "application/json")
            .send(body)
            .map_err(|e| {
                SkeinError::Model(format!(
                    "POST {url} failed: {e}; is a local provider listening at {}?",
                    self.endpoint.base_url
                ))
            })?;
        let status = response.status().as_u16();

        if !(200..300).contains(&status) {
            let raw = response.body_mut().read_to_string().map_err(|e| {
                SkeinError::Model(format!("POST {url} returned an unreadable body: {e}"))
            })?;
            return Ok((
                status,
                Answer {
                    raw,
                    ..Answer::default()
                },
            ));
        }

        let reader = BufReader::new(
            response
                .body_mut()
                .with_config()
                .limit(MAX_STREAM_BODY)
                .reader(),
        );
        Ok((status, drain(reader, &mut self.sink)))
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
        // Cleared before anything can fail, so a turn that never reaches a
        // socket cannot leave an earlier turn's bytes to be taken as its own.
        self.last_exchange = None;

        let body = serde_json::to_string(&ChatRequest {
            model: &self.model,
            messages: req.messages.iter().map(ChatMessage::from).collect(),
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
            tools: req.tools.iter().map(ChatTool::from).collect(),
        })?;

        let url = self.endpoint.chat_completions_url();
        let (status, answer) = self.send(&url, &body)?;
        // `body` and the stream's bytes are *moved* here, never re-serialized:
        // the recorded request is the same `String` whose bytes ureq
        // transmitted, and the recorded response is the same one the accumulator
        // was built from. Divergence between what crossed the wire and what the
        // chain says crossed it is not unlikely, it is unrepresentable.
        //
        // Stored before anything below can fail, and stored even when the read
        // stopped short, so the bytes that caused a failure outlive it.
        self.last_exchange = Some(WireExchange {
            url,
            status,
            request: body,
            response: answer.raw,
            streamed: answer.streamed,
        });
        // Borrowed back rather than kept from the store above: the two `&self`
        // helpers below cannot be called while a `&mut self` borrow is live,
        // and a clone here would spend a copy of the whole body to avoid four
        // characters of ceremony.
        let text = self
            .last_exchange
            .as_ref()
            .expect("the exchange was stored on the line above")
            .response
            .as_str();

        if !(200..300).contains(&status) {
            return Err(SkeinError::Model(format!(
                "{} returned {status}: {}",
                self.endpoint.base_url,
                truncated(text)
            )));
        }

        match answer.fault {
            Some(StreamFault::Unreadable(why)) => {
                return Err(SkeinError::Model(format!(
                    "{} stopped mid-stream: {why}",
                    self.endpoint.base_url
                )))
            }
            Some(StreamFault::Unparseable(event)) => {
                return Err(self.unrecognised(format!(
                    "an event payload is not JSON: {}",
                    truncated(&event)
                )))
            }
            None => {}
        }

        if answer.events == 0 {
            // A 200 carrying no `data:` line at all is not a stream that said
            // nothing, it is something else entirely — an interposing proxy's
            // page, most plausibly. Falling through to the metering refusal
            // would name the wrong problem and never show the operator what
            // actually answered.
            return Err(self.unrecognised(format!("no SSE events: {}", truncated(text))));
        }
        let acc = answer.acc;
        if !acc.saw_choice {
            // Well-framed and still not an answer. The metering event carries
            // `"choices":[]` by design, so a stream of nothing but one would
            // otherwise become an empty assistant message.
            return Err(self.unrecognised("no choices[0]"));
        }

        let tool_calls = acc
            .calls
            .into_iter()
            .map(|(index, call)| {
                // Normalized here and nowhere else, so every `ToolCall` leaving
                // this crate has a non-empty id and the loop that echoes them
                // needs no fallback of its own. Ollama supplies ids; the
                // OpenAI-compat ecosystem does not guarantee one, and an empty
                // id reaching the echo would produce a request answering a call
                // it never made. The delta's own `index` is what keeps two
                // id-less calls distinct.
                let id = match call.id.is_empty() {
                    true => format!("call_{index}"),
                    false => call.id,
                };
                // Non-streamed, a no-argument call arrives as `"arguments":"{}"`
                // and parses. Streamed, it accumulates to `""`, which
                // `serde_json` rejects — so without this equivalence streaming
                // would introduce a failure the non-streamed path did not have.
                let arguments = match call.arguments.is_empty() {
                    true => "{}",
                    false => &call.arguments,
                };
                serde_json::from_str(arguments)
                    .map(|args| ToolCall::with_id(id, call.name, args))
                    .map_err(|e| {
                        self.unrecognised(format!(
                            "tool call arguments are not JSON: {e}: {}",
                            truncated(arguments)
                        ))
                    })
            })
            .collect::<Result<Vec<ToolCall>>>()?;

        Ok(TurnResponse {
            message: Message::assistant_text(acc.content),
            tokens_used: self.metered(acc.usage)?,
            // The provider's `"stop"` is a *claim* that the model is done, and
            // it is only believed when the model asked for nothing further.
            // `"length"` is deliberately not final: it means the provider
            // truncated the model mid-thought, and treating that as a completed
            // answer would let a truncation launder itself past
            // `LoopController`, which Constitution VIII(a) reserves to the
            // engine.
            final_output: acc.finish_reason.as_deref() == Some("stop") && tool_calls.is_empty(),
            tool_calls,
        })
    }

    fn take_wire_exchange(&mut self) -> Option<WireExchange> {
        self.last_exchange.take()
    }

    fn set_text_sink(&mut self, sink: Box<dyn TextSink>) {
        self.sink = Some(sink);
    }
}

/// One provider answer, in whichever of its two shapes arrived: the bytes
/// verbatim, whether they were read as an event stream, the turn reassembled
/// from them, and why the read stopped short if it did.
#[derive(Default)]
struct Answer {
    raw: String,
    streamed: bool,
    /// How many `data:` payloads parsed. Zero on a 200 means the body was not a
    /// stream at all — see `turn`.
    events: usize,
    acc: Accumulated,
    fault: Option<StreamFault>,
}

/// Why a stream stopped short. Carried out of the read rather than raised there,
/// so the bytes that arrived reach the chain before the failure propagates.
enum StreamFault {
    /// The socket failed part-way, or the reader's ceiling was reached.
    Unreadable(String),
    /// An event is framed as `data:` but its payload is not JSON.
    Unparseable(String),
}

/// Reads the event stream to its end, building the verbatim capture and the
/// reassembled turn in one pass, and pushing each content delta to `sink` as it
/// is absorbed — which is the whole point of the stream and the reason this is
/// one pass rather than read-then-parse.
///
/// Read as bytes with `read_until`, not as lines: the terminator is **kept**, so
/// the capture is byte-identical to the wire even if a provider frames with
/// CRLF, where `lines()` would strip it and quietly forge the record. The
/// lossy conversion is this function's own, so a non-UTF-8 byte becomes U+FFFD
/// rather than erroring the read.
///
/// `[DONE]` ends the events, not the reading: the loop runs to EOF so trailing
/// bytes are captured too. A provider that sends `[DONE]` and then holds the
/// socket open is the same failure the client's global timeout already governs.
fn drain(mut reader: impl BufRead, sink: &mut Option<Box<dyn TextSink>>) -> Answer {
    let mut answer = Answer {
        streamed: true,
        ..Answer::default()
    };
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                answer.fault = Some(StreamFault::Unreadable(e.to_string()));
                break;
            }
        }
        let text = String::from_utf8_lossy(&line);
        answer.raw.push_str(&text);

        // Anything that is not a `data:` line is skipped: the blank separators,
        // and the `event:` / `id:` / comment lines a different provider might
        // send. They are already in `raw`, which is where they belong.
        let Some(payload) = text.trim_end_matches(['\r', '\n']).strip_prefix("data:") else {
            continue;
        };
        let payload = payload.strip_prefix(' ').unwrap_or(payload);
        if payload == "[DONE]" {
            continue;
        }
        match serde_json::from_str::<ChatChunk>(payload) {
            Ok(chunk) => {
                answer.events += 1;
                answer.acc.absorb(chunk, sink);
            }
            Err(_) => {
                answer.fault = Some(StreamFault::Unparseable(payload.to_string()));
                break;
            }
        }
    }
    answer
}

/// A turn under construction, reassembled event by event.
#[derive(Default)]
struct Accumulated {
    content: String,
    /// Keyed by the delta's own `index`, so a fragment finds its call however
    /// the events interleave, and *ordered* by it, so the calls come out in the
    /// order the provider numbered them rather than the order they finished.
    calls: BTreeMap<u64, PartialCall>,
    finish_reason: Option<String>,
    usage: Option<Usage>,
    /// Whether any event carried a `choices[0]` at all — see `turn`.
    saw_choice: bool,
}

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
}

impl Accumulated {
    fn absorb(&mut self, chunk: ChatChunk, sink: &mut Option<Box<dyn TextSink>>) {
        for choice in chunk.choices {
            self.saw_choice = true;
            if choice.finish_reason.is_some() {
                self.finish_reason = choice.finish_reason;
            }
            // Empty deltas are absorbed but never pushed. That is not
            // defensiveness: the provider sends `"content":""` on every
            // reasoning event, so a sink told about them would receive ~150
            // empty notifications for one short turn.
            if let Some(text) = choice.delta.content.filter(|t| !t.is_empty()) {
                self.content.push_str(&text);
                if let Some(sink) = sink {
                    sink.on_text(&text);
                }
            }
            for fragment in choice.delta.tool_calls {
                let call = self.calls.entry(fragment.index).or_default();
                if let Some(id) = fragment.id {
                    call.id.push_str(&id);
                }
                if let Some(name) = fragment.function.name {
                    call.name.push_str(&name);
                }
                if let Some(arguments) = fragment.function.arguments {
                    call.arguments.push_str(&arguments);
                }
            }
        }
        if chunk.usage.is_some() {
            self.usage = chunk.usage;
        }
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
    /// Always `true`. There is no flag and no config key: two wire paths would
    /// mean two shapes to test forever, and the non-streamed one would have no
    /// caller.
    stream: bool,
    /// Mandatory, not decorative, and measured: under a bare `"stream": true`
    /// the provider sends **no `usage` object at all**, and
    /// [`OpenAiCompatClient::metered`] refuses an unmetered turn so that the
    /// loop's token budget stays enforceable. Without this field every run
    /// fails.
    stream_options: StreamOptions,
    /// Skipped when empty, so a run that advertises nothing puts exactly the
    /// bytes on the wire it put there before this field existed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatTool<'a>>,
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
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

/// The request-side mirror of [`DeltaToolCall`]: the same envelope reassembled,
/// sent back so a provider sees the turn it produced.
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
    /// [`DeltaFunction::arguments`] arrives, once its fragments are joined.
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

/// One `data:` payload. Unknown fields are ignored, which is why a real
/// provider's `id`, `object`, `created` and vendor extensions cost nothing here.
#[derive(Deserialize)]
struct ChatChunk {
    /// Empty on the metering event, which carries `"choices":[]` by design.
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    /// Present on exactly one event of a well-formed stream. `None` everywhere
    /// else, and `None` on *every* event of a stream from a provider that
    /// ignored `stream_options` — which is a refusal, see
    /// [`OpenAiCompatClient::metered`].
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: Delta,
    /// Carried by the one event that closes the choice; `None` on the rest.
    #[serde(default)]
    finish_reason: Option<String>,
}

/// `reasoning` is deliberately absent, and its absence is the decision. The
/// provider sends it on a reasoning model — with `content` empty on those same
/// events — and it sends it in **both** modes, so the non-streamed shape this
/// replaces was already discarding it. Naming the field here would put text
/// into `TurnResponse.message` that the non-streamed path never had.
#[derive(Default, Deserialize)]
struct Delta {
    /// Explicitly nullable: a tool-calling turn carries `"content": null`.
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<DeltaToolCall>,
}

#[derive(Deserialize)]
struct DeltaToolCall {
    /// Which call this fragment belongs to, and the whole reason fragments can
    /// be reassembled at all.
    ///
    /// Defaulted because the wire does not guarantee it, though all three
    /// provider models measured always send it. A compat layer that omitted it
    /// would collapse every call into slot `0` — recorded as a residual in
    /// `spec.md` rather than guessed around, since there is no second key to
    /// fall back to that would not be a guess.
    #[serde(default)]
    index: u64,
    /// Absent on compat layers that do not synthesize one, and absent on the
    /// continuation fragments of a call whose first fragment carried it.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: DeltaFunction,
}

/// Every field optional: a fragment carries whichever parts of the call it
/// advances and nothing else.
#[derive(Default, Deserialize)]
struct DeltaFunction {
    #[serde(default)]
    name: Option<String>,
    /// A JSON *string* holding JSON, per the OpenAI wire format — and one that
    /// arrives in pieces, so the pieces are text until the last one lands.
    #[serde(default)]
    arguments: Option<String>,
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
