use cbeta_core::{parse_query, Action, Command, Filters, Format};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cbeta",
    version,
    about = "离线查 CBETA 经论（人用 CLI；MCP 是 cbeta serve）"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmds>,
    /// 检索字串（无子命令时等价 search）
    query: Option<String>,
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
}

#[derive(Subcommand)]
enum Cmds {
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

struct CliOut {
    action: Action,
    q: Option<String>,
    json: bool,
    explain: bool,
    plain: bool,
    filters: Filters,
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

fn resolve(cli: Cli) -> CliOut {
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

fn format_of(json: bool, plain: bool) -> Format {
    if json {
        Format::Json
    } else if plain {
        Format::Plain
    } else {
        Format::Tty
    }
}

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

    if out.json || out.explain {
        // Command is our own Serialize types; serde_json pretty-print cannot fail here.
        #[allow(clippy::expect_used)]
        if out.json {
            println!("{}", serde_json::to_string_pretty(&cmd).expect("json"));
        } else if let Some(p) = &cmd.parsed_query {
            println!(
                "mode={} terms={:?} within={:?}",
                p.mode, p.terms, p.within_chars
            );
        }
        return;
    }
    eprintln!("index not built yet; try: cbeta search --explain --json '空性+缘生'");
    std::process::exit(2);
}
