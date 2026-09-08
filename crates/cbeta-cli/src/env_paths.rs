//! Resolve CBETA_CORPUS / CBETA_INDEX roots for the binary.

use std::env;
use std::path::PathBuf;

/// Default corpus cache under `$HOME` when `CBETA_CORPUS` is unset.
const DEFAULT_CORPUS_REL: &str = ".cbeta/corpus/2026R2";

/// Resolve corpus root: `CBETA_CORPUS` or `$HOME/.cbeta/corpus/2026R2`.
pub fn corpus_root() -> Result<PathBuf, String> {
    if let Ok(p) = env::var("CBETA_CORPUS") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = env::var_os("HOME").ok_or_else(|| {
        "HOME unset and CBETA_CORPUS not set; export CBETA_CORPUS or HOME".to_string()
    })?;
    Ok(PathBuf::from(home).join(DEFAULT_CORPUS_REL))
}

/// Re-export index root from cbeta-index for a single binary entry point.
pub fn index_root() -> Result<PathBuf, String> {
    cbeta_index::index_root().map_err(|e| e.to_string())
}
