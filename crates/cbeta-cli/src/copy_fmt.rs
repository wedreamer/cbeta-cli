//! Citation copy-block formatter (testable without a clipboard).

use cbeta_core::Hit;

/// Notes-ready block: `line_id title卷N：text` plus citation line.
pub fn format_copy_block(hit: &Hit) -> String {
    format!(
        "{} {}卷{}：{}\n{}",
        hit.line_id, hit.title, hit.juan, hit.text_raw, hit.citation
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hit() -> Hit {
        Hit {
            line_id: "T30n1578_p0268b21".into(),
            work_id: "T1578".into(),
            title: "大乘掌珍論".into(),
            author: "清辯菩薩,玄奘".into(),
            juan: 1,
            text_raw: "真性有為空，如幻緣生故；".into(),
            citation: "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)".into(),
            score: 1.0,
            cbeta_tag: "2026R2".into(),
        }
    }

    #[test]
    fn copy_block_has_line_id_title_juan_text_citation() {
        // Given: a known Hit
        // When: format_copy_block
        // Then: human-ux notes shape
        let s = format_copy_block(&sample_hit());
        assert!(s.starts_with("T30n1578_p0268b21 大乘掌珍論卷1："));
        assert!(s.contains("真性有為空"));
        assert!(s.contains("(CBETA 2026.R2, T30, no. 1578, p. 268, b21)"));
        assert_eq!(s.lines().count(), 2);
    }
}
