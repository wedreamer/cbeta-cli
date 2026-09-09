//! Index root and artifact path helpers.

use std::env;
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Reject empty / `.` / `..` / separators so `{root}/{seg}` cannot escape `root`.
pub fn require_path_segment(s: &str) -> Result<&str> {
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.contains('/')
        || s.contains('\\')
        || s.contains('\0')
    {
        return Err(Error::Path(format!("unsafe path segment: {s:?}")));
    }
    Ok(s)
}

/// `{tag}-{scope_hash}` after both halves pass [`require_path_segment`].
pub fn artifact_dir_name(tag: &str, scope_hash: &str) -> Result<String> {
    Ok(format!(
        "{}-{}",
        require_path_segment(tag)?,
        require_path_segment(scope_hash)?
    ))
}

/// Default relative index root under `$HOME`.
const DEFAULT_REL: &str = ".cbeta/index";

/// Resolve the index root: `CBETA_INDEX` or `$HOME/.cbeta/index`.
pub fn index_root() -> Result<PathBuf> {
    if let Ok(p) = env::var("CBETA_INDEX") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = env::var_os("HOME")
        .ok_or_else(|| Error::Path("HOME unset and CBETA_INDEX not set".to_string()))?;
    Ok(PathBuf::from(home).join(DEFAULT_REL))
}

/// Artifact directory: `{root}/{cbeta_tag}-{scope_hash}/`.
pub fn index_dir(tag: &str, scope_hash: &str) -> Result<PathBuf> {
    Ok(index_root()?.join(artifact_dir_name(tag, scope_hash)?))
}

/// Artifact id string `{cbeta_tag}+{scope_hash}`.
pub fn artifact_id(tag: &str, scope_hash: &str) -> String {
    format!("{tag}+{scope_hash}")
}

/// Sibling temp dir used for atomic builds: `{dir}.tmp`.
pub fn tmp_dir_for(final_dir: &std::path::Path) -> PathBuf {
    let mut os = final_dir.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_id_joins_with_plus() {
        assert_eq!(artifact_id("2026R2", "a3f91c2e"), "2026R2+a3f91c2e");
    }

    #[test]
    fn tmp_dir_appends_dot_tmp() {
        let p = PathBuf::from("/tmp/idx/2026R2-abc");
        assert_eq!(tmp_dir_for(&p), PathBuf::from("/tmp/idx/2026R2-abc.tmp"));
    }

    #[test]
    fn index_root_env_and_home_fallback() {
        std::env::set_var("CBETA_INDEX", "/tmp/cbeta-idx-ut");
        assert_eq!(index_root().unwrap(), PathBuf::from("/tmp/cbeta-idx-ut"));
        std::env::set_var("CBETA_INDEX", "");
        std::env::set_var("HOME", "/tmp/home-idx-ut");
        assert_eq!(
            index_root().unwrap(),
            PathBuf::from("/tmp/home-idx-ut").join(DEFAULT_REL)
        );
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        assert!(index_root().is_err());
        std::env::set_var("HOME", "/tmp/home-idx-ut2");
        let d = index_dir("2026R2", "abc").unwrap();
        assert!(d.ends_with("2026R2-abc"));
        std::env::remove_var("HOME");
    }

    #[test]
    fn rejects_path_escape_segments() {
        assert!(require_path_segment("").is_err());
        assert!(require_path_segment(".").is_err());
        assert!(require_path_segment("..").is_err());
        assert!(require_path_segment("../x").is_err());
        assert!(require_path_segment("a/b").is_err());
        assert!(require_path_segment("a\\b").is_err());
        assert!(artifact_dir_name("2026R2", "../etc").is_err());
        assert_eq!(artifact_dir_name("2026R2", "abc").unwrap(), "2026R2-abc");
    }

    /// #37 RED: missing `write_current_pointer` (tempfile+rename CURRENT, not in-place write).
    #[test]
    fn write_current_pointer_api_publishes_via_rename() {
        use std::fs;

        let mut root = std::env::temp_dir();
        root.push(format!(
            "cbeta-index-current-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&root).expect("tmpdir");

        crate::write_current_pointer(&root, "2026R2-c1f1x7a0")
            .expect("write_current_pointer should publish CURRENT atomically");

        let body = fs::read_to_string(root.join("CURRENT")).expect("CURRENT");
        assert_eq!(body.trim(), "2026R2-c1f1x7a0");

        let leftovers: Vec<_> = fs::read_dir(&root)
            .expect("read root")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "CURRENT" && n.starts_with("CURRENT"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp CURRENT* leftovers after rename: {leftovers:?}"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
