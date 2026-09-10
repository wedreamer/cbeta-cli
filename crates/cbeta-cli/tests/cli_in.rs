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

#[test]
fn unknown_mode_fuzzy_and_nope_exit_2() {
    // Given: Search query so parsed_query is Some (main.rs mode override path)
    // When: --mode fuzzy / --mode nope
    // Then: exit 2; stderr names unknown --mode and expected keyword or phrase
    for mode in ["fuzzy", "nope"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
            .args(["search", "--mode", mode, "真性有为空"])
            .output()
            .unwrap_or_else(|e| panic!("spawn --mode {mode}: {e}"));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "--mode {mode}; stderr={stderr}");
        assert!(
            stderr.contains("unknown --mode"),
            "--mode {mode}: expected 'unknown --mode' in stderr; got: {stderr}"
        );
        assert!(
            stderr.contains("expected keyword or phrase"),
            "--mode {mode}: expected 'expected keyword or phrase'; got: {stderr}"
        );
    }
}

#[test]
fn script_t_tty_traditional_json_line_id_unchanged() {
    // Given: mini index; text_raw is 繁體
    let (corpus, index) = built();
    // When: --script t TTY/plain display
    let tty = run(
        &corpus,
        &index,
        &["search", "--script", "t", "--plain", "真性有为空"],
    );
    let tty_out = stdout_utf8(&tty);
    assert_eq!(
        tty.status.code(),
        Some(0),
        "--script t TTY; stderr={}",
        String::from_utf8_lossy(&tty.stderr)
    );
    // Then: 繁體 display; line_id never mutated
    assert!(
        tty_out.contains("T30n1578_p0268b21"),
        "line_id must stay; got:\n{tty_out}"
    );
    assert!(
        tty_out.contains('為') || tty_out.contains("有為空"),
        "--script t must show 繁體; got:\n{tty_out}"
    );

    // When: --script t --json
    let json = run(
        &corpus,
        &index,
        &["search", "--script", "t", "--json", "真性有为空"],
    );
    let json_out = stdout_utf8(&json);
    assert_eq!(
        json.status.code(),
        Some(0),
        "--script t --json; stderr={}",
        String::from_utf8_lossy(&json.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&json_out).unwrap_or_else(|e| panic!("json: {e}; {json_out}"));
    let hits = v["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits; {json_out}"));
    assert!(!hits.is_empty(), "expected hits; {json_out}");
    for h in hits {
        assert_eq!(
            h["line_id"], "T30n1578_p0268b21",
            "--script t must not mutate line_id; got {h}"
        );
    }
}

#[test]
fn search_filters_title_author_canon_work_t1578() {
    // Given: mini index (T0235 + T1578 only — never T1585 / 成唯識)
    let (corpus, index) = built();
    let cases: &[(&[&str], &str)] = &[
        (
            &["search", "--title", "掌珍", "--json", "真性有为空"],
            "--title 掌珍",
        ),
        (
            &["search", "--author", "玄奘", "--json", "真性有为空"],
            "--author 玄奘",
        ),
        (
            &["search", "--canon", "T", "--json", "真性有为空"],
            "--canon T",
        ),
        (
            &["search", "--work", "T1578", "--json", "真性有为空"],
            "--work T1578",
        ),
    ];
    for (args, label) in cases {
        let out = run(&corpus, &index, args);
        let stdout = stdout_utf8(&out);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{label}; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{label} json: {e}; {stdout}"));
        let hits = v["hits"]
            .as_array()
            .unwrap_or_else(|| panic!("{label} hits; {stdout}"));
        assert!(!hits.is_empty(), "{label}: expected hits; {stdout}");
        for h in hits {
            assert_eq!(
                h["work_id"], "T1578",
                "{label}: every hit.work_id must be T1578; got {h}"
            );
        }
    }
}

#[test]
fn search_dash_c_is_clap_usage_exit_2() {
    // Given/When: -C is Get-only, not global on Search (Oracle r2 / clap 4.5.23)
    // Then: clap usage exit 2; do NOT assert a hits list
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
        .args(["search", "-C", "4", "--json", "真性有为空"])
        .output()
        .unwrap_or_else(|e| panic!("spawn search -C: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr={stderr}");
    assert!(
        stderr.contains("unexpected argument '-C'"),
        "expected clap unexpected -C; got: {stderr}"
    );
}

#[test]
fn search_json_has_hits_no_top_level_before() {
    // Given: mini index; search without -C
    let (corpus, index) = built();
    // When: search --json (no -C)
    let out = run(&corpus, &index, &["search", "--json", "真性有为空"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; {stdout}"));
    let obj = v.as_object().unwrap_or_else(|| panic!("object; {stdout}"));
    // Then: hits key present; no top-level before (Get -C envelope is separate)
    assert!(
        obj.contains_key("hits"),
        "expected hits; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
    assert!(
        !obj.contains_key("before"),
        "search JSON must not have top-level before; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[test]
fn fullwidth_comma_ok_operators_exit_2() {
    // Given/When: fullwidth ideographic comma ， is NOT in the reject set
    let comma = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
        .args(["search", "--explain", "--json", "真性有为空，如幻"])
        .output()
        .unwrap_or_else(|e| panic!("spawn comma: {e}"));
    assert_eq!(
        comma.status.code(),
        Some(0),
        "fullwidth ， must not reject; stderr={}",
        String::from_utf8_lossy(&comma.stderr)
    );

    // When: — ＋ ＊ ＆ ？ still rejected (existing fullwidth_plus may stay)
    for q in [
        "空性—缘生",
        "空性＋缘生",
        "空性＊缘生",
        "空性＆缘生",
        "空性？缘生",
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
            .args(["search", "--explain", "--json", q])
            .output()
            .unwrap_or_else(|e| panic!("spawn {q}: {e}"));
        assert_eq!(
            out.status.code(),
            Some(2),
            "fullwidth op in {q:?} must exit 2; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
