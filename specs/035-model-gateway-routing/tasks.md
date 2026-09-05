# Tasks: named-provider routing and the egress policy layer (v0 slice)

**Spec:** `specs/035-model-gateway-routing/spec.md` · **Plan:** `specs/035-model-gateway-routing/plan.md` ·
TDD (red→green), branch `feat/model-gateway-multiprovider`.

Each task records its **observed red** verbatim. Where a step had no red, the entry says so and why,
and carries a measured counterfactual instead.

---

## T1 — RED: a named local provider routes to its own address

**Action:** CREATE `crates/skein-gateway/tests/provider_routing.rs` — a `Stub` that counts
`accept()`s, an in-test `FakeSecrets` implementing `skein_core::SecretProvider` directly, and three
tests: a named route reaches its own base URL with its own model and sends no `Authorization`
header; an unknown name is refused and names what *is* configured; a `Local` route pointed off this
machine is still refused.

### Observed red

```
error[E0432]: unresolved imports `skein_gateway::ProviderKind`, `skein_gateway::ProviderRoute`,
              `skein_gateway::ProviderTable`, `skein_gateway::Router`
  --> crates\skein-gateway\tests\provider_routing.rs:22:21
   |                     no `ProviderKind` in the root … no `Router` in the root
error: could not compile `skein-gateway` (test "provider_routing") due to 1 previous error
```

**Note.** `skein-silo`'s `TestSecret` fixture was deliberately *not* reused: it is backed by the
real OS keychain and lives in a crate `skein-gateway` must not depend on (Constitution IV).
Implementing the trait in-test is a handful of lines and tests the router against the boundary it
actually promises.

---

## T2 — GREEN: `route.rs` — `ProviderKind`, `ProviderRoute`, `ProviderTable`, `Router`

**Action:** CREATE `crates/skein-gateway/src/route.rs`; wire `mod route` + re-exports into
`lib.rs`.

One incidental change was needed to compile the test: `expect_err` requires `T: Debug`, so
`OpenAiCompatClient` gained a `Debug`. It was **derived** at this point; T4 replaces it with a
hand-written one for the reason recorded there.

**Result:** `3 passed; 0 failed`.

---

## T3 — RED: a cloud route resolves its credential and sends `Authorization: Bearer`

**Action:** UPDATE `crates/skein-gateway/tests/provider_routing.rs` — four tests: the bearer header
appears on the wire and the token appears in neither the request line nor the body; a cloud route
with no credential sends no header at all; a 401 produces an error naming the status and **not** the
token; a credential store answering `requires_network() == true` is refused when egress is off.

### Observed red

```
---- a_cloud_route_resolves_its_credential_and_sends_it_as_a_bearer_token stdout ----
panicked at crates\skein-gateway\tests\provider_routing.rs:356:5:
the resolved credential is sent as a bearer token, in:
POST /v1/chat/completions HTTP/1.1
content-length: 85
user-agent: ureq/3.4.0
accept: */*
host: 127.0.0.1:65043
content-type: application/json

---- a_credential_store_that_needs_the_network_is_refused_when_egress_is_off stdout ----
panicked at crates\skein-gateway\tests\provider_routing.rs:457:10:
a networked secret store is egress, whatever the route's kind: OpenAiCompatClient { endpoint:
LocalEndpoint { base_url: "http://127.0.0.1:65045/v1" }, model: "llama3.1", agent: Agent { config:
Config { http_status_as_error: false, https_only: false, ip_family: Any, proxy: None, no_delay:
true, max_redirects: 10, redirect_auth_headers: Never, save_redirect_history: false, user_agent:
Default, timeouts: Timeouts { global: Some(10s), … } … } } }

test result: FAILED. 5 passed; 2 failed
```

**What the second failure taught, beyond the assertion.** The derived `Debug` from T2 dumped
`ureq::Agent`'s entire configuration into the failure message. That is the observation that produced
T4's hand-written formatter — the credential would have been safe either way, but a formatter that
prints everything is the wrong default on a type that is about to hold one.

---

## T4 — GREEN: `NetworkEndpoint` + the bearer token on `OpenAiCompatClient`

**Action:** UPDATE `route.rs` (add `NetworkEndpoint`, wire the `Cloud` branch and credential
resolution) and `lib.rs` (private `Endpoint` enum, `networked()` constructor, `with_bearer_token()`,
hand-written `Debug`, `provider_noun()`).

### A regression this caught, and the decision it forced

Rewriting the unreachable-provider message to cover both endpoint kinds broke a spec-012 test:

```
---- an_unreachable_provider_fails_with_a_message_naming_the_endpoint stdout ----
panicked at crates\skein-gateway\tests\openai_compat.rs:557:5:
the operator must be told which endpoint and what to check, got: POST
http://127.0.0.1:57266/v1/chat/completions failed: io: Connection refused; is a provider
listening at http://127.0.0.1:57266/v1?
```

The assertion was **not** loosened. "is a *local* provider listening" is what sends someone to check
whether Ollama is running — the message's only actionable word — and `cli_chat.rs` asserts it too.
`Endpoint::provider_noun()` keeps it accurate per kind instead.

`LocalEndpoint::chat_completions_url` became dead when `Endpoint` gained its own and was removed.

**Result:** `24 passed; 0 failed; 1 ignored` across the crate.

---

## T5 / T6 — egress refusal: **no red**, and a measured mutation instead

**Action:** UPDATE `crates/skein-gateway/tests/provider_routing.rs` — three tests: a cloud route
with egress off is refused before any connection is opened; a local route is unaffected by egress
being off; a cloud route with egress on reaches its address.

### There was no red, and this is why

The egress guard was written into `Router::client_for` during **T2**, not deferred to T6 — the
ordering constraint ("egress before anything network-shaped is built") is a property of the
function's structure, and writing the function without it and then inserting it would have been a
worse sequence than writing it once in the right order. All three tests therefore passed on first
run: `10 passed; 0 failed`.

A test that has never failed is not yet known to be load-bearing, so the guard's condition was
**mutated to `false`** and the suite re-run:

```
test egress_off_refuses_a_cloud_route_before_any_connection_is_opened ... FAILED
test result: FAILED. 9 passed; 1 failed
```

Exactly one test fails, and it is the right one — a stronger guarantee than a red, because it is
repeatable at any later point. The guard was restored and the suite re-verified green.

**Why the stub is live and listening in that test.** A dead port would pass even with no refusal at
all. `connection_count()` increments on `accept()` rather than on a parsed request, so a client that
connects and says nothing is still counted as egress.

---

## T7 — RED+GREEN: `ProviderTable::from_toml_str` and the `toml` dependency

**Action:** CREATE tests (seven), UPDATE `route.rs`, `crates/skein-gateway/Cargo.toml`, workspace
`Cargo.toml`.

### Observed red

```
error[E0599]: no associated function or constant named `from_toml_str` found for struct
              `ProviderTable` in the current scope
   (×5, plus `from_path`)
```

**Result:** `17 passed; 0 failed`.

### The dependency risk, measured

This slice's top pre-recorded risk was that `toml` would pull a TLS crate transitively and silently
break spec 012 SC-007. Measured after the addition:

```
$ cargo tree -e normal -p skein-gateway | grep -icE 'rustls|native-tls|webpki|openssl'
0

$ cargo tree -e normal -p skein-gateway
├── toml v1.1.5+spec-1.1.0
│   ├── serde_core v1.0.229
│   ├── serde_spanned v1.1.1
│   ├── toml_datetime v1.1.1+spec-1.1.0
│   ├── toml_parser v1.1.3+spec-1.1.0
│   │   └── winnow v1.0.4
│   └── winnow v1.0.4
```

Five packages, all parse-only. The prepared fallback — hand-rolling the flat `[[provider]]` parser —
was not needed. `default-features = false` also drops `display`, TOML's *writer*: Skein reads an
operator's file and never writes one, so the writer could only ever be dead code.

`deny_unknown_fields` is load-bearing rather than decorative: the realistic typo is `credentials`
for `credential`, and serde's default (ignore it) yields a cloud provider that silently sends no
`Authorization` header — a failure an operator debugs at the provider, having been given no reason
to suspect their own file.

---

## T8 — `wiring.rs`: `ProviderArgs` and `LazyKeychain`

**Action:** UPDATE `crates/skein-cli/src/wiring.rs`.

`ProviderArgs::client()` returns `Result<Option<OpenAiCompatClient>>` — `None` when `--provider` is
absent, and in that case the provider file is **not read at all**.

`LazyKeychain` opens `OsKeychain` on first `resolve()`. Plan-level guidance was to reuse the
`OsKeychain` already built for `RedactArgs::redactor()`, but there is no such instance to reuse:
`redactor()` constructs one internally and only when `--redact` is non-empty, precisely so a run
configuring no secret acquires no credential-store dependency. Eagerly building one here would have
broken that rule for every named provider — including a cloud one about to be refused for egress,
since `client_for` checks egress *before* resolving anything. The lazy adapter preserves the rule
one layer down.

**Result:** `cargo build -p skein-cli` clean.

---

## T9 — `chat.rs` / `main.rs`: branch on `--provider`

**Action:** UPDATE both. `ProviderArgs` is flattened into `ChatArgs` only. Resolution happens in the
position `ModelArgs::endpoint()` occupied, before the silo — so an unreadable table, an unknown name
and a refused egress are all exit codes before a chain exists.

`--timeout-secs` is threaded through both paths: it is a budget for the request rather than a
property of the provider.

**Result:** `cargo build -p skein-cli` clean; `skein chat --help` lists all three flags.

---

## T10 — CLI-level subprocess tests

**Action:** UPDATE `crates/skein-cli/tests/cli_chat.rs`. Six tests, each a real-binary invocation;
`StubProvider` gained the same `accept()`-counter as the gateway stub.

The refusal test asserts, beyond the exit code and the message, that **no silo directory was
created** — the end-to-end form of "refused before the chain".

The backward-compatibility test points `--providers-file` at deliberately malformed TOML and asserts
the run succeeds, which proves the file is not read when `--provider` is absent. The twelve
pre-existing `cli_chat` tests are unchanged live controls.

**Result:** `18 passed; 0 failed`.

---

## T11 — Validation, spec docs, README

```
$ cargo fmt --all -- --check                                   → clean
$ cargo clippy --workspace --all-targets -- -D warnings        → clean
$ cargo test --workspace                                       → 251 passed, 5 ignored, 0 failed
$ cargo tree -e normal -p skein-gateway | grep -icE 'rustls|native-tls|webpki|openssl'  → 0
$ git diff --stat -- crates/skein-mcp crates/skein-acp crates/skein-silo crates/skein-core → empty
```

**Environment note, recorded because it interrupted the run and not because it is a code fact.** The
first `cargo test --workspace` failed with `os error 112` (`Espace insuffisant sur le disque`) —
drive `D:` was at 100% with 3.1 MB free. `target/debug/incremental` (4.0 GB) was removed and the
suite re-run with `CARGO_INCREMENTAL=0`. Nothing outside this worktree's build artifacts was
touched. **The drive remains close to full and is worth attention independently of this slice.**

---

## Next slice (not this feature)

- [ ] **A TLS backend, decided on its own merits.** Until then a `Cloud` route at a real `https://`
      address fails with `ureq::Error::TlsRequired`. This is the one thing standing between this
      slice and a working cloud call, and spec 012 FR-003 says explicitly that it must not be
      relaxed *for convenience* — so it needs a slice that argues for it, not a feature flag flipped
      in passing.
- [ ] **`--model` when `--provider` is given.** Still required by clap and then ignored. The fix is
      either flattening `ProviderArgs` into `skein acp-agent` too, or splitting `ModelArgs` so the
      model name is separable from the budget flags. Both are decisions about `acp-agent`'s session
      model. See spec.md Assumptions.
- [ ] **`--provider` for `skein acp-agent`**, which raises the real question this slice deferred:
      may a session switch provider mid-conversation, and if so what does the chain record about the
      switch?
- [ ] **redaction on the `LlmRequest`/`LlmResponse` path.** Carried from slices 011 and 012 and
      still open. This slice's credential never reaches the chain, but conversation content still
      does, raw.
- [ ] **`ModeSupervisor` and the `Mode` hierarchy** (design §4.8). `egress_allowed: bool` is the
      seam a real mode would replace; nothing else in the product has a mode-shaped thing yet.
- [ ] **ADR-0002 D4's process-level socket-deny boundary.** This slice added the policy layer above
      it. The boundary itself would close `LocalEndpoint`'s `localhost` re-resolution TOCTOU
      residual and would make the egress refusal enforced rather than merely obeyed.
- [ ] **A layered configuration system** (design §5.5, ADR-0002 D3). The flat `[[provider]]` table
      should become a section inside it, not something reconciled with it.
- [ ] **raw-wire-byte capture**, **streaming (SSE)**, **sampling parameters** — carried unchanged
      from slice 012's list.
