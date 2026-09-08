//! Protocol value types shared by every transport.

use serde::{Deserialize, Serialize};

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
    /// Filled when `q` was run through [`crate::parse_query`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_query: Option<crate::ParsedQuery>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
