//! Open the active on-disk Tantivy artifact.

use std::fs;
use std::path::{Path, PathBuf};

use cbeta_index::{build_line_schema, register_tokenizers, LineSchema};
use tantivy::directory::MmapDirectory;
use tantivy::Index;

use crate::error::{Error, Result};

/// Resolve `{root}/CURRENT` (or the sole artifact dir) to an absolute path.
pub fn active_artifact(root: &Path) -> Result<PathBuf> {
    let current = root.join("CURRENT");
    if current.is_file() {
        let name = fs::read_to_string(&current)?;
        let name = name.trim();
        if !name.is_empty() {
            let p = root.join(name);
            if p.is_dir() {
                return Ok(p);
            }
        }
    }
    // Fallback: exactly one `{tag}-{hash}` child directory.
    if root.is_dir() {
        let mut dirs = Vec::new();
        for ent in fs::read_dir(root)? {
            let ent = ent?;
            if ent.file_type()?.is_dir() {
                let name = ent.file_name().to_string_lossy().into_owned();
                if name.contains('-') && !name.ends_with(".tmp") {
                    dirs.push(ent.path());
                }
            }
        }
        if dirs.len() == 1 {
            return Ok(dirs.remove(0));
        }
    }
    Err(Error::NoIndex(root.display().to_string()))
}

/// Open mmap index + register CJK tokenizer; return field handles.
pub fn open_search_index(root: &Path) -> Result<(Index, LineSchema, PathBuf)> {
    let art = active_artifact(root)?;
    let dir = MmapDirectory::open(&art)?;
    let index = Index::open(dir)?;
    register_tokenizers(&index);
    let fields = fields_from_index(&index)?;
    Ok((index, fields, art))
}

fn fields_from_index(index: &Index) -> Result<LineSchema> {
    let schema = index.schema();
    let get = |name: &str| {
        schema
            .get_field(name)
            .map_err(|_| Error::Schema(format!("missing field `{name}`")))
    };
    // Keep a full schema object for callers that need it; field ids match the open index.
    let _built = build_line_schema();
    Ok(LineSchema {
        schema: schema.clone(),
        line_id: get("line_id")?,
        work_id: get("work_id")?,
        title: get("title")?,
        author: get("author")?,
        cbeta_tag: get("cbeta_tag")?,
        juan: get("juan")?,
        text_raw: get("text_raw")?,
        text_norm: get("text_norm")?,
        citation: get("citation")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_index::{write_artifact, IndexableLine};
    use cbeta_parse::{GaijiMap, ParsedLine};
    use std::fs;

    fn sample() -> Vec<IndexableLine> {
        vec![IndexableLine {
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
        }]
    }

    fn temp_root() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-open-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn active_artifact_current_and_single_dir_fallback() {
        let root = temp_root();
        assert!(matches!(active_artifact(&root), Err(Error::NoIndex(_))));

        let art = write_artifact(&root, "2026R2", "h1", &sample(), &GaijiMap::default()).unwrap();
        let got = active_artifact(&root).unwrap();
        assert_eq!(got, art);

        fs::write(root.join("CURRENT"), "\n").unwrap();
        let got2 = active_artifact(&root).unwrap();
        assert_eq!(got2, art);

        fs::write(root.join("CURRENT"), "2026R2-h1\n").unwrap();
        assert_eq!(active_artifact(&root).unwrap(), art);

        fs::write(root.join("CURRENT"), "missing-name\n").unwrap();
        assert_eq!(active_artifact(&root).unwrap(), art);

        let _ = write_artifact(&root, "2026R2", "h2", &sample(), &GaijiMap::default());
        fs::write(root.join("CURRENT"), "nope\n").unwrap();
        assert!(matches!(active_artifact(&root), Err(Error::NoIndex(_))));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn open_search_index_ok() {
        let root = temp_root();
        let _ = write_artifact(&root, "2026R2", "hx", &sample(), &GaijiMap::default()).unwrap();
        fs::write(root.join("CURRENT"), "2026R2-hx\n").unwrap();
        let (idx, fields, art) = open_search_index(&root).unwrap();
        assert!(art.ends_with("2026R2-hx"));
        let _ = idx;
        let _ = fields;
        let _ = fs::remove_dir_all(&root);
    }
}
