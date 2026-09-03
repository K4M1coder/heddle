# Implementation Plan: `skein-cli` — the reference CLI client (v0 slice)

**Branch**: `011-skein-cli` | **Date**: 2026-09-03 | **Spec**: `specs/011-skein-cli/spec.md`

## Summary
A new workspace member `crates/skein-cli` with **`[[bin]] name = "skein"`** and no `lib` target —
the first executable in the repository. It exposes two command groups, both end-to-end real:

```
skein ledger log|show|verify --silo <ID> [--root <PATH>] …
skein secret set|delete <REFERENCE>
```

`ledger log|show|verify` map one-to-one onto `Ledger::log`, `Ledger::show` and
`Ledger::verify_chain` over a `Silo`-backed chain on disk. `secret set|delete` are the second
caller of `OsKeychain::store`/`delete` — the callers spec 010's Complexity Tracking promised.

The slice makes **one** addition to the library, and it is forced by Principle I:

```rust
impl Ledger {
    pub fn runs(&self) -> Vec<&str>;   // distinct run_ids, first-append order
}
```

`Ledger::log(run_id)` needs a run id and nothing in the workspace can produce one, so without
`runs()` a person who does not already know an id cannot use `skein ledger log` at all. "Everything
the CLI does, the API exposes" therefore *requires* the API to enumerate runs. It is a read-model
accessor on the type that already owns the read model (Principle IV untouched), six lines with one
caller today (Principle VII satisfied), and **purely additive** — no existing signature changes, so
all 71 pre-existing tests stay live controls, exactly as `Redactor::new` staying untouched kept 63
live in slice 010.

*Alternative rejected:* the CLI opens `SqliteLedgerStore::open(silo.ledger_path())` and derives run
ids from `LedgerStore::load()`, needing no core change at all — the strongest argument for it. It
loses because `LedgerStore` is the **storage port** the core discovers durability through, not the
product's read API. Consuming it from the CLI would make the CLI a *second read-model
implementation beside `Ledger`*, duplicating knowledge (`log`, `verify_chain` and `runs` all derive
from the same private `steps` vector) that Constitution V exists to keep in one place. It would
also open two SQLite connections on one file for a single command, since `show`/`verify` need a
`Ledger` anyway.

`crates/skein-silo` gets **no** API change; `skein-mcp` and `skein-acp` are untouched.

## Technical Context
**Language/Version**: Rust 1.97 (pinned in `rust-toolchain.toml`, unchanged this slice)
**Primary Dependencies**: `clap = { version = "4.6", default-features = false, features = ["std",
"derive", "help", "usage", "error-context"] }` in `[workspace.dependencies]`, used only by
`skein-cli`. Plus `skein-core`, `skein-silo` and `serde_json` (for `FR-012`'s serde kind name).
**Not** `skein-acp` or `skein-mcp`: there is nothing in either that v0 can call without a model,
and an unused dependency is a Principle VII violation. (The invariant says the CLI *may* depend on
all four; "may" is not "must".) No `tokio`: every path is synchronous.
**Storage**: the silo directory + its SQLite ledger; the platform credential store
**Testing**: `cargo test`; one seam test in `skein-core`, nine **process** tests in `skein-cli`
driving the real binary via `env!("CARGO_BIN_EXE_skein")`. Dev-dependencies `tempfile` (silo roots)
and `rusqlite` (the `DROP TRIGGER` + `UPDATE` tamper technique `silo_ledger.rs::s6` established) —
both already `[workspace.dependencies]`. **No CLI-testing crate is needed**; see below.
**Target Platform**: Windows + macOS + Linux
**Project Type**: workspace, four library crates + one binary crate
**Performance Goals**: N/A
**Constraints**: no capability the library does not expose; no secret through a flag or the screen;
`crates/skein-mcp/`, `crates/skein-acp/`, `crates/skein-silo/` byte-identical to `dev`
**Scale/Scope**: one new crate (four small modules), one additive core method, one root
`Cargo.toml` line, two corrected `docs/DEVELOPMENT.md` lines

## Why `clap`, and why this feature set
Principle VII is explicit that we "reuse proven existing tools rather than rewrite them". A
hand-rolled `std::env::args` matcher has the smaller dependency graph (zero crates) and was
seriously considered; it loses because `--help`, `--version`, usage text, exit code 2 on a bad
invocation and value validation **are the CLI's user contract**, this surface will grow (`chat`,
`acp-agent`, `run`), and hand-rolling all of that is precisely the rewrite Principle VII names.

The minimal feature set is a measured cost control, not a guess (see `tasks.md` T1): **10 crates
instead of 21**. Dropping `color` removes the whole `anstream`/`anstyle-parse`/`anstyle-query`/
`anstyle-wincon`/`colorchoice`/`is_terminal_polyfill`/`utf8parse`/`windows-sys` stack; dropping
`suggestions` removes `strsim`. Colour is not wanted anyway — a "100% scriptable" CLI should not
emit ANSI by default. `derive` rather than the builder, because the command tree is data.

## Why no CLI-testing dependency
`assert_cmd` / `predicates` / `trycmd` were considered and are not used. Cargo already sets
`CARGO_BIN_EXE_<name>` for every integration test of a package with a `[[bin]]`, so
`Command::new(env!("CARGO_BIN_EXE_skein"))` reaches the real binary with zero dependencies, and
`Stdio::piped()` drives its stdin. Verified in the T1 probe. (`docs/DEVELOPMENT.md` claimed
`assert_cmd` was "pulled by Cargo on first build"; no crate in the workspace has ever referenced
it. That line is corrected at closeout.)

## Reading a secret: stdin only, and a terminal stdin is refused
`skein secret set <REFERENCE>` reads stdin to EOF and strips at most one trailing newline. There is
deliberately **no `--value` flag** (Constitution VI: it would land in shell history and in
`ps`/Task Manager listings). If stdin `is_terminal()`, the command **refuses** with a message naming
safe idioms rather than prompting:

- It closes the leak the invariant names — history, process listing — completely.
- Zero new dependencies, identical on all three OSes.
- Unlike an echoing prompt, it never renders the secret on screen or into terminal scrollback,
  which on a Principle VI slice is a real leak an auditor would flag.

*Alternative rejected:* `rpassword` for a non-echoing interactive prompt. It is the nicest UX and a
small, proven, tri-OS crate. It loses for v0 because it adds a dependency for a path **no automated
test can exercise** (the harness has no PTY), and the invariant's "prompt *or* stdin" is satisfied
by stdin. Recorded on the next-slice list.

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ **this is the slice that makes Principle I true of the product.** The
  core was already headless; it had no client. Every CLI command is a thin call onto an existing
  public API, and the one place where the CLI would have needed knowledge the API lacked
  (enumerating runs) is closed by *adding it to the API*, not by reaching around it.
- **II. Local-first / silo isolation**: ✅ `Silo::open`'s own id validation is what the CLI calls;
  the CLI never joins a path from user input itself, so containment stays a property of the id.
  No network anywhere.
- **III. Test-First**: ✅ T1 pins the clap surface against the vendored source **and** a compiled
  probe before any product code; T3's red observed before T4, T5's before T6, T7's before T8.
- **IV. Inverted coupling**: ✅ `skein-cli` has no `lib` target, so nothing can depend on it. It
  depends on `skein-core` and `skein-silo` and on neither of the two protocol crates.
- **V. Traceability**: ✅ `ledger log`'s `{kind}` column is the step kind's **serde** name, taken
  through `serde_json::to_value` — the same string the hash is fed, mirroring
  `ledger_store.rs`'s own stated rule, so there is no second name mapping that can drift.
- **VI. Security / deny-by-default**: ✅ no `--value` flag (machine-asserted: clap exits 2 on it);
  a terminal stdin is refused rather than prompted; an empty secret is refused; no code path
  formats the value into any stream; an unknown silo is an error, not an empty answer.
- **VII. Neutrality / YAGNI**: ✅ two command groups, five subcommands, one additive core method.
  **No chat, no `acp-agent`, no placeholder model, no `--json`, no colour, no config file, no
  completions, no `--silo` on `secret`.** The absent `ModelClient` is stated in the spec rather
  than papered over.
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ✅ `LoopController`, `ProgressProbe` and `NativeLoop`
  are untouched. v0 runs no loop.
- **Cross-platform**: ✅ no `#[cfg]` anywhere in the new crate. `#[command(bin_name = "skein")]`
  makes usage text identical on all three OSes (without it clap renders `Usage: skein.exe …` on
  Windows), so stderr assertions are not OS-dependent. `core.yml`'s `paths:` already covers
  `crates/**` and `Cargo.toml`, and `members = ["crates/*"]` already covers a new crate — so
  neither CI nor the workspace manifest's member list needs an edit. Confirmed by reading.

## Project Structure

### Documentation (this feature)
```text
specs/011-skein-cli/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
Cargo.toml                        # +clap in [workspace.dependencies]
docs/DEVELOPMENT.md               # two corrected lines (test tooling; `skein secret set`)
crates/skein-core/
  src/ledger.rs                   # +Ledger::runs()
  tests/core.rs                   # +1 test
crates/skein-cli/                 # NEW
  Cargo.toml                      # [[bin]] name = "skein"; no lib target
  src/main.rs                     # the clap types; main() -> ExitCode
  src/ledger.rs                   # log / show / verify + open_silo
  src/secret.rs                   # set / delete
  tests/cli_ledger.rs             # NEW — e1..e7, real binary against a real silo
  tests/cli_secret.rs             # NEW — e8..e9, real binary against the real keychain
```
**Structure Decision**: `crates/skein-mcp/`, `crates/skein-acp/` and `crates/skein-silo/` are
byte-identical to `dev`, so specs 005, 008, 009 and 010's suites — 32 of the 71 baseline tests —
remain live controls on the core change.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **A new public method on `Ledger` with exactly one caller** (Principle VII) | Principle I is the harder constraint: the CLI enumerates runs, so the API must be able to. Without it `skein ledger log` is unusable by anyone who does not already know a run id. | Deriving run ids in the CLI from `LedgerStore::load()`: makes the CLI a second read-model implementation beside `Ledger` (Constitution V), and opens two SQLite connections on one file per command. |
| **`clap` and its nine transitive crates** for a five-subcommand surface (Principle VII) | `--help`, `--version`, usage text and exit code 2 are the CLI's user contract, and the surface will grow. Principle VII itself says reuse proven tools rather than rewrite them. | A hand-rolled `std::env::args` matcher: zero crates, but re-implements the contract above, and the first `--flag`-shaped mistake in it is a user-visible defect in the product's authoritative client. |
| **An unknown `--silo` leaves an empty directory behind** | `Silo::open`'s id validation is the security-relevant part and must not be re-implemented in the CLI, and `Silo::open` `create_dir_all`s. The ledger file guard turns the wart into a loud exit 1 rather than a silently empty log. | `Silo::open_existing()` in `skein-silo`: a second crate's API change for a cosmetic gain, and this slice's control argument depends on `skein-silo` being byte-identical to `dev`. |
| **The output format is asserted by tests, so any format change breaks them** | For a "100% scriptable" client, stdout *is* the user contract. A contract that can change without breaking a test is not a contract. | Asserting only exit codes: would let a column reorder ship silently and break every script downstream. |
| **`skein ledger show` can print a secret that reached an `LlmRequest`/`LlmResponse` payload** | It is a pre-existing property of the Ledger (`NativeLoop::run` appends model I/O raw), and Principle I says the CLI exposes what the API exposes. | Redacting in the CLI: would make the CLI's view *differ* from the API's, hiding a real gap behind the one surface an auditor looks at. The fix belongs on the model-I/O path; it is on the next-slice list. |
