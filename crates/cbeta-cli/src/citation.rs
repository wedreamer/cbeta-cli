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
