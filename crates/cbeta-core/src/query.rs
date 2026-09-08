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

/// Structured view of a human/agent query string after [`parse_query`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedQuery {
    /// Original trimmed input.
    pub raw: String,
    /// DSL mode: `keyword`, `near`, `before`, or `wildcard`.
    pub mode: String,
    /// Term list derived from the DSL (order preserved).
    pub terms: Vec<String>,
    /// Character window for near/before (normalized 汉字, not tokens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within_chars: Option<u32>,
    /// Set when `?` single-char wildcard mode is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wildcard: Option<bool>,
}

/// Parse CBReader-style query DSL into a [`ParsedQuery`].
///
/// Distance is always **normalized 汉字**, never tokens. Operators:
/// - `+` → `near` with window 30
/// - `*` → `before` with window 30
/// - `NEAR/N` / `BEFORE/N` → ordered window `N`
/// - `?` → single-char wildcard mode
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
        return Ok(ParsedQuery {
            raw,
            mode: "near".into(),
            terms,
            within_chars: Some(30),
            wildcard: None,
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
        });
    }
    if raw.contains('?') {
        return Ok(ParsedQuery {
            raw: raw.clone(),
            mode: "wildcard".into(),
            terms: vec![raw],
            within_chars: None,
            wildcard: Some(true),
        });
    }
    Ok(ParsedQuery {
        raw: raw.clone(),
        mode: "keyword".into(),
        terms: vec![raw],
        within_chars: None,
        wildcard: None,
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
}
