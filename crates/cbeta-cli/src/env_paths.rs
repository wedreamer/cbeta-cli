//! Resolve CBETA_CORPUS / CBETA_INDEX roots for the binary.

use std::env;
use std::path::{Path, PathBuf};

use crate::lifecycle::paths::{current_tag, tag_dir};

/// Resolve corpus root: `CBETA_CORPUS` or `$HOME/.cbeta/corpus/<CURRENT>`.
pub fn corpus_root() -> Result<PathBuf, String> {
    if let Ok(p) = env::var("CBETA_CORPUS") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let tag = current_tag()?;
    tag_dir(&tag)
}

/// TEI XML root: fixture layout `xml-p5/` or fetch.sh cache `src/xml-p5/`.
pub fn xml_p5_root(corpus: &Path) -> PathBuf {
    let top = corpus.join("xml-p5");
    if top.is_dir() {
        top
    } else {
        corpus.join("src").join("xml-p5")
    }
}

/// Re-export index root from cbeta-index for a single binary entry point.
pub fn index_root() -> Result<PathBuf, String> {
    cbeta_index::index_root().map_err(|e| e.to_string())
}

/// Process-wide lock for tests that mutate `CBETA_*` / `HOME` / `NO_COLOR`.
///
/// WHY: rustc runs unit tests in parallel inside one binary; per-module mutexes
/// still race on the process environment.
#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::paths::FETCH_HINT;

    #[test]
    fn corpus_root_prefers_cbeta_corpus_env() {
        let _g = env_lock();
        std::env::set_var("CBETA_CORPUS", "/tmp/cbeta-corpus-ut");
        let p = corpus_root().unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cbeta-corpus-ut"));
        std::env::remove_var("CBETA_CORPUS");
    }

    #[test]
    fn corpus_root_falls_back_to_current_pointer() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-home-ut-corpus-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&base);
        let cache = base.join(".cbeta").join("corpus");
        std::fs::create_dir_all(cache.join("2026R2")).unwrap();
        std::fs::write(cache.join("CURRENT"), "2026R2\n").unwrap();
        std::env::set_var("CBETA_CORPUS", "");
        std::env::set_var("HOME", &base);
        let p = corpus_root().unwrap();
        assert_eq!(p, cache.join("2026R2"));
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("HOME");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn corpus_root_errors_without_current_mentions_fetch() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-home-no-cur-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join(".cbeta").join("corpus")).unwrap();
        std::env::remove_var("CBETA_CORPUS");
        std::env::set_var("HOME", &base);
        let err = corpus_root().unwrap_err();
        assert!(
            err.contains(FETCH_HINT),
            "expected fetch hint in err: {err}"
        );
        std::env::remove_var("HOME");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn corpus_root_errors_without_home_or_corpus() {
        let _g = env_lock();
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("HOME");
        let err = corpus_root().unwrap_err();
        assert!(err.contains("HOME") || err.contains(FETCH_HINT));
    }

    #[test]
    fn xml_p5_root_prefers_top_level_then_src() {
        let base = std::env::temp_dir().join(format!(
            "cbeta-xml-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("src/xml-p5")).unwrap();
        assert_eq!(xml_p5_root(&base), base.join("src/xml-p5"));
        std::fs::create_dir_all(base.join("xml-p5")).unwrap();
        assert_eq!(xml_p5_root(&base), base.join("xml-p5"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn index_root_maps_cbeta_index_error_to_string() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        let err = index_root().unwrap_err();
        assert!(!err.is_empty());
    }
}
