//! `cbeta releases` — local lock tags; `--remote` via `git ls-remote`.

use crate::cli_resolve::CliOut;
use crate::lifecycle::git::{git_available, ls_remote_tags, rewrite_url, MISSING_GIT_MSG};
use crate::lifecycle::lock::{cmp_release_tag, load_lock, lock_tags, newest_lock_tag, ReleaseLock};
use crate::lifecycle::paths::current_tag;

/// Offline / network failure hint (Chinese + CBETA_GIT_BASE).
const OFFLINE_HINT: &str =
    "无法访问远程 xml-p5（网络或镜像）。可设置 CBETA_GIT_BASE 指向镜像，例如 https://mirror.example/github.com";

/// Print lock tags; mark CURRENT with `* `; mark newest as `latest`.
pub fn run(out: &CliOut) -> i32 {
    let lock = match load_lock() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    if out.remote {
        return run_remote(&lock);
    }

    print_tags(&lock, lock_tags(&lock));
    0
}

fn run_remote(lock: &ReleaseLock) -> i32 {
    if !git_available() {
        eprintln!("{MISSING_GIT_MSG}");
        eprintln!("{OFFLINE_HINT}");
        return 2;
    }
    let url = rewrite_url(&lock.official.xml_p5);
    let remote = match ls_remote_tags(&url) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("git ls-remote failed: {e}");
            eprintln!("{OFFLINE_HINT}");
            return 2;
        }
    };

    // Tags in the lock, optionally annotated with remote presence.
    let mut tags = lock_tags(lock);
    // Also surface remote-only tags that are not master / xml-p5-2018 (already filtered).
    for t in remote.keys() {
        if !tags.iter().any(|x| x == t) && lock.releases.contains_key(t) {
            tags.push(t.clone());
        }
    }
    tags.sort_by(|a, b| cmp_release_tag(a, b));
    tags.dedup();
    print_tags(lock, tags);
    0
}

fn print_tags(lock: &ReleaseLock, tags: Vec<String>) {
    let current = current_tag().ok();
    let latest = newest_lock_tag(lock).map(str::to_string);
    for tag in tags {
        let star = if current.as_deref() == Some(tag.as_str()) {
            "* "
        } else {
            "  "
        };
        if latest.as_deref() == Some(tag.as_str()) {
            println!("{star}{tag}  (latest)");
        } else {
            println!("{star}{tag}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_resolve::CliOut;
    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Filters};

    fn bare() -> CliOut {
        CliOut {
            action: Action::Releases,
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

    #[test]
    fn releases_local_lists_lock_tags() {
        let code = run(&bare());
        assert_eq!(code, 0);
    }

    #[test]
    fn releases_remote_exits_2() {
        let _g = env_lock();
        std::env::set_var("CBETA_GIT_BASE", "http://127.0.0.1:1");
        let mut out = bare();
        out.remote = true;
        let code = run(&out);
        assert_eq!(code, 2);
        std::env::remove_var("CBETA_GIT_BASE");
    }

    #[test]
    fn releases_local_with_current_marks_star() {
        let _g = env_lock();
        let home = std::env::temp_dir().join(format!(
            "cbeta-rel-cur-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cbeta/corpus/2026R2")).unwrap();
        std::fs::write(home.join(".cbeta/corpus/CURRENT"), "2026R2\n").unwrap();
        std::env::set_var("HOME", &home);
        assert_eq!(run(&bare()), 0);
        std::env::remove_var("HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn releases_remote_success_with_fake_git() {
        use std::os::unix::fs::PermissionsExt;
        let _g = env_lock();
        let home = std::env::temp_dir().join(format!(
            "cbeta-rel-remote-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&home);
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("git");
        std::fs::write(
            &script,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "git version 2.40.0"; exit 0; fi
if [ "$1" = "ls-remote" ]; then
  echo "dbdea41071e1e260ad84b72faefd4587333cf76d	refs/tags/2026R2"
  echo "dbdea41071e1e260ad84b72faefd4587333cf76d	refs/tags/2026R2^{}"
  exit 0
fi
exit 1
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        let old_path = std::env::var("PATH").ok();
        std::env::set_var(
            "PATH",
            format!("{}:{}", bin.display(), old_path.as_deref().unwrap_or("")),
        );
        std::env::remove_var("CBETA_GIT_BASE");
        let mut out = bare();
        out.remote = true;
        assert_eq!(run(&out), 0);
        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
