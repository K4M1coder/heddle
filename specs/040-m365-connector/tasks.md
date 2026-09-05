# Tasks: Microsoft 365 connector (Outlook, SharePoint, Teams)

All tasks completed; recorded for the trail, not as a to-do list.

- [X] T01 `GraphSite::parse` — scheme/host/version-prefix/no-query/no-fragment validation,
      unit-tested.
- [X] T02 `Wire` — Bearer auth, error-body scrubbing of the resolved credential, JSON
      `get`/`post`, empty-2xx-body tolerance, and `get_text` for a file's raw content.
- [X] T03 `path_segment_encode`/`sharepoint_path_encode`/`site_key`/`required` — hand-written
      encoding/validation helpers, unit-tested against a real-shaped Teams channel id and a
      traversal segment.
- [X] T04 `M365Config` + `M365Server::connect` — the egress gate, in the order
      egress → address → credential → build.
- [X] T05 `outlook_read_mail`, `outlook_send_mail` — the mail tools, including the empty
      `202 Accepted` success path.
- [X] T06 `sharepoint_read_file` — path-form and id-form addressing over `Wire::get_text`.
- [X] T07 `teams_read_messages`, `teams_send_message` — the channel tools.
- [X] T08 `m365_connector` in `connector.rs` and the `lib.rs` exports, reusing the generic
      `serve` unchanged.
- [X] T09 Acceptance suite (`tests/m365_connector.rs`): tool advertisement, the three
      egress-refusal orderings, all five tools against a real-socket stub, the Bearer header's
      exact bytes, and the credential-never-leaks / malformed-body / isError contract.
- [X] T10 `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D
      warnings` green for the whole workspace with this slice merged.
