# Implementation Plan: named-provider routing and the egress policy layer (v0 slice)

**Spec:** `specs/035-model-gateway-routing/spec.md` · **Tasks:** `specs/035-model-gateway-routing/tasks.md`
**Branch:** `feat/model-gateway-multiprovider` · TDD (red→green).

## Constitution Check (ADR-0004 D1 solo-v0 bar)

- **I Headless core** ✅ `skein chat` gains three flags and stays the authoritative client. The
  routing decision lives in `skein-gateway` and is reachable through its public API; the CLI holds
  no policy of its own beyond turning flags into arguments.
- **II Local-first** ✅ NON-NEGOTIABLE and *strengthened*, not relaxed. `ureq` keeps
  `default-features = false`, so no TLS backend is compiled in and the structural guarantee is
  unchanged and re-measured (SC-002). What is added is a **policy** check above the address check:
  a route declared cloud is refused before a socket exists, which is a guarantee the workspace
  previously asserted in prose and could not test. `--allow-egress` defaults to `false`.
- **III Test-First** ✅ every step's outcome is recorded verbatim in `tasks.md` under `## Observed
  red`. T5/T6 had **no** red because the guard was written during T2, and that entry says so rather
  than dressing one up — it carries a **measured mutation** instead, which is a stronger guarantee
  than a red because it keeps working after the fact.
- **IV Inverted coupling** ✅ `skein-core` gains nothing and is unchanged. `skein-gateway` remains
  the only crate naming HTTP, gains one parse-only dependency (`toml`), and does **not** gain a
  dependency on `skein-silo` — the router takes `&dyn SecretProvider` and the tests implement that
  trait directly rather than reusing `skein-silo`'s keychain-backed fixture.
- **V Traceability** ✅ unchanged machinery, unchanged shape. No new `StepKind`, no change to
  `ToolGateway`, `Approval` or `Redactor`. A routed run lands the same five steps on the chain as a
  `--base-url` run, asserted by running `skein ledger verify` inside the CLI test.
- **VI Security** ✅ deny-by-default is structural in three places: `--allow-egress` is off by
  default; the refusal returns `Result<T>` rather than `Result<Option<T>>` so a caller cannot
  downgrade it; and the credential is a `SecretValue` whose one `expose()` call site builds a header
  and binds no local. `deny_unknown_fields` on the config is part of this — a silently-ignored
  `credentials` key yields an unauthenticated request.
- **VII No capability without a real need** ✅ the flat `[[provider]]` table is the whole schema; no
  layering, no includes, no writer (`toml`'s `display` feature is off).
- **VIII Loop discipline** ✅ untouched. The loop, the budget and the controller are not in this
  slice's diff.

## Architecture

```
skein chat --provider <name>
        │
        ▼
wiring::ProviderArgs::client(timeout)            [skein-cli]
        │   None when --provider absent → the pre-existing ModelArgs path, file never read
        ▼
ProviderTable::from_path(--providers-file)       [skein-gateway::route]
        │
        ▼
Router::client_for(name, &LazyKeychain, allow_egress, timeout)
        │
        ├─ 1. table.find(name)              → SkeinError::Model listing configured names
        ├─ 2. EGRESS CHECK                  → SkeinError::Model naming provider + --allow-egress
        ├─ 3. requires_network() check      → SkeinError::Model (ADR-0002 D4)
        ├─ 4. credential resolve            → SecretProvider::resolve → SecretValue
        └─ 5. endpoint parse + construct
                 Local → LocalEndpoint::parse  (loopback guard, unchanged)
                 Cloud → NetworkEndpoint::parse (any host, http|https)
        │
        ▼
OpenAiCompatClient { endpoint: Endpoint, bearer_token: Option<SecretValue>, … }
        │
        └─ post() sets `authorization: Bearer …` only when present — the one expose() call site
```

### Key decisions

**`Endpoint` as a private enum inside `OpenAiCompatClient`, not two client types.** The protocol is
identical between a local Ollama, a LiteLLM sidecar and a cloud gateway; only the address rules and
the presence of a credential differ. Two clients would duplicate the entire chat-completions
translation to express a difference of two fields. The enum keeps each address's *proof* — the thing
worth not losing — while sharing everything that is genuinely the same.

**`NetworkEndpoint` as a separate public type, not a flag on `LocalEndpoint`.** `LocalEndpoint`
carries the guarantee "this address is this machine". A boolean that could switch that off would
make the guarantee reviewable rather than structural, and every existing holder of a `LocalEndpoint`
would have to be re-checked. A distinct type means nothing that holds a `LocalEndpoint` can be
handed an off-machine address by mistake.

**A private `RawRoute` for deserialization, not `#[derive(Deserialize)]` on `ProviderRoute`.** Two
reasons. `SecretRef` does not implement `Deserialize` and should not — a type a config file can
deserialize *into* a credential is a shape worth not having. And the public route is the product's
vocabulary while `RawRoute` is one file format's; letting the format dictate the public type is how
a schema change becomes an API change.

**`LazyKeychain` in `wiring.rs`, opening `OsKeychain` on first `resolve()`.** An eagerly-built
keychain would be opened for every named provider, including the common local one with no credential
and including a cloud one about to be refused for egress — `client_for` checks egress before it
resolves anything, so an eager build would open the store *for a run that is being refused*. The
laziness is `RedactArgs::redactor`'s existing rule ("a run that configures no secret must not
acquire a runtime credential-store dependency") applied one layer down.

**Hand-written `Debug` on `OpenAiCompatClient`.** Not because a derived one would leak the token —
`SecretValue`'s own formatter prevents that — but because a derived one prints `ureq::Agent`'s
entire connector and timeout configuration, burying the fields a reader of a failure message wants.
This was discovered by reading an actual test failure during T3. "Print only what you meant to
print" is the rule; the credential is what makes it non-negotiable rather than merely tidy.

**`provider_noun()` on `Endpoint`.** The unreachable-provider message says "is **a local provider**
listening at …" for a `Local` endpoint and "is a provider listening at …" for a network one.
Widening it to cover both was the first thing tried, and it broke an existing spec-012 test — for a
good reason: "local" is the word that sends someone to check whether Ollama is running, and it is
the message's only actionable content.

## Files

| File | Action | Lines |
|------|--------|-------|
| `crates/skein-gateway/src/route.rs` | CREATE | +294 |
| `crates/skein-gateway/tests/provider_routing.rs` | CREATE | +560 |
| `crates/skein-gateway/src/lib.rs` | UPDATE | +138/−25 |
| `crates/skein-gateway/Cargo.toml` | UPDATE | +3 |
| `Cargo.toml` (workspace) | UPDATE | +9 |
| `crates/skein-cli/src/wiring.rs` | UPDATE | +103 |
| `crates/skein-cli/src/chat.rs` | UPDATE | +24/−6 |
| `crates/skein-cli/src/main.rs` | UPDATE | +6 |
| `crates/skein-cli/tests/cli_chat.rs` | UPDATE | +291 |
| `specs/035-model-gateway-routing/{spec,plan,tasks}.md` | CREATE | — |
| `README.md` | UPDATE | Current status |

**Unchanged, and asserted so:** `crates/skein-core`, `crates/skein-mcp`, `crates/skein-acp`,
`crates/skein-silo`, `crates/skein-sandbox`, `crates/skein-connectors`.

## Risks

| Risk | Outcome |
|------|---------|
| `toml` pulls a transitive TLS crate, silently breaking spec 012 SC-007 | **Resolved by measurement.** `cargo tree -e normal -p skein-gateway` matches 0 of `rustls\|native-tls\|webpki\|openssl` after the addition. `toml` adds five parse-only packages. The fallback (hand-rolling the flat parser) was not needed. |
| A reader mistakes the `Cloud`-route-on-loopback tests for a bug | Mitigated in three places: the test file's module docstring, `ProviderKind::Cloud`'s docstring, and spec.md point 1 / Assumptions. |
| `--provider` and `--base-url` both given: ambiguous precedence | `--provider` wins, no merge, documented in `--provider`'s `--help`. Not enforced by clap `conflicts_with`, because the two flags live in different `Args` structs and only one is flattened into `acp-agent`. |
| `--model` still required when `--provider` is given | Accepted for v0 and recorded in spec.md Assumptions and on the next-slice list. The clap-level fix names an argument `acp-agent` does not have. |
| Scope creep toward a `ModeSupervisor` | One `bool`, one flag. No `Mode` type exists in the diff. |
| `SkeinError::Model(String)` makes refusal reasons distinguishable only by message text | Accepted, matching spec 012's established trade-off for every gateway failure. Tests assert on message content, as the existing ones do. A struct variant is the right change *when a caller needs to match on the reason*, not before. |
