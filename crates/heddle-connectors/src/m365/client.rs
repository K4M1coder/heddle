//! The transport half of the Microsoft 365 connector: the proved Graph
//! address, the one place the access token is read, and the three request
//! shapes Outlook, SharePoint and Teams are reached through.
//!
//! Nothing here decides policy. The egress gate is
//! [`crate::m365::M365Server::connect`], above this, for the reason
//! `atlassian::client` records and `Router::client_for` records before it: a
//! refusal must happen before an address is parsed or a credential is resolved,
//! and a type that can only be built after those steps cannot express one.

use heddle_core::{HeddleError, Result, SecretValue};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Connect budget, separate from the whole-request one so a wrong port fails
/// fast — `heddle-gateway`'s reasoning, and its number.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Whole-request budget for one Graph call. A constant and not a flag, for the
/// reason `atlassian::client` records: a mailbox read that has not answered in
/// thirty seconds is not going to, and a knob here would only be one nobody
/// could set correctly.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of Graph's error body reaches the operator. Enough for
/// `{"error":{"code":…,"message":…}}`, short of pasting a whole HTML error page
/// into a model's context and onto the chain.
const ERROR_BODY_CHARS: usize = 400;

/// A base URL that has been proved to name a Graph endpoint rather than a typo.
///
/// The proof happens at construction and not at request time, exactly as
/// [`crate::AtlassianServer`]'s `AtlassianSite` does: an address that cannot be
/// built is an address no socket was ever opened to. Like that one it places
/// **no** restriction on where the endpoint is — there is no local Microsoft
/// Graph, so a network connector's address is never the thing that makes it
/// egress. Its *existence* is, which is why the gate is a construction-order
/// decision one layer up rather than an address rule here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSite {
    base_url: String,
}

impl GraphSite {
    /// Accepts `http[s]://<host>[:port][/<version prefix>]`, with or without a
    /// trailing slash, and nothing else.
    ///
    /// **A path is kept here, where `AtlassianSite::parse` refuses one**, and
    /// that is the one deliberate difference. Atlassian's rule is right for
    /// Atlassian: `/wiki` and `/rest` are prefixes *that* connector appends, so
    /// a base URL carrying one produces `/wiki/wiki/api/v2/…`. Graph's
    /// documented base URL *is* `https://graph.microsoft.com/v1.0` — the
    /// version is part of the address an operator is given, not part of an
    /// endpoint this crate knows — and hard-coding `/v1.0` into every path
    /// below would make `/beta` unreachable without a code change. So the
    /// operator names the whole prefix and every endpoint below is written
    /// relative to it.
    ///
    /// A query string is still refused, and so is a fragment: neither can be a
    /// base URL's business when every endpoint appends its own.
    ///
    /// **`https://` is accepted here and unreachable at the transport.** No TLS
    /// backend is compiled in (this crate's `Cargo.toml` records why it must
    /// stay that way while `heddle-gateway` shares the dependency), so a real
    /// `https://graph.microsoft.com` fails with `ureq::Error::TlsRequired` on
    /// the first call. Refusing the scheme *here* instead would be a worse
    /// trade: the message an operator needs names TLS, and it is spec 012
    /// FR-003's precedent for the identical situation.
    pub fn parse(base_url: &str) -> Result<GraphSite> {
        let refuse = |why: &str| -> HeddleError {
            HeddleError::Tool(format!(
                "the Microsoft Graph endpoint {base_url:?} is refused: {why}"
            ))
        };
        let uri: http::Uri = base_url
            .parse()
            .map_err(|e| refuse(&format!("it is not a URL ({e})")))?;
        let scheme = uri
            .scheme_str()
            .ok_or_else(|| refuse("it names no scheme; write http:// or https://"))?;
        if scheme != "http" && scheme != "https" {
            return Err(refuse(&format!(
                "{scheme}:// is not a scheme an endpoint is reached over; write http:// or \
                 https://"
            )));
        }
        let authority = uri
            .authority()
            .ok_or_else(|| refuse("it names no host"))?
            .as_str();
        if uri.query().is_some() {
            return Err(refuse("it carries a query string"));
        }
        if base_url.contains('#') {
            return Err(refuse("it carries a fragment"));
        }
        // Kept, minus any trailing slash, so joining an endpoint's absolute
        // path onto it can never produce a doubled separator.
        let prefix = uri.path().trim_end_matches('/');
        Ok(GraphSite {
            base_url: format!("{scheme}://{authority}{prefix}"),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// The wire: an agent, the endpoint it is pointed at, and the access token
/// every request is authenticated with.
///
/// One field fewer than the Atlassian connector's wire of the same name, and
/// that absence is the whole of the auth difference: Graph authenticates with a
/// single opaque Bearer token, so there is no account email to pair it with.
///
/// Behind an [`Arc`] in [`crate::M365Server`] because rmcp hands each request a
/// clone of the handler, and every clone must read the *same* credential rather
/// than a copy of it.
pub struct Wire {
    site: GraphSite,
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
            .field("credential", &self.credential)
            .finish_non_exhaustive()
    }
}

impl Wire {
    /// Builds no socket. `ureq` connects lazily, which is what lets the egress
    /// gate above this sit before construction with no ordering subtlety.
    ///
    /// `http_status_as_error(false)` is what lets Graph's own error body reach
    /// the operator instead of being flattened into a status code.
    pub fn new(site: GraphSite, credential: SecretValue) -> Wire {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .into();
        Wire {
            site,
            credential,
            agent,
        }
    }

    /// The one place `expose()` is called.
    ///
    /// The value goes straight into a header value and is never bound to a
    /// local, so there is no variable a later `format!` in this file could pick
    /// up by accident — `heddle_gateway`'s `with_bearer_token` discipline, and
    /// the reason it is worth repeating rather than abbreviating.
    fn authorization(&self) -> String {
        format!("Bearer {}", self.credential.expose())
    }

    pub fn get(&self, path: &str) -> std::result::Result<Value, String> {
        let (status, raw) = self.fetch("GET", path)?;
        self.decode("GET", path, status, raw)
    }

    /// A `GET` whose answer is **not** parsed as JSON.
    ///
    /// Graph's `/content` endpoint hands back a file's own bytes, which promise
    /// nothing about their shape; forcing them through [`Wire::get`] would turn
    /// every non-JSON file into a spurious parse error. The status check and the
    /// scrubbing are shared with [`Wire::get`] rather than repeated — the
    /// non-2xx branch lives once, in [`Wire::fetch`].
    pub fn get_text(&self, path: &str) -> std::result::Result<String, String> {
        let (status, raw) = self.fetch("GET", path)?;
        self.refuse_non_2xx("GET", path, status, &raw)?;
        Ok(raw)
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
        let (status, raw) = self.read_body("POST", path, response)?;
        self.decode("POST", path, status, raw)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.site.base_url())
    }

    fn fetch(&self, method: &str, path: &str) -> std::result::Result<(u16, String), String> {
        let response = self
            .agent
            .get(self.url(path))
            .header("accept", "application/json")
            .header("authorization", self.authorization())
            .call()
            .map_err(|e| self.scrub(format!("{method} {path} failed: {e}")))?;
        self.read_body(method, path, response)
    }

    /// Reads the whole answer, and nothing more: the status and the body text.
    ///
    /// Split out of the Atlassian connector's single `answer` because two
    /// callers need the body without the JSON step — a file's content, and the
    /// empty `202 Accepted` Graph answers `sendMail` with.
    fn read_body(
        &self,
        method: &str,
        path: &str,
        mut response: ureq::http::Response<ureq::Body>,
    ) -> std::result::Result<(u16, String), String> {
        let status = response.status().as_u16();
        let raw = response
            .body_mut()
            .read_to_string()
            .map_err(|e| self.scrub(format!("{method} {path} returned an unreadable body: {e}")))?;
        Ok((status, raw))
    }

    /// A non-2xx is an `Err` carrying the status **and** a clipped copy of
    /// Graph's own body, because "that message does not exist" and "you may not
    /// read this channel" are both 403s and only the body tells them apart.
    /// Every string that leaves here goes through [`Wire::scrub`] first, so an
    /// endpoint that echoes the token back — which is exactly what a
    /// rejected-token response is most tempting to do — cannot put it on the
    /// chain or in front of a model.
    fn refuse_non_2xx(
        &self,
        method: &str,
        path: &str,
        status: u16,
        raw: &str,
    ) -> std::result::Result<(), String> {
        if (200..300).contains(&status) {
            return Ok(());
        }
        Err(self.scrub(format!("{method} {path} returned {status}: {}", clip(raw))))
    }

    /// The JSON half: refuse a non-2xx, tolerate an empty 2xx, parse the rest.
    ///
    /// The empty case is Graph's and not Atlassian's: `POST /me/sendMail`
    /// answers `202 Accepted` with no content at all, and
    /// `serde_json::from_str("")` fails — so a verbatim copy of the Atlassian
    /// wire would report a *successful* send as a tool error. A caller that
    /// receives [`Value::Null`] must not read a field out of it; see
    /// `outlook_send_mail`, which returns a fixed string instead.
    fn decode(
        &self,
        method: &str,
        path: &str,
        status: u16,
        raw: String,
    ) -> std::result::Result<Value, String> {
        self.refuse_non_2xx(method, path, status, &raw)?;
        if raw.trim().is_empty() {
            return Ok(Value::Null);
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

/// Percent-encodes one path segment: everything outside RFC 3986's unreserved
/// set, so a Graph id containing `:`, `@`, `!` or `=` still addresses exactly
/// one segment.
///
/// **The deliberate difference from `atlassian::client::path_segment`**, which
/// refuses anything outside `[A-Za-z0-9_-]`: that allowlist is right for Jira,
/// whose keys and page ids are that by Atlassian's own rules, and wrong here,
/// because a real Teams channel id looks like `19:abc0123@thread.tacv2` and a
/// message id is base64url-ish. Refusing those would make the Teams tools
/// unusable against real ids. So this encodes rather than allowlists — the same
/// encode-don't-refuse strategy `encode_query_value` already uses one directory
/// over, applied to a path segment instead of a query value. The safety
/// property is unchanged: `/` and `..` come back percent-encoded, so a model's
/// id can address one segment and never climb out of it.
pub fn path_segment_encode(what: &str, value: &str) -> std::result::Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("name the {what}; it was empty"));
    }
    Ok(percent_encode(value))
}

/// A SharePoint item path, encoded segment by segment.
///
/// The one place a multi-segment value is legitimate — every Graph *id* this
/// connector handles is exactly one segment — so `/` is the one character kept
/// as a delimiter and everything else in each segment goes through
/// [`path_segment_encode`]. `..` and an empty segment are refused outright
/// rather than encoded: a path that climbs is not a path this connector should
/// spell correctly on a model's behalf.
pub fn sharepoint_path_encode(path: &str) -> std::result::Result<String, String> {
    let path = path.trim().trim_matches('/');
    if path.is_empty() {
        return Err("name the file path; it was empty".to_string());
    }
    let mut out = Vec::new();
    for segment in path.split('/') {
        let segment = segment.trim();
        if segment.is_empty() {
            return Err(format!(
                "the file path {path:?} has an empty segment; write one name between each `/`"
            ));
        }
        if segment == ".." || segment == "." {
            return Err(format!(
                "the file path {path:?} climbs out of the drive; name a path relative to the \
                 site's document library root"
            ));
        }
        out.push(percent_encode(segment));
    }
    Ok(out.join("/"))
}

/// A SharePoint site key, kept as the operator wrote it.
///
/// The one value this connector puts in a URL **unencoded**, and the reason is
/// Graph's own addressing: `/sites/{key}` accepts both an id triple
/// (`contoso.sharepoint.com,<guid>,<guid>`) and a `hostname:/path` compound
/// key, and percent-encoding either would stop Graph recognising it. So this is
/// a refusal rather than an encoding — the narrow set of characters that could
/// leave the path (a query, a fragment, whitespace, or a climbing segment) is
/// refused by name, and everything else is passed through.
pub fn site_key(value: &str) -> std::result::Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("name the site; it was empty".to_string());
    }
    if let Some(bad) = value
        .chars()
        .find(|c| matches!(c, '?' | '#' | '%' | '\\') || c.is_whitespace() || c.is_control())
    {
        return Err(format!(
            "the site {value:?} carries {bad:?}, which cannot appear in a site key; name the \
             site id or a hostname:/path key"
        ));
    }
    if value
        .split(['/', ':'])
        .any(|part| part == ".." || part == ".")
    {
        return Err(format!(
            "the site {value:?} climbs out of the endpoint; name the site id or a \
             hostname:/path key"
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

/// RFC 3986's unreserved set, kept; everything else percent-encoded, byte by
/// byte, so a non-ASCII character encodes as its UTF-8 octets.
fn percent_encode(value: &str) -> String {
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

/// Shared by both product code and the `Arc` the handler clones hold.
pub type SharedWire = Arc<Wire>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_endpoint_keeps_its_version_prefix_and_drops_a_trailing_slash() {
        // The address Microsoft's own documentation prints, kept verbatim: the
        // deliberate difference from `AtlassianSite::parse`.
        assert_eq!(
            GraphSite::parse("https://graph.microsoft.com/v1.0")
                .expect("a well-formed endpoint")
                .base_url(),
            "https://graph.microsoft.com/v1.0"
        );
        assert_eq!(
            GraphSite::parse("https://graph.microsoft.com/beta/")
                .expect("a well-formed endpoint")
                .base_url(),
            "https://graph.microsoft.com/beta"
        );
        assert_eq!(
            GraphSite::parse("https://graph.microsoft.com/")
                .expect("a well-formed endpoint")
                .base_url(),
            "https://graph.microsoft.com"
        );
        assert_eq!(
            GraphSite::parse("http://127.0.0.1:8080")
                .expect("a well-formed endpoint")
                .base_url(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn an_endpoint_with_no_scheme_a_query_or_a_fragment_is_refused() {
        for bad in [
            "graph.microsoft.com",
            "ftp://graph.microsoft.com",
            "https://graph.microsoft.com?a=1",
            "https://graph.microsoft.com/v1.0#me",
        ] {
            let err = GraphSite::parse(bad).expect_err("refused");
            assert!(
                err.to_string().contains(bad),
                "the refusal names what was typed, got: {err}"
            );
        }
    }

    #[test]
    fn a_path_segment_encodes_a_real_teams_channel_id_rather_than_refusing_it() {
        assert_eq!(
            path_segment_encode("channel id", " 19:abcdef0123456789@thread.tacv2 ")
                .expect("a real channel id is usable"),
            "19%3Aabcdef0123456789%40thread.tacv2"
        );
        assert_eq!(
            path_segment_encode("message id", "AAMkAD==").expect("a real message id"),
            "AAMkAD%3D%3D"
        );
        assert_eq!(
            path_segment_encode("item id", "01ABCDEF!123").expect("a real drive-item id"),
            "01ABCDEF%21123"
        );
    }

    #[test]
    fn a_path_segment_encodes_every_separator_so_one_id_stays_one_segment() {
        assert_eq!(
            path_segment_encode("item id", "../../etc").expect("encoded, not refused"),
            "..%2F..%2Fetc",
            "traversal in a single-segment id is neutralised by encoding, not by refusal"
        );
        path_segment_encode("item id", "   ").expect_err("an empty id is refused by name");
    }

    #[test]
    fn a_sharepoint_path_encodes_each_segment_and_keeps_the_separators() {
        assert_eq!(
            sharepoint_path_encode("Reports/Q3 plan.md").expect("a file path"),
            "Reports/Q3%20plan.md"
        );
        assert_eq!(
            sharepoint_path_encode("/Shared Documents/report.txt").expect("a file path"),
            "Shared%20Documents/report.txt"
        );
    }

    #[test]
    fn a_sharepoint_path_refuses_traversal_and_empty_segments() {
        for bad in [
            "",
            "  ",
            "Reports/../../secrets.txt",
            "Reports//report.txt",
            ".",
        ] {
            sharepoint_path_encode(bad).expect_err("refused");
        }
    }

    #[test]
    fn a_site_key_keeps_graphs_own_compound_forms_and_refuses_a_breakout() {
        assert_eq!(
            site_key("acme.sharepoint.com,1a2b,3c4d").expect("an id triple"),
            "acme.sharepoint.com,1a2b,3c4d"
        );
        assert_eq!(
            site_key("acme.sharepoint.com:/sites/ops:").expect("a hostname:/path key"),
            "acme.sharepoint.com:/sites/ops:"
        );
        for bad in ["", "acme?leak=1", "acme#fragment", "acme site", "acme/../x"] {
            site_key(bad).expect_err("refused");
        }
    }
}
