//! Incremental build helpers: XML sha256, works sidecar, parsed.jsonl.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use cbeta_index::IndexableLine;
use cbeta_parse::ParsedLine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Filename for per-work sha sidecar inside a published artifact.
pub const WORKS_FILE: &str = "works.json";

/// Filename for serialized lines (resume + incremental keep).
pub const PARSED_FILE: &str = "parsed.jsonl";

/// One work entry in `works.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSidecar {
    /// Catalog work id (e.g. `T1578`).
    pub work_id: String,
    /// Relative path under xml-p5 (as in files.txt).
    pub path: String,
    /// SHA-256 hex of the XML file bytes.
    pub xml_sha256: String,
    /// Number of indexed lines for this work.
    pub line_count: u64,
}

/// Serializable line for `parsed.jsonl` (IndexableLine is not Serialize).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredLine {
    /// `{xml:id}_p{lb@n}`.
    pub line_id: String,
    /// Work id.
    pub work_id: String,
    /// Juan number.
    pub juan: u64,
    /// Character text.
    pub text_raw: String,
    /// Raw `lb@n`.
    pub lb_n: String,
    /// Work title.
    pub title: String,
    /// 作译者.
    pub author: String,
    /// Citation string.
    pub citation: String,
    /// Corpus tag.
    pub cbeta_tag: String,
}

impl From<&IndexableLine> for StoredLine {
    fn from(il: &IndexableLine) -> Self {
        Self {
            line_id: il.line.line_id.clone(),
            work_id: il.line.work_id.clone(),
            juan: il.line.juan,
            text_raw: il.line.text_raw.clone(),
            lb_n: il.line.lb_n.clone(),
            title: il.title.clone(),
            author: il.author.clone(),
            citation: il.citation.clone(),
            cbeta_tag: il.cbeta_tag.clone(),
        }
    }
}

impl From<StoredLine> for IndexableLine {
    fn from(s: StoredLine) -> Self {
        Self {
            line: ParsedLine {
                line_id: s.line_id,
                work_id: s.work_id,
                juan: s.juan,
                text_raw: s.text_raw,
                lb_n: s.lb_n,
            },
            title: s.title,
            author: s.author,
            citation: s.citation,
            cbeta_tag: s.cbeta_tag,
        }
    }
}

/// SHA-256 hex of arbitrary bytes.
pub fn sha256_bytes(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// SHA-256 hex of a file on disk.
///
/// # Errors
/// Read failure.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(sha256_bytes(&data))
}

/// Load `works.json` when present.
///
/// # Errors
/// IO / parse failure (missing → `Ok(None)`).
pub fn load_works_sidecar(art: &Path) -> Result<Option<Vec<WorkSidecar>>, String> {
    let path = art.join(WORKS_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let rows: Vec<WorkSidecar> =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(rows))
}

/// Write `works.json` into an artifact directory.
///
/// # Errors
/// IO / serialize failure.
pub fn write_works_sidecar(art: &Path, works: &[WorkSidecar]) -> Result<(), String> {
    let path = art.join(WORKS_FILE);
    let body = serde_json::to_string_pretty(works).map_err(|e| format!("serialize works: {e}"))?;
    fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

/// Map path → sidecar row for quick lookup.
pub fn works_by_path(works: &[WorkSidecar]) -> HashMap<String, WorkSidecar> {
    works.iter().map(|w| (w.path.clone(), w.clone())).collect()
}

/// Load all lines from `parsed.jsonl`.
///
/// # Errors
/// IO / parse failure (missing → empty vec).
pub fn load_parsed_jsonl(art: &Path) -> Result<Vec<IndexableLine>, String> {
    let path = art.join(PARSED_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let f = fs::File::open(&path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = line.map_err(|e| format!("read {}: {e}", path.display()))?;
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let stored: StoredLine =
            serde_json::from_str(t).map_err(|e| format!("parsed.jsonl line {}: {e}", i + 1))?;
        out.push(IndexableLine::from(stored));
    }
    Ok(out)
}

/// Keep only lines whose `work_id` is in `keep`.
pub fn filter_lines_by_works(
    lines: &[IndexableLine],
    keep: &HashMap<String, ()>,
) -> Vec<IndexableLine> {
    lines
        .iter()
        .filter(|l| keep.contains_key(&l.line.work_id))
        .cloned()
        .collect()
}

/// Rewrite `parsed.jsonl` from the full line set (atomic via tmp+rename).
///
/// # Errors
/// IO / serialize failure.
pub fn write_parsed_jsonl(art: &Path, lines: &[IndexableLine]) -> Result<(), String> {
    let path = art.join(PARSED_FILE);
    let tmp = art.join("parsed.jsonl.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        for il in lines {
            let stored = StoredLine::from(il);
            let row = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
            writeln!(f, "{row}").map_err(|e| format!("write parsed: {e}"))?;
        }
        f.sync_all().map_err(|e| format!("sync parsed: {e}"))?;
    }
    fs::rename(&tmp, &path).map_err(|e| format!("rename parsed.jsonl: {e}"))?;
    Ok(())
}

/// Append lines for one work to `parsed.jsonl` (resume path).
///
/// # Errors
/// IO / serialize failure.
pub fn append_parsed_jsonl(art: &Path, lines: &[IndexableLine]) -> Result<(), String> {
    let path = art.join(PARSED_FILE);
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    for il in lines {
        let stored = StoredLine::from(il);
        let row = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
        writeln!(f, "{row}").map_err(|e| format!("append parsed: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_bytes_stable() {
        let a = sha256_bytes(b"hello");
        let b = sha256_bytes(b"hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert_ne!(a, sha256_bytes(b"world"));
    }

    #[test]
    fn stored_line_roundtrip() {
        let il = IndexableLine {
            line: ParsedLine {
                line_id: "T30n1578_p0268b21".into(),
                work_id: "T1578".into(),
                juan: 1,
                text_raw: "真性有為空".into(),
                lb_n: "0268b21".into(),
            },
            title: "t".into(),
            author: "a".into(),
            citation: "c".into(),
            cbeta_tag: "2026R2".into(),
        };
        let back = IndexableLine::from(StoredLine::from(&il));
        assert_eq!(back.line.line_id, il.line.line_id);
        assert_eq!(back.title, il.title);
    }
}
