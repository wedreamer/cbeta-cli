//! Shared protocol types and query DSL for CBETA offline search.
//!
//! This crate owns `Command` / `Hit` / `Filters` / `parse_query` used by every
//! transport (CLI, MCP, HTTP). It is **not** the `cbeta` binary — that lives in
//! `cbeta-cli`.

#![deny(missing_docs)]

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

/// What the user/agent asked the system to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Full-text / DSL search over the index.
    #[default]
    Search,
    /// Check whether pasted text is original CBETA wording.
    Verify,
    /// Fetch one line by `line_id`.
    Get,
    /// Open a work/passage (type-only today; not yet in clap).
    Read,
    /// Format a citation string (type-only today; not yet in clap).
    Cite,
    /// Browse catalog metadata.
    Catalog,
    /// Show index / build info.
    Info,
    /// Build or rebuild the local index.
    Build,
    /// Serve MCP/HTTP over the same `Command` surface.
    Serve,
}

/// How results should be rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Human TTY output (default when interactive).
    #[default]
    Tty,
    /// Plain text without color/decoration.
    Plain,
    /// Single pretty JSON document.
    Json,
    /// One JSON object per line (pipe-friendly).
    Jsonl,
}

/// Corpus scope filters shared by CLI flags and MCP fields.
///
/// Empty vectors mean “no restriction” on that axis. Field-level docs are
/// intentionally omitted — the names are the contract.
#[allow(missing_docs)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filters {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub works: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub juans: Vec<u32>,
}

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

/// Transport-neutral request: one shape for CLI, MCP, and HTTP.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Command {
    /// Requested action.
    pub action: Action,
    /// Raw query / text / id / scope string when the action needs one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// Scope filters.
    #[serde(default)]
    pub filters: Filters,
    /// Output format.
    pub format: Format,
    /// When true, surface parse/plan details instead of (or before) hits.
    #[serde(default)]
    pub explain: bool,
    /// Filled when `q` was run through [`parse_query`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_query: Option<ParsedQuery>,
}

/// One search hit (product shape; search engine not wired yet).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// CBETA line id, shape `T31n1585_p0001a12`
    /// (canon+vol `n` work `_p` page col line).
    pub line_id: String,
    /// Work id such as `T1585`.
    pub work_id: String,
    /// Work title (display default 繁體).
    pub title: String,
    /// Raw line text as stored.
    pub text_raw: String,
    /// Human citation, e.g. `(CBETA 2026.R2, T31, no. 1585, p. 1, a12)`.
    pub citation: String,
    /// Ranker score (higher is better).
    pub score: f32,
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

    #[test]
    fn action_read_cite_serde_roundtrip() {
        assert_eq!(serde_json::to_string(&Action::Read).unwrap(), "\"read\"");
        assert_eq!(
            serde_json::from_str::<Action>("\"read\"").unwrap(),
            Action::Read
        );
        assert_eq!(serde_json::to_string(&Action::Cite).unwrap(), "\"cite\"");
        assert_eq!(
            serde_json::from_str::<Action>("\"cite\"").unwrap(),
            Action::Cite
        );
    }

    #[test]
    fn format_plain_jsonl_serde_snake_case() {
        assert_eq!(serde_json::to_string(&Format::Plain).unwrap(), "\"plain\"");
        assert_eq!(
            serde_json::from_str::<Format>("\"plain\"").unwrap(),
            Format::Plain
        );
        assert_eq!(serde_json::to_string(&Format::Jsonl).unwrap(), "\"jsonl\"");
        assert_eq!(
            serde_json::from_str::<Format>("\"jsonl\"").unwrap(),
            Format::Jsonl
        );
    }

    #[test]
    fn hit_constructs_with_line_id() {
        let hit = Hit {
            line_id: "T31n1585_p0001a12".into(),
            work_id: "T1585".into(),
            title: "成唯識論".into(),
            text_raw: "色即是空".into(),
            citation: "(CBETA 2026.R2, T31, no. 1585, p. 1, a12)".into(),
            score: 1.0,
        };
        assert_eq!(hit.line_id, "T31n1585_p0001a12");
    }
}
