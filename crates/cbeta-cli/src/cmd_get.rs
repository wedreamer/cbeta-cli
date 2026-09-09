//! `cbeta get` / `read` / `cite`: line lookup, context, juan listing, copy.

use cbeta_core::{Action, Command, Format, Hit};
use cbeta_search::{get_context, get_line, list_work_juan, Error as SearchError, GetContext};

use crate::copy_fmt::format_copy_block;
use crate::env_paths::index_root;

/// CLI-only get options (not yet on transport `Command`).
#[derive(Debug, Clone, Copy, Default)]
pub struct GetOpts {
    /// `-C` / `--context` radius (sorted line_id neighbors).
    pub context: u32,
    /// `--copy`: print notes block and best-effort clipboard.
    pub copy: bool,
}

/// Exit 0 found / 1 missing / 2 usage or index.
pub fn run(cmd: &Command, opts: GetOpts) -> i32 {
    match cmd.action {
        Action::Get => run_get(cmd, opts),
        Action::Read => run_read(cmd),
        Action::Cite => run_cite(cmd),
        other => {
            eprintln!("cmd_get: unexpected action {other:?}");
            2
        }
    }
}

fn run_get(cmd: &Command, opts: GetOpts) -> i32 {
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

    let result = if opts.context > 0 {
        get_context(&root, line_id, opts.context)
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
            if opts.copy {
                let block = format_copy_block(&got.hit);
                println!("{block}");
                try_clipboard(&block);
            } else if cmd.format == Format::Json {
                print_get_json(&got);
            } else {
                print_get_tty(&got);
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
            eprintln!("no index found under {p}; run: cbeta build --scope <name>");
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
            println!("{}", hit.citation);
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

fn print_get_json(got: &GetContext) {
    #[allow(clippy::expect_used)]
    {
        let body = serde_json::json!({
            "hit": got.hit,
            "context": got.context,
        });
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
    let center = &got.hit.line_id;
    let (before, after): (Vec<&Hit>, Vec<&Hit>) = got
        .context
        .iter()
        .partition(|h| h.line_id.as_str() < center.as_str());
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

/// Best-effort clipboard; failure is a stderr warning only.
fn try_clipboard(text: &str) {
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            if let Err(e) = cb.set_text(text.to_string()) {
                eprintln!("clipboard: {e}");
            }
        }
        Err(e) => {
            eprintln!("clipboard unavailable: {e}");
        }
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
        assert_eq!(run(&cmd, GetOpts::default()), 2);
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
        assert_eq!(run(&cmd, GetOpts::default()), 2);
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
        assert_eq!(run(&cmd, GetOpts::default()), 2);

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
        assert_eq!(run(&get, GetOpts::default()), 0);
        get.format = Format::Tty;
        assert_eq!(run(&get, GetOpts::default()), 0);
        get.q = Some("T30n1578_p0268a12".into());
        assert_eq!(run(&get, GetOpts::default()), 1);

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&empty);
        let _ = std::fs::remove_dir_all(&index);
    }
}
