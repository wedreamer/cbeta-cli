//! Neighbor window and work+juan listing by sorted `line_id`.

use cbeta_core::Hit;
use cbeta_index::LineSchema;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::{IndexRecordOption, Term, Value};
use tantivy::{Searcher, TantivyDocument};

use crate::error::Result;
use crate::hitmap::doc_to_hit;
use crate::open::open_search_index;

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
    let searcher = reader.searcher();

    let Some(hit) = fetch_line(&searcher, &fields, line_id)? else {
        return Ok(None);
    };

    if radius == 0 {
        return Ok(Some(GetContext {
            hit,
            context: Vec::new(),
        }));
    }

    let mut lines = collect_work_lines(&searcher, &fields, &hit.work_id, Some(hit.juan))?;
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
    let searcher = reader.searcher();
    let mut lines = collect_work_lines(&searcher, &fields, work_id, Some(u64::from(juan)))?;
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
