//! Shared search-arg parsing for MCP tools and REST `POST /search`.

use cbeta_core::{Filters, ParsedQuery};

use super::tools::SearchArgs;

/// Build `ParsedQuery` from search args (`q` or `clauses` + `mode`).
pub(crate) fn build_parsed_query(args: &SearchArgs) -> Result<ParsedQuery, String> {
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

/// Map optional filter fields into `Filters` (CLI / MCP / REST twins).
pub(crate) fn filters_from(
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::mcp::tools::SearchArgs;

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
    fn filters_from_maps_fields() {
        let f = filters_from(
            Some("T".into()),
            vec!["玄奘".into()],
            vec!["lun".into()],
            vec!["T1578".into()],
            vec!["掌珍".into()],
        );
        assert_eq!(f.canons, vec!["T"]);
        assert_eq!(f.authors, vec!["玄奘"]);
        assert_eq!(f.types, vec!["lun"]);
        assert_eq!(f.works, vec!["T1578"]);
        assert_eq!(f.titles, vec!["掌珍"]);
    }
}
