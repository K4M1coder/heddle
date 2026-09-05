//! Acceptance tests for named-provider routing (spec 021).
//!
//! Same discipline as `openai_compat.rs` (spec 012 SC-003): every wire claim is
//! proved against a **real socket** served by the `std::net::TcpListener` stub
//! below, never an HTTP-mocking crate. What is new here is a claim a mock could
//! not express at all — that a refused route opens **no connection** — so the
//! stub counts `accept()`s, not parsed requests: a socket opened and then
//! abandoned is still egress.
//!
//! **Why a `Cloud`-kind route points at loopback in these tests.** `ProviderKind`
//! records what the operator *declared* a provider to be, and that declaration
//! is what the egress policy acts on. It is deliberately not inferred from the
//! address: `NetworkEndpoint` places no loopback restriction on a `Cloud` route,
//! precisely so the routing, credential and refusal machinery is testable
//! without a TLS backend the build does not have (spec 012 FR-003/SC-007 stand).
//! A `Cloud` route at `http://127.0.0.1:<port>` is therefore not a contradiction
//! — it is the only honest way to observe the bytes a cloud route would send.

use heddle_core::{
    HeddleError, Message, ModelClient, Result, SecretProvider, SecretRef, SecretValue, TurnRequest,
};
use heddle_gateway::{ProviderKind, ProviderRoute, ProviderTable, Router};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

/// Long enough that a slow CI runner never trips it, short enough that a client
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(10);

const TIMEOUT: Duration = Duration::from_secs(10);

/// The reference this file's fake provider resolves. A URI naming a secret,
/// exactly as `SecretRef`'s docstring describes one — never a value.
const TOKEN_REF: &str = "keychain://heddle-test/cloud-primary";

/// A value that would be unmistakable in any error text it leaked into.
const TOKEN: &str = "sk-test-DO-NOT-LEAK-4c1f9";

struct Reply {
    status: &'static str,
    body: String,
}

impl Reply {
    fn ok(body: impl Into<String>) -> Reply {
        Reply {
            status: "200 OK",
            body: body.into(),
        }
    }

    /// A provider rejecting the credential — the one response whose error text
    /// is most tempting to build out of "everything we sent".
    fn status(status: &'static str, body: impl Into<String>) -> Reply {
        Reply {
            status,
            body: body.into(),
        }
    }
}

/// A provider that answers `replies` in order, reports the exact request bytes
/// it was sent, and — the point of this file — counts every connection it
/// accepted.
struct Stub {
    base_url: String,
    requests: Receiver<String>,
    connections: Arc<AtomicUsize>,
}

impl Stub {
    fn serving(replies: Vec<Reply>) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("the bound address");
        let (tx, requests) = mpsc::channel();
        let connections = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&connections);
        std::thread::spawn(move || {
            for reply in replies {
                let Ok((mut socket, _)) = listener.accept() else {
                    return;
                };
                // Counted on accept and before the request is read, so a client
                // that connects and says nothing is still recorded as egress.
                counted.fetch_add(1, Ordering::SeqCst);
                let Some(seen) = read_request(&mut socket) else {
                    return;
                };
                if tx.send(seen).is_err() {
                    return;
                }
                // `connection: close` makes each turn a fresh accept, so the
                // connection count is deterministic instead of racing ureq's
                // pool.
                let _ = socket.write_all(
                    format!(
                        "HTTP/1.1 {}\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        reply.status,
                        reply.body.len(),
                        reply.body
                    )
                    .as_bytes(),
                );
                let _ = socket.flush();
            }
        });
        Stub {
            base_url: format!("http://{addr}/v1"),
            requests,
            connections,
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    /// How many connections this stub has accepted. Read after a refusal to
    /// prove no socket was opened, which is a stronger claim than "the response
    /// was discarded".
    fn connection_count(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// The next request's raw bytes, as text with `\r` stripped so an assertion
    /// failure is readable.
    fn request(&self) -> String {
        match self.requests.recv_timeout(OBSERVE_TIMEOUT) {
            Ok(raw) => raw.replace('\r', ""),
            Err(RecvTimeoutError::Timeout) => {
                panic!("the client sent no request within {OBSERVE_TIMEOUT:?}")
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the stub server stopped before a request arrived")
            }
        }
    }
}

/// Reads one HTTP/1.1 request: the request line, the headers, and exactly
/// `content-length` body bytes.
fn read_request(socket: &mut TcpStream) -> Option<String> {
    let mut reader = BufReader::new(socket.try_clone().ok()?);
    let mut raw = String::new();
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = value.trim().parse().ok()?;
        }
        let blank = line == "\r\n" || line == "\n";
        raw.push_str(&line);
        if blank {
            break;
        }
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).ok()?;
    raw.push_str(&String::from_utf8_lossy(&body));
    Some(raw)
}

/// SSE framing as the real provider writes it, with a bare `\n\n` separator and
/// a terminating `[DONE]`.
fn sse(events: Vec<serde_json::Value>) -> String {
    let mut raw = String::new();
    for event in events {
        raw.push_str(&format!("data: {event}\n\n"));
    }
    raw.push_str("data: [DONE]\n\n");
    raw
}

/// A response shaped the way Ollama's OpenAI-compatible endpoint streams one.
fn provider_reply(content: &str) -> String {
    sse(vec![
        serde_json::json!({
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": content}}]
        }),
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        }),
        serde_json::json!({"choices": [], "usage": {"total_tokens": 18}}),
    ])
}

/// A `SecretProvider` that is the trait and nothing else.
///
/// Deliberately **not** `heddle-silo`'s `TestSecret`: that fixture is backed by
/// the real OS keychain and lives in a crate `heddle-gateway` must not depend on
/// (Constitution IV). Testing the router against the trait boundary is what the
/// router actually promises.
struct FakeSecrets {
    /// What `requires_network()` answers — the property design §7.3 makes the
    /// egress policy consult, and which nothing in this workspace read before
    /// this slice.
    requires_network: bool,
}

impl FakeSecrets {
    fn offline() -> FakeSecrets {
        FakeSecrets {
            requires_network: false,
        }
    }

    /// A store that must leave this machine to answer — a cloud-hosted vault,
    /// the case ADR-0002 D4 makes `requires_network()` exist for.
    fn networked() -> FakeSecrets {
        FakeSecrets {
            requires_network: true,
        }
    }
}

impl SecretProvider for FakeSecrets {
    fn resolve(&self, secret: &SecretRef) -> Result<SecretValue> {
        if secret.0 == TOKEN_REF {
            return Ok(SecretValue::new(TOKEN));
        }
        Err(HeddleError::Secret(format!("no such secret: {}", secret.0)))
    }

    fn requires_network(&self) -> bool {
        self.requires_network
    }
}

fn local_route(name: &str, base_url: &str, model: &str) -> ProviderRoute {
    ProviderRoute {
        name: name.into(),
        kind: ProviderKind::Local,
        base_url: base_url.into(),
        model: model.into(),
        credential: None,
    }
}

fn cloud_route(name: &str, base_url: &str, model: &str, credential: Option<&str>) -> ProviderRoute {
    ProviderRoute {
        name: name.into(),
        kind: ProviderKind::Cloud,
        base_url: base_url.into(),
        model: model.into(),
        credential: credential.map(|reference| SecretRef(reference.into())),
    }
}

fn ask(text: &str) -> TurnRequest {
    TurnRequest {
        run_id: "run-1".into(),
        messages: vec![Message::user_text(text)],
        tools: Vec::new(),
    }
}

fn headers_of(raw: &str) -> String {
    raw.split_once("\n\n")
        .expect("a blank-line separator")
        .0
        .to_string()
}

fn body_of(raw: &str) -> serde_json::Value {
    let (_, body) = raw.split_once("\n\n").expect("a blank-line separator");
    serde_json::from_str(body).expect("a JSON request body")
}

#[test]
fn a_named_local_provider_routes_to_its_own_base_url_and_model() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("hello back"))]);
    let table = ProviderTable::new(vec![
        local_route("local-ollama", stub.base_url(), "llama3.1"),
        // A second route the name must *not* select, so the test proves a
        // lookup happened rather than that there was only ever one answer.
        local_route("local-other", "http://127.0.0.1:1/v1", "some-other-model"),
    ]);

    let mut model = Router::new(&table)
        .client_for("local-ollama", &FakeSecrets::offline(), false, TIMEOUT)
        .expect("a local route needs no egress permission");

    model.turn(&ask("hello")).expect("the stub answers");

    let seen = stub.request();
    let headers = headers_of(&seen);
    assert!(
        headers.starts_with("POST /v1/chat/completions HTTP/1.1\n"),
        "the named route's base URL is the one dialled, in:\n{headers}"
    );
    assert_eq!(
        body_of(&seen)["model"],
        "llama3.1",
        "the model comes from the route, not from a flag the operator retyped"
    );
    assert!(
        !headers
            .lines()
            .any(|l| l.to_ascii_lowercase().starts_with("authorization:")),
        "a route with no credential sends no Authorization header, in:\n{headers}"
    );
    assert_eq!(
        stub.connection_count(),
        1,
        "exactly one turn, one connection"
    );
}

#[test]
fn an_unknown_provider_name_is_refused_and_named() {
    let table = ProviderTable::new(vec![local_route(
        "local-ollama",
        "http://127.0.0.1:11434/v1",
        "llama3.1",
    )]);

    let err = Router::new(&table)
        .client_for("typo-ollama", &FakeSecrets::offline(), false, TIMEOUT)
        .expect_err("an unconfigured name is a refusal, not a panic");

    let message = err.to_string();
    assert!(
        message.contains("typo-ollama") && message.contains("local-ollama"),
        "the refusal must name the miss and what is configured, got: {message}"
    );
}

#[test]
fn a_local_route_pointed_off_this_machine_is_still_refused() {
    let table = ProviderTable::new(vec![local_route(
        "local-ollama",
        "http://198.51.100.7:11434/v1",
        "llama3.1",
    )]);

    let err = Router::new(&table)
        .client_for("local-ollama", &FakeSecrets::offline(), true, TIMEOUT)
        .expect_err("a Local route keeps LocalEndpoint's loopback guard");

    assert!(
        err.to_string().contains("loopback"),
        "declaring a route Local must not weaken the loopback guard, even with \
         egress allowed, got: {err}"
    );
}

#[test]
fn a_cloud_route_resolves_its_credential_and_sends_it_as_a_bearer_token() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("hello from the cloud"))]);
    let table = ProviderTable::new(vec![cloud_route(
        "cloud-primary",
        stub.base_url(),
        "gpt-4o-mini",
        Some(TOKEN_REF),
    )]);

    let mut model = Router::new(&table)
        .client_for("cloud-primary", &FakeSecrets::offline(), true, TIMEOUT)
        .expect("egress is allowed and the credential resolves");

    model.turn(&ask("hello")).expect("the stub answers");

    let seen = stub.request();
    let headers = headers_of(&seen);
    assert!(
        headers
            .lines()
            .any(|l| l.eq_ignore_ascii_case(&format!("authorization: Bearer {TOKEN}"))),
        "the resolved credential is sent as a bearer token, in:
{headers}"
    );
    assert_eq!(
        body_of(&seen)["model"],
        "gpt-4o-mini",
        "the cloud route carries its own model name"
    );
    // The credential belongs in the header and nowhere else: not in the path,
    // not in a query string, not in the body.
    let request_line = headers.lines().next().expect("a request line");
    assert!(
        !request_line.contains(TOKEN),
        "no credential in the request line: {request_line}"
    );
    assert!(
        !body_of(&seen).to_string().contains(TOKEN),
        "no credential in the request body"
    );
}

#[test]
fn a_cloud_route_without_a_credential_sends_no_authorization_header() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("no key needed"))]);
    let table = ProviderTable::new(vec![cloud_route(
        "cloud-open",
        stub.base_url(),
        "some-open-model",
        None,
    )]);

    let mut model = Router::new(&table)
        .client_for("cloud-open", &FakeSecrets::offline(), true, TIMEOUT)
        .expect("a cloud provider needing no authentication is an ordinary provider");

    model.turn(&ask("hello")).expect("the stub answers");

    let headers = headers_of(&stub.request());
    assert!(
        !headers
            .lines()
            .any(|l| l.to_ascii_lowercase().starts_with("authorization:")),
        "no credential configured means no header at all, in:
{headers}"
    );
}

#[test]
fn a_rejected_credential_never_appears_in_the_error_it_produces() {
    // 401 is the response whose error text is most tempting to assemble out of
    // "everything we sent". The provider even echoes the token back in its own
    // body, so a client that pastes the body through unfiltered would leak it
    // without ever formatting the credential itself.
    let stub = Stub::serving(vec![Reply::status(
        "401 Unauthorized",
        serde_json::json!({"error": {"message": "invalid api key"}}).to_string(),
    )]);
    let table = ProviderTable::new(vec![cloud_route(
        "cloud-primary",
        stub.base_url(),
        "gpt-4o-mini",
        Some(TOKEN_REF),
    )]);

    let mut model = Router::new(&table)
        .client_for("cloud-primary", &FakeSecrets::offline(), true, TIMEOUT)
        .expect("the client is built; the provider rejects it, which is a turn failure");

    let err = model
        .turn(&ask("hello"))
        .expect_err("a 401 is a refusal the operator must see");

    let message = err.to_string();
    assert!(
        message.contains("401"),
        "the operator is told what happened, got: {message}"
    );
    assert!(
        !message.contains(TOKEN),
        "the credential must never reach an error message, got: {message}"
    );
}

#[test]
fn a_credential_store_that_needs_the_network_is_refused_when_egress_is_off() {
    // ADR-0002 D4: `requires_network()` is a property of every network-capable
    // pluggable interface, checked at enable-time. A *local* provider whose key
    // lives in a cloud-hosted vault is still egress, and the route's own kind
    // does not excuse it.
    let stub = Stub::serving(vec![Reply::ok(provider_reply("unreachable"))]);
    let table = ProviderTable::new(vec![ProviderRoute {
        credential: Some(SecretRef(TOKEN_REF.into())),
        ..local_route("local-gated", stub.base_url(), "llama3.1")
    }]);

    let err = Router::new(&table)
        .client_for("local-gated", &FakeSecrets::networked(), false, TIMEOUT)
        .expect_err("a networked secret store is egress, whatever the route's kind");

    let message = err.to_string();
    assert!(
        message.contains("local-gated") && message.contains("egress"),
        "the refusal must name the provider and the reason, got: {message}"
    );
    assert_eq!(
        stub.connection_count(),
        0,
        "no connection is opened by a refused route"
    );
}

#[test]
fn egress_off_refuses_a_cloud_route_before_any_connection_is_opened() {
    // Acceptance criterion (c). The stub is live and listening, so nothing but
    // the router itself prevents a connection — a test against a dead port
    // would pass even if the refusal did not exist.
    let stub = Stub::serving(vec![Reply::ok(provider_reply("must never be reached"))]);
    let table = ProviderTable::new(vec![cloud_route(
        "cloud-primary",
        stub.base_url(),
        "gpt-4o-mini",
        Some(TOKEN_REF),
    )]);

    let err = Router::new(&table)
        .client_for("cloud-primary", &FakeSecrets::offline(), false, TIMEOUT)
        .expect_err("a cloud route with egress off is refused");

    let message = err.to_string();
    assert!(
        message.contains("cloud-primary"),
        "the refusal names the provider the operator asked for, got: {message}"
    );
    assert!(
        message.contains("egress"),
        "the refusal names the policy that refused it, got: {message}"
    );
    assert!(
        message.contains("--allow-egress"),
        "the refusal tells the operator how to permit it, got: {message}"
    );
    // The claim is stronger than "the answer was discarded": no socket was ever
    // opened, so nothing about this run left the machine.
    assert_eq!(
        stub.connection_count(),
        0,
        "a refused cloud route must open no connection at all"
    );
}

#[test]
fn egress_off_leaves_local_routes_entirely_alone() {
    // The refusal is scoped to what the operator declared cloud. If it were not,
    // the safe default would make the default configuration unusable, and an
    // operator would learn to pass --allow-egress reflexively.
    let stub = Stub::serving(vec![Reply::ok(provider_reply("still here"))]);
    let table = ProviderTable::new(vec![local_route(
        "local-ollama",
        stub.base_url(),
        "llama3.1",
    )]);

    let mut model = Router::new(&table)
        .client_for("local-ollama", &FakeSecrets::offline(), false, TIMEOUT)
        .expect("egress off is not a refusal of this machine");

    model.turn(&ask("hello")).expect("the stub answers");
    assert_eq!(stub.connection_count(), 1);
}

#[test]
fn egress_on_lets_a_cloud_route_through_to_its_address() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("allowed"))]);
    let table = ProviderTable::new(vec![cloud_route(
        "cloud-primary",
        stub.base_url(),
        "gpt-4o-mini",
        None,
    )]);

    let mut model = Router::new(&table)
        .client_for("cloud-primary", &FakeSecrets::offline(), true, TIMEOUT)
        .expect("--allow-egress is what permits this");

    model.turn(&ask("hello")).expect("the stub answers");
    assert_eq!(stub.connection_count(), 1);
}

// ---------------------------------------------------------------------------
// The config file. This is the first config-file loader in the workspace, so
// the tests below are as much about what it *refuses* as what it parses: a
// silently-ignored key in a provider table is how a request ends up
// unauthenticated, and a silently-shadowed duplicate name is how it ends up at
// the wrong address.
// ---------------------------------------------------------------------------

const PROVIDERS_TOML: &str = r#"
[[provider]]
name = "local-ollama"
kind = "local"
base_url = "http://localhost:11434/v1"
model = "llama3.1"

[[provider]]
name = "cloud-primary"
kind = "cloud"
base_url = "https://api.example.invalid/v1"
model = "gpt-4o-mini"
credential = "keychain://heddle/cloud-primary"
"#;

#[test]
fn a_provider_table_round_trips_from_toml() {
    let table = ProviderTable::from_toml_str(PROVIDERS_TOML).expect("a well-formed table");

    assert_eq!(
        table.routes(),
        &[
            ProviderRoute {
                name: "local-ollama".into(),
                kind: ProviderKind::Local,
                base_url: "http://localhost:11434/v1".into(),
                model: "llama3.1".into(),
                credential: None,
            },
            ProviderRoute {
                name: "cloud-primary".into(),
                kind: ProviderKind::Cloud,
                base_url: "https://api.example.invalid/v1".into(),
                model: "gpt-4o-mini".into(),
                credential: Some(SecretRef("keychain://heddle/cloud-primary".into())),
            },
        ],
        "both routes, in file order, with kind and credential mapped"
    );
}

#[test]
fn an_empty_table_parses_and_refuses_every_name() {
    let table = ProviderTable::from_toml_str("").expect("an empty file is not malformed");
    assert!(table.routes().is_empty());
    let err = table.find("anything").expect_err("nothing is configured");
    assert!(
        err.to_string().contains("none"),
        "the refusal says so plainly, got: {err}"
    );
}

#[test]
fn an_unrecognised_kind_is_refused() {
    let err = ProviderTable::from_toml_str(
        r#"
[[provider]]
name = "somewhere"
kind = "hybrid"
base_url = "http://127.0.0.1:1/v1"
model = "m"
"#,
    )
    .expect_err("kind is a closed set");

    assert!(
        err.to_string().contains("hybrid"),
        "the refusal names the value it did not understand, got: {err}"
    );
}

#[test]
fn a_misspelled_key_is_refused_rather_than_ignored() {
    // `credentials` is the plausible typo, and ignoring it would produce a
    // provider that silently sends no Authorization header — a failure the
    // operator would debug at the provider rather than in their config.
    let err = ProviderTable::from_toml_str(
        r#"
[[provider]]
name = "cloud-primary"
kind = "cloud"
base_url = "https://api.example.invalid/v1"
model = "gpt-4o-mini"
credentials = "keychain://heddle/cloud-primary"
"#,
    )
    .expect_err("an unknown key is a mistake, not an extension point");

    assert!(
        err.to_string().contains("credentials"),
        "the refusal names the key it did not recognise, got: {err}"
    );
}

#[test]
fn two_providers_with_one_name_are_refused() {
    // `find` returns the first match, so a duplicate would silently shadow the
    // second — and the operator editing the second one would see no effect.
    let err = ProviderTable::from_toml_str(
        r#"
[[provider]]
name = "local-ollama"
kind = "local"
base_url = "http://127.0.0.1:11434/v1"
model = "llama3.1"

[[provider]]
name = "local-ollama"
kind = "local"
base_url = "http://127.0.0.1:11435/v1"
model = "qwen2.5"
"#,
    )
    .expect_err("a name selects one provider or the table is ambiguous");

    assert!(
        err.to_string().contains("local-ollama"),
        "the refusal names the duplicate, got: {err}"
    );
}

#[test]
fn a_missing_providers_file_is_refused_by_name() {
    let missing = std::env::temp_dir().join("heddle-no-such-providers-file.toml");
    let err = ProviderTable::from_path(&missing).expect_err("the file does not exist");

    let message = err.to_string();
    assert!(
        message.contains("heddle-no-such-providers-file.toml"),
        "the refusal names the path it tried, got: {message}"
    );
    assert!(
        matches!(err, HeddleError::Model(_)),
        "an unreadable config is a model-configuration refusal, not a raw io error"
    );
}

#[test]
fn a_table_parsed_from_toml_routes_the_same_as_one_built_by_hand() {
    let stub = Stub::serving(vec![Reply::ok(provider_reply("from the file"))]);
    let table = ProviderTable::from_toml_str(&format!(
        r#"
[[provider]]
name = "local-ollama"
kind = "local"
base_url = "{}"
model = "llama3.1"
"#,
        stub.base_url()
    ))
    .expect("a well-formed table");

    let mut model = Router::new(&table)
        .client_for("local-ollama", &FakeSecrets::offline(), false, TIMEOUT)
        .expect("the parsed route is an ordinary route");

    model.turn(&ask("hello")).expect("the stub answers");
    assert_eq!(body_of(&stub.request())["model"], "llama3.1");
}
