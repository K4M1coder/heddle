# Feature Specification: `SecretProvider` (OS keychain) + just-in-time `Redactor` (v0 slice)

**Feature Branch:** `010-secret-provider` · **Created:** 2026-09-03 · **Status:** Implemented (v0 slice)
**Input:** `specs/009-silo-ledger/tasks.md` "Next slice" — *"`SecretProvider` (OS keychain) + JIT
`Redactor` — spec 010, extending `crates/skein-silo`"* · Constitution VI (**secrets by reference,
never by value**, resolved just-in-time, redacted from logs), IV (**the core discovers secrets
through a trait**), III (**test-first**), VII (**no capability without a real need**) ·
design §7.13, §7.2 ("one keychain per silo"), §7.3 (egress).

Nine merged slices built a governed loop whose Ledger already scrubs secrets — but only secrets
the operator handed it **by value**. `crates/skein-core/src/tool.rs` said so in `Redactor`'s own
doc comment: *"The values are configuration today; they will come from `SecretProvider::resolve`
(design §7.13) once that lands."* A config that carries the literal secret so the redactor can
recognise it is precisely the thing Principle VI forbids.

This slice closes that loop. `skein-core` gains the §7.13 seam — `SecretRef` (a URI, never a
value), `SecretValue` (zeroized on drop, `Debug`-redacted) and the `SecretProvider` trait — and a
`Redactor::resolve` constructor that turns configuration-held *references* into in-memory values
at gateway-construction time. `skein-silo` gains `OsKeychain`, the offline zero-config default
backend, reading the platform credential store.

## User Scenarios & Testing

### User Story 1 — A resolved secret cannot leak through a formatter (P1)
As an auditor, a secret must not reach a log because someone derived `Debug` on a struct holding
one.
**Acceptance:**
1. **Given** a `SecretValue` built from `"hunter2"`, **When** it is formatted with `{:?}`,
   **Then** the output does not contain `hunter2`.

### User Story 2 — The Ledger is scrubbed from a reference, not from a literal (P1)
As an operator, my config names `keychain://<service>/<account>` and never the secret itself.
**Acceptance:**
1. **Given** a `SecretProvider` that resolves one `SecretRef`, **When** a `Redactor` is built with
   `Redactor::resolve(&provider, &[r])`, **Then** it scrubs the resolved value out of text.
2. **Given** a reference the provider cannot resolve, **When** `Redactor::resolve` is called,
   **Then** it returns `Err` — a misconfigured reference fails loudly rather than silently
   producing a redactor that scrubs nothing.

### User Story 3 — The OS credential store is a real backend (P1)
As an operator on Windows, macOS or Linux, the zero-config default reads the platform's own vault.
**Acceptance:**
1. **Given** an `OsKeychain`, **When** a value is stored under a test-unique reference, resolved,
   and deleted, **Then** `expose()` matches what was stored and a second `resolve` is `Err`.
2. **Given** a reference that was never stored, **When** it is resolved, **Then** the result is
   `Err(SkeinError::Secret)` — never an empty value.
3. **Given** a reference in a scheme this backend does not implement (`op://vault/item`),
   **When** it is resolved, **Then** the result is `Err` — the one implemented backend does not
   silently pretend to serve other schemes.

### User Story 4 — A provider-resolved secret is redacted before it reaches the Ledger (P1)
As an auditor, the end-to-end claim is about the governed path, not about a string helper.
**Acceptance:**
1. **Given** a secret in the OS keychain, a `Redactor` resolved from its reference, and a
   `ToolGateway` over a transport whose outcome echoes the secret, **When** an allowlisted
   read-only tool is called against a Ledger, **Then** the raw `ToolOutcome` still carries the
   real secret (the caller needs it), the `ToolResult` step's payload contains `***` and **not**
   the secret, and `verify_chain` passes.

### User Story 5 — Availability is governed by the egress policy (P1)
As an operator in Local mode with egress OFF, only offline backends are usable.
**Acceptance:**
1. **Given** an `OsKeychain`, **When** `requires_network()` is asked, **Then** it is `false`.

## Requirements
- **FR-001**: `skein-core` MUST define `SecretRef`, `SecretValue` and `SecretProvider`, and MUST
  NOT name a credential store, `keyring`, or any OS API (Constitution IV).
- **FR-002**: `SecretValue` MUST zeroize its bytes on drop and MUST NOT render its value through
  `Debug`. Reading it MUST require an explicit `expose()` call.
- **FR-003**: `SecretRef` MUST carry a URI naming a secret and never a value.
- **FR-004**: `Redactor::new`'s signature MUST be unchanged, so every pre-existing test stays a
  live control on this slice.
- **FR-005**: `Redactor::resolve(provider, refs)` MUST resolve every reference and MUST return
  `Err` if any one fails.
- **FR-006**: `Redactor` MUST store its secrets as `SecretValue`, so the values it holds are
  zeroized on drop on both the literal and the resolved path.
- **FR-007**: `OsKeychain` MUST live in `skein-silo` and MUST implement `SecretProvider` with
  `requires_network() == false`.
- **FR-008**: `OsKeychain::resolve` MUST accept only `keychain://<service>/<account>` and MUST
  reject any other scheme, an empty service, or an empty account.
- **FR-009**: Provisioning (`store`, `delete`) MUST be **inherent** methods on `OsKeychain` and
  MUST NOT appear on `SecretProvider`: a provider handed to a `ToolGateway` must have no
  expressible way to write a secret.
- **FR-010**: `OsKeychain` MUST use the platform's native credential store on all three OSes,
  each behind `#[cfg]`, with no OS-specific call lacking an equivalent.
- **FR-011**: `OsKeychain` MUST NOT install a process-global default credential store; the store
  is owned by the `OsKeychain` value, so design §7.2's "one keychain per silo" stays expressible.
- **FR-012**: A missing credential MUST surface as `SkeinError::Secret`, distinct from a
  transport or storage failure.

## Success Criteria
- **SC-001**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` all clean; the suite is 63 pre-existing + 8 new = **71** tests
  (2026-09-03).
- **SC-002**: The `OsKeychain` acceptance runs against the **real** platform credential store. No
  in-memory stand-in for the backend under test.
- **SC-003**: `git diff dev -- crates/skein-mcp/ crates/skein-acp/` is empty.
- **SC-004**: `git diff dev -- spikes/ .github/ rust-toolchain.toml` is empty.
- **SC-005**: `git diff dev -- Cargo.toml` shows only added `[workspace.dependencies]` entries.
- **SC-006**: Every pre-existing test still passes with its body unchanged — `Redactor::new` is
  untouched, so all 63 are live controls.
- As in specs 004–009, the macOS and Linux legs of `core.yml` are unobserved until the repository
  has a remote; only the Windows leg is run locally.

## Assumptions
- **Resolution happens once, at gateway construction, not per call.** §7.13 says the secret is
  resolved "at the precise moment a command is executed". The `Redactor` must know a secret's
  *value* to scrub it out of **every** payload, so resolving per call would resolve on every step
  and hold the value anyway. Resolving once from a reference the config stored is the honest
  reading of §7.13 for this consumer; a per-call resolution belongs to the future consumer that
  *injects* a secret into a subprocess env or an auth header, which has no caller yet.
- **The `Redactor` holds plaintext in memory**, in tension with "references, never values". This
  is inherent: you cannot scrub a string you cannot recognise. The mitigations are real — the
  value comes from a reference, is never persisted, is zeroized on drop, and never renders in
  `Debug`. Recorded in the plan's Complexity Tracking.
- **`SecretRef` is a string URI, not an enum of schemes.** One scheme is implemented; the enum
  arrives with the second backend (`sops://`, `op://`, …). The parse rejects everything else, so
  no backend silently serves a scheme it does not implement.
- **`keyring-core` + per-OS store crates, not the `keyring` facade.** See the plan; the facade
  refuses to compile without its `v1` or `cli` feature, and `v1` selects D-Bus Secret Service on
  Linux, which a headless runner does not have.
- **The Linux backend is the kernel session keyring (`linux-keyutils`).** It needs no D-Bus, no
  `gnome-keyring` and no graphical session, so it works on a headless `ubuntu-latest`. Its
  credentials are **session-scoped** rather than persisted to disk — correct for a JIT secret
  cache, and what makes the round-trip test runnable in CI.
- **The macOS leg is unobservable from this repository today.** If a hosted runner's login
  keychain proves to be locked, the honest response is `#[ignore]` with a recorded reason, never
  a fake in-memory provider standing in for the acceptance.
