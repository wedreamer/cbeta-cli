//! Single-instance lock for `fetch` / `build` (exclusive lockfile + pid).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Human message when another cbeta holds the lifecycle lock.
pub const LOCK_HELD_MSG: &str =
    "另一个 cbeta 进程正在 fetch/build（锁文件被占用）。请等待结束后重试。";

/// Held exclusive lock; released on drop (removes lockfile when we own it).
#[derive(Debug)]
pub struct LifecycleLock {
    path: PathBuf,
    _file: File,
}

impl Drop for LifecycleLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Default lock path: inside the index root (or `$HOME/.cbeta/cbeta.lock`).
///
/// WHY: must not use the parent of `CBETA_INDEX` — CI temps often share `/tmp`
/// as parent, which would serialize every parallel build incorrectly.
pub fn default_lock_path() -> Result<PathBuf, String> {
    if let Ok(index) = std::env::var("CBETA_INDEX") {
        if !index.is_empty() {
            return Ok(PathBuf::from(index).join("cbeta.lock"));
        }
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        "HOME unset; cannot place lifecycle lock (export HOME or CBETA_INDEX)".to_string()
    })?;
    Ok(PathBuf::from(home).join(".cbeta").join("cbeta.lock"))
}

/// Acquire exclusive lifecycle lock, or error if already held.
///
/// # Errors
/// Lock held by another process, or IO failure creating the lockfile.
pub fn try_acquire() -> Result<LifecycleLock, String> {
    let path = default_lock_path()?;
    try_acquire_at(&path)
}

/// Acquire lock at an explicit path (tests + callers that pin the location).
///
/// # Errors
/// Same as [`try_acquire`].
pub fn try_acquire_at(path: &Path) -> Result<LifecycleLock, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!("无权限创建锁目录 {}: {e}", parent.display())
            } else {
                format!("mkdir {}: {e}", parent.display())
            }
        })?;
    }

    match try_create_lock(path) {
        Ok(lock) => Ok(lock),
        Err(CreateLockErr::Exists) => {
            if steal_stale_lock(path) {
                return try_create_lock(path).map_err(|e| match e {
                    CreateLockErr::Exists => {
                        let holder = read_lock_pid(path).unwrap_or_else(|| "?".into());
                        format!("{LOCK_HELD_MSG} (pid={holder}, {})", path.display())
                    }
                    CreateLockErr::Other(s) => s,
                });
            }
            let holder = read_lock_pid(path).unwrap_or_else(|| "?".into());
            Err(format!(
                "{LOCK_HELD_MSG} (pid={holder}, {})",
                path.display()
            ))
        }
        Err(CreateLockErr::Other(s)) => Err(s),
    }
}

enum CreateLockErr {
    Exists,
    Other(String),
}

fn try_create_lock(path: &Path) -> Result<LifecycleLock, CreateLockErr> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut f) => {
            let pid = std::process::id();
            if let Err(e) = f.write_all(format!("{pid}\n").as_bytes()) {
                return Err(CreateLockErr::Other(format!(
                    "write lock {}: {e}",
                    path.display()
                )));
            }
            let _ = f.sync_all();
            Ok(LifecycleLock {
                path: path.to_path_buf(),
                _file: f,
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(CreateLockErr::Exists),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(CreateLockErr::Other(
            format!("无权限写入锁文件 {}: {e}", path.display()),
        )),
        Err(e) => Err(CreateLockErr::Other(format!(
            "create lock {}: {e}",
            path.display()
        ))),
    }
}

fn steal_stale_lock(path: &Path) -> bool {
    let Some(pid_s) = read_lock_pid(path) else {
        let _ = fs::remove_file(path);
        return true;
    };
    let Ok(pid) = pid_s.parse::<u32>() else {
        let _ = fs::remove_file(path);
        return true;
    };
    if pid == std::process::id() {
        return false;
    }
    if process_alive(pid) {
        return false;
    }
    let _ = fs::remove_file(path);
    true
}

fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn read_lock_pid(path: &Path) -> Option<String> {
    let mut s = String::new();
    let mut f = File::open(path).ok()?;
    f.read_to_string(&mut s).ok()?;
    let pid = s.trim();
    if pid.is_empty() {
        None
    } else {
        Some(pid.to_string())
    }
}

/// Ensure `dest`'s parent exists and is writable (disk / permission precheck).
///
/// # Errors
/// Parent missing and uncreatable, or not writable.
pub fn ensure_parent_writable(dest: &Path) -> Result<(), String> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        fs::create_dir_all(parent).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!("无权限创建目录 {}: {e}", parent.display())
            } else {
                format!("无法创建目录 {}: {e}", parent.display())
            }
        })?;
    }
    let probe = parent.join(format!(".cbeta-write-probe-{}", std::process::id()));
    match File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            Err(format!("无权限写入 {}: {e}", parent.display()))
        }
        Err(e) => Err(format!("无法写入 {}: {e}", parent.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_lock_path(prefix: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("cbeta-flock-{prefix}-{}-{n}", std::process::id()))
    }

    #[test]
    fn second_acquire_fails_while_held() {
        let dir = temp_lock_path("held");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cbeta.lock");
        let first = try_acquire_at(&path).expect("first lock");
        let second = try_acquire_at(&path);
        assert!(second.is_err(), "second must fail");
        let err = second.unwrap_err();
        assert!(
            err.contains("锁") || err.contains("占用") || err.contains(LOCK_HELD_MSG),
            "err={err}"
        );
        drop(first);
        let third = try_acquire_at(&path);
        assert!(third.is_ok(), "after drop lock is free: {third:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_parent_writable_ok() {
        let dir = temp_lock_path("disk");
        let _ = fs::remove_dir_all(&dir);
        let dest = dir.join("tag").join("nested");
        ensure_parent_writable(&dest).unwrap();
        assert!(dir.join("tag").is_dir() || dest.parent().map(|p| p.exists()).unwrap_or(false));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn steal_stale_lock_dead_pid() {
        let dir = temp_lock_path("stale");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cbeta.lock");
        fs::write(&path, "1000000\n").unwrap();
        let lock = try_acquire_at(&path).expect("should steal dead pid lock");
        drop(lock);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn steal_empty_and_garbage_pid() {
        let dir = temp_lock_path("garbage");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cbeta.lock");
        fs::write(&path, "").unwrap();
        assert!(try_acquire_at(&path).is_ok());
        drop(try_acquire_at(&path).ok());
        fs::write(&path, "not-a-pid\n").unwrap();
        assert!(try_acquire_at(&path).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_lock_path_uses_cbeta_index() {
        let _g = crate::env_paths::env_lock();
        let dir = temp_lock_path("idx-env");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CBETA_INDEX", &dir);
        let p = default_lock_path().unwrap();
        assert_eq!(p, dir.join("cbeta.lock"));
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_lock_path_falls_back_to_home() {
        let _g = crate::env_paths::env_lock();
        let home = temp_lock_path("home-env");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        std::env::set_var("CBETA_INDEX", "");
        std::env::set_var("HOME", &home);
        let p = default_lock_path().unwrap();
        assert_eq!(p, home.join(".cbeta").join("cbeta.lock"));
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn process_alive_pid1_and_huge() {
        assert!(process_alive(1), "pid 1 should exist on Linux");
        assert!(!process_alive(4_000_000_000), "huge pid must be dead");
    }

    #[test]
    fn try_acquire_uses_default_lock_path() {
        let _g = crate::env_paths::env_lock();
        let dir = temp_lock_path("try-acq");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CBETA_INDEX", &dir);
        let lock = try_acquire().expect("acquire");
        assert!(dir.join("cbeta.lock").is_file());
        drop(lock);
        std::env::remove_var("CBETA_INDEX");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_lock_path_errors_without_home() {
        let _g = crate::env_paths::env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        let err = default_lock_path().unwrap_err();
        assert!(err.contains("HOME"), "err={err}");
    }

    #[test]
    fn steal_refuses_own_pid() {
        let dir = temp_lock_path("own-pid");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cbeta.lock");
        fs::write(&path, format!("{}\n", std::process::id())).unwrap();
        assert!(!steal_stale_lock(&path));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn steal_refuses_live_pid1() {
        let dir = temp_lock_path("live-pid");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cbeta.lock");
        fs::write(&path, "1\n").unwrap();
        assert!(!steal_stale_lock(&path));
        let err = try_acquire_at(&path).unwrap_err();
        assert!(
            err.contains("锁") || err.contains(LOCK_HELD_MSG),
            "err={err}"
        );
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }
}
