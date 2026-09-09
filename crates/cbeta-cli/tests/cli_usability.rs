//! Scholar-shaped CLI usability: real `cbeta` binary against the mini fixture.
//!
//! These tests lock the recipes in AGENTS.md TESTS. They do **not** replace a
//! local transcript of the same commands run by hand (or as a human would).

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::path::{Path, PathBuf};
use std::process::Output;

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("use");
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
fn tty_keyword_search_shows_rank_line_id_title() {
    // Given: mini index
    let (corpus, index) = built();
    // When: scholar types a simplified keyword (bare search, TTY)
    let out = run(&corpus, &index, &["真性有为空"]);
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: rank + real line_id + 掌珍 title (not ghost a12)
    assert_eq!(
        out.status.code(),
        Some(0),
        "TTY search exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "expected line_id; got:\n{stdout}"
    );
    assert!(
        stdout.contains("大乘掌珍論"),
        "expected title; got:\n{stdout}"
    );
    assert!(
        stdout.contains("  1  ") || stdout.contains("1  T30n1578"),
        "expected rank column; got:\n{stdout}"
    );
}

#[test]
fn json_search_hit_has_product_fields() {
    // Given: mini index
    let (corpus, index) = built();
    // When: same recipe with --json
    let out = run(&corpus, &index, &["search", "--json", "真性有为空"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "json search; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    let Some(hits) = v["hits"].as_array() else {
        panic!("hits array; got {stdout}");
    };
    assert!(!hits.is_empty(), "expected a hit; got {stdout}");
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
}

#[test]
fn no_hit_exits_1() {
    // Given: mini index
    let (corpus, index) = built();
    // When: a string that cannot occur in the fixture
    let out = run(&corpus, &index, &["search", "xyzzy-not-in-corpus"]);
    // Then: rg-style no-hit
    assert_eq!(
        out.status.code(),
        Some(1),
        "no-hit exit 1; stdout={} stderr={}",
        stdout_utf8(&out),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn catalog_author_xuanzang_lists_t1578() {
    // Given: mini index
    let (corpus, index) = built();
    // When: scholar filters catalog by 作译者
    let out = run(&corpus, &index, &["catalog", "--author", "玄奘"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "catalog --author; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("T1578"), "expected T1578; got:\n{stdout}");
}

#[test]
fn catalog_type_lun_canon_t() {
    // Given: mini index
    let (corpus, index) = built();
    // When: lun + Taisho
    let out = run(
        &corpus,
        &index,
        &["catalog", "--type", "lun", "--canon", "T"],
    );
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "catalog --type/--canon; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("T1578"),
        "expected lun T1578; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("T0235"),
        "jing T0235 must be filtered out; got:\n{stdout}"
    );
}

#[test]
fn info_prints_artifact() {
    // Given: mini index
    let (corpus, index) = built();
    // When: cbeta info
    let out = run(&corpus, &index, &["info"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "info; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("2026R2") && stdout.contains("ci-minimal"),
        "expected tag+scope; got:\n{stdout}"
    );
}

#[test]
fn get_known_line_id_and_ghost() {
    // Given: mini index
    let (corpus, index) = built();
    // When/Then: real lb vs docs-ghost a12
    let hit = run(&corpus, &index, &["get", "T30n1578_p0268b21"]);
    assert_eq!(
        hit.status.code(),
        Some(0),
        "get known; stderr={}",
        String::from_utf8_lossy(&hit.stderr)
    );
    assert!(
        stdout_utf8(&hit).contains("T30n1578_p0268b21"),
        "get stdout: {}",
        stdout_utf8(&hit)
    );
    let ghost = run(&corpus, &index, &["get", "T30n1578_p0268a12"]);
    assert_eq!(ghost.status.code(), Some(1), "ghost line_id must exit 1");
}

#[test]
fn search_mode_phrase_hits_yuan_sheng() {
    // Given: mini index aligned to xml-p5@2026R2 (如幻緣生故 at 0268b21)
    let (corpus, index) = built();
    // When: real-canon consecutive string, not the synthetic 缘生故如幻
    let out = run(
        &corpus,
        &index,
        &["search", "--mode", "phrase", "如幻緣生故"],
    );
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: hit the real line_id, not b22 / ghost a12
    assert_eq!(
        out.status.code(),
        Some(0),
        "--mode phrase 如幻緣生故; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "expected T30n1578_p0268b21; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("T30n1578_p0268b22") && !stdout.contains("T30n1578_p0268a12"),
        "must not hit synthetic b22 or ghost a12; got:\n{stdout}"
    );
}

#[test]
fn script_s_converts_display_keeps_line_id() {
    // Given: mini index with 繁體 text_raw 真性有為空
    let (corpus, index) = built();
    // When: --script s --plain on a known hit
    let out = run(
        &corpus,
        &index,
        &["search", "--script", "s", "--plain", "真性有为空"],
    );
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: line_id unchanged; display snippet/title in 简体
    assert_eq!(
        out.status.code(),
        Some(0),
        "--script s; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "line_id must stay; got:\n{stdout}"
    );
    assert!(
        stdout.contains("真性有为空") || stdout.contains("大乘掌珍论"),
        "expected 简体 display; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("真性有為空"),
        "must not keep 繁體 snippet when --script s; got:\n{stdout}"
    );
}

#[test]
fn script_unknown_exits_2() {
    let (corpus, index) = built();
    let out = run(
        &corpus,
        &index,
        &["search", "--script", "x", "--plain", "空"],
    );
    assert_eq!(out.status.code(), Some(2), "unknown --script must exit 2");
}

#[test]
fn mode_near_accepted_unknown_mode_exits_2() {
    let (corpus, index) = built();
    let ok = run(
        &corpus,
        &index,
        &["search", "--mode", "near", "--explain", "--json", "空性+缘生"],
    );
    assert_eq!(
        ok.status.code(),
        Some(0),
        "--mode near explain; stderr={}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let bad = run(
        &corpus,
        &index,
        &["search", "--mode", "fuzzy", "空"],
    );
    assert_eq!(bad.status.code(), Some(2), "unknown --mode must exit 2");
}

#[test]
fn explain_json_plus_is_near_ordered_false_window_30() {
    // Given: no index needed for --explain --json (Command dump)
    let (corpus, index) = built();
    // When: scholar asks 空性+缘生 with explain JSON
    let out = run(
        &corpus,
        &index,
        &["search", "--explain", "--json", "空性+缘生"],
    );
    let stdout = stdout_utf8(&out);
    // Then: Command dump, mode=near, ordered=false, within_chars=30, two clauses
    assert_eq!(
        out.status.code(),
        Some(0),
        "explain near; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; {stdout}"));
    assert!(
        v.get("hits").is_none(),
        "--explain --json must dump Command not hits; got {stdout}"
    );
    let pq = &v["parsed_query"];
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["ordered"], false);
    assert_eq!(pq["within_chars"], 30);
    let clauses = pq["clauses"].as_array().expect("clauses array");
    assert_eq!(clauses.len(), 2, "two clauses; got {pq}");
    assert_eq!(v["script"], serde_json::Value::Null);
}

#[test]
fn explain_json_star_is_before_ordered_true() {
    let (corpus, index) = built();
    let out = run(
        &corpus,
        &index,
        &["search", "--explain", "--json", "空性*缘生"],
    );
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json");
    let pq = &v["parsed_query"];
    assert_eq!(pq["mode"], "before");
    assert_eq!(pq["ordered"], true);
    assert_eq!(pq["within_chars"], 30);
}

#[test]
fn explain_json_near_n_sets_within_chars() {
    let (corpus, index) = built();
    let out = run(
        &corpus,
        &index,
        &["search", "--explain", "--json", "真如 NEAR/16 缘起"],
    );
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json");
    let pq = &v["parsed_query"];
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["within_chars"], 16);
    assert_eq!(pq["ordered"], false);
}

#[test]
fn explain_json_question_is_wildcard() {
    let (corpus, index) = built();
    let out = run(
        &corpus,
        &index,
        &["search", "--explain", "--json", "莲?色"],
    );
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["parsed_query"]["mode"], "wildcard");
}

#[test]
fn explain_json_script_s_on_command() {
    let (corpus, index) = built();
    let out = run(
        &corpus,
        &index,
        &["search", "--script", "s", "--explain", "--json", "空性"],
    );
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["script"], "s");
}

#[test]
fn work_filter_t1578_all_hits_same_work() {
    // Given: mini has T0235 + T1578
    let (corpus, index) = built();
    // When: --work T1578 on a phrase present in T1578
    let out = run(
        &corpus,
        &index,
        &["search", "--work", "T1578", "--json", "真性有为空"],
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
    let hits = v["hits"].as_array().expect("hits");
    assert!(!hits.is_empty(), "expected hits; {stdout}");
    // Then: every hit is work_id T1578
    for h in hits {
        assert_eq!(
            h["work_id"], "T1578",
            "all hits must be T1578; got {h}"
        );
    }
}
