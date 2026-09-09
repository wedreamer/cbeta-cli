//! Library-side handlers for MCP tools (blocking; call via spawn_blocking).

use cbeta_core::{Filters, Hit, ParsedQuery};
use cbeta_search::{
    get_context, get_line, list_work_juan, search, verify, Error as SearchError, GetContext,
    DEFAULT_LIMIT,
};
use serde_json::json;

use crate::cmd_catalog::{load_catalog_entries, load_info};
use crate::env_paths::index_root;

use super::tools::{CatalogArgs, GetPassageArgs, SearchArgs};

/// Build `ParsedQuery` from MCP search args (`q` or `clauses` + `mode`).
pub(super) fn build_parsed_query(args: &SearchArgs) -> Result<ParsedQuery, String> {
    let clauses = args
        .clauses
        .as_ref()
        .map(|c| {
            c.iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !clauses.is_empty() {
        let mode = args
            .mode
            .as_deref()
            .unwrap_or("near")
            .trim()
            .to_ascii_lowercase();
        return match mode.as_str() {
            "near" => Ok(ParsedQuery {
                raw: clauses.join("+"),
                mode: "near".into(),
                terms: clauses,
                within_chars: Some(30),
                wildcard: None,
                ordered: Some(false),
                bool_op: None,
                excluded: Vec::new(),
            }),
            "before" => Ok(ParsedQuery {
                raw: clauses.join("*"),
                mode: "before".into(),
                terms: clauses,
                within_chars: Some(30),
                wildcard: None,
                ordered: Some(true),
                bool_op: None,
                excluded: Vec::new(),
            }),
            "keyword" | "phrase" => Ok(ParsedQuery {
                raw: clauses.join(" "),
                mode: mode.clone(),
                terms: clauses,
                within_chars: None,
                wildcard: None,
                ordered: None,
                bool_op: None,
                excluded: Vec::new(),
            }),
            other => Err(format!(
                "unknown mode {other}; expected keyword, phrase, near, or before"
            )),
        };
    }

    let raw = args
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "cbeta_search requires q or clauses".to_string())?;
    let mut parsed = cbeta_core::parse_query(raw).map_err(|e| e.to_string())?;
    if let Some(flag) = args.mode.as_deref() {
        match flag {
            "keyword" | "phrase" => parsed.mode = flag.to_string(),
            "near" | "before" => {}
            other => {
                return Err(format!(
                    "unknown mode {other}; expected keyword, phrase, near, or before"
                ));
            }
        }
    }
    Ok(parsed)
}

/// Run search and return JSON `{ "hits": [...] }`.
pub(super) fn run_search(args: SearchArgs) -> Result<serde_json::Value, String> {
    let parsed = build_parsed_query(&args)?;
    let filters = filters_from(
        args.canon,
        args.author.unwrap_or_default(),
        args.types.unwrap_or_default(),
        args.work.unwrap_or_default(),
        args.title.unwrap_or_default(),
    );
    let root = index_root()?;
    let hits = search(&root, &parsed, &filters, DEFAULT_LIMIT).map_err(map_search_err)?;
    Ok(json!({ "hits": hits }))
}

/// Run verify and serialize `VerifyReport`.
pub(super) fn run_verify(quote: String) -> Result<serde_json::Value, String> {
    let root = index_root()?;
    let report = verify(&root, &quote).map_err(map_search_err)?;
    serde_json::to_value(report).map_err(|e| e.to_string())
}

/// Run get/read/cite.
pub(super) fn run_get_passage(args: GetPassageArgs) -> Result<serde_json::Value, String> {
    let root = index_root()?;
    let action = args.action.trim().to_ascii_lowercase();
    match action.as_str() {
        "get" => {
            let line_id = args
                .line_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "get requires line_id".to_string())?;
            let radius = args.context.unwrap_or(0);
            if radius > 0 {
                match get_context(&root, line_id, radius).map_err(map_search_err)? {
                    Some(got) => Ok(get_json(&got, true)),
                    None => Err(format!("line_id not found: {line_id}")),
                }
            } else {
                match get_line(&root, line_id).map_err(map_search_err)? {
                    Some(hit) => Ok(json!({ "hit": hit })),
                    None => Err(format!("line_id not found: {line_id}")),
                }
            }
        }
        "cite" => {
            let line_id = args
                .line_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "cite requires line_id".to_string())?;
            match get_line(&root, line_id).map_err(map_search_err)? {
                Some(hit) => Ok(json!({ "citation": hit.citation, "hit": hit })),
                None => Err(format!("line_id not found: {line_id}")),
            }
        }
        "read" => {
            let work = args
                .work
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "read requires work".to_string())?;
            let juan = args.juan.ok_or_else(|| "read requires juan".to_string())?;
            let hits = list_work_juan(&root, work, juan).map_err(map_search_err)?;
            if hits.is_empty() {
                Err(format!("no lines for {work} juan {juan}"))
            } else {
                Ok(json!({ "hits": hits }))
            }
        }
        other => Err(format!(
            "unknown action {other}; expected get, read, or cite"
        )),
    }
}

/// Run catalog list.
pub(super) fn run_catalog(args: CatalogArgs) -> Result<serde_json::Value, String> {
    let filters = filters_from(
        args.canon,
        args.author.unwrap_or_default(),
        args.types.unwrap_or_default(),
        args.work.unwrap_or_default(),
        args.title.unwrap_or_default(),
    );
    let entries = load_catalog_entries(&filters)?;
    serde_json::to_value(entries).map_err(|e| e.to_string())
}

/// Run index info.
pub(super) fn run_info() -> Result<serde_json::Value, String> {
    let info = load_info()?;
    serde_json::to_value(info).map_err(|e| e.to_string())
}

fn get_json(got: &GetContext, with_window: bool) -> serde_json::Value {
    if with_window {
        let center = got.hit.line_id.as_str();
        let (before, after): (Vec<&Hit>, Vec<&Hit>) = got
            .context
            .iter()
            .partition(|h| h.line_id.as_str() < center);
        json!({
            "hit": got.hit,
            "before": before,
            "after": after,
        })
    } else {
        json!({ "hit": got.hit })
    }
}

fn filters_from(
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

fn map_search_err(e: SearchError) -> String {
    match e {
        SearchError::NoIndex(p) => {
            format!("no index found under {p}; run: cbeta build --scope <name>")
        }
        other => other.to_string(),
    }
}
