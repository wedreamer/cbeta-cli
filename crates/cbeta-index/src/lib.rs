//! Tantivy index builder for CBETA line units.
//!
//! Builds per-scope artifacts under `{CBETA_INDEX|$HOME/.cbeta/index}/{tag}-{scope_hash}/`
//! via atomic `{dir}.tmp` → rename. Search/get product wiring lives elsewhere (L3).

#![deny(missing_docs)]

mod error;
mod paths;
mod schema;
mod tokenizer;
mod writer;

pub use error::{Error, Result};
pub use paths::{
    artifact_dir_name, artifact_id, index_dir, index_root, require_path_segment, tmp_dir_for,
};
pub use schema::{build_line_schema, LineSchema};
pub use tokenizer::{tokenize_all, CjkNgramTokenizer, CJK_TOKENIZER_NAME};
pub use writer::{
    add_lines, create_ram_index, hash_text_norm, register_tokenizers, write_artifact,
    write_atomic_dir, IndexableLine,
};
