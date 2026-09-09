//! `cbeta` binary: clap → [`cbeta_core::Command`] → product handlers.

mod citation;
mod cli_args;
mod cli_resolve;
mod cmd_bench;
mod cmd_build;
mod cmd_catalog;
mod cmd_completion;
mod cmd_get;
mod cmd_repl;
mod cmd_search;
mod cmd_serve;
mod cmd_verify;
mod copy_fmt;
mod env_paths;
mod mcp;
mod scope_io;
mod session;

use cbeta_core::{parse_query, Action, Command};
use clap::Parser;

use cli_args::Cli;
use cli_resolve::{format_of, resolve, CliOut};

fn main() {
    // WHY: bare `cbeta` (args_os len == 1) is REPL even when stdin is a pipe;
    // any extra argv/flag stays clap Search/etc. (not REPL).
    if std::env::args_os().len() == 1 {
        std::process::exit(cmd_repl::run());
    }

    let mut out = resolve(Cli::parse());

    // WHY: completion is clap-only; skip parse_query / Command build entirely.
    if out.action == Action::Completion {
        match out.shell {
            Some(shell) => std::process::exit(cmd_completion::run(shell)),
            None => {
                eprintln!("internal: completion without shell");
                std::process::exit(2);
            }
        }
    }

    if let Err(code) = validate_save_from(&out) {
        std::process::exit(code);
    }

    if out.from.as_deref() == Some("last") {
        if let Err(code) = apply_from_last(&mut out) {
            std::process::exit(code);
        }
    }

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
        q: out.q.clone(),
        filters: out.filters.clone(),
        format: format_of(out.json, out.plain),
        explain: out.explain,
        parsed_query: parsed,
        context: if out.context > 0 {
            Some(out.context)
        } else {
            None
        },
        copy: out.copy,
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
        Action::Search => std::process::exit(cmd_search::run(
            &cmd,
            out.script.as_deref(),
            out.save.as_deref(),
        )),
        Action::Get | Action::Read | Action::Cite => std::process::exit(cmd_get::run(&cmd)),
        Action::Catalog => std::process::exit(cmd_catalog::run_catalog(&cmd)),
        Action::Info => std::process::exit(cmd_catalog::run_info(&cmd)),
        Action::Verify => std::process::exit(cmd_verify::run(&cmd)),
        Action::Bench => std::process::exit(cmd_bench::run(&cmd)),
        Action::Serve => std::process::exit(cmd_serve::run(out.http.as_deref())),
        // Handled before Command construction; unreachable here.
        Action::Completion => std::process::exit(2),
    }
}

/// Reject unknown `--save` / `--from` names and `--save` outside Search.
fn validate_save_from(out: &CliOut) -> Result<(), i32> {
    if let Some(name) = out.save.as_deref() {
        if name != "last" {
            eprintln!("--save only supports 'last' (got {name})");
            return Err(2);
        }
        if out.action != Action::Search {
            eprintln!("--save is only valid with search");
            return Err(2);
        }
    }
    if let Some(name) = out.from.as_deref() {
        if name != "last" {
            eprintln!("--from only supports 'last' (got {name})");
            return Err(2);
        }
    }
    Ok(())
}

/// Resolve `--from last` into a Get of the chosen hit's `line_id`.
///
/// WHY: never re-runs search; refuses when `artifact_id` no longer matches the
/// active index so a rebuild cannot silently open a stale line.
fn apply_from_last(out: &mut CliOut) -> Result<(), i32> {
    let session = match session::load_last() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return Err(2);
        }
    };

    let info = match cmd_catalog::load_info() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{e}");
            return Err(2);
        }
    };
    if session.artifact_id != info.artifact_id {
        eprintln!("index changed; re-run search");
        return Err(2);
    }

    let n = if out.action == Action::Get {
        match out.hit_index {
            Some(n) => n,
            None => {
                eprintln!("get --from last requires --index <n>");
                return Err(2);
            }
        }
    } else if out.copy {
        match out.q.as_deref().and_then(|s| s.parse::<u32>().ok()) {
            Some(n) => n,
            None => {
                eprintln!("--from last --copy requires a 1-based hit index");
                return Err(2);
            }
        }
    } else {
        eprintln!("--from last requires get --index <n> or --copy <n>");
        return Err(2);
    };

    let hit = match session::pick_hit(&session, n) {
        Ok(h) => h,
        Err(code) => {
            eprintln!("hit index {n} out of range ({} hits)", session.hits.len());
            return Err(code);
        }
    };

    out.q = Some(hit.line_id.clone());
    out.action = Action::Get;
    Ok(())
}
