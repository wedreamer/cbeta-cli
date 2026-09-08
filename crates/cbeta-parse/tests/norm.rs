//! Integration goldens for search-key normalization.

use cbeta_parse::{normalize_index, normalize_query, GaijiMap};

#[test]
fn s2t_kongxing() {
    let g = GaijiMap::default();
    assert_eq!(normalize_query("空性", &g), normalize_index("空性", &g));
    assert_eq!(
        normalize_query("真性有为空", &g),
        normalize_index("真性有為空", &g)
    );
}

#[test]
fn strip_punct_keeps_han_order() {
    let g = GaijiMap::default();
    let out = normalize_query("甲，乙。丙、丁", &g);
    assert_eq!(out, "甲乙丙丁");
}

#[test]
fn gaiji_keeps_slot() {
    let g = GaijiMap::from_pairs([("[gaiji:A]", "佛"), ("[gaiji:B]", "□")]);
    assert_eq!(normalize_query("南[gaiji:A]", &g), "南佛");
    assert_eq!(g.resolve("missing"), "□");
    assert_eq!(g.resolve("missing").chars().count(), 1);
}

#[test]
fn nfkc_compat() {
    let g = GaijiMap::default();
    let out = normalize_query("\u{FF21}", &g);
    assert_eq!(out, "A");
}
