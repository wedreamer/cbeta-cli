//! Shared NoIndex stderr copy (fetch hint + incomplete-build message).

use std::path::Path;

use crate::build_progress::has_incomplete_build;
use crate::lifecycle::paths::FETCH_HINT;

/// Message when search/get/verify cannot open an index.
///
/// Always includes the fetch hint; adds incomplete-build copy when a
/// `{artifact}.tmp/PROGRESS.json` exists under the index root.
pub fn no_index_message(index_path: &str) -> String {
    let mut msg = format!("no index found under {index_path}; run: {FETCH_HINT}");
    let root = Path::new(index_path);
    // SearchError::NoIndex may pass the root or a nested path; check both.
    let check = if root.join("CURRENT").exists() || root.is_dir() {
        root
    } else {
        root.parent().unwrap_or(root)
    };
    if has_incomplete_build(check) {
        msg.push('\n');
        msg.push_str("上次建索引没完，跑 cbeta build");
    }
    msg
}

/// Print [`no_index_message`] to stderr.
pub fn eprint_no_index(index_path: &str) {
    eprintln!("{}", no_index_message(index_path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn no_index_message_always_has_fetch_hint() {
        let msg = no_index_message("/tmp/no-such-cbeta-index");
        assert!(msg.contains(FETCH_HINT), "msg={msg}");
    }

    #[test]
    fn no_index_message_mentions_incomplete_build() {
        let base = std::env::temp_dir().join(format!(
            "cbeta-hint-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        let tmp = base.join("2026R2+deadbeef.tmp");
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("PROGRESS.json"), r#"{"done":[]}"#).unwrap();
        let msg = no_index_message(base.to_str().unwrap());
        assert!(msg.contains("上次建索引没完"), "msg={msg}");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn eprint_no_index_runs() {
        eprint_no_index("/tmp/cbeta-eprint-no-index");
    }
}
