//! Map Tantivy docs to [`cbeta_core::Hit`] and apply [`Filters`].

use cbeta_core::{Filters, Hit};
use cbeta_index::LineSchema;
use tantivy::schema::{TantivyDocument, Value};
use tantivy::Searcher;

use crate::error::Result;

/// Read stored fields into a [`Hit`].
pub fn doc_to_hit(doc: &TantivyDocument, fields: &LineSchema, score: f32) -> Hit {
    let text = |f| {
        doc.get_first(f)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let juan = doc
        .get_first(fields.juan)
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Hit {
        line_id: text(fields.line_id),
        work_id: text(fields.work_id),
        title: text(fields.title),
        author: text(fields.author),
        juan,
        text_raw: text(fields.text_raw),
        citation: text(fields.citation),
        score,
        cbeta_tag: text(fields.cbeta_tag),
    }
}

/// Stored normalized line text used for near/before char-span confirm.
pub fn doc_text_norm(doc: &TantivyDocument, fields: &LineSchema) -> String {
    doc.get_first(fields.text_norm)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// True when hit passes all non-empty filter axes (substring for author/title).
pub fn passes_filters(hit: &Hit, filters: &Filters) -> bool {
    if !filters.works.is_empty() && !filters.works.iter().any(|w| w == &hit.work_id) {
        return false;
    }
    if !filters.authors.is_empty()
        && !filters
            .authors
            .iter()
            .any(|a| hit.author.contains(a.as_str()))
    {
        return false;
    }
    if !filters.titles.is_empty()
        && !filters
            .titles
            .iter()
            .any(|t| hit.title.contains(t.as_str()))
    {
        return false;
    }
    if !filters.juans.is_empty() && !filters.juans.iter().any(|j| u64::from(*j) == hit.juan) {
        return false;
    }
    if !filters.canons.is_empty() {
        // work_id like T1578 → canon letter prefix before digits.
        let canon = hit
            .work_id
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect::<String>();
        if !filters.canons.iter().any(|c| c == &canon) {
            return false;
        }
    }
    // types not stored on line docs in P0 — skip silently.
    let _ = &filters.types;
    let _ = &filters.categories;
    true
}

/// Collect top docs into filtered hits.
pub fn collect_hits(
    searcher: &Searcher,
    fields: &LineSchema,
    top: &[(f32, tantivy::DocAddress)],
    filters: &Filters,
) -> Result<Vec<Hit>> {
    let mut hits = Vec::with_capacity(top.len());
    for (score, addr) in top {
        let doc: TantivyDocument = searcher.doc(*addr)?;
        let hit = doc_to_hit(&doc, fields, *score);
        if passes_filters(&hit, filters) {
            hits.push(hit);
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(work: &str, author: &str, title: &str, juan: u64) -> Hit {
        Hit {
            line_id: "L".into(),
            work_id: work.into(),
            title: title.into(),
            author: author.into(),
            juan,
            text_raw: "t".into(),
            citation: "c".into(),
            score: 1.0,
            cbeta_tag: "2026R2".into(),
        }
    }

    #[test]
    fn passes_filters_all_axes() {
        let h = hit("T1578", "清辯菩薩,玄奘", "大乘掌珍論", 1);
        assert!(passes_filters(&h, &Filters::default()));
        assert!(!passes_filters(
            &h,
            &Filters {
                works: vec!["T0235".into()],
                ..Filters::default()
            }
        ));
        assert!(passes_filters(
            &h,
            &Filters {
                works: vec!["T1578".into()],
                ..Filters::default()
            }
        ));
        assert!(!passes_filters(
            &h,
            &Filters {
                authors: vec!["羅什".into()],
                ..Filters::default()
            }
        ));
        assert!(passes_filters(
            &h,
            &Filters {
                authors: vec!["玄奘".into()],
                ..Filters::default()
            }
        ));
        assert!(!passes_filters(
            &h,
            &Filters {
                titles: vec!["金剛".into()],
                ..Filters::default()
            }
        ));
        assert!(passes_filters(
            &h,
            &Filters {
                titles: vec!["掌珍".into()],
                ..Filters::default()
            }
        ));
        assert!(!passes_filters(
            &h,
            &Filters {
                juans: vec![2],
                ..Filters::default()
            }
        ));
        assert!(passes_filters(
            &h,
            &Filters {
                juans: vec![1],
                ..Filters::default()
            }
        ));
        assert!(!passes_filters(
            &h,
            &Filters {
                canons: vec!["X".into()],
                ..Filters::default()
            }
        ));
        assert!(passes_filters(
            &h,
            &Filters {
                canons: vec!["T".into()],
                types: vec!["lun".into()],
                categories: vec!["x".into()],
                ..Filters::default()
            }
        ));
    }
}
