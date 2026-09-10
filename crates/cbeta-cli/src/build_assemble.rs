//! Parse / resume / incremental assembly of IndexableLine batches.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use cbeta_index::IndexableLine;
use cbeta_parse::parse_tei_lines;

use crate::build_incr::{
    append_parsed_jsonl, filter_lines_by_works, load_parsed_jsonl, load_works_sidecar,
    sha256_bytes, sha256_file, works_by_path, WorkSidecar,
};
use crate::build_progress::{write_progress, Progress, PROGRESS_SCHEMA};
use crate::citation::format_citation;
use crate::scope_io::CatalogRow;

/// Canons never indexed by default (Category B).
pub const SKIP_CANONS: &[&str] = &["Y", "TX", "LC", "YP"];

/// True when `rel` is a safe relative path under xml-p5.
pub fn rel_is_under(rel: &str) -> bool {
    let p = Path::new(rel);
    !p.is_absolute()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Inputs for assembling lines into a staging tmp directory.
pub struct AssembleInput<'a> {
    /// files.txt relative paths.
    pub files: &'a [String],
    /// path → catalog row.
    pub by_path: &'a HashMap<&'a str, &'a CatalogRow>,
    /// Canons to skip.
    pub skip: &'a [String],
    /// xml-p5 root.
    pub xml_root: &'a Path,
    /// Staging tmp directory.
    pub tmp: &'a Path,
    /// Published artifact (for incremental cache).
    pub intended: &'a Path,
    /// Resume after this work_id (exclusive of re-parse).
    pub resume_after: Option<&'a str>,
    /// Force full reparse (ignore published works.json).
    pub full: bool,
    /// Corpus release tag.
    pub cbeta_tag: &'a str,
    /// Scope hash.
    pub scope_hash: &'a str,
    /// Print `[i/n] path` progress.
    pub tty_progress: bool,
}

/// Result of assembling all works.
pub struct AssembleOutput {
    /// All indexable lines.
    pub lines: Vec<IndexableLine>,
    /// Per-work sidecar rows.
    pub works: Vec<WorkSidecar>,
    /// Count of works newly parsed or reused this run.
    pub works_seen: u64,
}

/// Parse / resume / incremental-fill lines into `tmp`, updating PROGRESS.json.
///
/// # Errors
/// Path escape, read/parse failure, progress write failure.
pub fn assemble_lines(input: &AssembleInput<'_>) -> Result<AssembleOutput, String> {
    let mut prev_works: HashMap<String, WorkSidecar> = HashMap::new();
    let mut prev_lines: Vec<IndexableLine> = Vec::new();
    if input.resume_after.is_none() && !input.full {
        if let Ok(Some(works)) = load_works_sidecar(input.intended) {
            prev_works = works_by_path(&works);
            prev_lines = load_parsed_jsonl(input.intended).unwrap_or_default();
        }
    }

    let mut lines: Vec<IndexableLine> = if let Some(after) = input.resume_after {
        let existing = load_parsed_jsonl(input.tmp).unwrap_or_default();
        let mut allowed = HashMap::new();
        for rel in input.files {
            let Some(row) = input.by_path.get(rel.as_str()) else {
                continue;
            };
            allowed.insert(row.work_id.clone(), ());
            if row.work_id == after {
                break;
            }
        }
        filter_lines_by_works(&existing, &allowed)
    } else {
        Vec::new()
    };

    let mut works_out: Vec<WorkSidecar> = Vec::new();
    if let Some(after) = input.resume_after {
        let mut seen = HashMap::new();
        for l in &lines {
            *seen.entry(l.line.work_id.clone()).or_insert(0u64) += 1;
        }
        for rel in input.files {
            let Some(row) = input.by_path.get(rel.as_str()) else {
                continue;
            };
            if let Some(&count) = seen.get(&row.work_id) {
                let xml_path = input.xml_root.join(rel);
                let sha = sha256_file(&xml_path).unwrap_or_default();
                works_out.push(WorkSidecar {
                    work_id: row.work_id.clone(),
                    path: rel.clone(),
                    xml_sha256: sha,
                    line_count: count,
                });
            }
            if row.work_id == after {
                break;
            }
        }
    }

    let mut skip_until_done = input.resume_after.is_some();
    let total = input.files.len();
    let mut works_seen = 0_u64;

    for (i, rel) in input.files.iter().enumerate() {
        let Some(row) = input.by_path.get(rel.as_str()).copied() else {
            if input.tty_progress {
                eprintln!("warn: no catalog row for {rel}; skipping");
            }
            continue;
        };
        if input.skip.iter().any(|c| c == &row.canon) {
            continue;
        }
        if !rel_is_under(rel) {
            return Err(format!("files.txt path escapes xml-p5: {rel}"));
        }

        if skip_until_done {
            if Some(row.work_id.as_str()) == input.resume_after {
                skip_until_done = false;
            }
            continue;
        }

        if input.tty_progress {
            eprintln!("[{}/{}] {rel}", i + 1, total);
        }

        let xml_path = input.xml_root.join(rel);
        let xml_bytes =
            fs::read(&xml_path).map_err(|e| format!("read {}: {e}", xml_path.display()))?;
        let sha = sha256_bytes(&xml_bytes);

        if let Some(prev) = prev_works.get(rel) {
            if prev.xml_sha256 == sha && prev.work_id == row.work_id {
                let mut keep = HashMap::new();
                keep.insert(row.work_id.clone(), ());
                let cached = filter_lines_by_works(&prev_lines, &keep);
                let count = cached.len() as u64;
                lines.extend(cached);
                works_out.push(WorkSidecar {
                    work_id: row.work_id.clone(),
                    path: rel.clone(),
                    xml_sha256: sha,
                    line_count: count,
                });
                works_seen += 1;
                continue;
            }
        }

        let xml = String::from_utf8_lossy(&xml_bytes);
        let parsed =
            parse_tei_lines(&xml).map_err(|e| format!("parse {}: {e}", xml_path.display()))?;
        works_seen += 1;
        let mut work_lines: Vec<IndexableLine> = Vec::with_capacity(parsed.len());
        for pl in parsed {
            let citation = format_citation(input.cbeta_tag, &pl);
            work_lines.push(IndexableLine {
                line: pl,
                title: row.title.clone(),
                author: row.author.clone(),
                citation,
                cbeta_tag: input.cbeta_tag.to_string(),
            });
        }
        let line_count = work_lines.len() as u64;
        append_parsed_jsonl(input.tmp, &work_lines)?;
        lines.extend(work_lines);
        works_out.push(WorkSidecar {
            work_id: row.work_id.clone(),
            path: rel.clone(),
            xml_sha256: sha.clone(),
            line_count,
        });
        write_progress(
            input.tmp,
            &Progress {
                schema: PROGRESS_SCHEMA.into(),
                work_id: row.work_id.clone(),
                xml_sha256: sha,
                tag: input.cbeta_tag.to_string(),
                scope_hash: input.scope_hash.to_string(),
            },
        )?;
    }

    Ok(AssembleOutput {
        lines,
        works: works_out,
        works_seen,
    })
}
