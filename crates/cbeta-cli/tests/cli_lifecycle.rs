//! Wave 1–2 lifecycle surface: clap parse, releases, fetch offline paths.

use std::process::Command;

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_cbeta"));
    c.env("NO_COLOR", "1");
    c
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
