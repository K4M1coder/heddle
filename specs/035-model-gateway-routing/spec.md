# Feature Specification: named-provider routing and the egress policy layer (v0 slice)

**Feature Branch:** `feat/model-gateway-multiprovider` · **Created:** 2026-09-05 · **Status:** Implemented (v0 slice)
**Input:** `specs/012-model-gateway/tasks.md` "Next slice" — three of its open items at once:
*"**provider authentication**, when a local gateway needs one: an `Authorization: Bearer` whose
value arrives as a `SecretRef` resolved through `SecretProvider`"*, *"a config file holding the base
URL, the model … so `--base-url`/`--model` are not the only way to name them"*, and *"the
egress-policy layer"* · Constitution II (**local-first, NON-NEGOTIABLE**), III (**test-first**),
IV (**inverted coupling**), VI (**security, deny-by-default**), VII (**no capability without a real
need**) · design §4.5 (Model Gateway), §7.3 (egress policy), Phase 1 MVP axis 1b
(*"switching cloud↔local"*) · ADR-0002 D4 (*"`requires_network()` … checked at enable-time"*).

Slice 012 shipped one `ModelClient` pointed at one hardcoded loopback address. A provider was not a
*thing* in the product — it was a `--base-url` string an operator retyped, with no name, no
credential and no declared nature. Egress enforcement was entirely `LocalEndpoint::parse`'s loopback
check, which is a check on an *address*; there was no check on a **policy**. And despite
`SecretProvider::requires_network()` existing since slice 010, `grep -rn "requires_network"
crates/*/src/` returned exactly one hit before this slice: the `false` that `OsKeychain` answers
with. Nothing read it.

This slice makes a provider a named, switchable, credentialed thing, and makes egress a decision the
router takes before anything network-shaped exists.

## What this slice lets a user do, and what it does not

**It does:**

```
skein chat --silo <ID> --model <NAME> --provider <PROVIDER-NAME>
           [--providers-file <PATH>] [--allow-egress] [--prompt <TEXT>]
```

with a `providers.toml` an operator writes once:

```toml
[[provider]]
name = "local-ollama"
kind = "local"
base_url = "http://localhost:11434/v1"
model = "llama3.1"

[[provider]]
name = "cloud-primary"
kind = "cloud"
base_url = "https://api.example.com/v1"
model = "gpt-4o-mini"
credential = "keychain://skein/cloud-primary"
```

Naming `--provider local-ollama` routes to that address with that model. Naming
`--provider cloud-primary` **without** `--allow-egress` is refused by name, with no socket opened.
Naming it **with** `--allow-egress` resolves the credential through the existing `SecretProvider`
and sends it as `Authorization: Bearer`.

**It does not:**

- **Complete a real cloud HTTPS call.** `ureq` still carries `default-features = false`, so no TLS
  backend is compiled in and a real `https://` provider fails at the transport with
  `ureq::Error::TlsRequired`. Spec 012's FR-003 and SC-007 are unchanged and remain measured. This
  slice ships the routing, the credential path and the refusal; the transport is a separate,
  separately-justified decision. See Assumptions.
- **Build a `ModeSupervisor` or a `Mode` hierarchy** (design §4.8). One `bool` threaded from one
  flag, not an enum with a lifecycle.
- **Enforce egress at the OS level.** This is the policy layer *above* ADR-0002 D4's process-level
  socket-deny boundary — the same role `LocalEndpoint::parse` already plays. The boundary itself
  still does not exist.
- **Introduce a general configuration system.** No `[team]`/`[project]`/`[conversation]` layering
  (design §5.5, ADR-0002 D3). One flat `[[provider]]` table.
- **Change `skein acp-agent`.** It keeps `--base-url`/`--model`. What a session may switch
  mid-conversation is an unanswered question, and answering it by accident is worse than deferring
  it.

## Five things a reader must know up front

1. **`ProviderKind` is a declaration, not an inference.** `kind = "cloud"` is what the *operator*
   said the provider is, and it is what the egress policy acts on. It is deliberately not derived
   from the address, because deriving it would make a security policy a property of DNS and of
   whatever the address happens to resolve to today. This is why the tests can point a `Cloud` route
   at a loopback stub and observe the real bytes without a TLS backend — and why that is not a
   contradiction.

2. **The refusal is ordered, and the order is the guarantee.** `Router::client_for` runs: find route
   → **egress check** → parse address → resolve credential → build client. The egress check sits
   above credential resolution even though neither opens a socket, because a future `SecretProvider`
   backend may reach a network to answer. "No connection was attempted" must be true of the whole
   call, not only of the model request.

3. **The refusal is unrepresentable as a client.** `client_for` returns `Result<OpenAiCompatClient>`
   — not `Result<Option<…>>`, and not a client carrying a "disabled" flag. An `Option` a caller
   could `unwrap_or_else(build_it_anyway)` would move the egress decision out of the router and into
   every call site.

4. **This is the first code in the workspace to read `requires_network()`.** ADR-0002 D4 asked for
   it at enable-time; it is checked here. With egress off, a credential store that must leave this
   machine to answer is itself egress — **whatever the route's own kind is**. A `local` provider
   whose key lives in a cloud-hosted vault is exactly that case, which is why the check is not
   folded into the `Cloud` branch.

5. **The credential is a `SecretValue` from resolution to header, and `expose()` is called once.**
   It is never bound to a named local in a function that also builds an error message, never
   `Debug`-formatted (`SecretValue`'s hand-written formatter renders `SecretValue(***)`), and never
   placed anywhere on the wire but the `Authorization` header. `OpenAiCompatClient`'s `Debug` is
   hand-written for the adjacent reason: a derived one would print `ureq::Agent`'s whole
   configuration, and "print only what you meant to print" is the discipline that keeps rule 5 true
   by default rather than by review.

## User Scenarios & Testing

### User Story 1 — An operator names a provider instead of retyping a URL (P1)

A `providers.toml` names `local-ollama`. `skein chat --provider local-ollama` reaches that address
with that model, and the run lands on the chain exactly as a `--base-url` run does.

**Acceptance**: `chat_routes_through_a_named_local_provider` — a subprocess invocation whose
`--model` deliberately disagrees with the route's, asserting the wire carries the *route's* model,
and whose run then passes `skein ledger verify`.

### User Story 2 — A cloud provider is refused when egress is off, before any socket exists (P1)

The operator is in local mode. Naming a `cloud` provider fails with an error that names the
provider, the policy and the flag that would permit it — and nothing leaves the machine.

**Acceptance**: `egress_off_refuses_a_cloud_route_before_any_connection_is_opened` and
`chat_refuses_a_cloud_provider_without_allow_egress_and_opens_no_socket`. Both point the route at a
**live, listening** stub, so nothing but the router prevents the connection; both assert
`connection_count() == 0`. The CLI one additionally asserts no silo was created, proving the refusal
precedes the chain.

### User Story 3 — A cloud provider's credential is resolved and sent, and never leaks (P1)

The route names a `SecretRef`. The router resolves it through `SecretProvider` and the client sends
`Authorization: Bearer <token>`. When the provider rejects it, the operator is told — and the token
is not in the message.

**Acceptance**: `a_cloud_route_resolves_its_credential_and_sends_it_as_a_bearer_token` asserts the
header on the wire and asserts the token is absent from the request line and the body;
`a_rejected_credential_never_appears_in_the_error_it_produces` drives a 401 and asserts the error
names the status and not the token.

### User Story 4 — A local provider stays local, whatever the operator declared (P1)

Declaring a route `kind = "local"` narrows what it may address and never widens it.

**Acceptance**: `a_local_route_pointed_off_this_machine_is_still_refused` — a `Local` route at a
public address is refused **even with `--allow-egress`**, because `LocalEndpoint::parse` is
unchanged and still the only way a `Local` route obtains an address.

### User Story 5 — A mistake in the config file is a refusal, not a silent difference (P2)

An unknown provider name, a misspelled key, a duplicate name, an unrecognised `kind`, an unreadable
file: each is an exit code and a message naming the offending value.

**Acceptance**: `an_unknown_provider_name_is_refused_and_named` (which also lists what *is*
configured), `a_misspelled_key_is_refused_rather_than_ignored`, `two_providers_with_one_name_are_refused`,
`an_unrecognised_kind_is_refused`, `a_missing_providers_file_is_refused_by_name`, and the CLI twins
`chat_refuses_an_unknown_provider_name` / `chat_refuses_a_providers_file_it_cannot_read`.

### User Story 6 — Every existing invocation behaves exactly as it did (P1)

A run that does not pass `--provider` is unaffected, down to never opening the provider file.

**Acceptance**: `chat_without_a_provider_never_reads_the_providers_file` points `--providers-file`
at deliberately malformed TOML and asserts the run succeeds. The twelve pre-existing `cli_chat`
tests are unchanged live controls.

## Requirements

- **FR-001**: `ProviderKind` MUST be a closed set of `Local` and `Cloud`, deserialized from
  `kind = "local" | "cloud"`, and MUST record what the operator declared. It MUST NOT be inferred
  from the base URL.
- **FR-002**: `ProviderRoute` MUST carry `name`, `kind`, `base_url`, `model` and an optional
  `credential`. The credential MUST be a `SecretRef` — a reference, never a value (Constitution VI).
  A route without one MUST be routed without an `Authorization` header, which is not an error.
- **FR-003**: `ProviderTable::from_toml_str` MUST parse a flat `[[provider]]` table and MUST refuse,
  naming the offending value: an unrecognised `kind`; an unknown key (`deny_unknown_fields`); two
  providers sharing a name. An empty input MUST parse to an empty table rather than fail.
- **FR-004**: `ProviderTable::from_path` MUST return `SkeinError::Model` naming the path for every
  failure, including io ones. A raw `io::Error` MUST NOT reach the operator, because it names no
  path.
- **FR-005**: `ProviderTable::find` MUST refuse an unknown name with a message that lists the
  configured names.
- **FR-006**: `Router::client_for` MUST take `(name, &dyn SecretProvider, egress_allowed: bool,
  timeout)` and return `Result<OpenAiCompatClient>`. It MUST NOT return an `Option` or a client with
  an internal disabled flag: a refused route must be unrepresentable as a client.
- **FR-007**: `Router::client_for` MUST refuse a `Cloud` route when `egress_allowed` is `false`,
  **before** parsing the address and **before** resolving any credential, with a message naming the
  provider, the policy and `--allow-egress`.
- **FR-008**: `Router::client_for` MUST refuse a route with a credential when `egress_allowed` is
  `false` and `SecretProvider::requires_network()` is `true`, regardless of the route's kind
  (ADR-0002 D4).
- **FR-009**: A `Local` route MUST obtain its address through `LocalEndpoint::parse`, unchanged.
  Declaring a route `Local` MUST NOT weaken the loopback guard, and `--allow-egress` MUST NOT
  weaken it either.
- **FR-010**: `NetworkEndpoint::parse` MUST accept `http` and `https` and any host, and MUST keep
  `LocalEndpoint::parse`'s "not a URL" and "no scheme" refusals identical. It MUST be a distinct
  type from `LocalEndpoint`, so nothing holding a `LocalEndpoint` can be handed an off-machine
  address.
- **FR-011**: `ureq` MUST keep `default-features = false`. Spec 012 FR-003 and SC-007 stand
  unmodified; an `https://` `Cloud` route failing with `TlsRequired` is the expected outcome of this
  slice, not a defect of it.
- **FR-012**: The resolved credential MUST be held as `SecretValue` from resolution to use, MUST
  reach the wire only as an `Authorization: Bearer` header, and MUST NOT appear in any
  `SkeinError` message or any `Debug` output.
- **FR-013**: The new `toml` dependency MUST be declared `default-features = false` with parse-only
  features, and MUST NOT introduce a TLS crate into `skein-gateway`'s tree.
- **FR-014**: `skein chat` MUST gain `--provider`, `--providers-file` (default `providers.toml`) and
  `--allow-egress`. `--allow-egress` MUST default to `false`.
- **FR-015**: When `--provider` is absent, the provider file MUST NOT be read and the
  `--base-url`/`--model` path MUST be byte-for-byte the path taken before this slice.
- **FR-016**: When `--provider` is present it MUST win over `--base-url` and `--model`, with no
  merge: a named provider carries its own address and model, and taking half of each would produce
  a configuration no operator wrote down.
- **FR-017**: The platform credential store MUST NOT be opened for a run whose selected route names
  no credential, mirroring `RedactArgs::redactor`'s rule that a run configuring no secret acquires
  no runtime credential-store dependency.
- **FR-018**: `--provider` resolution MUST happen before the silo is opened, in the position
  `ModelArgs::endpoint()` occupies today, so a refused route leaves no chain recording an attempt
  that never left the process.
- **FR-019**: `crates/skein-mcp`, `crates/skein-acp`, `crates/skein-silo` and `crates/skein-core`
  MUST be unchanged.
- **FR-020**: No automated test may require a running Ollama or any real network egress.

## Success Criteria

- **SC-001**: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` all clean. Measured: **251 passing, 5 ignored** — 228 pre-existing
  plus 23 new (17 in `provider_routing.rs`, 6 in `cli_chat.rs`).
- **SC-002**: `cargo tree -e normal -p skein-gateway` shows **no** `rustls`, `native-tls`, `webpki`
  or `openssl`. Measured after adding `toml`: 0 matches. `toml` contributes five packages
  (`serde_core`, `serde_spanned`, `toml_datetime`, `toml_parser`, `winnow`), all parse-only.
- **SC-003**: Every wire test asserts **bytes on a real socket** served by `std::net::TcpListener`.
  No HTTP-mocking dependency is added (spec 012 SC-003 held).
- **SC-004**: Every `skein chat` test is a **process invocation of the real binary** (spec 012
  SC-004 held).
- **SC-005**: The "no connection was opened" claim is asserted by a **counter incremented on
  `accept()`**, at both the router level and the subprocess level, against a live listening stub —
  not by a dead port, and not by the absence of a parsed request.
- **SC-006**: `git diff -- crates/skein-mcp/ crates/skein-acp/ crates/skein-silo/ crates/skein-core/`
  is empty. Measured: empty.
- **SC-007**: The egress guard is shown load-bearing by mutation: replacing its condition with
  `false` fails `egress_off_refuses_a_cloud_route_before_any_connection_is_opened` and nothing else.
  Measured — see `tasks.md` T5.
- As in specs 004–020, the macOS and Linux legs of `core.yml` are unobserved locally; only the
  Windows leg is run.

## Assumptions

- **A `Cloud` route cannot complete a real HTTPS call, and that is deliberate.** Adding a TLS
  backend would relax spec 012 FR-003 — a structural security property the workspace states is not
  to be relaxed for convenience — and it is a decision that deserves its own slice and its own
  recorded justification. Deferring `ProviderKind::Cloud` until then was considered and rejected:
  it would leave the egress-refusal behaviour (the whole point of ADR-0002 D4's mandate) untestable
  and axis 1b's exit criterion unmet, while the routing, credential and refusal machinery is
  complete and observable today.
- **A `Cloud` route pointed at loopback is legitimate, and the tests rely on it.** `ProviderKind` is
  a declaration (point 1 above). A LiteLLM sidecar on `localhost:4000` fronting a cloud model is a
  real deployment of exactly this shape, so the test configuration is not a contrivance built only
  for testing.
- **`--model` is still required by clap even when `--provider` is given, and is then ignored.**
  `ModelArgs` is flattened into both `skein chat` and `skein acp-agent`; a
  `required_unless_present = "provider"` on `--model` names an argument that does not exist in
  `acp-agent`, which clap rejects. Making `--model` optional would push its absence from a parse
  error to a runtime one for `acp-agent`, which is worse. The precedence is documented in
  `--provider`'s help text. Closing this properly means either flattening `ProviderArgs` into
  `acp-agent` too or splitting `ModelArgs`, and both are decisions about `acp-agent`'s session
  model rather than about routing. Recorded on the next-slice list.
- **`LocalEndpoint`'s `localhost` re-resolution TOCTOU residual is unchanged.** Slice 012 recorded
  it; this slice neither closes nor widens it. The full fix remains ADR-0002 D4's process-level
  socket-deny boundary.
- **Model I/O redaction is still open.** `NativeLoop::run` appends `LlmRequest`/`LlmResponse`
  payloads through a `Redactor` built only from `--redact` references. This slice's "never logs the
  raw key" guarantee is scoped precisely to the credential *it* resolves — that credential is never
  formatted into an error and never placed anywhere but one header. The broader gap stays on slice
  012's backlog, where it belongs to the governed loop.
- **The flat table is a subset of a future config system, not a competitor to it.** If design
  §5.5's layered configuration lands, `[[provider]]` should be a section inside it rather than
  something to reconcile with.
