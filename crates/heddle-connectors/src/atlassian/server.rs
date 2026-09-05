//! The Atlassian MCP server: six tools over one [`Wire`] — Jira issues (search,
//! read, create, comment) and Confluence pages (read, create).
//!
//! [`AtlassianServer::connect`] is the egress gate (ADR-0002 D4), and it is the
//! **only** place this crate builds a [`Wire`]: a type that can only be built
//! after the gate passes cannot express a connector that skipped it.

use super::client::{encode_query_value, path_segment, required, AtlassianSite, SharedWire, Wire};
use heddle_core::{HeddleError, SecretProvider, SecretRef, SecretValue};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// One Atlassian site and the credential every request authenticates with.
///
/// Public because [`crate::atlassian_connector`]'s caller — ultimately an
/// operator's connector configuration — builds this directly, the way
/// [`crate::server::ReadParams`] is public for the schema it carries and this
/// is public for the configuration it carries.
#[derive(Debug, Clone)]
pub struct AtlassianConfig {
    /// `https://<tenant>.atlassian.net` or `http://host:port` for a test stub.
    /// Carries no path: every endpoint below appends one Atlassian documents.
    pub base_url: String,
    /// The Atlassian account's email. Not a secret (§ [`Wire`]'s field docs),
    /// which is why it travels beside the reference and not inside it.
    pub email: String,
    /// A reference to the API token, resolved once at [`AtlassianServer::connect`].
    pub token: SecretRef,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IssueKeyParams {
    /// A Jira issue key, e.g. `PROJ-123`.
    pub key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// A JQL expression, e.g. `project = PROJ AND status != Done`.
    pub jql: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateIssueParams {
    /// The project key, e.g. `PROJ`.
    pub project: String,
    pub summary: String,
    /// Plain text. Wrapped as an Atlassian Document Format document before it
    /// is sent — Jira's v3 API refuses a bare string.
    pub description: String,
    /// e.g. `Task`, `Bug`, `Story`.
    pub issue_type: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddCommentParams {
    pub key: String,
    /// Plain text, wrapped as a document the same way a created issue's
    /// description is.
    pub body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetPageParams {
    pub page_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreatePageParams {
    pub space_id: String,
    pub title: String,
    /// Confluence storage format (HTML-like XML), not plain text.
    pub body: String,
}

/// The tool holder. `Clone` for the reason [`crate::server::EmbeddedServer`]
/// is: rmcp hands each request a clone of the handler, and every clone must
/// reach the *same* wire rather than a copy of it.
#[derive(Clone)]
pub struct AtlassianServer {
    wire: SharedWire,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AtlassianServer {
    /// The egress gate and the only constructor.
    ///
    /// Order is the guarantee, [`heddle_gateway::Router::client_for`]'s:
    ///
    /// 1. **egress**, before anything else is built or resolved — this
    ///    connector has no local form, so a refusal here needs neither the
    ///    address nor the credential to already be worth reporting;
    /// 2. parse the site address;
    /// 3. resolve the credential;
    /// 4. build the wire.
    pub fn connect(
        config: AtlassianConfig,
        secrets: &dyn SecretProvider,
        egress_allowed: bool,
    ) -> heddle_core::Result<AtlassianServer> {
        if !egress_allowed {
            return Err(HeddleError::Tool(
                "the Atlassian connector is refused: it reaches a network Jira/Confluence \
                 site and egress is off; pass --allow-egress to permit this run to leave the \
                 machine"
                    .to_string(),
            ));
        }
        let site = AtlassianSite::parse(&config.base_url)?;
        let credential: SecretValue = secrets.resolve(&config.token)?;
        let wire = Wire::new(site, config.email, credential);
        Ok(AtlassianServer {
            wire: Arc::new(wire),
            tool_router: Self::tool_router(),
        })
    }

    #[tool(
        description = "Search Jira issues with a JQL expression. Returns each match's key, \
                        summary and status, one per line."
    )]
    pub fn jira_search(&self, params: Parameters<SearchParams>) -> Result<String, String> {
        let jql = required("jql", &params.0.jql)?;
        let value = self.wire.get(&format!(
            "/rest/api/3/search?jql={}",
            encode_query_value(&jql)
        ))?;
        let issues = value["issues"].as_array().cloned().unwrap_or_default();
        let lines: Vec<String> = issues
            .iter()
            .map(|issue| {
                format!(
                    "{}: {} ({})",
                    text(&issue["key"]),
                    text(&issue["fields"]["summary"]),
                    text(&issue["fields"]["status"]["name"])
                )
            })
            .collect();
        Ok(lines.join("\n"))
    }

    #[tool(
        description = "Read one Jira issue by key. Returns its summary, status and description \
                        (flattened to plain text) as text."
    )]
    pub fn jira_get_issue(&self, params: Parameters<IssueKeyParams>) -> Result<String, String> {
        let key = path_segment("issue key", &params.0.key)?;
        let value = self.wire.get(&format!("/rest/api/3/issue/{key}"))?;
        let fields = &value["fields"];
        Ok(format!(
            "{key}: {}\nstatus: {}\n\n{}",
            text(&fields["summary"]),
            text(&fields["status"]["name"]),
            adf_text(&fields["description"])
        ))
    }

    #[tool(
        description = "Create a Jira issue. `description` is plain text and is wrapped as a \
                        document automatically; Jira's v3 API refuses a bare string."
    )]
    pub fn jira_create_issue(
        &self,
        params: Parameters<CreateIssueParams>,
    ) -> Result<String, String> {
        let CreateIssueParams {
            project,
            summary,
            description,
            issue_type,
        } = params.0;
        let project = required("project", &project)?;
        let summary = required("summary", &summary)?;
        let issue_type = required("issue type", &issue_type)?;
        let body = json!({
            "fields": {
                "project": {"key": project},
                "summary": summary,
                "description": adf_doc(&description),
                "issuetype": {"name": issue_type}
            }
        });
        let value = self.wire.post("/rest/api/3/issue", &body)?;
        Ok(format!("created {}", text(&value["key"])))
    }

    #[tool(description = "Add a comment to a Jira issue. `body` is plain text.")]
    pub fn jira_add_comment(&self, params: Parameters<AddCommentParams>) -> Result<String, String> {
        let key = path_segment("issue key", &params.0.key)?;
        let comment = required("comment body", &params.0.body)?;
        let body = json!({"body": adf_doc(&comment)});
        let value = self
            .wire
            .post(&format!("/rest/api/3/issue/{key}/comment"), &body)?;
        Ok(format!("added comment {} to {key}", text(&value["id"])))
    }

    #[tool(description = "Read a Confluence page's title and body (storage format) by page id.")]
    pub fn confluence_get_page(&self, params: Parameters<GetPageParams>) -> Result<String, String> {
        let page_id = path_segment("page id", &params.0.page_id)?;
        let value = self
            .wire
            .get(&format!("/wiki/api/v2/pages/{page_id}?body-format=storage"))?;
        Ok(format!(
            "{}\n\n{}",
            text(&value["title"]),
            text(&value["body"]["storage"]["value"])
        ))
    }

    #[tool(
        description = "Create a Confluence page. `body` is Confluence storage format \
                        (HTML-like XML), not plain text."
    )]
    pub fn confluence_create_page(
        &self,
        params: Parameters<CreatePageParams>,
    ) -> Result<String, String> {
        let CreatePageParams {
            space_id,
            title,
            body,
        } = params.0;
        let space_id = required("space id", &space_id)?;
        let title = required("title", &title)?;
        let body = required("body", &body)?;
        let payload = json!({
            "spaceId": space_id,
            "status": "current",
            "title": title,
            "body": {"representation": "storage", "value": body}
        });
        let value = self.wire.post("/wiki/api/v2/pages", &payload)?;
        Ok(format!("created page {}", text(&value["id"])))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AtlassianServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Heddle's Atlassian connector, over one operator-named site and one credential: \
             search, read, create and comment on Jira issues; read and create Confluence pages.",
        )
    }
}

/// A JSON string field, or `""` when it is absent or not a string — every
/// caller above already has a real value in the case that matters and treats
/// this as a diagnostic fallback, not a validated read.
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").to_string()
}

/// The Atlassian Document Format wrapper Jira's v3 API requires for a
/// description or a comment body: one paragraph holding one text run. Real
/// ADF nests far more than this, but a model composes plain text, not a
/// document tree, so this is the only shape this connector ever writes.
fn adf_doc(text: &str) -> Value {
    json!({
        "type": "doc",
        "version": 1,
        "content": [{
            "type": "paragraph",
            "content": [{"type": "text", "text": text}]
        }]
    })
}

/// Flattens an Atlassian Document Format value to plain text: every `text`
/// leaf, in document order, with a blank line between paragraphs.
///
/// Reads only what [`adf_doc`] writes and what a real site is documented to
/// send back — `type`/`content`/`text` — and ignores every other ADF node
/// kind (headings, lists, marks) by falling through to nothing rather than
/// refusing: a description a human wrote is not this connector's to reject
/// for using a node kind it does not flatten specially.
fn adf_text(value: &Value) -> String {
    let mut out = String::new();
    collect_adf_text(value, &mut out);
    out.trim().to_string()
}

fn collect_adf_text(value: &Value, out: &mut String) {
    let Value::Object(map) = value else {
        if let Value::Array(items) = value {
            for item in items {
                collect_adf_text(item, out);
            }
        }
        return;
    };
    if let Some(Value::String(leaf)) = map.get("text") {
        out.push_str(leaf);
    }
    if let Some(content) = map.get("content") {
        collect_adf_text(content, out);
        if map.get("type").and_then(Value::as_str) == Some("paragraph") {
            out.push_str("\n\n");
        }
    }
}
