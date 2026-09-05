# Tasks: Atlassian connector (Jira + Confluence)

All tasks completed; recorded for the trail, not as a to-do list.

- [X] T01 `AtlassianSite::parse` — scheme/host/no-path/no-query validation, unit-tested.
- [X] T02 `Wire` — HTTP Basic auth, error-body scrubbing of the resolved credential,
      JSON get/post, unit-tested against RFC 4648 base64 vectors.
- [X] T03 `encode_query_value`/`path_segment`/`required` — hand-written encoding/validation
      helpers, unit-tested.
- [X] T04 `AtlassianConfig` + `AtlassianServer::connect` — the egress gate, in the order
      egress → address → credential → build.
- [X] T05 `jira_search`, `jira_get_issue` — read-path tools, including ADF-to-text
      flattening.
- [X] T06 `jira_create_issue`, `jira_add_comment` — write-path tools, including
      plain-text-to-ADF wrapping.
- [X] T07 `confluence_get_page`, `confluence_create_page` — Confluence v2 API tools.
- [X] T08 `atlassian_connector` in `connector.rs`; generalized `serve` to host both
      `EmbeddedServer` and `AtlassianServer` over one runtime-ownership implementation.
- [X] T09 Acceptance suite (`tests/atlassian_connector.rs`, 15 tests): tool advertisement,
      the three egress-refusal orderings, all six tools against a real-socket stub, the
      Basic-auth header's exact bytes, and the credential-never-leaks / malformed-body /
      isError contract.
- [X] T10 `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D
      warnings` green for the whole workspace with this slice merged.
