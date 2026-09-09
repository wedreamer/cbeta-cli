//! Keyword / phrase / near (Boolean AND + char-span confirm) search.

#![deny(missing_docs)]

mod confirm;
mod error;
mod hitmap;
mod open;
mod query_build;

pub use error::{Error, Result};
pub use open::{active_artifact, open_search_index};

use cbeta_core::{Filters, Hit, ParsedQuery};
use cbeta_parse::GaijiMap;
use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Term};

use crate::confirm::{confirm_near_before, needs_span_confirm};
use crate::hitmap::{collect_hits, doc_text_norm, doc_to_hit};
use crate::query_build::build_query;

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
        let doc: tantivy::schema::TantivyDocument = searcher.doc(*addr)?;
        let hit = doc_to_hit(&doc, fields, *score);
        if !crate::hitmap::passes_filters(&hit, filters) {
            continue;
        }
        let text_norm = doc_text_norm(&doc, fields);
        if confirm_near_before(&text_norm, parsed, gaiji) {
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

    fn line(id: &str, work: &str, text: &str) -> IndexableLine {
        IndexableLine {
            line: ParsedLine {
                line_id: id.into(),
                work_id: work.into(),
                juan: 1,
                text_raw: text.into(),
                lb_n: "0001a01".into(),
            },
            title: "t".into(),
            author: "a".into(),
            citation: "c".into(),
            cbeta_tag: "2026R2".into(),
        }
    }

    fn write_lines(dir: &std::path::Path, tag: &str, lines: &[IndexableLine]) {
        fs::create_dir_all(dir).unwrap();
        write_artifact(dir, "2026R2", tag, lines, &GaijiMap::default()).unwrap();
        fs::write(dir.join("CURRENT"), format!("2026R2-{tag}\n")).unwrap();
    }

    // Given close + far lines both containing 真如 and 缘起,
    // When NEAR/16, Then only the close line survives char-span confirm.
    #[test]
    fn near_char_span_confirm_drops_far_pair() {
        let dir = std::env::temp_dir().join(format!(
            "cbeta-search-span-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let close = format!("真如{}缘起", "中".repeat(5));
        let far = format!("真如{}缘起", "中".repeat(20));
        write_lines(
            &dir,
            "span1",
            &[
                line("T01n0001_p0001a01", "T0001", &close),
                line("T01n0001_p0001a02", "T0001", &far),
            ],
        );

        let pq = cbeta_core::parse_query("真如 NEAR/16 缘起").expect("parse");
        assert_eq!(pq.mode, "near");
        assert_eq!(pq.within_chars, Some(16));
        let hits = search(&dir, &pq, &Filters::default(), 10).expect("search");
        let ids: Vec<_> = hits.iter().map(|h| h.line_id.as_str()).collect();
        assert_eq!(ids, vec!["T01n0001_p0001a01"], "far pair (>16) must miss");
        let _ = fs::remove_dir_all(&dir);
    }

    // Given reverse-order line, When before 真如*缘起, Then miss;
    // forward within window hits.
    #[test]
    fn before_char_span_requires_order_and_window() {
        let dir = std::env::temp_dir().join(format!(
            "cbeta-search-before-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let forward = format!("真如{}缘起", "中".repeat(3));
        let reverse = format!("缘起{}真如", "中".repeat(3));
        write_lines(
            &dir,
            "bef1",
            &[
                line("T01n0001_p0001b01", "T0001", &forward),
                line("T01n0001_p0001b02", "T0001", &reverse),
            ],
        );

        let pq = cbeta_core::parse_query("真如*缘起").expect("parse");
        assert_eq!(pq.mode, "before");
        assert_eq!(pq.ordered, Some(true));
        let hits = search(&dir, &pq, &Filters::default(), 10).expect("search");
        let ids: Vec<_> = hits.iter().map(|h| h.line_id.as_str()).collect();
        assert_eq!(ids, vec!["T01n0001_p0001b01"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
