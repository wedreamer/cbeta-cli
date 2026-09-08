//! CJK character n-gram tokenizer with real char positions.
//!
//! Stock [`tantivy::tokenizer::NgramTokenizer`] zeros `position`, which breaks
//! phrase queries. This tokenizer emits unigrams and 2–3-char ngrams where
//! each token's `position` is the starting character index (CJK char = 1).

use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

/// Registered name for [`CjkNgramTokenizer`] on a Tantivy index.
pub const CJK_TOKENIZER_NAME: &str = "cbeta_cjk";

/// Max n-gram length in characters (unigram + bi + tri).
const MAX_NGRAM: usize = 3;

/// Character n-gram tokenizer: unigrams and 2–3-grams with char positions.
#[derive(Clone, Default, Debug)]
pub struct CjkNgramTokenizer;

/// Precomputed n-gram stream over one input string.
#[derive(Debug)]
pub struct CjkNgramTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl Tokenizer for CjkNgramTokenizer {
    type TokenStream<'a> = CjkNgramTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        CjkNgramTokenStream {
            tokens: build_tokens(text),
            index: 0,
        }
    }
}

impl TokenStream for CjkNgramTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        // advance() returns true only when index is in 1..=len
        &self.tokens[self.index - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

/// Collect all tokens for tests and diagnostics.
pub fn tokenize_all(text: &str) -> Vec<Token> {
    let mut tok = CjkNgramTokenizer;
    let mut stream = tok.token_stream(text);
    let mut out = Vec::new();
    while stream.advance() {
        out.push(stream.token().clone());
    }
    out
}

fn build_tokens(text: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let mut tokens = Vec::new();

    for start in 0..n {
        let max_gram = (n - start).min(MAX_NGRAM);
        for gram in 1..=max_gram {
            let end = start + gram;
            let offset_from = chars[start].0;
            let offset_to = if end < n {
                chars[end].0
            } else {
                text.len()
            };
            let piece: String = chars[start..end].iter().map(|(_, c)| *c).collect();
            tokens.push(Token {
                offset_from,
                offset_to,
                position: start,
                text: piece,
                position_length: gram,
            });
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unigram_positions_increase_by_one() {
        // Given: three CJK chars
        // When: tokenize
        // Then: successive unigrams sit at positions 0,1,2
        let tokens = tokenize_all("真性有");
        let unigrams: Vec<_> = tokens
            .iter()
            .filter(|t| t.position_length == 1)
            .collect();
        assert_eq!(unigrams.len(), 3);
        assert_eq!(unigrams[0].text, "真");
        assert_eq!(unigrams[0].position, 0);
        assert_eq!(unigrams[1].text, "性");
        assert_eq!(unigrams[1].position, 1);
        assert_eq!(unigrams[2].text, "有");
        assert_eq!(unigrams[2].position, 2);
    }

    #[test]
    fn emits_unigrams_and_2_3_grams() {
        let tokens = tokenize_all("真性有為");
        let texts: Vec<&str> = tokens.iter().map(|t| t.text.as_str()).collect();
        assert!(texts.contains(&"真"));
        assert!(texts.contains(&"真性"));
        assert!(texts.contains(&"真性有"));
        assert!(texts.contains(&"性有為"));
        assert!(texts.contains(&"為"));
        // no 4-gram
        assert!(!texts.iter().any(|t| t.chars().count() > 3));
    }

    #[test]
    fn bigram_shares_start_char_position() {
        let tokens = tokenize_all("空性");
        let bi = tokens
            .iter()
            .find(|t| t.text == "空性")
            .expect("bigram");
        assert_eq!(bi.position, 0);
        assert_eq!(bi.position_length, 2);
        assert_eq!(bi.offset_from, 0);
        assert_eq!(bi.offset_to, "空性".len());
    }
}
