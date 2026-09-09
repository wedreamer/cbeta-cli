//! CLI contract for L-dsl (#19): `--explain --json` dumps Command with ordered/within_chars.

#![allow(clippy::expect_used, clippy::unwrap_used)]

fn cbeta() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
}

fn explain_json(q: &str) -> (i32, serde_json::Value, String) {
    let out = cbeta()
        .args(["search", "--explain", "--json", q])
        .output()
        .expect("spawn cbeta search --explain --json");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let value = if stdout.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null)
    };
    (
        code,
        value,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn explain_json_plus_near_ordered_false() {
    let (code, value, stderr) = explain_json("空性+缘生");
    assert_eq!(code, 0, "stderr={stderr}");
    let pq = &value["parsed_query"];
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["within_chars"], 30);
    assert_eq!(pq["ordered"], false);
}

#[test]
fn explain_json_star_before_ordered_true() {
    let (code, value, stderr) = explain_json("空性*缘生");
    assert_eq!(code, 0, "stderr={stderr}");
    let pq = &value["parsed_query"];
    assert_eq!(pq["mode"], "before");
    assert_eq!(pq["within_chars"], 30);
    assert_eq!(pq["ordered"], true);
}

#[test]
fn explain_json_near_16_ordered_false() {
    let (code, value, stderr) = explain_json("真如 NEAR/16 缘起");
    assert_eq!(code, 0, "stderr={stderr}");
    let pq = &value["parsed_query"];
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["within_chars"], 16);
    assert_eq!(pq["ordered"], false);
}

#[test]
fn explain_json_lotus_wildcard() {
    let (code, value, stderr) = explain_json("莲?色");
    assert_eq!(code, 0, "stderr={stderr}");
    assert_eq!(value["parsed_query"]["mode"], "wildcard");
}

#[test]
fn explain_json_fullwidth_plus_exit_2() {
    let out = cbeta()
        .args(["search", "--explain", "--json", "空性＋缘生"])
        .output()
        .expect("spawn fullwidth");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fullwidth") || stderr.contains("halfwidth"),
        "stderr should ask halfwidth, got: {stderr}"
    );
}
