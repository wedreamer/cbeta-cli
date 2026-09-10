//! Corpus cache layout: `$HOME/.cbeta/corpus/<tag>/` + `CURRENT` pointer.

use std::env;
use std::fs;
use std::path::PathBuf;

/// Human hint when CURRENT is missing (also used by search NoIndex copy).
pub const FETCH_HINT: &str = "cbeta fetch --release 2026R2 --scope taisho";

/// Cache root `$HOME/.cbeta/corpus` (not a tag directory).
pub fn corpus_cache_root() -> Result<PathBuf, String> {
    let home = env::var_os("HOME")
        .ok_or_else(|| format!("HOME unset; export HOME or CBETA_CORPUS, or run: {FETCH_HINT}"))?;
    Ok(PathBuf::from(home).join(".cbeta").join("corpus"))
}

/// Read one-line tag from `$HOME/.cbeta/corpus/CURRENT`.
pub fn current_tag() -> Result<String, String> {
    let path = corpus_cache_root()?.join("CURRENT");
    let raw = fs::read_to_string(&path).map_err(|_| {
        format!(
            "no CURRENT corpus tag under {}; run: {FETCH_HINT}",
            path.display()
        )
    })?;
    let tag = raw.trim();
    if tag.is_empty() {
        return Err(format!(
            "CURRENT file empty at {}; run: {FETCH_HINT}",
            path.display()
        ));
    }
    Ok(tag.to_string())
}

/// Tag directory under the cache root (`…/corpus/<tag>`).
pub fn tag_dir(tag: &str) -> Result<PathBuf, String> {
    Ok(corpus_cache_root()?.join(tag))
}

/// Atomically write corpus `CURRENT` (one-line tag) under the cache root.
///
/// Same pattern as index `write_current_pointer`: `{cache}/CURRENT.tmp` + sync + rename.
///
/// # Errors
/// IO failure creating parent or renaming.
pub fn write_corpus_current(tag: &str) -> Result<(), String> {
    let root = corpus_cache_root()?;
    fs::create_dir_all(&root).map_err(|e| format!("mkdir {}: {e}", root.display()))?;
    let final_path = root.join("CURRENT");
    let tmp_path = root.join("CURRENT.tmp");
    if tmp_path.exists() {
        fs::remove_file(&tmp_path).map_err(|e| format!("remove CURRENT.tmp: {e}"))?;
    }
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp_path).map_err(|e| format!("create CURRENT.tmp: {e}"))?;
        f.write_all(format!("{tag}\n").as_bytes())
            .map_err(|e| format!("write CURRENT.tmp: {e}"))?;
        f.sync_all().map_err(|e| format!("sync CURRENT.tmp: {e}"))?;
    }
    fs::rename(&tmp_path, &final_path).map_err(|e| format!("rename CURRENT: {e}"))?;
    Ok(())
}

/// Write `{cache}/DEFAULT` one-line concrete tag (use --default).
///
/// # Errors
/// IO failure.
pub fn write_corpus_default(tag: &str) -> Result<(), String> {
    let root = corpus_cache_root()?;
    fs::create_dir_all(&root).map_err(|e| format!("mkdir {}: {e}", root.display()))?;
    let path = root.join("DEFAULT");
    fs::write(&path, format!("{tag}\n")).map_err(|e| format!("write DEFAULT: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_paths::env_lock;

    #[test]
    fn corpus_root_uses_current_pointer() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-current-ptr-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        let cache = base.join(".cbeta").join("corpus");
        fs::create_dir_all(cache.join("2025R3")).unwrap();
        fs::write(cache.join("CURRENT"), "2025R3\n").unwrap();

        std::env::remove_var("CBETA_CORPUS");
        std::env::set_var("HOME", &base);

        let root = crate::env_paths::corpus_root().unwrap();
        assert_eq!(root, cache.join("2025R3"));
        assert_eq!(current_tag().unwrap(), "2025R3");
        assert_eq!(tag_dir("2025R3").unwrap(), cache.join("2025R3"));

        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn current_tag_errors_without_file() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-no-current-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".cbeta").join("corpus")).unwrap();
        std::env::remove_var("CBETA_CORPUS");
        std::env::set_var("HOME", &base);
        let err = current_tag().unwrap_err();
        assert!(err.contains(FETCH_HINT), "err={err}");
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn write_corpus_current_roundtrip() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-write-cur-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".cbeta").join("corpus")).unwrap();
        std::env::set_var("HOME", &base);
        write_corpus_current("2026R2").unwrap();
        assert_eq!(current_tag().unwrap(), "2026R2");
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn write_corpus_default_writes_file() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-write-def-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".cbeta").join("corpus")).unwrap();
        std::env::set_var("HOME", &base);
        write_corpus_default("2026R2").unwrap();
        let raw = fs::read_to_string(base.join(".cbeta/corpus/DEFAULT")).unwrap();
        assert_eq!(raw.trim(), "2026R2");
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn write_corpus_current_removes_leftover_tmp() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-cur-tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        let cache = base.join(".cbeta").join("corpus");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("CURRENT.tmp"), "stale\n").unwrap();
        std::env::set_var("HOME", &base);
        write_corpus_current("2025R3").unwrap();
        assert!(!cache.join("CURRENT.tmp").exists());
        assert_eq!(current_tag().unwrap(), "2025R3");
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn empty_current_file_errors_with_fetch_hint() {
        let _g = env_lock();
        let base = std::env::temp_dir().join(format!(
            "cbeta-empty-cur-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        let cache = base.join(".cbeta").join("corpus");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("CURRENT"), "\n").unwrap();
        std::env::set_var("HOME", &base);
        let err = current_tag().unwrap_err();
        assert!(err.contains(FETCH_HINT), "err={err}");
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&base);
    }
}
