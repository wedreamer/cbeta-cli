//! Git helpers via `std::process::Command` (not git2). Honors `CBETA_GIT_BASE`.

use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::process::Command;

/// Official GitHub host prefix rewritten by `CBETA_GIT_BASE`.
pub const GITHUB_HTTPS_PREFIX: &str = "https://github.com";

/// Human message when `git` is missing from PATH.
pub const MISSING_GIT_MSG: &str = "git not found on PATH; install git";

/// Rewrite an official HTTPS GitHub URL using `CBETA_GIT_BASE` when set.
///
/// Empty or unset `CBETA_GIT_BASE` leaves `official` unchanged. Only the
/// `https://github.com` prefix is replaced; path and `.git` suffix stay intact.
pub fn rewrite_url(official: &str) -> String {
    match env::var("CBETA_GIT_BASE") {
        Ok(base) => {
            let base = base.trim().trim_end_matches('/');
            if base.is_empty() {
                return official.to_string();
            }
            if let Some(rest) = official.strip_prefix(GITHUB_HTTPS_PREFIX) {
                format!("{base}{rest}")
            } else {
                official.to_string()
            }
        }
        Err(_) => official.to_string(),
    }
}

/// True when `git` resolves on PATH.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run `git` with inherited env (`HTTPS_PROXY`, `GIT_CONFIG*`, `CBETA_GIT_BASE`).
///
/// # Errors
/// Returns a human string when spawn fails or git exits non-zero.
pub fn run_git(args: &[&str], cwd: Option<&Path>) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    // Inherit env so HTTPS_PROXY / GIT_CONFIG / CBETA_GIT_BASE reach child git.
    let out = cmd
        .output()
        .map_err(|e| format!("git {}: {e}", args.first().unwrap_or(&"")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        if err.is_empty() {
            return Err(format!(
                "git {} failed (exit {:?})",
                args.join(" "),
                out.status.code()
            ));
        }
        return Err(err.to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `git ls-remote --tags <url>` then [`parse_ls_remote`].
///
/// # Errors
/// Network/git failures surface as strings (caller maps to exit 2 + mirror hint).
pub fn ls_remote_tags(url: &str) -> Result<BTreeMap<String, String>, String> {
    let stdout = run_git(&["ls-remote", "--tags", url], None)?;
    Ok(parse_ls_remote(&stdout))
}

/// Parse `git ls-remote --tags` stdout into tag → preferred SHA.
///
/// Drops `master`, any name containing `xml-p5-2018`, and non-tag refs.
/// When both lightweight and peeled (`^{}`) lines exist, keeps the peeled SHA.
pub fn parse_ls_remote(stdout: &str) -> BTreeMap<String, String> {
    let mut lightweight: BTreeMap<String, String> = BTreeMap::new();
    let mut peeled: BTreeMap<String, String> = BTreeMap::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(sha) = parts.next() else {
            continue;
        };
        let Some(rref) = parts.next() else {
            continue;
        };
        let peeled_ref = rref.ends_with("^{}");
        let bare = rref.trim_end_matches("^{}");
        let Some(tag) = bare.strip_prefix("refs/tags/") else {
            continue;
        };
        if tag == "master" || tag.contains("xml-p5-2018") {
            continue;
        }
        if peeled_ref {
            peeled.insert(tag.to_string(), sha.to_string());
        } else {
            lightweight.insert(tag.to_string(), sha.to_string());
        }
    }

    let mut out = BTreeMap::new();
    for (tag, sha) in lightweight {
        out.insert(tag, sha);
    }
    for (tag, sha) in peeled {
        out.insert(tag, sha);
    }
    out
}

/// `git rev-parse HEAD` in `dir`.
pub fn rev_parse_head(dir: &Path) -> Result<String, String> {
    run_git(&["rev-parse", "HEAD"], Some(dir))
}

/// True when `dir` looks like a usable git work tree (has `.git` + rev-parse).
pub fn is_git_worktree(dir: &Path) -> bool {
    if !dir.join(".git").exists() {
        return false;
    }
    run_git(&["rev-parse", "--git-dir"], Some(dir)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_paths::env_lock;

    #[test]
    fn git_url_rewrites_with_cbeta_git_base() {
        let _g = env_lock();
        let official = "https://github.com/cbeta-org/xml-p5.git";

        std::env::remove_var("CBETA_GIT_BASE");
        assert_eq!(rewrite_url(official), official);

        std::env::set_var("CBETA_GIT_BASE", "");
        assert_eq!(rewrite_url(official), official);

        std::env::set_var("CBETA_GIT_BASE", "https://git.example.com");
        assert_eq!(
            rewrite_url(official),
            "https://git.example.com/cbeta-org/xml-p5.git"
        );

        std::env::set_var("CBETA_GIT_BASE", "https://git.example.com/");
        assert_eq!(
            rewrite_url(official),
            "https://git.example.com/cbeta-org/xml-p5.git"
        );

        std::env::remove_var("CBETA_GIT_BASE");
    }

    #[test]
    fn parse_ls_remote_skips_master_and_xml_p5_2018() {
        let fixture = "\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trefs/heads/master
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\trefs/tags/master
cccccccccccccccccccccccccccccccccccccccc\trefs/tags/xml-p5-2018
dddddddddddddddddddddddddddddddddddddddd\trefs/tags/2026R2
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\trefs/tags/2026R2^{}
ffffffffffffffffffffffffffffffffffffffff\trefs/tags/2026R1
";
        let tags = parse_ls_remote(fixture);
        assert!(!tags.contains_key("master"));
        assert!(!tags.contains_key("xml-p5-2018"));
        assert_eq!(
            tags.get("2026R2").map(String::as_str),
            Some("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
        );
        assert_eq!(
            tags.get("2026R1").map(String::as_str),
            Some("ffffffffffffffffffffffffffffffffffffffff")
        );
        assert_eq!(tags.len(), 2);
    }
}
