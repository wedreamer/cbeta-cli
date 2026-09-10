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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use cbeta_core::{Action, Filters};

    use crate::cli_resolve::CliOut;
    use crate::env_paths::env_lock;

    fn bare() -> CliOut {
        CliOut {
            action: Action::Gc,
            q: None,
            json: false,
            mode: None,
            explain: false,
            plain: false,
            script: None,
            filters: Filters::default(),
            context: 0,
            copy: false,
            save: None,
            from: None,
            hit_index: None,
            shell: None,
            http: None,
            release: None,
            remote: false,
            apply: false,
            full: false,
            dry_run: false,
            default_tag: None,
            scope: None,
        }
    }

    fn temp_home(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-gc-{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join(".cbeta").join("corpus")).unwrap();
        p
    }

    #[test]
    fn list_skips_current_and_pointer_files() {
        let base = std::env::temp_dir().join(format!(
            "cbeta-gc-list-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
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

    #[test]
    fn list_prunable_on_non_dir_is_empty() {
        let p = PathBuf::from("/tmp/cbeta-gc-not-a-dir-does-not-exist-xyz");
        assert!(list_prunable_tags(&p, None).unwrap().is_empty());
    }

    #[test]
    fn run_gc_removes_non_current_keeps_current() {
        let _g = env_lock();
        let home = temp_home("run");
        let cache = home.join(".cbeta/corpus");
        fs::create_dir_all(cache.join("2026R2")).unwrap();
        fs::create_dir_all(cache.join("2025R3")).unwrap();
        fs::write(cache.join("CURRENT"), "2026R2\n").unwrap();
        let idx = home.join("idx");
        fs::create_dir_all(&idx).unwrap();

        std::env::set_var("HOME", &home);
        std::env::set_var("CBETA_INDEX", &idx);
        std::env::remove_var("CBETA_CORPUS");

        assert_eq!(run(&bare()), 0);
        assert!(cache.join("2026R2").is_dir());
        assert!(!cache.join("2025R3").exists());

        std::env::remove_var("HOME");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_prune_dry_run_lists_would_delete() {
        let _g = env_lock();
        let home = temp_home("dry");
        let cache = home.join(".cbeta/corpus");
        fs::create_dir_all(cache.join("2026R2")).unwrap();
        fs::create_dir_all(cache.join("2025R3")).unwrap();
        fs::write(cache.join("CURRENT"), "2026R2\n").unwrap();
        std::env::set_var("HOME", &home);
        std::env::remove_var("CBETA_CORPUS");

        let mut out = bare();
        out.action = Action::Prune;
        out.dry_run = true;
        assert_eq!(run_prune(&out), 0);
        assert!(cache.join("2025R3").is_dir(), "dry-run must not delete");

        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_prune_deletes_named_tag() {
        let _g = env_lock();
        let home = temp_home("del");
        let cache = home.join(".cbeta/corpus");
        fs::create_dir_all(cache.join("2026R2")).unwrap();
        fs::create_dir_all(cache.join("2025R3")).unwrap();
        fs::write(cache.join("CURRENT"), "2026R2\n").unwrap();
        std::env::set_var("HOME", &home);

        let mut out = bare();
        out.action = Action::Prune;
        out.q = Some("2025R3".into());
        assert_eq!(run_prune(&out), 0);
        assert!(!cache.join("2025R3").exists());
        assert!(cache.join("2026R2").is_dir());

        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_prune_missing_tag_exits_2() {
        let _g = env_lock();
        let home = temp_home("miss");
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2026R2\n").unwrap();
        fs::create_dir_all(home.join(".cbeta/corpus/2026R2")).unwrap();
        std::env::set_var("HOME", &home);

        let mut out = bare();
        out.q = Some("2099R1".into());
        assert_eq!(run_prune(&out), 2);

        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_prune_empty_q_without_dry_run_exits_2() {
        let _g = env_lock();
        let home = temp_home("empty-q");
        fs::create_dir_all(home.join(".cbeta/corpus")).unwrap();
        std::env::set_var("HOME", &home);

        let out = bare();
        assert_eq!(run_prune(&out), 2);

        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn gc_unused_index_drops_stale_keeps_current_artifact() {
        let _g = env_lock();
        let home = temp_home("idx");
        let cache = home.join(".cbeta/corpus");
        fs::create_dir_all(cache.join("2026R2")).unwrap();
        fs::write(cache.join("CURRENT"), "2026R2\n").unwrap();
        let idx = home.join("idx");
        fs::create_dir_all(idx.join("2026R2+abc")).unwrap();
        fs::create_dir_all(idx.join("2025R3+old")).unwrap();
        fs::write(idx.join("CURRENT"), "2026R2+abc\n").unwrap();

        std::env::set_var("HOME", &home);
        std::env::set_var("CBETA_INDEX", &idx);

        assert_eq!(run(&bare()), 0);
        assert!(idx.join("2026R2+abc").is_dir());
        assert!(!idx.join("2025R3+old").exists());

        std::env::remove_var("HOME");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn read_index_current_empty_is_none() {
        let base = temp_home("empty-cur");
        let idx = base.join("idx");
        fs::create_dir_all(&idx).unwrap();
        fs::write(idx.join("CURRENT"), "\n").unwrap();
        assert!(read_index_current(&idx).is_none());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn run_gc_nothing_to_remove() {
        let _g = env_lock();
        let home = temp_home("empty-gc");
        let cache = home.join(".cbeta/corpus");
        fs::create_dir_all(cache.join("2026R2")).unwrap();
        fs::write(cache.join("CURRENT"), "2026R2\n").unwrap();
        fs::write(cache.join("note.txt"), "file not dir\n").unwrap();
        let idx = home.join("idx");
        fs::create_dir_all(&idx).unwrap();
        std::env::set_var("HOME", &home);
        std::env::set_var("CBETA_INDEX", &idx);
        assert_eq!(run(&bare()), 0);
        assert!(cache.join("2026R2").is_dir());
        std::env::remove_var("HOME");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_prune_dry_run_named_missing_exits_2() {
        let _g = env_lock();
        let home = temp_home("dry-miss");
        fs::create_dir_all(home.join(".cbeta/corpus/2026R2")).unwrap();
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2026R2\n").unwrap();
        std::env::set_var("HOME", &home);
        let mut out = bare();
        out.dry_run = true;
        out.q = Some("2099R1".into());
        assert_eq!(run_prune(&out), 2);
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_prune_dry_run_nothing() {
        let _g = env_lock();
        let home = temp_home("dry-empty");
        fs::create_dir_all(home.join(".cbeta/corpus/2026R2")).unwrap();
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2026R2\n").unwrap();
        std::env::set_var("HOME", &home);
        let mut out = bare();
        out.dry_run = true;
        assert_eq!(run_prune(&out), 0);
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }
}
