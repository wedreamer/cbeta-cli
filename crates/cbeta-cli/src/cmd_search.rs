//! `cbeta search`: keyword/phrase hits (TTY + JSON).

use std::io::{stdout, IsTerminal};

use cbeta_core::{Command, Format, Hit};
use cbeta_search::{search, Error as SearchError, DEFAULT_LIMIT};

use crate::env_paths::index_root;

/// Run search; exit 0 hits / 1 no-hit / 2 no-index or usage.
///
/// `script`: `Some("s")` converts title/author/text for display only; never mutates `line_id`.
/// `save`: `Some("last")` writes RAW hits to `last.json` before display (empty hits still save).
pub fn run(cmd: &Command, script: Option<&str>, save: Option<&str>) -> i32 {
    let Some(parsed) = cmd.parsed_query.as_ref() else {
        eprintln!("search requires a query");
        return 2;
    };

    let root = match index_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let hits = match search(&root, parsed, &cmd.filters, DEFAULT_LIMIT) {
        Ok(h) => h,
        Err(SearchError::NoIndex(p)) => {
            crate::index_hint::eprint_no_index(&p);
            return 2;
        }
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    // WHY: persist RAW engine hits (not display clones) so --from last stays index-faithful.
    if save == Some("last") {
        let (artifact_id, cbeta_tag) = match crate::cmd_catalog::load_info() {
            Ok(info) => (info.artifact_id, info.cbeta_tag),
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        };
        let session = crate::session::SavedSearch {
            query: cmd.q.clone().unwrap_or_default(),
            filters: cmd.filters.clone(),
            hits: hits.clone(),
            artifact_id,
            cbeta_tag,
        };
        if let Err(e) = crate::session::save_last(&session) {
            eprintln!("{e}");
            return 2;
        }
    }

    let display: Vec<Hit> = hits.iter().map(|h| display_hit(h, script)).collect();

    match cmd.format {
        Format::Json => {
            #[allow(clippy::expect_used)]
            {
                let body = serde_json::json!({ "hits": display });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).expect("hits json")
                );
            }
        }
        Format::Tty | Format::Plain | Format::Jsonl => {
            let color = want_color(cmd.format == Format::Plain);
            print_human(&display, &parsed.terms, color, script);
        }
    }

    if hits.is_empty() {
        1
    } else {
        0
    }
}

/// Clone a hit for display; t2s title/author/text when `--script s`. `line_id` untouched.
fn display_hit(hit: &Hit, script: Option<&str>) -> Hit {
    if script != Some("s") {
        return hit.clone();
    }
    Hit {
        line_id: hit.line_id.clone(),
        work_id: hit.work_id.clone(),
        title: cbeta_parse::t2s(&hit.title),
        author: cbeta_parse::t2s(&hit.author),
        juan: hit.juan,
        text_raw: cbeta_parse::t2s(&hit.text_raw),
        citation: hit.citation.clone(),
        score: hit.score,
        cbeta_tag: hit.cbeta_tag.clone(),
    }
}

fn want_color(plain: bool) -> bool {
    if plain {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    stdout().is_terminal()
}

fn print_human(hits: &[Hit], terms: &[String], color: bool, script: Option<&str>) {
    for (i, h) in hits.iter().enumerate() {
        let snip = highlight(&h.text_raw, terms, color, script);
        println!(
            "{:>3}  {}  {}  {}  j{}  {}",
            i + 1,
            h.line_id,
            h.title,
            h.author,
            h.juan,
            snip
        );
    }
}

fn highlight(text: &str, terms: &[String], color: bool, script: Option<&str>) -> String {
    if !color || terms.is_empty() {
        return text.to_string();
    }
    // Display may be 简体; user terms may be either — try both scripts so highlight still hits.
    for t in terms {
        if t.is_empty() {
            continue;
        }
        let trad = cbeta_parse::s2t(t);
        let simp = if script == Some("s") {
            cbeta_parse::t2s(t)
        } else {
            t.clone()
        };
        for needle in [t.as_str(), trad.as_str(), simp.as_str()] {
            if let Some(pos) = text.find(needle) {
                let end = pos + needle.len();
                return format!(
                    "{}\x1b[1;31m{}\x1b[0m{}",
                    &text[..pos],
                    &text[pos..end],
                    &text[end..]
                );
            }
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Filters, Format, Hit};

    fn mini_corpus() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-cmd-search-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn with_index<F: FnOnce() -> R, R>(f: F) -> R {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("idx");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        std::env::set_var("NO_COLOR", "1");
        let build_cmd = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(crate::cmd_build::run(&build_cmd, false), 0);
        let out = f();
        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("NO_COLOR");
        let _ = std::fs::remove_dir_all(&index);
        out
    }

    fn sample_hit() -> Hit {
        Hit {
            line_id: "T30n1578_p0268b21".into(),
            work_id: "T1578".into(),
            title: "大乘掌珍論".into(),
            author: "玄奘".into(),
            juan: 1,
            text_raw: "真性有為空".into(),
            citation: "c".into(),
            score: 1.0,
            cbeta_tag: "2026R2".into(),
        }
    }

    #[test]
    fn run_requires_parsed_query() {
        let cmd = Command {
            action: Action::Search,
            q: Some("x".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd, None, None), 2);
    }

    #[test]
    fn run_index_root_error_exits_2() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        let pq = cbeta_core::parse_query("空").unwrap();
        let cmd = Command {
            action: Action::Search,
            q: Some("空".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: Some(pq),
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd, None, None), 2);
    }

    #[test]
    fn run_no_index_exits_2() {
        let _g = env_lock();
        let index = temp_dir("empty");
        std::env::set_var("CBETA_INDEX", &index);
        let pq = cbeta_core::parse_query("空").unwrap();
        let cmd = Command {
            action: Action::Search,
            q: Some("空".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: Some(pq),
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd, None, None), 2);
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }

    #[test]
    fn run_json_and_plain_hit_paths() {
        with_index(|| {
            let pq = cbeta_core::parse_query("真性有为空").unwrap();
            let mut cmd = Command {
                action: Action::Search,
                q: Some("真性有为空".into()),
                filters: Filters::default(),
                format: Format::Json,
                explain: false,
                parsed_query: Some(pq.clone()),
                context: None,
                copy: false,
            };
            assert_eq!(run(&cmd, None, None), 0);
            cmd.format = Format::Plain;
            assert_eq!(run(&cmd, None, None), 0);
            cmd.format = Format::Tty;
            assert_eq!(run(&cmd, None, None), 0);
            cmd.format = Format::Jsonl;
            assert_eq!(run(&cmd, None, None), 0);
            assert_eq!(run(&cmd, Some("s"), None), 0);
            let miss = cbeta_core::parse_query("完全不存在的词xyz").unwrap();
            cmd.parsed_query = Some(miss);
            cmd.format = Format::Json;
            assert_eq!(run(&cmd, None, None), 1);
        });
    }

    #[test]
    fn want_color_respects_plain_and_no_color() {
        let _g = env_lock();
        assert!(!want_color(true));
        std::env::set_var("NO_COLOR", "1");
        assert!(!want_color(false));
        std::env::remove_var("NO_COLOR");
        let _ = want_color(false);
    }

    #[test]
    fn highlight_color_s2t_empty_term_and_no_match() {
        let text = "真性有為空";
        assert_eq!(highlight(text, &[], true, None), text);
        assert_eq!(highlight(text, &["空".into()], false, None), text);
        let terms = vec![String::new(), "有为空".into()];
        let h = highlight(text, &terms, true, None);
        assert!(h.contains("\x1b[1;31m"));
        assert_eq!(highlight(text, &["不存在".into()], true, None), text);
        let h2 = highlight(text, &["有為空".into()], true, None);
        assert!(h2.contains("\x1b[1;31m"));
        let simp = cbeta_parse::t2s(text);
        let h3 = highlight(&simp, &["有为空".into()], true, Some("s"));
        assert!(h3.contains("\x1b[1;31m"));
    }

    #[test]
    fn print_human_emits_rank_line() {
        print_human(&[sample_hit()], &["真性".into()], false, None);
    }

    #[test]
    fn display_hit_script_s_keeps_line_id() {
        let h = sample_hit();
        let d = display_hit(&h, Some("s"));
        assert_eq!(d.line_id, h.line_id);
        assert!(d.text_raw.contains('为'));
        assert!(!d.text_raw.contains('為'));
    }
}
