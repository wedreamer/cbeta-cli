//! CLI contract for `cbeta verify` (L-verify / issue #18).

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::path::{Path, PathBuf};
use std::process::Output;

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("verify");
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
fn verify_json_shape() {
    // Given: mini index (T0235 + T1578)
    let (corpus, index) = built();
    // When: exact original
    let exact = run(
        &corpus,
        &index,
        &["verify", "--json", "真性有為空，如幻緣生故"],
    );
    let exact_out = stdout_utf8(&exact);
    assert_eq!(
        exact.status.code(),
        Some(0),
        "exact; stderr={}",
        String::from_utf8_lossy(&exact.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&exact_out).unwrap_or_else(|e| panic!("json: {e}; {exact_out}"));
    assert_eq!(v["is_original"], true);
    assert_eq!(v["exact_hit"]["line_id"], "T30n1578_p0268b21");
    assert_eq!(v["similar"], serde_json::json!([]));
    assert!(
        v.get("action").is_none(),
        "must not dump Command; got {exact_out}"
    );

    // When: issue #18 异文 verse
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
        "variant; stderr={}",
        String::from_utf8_lossy(&variant.stderr)
    );
    let vv: serde_json::Value =
        serde_json::from_str(&var_out).unwrap_or_else(|e| panic!("json: {e}; {var_out}"));
    assert_eq!(vv["is_original"], false);
    assert!(
        vv["exact_hit"].is_null(),
        "exact_hit must be null; {var_out}"
    );
    let similar = vv["similar"].as_array().expect("similar array");
    assert!(!similar.is_empty(), "expected similar; got {var_out}");
    assert_eq!(similar[0]["work_id"], "T1578");
}

#[test]
fn verify_tty_not_original() {
    // Given: mini index
    let (corpus, index) = built();
    // When: 异文 verse on TTY
    let out = run(
        &corpus,
        &index,
        &["verify", "真性有为空，缘生故如幻，无为无起灭，不实若空华。"],
    );
    let stdout = stdout_utf8(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: not original; shows similar work_id / line_id
    assert_eq!(
        out.status.code(),
        Some(0),
        "tty variant; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("is_original=false"),
        "expected is_original=false; got:\n{stdout}"
    );
    assert!(
        stdout.contains("T1578") || stdout.contains("T30n1578"),
        "expected T1578 similar; got:\n{stdout}"
    );
}
