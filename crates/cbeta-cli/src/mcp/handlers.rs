//! Library-side handlers for MCP tools (blocking; call via spawn_blocking).

use std::sync::Arc;

use cbeta_core::Hit;
use cbeta_search::{
    get_context, get_context_open, get_line, get_line_open, list_work_juan, list_work_juan_open,
    search, search_open, verify, verify_open, Error as SearchError, GetContext, OpenIndex,
    DEFAULT_LIMIT,
};
use serde_json::json;

use crate::cmd_catalog::{load_catalog_entries, load_info};
use crate::env_paths::index_root;

use super::query::{build_parsed_query, filters_from};
use super::tools::{CatalogArgs, GetPassageArgs, SearchArgs};

/// Run search and return JSON `{ "hits": [...] }`.
pub(crate) fn run_search(
    args: SearchArgs,
    index: Option<Arc<OpenIndex>>,
) -> Result<serde_json::Value, String> {
    let parsed = build_parsed_query(&args)?;
    let filters = filters_from(
        args.canon,
        args.author.unwrap_or_default(),
        args.types.unwrap_or_default(),
        args.work.unwrap_or_default(),
        args.title.unwrap_or_default(),
    );
    let hits = match index.as_ref() {
        Some(idx) => search_open(idx, &parsed, &filters, DEFAULT_LIMIT).map_err(map_search_err)?,
        None => {
            let root = index_root()?;
            search(&root, &parsed, &filters, DEFAULT_LIMIT).map_err(map_search_err)?
        }
    };
    Ok(json!({ "hits": hits }))
}

/// Run verify and serialize `VerifyReport`.
pub(crate) fn run_verify(
    quote: String,
    index: Option<Arc<OpenIndex>>,
) -> Result<serde_json::Value, String> {
    let report = match index.as_ref() {
        Some(idx) => verify_open(idx, &quote).map_err(map_search_err)?,
        None => {
            let root = index_root()?;
            verify(&root, &quote).map_err(map_search_err)?
        }
    };
    serde_json::to_value(report).map_err(|e| e.to_string())
}

/// Run get/read/cite.
pub(crate) fn run_get_passage(
    args: GetPassageArgs,
    index: Option<Arc<OpenIndex>>,
) -> Result<serde_json::Value, String> {
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
                let got = match index.as_ref() {
                    Some(idx) => get_context_open(idx, line_id, radius).map_err(map_search_err)?,
                    None => {
                        let root = index_root()?;
                        get_context(&root, line_id, radius).map_err(map_search_err)?
                    }
                };
                match got {
                    Some(got) => Ok(get_json(&got, true)),
                    None => Err(format!("line_id not found: {line_id}")),
                }
            } else {
                let hit = match index.as_ref() {
                    Some(idx) => get_line_open(idx, line_id).map_err(map_search_err)?,
                    None => {
                        let root = index_root()?;
                        get_line(&root, line_id).map_err(map_search_err)?
                    }
                };
                match hit {
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
            let hit = match index.as_ref() {
                Some(idx) => get_line_open(idx, line_id).map_err(map_search_err)?,
                None => {
                    let root = index_root()?;
                    get_line(&root, line_id).map_err(map_search_err)?
                }
            };
            match hit {
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
            let hits = match index.as_ref() {
                Some(idx) => list_work_juan_open(idx, work, juan).map_err(map_search_err)?,
                None => {
                    let root = index_root()?;
                    list_work_juan(&root, work, juan).map_err(map_search_err)?
                }
            };
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
pub(crate) fn run_catalog(args: CatalogArgs) -> Result<serde_json::Value, String> {
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
pub(crate) fn run_info() -> Result<serde_json::Value, String> {
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
        assert_eq!(crate::cmd_build::run(&build, false), 0);
    }

    #[test]
    fn run_handlers_against_mini_index() {
        let _g = env_lock();
        let corpus = mini_corpus();
        let index = temp_dir("ok");
        build_mini(&corpus, &index);

        let hits = run_search(
            SearchArgs {
                q: Some("真性有为空".into()),
                work: Some(vec!["T1578".into()]),
                author: Some(vec!["玄奘".into()]),
                canon: Some("T".into()),
                types: Some(vec!["lun".into()]),
                title: Some(vec!["掌珍".into()]),
                ..empty_search()
            },
            None,
        )
        .unwrap();
        assert!(hits["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| { h["line_id"].as_str() == Some("T30n1578_p0268b21") }));

        let clauses = run_search(
            SearchArgs {
                clauses: Some(vec!["真性".into(), "有為".into()]),
                mode: Some("near".into()),
                ..empty_search()
            },
            None,
        )
        .unwrap();
        assert!(!clauses["hits"].as_array().unwrap().is_empty());

        let report = run_verify("真性有為空，如幻緣生故".into(), None).unwrap();
        assert_eq!(report["is_original"], true);

        let got = run_get_passage(
            GetPassageArgs {
                action: "get".into(),
                line_id: Some("T30n1578_p0268b21".into()),
                context: None,
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap();
        assert_eq!(got["hit"]["line_id"], "T30n1578_p0268b21");

        let ctx = run_get_passage(
            GetPassageArgs {
                action: "get".into(),
                line_id: Some("T30n1578_p0268b21".into()),
                context: Some(2),
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap();
        assert!(ctx.get("before").is_some());
        assert!(ctx.get("after").is_some());

        let cite = run_get_passage(
            GetPassageArgs {
                action: "cite".into(),
                line_id: Some("T30n1578_p0268b21".into()),
                context: None,
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap();
        assert!(cite["citation"].as_str().unwrap().contains("CBETA"));

        let read = run_get_passage(
            GetPassageArgs {
                action: "read".into(),
                line_id: None,
                context: None,
                work: Some("T0235".into()),
                juan: Some(1),
            },
            None,
        )
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

        assert!(run_get_passage(
            GetPassageArgs {
                action: "get".into(),
                line_id: None,
                context: None,
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap_err()
        .contains("requires line_id"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "get".into(),
                line_id: Some("T30n1578_p0268a12".into()),
                context: None,
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap_err()
        .contains("not found"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "get".into(),
                line_id: Some("T30n1578_p0268a12".into()),
                context: Some(1),
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap_err()
        .contains("not found"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "cite".into(),
                line_id: Some("  ".into()),
                context: None,
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap_err()
        .contains("requires line_id"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "cite".into(),
                line_id: Some("T30n1578_p0268a12".into()),
                context: None,
                work: None,
                juan: None,
            },
            None,
        )
        .unwrap_err()
        .contains("not found"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "read".into(),
                line_id: None,
                context: None,
                work: None,
                juan: Some(1),
            },
            None,
        )
        .unwrap_err()
        .contains("requires work"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "read".into(),
                line_id: None,
                context: None,
                work: Some("T0235".into()),
                juan: None,
            },
            None,
        )
        .unwrap_err()
        .contains("requires juan"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "read".into(),
                line_id: None,
                context: None,
                work: Some("T0235".into()),
                juan: Some(99),
            },
            None,
        )
        .unwrap_err()
        .contains("no lines"));

        assert!(run_get_passage(
            GetPassageArgs {
                action: "zoom".into(),
                line_id: None,
                context: None,
                work: None,
                juan: None,
            },
            None,
        )
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
        let err = run_search(
            SearchArgs {
                q: Some("空".into()),
                ..empty_search()
            },
            None,
        )
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
