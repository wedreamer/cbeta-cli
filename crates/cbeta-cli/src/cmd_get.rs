//! `cbeta get` / `read` / `cite`: line lookup, context, juan listing, copy.

use cbeta_core::{Action, Command, Format, Hit};
use cbeta_search::{get_context, get_line, list_work_juan, Error as SearchError, GetContext};

use crate::copy_fmt::format_copy_block;
use crate::env_paths::index_root;

/// Exit 0 found / 1 missing / 2 usage or index.
pub fn run(cmd: &Command) -> i32 {
    match cmd.action {
        Action::Get => run_get(cmd),
        Action::Read => run_read(cmd),
        Action::Cite => run_cite(cmd),
        other => {
            eprintln!("cmd_get: unexpected action {other:?}");
            2
        }
    }
}

fn context_radius(cmd: &Command) -> u32 {
    cmd.context.unwrap_or(0)
}

fn run_get(cmd: &Command) -> i32 {
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

    let radius = context_radius(cmd);
    let result = if radius > 0 {
        get_context(&root, line_id, radius)
    } else {
        get_line(&root, line_id).map(|o| {
            o.map(|hit| GetContext {
                hit,
                context: Vec::new(),
            })
        })
    };

    match result {
        Ok(Some(got)) => {
            if cmd.copy {
                println!("{}", format_copy_block(&got.hit));
            } else if cmd.format == Format::Json {
                print_get_json(&got, radius > 0);
            } else {
                print_get_tty(&got);
            }
            0
        }
        Ok(None) => {
            ghost_not_found(line_id);
            1
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

/// Ghost line_id: `当前 {tag} 无此行` (exit 1).
fn ghost_not_found(line_id: &str) {
    let tag = crate::cmd_catalog::load_info()
        .map(|i| i.cbeta_tag)
        .unwrap_or_default();
    if tag.is_empty() {
        eprintln!("当前 无此行 ({line_id})");
    } else {
        eprintln!("当前 {tag} 无此行");
    }
}

fn run_read(cmd: &Command) -> i32 {
    let Some(work) = cmd.q.as_deref() else {
        eprintln!("read requires a work id (e.g. T0235)");
        return 2;
    };
    let Some(&juan) = cmd.filters.juans.first() else {
        eprintln!("read requires --juan <n>");
        return 2;
    };
    let root = match index_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match list_work_juan(&root, work, juan) {
        Ok(hits) if hits.is_empty() => {
            eprintln!("no lines for {work} juan {juan}");
            1
        }
        Ok(hits) => {
            if cmd.format == Format::Json {
                print_hits_json(&hits);
            } else {
                for h in &hits {
                    println!(
                        "{}  {}  {}  j{}  {}",
                        h.line_id, h.title, h.author, h.juan, h.text_raw
                    );
                }
            }
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

fn run_cite(cmd: &Command) -> i32 {
    let Some(line_id) = cmd.q.as_deref() else {
        eprintln!("cite requires a line_id");
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
            if cmd.copy {
                println!("{}", format_copy_block(&hit));
            } else {
                println!("{}", hit.citation);
            }
            0
        }
        Ok(None) => {
            ghost_not_found(line_id);
            1
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

fn split_neighbors(got: &GetContext) -> (Vec<&Hit>, Vec<&Hit>) {
    let center = got.hit.line_id.as_str();
    got.context
        .iter()
        .partition(|h| h.line_id.as_str() < center)
}

fn print_get_json(got: &GetContext, with_window: bool) {
    #[allow(clippy::expect_used)]
    {
        let body = if with_window {
            let (before, after) = split_neighbors(got);
            serde_json::json!({
                "hit": got.hit,
                "before": before,
                "after": after,
            })
        } else {
            serde_json::json!({ "hit": got.hit })
        };
        println!("{}", serde_json::to_string_pretty(&body).expect("get json"));
    }
}

fn print_hits_json(hits: &[Hit]) {
    #[allow(clippy::expect_used)]
    {
        let body = serde_json::json!({ "hits": hits });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).expect("hits json")
        );
    }
}

fn print_get_tty(got: &GetContext) {
    let (before, after) = split_neighbors(got);
    for h in before {
        println!("{}  {}", h.line_id, h.text_raw);
    }
    println!(
        "{}  {}  {}  j{}  {}",
        got.hit.line_id, got.hit.title, got.hit.author, got.hit.juan, got.hit.text_raw
    );
    println!("{}", got.hit.citation);
    for h in after {
        println!("{}  {}", h.line_id, h.text_raw);
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

    fn cmd_get(q: Option<&str>, format: Format) -> Command {
        Command {
            action: Action::Get,
            q: q.map(str::to_string),
            filters: Filters::default(),
            format,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        }
    }

    #[test]
    fn run_requires_line_id() {
        assert_eq!(run(&cmd_get(None, Format::Json)), 2);
    }

    #[test]
    fn run_index_root_error() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        assert_eq!(run(&cmd_get(Some("T30n1578_p0268b21"), Format::Json)), 2);
    }

    #[test]
    fn run_no_index_and_found_and_missing() {
        let _g = env_lock();
        let empty = temp_dir("empty");
        std::env::set_var("CBETA_INDEX", &empty);
        assert_eq!(run(&cmd_get(Some("T30n1578_p0268b21"), Format::Json)), 2);

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
            context: None,
            copy: false,
        };
        assert_eq!(crate::cmd_build::run(&build, false), 0);

        let mut get = cmd_get(Some("T30n1578_p0268b21"), Format::Json);
        assert_eq!(run(&get), 0);
        get.format = Format::Tty;
        assert_eq!(run(&get), 0);
        get.context = Some(4);
        assert_eq!(run(&get), 0);
        get.copy = true;
        assert_eq!(run(&get), 0);
        get.q = Some("T30n1578_p0268a12".into());
        get.context = None;
        get.copy = false;
        assert_eq!(run(&get), 1);

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&empty);
        let _ = std::fs::remove_dir_all(&index);
    }

    #[test]
    fn run_unexpected_action_is_usage() {
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
        assert_eq!(run(&cmd), 2);
    }

    #[test]
    fn run_read_and_cite_paths() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("read-cite");
        std::env::set_var("CBETA_CORPUS", &corpus);
        std::env::set_var("CBETA_INDEX", &index);
        let build = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Plain,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(crate::cmd_build::run(&build, false), 0);

        let mut read = Command {
            action: Action::Read,
            q: None,
            filters: Filters::default(),
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&read), 2);
        read.q = Some("T0235".into());
        assert_eq!(run(&read), 2);
        read.filters.juans = vec![1];
        assert_eq!(run(&read), 0);
        read.format = Format::Tty;
        assert_eq!(run(&read), 0);
        read.filters.juans = vec![99];
        assert_eq!(run(&read), 1);

        let mut cite = Command {
            action: Action::Cite,
            q: None,
            filters: Filters::default(),
            format: Format::Tty,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cite), 2);
        cite.q = Some("T30n1578_p0268b21".into());
        assert_eq!(run(&cite), 0);
        cite.copy = true;
        assert_eq!(run(&cite), 0);
        cite.copy = false;
        cite.q = Some("T30n1578_p0268a12".into());
        assert_eq!(run(&cite), 1);

        let empty = temp_dir("rc-empty");
        std::env::set_var("CBETA_INDEX", &empty);
        read.q = Some("T0235".into());
        read.filters.juans = vec![1];
        assert_eq!(run(&read), 2);
        cite.q = Some("T30n1578_p0268b21".into());
        assert_eq!(run(&cite), 2);

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn run_read_cite_index_root_error() {
        let _g = env_lock();
        std::env::remove_var("CBETA_INDEX");
        std::env::remove_var("HOME");
        let read = Command {
            action: Action::Read,
            q: Some("T0235".into()),
            filters: Filters {
                juans: vec![1],
                ..Filters::default()
            },
            format: Format::Json,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&read), 2);
        let cite = Command {
            action: Action::Cite,
            q: Some("T30n1578_p0268b21".into()),
            filters: Filters::default(),
            format: Format::Tty,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(run(&cite), 2);
    }
}
