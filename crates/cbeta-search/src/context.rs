//! Neighbor window and work+juan listing by sorted `line_id`.

use cbeta_core::Hit;
use cbeta_index::LineSchema;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::{IndexRecordOption, Term, Value};
use tantivy::{Searcher, TantivyDocument};

use crate::error::Result;
use crate::hitmap::doc_to_hit;
use crate::open::{open_search_index, OpenIndex};

/// Center line plus neighbors within `radius` on the sorted `line_id` axis.
#[derive(Debug, Clone, PartialEq)]
pub struct GetContext {
    /// Exact `line_id` hit.
    pub hit: Hit,
    /// Neighbors only (excludes center), sorted by `line_id`.
    pub context: Vec<Hit>,
}

/// Cap on lines collected for one work (or work+juan) window.
const WORK_LINE_CAP: usize = 50_000;

/// Lookup `line_id` and return ±`radius` neighbors within the same `work_id`.
///
/// Neighbors are chosen by sorting stored `line_id`s — never by arithmetic on
/// lb page/col/line numbers (lines skip).
pub fn get_context(
    index_root: &std::path::Path,
    line_id: &str,
    radius: u32,
) -> Result<Option<GetContext>> {
    let (index, fields, _art) = open_search_index(index_root)?;
    let reader = index.reader()?;
    get_context_on(&reader, &fields, line_id, radius)
}

/// Context using an already-open [`OpenIndex`].
pub fn get_context_open(
    index: &OpenIndex,
    line_id: &str,
    radius: u32,
) -> Result<Option<GetContext>> {
    get_context_on(&index.reader, &index.fields, line_id, radius)
}

fn get_context_on(
    reader: &tantivy::IndexReader,
    fields: &LineSchema,
    line_id: &str,
    radius: u32,
) -> Result<Option<GetContext>> {
    let searcher = reader.searcher();

    let Some(hit) = fetch_line(&searcher, fields, line_id)? else {
        return Ok(None);
    };

    if radius == 0 {
        return Ok(Some(GetContext {
            hit,
            context: Vec::new(),
        }));
    }

    let mut lines = collect_work_lines(&searcher, fields, &hit.work_id, Some(hit.juan))?;
    lines.sort_by(|a, b| a.line_id.cmp(&b.line_id));

    let Some(pos) = lines.iter().position(|h| h.line_id == hit.line_id) else {
        return Ok(Some(GetContext {
            hit,
            context: Vec::new(),
        }));
    };

    let r = radius as usize;
    let start = pos.saturating_sub(r);
    let end = (pos + r + 1).min(lines.len());
    let mut context = Vec::with_capacity(end - start - 1);
    for (i, h) in lines.into_iter().enumerate().take(end).skip(start) {
        if i != pos {
            context.push(h);
        }
    }

    Ok(Some(GetContext { hit, context }))
}

/// All lines for `work_id` + `juan`, sorted by `line_id`.
pub fn list_work_juan(index_root: &std::path::Path, work_id: &str, juan: u32) -> Result<Vec<Hit>> {
    let (index, fields, _art) = open_search_index(index_root)?;
    let reader = index.reader()?;
    list_work_juan_on(&reader, &fields, work_id, juan)
}

/// Juan listing using an already-open [`OpenIndex`].
pub fn list_work_juan_open(index: &OpenIndex, work_id: &str, juan: u32) -> Result<Vec<Hit>> {
    list_work_juan_on(&index.reader, &index.fields, work_id, juan)
}

fn list_work_juan_on(
    reader: &tantivy::IndexReader,
    fields: &LineSchema,
    work_id: &str,
    juan: u32,
) -> Result<Vec<Hit>> {
    let searcher = reader.searcher();
    let mut lines = collect_work_lines(&searcher, fields, work_id, Some(u64::from(juan)))?;
    lines.sort_by(|a, b| a.line_id.cmp(&b.line_id));
    Ok(lines)
}

fn fetch_line(searcher: &Searcher, fields: &LineSchema, line_id: &str) -> Result<Option<Hit>> {
    let term = Term::from_field_text(fields.line_id, line_id);
    let q = TermQuery::new(term, IndexRecordOption::Basic);
    let top = searcher.search(&q, &TopDocs::with_limit(1))?;
    if top.is_empty() {
        return Ok(None);
    }
    let doc: TantivyDocument = searcher.doc(top[0].1)?;
    Ok(Some(doc_to_hit(&doc, fields, top[0].0)))
}

fn collect_work_lines(
    searcher: &Searcher,
    fields: &LineSchema,
    work_id: &str,
    juan: Option<u64>,
) -> Result<Vec<Hit>> {
    let work_term = Term::from_field_text(fields.work_id, work_id);
    let work_q = TermQuery::new(work_term, IndexRecordOption::Basic);
    let query: Box<dyn Query> = match juan {
        Some(j) => {
            let juan_term = Term::from_field_u64(fields.juan, j);
            let juan_q = TermQuery::new(juan_term, IndexRecordOption::Basic);
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, Box::new(work_q) as Box<dyn Query>),
                (Occur::Must, Box::new(juan_q) as Box<dyn Query>),
            ]))
        }
        None => Box::new(work_q),
    };
    let top = searcher.search(&*query, &TopDocs::with_limit(WORK_LINE_CAP))?;
    let mut hits = Vec::with_capacity(top.len());
    for (score, addr) in top {
        let doc: TantivyDocument = searcher.doc(addr)?;
        // Prefer stored work_id match; juan already constrained when Some.
        let wid = doc
            .get_first(fields.work_id)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if wid != work_id {
            continue;
        }
        hits.push(doc_to_hit(&doc, fields, score));
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use cbeta_index::{write_artifact, IndexableLine};
    use cbeta_parse::{GaijiMap, ParsedLine};
    use std::fs;
    use std::path::PathBuf;

    fn sample_lines() -> Vec<IndexableLine> {
        let mk = |id: &str, juan: u64, raw: &str| IndexableLine {
            line: ParsedLine {
                line_id: id.into(),
                work_id: "T1578".into(),
                juan,
                text_raw: raw.into(),
                lb_n: id.rsplit('_').next().unwrap_or("").into(),
            },
            title: "t".into(),
            author: "a".into(),
            citation: "c".into(),
            cbeta_tag: "2026R2".into(),
        };
        vec![
            mk("T30n1578_p0268b20", 1, "前"),
            mk("T30n1578_p0268b21", 1, "真性有為空"),
            mk("T30n1578_p0268b22", 1, "後"),
        ]
    }

    fn temp_root() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cbeta-ctx-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn get_context_radius_zero_and_neighbors() {
        let root = temp_root();
        let _ =
            write_artifact(&root, "2026R2", "c1", &sample_lines(), &GaijiMap::default()).unwrap();
        fs::write(root.join("CURRENT"), "2026R2-c1\n").unwrap();

        let z = get_context(&root, "T30n1578_p0268b21", 0)
            .unwrap()
            .expect("hit");
        assert!(z.context.is_empty());

        let w = get_context(&root, "T30n1578_p0268b21", 1)
            .unwrap()
            .expect("hit");
        assert_eq!(w.context.len(), 2);

        let open = OpenIndex::open(&root).unwrap();
        let o = get_context_open(&open, "T30n1578_p0268b21", 1)
            .unwrap()
            .expect("hit");
        assert_eq!(o.context.len(), 2);

        let listed = list_work_juan(&root, "T1578", 1).unwrap();
        assert_eq!(listed.len(), 3);
        let listed2 = list_work_juan_open(&open, "T1578", 1).unwrap();
        assert_eq!(listed2.len(), 3);

        assert!(get_context(&root, "ghost-line", 1).unwrap().is_none());

        let _ = fs::remove_dir_all(&root);
    }
}
