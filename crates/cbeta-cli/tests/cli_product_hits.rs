//! Tier-P product boundary: after L-search, `search --json` emits Hits, not Command.
//! Kept `#[ignore = "L-search"]` so default CI/`cargo test` skips it; run with
//! `--ignored` to prove today's scaffold still dumps Command (this test must fail).

use std::process::Command;

#[test]
#[ignore = "L-search"]
fn json_search_emits_hits_not_command() {
    // Given: current scaffold — `--json` pretty-prints `cbeta_core::Command`, not Hits.
    // When: product search JSON for a simple query.
    let output = Command::new(env!("CARGO_BIN_EXE_cbeta"))
        .args(["search", "--json", "空性"])
        .output()
        .expect("spawn cbeta");

    let stdout = String::from_utf8(output.stdout).expect("stdout utf-8");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "search --json should exit 0; status={:?} stderr={stderr}",
        output.status
    );

    // Then: future product shape is a JSON object with a top-level `hits` key.
    // Do NOT treat deserializing as Command as success — that passes on today's scaffold.
    assert!(
        json_object_has_key(&stdout, "hits"),
        "expected Hits JSON with top-level `hits`; got Command-shaped dump:\n{stdout}"
    );
}

/// Std-only top-level object key probe (integration crate has no serde_json dep).
fn json_object_has_key(json: &str, key: &str) -> bool {
    let trimmed = json.trim();
    if !trimmed.starts_with('{') {
        return false;
    }
    // serde_json compact (`"hits":`) or spaced (`"hits" :`) member form.
    let compact = format!("\"{key}\":");
    let spaced = format!("\"{key}\" :");
    trimmed.contains(&compact) || trimmed.contains(&spaced)
}
