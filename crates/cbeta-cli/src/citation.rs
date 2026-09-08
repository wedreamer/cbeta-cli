//! Human citation strings from `line_id`.

use cbeta_parse::ParsedLine;

/// `(CBETA 2026.R2, T30, no. 1578, p. 268, b21)` from tag + line.
pub fn format_citation(cbeta_tag: &str, line: &ParsedLine) -> String {
    let tag_disp = display_tag(cbeta_tag);
    if let Some((xml_id, lb)) = line.line_id.split_once("_p") {
        if let Some(n_pos) = xml_id.find('n') {
            let canon_vol = &xml_id[..n_pos];
            let work_no = &xml_id[n_pos + 1..];
            if let Some(col_pos) = lb.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
                let page_raw = &lb[..col_pos.0];
                let page = page_raw
                    .parse::<u32>()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| page_raw.trim_start_matches('0').to_string());
                let rest = &lb[col_pos.0..];
                return format!(
                    "(CBETA {tag_disp}, {canon_vol}, no. {work_no}, p. {page}, {rest})"
                );
            }
        }
    }
    format!("(CBETA {tag_disp}, {})", line.line_id)
}

fn display_tag(tag: &str) -> String {
    // 2026R2 → 2026.R2 for human citation.
    if let Some(i) = tag.find('R') {
        if i > 0 {
            return format!("{}.{}", &tag[..i], &tag[i..]);
        }
    }
    tag.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_parse::ParsedLine;

    fn line(id: &str) -> ParsedLine {
        ParsedLine {
            line_id: id.into(),
            work_id: "T1578".into(),
            juan: 1,
            text_raw: "x".into(),
            lb_n: "0268b21".into(),
        }
    }

    #[test]
    fn format_citation_parses_standard_line_id() {
        // Given: T30n1578_p0268b21
        // When: format_citation
        // Then: human CBETA citation with dotted tag and stripped page zeros
        let c = format_citation("2026R2", &line("T30n1578_p0268b21"));
        assert_eq!(c, "(CBETA 2026.R2, T30, no. 1578, p. 268, b21)");
    }

    #[test]
    fn format_citation_falls_back_on_bad_line_id() {
        // Given: line_id without `_p` / volume `n`
        // When: format
        // Then: bare fallback keeps the raw id
        let c = format_citation("2026R2", &line("not-a-line-id"));
        assert_eq!(c, "(CBETA 2026.R2, not-a-line-id)");
        let c2 = format_citation("2026R2", &line("Tn1578_p0268b21"));
        // still has _p but n at index 1 with empty work — still structured if parseable
        assert!(c2.contains("CBETA"));
        let c3 = format_citation("2026R2", &line("T30n1578_p0268"));
        // no alphabetic col → fallback
        assert_eq!(c3, "(CBETA 2026.R2, T30n1578_p0268)");
    }

    #[test]
    fn display_tag_inserts_dot_before_r_or_passthrough() {
        assert_eq!(display_tag("2026R2"), "2026.R2");
        assert_eq!(display_tag("R2"), "R2"); // R at 0
        assert_eq!(display_tag("2026"), "2026"); // no R
    }
}
