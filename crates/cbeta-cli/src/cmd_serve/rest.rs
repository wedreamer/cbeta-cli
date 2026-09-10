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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::env_paths::env_lock;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use cbeta_core::{Action, Command, Filters, Format};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn mini_corpus() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-rest-ut-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn app(index: Option<Arc<OpenIndex>>) -> Router {
        Router::new()
            .route("/search", post(post_search))
            .with_state(RestState { index })
    }

    async fn post_body(app: Router, body: &str) -> (StatusCode, String) {
        let req = Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test]
    async fn post_search_bad_json_is_400() {
        let (st, body) = post_body(app(None), "not-json").await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(body.contains("bad JSON") || body.contains("error"));
    }

    #[tokio::test]
    async fn post_search_no_index_is_503() {
        let (st, body) = post_body(app(None), r#"{"q":"真性有为空"}"#).await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("no index"));
    }

    #[tokio::test]
    async fn post_search_hit_with_mini_index() {
        let (open, index_dir) = {
            let _g = env_lock();
            let corpus = mini_corpus();
            let index_dir = temp_dir("idx");
            std::env::set_var("CBETA_CORPUS", &corpus);
            std::env::set_var("CBETA_INDEX", &index_dir);
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
            assert_eq!(crate::cmd_build::run(&build, false), 0);
            let open = Arc::new(OpenIndex::open(&index_dir).unwrap());
            std::env::remove_var("CBETA_CORPUS");
            std::env::remove_var("CBETA_INDEX");
            (open, index_dir)
        };

        let (st, body) = post_body(app(Some(open.clone())), r#"{"q":"真性有为空"}"#).await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert!(body.contains("hits"));
        assert!(body.contains("T30n1578") || body.contains("line_id"));

        let (st, body) = post_body(app(Some(open)), r#"{"q":"x","mode":"not-a-mode"}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");

        let _ = std::fs::remove_dir_all(&index_dir);
    }
}
