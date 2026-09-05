//! Acceptance tests for the Microsoft 365 connector (spec 040).
//!
//! Same discipline as `tests/atlassian_connector.rs`, and for the same reason:
//! every wire claim is proved against a **real socket** served by the
//! `std::net::TcpListener` stub below, never an HTTP-mocking crate. The claim a
//! mock could not express at all is the one this file exists for — that a
//! connector refused for egress opens **no connection** — so the stub counts
//! `accept()`s rather than parsed requests: a socket opened and then abandoned
//! is still egress.
//!
//! **Why the base URL is `http://127.0.0.1:<port>` here.** Nothing about the
//! connector infers policy from an address: it is network-capable by
//! declaration, always, because there is no local Microsoft Graph. Pointing it
//! at loopback is the only honest way to observe the bytes it would send to
//! `https://graph.microsoft.com/v1.0` without a TLS backend this build
//! deliberately does not have (spec 012 FR-003/SC-007 stand, and this crate's
//! `Cargo.toml` records why they must keep standing for both crates at once).

use heddle_connectors::{m365_connector, M365Config};
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
const TOKEN_REF: &str = "keychain://heddle-m365/tenant";

/// A value that would be unmistakable in any error text it leaked into.
const TOKEN: &str = "m365-test-DO-NOT-LEAK-9f2a1";

/// A real-shaped Teams channel id: the `:` and `@` Atlassian's `path_segment`
/// would refuse and this connector must percent-encode instead.
const CHANNEL_ID: &str = "19:abcdef0123456789@thread.tacv2";

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

fn config(base_url: &str) -> M365Config {
    M365Config {
        base_url: base_url.into(),
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
fn egress_off_refuses_the_m365_connector_before_any_connection_is_opened() {
    // Acceptance criterion (b). The stub is live and listening, so nothing but
    // the connector's own gate prevents a connection — a test against a dead
    // port would pass even if the refusal did not exist.
    let stub = Stub::serving(vec![Reply::ok("{\"never\":\"reached\"}")]);

    let err = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), false)
        .expect_err("a network connector with egress off is refused");

    let message = err.to_string();
    assert!(
        message.contains("Microsoft 365"),
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
    // ADR-0002 D4, the second half: even with egress permitted for Graph
    // itself, a store that must leave this machine to answer is its own egress.
    // Here egress is off, so the connector's own check fires first — the case
    // worth its own test is the *ordering*.
    let stub = Stub::serving(vec![Reply::ok("{\"never\":\"reached\"}")]);

    let err = m365_connector(config(stub.base_url()), &FakeSecrets::networked(), false)
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
    let unknown = M365Config {
        token: SecretRef("keychain://heddle-m365/nobody".into()),
        ..config(stub.base_url())
    };

    let err = m365_connector(unknown, &FakeSecrets::offline(), true)
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
    let err = m365_connector(config("not-a-url"), &FakeSecrets::offline(), true)
        .expect_err("a malformed base URL is refused");

    let message = err.to_string();
    assert!(
        message.contains("not-a-url"),
        "the refusal names what the operator typed, got: {message}"
    );
}

#[test]
fn the_connector_advertises_exactly_its_five_m365_tools_when_enabled() {
    // Acceptance criterion (a). Five names and no others: the advertisement is
    // the contract the model is shown, so an extra name here is a tool nobody
    // wrote a policy entry for.
    let stub = Stub::serving(Vec::new());
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
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
            "outlook_read_mail",
            "outlook_send_mail",
            "sharepoint_read_file",
            "teams_read_messages",
            "teams_send_message",
        ]
    );
    assert_eq!(
        stub.connection_count(),
        0,
        "advertising a tool reaches no site: tools/list is answered in this process"
    );
}

#[test]
fn reading_recent_outlook_mail_works_against_a_real_socket_stub() {
    // Acceptance criterion (c), the mail-list half.
    let page = serde_json::json!({
        "value": [
            {
                "subject": "Egress is off by default",
                "from": {"emailAddress": {"address": "ops@acme.example"}},
                "bodyPreview": "Nothing leaves the machine without --allow-egress."
            },
            {
                "subject": "Second message",
                "from": {"emailAddress": {"address": "dev@acme.example"}},
                "bodyPreview": "Reproduced on a loopback stub."
            }
        ]
    });
    let stub = Stub::serving(vec![Reply::ok(page.to_string())]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call("outlook_read_mail", serde_json::json!({"top": 2})))
        .expect("the stub answers");
    let text = text_of(&outcome.content);

    assert!(
        text.contains("Egress is off by default") && text.contains("ops@acme.example"),
        "each message's subject and sender reach the model, got: {text}"
    );
    assert!(
        text.contains("Nothing leaves the machine without --allow-egress."),
        "the body preview reaches the model, got: {text}"
    );
    assert!(text.contains("Second message"), "{text}");

    let raw = stub.request();
    assert!(
        raw.starts_with("GET /me/messages?") && raw.contains("$top=2"),
        "the list-messages endpoint is the one addressed, with the caller's $top, got: {raw}"
    );
    let headers = headers_of(&raw).to_ascii_lowercase();
    assert!(
        headers.contains(&format!(
            "authorization: bearer {}",
            TOKEN.to_ascii_lowercase()
        )),
        "Graph's auth is a single Bearer access token, not Basic, got: {headers}"
    );
    assert_eq!(stub.connection_count(), 1);
}

#[test]
fn the_bearer_header_carries_exactly_the_resolved_token() {
    // The wrong-scheme regression a same-shaped stub would otherwise hide: the
    // stub enforces no auth, so only asserting the exact header value catches a
    // Basic copied from the Atlassian precedent one directory over.
    let stub = Stub::serving(vec![Reply::ok("{\"value\":[]}")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    connector
        .call(&call("outlook_read_mail", serde_json::json!({})))
        .expect("the stub answers");

    let headers = headers_of(&stub.request());
    let sent = headers
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
        .expect("an authorization header")
        .to_string();
    assert_eq!(
        sent.trim(),
        format!("authorization: Bearer {TOKEN}"),
        "the resolved access token, verbatim, behind `Bearer ` — Graph's own scheme"
    );
}

#[test]
fn reading_one_outlook_message_by_id_works_against_a_real_socket_stub() {
    let message = serde_json::json!({
        "subject": "The connector refuses when egress is off",
        "from": {"emailAddress": {"address": "ops@acme.example"}},
        "body": {"contentType": "text", "content": "Reproduced on a loopback stub."}
    });
    let stub = Stub::serving(vec![Reply::ok(message.to_string())]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "outlook_read_mail",
            serde_json::json!({"message_id": "AAMkAD=="}),
        ))
        .expect("the stub answers");
    let text = text_of(&outcome.content);

    assert!(
        text.contains("The connector refuses when egress is off"),
        "the subject reaches the model, got: {text}"
    );
    assert!(
        text.contains("Reproduced on a loopback stub."),
        "the full body, not the preview, reaches the model, got: {text}"
    );

    let raw = stub.request();
    assert!(
        raw.starts_with("GET /me/messages/AAMkAD%3D%3D HTTP/1.1"),
        "a Graph message id is percent-encoded into exactly one segment, got: {raw}"
    );
    assert_eq!(stub.connection_count(), 1);
}

#[test]
fn sending_outlook_mail_posts_to_sendmail_and_tolerates_an_empty_response() {
    // Graph answers `sendMail` with 202 Accepted and no body at all: the one
    // shape a verbatim copy of the Atlassian wire would report as a failure.
    let stub = Stub::serving(vec![Reply::status("202 Accepted", "")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "outlook_send_mail",
            serde_json::json!({
                "to": "ops@acme.example",
                "subject": "Filed from a governed run",
                "body": "Nothing left the machine that was not allowed to."
            }),
        ))
        .expect("the stub answers");

    assert!(
        !is_error(&outcome.content),
        "an empty 202 is a successful send, not a parse failure: {}",
        outcome.content
    );
    let text = text_of(&outcome.content);
    assert!(
        text.contains("ops@acme.example"),
        "the answer names who it was sent to, got: {text}"
    );

    let raw = stub.request();
    assert!(raw.starts_with("POST /me/sendMail HTTP/1.1"), "{raw}");
    assert_eq!(
        body_of(&raw),
        serde_json::json!({
            "message": {
                "subject": "Filed from a governed run",
                "body": {
                    "contentType": "Text",
                    "content": "Nothing left the machine that was not allowed to."
                },
                "toRecipients": [{"emailAddress": {"address": "ops@acme.example"}}]
            },
            "saveToSentItems": true
        }),
        "the request body is the one Graph's sendMail endpoint documents"
    );
}

#[test]
fn reading_a_sharepoint_file_by_path_works_against_a_real_socket_stub() {
    // A file's `/content` answer is not JSON and must never be parsed as any:
    // this fixture is plain text on purpose.
    let stub = Stub::serving(vec![Reply::ok("How to roll back.\nStep one: stop.\n")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "sharepoint_read_file",
            serde_json::json!({"site": "acme.sharepoint.com,1,2", "path": "Reports/Q3 plan.md"}),
        ))
        .expect("the stub answers");

    assert!(!is_error(&outcome.content), "{}", outcome.content);
    assert_eq!(
        text_of(&outcome.content),
        "How to roll back.\nStep one: stop.",
        "the file's own bytes reach the model, not a JSON parse of them"
    );

    let raw = stub.request();
    assert!(
        raw.starts_with(
            "GET /sites/acme.sharepoint.com,1,2/drive/root:/Reports/Q3%20plan.md:/content \
             HTTP/1.1"
        ),
        "the path form addresses `root:/<path>:/content` with each path segment encoded, while \
         the site key reaches Graph as the operator wrote it, got: {raw}"
    );
    assert_eq!(stub.connection_count(), 1);
}

#[test]
fn reading_a_sharepoint_file_by_item_id_works_against_a_real_socket_stub() {
    let stub = Stub::serving(vec![Reply::ok("by id\n")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "sharepoint_read_file",
            serde_json::json!({"site": "acme", "item_id": "01ABCDEF!123"}),
        ))
        .expect("the stub answers");

    assert_eq!(text_of(&outcome.content), "by id");
    let raw = stub.request();
    assert!(
        raw.starts_with("GET /sites/acme/drive/items/01ABCDEF%21123/content HTTP/1.1"),
        "the id form addresses `items/<id>/content`, the id encoded, got: {raw}"
    );
}

#[test]
fn sharepoint_read_file_refuses_when_neither_path_nor_item_id_is_given() {
    let stub = Stub::serving(vec![Reply::ok("never reached")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "sharepoint_read_file",
            serde_json::json!({"site": "acme"}),
        ))
        .expect("an ambiguous call is a tool-level refusal");

    assert!(is_error(&outcome.content));
    let message = text_of(&outcome.content);
    assert!(
        message.contains("path") && message.contains("item_id"),
        "the refusal names both ways of addressing a file, got: {message}"
    );
    assert_eq!(
        stub.connection_count(),
        0,
        "a refusal the connector can make itself opens no connection"
    );
}

#[test]
fn a_sharepoint_path_that_climbs_out_of_the_drive_is_refused() {
    let stub = Stub::serving(vec![Reply::ok("never reached")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "sharepoint_read_file",
            serde_json::json!({"site": "acme", "path": "Reports/../../secrets.txt"}),
        ))
        .expect("a traversal path is a tool-level refusal");

    assert!(is_error(&outcome.content));
    assert_eq!(stub.connection_count(), 0);
}

#[test]
fn a_site_key_that_would_break_out_of_the_path_is_refused() {
    // The site key is the one value this connector puts in a URL unencoded, so
    // that Graph's `hostname:/path` compound form keeps working — which makes
    // "it may not carry a query or a fragment" a claim worth its own test.
    let stub = Stub::serving(vec![Reply::ok("never reached")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "sharepoint_read_file",
            serde_json::json!({"site": "acme?leak=1", "item_id": "01ABC"}),
        ))
        .expect("a site key carrying a query is a tool-level refusal");

    assert!(is_error(&outcome.content));
    assert_eq!(stub.connection_count(), 0);
}

#[test]
fn reading_teams_channel_messages_works_against_a_real_socket_stub() {
    let page = serde_json::json!({
        "value": [
            {
                "from": {"user": {"displayName": "Ops"}},
                "body": {"contentType": "text", "content": "Deploy is green."}
            },
            {
                "from": {"user": {"displayName": "Dev"}},
                "body": {"contentType": "text", "content": "Thanks."}
            }
        ]
    });
    let stub = Stub::serving(vec![Reply::ok(page.to_string())]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "teams_read_messages",
            serde_json::json!({"team_id": "team-1", "channel_id": CHANNEL_ID}),
        ))
        .expect("the stub answers");
    let text = text_of(&outcome.content);

    assert!(
        text.contains("Ops") && text.contains("Deploy is green."),
        "{text}"
    );
    assert!(text.contains("Dev") && text.contains("Thanks."), "{text}");

    let raw = stub.request();
    assert!(
        raw.starts_with(
            "GET /teams/team-1/channels/19%3Aabcdef0123456789%40thread.tacv2/messages?"
        ),
        "a real Teams channel id is percent-encoded, not refused for its `:` and `@`, got: {raw}"
    );
    assert!(
        raw.contains("$top="),
        "the page size is asked for, got: {raw}"
    );
}

#[test]
fn sending_a_teams_channel_message_posts_the_body_and_returns_its_id() {
    let stub = Stub::serving(vec![Reply::ok(
        serde_json::json!({"id": "1690000000000"}).to_string(),
    )]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "teams_send_message",
            serde_json::json!({
                "team_id": "team-1",
                "channel_id": CHANNEL_ID,
                "body": "Reproduced on a loopback stub."
            }),
        ))
        .expect("the stub answers");

    assert!(
        text_of(&outcome.content).contains("1690000000000"),
        "the new message's id is what a follow-up call needs, got: {}",
        text_of(&outcome.content)
    );

    let raw = stub.request();
    assert!(
        raw.starts_with(
            "POST /teams/team-1/channels/19%3Aabcdef0123456789%40thread.tacv2/messages HTTP/1.1"
        ),
        "{raw}"
    );
    assert_eq!(
        body_of(&raw),
        serde_json::json!({"body": {"content": "Reproduced on a loopback stub."}}),
        "the request body is the one Graph's channel-post endpoint documents"
    );
}

#[test]
fn a_rejected_m365_token_never_appears_in_the_error_it_produces() {
    // Acceptance criterion (d), the error half. 401 is the response whose text
    // is most tempting to assemble out of "everything we sent", and this stub
    // goes further: it echoes the token back in its own body, so a connector
    // that passes the body through unfiltered leaks the credential without ever
    // formatting it.
    let stub = Stub::serving(vec![Reply::status(
        "401 Unauthorized",
        serde_json::json!({
            "error": {
                "code": "InvalidAuthenticationToken",
                "message": format!("the access token {TOKEN} was rejected")
            }
        })
        .to_string(),
    )]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("the connector is built; Graph rejects it, which is a tool failure");

    let outcome = connector
        .call(&call("outlook_read_mail", serde_json::json!({})))
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
fn a_rejected_token_never_leaks_through_a_file_read_either() {
    // `sharepoint_read_file` is the one tool whose body never goes through the
    // JSON path, so its scrubbing is a separate claim from the one above.
    let stub = Stub::serving(vec![Reply::status(
        "401 Unauthorized",
        format!("the access token {TOKEN} was rejected"),
    )]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call(
            "sharepoint_read_file",
            serde_json::json!({"site": "acme", "item_id": "01ABC"}),
        ))
        .expect("a 401 is a tool-level refusal");

    assert!(is_error(&outcome.content));
    assert!(
        !outcome.content.contains(TOKEN),
        "the raw-text path scrubs the credential too, got: {}",
        outcome.content
    );
}

#[test]
fn a_malformed_answer_is_a_tool_error_and_not_a_panic() {
    let stub = Stub::serving(vec![Reply::ok("this is not json")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call("outlook_read_mail", serde_json::json!({})))
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
    let stub = Stub::serving(vec![Reply::ok("{\"value\":[]}")]);
    let mut connector = m365_connector(config(stub.base_url()), &FakeSecrets::offline(), true)
        .expect("egress on builds the connector");

    let outcome = connector
        .call(&call("outlook_read_mail", serde_json::json!({})))
        .expect("the stub answers");

    assert!(!is_error(&outcome.content), "{}", outcome.content);
}
