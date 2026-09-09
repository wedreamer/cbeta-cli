//! Clap surface and resolve into a transport-neutral [`Command`] precursor.

use cbeta_core::{Action, Filters, Format};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cbeta",
    version,
    about = "离线查 CBETA 经论（人用 CLI；MCP 是 cbeta serve）"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Cmds>,
    /// 检索字串（无子命令时等价 search）
    pub query: Option<String>,
    #[arg(long, global = true)]
    pub json: bool,
    /// Search engine mode override (`keyword` or `phrase`).
    #[arg(long, global = true)]
    pub mode: Option<String>,
    #[arg(long, global = true)]
    pub explain: bool,
    #[arg(long, global = true)]
    pub plain: bool,
    #[arg(long, global = true)]
    pub canon: Option<String>,
    #[arg(long = "author", global = true)]
    pub authors: Vec<String>,
    #[arg(long = "type", global = true)]
    pub types: Vec<String>,
    #[arg(long = "work", global = true)]
    pub works: Vec<String>,
    #[arg(long = "title", global = true)]
    pub titles: Vec<String>,
    /// Display script: `s` 简体, `t` 繁體. Unset keeps default 繁體.
    #[arg(long, global = true, value_parser = ["s", "t"])]
    pub script: Option<String>,
    /// Persist search hits as `last.json` (`last` only).
    #[arg(long, global = true)]
    pub save: Option<String>,
    /// Load a prior `--save last` session (`last` only).
    #[arg(long, global = true)]
    pub from: Option<String>,
    /// 1-based hit index within a saved session (with `--from`).
    #[arg(long, global = true)]
    pub index: Option<u32>,
    /// Print notes-ready citation block to stdout (no OS clipboard).
    #[arg(long, global = true)]
    pub copy: bool,
}

#[derive(Subcommand)]
pub enum Cmds {
    Search {
        q: Option<String>,
    },
    Verify {
        text: String,
    },
    Get {
        /// Line id; optional when `--from last --index N` supplies the hit.
        line_id: Option<String>,
        /// Neighbor radius on sorted `line_id` (like `rg -C`).
        #[arg(short = 'C', long = "context", default_value_t = 0)]
        context: u32,
    },
    /// List lines of a work (optionally one juan) in `line_id` order.
    Read {
        work: String,
        #[arg(long)]
        juan: Option<u32>,
    },
    /// Print CBETA citation string only.
    Cite {
        line_id: String,
    },
    Catalog,
    Info,
    Build {
        #[arg(long)]
        scope: Option<String>,
    },
    /// CLI-only micro-benchmark (keyword/phrase/near); not an MCP tool.
    Bench {
        #[arg(long)]
        scope: Option<String>,
    },
    Serve,
    /// Emit shell completion script to stdout (bash/zsh/fish/…).
    Completion {
        /// Target shell (`clap_complete::Shell` value_enum).
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

/// Resolved clap values before `parse_query` / product dispatch.
pub struct CliOut {
    pub action: Action,
    pub q: Option<String>,
    pub json: bool,
    /// CLI `--mode` override; applied to `parsed_query` after parse.
    pub mode: Option<String>,
    pub explain: bool,
    pub plain: bool,
    /// Display script (`s`/`t`); display-only, never mutates `line_id` / index.
    pub script: Option<String>,
    pub filters: Filters,
    /// `-C` / `--context` for Get.
    pub context: u32,
    /// `--copy` (global); notes block on stdout, no OS clipboard.
    pub copy: bool,
    /// `--save` name (`last` only once validated in main).
    pub save: Option<String>,
    /// `--from` name (`last` only once validated in main).
    pub from: Option<String>,
    /// `--index` 1-based hit rank within a saved session.
    pub hit_index: Option<u32>,
    /// Target shell for [`Action::Completion`]; kept here so cbeta-core stays clap-free.
    pub shell: Option<clap_complete::Shell>,
}

fn merge_filters(
    canon: Option<String>,
    authors: Vec<String>,
    types: Vec<String>,
    works: Vec<String>,
    titles: Vec<String>,
) -> Filters {
    Filters {
        canons: canon.into_iter().collect(),
        authors,
        types,
        works,
        titles,
        ..Filters::default()
    }
}

/// Shared transport flags copied onto every [`CliOut`] branch.
fn transport(cli: &Cli) -> (bool, Option<String>, bool, bool, Option<String>) {
    (
        cli.json,
        cli.mode.clone(),
        cli.explain,
        cli.plain,
        cli.script.clone(),
    )
}

fn session_flags(cli: &Cli) -> (bool, Option<String>, Option<String>, Option<u32>) {
    (cli.copy, cli.save.clone(), cli.from.clone(), cli.index)
}

/// Map clap parse result to action + flags + filters.
pub fn resolve(cli: Cli) -> CliOut {
    let (json, mode, explain, plain, script) = transport(&cli);
    let (copy, save, from, hit_index) = session_flags(&cli);
    let filters = merge_filters(cli.canon, cli.authors, cli.types, cli.works, cli.titles);
    match cli.command {
        None => CliOut {
            action: Action::Search,
            q: cli.query,
            json,
            mode,
            explain,
            plain,
            script,
            filters,
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Search { q }) => CliOut {
            action: Action::Search,
            q,
            json,
            mode,
            explain,
            plain,
            script,
            filters,
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Verify { text }) => CliOut {
            action: Action::Verify,
            q: Some(text),
            json,
            mode: None,
            explain,
            plain,
            script,
            filters: Filters::default(),
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Get { line_id, context }) => CliOut {
            action: Action::Get,
            q: line_id,
            json,
            mode: None,
            explain: false,
            plain,
            script,
            filters: Filters::default(),
            context,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Read { work, juan }) => {
            let mut filters = filters;
            if let Some(j) = juan {
                if !filters.juans.contains(&j) {
                    filters.juans.push(j);
                }
            }
            CliOut {
                action: Action::Read,
                q: Some(work),
                json,
                mode: None,
                explain: false,
                plain,
                script,
                filters,
                context: 0,
                copy,
                save,
                from,
                hit_index,
                shell: None,
            }
        }
        Some(Cmds::Cite { line_id }) => CliOut {
            action: Action::Cite,
            q: Some(line_id),
            json,
            mode: None,
            explain: false,
            plain,
            script,
            filters: Filters::default(),
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Catalog) => CliOut {
            action: Action::Catalog,
            q: None,
            json,
            mode: None,
            explain: false,
            plain,
            script,
            filters,
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Info) => CliOut {
            action: Action::Info,
            q: None,
            json,
            mode: None,
            explain: false,
            plain,
            script,
            filters: Filters::default(),
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Build { scope }) => CliOut {
            action: Action::Build,
            q: scope,
            json,
            mode: None,
            explain: false,
            plain,
            script: None,
            filters: Filters::default(),
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Bench { scope }) => CliOut {
            action: Action::Bench,
            q: scope,
            json,
            mode: None,
            explain: false,
            plain,
            script,
            filters: Filters::default(),
            context: 0,
            copy,
            save,
            from,
            hit_index,
            shell: None,
        },
        Some(Cmds::Serve) => CliOut {
            action: Action::Serve,
            q: None,
            json: false,
            mode: None,
            explain: false,
            plain: false,
            script: None,
            filters: Filters::default(),
            context: 0,
            copy: false,
            save: None,
            from: None,
            hit_index: None,
            shell: None,
        },
        Some(Cmds::Completion { shell }) => CliOut {
            action: Action::Completion,
            q: None,
            json: false,
            mode: None,
            explain: false,
            plain: false,
            script: None,
            filters: Filters::default(),
            context: 0,
            copy: false,
            save: None,
            from: None,
            hit_index: None,
            shell: Some(shell),
        },
    }
}

/// Map json/plain flags to [`Format`].
pub fn format_of(json: bool, plain: bool) -> Format {
    if json {
        Format::Json
    } else if plain {
        Format::Plain
    } else {
        Format::Tty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cli() -> Cli {
        Cli {
            command: None,
            query: None,
            json: false,
            mode: None,
            explain: false,
            plain: false,
            canon: None,
            authors: vec![],
            types: vec![],
            works: vec![],
            titles: vec![],
            script: None,
            save: None,
            from: None,
            index: None,
            copy: false,
        }
    }

    #[test]
    fn format_of_json_plain_tty_precedence() {
        assert_eq!(format_of(true, true), Format::Json);
        assert_eq!(format_of(false, true), Format::Plain);
        assert_eq!(format_of(false, false), Format::Tty);
    }

    #[test]
    fn resolve_bare_query_is_search() {
        let mut cli = base_cli();
        cli.query = Some("空".into());
        cli.canon = Some("T".into());
        cli.authors = vec!["玄奘".into()];
        let out = resolve(cli);
        assert_eq!(out.action, Action::Search);
        assert_eq!(out.q.as_deref(), Some("空"));
        assert_eq!(out.filters.canons, vec!["T".to_string()]);
        assert_eq!(out.filters.authors, vec!["玄奘".to_string()]);
    }

    #[test]
    fn resolve_all_subcommands() {
        let cases: Vec<(Cmds, Action, Option<&str>)> = vec![
            (
                Cmds::Search {
                    q: Some("q".into()),
                },
                Action::Search,
                Some("q"),
            ),
            (Cmds::Verify { text: "t".into() }, Action::Verify, Some("t")),
            (
                Cmds::Get {
                    line_id: Some("L".into()),
                    context: 0,
                },
                Action::Get,
                Some("L"),
            ),
            (
                Cmds::Read {
                    work: "T0235".into(),
                    juan: Some(1),
                },
                Action::Read,
                Some("T0235"),
            ),
            (
                Cmds::Cite {
                    line_id: "L".into(),
                },
                Action::Cite,
                Some("L"),
            ),
            (Cmds::Catalog, Action::Catalog, None),
            (Cmds::Info, Action::Info, None),
            (
                Cmds::Build {
                    scope: Some("s".into()),
                },
                Action::Build,
                Some("s"),
            ),
            (
                Cmds::Bench {
                    scope: Some("s".into()),
                },
                Action::Bench,
                Some("s"),
            ),
            (Cmds::Serve, Action::Serve, None),
            (
                Cmds::Completion {
                    shell: clap_complete::Shell::Bash,
                },
                Action::Completion,
                None,
            ),
        ];
        for (cmd, action, q) in cases {
            let mut cli = base_cli();
            cli.command = Some(cmd);
            cli.json = true;
            let out = resolve(cli);
            assert_eq!(out.action, action);
            assert_eq!(out.q.as_deref(), q);
            if action == Action::Completion {
                assert_eq!(out.shell, Some(clap_complete::Shell::Bash));
            } else {
                assert!(out.shell.is_none());
            }
        }
    }

    #[test]
    fn resolve_catalog_keeps_filter_flags() {
        let mut cli = base_cli();
        cli.command = Some(Cmds::Catalog);
        cli.works = vec!["T1578".into()];
        cli.types = vec!["lun".into()];
        cli.titles = vec!["掌珍".into()];
        let out = resolve(cli);
        assert_eq!(out.filters.works, vec!["T1578".to_string()]);
        assert_eq!(out.filters.types, vec!["lun".to_string()]);
        assert_eq!(out.filters.titles, vec!["掌珍".to_string()]);
    }

    #[test]
    fn resolve_get_clears_explain_keeps_context_copy() {
        let mut cli = base_cli();
        cli.command = Some(Cmds::Get {
            line_id: Some("x".into()),
            context: 4,
        });
        cli.explain = true;
        cli.copy = true;
        let out = resolve(cli);
        assert!(!out.explain);
        assert_eq!(out.context, 4);
        assert!(out.copy);
    }

    #[test]
    fn resolve_read_wires_juan_into_filters() {
        let mut cli = base_cli();
        cli.command = Some(Cmds::Read {
            work: "T0235".into(),
            juan: Some(1),
        });
        let out = resolve(cli);
        assert_eq!(out.action, Action::Read);
        assert_eq!(out.filters.juans, vec![1]);
    }

    #[test]
    fn resolve_search_keeps_mode_flag() {
        let mut cli = base_cli();
        cli.command = Some(Cmds::Search {
            q: Some("缘生故如幻".into()),
        });
        cli.mode = Some("phrase".into());
        let out = resolve(cli);
        assert_eq!(out.mode.as_deref(), Some("phrase"));
    }

    #[test]
    fn resolve_save_from_index_global_flags() {
        let mut cli = base_cli();
        cli.command = Some(Cmds::Search {
            q: Some("空".into()),
        });
        cli.save = Some("last".into());
        let out = resolve(cli);
        assert_eq!(out.save.as_deref(), Some("last"));

        let mut cli = base_cli();
        cli.command = Some(Cmds::Get {
            line_id: None,
            context: 0,
        });
        cli.from = Some("last".into());
        cli.index = Some(1);
        let out = resolve(cli);
        assert_eq!(out.from.as_deref(), Some("last"));
        assert_eq!(out.hit_index, Some(1));
        assert!(out.q.is_none());

        let mut cli = base_cli();
        cli.from = Some("last".into());
        cli.copy = true;
        cli.query = Some("1".into());
        let out = resolve(cli);
        assert_eq!(out.action, Action::Search);
        assert!(out.copy);
        assert_eq!(out.q.as_deref(), Some("1"));
        assert_eq!(out.from.as_deref(), Some("last"));
    }

    #[test]
    fn clap_parse_save_and_from_last_copy() {
        let cli = Cli::try_parse_from([
            "cbeta",
            "search",
            "--json",
            "--save",
            "last",
            "真性有为空",
        ])
        .expect("parse search --save last");
        assert_eq!(cli.save.as_deref(), Some("last"));
        let out = resolve(cli);
        assert_eq!(out.action, Action::Search);
        assert_eq!(out.save.as_deref(), Some("last"));

        let cli = Cli::try_parse_from(["cbeta", "get", "--from", "last", "--index", "1", "-C", "4"])
            .expect("parse get --from last");
        assert_eq!(cli.from.as_deref(), Some("last"));
        assert_eq!(cli.index, Some(1));
        let out = resolve(cli);
        assert_eq!(out.action, Action::Get);
        assert!(out.q.is_none());
        assert_eq!(out.hit_index, Some(1));
        assert_eq!(out.context, 4);

        let cli = Cli::try_parse_from(["cbeta", "--from", "last", "--copy", "1"])
            .expect("parse bare --from last --copy 1");
        assert!(cli.copy);
        assert_eq!(cli.from.as_deref(), Some("last"));
        assert_eq!(cli.query.as_deref(), Some("1"));
    }
}
