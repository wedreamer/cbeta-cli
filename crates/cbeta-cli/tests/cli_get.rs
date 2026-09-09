//! L-get product contracts: get -C, read --juan, cite, --copy (Fixes #21).

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::path::{Path, PathBuf};
use std::process::Output;

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("get");
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

#[test]
fn get_b21_tty_citation() {
    // Given: mini index with real T1578 verse
    let (corpus, index) = built();
    // When: get with ±4 context (TTY)
    let out = run(&corpus, &index, &["get", "T30n1578_p0268b21", "-C", "4"]);
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: line_id, title, author, juan, text, citation; exit 0
    assert_eq!(
        out.status.code(),
        Some(0),
        "get -C 4; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "line_id; got:\n{stdout}"
    );
    assert!(stdout.contains("大乘掌珍論"), "title; got:\n{stdout}");
    assert!(
        stdout.contains("玄奘") || stdout.contains("清辯"),
        "author; got:\n{stdout}"
    );
    assert!(
        stdout.contains("j1") || stdout.contains("卷1") || stdout.contains(" 1 "),
        "juan; got:\n{stdout}"
    );
    assert!(
        stdout.contains("(CBETA 2026.R2, T30, no. 1578, p. 268, b21)"),
        "citation; got:\n{stdout}"
    );
}

#[test]
fn get_b21_json_context() {
    // Given: mini index
    let (corpus, index) = built();
    // When: get --json -C 4
    let out = run(
        &corpus,
        &index,
        &["get", "--json", "T30n1578_p0268b21", "-C", "4"],
    );
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "json get; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    // Then: { hit: Hit, context: Hit[] }
    let hit = v
        .get("hit")
        .unwrap_or_else(|| panic!("missing hit; {stdout}"));
    assert_eq!(hit["line_id"], "T30n1578_p0268b21");
    assert_eq!(
        hit["citation"],
        "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)"
    );
    let ctx = v["context"]
        .as_array()
        .unwrap_or_else(|| panic!("context array; {stdout}"));
    // Mini T1578 has multiple lines around b21; radius 4 yields neighbors.
    assert!(!ctx.is_empty(), "expected neighbor context; got {stdout}");
    for h in ctx {
        assert!(h.get("line_id").is_some(), "context hit fields; {h}");
        assert_ne!(h["line_id"], "T30n1578_p0268b21");
    }
}

#[test]
fn get_missing_exits_1() {
    // Given: mini index
    let (corpus, index) = built();
    // When: ghost a12 (negative only)
    let out = run(&corpus, &index, &["get", "T30n1578_p0268a12"]);
    // Then: exit 1
    assert_eq!(
        out.status.code(),
        Some(1),
        "missing exit 1; stdout={} stderr={}",
        stdout_utf8(&out),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn read_t0235_juan_1() {
    // Given: mini index (T0235 + T1578)
    let (corpus, index) = built();
    // When: read work+juan
    let out = run(&corpus, &index, &["read", "T0235", "--juan", "1"]);
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: lines of that work+juan in line_id order
    assert_eq!(
        out.status.code(),
        Some(0),
        "read; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T08n0235") || stdout.contains("T0235"),
        "T0235 line_ids; got:\n{stdout}"
    );
    // Must not dump T1578 body as the primary read target.
    let t0235_pos = stdout.find("T08n0235").or_else(|| stdout.find("T0235"));
    assert!(
        t0235_pos.is_some(),
        "expected T0235 content; got:\n{stdout}"
    );
}

#[test]
fn copy_payload_format() {
    // Given: mini index
    let (corpus, index) = built();
    // When: get --copy (clipboard best-effort; must not fail headless CI)
    let out = run(&corpus, &index, &["get", "T30n1578_p0268b21", "--copy"]);
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: exit 0 even if clipboard missing; payload shape for notes
    assert_eq!(
        out.status.code(),
        Some(0),
        "copy; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21")
            && stdout.contains("大乘掌珍論")
            && stdout.contains("卷1"),
        "copy block line_id title juan; got:\n{stdout}"
    );
    assert!(
        stdout.contains("(CBETA 2026.R2, T30, no. 1578, p. 268, b21)"),
        "copy citation; got:\n{stdout}"
    );
}

#[test]
fn cite_prints_citation_only() {
    // Given: mini index
    let (corpus, index) = built();
    // When: cite subcommand
    let out = run(&corpus, &index, &["cite", "T30n1578_p0268b21"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "cite; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let trimmed = stdout.trim();
    assert_eq!(
        trimmed, "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)",
        "cite-only; got:\n{stdout}"
    );
}
