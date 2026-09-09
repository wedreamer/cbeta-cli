//! `cbeta catalog` / `cbeta info` product handlers.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use cbeta_core::{CatalogEntry, Command, Filters, Format, IndexInfo};
use cbeta_search::active_artifact;

use crate::env_paths::{corpus_root, index_root};
use crate::scope_io::{read_json, CatalogRow, ScopeManifest};

/// Exit 0 hits / 1 no match / 2 missing catalog or index meta.
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
            if entries.is_empty() {
                1
            } else {
                0
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Filters, Format};

    fn mini_corpus() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-cmd-cat-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn build_mini(index: &Path) {
        std::env::set_var("CBETA_CORPUS", mini_corpus());
        std::env::set_var("CBETA_INDEX", index);
        let cmd = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(crate::cmd_build::run(&cmd), 0);
    }

    #[test]
    fn catalog_and_info_json_tty_and_filters() {
        let _g = env_lock();
        let index = temp_dir("idx");
        build_mini(&index);

        let mut cmd = Command {
            action: Action::Catalog,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run_catalog(&cmd), 0);
        cmd.format = Format::Tty;
        assert_eq!(run_catalog(&cmd), 0);

        cmd.filters.works = vec!["T1578".into()];
        assert_eq!(run_catalog(&cmd), 0);
        cmd.filters = Filters {
            authors: vec!["玄奘".into()],
            ..Filters::default()
        };
        assert_eq!(run_catalog(&cmd), 0);
        cmd.filters = Filters {
            titles: vec!["掌珍".into()],
            ..Filters::default()
        };
        assert_eq!(run_catalog(&cmd), 0);
        cmd.filters = Filters {
            types: vec!["lun".into()],
            ..Filters::default()
        };
        assert_eq!(run_catalog(&cmd), 0);
        cmd.filters = Filters {
            canons: vec!["T".into()],
            ..Filters::default()
        };
        assert_eq!(run_catalog(&cmd), 0);
        cmd.filters = Filters {
            works: vec!["NOPE".into()],
            ..Filters::default()
        };
        assert_eq!(run_catalog(&cmd), 1);

        cmd.action = Action::Info;
        cmd.filters = Filters::default();
        cmd.format = Format::Json;
        assert_eq!(run_info(&cmd), 0);
        cmd.format = Format::Tty;
        assert_eq!(run_info(&cmd), 0);

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&index);
    }

    #[test]
    fn catalog_and_info_error_paths() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("HOME");
        let cmd = Command {
            action: Action::Catalog,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run_catalog(&cmd), 2);
        assert_eq!(run_info(&cmd), 2);
    }

    #[test]
    fn info_manifest_fallback_and_tag_hash_paths() {
        let _g = env_lock();
        let index = temp_dir("man-fb");
        build_mini(&index);
        let art = index.join("2026R2-c1f1x7a0");
        let _ = fs::remove_file(art.join("cbeta-meta.json"));
        std::env::set_var("CBETA_INDEX", &index);
        let cmd = Command {
            action: Action::Info,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run_info(&cmd), 0);

        let (tag, hash) = tag_hash_from_art(&art).unwrap();
        assert_eq!(tag, "2026R2");
        assert_eq!(hash, "c1f1x7a0");

        let bare = temp_dir("bare-art");
        let (t2, h2) = tag_hash_from_art(&bare).unwrap();
        assert_eq!(t2, "2026R2");
        assert!(h2.is_empty());

        let meta_only = temp_dir("meta-art");
        fs::write(
            meta_only.join("cbeta-meta.json"),
            r#"{"cbeta_tag":"2026R2","artifact_id":"2026R2+deadbeef","scope":"s","work_count":1,"index_path":"x"}"#,
        )
        .unwrap();
        let (t3, h3) = tag_hash_from_art(&meta_only).unwrap();
        assert_eq!(t3, "2026R2");
        assert_eq!(h3, "deadbeef");

        fs::write(index.join("CURRENT"), "2026R2-c1f1x7a0\n").unwrap();
        let _ = fs::remove_file(art.join("MANIFEST.json"));
        assert!(load_info().is_err());

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&index);
        let _ = fs::remove_dir_all(&bare);
        let _ = fs::remove_dir_all(&meta_only);
    }

    #[test]
    fn catalog_falls_back_to_corpus_scope() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("no-cat-sidecar");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        let cmd = Command {
            action: Action::Catalog,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run_catalog(&cmd), 0);

        let cat = corpus.join("scopes/ci-minimal/catalog.jsonl");
        let rows = read_catalog_rows(&cat).unwrap();
        assert!(!rows.is_empty());
        let blank = temp_dir("blank-cat");
        let blank_cat = blank.join("c.jsonl");
        fs::write(&blank_cat, "\n\n").unwrap();
        assert!(read_catalog_rows(&blank_cat).unwrap().is_empty());

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&index);
        let _ = fs::remove_dir_all(&blank);
    }

    #[test]
    fn catalog_missing_corpus_catalog_errors() {
        let _g = env_lock();
        let corpus = temp_dir("empty-corp");
        let index = temp_dir("empty-idx");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        let cmd = Command {
            action: Action::Catalog,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run_catalog(&cmd), 2);
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&corpus);
        let _ = fs::remove_dir_all(&index);
    }
}
