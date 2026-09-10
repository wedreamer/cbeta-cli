//! L-save product contracts: `--save last` / `--from last` / `--index` (GitHub #39).
//!
//! TDD RED until session save lands — do not implement production code here.
//! last.json lives under `CBETA_INDEX` when that env is set (not `$HOME/.cbeta`).

mod common;

use common::{cbeta_env, cbeta_env_with_home, mini_corpus, temp_dir};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

const QUERY: &str = "真性有为空";

fn built() -> (PathBuf, PathBuf) {
    let corpus = mini_corpus();
    let index = temp_dir("save");
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

fn stderr_utf8(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap_or_else(|e| panic!("stderr utf-8: {e}"))
}

fn last_json_path(index: &Path) -> PathBuf {
    index.join("last.json")
}

fn save_last(corpus: &Path, index: &Path) -> Output {
    run(
        corpus,
        index,
        &["search", "--json", "--save", "last", QUERY],
    )
}

#[test]
fn save_last_then_from_get_index_with_context() {
    // Given: mini index (T0235+T1578) built ci-minimal
    let (corpus, index) = built();

    // When: search --json --save last
    let save = save_last(&corpus, &index);
    let save_stdout = stdout_utf8(&save);
    let save_stderr = stderr_utf8(&save);
    assert_eq!(
        save.status.code(),
        Some(0),
        "search --save last exit 0; stdout={save_stdout} stderr={save_stderr}"
    );

    // Then: last.json under CBETA_INDEX (not $HOME/.cbeta)
    let last_path = last_json_path(&index);
    assert!(
        last_path.is_file(),
        "last.json must exist at {}; stderr={save_stderr}",
        last_path.display()
    );
    let raw = fs::read_to_string(&last_path)
        .unwrap_or_else(|e| panic!("read last.json {}: {e}", last_path.display()));
    let last: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse last.json: {e}; raw={raw}"));
    for key in ["query", "filters", "hits", "artifact_id"] {
        assert!(
            last.get(key).is_some(),
            "last.json missing `{key}`; got {last}"
        );
    }
    let hits = last["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("hits array; got {last}"));
    assert!(!hits.is_empty(), "hits non-empty; got {last}");
    let saved_line_id = hits[0]["line_id"]
        .as_str()
        .unwrap_or_else(|| panic!("hits[0].line_id; got {last}"))
        .to_string();
    assert!(!saved_line_id.is_empty(), "line_id non-empty; got {last}");
    let saved_artifact = last["artifact_id"]
        .as_str()
        .unwrap_or_else(|| panic!("artifact_id string; got {last}"))
        .to_string();

    // When: get --from last --index 1 -C 4 --json
    let get = run(
        &corpus,
        &index,
        &["get", "--from", "last", "--index", "1", "-C", "4", "--json"],
    );
    let get_stdout = stdout_utf8(&get);
    let get_stderr = stderr_utf8(&get);
    assert_eq!(
        get.status.code(),
        Some(0),
        "get --from last --index 1 -C 4 --json; stdout={get_stdout} stderr={get_stderr}"
    );
    let v: serde_json::Value = serde_json::from_str(get_stdout.trim())
        .unwrap_or_else(|e| panic!("get json: {e}; stdout={get_stdout}"));
    let hit = v
        .get("hit")
        .unwrap_or_else(|| panic!("missing hit; {get_stdout}"));
    assert_eq!(
        hit["line_id"].as_str(),
        Some(saved_line_id.as_str()),
        "hit.line_id must match last.json hits[0]; got {v}"
    );

    // Then: does not rebuild index (artifact_id unchanged on disk)
    let raw_after =
        fs::read_to_string(&last_path).unwrap_or_else(|e| panic!("re-read last.json: {e}"));
    let last_after: serde_json::Value = serde_json::from_str(&raw_after)
        .unwrap_or_else(|e| panic!("re-parse last.json: {e}; raw={raw_after}"));
    assert_eq!(
        last_after["artifact_id"].as_str(),
        Some(saved_artifact.as_str()),
        "artifact_id must stay stable (no rebuild); before={saved_artifact} after={last_after}"
    );
}

#[test]
fn from_last_copy_n_prints_notes_block() {
    // Given: mini index + search --save last
    let (corpus, index) = built();
    let save = save_last(&corpus, &index);
    assert_eq!(
        save.status.code(),
        Some(0),
        "search --save last; stderr={}",
        stderr_utf8(&save)
    );

    // When: bare `cbeta --from last --copy 1` (no get subcommand)
    let out = run(&corpus, &index, &["--from", "last", "--copy", "1"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: exit 0; notes/citation block on stdout (no OS clipboard)
    assert_eq!(
        out.status.code(),
        Some(0),
        "cbeta --from last --copy 1; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("CBETA") || stdout.contains("(CBETA"),
        "notes/citation block; got:\n{stdout}"
    );
    assert!(
        !stderr.to_lowercase().contains("clipboard"),
        "no clipboard API noise; stderr={stderr}"
    );
}

#[test]
fn from_last_artifact_mismatch_exits_2() {
    // Given: mini index + search --save last, then stale artifact_id
    let (corpus, index) = built();
    let save = save_last(&corpus, &index);
    assert_eq!(
        save.status.code(),
        Some(0),
        "search --save last; stderr={}",
        stderr_utf8(&save)
    );
    let last_path = last_json_path(&index);
    let raw = fs::read_to_string(&last_path).unwrap_or_else(|e| panic!("read last.json: {e}"));
    let mut last: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse last.json: {e}; raw={raw}"));
    last["artifact_id"] = serde_json::Value::String("2026R2+deadbeef".to_string());
    fs::write(
        &last_path,
        serde_json::to_string_pretty(&last).expect("serialize last.json"),
    )
    .unwrap_or_else(|e| panic!("rewrite last.json: {e}"));

    // When: get --from last --index 1 against mismatched artifact
    let out = run(&corpus, &index, &["get", "--from", "last", "--index", "1"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: exit 2; hint to re-run search / index changed; no successful get passage
    assert_eq!(
        out.status.code(),
        Some(2),
        "artifact mismatch exit 2; stdout={stdout} stderr={stderr}"
    );
    let err_l = stderr.to_lowercase();
    assert!(
        err_l.contains("index changed") || err_l.contains("re-run search"),
        "stderr must mention index changed or re-run search; got:\n{stderr}"
    );
    // Must NOT auto re-search into a successful get body.
    assert!(
        !stdout.contains("T30n1578_p0268b21") || out.status.code() != Some(0),
        "must not auto re-search successful get; stdout={stdout}"
    );
    assert_ne!(
        out.status.code(),
        Some(0),
        "must not succeed on mismatch; stdout={stdout}"
    );
}

#[test]
fn save_rejects_non_last_name() {
    // Given: mini index
    let (corpus, index) = built();

    // When: search --save other (only `last` is allowed)
    let out = run(&corpus, &index, &["search", "--save", "other", QUERY]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: usage exit 2
    assert_eq!(
        out.status.code(),
        Some(2),
        "search --save other must exit 2; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn from_missing_last_json_exits_2() {
    // Given: empty index dir (no last.json; may also lack a built index)
    let corpus = mini_corpus();
    let index = temp_dir("save-no-last");
    assert!(
        !last_json_path(&index).exists(),
        "precondition: no last.json"
    );

    // When: get --from last --index 1
    let out = run(&corpus, &index, &["get", "--from", "last", "--index", "1"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: exit 2 (missing last.json and/or no index — both ok; never 0)
    assert_eq!(
        out.status.code(),
        Some(2),
        "missing last.json exit 2; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn from_index_out_of_range_exits_1() {
    // Given: mini index + search --save last (some hits)
    let (corpus, index) = built();
    let save = save_last(&corpus, &index);
    assert_eq!(
        save.status.code(),
        Some(0),
        "search --save last; stderr={}",
        stderr_utf8(&save)
    );

    // When: get --from last --index 999 (out of range)
    let out = run(
        &corpus,
        &index,
        &["get", "--from", "last", "--index", "999"],
    );
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);

    // Then: no-hit style exit 1
    assert_eq!(
        out.status.code(),
        Some(1),
        "index 999 out of range exit 1; stdout={stdout} stderr={stderr}"
    );
}

#[test]
fn save_last_writes_index_not_home() {
    // Given: mini index + fake HOME (proves last.json is NOT under ~/.cbeta)
    let corpus = mini_corpus();
    let index = temp_dir("save-index-home");
    let home = temp_dir("save-fake-home");
    let build = cbeta_env_with_home(&corpus, &index, &home)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("build: {e}"));
    assert_eq!(
        build.status.code(),
        Some(0),
        "build: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // When: search --json --save last with CBETA_INDEX set and fake HOME
    let save = cbeta_env_with_home(&corpus, &index, &home)
        .args(["search", "--json", "--save", "last", QUERY])
        .output()
        .unwrap_or_else(|e| panic!("search --save last: {e}"));
    let save_stdout = stdout_utf8(&save);
    let save_stderr = stderr_utf8(&save);
    assert_eq!(
        save.status.code(),
        Some(0),
        "search --save last exit 0; stdout={save_stdout} stderr={save_stderr}"
    );

    // Then: $CBETA_INDEX/last.json exists; $HOME/.cbeta/last.json does not
    let index_last = index.join("last.json");
    let home_last = home.join(".cbeta").join("last.json");
    assert!(
        index_last.is_file(),
        "last.json must be under CBETA_INDEX {}; stderr={save_stderr}",
        index_last.display()
    );
    assert!(
        !home_last.exists(),
        "last.json must NOT be under HOME/.cbeta {}; index has {}",
        home_last.display(),
        index_last.display()
    );
}
