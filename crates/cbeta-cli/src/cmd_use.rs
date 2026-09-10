//! `cbeta use` / `cbeta current` — switch corpus CURRENT and ensure index.

use std::env;
use std::fs;

use cbeta_index::{artifact_dir_name, tmp_dir_for};

use crate::build_progress::progress_path;
use crate::cli_resolve::CliOut;
use crate::cmd_build::build_scope_with_full;
use crate::env_paths::{corpus_root, index_root};
use crate::lifecycle::fetched::{is_complete, read_fetched};
use crate::lifecycle::lock::{load_lock, resolve_release};
use crate::lifecycle::paths::{
    current_tag, tag_dir, write_corpus_current, write_corpus_default, FETCH_HINT,
};
use crate::session::last_json_path;

/// Print CURRENT tag, or exit 2 with the fetch hint.
pub fn run_current(_out: &CliOut) -> i32 {
    match current_tag() {
        Ok(tag) => {
            println!("{tag}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            if !e.contains(FETCH_HINT) {
                eprintln!("run: {FETCH_HINT}");
            }
            2
        }
    }
}

/// Switch corpus CURRENT to a concrete lock tag; build index when needed.
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
    let raw = out
        .default_tag
        .as_deref()
        .or(out.q.as_deref())
        .ok_or_else(|| "use requires a tag or --default <tag>".to_string())?;

    let lock = load_lock()?;
    let tag = resolve_release(raw, &lock, None)?;

    let scope = resolve_use_scope(out.scope.as_deref())?;

    // Completeness: CBETA_CORPUS override (mini) → treat complete; else require FETCHED.
    let corpus_override = env::var_os("CBETA_CORPUS").filter(|v| !v.is_empty());
    if corpus_override.is_none() {
        let pins = lock
            .releases
            .get(&tag)
            .ok_or_else(|| format!("unknown release {tag}; see: cbeta releases"))?;
        match read_fetched(&tag)? {
            None => {
                return Err(format!(
                    "corpus {tag} incomplete (no FETCHED.yaml); run: {FETCH_HINT}"
                ));
            }
            Some(_) if !is_complete(&tag, pins) => {
                return Err(format!("corpus {tag} incomplete; run: {FETCH_HINT}"));
            }
            Some(_) => {}
        }
    }

    let prev_tag = current_tag().ok();

    // Build (if needed) BEFORE writing CURRENT so a failed build keeps the previous tag.
    let need_full = matches!(prev_tag.as_deref(), Some(p) if p != tag.as_str());
    if index_needs_build(&tag, &scope)? {
        let set_corpus = if corpus_override.is_none() {
            let dir = tag_dir(&tag)?;
            if dir.is_dir() {
                env::set_var("CBETA_CORPUS", &dir);
                true
            } else {
                false
            }
        } else {
            false
        };
        let result = build_scope_with_full(&scope, need_full);
        if set_corpus {
            env::remove_var("CBETA_CORPUS");
        }
        result?;
    }

    write_corpus_current(&tag)?;
    if out.default_tag.is_some() {
        write_corpus_default(&tag)?;
    }

    if let Ok(path) = last_json_path() {
        if path.is_file() {
            let _ = fs::remove_file(&path);
        }
    }

    Ok(())
}

fn resolve_use_scope(explicit: Option<&str>) -> Result<String, String> {
    if let Some(s) = explicit {
        if !s.is_empty() {
            return Ok(s.to_string());
        }
    }
    // Mini / CBETA_CORPUS override: prefer ci-minimal when that scope exists.
    if let Ok(corpus) = corpus_root() {
        let ci = corpus.join("scopes").join("ci-minimal");
        if ci.join("MANIFEST.json").is_file() {
            return Ok("ci-minimal".into());
        }
    }
    Ok("taisho".into())
}

fn index_needs_build(tag: &str, scope: &str) -> Result<bool, String> {
    // Need MANIFEST scope_hash to know artifact name — read from corpus scope.
    let corpus = match corpus_root() {
        Ok(c) => c,
        Err(_) => return Ok(true),
    };
    let man_path = corpus.join("scopes").join(scope).join("MANIFEST.json");
    if !man_path.is_file() {
        return Ok(true);
    }
    let man: crate::scope_io::ScopeManifest = crate::scope_io::read_json(&man_path)?;
    if man.cbeta_tag != tag {
        // Scope MANIFEST tag differs; still try build (may fail).
    }
    let root = index_root()?;
    let art_name = artifact_dir_name(&man.cbeta_tag, &man.scope_hash).map_err(|e| e.to_string())?;
    let intended = root.join(&art_name);
    let tmp = tmp_dir_for(&intended);
    if progress_path(&tmp).is_file() {
        return Ok(true);
    }
    if !intended.is_dir() {
        return Ok(true);
    }
    // Live artifact present and no incomplete tmp.
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Filters};

    fn bare_out() -> CliOut {
        CliOut {
            action: Action::Use,
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
            "cbeta-use-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join(".cbeta").join("corpus")).unwrap();
        p
    }

    #[test]
    fn use_latest_pins_concrete_tag() {
        let _g = env_lock();
        let home = temp_home("latest");
        let mini = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini");
        env::set_var("HOME", &home);
        env::set_var("CBETA_INDEX", home.join("idx"));
        env::set_var("CBETA_CORPUS", &mini);

        let mut out = bare_out();
        out.q = Some("latest".into());
        out.scope = Some("ci-minimal".into());
        let code = run(&out);
        assert_eq!(code, 0, "use latest should build mini then switch");
        let cur = fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
        assert_eq!(cur.trim(), "2026R2");
        assert!(!cur.contains("latest"));

        env::remove_var("HOME");
        env::remove_var("CBETA_INDEX");
        env::remove_var("CBETA_CORPUS");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn use_does_not_switch_when_fetched_incomplete() {
        let _g = env_lock();
        let home = temp_home("incomplete");
        env::remove_var("CBETA_CORPUS");
        env::set_var("HOME", &home);
        fs::create_dir_all(home.join(".cbeta/corpus/2026R2")).unwrap();

        let mut out = bare_out();
        out.q = Some("2026R2".into());
        assert_eq!(run(&out), 2);
        assert!(!home.join(".cbeta/corpus/CURRENT").exists());

        env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_current_success_and_missing() {
        let _g = env_lock();
        let home = temp_home("current");
        env::set_var("HOME", &home);
        env::remove_var("CBETA_CORPUS");
        assert_eq!(run_current(&bare_out()), 2);
        fs::write(home.join(".cbeta/corpus/CURRENT"), "2026R2\n").unwrap();
        assert_eq!(run_current(&bare_out()), 0);
        env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn use_requires_tag() {
        let out = bare_out();
        assert_eq!(run(&out), 2);
    }

    #[test]
    fn resolve_use_scope_taisho_and_ci_minimal() {
        let _g = env_lock();
        assert_eq!(resolve_use_scope(Some("taisho")).unwrap(), "taisho");
        let mini = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini");
        env::set_var("CBETA_CORPUS", &mini);
        assert_eq!(resolve_use_scope(None).unwrap(), "ci-minimal");
        env::remove_var("CBETA_CORPUS");
    }

    #[test]
    fn index_needs_build_without_manifest() {
        let _g = env_lock();
        let home = temp_home("need-build");
        env::set_var("HOME", &home);
        env::set_var("CBETA_INDEX", home.join("idx"));
        env::set_var("CBETA_CORPUS", home.join("empty-corpus"));
        fs::create_dir_all(home.join("empty-corpus")).unwrap();
        assert!(index_needs_build("2026R2", "ci-minimal").unwrap());
        env::remove_var("HOME");
        env::remove_var("CBETA_INDEX");
        env::remove_var("CBETA_CORPUS");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn use_default_writes_default_and_current() {
        let _g = env_lock();
        let home = temp_home("default");
        let mini = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini");
        env::set_var("HOME", &home);
        env::set_var("CBETA_INDEX", home.join("idx"));
        env::set_var("CBETA_CORPUS", &mini);

        let mut out = bare_out();
        out.default_tag = Some("2026R2".into());
        out.scope = Some("ci-minimal".into());
        assert_eq!(run(&out), 0);
        let cur = fs::read_to_string(home.join(".cbeta/corpus/CURRENT")).unwrap();
        assert_eq!(cur.trim(), "2026R2");
        let def = fs::read_to_string(home.join(".cbeta/corpus/DEFAULT")).unwrap();
        assert_eq!(def.trim(), "2026R2");

        env::remove_var("HOME");
        env::remove_var("CBETA_INDEX");
        env::remove_var("CBETA_CORPUS");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn use_incomplete_when_fetched_pins_mismatch() {
        let _g = env_lock();
        let home = temp_home("pin-bad");
        env::remove_var("CBETA_CORPUS");
        env::set_var("HOME", &home);
        let lock = load_lock().unwrap();
        let mut pins = lock.releases.get("2026R2").unwrap().clone();
        pins.xml_p5 = "a".repeat(40);
        let dest = home.join(".cbeta/corpus/2026R2");
        crate::lifecycle::fetched::write_fetched("2026R2", &dest, &pins).unwrap();
        let mut out = bare_out();
        out.q = Some("2026R2".into());
        assert_eq!(run(&out), 2);
        assert!(!home.join(".cbeta/corpus/CURRENT").exists());
        env::remove_var("HOME");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn index_needs_build_false_after_real_build() {
        let _g = env_lock();
        let home = temp_home("built");
        let mini = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini");
        env::set_var("HOME", &home);
        env::set_var("CBETA_INDEX", home.join("idx"));
        env::set_var("CBETA_CORPUS", &mini);
        build_scope_with_full("ci-minimal", true).unwrap();
        assert!(!index_needs_build("2026R2", "ci-minimal").unwrap());
        env::remove_var("HOME");
        env::remove_var("CBETA_INDEX");
        env::remove_var("CBETA_CORPUS");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn resolve_use_scope_empty_explicit_falls_through() {
        let _g = env_lock();
        env::remove_var("CBETA_CORPUS");
        assert_eq!(resolve_use_scope(Some("")).unwrap(), "taisho");
        assert_eq!(resolve_use_scope(None).unwrap(), "taisho");
    }
}
