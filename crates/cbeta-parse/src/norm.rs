//! Search-key normalization: NFKC → OpenCC s2t → gaiji → strip punct.

use crate::gaiji::GaijiMap;

/// NFKC normalize a string (stub until feat commit).
pub fn nfkc(text: &str) -> String {
    text.to_string()
}

/// Simplified → Traditional (stub).
pub fn s2t(text: &str) -> String {
    text.to_string()
}

/// Strip punctuation (stub).
pub fn strip_punct(text: &str) -> String {
    text.to_string()
}

/// Full search-key pipeline (stub).
pub fn normalize_for_search(text: &str, _gaiji: &GaijiMap) -> String {
    text.to_string()
}

/// Query side of the shared key space (stub).
pub fn normalize_query(text: &str, gaiji: &GaijiMap) -> String {
    normalize_for_search(text, gaiji)
}

/// Index side of the shared key space (stub).
pub fn normalize_index(text: &str, gaiji: &GaijiMap) -> String {
    normalize_for_search(text, gaiji)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s2t_kongxing() {
        let g = GaijiMap::default();
        let q = normalize_query("空性", &g);
        let idx = normalize_index("空性", &g);
        assert_eq!(q, idx);
        assert_eq!(q, "空性");
        let simp = normalize_query("真性有为空", &g);
        let trad = normalize_index("真性有為空", &g);
        assert_eq!(simp, trad);
        assert_eq!(simp, "真性有為空");
    }

    #[test]
    fn strip_punct_keeps_han_order() {
        let g = GaijiMap::default();
        let out = normalize_query("真性有为空，缘生故如幻。", &g);
        assert_eq!(out, "真性有為空緣生故如幻");
        assert!(!out.contains('，'));
        assert!(!out.contains('。'));
    }

    #[test]
    fn gaiji_keeps_slot() {
        let g = GaijiMap::from_pairs([("[gaiji:A]", "龍"), ("[gaiji:B]", "象")]);
        let mapped = normalize_query("見[gaiji:A]王", &g);
        assert_eq!(mapped, "見龍王");
        let missing = g.resolve("[gaiji:MISSING]");
        assert_eq!(missing, "□");
        assert_eq!(missing.chars().count(), 1);
    }

    #[test]
    fn nfkc_compat() {
        let g = GaijiMap::default();
        let out = normalize_query("\u{FF21}空", &g);
        assert!(out.starts_with('A'), "got {out}");
    }
}
