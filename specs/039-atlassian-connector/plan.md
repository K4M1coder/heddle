# Implementation Plan: Atlassian connector (Jira + Confluence)

**Spec**: [spec.md](spec.md) · **Branch**: `039-atlassian-connector`

## Approach

A new `atlassian` submodule inside `crates/heddle-connectors` (not a separate crate: it is one
more `ServerHandler` served the same way `EmbeddedServer` is, via the crate's existing
in-process duplex transport — a second crate would buy nothing and would cost a second
`Cargo.toml` to keep `ureq`'s no-TLS-backend posture consistent with).

- `atlassian/client.rs` — the transport half: `AtlassianSite::parse` (proves the address
  well-formed, no local-only restriction because there is no local Jira), `Wire` (the
  authenticated `ureq::Agent`, HTTP Basic over `email:token`, error-body scrubbing of the
  resolved credential), and three small hand-written helpers (`base64`, `encode_query_value`,
  `path_segment`/`required`) rather than new dependencies, matching this crate's and
  `heddle-gateway`'s recorded "twenty lines rather than a dependency" precedent.
- `atlassian/server.rs` — `AtlassianConfig`, the egress gate (`AtlassianServer::connect`), and
  the six `#[tool]` methods, using the same `#[tool_router]`/`#[tool_handler]` rmcp macros
  `EmbeddedServer` already uses.
- `connector.rs` — `pub fn atlassian_connector(config, secrets, egress_allowed) ->
  Result<LocalConnector>`, mirroring `local_connector`/`local_connector_with_run`. `serve` (the
  duplex-hosting helper) is generalized from `fn serve(server: EmbeddedServer)` to
  `pub(crate) fn serve<S: ServerHandler>(server: S)` so both connectors share one runtime-
  ownership implementation rather than two copies of the same fifteen lines.

## Egress gate ordering (Constitution II, NON-NEGOTIABLE)

Mirrors `heddle_gateway::Router::client_for`'s recorded order exactly, because it is the same
proof for the same reason:

1. **egress** — this connector has no local form, so refusing here needs neither an address nor
   a credential to already be worth reporting;
2. parse the site address;
3. resolve the credential;
4. build the wire (`AtlassianServer`, held behind an `Arc` inside `LocalConnector`).

## Test strategy

Same discipline as `heddle-gateway/tests/provider_routing.rs`: a real `std::net::TcpListener`
stub that counts `accept()`s (not parsed requests), never an HTTP-mocking crate — the claim this
connector's egress tests make ("no connection was opened") a mock cannot express. The stub
answers with `email:token`-shaped fixtures Jira Cloud and Confluence's v2 API document.

## Files

- `crates/heddle-connectors/src/atlassian/mod.rs` (new)
- `crates/heddle-connectors/src/atlassian/client.rs` (new — the plumbing; written first)
- `crates/heddle-connectors/src/atlassian/server.rs` (new — the MCP surface; the gap this plan
  closes)
- `crates/heddle-connectors/src/connector.rs` (`atlassian_connector`, generalized `serve`)
- `crates/heddle-connectors/src/lib.rs` (exports)
- `crates/heddle-connectors/Cargo.toml` (`ureq`, `http`, `serde_json` as product dependencies,
  `heddle-gateway`'s recorded no-default-features TLS reasoning restated for why this crate
  must keep it too)
- `crates/heddle-connectors/tests/atlassian_connector.rs` (new)

## Complexity tracking

None. No deviation from an existing pattern in this workspace.
