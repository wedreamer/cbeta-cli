//! `cbeta` binary: clap → [`cbeta_core::Command`] → product handlers.

mod citation;
mod cli_args;
mod cmd_build;
mod env_paths;
mod scope_io;

use cbeta_core::{parse_query, Action, Command};
use clap::Parser;

use cli_args::{format_of, resolve, Cli};

fn main() {
    let out = resolve(Cli::parse());

    // WHY: only Search runs parse_query; get/verify/build must keep raw q untouched.
    let parsed = if out.action == Action::Search {
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
        Action::Search if cmd.explain && !out.json => {
            if let Some(p) = &cmd.parsed_query {
                println!(
                    "mode={} terms={:?} within={:?}",
                    p.mode, p.terms, p.within_chars
                );
            }
        }
        Action::Search | Action::Verify | Action::Get | Action::Catalog | Action::Info
            if out.json || cmd.explain =>
        {
            // Command is our own Serialize types; serde_json pretty-print cannot fail here.
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
