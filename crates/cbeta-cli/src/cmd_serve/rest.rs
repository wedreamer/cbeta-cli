//! REST `POST /search` — same JSON body as MCP `SearchArgs` / CLI search filters.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cbeta_search::OpenIndex;
use serde_json::json;

use crate::mcp::handlers;
use crate::mcp::tools::SearchArgs;

/// Shared state for REST handlers (index opened once at serve start).
#[derive(Clone)]
pub struct RestState {
    /// `None` when no index was available at start → POST returns 503.
    pub index: Option<Arc<OpenIndex>>,
}

/// `POST /search` → `{ "hits": [ Hit… ] }` (200 even when empty).
///
/// Body is `SearchArgs` JSON (`q`, optional `mode`/`clauses`/filters).
pub async fn post_search(
    State(state): State<RestState>,
    body: Result<Json<SearchArgs>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Json(args) = match body {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("bad JSON: {e}") })),
            )
                .into_response();
        }
    };

    let Some(index) = state.index.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "no index; run: cbeta build --scope <name>"
            })),
        )
            .into_response();
    };

    let outcome = tokio::task::spawn_blocking(move || handlers::run_search(args, Some(index)))
        .await
        .unwrap_or_else(|e| Err(format!("join: {e}")));

    match outcome {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(msg) => {
            let status = if msg.contains("no index") {
                StatusCode::SERVICE_UNAVAILABLE
            } else if msg.contains("unknown mode")
                || msg.contains("requires q")
                || msg.contains("fullwidth")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(json!({ "error": msg }))).into_response()
        }
    }
}
