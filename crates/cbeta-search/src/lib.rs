//! Keyword / phrase / near (2-gram recall + char-span confirm) search.

#![deny(missing_docs)]

mod context;
mod error;
mod hitmap;
mod open;
mod query_build;
mod span;
mod verify;

pub use context::{get_context, list_work_juan, GetContext};
pub use error::{Error, Result};
pub use open::{active_artifact, open_search_index};
pub use verify::verify;

use cbeta_core::{Filters, Hit, ParsedQuery};
use cbeta_parse::GaijiMap;
use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, TantivyDocument, Term};

use crate::hitmap::{collect_hits, doc_text_norm, doc_to_hit, passes_filters};
use crate::query_build::build_query;
use crate::span::{confirm_span, needs_span_confirm};

/// Default max hits returned to the CLI.
pub const DEFAULT_LIMIT: usize = 50;

/// Oversample factor before char-span confirm cuts to `limit`.
const CONFIRM_OVERSAMPLE: usize = 8;

/// Run a parsed query against the active artifact under `index_root`.
pub fn search(
    index_root: &std::path::Path,
    parsed: &ParsedQuery,
    filters: &Filters,
    limit: usize,
) -> Result<Vec<Hit>> {
    let (index, fields, _art) = open_search_index(index_root)?;
    let reader = index.reader()?;
    let searcher = reader.searcher();
    let gaiji = GaijiMap::default();
    let query = build_query(parsed, &fields, &gaiji)?;
    let want = limit.max(1);
    let fetch = if needs_span_confirm(&parsed.mode) {
        want.saturating_mul(CONFIRM_OVERSAMPLE).max(want)
    } else {
        want
    };
    let top = searcher.search(&*query, &TopDocs::with_limit(fetch))?;
    if needs_span_confirm(&parsed.mode) {
        return confirm_hits(&searcher, &fields, &top, filters, parsed, &gaiji, want);
    }
    let mut hits = collect_hits(&searcher, &fields, &top, filters)?;
    if hits.len() > want {
        hits.truncate(want);
    }
    Ok(hits)
}

fn confirm_hits(
    searcher: &tantivy::Searcher,
    fields: &cbeta_index::LineSchema,
    top: &[(f32, tantivy::DocAddress)],
    filters: &Filters,
    parsed: &ParsedQuery,
    gaiji: &GaijiMap,
    limit: usize,
) -> Result<Vec<Hit>> {
    let mut hits = Vec::with_capacity(limit.min(top.len()));
    for (score, addr) in top {
        if hits.len() >= limit {
            break;
        }
        let doc: TantivyDocument = searcher.doc(*addr)?;
        let hit = doc_to_hit(&doc, fields, *score);
        if !passes_filters(&hit, filters) {
            continue;
        }
        let text_norm = doc_text_norm(&doc, fields);
        if confirm_span(&text_norm, parsed, gaiji) {
            hits.push(hit);
        }
    }
    Ok(hits)
}

/// Lookup one line by exact `line_id` STRING field.
pub fn get_line(index_root: &std::path::Path, line_id: &str) -> Result<Option<Hit>> {
    let (index, fields, _art) = open_search_index(index_root)?;
    let reader = index.reader()?;
    let searcher = reader.searcher();
    let term = Term::from_field_text(fields.line_id, line_id);
    let q = TermQuery::new(term, IndexRecordOption::Basic);
    let top = searcher.search(&q, &TopDocs::with_limit(1))?;
    if top.is_empty() {
        return Ok(None);
    }
    let doc = searcher.doc(top[0].1)?;
    Ok(Some(doc_to_hit(&doc, &fields, top[0].0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_index::{add_lines, create_ram_index, write_artifact, IndexableLine};
    use cbeta_parse::ParsedLine;
    use std::fs;

    fn sample() -> Vec<IndexableLine> {
        vec![IndexableLine {
            line: ParsedLine {
                line_id: "T30n1578_p0268b21".into(),
                work_id: "T1578".into(),
                juan: 1,
                text_raw: "真性有為空".into(),
                lb_n: "0268b21".into(),
            },
            title: "大乘掌珍論".into(),
            author: "清辯菩薩,玄奘".into(),
            citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)".into(),
            cbeta_tag: "2026R2".into(),
        }]
    }

    #[test]
    fn ram_keyword_simplified_hits_traditional() {
        let (index, fields) = create_ram_index().expect("ram");
        let mut writer = index.writer(15_000_000).expect("w");
        add_lines(&mut writer, &fields, &sample(), &GaijiMap::default()).expect("add");
        writer.commit().expect("c");
        // Write to disk for open_search_index path is heavier; exercise build_query via search on disk.
        let dir = std::env::temp_dir().join(format!(
            "cbeta-search-ut-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("d");
        let art = write_artifact(&dir, "2026R2", "testhash", &sample(), &GaijiMap::default())
            .expect("write");
        fs::write(dir.join("CURRENT"), "2026R2-testhash\n").expect("cur");
        let _ = art;

        let pq = cbeta_core::parse_query("真性有为空").expect("parse");
        let hits = search(&dir, &pq, &Filters::default(), 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line_id, "T30n1578_p0268b21");

        let got = get_line(&dir, "T30n1578_p0268b21").expect("get");
        assert!(got.is_some());
        let miss = get_line(&dir, "T30n1578_p0268a12").expect("miss");
        assert!(miss.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ram_near_and_before_and_filters() {
        let dir = std::env::temp_dir().join(format!(
            "cbeta-search-near-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        write_artifact(&dir, "2026R2", "nearh", &sample(), &GaijiMap::default()).unwrap();
        fs::write(dir.join("CURRENT"), "2026R2-nearh\n").unwrap();

        let near = cbeta_core::parse_query("真性+有為").expect("near");
        assert_eq!(near.mode, "near");
        let hits = search(&dir, &near, &Filters::default(), 10).unwrap();
        assert_eq!(hits.len(), 1);

        let before = cbeta_core::parse_query("真性*有為").expect("before");
        assert_eq!(before.mode, "before");
        let hits2 = search(&dir, &before, &Filters::default(), 10).unwrap();
        assert!(!hits2.is_empty());

        let filtered = search(
            &dir,
            &near,
            &Filters {
                works: vec!["T0235".into()],
                ..Filters::default()
            },
            10,
        )
        .unwrap();
        assert!(filtered.is_empty());

        assert!(search(&dir, &near, &Filters::default(), 0).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    fn plant_near_lines() -> Vec<IndexableLine> {
        let meta = |line_id: &str, text: &str| IndexableLine {
            line: ParsedLine {
                line_id: line_id.into(),
                work_id: "T1578".into(),
                juan: 1,
                text_raw: text.into(),
                lb_n: "0001a01".into(),
            },
            title: "大乘掌珍論".into(),
            author: "清辯菩薩,玄奘".into(),
            citation: "(CBETA 2026.R2, T30, no. 1578, p. 1, a01)".into(),
            cbeta_tag: "2026R2".into(),
        };
        vec![
            // covering span 2+5+2 = 9 ≤ 16
            meta("T30n1578_p0001a01", &format!("真如{}缘起", "中".repeat(5))),
            // covering span 2+20+2 = 24 > 16
            meta("T30n1578_p0001a02", &format!("真如{}缘起", "中".repeat(20))),
            // reverse order for BEFORE
            meta("T30n1578_p0001a03", &format!("缘起{}真如", "中".repeat(3))),
            // + / NEAR/30 close
            meta("T30n1578_p0001a04", &format!("空性{}缘生", "中".repeat(20))),
            {
                let mut s = sample();
                s.remove(0)
            },
        ]
    }

    #[test]
    fn span_near_16_accepts_via_search() {
        let dir = std::env::temp_dir().join(format!(
            "cbeta-span-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        write_artifact(
            &dir,
            "2026R2",
            "spanok",
            &plant_near_lines(),
            &GaijiMap::default(),
        )
        .unwrap();
        fs::write(dir.join("CURRENT"), "2026R2-spanok\n").unwrap();
        let pq = cbeta_core::parse_query("真如 NEAR/16 缘起").expect("parse");
        let hits = search(&dir, &pq, &Filters::default(), 10).unwrap();
        assert!(
            hits.iter().any(|h| h.line_id == "T30n1578_p0001a01"),
            "expected close line; got {:?}",
            hits.iter().map(|h| &h.line_id).collect::<Vec<_>>()
        );
        assert!(
            !hits.iter().any(|h| h.line_id == "T30n1578_p0001a02"),
            "far line must be dropped by span"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_errors_without_index() {
        let dir = std::env::temp_dir().join(format!(
            "cbeta-search-empty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        let pq = cbeta_core::parse_query("空").unwrap();
        assert!(matches!(
            search(&dir, &pq, &Filters::default(), 5),
            Err(Error::NoIndex(_))
        ));
        assert!(matches!(get_line(&dir, "x"), Err(Error::NoIndex(_))));
        let _ = fs::remove_dir_all(&dir);
    }

    fn line(id: &str, text: &str) -> IndexableLine {
        let lb = id
            .split_once("_p")
            .map(|(_, r)| r.to_string())
            .unwrap_or_default();
        IndexableLine {
            line: ParsedLine {
                line_id: id.into(),
                work_id: "T1578".into(),
                juan: 1,
                text_raw: text.into(),
                lb_n: lb,
            },
            title: "大乘掌珍論".into(),
            author: "清辯菩薩,玄奘".into(),
            citation: format!(
                "(CBETA 2026.R2, T30, no. 1578, p. 268, {})",
                &id[id.len().saturating_sub(3)..]
            ),
            cbeta_tag: "2026R2".into(),
        }
    }

    fn neighbor_fixture() -> Vec<IndexableLine> {
        // Non-contiguous lb numbers: arithmetic on b21 must NOT invent b22.
        vec![
            line("T30n1578_p0268b10", "鄰前四"),
            line("T30n1578_p0268b12", "鄰前三"),
            line("T30n1578_p0268b14", "鄰前二"),
            line("T30n1578_p0268b16", "鄰前一"),
            line("T30n1578_p0268b21", "真性有為空"),
            line("T30n1578_p0268b30", "鄰後一"),
            line("T30n1578_p0268b32", "鄰後二"),
            line("T30n1578_p0268b34", "鄰後三"),
            line("T30n1578_p0268b36", "鄰後四"),
            line("T30n1578_p0268b40", "鄰後五"),
        ]
    }

    fn write_fixture(lines: &[IndexableLine], tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cbeta-search-ctx-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        write_artifact(&dir, "2026R2", tag, lines, &GaijiMap::default()).unwrap();
        fs::write(dir.join("CURRENT"), format!("2026R2-{tag}\n")).unwrap();
        dir
    }

    #[test]
    fn get_context_radius_4_includes_neighbors() {
        // Given: sorted line_ids with gaps around b21
        let dir = write_fixture(&neighbor_fixture(), "nbr");
        // When: radius 4 around the real verse line
        let got = get_context(&dir, "T30n1578_p0268b21", 4)
            .expect("ok")
            .expect("found");
        // Then: center hit + 4 before + 4 after by sorted line_id (not arithmetic)
        assert_eq!(got.hit.line_id, "T30n1578_p0268b21");
        assert_eq!(got.context.len(), 8);
        let ids: Vec<_> = got.context.iter().map(|h| h.line_id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "T30n1578_p0268b10",
                "T30n1578_p0268b12",
                "T30n1578_p0268b14",
                "T30n1578_p0268b16",
                "T30n1578_p0268b30",
                "T30n1578_p0268b32",
                "T30n1578_p0268b34",
                "T30n1578_p0268b36",
            ]
        );
        assert!(!ids.iter().any(|id| id.contains("b22")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_line_missing_is_none() {
        // Given: index with only b21 neighbors fixture
        let dir = write_fixture(&neighbor_fixture(), "miss");
        // When: ghost a12 (negative only)
        let miss = get_context(&dir, "T30n1578_p0268a12", 4).expect("ok");
        // Then: None, not an error
        assert!(miss.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_work_juan_t0235() {
        // Given: T0235 juan 1 lines mixed with T1578
        let mut lines = neighbor_fixture();
        lines.push(IndexableLine {
            line: ParsedLine {
                line_id: "T08n0235_p0748c17".into(),
                work_id: "T0235".into(),
                juan: 1,
                text_raw: "如是我聞".into(),
                lb_n: "0748c17".into(),
            },
            title: "金剛般若波羅蜜經".into(),
            author: "鳩摩羅什".into(),
            citation: "(CBETA 2026.R2, T08, no. 235, p. 748, c17)".into(),
            cbeta_tag: "2026R2".into(),
        });
        lines.push(IndexableLine {
            line: ParsedLine {
                line_id: "T08n0235_p0748c18".into(),
                work_id: "T0235".into(),
                juan: 1,
                text_raw: "一時佛在".into(),
                lb_n: "0748c18".into(),
            },
            title: "金剛般若波羅蜜經".into(),
            author: "鳩摩羅什".into(),
            citation: "(CBETA 2026.R2, T08, no. 235, p. 748, c18)".into(),
            cbeta_tag: "2026R2".into(),
        });
        lines.push(IndexableLine {
            line: ParsedLine {
                line_id: "T08n0235_p0750a01".into(),
                work_id: "T0235".into(),
                juan: 2,
                text_raw: "他卷".into(),
                lb_n: "0750a01".into(),
            },
            title: "金剛般若波羅蜜經".into(),
            author: "鳩摩羅什".into(),
            citation: "(CBETA 2026.R2, T08, no. 235, p. 750, a01)".into(),
            cbeta_tag: "2026R2".into(),
        });
        let dir = write_fixture(&lines, "t0235");
        // When: list work+juan
        let hits = list_work_juan(&dir, "T0235", 1).expect("list");
        // Then: only juan 1, sorted by line_id
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].line_id, "T08n0235_p0748c17");
        assert_eq!(hits[1].line_id, "T08n0235_p0748c18");
        assert!(hits.iter().all(|h| h.work_id == "T0235" && h.juan == 1));
        let _ = fs::remove_dir_all(&dir);
    }
}
