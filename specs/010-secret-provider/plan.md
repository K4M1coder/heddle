# Implementation Plan: `SecretProvider` (OS keychain) + JIT `Redactor` (v0 slice)

**Branch**: `010-secret-provider` | **Date**: 2026-09-03 | **Spec**: `specs/010-secret-provider/spec.md`

## Summary
`skein-core` gains a `secret` module holding design §7.13's seam, trimmed to what has a caller
today (Principle VII):

```rust
pub struct SecretRef(pub String);   // a URI naming a secret, never its value
pub struct SecretValue(Zeroizing<String>);
impl SecretValue { fn new(v: impl Into<String>) -> Self; fn expose(&self) -> &str; }
// Debug is hand-written: `SecretValue(***)`.

pub trait SecretProvider {
    fn resolve(&self, r: &SecretRef) -> Result<SecretValue>;   // JIT, in memory
    fn requires_network(&self) -> bool;                        // governs the egress policy
}
```

`Redactor`'s **private** field changes from `Vec<String>` to `Vec<SecretValue>` and it gains one
constructor:

```rust
impl Redactor {
    pub fn new(secrets: Vec<String>) -> Self;                                       // UNCHANGED
    pub fn resolve(provider: &dyn SecretProvider, refs: &[SecretRef]) -> Result<Redactor>;
}
```

`Redactor::new`'s signature is untouched, so **all 63 pre-existing tests remain controls**; the
stored copy becoming zeroizing is a strict improvement even for the literal path.

`skein-silo` gains `src/secret.rs` with `OsKeychain`, the offline zero-config backend:

```rust
pub struct OsKeychain { store: Arc<CredentialStore> }
impl OsKeychain { pub fn new() -> Result<Self>; }
impl SecretProvider for OsKeychain { /* resolve parses keychain://svc/account */ }
// Provisioning lives on the concrete backend, NOT on the trait:
impl OsKeychain {
    pub fn store(&self, r: &SecretRef, value: &str) -> Result<()>;
    pub fn delete(&self, r: &SecretRef) -> Result<()>;
}
```

**Why `store`/`delete` are inherent and not on the trait**: the trait is what the product
*consumes*, and the product only ever reads. A `SecretProvider` handed to a `ToolGateway` must
have no expressible way to write a secret. Their callers today are the round-trip test and, next,
`skein secret set` in the CLI slice — so they are not a capability without a caller.

**Crate placement**: `skein-silo`, not a new `skein-keyring`. Design §7.2 says "one keychain per
silo": the OS credential store and the silo's SQLite file are the same per-silo local-backend
concern, and §4.8's `EmbeddedBackend` is named for exactly that role. A fifth crate for one
`impl` would be structure without a caller.

## Technical Context
**Language/Version**: Rust 1.97 (pinned in `rust-toolchain.toml`, unchanged this slice)
**Primary Dependencies**: `zeroize = { version = "1", default-features = false, features =
["alloc"] }` in `skein-core` (four direct dependencies become five). In `skein-silo`:
`keyring-core = "1"` plus one native store crate per OS behind
`[target.'cfg(target_os = "…")'.dependencies]` — `windows-native-keyring-store`,
`apple-native-keyring-store` (feature `keychain`), `linux-keyutils-keyring-store`. All confined to
`crates/skein-silo/src/secret.rs`.
**Storage**: the platform credential store — Windows Credential Manager, macOS Keychain Services,
Linux kernel session keyring
**Testing**: `cargo test`; three seam tests in `skein-core` against a `FakeProvider` double, five
tests in `skein-silo` against the **real** platform credential store
**Target Platform**: Windows + macOS + Linux
**Project Type**: library (four workspace members, unchanged)
**Performance Goals**: N/A
**Constraints**: `skein-core` may not name a credential store; `crates/skein-mcp/` and
`crates/skein-acp/` unchanged; no network
**Scale/Scope**: one new module per crate, one trait with two methods, one new error variant, one
new `Redactor` constructor

## Why not the `keyring` facade crate
The advisory plan this slice came from assumed a single `keyring = "4.2"` dependency with the
per-OS *features* `windows-native-keyring-store` / `apple-native-keyring-store` /
`linux-keyutils-keyring-store`. Reading the vendored 4.2.0 source refuted that shape:

- Those names are **optional, target-gated dependencies**, not standalone entry points.
  `keyring/src/lib.rs:32` is `compile_error!("At least one of the features 'v1' or 'cli' must be
  enabled")`, so enabling only the store features does not compile.
- The `v1` feature (the crate default) hard-codes **zbus Secret Service** on every non-Apple unix
  (`keyring/src/v1.rs:115`) — D-Bus, which a headless `ubuntu-latest` does not have. It is
  therefore the one shape this slice must not take.
- The `cli` feature does select keyutils on Linux, but it enables **every** store at once
  (`db-keystore`, `dbus-secret-service`, the sample store), which is a large dependency surface
  for one `impl`.
- `keyring`'s own module doc (`src/lib.rs:17-27`) says applications "which want to control which
  credential stores they use on which platforms … should not be linking to this library at all;
  they should instead be linking to `keyring-core` and any specific credential stores they want".

So the dependency is `keyring-core` plus exactly one store crate per OS. That is the same *intent*
the advisory plan expressed — a native store per OS, keyutils on Linux — reached through the
shape the library actually supports.

A second consequence: `keyring::v1` registers its store in a **process-global** `RwLock` static
(`keyring/src/v1.rs:108`). `OsKeychain` instead owns its `Arc<CredentialStore>` and calls
`CredentialStoreApi::build` directly, so a provider is an ordinary value. That keeps §7.2's "one
keychain per silo" expressible, and keeps two tests in one binary from fighting over a global.

## Constitution Check
*GATE: must pass before implementation.*
- **I. Headless core**: ✅ library only; `SecretProvider` is reachable through the existing
  headless API. No `[[bin]]`, no UI.
- **II. Local-first / silo isolation**: ✅ the OS credential store is local by construction;
  `requires_network()` is `false`, which is what makes the backend usable in Local mode with
  egress OFF (§7.3). `OsKeychain` installs no process-global store, so per-silo keychains stay
  expressible (§7.2).
- **III. Test-First**: ✅ T1 pins the `keyring-core` surface against the vendored source and a
  compiled probe **before** any product code; T3's red is observed and recorded before T4, T5's
  before T6.
- **IV. Inverted coupling**: ✅ `SecretProvider` is the seam. `keyring-core` and the store crates
  are named in exactly one module of one crate and never in `skein-core`.
- **V. Traceability**: ✅ one `Redactor`, one `redact`, one `redact_value` — the resolved path and
  the literal path share every line that touches the Ledger. `k4` proves the redaction end to end
  through the governed gateway and re-verifies the chain.
- **VI. Security / deny-by-default**: ✅ **this slice is the one that makes Principle VI's
  "by reference, never by value" true of the product.** `SecretValue` is zeroized on drop and
  `Debug`-redacted; `SecretProvider` cannot write; an unresolvable reference fails loudly rather
  than yielding a redactor that scrubs nothing; an unimplemented scheme is refused.
- **VII. Neutrality / YAGNI**: ✅ one backend, one scheme, two trait methods, one new constructor.
  No `SecretRef` enum, no provider registry, no `sops://`/`op://` stubs, no `skein secret set`
  CLI (there is no CLI crate yet).
- **VIII. Loop discipline (NON-NEGOTIABLE)**: ✅ `LoopController`, `ProgressProbe` and
  `NativeLoop` are untouched.
- **Cross-platform**: ✅ three `#[cfg(target_os)]` arms, one per supported OS, each with a native
  equivalent; the `#[cfg]` is one `use` and one constructor call. `core.yml`'s `paths:` already
  covers `crates/**` and `Cargo.toml`, so no CI edit — confirmed by reading, not edited.

## Project Structure

### Documentation (this feature)
```text
specs/010-secret-provider/
├── spec.md      # this feature's requirements
├── plan.md      # this file
└── tasks.md     # executable breakdown
```

### Source Code (repository root)
```text
Cargo.toml                        # +zeroize, +keyring-core and the three store crates
crates/skein-core/
  Cargo.toml                      # +zeroize
  src/error.rs                    # +SkeinError::Secret
  src/secret.rs                   # NEW — SecretRef, SecretValue, SecretProvider
  src/lib.rs                      # +mod secret, re-exports
  src/tool.rs                     # Redactor's field becomes Vec<SecretValue>; +Redactor::resolve
  tests/core.rs                   # +3 seam tests
crates/skein-silo/
  Cargo.toml                      # +keyring-core, +per-OS store crates
  src/lib.rs                      # +mod secret, re-export OsKeychain
  src/secret.rs                   # NEW — the only module that names a credential store
  tests/silo_secret.rs            # NEW — k1..k5
```
**Structure Decision**: `crates/skein-mcp/` and `crates/skein-acp/` are byte-identical to `dev`,
so specs 005 and 008's suites remain live controls. `Redactor::new` is unchanged, so every
pre-existing test is a live control on the literal path.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| **The `Redactor` holds resolved plaintext in memory**, in tension with Principle VI's "references, never values" | Inherent to redaction: you cannot scrub a string you cannot recognise. The value arrives from a reference, is never persisted, is zeroized on drop and never renders in `Debug`. | Scrubbing by *pattern* rather than by value (regex over `token=…`): reconstructs intent from free text, catches the shapes it was told about and silently misses the rest — the opposite of deny-by-default. Hashing the secret and scrubbing by hash: you cannot find a substring by its hash. |
| **Four new dependencies in `skein-silo` (one shared, three target-gated)** instead of the single `keyring` facade the advisory plan assumed (Principle VII) | The facade does not compile without `v1` or `cli`; `v1` hard-codes D-Bus on Linux and `cli` enables every store. `keyring`'s own docs direct applications that pick their stores to `keyring-core` + specific stores. Only one store crate is ever compiled for a given target. | `keyring` with `cli`: pulls `db-keystore`, `dbus-secret-service` (with vendored crypto), `zbus`, and the sample store into a build that uses one of them. `keyring` with `v1`: no keyutils, so the Linux CI leg would need D-Bus. |
| **`SecretValue` is a newtype over `Zeroizing<String>` rather than a plain `String`** | §7.13 names `zeroize`-on-drop explicitly, and it is the whole reason the type exists. | A hand-rolled `Drop` overwriting a `String`'s bytes: `String` reallocation and the optimizer make it unreliable, and reliability is the point. `zeroize` has no transitive dependencies. |
| **`OsKeychain::store` / `::delete` exist with only a test caller in this slice** (Principle VII) | A round-trip acceptance against a *real* credential store cannot be written without a way to put a credential there, and a test that leaves a credential behind is worse than no test. The next caller is `skein secret set` in the CLI slice. | Provisioning the credential out-of-band with a platform CLI in a test fixture: three OS-specific shell paths in test code, and no product API for the CLI slice to build on. |
