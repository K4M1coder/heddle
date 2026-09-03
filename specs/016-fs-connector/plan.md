# Implementation Plan: the `fs` connector (slice 016)

**Spec:** `specs/016-fs-connector/spec.md` · **Branch:** `016-fs-connector`, cut from `dev` with
slice 015 merged (`c4da8f7`) · **Tasks:** `specs/016-fs-connector/tasks.md`

Slice 015 shipped the advertisement path with no caller. This slice is the caller: a new workspace
member holding an embedded MCP filesystem server bounded to one canonicalized directory, and a
`--fs-root` flag that wires it into both loop-running commands under named allowlists.

## Decisions

### D1 — Build the fs server in-tree; do not depend on a third-party MCP server

Principle VII says reuse proven tools rather than rewrite them, so the candidates were researched
rather than assumed:

- **`mcp-server-filesystem` (crates.io)** — `0.1.2`, **923 total downloads**, last published
  **2025-09-22**, single-author repository. Not proven, not actively maintained. Rejected.
- **`@modelcontextprotocol/server-filesystem`** — the reference implementation, and a **Node
  package**. Depending on it puts a Node runtime and an out-of-process child on `skein chat`'s
  critical path, and hands back at runtime the local-only guarantee `skein-gateway` makes a property
  of the build. Rejected on Constitution II grounds, not on taste.
- **Build it** — three tools over one canonicalized directory, ~150 lines against the official
  `rmcp` SDK's `#[tool_router]`/`#[tool]` macros, in exactly the shape
  `crates/skein-mcp/tests/rmcp_gateway.rs`'s `DownstreamServer` already proves works against the
  real `RmcpToolTransport`. **Chosen.**

The decisive argument is not size. It is that a *bounded* filesystem server is the whole point: what
makes this safe is `FsRoot`, and no third-party server would be enforcing our containment rule.

### D2 — `FsRoot::resolve` refuses an absolute argument **before** joining

```rust
pub struct FsRoot { root: PathBuf }   // canonicalized at construction
impl FsRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<FsRoot>;             // canonicalize; missing = loud failure
    pub fn resolve(&self, arg: &str) -> std::result::Result<PathBuf, String>;
    pub fn resolve_new(&self, arg: &str) -> std::result::Result<PathBuf, String>;
}
```

`resolve`, in order:

1. **Refuse an absolute `arg` outright, before joining.** `Path::join` with an absolute path
   *discards the base* — `root.join("C:\\Windows\\…")` yields `C:\Windows\…`. This is the single most
   likely bug in the slice and it has its own test.
2. Join, then `fs::canonicalize`, then require `starts_with(self.root)`. Canonicalization resolves
   `..` **and symlinks**, so a symlink inside the root pointing outside it is refused. On Windows
   both sides are `\\?\`-verbatim because both went through `canonicalize`, so the prefix comparison
   compares like with like.
3. `resolve_new`, for `fs_write` to a not-yet-existing file: canonicalize the **parent**, check
   *that*, then re-append the file name. A path whose parent does not exist is refused rather than
   created.
4. A refusal returns `Err(String)`, which rmcp turns into `is_error: true` (D3).

**Residual, recorded rather than hidden:** TOCTOU between `canonicalize` and `File::open` — a symlink
swapped in that window escapes the root. Closing it needs `cap-std`-style directory-handle-relative
opens. This is the same species of honest residual `LocalEndpoint::parse`'s docstring already records
about `ureq` re-resolving DNS after the loopback check.

### D3 — The three tools return `Result<String, String>`

rmcp's `impl<T: IntoCallToolResult, E: IntoCallToolResult> IntoCallToolResult for Result<T, E>` turns
the `Err` arm into a `CallToolResult` with `is_error = true` (verified in
`rmcp-2.2.0/src/handler/server/tool.rs:100`). So a containment refusal is a **tool-level** error the
model is told about, not a transport error that kills the run.

Each method takes `Parameters<T>` over a real `Deserialize + JsonSchema` struct, so the JSON Schema
the model is shown is **derived from the type the server deserializes against**. Never hand-written:
two copies of a schema drift, and the drift is a runtime failure no compiler sees.

`fs_read` enforces a byte cap. A tool result travels into the next request's payload and onto the
chain, so an unbounded read is an unbounded prompt and an unbounded Ledger row. `fs_list` is
non-recursive.

### D4 — `RmcpToolTransport::list` has to exist

**This is a refinement of the plan the slice was cut from, and it is load-bearing.** Slice 015 made
`ToolTransport::list` a *defaulted* method and deliberately left `RmcpToolTransport` inheriting the
empty default, because 015 shipped no server. `LocalConnector` delegates `list` to
`RmcpToolTransport` — so without a real override, delegation returns nothing and every connector
advertises nothing, silently, with everything compiling.

`skein-mcp` is where it belongs: it is the MCP client, `tools/list` is an MCP method, and
`RunningService<RoleClient, ()>` derefs to `Peer<RoleClient>`, which has `list_all_tools`. Mapping
`rmcp::model::Tool` onto `ToolSpec` takes three fields; `description` is `Option` on the wire and
becomes the empty string when a server omits it.

### D5 — `crates/skein-connectors`, a new workspace member

*Rejected: putting the server in `skein-mcp`.* That crate's product dependencies are deliberately
client-only (`rmcp` with `client` + `transport-async-rw`); `server`, `macros` and `schemars` are
**dev-dependencies today**. Promoting them to product dependencies to host a connector would thicken
the client port for a reason unrelated to being a client.

The cost is that `skein-mcp`'s docstring invariant — *"the only crate in the product that names the
MCP protocol"* — becomes false and must be amended in both crates, to: **`skein-mcp` is the only
crate naming MCP as a client; `skein-connectors` is the only one naming it as a server.** That
amendment is part of the work, not an afterthought — an invariant left stale is worse than one
restated.

```rust
pub struct LocalConnector { transport: RmcpToolTransport, _runtime: Runtime }  // field order is load-bearing
impl ToolTransport for LocalConnector { /* call + list delegate */ }
pub fn fs_connector(root: FsRoot) -> Result<LocalConnector>;
```

**Field order is load-bearing**: `transport` is declared first so the client is torn down before the
runtime driving the server task — the same reasoning `RmcpToolTransport`'s own field comment records,
and the same fact `rmcp_gateway.rs`'s *"the returned runtime must be bound to a named local for the
whole test body"* warning depends on.

`Runtime::block_on` panics inside an entered **tokio** runtime. Both call sites were traced:
`skein chat` builds the connector on the main thread; `skein acp-agent` builds it inside
`SkeinAgent::open`, which runs under `futures::executor::block_on` (see `serve_stdio`), not a tokio
context. Both safe. The hazard is restated in `LocalConnector`'s docstring anyway.

`SkeinAgent` requires `T: ToolTransport + Send + 'static`. `RunningService<RoleClient, ()>`'s fields
are all `Send` and `Runtime` is `Send`, so `LocalConnector: Send` holds.

### D6 — Opt-in via `--fs-root`, with different allowlists per command

Absent `--fs-root`, both commands keep today's exact behaviour (`NoTools` + empty policy). Present,
they get the connector.

| tool | `ToolAccess` | `skein chat` | `skein acp-agent` |
|---|---|---|---|
| `fs_read` | `ReadOnly` | allowlisted | allowlisted |
| `fs_list` | `ReadOnly` | allowlisted | allowlisted |
| `fs_write` | `Mutating` | **not allowlisted** | allowlisted **and** in `approved` |

The asymmetry is documented at both wiring sites:

- **`skein chat` is non-interactive.** Constitution VI requires confirmation for destructive actions
  and there is nobody to ask. Rather than ship a tool that can only ever be denied, `chat` does not
  allowlist it at all — so `fs_write` is a genuinely unlisted tool there, and the deny-by-default
  refusal is provable end to end.
- **`skein acp-agent` has a human behind an editor.** `fs_write` goes in `approved` so the policy
  stops gating it and `AcpPermissionTransport` becomes the confirmation gate. The ordering fact that
  makes this necessary: `ToolGateway::call_captured` consults the policy **before** the transport, so
  a `Mutating` tool absent from `approved` never reaches the ACP permission prompt at all. Putting it
  in `approved` is not weakening the policy; it is the only way to reach the human.

```rust
pub struct ToolArgs { pub fs_root: Option<PathBuf> }
impl ToolArgs {
    pub fn transport(&self) -> Result<ConfiguredTools>;
    pub fn chat_policy(&self) -> ToolPolicy;    // fs_read, fs_list
    pub fn agent_policy(&self) -> ToolPolicy;   // + fs_write, approved
}
pub enum ConfiguredTools { None, Fs(Box<LocalConnector>) }
impl ToolTransport for ConfiguredTools { /* call + list */ }
```

`Box` on the variant pre-empts `clippy::large_enum_variant` (`LocalConnector` holds a `Runtime`;
`NoTools` is zero-sized) — CI runs `clippy -D warnings`. An enum, not `Box<dyn ToolTransport>`,
because `NativeLoop` is generic over `T` by deliberate design and the dispatch is a two-arm match.

**Ordering:** `FsRoot::new` runs **after** the endpoint guard and the redactor, **before**
`Silo::open` — the rule both commands already document: a refusal must leave no one-step run in a
chain. A `--fs-root` that does not exist is exit 1 with no chain opened.

### D7 — `fs` only. Not `git`. Not `shell`.

The reasoning is in the spec's "Out of scope", where a reader looking for *"why is there no shell
tool?"* will find it. In one line: `fs`'s blast radius is one canonicalized directory, shell's is
bounded by nothing this tree has, and an allowlist of command *names* is not an allowlist of
*effects*.

**ADR-0004 D3's "MCP tools (fs/git/shell)" closes for `fs` and remains open for `git` and `shell`.**

## Steps

- **T0** `spec.md`, `plan.md`, `tasks.md`.
- **T1** Control baseline: `cargo test --workspace`, recorded verbatim.
- **T2 · RED→GREEN** `crates/skein-connectors` created: manifest, `lib.rs` docstring carrying D5's
  amended MCP invariant, `FsRoot`. Containment tests in `crates/skein-connectors/tests/fs_root.rs`.
  **No `#[cfg]`** — the same bodies must pass on all three OSes.
- **T3 · RED→GREEN** `FsServer` with `#[tool_router]` and the three `#[tool]` methods.
- **T4 · RED→GREEN** `RmcpToolTransport::list` (D4), then `LocalConnector` / `fs_connector`.
- **T5 · RED→GREEN** The headline acceptance test in
  `crates/skein-connectors/tests/governed_fs_run.rs`, with `skein-gateway` as a dev-dependency.
- **T6 · RED→GREEN** The refusal twins: unlisted `fs_write` never reaches the server; an out-of-root
  read is refused by the server and the run survives.
- **T7 · RED→GREEN** Redaction composition: a secret in a file's **contents**, scrubbed from the
  chain.
- **T8 · RED→GREEN** `skein-cli`: `wiring::ToolArgs`, `ConfiguredTools`, the two policies,
  `--fs-root` on both commands, resolution ordered per D6.
- **T9 · RED→GREEN** CLI acceptance tests against the real binary.
- **T10** `#[ignore]`d live test, gated on `SKEIN_LIVE_MODEL`, following `openai_compat.rs`'s
  precedent.
- **T11** Gates, control diff, dependency drift. **This slice adds packages** — record the delta
  honestly rather than claiming zero.
- **T12** Hand-verification against live Ollama. **Not part of the implementation run**; performed
  and recorded separately.

## Validation

The project's own gates, per ADR-0004 D1(c)/(d): `cargo fmt --all --check`;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`;
`cargo build --workspace`. Tri-OS CI (`.github/workflows/core.yml`) picks up
`crates/skein-connectors` automatically — the workspace is `members = ["crates/*"]` and the
workflow's `paths:` already reads `crates/**`, so no CI file changes.

Every pre-existing test body must be unchanged. `cli_chat.rs`'s `StubProvider` gains request
observability, which is additive.

## Risks

| Risk | Mitigation |
|---|---|
| **Root escape via the `Path::join`-absolute footgun.** | Its own test, and the refusal happens before the join, not after. |
| **`RmcpToolTransport` silently inherits 015's empty `list`**, so every connector advertises nothing and nothing fails to compile. | D4: a real override, driven by T4's catalogue test. |
| **TOCTOU symlink swap between `canonicalize` and open.** | Accepted and recorded as a residual, in `LocalEndpoint::parse`'s idiom. Closing it needs `cap-std`; not this slice. |
| **`LocalConnector` unexpectedly not `Send`**, so `skein acp-agent` will not compile. | `RunningService`'s fields were checked and are all `Send`. Fallback: land `--fs-root` on `chat` only and split the ACP wiring. Do **not** reach for `Arc<Mutex<…>>`. |
| **`Runtime::block_on` panic inside an async context.** | Both call sites traced (`chat` = main thread; `acp` = `futures::executor::block_on`). Restated in the docstring. |
| **One tokio runtime per ACP session.** | Accepted for v0, matching `acp.rs`'s one-client-per-session shape. Cost stated in the spec. |
| **`clippy::large_enum_variant` on `ConfiguredTools`.** | `Box` the `Fs` variant from the start. |
| **A local model ignores or mangles the `tools` array.** | The stubbed tests do not depend on model competence at all. T12 names a tool-capable model explicitly. |
