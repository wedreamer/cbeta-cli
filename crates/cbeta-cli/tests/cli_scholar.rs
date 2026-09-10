//! Full-corpus scholar recipes (AGENTS.md local usability, 2026R2 lane).
//!
//! Fail-closed: every test returns immediately unless `CBETA_FULL=1`.
//! Default CI never opens `$HOME/.cbeta/corpus/2026R2`.

mod common;

use common::{cbeta_env, skip_unless_cbeta_full, temp_dir};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::OnceLock;

/// Real xml-p5 pin path. Call only after [`skip_unless_cbeta_full`] is false.
fn corpus_2026r2() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| {
        panic!("HOME must be set to resolve scholar corpus ~/.cbeta/corpus/2026R2")
    });
    let p = PathBuf::from(home).join(".cbeta").join("corpus").join("2026R2");
    if !p.is_dir() {
        panic!(
            "scholar corpus missing at {} — expected real xml-p5 2026R2 (no mini fallback)",
            p.display()
        );
    }
    p
}

/// Build once per process: temp index + `ci-minimal` over the full 2026R2 tree.
///
/// Prefer temp over `$HOME/.cbeta/index` so scholar runs stay isolated. Scope is
/// `ci-minimal` (T0235/T1578/…) — never `taisho` (9+ minutes).
fn built_scholar() -> &'static (PathBuf, PathBuf) {
    static BUILT: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    BUILT.get_or_init(|| {
        let corpus = corpus_2026r2();
        let index = temp_dir("scholar-full");
        let build = cbeta_env(&corpus, &index)
            .args(["build", "--scope", "ci-minimal"])
            .output()
            .unwrap_or_else(|e| panic!("scholar build: {e}"));
        assert_eq!(
            build.status.code(),
            Some(0),
            "build --scope ci-minimal on 2026R2: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        (corpus, index)
    })
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
fn full_tty_keyword_search_shows_rank_line_id_title() {
    // Given: CBETA_FULL off → skip before any 2026R2 fs access
    if skip_unless_cbeta_full() {
        return;
    }
    // Given: real 2026R2 corpus + ci-minimal index
    let (corpus, index) = built_scholar();
    // When: scholar types simplified keyword (bare search, TTY)
    let out = run(corpus, index, &["真性有为空"]);
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: exit 0, rank-ish + line_id + 掌珍 title
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
fn full_json_search_hit_has_product_fields() {
    if skip_unless_cbeta_full() {
        return;
    }
    // Given: real 2026R2 + ci-minimal index
    let (corpus, index) = built_scholar();
    // When: search --json 真性有为空
    let out = run(corpus, index, &["search", "--json", "真性有为空"]);
    let stdout = stdout_utf8(&out);
    assert_eq!(
        out.status.code(),
        Some(0),
        "json search; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Then: hits[] product fields — not Command dump
    let v: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("json: {e}; stdout={stdout}"));
    assert!(
        v.get("action").is_none(),
        "must not dump Command; got {stdout}"
    );
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
fn full_search_mode_phrase_hits_yuan_sheng() {
    if skip_unless_cbeta_full() {
        return;
    }
    // Given: real 2026R2 + ci-minimal index
    let (corpus, index) = built_scholar();
    // When: phrase mode on consecutive canon string
    let out = run(
        corpus,
        index,
        &["search", "--mode", "phrase", "如幻緣生故"],
    );
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: hits T30n1578_p0268b21
    assert_eq!(
        out.status.code(),
        Some(0),
        "--mode phrase 如幻緣生故; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "expected T30n1578_p0268b21; got:\n{stdout}"
    );
}

#[test]
fn full_verify_original_quote_is_original() {
    if skip_unless_cbeta_full() {
        return;
    }
    // Given: real 2026R2 + ci-minimal index
    let (corpus, index) = built_scholar();
    // When: original-like quote (繁體, consecutive)
    let out = run(corpus, index, &["verify", "真性有為空，如幻緣生故"]);
    let stdout = stdout_utf8(&out);
    // Then: is_original true, exit 0
    assert_eq!(
        out.status.code(),
        Some(0),
        "verify original; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("is_original=true"),
        "expected is_original=true; got:\n{stdout}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "expected line_id; got:\n{stdout}"
    );
}

#[test]
fn full_catalog_author_xuanzang_lists_t1578() {
    if skip_unless_cbeta_full() {
        return;
    }
    // Given: real 2026R2 + ci-minimal index (full tree may list more works)
    let (corpus, index) = built_scholar();
    // When: catalog --author 玄奘
    let out = run(corpus, index, &["catalog", "--author", "玄奘"]);
    let stdout = stdout_utf8(&out);
    // Then: MUST contain T1578 (may contain more on full corpus)
    assert_eq!(
        out.status.code(),
        Some(0),
        "catalog --author; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("T1578"), "expected T1578; got:\n{stdout}");
}
