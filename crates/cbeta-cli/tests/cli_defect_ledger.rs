//! DEFECT_LEDGER: lock CURRENT broken/awkward CLI observables (characterization only).
//!
//! No product patches here — clap / sidecar / scorer stay as-is. These tests freeze
//! what the binary does today so a later fix must flip the assertion deliberately.
//!
//! Unfixed classes mentioned for context only (no ranking-order or empty-TopN asserts):
//! - filter-after-TopN `--work T1585` can yield empty hits after score cut
//! - keyword ranking X-over-T (cross-canon score order)

mod common;

use common::{cbeta_env, mini_corpus, skip_unless_cbeta_full, temp_dir};
use std::path::{Path, PathBuf};
use std::process::Output;

fn built_mini() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("defect-ledger");
    let build = cbeta_env(&corpus, &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("build ci-minimal: {e}"));
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

/// Real xml-p5 pin. Call only after [`skip_unless_cbeta_full`] is false.
fn corpus_2026r2() -> PathBuf {
    let home = std::env::var("HOME")
        .unwrap_or_else(|_| panic!("HOME must be set to resolve ~/.cbeta/corpus/2026R2"));
    let p = PathBuf::from(home)
        .join(".cbeta")
        .join("corpus")
        .join("2026R2");
    if !p.is_dir() {
        panic!(
            "full corpus missing at {} — DEFECT_LEDGER (b) must not fall back to mini",
            p.display()
        );
    }
    p
}

/// DEFECT_LEDGER: unquoted `NEAR/N` is clap usage (extra argv), not query DSL.
///
/// Quoted `'NEAR/30 空 有'` remains DSL (see cli_dsl / cli_near — do not duplicate
/// the happy quoted path here beyond this comment).
#[test]
fn defect_ledger_unquoted_near_argv_exits_2() {
    // Given: mini corpus + ci-minimal index
    let (corpus, index) = built_mini();
    // When: unquoted NEAR/30 as SEPARATE argv tokens (not one quoted string)
    let out = run(&corpus, &index, &["search", "NEAR/30", "空", "有"]);
    // Then: usage exit 2 only (do not lock a specific clap message — clap may
    // complain about extra positionals and/or `/30`)
    assert_eq!(
        out.status.code(),
        Some(2),
        "DEFECT_LEDGER unquoted NEAR/N must be clap usage exit 2; stdout={} stderr={}",
        stdout_utf8(&out),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// DEFECT_LEDGER (full corpus): `catalog --title 成唯識` must never usage-fail.
///
/// Plan text expected sidecar-null titles so T1585 would be absent from substring
/// match. CURRENT 2026R2 catalog fills titles (`成唯識論`), so T1585 **does** appear
/// — locking "no T1585" would be a false characterization. We lock the stable
/// awkward contract that remains: exit is 0 or 1, **never 2** (TTY and `--json`).
///
/// Related CURRENT awkwardness (raw title substring, no 繁简 fold): simplified
/// `成唯识` misses traditional titles — locked in
/// [`defect_ledger_catalog_title_simplified_misses_t1585`].
#[test]
fn defect_ledger_catalog_title_chengweishi_never_exit_2() {
    if skip_unless_cbeta_full() {
        return;
    }
    // Given: real 2026R2 only (never mini)
    let corpus = corpus_2026r2();
    let index = temp_dir("defect-ledger-full");
    // When: catalog --title 成唯識 (TTY)
    let tty = run(&corpus, &index, &["catalog", "--title", "成唯識"]);
    let tty_code = tty.status.code();
    assert!(
        tty_code == Some(0) || tty_code == Some(1),
        "DEFECT_LEDGER catalog TTY exit must be 0|1 never 2; got {tty_code:?}; stdout={} stderr={}",
        stdout_utf8(&tty),
        String::from_utf8_lossy(&tty.stderr)
    );
    // When: same filter with --json
    let json = run(&corpus, &index, &["catalog", "--json", "--title", "成唯識"]);
    let json_code = json.status.code();
    assert!(
        json_code == Some(0) || json_code == Some(1),
        "DEFECT_LEDGER catalog --json exit must be 0|1 never 2; got {json_code:?}; stdout={} stderr={}",
        stdout_utf8(&json),
        String::from_utf8_lossy(&json.stderr)
    );
}

/// DEFECT_LEDGER (full corpus): catalog `--title` is raw substring — simplified
/// `成唯识` does not match traditional `成唯識論`, so T1585 is absent.
///
/// This is the CURRENT observable closest to the plan's "no T1585" claim (the
/// traditional-title path no longer drops T1585 once sidecar titles are filled).
#[test]
fn defect_ledger_catalog_title_simplified_misses_t1585() {
    if skip_unless_cbeta_full() {
        return;
    }
    // Given: real 2026R2 only (never mini; never assert T1585 against mini)
    let corpus = corpus_2026r2();
    let index = temp_dir("defect-ledger-full-s");
    // When: catalog --title 成唯识 (simplified) TTY
    let tty = run(&corpus, &index, &["catalog", "--title", "成唯识"]);
    let tty_out = stdout_utf8(&tty);
    let tty_code = tty.status.code();
    assert!(
        tty_code == Some(0) || tty_code == Some(1),
        "DEFECT_LEDGER simplified title TTY exit 0|1 never 2; got {tty_code:?}; stderr={}",
        String::from_utf8_lossy(&tty.stderr)
    );
    assert!(
        !tty_out.contains("T1585"),
        "DEFECT_LEDGER: simplified 成唯识 must not surface T1585; got:\n{tty_out}"
    );
    // When: --json
    let json = run(&corpus, &index, &["catalog", "--json", "--title", "成唯识"]);
    let json_out = stdout_utf8(&json);
    let json_code = json.status.code();
    assert!(
        json_code == Some(0) || json_code == Some(1),
        "DEFECT_LEDGER simplified title --json exit 0|1 never 2; got {json_code:?}; stderr={}",
        String::from_utf8_lossy(&json.stderr)
    );
    assert!(
        !json_out.contains("T1585"),
        "DEFECT_LEDGER: simplified 成唯识 --json must not surface T1585; got:\n{json_out}"
    );
}
