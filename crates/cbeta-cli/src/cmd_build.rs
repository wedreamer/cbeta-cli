//! `cbeta build --scope`: parse TEI + write Tantivy artifact (incr + resume).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use cbeta_core::{Command, Format, IndexInfo};
use cbeta_index::{
    artifact_dir_name, artifact_id, publish_dir, require_path_segment, resolve_publish_dest,
    stage_tmp_dir, tmp_dir_for, write_current_pointer, write_tantivy_dir,
};
use cbeta_parse::GaijiMap;

use crate::build_assemble::{assemble_lines, AssembleInput, SKIP_CANONS};
use crate::build_golden::{check_golden, verify_lock};
use crate::build_incr::{write_parsed_jsonl, write_works_sidecar};
use crate::build_progress::{decide_tmp, progress_path, TmpDecision};
use crate::env_paths::{corpus_root, index_root, xml_p5_root};
use crate::scope_io::{read_catalog, read_files_list, read_json, CatalogRow, ScopeManifest};

/// Run build; returns process exit code (0 ok, 2 usage/missing corpus).
pub fn run(cmd: &Command, full: bool) -> i32 {
    let scope = match cmd.q.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!("build requires --scope <name>");
            return 2;
        }
    };
    let tty = cmd.format != Format::Json;
    match build_scope(scope, full, tty) {
        Ok(info) => {
            if cmd.format == Format::Json {
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

/// Build one scope; `full` forces wipe + reparse all works.
///
/// # Errors
/// Missing corpus/scope, parse failure, golden/lock gate, publish failure.
pub(crate) fn build_scope_with_full(scope: &str, full: bool) -> Result<IndexInfo, String> {
    build_scope(scope, full, true)
}

fn build_scope(scope: &str, full: bool, tty_progress: bool) -> Result<IndexInfo, String> {
    let _lock_guard = crate::lifecycle::flock::try_acquire()?;

    let corpus = corpus_root()?;
    if !corpus.is_dir() {
        return Err(format!(
            "corpus not found at {} (set CBETA_CORPUS)",
            corpus.display()
        ));
    }

    require_path_segment(scope).map_err(|e| e.to_string())?;
    let scope_dir = corpus.join("scopes").join(scope);
    let manifest_path = scope_dir.join("MANIFEST.json");
    if !manifest_path.is_file() {
        return Err(format!(
            "scope `{scope}` not found (missing {})",
            manifest_path.display()
        ));
    }

    let manifest: ScopeManifest = read_json(&manifest_path)?;
    require_path_segment(&manifest.cbeta_tag).map_err(|e| e.to_string())?;
    require_path_segment(&manifest.scope_hash).map_err(|e| e.to_string())?;
    require_path_segment(&manifest.scope).map_err(|e| e.to_string())?;
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

    let by_path: HashMap<&str, &CatalogRow> =
        catalog.iter().map(|r| (r.path.as_str(), r)).collect();

    let xml_root = xml_p5_root(&corpus);
    if !xml_root.is_dir() {
        return Err(format!(
            "xml-p5 not found at {} or {}/src/xml-p5 (set CBETA_CORPUS)",
            corpus.join("xml-p5").display(),
            corpus.display()
        ));
    }

    let root = index_root()?;
    fs::create_dir_all(&root).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            format!("无权限创建索引目录 {}: {e}", root.display())
        } else {
            format!("create index root: {e}")
        }
    })?;
    crate::lifecycle::flock::ensure_parent_writable(&root.join(".probe"))?;

    let art_name =
        artifact_dir_name(&manifest.cbeta_tag, &manifest.scope_hash).map_err(|e| e.to_string())?;
    let intended = root.join(&art_name);
    let tmp_path = tmp_dir_for(&intended);

    let decision = decide_tmp(&tmp_path, &manifest.cbeta_tag, &manifest.scope_hash, full);
    let (tmp, resume_after) = match decision {
        TmpDecision::Reuse { progress } => (tmp_path, Some(progress.work_id)),
        TmpDecision::Wipe => {
            let t = stage_tmp_dir(&intended).map_err(|e| format!("stage tmp: {e}"))?;
            (t, None)
        }
    };

    let assembled = assemble_lines(&AssembleInput {
        files: &files,
        by_path: &by_path,
        skip: &skip,
        xml_root: &xml_root,
        tmp: &tmp,
        intended: &intended,
        resume_after: resume_after.as_deref(),
        full,
        cbeta_tag: &manifest.cbeta_tag,
        scope_hash: &manifest.scope_hash,
        tty_progress,
    })?;

    check_golden(&assembled.lines, &catalog)?;
    if std::env::var_os("CBETA_CORPUS").is_none() {
        verify_lock(&manifest.cbeta_tag)?;
    }

    let gaiji = GaijiMap::default();
    write_tantivy_dir(&tmp, &assembled.lines, &gaiji).map_err(|e| format!("write index: {e}"))?;

    let dest = resolve_publish_dest(&intended).map_err(|e| format!("resolve publish dest: {e}"))?;

    let work_count = if manifest.work_count > 0 {
        manifest.work_count
    } else {
        assembled.works.len() as u64
    };

    let info = IndexInfo {
        cbeta_tag: manifest.cbeta_tag.clone(),
        scope: manifest.scope.clone(),
        artifact_id: artifact_id(&manifest.cbeta_tag, &manifest.scope_hash),
        work_count,
        index_path: dest.display().to_string(),
    };

    write_sidecar(&tmp, &info, &manifest, &catalog, &catalog_path)?;
    write_works_sidecar(&tmp, &assembled.works)?;
    write_parsed_jsonl(&tmp, &assembled.lines)?;
    let _ = fs::remove_file(progress_path(&tmp));

    publish_dir(&tmp, &dest).map_err(|e| format!("publish artifact: {e}"))?;

    let dest_basename = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("bad dest basename: {}", dest.display()))?;
    write_current_pointer(&root, dest_basename).map_err(|e| format!("write CURRENT: {e}"))?;

    if tty_progress {
        eprintln!(
            "indexed {} works, {} lines → {}",
            assembled.works_seen,
            assembled.lines.len(),
            dest.display()
        );
    }

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
            "cbeta-cmd-build-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn copy_tree(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for ent in fs::read_dir(src).unwrap() {
            let ent = ent.unwrap();
            let to = dst.join(ent.file_name());
            if ent.file_type().unwrap().is_dir() {
                copy_tree(&ent.path(), &to);
            } else {
                fs::copy(ent.path(), to).unwrap();
            }
        }
    }

    #[test]
    fn run_requires_scope() {
        let cmd = Command {
            action: Action::Build,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd, false), 2);
        let cmd2 = Command {
            q: Some(String::new()),
            ..cmd
        };
        assert_eq!(run(&cmd2, false), 2);
    }

    #[test]
    fn run_missing_corpus_and_scope() {
        let _g = env_lock();
        let index = temp_dir("idx");
        let missing = temp_dir("no-corpus");
        std::env::set_var("CBETA_CORPUS", &missing);
        std::env::set_var("CBETA_INDEX", &index);
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
        assert_eq!(run(&cmd, false), 2);
        let file_corpus = temp_dir("file-corp").join("not-a-dir");
        fs::write(&file_corpus, "x").unwrap();
        std::env::set_var("CBETA_CORPUS", &file_corpus);
        assert_eq!(run(&cmd, false), 2);
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&index);
        let _ = fs::remove_dir_all(&missing);
    }

    #[test]
    fn run_missing_xml_p5() {
        let _g = env_lock();
        let corpus = temp_dir("corp-no-xml");
        copy_tree(&mini_corpus().join("scopes"), &corpus.join("scopes"));
        let index = temp_dir("idx-no-xml");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        let cmd = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Plain,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd, false), 2);
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&corpus);
        let _ = fs::remove_dir_all(&index);
    }

    #[test]
    fn run_builds_json_and_rebuilds_existing() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("idx-ok");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        let mut cmd = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd, false), 0);
        assert!(index.join("2026R2-c1f1x7a0").is_dir());
        assert!(index.join("2026R2-c1f1x7a0").join("works.json").is_file());
        cmd.format = Format::Plain;
        assert_eq!(run(&cmd, false), 0);
        assert_eq!(run(&cmd, true), 0);
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&index);
    }

    #[test]
    fn build_skips_unknown_file_and_category_b_canon() {
        let _g = env_lock();
        let corpus = temp_dir("corp-skip");
        copy_tree(&mini_corpus().join("scopes"), &corpus.join("scopes"));
        copy_tree(&mini_corpus().join("xml-p5"), &corpus.join("xml-p5"));
        let files = corpus.join("scopes/ci-minimal/files.txt");
        let mut body = fs::read_to_string(&files).unwrap();
        body.push_str("T/missing/nope.xml\n");
        fs::write(&files, body).unwrap();
        let index = temp_dir("idx-skip");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
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
        assert_eq!(run(&cmd, false), 0);
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&corpus);
        let _ = fs::remove_dir_all(&index);
    }

    #[test]
    fn write_sidecar_writes_catalog_when_src_missing() {
        let dir = temp_dir("sidecar");
        let art = dir.join("art");
        fs::create_dir_all(&art).unwrap();
        let info = IndexInfo {
            cbeta_tag: "2026R2".into(),
            scope: "s".into(),
            artifact_id: "2026R2+h".into(),
            work_count: 1,
            index_path: art.display().to_string(),
        };
        let manifest = ScopeManifest {
            cbeta_tag: "2026R2".into(),
            scope: "s".into(),
            scope_hash: "h".into(),
            work_count: 0,
            exclude_canons: vec![],
        };
        fs::write(
            art.join("MANIFEST.json"),
            r#"{"cbeta_tag":"2026R2","scope":"s","scope_hash":"h"}"#,
        )
        .unwrap();
        let catalog = vec![CatalogRow {
            work_id: "T1".into(),
            canon: "T".into(),
            path: "a.xml".into(),
            title: "t".into(),
            author: "a".into(),
            dynasty: String::new(),
            category: String::new(),
            work_type: String::new(),
        }];
        let missing_cat = art.join("no-catalog.jsonl");
        write_sidecar(&art, &info, &manifest, &catalog, &missing_cat).unwrap();
        assert!(art.join("cbeta-meta.json").is_file());
        assert!(art.join("catalog.jsonl").is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
