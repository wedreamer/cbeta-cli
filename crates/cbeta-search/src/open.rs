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
