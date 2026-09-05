//! Acceptance tests for the Atlassian connector (spec 037).
//!
//! Same discipline as `heddle-gateway`'s `provider_routing.rs`, and for the same
//! reason: every wire claim is proved against a **real socket** served by the
//! `std::net::TcpListener` stub below, never an HTTP-mocking crate. The claim a
//! mock could not express at all is the one this file exists for — that a
//! connector refused for egress opens **no connection** — so the stub counts
//! `accept()`s rather than parsed requests: a socket opened and then abandoned
//! is still egress.
//!
//! **Why the site's base URL is `http://127.0.0.1:<port>` here.** Nothing about
//! the connector infers policy from an address: it is network-capable by
//! declaration, always, because there is no local Jira. Pointing it at loopback
//! is the only honest way to observe the bytes it would send to a real site
//! without a TLS backend this build deliberately does not have (spec 012
//! FR-003/SC-007 stand, and this crate's `Cargo.toml` records why they must keep
//! standing for both crates at once).

use heddle_connectors::{atlassian_connector, AtlassianConfig};
use heddle_core::ToolTransport;
use heddle_core::{HeddleError, Result, SecretProvider, SecretRef, SecretValue, ToolCall};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

/// Long enough that a slow CI runner never trips it, short enough that a client
/// which silently sends nothing fails as a failure rather than as a hang.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(10);

/// The reference this file's fake store resolves. A URI naming a secret,
/// exactly as `SecretRef`'s docstring describes one — never a value.
const TOKEN_REF: &str = "keychain://heddle-atlassian/acme";

/// A value that would be unmistakable in any error text it leaked into.
const TOKEN: &str = "atl-test-DO-NOT-LEAK-4c1f9";

const EMAIL: &str = "operator@acme.example";

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

    fn status(status: &'static str, body: impl Into<String>) -> Reply {
        Reply {
            status,
            body: body.into(),
        }
    }
}

/// A site that answers `replies` in order, reports the exact request bytes it
/// was sent, and — the point of this file — counts every connection it
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
                // `connection: close` makes each call a fresh accept, so the
                // connection count is deterministic instead of racing ureq's
                // pool.
                let _ = socket.write_all(
                    format!(
                        "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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
            base_url: format!("http://{addr}"),
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
                panic!("the stub site stopped before a request arrived")
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

/// A `SecretProvider` that is the trait and nothing else.
///
/// Deliberately **not** `heddle-silo`'s `TestSecret`: that fixture is backed by
/// the real OS keychain and lives in a crate this one must not depend on
/// (Constitution IV). Testing the connector against the trait boundary is what
/// the connector actually promises.
struct FakeSecrets {
    /// What `requires_network()` answers — the property design §7.3 makes the
    /// egress policy consult.
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

fn config(base_url: &str) -> AtlassianConfig {
    AtlassianConfig {
        base_url: base_url.into(),
        email: EMAIL.into(),
        token: SecretRef(TOKEN_REF.into()),
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

/// What a tool answered, as the model is shown it. `ToolOutcome.content` is the
/// serialized `CallToolResult` rmcp built, so the text is one level in.
fn text_of(outcome: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(outcome).expect("a tool outcome is JSON the transport serialized");
    parsed["content"][0]["text"]
        .as_str()
        .expect("a text content block")
        .to_string()
}

fn is_error(outcome: &str) -> bool {
    let parsed: serde_json::Value =
        serde_json::from_str(outcome).expect("a tool outcome is JSON the transport serialized");
    parsed["isError"].as_bool().unwrap_or(false)
}

fn call(tool: &str, args: serde_json::Value) -> ToolCall {
    ToolCall::new(tool, args)
}

#[test]
fn egress_off_refuses_the_atlassian_connector_before_any_connection_is_opened() {
    // Acceptance criterion (b). The stub is live and listening, so nothing but
    // the connector's own gate prevents a connection — a test against a dead
    // port would pass even if the refusal did not exist.
    let stub = Stub::serving(vec![Reply::ok("{\"never\":\"reached\"}")]);

    let err = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), false)
        .expect_err("a network connector with egress off is refused");

    let message = err.to_string();
    assert!(
        message.contains("Atlassian"),
        "the refusal names the connector the operator asked for, got: {message}"
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
        "a refused connector must open no connection at all"
    );
}

#[test]
fn a_credential_store_that_needs_the_network_is_refused_when_egress_is_off() {
    // ADR-0002 D4, the second half: even with egress permitted for the site
    // itself, a store that must leave this machine to answer is its own egress.
    // Here egress is off, so the site check fires first — the case worth its own
    // test is the *ordering*, proved by the message naming the store rather
    // than the site.
    let stub = Stub::serving(vec![Reply::ok("{\"never\":\"reached\"}")]);

    let err = atlassian_connector(config(stub.base_url()), &FakeSecrets::networked(), false)
        .expect_err("a networked secret store with egress off is refused");

    assert!(
        err.to_string().contains("egress"),
        "the refusal names the policy, got: {err}"
    );
    assert_eq!(
        stub.connection_count(),
        0,
        "no connection is opened by a refused connector"
    );
}

#[test]
fn an_unresolvable_token_is_refused_before_any_connection_is_opened() {
    // The credential is resolved at construction, not per call, so a reference
    // the store does not know is an exit code before a model is shown a tool —
    // `EmbeddedServer::with_run`'s reasoning, applied to a secret.
    let stub = Stub::serving(vec![Reply::ok("{\"never\":\"reached\"}")]);
    let unknown = AtlassianConfig {
        token: SecretRef("keychain://heddle-atlassian/nobody".into()),
        ..config(stub.base_url())
    };

    let err = atlassian_connector(unknown, &FakeSecrets::offline(), true)
        .expect_err("an unknown credential reference is refused");

    assert!(
        err.to_string().contains("no such secret"),
        "the store's own answer reaches the operator, got: {err}"
    );
    assert_eq!(stub.connection_count(), 0);
}

#[test]
fn a_base_url_that_is_not_a_site_is_refused_before_the_credential_is_resolved() {
    // Order matters, and it is `Router::client_for`'s: the address is proved
    // well-formed before anything is resolved, so a typo never opens a
    // credential store.
    let err = atlassian_connector(config("not-a-url"), &FakeSecrets::offline(), true)
        .expect_err("a malformed base URL is refused");

    let message = err.to_string();
    assert!(
        message.contains("not-a-url"),
        "the refusal names what the operator typed, got: {message}"
    );
}

#[test]
fn the_connector_advertises_its_jira_and_confluence_tools_when_enabled() {
    // Acceptance criterion (a). Six names and no others: the advertisement is
    // the contract the model is shown, so an extra name here is a tool nobody
    // wrote a policy entry for.
    let stub = Stub::serving(Vec::new());
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let mut names: Vec<String> = connector
        .list()
        .expect("the server answers tools/list")
        .into_iter()
        .map(|spec| spec.name)
        .collect();
    names.sort();

    assert_eq!(
        names,
        vec![
            "confluence_create_page",
            "confluence_get_page",
            "jira_add_comment",
            "jira_create_issue",
            "jira_get_issue",
            "jira_search",
        ]
    );
    assert_eq!(
        stub.connection_count(),
        0,
        "advertising a tool reaches no site: tools/list is answered in this process"
    );
}

#[test]
fn reading_a_jira_issue_works_against_a_real_socket_stub() {
    // Acceptance criterion (c), the read half.
    let issue = serde_json::json!({
        "key": "PROJ-123",
        "fields": {
            "summary": "The connector refuses when egress is off",
            "status": {"name": "In Progress"},
            "description": {
                "type": "doc",
                "version": 1,
                "content": [{
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "Reproduced on a loopback stub."}]
                }]
            }
        }
    });
    let stub = Stub::serving(vec![Reply::ok(issue.to_string())]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "jira_get_issue",
            serde_json::json!({"key": "PROJ-123"}),
        ))
        .expect("the stub answers");
    let text = text_of(&outcome.content);

    assert!(
        text.contains("The connector refuses when egress is off"),
        "the issue's summary reaches the model, got: {text}"
    );
    assert!(
        text.contains("Reproduced on a loopback stub."),
        "the description's text is flattened out of ADF, got: {text}"
    );
    assert!(
        text.contains("In Progress"),
        "the status reaches the model, got: {text}"
    );

    let raw = stub.request();
    assert!(
        raw.starts_with("GET /rest/api/3/issue/PROJ-123 HTTP/1.1"),
        "the v3 issue endpoint is the one addressed, got: {raw}"
    );
    let headers = headers_of(&raw).to_ascii_lowercase();
    assert!(
        headers.contains("authorization: basic "),
        "Jira Cloud's API-token auth is HTTP Basic, not Bearer, got: {headers}"
    );
    assert_eq!(stub.connection_count(), 1);
}

#[test]
fn the_basic_auth_header_carries_the_email_and_the_resolved_token() {
    // The wrong-scheme regression a same-shaped stub would otherwise hide: the
    // stub enforces no auth, so only asserting the exact header value catches a
    // Bearer copied from the model-gateway precedent.
    let stub = Stub::serving(vec![Reply::ok("{\"key\":\"PROJ-1\",\"fields\":{}}")]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    connector
        .call(&call(
            "jira_get_issue",
            serde_json::json!({"key": "PROJ-1"}),
        ))
        .expect("the stub answers");

    let headers = headers_of(&stub.request());
    let sent = headers
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
        .expect("an authorization header")
        .to_string();
    // Computed here rather than imported, so the test proves the encoding and
    // not merely that both sides call the same function.
    let expected = base64_of(&format!("{EMAIL}:{TOKEN}"));
    assert_eq!(
        sent.trim(),
        format!("authorization: Basic {expected}"),
        "email and API token, base64 of `email:token`, per Jira Cloud's own scheme"
    );
}

/// The reference encoder, written independently of the product's.
fn base64_of(text: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn searching_jira_sends_the_jql_url_encoded() {
    let results = serde_json::json!({
        "total": 2,
        "issues": [
            {"key": "PROJ-1", "fields": {"summary": "First", "status": {"name": "To Do"}}},
            {"key": "PROJ-2", "fields": {"summary": "Second", "status": {"name": "Done"}}}
        ]
    });
    let stub = Stub::serving(vec![Reply::ok(results.to_string())]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "jira_search",
            serde_json::json!({"jql": "project = PROJ AND status != Done"}),
        ))
        .expect("the stub answers");
    let text = text_of(&outcome.content);

    assert!(text.contains("PROJ-1") && text.contains("First"), "{text}");
    assert!(text.contains("PROJ-2") && text.contains("Second"), "{text}");

    let raw = stub.request();
    assert!(
        raw.contains("jql=project%20%3D%20PROJ%20AND%20status%20%21%3D%20Done"),
        "a JQL with spaces and operators must reach the site percent-encoded, got: {raw}"
    );
}

#[test]
fn creating_a_confluence_page_works_against_a_real_socket_stub() {
    // Acceptance criterion (c), the create half.
    let created = serde_json::json!({"id": "98765", "title": "Runbook", "spaceId": "42"});
    let stub = Stub::serving(vec![Reply::ok(created.to_string())]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "confluence_create_page",
            serde_json::json!({
                "space_id": "42",
                "title": "Runbook",
                "body": "<p>How to roll back.</p>"
            }),
        ))
        .expect("the stub answers");
    let text = text_of(&outcome.content);

    assert!(
        text.contains("98765"),
        "the new page's id is what a follow-up call needs, got: {text}"
    );

    let raw = stub.request();
    assert!(
        raw.starts_with("POST /wiki/api/v2/pages HTTP/1.1"),
        "the v2 pages endpoint is the one addressed, got: {raw}"
    );
    assert_eq!(
        body_of(&raw),
        serde_json::json!({
            "spaceId": "42",
            "status": "current",
            "title": "Runbook",
            "body": {"representation": "storage", "value": "<p>How to roll back.</p>"}
        }),
        "the request body is the one Confluence's v2 create-page endpoint documents"
    );
    assert_eq!(stub.connection_count(), 1);
}

#[test]
fn reading_a_confluence_page_asks_for_storage_format() {
    let page = serde_json::json!({
        "id": "98765",
        "title": "Runbook",
        "body": {"storage": {"value": "<p>How to roll back.</p>"}}
    });
    let stub = Stub::serving(vec![Reply::ok(page.to_string())]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "confluence_get_page",
            serde_json::json!({"page_id": "98765"}),
        ))
        .expect("the stub answers");
    let text = text_of(&outcome.content);

    assert!(text.contains("Runbook"), "{text}");
    assert!(text.contains("How to roll back."), "{text}");

    let raw = stub.request();
    assert!(
        raw.starts_with("GET /wiki/api/v2/pages/98765?body-format=storage HTTP/1.1"),
        "the body format is asked for explicitly; v2 omits the body otherwise, got: {raw}"
    );
}

#[test]
fn creating_a_jira_issue_wraps_its_description_in_a_document() {
    let stub = Stub::serving(vec![Reply::ok(
        serde_json::json!({"key": "PROJ-9", "id": "10009"}).to_string(),
    )]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "jira_create_issue",
            serde_json::json!({
                "project": "PROJ",
                "summary": "Connector leaks nothing",
                "description": "Filed from a governed run.",
                "issue_type": "Task"
            }),
        ))
        .expect("the stub answers");

    assert!(text_of(&outcome.content).contains("PROJ-9"));

    let raw = stub.request();
    assert!(raw.starts_with("POST /rest/api/3/issue HTTP/1.1"), "{raw}");
    assert_eq!(
        body_of(&raw)["fields"]["description"],
        serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "Filed from a governed run."}]
            }]
        }),
        "Jira v3 takes a document and not a string; a bare string is a 400"
    );
    assert_eq!(body_of(&raw)["fields"]["project"]["key"], "PROJ");
    assert_eq!(body_of(&raw)["fields"]["issuetype"]["name"], "Task");
}

#[test]
fn commenting_on_a_jira_issue_posts_to_that_issues_comment_endpoint() {
    let stub = Stub::serving(vec![Reply::ok(
        serde_json::json!({"id": "20001"}).to_string(),
    )]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "jira_add_comment",
            serde_json::json!({"key": "PROJ-9", "body": "Reproduced."}),
        ))
        .expect("the stub answers");

    assert!(text_of(&outcome.content).contains("PROJ-9"));
    let raw = stub.request();
    assert!(
        raw.starts_with("POST /rest/api/3/issue/PROJ-9/comment HTTP/1.1"),
        "{raw}"
    );
    assert_eq!(
        body_of(&raw)["body"]["type"],
        "doc",
        "a comment is a document too"
    );
}

#[test]
fn a_rejected_atlassian_token_never_appears_in_the_error_it_produces() {
    // Acceptance criterion (d), the error half. 401 is the response whose text
    // is most tempting to assemble out of "everything we sent", and this stub
    // goes further: it echoes the token back in its own body, so a connector
    // that passes the body through unfiltered leaks the credential without ever
    // formatting it.
    let stub = Stub::serving(vec![Reply::status(
        "401 Unauthorized",
        serde_json::json!({
            "errorMessages": [format!("the token {TOKEN} was rejected")]
        })
        .to_string(),
    )]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("the connector is built; the site rejects it, which is a tool failure");

    let outcome = connector
        .call(&call(
            "jira_get_issue",
            serde_json::json!({"key": "PROJ-1"}),
        ))
        .expect("a 401 is a tool-level refusal, not a transport failure");

    assert!(
        is_error(&outcome.content),
        "a 401 must reach the model as isError, so the run survives it: {}",
        outcome.content
    );
    let message = text_of(&outcome.content);
    assert!(
        message.contains("401"),
        "the operator is told what happened, got: {message}"
    );
    assert!(
        !message.contains(TOKEN),
        "the credential must never reach an error message, got: {message}"
    );
    assert!(
        !outcome.content.contains(TOKEN),
        "not anywhere in the serialized outcome either, got: {}",
        outcome.content
    );
}

#[test]
fn a_malformed_answer_is_a_tool_error_and_not_a_panic() {
    let stub = Stub::serving(vec![Reply::ok("this is not json")]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "jira_get_issue",
            serde_json::json!({"key": "PROJ-1"}),
        ))
        .expect("a malformed body is a tool-level refusal");

    assert!(is_error(&outcome.content));
    let message = text_of(&outcome.content);
    assert!(
        message.to_ascii_lowercase().contains("json"),
        "the refusal names the parse failure, got: {message}"
    );
}

/// Sanity: the transport reports success the way every other connector does, so
/// the assertions above are reading the shape they think they are.
#[test]
fn a_successful_call_is_not_flagged_as_an_error() {
    let stub = Stub::serving(vec![Reply::ok("{\"key\":\"PROJ-1\",\"fields\":{}}")]);
    let mut connector = atlassian_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "jira_get_issue",
            serde_json::json!({"key": "PROJ-1"}),
        ))
        .expect("the stub answers");

    assert!(!is_error(&outcome.content), "{}", outcome.content);
}
