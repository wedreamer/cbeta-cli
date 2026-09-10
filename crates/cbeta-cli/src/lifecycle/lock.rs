//! Embedded `releases.lock.yaml` pins (official SHAs; never `master`).

use std::collections::BTreeMap;

use serde::Deserialize;

const LOCK_YAML: &str = include_str!("../../data/releases.lock.yaml");

/// Official upstream clone URLs for the three corpus repos.
#[derive(Debug, Clone, Deserialize)]
pub struct OfficialRepos {
    /// xml-p5 TEI tree.
    #[serde(rename = "xml-p5")]
    pub xml_p5: String,
    /// DILA metadata.
    pub metadata: String,
    /// Gaiji / 缺字 table.
    pub gaiji: String,
}

/// Per-release commit pins for xml-p5 / metadata / gaiji.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleasePins {
    /// xml-p5 commit SHA.
    #[serde(rename = "xml-p5")]
    pub xml_p5: String,
    /// metadata commit SHA.
    pub metadata: String,
    /// gaiji commit SHA.
    pub gaiji: String,
}

/// Top-level lock file shape (`schema` + `official` + `releases`).
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseLock {
    /// Lock schema id, e.g. `cbeta-cli.releases/v1`.
    pub schema: String,
    /// Upstream repo URLs.
    pub official: OfficialRepos,
    /// Tag → commit pins. Keys must never be `master` or contain `xml-p5-2018`.
    pub releases: BTreeMap<String, ReleasePins>,
}

/// Parse the embedded lock and reject forbidden release keys.
pub fn load_lock() -> Result<ReleaseLock, String> {
    let lock: ReleaseLock =
        serde_yaml::from_str(LOCK_YAML).map_err(|e| format!("releases.lock.yaml: {e}"))?;
    validate_release_keys(&lock)?;
    Ok(lock)
}

fn validate_release_keys(lock: &ReleaseLock) -> Result<(), String> {
    if lock.schema.is_empty() {
        return Err("releases.lock.yaml missing schema".into());
    }
    if lock.official.xml_p5.is_empty()
        || lock.official.metadata.is_empty()
        || lock.official.gaiji.is_empty()
    {
        return Err("releases.lock.yaml official URLs incomplete".into());
    }
    for (key, pins) in &lock.releases {
        if key == "master" {
            return Err("releases.lock.yaml must not pin key \"master\"".into());
        }
        if key.contains("xml-p5-2018") {
            return Err(format!(
                "releases.lock.yaml must not pin legacy key \"{key}\""
            ));
        }
        if pins.xml_p5.is_empty() || pins.metadata.is_empty() || pins.gaiji.is_empty() {
            return Err(format!("releases.lock.yaml pin \"{key}\" incomplete"));
        }
    }
    Ok(())
}

/// Sorted release tags from the lock (stable for `cbeta releases`).
pub fn lock_tags(lock: &ReleaseLock) -> Vec<String> {
    lock.releases.keys().cloned().collect()
}

/// Compare CBETA release tags (`2026R2` > `2026R1` > `2025R3`).
pub fn cmp_release_tag(a: &str, b: &str) -> std::cmp::Ordering {
    parse_tag_parts(a).cmp(&parse_tag_parts(b))
}

fn parse_tag_parts(tag: &str) -> (u32, u32) {
    match tag.split_once('R') {
        Some((year, rest)) => (
            year.parse::<u32>().unwrap_or(0),
            rest.parse::<u32>().unwrap_or(0),
        ),
        None => (0, 0),
    }
}

/// Newest concrete tag in the lock (never the string `latest`).
pub fn newest_lock_tag(lock: &ReleaseLock) -> Option<&str> {
    lock.releases
        .keys()
        .max_by(|a, b| cmp_release_tag(a, b))
        .map(String::as_str)
}

/// Resolve `--release TAG|latest` to a concrete lock tag.
///
/// When `release == "latest"` and `remote_tags` is `Some`, prefer the newest
/// remote tag that also exists in the lock; otherwise use [`newest_lock_tag`].
///
/// # Errors
/// Unknown release (not in lock and not `latest`).
pub fn resolve_release(
    release: &str,
    lock: &ReleaseLock,
    remote_tags: Option<&[String]>,
) -> Result<String, String> {
    if release == "latest" {
        if let Some(remote) = remote_tags {
            let mut candidates: Vec<&str> = remote
                .iter()
                .map(String::as_str)
                .filter(|t| lock.releases.contains_key(*t))
                .collect();
            candidates.sort_by(|a, b| cmp_release_tag(a, b));
            if let Some(best) = candidates.last() {
                return Ok((*best).to_string());
            }
        }
        return newest_lock_tag(lock)
            .map(str::to_string)
            .ok_or_else(|| "releases.lock.yaml has no releases".to_string());
    }
    if lock.releases.contains_key(release) {
        return Ok(release.to_string());
    }
    Err(format!("unknown release {release}; see: cbeta releases"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_pins_2026r2_not_master() {
        let lock = load_lock().expect("embedded lock parses");
        assert_eq!(lock.schema, "cbeta-cli.releases/v1");
        assert!(!lock.releases.contains_key("master"));
        for key in lock.releases.keys() {
            assert!(!key.contains("xml-p5-2018"), "forbidden key {key}");
        }
        let r2 = lock.releases.get("2026R2").expect("2026R2 pin");
        assert_eq!(r2.xml_p5, "dbdea41071e1e260ad84b72faefd4587333cf76d");
        assert_eq!(r2.metadata, "af914b8d3ce6b7c929cbef5cef061e00bddda929");
        assert_eq!(r2.gaiji, "30fb27a9fb7b8619d0f1c7a78c90c0a60108a067");
        assert!(lock.releases.contains_key("2026R1"));
        assert!(lock.releases.contains_key("2025R3"));
        assert!(lock.official.xml_p5.contains("xml-p5"));
        assert!(lock.official.metadata.contains("cbeta-metadata"));
        assert!(lock.official.gaiji.contains("gaiji"));
    }

    #[test]
    fn validate_rejects_master_key() {
        let mut lock = load_lock().unwrap();
        lock.releases.insert(
            "master".into(),
            ReleasePins {
                xml_p5: "x".into(),
                metadata: "y".into(),
                gaiji: "z".into(),
            },
        );
        let err = validate_release_keys(&lock).unwrap_err();
        assert!(err.contains("master"));
    }
}
