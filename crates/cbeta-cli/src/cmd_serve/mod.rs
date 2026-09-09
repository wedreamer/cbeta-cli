//! `cbeta serve`: stdio MCP, or HTTP when `--http ADDR` is set.

mod bind;
mod http;
mod rest;

use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

use crate::mcp::CbetaMcp;

/// Run serve. `http` is `Some(addr)` for HTTP mode; `None` is stdio MCP.
pub fn run(http: Option<&str>) -> i32 {
    if let Some(addr) = http {
        return http::run_http(addr);
    }
    run_stdio()
}

fn run_stdio() -> i32 {
    // WHY: product output must never touch stdout; agents parse NDJSON there.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_http_branch_rejects_bare_port() {
        assert_eq!(run(Some("1873")), 2);
    }
}
