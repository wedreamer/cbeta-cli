//! Char-span confirm for near/before on stored `text_norm`.
//!
//! Recall is Boolean AND of char 2-grams; this module drops candidates whose
//! covering span on normalized 汉字 exceeds `within_chars`. Not PhraseQuery slop.

use cbeta_core::ParsedQuery;
use cbeta_parse::{normalize_query, GaijiMap};

/// Default CBReader proximity window (normalized 汉字).
pub const DEFAULT_WINDOW: u32 = 30;

/// Whether near/before need post-recall confirm on stored `text_norm`.
pub fn needs_span_confirm(mode: &str) -> bool {
    matches!(mode, "near" | "before")
}

/// Confirm one candidate line against a near/before [`ParsedQuery`].
///
/// Span = `max(end) − min(start)` of the chosen match pair on `text_norm`
/// (end exclusive, char indices). NEAR ignores order; BEFORE requires A's
/// match to end at or before B's match starts.
pub fn confirm_span(text_norm: &str, parsed: &ParsedQuery, gaiji: &GaijiMap) -> bool {
    if !needs_span_confirm(&parsed.mode) {
        return true;
    }
    let window = parsed.within_chars.unwrap_or(DEFAULT_WINDOW) as usize;
    let ordered = parsed.ordered.unwrap_or(false) || parsed.mode == "before";
    let norms: Vec<String> = parsed
        .terms
        .iter()
        .map(|t| normalize_query(t, gaiji))
        .filter(|t| !t.is_empty())
        .collect();
    if norms.is_empty() {
        return true;
    }
    if norms.len() == 1 {
        return !find_spans(text_norm, &norms[0]).is_empty();
    }
    let occs: Vec<Vec<(usize, usize)>> = norms.iter().map(|t| find_spans(text_norm, t)).collect();
    if occs.iter().any(Vec::is_empty) {
        return false;
    }
    if ordered {
        confirm_ordered_pair_chain(&occs, window)
    } else {
        confirm_unordered(&occs, window)
    }
}

/// All `(start, end)` char-index spans of `needle` in `hay` (end exclusive).
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

/// Covering span of two half-open ranges.
fn covering_span(a: (usize, usize), b: (usize, usize)) -> usize {
    let lo = a.0.min(b.0);
    let hi = a.1.max(b.1);
    hi.saturating_sub(lo)
}

fn confirm_unordered(occs: &[Vec<(usize, usize)>], window: usize) -> bool {
    if occs.len() == 1 {
        return !occs[0].is_empty();
    }
    if occs.len() == 2 {
        for &a in &occs[0] {
            for &b in &occs[1] {
                if covering_span(a, b) <= window {
                    return true;
                }
            }
        }
        return false;
    }
    // N>2: try every term order, then ordered chain under that order.
    let n = occs.len();
    let mut order: Vec<usize> = (0..n).collect();
    loop {
        let reordered: Vec<&[(usize, usize)]> = order.iter().map(|&i| occs[i].as_slice()).collect();
        if confirm_ordered_slices(&reordered, window) {
            return true;
        }
        if !next_permutation(&mut order) {
            break;
        }
    }
    false
}

fn confirm_ordered_pair_chain(occs: &[Vec<(usize, usize)>], window: usize) -> bool {
    let slices: Vec<&[(usize, usize)]> = occs.iter().map(Vec::as_slice).collect();
    confirm_ordered_slices(&slices, window)
}

/// Ordered: each match ends before the next starts; covering span of first..last ≤ window.
fn confirm_ordered_slices(occs: &[&[(usize, usize)]], window: usize) -> bool {
    fn dfs(
        occs: &[&[(usize, usize)]],
        idx: usize,
        prev_end: Option<usize>,
        first_start: Option<usize>,
        window: usize,
    ) -> bool {
        if idx == occs.len() {
            return true;
        }
        for &(start, end) in occs[idx] {
            if let Some(pe) = prev_end {
                if start < pe {
                    continue;
                }
            }
            let fs = first_start.unwrap_or(start);
            // Partial covering so far must stay within window (and final too).
            if end.saturating_sub(fs) > window {
                continue;
            }
            if dfs(occs, idx + 1, Some(end), Some(fs), window) {
                return true;
            }
        }
        false
    }
    dfs(occs, 0, None, None, window)
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

    fn ok(text: &str, q: &str) -> bool {
        let pq = parse_query(q).expect("parse");
        confirm_span(&norm(text), &pq, &GaijiMap::default())
    }

    #[test]
    fn span_near_16_accepts() {
        // Given: 真如 … 缘起 covering span ≤ 16 on text_norm
        // When: NEAR/16
        // Then: accept
        let filler = "中".repeat(5);
        let text = format!("真如{filler}缘起");
        // chars: 2 + 5 + 2 = 9 covering span
        assert!(ok(&text, "真如 NEAR/16 缘起"));
    }

    #[test]
    fn span_near_16_rejects_far() {
        // Given: covering span > 16
        // When: NEAR/16
        // Then: reject
        let filler = "中".repeat(20);
        let text = format!("真如{filler}缘起");
        // 2 + 20 + 2 = 24 > 16
        assert!(!ok(&text, "真如 NEAR/16 缘起"));
    }

    #[test]
    fn span_before_requires_order() {
        // Given: forward vs reverse order
        // When: BEFORE
        // Then: only forward passes
        let filler = "中".repeat(3);
        let forward = format!("真如{filler}缘起");
        let reverse = format!("缘起{filler}真如");
        assert!(ok(&forward, "真如 BEFORE/16 缘起"));
        assert!(!ok(&reverse, "真如 BEFORE/16 缘起"));
        // NEAR still accepts reverse
        assert!(ok(&reverse, "真如 NEAR/16 缘起"));
    }

    #[test]
    fn plus_equivalent_near_30() {
        // Given: 空性 … 缘生 within 30
        // When: `+` (near/30)
        // Then: accept close, reject far
        let close = format!("空性{}缘生", "中".repeat(20));
        // covering 2+20+2 = 24 ≤ 30
        assert!(ok(&close, "空性+缘生"));
        let far = format!("空性{}缘生", "中".repeat(40));
        // 2+40+2 = 44 > 30
        assert!(!ok(&far, "空性+缘生"));
        let pq = parse_query("空性+缘生").expect("parse");
        assert_eq!(pq.within_chars, Some(30));
        assert_eq!(pq.ordered, Some(false));
    }
}
