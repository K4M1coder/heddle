//! The transport half of the Atlassian connector: the proved site address, the
//! one place the credential is read, and the two request shapes Jira and
//! Confluence are reached through.
//!
//! Nothing here decides policy. The egress gate is
//! [`crate::atlassian::AtlassianServer::connect`], above this, for
//! `Router::client_for`'s recorded reason: a refusal must happen before an
//! address is parsed or a credential is resolved, and a type that can only be
//! built after those steps cannot express one.

use heddle_core::{HeddleError, Result, SecretValue};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Connect budget, separate from the whole-request one so a wrong port fails
/// fast — `heddle-gateway`'s reasoning, and its number.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Whole-request budget for one REST call.
///
/// A constant rather than a flag, where `heddle-gateway` makes its timeout the
/// caller's: a generation's duration is a property of the model and the prompt,
/// and an operator has to be able to raise it. A tracker read is not — a site
/// that has not answered a page fetch in thirty seconds is not going to, and a
/// knob here would only be a knob nobody could set correctly.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of a site's error body reaches the operator. Enough for Jira's
/// `{"errorMessages":[…]}`, short of pasting a whole HTML error page into a
/// model's context and onto the chain.
const ERROR_BODY_CHARS: usize = 400;

/// A base URL that has been proved to name a site rather than a typo.
///
/// The proof happens at construction and not at request time, exactly as
/// `LocalEndpoint`'s does: an address that cannot be built is an address no
/// socket was ever opened to. Unlike `LocalEndpoint` this places **no**
/// restriction on where the site is — there is no local Jira, so a network
/// connector's address is never the thing that makes it egress. Its
/// *existence* is, which is why the gate is a construction-order decision one
/// layer up rather than an address rule here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlassianSite {
    base_url: String,
}

impl AtlassianSite {
    /// Accepts `http[s]://<host>[:port]`, with or without a trailing slash, and
    /// nothing else.
    ///
    /// A path is refused rather than kept: every endpoint below is built by
    /// appending an absolute path Atlassian documents, so a base URL carrying
    /// one of its own would produce `/wiki/wiki/api/v2/…` — a 404 that reads
    /// like a bug in this crate.
    ///
    /// **`https://` is accepted here and unreachable at the transport.** No TLS
    /// backend is compiled in (this crate's `Cargo.toml` records why it must
    /// stay that way while `heddle-gateway` shares the dependency), so a real
    /// `https://acme.atlassian.net` fails with `ureq::Error::TlsRequired` on the
    /// first call. Refusing the scheme *here* instead would be a worse trade:
    /// the message an operator needs names TLS, and it is spec 012 FR-003's
    /// precedent for the identical situation.
    pub fn parse(base_url: &str) -> Result<AtlassianSite> {
        let refuse = |why: &str| -> HeddleError {
            HeddleError::Tool(format!("the Atlassian site {base_url:?} is refused: {why}"))
        };
        let uri: http::Uri = base_url
            .parse()
            .map_err(|e| refuse(&format!("it is not a URL ({e})")))?;
        let scheme = uri
            .scheme_str()
            .ok_or_else(|| refuse("it names no scheme; write http:// or https://"))?;
        if scheme != "http" && scheme != "https" {
            return Err(refuse(&format!(
                "{scheme}:// is not a scheme a site is reached over; write http:// or https://"
            )));
        }
        let authority = uri
            .authority()
            .ok_or_else(|| refuse("it names no host"))?
            .as_str();
        if !matches!(uri.path(), "" | "/") {
            return Err(refuse(
                "it carries a path; name only the site, because every endpoint's path is \
                 appended to it",
            ));
        }
        if uri.query().is_some() {
            return Err(refuse("it carries a query string"));
        }
        Ok(AtlassianSite {
            base_url: format!("{scheme}://{authority}"),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// The wire: an agent, the site it is pointed at, and the credential every
/// request is authenticated with.
///
/// Behind an [`Arc`] in [`crate::AtlassianServer`] because rmcp hands each
/// request a clone of the handler, and every clone must read the *same*
/// credential rather than a copy of it.
pub struct Wire {
    site: AtlassianSite,
    /// Not a secret: an Atlassian account's email is on every issue it has ever
    /// touched. It is a config field rather than half of one because Jira
    /// Cloud's API-token scheme is HTTP Basic over `email:token`, so the pair is
    /// what authenticates and neither half alone does.
    email: String,
    /// Resolved once, at construction, and held as a [`SecretValue`] the whole
    /// way: zeroized on drop, and rendered `SecretValue(***)` by any formatter
    /// that reaches it. Read in exactly one place, [`Wire::authorization`].
    credential: SecretValue,
    agent: ureq::Agent,
}

/// Written by hand rather than derived, for `OpenAiCompatClient`'s recorded
/// reason: a derived one prints `ureq::Agent`'s whole connector configuration
/// and buries the two fields a reader of a failure actually wants.
impl std::fmt::Debug for Wire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wire")
            .field("site", &self.site)
            .field("email", &self.email)
            .field("credential", &self.credential)
            .finish_non_exhaustive()
    }
}

impl Wire {
    /// Builds no socket. `ureq` connects lazily, which is what lets the egress
    /// gate above this sit before construction with no ordering subtlety.
    ///
    /// `http_status_as_error(false)` is what lets a site's own error body reach
    /// the operator instead of being flattened into a status code.
    pub fn new(site: AtlassianSite, email: impl Into<String>, credential: SecretValue) -> Wire {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .into();
        Wire {
            site,
            email: email.into(),
            credential,
            agent,
        }
    }

    /// The one place `expose()` is called.
    ///
    /// The value goes straight into a header value and is never bound to a
    /// local, so there is no variable a later `format!` in this file could pick
    /// up by accident — `OpenAiCompatClient::send`'s discipline, and the reason
    /// it is worth repeating rather than abbreviating.
    fn authorization(&self) -> String {
        format!(
            "Basic {}",
            base64(format!("{}:{}", self.email, self.credential.expose()).as_bytes())
        )
    }

    pub fn get(&self, path: &str) -> std::result::Result<Value, String> {
        let response = self
            .agent
            .get(self.url(path))
            .header("accept", "application/json")
            .header("authorization", self.authorization())
            .call()
            .map_err(|e| self.scrub(format!("GET {path} failed: {e}")))?;
        self.answer("GET", path, response)
    }

    pub fn post(&self, path: &str, body: &Value) -> std::result::Result<Value, String> {
        let response = self
            .agent
            .post(self.url(path))
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .header("authorization", self.authorization())
            .send(body.to_string())
            .map_err(|e| self.scrub(format!("POST {path} failed: {e}")))?;
        self.answer("POST", path, response)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.site.base_url())
    }

    /// Reads the whole answer and decides what it was.
    ///
    /// A non-2xx is an `Err` carrying the status **and** a clipped copy of the
    /// site's own body, because "PROJ-9 does not exist" and "you may not comment
    /// on this issue" are both 403s and only the body tells them apart. Every
    /// string that leaves this function goes through [`Wire::scrub`] first, so
    /// a site that echoes the credential back — which is exactly what a
    /// rejected-token response is most tempting to do — cannot put it on the
    /// chain or in front of a model.
    fn answer(
        &self,
        method: &str,
        path: &str,
        mut response: ureq::http::Response<ureq::Body>,
    ) -> std::result::Result<Value, String> {
        let status = response.status().as_u16();
        let raw = response
            .body_mut()
            .read_to_string()
            .map_err(|e| self.scrub(format!("{method} {path} returned an unreadable body: {e}")))?;
        if !(200..300).contains(&status) {
            return Err(self.scrub(format!("{method} {path} returned {status}: {}", clip(&raw))));
        }
        serde_json::from_str(&raw).map_err(|e| {
            self.scrub(format!(
                "{method} {path} returned a body that is not JSON: {e}"
            ))
        })
    }

    /// The connector's own floor under Constitution V, independent of whether
    /// the operator remembered `--redact`.
    ///
    /// `Redactor` scrubs what an operator *named*; this scrubs what this
    /// connector *knows*, which is the one secret it was handed. Both are worth
    /// having: the Ledger's copy is the Redactor's job and the escaping subtlety
    /// spec 029 records is its own, while this one covers the path no redactor
    /// sees at all — a message returned straight to the model.
    fn scrub(&self, text: String) -> String {
        let secret = self.credential.expose();
        if secret.is_empty() || !text.contains(secret) {
            return text;
        }
        text.replace(secret, "***")
    }
}

/// Enough of a body to diagnose with, and not a whole page.
fn clip(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.char_indices().nth(ERROR_BODY_CHARS) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed.to_string(),
    }
}

/// Standard base64, no line breaks: the encoding `Authorization: Basic`
/// requires (RFC 7617).
///
/// Twenty lines rather than a dependency, and the trade is this workspace's
/// recorded one — `toml`, `git2` and `cap-std` each carry a comment explaining
/// why they earn their place in the tree. A base64 alphabet does not: it is
/// fully specified, it will not change, and the alternative is a crate on the
/// product's dependency graph for one header value.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let packed = u32::from(chunk[0]) << 16
            | u32::from(chunk.get(1).copied().unwrap_or(0)) << 8
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for slot in 0..4 {
            if slot <= chunk.len() {
                out.push(ALPHABET[((packed >> (18 - 6 * slot)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Percent-encodes one query-string value: everything outside RFC 3986's
/// unreserved set, which is what a JQL expression is full of.
///
/// Hand-written for [`base64`]'s reason, and narrower than a general URL
/// encoder on purpose — it encodes `/`, `&`, `=` and `+` too, because the only
/// caller is building a value and never a delimiter.
pub fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// An identifier a model supplied, on its way into a URL path.
///
/// The only model-chosen values this connector ever puts in a path are issue
/// keys and page ids, and both are `[A-Za-z0-9_-]` by Atlassian's own rules. So
/// the check is an allowlist rather than an escape: there is no encoding of `/`
/// or `..` that this accepts and then has to get right later, which is
/// `RunParams`'s "a vector and never a command line" applied to a URL.
pub fn path_segment(what: &str, value: &str) -> std::result::Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("name the {what}; it was empty"));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "{what} {value:?} is not one: letters, digits, `-` and `_` only"
        ));
    }
    Ok(value.to_string())
}

/// A required text argument, refused by name when it is missing.
///
/// Named rather than merely "invalid", following `fs_read`'s cap refusal: the
/// message a model can act on is the one that says which argument and what was
/// wrong with it.
pub fn required(what: &str, value: &str) -> std::result::Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("name the {what}; it was empty"));
    }
    Ok(value.to_string())
}

/// Shared by both product code and the `Arc` the handler clones hold.
pub type SharedWire = Arc<Wire>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648_test_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_query_value_encodes_every_reserved_character() {
        assert_eq!(
            encode_query_value("project = PROJ AND status != Done"),
            "project%20%3D%20PROJ%20AND%20status%20%21%3D%20Done"
        );
        assert_eq!(encode_query_value("a&b=c/d+e"), "a%26b%3Dc%2Fd%2Be");
        assert_eq!(encode_query_value("safe-_.~"), "safe-_.~");
    }

    #[test]
    fn a_site_keeps_only_its_scheme_and_authority() {
        assert_eq!(
            AtlassianSite::parse("https://acme.atlassian.net/")
                .expect("a well-formed site")
                .base_url(),
            "https://acme.atlassian.net"
        );
        assert_eq!(
            AtlassianSite::parse("http://127.0.0.1:8080")
                .expect("a well-formed site")
                .base_url(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn a_site_with_a_path_or_no_scheme_is_refused() {
        for bad in [
            "acme.atlassian.net",
            "https://acme.atlassian.net/wiki",
            "ftp://acme.atlassian.net",
            "https://acme.atlassian.net?a=1",
        ] {
            let err = AtlassianSite::parse(bad).expect_err("refused");
            assert!(
                err.to_string().contains(bad),
                "the refusal names what was typed, got: {err}"
            );
        }
    }

    #[test]
    fn a_path_segment_refuses_traversal_and_separators() {
        assert_eq!(
            path_segment("issue key", " PROJ-1 ").expect("a key"),
            "PROJ-1"
        );
        for bad in ["", "..", "PROJ-1/../..", "PROJ 1", "PROJ-1?x=1"] {
            path_segment("issue key", bad).expect_err("refused");
        }
    }
}
