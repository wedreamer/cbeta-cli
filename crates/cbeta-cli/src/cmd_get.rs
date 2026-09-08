//! `cbeta get <line_id>`: exact line lookup.

use cbeta_core::{Command, Format};
use cbeta_search::{get_line, Error as SearchError};

use crate::env_paths::index_root;

/// Exit 0 found / 1 missing / 2 no index.
pub fn run(cmd: &Command) -> i32 {
    let Some(line_id) = cmd.q.as_deref() else {
        eprintln!("get requires a line_id");
        return 2;
    };
    let root = match index_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match get_line(&root, line_id) {
        Ok(Some(hit)) => {
            if cmd.format == Format::Json {
                #[allow(clippy::expect_used)]
                {
                    let body = serde_json::json!({ "hit": hit });
                    println!("{}", serde_json::to_string_pretty(&body).expect("hit json"));
                }
            } else {
                println!(
                    "{}  {}  {}  j{}  {}",
                    hit.line_id, hit.title, hit.author, hit.juan, hit.text_raw
                );
                println!("{}", hit.citation);
            }
            0
        }
        Ok(None) => {
            eprintln!("line_id not found: {line_id}");
            1
        }
        Err(SearchError::NoIndex(p)) => {
            eprintln!("no index found under {p}; run: cbeta build --scope <name>");
            2
        }
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_core::{Action, Filters, Format};
    use crate::env_paths::env_lock;

    fn mini_corpus() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-cmd-get-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn run_requires_line_id() {
        let cmd = Command {
            action: Action::Get,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
        };
        assert_eq!(run(&cmd), 2);
    }

    #[test]
    fn run_index_root_error() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        let cmd = Command {
            action: Action::Get,
            q: Some("T30n1578_p0268b21".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
        };
        assert_eq!(run(&cmd), 2);
    }

    #[test]
    fn run_no_index_and_found_and_missing() {
        let _g = env_lock();
        let empty = temp_dir("empty");
        std::env::set_var("CBETA_INDEX", &empty);
        let cmd = Command {
            action: Action::Get,
            q: Some("T30n1578_p0268b21".into()),
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
        };
        assert_eq!(run(&cmd), 2);

        let corpus = mini_corpus();
        let index = temp_dir("idx");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        let build = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Plain,
            explain: false,
            parsed_query: None,
        };
        assert_eq!(crate::cmd_build::run(&build), 0);

        let mut get = cmd;
        get.format = Format::Json;
        assert_eq!(run(&get), 0);
        get.format = Format::Tty;
        assert_eq!(run(&get), 0);
        get.q = Some("T30n1578_p0268a12".into());
        assert_eq!(run(&get), 1);

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&empty);
        let _ = std::fs::remove_dir_all(&index);
    }
}
