//! Index writers: RAM (tests) and on-disk Tantivy trees (staged by [`crate::publish`]).

use std::fs;
use std::path::{Path, PathBuf};

use cbeta_parse::{normalize_index, GaijiMap, ParsedLine};
use sha2::{Digest, Sha256};
use tantivy::directory::MmapDirectory;
use tantivy::schema::TantivyDocument;
use tantivy::{Index, IndexWriter};

use crate::error::Result;
use crate::publish::{publish_dir, resolve_publish_dest, stage_tmp_dir};
use crate::schema::{build_line_schema, LineSchema};
use crate::tokenizer::{CjkNgramTokenizer, CJK_TOKENIZER_NAME};

/// Stable SHA-256 hex of normalized text (not `DefaultHasher`).
pub fn hash_text_norm(text_norm: &str) -> String {
    let digest = Sha256::digest(text_norm.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// One line plus display/meta fields needed for later get/search hits.
#[derive(Debug, Clone)]
pub struct IndexableLine {
    /// Parsed TEI body line.
    pub line: ParsedLine,
    /// Work title (繁體).
    pub title: String,
    /// 作译者.
    pub author: String,
    /// Human citation string.
    pub citation: String,
    /// Corpus release tag.
    pub cbeta_tag: String,
}

/// Register the CJK tokenizer on an index (call after create/open).
pub fn register_tokenizers(index: &Index) {
    index
        .tokenizers()
        .register(CJK_TOKENIZER_NAME, CjkNgramTokenizer);
}

/// Open or create an in-RAM index with the line schema + CJK tokenizer.
pub fn create_ram_index() -> Result<(Index, LineSchema)> {
    let line_schema = build_line_schema();
    let index = Index::create_in_ram(line_schema.schema.clone());
    register_tokenizers(&index);
    Ok((index, line_schema))
}

/// Add lines to a writer; `text_norm` via [`normalize_index`].
pub fn add_lines(
    writer: &mut IndexWriter,
    fields: &LineSchema,
    lines: &[IndexableLine],
    gaiji: &GaijiMap,
) -> Result<()> {
    for item in lines {
        let text_norm = normalize_index(&item.line.text_raw, gaiji);
        let norm_hash = hash_text_norm(&text_norm);
        let mut doc = TantivyDocument::default();
        doc.add_text(fields.line_id, &item.line.line_id);
        doc.add_text(fields.work_id, &item.line.work_id);
        doc.add_text(fields.title, &item.title);
        doc.add_text(fields.author, &item.author);
        doc.add_text(fields.cbeta_tag, &item.cbeta_tag);
        doc.add_u64(fields.juan, item.line.juan);
        doc.add_text(fields.text_raw, &item.line.text_raw);
        doc.add_text(fields.text_norm, &text_norm);
        doc.add_text(fields.norm_hash, &norm_hash);
        doc.add_text(fields.citation, &item.citation);
        writer.add_document(doc)?;
    }
    Ok(())
}

/// Write Tantivy segment files into an existing directory (no rename; caller publishes).
pub fn write_tantivy_dir(dir: &Path, lines: &[IndexableLine], gaiji: &GaijiMap) -> Result<()> {
    let line_schema = build_line_schema();
    let mmap = MmapDirectory::open(dir)?;
    let index = Index::create(mmap, line_schema.schema.clone(), Default::default())?;
    register_tokenizers(&index);
    let mut writer = index.writer(15_000_000)?;
    add_lines(&mut writer, &line_schema, lines, gaiji)?;
    writer.commit()?;
    drop(writer);
    Ok(())
}

/// Stage under `{final_dir}.tmp`, write Tantivy, rename to free dest (never deletes live).
///
/// If `final_dir` already exists, publishes to a unique sibling `{name}.{unix_nanos}` and
/// returns that path. Previous artifact directories are left untouched.
pub fn write_atomic_dir(
    final_dir: &Path,
    lines: &[IndexableLine],
    gaiji: &GaijiMap,
) -> Result<PathBuf> {
    let tmp = stage_tmp_dir(final_dir)?;
    if let Err(e) = write_tantivy_dir(&tmp, lines, gaiji) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let dest = match resolve_publish_dest(final_dir) {
        Ok(d) => d,
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    if let Err(e) = publish_dir(&tmp, &dest) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    Ok(dest)
}

/// Convenience: write under `root/{tag}-{scope_hash}/` (or unique sibling) atomically.
pub fn write_artifact(
    root: &Path,
    tag: &str,
    scope_hash: &str,
    lines: &[IndexableLine],
    gaiji: &GaijiMap,
) -> Result<PathBuf> {
    let final_dir = root.join(crate::paths::artifact_dir_name(tag, scope_hash)?);
    write_atomic_dir(&final_dir, lines, gaiji)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::tmp_dir_for;
    use crate::tokenizer::tokenize_all;
    use cbeta_parse::ParsedLine;
    use tantivy::collector::TopDocs;
    use tantivy::query::{PhraseQuery, TermQuery};
    use tantivy::schema::{IndexRecordOption, Term, Value};

    fn sample_lines() -> Vec<IndexableLine> {
        vec![
            IndexableLine {
                line: ParsedLine {
                    line_id: "T30n1578_p0268a01".into(),
                    work_id: "T1578".into(),
                    juan: 1,
                    text_raw: "真性有為空".into(),
                    lb_n: "0268a01".into(),
                },
                title: "大乘掌珍論".into(),
                author: "清辯".into(),
                citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, a01)".into(),
                cbeta_tag: "2026R2".into(),
            },
            IndexableLine {
                line: ParsedLine {
                    line_id: "T31n1585_p0001a12".into(),
                    work_id: "T1585".into(),
                    juan: 1,
                    text_raw: "色不異空".into(),
                    lb_n: "0001a12".into(),
                },
                title: "成唯識論".into(),
                author: "玄奘".into(),
                citation: "(CBETA 2026.R2, T31, no. 1585, p. 1, a12)".into(),
                cbeta_tag: "2026R2".into(),
            },
            IndexableLine {
                line: ParsedLine {
                    line_id: "T08n0235_p0752b01".into(),
                    work_id: "T0235".into(),
                    juan: 1,
                    text_raw: "應無所住而生其心".into(),
                    lb_n: "0752b01".into(),
                },
                title: "金剛般若波羅蜜經".into(),
                author: "鳩摩羅什".into(),
                citation: "(CBETA 2026.R2, T08, no. 235, p. 752, b01)".into(),
                cbeta_tag: "2026R2".into(),
            },
        ]
    }

    #[test]
    fn ram_keyword_finds_zhenxing_youweikong() {
        // Given: three synthetic lines in RAM
        let (index, fields) = create_ram_index().expect("ram");
        let mut writer = index.writer(15_000_000).expect("writer");
        add_lines(&mut writer, &fields, &sample_lines(), &GaijiMap::default()).expect("add");
        writer.commit().expect("commit");

        // When: keyword search 真性有為空 (as term on a 3-gram / full match via unigrams)
        let reader = index.reader().expect("reader");
        let searcher = reader.searcher();
        let term = Term::from_field_text(fields.text_norm, "真性有");
        let query = TermQuery::new(term, IndexRecordOption::WithFreqsAndPositions);
        let hits = searcher
            .search(&query, &TopDocs::with_limit(10))
            .expect("search");
        assert!(!hits.is_empty(), "expected hit for 真性有");

        let doc: TantivyDocument = searcher.doc(hits[0].1).expect("doc");
        let line_id = doc
            .get_first(fields.line_id)
            .and_then(|v| v.as_str())
            .expect("line_id");
        assert_eq!(line_id, "T30n1578_p0268a01");
    }

    #[test]
    fn ram_phrase_uses_char_positions() {
        let (index, fields) = create_ram_index().expect("ram");
        let mut writer = index.writer(15_000_000).expect("writer");
        add_lines(&mut writer, &fields, &sample_lines(), &GaijiMap::default()).expect("add");
        writer.commit().expect("commit");

        // Phrase of successive unigrams 真 性 有 為 空
        let terms: Vec<Term> = ["真", "性", "有", "為", "空"]
            .into_iter()
            .map(|t| Term::from_field_text(fields.text_norm, t))
            .collect();
        let query = PhraseQuery::new(terms);
        let reader = index.reader().expect("reader");
        let searcher = reader.searcher();
        let hits = searcher
            .search(&query, &TopDocs::with_limit(5))
            .expect("phrase");
        assert_eq!(hits.len(), 1);

        // Positions on the analyzer stream must be 0..4 for unigrams
        let uni_pos: Vec<_> = tokenize_all("真性有為空")
            .into_iter()
            .filter(|t| t.position_length == 1)
            .map(|t| t.position)
            .collect();
        assert_eq!(uni_pos, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn atomic_write_uses_tmp_then_rename() {
        let dir = tempfile_dir();
        let root = dir.join("index");
        fs::create_dir_all(&root).expect("root");
        let gaiji = GaijiMap::default();
        let final_path =
            write_artifact(&root, "2026R2", "deadbeef", &sample_lines(), &gaiji).expect("write");

        assert!(final_path.is_dir());
        assert!(!tmp_dir_for(&final_path).exists());
        // second write must keep the first dir and publish a unique sibling
        let second =
            write_artifact(&root, "2026R2", "deadbeef", &sample_lines(), &gaiji).expect("rewrite");
        assert!(final_path.is_dir(), "previous artifact must remain");
        assert_ne!(second, final_path);
        assert!(second.is_dir());
        assert!(!tmp_dir_for(&root.join("2026R2-deadbeef")).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    fn tempfile_dir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "cbeta-index-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&p).expect("tmpdir");
        p
    }

    #[test]
    fn write_atomic_clears_stale_tmp_and_creates_parent() {
        let dir = tempfile_dir();
        let nested = dir.join("nested").join("root");
        let final_dir = nested.join("2026R2-tmpclean");
        let tmp = tmp_dir_for(&final_dir);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("stale"), "x").unwrap();
        let dest = write_atomic_dir(&final_dir, &sample_lines(), &GaijiMap::default()).unwrap();
        assert_eq!(dest, final_dir);
        assert!(final_dir.is_dir());
        assert!(!tmp.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ram_stores_meta_fields() {
        let (index, fields) = create_ram_index().expect("ram");
        let mut writer = index.writer(15_000_000).expect("writer");
        add_lines(&mut writer, &fields, &sample_lines(), &GaijiMap::default()).expect("add");
        writer.commit().expect("commit");
        let reader = index.reader().expect("reader");
        let searcher = reader.searcher();
        let term = Term::from_field_text(fields.line_id, "T31n1585_p0001a12");
        let q = TermQuery::new(term, IndexRecordOption::Basic);
        let hits = searcher.search(&q, &TopDocs::with_limit(1)).expect("q");
        assert_eq!(hits.len(), 1);
        let doc: TantivyDocument = searcher.doc(hits[0].1).expect("doc");
        let author = doc
            .get_first(fields.author)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(author, "玄奘");
    }
}
