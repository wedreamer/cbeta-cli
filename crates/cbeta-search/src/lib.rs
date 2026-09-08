//! Keyword / phrase / near-as-AND search over a CBETA Tantivy index.

#![deny(missing_docs)]

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

use crate::hitmap::{collect_hits, doc_to_hit};
use crate::query_build::build_query;

/// Default max hits returned to the CLI.
pub const DEFAULT_LIMIT: usize = 50;

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
    let top = searcher.search(&*query, &TopDocs::with_limit(limit.max(1)))?;
    collect_hits(&searcher, &fields, &top, filters)
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
}
