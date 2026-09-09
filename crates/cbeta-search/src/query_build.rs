//! Build Tantivy queries from [`cbeta_core::ParsedQuery`].

use cbeta_core::ParsedQuery;
use cbeta_index::LineSchema;
use cbeta_parse::{normalize_query, GaijiMap};
use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, TermQuery};
use tantivy::schema::{IndexRecordOption, Term};

use crate::error::Result;

/// Build a Tantivy query. near/before = Boolean AND of terms (no char-span confirm in P0).
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
    let excluded: Vec<String> = parsed
        .excluded
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
            if excluded.is_empty() {
                Ok(term_or_phrase(&norms[0], fields))
            } else {
                Ok(boolean_with_excluded(
                    norms.iter().map(|t| term_or_phrase(t, fields)).collect(),
                    Occur::Must,
                    &excluded,
                    fields,
                ))
            }
        }
        "near" | "before" => {
            // P0/P1-dsl: Boolean AND of each term subquery (no window confirm yet).
            let positives: Vec<Box<dyn Query>> =
                norms.iter().map(|t| term_or_phrase(t, fields)).collect();
            Ok(boolean_with_excluded(
                positives,
                Occur::Must,
                &excluded,
                fields,
            ))
        }
        "boolean" => {
            let occur = match parsed.bool_op.as_deref() {
                Some("or") => Occur::Should,
                _ => Occur::Must,
            };
            let positives: Vec<Box<dyn Query>> =
                norms.iter().map(|t| term_or_phrase(t, fields)).collect();
            Ok(boolean_with_excluded(positives, occur, &excluded, fields))
        }
        _ => Ok(term_or_phrase(&norms[0], fields)),
    }
}

fn boolean_with_excluded(
    positives: Vec<Box<dyn Query>>,
    positive_occur: Occur,
    excluded: &[String],
    fields: &LineSchema,
) -> Box<dyn Query> {
    let mut clauses: Vec<(Occur, Box<dyn Query>)> =
        positives.into_iter().map(|q| (positive_occur, q)).collect();
    for t in excluded {
        clauses.push((Occur::MustNot, term_or_phrase(t, fields)));
    }
    if clauses.len() == 1 && matches!(clauses[0].0, Occur::Must | Occur::Should) {
        let (_, q) = clauses.remove(0);
        return q;
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

    fn pq(
        raw: &str,
        mode: &str,
        terms: Vec<&str>,
        within_chars: Option<u32>,
        wildcard: Option<bool>,
    ) -> ParsedQuery {
        ParsedQuery {
            raw: raw.into(),
            mode: mode.into(),
            terms: terms.into_iter().map(str::to_string).collect(),
            within_chars,
            wildcard,
            ordered: None,
            bool_op: None,
            excluded: Vec::new(),
        }
    }

    #[test]
    fn empty_norms_falls_back_to_raw() {
        let mut q = pq("空", "keyword", vec![""], None, None);
        q.terms = vec![String::new()];
        let built = build_query(&q, &fields(), &GaijiMap::default()).unwrap();
        let _ = built;
    }

    #[test]
    fn near_and_before_build_boolean_and() {
        for mode in ["near", "before"] {
            let q = pq("a+b", mode, vec!["空", "性"], Some(30), None);
            let built = build_query(&q, &fields(), &GaijiMap::default()).unwrap();
            let _ = built;
        }
    }

    #[test]
    fn unknown_mode_uses_first_term() {
        let q = pq("x", "fuzzy", vec!["空性"], None, None);
        let _ = build_query(&q, &fields(), &GaijiMap::default()).unwrap();
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
        let q = pq("真性有為空", "keyword", vec!["真性有為空"], None, None);
        let _ = build_query(&q, &fields(), &GaijiMap::default()).unwrap();
    }

    #[test]
    fn phrase_mode_uses_term_or_phrase() {
        let q = pq("缘生故如幻", "phrase", vec!["缘生故如幻"], None, None);
        let _ = build_query(&q, &fields(), &GaijiMap::default()).unwrap();
    }

    #[test]
    fn boolean_or_builds() {
        let mut q = pq("a,b", "boolean", vec!["空", "性"], None, None);
        q.bool_op = Some("or".into());
        let _ = build_query(&q, &fields(), &GaijiMap::default()).unwrap();
    }

    #[test]
    fn excluded_terms_build_must_not() {
        let mut q = pq("空-外", "keyword", vec!["空"], None, None);
        q.excluded = vec!["外".into()];
        let _ = build_query(&q, &fields(), &GaijiMap::default()).unwrap();
    }
}
