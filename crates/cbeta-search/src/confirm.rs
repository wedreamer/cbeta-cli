//! Char-span confirm for near/before on stored `text_norm`.
//!
//! Recall stays Boolean AND of clause terms; this module rejects candidates
//! whose normalized 汉字 span exceeds the window (Tantivy PhraseQuery slop
//! cannot encode multi-char clause proximity).

use cbeta_core::ParsedQuery;
use cbeta_parse::{normalize_query, GaijiMap};

/// Default CBReader proximity window (normalized 汉字).
pub const DEFAULT_WINDOW: u32 = 30;

/// Whether this mode needs post-recall confirm on stored `text_norm`.
pub fn needs_span_confirm(mode: &str) -> bool {
    matches!(mode, "near" | "before" | "wildcard")
}

/// Confirm `?` wildcard: each `?` is exactly one normalized 汉字.
///
/// Pattern literals are split on `?` then normalized (so `?` is not stripped
/// as punctuation). Sliding match over `text_norm`.
pub fn confirm_wildcard(text_norm: &str, parsed: &ParsedQuery, gaiji: &GaijiMap) -> bool {
    let pattern = parsed
        .terms
        .first()
        .map(|s| s.as_str())
        .unwrap_or(&parsed.raw);
    let parts = wildcard_literal_parts(pattern, gaiji);
    let qmarks = pattern.chars().filter(|c| *c == '?').count();
    if parts.iter().all(|p| p.is_empty()) && qmarks == 0 {
        return false;
    }
    matches_wildcard_parts(text_norm, &parts, qmarks)
}

/// Split `pattern` on `?`, normalize each literal (empty allowed at ends).
pub fn wildcard_literal_parts(pattern: &str, gaiji: &GaijiMap) -> Vec<String> {
    pattern
        .split('?')
        .map(|s| normalize_query(s, gaiji))
        .collect()
}

fn matches_wildcard_parts(hay: &str, parts: &[String], qmarks: usize) -> bool {
    // parts.len() == qmarks + 1 when pattern is well-formed.
    let hay: Vec<char> = hay.chars().collect();
    let part_chars: Vec<Vec<char>> = parts.iter().map(|p| p.chars().collect()).collect();
    let lit_len: usize = part_chars.iter().map(|p| p.len()).sum();
    let need = lit_len + qmarks;
    if need == 0 || hay.len() < need {
        return false;
    }
    // For each start offset, try to match parts with exactly one char per ?.
    'start: for start in 0..=hay.len() - need {
        let mut i = start;
        for (pi, lit) in part_chars.iter().enumerate() {
            if i + lit.len() > hay.len() {
                continue 'start;
            }
            if hay[i..i + lit.len()] != lit[..] {
                continue 'start;
            }
            i += lit.len();
            if pi + 1 < part_chars.len() {
                // one char for this ?
                if i >= hay.len() {
                    continue 'start;
                }
                i += 1;
            }
        }
        if i == start + need {
            return true;
        }
    }
    false
}

/// Dispatch confirm for modes that need stored `text_norm` checks.
pub fn confirm_candidate(text_norm: &str, parsed: &ParsedQuery, gaiji: &GaijiMap) -> bool {
    match parsed.mode.as_str() {
        "near" | "before" => confirm_near_before(text_norm, parsed, gaiji),
        "wildcard" => confirm_wildcard(text_norm, parsed, gaiji),
        _ => true,
    }
}

/// Confirm near/before on one stored `text_norm` line.
///
/// - **near** (`ordered == false`): clauses appear with some placement whose
///   pairwise span gaps are ≤ window (order ignored).
/// - **before** (`ordered == true`): clauses appear in order; gap between the
///   end of clause i and start of clause i+1 is ≤ window.
pub fn confirm_near_before(text_norm: &str, parsed: &ParsedQuery, gaiji: &GaijiMap) -> bool {
    let window = parsed.within_chars.unwrap_or(DEFAULT_WINDOW) as usize;
    let ordered = parsed.ordered.unwrap_or(false) || parsed.mode == "before";
    let norms = clause_norms(parsed, gaiji);
    if norms.is_empty() {
        return true;
    }
    if norms.iter().any(|t| t.is_empty()) {
        return false;
    }
    let occs: Vec<Vec<(usize, usize)>> = norms.iter().map(|t| find_spans(text_norm, t)).collect();
    if occs.iter().any(|o| o.is_empty()) {
        return false;
    }
    if ordered {
        confirm_ordered(&occs, window)
    } else {
        confirm_unordered(&occs, window)
    }
}

fn clause_norms(parsed: &ParsedQuery, gaiji: &GaijiMap) -> Vec<String> {
    if !parsed.clauses.is_empty() {
        return parsed
            .clauses
            .iter()
            .map(|c| normalize_query(&c.text, gaiji))
            .filter(|t| !t.is_empty())
            .collect();
    }
    parsed
        .terms
        .iter()
        .map(|t| normalize_query(t, gaiji))
        .filter(|t| !t.is_empty())
        .collect()
}

/// All (start, end) char-index spans of `needle` inside `hay` (end exclusive).
fn find_spans(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    let hay_chars: Vec<char> = hay.chars().collect();
    let needle_chars: Vec<char> = needle.chars().collect();
    let n = needle_chars.len();
    if n == 0 || n > hay_chars.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for start in 0..=hay_chars.len() - n {
        if hay_chars[start..start + n] == needle_chars[..] {
            out.push((start, start + n));
        }
    }
    out
}

/// Gap in chars strictly between two spans; 0 if they touch or overlap.
fn gap_between(a: (usize, usize), b: (usize, usize)) -> usize {
    if a.1 <= b.0 {
        b.0.saturating_sub(a.1)
    } else if b.1 <= a.0 {
        a.0.saturating_sub(b.1)
    } else {
        0
    }
}

fn confirm_ordered(occs: &[Vec<(usize, usize)>], window: usize) -> bool {
    fn dfs(
        occs: &[Vec<(usize, usize)>],
        idx: usize,
        prev_end: Option<usize>,
        window: usize,
    ) -> bool {
        if idx == occs.len() {
            return true;
        }
        for &(start, end) in &occs[idx] {
            if let Some(pe) = prev_end {
                if start < pe {
                    continue;
                }
                if start - pe > window {
                    continue;
                }
            }
            if dfs(occs, idx + 1, Some(end), window) {
                return true;
            }
        }
        false
    }
    dfs(occs, 0, None, window)
}

fn confirm_unordered(occs: &[Vec<(usize, usize)>], window: usize) -> bool {
    if occs.len() == 1 {
        return !occs[0].is_empty();
    }
    if occs.len() == 2 {
        for &a in &occs[0] {
            for &b in &occs[1] {
                if gap_between(a, b) <= window {
                    return true;
                }
            }
        }
        return false;
    }
    let n = occs.len();
    let mut order: Vec<usize> = (0..n).collect();
    loop {
        let reordered: Vec<Vec<(usize, usize)>> = order.iter().map(|&i| occs[i].clone()).collect();
        if confirm_ordered(&reordered, window) {
            return true;
        }
        if !next_permutation(&mut order) {
            break;
        }
    }
    false
}

fn next_permutation(a: &mut [usize]) -> bool {
    let n = a.len();
    if n < 2 {
        return false;
    }
    let mut i = n - 1;
    while i > 0 && a[i - 1] >= a[i] {
        i -= 1;
    }
    if i == 0 {
        return false;
    }
    let mut j = n - 1;
    while a[j] <= a[i - 1] {
        j -= 1;
    }
    a.swap(i - 1, j);
    a[i..].reverse();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_core::parse_query;
    use cbeta_parse::normalize_index;

    fn norm(s: &str) -> String {
        normalize_index(s, &GaijiMap::default())
    }

    // Given/When/Then: close unordered pair must pass near window.
    #[test]
    fn near_hits_when_gap_within_window() {
        let text = norm(&format!("真如{}缘起", "中".repeat(5)));
        let pq = parse_query("真如 NEAR/16 缘起").expect("parse");
        assert!(confirm_near_before(&text, &pq, &GaijiMap::default()));
    }

    // Given/When/Then: far pair beyond window must miss.
    #[test]
    fn near_misses_when_gap_exceeds_window() {
        let text = norm(&format!("真如{}缘起", "中".repeat(20)));
        let pq = parse_query("真如 NEAR/16 缘起").expect("parse");
        assert!(!confirm_near_before(&text, &pq, &GaijiMap::default()));
    }

    // Given/When/Then: reverse order still hits unordered near.
    #[test]
    fn near_allows_reverse_order() {
        let text = norm(&format!("缘起{}真如", "中".repeat(3)));
        let pq = parse_query("真如+缘起").expect("parse");
        assert!(confirm_near_before(&text, &pq, &GaijiMap::default()));
    }

    // Given/When/Then: before requires A then B; reverse must miss.
    #[test]
    fn before_requires_order() {
        let forward = norm(&format!("真如{}缘起", "中".repeat(3)));
        let reverse = norm(&format!("缘起{}真如", "中".repeat(3)));
        let pq = parse_query("真如*缘起").expect("parse");
        assert!(confirm_near_before(&forward, &pq, &GaijiMap::default()));
        assert!(!confirm_near_before(&reverse, &pq, &GaijiMap::default()));
    }

    // Given/When/Then: before also respects window.
    #[test]
    fn before_misses_far_pair() {
        let text = norm(&format!("真如{}缘起", "中".repeat(40)));
        let pq = parse_query("真如*缘起").expect("parse");
        assert!(!confirm_near_before(&text, &pq, &GaijiMap::default()));
    }

    #[test]
    fn plus_default_window_thirty() {
        let text = norm(&format!("空性{}缘生", "中".repeat(25)));
        let pq = parse_query("空性+缘生").expect("parse");
        assert_eq!(pq.within_chars, Some(30));
        assert!(confirm_near_before(&text, &pq, &GaijiMap::default()));
        let far = norm(&format!("空性{}缘生", "中".repeat(35)));
        assert!(!confirm_near_before(&far, &pq, &GaijiMap::default()));
    }

    // Given/When/Then: ? is exactly one normalized char.
    #[test]
    fn wildcard_one_char_only() {
        let g = GaijiMap::default();
        let pq = parse_query("莲?色").expect("parse");
        assert!(confirm_wildcard(&norm("莲華色"), &pq, &g));
        assert!(confirm_wildcard(&norm("莲花色"), &pq, &g));
        assert!(!confirm_wildcard(&norm("莲色"), &pq, &g));
        assert!(!confirm_wildcard(&norm("莲XY色"), &pq, &g));
    }
}
