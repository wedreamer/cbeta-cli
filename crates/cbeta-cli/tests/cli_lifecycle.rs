//! Wave 1–2 lifecycle surface: clap parse, releases, fetch offline paths.
//!
//! Successful `use` paths plant mini under `$HOME/.cbeta/corpus/<tag>/` with a
//! complete synthetic FETCHED.yaml and **must not** set `CBETA_CORPUS` (that
//! override skips the FETCHED completeness gate in `cmd_use`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{lock_pins_for, plant_mini_under_tag, temp_dir};

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_cbeta"));
    c.env("NO_COLOR", "1");
    c
}

/// Spawn `cbeta` for successful-FETCHED lifecycle: HOME + INDEX, no CBETA_CORPUS.
fn bin_fetched_home(home: &Path, index: &Path) -> Command {
    let mut c = bin();
    c.env("HOME", home);
    c.env("CBETA_INDEX", index);
    c.env_remove("CBETA_CORPUS");
    c
}

/// Plant mini under tag + complete FETCHED; returns tag_dir. Never writes CURRENT.
///
/// Writes FETCHED with left-aligned keys (product `FetchedYaml` parse). Pins come
/// from `lock_pins_for` — never hard-coded SHAs. Does not set CURRENT.
fn plant_complete_2026r2(home: &Path) -> PathBuf {
    let tag = "2026R2";
    let tag_dir = plant_mini_under_tag(home, tag);
    let pins = lock_pins_for(tag);
    // WHY: do not use `\n\` continuations — Rust strips next-line indent, breaking
    // nested `sources.xml-p5`. common::write_complete_fetched_yaml has the same bug.
    let yaml = [
        "schema: cbeta-cli.fetched/v1".to_string(),
        format!("cbeta_release: {tag}"),
        "fetched_at: 1970-01-01T00:00:00Z".to_string(),
        format!("cache_root: {}", tag_dir.display()),
        "sources:".to_string(),
        "  xml-p5:".to_string(),
        format!("    tag: {tag}"),
        format!("    commit: {}", pins.xml_p5),
        "  metadata:".to_string(),
        format!("    commit: {}", pins.metadata),
        "  gaiji:".to_string(),
        format!("    commit: {}", pins.gaiji),
        String::new(),
    ]
    .join("\n");
    fs::write(tag_dir.join("FETCHED.yaml"), yaml)
        .unwrap_or_else(|e| panic!("write FETCHED.yaml: {e}"));
    tag_dir
}

fn read_trim(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .trim()
        .to_string()
}

#[test]
fn clap_parse_fetch_and_build_full() {
    // Unknown release: exit 2, never clones github.
    let out = bin()
        .args(["fetch", "--release", "not-a-real-tag", "--scope", "taisho"])
        .output()
        .unwrap_or_else(|e| panic!("run fetch: {e}"));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unknown release") || err.contains("not-a-real-tag"),
        "stderr={err}"
    );
    assert!(
        !err.contains("not wired"),
        "Wave 2 must not keep Wave 1 stub: stderr={err}"
    );

    // Valid release with unreachable mirror must not hit public GitHub.
    let home = tempfile_home("cbeta-lc-fetch-offline");
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_GIT_BASE", "http://127.0.0.1:1")
        .args(["fetch", "--release", "2026R2", "--scope", "taisho"])
        .output()
        .unwrap_or_else(|e| panic!("run fetch offline: {e}"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.to_lowercase().contains("github.com/cbeta-org"),
        "must not clone official github in CI: stderr={err}"
    );
    // No CURRENT written on failed fetch.
    assert!(
        !home.join(".cbeta/corpus/CURRENT").exists(),
        "CURRENT must not appear after failed fetch"
    );
    let _ = std::fs::remove_dir_all(&home);

    let out = bin()
        .args(["build", "--scope", "ci-minimal", "--full", "--help"])
        .output()
        .unwrap_or_else(|e| panic!("build --help: {e}"));
    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(help.contains("--full") || out.status.success());
}

#[test]
fn releases_local_lists_lock_tags() {
    let out = bin()
        .args(["releases"])
        .output()
        .unwrap_or_else(|e| panic!("releases: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("2026R2"), "stdout={stdout}");
    assert!(stdout.contains("2026R1"), "stdout={stdout}");
    assert!(stdout.contains("2025R3"), "stdout={stdout}");
    assert!(!stdout.contains("master"), "stdout={stdout}");
    assert!(
        stdout.contains("latest"),
        "newest tag should be marked latest: stdout={stdout}"
    );
}

#[test]
fn releases_remote_offline_hints_mirror() {
    let home = tempfile_home("cbeta-lc-rel-remote");
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_GIT_BASE", "http://127.0.0.1:1")
        .args(["releases", "--remote"])
        .output()
        .unwrap_or_else(|e| panic!("releases --remote: {e}"));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("镜像")
            || err.contains("CBETA_GIT_BASE")
            || err.contains("网络")
            || err.contains("ls-remote")
            || err.contains("failed"),
        "stderr={err}"
    );
    assert!(!err.contains("Wave 2"), "stderr={err}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn current_without_pointer_exits_2_with_fetch_hint() {
    let home = tempfile_home("cbeta-lc-current");
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .args(["current"])
        .output()
        .unwrap_or_else(|e| panic!("current: {e}"));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("cbeta fetch --release 2026R2 --scope taisho"),
        "stderr={err}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn search_no_index_mentions_fetch() {
    let home = tempfile_home("cbeta-lc-noindex");
    let index = home.join("empty-index");
    if let Err(e) = std::fs::create_dir_all(&index) {
        panic!("index dir: {e}");
    }
    let out = bin()
        .env("HOME", &home)
        .env("CBETA_INDEX", &index)
        .env(
            "CBETA_CORPUS",
            env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/mini",
        )
        .args(["search", "空"])
        .output()
        .unwrap_or_else(|e| panic!("search: {e}"));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("cbeta fetch --release 2026R2 --scope taisho"),
        "stderr={err}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

fn tempfile_home(prefix: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&p);
    if let Err(e) = std::fs::create_dir_all(p.join(".cbeta").join("corpus")) {
        panic!("home: {e}");
    }
    p
}

#[test]
fn ghost_get_mentions_current_tag() {
    let home = tempfile_home("cbeta-lc-ghost");
    let index = home.join("idx");
    let corpus = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/mini";
    let build = bin()
        .env("HOME", &home)
        .env("CBETA_CORPUS", &corpus)
        .env("CBETA_INDEX", &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("build: {e}"));
    assert_eq!(
        build.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&build.stderr)
    );
    let out = bin()
        .env("HOME", &home)
        .env("CBETA_CORPUS", &corpus)
        .env("CBETA_INDEX", &index)
        .args(["get", "T30n1578_p0268a12"])
        .output()
        .unwrap_or_else(|e| panic!("get: {e}"));
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("当前"), "stderr={err}");
    assert!(err.contains("无此行"), "stderr={err}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn pull_does_not_write_current() {
    let home = tempfile_home("cbeta-lc-pull");
    std::fs::write(home.join(".cbeta/corpus/CURRENT"), "2025R3\n").unwrap();
    let before = std::fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_GIT_BASE", "http://127.0.0.1:1")
        .args(["pull"])
        .output()
        .unwrap_or_else(|e| panic!("pull: {e}"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
    assert_eq!(before, after, "pull must never rewrite CURRENT");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn pull_apply_does_not_write_current() {
    let home = tempfile_home("cbeta-lc-pull-apply");
    std::fs::write(home.join(".cbeta/corpus/CURRENT"), "2025R3\n").unwrap();
    let before = std::fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_GIT_BASE", "http://127.0.0.1:1")
        .args(["pull", "--apply"])
        .output()
        .unwrap_or_else(|e| panic!("pull --apply: {e}"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
    assert_eq!(before, after, "pull --apply must never rewrite CURRENT");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn pull_offline_hints_mirror() {
    let home = tempfile_home("cbeta-lc-pull-off");
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_GIT_BASE", "http://127.0.0.1:1")
        .args(["pull"])
        .output()
        .unwrap_or_else(|e| panic!("pull offline: {e}"));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("CBETA_GIT_BASE") || err.contains("镜像") || err.contains("网络"),
        "stderr={err}"
    );
    assert!(!home.join(".cbeta/corpus/CURRENT").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn prune_never_deletes_current() {
    let home = tempfile_home("cbeta-lc-prune-cur");
    let corpus = home.join(".cbeta/corpus");
    std::fs::create_dir_all(corpus.join("2026R2")).unwrap();
    std::fs::create_dir_all(corpus.join("2025R3")).unwrap();
    std::fs::write(corpus.join("CURRENT"), "2026R2\n").unwrap();
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .args(["prune", "2026R2"])
        .output()
        .unwrap_or_else(|e| panic!("prune CURRENT tag: {e}"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        corpus.join("2026R2").is_dir(),
        "CURRENT tag dir must survive prune"
    );
    assert!(corpus.join("CURRENT").is_file());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn prune_dry_run_lists_without_delete() {
    let home = tempfile_home("cbeta-lc-prune-dry");
    let corpus = home.join(".cbeta/corpus");
    std::fs::create_dir_all(corpus.join("2026R2")).unwrap();
    std::fs::create_dir_all(corpus.join("2025R3")).unwrap();
    std::fs::write(corpus.join("CURRENT"), "2026R2\n").unwrap();
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .args(["prune", "--dry-run", "2025R3"])
        .output()
        .unwrap_or_else(|e| panic!("prune --dry-run: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("2025R3"),
        "dry-run should list would-delete tag: {combined}"
    );
    assert!(
        corpus.join("2025R3").is_dir(),
        "dry-run must not unlink tag dir"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn gc_never_deletes_current() {
    let home = tempfile_home("cbeta-lc-gc");
    let corpus = home.join(".cbeta/corpus");
    std::fs::create_dir_all(corpus.join("2026R2")).unwrap();
    std::fs::create_dir_all(corpus.join("2025R3")).unwrap();
    std::fs::write(corpus.join("CURRENT"), "2026R2\n").unwrap();
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .args(["gc"])
        .output()
        .unwrap_or_else(|e| panic!("gc: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        corpus.join("2026R2").is_dir(),
        "gc must keep CURRENT tag dir"
    );
    assert!(corpus.join("CURRENT").is_file());
    assert!(
        !corpus.join("2025R3").exists(),
        "gc should remove unused non-CURRENT tag cache"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn use_does_not_switch_current_until_build_ok() {
    let home = tempfile_home("cbeta-lc-use-fail");
    let corpus_cache = home.join(".cbeta/corpus");
    std::fs::write(corpus_cache.join("CURRENT"), "2025R3\n").unwrap();
    std::fs::create_dir_all(corpus_cache.join("2026R2").join("src")).unwrap();

    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_INDEX", home.join("idx"))
        .args(["use", "2026R2", "--scope", "taisho"])
        .output()
        .unwrap_or_else(|e| panic!("use incomplete: {e}"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cur = std::fs::read_to_string(corpus_cache.join("CURRENT")).unwrap();
    assert_eq!(
        cur.trim(),
        "2025R3",
        "CURRENT must stay previous when use fails"
    );

    let mini = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/mini";
    let out = bin()
        .env("HOME", &home)
        .env("CBETA_CORPUS", &mini)
        .env("CBETA_INDEX", home.join("idx-fail"))
        .args(["use", "2026R2", "--scope", "no-such-scope-xyz"])
        .output()
        .unwrap_or_else(|e| panic!("use bad scope: {e}"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cur = std::fs::read_to_string(corpus_cache.join("CURRENT")).unwrap();
    assert_eq!(
        cur.trim(),
        "2025R3",
        "on build failure CURRENT must remain previous tag"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn fetch_does_not_write_current() {
    let home = tempfile_home("cbeta-lc-fetch-nocur");
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_GIT_BASE", "http://127.0.0.1:1")
        .args(["fetch", "--release", "2026R2", "--scope", "taisho"])
        .output()
        .unwrap_or_else(|e| panic!("fetch: {e}"));
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !home.join(".cbeta/corpus/CURRENT").exists(),
        "fetch must never write CURRENT"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn fetch_latest_stores_concrete_tag() {
    let home = tempfile_home("cbeta-lc-fetch-latest");
    let out = bin()
        .env("HOME", &home)
        .env_remove("CBETA_CORPUS")
        .env("CBETA_GIT_BASE", "http://127.0.0.1:1")
        .args(["fetch", "--release", "latest", "--scope", "taisho"])
        .output()
        .unwrap_or_else(|e| panic!("fetch latest: {e}"));
    assert_eq!(out.status.code(), Some(2));
    assert!(!home.join(".cbeta/corpus/CURRENT").exists());
    assert!(!home.join(".cbeta/corpus/latest").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn use_2026r2_scope_ci_minimal_switches_current() {
    let home = temp_dir("use-home");
    let index = temp_dir("use-idx");
    let _tag_dir = plant_complete_2026r2(&home);

    let out = bin_fetched_home(&home, &index)
        .args(["use", "2026R2", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("use 2026R2: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let current_path = home.join(".cbeta/corpus/CURRENT");
    assert!(current_path.is_file(), "CURRENT must exist after use");
    let cur = read_trim(&current_path);
    assert_eq!(cur, "2026R2");
    assert!(
        !cur.contains(".tmp"),
        "CURRENT must not be a .tmp path: {cur}"
    );
    assert!(
        !current_path.to_string_lossy().contains(".tmp"),
        "CURRENT path must not be under .tmp"
    );

    let cur_out = bin_fetched_home(&home, &index)
        .args(["current"])
        .output()
        .unwrap_or_else(|e| panic!("current: {e}"));
    assert_eq!(
        cur_out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&cur_out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&cur_out.stdout).trim(), "2026R2");

    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(&index);
}

#[test]
fn use_default_writes_default_and_current() {
    let home = temp_dir("use-def-home");
    let index = temp_dir("use-def-idx");
    let _tag_dir = plant_complete_2026r2(&home);

    let out = bin_fetched_home(&home, &index)
        .args(["use", "--default", "2026R2", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("use --default: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(read_trim(&home.join(".cbeta/corpus/CURRENT")), "2026R2");
    assert_eq!(read_trim(&home.join(".cbeta/corpus/DEFAULT")), "2026R2");

    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(&index);
}

#[test]
fn use_latest_pins_concrete_2026r2_no_latest_dir() {
    let home = temp_dir("use-latest-home");
    let index = temp_dir("use-latest-idx");
    let tag_dir = plant_complete_2026r2(&home);

    let out = bin_fetched_home(&home, &index)
        .args(["use", "latest", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("use latest: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(read_trim(&home.join(".cbeta/corpus/CURRENT")), "2026R2");
    assert!(
        !home.join(".cbeta/corpus/latest").exists(),
        "use latest must not create a latest/ directory"
    );

    let fetched = fs::read_to_string(tag_dir.join("FETCHED.yaml"))
        .unwrap_or_else(|e| panic!("read FETCHED: {e}"));
    assert!(
        fetched.contains("cbeta_release: 2026R2"),
        "FETCHED cbeta_release must stay concrete: {fetched}"
    );
    assert!(
        !fetched.contains("cbeta_release: latest")
            && !fetched.contains("cbeta_release: \"latest\""),
        "FETCHED cbeta_release must never be latest: {fetched}"
    );

    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(&index);
}

#[test]
fn use_incomplete_fetched_exits_2_current_unchanged() {
    let home = temp_dir("use-inc-home");
    let index = temp_dir("use-inc-idx");
    let corpus = home.join(".cbeta/corpus");
    fs::create_dir_all(&corpus).unwrap_or_else(|e| panic!("mkdir corpus: {e}"));
    fs::write(corpus.join("CURRENT"), "2025R3\n").unwrap_or_else(|e| panic!("write CURRENT: {e}"));

    let _tag_dir = plant_mini_under_tag(&home, "2026R2");

    let out = bin_fetched_home(&home, &index)
        .args(["use", "2026R2", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("use incomplete: {e}"));
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        read_trim(&corpus.join("CURRENT")),
        "2025R3",
        "CURRENT must stay previous when FETCHED incomplete"
    );

    let home2 = temp_dir("use-omit-home");
    let index2 = temp_dir("use-omit-idx");
    let corpus2 = home2.join(".cbeta/corpus");
    fs::create_dir_all(corpus2.join("2026R2")).unwrap_or_else(|e| panic!("mkdir tag: {e}"));
    fs::write(corpus2.join("CURRENT"), "2025R3\n").unwrap_or_else(|e| panic!("write CURRENT: {e}"));

    let out2 = bin_fetched_home(&home2, &index2)
        .args(["use", "2026R2", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("use no FETCHED: {e}"));
    assert_eq!(
        out2.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&out2.stderr)
    );
    assert_eq!(read_trim(&corpus2.join("CURRENT")), "2025R3");

    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(&index);
    let _ = fs::remove_dir_all(&home2);
    let _ = fs::remove_dir_all(&index2);
}

#[test]
fn use_invalidates_prior_last_json() {
    let home = temp_dir("use-last-home");
    let index = temp_dir("use-last-idx");
    let _tag_dir = plant_complete_2026r2(&home);

    fs::create_dir_all(&index).unwrap_or_else(|e| panic!("mkdir index: {e}"));
    let last_path = index.join("last.json");
    fs::write(
        &last_path,
        r#"{
  "query": "真性有为空",
  "filters": {},
  "hits": [{
    "line_id": "T30n1578_p0268b21",
    "work_id": "T1578",
    "title": "大乘掌珍論",
    "author": "玄奘",
    "juan": 1,
    "text_raw": "真性有為空",
    "citation": "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)",
    "score": 1.0,
    "cbeta_tag": "2026R2"
  }],
  "artifact_id": "2026R2+c1f1x7a0",
  "cbeta_tag": "2026R2"
}"#,
    )
    .unwrap_or_else(|e| panic!("write last.json: {e}"));
    assert!(last_path.is_file());

    let out = bin_fetched_home(&home, &index)
        .args(["use", "2026R2", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("use after last.json: {e}"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !last_path.exists(),
        "successful use must remove prior last.json"
    );

    let from = bin_fetched_home(&home, &index)
        .args(["get", "--from", "last", "--index", "1"])
        .output()
        .unwrap_or_else(|e| panic!("get --from last: {e}"));
    assert_eq!(
        from.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&from.stderr)
    );
    let err = String::from_utf8_lossy(&from.stderr);
    assert!(
        err.contains("no last.json") || err.contains("last.json"),
        "stderr should mention missing last.json: {err}"
    );

    let _ = fs::remove_dir_all(&home);
    let _ = fs::remove_dir_all(&index);
}
