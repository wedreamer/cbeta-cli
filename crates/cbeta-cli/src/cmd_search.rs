//! `cbeta search`: keyword/phrase hits (TTY + JSON).

use std::io::{stdout, IsTerminal};

use cbeta_core::{Command, Format, Hit};
use cbeta_search::{search, Error as SearchError, DEFAULT_LIMIT};

use crate::env_paths::index_root;

/// Run search; exit 0 hits / 1 no-hit / 2 no-index or usage.
pub fn run(cmd: &Command) -> i32 {
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
            eprintln!("no index found under {p}; run: cbeta build --scope <name>");
            return 2;
        }
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    match cmd.format {
        Format::Json => {
            #[allow(clippy::expect_used)]
            {
                let body = serde_json::json!({ "hits": hits });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).expect("hits json")
                );
            }
        }
        Format::Tty | Format::Plain | Format::Jsonl => {
            let color = want_color(cmd.format == Format::Plain);
            print_human(&hits, &parsed.terms, color);
        }
    }

    if hits.is_empty() {
        1
    } else {
        0
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

fn print_human(hits: &[Hit], terms: &[String], color: bool) {
    for (i, h) in hits.iter().enumerate() {
        let snip = highlight(&h.text_raw, terms, color);
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

fn highlight(text: &str, terms: &[String], color: bool) -> String {
    if !color || terms.is_empty() {
        return text.to_string();
    }
    // Display is 繁體; user terms may be 简体 — try s2t so the snippet actually lights up.
    for t in terms {
        if t.is_empty() {
            continue;
        }
        let trad = cbeta_parse::s2t(t);
        for needle in [t.as_str(), trad.as_str()] {
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
