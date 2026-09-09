//! `cbeta serve`: stdio MCP server (five tools; no HTTP).

use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

use crate::mcp::CbetaMcp;

/// Run the MCP server on stdin/stdout until the client disconnects.
///
/// Logs go to stderr only — stdout is the JSON-RPC channel.
pub fn run() -> i32 {
    // WHY: product output must never touch stdout; agents parse NDJSON there.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

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

    match rt.block_on(serve_stdio()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("mcp serve error: {e}");
            2
        }
    }
}

async fn serve_stdio() -> Result<(), String> {
    let server = CbetaMcp::new();
    let running = server
        .serve(stdio())
        .await
        .map_err(|e| format!("serve init: {e}"))?;
    running
        .waiting()
        .await
        .map_err(|e| format!("serve wait: {e}"))?;
    Ok(())
}
