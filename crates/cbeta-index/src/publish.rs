//! Atomic publish of index artifacts and the `CURRENT` pointer (GitHub #37).
//!
//! Live artifact directories are never deleted. A rebuild stages under `{intended}.tmp`,
//! writes sidecars into that tmp tree, renames onto a free dest (intended name or a
//! unique sibling), then swings `CURRENT` via tempfile + rename.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::paths::{require_path_segment, tmp_dir_for};

/// Prepare `{intended_final}.tmp` for staging (wipe stale tmp only; never touch live final).
pub fn stage_tmp_dir(intended_final: &Path) -> Result<PathBuf> {
    let tmp = tmp_dir_for(intended_final);
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::create_dir_all(&tmp)?;
    if let Some(parent) = intended_final.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(tmp)
}

/// Destination path for publish: `intended_final` if free, else `{name}.{unix_nanos}` sibling.
///
/// The sibling basename still passes [`require_path_segment`]. Previous dirs are left alone.
pub fn resolve_publish_dest(intended_final: &Path) -> Result<PathBuf> {
    if !intended_final.exists() {
        return Ok(intended_final.to_path_buf());
    }
    let parent = intended_final.parent().ok_or_else(|| {
        Error::Path(format!(
            "artifact has no parent: {}",
            intended_final.display()
        ))
    })?;
    let base = intended_final
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Path(format!("bad artifact name: {}", intended_final.display())))?;
    require_path_segment(base)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let sibling = format!("{base}.{nanos}");
    require_path_segment(&sibling)?;
    Ok(parent.join(sibling))
}

/// Atomically rename staged `tmp` directory to `dest` (refuses if `dest` already exists).
pub fn publish_dir(tmp: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err(Error::Path(format!(
            "publish dest already exists: {}",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(tmp, dest)?;
    Ok(())
}

/// Publish `CURRENT` as a plain file via `{root}/CURRENT.tmp` + `sync_all` + rename.
///
/// Never in-place `fs::write` on the live pointer; never a symlink.
pub fn write_current_pointer(root: &Path, dest_basename: &str) -> Result<()> {
    let name = require_path_segment(dest_basename)?;
    fs::create_dir_all(root)?;
    let final_path = root.join("CURRENT");
    let tmp_path = root.join("CURRENT.tmp");
    // Clean a leftover tmp from a crashed prior publish so create is not EEXIST.
    if tmp_path.exists() {
        fs::remove_file(&tmp_path)?;
    }
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(format!("{name}\n").as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempfile_dir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "cbeta-publish-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&p).expect("tmpdir");
        p
    }

    #[test]
    fn resolve_dest_uses_intended_when_absent() {
        let root = tempfile_dir();
        let intended = root.join("2026R2-c1f1x7a0");
        let dest = resolve_publish_dest(&intended).expect("dest");
        assert_eq!(dest, intended);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_dest_unique_sibling_when_live_exists() {
        let root = tempfile_dir();
        let intended = root.join("2026R2-c1f1x7a0");
        fs::create_dir_all(&intended).expect("live");
        let dest = resolve_publish_dest(&intended).expect("dest");
        assert_ne!(dest, intended);
        assert!(dest
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.starts_with("2026R2-c1f1x7a0.")));
        assert!(!dest.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn publish_dir_renames_tmp_and_leaves_other() {
        let root = tempfile_dir();
        let intended = root.join("2026R2-abc");
        let other = root.join("2026R2-keep");
        fs::create_dir_all(&other).expect("other");
        let tmp = stage_tmp_dir(&intended).expect("tmp");
        fs::write(tmp.join("marker"), b"ok").expect("marker");
        let dest = resolve_publish_dest(&intended).expect("dest");
        publish_dir(&tmp, &dest).expect("publish");
        assert!(dest.join("marker").is_file());
        assert!(!tmp.exists());
        assert!(other.is_dir());
        let _ = fs::remove_dir_all(&root);
    }
}
