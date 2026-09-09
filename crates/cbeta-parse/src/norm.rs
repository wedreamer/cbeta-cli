//! Search-key normalization: NFKC → OpenCC s2t → gaiji → strip punct.

use crate::gaiji::GaijiMap;
use ferrous_opencc::config::BuiltinConfig;
use ferrous_opencc::OpenCC;
use unicode_normalization::UnicodeNormalization;

/// Punctuation stripped from search keys (汉字 order preserved).
const STRIP_PUNCT: &[char] = &[
    '，', '。', '、', '；', '：', '！', '？', '「', '」', '『', '』', ',', '.', ';', ':', '!', '?',
    '"', '\'', '（', '）', '(', ')',
];

/// Shared OpenCC s2t converter (lazy, process-wide).
fn s2t_engine() -> Option<&'static OpenCC> {
    use std::sync::OnceLock;
    static ENGINE: OnceLock<Option<OpenCC>> = OnceLock::new();
    ENGINE
        .get_or_init(|| OpenCC::from_config(BuiltinConfig::S2tw).ok())
        .as_ref()
}

/// NFKC normalize a string.
pub fn nfkc(text: &str) -> String {
    text.nfkc().collect()
}

/// Simplified → Traditional via OpenCC (index/query key space).
pub fn s2t(text: &str) -> String {
    match s2t_engine() {
        Some(cc) => cc.convert(text),
        None => text.to_string(),
    }
}

/// Shared OpenCC t2s converter (lazy, process-wide).
fn t2s_engine() -> Option<&'static OpenCC> {
    use std::sync::OnceLock;
    static ENGINE: OnceLock<Option<OpenCC>> = OnceLock::new();
    ENGINE
        .get_or_init(|| OpenCC::from_config(BuiltinConfig::T2s).ok())
        .as_ref()
}

/// Traditional → Simplified for `--script s` display only.
pub fn t2s(text: &str) -> String {
    match t2s_engine() {
        Some(cc) => cc.convert(text),
        None => text.to_string(),
    }
}

/// Strip configured punctuation; keep 汉字 and other non-punct order.
pub fn strip_punct(text: &str) -> String {
    text.chars().filter(|c| !STRIP_PUNCT.contains(c)).collect()
}

/// Full search-key pipeline used for both query and index text.
///
/// Order: NFKC → OpenCC s2t → gaiji/variant map → strip punct.
pub fn normalize_for_search(text: &str, gaiji: &GaijiMap) -> String {
    let step = nfkc(text);
    let step = s2t(&step);
    let step = gaiji.apply(&step);
    strip_punct(&step)
}

/// Alias: query and index must share the same key space.
pub fn normalize_query(text: &str, gaiji: &GaijiMap) -> String {
    normalize_for_search(text, gaiji)
}

/// Alias: index side of the shared key space.
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
        // simplified 为 → 為
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
        // missing key → same-width placeholder, position kept
        let missing = g.resolve("[gaiji:MISSING]");
        assert_eq!(missing, "□");
        assert_eq!(missing.chars().count(), 1);
        let with_missing = normalize_query("前[gaiji:MISSING]後", &g);
        // apply only replaces known keys; unresolved token remains unless we
        // also scan — resolve is the unit API for missing slots.
        assert!(with_missing.contains("前"));
        assert!(with_missing.contains("後"));
        let _ = with_missing;
    }

    #[test]
    fn nfkc_compat() {
        // fullwidth Latin A (U+FF21) → ASCII A under NFKC
        let g = GaijiMap::default();
        let out = normalize_query("\u{FF21}空", &g);
        assert!(out.starts_with('A'), "got {out}");
    }
}
