//! Binary contract for `cbeta completion bash|zsh|fish` (issue #40).
//! Locks shell scripts that list product subcommands/flags so tab-complete stays honest.
//! No index required — completion is pure clap surface.

use std::process::{Command, Output};

/// Spawn the real `cbeta` binary with host home/color isolated.
fn cbeta() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cbeta"));
    cmd.env("NO_COLOR", "1");
    cmd.env_remove("HOME");
    cmd
}

fn run(args: &[&str]) -> Output {
    cbeta()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("spawn cbeta {args:?}: {e}"))
}

fn stdout_utf8(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap_or_else(|e| panic!("stdout utf-8: {e}"))
}

fn stderr_utf8(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap_or_else(|e| panic!("stderr utf-8: {e}"))
}

#[test]
fn completion_bash_prints_search_and_flags() {
    // Given: no index needed (completion is clap-only)
    // When: scholar dumps bash completion
    let out = run(&["completion", "bash"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);
    // Then: exit 0 and script names product surface
    assert_eq!(
        out.status.code(),
        Some(0),
        "completion bash exit 0; stdout={stdout} stderr={stderr}"
    );
    // SOURCE-EMITTED presence lock: every unhidden clap Cmds variant must appear
    // as a completion word (cli_args.rs Cmds — never hide=true).
    for sub in [
        "search",
        "verify",
        "get",
        "read",
        "cite",
        "catalog",
        "info",
        "build",
        "bench",
        "serve",
        "completion",
        "fetch",
        "releases",
        "use",
        "current",
        "pull",
        "gc",
        "prune",
    ] {
        assert!(
            stdout.contains(sub),
            "bash completion missing subcommand `{sub}`; got:\n{stdout}"
        );
    }
    for flag in [
        "--canon", "--work", "--author", "--scope", "--json", "--script",
    ] {
        assert!(
            stdout.contains(flag),
            "bash completion missing flag `{flag}`; got:\n{stdout}"
        );
    }
}

#[test]
fn completion_zsh_is_sourceable_script() {
    // Given: no index
    // When: dump zsh completion
    let out = run(&["completion", "zsh"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);
    // Then: exit 0, non-empty script mentioning the binary name
    assert_eq!(
        out.status.code(),
        Some(0),
        "completion zsh exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        !stdout.trim().is_empty(),
        "zsh completion stdout empty; stderr={stderr}"
    );
    assert!(
        stdout.contains("cbeta"),
        "zsh completion should mention cbeta; got:\n{stdout}"
    );
}

#[test]
fn completion_fish_is_sourceable_script() {
    // Given: no index
    // When: dump fish completion
    let out = run(&["completion", "fish"]);
    let stdout = stdout_utf8(&out);
    let stderr = stderr_utf8(&out);
    // Then: exit 0, non-empty script mentioning the binary name
    assert_eq!(
        out.status.code(),
        Some(0),
        "completion fish exit 0; stdout={stdout} stderr={stderr}"
    );
    assert!(
        !stdout.trim().is_empty(),
        "fish completion stdout empty; stderr={stderr}"
    );
    assert!(
        stdout.contains("cbeta"),
        "fish completion should mention cbeta; got:\n{stdout}"
    );
}

#[test]
fn completion_unknown_shell_exits_2() {
    // Given: clap value_enum only accepts known shells
    // When: unknown shell name
    let out = run(&["completion", "not-a-shell"]);
    let stderr = stderr_utf8(&out);
    // Then: usage/rejection exit 2 (not 0/1)
    assert_eq!(
        out.status.code(),
        Some(2),
        "unknown shell must exit 2; stderr={stderr}"
    );
}
