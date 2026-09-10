//! `cbeta pull` — report remote/lock latest vs CURRENT; optional `--apply` fetch.

use cbeta_core::{Action, Filters};

use crate::cli_resolve::CliOut;
use crate::cmd_fetch;
use crate::lifecycle::git::{git_available, ls_remote_tags, rewrite_url, MISSING_GIT_MSG};
use crate::lifecycle::lock::{cmp_release_tag, load_lock, newest_lock_tag, resolve_release};
use crate::lifecycle::paths::current_tag;

/// Offline / network failure hint (same spirit as `cmd_releases`).
const OFFLINE_HINT: &str =
    "无法访问远程 xml-p5（网络或镜像）。可设置 CBETA_GIT_BASE 指向镜像，例如 https://mirror.example/github.com";

/// Report newer releases; with `--apply`, fetch the newer lock tag. Never writes CURRENT.
pub fn run(out: &CliOut) -> i32 {
    match run_inner(out) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

fn run_inner(out: &CliOut) -> Result<(), String> {
    if !git_available() {
        return Err(format!("{MISSING_GIT_MSG}\n{OFFLINE_HINT}"));
    }

    let lock = load_lock()?;
    let url = rewrite_url(&lock.official.xml_p5);
    let remote =
        ls_remote_tags(&url).map_err(|e| format!("git ls-remote failed: {e}\n{OFFLINE_HINT}"))?;

    let current = current_tag().ok();
    let newest_lock = newest_lock_tag(&lock).map(str::to_string);
    let newest_remote_in_lock = remote
        .keys()
        .filter(|t| lock.releases.contains_key(t.as_str()))
        .max_by(|a, b| cmp_release_tag(a, b))
        .cloned();
    let latest = newest_remote_in_lock
        .or_else(|| newest_lock.clone())
        .ok_or_else(|| "no releases in lock".to_string())?;

    let behind = match current.as_deref() {
        None => true,
        Some(cur) => cmp_release_tag(cur, &latest) == std::cmp::Ordering::Less,
    };

    println!(
        "current={} latest={} behind={}",
        current.as_deref().unwrap_or("(none)"),
        latest,
        behind
    );

    if !out.apply {
        return Ok(());
    }

    if !behind {
        eprintln!("already at latest {latest}; nothing to fetch");
        return Ok(());
    }

    let tag = resolve_release(&latest, &lock, Some(std::slice::from_ref(&latest)))?;
    let fetch_out = CliOut {
        action: Action::Fetch,
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
        release: Some(tag),
        remote: false,
        apply: false,
        full: false,
        dry_run: false,
        default_tag: None,
        scope: out.scope.clone(),
    };
    let code = cmd_fetch::run(&fetch_out);
    if code != 0 {
        return Err("pull --apply: fetch failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use cbeta_core::{Action, Filters};

    use crate::env_paths::env_lock;

    fn bare() -> CliOut {
        CliOut {
            action: Action::Pull,
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
            "cbeta-pull-{prefix}-{}-{}",
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

    fn install_fake_git(bin_dir: &Path) {
        fs::create_dir_all(bin_dir).unwrap();
        let script = bin_dir.join("git");
        fs::write(
            &script,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "git version 2.40.0"; exit 0; fi
if [ "$1" = "ls-remote" ]; then
  echo "dbdea41071e1e260ad84b72faefd4587333cf76d	refs/tags/2026R2"
  echo "dbdea41071e1e260ad84b72faefd4587333cf76d	refs/tags/2026R2^{}"
  echo "2b8ab8d5e4fe957a9b94f2cde01cb0d2e2dcd2b9	refs/tags/2026R1"
  exit 0
fi
exit 1
"#,
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
    }

    fn prepend_path(bin_dir: &Path) -> String {
        let old = std::env::var("PATH").unwrap_or_default();
        format!("{}:{old}", bin_dir.display())
    }

    #[test]
    fn pull_report_behind_success() {
        let _g = env_lock();
        let home = temp_home("behind");
        let bin = home.join("bin");
        install_fake_git(&bin);
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2025R3\n").unwrap();
        fs::create_dir_all(home.join(".cbeta/corpus/2025R3")).unwrap();

        let old_path = std::env::var("PATH").ok();
        std::env::set_var("HOME", &home);
        std::env::set_var("PATH", prepend_path(&bin));
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_GIT_BASE");

        let code = run(&bare());
        assert_eq!(code, 0);
        let cur = fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
        assert_eq!(cur.trim(), "2025R3", "pull must never rewrite CURRENT");

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn pull_already_at_latest_with_apply() {
        let _g = env_lock();
        let home = temp_home("latest");
        let bin = home.join("bin");
        install_fake_git(&bin);
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2026R2\n").unwrap();
        fs::create_dir_all(home.join(".cbeta/corpus/2026R2")).unwrap();

        let old_path = std::env::var("PATH").ok();
        std::env::set_var("HOME", &home);
        std::env::set_var("PATH", prepend_path(&bin));
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_GIT_BASE");

        let mut out = bare();
        out.apply = true;
        let code = run(&out);
        assert_eq!(code, 0);
        let cur = fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
        assert_eq!(cur.trim(), "2026R2");

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn pull_apply_behind_fetch_fails() {
        let _g = env_lock();
        let home = temp_home("apply-fail");
        let bin = home.join("bin");
        install_fake_git(&bin);
        let idx = home.join("idx");
        fs::create_dir_all(&idx).unwrap();
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2025R3\n").unwrap();
        fs::create_dir_all(home.join(".cbeta/corpus/2025R3")).unwrap();

        let old_path = std::env::var("PATH").ok();
        std::env::set_var("HOME", &home);
        std::env::set_var("CBETA_INDEX", &idx);
        std::env::set_var("PATH", prepend_path(&bin));
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_GIT_BASE");

        let mut out = bare();
        out.apply = true;
        let code = run(&out);
        assert_eq!(code, 2);
        let cur = fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
        assert_eq!(cur.trim(), "2025R3", "failed apply must leave CURRENT");

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("HOME");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn pull_ls_remote_fail_exits_2() {
        let _g = env_lock();
        let home = temp_home("ls-fail");
        let bin = home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let script = bin.join("git");
        fs::write(
            &script,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "git version 2.40.0"; exit 0; fi
exit 1
"#,
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2026R2\n").unwrap();

        let old_path = std::env::var("PATH").ok();
        std::env::set_var("HOME", &home);
        std::env::set_var("PATH", prepend_path(&bin));
        std::env::remove_var("CBETA_GIT_BASE");

        assert_eq!(run(&bare()), 2);

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }
}
