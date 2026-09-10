//! Shared helpers for cbeta-cli integration tests (mini corpus + temp index).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Path to the in-repo mini corpus fixture (`tests/fixtures/mini`).
pub fn mini_corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
}

/// Unique temp directory under the system temp root (parallel-safe).
pub fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "cbeta-cli-{prefix}-{}-{}-{}",
        std::process::id(),
        nanos,
        n
    ));
    if let Err(e) = std::fs::create_dir_all(&p) {
        panic!("create temp dir {}: {e}", p.display());
    }
    p
}

/// Spawn `cbeta` with CBETA_CORPUS / CBETA_INDEX isolated from the host home.
///
/// Removes `HOME` so lifecycle paths cannot touch the developer home by accident.
pub fn cbeta_env(corpus: &Path, index: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cbeta"));
    cmd.env("CBETA_CORPUS", corpus);
    cmd.env("CBETA_INDEX", index);
    // Avoid host color / locale surprises in assertions.
    cmd.env("NO_COLOR", "1");
    cmd.env_remove("HOME");
    cmd
}

/// Like [`cbeta_env`], but sets a fake `HOME` (under `temp_dir`) for CURRENT/last.json tests.
///
/// Path contract (see `session.rs` `last_json_path`):
/// - when `CBETA_INDEX` is set → `last.json` is `$CBETA_INDEX/last.json`
/// - else → `$HOME/.cbeta/last.json` (beside `.cbeta`, not inside the index)
pub fn cbeta_env_with_home(corpus: &Path, index: &Path, home: &Path) -> Command {
    let cbeta_root = home.join(".cbeta");
    if let Err(e) = fs::create_dir_all(&cbeta_root) {
        panic!("create fake HOME .cbeta {}: {e}", cbeta_root.display());
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cbeta"));
    cmd.env("CBETA_CORPUS", corpus);
    cmd.env("CBETA_INDEX", index);
    cmd.env("NO_COLOR", "1");
    cmd.env("HOME", home);
    cmd
}

/// True only when `CBETA_FULL=1` (empty / `0` / unset → false).
///
/// Scholar / full-corpus tests gate on this; default CI stays fail-closed.
pub fn cbeta_full_enabled() -> bool {
    matches!(std::env::var("CBETA_FULL").as_deref(), Ok("1"))
}

/// Return `true` when the caller must return early (full-corpus lane off).
///
/// Prefer this over `#[ignore]` so the test stays compiled and counted in CI.
#[must_use]
pub fn skip_unless_cbeta_full() -> bool {
    !cbeta_full_enabled()
}

/// Commit pins for one release tag (xml-p5 / metadata / gaiji).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockPins {
    /// xml-p5 40-hex commit.
    pub xml_p5: String,
    /// metadata 40-hex commit.
    pub metadata: String,
    /// gaiji 40-hex commit.
    pub gaiji: String,
}

#[derive(Debug, Deserialize)]
struct LockFile {
    releases: BTreeMap<String, LockPinsSerde>,
}

#[derive(Debug, Deserialize)]
struct LockPinsSerde {
    #[serde(rename = "xml-p5")]
    xml_p5: String,
    metadata: String,
    gaiji: String,
}

/// Parse `CARGO_MANIFEST_DIR/data/releases.lock.yaml` pins for `tag` (no hard-coded SHAs).
pub fn lock_pins_for(tag: &str) -> LockPins {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/releases.lock.yaml");
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("read lock {}: {e}", path.display());
    });
    let lock: LockFile = serde_yaml::from_str(&raw).unwrap_or_else(|e| {
        panic!("parse releases.lock.yaml: {e}");
    });
    let pins = lock.releases.get(tag).unwrap_or_else(|| {
        panic!("tag {tag} missing from releases.lock.yaml");
    });
    LockPins {
        xml_p5: pins.xml_p5.clone(),
        metadata: pins.metadata.clone(),
        gaiji: pins.gaiji.clone(),
    }
}

/// Write a complete `FETCHED.yaml` under `tag_dir` matching product `FetchedYaml`.
///
/// First arg is the **tag directory** (`plant_mini_under_tag` return value), not cache root.
/// `cbeta_release` is always the concrete `tag` (never `"latest"`). Does not write CURRENT.
pub fn write_complete_fetched_yaml(tag_dir: &Path, tag: &str, pins: &LockPins) {
    assert_ne!(
        tag, "latest",
        "cbeta_release must never be the string latest"
    );
    fs::create_dir_all(tag_dir).unwrap_or_else(|e| {
        panic!("mkdir {}: {e}", tag_dir.display());
    });
    // Match crates/cbeta-cli/src/lifecycle/fetched.rs FetchedYaml field names exactly.
    let yaml = format!(
        "schema: cbeta-cli.fetched/v1\n\
         cbeta_release: {tag}\n\
         fetched_at: 1970-01-01T00:00:00Z\n\
         cache_root: {cache}\n\
         sources:\n\
           xml-p5:\n\
             tag: {tag}\n\
             commit: {xml}\n\
           metadata:\n\
             commit: {meta}\n\
           gaiji:\n\
             commit: {gaiji}\n",
        tag = tag,
        cache = tag_dir.display(),
        xml = pins.xml_p5,
        meta = pins.metadata,
        gaiji = pins.gaiji,
    );
    let path = tag_dir.join("FETCHED.yaml");
    fs::write(&path, yaml).unwrap_or_else(|e| {
        panic!("write {}: {e}", path.display());
    });
}

/// Copy in-repo mini corpus into `$home/.cbeta/corpus/{tag}/` and return that tag dir.
///
/// Does not live-clone, does not write CURRENT, does not overwrite the in-repo mini tree.
pub fn plant_mini_under_tag(home: &Path, tag: &str) -> PathBuf {
    let tag_dir = home.join(".cbeta").join("corpus").join(tag);
    fs::create_dir_all(&tag_dir).unwrap_or_else(|e| {
        panic!("mkdir {}: {e}", tag_dir.display());
    });
    copy_dir_recursive(&mini_corpus(), &tag_dir);
    tag_dir
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap_or_else(|e| {
        panic!("mkdir {}: {e}", dst.display());
    });
    let entries = fs::read_dir(src).unwrap_or_else(|e| {
        panic!("read_dir {}: {e}", src.display());
    });
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("dir entry: {e}"));
        let ty = entry
            .file_type()
            .unwrap_or_else(|e| panic!("file_type: {e}"));
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to);
        } else if ty.is_file() {
            fs::copy(&from, &to).unwrap_or_else(|e| {
                panic!("copy {} -> {}: {e}", from.display(), to.display());
            });
        }
    }
}
