//! L-http product contracts: `cbeta serve --http` REST `POST /search` only.
//!
//! Surface: `POST /search` + nest `/mcp` (StreamableHttp — not stdio JSON-RPC).
//! Non-surface REST (`/verify`, `/get`, GET `/search`) must not be product 200.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
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
fn thirty_two_concurrent_post_search_line_id() {
    // Given: one serve process on ephemeral port (mini T0235+T1578)
    const CHILD_TIMEOUT: Duration = Duration::from_secs(30);
    let (corpus, index) = built();
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");
    let port = child.wait_until_listening(READY_TIMEOUT);

    let body = serde_json::to_string(&json!({ "q": QUERY })).expect("serialize");
    let body = Arc::new(body);
    let wall = Instant::now();

    // When: 32 parallel POST /search (same body shape as SearchArgs)
    let mut handles = Vec::with_capacity(32);
    for i in 0..32 {
        let b = Arc::clone(&body);
        handles.push(thread::spawn(move || {
            let (status, resp) = http_post_json(port, "/search", &b);
            assert_eq!(status, 200, "concurrent POST #{i} must be 200; body={resp}");
            let v: Value = serde_json::from_str(resp.trim())
                .unwrap_or_else(|e| panic!("search json #{i}: {e}; body={resp}"));
            let hits = v["hits"]
                .as_array()
                .unwrap_or_else(|| panic!("expected hits array #{i}; got {v}"));
            // 200 + empty hits is a misleading success for QUERY 真性有为空
            assert!(
                !hits.is_empty(),
                "empty hits FAIL even if 200; #{i} got {v}"
            );
            assert!(
                hits.iter()
                    .any(|h| h["line_id"].as_str() == Some(EXPECT_LINE_ID)),
                "expected line_id {EXPECT_LINE_ID} in #{i}; got {v}"
            );
        }));
    }

    // Then: every worker returns EXPECT_LINE_ID; hang >30s fails
    for (i, h) in handles.into_iter().enumerate() {
        h.join().unwrap_or_else(|e| panic!("join #{i}: {e:?}"));
    }
    assert!(
        wall.elapsed() <= CHILD_TIMEOUT,
        "32 concurrent POST /search hung > {CHILD_TIMEOUT:?}; elapsed={:?}",
        wall.elapsed()
    );
}

#[test]
fn thirty_two_concurrent_post_search_records_elapsed() {
    // Given: one serve process on ephemeral port (mini T0235+T1578)
    // Records wall-clock elapsed only — no numeric SLO assert.
    const CHILD_TIMEOUT: Duration = Duration::from_secs(30);
    let (corpus, index) = built();
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");
    let port = child.wait_until_listening(READY_TIMEOUT);

    let body = serde_json::to_string(&json!({ "q": QUERY })).expect("serialize");
    let body = Arc::new(body);
    let wall = Instant::now();

    // When: 32 parallel POST /search (same body shape as SearchArgs)
    let mut handles = Vec::with_capacity(32);
    for i in 0..32 {
        let b = Arc::clone(&body);
        handles.push(thread::spawn(move || {
            let (status, resp) = http_post_json(port, "/search", &b);
            assert_eq!(status, 200, "concurrent POST #{i} must be 200; body={resp}");
            let v: Value = serde_json::from_str(resp.trim())
                .unwrap_or_else(|e| panic!("search json #{i}: {e}; body={resp}"));
            let hits = v["hits"]
                .as_array()
                .unwrap_or_else(|| panic!("expected hits array #{i}; got {v}"));
            // 200 + empty hits is a misleading success for QUERY 真性有为空
            assert!(
                !hits.is_empty(),
                "empty hits FAIL even if 200; #{i} got {v}"
            );
            assert!(
                hits.iter()
                    .any(|h| h["line_id"].as_str() == Some(EXPECT_LINE_ID)),
                "expected line_id {EXPECT_LINE_ID} in #{i}; got {v}"
            );
        }));
    }

    // Then: every worker returns EXPECT_LINE_ID; hang >30s fails; record elapsed
    for (i, h) in handles.into_iter().enumerate() {
        h.join().unwrap_or_else(|e| panic!("join #{i}: {e:?}"));
    }
    let elapsed = wall.elapsed();
    eprintln!("notes: thirty_two_concurrent_post_search_records_elapsed wall_elapsed={elapsed:?}");
    assert!(
        elapsed <= CHILD_TIMEOUT,
        "32 concurrent POST /search hung > {CHILD_TIMEOUT:?}; elapsed={elapsed:?}"
    );
}

/// POST `/mcp` StreamableHttp: Accept must include both JSON and SSE (unlike REST `http_post_json`).
/// Returns (status, lowercased response headers, body). For SSE, body is the first `data:` JSON
/// whose `id` matches the request (stops before keep-alive hang).
fn mcp_http_post(
    port: u16,
    session_id: Option<&str>,
    body: &str,
) -> (u16, Vec<(String, String)>, String) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream =
        TcpStream::connect_timeout(&addr.parse().expect("socket addr"), Duration::from_secs(2))
            .unwrap_or_else(|e| panic!("mcp connect {addr}: {e}"));
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("mcp read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("mcp write timeout");

    let mut req = format!(
        "POST /mcp HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Content-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n",
        body.len()
    );
    if let Some(sid) = session_id {
        req.push_str(&format!("mcp-session-id: {sid}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    stream
        .write_all(req.as_bytes())
        .unwrap_or_else(|e| panic!("mcp write: {e}"));

    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    let header_end = loop {
        match stream.read(&mut buf) {
            Ok(0) => panic!(
                "mcp EOF before headers; got={}",
                String::from_utf8_lossy(&raw)
            ),
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if let Some(pos) = find_http_header_end(&raw) {
                    break pos;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                if let Some(pos) = find_http_header_end(&raw) {
                    break pos;
                }
                panic!(
                    "mcp header read timeout; partial={}",
                    String::from_utf8_lossy(&raw)
                );
            }
            Err(e) => panic!("mcp read headers: {e}"),
        }
    };

    let head = String::from_utf8_lossy(&raw[..header_end]);
    let (status, headers) = parse_http_status_headers(&head);
    let mut body_raw = raw[header_end..].to_vec();

    let req_id = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("id").cloned());

    let ct = header_value(&headers, "content-type").unwrap_or("");
    if ct.starts_with("text/event-stream") {
        // SSE keep-alive would hang read_to_end; stop once matching JSON-RPC id arrives.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(payload) = sse_json_matching_id(&body_raw, &headers, req_id.as_ref()) {
                // Drop stream to cancel server keep-alive.
                drop(stream);
                return (status, headers, payload);
            }
            if Instant::now() >= deadline {
                panic!(
                    "mcp SSE hang waiting for id={req_id:?}; partial={}",
                    String::from_utf8_lossy(&body_raw)
                );
            }
            match stream.read(&mut buf) {
                Ok(0) => {
                    if let Some(payload) =
                        sse_json_matching_id(&body_raw, &headers, req_id.as_ref())
                    {
                        return (status, headers, payload);
                    }
                    if req_id.is_none() {
                        return (status, headers, String::new());
                    }
                    panic!(
                        "mcp SSE EOF without matching id={req_id:?}; body={}",
                        String::from_utf8_lossy(&body_raw)
                    );
                }
                Ok(n) => body_raw.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    continue;
                }
                Err(e) => panic!("mcp SSE read: {e}"),
            }
        }
    }

    // JSON or empty (notifications → 202).
    let content_len =
        header_value(&headers, "content-length").and_then(|s| s.parse::<usize>().ok());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(n) = content_len {
            if body_raw.len() >= n {
                break;
            }
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => body_raw.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                if content_len.is_none() || Instant::now() >= deadline {
                    break;
                }
            }
            Err(e) => panic!("mcp body read: {e}"),
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    drop(stream);
    let body_str = String::from_utf8_lossy(&body_raw).into_owned();
    (status, headers, body_str)
}

fn find_http_header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .or_else(|| raw.windows(2).position(|w| w == b"\n\n").map(|p| p + 2))
}

fn parse_http_status_headers(head: &str) -> (u16, Vec<(String, String)>) {
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("bad MCP status line `{status_line}`"));
    let mut headers = Vec::new();
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    (status, headers)
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Decode complete HTTP chunked frames; incomplete tail is ignored (caller reads more).
fn decode_chunked_prefix(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let Some(rel) = input[i..].windows(2).position(|w| w == b"\r\n") else {
            break;
        };
        let size_line = &input[i..i + rel];
        let size_str = match std::str::from_utf8(size_line) {
            Ok(s) => s.split(';').next().unwrap_or("").trim(),
            Err(_) => break,
        };
        let Ok(size) = usize::from_str_radix(size_str, 16) else {
            break;
        };
        i += rel + 2;
        if size == 0 {
            break;
        }
        if i + size > input.len() {
            break;
        }
        out.extend_from_slice(&input[i..i + size]);
        i += size;
        if i + 2 <= input.len() && &input[i..i + 2] == b"\r\n" {
            i += 2;
        }
    }
    out
}

fn sse_payload_bytes(body_raw: &[u8], headers: &[(String, String)]) -> Vec<u8> {
    let te = header_value(headers, "transfer-encoding").unwrap_or("");
    if te.to_ascii_lowercase().contains("chunked") {
        let decoded = decode_chunked_prefix(body_raw);
        if !decoded.is_empty() {
            return decoded;
        }
    }
    body_raw.to_vec()
}

fn sse_json_matching_id(
    body_raw: &[u8],
    headers: &[(String, String)],
    req_id: Option<&Value>,
) -> Option<String> {
    let payload = sse_payload_bytes(body_raw, headers);
    let text = String::from_utf8_lossy(&payload);
    for line in text.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        match req_id {
            None => return Some(data.to_string()),
            Some(id) if v.get("id") == Some(id) => return Some(data.to_string()),
            _ => {}
        }
    }
    None
}

fn mcp_session_id_from(headers: &[(String, String)]) -> String {
    header_value(headers, "mcp-session-id")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| panic!("missing non-empty mcp-session-id; headers={headers:?}"))
        .to_string()
}

fn mcp_initialize_session(port: u16) -> String {
    let init_body = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "clientInfo": { "name": "cli_http", "version": "0" },
            "capabilities": {}
        }
    }))
    .expect("serialize initialize");
    let (st, hdrs, body) = mcp_http_post(port, None, &init_body);
    assert_eq!(st, 200, "initialize must be 200; body={body}");
    let sid = mcp_session_id_from(&hdrs);
    let v: Value = serde_json::from_str(body.trim())
        .unwrap_or_else(|e| panic!("initialize JSON: {e}; body={body}"));
    assert!(v.get("result").is_some(), "initialize missing result: {v}");

    let note = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }))
    .expect("serialize initialized");
    let (st_n, _, body_n) = mcp_http_post(port, Some(&sid), &note);
    assert!(
        st_n == 200 || st_n == 202,
        "notifications/initialized must be 200|202; got {st_n}; body={body_n}"
    );
    sid
}

fn mcp_extract_tool_payload(resp: &Value) -> Value {
    let result = resp
        .get("result")
        .unwrap_or_else(|| panic!("tools/call no result: {resp}"));
    if let Some(sc) = result.get("structuredContent") {
        if !sc.is_null() {
            return sc.clone();
        }
    }
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .unwrap_or_else(|| panic!("tools/call no content: {resp}"));
    let text = content
        .iter()
        .find_map(|c| c.get("text").and_then(|t| t.as_str()))
        .unwrap_or_else(|| panic!("tools/call no text: {resp}"));
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tools/call text not JSON: {e}; text={text}; full={resp}"))
}

fn mcp_assert_search_line_id(payload: &Value, label: &str) {
    let hits = payload["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("{label}: expected hits array; got {payload}"));
    assert!(
        !hits.is_empty(),
        "{label}: empty hits FAIL even if 200; got {payload}"
    );
    assert!(
        hits.iter()
            .any(|h| h["line_id"].as_str() == Some(EXPECT_LINE_ID)),
        "{label}: expected line_id {EXPECT_LINE_ID}; got {payload}"
    );
}

#[test]
fn http_mcp_two_sessions_overlap() {
    // Given: one HTTP serve (REST + nest /mcp StreamableHttp + LocalSessionManager)
    const CHILD_TIMEOUT: Duration = Duration::from_secs(30);
    let (corpus, index) = built();
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");
    let port = child.wait_until_listening(READY_TIMEOUT);
    let wall = Instant::now();

    // When: two independent MCP sessions (distinct mcp-session-id)
    let sid_a = mcp_initialize_session(port);
    let sid_b = mcp_initialize_session(port);
    assert_ne!(
        sid_a, sid_b,
        "two sessions must get distinct mcp-session-id; both={sid_a}"
    );
    eprintln!("notes: http_mcp_two_sessions_overlap sid_a={sid_a} sid_b={sid_b}");

    let call_body = |id: u64| {
        serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": "cbeta_search",
                "arguments": { "q": QUERY }
            }
        }))
        .expect("serialize tools/call")
    };
    let body_a = call_body(10);
    let body_b = call_body(20);

    // Overlap the two tools/call in flight
    let h_a = thread::spawn(move || {
        let (st, _, body) = mcp_http_post(port, Some(&sid_a), &body_a);
        assert_eq!(st, 200, "session A tools/call must be 200; body={body}");
        let resp: Value = serde_json::from_str(body.trim())
            .unwrap_or_else(|e| panic!("session A JSON: {e}; body={body}"));
        assert!(
            resp.get("error").is_none(),
            "session A tools/call error: {resp}"
        );
        let payload = mcp_extract_tool_payload(&resp);
        mcp_assert_search_line_id(&payload, "session A");
        sid_a
    });
    let h_b = thread::spawn(move || {
        let (st, _, body) = mcp_http_post(port, Some(&sid_b), &body_b);
        assert_eq!(st, 200, "session B tools/call must be 200; body={body}");
        let resp: Value = serde_json::from_str(body.trim())
            .unwrap_or_else(|e| panic!("session B JSON: {e}; body={body}"));
        assert!(
            resp.get("error").is_none(),
            "session B tools/call error: {resp}"
        );
        let payload = mcp_extract_tool_payload(&resp);
        mcp_assert_search_line_id(&payload, "session B");
        sid_b
    });

    // Then: both return EXPECT_LINE_ID; hang >30s fails
    let got_a = h_a.join().unwrap_or_else(|e| panic!("join A: {e:?}"));
    let got_b = h_b.join().unwrap_or_else(|e| panic!("join B: {e:?}"));
    assert_ne!(
        got_a, got_b,
        "session ids must remain distinct after overlap"
    );
    assert!(
        wall.elapsed() <= CHILD_TIMEOUT,
        "http_mcp_two_sessions_overlap hung > {CHILD_TIMEOUT:?}; elapsed={:?}",
        wall.elapsed()
    );
}

#[test]
fn http_search_stale_after_current_swap() {
    // Given: one serve process on ephemeral port (mini T0235+T1578)
    const CHILD_TIMEOUT: Duration = Duration::from_secs(30);
    let (corpus, index) = built();
    let mut child = HttpChild::spawn_http(&corpus, &index, "127.0.0.1:0");
    let port = child.wait_until_listening(READY_TIMEOUT);
    let wall = Instant::now();

    let body = serde_json::to_string(&json!({ "q": QUERY })).expect("serialize");

    // When: baseline POST /search must hit EXPECT_LINE_ID + cbeta_tag
    let (code1, body1) = http_post_json(port, "/search", &body);
    assert_eq!(
        code1, 200,
        "baseline POST /search must be 200; body={body1}"
    );
    let v1: Value = serde_json::from_str(body1.trim())
        .unwrap_or_else(|e| panic!("baseline search json: {e}; body={body1}"));
    let hits1 = v1["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("baseline expected hits array; got {v1}"));
    assert!(
        !hits1.is_empty(),
        "baseline empty hits FAIL even if 200; got {v1}"
    );
    let hit1 = hits1
        .iter()
        .find(|h| h["line_id"].as_str() == Some(EXPECT_LINE_ID))
        .unwrap_or_else(|| panic!("baseline missing {EXPECT_LINE_ID}; got {v1}"));
    let tag1 = hit1["cbeta_tag"]
        .as_str()
        .unwrap_or_else(|| panic!("baseline hit missing cbeta_tag: {hit1}"))
        .to_string();
    assert!(!tag1.is_empty(), "baseline cbeta_tag empty: {hit1}");

    // When: CURRENT → ghost + rename real artifact away (blocks sole-child reopen)
    let current_path = index.join("CURRENT");
    let current_name = std::fs::read_to_string(&current_path)
        .unwrap_or_else(|e| panic!("read CURRENT {}: {e}", current_path.display()))
        .trim()
        .to_string();
    assert!(
        !current_name.is_empty(),
        "CURRENT empty under {}",
        index.display()
    );
    let artifact_dir = index.join(&current_name);
    assert!(
        artifact_dir.is_dir(),
        "artifact dir missing: {}",
        artifact_dir.display()
    );

    std::fs::write(&current_path, "ghost-artifact-not-present\n")
        .unwrap_or_else(|e| panic!("rewrite CURRENT {} to ghost: {e}", current_path.display()));
    let hidden = index.join(format!("{current_name}.hidden-for-stale-test"));
    std::fs::rename(&artifact_dir, &hidden).unwrap_or_else(|e| {
        panic!(
            "rename artifact {} -> {}: {e}",
            artifact_dir.display(),
            hidden.display()
        )
    });

    // Then: post-swap POST still returns same line_id + cbeta_tag (stale OpenIndex)
    let (code2, body2) = http_post_json(port, "/search", &body);
    assert_eq!(
        code2, 200,
        "post-swap POST /search must be 200 (hot-reload empty/error = FAIL); body={body2}"
    );
    let v2: Value = serde_json::from_str(body2.trim())
        .unwrap_or_else(|e| panic!("post-swap search json: {e}; body={body2}"));
    let hits2 = v2["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("post-swap expected hits array; got {v2}"));
    assert!(
        !hits2.is_empty(),
        "post-swap empty hits FAIL (stale index must still serve); got {v2}"
    );
    let hit2 = hits2
        .iter()
        .find(|h| h["line_id"].as_str() == Some(EXPECT_LINE_ID))
        .unwrap_or_else(|| panic!("post-swap missing {EXPECT_LINE_ID}; got {v2}"));
    let tag2 = hit2["cbeta_tag"]
        .as_str()
        .unwrap_or_else(|| panic!("post-swap hit missing cbeta_tag: {hit2}"));
    assert_eq!(
        tag2, tag1,
        "post-swap cbeta_tag drifted (want baseline {tag1}): {hit2}"
    );

    assert!(
        wall.elapsed() <= CHILD_TIMEOUT,
        "http_search_stale_after_current_swap hung > {CHILD_TIMEOUT:?}; elapsed={:?}",
        wall.elapsed()
    );
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
