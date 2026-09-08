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
