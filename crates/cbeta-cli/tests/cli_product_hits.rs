//! Product boundary: `search --json` emits Hits with real mini-index data.

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};

#[test]
fn json_search_emits_hits_not_command() {
    // Given: mini corpus built into a temp index
    let corpus = mini_corpus();
    let index = temp_dir("product-hits");
    let build = cbeta_env(&corpus, &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .expect("build");
    assert_eq!(
        build.status.code(),
        Some(0),
        "build: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // When: simplified query 真性有为空
    let output = cbeta_env(&corpus, &index)
        .args(["search", "--json", "真性有为空"])
        .output()
        .expect("spawn cbeta");

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "search --json should exit 0; stderr={stderr} stdout={stdout}"
    );

    assert!(
        json_object_has_key(&stdout, "hits"),
        "expected Hits JSON with top-level `hits`; got:\n{stdout}"
    );
    assert!(
        stdout.contains("T30n1578_p0268b21"),
        "expected line_id T30n1578_p0268b21 in hits; got:\n{stdout}"
    );
    assert!(
        !json_object_has_key(&stdout, "action"),
        "must not dump Command; got:\n{stdout}"
    );
}

/// Std-only top-level object key probe.
fn json_object_has_key(json: &str, key: &str) -> bool {
    let trimmed = json.trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    let compact = format!("\"{key}\":");
    let spaced = format!("\"{key}\" :");
    trimmed.contains(&compact) || trimmed.contains(&spaced)
}
