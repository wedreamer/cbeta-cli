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
    canon: Option<String>,
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
        canon: Option<String>,
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

fn filters(canon: Option<String>) -> Filters {
    Filters {
        canons: canon.into_iter().collect(),
        ..Filters::default()
    }
}

fn main() {
    let cli = Cli::parse();
    let (action, q, json, explain, canon) = match cli.command {
        None => (Action::Search, cli.query, cli.json, cli.explain, cli.canon),
        Some(Cmds::Search {
            q,
            json,
            explain,
            canon,
        }) => (Action::Search, q, json, explain, canon),
        Some(Cmds::Verify { text }) => (Action::Verify, Some(text), cli.json, cli.explain, None),
        Some(Cmds::Get { line_id }) => (Action::Get, Some(line_id), cli.json, false, None),
        Some(Cmds::Catalog) => (Action::Catalog, None, cli.json, false, None),
        Some(Cmds::Info) => (Action::Info, None, cli.json, false, None),
        Some(Cmds::Build { scope }) => (Action::Build, scope, cli.json, false, None),
        Some(Cmds::Serve) => (Action::Serve, None, false, false, None),
    };

    // WHY: global parse_query on any `q` is a scaffold bug (AGENTS.md KNOWN SCAFFOLD BUG) — do not treat as product.
    let parsed = match q.as_deref() {
        Some(raw) => match parse_query(raw) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        },
        None => None,
    };

    let cmd = Command {
        action,
        q,
        filters: filters(canon),
        format: if json { Format::Json } else { Format::Tty },
        explain,
        parsed_query: parsed,
    };

    if json || explain {
        // Command is our own Serialize types; serde_json pretty-print cannot fail here.
        #[allow(clippy::expect_used)]
        if json {
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
