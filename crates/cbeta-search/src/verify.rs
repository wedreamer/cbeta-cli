//! Original-text verify: `norm_hash` TermQuery, then n-gram + strsim.
//!
//! Exact: normalize → SHA-256 `norm_hash` TermQuery (O(1)).
//! Similar: 2-gram Boolean OR on `text_norm` → `normalized_levenshtein` on chars.
//! No vectors / DefaultHasher / bio SW.

use std::collections::HashMap;
use std::path::Path;

use cbeta_core::{Hit, VerifyReport};
use cbeta_index::{hash_text_norm, LineSchema};
use cbeta_parse::{normalize_query, GaijiMap};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::{IndexRecordOption, TantivyDocument, Term, Value};
use tantivy::Searcher;

use crate::error::Result;
use crate::hitmap::doc_to_hit;
use crate::open::{open_search_index, OpenIndex};

/// Candidate pool size for similar n-gram recall.
const SIMILAR_RECALL: usize = 80;
/// Max similar hits returned.
const SIMILAR_LIMIT: usize = 5;

/// Check whether `quote` is original CBETA wording under `index_root`.
///
/// # Errors
/// Returns [`crate::Error::NoIndex`] when no artifact is available, or other
/// index I/O / Tantivy failures.
pub fn verify(index_root: &Path, quote: &str) -> Result<VerifyReport> {
    let (index, fields, _art) = open_search_index(index_root)?;
    let reader = index.reader()?;
    verify_on(&reader, &fields, quote)
}

/// Verify using an already-open [`OpenIndex`].
pub fn verify_open(index: &OpenIndex, quote: &str) -> Result<VerifyReport> {
    verify_on(&index.reader, &index.fields, quote)
}

fn verify_on(
    reader: &tantivy::IndexReader,
    fields: &LineSchema,
    quote: &str,
) -> Result<VerifyReport> {
    let searcher = reader.searcher();
    let gaiji = GaijiMap::default();
    let norm = normalize_query(quote, &gaiji);
    if norm.is_empty() {
        return Ok(VerifyReport {
            is_original: false,
            exact_hit: None,
            similar: Vec::new(),
        });
    }

    if let Some(hit) = find_exact_by_hash(&searcher, fields, &norm)? {
        return Ok(VerifyReport {
            is_original: true,
            exact_hit: Some(hit),
            similar: Vec::new(),
        });
    }

    let similar = find_similar(&searcher, fields, &norm)?;
    Ok(VerifyReport {
        is_original: false,
        exact_hit: None,
        similar,
    })
}

fn stored_text_norm(doc: &TantivyDocument, fields: &LineSchema) -> String {
    doc.get_first(fields.text_norm)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// O(1) exact path: TermQuery on precomputed `norm_hash`.
fn find_exact_by_hash(searcher: &Searcher, fields: &LineSchema, norm: &str) -> Result<Option<Hit>> {
    let hash = hash_text_norm(norm);
    let term = Term::from_field_text(fields.norm_hash, &hash);
    let query = TermQuery::new(term, IndexRecordOption::Basic);
    let top = searcher.search(&query, &TopDocs::with_limit(4))?;
    for (score, addr) in top {
        let doc: TantivyDocument = searcher.doc(addr)?;
        let stored = stored_text_norm(&doc, fields);
        // Hash collision guard: still require full text_norm equality.
        if stored == norm {
            return Ok(Some(doc_to_hit(&doc, fields, score)));
        }
    }
    Ok(None)
}

/// 2-gram OR recall, rank by char-level normalized Levenshtein.
fn find_similar(searcher: &Searcher, fields: &LineSchema, norm: &str) -> Result<Vec<Hit>> {
    let grams = char_ngrams(norm, 2);
    if grams.is_empty() {
        return Ok(Vec::new());
    }
    let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::with_capacity(grams.len());
    for g in &grams {
        let term = Term::from_field_text(fields.text_norm, g);
        clauses.push((
            Occur::Should,
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }
    let query = BooleanQuery::new(clauses);
    let top = searcher.search(&query, &TopDocs::with_limit(SIMILAR_RECALL))?;

    let mut best: HashMap<String, Hit> = HashMap::new();
    for (_bm25, addr) in top {
        let doc: TantivyDocument = searcher.doc(addr)?;
        let stored = stored_text_norm(&doc, fields);
        if stored.is_empty() {
            continue;
        }
        let score = strsim::normalized_levenshtein(norm, &stored) as f32;
        if score <= 0.0 {
            continue;
        }
        let hit = doc_to_hit(&doc, fields, score);
        match best.get(&hit.line_id) {
            Some(prev) if prev.score >= score => {}
            _ => {
                best.insert(hit.line_id.clone(), hit);
            }
        }
    }

    let mut ranked: Vec<Hit> = best.into_values().collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.line_id.cmp(&b.line_id))
    });
    ranked.truncate(SIMILAR_LIMIT);
    Ok(ranked)
}

fn char_ngrams(text: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    if chars.len() < n {
        return vec![chars.iter().collect()];
    }
    chars
        .windows(n)
        .map(|w| w.iter().collect::<String>())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_index::{write_artifact, IndexableLine};
    use cbeta_parse::{GaijiMap, ParsedLine};
    use std::fs;

    fn sample_b21() -> Vec<IndexableLine> {
        vec![IndexableLine {
            line: ParsedLine {
                line_id: "T30n1578_p0268b21".into(),
                work_id: "T1578".into(),
                juan: 1,
                text_raw: "真性有為空，如幻緣生故".into(),
                lb_n: "0268b21".into(),
            },
            title: "大乘掌珍論".into(),
            author: "清辯菩薩,玄奘".into(),
            citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)".into(),
            cbeta_tag: "2026R2".into(),
        }]
    }

    fn temp_index(prefix: &str, lines: &[IndexableLine]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cbeta-verify-{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        write_artifact(&dir, "2026R2", "verhash", lines, &GaijiMap::default()).expect("write");
        fs::write(dir.join("CURRENT"), "2026R2-verhash\n").expect("CURRENT");
        dir
    }

    #[test]
    fn verify_exact_norm_is_original() {
        // Given: index with T1578 b21
        let dir = temp_index("exact", &sample_b21());
        // When: verify traditional original
        let report = verify(&dir, "真性有為空，如幻緣生故").expect("verify");
        // Then: is_original + exact line_id; similar empty on hash hit
        assert!(report.is_original, "expected original; got {report:?}");
        let hit = report.exact_hit.expect("exact_hit");
        assert_eq!(hit.line_id, "T30n1578_p0268b21");
        assert_eq!(hit.work_id, "T1578");
        assert!(report.similar.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_exact_accepts_simplified_quote() {
        let dir = temp_index("simp", &sample_b21());
        let report = verify(&dir, "真性有为空，如幻缘生故").expect("verify");
        assert!(report.is_original);
        assert_eq!(
            report.exact_hit.as_ref().map(|h| h.line_id.as_str()),
            Some("T30n1578_p0268b21")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    fn sample_verse_pair() -> Vec<IndexableLine> {
        vec![
            IndexableLine {
                line: ParsedLine {
                    line_id: "T30n1578_p0268b21".into(),
                    work_id: "T1578".into(),
                    juan: 1,
                    text_raw: "真性有為空，如幻緣生故".into(),
                    lb_n: "0268b21".into(),
                },
                title: "大乘掌珍論".into(),
                author: "清辯菩薩,玄奘".into(),
                citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)".into(),
                cbeta_tag: "2026R2".into(),
            },
            IndexableLine {
                line: ParsedLine {
                    line_id: "T30n1578_p0268b22".into(),
                    work_id: "T1578".into(),
                    juan: 1,
                    text_raw: "無為無有實，不起似空華".into(),
                    lb_n: "0268b22".into(),
                },
                title: "大乘掌珍論".into(),
                author: "清辯菩薩,玄奘".into(),
                citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b22)".into(),
                cbeta_tag: "2026R2".into(),
            },
            IndexableLine {
                line: ParsedLine {
                    line_id: "T08n0235_p0748c17".into(),
                    work_id: "T0235".into(),
                    juan: 1,
                    text_raw: "色不異空空不異色".into(),
                    lb_n: "0748c17".into(),
                },
                title: "般若波羅蜜多心經".into(),
                author: "玄奘".into(),
                citation: "(CBETA 2026.R2, T08, no. 235, p. 748, c17)".into(),
                cbeta_tag: "2026R2".into(),
            },
        ]
    }

    #[test]
    fn verify_user_verse_not_original_t1578() {
        // Given: b21/b22 original + unrelated T0235
        let dir = temp_index("sim", &sample_verse_pair());
        // When: issue #18 异文 verse
        let quote = "真性有为空，缘生故如幻，无为无起灭，不实若空华。";
        let report = verify(&dir, quote).expect("verify");
        // Then: not original; top similar work is T1578
        assert!(!report.is_original, "variant must not be original");
        assert!(report.exact_hit.is_none());
        assert!(
            !report.similar.is_empty(),
            "expected similar hits; got {report:?}"
        );
        assert_eq!(
            report.similar[0].work_id, "T1578",
            "top similar work_id; got {:?}",
            report.similar
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn char_ngrams_edges() {
        assert!(char_ngrams("空", 2).len() == 1);
        assert!(char_ngrams("", 2).is_empty());
        assert!(strsim::normalized_levenshtein("真性有為空", "真性有為空") > 0.99);
    }
}
