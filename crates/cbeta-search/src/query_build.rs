//! Build Tantivy queries from [`cbeta_core::ParsedQuery`].

use cbeta_core::ParsedQuery;
use cbeta_index::LineSchema;
use cbeta_parse::{normalize_query, GaijiMap};
use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, TermQuery};
use tantivy::schema::{IndexRecordOption, Term};

use crate::error::Result;

/// Build a Tantivy recall query.
///
/// near/before = Boolean AND of each clause (char-span confirm is separate).
/// boolean = Must / Should / MustNot from `boolean_op` / `not_terms`.
pub fn build_query(
    parsed: &ParsedQuery,
    fields: &LineSchema,
    gaiji: &GaijiMap,
) -> Result<Box<dyn Query>> {
    let norms: Vec<String> = parsed
        .terms
        .iter()
        .map(|t| normalize_query(t, gaiji))
        .filter(|t| !t.is_empty())
        .collect();
    if norms.is_empty() {
        let raw = normalize_query(&parsed.raw, gaiji);
        return Ok(term_or_phrase(&raw, fields));
    }
    match parsed.mode.as_str() {
        "keyword" | "wildcard" | "phrase" => {
            // Contiguous string → phrase of unigrams (positions are char indices).
            Ok(term_or_phrase(&norms[0], fields))
        }
        "near" | "before" => {
            // Recall only; [`crate::confirm`] enforces the window on text_norm.
            let mut clauses = Vec::new();
            for t in &norms {
                clauses.push((Occur::Must, term_or_phrase(t, fields)));
            }
            Ok(Box::new(BooleanQuery::new(clauses)))
        }
        "boolean" => Ok(build_boolean(parsed, &norms, fields, gaiji)),
        _ => Ok(term_or_phrase(&norms[0], fields)),
    }
}

fn build_boolean(
    parsed: &ParsedQuery,
    norms: &[String],
    fields: &LineSchema,
    gaiji: &GaijiMap,
) -> Box<dyn Query> {
    let mut clauses = Vec::new();
    let occur = match parsed.boolean_op.as_deref() {
        Some("or") => Occur::Should,
        _ => Occur::Must,
    };
    for t in norms {
        clauses.push((occur, term_or_phrase(t, fields)));
    }
    for t in &parsed.not_terms {
        let n = normalize_query(t, gaiji);
        if n.is_empty() {
            continue;
        }
        clauses.push((Occur::MustNot, term_or_phrase(&n, fields)));
    }
    if clauses.is_empty() {
        return term_or_phrase("", fields);
    }
    Box::new(BooleanQuery::new(clauses))
}

fn term_or_phrase(norm: &str, fields: &LineSchema) -> Box<dyn Query> {
    let chars: Vec<char> = norm.chars().collect();
    if chars.is_empty() {
        let term = Term::from_field_text(fields.text_norm, "");
        return Box::new(TermQuery::new(term, IndexRecordOption::Basic));
    }
    if chars.len() == 1 {
        let s: String = chars.iter().collect();
        let term = Term::from_field_text(fields.text_norm, &s);
        return Box::new(TermQuery::new(
            term,
            IndexRecordOption::WithFreqsAndPositions,
        ));
    }
    // Phrase of successive unigrams — tokenizer positions are char indices 0..n-1.
    let terms: Vec<Term> = chars
        .iter()
        .map(|c| {
            let mut s = String::new();
            s.push(*c);
            Term::from_field_text(fields.text_norm, &s)
        })
        .collect();
    Box::new(PhraseQuery::new(terms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_core::ParsedQuery;
    use cbeta_index::build_line_schema;
    use cbeta_parse::GaijiMap;

    fn fields() -> cbeta_index::LineSchema {
        build_line_schema()
    }

    #[test]
    fn empty_norms_falls_back_to_raw() {
        let pq = ParsedQuery {
            raw: "空".into(),
            mode: "keyword".into(),
            terms: vec![String::new()],
            within_chars: None,
            wildcard: None,
            ordered: None,
            clauses: vec![],
            boolean_op: None,
            not_terms: vec![],
        };
        let q = build_query(&pq, &fields(), &GaijiMap::default()).unwrap();
        let _ = q;
    }

    #[test]
    fn near_and_before_build_boolean_and() {
        for mode in ["near", "before"] {
            let pq = ParsedQuery {
                raw: "a+b".into(),
                mode: mode.into(),
                terms: vec!["空".into(), "性".into()],
                within_chars: Some(30),
                wildcard: None,
                ordered: None,
                clauses: vec![],
                boolean_op: None,
                not_terms: vec![],
            };
            let q = build_query(&pq, &fields(), &GaijiMap::default()).unwrap();
            let _ = q;
        }
    }

    #[test]
    fn unknown_mode_uses_first_term() {
        let pq = ParsedQuery {
            raw: "x".into(),
            mode: "fuzzy".into(),
            terms: vec!["空性".into()],
            within_chars: None,
            wildcard: None,
            ordered: None,
            clauses: vec![],
            boolean_op: None,
            not_terms: vec![],
        };
        let _ = build_query(&pq, &fields(), &GaijiMap::default()).unwrap();
    }

    #[test]
    fn term_or_phrase_empty_and_single_char() {
        let f = fields();
        let _ = term_or_phrase("", &f);
        let _ = term_or_phrase("空", &f);
        let _ = term_or_phrase("空性", &f);
    }

    #[test]
    fn keyword_mode_phrase() {
        let pq = ParsedQuery {
            raw: "真性有為空".into(),
            mode: "keyword".into(),
            terms: vec!["真性有為空".into()],
            within_chars: None,
            wildcard: None,
            ordered: None,
            clauses: vec![],
            boolean_op: None,
            not_terms: vec![],
        };
        let _ = build_query(&pq, &fields(), &GaijiMap::default()).unwrap();
    }

    #[test]
    fn phrase_mode_uses_term_or_phrase() {
        let pq = ParsedQuery {
            raw: "缘生故如幻".into(),
            mode: "phrase".into(),
            terms: vec!["缘生故如幻".into()],
            within_chars: None,
            wildcard: None,
            ordered: None,
            clauses: vec![],
            boolean_op: None,
            not_terms: vec![],
        };
        let _ = build_query(&pq, &fields(), &GaijiMap::default()).unwrap();
    }
}
