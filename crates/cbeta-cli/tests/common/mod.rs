//! Shared helpers for cbeta-cli integration tests (mini corpus + temp index).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
pub fn cbeta_env(corpus: &Path, index: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cbeta"));
    cmd.env("CBETA_CORPUS", corpus);
    cmd.env("CBETA_INDEX", index);
    // Avoid host color / locale surprises in assertions.
    cmd.env("NO_COLOR", "1");
    cmd.env_remove("HOME");
    cmd
}
