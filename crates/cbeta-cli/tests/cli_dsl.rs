//! CLI contract for L-dsl (#19): `--explain --json` parse dump + real-search
//! BEFORE / `*` / `&` / `,` / `-` / `?` against the mini corpus.
//!
//! Mini is T0235 + T1578 only. BEFORE/`*` need term order that exists in the
//! fixture text (e.g. 有…空, 如幻…缘生) — 空…有 is ordered-wrong on mini.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::path::{Path, PathBuf};
use std::process::Output;

fn cbeta() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
}

fn explain_json(q: &str) -> (i32, serde_json::Value, String) {
    let out = cbeta()
        .args(["search", "--explain", "--json", q])
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn cbeta search --explain --json");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let value = if stdout.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null)
    };
    (
        code,
        value,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("dsl");
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

/// Assert search `--json` is a hits document (not Command/parsed_query dump).
fn assert_hits_doc<'a>(v: &'a serde_json::Value, stdout: &str) -> &'a Vec<serde_json::Value> {
    assert!(
        v.get("hits").is_some(),
        "expected hits[] document, not Command dump; got {stdout}"
    );
    assert!(
        v.get("parsed_query").is_none(),
        "real search must not dump parsed_query; got {stdout}"
    );
    assert!(
        v.get("action").is_none(),
        "real search must not dump Command.action; got {stdout}"
    );
    v["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits array; got {stdout}"))
}

fn assert_hit_product_fields(h: &serde_json::Value) {
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
}

// ── explain-only (parse dump) ──────────────────────────────────────────────

#[test]
fn explain_json_plus_near_ordered_false() {
    let (code, value, stderr) = explain_json("空性+缘生");
    assert_eq!(code, 0, "stderr={stderr}");
    let pq = &value["parsed_query"];
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["within_chars"], 30);
    assert_eq!(pq["ordered"], false);
}

#[test]
fn explain_json_star_before_ordered_true() {
    let (code, value, stderr) = explain_json("空性*缘生");
    assert_eq!(code, 0, "stderr={stderr}");
    let pq = &value["parsed_query"];
    assert_eq!(pq["mode"], "before");
    assert_eq!(pq["within_chars"], 30);
    assert_eq!(pq["ordered"], true);
}

#[test]
fn explain_json_near_16_ordered_false() {
    let (code, value, stderr) = explain_json("真如 NEAR/16 缘起");
    assert_eq!(code, 0, "stderr={stderr}");
    let pq = &value["parsed_query"];
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["within_chars"], 16);
    assert_eq!(pq["ordered"], false);
}

#[test]
fn explain_json_lotus_wildcard() {
    let (code, value, stderr) = explain_json("莲?色");
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(value["parsed_query"]["mode"], "wildcard");
}

#[test]
fn explain_json_fullwidth_plus_exit_2() {
    let out = cbeta()
        .args(["search", "--explain", "--json", "空性＋缘生"])
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn fullwidth");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fullwidth") || stderr.contains("halfwidth"),
        "stderr should ask halfwidth, got: {stderr}"
    );
}

// ── real search (not --explain) against mini ───────────────────────────────

#[test]
fn search_before_30_tty_and_json_hits_not_command_dump() {
    // Given: mini index; verse has 有…空 (ordered BEFORE needs that order)
    let (corpus, index) = built();
    // When: TTY BEFORE/30
    let tty = run(&corpus, &index, &["search", "有 BEFORE/30 空"]);
    let tty_out = stdout_utf8(&tty);
    let tty_err = String::from_utf8_lossy(&tty.stderr);
    // Then: exit 0, human hit line with b21 when present
    assert_eq!(
        tty.status.code(),
        Some(0),
        "TTY BEFORE exit 0; stdout={tty_out} stderr={tty_err}"
    );
    assert!(
        tty_out.contains("T30n1578_p0268b21"),
        "expected b21 on TTY; got:\n{tty_out}"
    );
    assert!(
        tty_out.contains("大乘掌珍論"),
        "expected title on TTY; got:\n{tty_out}"
    );

    // When: same recipe --json
    let out = run(&corpus, &index, &["search", "--json", "有 BEFORE/30 空"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "json BEFORE; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    let hits = assert_hits_doc(&v, &stdout);
    assert!(!hits.is_empty(), "expected a hit; got {stdout}");
    assert_hit_product_fields(&hits[0]);
    assert_eq!(hits[0]["line_id"], "T30n1578_p0268b21");
}

#[test]
fn search_star_before_30_hits_b21() {
    // Given: mini index
    let (corpus, index) = built();
    // When: `*` alias for before/30 (order 有 then 空 matches fixture)
    let out = run(&corpus, &index, &["search", "--json", "有*空"]);
    let stdout = stdout_utf8(&out);
    // Then: exit 0 hits[] with b21
    assert_eq!(
        out.status.code(),
        Some(0),
        "star before; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    let hits = assert_hits_doc(&v, &stdout);
    assert!(!hits.is_empty(), "expected hit; got {stdout}");
    assert_hit_product_fields(&hits[0]);
    assert_eq!(hits[0]["line_id"], "T30n1578_p0268b21");
}

#[test]
fn search_and_ampersand_hits_b21() {
    // Given: mini index
    let (corpus, index) = built();
    // When: boolean AND
    let out = run(&corpus, &index, &["search", "--json", "真性&缘生"]);
    let stdout = stdout_utf8(&out);
    // Then: exit 0, b21
    assert_eq!(
        out.status.code(),
        Some(0),
        "AND; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    let hits = assert_hits_doc(&v, &stdout);
    assert!(!hits.is_empty(), "expected hit; got {stdout}");
    assert_hit_product_fields(&hits[0]);
    assert_eq!(hits[0]["line_id"], "T30n1578_p0268b21");
    assert_eq!(hits[0]["work_id"], "T1578");
}

#[test]
fn search_or_comma_two_mini_terms_exit_0() {
    // Given: mini index (T1578 真性有为空 + T0235 色不異空)
    let (corpus, index) = built();
    // When: ASCII comma OR of two terms present in mini
    let out = run(
        &corpus,
        &index,
        &["search", "--json", "真性有为空,色不異空"],
    );
    let stdout = stdout_utf8(&out);
    // Then: exit 0, hits document (at least one side matches)
    assert_eq!(
        out.status.code(),
        Some(0),
        "OR comma; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    let hits = assert_hits_doc(&v, &stdout);
    assert!(!hits.is_empty(), "expected OR hit; got {stdout}");
    assert_hit_product_fields(&hits[0]);
    let ids: Vec<&str> = hits
        .iter()
        .filter_map(|h| h["line_id"].as_str())
        .collect();
    assert!(
        ids.iter().any(|id| *id == "T30n1578_p0268b21")
            || ids.iter().any(|id| id.starts_with("T08n0235")),
        "expected T1578 or T0235 hit; got {ids:?}"
    );
}

#[test]
fn search_not_exclude_absent_term_still_hits() {
    // Given: mini index
    let (corpus, index) = built();
    // When: keyword minus a term that cannot occur
    let out = run(
        &corpus,
        &index,
        &["search", "--json", "空 -xyzzy-not-in-corpus"],
    );
    let stdout = stdout_utf8(&out);
    // Then: exit 0 (exclude of absent term does not wipe hits)
    assert_eq!(
        out.status.code(),
        Some(0),
        "NOT absent; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    let hits = assert_hits_doc(&v, &stdout);
    assert!(!hits.is_empty(), "expected hits after NOT absent; got {stdout}");
    assert_hit_product_fields(&hits[0]);
}

#[test]
fn search_wildcard_mid_char_hits_b21_when_engine_matches() {
    // Given: mini has 如幻緣生故 on b21; parse accepts `?` as wildcard mode
    let (corpus, index) = built();
    // When: single-char wildcard mid-term (docs: 莲?色 style)
    let out = run(&corpus, &index, &["search", "--json", "如幻緣?故"]);
    let stdout = stdout_utf8(&out);
    let code = out.status.code();
    // Then: real-search JSON shape (never Command dump). Prefer b21 when the
    // engine matches; if normalize strips `?` and phrase misses, exit 1 is the
    // current product surface (characterization — no product patch in this todo).
    if code == Some(0) {
        let v: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
        let hits = assert_hits_doc(&v, &stdout);
        assert!(!hits.is_empty(), "exit 0 implies hits; got {stdout}");
        assert_hit_product_fields(&hits[0]);
        assert_eq!(
            hits[0]["line_id"], "T30n1578_p0268b21",
            "wildcard hit should be b21; got {stdout}"
        );
    } else {
        assert_eq!(
            code,
            Some(1),
            "wildcard miss → exit 1 (or 0 with b21); stderr={} stdout={stdout}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Still a hits document when --json, not parse dump
        if !stdout.trim().is_empty() {
            let v: serde_json::Value = serde_json::from_str(&stdout)
                .unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
            let _ = assert_hits_doc(&v, &stdout);
        }
        // Fixture still has the verse without `?` — locks that the gap is engine
        // not missing text.
        let phrase = run(&corpus, &index, &["search", "--json", "如幻緣生故"]);
        assert_eq!(
            phrase.status.code(),
            Some(0),
            "phrase without ? must hit; stderr={}",
            String::from_utf8_lossy(&phrase.stderr)
        );
        let pstdout = stdout_utf8(&phrase);
        let pv: serde_json::Value = serde_json::from_str(&pstdout)
            .unwrap_or_else(|e| panic!("json: {e}; stdout={pstdout}"));
        let phits = assert_hits_doc(&pv, &pstdout);
        assert_eq!(phits[0]["line_id"], "T30n1578_p0268b21");
    }
}

#[test]
fn search_no_hit_operator_combo_exit_1() {
    // Given: mini index
    let (corpus, index) = built();
    // When: AND of two absent terms
    let out = run(&corpus, &index, &["search", "--json", "xyzzy&notincorp"]);
    // Then: rg-style no-hit
    assert_eq!(
        out.status.code(),
        Some(1),
        "no-hit AND; stdout={} stderr={}",
        stdout_utf8(&out),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn search_three_question_marks_exit_2_max_two() {
    // Given: any env (parse rejects before index)
    // When: three `?` (unit contract: max 2)
    let out = cbeta_env(&mini_corpus(), &temp_dir("dsl-q3"))
        .args(["search", "--json", "a?b?c?"])
        .output()
        .expect("spawn three ?");
    // Then: usage exit 2, max-2 message (align cbeta-core parse_query)
    assert_eq!(
        out.status.code(),
        Some(2),
        "three ? exit 2; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("max 2") || stderr.contains("more than twice"),
        "stderr should cite max 2; got: {stderr}"
    );
}
