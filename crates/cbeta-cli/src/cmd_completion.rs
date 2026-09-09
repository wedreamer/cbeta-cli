//! `cbeta completion <shell>`: dump clap_complete scripts to stdout (issue #40).

use std::io;

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli_args::Cli;

/// Write a completion script for `shell` to stdout.
///
/// Always returns 0 — generation cannot fail for a built-in clap shell.
pub fn run(shell: Shell) -> i32 {
    let mut cmd = Cli::command();
    // WHY: finalize so clap_complete sees every subcommand and global flag.
    cmd.build();
    generate(shell, &mut cmd, "cbeta", &mut io::stdout());
    0
}
