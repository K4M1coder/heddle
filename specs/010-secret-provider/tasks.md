# Tasks: `SecretProvider` (OS keychain) + JIT `Redactor` (v0 slice)

**Spec:** `specs/010-secret-provider/spec.md` · TDD (red→green), product code in
`crates/skein-core` and `crates/skein-silo`, branch `010-secret-provider` cut from `dev` after
slice 009 merged.

## Constitution Check (ADR-0004 D1 solo-v0 bar)
- I Headless core ✅ (library only; the seam is reachable through the existing headless API) ·
  II Local-first ✅ (the OS credential store is local by construction; `requires_network()` is
  `false`, which is what makes the backend usable in Local mode with egress OFF, §7.3;
  `OsKeychain` installs no process-global store, so §7.2's per-silo keychain stays expressible)
- III Test-First ✅ (T1 pins the `keyring-core` surface against the vendored source **and** a
  compiled probe before any product code; T3's red observed before T4, T5's before T6) ·
  IV Inverted coupling ✅ (`SecretProvider` is the seam; `keyring-core` and the store crates are
  named in exactly one module of one crate and never in `skein-core`)
- V Traceability ✅ (**one** `Redactor`, **one** `redact`, **one** `redact_value`, shared by the
  literal and the resolved path; `k4` proves the redaction end to end through the governed
  gateway and re-verifies the chain)
- VI Security ✅ **this slice is the one that makes Principle VI's "by reference, never by value"
  true of the product**: `SecretValue` zeroizes on drop and is `Debug`-redacted; `SecretProvider`
  has no write method; an unresolvable reference fails loudly instead of yielding a `Redactor`
  that scrubs nothing; an unimplemented scheme is refused
- VII Neutrality ✅ (one backend, one scheme, two trait methods, one new constructor; no
  `SecretRef` enum, no provider registry, no `sops://`/`op://` stubs, no CLI)
- VIII Loop discipline ✅ (`LoopController`, `ProgressProbe` and `NativeLoop` untouched)
- Cross-platform ✅ (three `#[cfg(target_os)]` arms, one per supported OS, each with a native
  equivalent. `core.yml`'s `paths:` already covers `crates/**` and `Cargo.toml` at 1.97 —
  confirmed by reading, not edited).

## Tasks
- [x] **T0** `specs/010-secret-provider/{spec.md,plan.md,tasks.md}`; branch `010-secret-provider`
      cut from `dev` with slice 009 merged
- [x] **T1** pinned the credential-store surface against the vendored `keyring 4.2.0`,
      `keyring-core 1.0.0` and store-crate sources, and against a compiled probe, *before* any
      product code. **The advisory plan's whole dependency shape was wrong**; see below
- [x] **T2** control baseline: `cargo test --workspace` before any edit — **63**
- [x] **T3** RED — the three `// ---- secrets (§7.13) ----` tests in
      `crates/skein-core/tests/core.rs` against the not-yet-existing API; red recorded below
- [x] **T4** GREEN — `secret.rs`, `zeroize`, `Redactor`'s field + `Redactor::resolve`,
      `SkeinError::Secret`, re-exports, and the now-stale `Redactor` doc comment corrected
- [x] **T5** RED — `crates/skein-silo/tests/silo_secret.rs` against the not-yet-existing
      `OsKeychain`; red recorded below
- [x] **T6** GREEN — `crates/skein-silo/src/secret.rs` and the per-OS `keyring-core` dependencies
- [x] **T7** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`; new total recorded below
- [x] **T8** control diff: `git diff dev` empty on `crates/skein-mcp/`, `crates/skein-acp/`,
      `spikes/`, `.github/` and `rust-toolchain.toml`
- [ ] **T9** dependency drift recorded below
- [ ] **T10** close out: tick the `SecretProvider` bullet in spec 009's "Next slice" list and in
      `specs/003-skein-core-foundation/tasks.md`, and set this spec's Status

## Control baseline (T2)

`cargo test --workspace` on `010-secret-provider` @ `25641a6` (identical to `dev`), working tree
clean, 2026-09-03: **63 passing** — `skein-acp/tests/acp_session.rs` 13, `skein-core/tests/core.rs`
9, `tests/native_loop.rs` 18, `tests/tool_gateway.rs` 9, `skein-mcp/tests/rmcp_gateway.rs` 7,
`skein-silo/tests/silo_ledger.rs` 7; 0 failed, 0 ignored. This is the number T7 diffs against.

## Pinned credential-store surface (T1)

**The advisory plan's dependency shape did not survive contact with the source.** It assumed a
single `keyring = "4.2"` dependency with per-OS *features*. Read from the vendored source:

1. `windows-native-keyring-store` and friends are **optional target-gated dependencies** of
   `keyring`, not standalone entry points. `keyring/src/lib.rs:32` is
   `compile_error!("At least one of the features 'v1' or 'cli' must be enabled")`, so a
   `default-features = false` build with only the store features **does not compile**.
2. `keyring`'s `v1` feature — the crate default — hard-codes **zbus Secret Service** on every
   non-Apple unix (`keyring/src/v1.rs:115`), i.e. D-Bus. That is exactly the shape a headless
   `ubuntu-latest` cannot run, so the default is the one thing this slice must not take.
3. The `cli` feature does reach keyutils (`keyring/src/cli.rs:97`,
   `use_native_store(prefer_secret_service: bool)`), but it enables **every** store at once —
   `db-keystore`, `dbus-secret-service` with vendored crypto, `zbus`, and the sample store.
4. `keyring`'s own module doc (`src/lib.rs:17-27`) says applications "which want to control which
   credential stores they use on which platforms … should not be linking to this library at all;
   they should instead be linking to `keyring-core` and any specific credential stores".

So the dependency is `keyring-core` plus exactly one native store crate per OS — the advisory
plan's *intent* (a native store per OS, keyutils on Linux) reached through the shape the library
supports. Every name below is used by `crates/skein-silo/src/secret.rs` exactly as spelled here.

| Item | Pinned spelling |
|---|---|
| `keyring-core` version | `1.0.0`, MIT OR Apache-2.0; one dependency (`log`) |
| `CredentialStore` | `pub type CredentialStore = dyn CredentialStoreApi + Send + Sync` (`keyring-core/src/api.rs:258`) |
| `CredentialStoreApi::build` | `fn build(&self, service: &str, user: &str, modifiers: Option<&HashMap<&str, &str>>) -> Result<Entry>` (`api.rs:204`) — the no-global path |
| `Entry::set_password` / `get_password` / `delete_credential` | `(&self, …) -> Result<()>` / `-> Result<String>` / `-> Result<()>` (`keyring-core/src/lib.rs:212,261,358`) |
| missing credential | `Error::NoEntry` (`keyring-core/src/error.rs:40`) — the variant `resolve` maps to `SkeinError::Secret` |
| Windows store | `windows_native_keyring_store::Store::new() -> Result<Arc<Self>>` (`store.rs:35`), crate `windows-native-keyring-store 1.1.0` |
| macOS store | `apple_native_keyring_store::keychain::Store::new() -> Result<Arc<Self>>` (`keychain.rs:181`), crate `apple-native-keyring-store 1.0.2`, **feature `keychain`** (`keychain = ["security-framework"]`) |
| Linux store | `linux_keyutils_keyring_store::Store::new() -> Result<Arc<Self>>` (`store.rs:34`), crate `linux-keyutils-keyring-store 1.0.0` |
| `zeroize` | `1.9.0`, no dependencies; `Zeroizing<String>` needs the default `alloc` feature and `Deref`s to `String` |

**Four facts were measured, not assumed** (a throwaway `cargo` probe outside this repository,
2026-09-03, this Windows host):

- `Store::new()` → `store.build(service, user, None)` → `set_password` / `get_password` /
  `delete_credential` **round-trips against the real Windows Credential Manager**, with no call to
  `keyring_core::set_default_store`. The process-global default store that `keyring::v1` installs
  (`v1.rs:108`, a `RwLock` static behind a `LazyLock`) is avoidable, so `OsKeychain` owns its
  store and two providers in one process cannot fight over a global.
- `get_password` after `delete_credential`, and `get_password` for a never-stored service, both
  give **`Error::NoEntry`** — so a missing secret is distinguishable from a platform failure.
- `delete_credential` of an absent credential also gives `Error::NoEntry`, so test cleanup must
  tolerate it rather than `unwrap`.
- **The Windows store accepts an empty service name without error.** Validation of
  `keychain://<service>/<account>` therefore has to be ours: an empty service or account is
  rejected by `SecretRef` parsing before the store is ever reached.

## Observed red (Constitution III)

- **T3** `cargo test -p skein-core --test core`, 2026-09-03:
  - `error[E0432]: unresolved imports skein_core::SecretProvider, skein_core::SecretRef,
    skein_core::SecretValue` — *"no `SecretProvider` in the root"*, *"no `SecretRef` in the
    root"*, *"no `SecretValue` in the root"* (`crates/skein-core/tests/core.rs:5:11`)
  - `error: could not compile skein-core (test "core") due to 1 previous error`
  - As in slices 007–009, rustc abandons the crate once import resolution fails, so this one
    diagnostic is the whole red: the `Redactor::resolve` and `SkeinError::Secret` errors
    underneath it are never reached.
- **T5** `cargo test -p skein-silo --test silo_secret`, 2026-09-03:
  - `error[E0432]: unresolved import skein_silo::OsKeychain` — *"no `OsKeychain` in the root"*
    (`crates/skein-silo/tests/silo_secret.rs:16:5`)
  - `error: could not compile skein-silo (test "silo_secret") due to 1 previous error`
  - Every name the slice adds comes through `OsKeychain`, so again one diagnostic is the whole
    red. `skein-core`'s `SecretRef`/`SecretProvider` already resolve, because T4 landed them.

## Gate run (T7)

2026-09-03, Windows leg observed locally; macOS and Linux legs unobserved until the repository has
a remote (SC-001).

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean. It raised `err_expect` twice on
  the new `skein-silo` tests; both are now `expect_err`, which works because `SecretValue`'s
  hand-written `Debug` is `SecretValue(***)` — the redaction property is what makes the idiomatic
  spelling available.
- `cargo test --workspace` — **71 passing**, 0 failed, 0 ignored: 63 pre-existing + 3 core seam
  tests + 5 `skein-silo` tests. Per binary: `acp_session` 13, `core` 12, `native_loop` 18,
  `tool_gateway` 9, `rmcp_gateway` 7, `silo_ledger` 7, `silo_secret` 5.
- The five `silo_secret` tests ran against the **real** Windows Credential Manager, under service
  names unique per process and per test (`skein-test-<pid>-<n>`), each removed by a `Drop` guard.
  `cmdkey /list` afterwards matches nothing containing `skein`, so the suite leaves the developer's
  credential store as it found it.

**The two unobserved legs were partially checked, not assumed.** `cargo check
--target {x86_64-unknown-linux-gnu,aarch64-apple-darwin}` cannot run against this workspace —
slice 009's `rusqlite` with `bundled` needs a *Linux/macOS* C compiler, which this host does not
have. So the three `native_store` bodies were lifted verbatim into the T1 probe crate, which has
no C dependency, and **all three type-check on their own target** under the pinned 1.97:
`linux-keyutils-keyring-store 1.0.0` and `apple-native-keyring-store 1.0.2` (feature `keychain`)
both compile with `Store::new()` coerced to `Arc<CredentialStore>` exactly as
`crates/skein-silo/src/secret.rs` spells it. What remains unobserved is *runtime* behaviour on
those two OSes, not the API surface.

**The macOS leg is not `#[ignore]`d.** The escape hatch the advisory plan authorised was not
taken: taking it on an unobserved platform would be a guess in the pessimistic direction, exactly
as much as omitting it would be a guess in the optimistic one, and the optimistic guess is the one
CI can refute. `apple-native-keyring-store`'s `keychain::Store` uses the running user's *login*
keychain, which GitHub's `macos-latest` runner unlocks for the session. If the hosted leg proves
otherwise once this repository has a remote, the correction is `#[ignore]` on `k1` and `k4` with
the runner's error recorded here — never an in-memory provider standing in for the acceptance,
which says *a real `SecretProvider`*.

## Control diff (T8)

`git diff dev --stat -- crates/skein-mcp/ crates/skein-acp/ spikes/ .github/ rust-toolchain.toml`
is empty (SC-003, SC-004), so specs 005 and 008's suites — 20 of the 63 baseline tests — are live
controls run against this slice's `skein-core`. `git diff dev -- Cargo.toml` is exactly five added
`[workspace.dependencies]` lines (SC-005).

Everything else is additive: two new modules, one new test binary, one new `Redactor` constructor,
one new `SkeinError` variant, and new files under `specs/010-secret-provider/`. The only deletion
anywhere in `git diff dev` outside the two new modules is `crates/skein-core/tests/core.rs`'s
two-line `use` list, replaced to import the new names — **no pre-existing test body changed**
(SC-006).

## Next slice (not this feature)
- [ ] `skein-cli` reference client: `skein secret set|delete` (the second caller of
      `OsKeychain::store`/`delete`) and `skein ledger log|show|verify`
- [ ] a config shape that actually *stores* `SecretRef`s, so `Redactor::resolve` is reached from
      a file rather than from a caller's literal `Vec<SecretRef>`
- [ ] per-call JIT injection: a secret placed into a subprocess env or an auth header at the
      moment of execution, which is the §7.13 consumer that this slice's `Redactor` is not
- [ ] the other §7.13 backends — `sops://`, `op://`, `bao://`, `infisical://` — and with the
      second one, `SecretRef` becomes an enum of schemes
- [ ] one keychain **per silo** (§7.2): `Silo::keychain()` naming the silo in the service prefix,
      once a silo has a config to point at
- [ ] SQLCipher / at-rest encryption of the silo file, with the per-silo key held here
- [ ] bounded / paged `Ledger` reads (still item 3 of 009's list)
- [ ] `Ledger` append-observer + streaming ACP session updates
- [ ] the rest of design §4.11's `Ledger`: `replay(from)`, `revert(to)`, `branch(from)`
- [ ] RBAC, team silos, `Mode` (Local/Server/Remote), the `Backend` trait and `ModeSupervisor`
- [ ] retention and egress policy over the journal (§7.9)
