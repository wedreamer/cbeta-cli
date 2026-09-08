//! Scope manifest / catalog / files.txt readers for build.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ScopeManifest {
    pub cbeta_tag: String,
    pub scope: String,
    pub scope_hash: String,
    #[serde(default)]
    pub work_count: u64,
    #[serde(default)]
    pub exclude_canons: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct CatalogRow {
    pub work_id: String,
    pub canon: String,
    pub path: String,
    pub title: String,
    pub author: String,
    #[serde(default)]
    pub dynasty: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub work_type: String,
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let s = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("parse {}: {e}", path.display()))
}

pub fn read_catalog(path: &Path) -> Result<Vec<CatalogRow>, String> {
    let f = fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = line.map_err(|e| format!("read {}: {e}", path.display()))?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let row: CatalogRow =
            serde_json::from_str(t).map_err(|e| format!("catalog.jsonl line {}: {e}", i + 1))?;
        out.push(row);
    }
    Ok(out)
}

pub fn read_files_list(path: &Path) -> Result<Vec<String>, String> {
    let f = fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut out = Vec::new();
    for line in BufReader::new(f).lines() {
        let line = line.map_err(|e| format!("read {}: {e}", path.display()))?;
        let t = line.trim();
        if !t.is_empty() && !t.starts_with('#') {
            out.push(t.to_string());
        }
    }
    Ok(out)
}
