//! Named-provider routing (design §4.5, axis 1b): the layer that turns
//! "local-ollama" into a [`OpenAiCompatClient`] pointed somewhere, and that
//! refuses to point it off this machine when egress is off.
//!
//! Two things live here that [`LocalEndpoint`] alone could not express:
//!
//! - **A provider is a named thing, not a URL.** An operator writes the address,
//!   the model and the credential down once, and every later run says which one.
//! - **A route declares its own kind.** [`ProviderKind::Cloud`] is what the
//!   operator *said*, not what the address looks like, and it is the declaration
//!   the egress policy acts on (ADR-0002 D4). Inferring it from the address
//!   would make the policy a property of DNS.
//!
//! What this is **not**: a general configuration system. There is no
//! `[team]`/`[project]`/`[conversation]` layering (design §5.5) — one flat
//! `[[provider]]` table, which a richer system would later contain rather than
//! compete with.

use crate::{LocalEndpoint, OpenAiCompatClient};
use heddle_core::{HeddleError, Result, SecretProvider, SecretRef};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

/// What the operator declared a provider to be.
///
/// `#[serde(rename_all = "lowercase")]` so the config file reads
/// `kind = "local"` rather than `kind = "Local"` — the TOML is written by a
/// person, and Rust's capitalisation is not their concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// On this machine. Keeps [`LocalEndpoint`]'s loopback guard exactly as it
    /// is: declaring a route `Local` narrows what it may address, never widens
    /// it.
    Local,
    /// Off this machine. Reachable only with egress allowed, and even then only
    /// as far as the transport goes — no TLS backend is compiled in, so a real
    /// `https://` provider still fails with `ureq::Error::TlsRequired`. That is
    /// spec 012 FR-003/SC-007 holding, not a defect of this slice.
    Cloud,
}

/// A base URL for a provider that is **not** on this machine.
///
/// [`LocalEndpoint`]'s twin, and deliberately a separate type rather than a
/// flag on it: the guarantee `LocalEndpoint` carries — "this address is this
/// machine" — is one no boolean should be able to switch off. A `Cloud` route
/// gets a different type, so nothing that holds a `LocalEndpoint` can be handed
/// an off-machine address by mistake.
///
/// The refusals are `LocalEndpoint::parse`'s, minus exactly one branch: the
/// address is not required to be loopback. "Not a URL at all" and "no scheme"
/// stay identical, and `https://` is *accepted here and refused at the
/// transport* — no TLS backend is compiled in (spec 012 FR-003/SC-007), so a
/// real cloud provider still fails with `ureq::Error::TlsRequired`. Accepting
/// the scheme here rather than refusing it early is the honest split: the URL
/// is well-formed and the policy permits it; what the build cannot do is speak
/// TLS, and that is the error the operator should see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkEndpoint {
    base_url: String,
}

impl NetworkEndpoint {
    pub fn parse(base_url: &str) -> Result<NetworkEndpoint> {
        let refuse = |why: String| -> HeddleError {
            HeddleError::Model(format!(
                "base URL {base_url:?} is not a usable provider address: {why}"
            ))
        };

        let uri: http::Uri = base_url
            .parse()
            .map_err(|e| refuse(format!("not a URL: {e}")))?;

        match uri.scheme_str() {
            Some("http") | Some("https") => {}
            Some(scheme) => {
                return Err(refuse(format!(
                    "scheme {scheme:?} is refused; a model provider is addressed over http or                      https"
                )))
            }
            None => return Err(refuse("no scheme; expected https://… or http://…".into())),
        }

        if uri.host().is_none() {
            return Err(refuse("no host".into()));
        }

        Ok(NetworkEndpoint {
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// The base, with any trailing slash removed, exactly as
    /// [`LocalEndpoint::base_url`] reports one.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// One provider, as an operator wrote it down.
///
/// Public fields, deliberately: this is a configuration record with no
/// invariant of its own to protect. Every invariant that matters — is the
/// address addressable, is egress allowed, does the credential resolve — is
/// [`Router::client_for`]'s, checked at the moment a client is built and not
/// before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRoute {
    /// What `--provider` names.
    pub name: String,
    pub kind: ProviderKind,
    /// OpenAI-compatible base URL, validated per [`ProviderKind`] and not here.
    pub base_url: String,
    /// The model as this provider knows it. Carried by the route so switching
    /// provider switches model too — an operator who has named the pair once
    /// does not retype either.
    pub model: String,
    /// A reference, never a value (Constitution VI). `None` is not an error: a
    /// provider that needs no authentication is an ordinary provider.
    pub credential: Option<SecretRef>,
}

/// Every provider this run may name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderTable {
    routes: Vec<ProviderRoute>,
}

impl ProviderTable {
    pub fn new(routes: Vec<ProviderRoute>) -> ProviderTable {
        ProviderTable { routes }
    }

    /// The route by name, or a refusal that lists what *is* configured.
    ///
    /// Listing the alternatives is the point: the overwhelmingly common failure
    /// is a typo or a stale name, and an operator who is told only "unknown
    /// provider" has to go and read the file to find out what they meant.
    pub fn find(&self, name: &str) -> Result<&ProviderRoute> {
        self.routes
            .iter()
            .find(|route| route.name == name)
            .ok_or_else(|| {
                let configured = self
                    .routes
                    .iter()
                    .map(|route| route.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                HeddleError::Model(format!(
                    "no provider named {name:?} is configured; configured providers are: {}",
                    if configured.is_empty() {
                        "none".to_string()
                    } else {
                        configured
                    }
                ))
            })
    }

    pub fn routes(&self) -> &[ProviderRoute] {
        &self.routes
    }

    /// Reads the operator's provider table from a file.
    ///
    /// Every failure comes back as `HeddleError::Model` naming the path,
    /// including the io ones: an operator who mistyped `--providers-file` needs
    /// to see which path was tried, and a bare `io::Error` says "the system
    /// cannot find the file specified" without saying which file.
    pub fn from_path(path: &Path) -> Result<ProviderTable> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            HeddleError::Model(format!(
                "could not read the provider table at {}: {e}",
                path.display()
            ))
        })?;
        ProviderTable::from_toml_str(&text)
            .map_err(|e| HeddleError::Model(format!("{}: {e}", path.display())))
    }

    /// Parses a flat `[[provider]]` table.
    ///
    /// Deliberately the whole schema. There is no `[team]`/`[project]`/
    /// `[conversation]` layering (design §5.5, ADR-0002 D3) and no include
    /// mechanism — a richer configuration system, if one lands, should be able
    /// to *contain* this table rather than having to reconcile with it.
    pub fn from_toml_str(text: &str) -> Result<ProviderTable> {
        let raw: RawTable = toml::from_str(text)
            .map_err(|e| HeddleError::Model(format!("the provider table is not valid: {e}")))?;

        let mut seen = BTreeSet::new();
        for route in &raw.provider {
            if !seen.insert(route.name.as_str()) {
                return Err(HeddleError::Model(format!(
                    "two providers are named {:?}; a name selects exactly one provider, so the \
                     second would be unreachable and edits to it would appear to do nothing",
                    route.name
                )));
            }
        }

        Ok(ProviderTable::new(
            raw.provider.into_iter().map(ProviderRoute::from).collect(),
        ))
    }
}

/// The file's shape, kept private and separate from [`ProviderRoute`].
///
/// Two reasons it is not a `Deserialize` derive on the public type. `SecretRef`
/// does not implement `Deserialize` and should not — a config file naming a
/// secret is fine, a config file that could deserialize *into* a credential
/// type is a shape worth not having. And the public route is the product's
/// vocabulary while this is one file format's; letting the format dictate the
/// type is how a schema change becomes an API change.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTable {
    /// Absent in an empty file, which is a table with no providers rather than
    /// a malformed one.
    #[serde(default)]
    provider: Vec<RawRoute>,
}

/// `deny_unknown_fields` is the load-bearing attribute here, not decoration.
/// The realistic typo is `credentials` for `credential`, and serde's default —
/// ignore it — yields a cloud provider that silently sends no `Authorization`
/// header at all. That is a failure an operator debugs at the provider, having
/// been given no reason to suspect their own file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoute {
    name: String,
    kind: ProviderKind,
    base_url: String,
    model: String,
    /// A reference string, turned into a [`SecretRef`] here and never into a
    /// value: nothing in this file is ever a credential (Constitution VI).
    #[serde(default)]
    credential: Option<String>,
}

impl From<RawRoute> for ProviderRoute {
    fn from(raw: RawRoute) -> ProviderRoute {
        ProviderRoute {
            name: raw.name,
            kind: raw.kind,
            base_url: raw.base_url,
            model: raw.model,
            credential: raw.credential.map(SecretRef),
        }
    }
}

/// Turns a provider's *name* into a client, under this run's egress policy.
///
/// Borrows its table rather than owning one, so the caller keeps a single
/// parsed configuration and the router stays a thin decision — there is nothing
/// here to keep in sync, cache or invalidate.
pub struct Router<'a> {
    table: &'a ProviderTable,
}

impl<'a> Router<'a> {
    pub fn new(table: &'a ProviderTable) -> Router<'a> {
        Router { table }
    }

    /// The one path from a provider name to a client.
    ///
    /// The return type is `Result<OpenAiCompatClient>` and not
    /// `Result<Option<…>>` or a client carrying a "disabled" flag, because a
    /// refused route must be **unrepresentable as a client**: an `Option` a
    /// caller could `unwrap_or_else(build_it_anyway)` would move the egress
    /// decision out of here and into every call site.
    ///
    /// The order of the checks is the guarantee:
    ///
    /// 1. find the route — an unknown name is not an egress question;
    /// 2. **egress**, before anything else is built or resolved;
    /// 3. parse the address for the declared kind;
    /// 4. resolve the credential, if there is one;
    /// 5. build the client.
    ///
    /// Step 2 sits above step 4 even though neither opens a socket, because a
    /// future `SecretProvider` backend may reach a network to answer, and "no
    /// connection was attempted" must stay true of the whole call and not only
    /// of the model request.
    pub fn client_for(
        &self,
        name: &str,
        secrets: &dyn SecretProvider,
        egress_allowed: bool,
        timeout: Duration,
    ) -> Result<OpenAiCompatClient> {
        let route = self.table.find(name)?;

        let refuse = |why: String| -> HeddleError {
            HeddleError::Model(format!(
                "provider {:?} is refused: {why}",
                route.name.as_str()
            ))
        };

        if route.kind == ProviderKind::Cloud && !egress_allowed {
            return Err(refuse(
                "it is declared a cloud provider and egress is off; pass --allow-egress to \
                 permit this run to leave the machine, or name a local provider"
                    .into(),
            ));
        }

        let credential = match &route.credential {
            // ADR-0002 D4, and the first place in this workspace to read
            // `requires_network()`: with egress off, a credential store that
            // must leave this machine to answer is itself egress — whatever the
            // route's own kind is. A local provider whose key lives in a
            // cloud-hosted vault is exactly that case, and it is why this check
            // is not folded into the `Cloud` branch above.
            Some(_) if !egress_allowed && secrets.requires_network() => {
                return Err(refuse(
                    "its credential lives in a store that must reach a network to answer, and                      egress is off"
                        .into(),
                ))
            }
            Some(reference) => Some(secrets.resolve(reference)?),
            None => None,
        };

        let client = match route.kind {
            ProviderKind::Local => OpenAiCompatClient::new(
                LocalEndpoint::parse(&route.base_url)?,
                &route.model,
                timeout,
            ),
            ProviderKind::Cloud => OpenAiCompatClient::networked(
                NetworkEndpoint::parse(&route.base_url)?,
                &route.model,
                timeout,
            ),
        };

        Ok(match credential {
            Some(token) => client.with_bearer_token(token),
            None => client,
        })
    }
}
