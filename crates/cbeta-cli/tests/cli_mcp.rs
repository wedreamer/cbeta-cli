//! MCP stdio contract: tools/list five tools + tool call smoke tests.
//!
//! Handshake: spawn `cbeta serve` with piped stdio, JSON-RPC initialize +
//! tools/list (and tools/call). Kill the child; never leave it hanging.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use common::{cbeta_env, mini_corpus, temp_dir};
use serde_json::{json, Value};

const TOOL_NAMES: [&str; 5] = [
    "cbeta_search",
    "cbeta_verify_quote",
    "cbeta_get_passage",
    "cbeta_list_catalog",
    "cbeta_index_info",
];

const CHILD_TIMEOUT: Duration = Duration::from_secs(30);

struct McpChild {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    started: Instant,
}

impl McpChild {
    fn spawn(corpus: &std::path::Path, index: &std::path::Path) -> Self {
        let mut cmd = cbeta_env(corpus, index);
        cmd.arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn cbeta serve");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            started: Instant::now(),
        }
    }

    fn ensure_alive(&mut self) {
        if self.started.elapsed() > CHILD_TIMEOUT {
            let _ = self.child.kill();
            panic!("cbeta serve exceeded {CHILD_TIMEOUT:?}");
        }
        if let Some(status) = self.child.try_wait().expect("try_wait") {
            let mut err = String::new();
            if let Some(mut e) = self.child.stderr.take() {
                use std::io::Read;
                let _ = e.read_to_string(&mut err);
            }
            panic!("cbeta serve exited early: {status}; stderr={err}");
        }
    }

    fn write_line(&mut self, v: &Value) {
        self.ensure_alive();
        let line = serde_json::to_string(v).expect("serialize rpc");
        writeln!(self.stdin, "{line}").expect("write stdin");
        self.stdin.flush().expect("flush stdin");
    }

    fn read_response(&mut self, expect_id: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.ensure_alive();
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("timeout waiting for JSON-RPC id={expect_id}");
            }
            let mut line = String::new();
            // Blocking read; bounded by overall CHILD_TIMEOUT via ensure_alive on retries.
            match self.stdout.read_line(&mut line) {
                Ok(0) => {
                    let _ = self.child.kill();
                    panic!("EOF from cbeta serve before id={expect_id}");
                }
                Ok(_) => {
                    let t = line.trim();
                    if t.is_empty() {
                        continue;
                    }
                    let v: Value = serde_json::from_str(t)
                        .unwrap_or_else(|e| panic!("bad JSON-RPC line `{t}`: {e}"));
                    // Skip notifications (no id).
                    if v.get("id").and_then(|i| i.as_u64()) == Some(expect_id) {
                        return v;
                    }
                    if v.get("id").and_then(|i| i.as_i64()) == Some(expect_id as i64) {
                        return v;
                    }
                }
                Err(e) => {
                    let _ = self.child.kill();
                    panic!("read stdout: {e}");
                }
            }
        }
    }

    fn initialize(&mut self) {
        self.write_line(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "cli_mcp", "version": "0.0.1" }
            }
        }));
        let resp = self.read_response(1);
        assert!(resp.get("result").is_some(), "initialize failed: {resp}");
        self.write_line(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }));
    }

    fn tools_list(&mut self) -> Vec<String> {
        self.write_line(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }));
        let resp = self.read_response(2);
        let tools = resp["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("tools/list missing tools: {resp}"));
        tools
            .iter()
            .map(|t| {
                t["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("tool missing name: {t}"))
                    .to_string()
            })
            .collect()
    }

    fn call_tool(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.write_line(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }));
        let resp = self.read_response(id);
        assert!(
            resp.get("error").is_none(),
            "tools/call {name} error: {resp}"
        );
        let result = resp
            .get("result")
            .unwrap_or_else(|| panic!("tools/call {name} no result: {resp}"));
        // Prefer structuredContent; else parse first text content block as JSON.
        if let Some(sc) = result.get("structuredContent") {
            if !sc.is_null() {
                return sc.clone();
            }
        }
        let content = result
            .get("content")
            .and_then(|c| c.as_array())
            .unwrap_or_else(|| panic!("tools/call {name} no content: {resp}"));
        let text = content
            .iter()
            .find_map(|c| c.get("text").and_then(|t| t.as_str()))
            .unwrap_or_else(|| panic!("tools/call {name} no text: {resp}"));
        serde_json::from_str(text).unwrap_or_else(|e| {
            panic!("tools/call {name} text not JSON: {e}; text={text}; full={resp}")
        })
    }
}

impl Drop for McpChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn build_mini(corpus: &std::path::Path, index: &std::path::Path) {
    let out = cbeta_env(corpus, index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .expect("build");
    assert_eq!(
        out.status.code(),
        Some(0),
        "build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn tools_list_exactly_five() {
    // Given: mini index + cbeta serve on stdio
    let corpus = mini_corpus();
    let index = temp_dir("mcp-list");
    build_mini(&corpus, &index);
    let mut mcp = McpChild::spawn(&corpus, &index);

    // When: initialize + tools/list
    mcp.initialize();
    let mut names = mcp.tools_list();
    names.sort();
    let mut expected = TOOL_NAMES.map(str::to_string).to_vec();
    expected.sort();

    // Then: exactly the five product tools (serve is the process, not a tool)
    assert_eq!(names, expected, "tools/list names mismatch");
    let _ = index;
}

#[test]
fn search_clauses_near() {
    // Given: mini index; MCP search with mode=near + clauses (agent twin of q DSL)
    let corpus = mini_corpus();
    let index = temp_dir("mcp-search");
    build_mini(&corpus, &index);
    let mut mcp = McpChild::spawn(&corpus, &index);
    mcp.initialize();

    // When: cbeta_search mode=near clauses=["真性","有為"]
    let body = mcp.call_tool(
        10,
        "cbeta_search",
        json!({
            "mode": "near",
            "clauses": ["真性", "有為"]
        }),
    );

    // Then: hits include the real T1578 b21 line
    let hits = body["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("expected hits: {body}"));
    assert!(
        hits.iter().any(|h| h["line_id"] == "T30n1578_p0268b21"),
        "expected b21 hit; body={body}"
    );
}

#[test]
fn get_passage_b21() {
    // Given: mini index
    let corpus = mini_corpus();
    let index = temp_dir("mcp-get");
    build_mini(&corpus, &index);
    let mut mcp = McpChild::spawn(&corpus, &index);
    mcp.initialize();

    // When: cbeta_get_passage line_id=b21 (real verse, not ghost a12)
    let body = mcp.call_tool(
        20,
        "cbeta_get_passage",
        json!({
            "line_id": "T30n1578_p0268b21",
            "action": "get"
        }),
    );

    // Then: hit.line_id is b21
    assert_eq!(
        body["hit"]["line_id"], "T30n1578_p0268b21",
        "get body={body}"
    );
}

#[test]
fn verify_quote_json() {
    // Given: mini index
    let corpus = mini_corpus();
    let index = temp_dir("mcp-verify");
    build_mini(&corpus, &index);
    let mut mcp = McpChild::spawn(&corpus, &index);
    mcp.initialize();

    // When: cbeta_verify_quote with exact traditional verse
    let body = mcp.call_tool(
        30,
        "cbeta_verify_quote",
        json!({
            "q": "真性有為空，如幻緣生故"
        }),
    );

    // Then: VerifyReport shape, is_original true, exact_hit b21
    assert_eq!(body["is_original"], true, "verify body={body}");
    assert_eq!(
        body["exact_hit"]["line_id"], "T30n1578_p0268b21",
        "verify body={body}"
    );
}

// Silence unused import warning if Command is only used via cbeta_env path.
#[allow(dead_code)]
fn _bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cbeta"))
}
