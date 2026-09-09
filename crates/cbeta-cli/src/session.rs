//! Persist and reload the last search session (`last.json`).
//!
//! WHY: scholars re-open a hit by rank without re-running search; the file is a
//! snapshot of RAW hits + the index `artifact_id` so a rebuild cannot silently
//! serve stale line_ids.

use std::fs;
use std::path::PathBuf;

use cbeta_core::{Filters, Hit};
use serde::{Deserialize, Serialize};

/// On-disk shape of `last.json` written by `search --save last`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSearch {
    /// Original search query string (raw `q`, not display-scripted).
    pub query: String,
    /// Filters active when the search ran.
    pub filters: Filters,
    /// RAW hits as returned by the engine (never `--script` display clones).
    pub hits: Vec<Hit>,
    /// Index artifact id at save time (`{cbeta_tag}+{scope_hash}`).
    pub artifact_id: String,
}

/// Path of `last.json`: under `CBETA_INDEX` when set, else `$HOME/.cbeta/last.json`.
///
/// WHY: default index lives at `$HOME/.cbeta/index`; the session file sits next to
/// the `.cbeta` root so it is not mistaken for an index artifact directory.
///
/// # Errors
/// Returns an error when neither a non-empty `CBETA_INDEX` nor `HOME` is available.
pub fn last_json_path() -> Result<PathBuf, String> {
    match std::env::var("CBETA_INDEX") {
        Ok(p) if !p.is_empty() => Ok(crate::env_paths::index_root()?.join("last.json")),
        _ => {
            let home = std::env::var_os("HOME").ok_or_else(|| {
                "HOME unset and CBETA_INDEX not set; cannot resolve last.json".to_string()
            })?;
            Ok(PathBuf::from(home).join(".cbeta").join("last.json"))
        }
    }
}

/// Atomically write `session` to [`last_json_path`] (`{path}.tmp` then rename).
///
/// # Errors
/// Returns an I/O or serialization error message when the write fails.
pub fn save_last(session: &SavedSearch) -> Result<(), String> {
    let path = last_json_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create last.json parent: {e}"))?;
    }
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp = PathBuf::from(tmp_os);
    let body =
        serde_json::to_string_pretty(session).map_err(|e| format!("serialize last.json: {e}"))?;
    fs::write(&tmp, body).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename to {}: {e}", path.display()))?;
    Ok(())
}

/// Load a previously saved session from [`last_json_path`].
///
/// # Errors
/// Missing file or invalid JSON → error string (caller maps to exit 2).
pub fn load_last() -> Result<SavedSearch, String> {
    let path = last_json_path()?;
    if !path.is_file() {
        return Err(format!(
            "no last.json at {}; run: cbeta search --save last <query>",
            path.display()
        ));
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Pick a 1-based hit from `session`.
///
/// # Errors
/// Returns `Err(1)` when `n` is 0 or greater than `hits.len()` (no-hit style).
pub fn pick_hit(session: &SavedSearch, n: u32) -> Result<&Hit, i32> {
    if n == 0 {
        return Err(1);
    }
    let idx = usize::try_from(n).map_err(|_| 1)?.checked_sub(1).ok_or(1)?;
    session.hits.get(idx).ok_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_paths::env_lock;

    fn sample_hit(line_id: &str) -> Hit {
        Hit {
            line_id: line_id.into(),
            work_id: "T1578".into(),
            title: "大乘掌珍論".into(),
            author: "玄奘".into(),
            juan: 1,
            text_raw: "真性有為空".into(),
            citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)".into(),
            score: 1.0,
            cbeta_tag: "2026R2".into(),
        }
    }

    fn sample_session() -> SavedSearch {
        SavedSearch {
            query: "真性有为空".into(),
            filters: Filters::default(),
            hits: vec![sample_hit("T30n1578_p0268b21")],
            artifact_id: "2026R2+abc".into(),
        }
    }

    #[test]
    fn last_json_path_prefers_cbeta_index() {
        let _g = env_lock();
        std::env::set_var("CBETA_INDEX", "/tmp/cbeta-session-idx");
        let p = last_json_path().unwrap();
        assert_eq!(p, PathBuf::from("/tmp/cbeta-session-idx/last.json"));
        std::env::remove_var("CBETA_INDEX");
    }

    #[test]
    fn last_json_path_home_is_beside_cbeta_not_inside_index() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::set_var("HOME", "/tmp/home-session-ut");
        let p = last_json_path().unwrap();
        assert_eq!(p, PathBuf::from("/tmp/home-session-ut/.cbeta/last.json"));
        std::env::remove_var("HOME");
    }

    #[test]
    fn pick_hit_one_based_and_out_of_range() {
        let s = sample_session();
        assert_eq!(pick_hit(&s, 1).unwrap().line_id, "T30n1578_p0268b21");
        assert_eq!(pick_hit(&s, 0), Err(1));
        assert_eq!(pick_hit(&s, 2), Err(1));
        assert_eq!(pick_hit(&s, 999), Err(1));
    }

    #[test]
    fn save_load_roundtrip_atomic() {
        let _g = env_lock();
        let dir = std::env::temp_dir().join(format!(
            "cbeta-session-rt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CBETA_INDEX", &dir);
        let s = sample_session();
        save_last(&s).unwrap();
        let loaded = load_last().unwrap();
        assert_eq!(loaded, s);
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_last_missing_file_errors() {
        let _g = env_lock();
        let dir = std::env::temp_dir().join(format!(
            "cbeta-session-miss-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CBETA_INDEX", &dir);
        let err = load_last().unwrap_err();
        assert!(err.contains("last.json"));
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_json_path_errors_without_home_or_index() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        let err = last_json_path().unwrap_err();
        assert!(err.contains("HOME") || err.contains("CBETA_INDEX"));
    }

    #[test]
    fn load_last_bad_json_errors() {
        let _g = env_lock();
        let dir = std::env::temp_dir().join(format!(
            "cbeta-session-bad-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("last.json"), b"{not-json").unwrap();
        std::env::set_var("CBETA_INDEX", &dir);
        let err = load_last().unwrap_err();
        assert!(err.contains("parse") || err.contains("last.json"));
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&dir);
    }
}
