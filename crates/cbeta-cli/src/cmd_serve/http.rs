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
    run_http_with_cancel(addr_raw, None)
}

fn run_http_with_cancel(addr_raw: &str, test_cancel: Option<CancellationToken>) -> i32 {
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

    let wire_os_signal = test_cancel.is_none();
    let cancel = test_cancel.unwrap_or_default();
    match rt.block_on(async move {
        if wire_os_signal {
            let signal_cancel = cancel.clone();
            tokio::spawn(async move {
                wait_for_shutdown_signal().await;
                signal_cancel.cancel();
            });
        }
        serve_http(std_listener, index, hosts, cancel).await
    }) {
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

/// Serve until `cancel` is cancelled (production wires ctrl_c / SIGTERM).
async fn serve_http(
    std_listener: TcpListener,
    index: Option<Arc<OpenIndex>>,
    allowed_hosts: Vec<String>,
    cancel: CancellationToken,
) -> Result<(), String> {
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .map_err(|e| format!("tokio listener: {e}"))?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;
    // Announce only after the tokio listener owns the socket (tests connect on this line).
    eprintln!("listening on {local}");

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
        shutdown.cancelled().await;
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Command, Filters, Format};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    fn mini_corpus() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-http-ut-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn http_post(addr: &str, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let req = format!(
            "POST /search HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
        let text = String::from_utf8_lossy(&buf).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = text
            .split("\r\n\r\n")
            .nth(1)
            .unwrap_or("")
            .trim()
            .to_string();
        (status, body)
    }

    #[test]
    fn run_http_rejects_bare_port() {
        assert_eq!(run_http("1873"), 2);
    }

    #[test]
    fn open_index_once_none_without_index() {
        let _g = env_lock();
        let empty = temp_dir("noidx");
        std::env::set_var("CBETA_INDEX", &empty);
        assert!(open_index_once().is_none());
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn open_index_once_some_with_mini() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("open-once");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
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
        assert!(open_index_once().is_some());
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }

    #[test]
    fn run_http_with_cancel_covers_bind_path() {
        let _g = env_lock();
        let empty = temp_dir("run-http");
        std::env::set_var("CBETA_INDEX", &empty);

        let cancel = CancellationToken::new();
        let cancel_bg = cancel.clone();
        let handle =
            std::thread::spawn(move || run_http_with_cancel("127.0.0.1:0", Some(cancel_bg)));

        // Give the server a moment to bind; then cancel.
        std::thread::sleep(Duration::from_millis(200));
        cancel.cancel();
        let code = handle.join().expect("join");
        assert_eq!(code, 0);

        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn serve_http_post_search_paths_with_cancel() {
        let _g = env_lock();
        let empty = temp_dir("empty");
        std::env::set_var("CBETA_INDEX", &empty);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let local = listener.local_addr().unwrap();
        let addr = format!("127.0.0.1:{}", local.port());
        let hosts = allowed_hosts_for(local);
        let cancel = CancellationToken::new();
        let cancel_bg = cancel.clone();

        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(serve_http(listener, None, hosts, cancel_bg))
                .unwrap();
        });

        // Wait until listening (brief spin).
        for _ in 0..50 {
            if TcpStream::connect(&addr).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let (st, body) = http_post(&addr, r#"{"q":"真性有为空"}"#);
        assert_eq!(st, 503, "{body}");
        assert!(body.contains("no index"));

        let (st, body) = http_post(&addr, "not-json");
        assert_eq!(st, 400, "{body}");

        cancel.cancel();
        handle.join().unwrap();
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn serve_http_search_hit_with_mini_index() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("idx");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
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

        let open = OpenIndex::open(&index).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let local = listener.local_addr().unwrap();
        let addr = format!("127.0.0.1:{}", local.port());
        let hosts = allowed_hosts_for(local);
        let cancel = CancellationToken::new();
        let cancel_bg = cancel.clone();

        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(serve_http(listener, Some(Arc::new(open)), hosts, cancel_bg))
                .unwrap();
        });

        for _ in 0..50 {
            if TcpStream::connect(&addr).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let (st, body) = http_post(&addr, r#"{"q":"真性有为空"}"#);
        assert_eq!(st, 200, "{body}");
        assert!(body.contains("hits"));
        assert!(body.contains("T30n1578_p0268b21") || body.contains("line_id"));

        let (st, body) = http_post(&addr, r#"{"q":""}"#);
        assert!(st == 400 || st == 500, "{st} {body}");

        cancel.cancel();
        handle.join().unwrap();
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }
}
