//! `cbeta build --scope`: parse TEI + write Tantivy artifact.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use cbeta_core::{Command, Format, IndexInfo};
use cbeta_index::{artifact_id, write_artifact, IndexableLine};
use cbeta_parse::{parse_tei_lines, GaijiMap};

use crate::citation::format_citation;
use crate::env_paths::{corpus_root, index_root, xml_p5_root};
use crate::scope_io::{read_catalog, read_files_list, read_json, CatalogRow, ScopeManifest};

/// Canons never indexed by default (Category B).
const SKIP_CANONS: &[&str] = &["Y", "TX", "LC", "YP"];

/// Run build; returns process exit code (0 ok, 2 usage/missing corpus).
pub fn run(cmd: &Command) -> i32 {
    let scope = match cmd.q.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!("build requires --scope <name>");
            return 2;
        }
    };

    match build_scope(scope) {
        Ok(info) => {
            if cmd.format == Format::Json {
                // IndexInfo is our Serialize type; pretty-print cannot fail on it.
                #[allow(clippy::expect_used)]
                {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&info).expect("IndexInfo json")
                    );
                }
            } else {
                eprintln!(
                    "built {} works → {} ({})",
                    info.work_count, info.artifact_id, info.index_path
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

fn build_scope(scope: &str) -> Result<IndexInfo, String> {
    let corpus = corpus_root()?;
    if !corpus.is_dir() {
        return Err(format!(
            "corpus not found at {} (set CBETA_CORPUS)",
            corpus.display()
        ));
    }

    let scope_dir = corpus.join("scopes").join(scope);
    let manifest_path = scope_dir.join("MANIFEST.json");
    if !manifest_path.is_file() {
        return Err(format!(
            "scope `{scope}` not found (missing {})",
            manifest_path.display()
        ));
    }

    let manifest: ScopeManifest = read_json(&manifest_path)?;
    let catalog_path = scope_dir.join("catalog.jsonl");
    let files_path = scope_dir.join("files.txt");
    let catalog = read_catalog(&catalog_path)?;
    let files = read_files_list(&files_path)?;

    let mut skip: Vec<String> = SKIP_CANONS.iter().map(|s| (*s).to_string()).collect();
    for c in &manifest.exclude_canons {
        if !skip.iter().any(|s| s == c) {
            skip.push(c.clone());
        }
    }

    let by_path: std::collections::HashMap<&str, &CatalogRow> =
        catalog.iter().map(|r| (r.path.as_str(), r)).collect();

    let xml_root = xml_p5_root(&corpus);
    if !xml_root.is_dir() {
        return Err(format!(
            "xml-p5 not found at {} or {}/src/xml-p5 (set CBETA_CORPUS)",
            corpus.join("xml-p5").display(),
            corpus.display()
        ));
    }
    let gaiji = GaijiMap::default();
    let mut lines: Vec<IndexableLine> = Vec::new();
    let mut works_seen = 0_u64;

    for rel in &files {
        let Some(row) = by_path.get(rel.as_str()).copied() else {
            eprintln!("warn: no catalog row for {rel}; skipping");
            continue;
        };
        if skip.iter().any(|c| c == &row.canon) {
            continue;
        }
        let xml_path = xml_root.join(rel);
        let xml = fs::read_to_string(&xml_path)
            .map_err(|e| format!("read {}: {e}", xml_path.display()))?;
        let parsed =
            parse_tei_lines(&xml).map_err(|e| format!("parse {}: {e}", xml_path.display()))?;
        works_seen += 1;
        for pl in parsed {
            let citation = format_citation(&manifest.cbeta_tag, &pl);
            lines.push(IndexableLine {
                line: pl,
                title: row.title.clone(),
                author: row.author.clone(),
                citation,
                cbeta_tag: manifest.cbeta_tag.clone(),
            });
        }
    }

    let root = index_root()?;
    fs::create_dir_all(&root).map_err(|e| format!("create index root: {e}"))?;

    let art_name = format!("{}-{}", manifest.cbeta_tag, manifest.scope_hash);
    let final_dir = root.join(&art_name);
    if final_dir.exists() {
        fs::remove_dir_all(&final_dir)
            .map_err(|e| format!("remove existing artifact {}: {e}", final_dir.display()))?;
    }

    let written = write_artifact(
        &root,
        &manifest.cbeta_tag,
        &manifest.scope_hash,
        &lines,
        &gaiji,
    )
    .map_err(|e| format!("write index: {e}"))?;

    let work_count = if manifest.work_count > 0 {
        manifest.work_count
    } else {
        works_seen
    };

    let info = IndexInfo {
        cbeta_tag: manifest.cbeta_tag.clone(),
        scope: manifest.scope.clone(),
        artifact_id: artifact_id(&manifest.cbeta_tag, &manifest.scope_hash),
        work_count,
        index_path: written.display().to_string(),
    };

    write_sidecar(&written, &info, &manifest, &catalog, &catalog_path)?;
    fs::write(root.join("CURRENT"), format!("{art_name}\n"))
        .map_err(|e| format!("write CURRENT: {e}"))?;

    eprintln!(
        "indexed {works_seen} works, {} lines → {}",
        lines.len(),
        written.display()
    );

    Ok(info)
}

fn write_sidecar(
    art: &Path,
    info: &IndexInfo,
    manifest: &ScopeManifest,
    catalog: &[CatalogRow],
    catalog_src: &Path,
) -> Result<(), String> {
    let meta_path = art.join("cbeta-meta.json");
    let mut meta = serde_json::to_value(info).map_err(|e| e.to_string())?;
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "scope_hash".into(),
            serde_json::Value::String(manifest.scope_hash.clone()),
        );
    }
    let meta_s = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(&meta_path, meta_s).map_err(|e| format!("write cbeta-meta.json: {e}"))?;

    let man_src = catalog_src
        .parent()
        .map(|p| p.join("MANIFEST.json"))
        .unwrap_or_else(|| PathBuf::from("MANIFEST.json"));
    fs::copy(&man_src, art.join("MANIFEST.json"))
        .map_err(|e| format!("copy MANIFEST.json: {e}"))?;

    let cat_dst = art.join("catalog.jsonl");
    if catalog_src.is_file() {
        fs::copy(catalog_src, &cat_dst).map_err(|e| format!("copy catalog.jsonl: {e}"))?;
    } else {
        let mut f = fs::File::create(&cat_dst).map_err(|e| e.to_string())?;
        for row in catalog {
            let line = serde_json::to_string(row).map_err(|e| e.to_string())?;
            writeln!(f, "{line}").map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
