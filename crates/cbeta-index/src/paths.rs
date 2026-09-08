//! Index root and artifact path helpers.

use std::env;
use std::path::PathBuf;

use crate::error::{Error, Result};

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
    Ok(index_root()?.join(format!("{tag}-{scope_hash}")))
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
}
