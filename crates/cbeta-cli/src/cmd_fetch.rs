//! `cbeta fetch` — clone three pinned repos; write FETCHED.yaml; never CURRENT.

use std::path::Path;

use crate::cli_resolve::CliOut;
use crate::lifecycle::clone::clone_at_sha;
use crate::lifecycle::fetched::{is_complete, write_fetched};
use crate::lifecycle::flock::{self, ensure_parent_writable};
use crate::lifecycle::git::{git_available, ls_remote_tags, rewrite_url, MISSING_GIT_MSG};
use crate::lifecycle::lock::{load_lock, resolve_release, ReleaseLock, ReleasePins};
use crate::lifecycle::paths::tag_dir;

/// CC BY-NC-SA notice printed on every successful fetch (stderr).
pub const LICENSE_NOTICE: &str =
    "NOTICE: CBETA 经文采用 CC BY-NC-SA 4.0，限非营利使用。详见 https://cbeta.org/copyright";

/// Disk size hint before clone (human; sparse T,X only).
pub const DISK_HINT: &str =
    "disk estimate: sparse xml-p5 (T,X) + metadata + gaiji — several GB possible";

/// Fetch a concrete lock release into `$HOME/.cbeta/corpus/<tag>/`.
///
/// Exit: 0 success, 2 usage/git/sha/unknown. Never writes `CURRENT`.
pub fn run(out: &CliOut) -> i32 {
    let release_spec = match out.release.as_deref().or(out.q.as_deref()) {
        Some(r) => r,
        None => {
            eprintln!("fetch requires --release <tag|latest>");
            return 2;
        }
    };

    if !git_available() {
        eprintln!("{MISSING_GIT_MSG}");
        return 2;
    }

    let _lock_guard = match flock::try_acquire() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let lock = match load_lock() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let remote_for_latest = if release_spec == "latest" {
        probe_remote_tag_names(&lock)
    } else {
        None
    };

    let tag = match resolve_release(release_spec, &lock, remote_for_latest.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let pins = match lock.releases.get(&tag) {
        Some(p) => p.clone(),
        None => {
            eprintln!("unknown release {tag}; see: cbeta releases");
            return 2;
        }
    };

    let dest = match tag_dir(&tag) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    if is_complete(&tag, &pins) {
        eprintln!("{LICENSE_NOTICE}");
        eprintln!("already fetched {tag} at {}", dest.display());
        return 0;
    }

    eprintln!("{DISK_HINT}");
    if let Err(e) = ensure_parent_writable(&dest) {
        eprintln!("{e}");
        eprintln!("{DISK_HINT}");
        return 2;
    }

    if let Err(e) = clone_three(&dest, &lock, &tag, &pins) {
        eprintln!("{e}");
        return 2;
    }

    if let Err(e) = check_xml_p5_remote_pin(&lock, &tag, &pins) {
        eprintln!("{e}");
        return 2;
    }

    if let Err(e) = write_fetched(&tag, &dest, &pins) {
        eprintln!("{e}");
        return 2;
    }

    eprintln!("{LICENSE_NOTICE}");
    eprintln!("fetched {tag} → {}", dest.display());
    0
}

fn probe_remote_tag_names(lock: &ReleaseLock) -> Option<Vec<String>> {
    let url = rewrite_url(&lock.official.xml_p5);
    match ls_remote_tags(&url) {
        Ok(map) => Some(map.into_keys().collect()),
        Err(_) => None,
    }
}

pub(crate) fn clone_three(
    dest: &Path,
    lock: &ReleaseLock,
    tag: &str,
    pins: &ReleasePins,
) -> Result<(), String> {
    let src = dest.join("src");
    let xml_url = rewrite_url(&lock.official.xml_p5);
    let meta_url = rewrite_url(&lock.official.metadata);
    let gaiji_url = rewrite_url(&lock.official.gaiji);

    eprintln!(
        "==> clone xml-p5 @ {}",
        &pins.xml_p5[..7.min(pins.xml_p5.len())]
    );
    clone_at_sha(&src.join("xml-p5"), &xml_url, &pins.xml_p5, true)?;

    eprintln!(
        "==> clone metadata @ {}",
        &pins.metadata[..7.min(pins.metadata.len())]
    );
    clone_at_sha(&src.join("metadata"), &meta_url, &pins.metadata, false)?;

    eprintln!(
        "==> clone gaiji @ {}",
        &pins.gaiji[..7.min(pins.gaiji.len())]
    );
    clone_at_sha(&src.join("gaiji"), &gaiji_url, &pins.gaiji, false)?;

    // tag used only for messaging context in callers
    let _ = tag;
    Ok(())
}

/// When ls-remote succeeds and the tag exists remotely, lock pin must match peeled SHA.
pub(crate) fn check_xml_p5_remote_pin(
    lock: &ReleaseLock,
    tag: &str,
    pins: &ReleasePins,
) -> Result<(), String> {
    let url = rewrite_url(&lock.official.xml_p5);
    let remote = match ls_remote_tags(&url) {
        Ok(m) => m,
        Err(_) => return Ok(()), // offline: skip remote cross-check
    };
    if let Some(remote_sha) = remote.get(tag) {
        if remote_sha != &pins.xml_p5 {
            return Err(format!(
                "xml-p5 pin mismatch for {tag}: lock={} remote={remote_sha}; refuse release",
                pins.xml_p5
            ));
        }
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
    use crate::lifecycle::lock::{newest_lock_tag, resolve_release};

    fn bare() -> CliOut {
        CliOut {
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
            "cbeta-fetch-{prefix}-{}-{}",
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

    fn install_fake_git(bin_dir: &Path, wrong_sha: bool) {
        fs::create_dir_all(bin_dir).unwrap();
        let script = bin_dir.join("git");
        let sha = if wrong_sha {
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        } else {
            "dbdea41071e1e260ad84b72faefd4587333cf76d"
        };
        let body = format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "git version 2.40.0"; exit 0; fi
if [ "$1" = "ls-remote" ]; then
  echo "{sha}	refs/tags/2026R2"
  echo "{sha}	refs/tags/2026R2^{{}}"
  echo "2b8ab8d5e4fe957a9b94f2cde01cb0d2e2dcd2b9	refs/tags/2026R1"
  exit 0
fi
exit 1
"#
        );
        fs::write(&script, body).unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
    }

    fn prepend_path(bin_dir: &Path) -> String {
        let old = std::env::var("PATH").unwrap_or_default();
        format!("{}:{old}", bin_dir.display())
    }

    #[test]
    fn fetch_latest_resolves_concrete_lock_tag() {
        let lock = load_lock().expect("lock");
        let t = resolve_release("latest", &lock, None).unwrap();
        assert_eq!(t, "2026R2");
        assert_eq!(newest_lock_tag(&lock), Some("2026R2"));
        assert_ne!(t, "latest");
    }

    #[test]
    fn fetch_latest_prefers_newest_remote_in_lock() {
        let lock = load_lock().expect("lock");
        let remote = vec!["2025R3".into(), "2026R1".into()];
        let t = resolve_release("latest", &lock, Some(&remote)).unwrap();
        assert_eq!(t, "2026R1");
    }

    #[test]
    fn fetch_missing_release_exits_2() {
        let out = bare();
        assert_eq!(run(&out), 2);
    }

    #[test]
    fn fetch_skip_complete_does_not_write_current() {
        let _g = env_lock();
        let home = temp_home("skip");
        let idx = home.join("idx");
        fs::create_dir_all(&idx).unwrap();
        let lock = load_lock().unwrap();
        let pins = lock.releases.get("2026R2").unwrap();
        let dest = home.join(".cbeta/corpus/2026R2");
        write_fetched("2026R2", &dest, pins).unwrap();

        let bin = home.join("bin");
        install_fake_git(&bin, false);
        let old_path = std::env::var("PATH").ok();
        std::env::set_var("HOME", &home);
        std::env::set_var("CBETA_INDEX", &idx);
        std::env::set_var("PATH", prepend_path(&bin));
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_GIT_BASE");

        let mut out = bare();
        out.release = Some("2026R2".into());
        assert_eq!(run(&out), 0);
        assert!(
            !home.join(".cbeta/corpus/CURRENT").exists(),
            "fetch must never write CURRENT"
        );

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("HOME");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn check_xml_p5_remote_pin_mismatch() {
        let _g = env_lock();
        let home = temp_home("pin-mm");
        let bin = home.join("bin");
        install_fake_git(&bin, true);
        let old_path = std::env::var("PATH").ok();
        std::env::set_var("PATH", prepend_path(&bin));
        std::env::remove_var("CBETA_GIT_BASE");

        let lock = load_lock().unwrap();
        let pins = lock.releases.get("2026R2").unwrap().clone();
        let err = check_xml_p5_remote_pin(&lock, "2026R2", &pins).unwrap_err();
        assert!(err.contains("pin mismatch"), "err={err}");

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn check_xml_p5_remote_pin_ok_when_match() {
        let _g = env_lock();
        let home = temp_home("pin-ok");
        let bin = home.join("bin");
        install_fake_git(&bin, false);
        let old_path = std::env::var("PATH").ok();
        std::env::set_var("PATH", prepend_path(&bin));
        std::env::remove_var("CBETA_GIT_BASE");

        let lock = load_lock().unwrap();
        let pins = lock.releases.get("2026R2").unwrap().clone();
        check_xml_p5_remote_pin(&lock, "2026R2", &pins).unwrap();

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn probe_remote_tag_names_offline_returns_none() {
        let _g = env_lock();
        std::env::set_var("CBETA_GIT_BASE", "http://127.0.0.1:1");
        let lock = load_lock().unwrap();
        assert!(probe_remote_tag_names(&lock).is_none());
        std::env::remove_var("CBETA_GIT_BASE");
    }

    #[test]
    fn fetch_unknown_release_exits_2() {
        let _g = env_lock();
        let home = temp_home("unk");
        let idx = home.join("idx");
        fs::create_dir_all(&idx).unwrap();
        let bin = home.join("bin");
        install_fake_git(&bin, false);
        let old_path = std::env::var("PATH").ok();
        std::env::set_var("HOME", &home);
        std::env::set_var("CBETA_INDEX", &idx);
        std::env::set_var("PATH", prepend_path(&bin));

        let mut out = bare();
        out.release = Some("2099R9".into());
        assert_eq!(run(&out), 2);

        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("HOME");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&home);
    }

    fn make_bare(root: &Path, name: &str) -> (PathBuf, String) {
        use std::process::Command;
        let work = root.join(format!("{name}-seed"));
        let bare = root.join(format!("{name}.git"));
        fs::create_dir_all(&work).unwrap();
        let git = |args: &[&str], cwd: &Path| {
            assert!(Command::new("git")
                .args(args)
                .current_dir(cwd)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .status()
                .unwrap()
                .success());
        };
        git(&["init"], &work);
        fs::write(work.join("README.md"), format!("{name}\n")).unwrap();
        git(&["add", "README.md"], &work);
        git(&["commit", "-m", "init"], &work);
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
        assert!(Command::new("git")
            .args([
                "clone",
                "--bare",
                work.to_str().unwrap(),
                bare.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success());
        (bare, sha)
    }

    #[test]
    fn clone_three_local_file_urls_ok() {
        let root = temp_home("clone3");
        let (xml_bare, xml_sha) = make_bare(&root, "xml");
        let (meta_bare, meta_sha) = make_bare(&root, "meta");
        let (gaiji_bare, gaiji_sha) = make_bare(&root, "gaiji");
        let lock = ReleaseLock {
            schema: "cbeta-cli.releases/v1".into(),
            official: crate::lifecycle::lock::OfficialRepos {
                xml_p5: format!("file://{}", xml_bare.display()),
                metadata: format!("file://{}", meta_bare.display()),
                gaiji: format!("file://{}", gaiji_bare.display()),
            },
            releases: std::collections::BTreeMap::from([(
                "2026R2".into(),
                ReleasePins {
                    xml_p5: xml_sha.clone(),
                    metadata: meta_sha.clone(),
                    gaiji: gaiji_sha.clone(),
                },
            )]),
        };
        let pins = lock.releases.get("2026R2").unwrap().clone();
        let dest = root.join("tag");
        clone_three(&dest, &lock, "2026R2", &pins).unwrap();
        assert!(dest.join("src/xml-p5").is_dir());
        assert!(dest.join("src/metadata").is_dir());
        assert!(dest.join("src/gaiji").is_dir());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fetch_run_fails_when_lock_held() {
        let _g = env_lock();
        let home = temp_home("flock");
        let idx = home.join("idx");
        fs::create_dir_all(&idx).unwrap();
        let bin = home.join("bin");
        install_fake_git(&bin, false);
        let old_path = std::env::var("PATH").ok();
        std::env::set_var("HOME", &home);
        std::env::set_var("CBETA_INDEX", &idx);
        std::env::set_var("PATH", prepend_path(&bin));
        let _held = flock::try_acquire().unwrap();
        let mut out = bare();
        out.release = Some("2026R2".into());
        assert_eq!(run(&out), 2);
        drop(_held);
        if let Some(p) = old_path {
            std::env::set_var("PATH", p);
        }
        std::env::remove_var("HOME");
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&home);
    }
}
