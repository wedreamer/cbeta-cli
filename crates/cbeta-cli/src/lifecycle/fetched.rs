//! `FETCHED.yaml` under `$HOME/.cbeta/corpus/<tag>/` — never writes `CURRENT`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::lock::ReleasePins;
use super::paths::tag_dir;

/// Schema id written into every FETCHED.yaml.
pub const FETCHED_SCHEMA: &str = "cbeta-cli.fetched/v1";

/// Filename under a tag directory.
pub const FETCHED_FILE: &str = "FETCHED.yaml";

/// Per-source commit recorded after a successful fetch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchedSource {
    /// Release tag for xml-p5 only (optional on metadata/gaiji).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// 40-hex commit SHA at checkout.
    pub commit: String,
}

/// On-disk FETCHED.yaml shape (no floating `latest`, no `ref: master`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchedYaml {
    /// Schema id.
    pub schema: String,
    /// Concrete release tag (never the string `latest`).
    pub cbeta_release: String,
    /// Fetch timestamp (RFC3339-ish UTC).
    pub fetched_at: String,
    /// Absolute cache root for this tag.
    pub cache_root: String,
    /// xml-p5 / metadata / gaiji commits.
    pub sources: FetchedSources,
}

/// Nested `sources` map in FETCHED.yaml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchedSources {
    /// xml-p5 pin + tag.
    #[serde(rename = "xml-p5")]
    pub xml_p5: FetchedSource,
    /// metadata pin.
    pub metadata: FetchedSource,
    /// gaiji pin.
    pub gaiji: FetchedSource,
}

/// Path to `FETCHED.yaml` for `tag`.
pub fn fetched_path(tag: &str) -> Result<PathBuf, String> {
    Ok(tag_dir(tag)?.join(FETCHED_FILE))
}

/// Read and parse FETCHED.yaml for `tag` when present.
pub fn read_fetched(tag: &str) -> Result<Option<FetchedYaml>, String> {
    let path = fetched_path(tag)?;
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let doc: FetchedYaml =
        serde_yaml::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(doc))
}

/// True when FETCHED exists, release matches, and all three commits equal `pins`.
pub fn is_complete(tag: &str, pins: &ReleasePins) -> bool {
    match read_fetched(tag) {
        Ok(Some(doc)) => {
            doc.cbeta_release == tag
                && doc.sources.xml_p5.commit == pins.xml_p5
                && doc.sources.metadata.commit == pins.metadata
                && doc.sources.gaiji.commit == pins.gaiji
        }
        _ => false,
    }
}

/// Write FETCHED.yaml under `cache_root` (the tag directory). Never touches CURRENT.
///
/// # Errors
/// IO / serialize failures.
pub fn write_fetched(tag: &str, cache_root: &Path, pins: &ReleasePins) -> Result<PathBuf, String> {
    fs::create_dir_all(cache_root).map_err(|e| format!("mkdir {}: {e}", cache_root.display()))?;
    let doc = FetchedYaml {
        schema: FETCHED_SCHEMA.to_string(),
        cbeta_release: tag.to_string(),
        fetched_at: utc_now_rfc3339(),
        cache_root: cache_root.display().to_string(),
        sources: FetchedSources {
            xml_p5: FetchedSource {
                tag: Some(tag.to_string()),
                commit: pins.xml_p5.clone(),
            },
            metadata: FetchedSource {
                tag: None,
                commit: pins.metadata.clone(),
            },
            gaiji: FetchedSource {
                tag: None,
                commit: pins.gaiji.clone(),
            },
        },
    };
    let path = cache_root.join(FETCHED_FILE);
    let yaml = serde_yaml::to_string(&doc).map_err(|e| format!("serialize FETCHED: {e}"))?;
    fs::write(&path, yaml).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// Approximate UTC RFC3339 without pulling in `chrono`.
fn utc_now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Howard Hinnant civil-from-days (UTC).
fn civil_from_unix(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let h = (tod / 3600) as u32;
    let mi = ((tod % 3600) / 60) as u32;
    let s = (tod % 60) as u32;
    // days since 1970-01-01 → civil
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d, h, mi, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_paths::env_lock;
    use crate::lifecycle::lock::load_lock;
    use crate::lifecycle::paths::corpus_cache_root;

    fn temp_home(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join(".cbeta").join("corpus")).unwrap();
        p
    }

    #[test]
    fn fetch_does_not_write_current() {
        let _g = env_lock();
        let home = temp_home("cbeta-fetched-no-cur");
        std::env::set_var("HOME", &home);
        std::env::remove_var("CBETA_CORPUS");

        let lock = load_lock().unwrap();
        let pins = lock.releases.get("2026R2").unwrap();
        let root = tag_dir("2026R2").unwrap();
        write_fetched("2026R2", &root, pins).unwrap();

        let fetched = root.join(FETCHED_FILE);
        assert!(fetched.is_file(), "FETCHED.yaml missing");
        let current = corpus_cache_root().unwrap().join("CURRENT");
        assert!(
            !current.exists(),
            "CURRENT must not be created by write_fetched"
        );

        let doc = read_fetched("2026R2").unwrap().unwrap();
        assert_eq!(doc.cbeta_release, "2026R2");
        assert_ne!(doc.cbeta_release, "latest");
        assert_eq!(doc.sources.xml_p5.commit, pins.xml_p5);
        assert!(is_complete("2026R2", pins));

        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn fetch_latest_stores_concrete_tag() {
        let _g = env_lock();
        let home = temp_home("cbeta-fetched-latest");
        std::env::set_var("HOME", &home);

        let lock = load_lock().unwrap();
        let concrete = super::super::lock::newest_lock_tag(&lock).unwrap();
        assert_eq!(concrete, "2026R2");
        let pins = lock.releases.get(concrete).unwrap();
        let root = tag_dir(concrete).unwrap();
        write_fetched(concrete, &root, pins).unwrap();
        let doc = read_fetched(concrete).unwrap().unwrap();
        assert_eq!(doc.cbeta_release, "2026R2");
        assert_ne!(doc.cbeta_release, "latest");

        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn is_complete_false_on_commit_mismatch() {
        let _g = env_lock();
        let home = temp_home("cbeta-fetched-mm");
        std::env::set_var("HOME", &home);
        let lock = load_lock().unwrap();
        let mut pins = lock.releases.get("2026R2").unwrap().clone();
        let root = tag_dir("2026R2").unwrap();
        write_fetched("2026R2", &root, &pins).unwrap();
        pins.xml_p5 = "0000000000000000000000000000000000000000".into();
        assert!(!is_complete("2026R2", &pins));
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn read_fetched_none_when_missing() {
        let _g = env_lock();
        let home = temp_home("cbeta-fetched-miss");
        std::env::set_var("HOME", &home);
        assert!(read_fetched("2026R2").unwrap().is_none());
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn civil_from_unix_epoch_day() {
        let (y, mo, d, h, mi, s) = civil_from_unix(0);
        assert_eq!((y, mo, d, h, mi, s), (1970, 1, 1, 0, 0, 0));
        let (y2, mo2, d2, _, _, _) = civil_from_unix(86_400);
        assert_eq!((y2, mo2, d2), (1970, 1, 2));
    }
}
