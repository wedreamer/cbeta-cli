//! `cbeta` binary: clap → [`cbeta_core::Command`] → product handlers.

mod citation;
mod cli_args;
mod cmd_build;
mod cmd_catalog;
mod cmd_get;
mod cmd_search;
mod env_paths;
mod scope_io;

use cbeta_core::{parse_query, Action, Command};
use clap::Parser;

use cli_args::{format_of, resolve, Cli};

fn main() {
    let out = resolve(Cli::parse());

    // WHY: only Search runs parse_query; get/verify/build must keep raw q untouched.
    let mut parsed = if out.action == Action::Search {
        match out.q.as_deref() {
            Some(raw) => match parse_query(raw) {
                Ok(p) => Some(p),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            },
            None => None,
        }
    } else {
        None
    };

    // WHY: clap --mode overrides DSL-inferred mode; P0 accepts keyword|phrase only.
    if let Some(flag) = out.mode.as_deref() {
        match parsed.as_mut() {
            Some(pq) if flag == "keyword" || flag == "phrase" => {
                pq.mode = flag.to_string();
            }
            Some(_) => {
                eprintln!("unknown --mode {flag}; expected keyword or phrase");
                std::process::exit(2);
            }
            None => {}
        }
    }

    let cmd = Command {
        action: out.action,
        q: out.q,
        filters: out.filters,
        format: format_of(out.json, out.plain),
        explain: out.explain,
        parsed_query: parsed,
    };

    // Product handlers first; remaining actions stay scaffold (Command dump / exit 2).
    match cmd.action {
        Action::Build => std::process::exit(cmd_build::run(&cmd)),
        // --explain (± --json): parse dump only, never hits.
        Action::Search if cmd.explain => {
            if out.json {
                #[allow(clippy::expect_used)]
                {
                    println!("{}", serde_json::to_string_pretty(&cmd).expect("json"));
                }
            } else if let Some(p) = &cmd.parsed_query {
                println!(
                    "mode={} terms={:?} within={:?} ordered={:?}",
                    p.mode, p.terms, p.within_chars, p.ordered
                );
            }
        }
        Action::Search => std::process::exit(cmd_search::run(&cmd)),
        Action::Get => std::process::exit(cmd_get::run(&cmd)),
        Action::Catalog => std::process::exit(cmd_catalog::run_catalog(&cmd)),
        Action::Info => std::process::exit(cmd_catalog::run_info(&cmd)),
        Action::Verify if out.json => {
            #[allow(clippy::expect_used)]
            {
                println!("{}", serde_json::to_string_pretty(&cmd).expect("json"));
            }
        }
        Action::Serve => {
            eprintln!("index not built yet; try: cbeta search --explain --json '空性+缘生'");
            std::process::exit(2);
        }
        _ => {
            eprintln!("index not built yet; try: cbeta search --explain --json '空性+缘生'");
            std::process::exit(2);
        }
    }
}
