//! Clone/update one corpus repo at a pinned commit SHA (not `master`).

use std::fs;
use std::path::Path;

use super::git::{is_git_worktree, rev_parse_head, run_git};

/// Sparse paths for xml-p5 (Category B Y/TX/LC/YP excluded).
pub const XML_P5_SPARSE: &[&str] = &["/README.md", "/canons.json", "/schema", "/T", "/X"];

/// Ensure `dir` is checked out at exactly `sha` from `url`.
///
/// Resume: valid `.git` → fetch + detach checkout. Rotten/half clone → discard
/// and reclone. After success, `HEAD` must equal `sha`.
///
/// # Errors
/// Git failures or HEAD ≠ pin.
pub fn clone_at_sha(dir: &Path, url: &str, sha: &str, sparse: bool) -> Result<(), String> {
    if dir.exists() {
        if is_git_worktree(dir) {
            match update_existing(dir, sha, sparse) {
                Ok(()) => return verify_head(dir, sha),
                Err(e) => {
                    eprintln!(
                        "resume failed for {} ({e}); discarding and recloning",
                        dir.display()
                    );
                    fs::remove_dir_all(dir)
                        .map_err(|err| format!("remove {}: {err}", dir.display()))?;
                }
            }
        } else {
            // Half-clone / non-repo debris is not a release.
            fs::remove_dir_all(dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
        }
    }

    fresh_clone(dir, url, sha, sparse)?;
    verify_head(dir, sha)
}

fn update_existing(dir: &Path, sha: &str, sparse: bool) -> Result<(), String> {
    if sparse {
        let _ = run_git(&["sparse-checkout", "init", "--no-cone"], Some(dir));
        let mut args = vec!["sparse-checkout", "set"];
        args.extend_from_slice(XML_P5_SPARSE);
        let _ = run_git(&args, Some(dir));
    }
    run_git(&["fetch", "--depth", "1", "origin", sha], Some(dir))?;
    run_git(&["checkout", "--detach", sha], Some(dir))?;
    Ok(())
}

fn fresh_clone(dir: &Path, url: &str, sha: &str, sparse: bool) -> Result<(), String> {
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    run_git(&["init"], Some(dir))?;
    // Replace origin if re-init edge cases left one.
    let _ = run_git(&["remote", "remove", "origin"], Some(dir));
    run_git(&["remote", "add", "origin", url], Some(dir))?;
    if sparse {
        run_git(&["sparse-checkout", "init", "--no-cone"], Some(dir))?;
        let mut args = vec!["sparse-checkout", "set"];
        args.extend_from_slice(XML_P5_SPARSE);
        run_git(&args, Some(dir))?;
    }
    run_git(&["fetch", "--depth", "1", "origin", sha], Some(dir))?;
    run_git(&["checkout", "--detach", "FETCH_HEAD"], Some(dir))?;
    Ok(())
}

/// Fail when `HEAD` is not exactly the lock pin (half-clone is not a release).
pub fn verify_head(dir: &Path, pin: &str) -> Result<(), String> {
    let head = rev_parse_head(dir)?;
    if head != pin {
        return Err(format!(
            "HEAD {head} != lock pin {pin} under {}",
            dir.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn git_ok(args: &[&str], cwd: &Path) {
        let st = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?}");
    }

    fn make_bare_with_commit(root: &Path) -> (PathBuf, String) {
        let bare = root.join("bare.git");
        let work = root.join("seed");
        fs::create_dir_all(&work).unwrap();
        git_ok(&["init"], &work);
        fs::write(work.join("README.md"), "hi\n").unwrap();
        git_ok(&["add", "README.md"], &work);
        git_ok(&["commit", "-m", "init"], &work);
        let sha = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&work)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        // bare clone for file:// URL
        let st = Command::new("git")
            .args([
                "clone",
                "--bare",
                work.to_str().unwrap(),
                bare.to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(st.success());
        (bare, sha)
    }

    #[test]
    fn fetch_sha_mismatch_refuses_release() {
        let root = temp_dir("cbeta-sha-mismatch");
        let (bare, good_sha) = make_bare_with_commit(&root);
        let url = format!("file://{}", bare.display());
        let dest = root.join("checkout");
        clone_at_sha(&dest, &url, &good_sha, false).unwrap();

        // Wrong pin must fail verify (and thus refuse treating as a release).
        let bad = "0000000000000000000000000000000000000000";
        let err = verify_head(&dest, bad).unwrap_err();
        assert!(err.contains("!="), "err={err}");
        assert!(err.contains(&good_sha) || err.contains(bad), "err={err}");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clone_at_sha_checks_out_pin() {
        let root = temp_dir("cbeta-clone-ok");
        let (bare, sha) = make_bare_with_commit(&root);
        let url = format!("file://{}", bare.display());
        let dest = root.join("xml");
        clone_at_sha(&dest, &url, &sha, false).unwrap();
        assert_eq!(rev_parse_head(&dest).unwrap(), sha);
        // resume path
        clone_at_sha(&dest, &url, &sha, false).unwrap();
        let _ = fs::remove_dir_all(&root);
    }
}
