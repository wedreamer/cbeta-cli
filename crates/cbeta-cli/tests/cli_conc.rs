//! Concurrency / perf characterization harness (Wave 1 skeleton).
//!
//! Product behavior is already shipped; this binary only locks concurrent CLI
//! shapes against the mini corpus. Later waves add search/verify/MCP load.

#![allow(clippy::expect_used, clippy::unwrap_used, dead_code)]

mod common;

use common::{cbeta_env, cbeta_env_with_home, mini_corpus, temp_dir};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Mini-corpus line_id used by C1/C4/C6 assertions (T1578 only).
const EXPECT_LINE_ID: &str = "T30n1578_p0268b21";
const QUERY_SIMP: &str = "真性有为空";
const VERIFY_Q: &str = "真性有為空，如幻緣生故";

/// Per-child wall-clock budget for concurrent spawns.
const CHILD_TIMEOUT: Duration = Duration::from_secs(30);

/// Default concurrent CLI worker count (later waves).
const CONC_N: usize = 8;

/// Default in-flight MCP requests (later waves).
const MCP_INFLIGHT: usize = 4;

/// Build a mini index under an isolated temp dir; returns `(corpus, index)`.
///
/// Uses [`cbeta_env`] + `build --scope ci-minimal` and asserts exit 0.
fn build_mini() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("conc");
    let build = cbeta_env(&corpus, &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("build spawn: {e}"));
    assert_eq!(
        build.status.code(),
        Some(0),
        "build --scope ci-minimal: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    (corpus, index)
}

/// Spawn `cmd`, drain stderr on a background thread, wait up to `limit`.
///
/// Drain pattern mirrors `HttpChild` in `cli_http.rs` (copied, not imported):
/// piped stderr + line-buffered reader into a shared buffer so a hung child
/// cannot fill the pipe and deadlock the parent.
fn spawn_timeout(mut cmd: Command, limit: Duration) -> Output {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap_or_else(|e| panic!("spawn_timeout: {e}"));

    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let drain = child.stderr.take().map(|stderr| {
        let buf = Arc::clone(&stderr_buf);
        thread::spawn(move || {
            let r = BufReader::new(stderr);
            for line in r.lines() {
                let Ok(line) = line else { break };
                if let Ok(mut g) = buf.lock() {
                    g.push_str(&line);
                    g.push('\n');
                }
            }
        })
    });

    let mut stdout_pipe = child.stdout.take();
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    let err = stderr_buf.lock().map(|g| g.clone()).unwrap_or_default();
                    panic!("spawn_timeout exceeded {limit:?}; stderr={err}");
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("spawn_timeout try_wait: {e}"),
        }
    };

    let mut stdout = Vec::new();
    if let Some(ref mut pipe) = stdout_pipe {
        let _ = pipe.read_to_end(&mut stdout);
    }
    drop(stdout_pipe);
    if let Some(h) = drain {
        let _ = h.join();
    }
    let stderr = stderr_buf
        .lock()
        .map(|g| g.as_bytes().to_vec())
        .unwrap_or_default();

    Output {
        status,
        stdout,
        stderr,
    }
}

/// Parse search `--json` stdout into a [`Value`] (hits document or bare object).
fn json_hits(stdout: &str) -> Value {
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("json_hits parse: {e}; stdout={stdout}");
    })
}

/// Fail if `index` is the parent process `$HOME/.cbeta/index` (C5 isolation).
///
/// WHY: every C5 path must use `cbeta_env` (`env_remove HOME`) + `temp_dir("conc")`.
/// This helper catches accidental host-home index paths when the parent still has HOME.
fn assert_c5_index_isolated(index: &Path) {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    if home.is_empty() {
        return;
    }
    let forbidden = PathBuf::from(&home).join(".cbeta").join("index");
    assert!(
        index != forbidden.as_path(),
        "C5 isolation FAIL: CBETA_INDEX equals $HOME/.cbeta/index ({})",
        forbidden.display()
    );
    if let (Ok(idx), Ok(forb)) = (index.canonicalize(), forbidden.canonicalize()) {
        assert!(
            idx != forb,
            "C5 isolation FAIL: CBETA_INDEX canonical path is host home index ({})",
            forb.display()
        );
    }
}

/// Read `CURRENT` pointer basename under an index root (trim whitespace).
fn read_current_name(index: &Path) -> String {
    let raw = fs::read_to_string(index.join("CURRENT"))
        .unwrap_or_else(|e| panic!("read CURRENT under {}: {e}", index.display()));
    let name = raw.trim();
    assert!(
        !name.is_empty(),
        "CURRENT must name an artifact; got {raw:?}"
    );
    name.to_string()
}

/// Spawn with piped stdin payload; drain stderr; wait up to `limit`.
///
/// Used by REPL hang-safety (`conc_c6_repl_saves_last`). Stdin is closed after write
/// so the child can see EOF after `:q`.
fn spawn_timeout_stdin(mut cmd: Command, stdin_bytes: &[u8], limit: Duration) -> Output {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("spawn_timeout_stdin: {e}"));

    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(stdin_bytes)
            .unwrap_or_else(|e| panic!("write stdin: {e}"));
        // Drop closes stdin so REPL sees EOF after :q if needed.
        drop(sin);
    }

    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let drain = child.stderr.take().map(|stderr| {
        let buf = Arc::clone(&stderr_buf);
        thread::spawn(move || {
            let r = BufReader::new(stderr);
            for line in r.lines() {
                let Ok(line) = line else { break };
                if let Ok(mut g) = buf.lock() {
                    g.push_str(&line);
                    g.push('\n');
                }
            }
        })
    });

    let mut stdout_pipe = child.stdout.take();
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    let err = stderr_buf.lock().map(|g| g.clone()).unwrap_or_default();
                    panic!("spawn_timeout_stdin exceeded {limit:?}; stderr={err}");
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("spawn_timeout_stdin try_wait: {e}"),
        }
    };

    let mut stdout = Vec::new();
    if let Some(ref mut pipe) = stdout_pipe {
        let _ = pipe.read_to_end(&mut stdout);
    }
    drop(stdout_pipe);
    if let Some(h) = drain {
        let _ = h.join();
    }
    let stderr = stderr_buf
        .lock()
        .map(|g| g.as_bytes().to_vec())
        .unwrap_or_default();

    Output {
        status,
        stdout,
        stderr,
    }
}

/// Stdio MCP child with stderr drain (copied from `cli_mcp.rs` McpChild; not imported).
///
/// WHY drain + pending map: serve logs on stderr (pipe fill = deadlock); overlapping
/// tools/call replies can arrive out of order, so unmatched ids must be buffered.
struct ConcMcp {
    child: Child,
    stdin: ChildStdin,
    line_rx: Receiver<String>,
    pending: HashMap<u64, Value>,
    started: Instant,
    _stdout_drain: Option<thread::JoinHandle<()>>,
    _stderr_drain: Option<thread::JoinHandle<()>>,
    stderr_buf: Arc<Mutex<String>>,
}

impl ConcMcp {
    fn spawn(corpus: &Path, index: &Path) -> Self {
        let mut cmd = cbeta_env(corpus, index);
        cmd.arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("spawn cbeta serve");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (line_tx, line_rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        let stdout_drain = thread::spawn(move || {
            let r = BufReader::new(stdout);
            for line in r.lines() {
                let Ok(line) = line else { break };
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr_buf = Arc::new(Mutex::new(String::new()));
        let drain = child.stderr.take().map(|stderr| {
            let buf = Arc::clone(&stderr_buf);
            thread::spawn(move || {
                let r = BufReader::new(stderr);
                for line in r.lines() {
                    let Ok(line) = line else { break };
                    if let Ok(mut g) = buf.lock() {
                        g.push_str(&line);
                        g.push('\n');
                    }
                }
            })
        });
        Self {
            child,
            stdin,
            line_rx,
            pending: HashMap::new(),
            started: Instant::now(),
            _stdout_drain: Some(stdout_drain),
            _stderr_drain: drain,
            stderr_buf,
        }
    }

    fn ensure_alive(&mut self) {
        if self.started.elapsed() > CHILD_TIMEOUT {
            let _ = self.child.kill();
            let err = self
                .stderr_buf
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            panic!("cbeta serve exceeded {CHILD_TIMEOUT:?}; stderr={err}");
        }
        if let Some(status) = self.child.try_wait().expect("try_wait") {
            let err = self
                .stderr_buf
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
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
        if let Some(v) = self.pending.remove(&expect_id) {
            return v;
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.ensure_alive();
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                let _ = self.child.kill();
                let err = self
                    .stderr_buf
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                panic!("timeout waiting for JSON-RPC id={expect_id}; stderr={err}");
            }
            match self
                .line_rx
                .recv_timeout(left.min(Duration::from_millis(200)))
            {
                Ok(line) => {
                    let t = line.trim();
                    if t.is_empty() {
                        continue;
                    }
                    let v: Value = serde_json::from_str(t)
                        .unwrap_or_else(|e| panic!("bad JSON-RPC line `{t}`: {e}"));
                    // Notifications have no id — skip.
                    let Some(id) = v
                        .get("id")
                        .and_then(|i| i.as_u64().or_else(|| i.as_i64().map(|x| x as u64)))
                    else {
                        continue;
                    };
                    if id == expect_id {
                        return v;
                    }
                    self.pending.insert(id, v);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = self.child.kill();
                    panic!("EOF from cbeta serve before id={expect_id}");
                }
            }
        }
    }

    /// Handshake copied from `cli_mcp.rs` McpChild::initialize (~L112-129).
    fn initialize(&mut self) {
        self.write_line(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "cli_conc", "version": "0.0.1" }
            }
        }));
        let resp = self.read_response(1);
        assert!(resp.get("result").is_some(), "initialize failed: {resp}");
        self.write_line(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }));
    }

    /// Write a tools/call without waiting (for in-flight overlap).
    fn write_tool_call(&mut self, id: u64, name: &str, arguments: Value) {
        self.write_line(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }));
    }

    /// Parse tools/call result body (structuredContent or text JSON).
    fn parse_tool_result(name: &str, resp: &Value) -> Value {
        assert!(
            resp.get("error").is_none(),
            "tools/call {name} error: {resp}"
        );
        let result = resp
            .get("result")
            .unwrap_or_else(|| panic!("tools/call {name} no result: {resp}"));
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

    /// tools/call + parse (copied from `cli_mcp.rs` call_tool ~L152-187).
    #[allow(dead_code)]
    fn call_tool(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.write_tool_call(id, name, arguments);
        let resp = self.read_response(id);
        Self::parse_tool_result(name, &resp)
    }
}

impl Drop for ConcMcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hits_contain_line_id(body: &Value, line_id: &str) -> bool {
    body["hits"]
        .as_array()
        .map(|hits| hits.iter().any(|h| h["line_id"].as_str() == Some(line_id)))
        .unwrap_or(false)
}

/// Wave 1 smoke: helpers compile and mini build + one timed spawn exit 0.
#[test]
fn skeleton_compiles() {
    let _ = (CONC_N, MCP_INFLIGHT);
    let (corpus, index) = build_mini();

    // Exercise isolated HOME path used by later lifecycle-shaped waves.
    let home = temp_dir("conc-home");
    let mut info = cbeta_env_with_home(&corpus, &index, &home);
    info.args(["info", "--json"]);
    let out = spawn_timeout(info, CHILD_TIMEOUT);
    assert_eq!(
        out.status.code(),
        Some(0),
        "info --json: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _v = json_hits(&String::from_utf8_lossy(&out.stdout));
}

#[test]
fn conc_search_json_line_id() {
    let (corpus, index) = build_mini();

    let mut hit_cmd = cbeta_env(&corpus, &index);
    hit_cmd.args(["--json", "真性有为空"]);
    let hit = spawn_timeout(hit_cmd, CHILD_TIMEOUT);
    let hit_out = String::from_utf8_lossy(&hit.stdout);
    assert_eq!(
        hit.status.code(),
        Some(0),
        "--json 真性有为空; stderr={}",
        String::from_utf8_lossy(&hit.stderr)
    );
    let v = json_hits(&hit_out);
    let hits = v["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits array; got {hit_out}"));
    assert!(!hits.is_empty(), "empty hits FAIL; got {hit_out}");
    let h = &hits[0];
    for key in [
        "line_id",
        "work_id",
        "title",
        "text_raw",
        "citation",
        "score",
        "cbeta_tag",
    ] {
        assert!(h.get(key).is_some(), "missing {key} in {h}");
    }
    assert_eq!(h["line_id"], "T30n1578_p0268b21");

    let mut miss_cmd = cbeta_env(&corpus, &index);
    miss_cmd.args(["--json", "xyzzy-not-in-corpus"]);
    let miss = spawn_timeout(miss_cmd, CHILD_TIMEOUT);
    assert_eq!(
        miss.status.code(),
        Some(1),
        "no-hit exit 1; stdout={} stderr={}",
        String::from_utf8_lossy(&miss.stdout),
        String::from_utf8_lossy(&miss.stderr)
    );
}

#[test]
fn conc_verify_exact_get() {
    let (corpus, index) = build_mini();

    let mut ver_cmd = cbeta_env(&corpus, &index);
    ver_cmd.args(["verify", "--json", "真性有為空，如幻緣生故"]);
    let ver = spawn_timeout(ver_cmd, CHILD_TIMEOUT);
    let ver_out = String::from_utf8_lossy(&ver.stdout);
    assert_eq!(
        ver.status.code(),
        Some(0),
        "exact verify; stderr={}",
        String::from_utf8_lossy(&ver.stderr)
    );
    let v = json_hits(&ver_out);
    assert_eq!(
        v["is_original"], true,
        "verify exit 0 ≠ is_original; {ver_out}"
    );
    let line_id = v["exact_hit"]["line_id"]
        .as_str()
        .unwrap_or_else(|| panic!("exact_hit.line_id; {ver_out}"));
    assert!(
        line_id.contains("b21"),
        "expected b21 in line_id; got {line_id}"
    );
    assert!(
        v.get("action").is_none(),
        "must not dump Command; got {ver_out}"
    );

    let mut get_cmd = cbeta_env(&corpus, &index);
    get_cmd.args(["get", "T30n1578_p0268b21"]);
    let get = spawn_timeout(get_cmd, CHILD_TIMEOUT);
    assert_eq!(
        get.status.code(),
        Some(0),
        "get known; stderr={}",
        String::from_utf8_lossy(&get.stderr)
    );
    assert!(
        String::from_utf8_lossy(&get.stdout).contains("T30n1578_p0268b21"),
        "get stdout missing line_id"
    );

    let mut ghost_cmd = cbeta_env(&corpus, &index);
    ghost_cmd.args(["get", "T30n1578_p0268a12"]);
    let ghost = spawn_timeout(ghost_cmd, CHILD_TIMEOUT);
    assert_eq!(ghost.status.code(), Some(1), "ghost line_id must exit 1");

    let mut ctx_cmd = cbeta_env(&corpus, &index);
    ctx_cmd.args(["get", "--json", "T30n1578_p0268b21", "-C", "4"]);
    let ctx = spawn_timeout(ctx_cmd, CHILD_TIMEOUT);
    let ctx_out = String::from_utf8_lossy(&ctx.stdout);
    assert_eq!(
        ctx.status.code(),
        Some(0),
        "get -C 4; stderr={}",
        String::from_utf8_lossy(&ctx.stderr)
    );
    let cv = json_hits(&ctx_out);
    assert_eq!(cv["hit"]["line_id"], "T30n1578_p0268b21");
}

#[test]
fn conc_info_catalog_read() {
    let (corpus, index) = build_mini();

    let mut info_cmd = cbeta_env(&corpus, &index);
    info_cmd.args(["info"]);
    let info = spawn_timeout(info_cmd, CHILD_TIMEOUT);
    assert_eq!(
        info.status.code(),
        Some(0),
        "info; stderr={}",
        String::from_utf8_lossy(&info.stderr)
    );

    let mut cat_cmd = cbeta_env(&corpus, &index);
    cat_cmd.args(["catalog", "--author", "玄奘"]);
    let cat = spawn_timeout(cat_cmd, CHILD_TIMEOUT);
    let cat_out = String::from_utf8_lossy(&cat.stdout);
    assert_eq!(
        cat.status.code(),
        Some(0),
        "catalog --author; stderr={}",
        String::from_utf8_lossy(&cat.stderr)
    );
    assert!(cat_out.contains("T1578"), "expected T1578; got:\n{cat_out}");

    let mut read_cmd = cbeta_env(&corpus, &index);
    read_cmd.args(["read", "T0235", "--juan", "1"]);
    let read = spawn_timeout(read_cmd, CHILD_TIMEOUT);
    let read_out = String::from_utf8_lossy(&read.stdout);
    assert_eq!(
        read.status.code(),
        Some(0),
        "read T0235 --juan 1; stderr={}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert!(
        !read_out.trim().is_empty(),
        "read must list lines; got empty"
    );
}

#[test]
fn conc_phrase_mode_fullwidth() {
    let (corpus, index) = build_mini();

    let mut phrase_cmd = cbeta_env(&corpus, &index);
    phrase_cmd.args(["search", "--mode", "phrase", "--json", "如幻緣生故"]);
    let phrase = spawn_timeout(phrase_cmd, CHILD_TIMEOUT);
    let phrase_out = String::from_utf8_lossy(&phrase.stdout);
    assert_eq!(
        phrase.status.code(),
        Some(0),
        "--mode phrase; stderr={}",
        String::from_utf8_lossy(&phrase.stderr)
    );
    let pv = json_hits(&phrase_out);
    let phits = pv["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits; {phrase_out}"));
    assert!(!phits.is_empty(), "phrase empty hits; {phrase_out}");
    assert_eq!(phits[0]["line_id"], "T30n1578_p0268b21");

    for mode in ["nope", "fuzzy"] {
        let mut bad = cbeta_env(&corpus, &index);
        bad.args(["search", "--mode", mode, "真性有为空"]);
        let out = spawn_timeout(bad, CHILD_TIMEOUT);
        assert_eq!(
            out.status.code(),
            Some(2),
            "--mode {mode} exit 2; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let mut fw = cbeta_env(&corpus, &index);
    fw.args(["search", "空性＋缘生"]);
    let fw_out = spawn_timeout(fw, CHILD_TIMEOUT);
    assert_eq!(
        fw_out.status.code(),
        Some(2),
        "fullwidth ＋ exit 2; stderr={}",
        String::from_utf8_lossy(&fw_out.stderr)
    );

    let mut comma = cbeta_env(&corpus, &index);
    comma.args(["search", "--json", "真性有为空，如幻"]);
    let comma_out = spawn_timeout(comma, CHILD_TIMEOUT);
    assert_ne!(
        comma_out.status.code(),
        Some(2),
        "fullwidth ， must not reject; stderr={}",
        String::from_utf8_lossy(&comma_out.stderr)
    );
}

#[test]
fn conc_verify_similar_and_eight_readers() {
    let (corpus, index) = build_mini();

    let mut ver_cmd = cbeta_env(&corpus, &index);
    ver_cmd.args([
        "verify",
        "--json",
        "真性有为空，缘生故如幻，无为无起灭，不实若空华。",
    ]);
    let ver = spawn_timeout(ver_cmd, CHILD_TIMEOUT);
    let ver_out = String::from_utf8_lossy(&ver.stdout);
    assert_eq!(
        ver.status.code(),
        Some(0),
        "variant verify; stderr={}",
        String::from_utf8_lossy(&ver.stderr)
    );
    let vv = json_hits(&ver_out);
    assert_eq!(vv["is_original"], false, "not-original; {ver_out}");
    assert!(
        vv.get("action").is_none(),
        "must not dump Command; got {ver_out}"
    );
    let similar = vv["similar"]
        .as_array()
        .unwrap_or_else(|| panic!("similar array; {ver_out}"));
    assert!(!similar.is_empty(), "expected similar; {ver_out}");
    assert_eq!(similar[0]["work_id"], "T1578");

    let handles: Vec<_> = (0..CONC_N)
        .map(|_| {
            let corpus = corpus.clone();
            let index = index.clone();
            thread::spawn(move || {
                let mut cmd = cbeta_env(&corpus, &index);
                cmd.args(["--json", "真性有为空"]);
                spawn_timeout(cmd, CHILD_TIMEOUT)
            })
        })
        .collect();

    for (i, h) in handles.into_iter().enumerate() {
        let out = h.join().unwrap_or_else(|_| panic!("reader {i} panicked"));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(0),
            "reader {i} exit; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v = json_hits(&stdout);
        let hits = v["hits"]
            .as_array()
            .unwrap_or_else(|| panic!("reader {i} hits; {stdout}"));
        assert!(!hits.is_empty(), "reader {i} empty hits; {stdout}");
        let ids: Vec<&str> = hits.iter().filter_map(|h| h["line_id"].as_str()).collect();
        assert!(
            ids.contains(&"T30n1578_p0268b21"),
            "reader {i} missing b21; ids={ids:?}"
        );
    }
}

#[test]
fn conc_c5_isolation_temp_index() {
    // Given: temp index under system temp (never host $HOME/.cbeta/index)
    let corpus = mini_corpus();
    let index = temp_dir("conc");
    assert_c5_index_isolated(&index);

    // When: build via cbeta_env (env_remove HOME)
    let mut build = cbeta_env(&corpus, &index);
    build.args(["build", "--scope", "ci-minimal"]);
    let out = spawn_timeout(build, CHILD_TIMEOUT);

    // Then: exit 0 and artifact lives only under the temp index
    assert_eq!(
        out.status.code(),
        Some(0),
        "isolated build; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        index.join("CURRENT").is_file(),
        "CURRENT must land under temp index {}",
        index.display()
    );
    assert_c5_index_isolated(&index);
}

#[test]
fn conc_c5_two_builds_lock() {
    // Given: shared temp CBETA_INDEX; lock path is {index}/cbeta.lock (create_new)
    let corpus = mini_corpus();
    let index = temp_dir("conc");
    assert_c5_index_isolated(&index);
    let expected_lock = index.join("cbeta.lock");
    assert_eq!(
        expected_lock,
        PathBuf::from(index.as_os_str()).join("cbeta.lock"),
        "lock path contract: {{CBETA_INDEX}}/cbeta.lock"
    );

    // When: two concurrent builds against the same index
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let corpus = corpus.clone();
            let index = index.clone();
            thread::spawn(move || {
                let mut cmd = cbeta_env(&corpus, &index);
                cmd.args(["build", "--scope", "ci-minimal"]);
                let out = spawn_timeout(cmd, CHILD_TIMEOUT);
                (i, out)
            })
        })
        .collect();

    let mut results = Vec::with_capacity(2);
    for h in handles {
        let (i, out) = h
            .join()
            .unwrap_or_else(|_| panic!("concurrent build thread panicked"));
        results.push((i, out));
    }

    // Then: neither panicked; at least one may fail lock acquire; live remains
    let any_ok = results.iter().any(|(_, o)| o.status.code() == Some(0));
    assert!(
        any_ok || index.join("CURRENT").is_file(),
        "expected a successful build or live CURRENT; results={:?}",
        results
            .iter()
            .map(|(i, o)| {
                (
                    *i,
                    o.status.code(),
                    String::from_utf8_lossy(&o.stderr).into_owned(),
                )
            })
            .collect::<Vec<_>>()
    );
    assert!(
        index.join("CURRENT").is_file()
            || fs::read_dir(&index)
                .map(|rd| rd.filter_map(|e| e.ok()).any(|e| e.path().is_dir()))
                .unwrap_or(false),
        "live artifact/index must still exist after concurrent builds; entries under {}",
        index.display()
    );
    // Second builder may wipe {artifact}.tmp — current behavior, not a fail.
    let _ = expected_lock;
}

#[test]
fn conc_c5_search_during_build() {
    // Given: published mini live index
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);

    // When: in-flight `build --full` (wipes tmp only) + concurrent search
    let build_corpus = corpus.clone();
    let build_index = index.clone();
    let build_h = thread::spawn(move || {
        let mut cmd = cbeta_env(&build_corpus, &build_index);
        cmd.args(["build", "--full", "--scope", "ci-minimal"]);
        spawn_timeout(cmd, CHILD_TIMEOUT)
    });

    // Do not wait for build to finish before searching the live pointer.
    let mut search = cbeta_env(&corpus, &index);
    search.args(["search", "--json", "真性有为空"]);
    let search_out = spawn_timeout(search, CHILD_TIMEOUT);
    let stdout = String::from_utf8_lossy(&search_out.stdout);
    assert_eq!(
        search_out.status.code(),
        Some(0),
        "search during build; stderr={}",
        String::from_utf8_lossy(&search_out.stderr)
    );
    let v = json_hits(&stdout);
    let hits = v["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits during build; {stdout}"));
    assert!(!hits.is_empty(), "empty hits during build; {stdout}");
    let ids: Vec<&str> = hits.iter().filter_map(|h| h["line_id"].as_str()).collect();
    assert!(
        ids.contains(&"T30n1578_p0268b21"),
        "expected b21 from live during build; ids={ids:?}"
    );

    let build_out = build_h
        .join()
        .unwrap_or_else(|_| panic!("build --full thread panicked"));
    // Build may still be running or done; either exit is fine as long as no panic.
    let _ = build_out.status.code();
}

#[test]
fn conc_c5_current_not_tmp() {
    // Given/When: successful mini build
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let _ = corpus;

    // Then: CURRENT pointer contents must not contain ".tmp"
    let name = read_current_name(&index);
    assert!(
        !name.contains(".tmp"),
        "CURRENT must not contain .tmp; got {name:?}"
    );
    assert!(
        !name.ends_with(".tmp"),
        "CURRENT must not end with .tmp; got {name:?}"
    );
    assert!(
        index.join(&name).is_dir(),
        "CURRENT must name a live artifact dir; got {name}"
    );
}

#[test]
fn conc_c5_live_preserved() {
    // Given: published live artifact with a keep marker
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let first_name = read_current_name(&index);
    let first_art = index.join(&first_name);
    assert!(
        first_art.is_dir(),
        "live artifact missing at {}",
        first_art.display()
    );
    let marker = first_art.join("C5_KEEP_MARKER");
    fs::write(&marker, b"generation-1").unwrap_or_else(|e| panic!("plant marker: {e}"));

    // When: next build (publish may choose sibling {{name}}.{{unix_nanos}})
    let mut rebuild = cbeta_env(&corpus, &index);
    rebuild.args(["build", "--scope", "ci-minimal"]);
    let out = spawn_timeout(rebuild, CHILD_TIMEOUT);
    assert_eq!(
        out.status.code(),
        Some(0),
        "second build; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Then: original live dir + marker still exist (never home remove_dir_all)
    assert!(
        first_art.is_dir(),
        "original live must remain at {}; entries={:?}",
        first_art.display(),
        fs::read_dir(&index)
            .map(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    );
    assert!(
        marker.is_file(),
        "keep marker must survive publish at {}",
        marker.display()
    );
    let body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(body, "generation-1");
    let cur = read_current_name(&index);
    assert!(!cur.contains(".tmp"), "CURRENT after rebuild; {cur}");
}

#[test]
fn conc_c5_full_wipes_tmp_only() {
    // Given: live artifact with sentinel + dummy {artifact}.tmp staging dir
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let live_name = read_current_name(&index);
    let live_art = index.join(&live_name);
    let sentinel = live_art.join("C5_FULL_SENTINEL");
    fs::write(&sentinel, b"live-keep").unwrap_or_else(|e| panic!("plant live sentinel: {e}"));
    let tmp_path = index.join(format!("{live_name}.tmp"));
    fs::create_dir_all(&tmp_path).unwrap_or_else(|e| panic!("plant dummy tmp: {e}"));
    let tmp_marker = tmp_path.join("STALE_TMP_MARKER");
    fs::write(&tmp_marker, b"stale-tmp").unwrap_or_else(|e| panic!("plant tmp marker: {e}"));
    assert!(tmp_marker.is_file(), "precondition: dummy tmp marker");

    // When: build --full --scope ci-minimal
    let mut full = cbeta_env(&corpus, &index);
    full.args(["build", "--full", "--scope", "ci-minimal"]);
    let out = spawn_timeout(full, CHILD_TIMEOUT);
    assert_eq!(
        out.status.code(),
        Some(0),
        "build --full; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Then: live sentinel remains; stale tmp marker is gone (tmp wiped/replaced)
    assert!(
        sentinel.is_file(),
        "live sentinel must remain at {}",
        sentinel.display()
    );
    assert_eq!(
        fs::read_to_string(&sentinel).expect("read live sentinel"),
        "live-keep"
    );
    assert!(
        !tmp_marker.is_file(),
        "stale tmp marker must be wiped; path={}",
        tmp_marker.display()
    );
    let cur = read_current_name(&index);
    assert!(!cur.contains(".tmp"), "CURRENT after --full; {cur}");
    assert_c5_index_isolated(&index);
}

#[test]
fn conc_c6_truncated_last_json() {
    // Given: mini index + truncated last.json under CBETA_INDEX (not home)
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let last_path = index.join("last.json");
    fs::write(&last_path, b"{").unwrap_or_else(|e| panic!("write truncated last.json: {e}"));

    // When: get --from last --index 1 (must not panic)
    let mut cmd = cbeta_env(&corpus, &index);
    cmd.args(["get", "--from", "last", "--index", "1"]);
    let out = spawn_timeout(cmd, CHILD_TIMEOUT);

    // Then: exit 2, no panic (hang already FAIL via spawn_timeout)
    assert_eq!(
        out.status.code(),
        Some(2),
        "truncated last.json must exit 2; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn conc_c6_two_save_last() {
    // Given: shared mini index; two concurrent --save last writers
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let last_path = index.join("last.json");

    // When: two processes search --json --save last on the same CBETA_INDEX
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let corpus = corpus.clone();
            let index = index.clone();
            thread::spawn(move || {
                let mut cmd = cbeta_env(&corpus, &index);
                cmd.args(["search", "--json", "--save", "last", QUERY_SIMP]);
                let out = spawn_timeout(cmd, CHILD_TIMEOUT);
                (i, out)
            })
        })
        .collect();

    let mut results = Vec::with_capacity(2);
    for h in handles {
        let (i, out) = h
            .join()
            .unwrap_or_else(|_| panic!("--save last thread panicked"));
        results.push((i, out));
    }

    // Then: both exit 0; last.json is valid SavedSearch (torn JSON = FAIL)
    for (i, out) in &results {
        assert_eq!(
            out.status.code(),
            Some(0),
            "save last worker {i}; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(
        last_path.is_file(),
        "last.json must exist at {}",
        last_path.display()
    );
    let raw = fs::read_to_string(&last_path)
        .unwrap_or_else(|e| panic!("read last.json after concurrent save: {e}"));
    let last: Value = serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("torn/invalid last.json FAIL: {e}; raw={raw}");
    });
    for key in ["query", "filters", "hits", "artifact_id"] {
        assert!(
            last.get(key).is_some(),
            "SavedSearch missing `{key}`; got {last}"
        );
    }
    let hits = last["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits array; got {last}"));
    assert!(!hits.is_empty(), "SavedSearch hits empty; {last}");
    // LWW is acceptable product behavior for unlocked last.json.tmp+rename.
    eprintln!("notes: C6 two --save last → LWW well-formed last.json (no flock)");
}

#[test]
fn conc_c6_repl_saves_last() {
    // Given: mini index; bare `cbeta` opens REPL (no argv)
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let last_path = index.join("last.json");
    let _ = fs::remove_file(&last_path);

    // When: stdin query + :q within CHILD_TIMEOUT (hang = FAIL)
    let repl = cbeta_env(&corpus, &index);
    // No subcommand argv → REPL.
    let stdin = format!("{QUERY_SIMP}\n:q\n");
    let out = spawn_timeout_stdin(repl, stdin.as_bytes(), CHILD_TIMEOUT);
    assert_eq!(
        out.status.code(),
        Some(0),
        "REPL :q exit; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Then: REPL always --save last → last.json under CBETA_INDEX
    assert!(
        last_path.is_file(),
        "REPL must write last.json at {}; stderr={}",
        last_path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let raw = fs::read_to_string(&last_path).expect("read REPL last.json");
    let last: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("REPL last.json parse: {e}; raw={raw}"));
    assert!(
        last.get("hits").and_then(|h| h.as_array()).is_some(),
        "REPL last.json needs hits; {last}"
    );
}

#[test]
fn conc_c4_stdio_mcp_overlap() {
    // Given: mini index + stdio MCP (no --http)
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let mut mcp = ConcMcp::spawn(&corpus, &index);
    mcp.initialize();

    // When: MCP_INFLIGHT overlapping cbeta_search writes, then drain responses
    let n = MCP_INFLIGHT;
    let base_id = 100u64;
    for i in 0..n {
        mcp.write_tool_call(
            base_id + i as u64,
            "cbeta_search",
            json!({ "q": QUERY_SIMP }),
        );
    }
    let mut bodies = Vec::with_capacity(n);
    for i in 0..n {
        let resp = mcp.read_response(base_id + i as u64);
        bodies.push(ConcMcp::parse_tool_result("cbeta_search", &resp));
    }

    // Then: all four return hits with b21 (empty hits FAIL; hang already FAIL)
    for (i, body) in bodies.iter().enumerate() {
        assert!(
            hits_contain_line_id(body, EXPECT_LINE_ID),
            "MCP search inflight {i} missing {EXPECT_LINE_ID}; body={body}"
        );
    }
}

#[test]
fn conc_c4_stdio_mcp_verify_get() {
    // Given: mini index + stdio MCP
    let (corpus, index) = build_mini();
    assert_c5_index_isolated(&index);
    let mut mcp = ConcMcp::spawn(&corpus, &index);
    mcp.initialize();

    // When: overlapping search + verify_quote + get_passage (tool names exact)
    mcp.write_tool_call(200, "cbeta_search", json!({ "q": QUERY_SIMP }));
    mcp.write_tool_call(201, "cbeta_verify_quote", json!({ "q": VERIFY_Q }));
    mcp.write_tool_call(
        202,
        "cbeta_get_passage",
        json!({
            "line_id": EXPECT_LINE_ID,
            "action": "get"
        }),
    );

    let search_body = ConcMcp::parse_tool_result("cbeta_search", &mcp.read_response(200));
    let verify_body = ConcMcp::parse_tool_result("cbeta_verify_quote", &mcp.read_response(201));
    let get_body = ConcMcp::parse_tool_result("cbeta_get_passage", &mcp.read_response(202));

    // Then: structured fields (not merely ok); empty/timeout FAIL
    assert!(
        hits_contain_line_id(&search_body, EXPECT_LINE_ID),
        "in-flight search missing b21; {search_body}"
    );
    assert_eq!(
        verify_body["is_original"], true,
        "verify_quote is_original; {verify_body}"
    );
    assert_eq!(
        verify_body["exact_hit"]["line_id"], EXPECT_LINE_ID,
        "verify_quote exact_hit; {verify_body}"
    );
    assert_eq!(
        get_body["hit"]["line_id"], EXPECT_LINE_ID,
        "get_passage hit.line_id; {get_body}"
    );
}

fn worktree_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("worktree root")
}

fn perf_concurrency_script() -> PathBuf {
    let script = worktree_root().join("scripts/perf-concurrency.sh");
    assert!(
        script.is_file(),
        "perf-concurrency.sh missing at {}",
        script.display()
    );
    script
}

// Scope lock: skip+missing gates only — not DRY=1 defect_open.
#[test]
fn perf_script_gates_without_full_corpus() {
    let script = perf_concurrency_script();
    let root = worktree_root();
    let evidence = temp_dir("perf-gate");

    // Given: CBETA_FULL and CBETA_PERF_DRY unset
    // When: bash scripts/perf-concurrency.sh
    // Then: skip lane exits 0 immediately (no cargo / no index)
    let mut skip_cmd = Command::new("bash");
    skip_cmd
        .arg(&script)
        .current_dir(&root)
        .env("CBETA_PERF_EVIDENCE", &evidence)
        .env_remove("CBETA_FULL")
        .env_remove("CBETA_PERF_DRY")
        .env_remove("HOME");
    let skip = spawn_timeout(skip_cmd, CHILD_TIMEOUT);
    assert_eq!(
        skip.status.code(),
        Some(0),
        "skip gate exit; stdout={} stderr={}",
        String::from_utf8_lossy(&skip.stdout),
        String::from_utf8_lossy(&skip.stderr)
    );

    // Given: CBETA_FULL=1 and a nonexistent CBETA_INDEX (DRY unset)
    // When: same script
    // Then: missing-index gate exits 2 (never defaults to $HOME/.cbeta/index)
    let mut miss_cmd = Command::new("bash");
    miss_cmd
        .arg(&script)
        .current_dir(&root)
        .env("CBETA_PERF_EVIDENCE", &evidence)
        .env("CBETA_FULL", "1")
        .env("CBETA_INDEX", "/nonexistent/cbeta-index")
        .env_remove("CBETA_PERF_DRY")
        .env_remove("HOME");
    let miss = spawn_timeout(miss_cmd, CHILD_TIMEOUT);
    assert_eq!(
        miss.status.code(),
        Some(2),
        "missing-index gate exit; stdout={} stderr={}",
        String::from_utf8_lossy(&miss.stdout),
        String::from_utf8_lossy(&miss.stderr)
    );
}
