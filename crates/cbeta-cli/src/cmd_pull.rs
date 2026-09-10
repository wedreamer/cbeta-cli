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
