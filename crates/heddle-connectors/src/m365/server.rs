//! The Microsoft 365 MCP server: five tools over one [`Wire`] — Outlook mail
//! (read, send), one SharePoint file (read), and Teams channel messages (read,
//! send).
//!
//! [`M365Server::connect`] is the egress gate (ADR-0002 D4), and it is the
//! **only** place this module builds a [`Wire`]: a type that can only be built
//! after the gate passes cannot express a connector that skipped it. Same
//! structure, same order and same reasoning as `AtlassianServer::connect` one
//! directory over.

use super::client::{
    path_segment_encode, required, sharepoint_path_encode, site_key, GraphSite, SharedWire, Wire,
};
use heddle_core::{HeddleError, SecretProvider, SecretRef, SecretValue};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// How many messages a read returns when the caller names no number, and the
/// most it will ask for however large a number the caller names.
///
/// Capped rather than passed through for `fs_read`'s recorded reason: a page
/// size is a model's guess, and a mailbox has no upper bound of its own — the
/// cap is what keeps one tool call from filling a context window. There is no
/// pagination behind it (spec 040, out of scope), so a caller that needs more
/// raises `$top` within this cap and no further.
const DEFAULT_MAIL_PAGE: u32 = 10;
const DEFAULT_CHANNEL_PAGE: u32 = 20;
const MAX_PAGE: u32 = 50;

/// One Microsoft Graph endpoint and the access token every request
/// authenticates with.
///
/// Public because [`crate::m365_connector`]'s caller — ultimately an operator's
/// connector configuration — builds this directly, exactly as
/// [`crate::AtlassianConfig`] is public for the configuration it carries.
///
/// One field fewer than [`crate::AtlassianConfig`]: Graph authenticates with a
/// single Bearer token, so there is no account email to pair it with.
#[derive(Debug, Clone)]
pub struct M365Config {
    /// `https://graph.microsoft.com/v1.0` in production, or
    /// `http://host:port` for a test stub. Unlike an Atlassian site this **may**
    /// carry a path: Graph's version prefix is part of the address an operator
    /// is given, and every endpoint below is written relative to it.
    pub base_url: String,
    /// A reference to the OAuth access token, resolved once at
    /// [`M365Server::connect`]. This connector never acquires or refreshes one;
    /// that is the `SecretProvider` backend's concern.
    pub token: SecretRef,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadMailParams {
    /// One message's id. Omit it to read the most recent messages instead.
    pub message_id: Option<String>,
    /// How many messages to return when no `message_id` is given.
    pub top: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendMailParams {
    /// One recipient's address.
    pub to: String,
    pub subject: String,
    /// Plain text. Sent as `contentType: "Text"`; this connector composes no
    /// HTML and carries no attachments.
    pub body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadSharePointFileParams {
    /// The SharePoint site key: a site id triple
    /// (`contoso.sharepoint.com,<guid>,<guid>`) or a `hostname:/path` key.
    pub site: String,
    /// The file's path inside the site's document library, e.g.
    /// `Reports/Q3 plan.md`. Give exactly one of `path` or `item_id`.
    pub path: Option<String>,
    /// The file's drive-item id. Give exactly one of `path` or `item_id`.
    pub item_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadTeamsMessagesParams {
    pub team_id: String,
    /// A channel id, e.g. `19:abcdef0123456789@thread.tacv2`.
    pub channel_id: String,
    /// How many messages to return.
    pub top: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendTeamsMessageParams {
    pub team_id: String,
    pub channel_id: String,
    /// The message body, as text.
    pub body: String,
}

/// The tool holder. `Clone` for the reason [`crate::server::EmbeddedServer`]
/// is: rmcp hands each request a clone of the handler, and every clone must
/// reach the *same* wire rather than a copy of it.
#[derive(Clone)]
pub struct M365Server {
    wire: SharedWire,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl M365Server {
    /// The egress gate and the only constructor.
    ///
    /// Order is the guarantee, `AtlassianServer::connect`'s and
    /// [`heddle_gateway::Router::client_for`]'s before it:
    ///
    /// 1. **egress**, before anything else is built or resolved — this
    ///    connector has no local form, so a refusal here needs neither the
    ///    address nor the credential to already be worth reporting;
    /// 2. parse the endpoint address;
    /// 3. resolve the credential;
    /// 4. build the wire.
    pub fn connect(
        config: M365Config,
        secrets: &dyn SecretProvider,
        egress_allowed: bool,
    ) -> heddle_core::Result<M365Server> {
        if !egress_allowed {
            return Err(HeddleError::Tool(
                "the Microsoft 365 connector is refused: it reaches Microsoft Graph and egress \
                 is off; pass --allow-egress to permit this run to leave the machine"
                    .to_string(),
            ));
        }
        let site = GraphSite::parse(&config.base_url)?;
        let credential: SecretValue = secrets.resolve(&config.token)?;
        let wire = Wire::new(site, credential);
        Ok(M365Server {
            wire: Arc::new(wire),
            tool_router: Self::tool_router(),
        })
    }

    #[tool(
        description = "Read Outlook mail. With `message_id`, returns that one message's subject, \
                        sender and full body; without it, returns the most recent messages, \
                        newest first, as subject, sender and preview."
    )]
    pub fn outlook_read_mail(&self, params: Parameters<ReadMailParams>) -> Result<String, String> {
        let ReadMailParams { message_id, top } = params.0;
        if let Some(id) = message_id.as_deref().filter(|id| !id.trim().is_empty()) {
            let id = path_segment_encode("message id", id)?;
            let message = self.wire.get(&format!("/me/messages/{id}"))?;
            return Ok(format!(
                "{}\nfrom: {}\n\n{}",
                text(&message["subject"]),
                text(&message["from"]["emailAddress"]["address"]),
                text(&message["body"]["content"])
            ));
        }
        // `$orderby`'s space is percent-encoded here rather than left for the
        // transport: a raw space in a request line is not a URL at all.
        let page = self.wire.get(&format!(
            "/me/messages?$top={}&$orderby=receivedDateTime%20desc",
            page_size(top, DEFAULT_MAIL_PAGE)
        ))?;
        Ok(join(&page, |message| {
            format!(
                "{} — {}\n{}",
                text(&message["subject"]),
                text(&message["from"]["emailAddress"]["address"]),
                text(&message["bodyPreview"])
            )
        }))
    }

    #[tool(
        description = "Send an Outlook mail to one recipient. `body` is plain text; attachments \
                        are not supported."
    )]
    pub fn outlook_send_mail(&self, params: Parameters<SendMailParams>) -> Result<String, String> {
        let SendMailParams { to, subject, body } = params.0;
        let to = required("recipient", &to)?;
        let subject = required("subject", &subject)?;
        let body = required("body", &body)?;
        let payload = json!({
            "message": {
                "subject": subject,
                "body": {"contentType": "Text", "content": body},
                "toRecipients": [{"emailAddress": {"address": to}}]
            },
            "saveToSentItems": true
        });
        // Graph answers 202 Accepted with no body at all, which `Wire::post`
        // hands back as `Value::Null`. Nothing is read out of it: the answer is
        // this fixed string, because there is no id to echo.
        self.wire.post("/me/sendMail", &payload)?;
        Ok(format!("mail sent to {to}"))
    }

    #[tool(
        description = "Read one SharePoint file's content as text. Give the site key and exactly \
                        one of `path` (inside the site's document library) or `item_id`."
    )]
    pub fn sharepoint_read_file(
        &self,
        params: Parameters<ReadSharePointFileParams>,
    ) -> Result<String, String> {
        let ReadSharePointFileParams {
            site,
            path,
            item_id,
        } = params.0;
        let site = site_key(&site)?;
        let path = path.filter(|value| !value.trim().is_empty());
        let item_id = item_id.filter(|value| !value.trim().is_empty());
        let endpoint =
            match (path, item_id) {
                (Some(path), None) => format!(
                    "/sites/{site}/drive/root:/{}:/content",
                    sharepoint_path_encode(&path)?
                ),
                (None, Some(item_id)) => format!(
                    "/sites/{site}/drive/items/{}/content",
                    path_segment_encode("item id", &item_id)?
                ),
                (Some(_), Some(_)) => return Err(
                    "name either `path` or `item_id`, not both: they are two ways of addressing \
                     the same file"
                        .to_string(),
                ),
                (None, None) => return Err(
                    "name the file: either `path`, relative to the site's document library, or \
                     `item_id`, its drive-item id"
                        .to_string(),
                ),
            };
        // Read as text and never parsed: a file's bytes promise no shape, so
        // `Wire::get` would turn every non-JSON file into a parse error.
        Ok(self.wire.get_text(&endpoint)?.trim_end().to_string())
    }

    #[tool(
        description = "Read the most recent messages in a Teams channel, as sender and body, one \
                        message per block."
    )]
    pub fn teams_read_messages(
        &self,
        params: Parameters<ReadTeamsMessagesParams>,
    ) -> Result<String, String> {
        let ReadTeamsMessagesParams {
            team_id,
            channel_id,
            top,
        } = params.0;
        let team = path_segment_encode("team id", &team_id)?;
        let channel = path_segment_encode("channel id", &channel_id)?;
        let page = self.wire.get(&format!(
            "/teams/{team}/channels/{channel}/messages?$top={}",
            page_size(top, DEFAULT_CHANNEL_PAGE)
        ))?;
        Ok(join(&page, |message| {
            format!(
                "{}: {}",
                text(&message["from"]["user"]["displayName"]),
                text(&message["body"]["content"])
            )
        }))
    }

    #[tool(description = "Post a message to a Teams channel. `body` is the message text.")]
    pub fn teams_send_message(
        &self,
        params: Parameters<SendTeamsMessageParams>,
    ) -> Result<String, String> {
        let SendTeamsMessageParams {
            team_id,
            channel_id,
            body,
        } = params.0;
        let team = path_segment_encode("team id", &team_id)?;
        let channel = path_segment_encode("channel id", &channel_id)?;
        let body = required("message body", &body)?;
        let sent = self.wire.post(
            &format!("/teams/{team}/channels/{channel}/messages"),
            &json!({"body": {"content": body}}),
        )?;
        // Unlike `sendMail`, a channel post answers with the created message —
        // but `text` is total over a `Value::Null` answer too, so a stub or a
        // tenant that returns nothing yields the generic form rather than a
        // panic.
        match text(&sent["id"]) {
            id if id.is_empty() => Ok("message sent".to_string()),
            id => Ok(format!("sent message {id}")),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for M365Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Heddle's Microsoft 365 connector, over one Microsoft Graph tenant and one Bearer \
             token: read and send Outlook mail, read a SharePoint file, read and send Teams \
             channel messages.",
        )
    }
}

/// A JSON string field, or `""` when it is absent or not a string — every
/// caller above already has a real value in the case that matters and treats
/// this as a diagnostic fallback, not a validated read. Total over
/// [`Value::Null`], which is what makes indexing an empty Graph answer safe.
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").to_string()
}

/// The caller's page size, floored at one and capped at [`MAX_PAGE`].
///
/// Clamped rather than refused: a model that asks for a thousand messages has
/// made a guess, not an error, and the useful answer is the first fifty rather
/// than a refusal it has to learn to avoid.
fn page_size(asked: Option<u32>, default: u32) -> u32 {
    asked.unwrap_or(default).clamp(1, MAX_PAGE)
}

/// Formats a Graph collection answer.
///
/// Graph wraps every list in `{"value": [...]}` — not Jira's `{"issues": […]}`
/// — and this is the one place that shape is named, so a tool method reads only
/// the fields of one item.
fn join(page: &Value, one: impl Fn(&Value) -> String) -> String {
    let items = page["value"].as_array().cloned().unwrap_or_default();
    items.iter().map(one).collect::<Vec<_>>().join("\n\n")
}
