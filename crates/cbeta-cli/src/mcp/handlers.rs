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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::env_paths::env_lock;
    use crate::mcp::tools::{CatalogArgs, GetPassageArgs, SearchArgs};
    use cbeta_core::{Action, Command, Filters, Format, Hit};
    use cbeta_search::GetContext;
    use std::path::PathBuf;

    fn mini_corpus() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini")
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-mcp-h-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn build_mini(corpus: &std::path::Path, index: &std::path::Path) {
        std::env::set_var("CBETA_CORPUS", corpus);
        std::env::set_var("CBETA_INDEX", index);
        let build = Command {
            action: Action::Build,
            q: Some("ci-minimal".into()),
            filters: Filters::default(),
            format: Format::Plain,
            explain: false,
            parsed_query: None,
            context: None,
            copy: false,
        };
        assert_eq!(crate::cmd_build::run(&build), 0);
    }

    #[test]
    fn build_parsed_query_clauses_modes() {
        let near = build_parsed_query(&SearchArgs {
            clauses: Some(vec!["真性".into(), " 有為 ".into(), "".into()]),
            mode: Some("NEAR".into()),
            ..empty_search()
        })
        .unwrap();
        assert_eq!(near.mode, "near");
        assert_eq!(near.ordered, Some(false));
        assert_eq!(near.within_chars, Some(30));
        assert_eq!(near.terms, vec!["真性", "有為"]);

        let before = build_parsed_query(&SearchArgs {
            clauses: Some(vec!["空".into(), "缘".into()]),
            mode: Some("before".into()),
            ..empty_search()
        })
        .unwrap();
        assert_eq!(before.mode, "before");
        assert_eq!(before.ordered, Some(true));

        let kw = build_parsed_query(&SearchArgs {
            clauses: Some(vec!["空性".into()]),
            mode: Some("keyword".into()),
            ..empty_search()
        })
        .unwrap();
        assert_eq!(kw.mode, "keyword");
        assert!(kw.within_chars.is_none());

        let phrase = build_parsed_query(&SearchArgs {
            clauses: Some(vec!["如幻".into(), "缘生".into()]),
            mode: Some("phrase".into()),
            ..empty_search()
        })
        .unwrap();
        assert_eq!(phrase.mode, "phrase");

        let default_near = build_parsed_query(&SearchArgs {
            clauses: Some(vec!["a".into(), "b".into()]),
            mode: None,
            ..empty_search()
        })
        .unwrap();
        assert_eq!(default_near.mode, "near");

        let err = build_parsed_query(&SearchArgs {
            clauses: Some(vec!["a".into()]),
            mode: Some("fuzzy".into()),
            ..empty_search()
        })
        .unwrap_err();
        assert!(err.contains("unknown mode"));
    }

    #[test]
    fn build_parsed_query_from_q_and_mode_override() {
        let p = build_parsed_query(&SearchArgs {
            q: Some(" 真性有为空 ".into()),
            mode: Some("phrase".into()),
            ..empty_search()
        })
        .unwrap();
        assert_eq!(p.mode, "phrase");

        let near_keep = build_parsed_query(&SearchArgs {
            q: Some("空性+缘生".into()),
            mode: Some("near".into()),
            ..empty_search()
        })
        .unwrap();
        assert_eq!(near_keep.mode, "near");

        let bad = build_parsed_query(&SearchArgs {
            q: Some("空".into()),
            mode: Some("semantic".into()),
            ..empty_search()
        })
        .unwrap_err();
        assert!(bad.contains("unknown mode"));

        let missing = build_parsed_query(&SearchArgs {
            q: Some("  ".into()),
            ..empty_search()
        })
        .unwrap_err();
        assert!(missing.contains("requires q or clauses"));

        let parse_err = build_parsed_query(&SearchArgs {
            q: Some("空性＋缘生".into()),
            ..empty_search()
        })
        .unwrap_err();
        assert!(parse_err.contains("fullwidth"));
    }

    #[test]
    fn run_handlers_against_mini_index() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("ok");
        build_mini(&corpus, &index);

        let hits = run_search(SearchArgs {
            q: Some("真性有为空".into()),
            work: Some(vec!["T1578".into()]),
            author: Some(vec!["玄奘".into()]),
            canon: Some("T".into()),
            types: Some(vec!["lun".into()]),
            title: Some(vec!["掌珍".into()]),
            ..empty_search()
        })
        .unwrap();
        assert!(hits["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| { h["line_id"].as_str() == Some("T30n1578_p0268b21") }));

        let clauses = run_search(SearchArgs {
            clauses: Some(vec!["真性".into(), "有為".into()]),
            mode: Some("near".into()),
            ..empty_search()
        })
        .unwrap();
        assert!(!clauses["hits"].as_array().unwrap().is_empty());

        let report = run_verify("真性有為空，如幻緣生故".into()).unwrap();
        assert_eq!(report["is_original"], true);

        let got = run_get_passage(GetPassageArgs {
            action: "get".into(),
            line_id: Some("T30n1578_p0268b21".into()),
            context: None,
            work: None,
            juan: None,
        })
        .unwrap();
        assert_eq!(got["hit"]["line_id"], "T30n1578_p0268b21");

        let ctx = run_get_passage(GetPassageArgs {
            action: "get".into(),
            line_id: Some("T30n1578_p0268b21".into()),
            context: Some(2),
            work: None,
            juan: None,
        })
        .unwrap();
        assert!(ctx.get("before").is_some());
        assert!(ctx.get("after").is_some());

        let cite = run_get_passage(GetPassageArgs {
            action: "cite".into(),
            line_id: Some("T30n1578_p0268b21".into()),
            context: None,
            work: None,
            juan: None,
        })
        .unwrap();
        assert!(cite["citation"].as_str().unwrap().contains("CBETA"));

        let read = run_get_passage(GetPassageArgs {
            action: "read".into(),
            line_id: None,
            context: None,
            work: Some("T0235".into()),
            juan: Some(1),
        })
        .unwrap();
        assert!(!read["hits"].as_array().unwrap().is_empty());

        let cat = run_catalog(CatalogArgs {
            work: Some(vec!["T1578".into()]),
            author: Some(vec!["玄奘".into()]),
            canon: Some("T".into()),
            types: Some(vec!["lun".into()]),
            title: Some(vec!["掌珍".into()]),
        })
        .unwrap();
        assert!(!cat.as_array().unwrap().is_empty());

        let info = run_info().unwrap();
        assert!(info.get("cbeta_tag").is_some() || info.get("artifact_id").is_some());

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }

    #[test]
    fn run_get_passage_error_paths() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("get-err");
        build_mini(&corpus, &index);

        assert!(run_get_passage(GetPassageArgs {
            action: "get".into(),
            line_id: None,
            context: None,
            work: None,
            juan: None,
        })
        .unwrap_err()
        .contains("requires line_id"));

        assert!(run_get_passage(GetPassageArgs {
            action: "get".into(),
            line_id: Some("T30n1578_p0268a12".into()),
            context: None,
            work: None,
            juan: None,
        })
        .unwrap_err()
        .contains("not found"));

        assert!(run_get_passage(GetPassageArgs {
            action: "get".into(),
            line_id: Some("T30n1578_p0268a12".into()),
            context: Some(1),
            work: None,
            juan: None,
        })
        .unwrap_err()
        .contains("not found"));

        assert!(run_get_passage(GetPassageArgs {
            action: "cite".into(),
            line_id: Some("  ".into()),
            context: None,
            work: None,
            juan: None,
        })
        .unwrap_err()
        .contains("requires line_id"));

        assert!(run_get_passage(GetPassageArgs {
            action: "cite".into(),
            line_id: Some("T30n1578_p0268a12".into()),
            context: None,
            work: None,
            juan: None,
        })
        .unwrap_err()
        .contains("not found"));

        assert!(run_get_passage(GetPassageArgs {
            action: "read".into(),
            line_id: None,
            context: None,
            work: None,
            juan: Some(1),
        })
        .unwrap_err()
        .contains("requires work"));

        assert!(run_get_passage(GetPassageArgs {
            action: "read".into(),
            line_id: None,
            context: None,
            work: Some("T0235".into()),
            juan: None,
        })
        .unwrap_err()
        .contains("requires juan"));

        assert!(run_get_passage(GetPassageArgs {
            action: "read".into(),
            line_id: None,
            context: None,
            work: Some("T0235".into()),
            juan: Some(99),
        })
        .unwrap_err()
        .contains("no lines"));

        assert!(run_get_passage(GetPassageArgs {
            action: "zoom".into(),
            line_id: None,
            context: None,
            work: None,
            juan: None,
        })
        .unwrap_err()
        .contains("unknown action"));

        std::env::remove_var("CBETA_CORPUS");
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&index);
    }

    #[test]
    fn run_search_and_info_no_index() {
        let _g = env_lock();
        let empty = temp_dir("noidx");
        std::env::set_var("CBETA_INDEX", &empty);
        let err = run_search(SearchArgs {
            q: Some("空".into()),
            ..empty_search()
        })
        .unwrap_err();
        assert!(err.contains("no index") || err.contains("index"));
        let _ = run_info();
        std::env::remove_var("CBETA_INDEX");
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn get_json_and_map_search_err_helpers() {
        let hit = sample_hit("T30n1578_p0268b21");
        let before = sample_hit("T30n1578_p0268b20");
        let after = sample_hit("T30n1578_p0268b22");
        let got = GetContext {
            hit: hit.clone(),
            context: vec![before, after],
        };
        let with = get_json(&got, true);
        assert_eq!(with["hit"]["line_id"], "T30n1578_p0268b21");
        assert_eq!(with["before"].as_array().unwrap().len(), 1);
        assert_eq!(with["after"].as_array().unwrap().len(), 1);
        let bare = get_json(&got, false);
        assert!(bare.get("before").is_none());
        assert_eq!(bare["hit"]["line_id"], "T30n1578_p0268b21");

        let no_idx = map_search_err(SearchError::NoIndex("/tmp/x".into()));
        assert!(no_idx.contains("no index found"));
        let other = map_search_err(SearchError::Schema("boom".into()));
        assert!(other.contains("boom"));

        let f = filters_from(
            Some("T".into()),
            vec!["玄奘".into()],
            vec!["lun".into()],
            vec!["T1578".into()],
            vec!["掌珍".into()],
        );
        assert_eq!(f.canons, vec!["T"]);
        assert_eq!(f.authors, vec!["玄奘"]);
    }

    fn empty_search() -> SearchArgs {
        SearchArgs {
            q: None,
            mode: None,
            clauses: None,
            work: None,
            author: None,
            canon: None,
            types: None,
            title: None,
        }
    }

    fn sample_hit(line_id: &str) -> Hit {
        Hit {
            line_id: line_id.into(),
            work_id: "T1578".into(),
            title: "大乘掌珍論".into(),
            author: "玄奘".into(),
            juan: 1,
            text_raw: "真性有為空".into(),
            citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)".into(),
            score: 1.0,
            cbeta_tag: "2026R2".into(),
        }
    }
}
