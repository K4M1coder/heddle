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
- [ ] **T1** pinned the credential-store surface against the vendored `keyring 4.2.0`,
      `keyring-core 1.0.0` and store-crate sources, and against a compiled probe, *before* any
      product code. **The advisory plan's whole dependency shape was wrong**; see below
- [ ] **T2** control baseline: `cargo test --workspace` before any edit — **63**
- [ ] **T3** RED — the three `// ---- secrets (§7.13) ----` tests in
      `crates/skein-core/tests/core.rs` against the not-yet-existing API; red recorded below
- [ ] **T4** GREEN — `secret.rs`, `zeroize`, `Redactor`'s field + `Redactor::resolve`,
      `SkeinError::Secret`, re-exports, and the now-stale `Redactor` doc comment corrected
- [ ] **T5** RED — `crates/skein-silo/tests/silo_secret.rs` against the not-yet-existing
      `OsKeychain`; red recorded below
- [ ] **T6** GREEN — `crates/skein-silo/src/secret.rs` and the per-OS `keyring-core` dependencies
- [ ] **T7** gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace`; new total recorded below
- [ ] **T8** control diff: `git diff dev` empty on `crates/skein-mcp/`, `crates/skein-acp/`,
      `spikes/`, `.github/` and `rust-toolchain.toml`
- [ ] **T9** dependency drift recorded below
- [ ] **T10** close out: tick the `SecretProvider` bullet in spec 009's "Next slice" list and in
      `specs/003-skein-core-foundation/tasks.md`, and set this spec's Status

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
