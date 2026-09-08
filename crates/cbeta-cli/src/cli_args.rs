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
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub explain: bool,
    #[arg(long)]
    pub plain: bool,
    #[arg(long)]
    pub canon: Option<String>,
    #[arg(long = "author")]
    pub authors: Vec<String>,
    #[arg(long = "type")]
    pub types: Vec<String>,
    #[arg(long = "work")]
    pub works: Vec<String>,
    #[arg(long = "title")]
    pub titles: Vec<String>,
}

#[derive(Subcommand)]
pub enum Cmds {
    Search {
        q: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        explain: bool,
        #[arg(long)]
        plain: bool,
        #[arg(long)]
        canon: Option<String>,
        #[arg(long = "author")]
        authors: Vec<String>,
        #[arg(long = "type")]
        types: Vec<String>,
        #[arg(long = "work")]
        works: Vec<String>,
        #[arg(long = "title")]
        titles: Vec<String>,
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
        Some(Cmds::Search {
            q,
            json,
            explain,
            plain,
            canon,
            authors,
            types,
            works,
            titles,
        }) => CliOut {
            action: Action::Search,
            q,
            json: json || cli.json,
            explain: explain || cli.explain,
            plain: plain || cli.plain,
            filters: merge_filters(
                canon.or(cli.canon),
                if authors.is_empty() {
                    cli.authors
                } else {
                    authors
                },
                if types.is_empty() { cli.types } else { types },
                if works.is_empty() { cli.works } else { works },
                if titles.is_empty() {
                    cli.titles
                } else {
                    titles
                },
            ),
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
