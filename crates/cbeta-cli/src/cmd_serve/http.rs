//! HTTP serve: REST `/search` + MCP `/mcp` on one listener.

use std::net::TcpListener;
use std::sync::Arc;

use axum::routing::post;
use axum::Router;
use cbeta_search::OpenIndex;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use super::bind::{allowed_hosts_for, bind_listener, parse_http_addr};
use super::rest::{post_search, RestState};
use crate::env_paths::index_root;
use crate::mcp::CbetaMcp;

/// Run HTTP server until ctrl_c / SIGTERM. Returns process exit code.
pub fn run_http(addr_raw: &str) -> i32 {
    init_stderr_tracing();

    let addr = match parse_http_addr(addr_raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let std_listener = match bind_listener(addr) {
        Ok(l) => l,
        Err(code) => return code,
    };
    if let Err(e) = std_listener.set_nonblocking(true) {
        eprintln!("set_nonblocking: {e}");
        return 2;
    }

    let local = match std_listener.local_addr() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("local_addr: {e}");
            return 2;
        }
    };

    let index = open_index_once();
    let hosts = allowed_hosts_for(local);

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to start tokio runtime: {e}");
            return 2;
        }
    };

    match rt.block_on(serve_http(std_listener, index, hosts)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("http serve error: {e}");
            2
        }
    }
}

fn init_stderr_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init();
}

fn open_index_once() -> Option<Arc<OpenIndex>> {
    match index_root() {
        Ok(root) => match OpenIndex::open(&root) {
            Ok(idx) => Some(Arc::new(idx)),
            Err(e) => {
                eprintln!("index open deferred (POST /search → 503): {e}");
                None
            }
        },
        Err(e) => {
            eprintln!("index root unavailable (POST /search → 503): {e}");
            None
        }
    }
}

async fn serve_http(
    std_listener: TcpListener,
    index: Option<Arc<OpenIndex>>,
    allowed_hosts: Vec<String>,
) -> Result<(), String> {
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .map_err(|e| format!("tokio listener: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;
    // Announce only after the tokio listener owns the socket (tests connect on this line).
    eprintln!("listening on {local}");

    let cancel = CancellationToken::new();
    let mcp_index = index.clone();
    let mcp_config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts)
        .with_cancellation_token(cancel.clone());
    let mcp_service = StreamableHttpService::new(
        move || Ok(CbetaMcp::with_index(mcp_index.clone())),
        Arc::new(LocalSessionManager::default()),
        mcp_config,
    );

    let rest_state = RestState { index };
    let app = Router::new()
        .route("/search", post(post_search))
        .with_state(rest_state)
        .nest_service("/mcp", mcp_service);

    let shutdown = cancel.clone();
    let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
        wait_for_shutdown_signal().await;
        shutdown.cancel();
    });

    serve.await.map_err(|e| format!("axum serve: {e}"))
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("SIGTERM listener unavailable: {e}");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
