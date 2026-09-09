//! CLI contract for L-in (#25): `--work` filter, `--script s` display, explain near/30.

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::path::{Path, PathBuf};
use std::process::Output;

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("in");
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
fn work_t1578_filters() {
    // Given: mini index (T0235 + T1578)
    let (corpus, index) = built();
    // When: 本经内搜 --work T1578 (mini substitute for product T1585)
    let out = run(
        &corpus,
        &index,
        &["search", "--json", "--work", "T1578", "真性"],
    );
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "--work T1578; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; {stdout}"));
    let hits = v["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits array; {stdout}"));
    assert!(!hits.is_empty(), "expected hits; {stdout}");
    for h in hits {
        assert_eq!(
            h["work_id"], "T1578",
            "every hit.work_id must be T1578; got {h}"
        );
    }

    // When: T0235-only filter
    let out235 = run(
        &corpus,
        &index,
        &["search", "--json", "--work", "T0235", "真性"],
    );
    let stdout235 = stdout_utf8(&out235);
    let v235: serde_json::Value =
        serde_json::from_str(&stdout235).unwrap_or_else(|e| panic!("json: {e}; {stdout235}"));
    let hits235 = v235["hits"].as_array().cloned().unwrap_or_default();
    // Then: T1578 hits dropped (no-hit or only T0235)
    for h in &hits235 {
        assert_ne!(h["work_id"], "T1578", "T0235 filter must drop T1578; {h}");
        assert_eq!(h["work_id"], "T0235");
    }
}

#[test]
fn script_s_plain_snippet() {
    // Given: mini index with traditional 為 in text_raw
    let (corpus, index) = built();
    // When: --script s --plain (display only)
    let out = run(
        &corpus,
        &index,
        &["search", "--script", "s", "--plain", "真性有为空"],
    );
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "--script s; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Then: snippet simplified 为; line_id never mutated
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "line_id must stay traditional id form; got:\n{stdout}"
    );
    assert!(
        stdout.contains('为') || stdout.contains("有为空"),
        "snippet must show simplified 为; got:\n{stdout}"
    );
    assert!(
        !stdout.contains('為'),
        "snippet must not keep traditional 為 under --script s; got:\n{stdout}"
    );

    // Default (no --script) stays 繁體
    let trad = run(&corpus, &index, &["search", "--plain", "真性有为空"]);
    let trad_out = stdout_utf8(&trad);
    assert_eq!(trad.status.code(), Some(0));
    assert!(
        trad_out.contains('為') || trad_out.contains("有為空"),
        "default display stays 繁體; got:\n{trad_out}"
    );
}

#[test]
fn explain_json_near_30() {
    // Given/When: --explain --json on + near DSL (no index needed)
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
        .args(["search", "--explain", "--json", "空性+缘生"])
        .output()
        .unwrap_or_else(|e| panic!("spawn: {e}"));
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(code, 0, "stderr={}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; {stdout}"));
    let pq = &v["parsed_query"];
    // Then: mode=near within_chars=30 (do not rename to distance)
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["within_chars"], 30);
}

#[test]
fn fullwidth_plus_exit_2() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
        .args(["search", "--explain", "--json", "空性＋缘生"])
        .output()
        .unwrap_or_else(|e| panic!("spawn: {e}"));
    assert_eq!(out.status.code(), Some(2));
    let star = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
        .args(["search", "--explain", "--json", "空性＊缘生"])
        .output()
        .unwrap_or_else(|e| panic!("spawn: {e}"));
    assert_eq!(star.status.code(), Some(2));
}
