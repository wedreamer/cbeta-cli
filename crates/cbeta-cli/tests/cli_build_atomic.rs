//! Atomic index publish contract (GitHub #37 part 1).
//!
//! Rebuild must not `remove_dir_all` the live artifact; sidecars must be present
//! on the visible artifact; `CURRENT` must point at a complete package; previous
//! artifact directories remain. TDD RED until publish lands — do not "fix" by
//! weakening these assertions.

mod common;

use common::{cbeta_env, mini_corpus, temp_dir};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Mini `ci-minimal` artifact dir name (`{tag}-{scope_hash}`).
const ARTIFACT_NAME: &str = "2026R2-c1f1x7a0";

/// Sidecars that must exist inside the artifact the moment it is the live one.
const SIDECARS: &[&str] = &["cbeta-meta.json", "MANIFEST.json", "catalog.jsonl"];

/// Marker planted after the first build; `remove_dir_all` of the live dir destroys it.
const KEEP_MARKER: &str = "ATOMIC_KEEP_MARKER";

fn build_ci_minimal(corpus: &Path, index: &Path) -> Output {
    cbeta_env(corpus, index)
        .args(["build", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("spawn build: {e}"))
}

fn build_ci_minimal_full(corpus: &Path, index: &Path) -> Output {
    cbeta_env(corpus, index)
        .args(["build", "--full", "--scope", "ci-minimal"])
        .output()
        .unwrap_or_else(|e| panic!("spawn build --full: {e}"))
}

/// Leftover staging dir + PROGRESS.json (`cbeta-cli.progress/v1`) matching mini ci-minimal.
fn seed_leftover_tmp(index: &Path) -> PathBuf {
    let tmp = index.join(format!("{ARTIFACT_NAME}.tmp"));
    fs::create_dir_all(&tmp).unwrap_or_else(|e| panic!("mkdir leftover tmp: {e}"));
    // Poison proves --full wipe (resume would keep this file under a reused tmp).
    fs::write(tmp.join("POISON_LEFTOVER"), b"stale-tmp-must-die")
        .unwrap_or_else(|e| panic!("plant poison: {e}"));
    let progress = format!(
        r#"{{
  "schema": "cbeta-cli.progress/v1",
  "work_id": "T0235",
  "xml_sha256": "deadbeefcafebabe",
  "tag": "2026R2",
  "scope_hash": "c1f1x7a0"
}}
"#
    );
    fs::write(tmp.join("PROGRESS.json"), progress)
        .unwrap_or_else(|e| panic!("write PROGRESS.json: {e}"));
    tmp
}

/// CURRENT basename must name a complete final artifact — never a `.tmp` staging path.
fn assert_current_not_tmp(index: &Path) -> String {
    let name = read_current_name(index);
    assert!(
        !name.ends_with(".tmp"),
        "CURRENT must never publish a .tmp path; got {name:?}; index entries={:?}",
        list_names(index)
    );
    assert!(
        !name.contains(".tmp"),
        "CURRENT basename must not contain .tmp; got {name:?}"
    );
    let art = index.join(&name);
    assert_complete_artifact(&art);
    name
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

fn read_current_name(index: &Path) -> String {
    let raw = fs::read_to_string(index.join("CURRENT"))
        .unwrap_or_else(|e| panic!("read CURRENT under {}: {e}", index.display()));
    let name = raw.trim();
    assert!(
        !name.is_empty(),
        "CURRENT must name an artifact dir; got {raw:?}"
    );
    name.to_string()
}

/// Tantivy segment/meta files + product sidecars (complete publish package).
fn assert_complete_artifact(art: &Path) {
    assert!(
        art.is_dir(),
        "expected artifact directory at {}",
        art.display()
    );

    assert!(
        art.join("meta.json").is_file(),
        "expected Tantivy meta.json in {}",
        art.display()
    );

    let has_segment = fs::read_dir(art)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", art.display()))
        .filter_map(|e| e.ok())
        .any(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            s.ends_with(".store")
                || s.ends_with(".term")
                || s.ends_with(".idx")
                || s.ends_with(".fast")
        });
    assert!(
        has_segment,
        "expected Tantivy segment files under {}; entries={:?}",
        art.display(),
        list_names(art)
    );

    for side in SIDECARS {
        assert!(
            art.join(side).is_file(),
            "missing sidecar {side} in {}; entries={:?}",
            art.display(),
            list_names(art)
        );
    }
}

fn list_names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn first_build_artifact_has_tantivy_and_sidecars() {
    // Given: mini corpus + empty index root
    let corpus = mini_corpus();
    let index = temp_dir("atomic-first");

    // When: first `cbeta build --scope ci-minimal`
    assert_build_ok(&build_ci_minimal(&corpus, &index));

    // Then: CURRENT names the artifact; package is complete (Tantivy + sidecars)
    let name = read_current_name(&index);
    assert_eq!(
        name, ARTIFACT_NAME,
        "CURRENT should name the ci-minimal artifact"
    );
    let art = index.join(&name);
    assert_complete_artifact(&art);
}

#[test]
fn rebuild_preserves_previous_artifact_dir_and_current_stays_complete() {
    // Given: first successful build against mini corpus
    let corpus = mini_corpus();
    let index = temp_dir("atomic-rebuild");
    assert_build_ok(&build_ci_minimal(&corpus, &index));

    let first_name = read_current_name(&index);
    let first_art = index.join(&first_name);
    assert_complete_artifact(&first_art);

    // Identity proof: remove_dir_all on the live artifact destroys this marker.
    let marker = first_art.join(KEEP_MARKER);
    fs::write(&marker, b"generation-1").unwrap_or_else(|e| panic!("plant keep marker: {e}"));

    // Unrelated previous artifact (different scope hash) must also survive rebuild.
    let sibling = index.join("2026R2-oldscope99");
    fs::create_dir_all(&sibling).unwrap_or_else(|e| panic!("sibling dir: {e}"));
    fs::write(sibling.join("cbeta-meta.json"), b"{\"scope\":\"old\"}\n")
        .unwrap_or_else(|e| panic!("sibling meta: {e}"));

    // When: second build of the same scope (today: remove_dir_all live dir)
    assert_build_ok(&build_ci_minimal(&corpus, &index));

    // Then: first artifact directory still exists on disk (no live remove_dir_all)
    assert!(
        first_art.is_dir(),
        "first artifact dir must remain after rebuild at {}; index entries={:?}",
        first_art.display(),
        list_names(&index)
    );
    assert!(
        marker.is_file(),
        "rebuild must NOT remove_dir_all the live artifact dir (marker {} missing); \
         entries in first art={:?}",
        marker.display(),
        list_names(&first_art)
    );
    let marker_body = fs::read_to_string(&marker).expect("read marker");
    assert_eq!(
        marker_body, "generation-1",
        "keep marker contents must be unchanged"
    );

    assert!(
        sibling.is_dir() && sibling.join("cbeta-meta.json").is_file(),
        "unrelated previous artifact dirs must remain; index entries={:?}",
        list_names(&index)
    );

    // CURRENT still points at a valid complete artifact (Tantivy + sidecars)
    let cur_name = read_current_name(&index);
    let cur_art = index.join(&cur_name);
    assert_complete_artifact(&cur_art);

    // Scholar surface still works against the published pointer
    let info = cbeta_env(&corpus, &index)
        .args(["info", "--json"])
        .output()
        .unwrap_or_else(|e| panic!("spawn info: {e}"));
    let info_stdout = String::from_utf8_lossy(&info.stdout);
    let info_stderr = String::from_utf8_lossy(&info.stderr);
    assert_eq!(
        info.status.code(),
        Some(0),
        "info --json after rebuild; stdout={info_stdout} stderr={info_stderr}"
    );
    let v: serde_json::Value = serde_json::from_str(&info_stdout).unwrap_or_else(|e| {
        panic!("info --json parse: {e}; stdout={info_stdout}");
    });
    assert_eq!(
        v["artifact_id"].as_str(),
        Some("2026R2+c1f1x7a0"),
        "info artifact_id; got {v}"
    );
    assert_eq!(v["cbeta_tag"].as_str(), Some("2026R2"));
    assert_eq!(v["scope"].as_str(), Some("ci-minimal"));
    let path = v["index_path"].as_str().unwrap_or("");
    assert!(
        PathBuf::from(path).is_dir(),
        "info index_path must be a live dir; got {path:?}"
    );
}

#[test]
fn build_full_wipes_leftover_tmp_and_never_publishes_tmp_as_current() {
    // Given: empty index + leftover {artifact}.tmp with matching PROGRESS.json
    let corpus = mini_corpus();
    let index = temp_dir("atomic-full-wipe");
    let leftover = seed_leftover_tmp(&index);
    assert!(
        leftover.join("PROGRESS.json").is_file(),
        "seed must plant PROGRESS.json"
    );
    assert!(
        leftover.join("POISON_LEFTOVER").is_file(),
        "seed must plant poison marker"
    );

    // When: `cbeta build --full --scope ci-minimal` (decide_tmp → Wipe)
    assert_build_ok(&build_ci_minimal_full(&corpus, &index));

    // Then: leftover staging is gone (wiped + published away); poison cannot survive
    assert!(
        !leftover.exists() || !leftover.join("POISON_LEFTOVER").is_file(),
        "build --full must wipe leftover tmp; leftover still has poison; entries={:?}",
        if leftover.exists() {
            list_names(&leftover)
        } else {
            vec![]
        }
    );
    // Successful publish renames staging off the .tmp path — no incomplete PROGRESS left.
    if leftover.exists() {
        assert!(
            !leftover.join("PROGRESS.json").is_file(),
            "incomplete PROGRESS.json must not remain after successful --full; entries={:?}",
            list_names(&leftover)
        );
    }

    // CURRENT names a complete final artifact — never a .tmp basename
    let cur = assert_current_not_tmp(&index);
    assert!(
        cur.starts_with(ARTIFACT_NAME) || cur == ARTIFACT_NAME,
        "CURRENT should be ci-minimal artifact (or unique sibling); got {cur}"
    );

    // Incremental (no --full) still exit 0 against the published index
    assert_build_ok(&build_ci_minimal(&corpus, &index));
    let _ = assert_current_not_tmp(&index);
}
