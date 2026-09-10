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
