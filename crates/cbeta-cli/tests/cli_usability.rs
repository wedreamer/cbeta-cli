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
fn catalog_work_t1578_exact_only() {
    // Given: mini index (T0235 + T1578 only)
    let (corpus, index) = built();
    // When: exact work_id
    let out = run(&corpus, &index, &["catalog", "--work", "T1578"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "catalog --work T1578; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("T1578"), "expected T1578; got:\n{stdout}");
    assert!(
        !stdout.contains("T0235"),
        "T0235 must not appear under --work T1578; got:\n{stdout}"
    );

    // When: prefix that is not an exact work_id (must not fuzzy-match T1578)
    let prefix = run(&corpus, &index, &["catalog", "--work", "T157"]);
    assert_eq!(
        prefix.status.code(),
        Some(1),
        "T157 must not fuzzy-match T1578; stdout={} stderr={}",
        stdout_utf8(&prefix),
        String::from_utf8_lossy(&prefix.stderr)
    );
    assert!(
        !stdout_utf8(&prefix).contains("T1578"),
        "prefix T157 must not list T1578; got:\n{}",
        stdout_utf8(&prefix)
    );
}

#[test]
fn catalog_title_zhangzhen_substring() {
    // Given: mini index; title filter is substring (not exact)
    let (corpus, index) = built();
    // When: scholar types a title fragment
    let out = run(&corpus, &index, &["catalog", "--title", "掌珍"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "catalog --title 掌珍; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("T1578"),
        "expected T1578 under --title 掌珍; got:\n{stdout}"
    );
    assert!(
        stdout.contains("大乘掌珍論"),
        "expected 大乘掌珍論; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("T0235"),
        "金剛經 must not match 掌珍; got:\n{stdout}"
    );
}

#[test]
fn catalog_work_t1585_absent_on_mini() {
    // Given: mini = T0235 + T1578 only (no T1585 rows)
    let (corpus, index) = built();
    // When: work_id not in mini catalog
    let out = run(&corpus, &index, &["catalog", "--work", "T1585"]);
    // Then: empty / exit 1 (do not invent T1585)
    assert_eq!(
        out.status.code(),
        Some(1),
        "catalog --work T1585 on mini; stdout={} stderr={}",
        stdout_utf8(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !stdout_utf8(&out).contains("T1585"),
        "must not invent T1585; got:\n{}",
        stdout_utf8(&out)
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
fn verify_json_schema_exact_and_variant() {
    // Given: mini index
    let (corpus, index) = built();
    // When: exact original quote
    let exact = run(
        &corpus,
        &index,
        &["verify", "--json", "真性有為空，如幻緣生故"],
    );
    let exact_out = stdout_utf8(&exact);
    assert_eq!(
        exact.status.code(),
        Some(0),
        "exact verify; stderr={}",
        String::from_utf8_lossy(&exact.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&exact_out).unwrap_or_else(|e| panic!("json: {e}; {exact_out}"));
    assert_eq!(v["is_original"], true);
    assert_eq!(v["exact_hit"]["line_id"], "T30n1578_p0268b21");
    assert!(
        v.get("action").is_none(),
        "must not dump Command; got {exact_out}"
    );

    // When: issue 异文 verse
    let variant = run(
        &corpus,
        &index,
        &[
            "verify",
            "--json",
            "真性有为空，缘生故如幻，无为无起灭，不实若空华。",
        ],
    );
    let var_out = stdout_utf8(&variant);
    assert_eq!(
        variant.status.code(),
        Some(0),
        "variant verify; stderr={}",
        String::from_utf8_lossy(&variant.stderr)
    );
    let vv: serde_json::Value =
        serde_json::from_str(&var_out).unwrap_or_else(|e| panic!("json: {e}; {var_out}"));
    assert_eq!(vv["is_original"], false);
    assert!(vv.get("exact_hit").is_none() || vv["exact_hit"].is_null());
    let similar = vv["similar"].as_array().expect("similar array");
    assert!(!similar.is_empty(), "expected similar; got {var_out}");
    assert_eq!(similar[0]["work_id"], "T1578");
}

#[test]
fn verify_tty_shows_is_original() {
    let (corpus, index) = built();
    let out = run(&corpus, &index, &["verify", "真性有為空，如幻緣生故"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout.contains("is_original=true"),
        "expected is_original=true; got:\n{stdout}"
    );
    assert!(stdout.contains("T30n1578_p0268b21"));
}
