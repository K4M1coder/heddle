# Feature Specification: Microsoft 365 connector (Outlook, SharePoint, Teams)

**Feature Branch**: `040-m365-connector`

**Created**: 2026-09-05

**Status**: Implemented

**Input**: design doc §4.3 (Connectors), §5.5 (hierarchical enablement); Phase 1 MVP axis 1c
(design doc §8) — the half of that axis whose Atlassian side is
[specs/039](../039-atlassian-connector/spec.md).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read and send Outlook mail from a governed run (Priority: P1)

As an operator with a Microsoft 365 tenant configured, I want a model to read recent mail, read
one message in full, and send a message through the same governed Tool Gateway every other
connector uses, so a run can act on real mailbox state without a second, ungoverned path to it.

**Independent Test**: enable the connector, run a session that lists recent mail, reads one
message by id and sends one; verify each call is a Ledger step and the access token never
appears on the chain or in an error string.

**Acceptance Scenarios**:
1. **Given** the connector is enabled with egress allowed, **When** `tools/list` is called,
   **Then** exactly five tools are advertised: `outlook_read_mail`, `outlook_send_mail`,
   `sharepoint_read_file`, `teams_read_messages`, `teams_send_message`.
2. **Given** no `message_id`, **When** `outlook_read_mail` is called, **Then** the first page of
   recent messages reaches the model as text: each message's subject, sender address and body
   preview.
3. **Given** a `message_id`, **When** `outlook_read_mail` is called, **Then** that one message's
   subject, sender and full body reach the model as text.
4. **Given** a recipient, a subject and a plain-text body, **When** `outlook_send_mail` is
   called, **Then** Graph's `sendMail` shape is posted and the empty `202 Accepted` answer is a
   **success** naming the recipient, not a JSON-parse failure.

### User Story 2 - Read one SharePoint file (Priority: P2)

As an operator, I want a model to read the content of a single SharePoint file, addressed either
by site plus path or by drive-item id, through the same connector.

**Acceptance Scenarios**:
1. **Given** a site id and a file path, **When** `sharepoint_read_file` is called, **Then** the
   file's raw content reaches the model as text, whether or not that content is JSON.
2. **Given** a site id and a drive-item id, **When** `sharepoint_read_file` is called, **Then**
   the same content reaches the model through the id-based address.
3. **Given** neither `path` nor `item_id`, or both, **When** `sharepoint_read_file` is called,
   **Then** it is refused by a message naming both parameters, before any request is made.
4. **Given** a site key carrying a query string or a climbing segment, **When**
   `sharepoint_read_file` is called, **Then** it is refused before any request is made — the site
   key is the one value this connector puts in a URL unencoded, so Graph's `hostname:/path`
   compound form keeps working.

### User Story 3 - Read and send Teams channel messages (Priority: P2)

As an operator, I want a model to read a Teams channel's recent messages and post one back,
through the same connector.

**Acceptance Scenarios**:
1. **Given** a team id and a channel id, **When** `teams_read_messages` is called, **Then** the
   first page of channel messages reaches the model as text: each message's sender display name
   and body.
2. **Given** a team id, a channel id and a body, **When** `teams_send_message` is called,
   **Then** `{"body": {"content": …}}` is posted to that channel and the new message's id
   reaches the model.
3. **Given** a real-shaped channel id such as `19:abcdef0123456789@thread.tacv2`, **When**
   either Teams tool is called, **Then** the id is percent-encoded into exactly one path
   segment rather than refused for containing `:` or `@`.

### User Story 4 - The connector is refused when it may not reach the network (Priority: P1)

As the platform, I must never let this connector open a socket when the run's egress policy
forbids it (Constitution II, NON-NEGOTIABLE; ADR-0002 D4).

**Acceptance Scenarios**:
1. **Given** egress is off, **When** the connector is constructed, **Then** it is refused before
   any connection is opened, naming "Microsoft 365", "egress" and `--allow-egress`.
2. **Given** an unresolvable credential reference, **When** the connector is constructed,
   **Then** it is refused with the `SecretProvider`'s own answer, before any connection is
   opened.
3. **Given** a malformed base URL, **When** the connector is constructed, **Then** it is refused
   naming what was typed, before the credential is resolved.

### Edge Cases

- A Graph error response (e.g. a rejected token, HTTP 401) is a **tool-level** error
  (`isError: true`), not a transport failure — the run survives it, matching every other
  connector's error contract.
- A credential that Graph's own error body echoes back must never reach the model or the
  Ledger; the connector scrubs its own resolved secret from every string it produces,
  independent of the Ledger's own `Redactor` (Constitution V's belt-and-braces).
- A 2xx answer with an **empty** body is a success, not a parse failure: `sendMail` answers
  `202 Accepted` with no content, and a connector that insisted on JSON would report a
  successful send as an error.
- A non-JSON response body from an endpoint that promises JSON is a named tool error
  ("… is not JSON: …"), never a panic. A file's content, which promises nothing, is read as
  text and never parsed.
- A `..` segment in a SharePoint path is refused before any request is made.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The connector MUST expose exactly five MCP tools: `outlook_read_mail`,
  `outlook_send_mail`, `sharepoint_read_file`, `teams_read_messages`, `teams_send_message`.
- **FR-002**: The connector MUST report itself as requiring network access and MUST be refused
  at construction when the run's egress policy is OFF, before an address is parsed or a
  credential resolved.
- **FR-003**: The connector MUST resolve its access token through the existing `SecretProvider`
  trait (a `SecretRef`, never a plaintext value) and MUST NOT let the resolved value appear in
  any Ledger step or any error/tool-outcome string it produces.
- **FR-004**: Graph authentication MUST be `Authorization: Bearer <token>` (the Microsoft
  identity platform's own scheme), never HTTP Basic.
- **FR-005**: A Graph identifier placed in a URL path (message id, team id, channel id,
  drive-item id) MUST be percent-encoded into exactly one segment rather than refused for
  containing characters Jira's simpler ids never carry.
- **FR-006**: A base URL MUST be proved well-formed (scheme, host, no query, no fragment) before
  any request is made. Unlike an Atlassian site it MAY carry a path: Graph's documented base URL
  is `https://graph.microsoft.com/v1.0`, the version prefix is part of the address an operator is
  given, and every endpoint is written relative to it so that `/beta` needs no code change.
- **FR-007**: A 2xx response with an empty body MUST be treated as a success, and a tool whose
  endpoint answers one MUST NOT read a field out of it.
- **FR-008**: A SharePoint site key MUST reach Graph as the operator wrote it, because
  `/sites/{key}` accepts both an id triple and a `hostname:/path` compound key and encoding
  either would stop Graph recognising it; a key carrying a query, a fragment, whitespace or a
  climbing segment MUST be refused instead, before any request is made.

### Key Entities

- **M365Config**: `{ base_url, token: SecretRef }` — the connector's configuration, named by the
  operator. No email field: a Bearer token authenticates alone.
- **M365Server**: the MCP `ServerHandler` implementing the five tools over one `Wire`.
- **Wire**: the authenticated HTTP client — one Graph base URL and one resolved access token.
- **GraphSite**: a base URL proved to name a Graph endpoint (host plus optional version prefix)
  rather than a typo.

## Out of scope for this slice

- **OAuth token acquisition.** The connector receives an already-resolved access token through
  `SecretProvider`, exactly as `heddle-gateway`'s `with_bearer_token` receives a provider's API
  key. Auth-code, client-credentials and device-code flows, and token refresh, are a
  `SecretProvider` backend concern (design §7.13).
- **Excel, OneDrive as a separate surface, and calendar.** SharePoint access is scoped to
  reading one file's content; no folder listing, no upload, no Excel parsing.
- **Site hostname → site-id resolution.** `sharepoint_read_file`'s `site` is the key Graph's
  `/sites/{key}` segment already accepts; the connector performs no separate lookup, keeping
  every tool to exactly one HTTP round trip.
- **Pagination beyond one page.** No `@odata.nextLink` following; a model that needs more raises
  `$top` within Graph's own cap (Constitution VII, YAGNI).
- **Attachments.** Subject, body and recipients only.
- **Hierarchical enablement policy wiring (design §5.5)** — same exclusion specs/039 records:
  this connector implements the `requires_network` signal the policy consults; the policy engine
  is a cross-cutting concern and is not re-implemented per-connector.
