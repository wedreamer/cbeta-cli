//! `cbeta verify <quote>`: original-text check + similar lines.

use cbeta_core::{Command, Format, VerifyReport};
use cbeta_search::{verify, Error as SearchError};

use crate::env_paths::index_root;

/// Exit 0 on success (original or not); 2 no index / usage.
pub fn run(cmd: &Command) -> i32 {
    let Some(quote) = cmd.q.as_deref() else {
        eprintln!("verify requires a quote");
        return 2;
    };
    let root = match index_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match verify(&root, quote) {
        Ok(report) => {
            emit(cmd, &report);
            0
        }
        Err(SearchError::NoIndex(p)) => {
            crate::index_hint::eprint_no_index(&p);
            2
        }
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

fn emit(cmd: &Command, report: &VerifyReport) {
    match cmd.format {
        Format::Json => {
            #[allow(clippy::expect_used)]
            {
                println!(
                    "{}",
                    serde_json::to_string_pretty(report).expect("verify json")
                );
            }
        }
        Format::Tty | Format::Plain | Format::Jsonl => {
            print_human(report);
        }
    }
}

fn print_human(report: &VerifyReport) {
    if report.is_original {
        println!("is_original=true");
        if let Some(h) = &report.exact_hit {
            println!(
                "{}  {}  {}  j{}  {}",
                h.line_id, h.title, h.author, h.juan, h.text_raw
            );
            println!("{}", h.citation);
        }
        return;
    }
    println!("is_original=false");
    if report.similar.is_empty() {
        println!("(no similar lines)");
        return;
    }
    for (i, h) in report.similar.iter().enumerate() {
        println!(
            "{:>3}  {}  {}  {}  j{}  {}",
            i + 1,
            h.line_id,
            h.work_id,
            h.title,
            h.juan,
            h.text_raw
        );
        println!("     {}", h.citation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_paths::env_lock;
    use cbeta_core::{Action, Filters, Format};

    fn mini_corpus() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-cmd-verify-{prefix}-{}-{}",
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

    #[test]
    fn run_requires_quote() {
        let cmd = Command {
            action: Action::Verify,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd), 2);
    }

    #[test]
    fn run_no_index_exits_2() {
        let _g = env_lock();
        let index = temp_dir("empty");
        std::env::set_var("CBETA_INDEX", &index);
        let cmd = Command {
            action: Action::Verify,
            q: Some("空".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cmd), 2);
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }

    #[test]
    fn run_exact_and_variant_json() {
        with_index(|| {
            let mut cmd = Command {
                action: Action::Verify,
                q: Some("真性有為空，如幻緣生故".into()),
                filters: Filters::default(),
                format: Format::Json,
                explain: false,
                parsed_query: None,
                context: None,
                copy: false,
            };
            assert_eq!(run(&cmd), 0);
            cmd.q = Some("真性有为空，缘生故如幻，无为无起灭，不实若空华。".into());
            cmd.format = Format::Plain;
            assert_eq!(run(&cmd), 0);
        });
    }
}
