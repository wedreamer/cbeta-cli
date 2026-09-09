//! Verify transport type: original-text check report.

use serde::{Deserialize, Serialize};

use crate::Hit;

/// Result of `verify`: exact original hit and/or similar lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyReport {
    /// True when the quote matches a stored `text_norm` exactly (after normalize).
    pub is_original: bool,
    /// Exact line on hash hit; always serialized (`null` when absent).
    #[serde(default)]
    pub exact_hit: Option<Hit>,
    /// Ranked similar lines when not original (empty on exact hash hit).
    #[serde(default)]
    pub similar: Vec<Hit>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_report_json_shape() {
        let report = VerifyReport {
            is_original: false,
            exact_hit: None,
            similar: vec![],
        };
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["is_original"], false);
        assert!(v.get("similar").and_then(|s| s.as_array()).is_some());
        assert!(v.get("exact_hit").is_some());
        assert!(v["exact_hit"].is_null());
    }
}
