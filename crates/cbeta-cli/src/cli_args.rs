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
        line_id: String,
    },
    Catalog,
    Info,
    Build {
        #[arg(long)]
        scope: Option<String>,
    },
    Serve,
}

/// Resolved clap values before `parse_query` / product dispatch.
pub struct CliOut {
    pub action: Action,
    pub q: Option<String>,
    pub json: bool,
    pub explain: bool,
    pub plain: bool,
    pub filters: Filters,
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
    match cli.command {
        None => CliOut {
            action: Action::Search,
            q: cli.query,
            json: cli.json,
            explain: cli.explain,
            plain: cli.plain,
            filters: merge_filters(cli.canon, cli.authors, cli.types, cli.works, cli.titles),
        },
        Some(Cmds::Search { q }) => CliOut {
            action: Action::Search,
            q,
            json: cli.json,
            explain: cli.explain,
            plain: cli.plain,
            filters: merge_filters(cli.canon, cli.authors, cli.types, cli.works, cli.titles),
        },
        Some(Cmds::Verify { text }) => CliOut {
            action: Action::Verify,
            q: Some(text),
            json: cli.json,
            explain: cli.explain,
            plain: cli.plain,
            filters: Filters::default(),
        },
        Some(Cmds::Get { line_id }) => CliOut {
            action: Action::Get,
            q: Some(line_id),
            json: cli.json,
            explain: false,
            plain: cli.plain,
            filters: Filters::default(),
        },
        Some(Cmds::Catalog) => CliOut {
            action: Action::Catalog,
            q: None,
            json: cli.json,
            explain: false,
            plain: cli.plain,
            filters: merge_filters(cli.canon, cli.authors, cli.types, cli.works, cli.titles),
        },
        Some(Cmds::Info) => CliOut {
            action: Action::Info,
            q: None,
            json: cli.json,
            explain: false,
            plain: cli.plain,
            filters: Filters::default(),
        },
        Some(Cmds::Build { scope }) => CliOut {
            action: Action::Build,
            q: scope,
            json: cli.json,
            explain: false,
            plain: cli.plain,
            filters: Filters::default(),
        },
        Some(Cmds::Serve) => CliOut {
            action: Action::Serve,
            q: None,
            json: false,
            explain: false,
            plain: false,
            filters: Filters::default(),
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
            explain: false,
            plain: false,
            canon: None,
            authors: vec![],
            types: vec![],
            works: vec![],
            titles: vec![],
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
            (Cmds::Search { q: Some("q".into()) }, Action::Search, Some("q")),
            (Cmds::Verify { text: "t".into() }, Action::Verify, Some("t")),
            (Cmds::Get { line_id: "L".into() }, Action::Get, Some("L")),
            (Cmds::Catalog, Action::Catalog, None),
            (Cmds::Info, Action::Info, None),
            (Cmds::Build { scope: Some("s".into()) }, Action::Build, Some("s")),
            (Cmds::Serve, Action::Serve, None),
        ];
        for (cmd, action, q) in cases {
            let mut cli = base_cli();
            cli.command = Some(cmd);
            cli.json = true;
            let out = resolve(cli);
            assert_eq!(out.action, action);
            assert_eq!(out.q.as_deref(), q);
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
    fn resolve_get_clears_explain() {
        let mut cli = base_cli();
        cli.command = Some(Cmds::Get {
            line_id: "x".into(),
        });
        cli.explain = true;
        let out = resolve(cli);
        assert!(!out.explain);
    }
}
