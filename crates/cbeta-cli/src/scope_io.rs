//! Scope manifest / catalog / files.txt readers for build.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde::Deserialize;

fn empty_if_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|v| v.unwrap_or_default())
}

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
    #[serde(default, deserialize_with = "empty_if_null")]
    pub title: String,
    #[serde(default, deserialize_with = "empty_if_null")]
    pub author: String,
    #[serde(default, deserialize_with = "empty_if_null")]
    pub dynasty: String,
    #[serde(default, deserialize_with = "empty_if_null")]
    pub category: String,
    #[serde(default, deserialize_with = "empty_if_null")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_file(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-scope-io-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p
    }

    #[test]
    fn read_json_ok_and_parse_error() {
        let p = tmp_file(
            "ok",
            r#"{"cbeta_tag":"2026R2","scope":"s","scope_hash":"h"}"#,
        );
        let m: ScopeManifest = read_json(&p).unwrap();
        assert_eq!(m.scope_hash, "h");
        let bad = tmp_file("bad", "not-json");
        assert!(read_json::<ScopeManifest>(&bad).is_err());
        let missing = std::env::temp_dir().join("cbeta-scope-io-missing-nope");
        assert!(read_json::<ScopeManifest>(&missing).is_err());
        let _ = fs::remove_file(&p);
        let _ = fs::remove_file(&bad);
    }

    #[test]
    fn read_catalog_skips_blank_lines_and_parses_rows() {
        let body = r#"
{"work_id":"T0235","canon":"T","path":"a.xml","title":"t","author":"a"}

{"work_id":"T1578","canon":"T","path":"b.xml","title":"u","author":"b"}
"#;
        let p = tmp_file("cat", body);
        let rows = read_catalog(&p).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].work_id, "T0235");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_catalog_rejects_bad_json_line() {
        let p = tmp_file("cat-bad", "not-json-line\n");
        let err = read_catalog(&p).unwrap_err();
        assert!(err.contains("line 1"));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_catalog_accepts_null_title_author() {
        let body = concat!(
            r#"{"work_id":"T1578","canon":"T","path":"T/T30/T30n1578.xml","#,
            r#""title":null,"author":null,"dynasty":null,"category":null,"work_type":null}"#,
            "\n",
        );
        let p = tmp_file("cat-null", body);
        let rows = read_catalog(&p).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].work_id, "T1578");
        assert_eq!(rows[0].title, "");
        assert_eq!(rows[0].author, "");
        assert_eq!(rows[0].dynasty, "");
        assert_eq!(rows[0].category, "");
        assert_eq!(rows[0].work_type, "");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn read_files_list_skips_comments_and_blanks() {
        let p = tmp_file("files", "# comment\n\nT/a.xml\n  \n#x\nT/b.xml\n");
        let files = read_files_list(&p).unwrap();
        assert_eq!(files, vec!["T/a.xml".to_string(), "T/b.xml".to_string()]);
        let _ = fs::remove_file(&p);
        assert!(read_files_list(Path::new("/no/such/files.txt")).is_err());
    }
}
