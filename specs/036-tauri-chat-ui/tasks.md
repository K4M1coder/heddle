# Tasks: a Tauri Chat window over `skein acp-agent` (slice 021)

## Constitution Check

| Principle | How this slice satisfies it |
|---|---|
| **I — Headless core, CLI as reference, UI as a thin layer** | The window spawns `skein acp-agent` and speaks ACP to it. `ui/src-tauri` depends on no crate under `crates/`, so it is structurally incapable of holding agent logic. Every action is one of three ACP methods; the table is in `spec.md` FR-002 and in `docs/UI.md`. |
| **II — Local-first, silo isolation** | Nothing new reaches the network. The webview loads only local files and its CSP forbids remote origins; the frontend has no HTTP permission; the child reaches only the loopback-only provider `skein-gateway` already restricts it to. The silo is the one the operator named. |
| **III — Test-First** | `chatState.test.ts` was written and observed red before `chatState.ts` existed. `chat_session.rs` was written before `session.rs`. `config.rs`'s tests cover its refusals. Both boundaries are testable with a double behind them: the model is a `TcpListener`, the update sink is a closure. |
| **IV — Inverted coupling & explicit boundaries** | `session.rs` names no Tauri type and reaches its caller through closures. `main.rs` supplies the Tauri implementation of those closures; a test supplies another. |
| **V — Traceability & reversibility** | The window adds no record. Everything it renders is `skein-acp`'s `project_updates` reading the run's chain, relayed 1:1. Runs land on the chain as `skein-1#1`, `skein-1#2`, verifiable with `skein ledger log` from a second process. |
| **VI — Security & secrets by reference** | Deny-by-default in three places: the Tauri capability grants the frontend `core:default`/`core:event:default` and nothing else; no `--fs-root` means no tools; and permission requests are **declined**, because a client may narrow what runs and never widen it. Tool output is rendered with `textContent`, never `innerHTML` — external content is data, never markup. |
| **VII — Neutrality & reuse (YAGNI)** | No view framework is introduced for one screen. The ACP client role is reused from `agent-client-protocol`, not reimplemented. The test double is the one slice 013 already uses. |
| **VIII — Loop discipline** | The window hosts no loop. Termination, budgets and no-progress detection stay in `skein-core`'s `LoopController`, where they are externally enforced; the window's Cancel is a request to that loop, honoured at its next turn boundary, and the UI reports the `StopReason` it was actually given rather than assuming success. |
| **Cross-platform** | No OS-specific code. `EXE_SUFFIX` is used for the binary name. The tri-OS CI matrix covers the slice, with a Linux webview step because Linux ships no webview. |
| **English-only content** | All UI copy, comments, docs and tests are English. |

**Complexity Tracking**: one departure worth naming — `ui/src-tauri` in `members` makes every workspace build compile Tauri, and forces a system-dependency step onto the Linux CI leg. Accepted deliberately: the alternative exempts the UI from `fmt`, `clippy -D warnings` and `cargo test --workspace`, which is the wrong trade for a slice whose entire claim is that it adds no logic. Recorded in `spec.md`'s residuals and in `plan.md` D9.

---

## Tasks

| # | Task | Status |
|---|---|---|
| T1 | `ui/src/chatState.test.ts`, observed red, then `ui/src/chatState.ts` green | done |
| T2 | `ui/package.json`, `vite.config.ts`, `tsconfig.json`, `src/index.html`, `src/main.ts`, `src/style.css` | done |
| T3 | `Cargo.toml` members; `ui/src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,icons/icon.ico}` | done |
| T4 | `ui/src-tauri/tests/chat_session.rs`, then `src/session.rs` and `src/lib.rs` | done |
| T5 | `ui/src-tauri/src/config.rs` with its unit tests | done |
| T6 | `ui/src-tauri/src/main.rs` — three commands, two events, one shutdown boundary | done |
| T7 | `docs/UI.md`, `README.md` "Current status" | done |
| T8 | `.github/workflows/core.yml` path triggers + webview and frontend steps; `.gitignore` | done |
| T9 | Full local validation (fmt, clippy, `cargo test --workspace`, `npm test`, `npm run build`) | **blocked** |

---

## Observed red

`ui/src/chatState.test.ts` before `chatState.ts` existed:

```
Error: Cannot find module './chatState' imported from '.../ui/src/chatState.test.ts'
 Test Files  1 failed (1)
      Tests  no tests
```

and green after:

```
 ✓ chatState.test.ts (20 tests) 4ms
 Test Files  1 passed (1)
      Tests  20 passed (20)
```

---

## Finding: the connection closure must not await its own requests

The first shape for `session.rs` awaited each `session/prompt` inside the `connect_with` closure and read the next command afterwards. It works for a request/response client and is wrong for this one: while the closure sits on a prompt's response it is not reading its command channel, so a `session/cancel` sent during a run could not be delivered until the run it was meant to cancel had already ended.

The fix is the idiom `crates/skein-acp/src/permission.rs` already records for the mirror-image case on the agent side: issue the request with `on_receiving_result`, which registers a callback and returns immediately, and carry the answer back to the caller over a `oneshot`. The closure's loop then never blocks on anything but its own channel.

This is not a performance detail. Without it, `a_cancel_stops_the_run_at_the_next_turn_boundary_and_says_so` cannot pass, because the feature it tests cannot happen.

---

## Finding: the cancellation test needed a gated provider, not a sleep

`session/cancel` only refuses the *next* model call, so proving it works needs a run with a second turn, and a cancel that provably lands between the two. A `sleep` between "send cancel" and "let the turn finish" would make the test a statement about runner speed.

Three things make it a statement about ordering instead:

1. **A gated stub provider.** It reads turn 1's request, reports it to the test, and then waits. The loop thread is parked inside its HTTP call and cannot advance until the test says so.
2. **A first turn that must be followed by a second.** With no `--fs-root` the policy allows nothing, so a turn asking for `fs_write` is refused, the refusal is captured on the chain, and the loop goes round again — a second turn with no tool configuration and no permission dialog needed.
3. **An `initialize` round trip after the cancel.** JSON-RPC dispatch is ordered, so an answer to a request sent *after* the cancel notification proves the child has already processed the cancel. Only then is the gate opened.

---

## Deviation from the source plan

| Item | Planned | Done | Why |
|---|---|---|---|
| Level-2 command | `cargo test -p skein-ui` | `cargo test --workspace` | `CARGO_BIN_EXE_*` covers only the current package's binaries and `skein` belongs to `skein-cli`, so `-p skein-ui` alone never builds the binary the test drives. The test resolves the path from its own location and, when it is absent, fails naming the remedy. |
| CI change | "path filter + an npm step; no new Rust step needed" | path filter, an apt step, and an npm build step ordered **before** the cargo steps | Correct as far as it went, but incomplete: `ui/src-tauri` in `members` means Linux CI now compiles Tauri, which needs a system webview, and `tauri-build` reads `ui/dist` at compile time, so the frontend must be built first. Without both, the Linux leg fails on every run. |
| `config.rs` | not planned | added, with 8 unit tests | The plan hardcoded `--model`/`--fs-root`/`--base-url` as "Tauri app config/env vars" without saying where that mapping lives. Left in `main.rs` it would have been untested wiring; as its own module it is tested, and its refusals are a stated requirement (FR-003). |
| Permission handling | not addressed | requests are declined, by protocol kind | The plan's design would have hung the child's loop thread the first time a tool call happened with `--fs-root` set, because nothing answered `session/request_permission`. See `plan.md` D5. |
| Tauri icon and capability | not planned | `icons/icon.ico`, `capabilities/default.json` | `tauri-build` refuses to build on Windows without the first; the frontend cannot call a command or listen to an event without the second. |

---

## Close-out (T9)

**Not run.** Local validation is blocked on a full disk (2.6 GB free on a 551 GB volume), which is a machine condition and not a property of this change. Everything up to it holds:

- `cargo check -p skein-ui --all-targets` — passes (lib, bin and the test target all compile).
- `cd ui && npm test` — 20 passed.
- `cd ui && npm run build` — `tsc --noEmit` clean, bundle written to `ui/dist`.

Still to run once there is disk: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and the manual smoke test in `docs/UI.md`.
