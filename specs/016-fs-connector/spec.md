# Feature Specification: a real `fs` connector — the first tool a model can actually run (v0 slice)

**Feature Branch:** `016-fs-connector` · **Created:** 2026-09-03 · **Status:** Implemented (v0
slice) **Input:** `specs/015-tool-advertisement/tasks.md` "Next slice" — *"the `fs` connector (slice
016) — a new `crates/skein-connectors` holding a root-bounded embedded rmcp filesystem server, opt-in
via `--fs-root`, wired into both commands with named allowlists. This is what gives this slice a
caller"* · ADR-0004 D3's sixth v0 item (*"MCP tools (fs/git/shell)"*) · Constitution II
(**local-first**, NON-NEGOTIABLE), III (**test-first**), IV (**inverted coupling**), V
(**traceability**), VI (**deny-by-default, confirmation for destructive actions**), VII (**no
capability without a real need**) · design §4.3, §5.4.

Slice 015 built the request path: `ToolGateway::advertise` discovers a transport's catalogue, filters
it to the allowlist, and `NativeLoop::run` stamps the result into every `TurnRequest` of a run, which
`skein-gateway` puts on the wire in OpenAI's `tools` shape. It shipped with **no caller in the
shipped binary**: both loop-running commands still wired `NoTools` behind an empty policy, so
`advertise` returned empty and no `tools` key was serialized anywhere.

This slice gives it something to advertise. It adds one workspace member,
`crates/skein-connectors`, holding a **root-bounded embedded MCP filesystem server** and the
`ToolTransport` that reaches it in-process, and one flag, `--fs-root`, that wires it into
`skein chat` and `skein acp-agent`. After this slice a local model can ask to read a file, the
governed gateway can decide, the server can do it, and the chain records the whole exchange.

## What this slice changes for a user

**Without `--fs-root`: nothing, observably.** Both commands keep today's exact behaviour —
`NoTools`, an empty policy, `advertise` returning empty, and therefore **no `tools` key on the
wire**. SC-010 pins this against the real binary.

**With `--fs-root <DIR>`:** the run gains up to three tools, named to the model with the JSON Schema
the server itself derived from its real parameter types:

| tool | args | `ToolAccess` | `skein chat` | `skein acp-agent` |
|---|---|---|---|---|
| `fs_read` | `path` | `ReadOnly` | allowlisted | allowlisted |
| `fs_list` | `path` | `ReadOnly` | allowlisted | allowlisted |
| `fs_write` | `path`, `content` | `Mutating` | **not allowlisted** | allowlisted **and** `approved` |

**The asymmetry is the point, not an oversight.** `skein chat` is non-interactive: Constitution VI
requires confirmation for a destructive action and there is nobody to ask. Rather than ship a tool
that could only ever be denied, `chat` does not allowlist `fs_write` at all — so it is a genuinely
unlisted tool there and the deny-by-default refusal is provable end to end (SC-006).
`skein acp-agent` has a human behind an editor, so `fs_write` goes in `approved`: the policy stops
gating it and `AcpPermissionTransport` becomes the confirmation gate. That is not a weakening.
`ToolGateway::call_captured` consults the policy **before** the transport, so a `Mutating` tool
absent from `approved` never reaches the ACP permission prompt at all — putting it in `approved` is
the only way to reach the human.

## Five things a reader must know up front

1. **The server is built in-tree, and that was researched rather than assumed.** Principle VII says
   reuse proven tools. The candidates were measured: `mcp-server-filesystem` on crates.io is `0.1.2`
   with **923 total downloads**, last published **2025-09-22**, from a single-author repository — not
   proven and not maintained. The reference `@modelcontextprotocol/server-filesystem` is a **Node
   package**, so depending on it would put a Node runtime and an out-of-process child on
   `skein chat`'s critical path and hand the loopback-only build property back at runtime. What this
   slice needs is three tools over one canonicalized directory: **~150 lines against the official
   `rmcp` SDK**, in exactly the shape `crates/skein-mcp/tests/rmcp_gateway.rs`'s `DownstreamServer`
   already proves works against the real `RmcpToolTransport`.
2. **`FsRoot::resolve` refuses an absolute argument *before* joining, and that ordering is the whole
   containment guarantee.** `Path::join` with an absolute path **discards the base**:
   `root.join("C:\\Windows\\System32")` is `C:\Windows\System32`, which no `starts_with(root)` check
   downstream will ever see as an escape, because by then it is not one — it is a legitimate
   canonicalization of a path outside the root. This is the single most likely bug in the slice and
   it has its own test (SC-001). Only after that refusal does `resolve` join, canonicalize and
   require the `root` prefix — and canonicalization is what also closes `..` traversal **and symlink
   escape**, because a symlink inside the root pointing outside it canonicalizes to its target.
3. **A containment refusal is a tool error, not a transport error.** The three tools return
   `Result<String, String>`, and rmcp's `impl<T: IntoCallToolResult, E: IntoCallToolResult>
   IntoCallToolResult for Result<T, E>` turns the `Err` arm into a `CallToolResult` with
   `is_error = true` (verified in `rmcp-2.2.0/src/handler/server/tool.rs:100`). So a model that asks
   for a path outside the root is **told so and keeps going**; the run survives (SC-007). A refusal
   that killed the run would make every governed conversation one mistake from over.
4. **No schema is hand-written anywhere.** Each `#[tool]` method takes `Parameters<T>` over a real
   `Deserialize + JsonSchema` struct, so the schema the model is shown is derived from the type the
   server will deserialize against. The alternative — typing a `ToolSpec` next to the allowlist —
   would put each schema in the tree twice and let the two drift, which is a runtime failure no
   compiler sees. Slice 015's `advertise` exists precisely so that this is structural.
5. **This is one operator-named root, not a scope hierarchy.** Design §5.5's
   `AccessScope::{Project,Folder,FullComputer}` and its scope-owner resolver **do not exist in
   `crates/`**; verified this slice. `--fs-root` is one directory with no traversal outside it. Say
   so plainly rather than implying the hierarchy landed.

## Functional requirements

- **FR-001** `crates/skein-connectors` is a new workspace member depending on `rmcp` (`server` +
  `macros`), `tokio` (`rt-multi-thread`), `schemars`, `serde`, `serde_json`, `skein-core` and
  `skein-mcp`. It is the **only** crate in the product that names MCP as a **server**.
- **FR-002** `skein-mcp`'s docstring invariant is amended, in both crates, from *"the only crate in
  the product that names the MCP protocol"* to **the only crate naming MCP as a client**. An
  invariant left stale is worse than one restated.
- **FR-003** `FsRoot::new(path)` canonicalizes at construction and **fails loudly** on a path that
  does not exist or is not a directory. `FsRoot::resolve(arg)` returns `Result<PathBuf, String>` and:
  refuses an **absolute** `arg` before joining; refuses an `arg` with no usable components; joins,
  canonicalizes, and requires the canonical result to start with the canonical root.
- **FR-004** `FsRoot::resolve_new(arg)` is the not-yet-existing-file case for `fs_write`: it
  canonicalizes the **parent** directory, checks *that* against the root, and re-appends the file
  name. A path whose parent does not exist is refused rather than created.
- **FR-005** `FsServer` is an rmcp `ServerHandler` with `#[tool_router]` and exactly three `#[tool]`
  methods — `fs_read`, `fs_list`, `fs_write` — each taking `Parameters<T>` over a
  `Deserialize + JsonSchema` struct and returning `Result<String, String>`.
- **FR-006** `fs_read` enforces a byte cap and refuses a larger file with a message naming the cap.
  A tool result travels into the next request's payload and onto the chain, so an unbounded read is
  an unbounded prompt and an unbounded Ledger row.
- **FR-007** `fs_list` is **non-recursive**: one directory's entries, each marked `dir` or `file`,
  sorted. No glob, no depth argument.
- **FR-008** `fs_write` replaces a file's whole contents under the root and reports the byte count.
- **FR-009** `RmcpToolTransport::list` overrides 015's defaulted body and forwards MCP's
  `tools/list`, mapping each `Tool` onto a `ToolSpec`. Without it a real MCP server's catalogue is
  unreachable and every connector advertises nothing.
- **FR-010** `LocalConnector` implements `ToolTransport` by delegating `call` and `list` to an
  `RmcpToolTransport` connected over an in-process duplex to an `FsServer` on its own runtime.
  **Field order is load-bearing**: the transport is declared before the `Runtime` so the client is
  torn down while the runtime driving the server task is still alive.
- **FR-011** `fs_connector(root)` is the one constructor. No method of `LocalConnector` may be called
  from inside a tokio context: `Runtime::block_on` panics when a runtime is already entered.
- **FR-012** `skein-cli` gains `wiring::ToolArgs { fs_root: Option<PathBuf> }`, flattened into
  `ChatArgs` and the `AcpAgent` command as `--fs-root`, with `transport()`, `chat_policy()` and
  `agent_policy()`. `ConfiguredTools::{None, Fs(Box<LocalConnector>)}` implements `ToolTransport`;
  the `Fs` variant is boxed because `LocalConnector` holds a `Runtime` and `NoTools` is zero-sized,
  which is `clippy::large_enum_variant` and CI runs `clippy -D warnings`.
- **FR-013** `FsRoot::new` runs **after** the endpoint guard and the redactor and **before**
  `Silo::open`, in both commands: a `--fs-root` that does not exist is exit 1 with **no ledger file
  created**.

## Success criteria

- **SC-001** `FsRoot::resolve` refuses an **absolute** argument — the `Path::join` footgun, on every
  OS, with no `#[cfg]` in the test.
- **SC-002** `FsRoot::resolve` refuses a `..` escape, refuses a symlink pointing outside the root,
  and accepts an in-root relative path. `FsRoot::new` fails on a missing directory.
- **SC-003** `fs_read` refuses a file over the byte cap with a message naming the cap.
- **SC-004** `LocalConnector::list` returns the three tools, each with a non-empty derived schema
  naming its real parameters.
- **SC-005** **The headline test.** In one run, with **no test double standing in for the tool
  server**: the first request's `tools` array names `fs_read`/`fs_list` with their derived schemas; a
  stub provider answers with a `tool_calls` reply asking for `fs_read`; the real
  `OpenAiCompatClient` → real `NativeLoop` → real `ToolGateway` → real `LocalConnector` → real
  `FsServer` chain returns the file's actual contents; the second request's messages carry
  `[tool_result tool=fs_read status=ok]` with those contents; and the run's Ledger holds
  `ToolCall`/`Approval`/`ToolResult` with `verify_chain` passing.
- **SC-006** An `fs_write` request under `chat_policy` **never reaches the server** — the server's
  own invocation counter is the ground truth — and the run survives with the model told `denied`.
- **SC-007** An out-of-root `fs_read` is refused **by the server** as a tool error (`is_error`) and
  the run survives.
- **SC-008** A configured secret that is the **contents of a file on disk**, read through the
  connector, appears in **no** Ledger payload of the run, and at least one payload contains `***`.
- **SC-009** `skein chat --fs-root <dir>` (the **real binary**) sends `tools` on the wire and reads a
  real file under the root.
- **SC-010** `skein chat` without `--fs-root` sends **no `tools` key** and behaves exactly as before.
- **SC-011** `skein chat --fs-root <nonexistent>` exits 1 with **no ledger file created**.
- **SC-012** `skein acp-agent --fs-root <dir>` starts, drives a session, and lists the flag in
  `--help`.
- **SC-013** All 132 pre-existing tests pass with **no assertion changed or removed**. The only edits
  to pre-existing test files are the two `StubProvider` helpers becoming observable, which SC-009 and
  SC-010 cannot be written without.
- **SC-014** `git diff dev -- crates/skein-silo/ spikes/ .github/ rust-toolchain.toml` is **empty**.

## Assumptions and residuals

- **TOCTOU between `canonicalize` and `File::open` is a recorded residual, not a closed hole.** A
  symlink swapped into the window escapes the root. Closing it needs `cap-std`-style
  directory-handle-relative opens, which this slice deliberately does not add. This is the same
  species of honest residual `LocalEndpoint::parse`'s docstring already records about `ureq`
  re-resolving DNS after the loopback check, and it is recorded the same way.
- **Automatic secret detection in file contents is out of scope and the gap is stated plainly.**
  `Redactor` is an exact-value scrubber. SC-008 proves the **configured** case works; a credential in
  a file the operator never registered still lands in the chain in cleartext.
- **One tokio runtime per ACP session.** Each session builds its own connector, matching `acp.rs`'s
  existing one-client-per-session shape. Sessions are few in v0; the cost is stated rather than
  hidden.
- **`Runtime::block_on` panics inside an entered tokio runtime**, so both call sites were traced:
  `skein chat` builds the connector on the main thread, and `skein acp-agent` builds it inside
  `SkeinAgent::open`, which runs under `futures::executor::block_on`, not a tokio context. Restated
  in `LocalConnector`'s docstring.
- **ADR-0004 D3 closes for `fs` and remains open for `git` and `shell`.** Said here rather than
  claiming the item done.
- **The Windows leg is observed locally; the macOS and Linux legs are unobserved** until the
  repository has a remote — the standing caveat of specs 004–015. Its bite is larger here than in any
  prior slice, because this is the first slice whose core logic is filesystem semantics. No `#[cfg]`
  appears in the containment code, so the same bodies must pass everywhere.

## Out of scope

Deliberately not done, so nobody helpfully does it:

- **A `git` connector.** Implementing `git status`/`git log` means shelling out to `git` — which *is*
  shell execution wearing a different name, with the argument-construction surface intact — or adding
  `git2` (libgit2 C bindings, a tri-OS build burden) or `gix` (large). Neither is justified when a
  model that can read files can already read repository state. YAGNI.
- **A `shell` connector, and the reason is risk rather than convenience.** Shell is categorically
  different from the other two. `fs`'s blast radius is bounded by one canonicalized directory;
  shell's is bounded by nothing the tree has — no sandbox, no timeout machinery, no environment
  scrubbing, and no access-scope hierarchy. An allowlist of command *names* is not an allowlist of
  *effects*: `git`, `npm`, `cargo` and `python` all reach arbitrary code through arguments and config
  files, so an allowlisted-binary shell tool is an unrestricted shell with extra steps.
  Constitution VI requires confirmation for destructive actions and `skein chat` has nobody to ask.
  And Constitution II is NON-NEGOTIABLE about egress: `skein-gateway` makes local-only a property of
  the *build* by compiling in no TLS backend, and a shell tool would hand that guarantee back at
  runtime, silently, the first time a model ran `curl`. **Shell gets its own slice, after an
  access-scope boundary exists.**
- **Design §5.4/§5.5's connector configuration and enablement hierarchy**, the scope-owner resolver,
  and `AccessScope::{Project,Folder,FullComputer}`. One operator-named root, nothing hierarchical.
- **The trust registry (design §7.6)** and any external-binary connector packaging.
- **Non-`fs`/`git`/`shell` connectors** (Atlassian, M365) — out of v0 per ADR-0004 D3.
- **Filesystem breadth**: no glob, no recursive listing, no rename, delete, mkdir or
  `create_dir_all`, no diffs, no MIME sniffing, no ZIP, no watch. Three tools.
- **Deriving `ToolAccess` from MCP tool annotations.** Classification stays operator configuration.
- **Closing the TOCTOU residual** with `cap-std`; **automatic secret detection in file contents**.
- **`role: "tool"` / `tool_call_id` conversation replay**, `strict: true`, `tool_choice`, parallel
  tool calls, streaming (SSE) — all carried unchanged from slice 015.
- **Raw wire-byte capture**, provider authentication, a config file, `--json` output.
- **`crates/skein-silo/`, `spikes/`** (ADR-0004 D2), **`.github/`, `rust-toolchain.toml`.**
