//! Characterization tests for today's cbeta CLI scaffold contract.
//! Pins exit codes and `--json` Command dump (not Hits). Do not "upgrade" these.
//! Build is product-wired (IndexInfo); search/get/catalog/info/verify stay scaffold until later slices.

mod common;

fn cbeta() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
}

#[test]
fn help_exits_0() {
    let out = cbeta().arg("--help").output().expect("spawn cbeta --help");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn bare_search_exits_2() {
    // No index (isolated env): search reports missing index and exits 2.
    use common::{cbeta_env, temp_dir};
    let index = temp_dir("no-idx");
    let corpus = temp_dir("no-corpus");
    let out = cbeta_env(&corpus, &index)
        .arg("色即是空")
        .output()
        .expect("spawn bare search");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn json_dumps_command_not_hits() {
    // Product: search --json emits top-level hits (build mini index first).
    use common::{cbeta_env, mini_corpus, temp_dir};

    let corpus = mini_corpus();
    let index = temp_dir("search-json");
    let build = cbeta_env(&corpus, &index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .expect("build");
    assert_eq!(
        build.status.code(),
        Some(0),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let out = cbeta_env(&corpus, &index)
        .args(["search", "--json", "真性有为空"])
        .output()
        .expect("spawn search --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "search hits exit 0; stdout={stdout} stderr={stderr}"
    );

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is JSON");
    let obj = value.as_object().expect("JSON object");
    assert!(
        obj.contains_key("hits"),
        "expected top-level hits; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
    let hits = obj["hits"].as_array().expect("hits array");
    assert!(!hits.is_empty(), "expected at least one hit");
    assert_eq!(hits[0]["line_id"], "T30n1578_p0268b21");
}

#[test]
fn explain_prints_mode() {
    let out = cbeta()
        .args(["search", "--explain", "空性+缘生"])
        .output()
        .expect("spawn search --explain");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("mode=near"),
        "expected mode=near in stdout, got: {stdout}"
    );
}

#[test]
fn explain_json_still_command() {
    // Scaffold: --json wins over --explain human line; still dumps Command, not hits.
    let out = cbeta()
        .args(["search", "--explain", "--json", "空性"])
        .output()
        .expect("spawn search --explain --json");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Search);

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is JSON");
    let obj = value.as_object().expect("JSON object");
    assert!(
        !obj.contains_key("hits"),
        "scaffold --json dumps Command, not hits; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[test]
fn canon_filter_json() {
    let out = cbeta()
        .args(["search", "--explain", "--json", "--canon", "T", "空性"])
        .output()
        .expect("spawn search --json --canon");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert!(
        cmd.filters.canons.iter().any(|c| c == "T"),
        "expected filters.canons to contain T, got {:?}",
        cmd.filters.canons
    );
}

#[test]
fn search_fullwidth_exits_2() {
    let out = cbeta()
        .args(["search", "空性＋缘生"])
        .output()
        .expect("spawn search fullwidth");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fullwidth"),
        "expected fullwidth in stderr, got: {stderr}"
    );
}

#[test]
fn catalog_json_dumps_command() {
    // Scaffold dump only until catalog product wire.
    let out = cbeta()
        .args(["--json", "catalog"])
        .output()
        .expect("spawn --json catalog");
    assert_eq!(out.status.code(), Some(0));
    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Catalog);
}

#[test]
fn info_json_dumps_command() {
    // Scaffold dump only until info product wire.
    let out = cbeta()
        .args(["--json", "info"])
        .output()
        .expect("spawn --json info");
    assert_eq!(out.status.code(), Some(0));
    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Info);
}

#[test]
fn serve_exits_2() {
    // No --json: scaffold reports missing index and exits 2.
    let out = cbeta().arg("serve").output().expect("spawn serve");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn build_json_dumps_command() {
    // Product: build --json against mini fixture emits IndexInfo, not Command.
    use common::{cbeta_env, mini_corpus, temp_dir};

    let corpus = mini_corpus();
    let index = temp_dir("build-json");
    let out = cbeta_env(&corpus, &index)
        .args(["--json", "build", "--scope", "ci-minimal"])
        .output()
        .expect("spawn --json build --scope ci-minimal");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "build --json exit 0; stdout={stdout} stderr={stderr}"
    );

    let info: cbeta_core::IndexInfo =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as IndexInfo");
    assert_eq!(info.cbeta_tag, "2026R2");
    assert_eq!(info.scope, "ci-minimal");
    assert_eq!(info.artifact_id, "2026R2+c1f1x7a0");
    assert!(info.work_count >= 1, "work_count={}", info.work_count);

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    let obj = value.as_object().expect("object");
    assert!(
        !obj.contains_key("action"),
        "must not dump Command; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[test]
fn verify_json_dumps_command() {
    // Scaffold dump only: parent --json precedes verify.
    let out = cbeta()
        .args(["--json", "verify", "色即是空"])
        .output()
        .expect("spawn --json verify");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Verify);
}

#[test]
fn get_json_dumps_command() {
    // Product: get known line_id against mini index → {hit: ...}.
    use common::{cbeta_env, mini_corpus, temp_dir};
    let corpus = mini_corpus();
    let index = temp_dir("get-json");
    assert_eq!(
        cbeta_env(&corpus, &index)
            .args(["build", "--scope", "ci-minimal"])
            .output()
            .expect("build")
            .status
            .code(),
        Some(0)
    );
    let out = cbeta_env(&corpus, &index)
        .args(["--json", "get", "T30n1578_p0268b21"])
        .output()
        .expect("get");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["hit"]["line_id"], "T30n1578_p0268b21");
    assert!(!v.as_object().unwrap().contains_key("action"));
}

#[test]
fn get_missing_line_id_exits_1() {
    use common::{cbeta_env, mini_corpus, temp_dir};
    let corpus = mini_corpus();
    let index = temp_dir("get-miss");
    assert_eq!(
        cbeta_env(&corpus, &index)
            .args(["build", "--scope", "ci-minimal"])
            .output()
            .expect("build")
            .status
            .code(),
        Some(0)
    );
    // Ghost id must not be invented.
    let out = cbeta_env(&corpus, &index)
        .args(["get", "T30n1578_p0268a12"])
        .output()
        .expect("get miss");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn get_json_has_no_parsed_query() {
    // Product get JSON is {hit}, never Command / parsed_query.
    use common::{cbeta_env, mini_corpus, temp_dir};
    let corpus = mini_corpus();
    let index = temp_dir("get-noparse");
    assert_eq!(
        cbeta_env(&corpus, &index)
            .args(["build", "--scope", "ci-minimal"])
            .output()
            .expect("build")
            .status
            .code(),
        Some(0)
    );
    let out = cbeta_env(&corpus, &index)
        .args(["--json", "get", "T30n1578_p0268b21"])
        .output()
        .expect("get");
    assert_eq!(out.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let obj = value.as_object().expect("object");
    assert!(!obj.contains_key("parsed_query") && !obj.contains_key("action"));
    assert!(obj.contains_key("hit"));
}

#[test]
fn verify_json_has_no_parsed_query() {
    // Given: verify pasted text with --json
    // When: CLI dumps Command
    // Then: action=Verify and parsed_query is absent
    let out = cbeta()
        .args(["--json", "verify", "色即是空"])
        .output()
        .expect("spawn --json verify");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Verify);
    assert!(
        cmd.parsed_query.is_none(),
        "verify must not run parse_query; got {:?}",
        cmd.parsed_query
    );
}

#[test]
fn build_json_has_no_parsed_query() {
    // Given: product build --json against mini fixture
    // When: stdout is IndexInfo
    // Then: no parsed_query / action keys (scope never goes through parse_query)
    use common::{cbeta_env, mini_corpus, temp_dir};

    let corpus = mini_corpus();
    let index = temp_dir("build-noparse");
    let out = cbeta_env(&corpus, &index)
        .args(["--json", "build", "--scope", "ci-minimal"])
        .output()
        .expect("spawn --json build");
    assert_eq!(out.status.code(), Some(0));

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    let obj = value.as_object().expect("object");
    assert!(
        !obj.contains_key("parsed_query") && !obj.contains_key("action"),
        "build product JSON must not be Command; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
    assert!(obj.contains_key("artifact_id"), "expected IndexInfo shape");
}

#[test]
fn author_type_flags_json() {
    // Given: search with --author / --type filters
    // When: --json dumps Command
    // Then: filters.authors and filters.types carry the flag values
    let out = cbeta()
        .args([
            "search",
            "--explain",
            "--json",
            "--author",
            "玄奘",
            "--type",
            "lun",
            "空性",
        ])
        .output()
        .expect("spawn search --json --author --type");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Search);
    assert!(
        cmd.filters.authors.iter().any(|a| a == "玄奘"),
        "expected filters.authors to contain 玄奘, got {:?}",
        cmd.filters.authors
    );
    assert!(
        cmd.filters.types.iter().any(|t| t == "lun"),
        "expected filters.types to contain lun, got {:?}",
        cmd.filters.types
    );
}
