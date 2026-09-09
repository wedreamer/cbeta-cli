//! Interactive REPL when `cbeta` is invoked with no argv after the binary.
//!
//! WHY: scholars iterate scope → search → open/copy without retyping flags.
//! Intercept is `args_os().len() == 1` (not TTY-only); prompt `>` is stderr-only
//! when stdin is a TTY so piped scripts stay quiet.

use std::io::{self, BufRead, IsTerminal, Write};

use cbeta_core::{parse_query, Action, Command, Filters, Format};

use crate::session;

/// Run the REPL until `:q` / EOF. Always returns 0 on clean quit.
///
/// Errors on individual lines print to stderr and keep the loop alive so a bad
/// `:open` does not kill the session; only process-level failures (I/O) exit 2.
pub fn run() -> i32 {
    let mut filters = Filters::default();
    let stdin = io::stdin();
    let mut stderr = io::stderr();
    let show_prompt = stdin.is_terminal();
    let mut lines = stdin.lock().lines();

    loop {
        if show_prompt && (write!(stderr, "> ").is_err() || stderr.flush().is_err()) {
            return 2;
        }

        let line = match lines.next() {
            None => return 0,
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                let _ = writeln!(stderr, "repl read error: {e}");
                return 2;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match dispatch_line(trimmed, &mut filters, &mut stderr) {
            LoopCtrl::Continue => {}
            LoopCtrl::Quit => return 0,
        }
    }
}

enum LoopCtrl {
    Continue,
    Quit,
}

fn dispatch_line(line: &str, filters: &mut Filters, stderr: &mut impl Write) -> LoopCtrl {
    if line == ":q" || line == ":quit" {
        return LoopCtrl::Quit;
    }

    if let Some(rest) = line.strip_prefix(':') {
        return dispatch_colon(rest, filters, stderr);
    }

    // Bare query → Search with current scope; always persist last.json.
    run_search(line, filters, stderr);
    LoopCtrl::Continue
}

fn dispatch_colon(rest: &str, filters: &mut Filters, stderr: &mut impl Write) -> LoopCtrl {
    let mut parts = rest.split_whitespace();
    let Some(cmd) = parts.next() else {
        let _ = writeln!(stderr, "unknown repl command: :");
        return LoopCtrl::Continue;
    };

    match cmd {
        "q" | "quit" => LoopCtrl::Quit,
        "scope" => {
            match parts.next() {
                Some(work) => {
                    filters.works = vec![work.to_string()];
                }
                None => {
                    let _ = writeln!(stderr, ":scope requires a work id (e.g. T1578)");
                }
            }
            LoopCtrl::Continue
        }
        "open" => {
            match parse_one_based(parts.next()) {
                Ok(n) => {
                    let _ = open_hit(n, false, stderr);
                }
                Err(msg) => {
                    let _ = writeln!(stderr, "{msg}");
                }
            }
            LoopCtrl::Continue
        }
        "copy" => {
            match parse_one_based(parts.next()) {
                Ok(n) => {
                    let _ = open_hit(n, true, stderr);
                }
                Err(msg) => {
                    let _ = writeln!(stderr, "{msg}");
                }
            }
            LoopCtrl::Continue
        }
        "verify" => {
            // Remaining text after the verb is the raw quote (no parse_query).
            let quote = rest.strip_prefix("verify").unwrap_or("").trim_start();
            if quote.is_empty() {
                let _ = writeln!(stderr, ":verify requires a quote");
            } else {
                run_verify(quote);
            }
            LoopCtrl::Continue
        }
        other => {
            let _ = writeln!(stderr, "unknown repl command: :{other}");
            LoopCtrl::Continue
        }
    }
}

fn parse_one_based(tok: Option<&str>) -> Result<u32, String> {
    let Some(s) = tok else {
        return Err("requires a 1-based hit index".into());
    };
    s.parse::<u32>()
        .map_err(|_| format!("invalid hit index: {s}"))
        .and_then(|n| {
            if n == 0 {
                Err("hit index must be >= 1".into())
            } else {
                Ok(n)
            }
        })
}

fn run_search(q: &str, filters: &Filters, stderr: &mut impl Write) {
    let parsed = match parse_query(q) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(stderr, "{e}");
            return;
        }
    };
    let cmd = Command {
        action: Action::Search,
        q: Some(q.to_string()),
        filters: filters.clone(),
        format: Format::Tty,
        explain: false,
        parsed_query: Some(parsed),
        context: None,
        copy: false,
    };
    // WHY: REPL always saves last so :open / :copy work without --save.
    let code = crate::cmd_search::run(&cmd, None, Some("last"));
    // Search exit 1 (no-hit) is fine inside the loop; 2 already printed.
    let _ = code;
}

fn run_verify(quote: &str) {
    let cmd = Command {
        action: Action::Verify,
        q: Some(quote.to_string()),
        filters: Filters::default(),
        format: Format::Tty,
        explain: false,
        parsed_query: None,
        context: None,
        copy: false,
    };
    let _ = crate::cmd_verify::run(&cmd);
}

/// Load last.json, pick hit N, run get (optionally --copy).
fn open_hit(n: u32, copy: bool, stderr: &mut impl Write) -> Result<(), ()> {
    let session = match session::load_last() {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(stderr, "{e}");
            return Err(());
        }
    };
    let hit = match session::pick_hit(&session, n) {
        Ok(h) => h,
        Err(_) => {
            let _ = writeln!(
                stderr,
                "hit index {n} out of range ({} hits)",
                session.hits.len()
            );
            return Err(());
        }
    };
    let cmd = Command {
        action: Action::Get,
        q: Some(hit.line_id.clone()),
        filters: Filters::default(),
        format: Format::Tty,
        explain: false,
        parsed_query: None,
        context: None,
        copy,
    };
    let code = crate::cmd_get::run(&cmd);
    if code != 0 {
        // cmd_get already printed; stay in loop.
        return Err(());
    }
    Ok(())
}
