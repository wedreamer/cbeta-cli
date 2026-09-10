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
        // Force offline: unreachable CBETA_GIT_BASE (no live GitHub).
        let _g = env_lock();
        std::env::set_var("CBETA_GIT_BASE", "http://127.0.0.1:1");
        let mut out = bare();
        out.remote = true;
        let code = run(&out);
        assert_eq!(code, 2);
        std::env::remove_var("CBETA_GIT_BASE");
    }
}
