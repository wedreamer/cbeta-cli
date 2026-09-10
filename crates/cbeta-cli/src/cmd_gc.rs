//! `cbeta gc` / `cbeta prune` — remove unused corpus tag caches; never CURRENT.

use std::fs;
use std::path::Path;

use crate::cli_resolve::CliOut;
use crate::env_paths::index_root;
use crate::lifecycle::paths::{corpus_cache_root, current_tag};

/// Remove all non-CURRENT corpus tag dirs (and unused index artifacts when safe).
pub fn run(_out: &CliOut) -> i32 {
    match run_gc() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

/// Prune one tag (`out.q`) or list candidates with `--dry-run`.
pub fn run_prune(out: &CliOut) -> i32 {
    match run_prune_inner(out) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

fn run_gc() -> Result<(), String> {
    let root = corpus_cache_root()?;
    let current = current_tag().ok();
    let victims = list_prunable_tags(&root, current.as_deref())?;
    for tag in &victims {
        let dir = root.join(tag);
        remove_dir_verbose(&dir)?;
        eprintln!("gc removed {}", dir.display());
    }
    if let Err(e) = gc_unused_index_artifacts(current.as_deref()) {
        eprintln!("gc index note: {e}");
    }
    if victims.is_empty() {
        eprintln!("gc: nothing to remove");
    }
    Ok(())
}

fn run_prune_inner(out: &CliOut) -> Result<(), String> {
    let root = corpus_cache_root()?;
    let current = current_tag().ok();

    if out.dry_run {
        let tag = out.q.as_deref();
        let list = match tag {
            Some(t) if !t.is_empty() => {
                ensure_not_current(t, current.as_deref())?;
                let dir = root.join(t);
                if dir.is_dir() {
                    vec![t.to_string()]
                } else {
                    return Err(format!("prune: tag cache not found: {}", dir.display()));
                }
            }
            _ => list_prunable_tags(&root, current.as_deref())?,
        };
        for t in &list {
            println!("would-delete {}", root.join(t).display());
        }
        if list.is_empty() {
            eprintln!("prune --dry-run: nothing to delete");
        }
        return Ok(());
    }

    let tag = out
        .q
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "prune requires TAG (or --dry-run)".to_string())?;
    ensure_not_current(tag, current.as_deref())?;
    let dir = root.join(tag);
    if !dir.is_dir() {
        return Err(format!("prune: tag cache not found: {}", dir.display()));
    }
    remove_dir_verbose(&dir)?;
    eprintln!("pruned {}", dir.display());
    Ok(())
}

fn ensure_not_current(tag: &str, current: Option<&str>) -> Result<(), String> {
    if current == Some(tag) {
        return Err(format!(
            "拒绝删除 CURRENT 语料标签 {tag}；先 cbeta use <other> 再 prune"
        ));
    }
    Ok(())
}

fn list_prunable_tags(root: &Path, current: Option<&str>) -> Result<Vec<String>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = fs::read_dir(root).map_err(|e| format!("read {}: {e}", root.display()))?;
    for ent in entries {
        let ent = ent.map_err(|e| format!("read {}: {e}", root.display()))?;
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name == "CURRENT" || name == "DEFAULT" || name.starts_with('.') {
            continue;
        }
        if !ent.path().is_dir() {
            continue;
        }
        if current == Some(name.as_ref()) {
            continue;
        }
        out.push(name.into_owned());
    }
    out.sort();
    Ok(out)
}

fn remove_dir_verbose(dir: &Path) -> Result<(), String> {
    fs::remove_dir_all(dir).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            format!("无权限删除 {}: {e}", dir.display())
        } else {
            format!("删除失败 {}: {e}", dir.display())
        }
    })
}

/// Drop index artifact dirs whose basename tag prefix is not CURRENT corpus tag.
fn gc_unused_index_artifacts(corpus_current: Option<&str>) -> Result<(), String> {
    let root = match index_root() {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };
    if !root.is_dir() {
        return Ok(());
    }
    let index_current = read_index_current(&root);
    let entries = fs::read_dir(&root).map_err(|e| format!("read {}: {e}", root.display()))?;
    for ent in entries {
        let ent = ent.map_err(|e| format!("read {}: {e}", root.display()))?;
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name == "CURRENT" || name == "CURRENT.tmp" || name.starts_with('.') {
            continue;
        }
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        if index_current.as_deref() == Some(name.as_ref()) {
            continue;
        }
        if let Some(cur) = corpus_current {
            if name.starts_with(cur) {
                continue;
            }
        }
        if name.ends_with(".tmp") {
            continue;
        }
        let _ = fs::remove_dir_all(&path);
        eprintln!("gc removed index {}", path.display());
    }
    Ok(())
}

fn read_index_current(root: &Path) -> Option<String> {
    let p = root.join("CURRENT");
    let raw = fs::read_to_string(p).ok()?;
    let t = raw.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_skips_current_and_pointer_files() {
        let base = std::env::temp_dir().join(format!(
            "cbeta-gc-list-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("2026R2")).unwrap();
        fs::create_dir_all(base.join("2025R3")).unwrap();
        fs::write(base.join("CURRENT"), "2026R2\n").unwrap();
        fs::write(base.join("DEFAULT"), "2026R2\n").unwrap();
        let list = list_prunable_tags(&base, Some("2026R2")).unwrap();
        assert_eq!(list, vec!["2025R3".to_string()]);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn ensure_not_current_blocks() {
        let err = ensure_not_current("2026R2", Some("2026R2")).unwrap_err();
        assert!(err.contains("CURRENT"));
    }
}
