//! Tantivy schema for one CBETA body line.

use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, Schema, SchemaBuilder, TextFieldIndexing,
    TextOptions, STORED, STRING,
};

use crate::tokenizer::CJK_TOKENIZER_NAME;

/// Named fields on the line schema.
#[derive(Debug, Clone)]
pub struct LineSchema {
    /// Built Tantivy schema.
    pub schema: Schema,
    /// Unique line id (`T31n1585_p0001a12`).
    pub line_id: Field,
    /// Work id (`T1585`).
    pub work_id: Field,
    /// Work title (display 繁體).
    pub title: Field,
    /// 作译者.
    pub author: Field,
    /// Corpus release tag (`2026R2`).
    pub cbeta_tag: Field,
    /// Juan number.
    pub juan: Field,
    /// Raw line text (stored only).
    pub text_raw: Field,
    /// Normalized search text (tokenized + stored for char-span confirm).
    pub text_norm: Field,
    /// SHA-256 hex of `text_norm` for O(1) exact verify (STRING|STORED).
    pub norm_hash: Field,
    /// Human citation string.
    pub citation: Field,
}

/// Build the line schema and field handles.
pub fn build_line_schema() -> LineSchema {
    let mut builder = SchemaBuilder::default();

    let line_id = builder.add_text_field("line_id", STRING | STORED);
    let work_id = builder.add_text_field("work_id", STRING | STORED);
    let title = builder.add_text_field("title", STRING | STORED);
    let author = builder.add_text_field("author", STRING | STORED);
    let cbeta_tag = builder.add_text_field("cbeta_tag", STRING | STORED);

    let juan_opts = NumericOptions::default().set_indexed().set_stored();
    let juan = builder.add_u64_field("juan", juan_opts);

    let text_raw = builder.add_text_field("text_raw", STORED);

    let text_indexing = TextFieldIndexing::default()
        .set_tokenizer(CJK_TOKENIZER_NAME)
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let text_norm_opts = TextOptions::default()
        .set_indexing_options(text_indexing)
        .set_stored();
    let text_norm = builder.add_text_field("text_norm", text_norm_opts);
    let norm_hash = builder.add_text_field("norm_hash", STRING | STORED);

    let citation = builder.add_text_field("citation", STORED);

    LineSchema {
        schema: builder.build(),
        line_id,
        work_id,
        title,
        author,
        cbeta_tag,
        juan,
        text_raw,
        text_norm,
        norm_hash,
        citation,
    }
}
