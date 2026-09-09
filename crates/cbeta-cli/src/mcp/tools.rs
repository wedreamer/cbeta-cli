//! Five MCP tools mapped onto the same library paths as the CLI handlers.
//!
//! Results return as JSON text on the MCP channel. Never write product output
//! with `println!` — stdout is reserved for JSON-RPC.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::Deserialize;

use super::handlers;

/// MCP server holding the generated tool router.
#[derive(Debug, Clone)]
pub struct CbetaMcp {
    tool_router: ToolRouter<Self>,
}

impl CbetaMcp {
    /// Build a server with the five product tools registered.
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for CbetaMcp {
    fn default() -> Self {
        Self::new()
    }
}

/// `cbeta_search` arguments: CLI `q` twin via `clauses`/`mode`, plus filter flags.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchArgs {
    /// Free-text query (CLI positional `q`). Ignored when `clauses` is non-empty.
    #[serde(default)]
    pub q: Option<String>,
    /// Mode override (`keyword` / `phrase` / `near` / `before`).
    #[serde(default)]
    pub mode: Option<String>,
    /// Structured terms (agent twin of DSL `q`); no CLI `--clauses` flag.
    #[serde(default)]
    pub clauses: Option<Vec<String>>,
    #[serde(default)]
    pub work: Option<Vec<String>>,
    #[serde(default)]
    pub author: Option<Vec<String>>,
    #[serde(default)]
    pub canon: Option<String>,
    #[serde(default, rename = "type")]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub title: Option<Vec<String>>,
}

/// `cbeta_verify_quote` arguments.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct VerifyArgs {
    /// Quote text (CLI `verify <text>` / field `q`).
    pub q: String,
}

/// `cbeta_get_passage` arguments: one tool for get/read/cite.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetPassageArgs {
    /// Action: `get` (default), `read`, or `cite`.
    #[serde(default = "default_get_action")]
    pub action: String,
    /// Target `line_id` for get/cite.
    #[serde(default)]
    pub line_id: Option<String>,
    /// Neighbor radius (CLI `-C` / `--context`).
    #[serde(default)]
    pub context: Option<u32>,
    /// Work id for `read` (CLI `read <work>`).
    #[serde(default)]
    pub work: Option<String>,
    /// Juan for `read` (CLI `--juan`).
    #[serde(default)]
    pub juan: Option<u32>,
}

fn default_get_action() -> String {
    "get".into()
}

/// `cbeta_list_catalog` filter args (same names as CLI flags).
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct CatalogArgs {
    #[serde(default)]
    pub work: Option<Vec<String>>,
    #[serde(default)]
    pub author: Option<Vec<String>>,
    #[serde(default)]
    pub canon: Option<String>,
    #[serde(default, rename = "type")]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub title: Option<Vec<String>>,
}

/// Empty object for tools with no parameters.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct EmptyArgs {}

#[tool_router]
impl CbetaMcp {
    /// Full-text / DSL search over the local index.
    #[tool(
        name = "cbeta_search",
        description = "Search CBETA index (keyword/phrase/near). clauses is the structured twin of CLI q."
    )]
    async fn cbeta_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let outcome = tokio::task::spawn_blocking(move || handlers::run_search(args))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(outcome_to_result(outcome))
    }

    /// Check whether pasted text is original CBETA wording.
    #[tool(
        name = "cbeta_verify_quote",
        description = "Verify a quote against the index (is_original / exact_hit / similar)."
    )]
    async fn cbeta_verify_quote(
        &self,
        Parameters(args): Parameters<VerifyArgs>,
    ) -> Result<CallToolResult, McpError> {
        let outcome = tokio::task::spawn_blocking(move || handlers::run_verify(args.q))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(outcome_to_result(outcome))
    }

    /// Fetch a line, neighbors, juan listing, or citation.
    #[tool(
        name = "cbeta_get_passage",
        description = "get/read/cite passage by line_id or work+juan (one tool, action enum)."
    )]
    async fn cbeta_get_passage(
        &self,
        Parameters(args): Parameters<GetPassageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let outcome = tokio::task::spawn_blocking(move || handlers::run_get_passage(args))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(outcome_to_result(outcome))
    }

    /// Browse catalog metadata with the same filters as CLI `catalog`.
    #[tool(
        name = "cbeta_list_catalog",
        description = "List catalog entries (filters: work, author, canon, type, title)."
    )]
    async fn cbeta_list_catalog(
        &self,
        Parameters(args): Parameters<CatalogArgs>,
    ) -> Result<CallToolResult, McpError> {
        let outcome = tokio::task::spawn_blocking(move || handlers::run_catalog(args))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(outcome_to_result(outcome))
    }

    /// Show active index / build info (CLI `info`).
    #[tool(
        name = "cbeta_index_info",
        description = "Show active index metadata (cbeta_tag, scope, artifact_id, work_count)."
    )]
    async fn cbeta_index_info(
        &self,
        Parameters(_args): Parameters<EmptyArgs>,
    ) -> Result<CallToolResult, McpError> {
        let outcome = tokio::task::spawn_blocking(handlers::run_info)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(outcome_to_result(outcome))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CbetaMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("CBETA offline search MCP (cbeta serve). Five tools only.")
    }
}

fn outcome_to_result(outcome: Result<serde_json::Value, String>) -> CallToolResult {
    match outcome {
        Ok(v) => {
            let text = serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string());
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
        Err(msg) => CallToolResult::error(vec![ContentBlock::text(msg)]),
    }
}
