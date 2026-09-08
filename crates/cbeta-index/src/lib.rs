//! Tantivy index builder for CBETA line units.

#![deny(missing_docs)]

mod tokenizer;

pub use tokenizer::{tokenize_all, CjkNgramTokenizer, CJK_TOKENIZER_NAME};
