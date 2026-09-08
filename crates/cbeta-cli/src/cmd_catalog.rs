//! `cbeta catalog` / `cbeta info` product handlers.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use cbeta_core::{CatalogEntry, Command, Filters, Format, IndexInfo};
use cbeta_search::active_artifact;

use crate::env_paths::{corpus_root, index_root};
use crate::scope_io::{read_json, CatalogRow, ScopeManifest};

/// Exit 0 ok / 2 missing catalog or index meta.
pub fn run_catalog(cmd: &Command) -> i32 {
    match load_catalog_entries(&cmd.filters) {
        Ok(entries) => {
            if cmd.format == Format::Json {
                #[allow(clippy::expect_used)]
                {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&entries).expect("catalog json")
                    );
                }
            } else {
                for e in &entries {
                    println!(
                        "{}  {}  {}  {}  {}",
                        e.work_id, e.title, e.author, e.dynasty, e.work_type
                    );
                }
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

/// Exit 0 ok / 2 missing index.
pub fn run_info(cmd: &Command) -> i32 {
    match load_info() {
        Ok(info) => {
            if cmd.format == Format::Json {
                #[allow(clippy::expect_used)]
                {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&info).expect("info json")
                    );
                }
            } else {
                println!(
                    "tag={} scope={} artifact={} works={} path={}",
                    info.cbeta_tag, info.scope, info.artifact_id, info.work_count, info.index_path
                );
            }
            0
        }
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

fn load_info() -> Result<IndexInfo, String> {
    let root = index_root()?;
    let art = active_artifact(&root).map_err(|e| e.to_string())?;
    let meta_path = art.join("cbeta-meta.json");
    if meta_path.is_file() {
        let mut info: IndexInfo = read_json(&meta_path)?;
        info.index_path = art.display().to_string();
        return Ok(info);
    }
    // Fallback MANIFEST in artifact
    let man = art.join("MANIFEST.json");
    if man.is_file() {
        let m: ScopeManifest = read_json(&man)?;
        return Ok(IndexInfo {
            cbeta_tag: m.cbeta_tag.clone(),
            scope: m.scope,
            artifact_id: format!("{}+{}", m.cbeta_tag, m.scope_hash),
            work_count: m.work_count,
            index_path: art.display().to_string(),
        });
    }
    Err(format!(
        "no index info under {} (missing cbeta-meta.json)",
        art.display()
    ))
}

fn load_catalog_entries(filters: &Filters) -> Result<Vec<CatalogEntry>, String> {
    let (path, tag, scope_hash) = resolve_catalog_path()?;
    let rows = read_catalog_rows(&path)?;
    let mut out = Vec::new();
    for r in rows {
        if !filters.works.is_empty() && !filters.works.iter().any(|w| w == &r.work_id) {
            continue;
        }
        if !filters.authors.is_empty()
            && !filters
                .authors
                .iter()
                .any(|a| r.author.contains(a.as_str()))
        {
            continue;
        }
        if !filters.titles.is_empty()
            && !filters.titles.iter().any(|t| r.title.contains(t.as_str()))
        {
            continue;
        }
        if !filters.types.is_empty() && !filters.types.iter().any(|t| t == &r.work_type) {
            continue;
        }
        if !filters.canons.is_empty() && !filters.canons.iter().any(|c| c == &r.canon) {
            continue;
        }
        out.push(CatalogEntry {
            work_id: r.work_id,
            title: r.title,
            author: r.author,
            dynasty: r.dynasty,
            category: r.category,
            work_type: r.work_type,
            cbeta_tag: tag.clone(),
            scope_hash: scope_hash.clone(),
        });
    }
    Ok(out)
}

fn resolve_catalog_path() -> Result<(PathBuf, String, String), String> {
    // Prefer artifact sidecar (offline after build).
    if let Ok(root) = index_root() {
        if let Ok(art) = active_artifact(&root) {
            let cat = art.join("catalog.jsonl");
            if cat.is_file() {
                let (tag, hash) = tag_hash_from_art(&art)?;
                return Ok((cat, tag, hash));
            }
        }
    }
    // Fall back to corpus scopes via CURRENT scope name in meta, or default ci-minimal/taisho.
    let corpus = corpus_root()?;
    let root = index_root().ok();
    let scope_name = root
        .as_ref()
        .and_then(|r| active_artifact(r).ok())
        .and_then(|art| {
            let meta = art.join("cbeta-meta.json");
            read_json::<IndexInfo>(&meta).ok().map(|i| i.scope)
        })
        .unwrap_or_else(|| "ci-minimal".into());
    let scope_dir = corpus.join("scopes").join(&scope_name);
    let cat = scope_dir.join("catalog.jsonl");
    if !cat.is_file() {
        return Err(format!(
            "catalog not found at {} (build an index or set CBETA_CORPUS)",
            cat.display()
        ));
    }
    let man: ScopeManifest = read_json(&scope_dir.join("MANIFEST.json"))?;
    Ok((cat, man.cbeta_tag, man.scope_hash))
}

fn tag_hash_from_art(art: &Path) -> Result<(String, String), String> {
    let meta = art.join("cbeta-meta.json");
    if meta.is_file() {
        let v: serde_json::Value = read_json(&meta)?;
        let tag = v
            .get("cbeta_tag")
            .and_then(|x| x.as_str())
            .unwrap_or("2026R2")
            .to_string();
        let hash = v
            .get("scope_hash")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                v.get("artifact_id")
                    .and_then(|x| x.as_str())
                    .and_then(|id| id.split_once('+').map(|(_, h)| h.to_string()))
            })
            .unwrap_or_default();
        return Ok((tag, hash));
    }
    let man = art.join("MANIFEST.json");
    if man.is_file() {
        let m: ScopeManifest = read_json(&man)?;
        return Ok((m.cbeta_tag, m.scope_hash));
    }
    Ok(("2026R2".into(), String::new()))
}

fn read_catalog_rows(path: &Path) -> Result<Vec<CatalogRow>, String> {
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
