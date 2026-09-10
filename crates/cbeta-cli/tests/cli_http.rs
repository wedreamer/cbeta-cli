//! L-http product contracts: `cbeta serve --http` REST `POST /search` only.
//!
//! Surface: `POST /search` + nest `/mcp` (StreamableHttp — not stdio JSON-RPC).
//! Non-surface REST (`/verify`, `/get`, GET `/search`) must not be product 200.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const QUERY: &str = "真性有为空";
const NO_HIT_Q: &str = "xyzzy-not-in-corpus";
const EXPECT_LINE_ID: &str = "T30n1578_p0268b21";
const HIT_KEYS: [&str; 9] = [
    "line_id",
    "work_id",
    "title",
    "author",
    "juan",
    "text_raw",
    "citation",
    "score",
    "cbeta_tag",
];

const READY_TIMEOUT: Duration = Duration::from_secs(5);

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("http");
    let build = cbeta_env(&corpus, &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("build: {e}"));
    assert_eq!(
        build.status.code(),
        Some(0),
        "build: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    (corpus, index)
}

fn run(corpus: &Path, index: &Path, args: &[&str]) -> Output {
    cbeta_env(corpus, index)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn cbeta {args:?}: {e}"))
}

fn stdout_utf8(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap_or_else(|e| panic!("stdout utf-8: {e}"))
}

fn stderr_utf8(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap_or_else(|e| panic!("stderr utf-8: {e}"))
}

/// RAII child: always kill + wait on drop so tests never leave orphans.
struct HttpChild {
    child: Child,
    /// Captured stderr lines (shared with a reader thread when spawned long-lived).
    stderr_buf: Arc<Mutex<String>>,
}

impl HttpChild {
    /// Spawn `cbeta serve --http <addr>` with piped stderr; kill on drop.
    fn spawn_http(corpus: &Path, index: &Path, addr: &str) -> Self {
        let mut cmd = cbeta_env(corpus, index);
        cmd.args(["serve", "--http", addr])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .unwrap_or_else(|e| panic!("spawn cbeta serve --http {addr}: {e}"));
        let stderr_buf = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let buf = Arc::clone(&stderr_buf);
            thread::spawn(move || {
                // Line-buffered so wait_until_listening sees `listening on` before EOF.
                let r = BufReader::new(stderr);
                for line in r.lines() {
                    let Ok(line) = line else { break };
                    if let Ok(mut g) = buf.lock() {
                        g.push_str(&line);
                        g.push('\n');
                    }
                }
            });
        }
        Self { child, stderr_buf }
    }

    fn stderr_snapshot(&self) -> String {
        self.stderr_buf
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn wait_until_listening(&mut self, limit: Duration) -> u16 {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                let err = self.stderr_snapshot();
                panic!(
                    "cbeta serve --http exited before listen: {status}; stderr={err}\n\
                     (RED expected today: unknown --http / no HTTP server)"
                );
            }
            let err = self.stderr_snapshot();
            if let Some(port) = parse_listen_port(&err) {
                // Confirm accept path is open.
                if TcpStream::connect_timeout(
                    &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                    Duration::from_millis(100),
                )
                .is_ok()
                {
                    return port;
                }
            }
            if start.elapsed() >= limit {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!(
                    "timeout waiting for serve --http listen within {limit:?}; stderr={}",
                    self.stderr_snapshot()
                );
            }
            thread::sleep(Duration::from_millis(30));
        }
    }
}

impl Drop for HttpChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Extract bound port from server stderr (e.g. `listening on 127.0.0.1:54321`).
fn parse_listen_port(stderr: &str) -> Option<u16> {
    for token in stderr.split_whitespace() {
        // Strip trailing punctuation.
        let t = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != ':');
        if let Some(rest) = t.strip_prefix("127.0.0.1:") {
            if let Ok(p) = rest.parse::<u16>() {
                if p > 0 {
                    return Some(p);
                }
            }
        }
    }
    // Fallback: bare `:PORT` near "listen".
    if stderr.to_lowercase().contains("listen") {
        for part in stderr.split(|c: char| !c.is_ascii_digit()) {
            if let Ok(p) = part.parse::<u16>() {
                if p >= 1024 {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Minimal HTTP/1.1 request; returns (status, body).
fn http_exchange(port: u16, method: &str, path: &str, body: Option<&str>) -> (u16, String) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream =
        TcpStream::connect_timeout(&addr.parse().expect("socket addr"), Duration::from_secs(2))
            .unwrap_or_else(|e| panic!("connect {addr}: {e}"));
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("write timeout");

    let req = match body {
        Some(b) => format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        ),
        None => format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        ),
    };
    stream
        .write_all(req.as_bytes())
        .unwrap_or_else(|e| panic!("write request: {e}"));

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .unwrap_or_else(|e| panic!("read response: {e}"));
    let text = String::from_utf8_lossy(&raw);
    parse_http_response(&text)
}

/// Minimal HTTP/1.1 POST JSON; returns (status, body).
fn http_post_json(port: u16, path: &str, body: &str) -> (u16, String) {
    http_exchange(port, "POST", path, Some(body))
}

/// Minimal HTTP/1.1 GET; returns (status, body).
fn http_get(port: u16, path: &str) -> (u16, String) {
    http_exchange(port, "GET", path, None)
}

fn parse_http_response(raw: &str) -> (u16, String) {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .or_else(|| raw.split_once("\n\n"))
        .unwrap_or_else(|| panic!("no HTTP header/body split; raw={raw}"));
    let status_line = head.lines().next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("bad status line `{status_line}`; raw={raw}"));
    (status, body.to_string())
}

fn assert_hit_product_fields(hit: &Value) {
    for key in HIT_KEYS {
        assert!(hit.get(key).is_some(), "missing hit field `{key}` in {hit}");
    }
}

#[test]
fn serve_http_bad_addr_exits_2() {
    // Given: mini index (HTTP serve still needs corpus/index env isolation)
    let (corpus, index) = built();

    // When: --http without host (port-only bare number)
    let out_num = run(&corpus, &index, &["serve", "--http", "1873"]);
    let stdout_num = stdout_utf8(&out_num);
    let stderr_num = stderr_utf8(&out_num);

    // Then: usage / bad-addr exit 2 (also true today when --http is unknown)
    assert_eq!(
        out_num.status.code(),
        Some(2),
        "serve --http 1873 must exit 2; stdout={stdout_num} stderr={stderr_num}"
    );

    // When: host-less `:port`
    let out_colon = run(&corpus, &index, &["serve", "--http", ":1873"]);
    let stdout_colon = stdout_utf8(&out_colon);
    let stderr_colon = stderr_utf8(&out_colon);

    // Then: exit 2
    assert_eq!(
        out_colon.status.code(),
        Some(2),
        "serve --http :1873 must exit 2; stdout={stdout_colon} stderr={stderr_colon}"
    );
}

#[test]
fn post_search_matches_cli_json_hit_fields() {
    // Given: mini index (T0235+T1578 only) + HTTP serve on ephemeral port
    let (corpus, index) = built();
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");

    // When: server becomes ready (RED today: unknown --http → immediate exit)
    let port = child.wait_until_listening(READY_TIMEOUT);

    // When: POST /search with known mini hit query
    let body = serde_json::to_string(&json!({ "q": QUERY })).expect("serialize");
    let (status, resp_body) = http_post_json(port, "/search", &body);
    assert_eq!(
        status, 200,
        "POST /search hit must be 200; body={resp_body}"
    );
    let v: Value = serde_json::from_str(resp_body.trim())
        .unwrap_or_else(|e| panic!("search json: {e}; body={resp_body}"));
    let hits = v["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("expected hits array; got {v}"));
    assert!(!hits.is_empty(), "expected non-empty hits; got {v}");
    for h in hits {
        assert_hit_product_fields(h);
        // Mini only — never assert T1585.
        let lid = h["line_id"].as_str().unwrap_or("");
        assert!(
            !lid.contains("T1585") && !lid.contains("n1585"),
            "mini must not yield T1585; hit={h}"
        );
    }
    assert!(
        hits.iter()
            .any(|h| h["line_id"].as_str() == Some(EXPECT_LINE_ID)),
        "expected line_id {EXPECT_LINE_ID}; got {v}"
    );

    // When: no-hit query
    let empty_body = serde_json::to_string(&json!({ "q": NO_HIT_Q })).expect("serialize");
    let (empty_status, empty_resp) = http_post_json(port, "/search", &empty_body);
    assert_eq!(
        empty_status, 200,
        "empty hits still HTTP 200; body={empty_resp}"
    );
    let empty_v: Value = serde_json::from_str(empty_resp.trim())
        .unwrap_or_else(|e| panic!("empty json: {e}; body={empty_resp}"));
    let empty_hits = empty_v["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("expected hits array; got {empty_v}"));
    assert!(
        empty_hits.is_empty(),
        "no-hit must be hits:[]; got {empty_v}"
    );

    // When: bad JSON body
    let (bad_status, bad_resp) = http_post_json(port, "/search", "{not-json");
    assert_eq!(bad_status, 400, "bad JSON must be 400; body={bad_resp}");
}

#[test]
fn thirty_two_concurrent_post_search_all_200() {
    // Given: one serve process on ephemeral port
    let (corpus, index) = built();
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");
    let port = child.wait_until_listening(READY_TIMEOUT);

    let body = serde_json::to_string(&json!({ "q": QUERY })).expect("serialize");
    let body = Arc::new(body);

    // When: 32 parallel POST /search
    let mut handles = Vec::with_capacity(32);
    for i in 0..32 {
        let b = Arc::clone(&body);
        handles.push(thread::spawn(move || {
            let (status, resp) = http_post_json(port, "/search", &b);
            assert_eq!(status, 200, "concurrent POST #{i} must be 200; body={resp}");
            status
        }));
    }

    // Then: all 200
    for (i, h) in handles.into_iter().enumerate() {
        let status = h.join().unwrap_or_else(|e| panic!("join #{i}: {e:?}"));
        assert_eq!(status, 200, "worker #{i}");
    }
}

#[test]
fn serve_help_documents_http_flag() {
    // Given: any env (help does not need index)
    let corpus = mini_corpus();
    let index = temp_dir("http-help");

    // When: cbeta serve --help
    let out = run(&corpus, &index, &["serve", "--help"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: documents --http
    assert_eq!(
        out.status.code(),
        Some(0),
        "serve --help exit 0; stderr={stderr}"
    );
    assert!(
        stdout.contains("--http"),
        "serve --help must document --http; got:\n{stdout}"
    );
}

#[test]
fn post_search_no_index_binary_is_503() {
    // Given: mini corpus env but NO build (empty index root)
    // Momus r2: empty body → Axum 400; 503 needs JSON Content-Type + {"q":…}
    let corpus = mini_corpus();
    let index = temp_dir("http-no-index");
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");
    let port = child.wait_until_listening(READY_TIMEOUT);

    // When: POST /search with valid SearchArgs JSON (binary path; unit is rest.rs)
    let body = serde_json::to_string(&json!({ "q": QUERY })).expect("serialize");
    let (status, resp_body) = http_post_json(port, "/search", &body);

    // Then: 503 service unavailable
    assert_eq!(
        status, 503,
        "no-index POST /search must be 503; body={resp_body}"
    );
    assert!(
        resp_body.contains("no index") || resp_body.contains("error"),
        "503 body should mention no index; got={resp_body}"
    );
}

#[test]
fn non_surface_rest_not_product_200() {
    // Given: mini index + HTTP serve (surface = POST /search + nest /mcp only)
    let (corpus, index) = built();
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");
    let port = child.wait_until_listening(READY_TIMEOUT);

    let q_body = serde_json::to_string(&json!({ "q": QUERY })).expect("serialize");

    // When: POST /verify
    let (st_verify, body_verify) = http_post_json(port, "/verify", &q_body);
    // Then: not product 200
    assert!(
        st_verify == 404 || st_verify == 405,
        "POST /verify must not be product 200; got {st_verify}; body={body_verify}"
    );
    assert_ne!(st_verify, 200, "POST /verify must not be 200");

    // When: GET /search
    let (st_get, body_get) = http_get(port, "/search");
    // Then: not product 200 (POST-only route)
    assert!(
        st_get == 404 || st_get == 405,
        "GET /search must not be product 200; got {st_get}; body={body_get}"
    );
    assert_ne!(st_get, 200, "GET /search must not be 200");

    // When: POST /get
    let get_body = serde_json::to_string(&json!({ "line_id": EXPECT_LINE_ID })).expect("serialize");
    let (st_post_get, body_post_get) = http_post_json(port, "/get", &get_body);
    // Then: not product 200
    assert!(
        st_post_get == 404 || st_post_get == 405,
        "POST /get must not be product 200; got {st_post_get}; body={body_post_get}"
    );
    assert_ne!(st_post_get, 200, "POST /get must not be 200");
}

// Keep CARGO_BIN_EXE_cbeta referenced for clarity (cbeta_env uses it).
#[allow(dead_code)]
fn _bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cbeta"))
}
