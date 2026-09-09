//! L-repl product contracts (GitHub #38).
//!
//! TDD RED until REPL lands — do not implement production code here.
//!
//! Intercept: `std::env::args_os().len() == 1` → REPL even when stdin is a pipe
//! (not TTY-only). Prompt `>` is stderr-only when stdin is a TTY; these tests
//! pipe stdin so they must not require `>` on stderr.
//!
//! Mini corpus is T0235 + T1578 only — never assert T1585.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

const LINE_ID: &str = "T30n1578_p0268b21";
const QUERY: &str = "真性有为空";
const CHILD_TIMEOUT: Duration = Duration::from_secs(30);

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("repl");
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

fn stdout_utf8(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap_or_else(|e| panic!("stdout utf-8: {e}"))
}

fn stderr_utf8(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap_or_else(|e| panic!("stderr utf-8: {e}"))
}

fn last_json_path(index: &Path) -> PathBuf {
    index.join("last.json")
}

/// Spawn bare `cbeta` (no argv after the binary), write `script` to stdin, wait.
///
/// Must not hang: if the binary treats empty argv as Search, it exits 2 immediately.
fn run_repl_script(corpus: &Path, index: &Path, script: &str) -> Output {
    let mut cmd = cbeta_env(corpus, index);
    // Explicit empty args: args_os().len() == 1 (binary only).
    cmd.args([] as [&str; 0])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("spawn bare cbeta: {e}"));
    let started = Instant::now();

    {
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin
            .write_all(script.as_bytes())
            .unwrap_or_else(|e| panic!("write repl script: {e}"));
        // Drop stdin → EOF so a real REPL can exit after :q / EOF.
    }

    loop {
        if started.elapsed() > CHILD_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("bare cbeta exceeded {CHILD_TIMEOUT:?} (possible hang on REPL/Search)");
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("try_wait: {e}"),
        }
    }

    child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("wait_with_output: {e}"))
}

fn run_args(corpus: &Path, index: &Path, args: &[&str]) -> Output {
    cbeta_env(corpus, index)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn cbeta {args:?}: {e}"))
}

#[test]
fn repl_pipe_scope_search_open_quit_exits_0() {
    // Given: mini index (T0235+T1578) built ci-minimal
    let (corpus, index) = built();

    // When: bare cbeta (no argv) with piped REPL script
    let script = format!(":scope T1578\n{QUERY}\n:open 1\n:q\n");
    let out = run_repl_script(&corpus, &index, &script);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: exit 0; search/open surface the known T1578 line_id; last.json has hits
    assert_eq!(
        out.status.code(),
        Some(0),
        "REPL scope+search+open+:q exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains(LINE_ID),
        "stdout must contain {LINE_ID} (search hit and/or :open); got:\n{stdout}"
    );
    let last_path = last_json_path(&index);
    assert!(
        last_path.is_file(),
        "last.json must exist under CBETA_INDEX at {}; stderr={stderr}",
        last_path.display()
    );
    let raw = std::fs::read_to_string(&last_path)
        .unwrap_or_else(|e| panic!("read last.json {}: {e}", last_path.display()));
    let last: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse last.json: {e}; raw={raw}"));
    let hits = last
        .get("hits")
        .and_then(|h| h.as_array())
        .unwrap_or_else(|| panic!("last.json hits array; got {last}"));
    assert!(!hits.is_empty(), "last.json hits non-empty; got {last}");
}

#[test]
fn repl_open_prints_passage_for_first_hit() {
    // Given: mini index built
    let (corpus, index) = built();

    // When: scope + search + :open 1 + quit
    let script = format!(":scope T1578\n{QUERY}\n:open 1\n:q\n");
    let out = run_repl_script(&corpus, &index, &script);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: get-style passage includes the first-hit line_id
    assert_eq!(
        out.status.code(),
        Some(0),
        "REPL :open exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains(LINE_ID),
        ":open 1 passage must include {LINE_ID}; got:\n{stdout}"
    );
}

#[test]
fn repl_copy_prints_cbeta_notes_block() {
    // Given: mini index built
    let (corpus, index) = built();

    // When: scope + search + :copy 1 + quit
    let script = format!(":scope T1578\n{QUERY}\n:copy 1\n:q\n");
    let out = run_repl_script(&corpus, &index, &script);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: exit 0; notes block mentions CBETA (same shape as get --copy)
    assert_eq!(
        out.status.code(),
        Some(0),
        "REPL :copy exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("CBETA") || stdout.contains("(CBETA"),
        ":copy 1 must print CBETA notes block; got:\n{stdout}"
    );
}

#[test]
fn empty_argv_with_flag_is_not_repl() {
    // Given: mini paths (index may be empty — usage fails before search)
    let corpus = mini_corpus();
    let index = temp_dir("repl-flag-not-repl");

    // When: `cbeta --json` (args_os len > 1) — must be Search, not REPL
    let out = run_args(&corpus, &index, &["--json"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: exit 2 immediately; usage / search requires a query; must not hang
    assert_eq!(
        out.status.code(),
        Some(2),
        "cbeta --json must exit 2 (not REPL); stdout={stdout} stderr={stderr}"
    );
    let err_l = stderr.to_lowercase();
    assert!(
        err_l.contains("search requires a query") || err_l.contains("usage"),
        "stderr must mention search requires a query or usage; got:\n{stderr}"
    );
}

#[test]
fn cbeta_search_without_query_still_exits_2() {
    // Given: mini paths
    let corpus = mini_corpus();
    let index = temp_dir("repl-search-no-q");

    // When: explicit `cbeta search` with no query
    let out = run_args(&corpus, &index, &["search"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: exit 2 (not REPL)
    assert_eq!(
        out.status.code(),
        Some(2),
        "cbeta search without query exit 2; stdout={stdout} stderr={stderr}"
    );
}
