//! CBReader-style query DSL parsing.

use serde::{Deserialize, Serialize};

/// Failure from [`parse_query`] (hand-rolled; not `thiserror` yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseError {}

/// One clause in a structured near/before/boolean query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clause {
    /// Clause text (pre-normalization display form from the DSL).
    pub text: String,
    /// Optional per-clause window (normalized 汉字).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within_chars: Option<u32>,
    /// Optional per-clause order constraint (`true` = before).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
}

/// Structured view of a human/agent query string after [`parse_query`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedQuery {
    /// Original trimmed input.
    pub raw: String,
    /// DSL mode: `keyword`, `near`, `before`, `wildcard`, or `boolean`.
    pub mode: String,
    /// Term list derived from the DSL (order preserved).
    pub terms: Vec<String>,
    /// Character window for near/before (normalized 汉字, not tokens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within_chars: Option<u32>,
    /// Set when `?` single-char wildcard mode is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wildcard: Option<bool>,
    /// Near/before order: `Some(false)` near, `Some(true)` before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
    /// Structured clauses mirrored from [`Self::terms`] (near/before/boolean).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clauses: Vec<Clause>,
    /// Boolean combinator when `mode == "boolean"`: `and` or `or`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boolean_op: Option<String>,
    /// Terms excluded by CBReader `-` NOT.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_terms: Vec<String>,
}

/// Parse CBReader-style query DSL into a [`ParsedQuery`].
///
/// Distance is always **normalized 汉字**, never tokens. Operators:
/// - `+` → `near` with window 30, `ordered = false`
/// - `*` → `before` with window 30, `ordered = true`
/// - `NEAR/N` / `BEFORE/N` → window `N` with matching ordered flag
/// - `&` → `boolean` / `and`; `,` → `boolean` / `or`; `-` → `not_terms`
/// - `?` → single-char wildcard mode
///
/// Fullwidth operators in the reject set `—＋＊＆？` error out (halfwidth only).
/// Fullwidth comma `，` is **not** rejected. Mixing `&` and `,` is a [`ParseError`].
pub fn parse_query(q: &str) -> Result<ParsedQuery, ParseError> {
    let raw = q.trim().to_string();
    if raw.contains('—')
        || raw.contains('＋')
        || raw.contains('＊')
        || raw.contains('＆')
        || raw.contains('？')
    {
        return Err(ParseError(
            "fullwidth operator rejected; use halfwidth + * & , - ? or NEAR/N".into(),
        ));
    }
    if let Some(parsed) = parse_near_like(&raw, "NEAR", "near", false) {
        return Ok(parsed);
    }
    if let Some(parsed) = parse_near_like(&raw, "BEFORE", "before", true) {
        return Ok(parsed);
    }
    if raw.contains('+') {
        let terms: Vec<String> = raw
            .split('+')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        // RED scaffold: ordered/clauses filled in feat commit.
        return Ok(ParsedQuery {
            raw,
            mode: "near".into(),
            terms,
            within_chars: Some(30),
            wildcard: None,
            ordered: None,
            clauses: vec![],
            boolean_op: None,
            not_terms: vec![],
        });
    }
    if raw.contains('*') && !raw.contains('?') {
        let terms: Vec<String> = raw
            .split('*')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok(ParsedQuery {
            raw,
            mode: "before".into(),
            terms,
            within_chars: Some(30),
            wildcard: None,
            ordered: None,
            clauses: vec![],
            boolean_op: None,
            not_terms: vec![],
        });
    }
    // Boolean aliases: single operator among & / , ; - fills not_terms.
    // Intentionally not implemented yet (RED tests).
    if raw.contains('?') {
        return Ok(ParsedQuery {
            raw: raw.clone(),
            mode: "wildcard".into(),
            terms: vec![raw],
            within_chars: None,
            wildcard: Some(true),
            ordered: None,
            clauses: vec![],
            boolean_op: None,
            not_terms: vec![],
        });
    }
    Ok(ParsedQuery {
        raw: raw.clone(),
        mode: "keyword".into(),
        terms: vec![raw],
        within_chars: None,
        wildcard: None,
        ordered: None,
        clauses: vec![],
        boolean_op: None,
        not_terms: vec![],
    })
}

fn parse_near_like(raw: &str, token: &str, mode: &str, _ordered: bool) -> Option<ParsedQuery> {
    let needle = format!(" {token}/");
    let idx = raw.find(&needle)?;
    let left = raw[..idx].trim().to_string();
    let rest = raw[idx + needle.len()..].trim();
    let (n, right) = rest.split_once(' ')?;
    let within: u32 = n.parse().ok()?;
    Some(ParsedQuery {
        raw: raw.to_string(),
        mode: mode.into(),
        terms: vec![left, right.trim().to_string()],
        within_chars: Some(within),
        wildcard: None,
        ordered: None,
        clauses: vec![],
        boolean_op: None,
        not_terms: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_is_near_30() {
        let p = parse_query("空性+缘生").unwrap();
        assert_eq!(p.mode, "near");
        assert_eq!(p.within_chars, Some(30));
        assert_eq!(p.terms, vec!["空性", "缘生"]);
    }

    #[test]
    fn near_16() {
        let p = parse_query("真如 NEAR/16 缘起").unwrap();
        assert_eq!(p.mode, "near");
        assert_eq!(p.within_chars, Some(16));
        assert_eq!(p.terms, vec!["真如", "缘起"]);
    }

    #[test]
    fn before_8() {
        let p = parse_query("空性 BEFORE/8 缘起").unwrap();
        assert_eq!(p.mode, "before");
        assert_eq!(p.within_chars, Some(8));
    }

    #[test]
    fn lotus_wildcard() {
        let p = parse_query("莲?色").unwrap();
        assert_eq!(p.mode, "wildcard");
    }

    #[test]
    fn reject_fullwidth_plus() {
        assert!(parse_query("空性＋缘生").is_err());
    }

    #[test]
    fn star_is_before_30() {
        let p = parse_query("空性*缘生").unwrap();
        assert_eq!(p.mode, "before");
        assert_eq!(p.within_chars, Some(30));
        assert_eq!(p.terms, vec!["空性", "缘生"]);
    }

    #[test]
    fn keyword_plain() {
        let p = parse_query("色即是空").unwrap();
        assert_eq!(p.mode, "keyword");
    }

    #[test]
    fn reject_fullwidth_ops() {
        // Fullwidth set: — ＋ ＊ ＆ ？ (＋ covered by reject_fullwidth_plus).
        // Fullwidth comma ， is intentionally not locked here.
        for q in ["空性—缘生", "空性＊缘生", "空性＆缘生", "空性？缘生"] {
            assert!(parse_query(q).is_err(), "expected reject for {q}");
        }
    }

    #[test]
    fn parse_error_display_contains_fullwidth() {
        let err = parse_query("空性＋缘生").unwrap_err();
        assert!(err.to_string().contains("fullwidth"), "Display was: {err}");
    }

    // --- P1: ordered + clauses + boolean DSL (Given/When/Then) ---

    #[test]
    fn plus_near_fills_ordered_false_and_clauses() {
        // Given: CBReader + near alias
        // When: parse
        let p = parse_query("空性+缘生").unwrap();
        // Then: unordered near with two clauses
        assert_eq!(p.ordered, Some(false));
        assert_eq!(p.clauses.len(), 2);
        assert_eq!(p.clauses[0].text, "空性");
        assert_eq!(p.clauses[1].text, "缘生");
        assert_eq!(p.clauses[0].within_chars, Some(30));
        assert_eq!(p.clauses[0].ordered, Some(false));
    }

    #[test]
    fn star_before_fills_ordered_true_and_clauses() {
        // Given: * before alias
        let p = parse_query("空性*缘生").unwrap();
        // Then
        assert_eq!(p.ordered, Some(true));
        assert_eq!(p.mode, "before");
        assert_eq!(p.clauses.len(), 2);
        assert_eq!(p.clauses[1].ordered, Some(true));
    }

    #[test]
    fn near_n_fills_ordered_false_window_on_clauses() {
        // Given: NEAR/16
        let p = parse_query("真如 NEAR/16 缘起").unwrap();
        // Then
        assert_eq!(p.ordered, Some(false));
        assert_eq!(p.within_chars, Some(16));
        assert_eq!(p.clauses.len(), 2);
        assert_eq!(p.clauses[0].text, "真如");
        assert_eq!(p.clauses[1].within_chars, Some(16));
    }

    #[test]
    fn before_n_fills_ordered_true() {
        let p = parse_query("空性 BEFORE/8 缘起").unwrap();
        assert_eq!(p.ordered, Some(true));
        assert_eq!(p.clauses.len(), 2);
        assert_eq!(p.clauses[0].ordered, Some(true));
    }

    #[test]
    fn amp_is_boolean_and() {
        // Given: CBReader &
        let p = parse_query("佛陀&阿难").unwrap();
        // Then
        assert_eq!(p.mode, "boolean");
        assert_eq!(p.boolean_op.as_deref(), Some("and"));
        assert_eq!(p.terms, vec!["佛陀", "阿难"]);
        assert_eq!(p.clauses.len(), 2);
        assert!(p.not_terms.is_empty());
    }

    #[test]
    fn comma_is_boolean_or() {
        let p = parse_query("莲?色,莲花色").unwrap();
        assert_eq!(p.mode, "boolean");
        assert_eq!(p.boolean_op.as_deref(), Some("or"));
        assert_eq!(p.terms, vec!["莲?色", "莲花色"]);
    }

    #[test]
    fn dash_fills_not_terms() {
        // Given: CBReader - EXCLUDE
        let p = parse_query("佛陀-佛陀曰").unwrap();
        // Then: positive term + not_terms
        assert_eq!(p.mode, "boolean");
        assert_eq!(p.terms, vec!["佛陀"]);
        assert_eq!(p.not_terms, vec!["佛陀曰"]);
        assert!(p.boolean_op.is_none() || p.boolean_op.as_deref() == Some("and"));
    }

    #[test]
    fn mixed_amp_and_comma_is_parse_error() {
        // Given: both & and ,
        // When/Then: ParseError
        assert!(parse_query("A&B,C").is_err());
        assert!(parse_query("A,B&C").is_err());
    }
}
