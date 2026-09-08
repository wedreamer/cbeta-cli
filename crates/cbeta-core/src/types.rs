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
    pub titles: Vec<String>,
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
    /// 作译者 display name (繁體).
    pub author: String,
    /// Juan (卷) number within the work.
    pub juan: u64,
    /// Raw line text as stored.
    pub text_raw: String,
    /// Human citation, e.g. `(CBETA 2026.R2, T31, no. 1585, p. 1, a12)`.
    pub citation: String,
    /// Ranker score (higher is better).
    pub score: f32,
    /// Corpus release tag, e.g. `2026R2`.
    pub cbeta_tag: String,
}

/// One catalog row for browse / filter UIs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Work id such as `T1585` (volume stripped).
    pub work_id: String,
    /// Work title (display default 繁體).
    pub title: String,
    /// 作译者 display name (繁體).
    pub author: String,
    /// Dynasty label when known (e.g. `唐`).
    pub dynasty: String,
    /// Catalog category label (e.g. `論`).
    pub category: String,
    /// Work type code used by filters (e.g. `lun`).
    pub work_type: String,
    /// Corpus release tag, e.g. `2026R2`.
    pub cbeta_tag: String,
    /// Hash of the index scope that produced this row.
    pub scope_hash: String,
}

/// Local index metadata shown by `info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexInfo {
    /// Corpus release tag, e.g. `2026R2`.
    pub cbeta_tag: String,
    /// Human scope name, e.g. `taisho`.
    pub scope: String,
    /// Artifact id `{cbeta_tag}+{scope_hash}`.
    pub artifact_id: String,
    /// Number of works present in the artifact.
    pub work_count: u64,
    /// Filesystem path of the index root.
    pub index_path: String,
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
            author: "玄奘".into(),
            juan: 1,
            text_raw: "色即是空".into(),
            citation: "(CBETA 2026.R2, T31, no. 1585, p. 1, a12)".into(),
            score: 1.0,
            cbeta_tag: "2026R2".into(),
        };
        assert_eq!(hit.line_id, "T31n1585_p0001a12");
    }

    #[test]
    fn hit_has_author_juan_cbeta_tag() {
        let hit = Hit {
            line_id: "T30n1578_p0268b21".into(),
            work_id: "T1578".into(),
            title: "攝大乘論".into(),
            author: "無著".into(),
            juan: 1,
            text_raw: "真性有為空".into(),
            citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)".into(),
            score: 0.9,
            cbeta_tag: "2026R2".into(),
        };
        assert_eq!(hit.author, "無著");
        assert_eq!(hit.juan, 1);
        assert_eq!(hit.cbeta_tag, "2026R2");
    }

    #[test]
    fn catalog_entry_json_fields() {
        let entry = CatalogEntry {
            work_id: "T1585".into(),
            title: "成唯識論".into(),
            author: "玄奘".into(),
            dynasty: "唐".into(),
            category: "論".into(),
            work_type: "lun".into(),
            cbeta_tag: "2026R2".into(),
            scope_hash: "a3f91c2e".into(),
        };
        let v = serde_json::to_value(&entry).unwrap();
        for key in [
            "work_id",
            "title",
            "author",
            "dynasty",
            "category",
            "work_type",
            "cbeta_tag",
            "scope_hash",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert_eq!(v["work_id"], "T1585");
        assert_eq!(v["scope_hash"], "a3f91c2e");
    }

    #[test]
    fn index_info_json_fields() {
        let info = IndexInfo {
            cbeta_tag: "2026R2".into(),
            scope: "taisho".into(),
            artifact_id: "2026R2+a3f91c2e".into(),
            work_count: 42,
            index_path: "/tmp/.cbeta/index".into(),
        };
        let v = serde_json::to_value(&info).unwrap();
        for key in [
            "cbeta_tag",
            "scope",
            "artifact_id",
            "work_count",
            "index_path",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert_eq!(v["artifact_id"], "2026R2+a3f91c2e");
        assert_eq!(v["work_count"], 42);
    }
}
