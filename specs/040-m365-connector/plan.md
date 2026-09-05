# Implementation Plan: Microsoft 365 connector (Outlook, SharePoint, Teams)

**Spec**: [spec.md](spec.md) · **Branch**: `040-m365-connector`

## Approach

A new `m365` submodule inside `crates/heddle-connectors`, alongside `atlassian` and for the same
recorded reason: it is one more `ServerHandler` served over the crate's existing in-process
duplex transport, so a second crate would buy nothing and would cost a second `Cargo.toml` to
keep `ureq`'s no-TLS-backend posture consistent with.

- `m365/client.rs` — the transport half: `GraphSite::parse` (proves the address well-formed, no
  local-only restriction because there is no local Microsoft Graph), `Wire` (the authenticated
  `ureq::Agent`, `Authorization: Bearer <token>`, error-body scrubbing of the resolved
  credential), and the two hand-written helpers Graph's shapes need
  (`path_segment_encode`, `sharepoint_path_encode`) plus `required`, rather than new
  dependencies — this crate's and `heddle-gateway`'s recorded "twenty lines rather than a
  dependency" precedent.
- `m365/server.rs` — `M365Config`, the egress gate (`M365Server::connect`), and the five
  `#[tool]` methods, using the same `#[tool_router]`/`#[tool_handler]` rmcp macros
  `EmbeddedServer` and `AtlassianServer` already use.
- `connector.rs` — `pub fn m365_connector(config, secrets, egress_allowed) ->
  Result<LocalConnector>`, mirroring `atlassian_connector` exactly and reusing the already
  generic `serve<S: ServerHandler>` unchanged. That reuse is the whole point of the
  generalization specs/039 performed: a third connector needs no third copy of the
  runtime-ownership rule.

## Egress gate ordering (Constitution II, NON-NEGOTIABLE)

Mirrors `heddle_gateway::Router::client_for` and `AtlassianServer::connect` exactly, because it
is the same proof for the same reason:

1. **egress** — this connector has no local form, so refusing here needs neither an address nor
   a credential to already be worth reporting;
2. parse the site address;
3. resolve the credential;
4. build the wire (`M365Server`, held behind an `Arc` inside `LocalConnector`).

## Test strategy

Same discipline as `tests/atlassian_connector.rs` and `heddle-gateway/tests/provider_routing.rs`:
a real `std::net::TcpListener` stub that counts `accept()`s (not parsed requests), never an
HTTP-mocking crate — the claim this connector's egress tests make ("no connection was opened") a
mock cannot express. The stub answers with the fixtures Microsoft Graph's own documentation
shows for each endpoint, including `sendMail`'s empty `202 Accepted`.

## Deviations from the Atlassian precedent

Three, each forced by a Graph shape rather than chosen:

- **Empty-body-tolerant `Wire::answer`.** Graph's `sendMail` answers `202 Accepted` with no
  content, so a verbatim copy of Atlassian's `answer` would turn a successful send into a
  spurious JSON-parse error; a whitespace-only 2xx body becomes `Value::Null` instead.
  `Wire::get_text` exists for the same family of reasons: a file's `/content` answer is not JSON
  and must not be parsed as any.
- **Percent-encoded path segments instead of an allowlist refusal.** Atlassian's `path_segment`
  refuses anything outside `[A-Za-z0-9_-]` because Jira keys are that by Atlassian's own rules;
  Teams channel ids look like `19:abcdef0123456789@thread.tacv2`, so refusing would make the
  Teams tools unusable against real ids. `path_segment_encode` encodes instead — the same
  encode-don't-refuse strategy `atlassian::client::encode_query_value` already uses, applied to
  a path segment rather than a query value.
- **`GraphSite::parse` keeps a path where `AtlassianSite::parse` refuses one.** Atlassian's rule
  is right for Atlassian, whose `/wiki` and `/rest` prefixes this crate appends itself. Graph's
  documented base URL *is* `https://graph.microsoft.com/v1.0`: the version is part of the address
  an operator is handed, and hard-coding it into every endpoint path would make `/beta`
  unreachable without a code change. A query string and a fragment are still refused. The
  corollary is `site_key`, the one value put in a URL unencoded — `/sites/{key}` accepts a
  `hostname:/path` compound key that percent-encoding would destroy — so the breakout characters
  (`?`, `#`, whitespace, a climbing segment) are refused by name instead.

## Files

- `crates/heddle-connectors/src/m365/mod.rs` (new)
- `crates/heddle-connectors/src/m365/client.rs` (new — the plumbing; written first)
- `crates/heddle-connectors/src/m365/server.rs` (new — the MCP surface)
- `crates/heddle-connectors/src/connector.rs` (`m365_connector`; `serve` untouched)
- `crates/heddle-connectors/src/lib.rs` (exports)
- `crates/heddle-connectors/Cargo.toml` (the `ureq`/`http`/`serde_json` boundary comment extended
  to name `src/m365/` too; no new dependency)
- `crates/heddle-connectors/tests/m365_connector.rs` (new)
- `README.md` (Current status)

## Complexity tracking

None. No new dependency, no new crate, no deviation from an existing pattern in this workspace
beyond the three Graph-forced ones recorded above.
