//! Characterization tests for today's cbeta CLI scaffold contract.
//! Pins exit codes and `--json` Command dump (not Hits). Do not "upgrade" these.

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
    // No --json / --explain: scaffold reports missing index and exits 2.
    let out = cbeta().arg("色即是空").output().expect("spawn bare search");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn json_dumps_command_not_hits() {
    let out = cbeta()
        .args(["search", "--json", "空性"])
        .output()
        .expect("spawn search --json");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Search);
    assert_eq!(cmd.q.as_deref(), Some("空性"));

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is JSON");
    let obj = value.as_object().expect("JSON object");
    assert!(
        !obj.contains_key("hits"),
        "scaffold --json dumps Command, not hits; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
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
        .args(["search", "--json", "--canon", "T", "空性"])
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
    // Scaffold dump only: parent --json must precede subcommand (not product protocol).
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
    // Scaffold dump only: parent --json must precede subcommand (not product protocol).
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
    // Scaffold dump only: parent --json + --scope; do NOT treat scope-through-parse_query as product.
    let out = cbeta()
        .args(["--json", "build", "--scope", "taisho"])
        .output()
        .expect("spawn --json build --scope");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Build);
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
    // Scaffold dump only: parent --json precedes get.
    let out = cbeta()
        .args(["--json", "get", "T31n1585_p0001a12"])
        .output()
        .expect("spawn --json get");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Get);
}

#[test]
fn get_json_has_no_parsed_query() {
    // Given: get by line_id with --json
    // When: CLI dumps Command
    // Then: action=Get and parsed_query is absent (not run through parse_query)
    let out = cbeta()
        .args(["--json", "get", "T31n1585_p0001a12"])
        .output()
        .expect("spawn --json get");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Get);
    assert!(
        cmd.parsed_query.is_none(),
        "get must not run parse_query; got {:?}",
        cmd.parsed_query
    );

    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is JSON");
    let obj = value.as_object().expect("JSON object");
    assert!(
        !obj.contains_key("parsed_query"),
        "parsed_query must be skipped when None; keys={:?}",
        obj.keys().collect::<Vec<_>>()
    );
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
    // Given: build --scope with --json
    // When: CLI dumps Command
    // Then: action=Build and parsed_query is absent (scope is not a search query)
    let out = cbeta()
        .args(["--json", "build", "--scope", "taisho"])
        .output()
        .expect("spawn --json build --scope");
    assert_eq!(out.status.code(), Some(0));

    let cmd: cbeta_core::Command =
        serde_json::from_slice(&out.stdout).expect("stdout deserializes as Command");
    assert_eq!(cmd.action, cbeta_core::Action::Build);
    assert!(
        cmd.parsed_query.is_none(),
        "build scope must not run parse_query; got {:?}",
        cmd.parsed_query
    );
}

#[test]
fn author_type_flags_json() {
    // Given: search with --author / --type filters
    // When: --json dumps Command
    // Then: filters.authors and filters.types carry the flag values
    let out = cbeta()
        .args([
            "search",
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
