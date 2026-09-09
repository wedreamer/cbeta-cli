//! Map clap [`Cli`](crate::cli_args::Cli) into a transport-neutral precursor.

use cbeta_core::{Action, Filters, Format};

use crate::cli_args::{Cli, Cmds};

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
    /// `serve --http ADDR`; None means stdio MCP.
    pub http: Option<String>,
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

/// Map clap parse result to action + flags + filters.
pub fn resolve(cli: Cli) -> CliOut {
    let json = cli.json;
    let mode = cli.mode.clone();
    let explain = cli.explain;
    let plain = cli.plain;
    let script = cli.script.clone();
    let copy = cli.copy;
    let save = cli.save.clone();
    let from = cli.from.clone();
    let hit_index = cli.index;
    let filters = merge_filters(cli.canon, cli.authors, cli.types, cli.works, cli.titles);
    let mut out = CliOut {
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
        http: None,
    };
    match cli.command {
        None => {}
        Some(Cmds::Search { q }) => {
            out.q = q;
        }
        Some(Cmds::Verify { text }) => {
            out.action = Action::Verify;
            out.q = Some(text);
            out.mode = None;
            out.filters = Filters::default();
        }
        Some(Cmds::Get { line_id, context }) => {
            out.action = Action::Get;
            out.q = line_id;
            out.mode = None;
            out.explain = false;
            out.filters = Filters::default();
            out.context = context;
        }
        Some(Cmds::Read { work, juan }) => {
            out.action = Action::Read;
            out.q = Some(work);
            out.mode = None;
            out.explain = false;
            if let Some(j) = juan {
                if !out.filters.juans.contains(&j) {
                    out.filters.juans.push(j);
                }
            }
        }
        Some(Cmds::Cite { line_id }) => {
            out.action = Action::Cite;
            out.q = Some(line_id);
            out.mode = None;
            out.explain = false;
            out.filters = Filters::default();
        }
        Some(Cmds::Catalog) => {
            out.action = Action::Catalog;
            out.q = None;
            out.mode = None;
            out.explain = false;
        }
        Some(Cmds::Info) => {
            out.action = Action::Info;
            out.q = None;
            out.mode = None;
            out.explain = false;
            out.filters = Filters::default();
        }
        Some(Cmds::Build { scope }) => {
            out.action = Action::Build;
            out.q = scope;
            out.mode = None;
            out.explain = false;
            out.script = None;
            out.filters = Filters::default();
        }
        Some(Cmds::Bench { scope }) => {
            out.action = Action::Bench;
            out.q = scope;
            out.mode = None;
            out.explain = false;
            out.filters = Filters::default();
        }
        Some(Cmds::Serve { http }) => {
            out.action = Action::Serve;
            out.q = None;
            out.json = false;
            out.mode = None;
            out.explain = false;
            out.plain = false;
            out.script = None;
            out.filters = Filters::default();
            out.copy = false;
            out.save = None;
            out.from = None;
            out.hit_index = None;
            out.http = http;
        }
        Some(Cmds::Completion { shell }) => {
            out.action = Action::Completion;
            out.q = None;
            out.json = false;
            out.mode = None;
            out.explain = false;
            out.plain = false;
            out.script = None;
            out.filters = Filters::default();
            out.copy = false;
            out.save = None;
            out.from = None;
            out.hit_index = None;
            out.shell = Some(shell);
        }
    }
    out
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
