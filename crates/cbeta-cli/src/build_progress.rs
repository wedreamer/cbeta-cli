//! `PROGRESS.json` inside `{artifact}.tmp` for resumable builds.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Schema id written into every PROGRESS.json.
pub const PROGRESS_SCHEMA: &str = "cbeta-cli.progress/v1";

/// Filename under the staging tmp directory.
pub const PROGRESS_FILE: &str = "PROGRESS.json";

/// On-disk progress checkpoint (last completed work).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Schema id; bump → treat as full rebuild.
    pub schema: String,
    /// Last successfully parsed `work_id`.
    pub work_id: String,
    /// SHA-256 hex of that work's XML bytes.
    pub xml_sha256: String,
    /// Corpus release tag for this build.
    pub tag: String,
    /// Scope hash from MANIFEST.
    pub scope_hash: String,
}

/// Whether an existing tmp dir can be reused for resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmpDecision {
    /// Reuse tmp; resume after `progress.work_id`.
    Reuse {
        /// Valid checkpoint matching current tag/scope/schema.
        progress: Progress,
    },
    /// Wipe via `stage_tmp_dir` and start fresh.
    Wipe,
}

/// Path of PROGRESS.json under a staging tmp directory.
pub fn progress_path(tmp: &Path) -> PathBuf {
    tmp.join(PROGRESS_FILE)
}

/// Write a progress checkpoint into `tmp`. Never touches CURRENT.
///
/// # Errors
/// IO or serialize failure.
pub fn write_progress(tmp: &Path, progress: &Progress) -> Result<(), String> {
    fs::create_dir_all(tmp).map_err(|e| format!("mkdir {}: {e}", tmp.display()))?;
    let path = progress_path(tmp);
    let body =
        serde_json::to_string_pretty(progress).map_err(|e| format!("serialize PROGRESS: {e}"))?;
    fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

/// Read PROGRESS.json when present.
///
/// # Errors
/// IO or parse failure (missing file → `Ok(None)`).
pub fn read_progress(tmp: &Path) -> Result<Option<Progress>, String> {
    let path = progress_path(tmp);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let doc: Progress =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(doc))
}

/// Decide whether to reuse `tmp` for resume or wipe it.
///
/// Resume only when `!full` and schema/tag/scope_hash all match.
pub fn decide_tmp(tmp: &Path, tag: &str, scope_hash: &str, full: bool) -> TmpDecision {
    if full {
        return TmpDecision::Wipe;
    }
    match read_progress(tmp) {
        Ok(Some(p))
            if p.schema == PROGRESS_SCHEMA && p.tag == tag && p.scope_hash == scope_hash =>
        {
            TmpDecision::Reuse { progress: p }
        }
        _ => TmpDecision::Wipe,
    }
}

/// True when any `{name}.tmp/PROGRESS.json` exists under `index_root`.
pub fn has_incomplete_build(index_root: &Path) -> bool {
    let Ok(entries) = fs::read_dir(index_root) else {
        return false;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let name = ent.file_name();
        let Some(s) = name.to_str() else {
            continue;
        };
        if s.ends_with(".tmp") && progress_path(&path).is_file() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-progress-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample() -> Progress {
        Progress {
            schema: PROGRESS_SCHEMA.into(),
            work_id: "T0235".into(),
            xml_sha256: "abc".into(),
            tag: "2026R2".into(),
            scope_hash: "c1f1x7a0".into(),
        }
    }

    #[test]
    fn progress_json_written_inside_tmp_not_as_current() {
        let root = temp_dir("write");
        let tmp = root.join("2026R2-c1f1x7a0.tmp");
        write_progress(&tmp, &sample()).unwrap();
        let path = progress_path(&tmp);
        assert!(path.ends_with("PROGRESS.json"));
        assert!(path.is_file());
        assert!(!root.join("CURRENT").exists());
        assert!(!tmp.join("CURRENT").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resume_keeps_tmp_when_tag_scope_schema_match() {
        let root = temp_dir("reuse");
        let tmp = root.join("art.tmp");
        write_progress(&tmp, &sample()).unwrap();
        match decide_tmp(&tmp, "2026R2", "c1f1x7a0", false) {
            TmpDecision::Reuse { progress } => {
                assert_eq!(progress.work_id, "T0235");
            }
            TmpDecision::Wipe => panic!("expected reuse"),
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resume_drops_tmp_when_schema_or_tag_changes() {
        let root = temp_dir("drop");
        let tmp = root.join("art.tmp");
        write_progress(&tmp, &sample()).unwrap();
        assert_eq!(
            decide_tmp(&tmp, "2025R3", "c1f1x7a0", false),
            TmpDecision::Wipe
        );
        let mut bad = sample();
        bad.schema = "cbeta-cli.progress/v0".into();
        write_progress(&tmp, &bad).unwrap();
        assert_eq!(
            decide_tmp(&tmp, "2026R2", "c1f1x7a0", false),
            TmpDecision::Wipe
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_full_skips_hash_resume() {
        let root = temp_dir("full");
        let tmp = root.join("art.tmp");
        write_progress(&tmp, &sample()).unwrap();
        assert_eq!(
            decide_tmp(&tmp, "2026R2", "c1f1x7a0", true),
            TmpDecision::Wipe
        );
        let _ = fs::remove_dir_all(&root);
    }
}
