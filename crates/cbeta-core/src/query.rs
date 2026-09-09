//! CBReader-style query DSL parsing.

#[path = "query_ops.rs"]
mod query_ops;

use serde::{Deserialize, Serialize};

use query_ops::{parse_exclusion, parse_near_like, split_terms};

/// Failure from [`parse_query`] (hand-rolled; not `thiserror` yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseError {}

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
    /// `Some(false)` for near/NEAR; `Some(true)` for before/BEFORE; else `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
    /// Boolean join for `&` / `,` (`"and"` / `"or"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_op: Option<String>,
    /// Terms excluded by `-` / `NOT` (omit from JSON when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<String>,
}

/// Parse CBReader-style query DSL into a [`ParsedQuery`].
///
/// Distance is always **normalized 汉字**, never tokens. Operators:
/// - `+` → `near` / 30, `ordered=false`
/// - `*` → `before` / 30, `ordered=true`
/// - `NEAR/N` → unordered window `N`; `BEFORE/N` → ordered window `N`
/// - `&` → boolean AND; `,` → boolean OR; `-` / `NOT` → excluded terms
/// - `?` → single-char wildcard (at most two `?`)
///
/// Fullwidth operators in the reject set `—＋＊＆？` error out (halfwidth only).
/// Fullwidth comma `，` is **not** rejected.
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

    let qmark_count = raw.chars().filter(|c| *c == '?').count();
    if qmark_count > 2 {
        return Err(ParseError(
            "wildcard '?' appears more than twice (max 2)".into(),
        ));
    }

    if let Some(parsed) = parse_near_like(&raw, "NEAR", "near", false)? {
        return Ok(parsed);
    }
    if let Some(parsed) = parse_near_like(&raw, "BEFORE", "before", true)? {
        return Ok(parsed);
    }
    if raw.contains('+') {
        let terms = split_terms(&raw, '+');
        return Ok(ParsedQuery {
            raw,
            mode: "near".into(),
            terms,
            within_chars: Some(30),
            wildcard: None,
            ordered: Some(false),
            bool_op: None,
            excluded: Vec::new(),
        });
    }
    if raw.contains('*') && qmark_count == 0 {
        let terms = split_terms(&raw, '*');
        return Ok(ParsedQuery {
            raw,
            mode: "before".into(),
            terms,
            within_chars: Some(30),
            wildcard: None,
            ordered: Some(true),
            bool_op: None,
            excluded: Vec::new(),
        });
    }
    if qmark_count > 0 {
        return Ok(ParsedQuery {
            raw: raw.clone(),
            mode: "wildcard".into(),
            terms: vec![raw],
            within_chars: None,
            wildcard: Some(true),
            ordered: None,
            bool_op: None,
            excluded: Vec::new(),
        });
    }
    if raw.contains('&') {
        let terms = split_terms(&raw, '&');
        return Ok(ParsedQuery {
            raw,
            mode: "boolean".into(),
            terms,
            within_chars: None,
            wildcard: None,
            ordered: None,
            bool_op: Some("and".into()),
            excluded: Vec::new(),
        });
    }
    if raw.contains(',') {
        let terms = split_terms(&raw, ',');
        return Ok(ParsedQuery {
            raw,
            mode: "boolean".into(),
            terms,
            within_chars: None,
            wildcard: None,
            ordered: None,
            bool_op: Some("or".into()),
            excluded: Vec::new(),
        });
    }
    if let Some(parsed) = parse_exclusion(&raw) {
        return Ok(parsed);
    }
    Ok(ParsedQuery {
        raw: raw.clone(),
        mode: "keyword".into(),
        terms: vec![raw],
        within_chars: None,
        wildcard: None,
        ordered: None,
        bool_op: None,
        excluded: Vec::new(),
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
    fn plus_is_near_30_ordered_false() {
        let p = parse_query("空性+缘生").unwrap();
        assert_eq!(p.mode, "near");
        assert_eq!(p.within_chars, Some(30));
        assert_eq!(p.ordered, Some(false));
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
    fn near_16_ordered_false() {
        let p = parse_query("真如 NEAR/16 缘起").unwrap();
        assert_eq!(p.mode, "near");
        assert_eq!(p.within_chars, Some(16));
        assert_eq!(p.ordered, Some(false));
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
        assert_eq!(p.wildcard, Some(true));
    }

    #[test]
    fn wildcard_more_than_two_errors() {
        assert!(parse_query("莲??色?").is_err());
        assert!(parse_query("a?b?c?").is_err());
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
    fn star_is_before_30_ordered_true() {
        let p = parse_query("空性*缘生").unwrap();
        assert_eq!(p.mode, "before");
        assert_eq!(p.within_chars, Some(30));
        assert_eq!(p.ordered, Some(true));
        assert_eq!(p.terms, vec!["空性", "缘生"]);
    }

    #[test]
    fn amp_is_boolean_and() {
        let p = parse_query("空性&缘生").unwrap();
        assert_eq!(p.mode, "boolean");
        assert_eq!(p.bool_op, Some("and".into()));
        assert_eq!(p.terms, vec!["空性", "缘生"]);
    }

    #[test]
    fn comma_is_boolean_or() {
        let p = parse_query("空性,缘生").unwrap();
        assert_eq!(p.mode, "boolean");
        assert_eq!(p.bool_op, Some("or".into()));
        assert_eq!(p.terms, vec!["空性", "缘生"]);
    }

    #[test]
    fn minus_is_not_excluded() {
        let p = parse_query("空性-外道").unwrap();
        assert_eq!(p.terms, vec!["空性"]);
        assert_eq!(p.excluded, vec!["外道"]);
    }

    #[test]
    fn before_not_composite() {
        let p = parse_query("真如 BEFORE/8 依他起 NOT 外道").unwrap();
        assert_eq!(p.mode, "before");
        assert_eq!(p.terms, vec!["真如", "依他起"]);
        assert_eq!(p.within_chars, Some(8));
        assert_eq!(p.ordered, Some(true));
        assert_eq!(p.excluded, vec!["外道"]);
    }

    #[test]
    fn keyword_plain() {
        let p = parse_query("色即是空").unwrap();
        assert_eq!(p.mode, "keyword");
        assert_eq!(p.ordered, None);
        assert!(p.excluded.is_empty());
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

    #[test]
    fn near_with_dash_exclusion_on_right() {
        let p = parse_query("真如 NEAR/12 缘起-外道").unwrap();
        assert_eq!(p.mode, "near");
        assert_eq!(p.terms, vec!["真如", "缘起"]);
        assert_eq!(p.excluded, vec!["外道"]);
        assert_eq!(p.within_chars, Some(12));
    }

    #[test]
    fn near_malformed_returns_keyword_or_none_path() {
        let p = parse_query("真如 NEAR/").unwrap();
        assert_ne!(p.mode, "near");
        let p2 = parse_query("NEAR/8 缘起").unwrap();
        assert_ne!(p2.mode, "near");
        let p3 = parse_query("真如 NEAR/xx 缘起").unwrap();
        assert_ne!(p3.mode, "near");
    }

    #[test]
    fn near_missing_right_term_errors() {
        let err = parse_query("真如 NEAR/8 -外道").unwrap_err();
        assert!(err.to_string().contains("requires a term"), "err={err}");
    }

    #[test]
    fn not_exclusion_keyword() {
        let p = parse_query("空性 NOT 外道").unwrap();
        assert_eq!(p.mode, "keyword");
        assert_eq!(p.terms, vec!["空性"]);
        assert_eq!(p.excluded, vec!["外道"]);
    }

    #[test]
    fn not_exclusion_with_dash_in_head_and_tail() {
        let p = parse_query("空性-边见 NOT 外道-戏论").unwrap();
        assert_eq!(p.mode, "keyword");
        assert_eq!(p.terms, vec!["空性"]);
        assert!(p.excluded.iter().any(|e| e == "边见"));
        assert!(p.excluded.iter().any(|e| e == "外道"));
        assert!(p.excluded.iter().any(|e| e == "戏论"));
    }

    #[test]
    fn not_with_empty_excluded_falls_through() {
        let p = parse_query("空性 NOT ").unwrap();
        assert_eq!(p.mode, "keyword");
        assert!(p.excluded.is_empty());
    }

    #[test]
    fn dash_only_trailing_empty_is_keyword() {
        let p = parse_query("空性-").unwrap();
        assert_eq!(p.mode, "keyword");
        assert!(p.excluded.is_empty() || p.terms == vec!["空性"]);
    }

    #[test]
    fn multi_dash_exclusions() {
        let p = parse_query("空性-外道-戏论").unwrap();
        assert_eq!(p.terms, vec!["空性"]);
        assert_eq!(p.excluded, vec!["外道", "戏论"]);
    }
}
