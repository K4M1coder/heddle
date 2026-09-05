# Tasks: `heddle acp-agent` — the ACP facade as a running process (v0 slice)

**Spec:** `specs/013-acp-agent/spec.md` · TDD (red→green), product code in `crates/heddle-cli` and
`crates/heddle-acp` plus one additive error variant in `crates/heddle-core`, branch `013-acp-agent`
cut from `dev` after slice 012 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ the ACP facade is a library and `heddle acp-agent` is a wiring-only subcommand
  that holds no capability of its own. The one thing the CLI needed that the API lacked — a name for
  "the protocol transport failed" — is closed by *adding to `HeddleError`*, not by reaching around it,
  the same move slice 012 made · II Local-first ✅ NON-NEGOTIABLE, and **reused rather than
  reimplemented**: the same `LocalEndpoint::parse` guard and the same no-TLS-by-construction `ureq`
  build, now with exactly one copy of the resolution logic shared by both commands (`wiring.rs`)
- III Test-First ✅ T2's red observed before its green, T3's before its green, and T6's subprocess
  test is T4's red — recorded as such in `## Observed red` rather than substituted with a weaker
  in-process one · IV Inverted coupling ✅ ACP names stay inside `heddle-acp` (`serve_stdio` is why),
  the executor choice stays there too (mirroring `heddle-mcp`'s `RmcpToolTransport`), `heddle-core`
  gains no dependency, and the one public-API change is stated and justified (plan D3)
- V Traceability ✅ every ACP session's runs land on a real silo chain and verify **from a second
  process**. The known gap is carried forward unchanged: the chain holds the *translated*
  `TurnRequest`/`TurnResponse`, and model I/O is **not** redacted
- VI Security ✅ no credential path in this slice; the deferral keeps its pre-written constraint (a
  provider token MUST arrive as a `SecretRef` through `SecretProvider`)
- VII Neutrality ✅ one subcommand, one method, one error variant, zero new packages. **No new ACP
  method, no streaming, no tools, no auth, no config file, no `--json`.**
- VIII Loop discipline ✅ NON-NEGOTIABLE and unchanged: real provider metering or a failed turn,
  `finish_reason: "length"` is not final, and the `ProgressProbe` returns `false` because a tool-less
  session has no external ground truth
- Cross-platform ✅ no `#[cfg]` in any new file; the stub binds `127.0.0.1:0`; subprocess spawning
  and process-group teardown are the dependency's concern (`AcpAgent::spawn_process` uses
  `process_group(0)` on Unix and `CREATE_NO_WINDOW` on Windows). `core.yml`'s `paths:` already covers
  `crates/**` and `Cargo.toml`, and this slice adds no workspace member — confirmed by reading, not
  edited.

## Tasks
- [x] **T0** `specs/013-acp-agent/{spec.md,plan.md,tasks.md}`; branch `013-acp-agent` cut from `dev`
      with slice 012 merged
- [x] **T1** control baseline: `cargo test --workspace` before any edit — **105 passed, 1 ignored**
- [x] **T2** RED→GREEN — `HeddleError::Protocol` with its own test. Ordered first because
      `serve_stdio` cannot compile without it
- [x] **T3** RED→GREEN — the fallible factory (plan D3), with
      `a_factory_that_fails_makes_session_new_fail_and_leaves_the_connection_usable` as its
      justifying test
- [x] **T4** GREEN — `HeddleAgent::serve_stdio` (plan D1/D2), plus `futures` in
      `[workspace.dependencies]` and in `heddle-acp`'s `[dependencies]`. Its red is T6
- [x] **T5** refactor — `crates/heddle-cli/src/wiring.rs` (plan D4), with the five `cli_chat.rs`
      tests unchanged as the control
- [x] **T6** RED — `crates/heddle-cli/tests/cli_acp_agent.rs`, a real ACP client spawning the real
      binary, against the not-yet-existing `acp-agent` subcommand
- [x] **T7** GREEN — `crates/heddle-cli/src/acp.rs`, the `AcpAgent` subcommand, and the `main.rs`
      docstring correction
- [x] **T8** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`, `cargo build --workspace`, + `heddle acp-agent --help`
- [x] **T9** control diff, dependency drift, close-out

## Control baseline (T1)

`cargo test --workspace` on `013-acp-agent` @ `2e41492` (identical to `dev`), working tree clean,
2026-09-03, before any edit: **105 passed, 0 failed, 1 ignored** — `acp_session` 13, `cli_chat` 6,
`cli_ledger` 8, `cli_secret` 2, `core` 14, `native_loop` 18, `tool_gateway` 9, `governed_run` 2,
`openai_compat` 14 (+1 ignored, the optional live-Ollama test), `rmcp_gateway` 7, `silo_ledger` 7,
`silo_secret` 5. The five `src/lib.rs`/`src/main.rs` unit-test targets and the five doc-test targets
each contribute `0 passed`. This is the number T8 diffs against, and it matches slice 012's recorded
gate figure exactly.

## Observed red (Constitution III)

All on 2026-09-03.

- **T2** `cargo test -p heddle-core --test core` before the variant existed:
  - `error[E0599]: no variant, associated function, or constant named Protocol found for enum
    HeddleError in the current scope`
  - `error: could not compile heddle-core (test "core") due to 1 previous error`
- **T3** `cargo test -p heddle-acp --test acp_session` against the infallible factory bound:
  - `error[E0308]: mismatched types` — *expected `SessionParts<_, _, _>`, found
    `Result<_, HeddleError>`*, twice: once on the `Err` arm and once on the `Ok` arm, with
    `note: return type inferred to be SessionParts<_, _, _> here`
  - The green then flipped **14 passed** where 13 had passed, and `git diff` on the thirteen
    slice-008 tests shows only `Ok(…)` at the three construction sites and the one bound on
    `with_facade` — no assertion changed, so all thirteen stayed live controls on the signature
    change.
- **T4 has no in-process red, and one was not invented.** `serve_stdio` hands `Stdio::new()` to the
  already-tested `serve`; an in-process test of it would have to substitute a different transport,
  which is exactly the thing it does not do. **Its red is T6's subprocess test**, which fails
  without it, and that is a stronger observation than a compile error on an absent name: T6
  exercises the real executor, the real pipes and the real process. Recorded here rather than
  papered over, the way slice 012 recorded its mutation-observed reds.
- **T5 is a refactor and has no red of its own.** Its control is the five pre-existing `cli_chat.rs`
  tests, run with **unchanged bodies** — `git diff dev -- crates/heddle-cli/tests/cli_chat.rs` is
  empty — including the two that pin the behaviours being moved,
  `the_base_url_falls_back_to_the_environment_and_the_local_default` and
  `chat_refuses_a_non_loopback_base_url`.
- **T6** `cargo test -p heddle-cli --test cli_acp_agent` before the subcommand existed: **3 failed,
  0 passed** — a red on output and exit codes rather than on a compile error, because clap rejects
  an unknown subcommand at runtime, the same shape slices 011 and 012 record.
  - `acp_agent_exits_zero_when_its_client_disconnects` — `left: Some(2), right: Some(0)`, with
    `error: unrecognized subcommand 'acp-agent'` on stderr: clap exits **2** where the command's own
    contract is 0.
  - `acp_agent_refuses_a_non_loopback_base_url_before_serving` — `left: Some(2), right: Some(1)`.
  - `an_acp_client_drives_the_real_binary_and_the_session_lands_on_the_chain` — the ACP client's own
    error: `Error { code: -32603: Internal error, message: "Incoming transport closed", data:
    {"reason": "incoming_transport_closed", "method": "initialize"} }`. The child died on argument
    parsing before answering the handshake, which is precisely what an editor would see.
- **T7's green** turned all three green on the first run, with no change to the test file.

## Gate run (T8)

2026-09-03, Windows leg observed locally; macOS and Linux legs unobserved until the repository has a
remote (SC-001).

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, no warning on any of the six
  crates.
- `cargo test --workspace` — **110 passed, 0 failed, 1 ignored**: the 105 baseline plus five, and
  every one of the 105 with an unchanged body. `acp_session` 13→**14** (the fallible-factory test),
  `core` 14→**15** (`HeddleError::Protocol`'s message), and the new `cli_acp_agent` **3**. Unchanged:
  `cli_chat` 6, `cli_ledger` 8, `cli_secret` 2, `native_loop` 18, `tool_gateway` 9, `governed_run` 2,
  `openai_compat` 14 (+1 ignored), `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5.
- `cargo build --workspace` — clean; the built `heddle` carries the new subcommand.
- `heddle acp-agent --help` — succeeds, listing `--root --silo --model --base-url --max-iters
  --max-tokens --no-progress-limit --timeout-secs`.
- `heddle chat --help` — still lists every flag it listed on `dev`, with identical value names,
  defaults and help strings. One honest difference: `--prompt` and `--run-id` now print **after** the
  four budget flags rather than between `--base-url` and them, because `ChatArgs` flattens
  `ModelArgs` first. SC-007 is "the same flags", and no test in the tree asserts help text.

### The headline claim, checked by running it

**The editor leg is unobserved, and this is the reason:** no ACP-speaking editor is installed on this
host — neither Zed nor goose is on `PATH` or under the user's `Programs` directory. SC-008's
alternative is taken: recorded as unobserved, plainly.

**The real-model leg is observed.** Following slice 012's T1 probe precedent — a probe crate built
outside the repository, run, and deleted, with `git status` clean before and after — a
`probe-client` binary using the crate's own `AcpAgent` transport (the same one an editor uses, and
the same one `cli_acp_agent.rs` uses) spawned the real `heddle acp-agent` against the **live local
Ollama**, no stub anywhere:

```
$ probe-client <heddle.exe> acp-agent --root <tmp> --silo handrun --model lfm2.5:latest --timeout-secs 120
SESSION heddle-1
UPDATE AgentMessageChunk(ContentChunk { content: Text(TextContent { … text: "hello from heddle" … }) })
STOP EndTurn

$ heddle ledger log --root <tmp> --silo handrun
heddle-1#1  0  iteration_boundary  1d470fb743862350bb7e846eeb3d191ceeb4977eeaa5bf70657f0187ed3085e5
heddle-1#1  1  llm_request         11f6763c27cce3076b054a74cd4ec2fb6deefa06ad1509f487b79181333c315c
heddle-1#1  2  llm_response        6d4b29aaeb895c5c4c30e31f1681fb0c05311ce8cb04ec48b88c5627237a1181
heddle-1#1  3  budget_spent        c03b5219046b6266861aec02ee71c5ac139b68f7c9526a07e19a008a6fef107a
heddle-1#1  4  exit                0141ca05ae9cf7d9a55e68ca2f1f7c4f6c6e4429f6114f9b68037b20884d41be

$ heddle ledger verify --root <tmp> --silo handrun
heddle-1#1  ok  5 steps
```

`heddle ledger show` of that `budget_spent` step prints `184` — the model's **real** metering, not a
number this repository invented (Constitution VIII).

So everything between the ACP wire and the disk is observed against a real model over a real
subprocess. What is unobserved is only whether a particular editor's *own* agent-server
configuration launches the command as expected.

## Control diff (T9)

`git diff dev --stat -- crates/heddle-mcp/ crates/heddle-silo/ crates/heddle-gateway/ spikes/ .github/
rust-toolchain.toml` is **empty** (SC-005), `spikes/` included per ADR-0004 D2. `core.yml`'s `paths:`
already covers `crates/**` and `Cargo.toml`, and this slice adds no workspace member — confirmed by
reading, not edited.

`git diff dev --stat -- crates/heddle-core/` is `src/error.rs | 4 +` and `tests/core.rs | 14 +` —
**one added error variant and one added test, 18 insertions and 0 deletions** (SC-006). No existing
variant, signature or test body changed.

`git diff dev --stat` over the branch is **1209 insertions and 121 deletions** across 15 files. The
deletions are accounted for in three places and nowhere else:

- `crates/heddle-cli/src/chat.rs` — the base-URL resolution, `DEFAULT_BASE_URL`, and the
  `NoGroundTruth`/`NoTools` definitions, all **moved** to `wiring.rs` with their comments intact.
  The only wording change is `NoTools`'s message, which named `heddle chat` and now names "this
  command", because two commands share it.
- `crates/heddle-cli/src/main.rs` — `ChatArgs`'s six model flags, replaced by
  `#[command(flatten)] model: wiring::ModelArgs`; plus the module docstring, which claimed
  `heddle acp-agent` was *"still absent, and now only for want of a stdio transport and an async
  runtime"*. Both halves of that sentence are now false, so it is rewritten rather than left — the
  same courtesy slice 012's T11 did for its predecessor's stale claim.
- `crates/heddle-acp/tests/acp_session.rs` — the three construction sites re-indented inside `Ok(…)`
  and the one `with_facade` bound. No assertion moved.

`git diff dev -- Cargo.toml` is **exactly one added `[workspace.dependencies]` line**, `futures`.

## Drift (T9)

Measured against a detached worktree at the branch point (`2e41492`), so both sides come from a real
resolution rather than from the previous slice's note.

**Zero new packages, on every target.** `cargo tree -e normal,build,dev [--target …] --prefix none`,
deduplicated by name and version with workspace-member paths normalised away:

| Target | before | after | added |
|---|---|---|---|
| `x86_64-pc-windows-msvc` (host) | 149 | 149 | none |
| `x86_64-unknown-linux-gnu` | 148 | 148 | none |
| `aarch64-apple-darwin` | 150 | 150 | none |

Nothing was added and nothing removed on any target. As in slices 010–012, a handful of package
versions differ between the two trees purely as resolution noise, because `Cargo.lock` is
`.gitignore`d here: the freshly resolved **base** worktree picked up `serde`/`serde_core`/
`serde_derive` 1.0.229, `serde_json` 1.0.151, `proc-macro2` 1.0.107, `quote` 1.0.47 and (on
Linux/macOS) `libc` 0.2.189, one patch ahead of the working tree's cached lock. Those are excluded
above.

This slice therefore adds **edges only**, exactly as SC-006 predicted:

- `heddle-acp → futures` — `futures 0.3.34` and `futures-executor 0.3.34` were already resolved,
  because `agent-client-protocol` declares `futures` with default features and `executor` is in that
  default set. The declaration is `default-features = false, features = ["std", "executor"]`, so
  this crate asks for the smallest subset it uses rather than re-enabling the default set.
- `heddle-cli → heddle-acp` — its fourth path dependency, bringing its direct list to `heddle-acp`,
  `heddle-core`, `heddle-gateway`, `heddle-silo`, `clap`, `serde_json`.
- `heddle-cli` **dev** → `agent-client-protocol` + `futures`, for the real ACP client in
  `cli_acp_agent.rs`. Deliberately dev-only: FR-007 keeps the protocol out of product code, and the
  manifest says so in a comment.

**No toolchain change and no new build prerequisite.** No crate entered the graph, so nothing can
have raised the MSRV, and `rust-toolchain.toml`, `workspace.package.rust-version` and
`.github/workflows/core.yml` are untouched. `docs/DEVELOPMENT.md`'s "Machine prerequisites" is
unchanged by this slice.

**Neither `heddle-acp` nor `heddle-cli` takes `tokio` in product code.** It remains a dev-dependency of
`heddle-acp` alone, exactly as it was on `dev`.

## Out of scope

Deliberately not done, so no one helpfully does it:

- **Tool advertisement, tool discovery, and any MCP wiring in the CLI.** `TurnRequest` has no
  `tools` field and `heddle-cli` does not depend on `heddle-mcp`. Consequence, stated in the spec:
  `AcpPermissionTransport`'s permission flow is **not reachable** through `heddle acp-agent` in this
  slice; a model-invented tool call is refused by the empty `ToolPolicy`, and that refusal is what
  the ACP client sees.
- **Streaming (SSE) and incremental `AgentMessageChunk`s.** `project_updates` emits one chunk per
  `LlmResponse` step *after* the run; `"stream": false` is still sent explicitly. Real streaming
  changes the Ledger capture shape and belongs with the gateway.
- **New ACP methods or capabilities** — `session/load`, `session/set_mode`, `fs/*`, MCP-over-ACP,
  extra `AgentCapabilities` fields, terminals, plan updates. Slice 008's surface, unchanged.
- **Redaction on the `LlmRequest`/`LlmResponse` path**, raw-wire-byte capture, provider
  authentication, sampling parameters, a config file. Separate, already-named items on slice 012's
  `## Next slice` list.
- **A silo-per-session or per-client silo mapping**, session persistence across process restarts, or
  ACP `session/load`. One process, one silo; sessions live only as long as the connection.
- **Changing `heddle chat`'s behaviour.** T5 moves code; the five `cli_chat.rs` tests are the proof
  nothing else moved.
- **A tokio runtime in `heddle-cli` or `heddle-acp` product code.** Slice 012's next-slice note
  predicted one; it is not needed, and the spec says why.
- **`spikes/`** — untouched (ADR-0004 D2).

## Next slice (not this feature)
- [ ] **redaction on the `LlmRequest`/`LlmResponse` path**, so `heddle ledger show` cannot print a
      conversation secret. Carried from slices 011 and 012, and now reachable from two commands
      rather than one: `ToolCall`/`ToolResult` payloads pass through the `Redactor` in
      `ToolGateway::call_captured`, but `NativeLoop::run` appends model I/O **raw**. The fix belongs
      to the governed loop.
- [ ] **tool advertisement** — a `tools` field on `TurnRequest`, which needs tool *discovery* from
      the Tool Gateway first. It is what would make `AcpPermissionTransport` reachable from a real
      editor, which is the largest untested-in-production path this slice leaves behind.
- [ ] **raw-wire-byte capture** — a `StepKind` for the provider's literal request and response
      bytes, which is what design §4.5's "exact model I/O" and Spike 1's criterion C1 actually ask
      for.
- [ ] **streaming (SSE)**, together with incremental ACP `AgentMessageChunk` notifications. Today a
      client sees one chunk per turn, after the turn.
- [ ] **provider authentication**, when a local gateway needs one: an `Authorization: Bearer` whose
      value arrives as a `SecretRef` resolved through `SecretProvider` (Principle VI).
- [ ] **sampling parameters** — temperature, top-p, seed. `TurnRequest` cannot express them.
- [ ] a config file holding the base URL, the model and a default silo root, so `--base-url`/
      `--model`/`--root` are not the only way to name them.
- [ ] the egress-policy layer and ADR-0002 D4's **process-level socket-deny boundary**, which is
      what would close `LocalEndpoint`'s `localhost` re-resolution residual.
