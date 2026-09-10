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

fn clone_three(
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
fn check_xml_p5_remote_pin(
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
    use crate::lifecycle::lock::{newest_lock_tag, resolve_release};

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
}
