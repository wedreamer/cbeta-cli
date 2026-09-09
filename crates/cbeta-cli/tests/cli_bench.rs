//! Binary contract for `cbeta bench` (GitHub #37 part 2).
//!
//! Scholar-facing micro-benchmark over keyword / phrase / near against a built
//! index. TDD RED until the `bench` subcommand lands — do not implement here.
//!
//! Local SSD target (not asserted in CI): keyword p99_ms < 20. CI runners vary.

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::path::Path;
use std::process::Output;

fn build_ci_minimal(corpus: &Path, index: &Path) -> Output {
    cbeta_env(corpus, index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("spawn build: {e}"))
}

fn assert_build_ok(out: &Output) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "build exit 0; stdout={stdout} stderr={stderr}"
    );
}

fn assert_mode_stats(v: &serde_json::Value, mode: &str) {
    let m = v
        .get(mode)
        .unwrap_or_else(|| panic!("missing top-level key `{mode}`; got {v}"));
    assert!(m.is_object(), "`{mode}` must be object; got {m}");

    for key in ["p50_ms", "p99_ms", "qps"] {
        let n = m
            .get(key)
            .unwrap_or_else(|| panic!("`{mode}` missing `{key}`; got {m}"));
        assert!(
            n.as_f64().is_some(),
            "`{mode}.{key}` must be f64 number; got {n}"
        );
    }

    let n = m
        .get("n")
        .unwrap_or_else(|| panic!("`{mode}` missing `n`; got {m}"));
    assert_eq!(
        n.as_u64(),
        Some(50),
        "`{mode}.n` must be 50 (sample count); got {n}"
    );
    // Do NOT assert p99_ms < 20 here: CI runners vary. Local SSD target is
    // keyword p99_ms < 20ms (product budget, not a CI gate).
}

#[test]
fn bench_json_has_keyword_phrase_near_stats() {
    // Given: mini corpus (T0235+T1578 only) + built ci-minimal index
    let corpus = mini_corpus();
    let index = temp_dir("bench-json");
    assert_build_ok(&build_ci_minimal(&corpus, &index));

    // When: scholar runs bench with JSON report
    let out = cbeta_env(&corpus, &index)
        .args(["bench", "--scope", "ci-minimal", "--json"])
        .output()
        .unwrap_or_else(|e| panic!("spawn bench: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // Then: exit 0 and JSON object with mode stats + scope metadata
    assert_eq!(
        out.status.code(),
        Some(0),
        "bench --json exit 0; stdout={stdout} stderr={stderr}"
    );

    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("bench --json parse: {e}; stdout={stdout}");
    });
    assert!(v.is_object(), "bench --json must be object; got {v}");

    assert_eq!(
        v["scope"].as_str(),
        Some("ci-minimal"),
        "top-level scope; got {v}"
    );
    let artifact_id = v["artifact_id"]
        .as_str()
        .unwrap_or_else(|| panic!("artifact_id must be string; got {v}"));
    assert!(
        !artifact_id.is_empty(),
        "artifact_id must be non-empty; got {v}"
    );

    for mode in ["keyword", "phrase", "near"] {
        assert_mode_stats(&v, mode);
    }
}

#[test]
fn bench_missing_index_exits_2() {
    // Given: mini corpus path but empty index root (no build)
    let corpus = mini_corpus();
    let index = temp_dir("bench-no-index");

    // When: bench without a published artifact
    let out = cbeta_env(&corpus, &index)
        .args(["bench", "--scope", "ci-minimal", "--json"])
        .output()
        .unwrap_or_else(|e| panic!("spawn bench: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // Then: usage/index error exit 2 (rg semantics)
    assert_eq!(
        out.status.code(),
        Some(2),
        "bench without index must exit 2; stdout={stdout} stderr={stderr}"
    );
}
