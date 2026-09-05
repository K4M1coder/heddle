# Feature Specification: Atlassian connector (Jira + Confluence)

**Feature Branch**: `039-atlassian-connector`

**Created**: 2026-09-05

**Status**: Implemented

**Input**: design doc §4.3 (Connectors), §5.5 (hierarchical enablement); Phase 1 MVP axis 1c (design doc §8).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Read and act on Jira issues from a governed run (Priority: P1)

As an operator with an Atlassian site configured, I want a model to search, read, create and
comment on Jira issues through the same governed Tool Gateway every other connector uses, so a
run can act on real tracker state without a second, ungoverned path to it.

**Independent Test**: enable the connector, run a session that searches for an issue, reads it,
creates a new one and comments on it; verify each call is a Ledger step and the API token never
appears on the chain or in an error string.

**Acceptance Scenarios**:
1. **Given** the connector is enabled with egress allowed, **When** `tools/list` is called,
   **Then** exactly six tools are advertised: `jira_search`, `jira_get_issue`,
   `jira_create_issue`, `jira_add_comment`, `confluence_get_page`, `confluence_create_page`.
2. **Given** a real Jira issue key, **When** `jira_get_issue` is called, **Then** the summary,
   status and description (flattened from Atlassian Document Format) reach the model as text.
3. **Given** a description or comment body as plain text, **When** `jira_create_issue` or
   `jira_add_comment` is called, **Then** the text is wrapped as an ADF document before it is
   sent, matching what Jira's v3 API requires.

### User Story 2 - Read and publish Confluence pages (Priority: P2)

As an operator, I want a model to read an existing runbook page and publish a new one in storage
format, through the same connector.

**Acceptance Scenarios**:
1. **Given** a page id, **When** `confluence_get_page` is called, **Then** the title and the
   storage-format body reach the model as text.
2. **Given** a space id, a title and an HTML-like body, **When** `confluence_create_page` is
   called, **Then** the page is created with `status: "current"` and the new page's id reaches
   the model.

### User Story 3 - The connector is refused when it may not reach the network (Priority: P1)

As the platform, I must never let this connector open a socket when the run's egress policy
forbids it (Constitution II, NON-NEGOTIABLE; ADR-0002 D4).

**Acceptance Scenarios**:
1. **Given** egress is off, **When** the connector is constructed, **Then** it is refused before
   any connection is opened, naming "Atlassian", "egress" and `--allow-egress`.
2. **Given** an unresolvable credential reference, **When** the connector is constructed,
   **Then** it is refused with the `SecretProvider`'s own answer, before any connection is
   opened.
3. **Given** a malformed base URL, **When** the connector is constructed, **Then** it is refused
   naming what was typed, before the credential is resolved.

### Edge Cases

- A site's error response (e.g. a rejected token, HTTP 401) is a **tool-level** error
  (`isError: true`), not a transport failure — the run survives it, matching every other
  connector's error contract.
- A credential that a site's own error body echoes back must never reach the model or the
  Ledger; the connector scrubs its own resolved secret from every string it produces,
  independent of the Ledger's own `Redactor` (Constitution V's belt-and-braces).
- A non-JSON response body is a named tool error ("... is not JSON: ..."), never a panic.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The connector MUST expose exactly six MCP tools: `jira_search`,
  `jira_get_issue`, `jira_create_issue`, `jira_add_comment`, `confluence_get_page`,
  `confluence_create_page`.
- **FR-002**: The connector MUST report itself as requiring network access and MUST be refused
  at construction when the run's egress policy is OFF, before an address is parsed or a
  credential resolved.
- **FR-003**: The connector MUST resolve its API token through the existing `SecretProvider`
  trait (a `SecretRef`, never a plaintext value) and MUST NOT let the resolved value appear in
  any Ledger step or any error/tool-outcome string it produces.
- **FR-004**: Jira authentication MUST be HTTP Basic over `email:api_token` (Jira Cloud's own
  scheme), not Bearer.
- **FR-005**: A Jira issue's description and a comment's body MUST be accepted as plain text
  and wrapped as an Atlassian Document Format document before being sent; a document read back
  MUST be flattened to plain text for the model.
- **FR-006**: A base URL MUST be proved well-formed (scheme, host, no path, no query) before
  any request is made.

### Key Entities

- **AtlassianConfig**: `{ base_url, email, token: SecretRef }` — the connector's configuration,
  named by the operator.
- **AtlassianServer**: the MCP `ServerHandler` implementing the six tools over one `Wire`.
- **Wire**: the authenticated HTTP client — one site, one email, one resolved credential.

## Out of scope for this slice

- Bitbucket and Trello (design §4.3 names Jira/Confluence/Bitbucket under "Atlassian"; this
  slice covers the two named in the MVP brief).
- Hierarchical enablement policy wiring (design §5.5) — this connector implements the
  `requires_network` signal the policy consults; the policy engine itself is a cross-cutting
  concern shared by every connector and is not re-implemented per-connector.
