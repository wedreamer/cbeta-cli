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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Command, Filters, Format};
    use serde_json::json;
    use std::path::PathBuf;

    fn mini_corpus() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-mcp-t-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn build_mini(corpus: &std::path::Path, index: &std::path::Path) {
        std::env::set_var("CBETA_CORPUS", corpus);
        std::env::set_var("CBETA_INDEX", index);
        let build = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Plain,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(crate::cmd_build::run(&build), 0);
    }

    #[test]
    fn new_default_and_server_info() {
        let a = CbetaMcp::new();
        let b = CbetaMcp::default();
        let _ = (
            a.tool_router.list_all().len(),
            b.tool_router.list_all().len(),
        );
        let info = CbetaMcp::new().get_info();
        let text = format!("{info:?}");
        assert!(
            text.contains("tools") || text.contains("CBETA") || text.contains("instructions"),
            "server info={text}"
        );
    }

    #[test]
    fn outcome_to_result_ok_and_err() {
        let ok = outcome_to_result(Ok(json!({"hits": []})));
        assert!(!ok.is_error.unwrap_or(true));
        let err = outcome_to_result(Err("boom".into()));
        assert!(err.is_error.unwrap_or(false));
    }

    #[test]
    fn default_get_action_is_get() {
        assert_eq!(default_get_action(), "get");
    }

    #[test]
    fn tool_methods_smoke_with_mini_index() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("tools");
        build_mini(&corpus, &index);
        let mcp = CbetaMcp::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio test runtime");
        rt.block_on(async {
            let search = mcp
                .cbeta_search(Parameters(SearchArgs {
                    q: Some("真性有为空".into()),
                    mode: None,
                    clauses: None,
                    work: None,
                    author: None,
                    canon: None,
                    types: None,
                    title: None,
                }))
                .await
                .unwrap();
            assert!(!search.is_error.unwrap_or(true));

            let verify = mcp
                .cbeta_verify_quote(Parameters(VerifyArgs {
                    q: "真性有為空，如幻緣生故".into(),
                }))
                .await
                .unwrap();
            assert!(!verify.is_error.unwrap_or(true));

            let get = mcp
                .cbeta_get_passage(Parameters(GetPassageArgs {
                    action: "get".into(),
                    line_id: Some("T30n1578_p0268b21".into()),
                    context: Some(1),
                    work: None,
                    juan: None,
                }))
                .await
                .unwrap();
            assert!(!get.is_error.unwrap_or(true));

            let catalog = mcp
                .cbeta_list_catalog(Parameters(CatalogArgs {
                    work: Some(vec!["T1578".into()]),
                    author: None,
                    canon: Some("T".into()),
                    types: None,
                    title: None,
                }))
                .await
                .unwrap();
            assert!(!catalog.is_error.unwrap_or(true));

            let info = mcp
                .cbeta_index_info(Parameters(EmptyArgs {}))
                .await
                .unwrap();
            assert!(!info.is_error.unwrap_or(true));

            let bad = mcp
                .cbeta_search(Parameters(SearchArgs {
                    q: None,
                    mode: None,
                    clauses: None,
                    work: None,
                    author: None,
                    canon: None,
                    types: None,
                    title: None,
                }))
                .await
                .unwrap();
            assert!(bad.is_error.unwrap_or(false));
        });

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }
}
