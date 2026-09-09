//! CLI contract for L-near (#20): 2-gram recall + char-span confirm.
//!
//! Mini corpus has no 真如/缘起 — plant a RAM artifact via `write_artifact`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use cbeta_index::{write_artifact, IndexableLine};
use cbeta_parse::{GaijiMap, ParsedLine};
use common::{cbeta_env, mini_corpus, temp_dir};
use std::fs;
use std::path::Path;
use std::process::Output;

fn line(line_id: &str, text: &str) -> IndexableLine {
    IndexableLine {
        line: ParsedLine {
            line_id: line_id.into(),
            work_id: "T1578".into(),
            juan: 1,
            text_raw: text.into(),
            lb_n: "0001a01".into(),
        },
        title: "大乘掌珍論".into(),
        author: "清辯菩薩,玄奘".into(),
        citation: "(CBETA 2026.R2, T30, no. 1578, p. 1, a01)".into(),
        cbeta_tag: "2026R2".into(),
    }
}

fn planted_index() -> std::path::PathBuf {
    let index = temp_dir("near");
    let lines = vec![
        line("T30n1578_p0001a01", &format!("真如{}缘起", "中".repeat(5))),
        line("T30n1578_p0001a02", &format!("真如{}缘起", "中".repeat(20))),
        line("T30n1578_p0001a03", &format!("缘起{}真如", "中".repeat(3))),
        line("T30n1578_p0001a04", &format!("空性{}缘生", "中".repeat(20))),
    ];
    write_artifact(&index, "2026R2", "nearplant", &lines, &GaijiMap::default())
        .expect("write_artifact");
    fs::write(index.join("CURRENT"), "2026R2-nearplant\n").expect("CURRENT");
    index
}

fn run(index: &Path, args: &[&str]) -> Output {
    // corpus unused when index is pre-planted; still isolate env
    cbeta_env(&mini_corpus(), index)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn cbeta {args:?}: {e}"))
}

#[test]
fn explain_near_16() {
    // Given: any env (explain dumps Command, no index needed)
    // When: --explain --json NEAR/16
    // Then: mode=near within_chars=16 ordered=false
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cbeta"))
        .args(["search", "--explain", "--json", "真如 NEAR/16 缘起"])
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn explain");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pq = &v["parsed_query"];
    assert_eq!(pq["mode"], "near");
    assert_eq!(pq["within_chars"], 16);
    assert_eq!(pq["ordered"], false);
}

#[test]
fn search_near_16_exit_0() {
    // Given: planted index with close 真如…缘起 (span ≤ 16)
    let index = planted_index();
    // When: search NEAR/16
    let out = run(&index, &["search", "--json", "真如 NEAR/16 缘起"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Then: exit 0 and close line_id present; far line dropped
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("T30n1578_p0001a01"),
        "expected close hit; got {stdout}"
    );
    assert!(
        !stdout.contains("T30n1578_p0001a02"),
        "far line must not appear; got {stdout}"
    );
}

#[test]
fn search_plus_near_30_exit_0() {
    let index = planted_index();
    let out = run(&index, &["search", "--json", "空性+缘生"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("T30n1578_p0001a04"),
        "expected + hit; got {stdout}"
    );
}
