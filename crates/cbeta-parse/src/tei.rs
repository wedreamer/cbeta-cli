//! Streaming TEI P5 line extraction (lb → next lb).

use crate::error::{Error, Result};
use crate::line::ParsedLine;

/// Build `line_id` as `{xml_id}_p{lb_n}`.
pub fn line_id_from_lb(xml_id: &str, lb_n: &str) -> String {
    format!("{xml_id}_p{lb_n}")
}

/// Strip volume digits between canon letter(s) and `n`: `T30n1578` → `T1578`.
pub fn work_id_from_xml_id(xml_id: &str) -> String {
    if let Some(n_pos) = xml_id.find('n') {
        let prefix = &xml_id[..n_pos];
        let work_num = &xml_id[n_pos + 1..];
        let letters_end = prefix
            .char_indices()
            .find(|(_, c)| c.is_ascii_digit())
            .map(|(i, _)| i)
            .unwrap_or(prefix.len());
        let letters = &prefix[..letters_end];
        return format!("{letters}{work_num}");
    }
    xml_id.to_string()
}

/// Parse `cb:juan@n` (or milestone) attribute into a juan number.
pub fn juan_from_cb_juan(n: &str) -> Result<u64> {
    n.parse::<u64>()
        .map_err(|_| Error::Missing("cb:juan@n numeric"))
}

/// Stream body lines from a TEI P5 document string.
///
/// Stub for golden tests — filled in by the streaming feat commit.
pub fn parse_tei_lines(_xml: &str) -> Result<Vec<ParsedLine>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_id_from_lb_t1578() {
        assert_eq!(line_id_from_lb("T30n1578", "0268b21"), "T30n1578_p0268b21");
    }

    #[test]
    fn work_id_strips_volume() {
        assert_eq!(work_id_from_xml_id("T30n1578"), "T1578");
        assert_eq!(work_id_from_xml_id("T08n0235"), "T0235");
        assert_eq!(work_id_from_xml_id("T31n1585"), "T1585");
    }

    #[test]
    fn juan_from_cb_juan_parses() {
        assert_eq!(juan_from_cb_juan("1").unwrap(), 1);
        assert_eq!(juan_from_cb_juan("12").unwrap(), 12);
    }

    #[test]
    fn text_raw_keeps_han_drops_tags() {
        let xml = r#"<?xml version="1.0"?>
<TEI xml:id="T30n1578">
  <teiHeader><lb n="x"/>noise</teiHeader>
  <text><body>
    <cb:juan n="1" fun="open"/>
    <lb n="0268b21"/><app><lem>真性有為空</lem><rdg>真性有为空</rdg></app>
  </body></text>
</TEI>"#;
        let lines = parse_tei_lines(xml).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text_raw, "真性有為空");
        assert!(!lines[0].text_raw.contains("有为空"));
    }

    #[test]
    fn t0235_t1578_t1585_fixtures_emit_line_id() {
        let cases = [
            (
                "T08n0235",
                "0001a01",
                "T0235",
                "如是我聞",
                r#"<?xml version="1.0"?><TEI xml:id="T08n0235"><teiHeader/>
<text><body><cb:juan n="1"/><lb n="0001a01"/>如是我聞</body></text></TEI>"#,
            ),
            (
                "T30n1578",
                "0268b21",
                "T1578",
                "真性有為空",
                include_str!("../tests/data/t1578_mini.xml"),
            ),
            (
                "T31n1585",
                "0001a12",
                "T1585",
                "色即是空",
                r#"<?xml version="1.0"?><TEI xml:id="T31n1585"><teiHeader/>
<text><body><milestone unit="juan" n="1"/><lb n="0001a12"/>色即是空</body></text></TEI>"#,
            ),
        ];
        for (xml_id, lb, work, text, xml) in cases {
            let lines = parse_tei_lines(xml).unwrap();
            assert!(!lines.is_empty(), "expected lines for {xml_id}");
            let hit = lines
                .iter()
                .find(|l| l.lb_n == lb)
                .unwrap_or_else(|| panic!("missing lb {lb} in {xml_id}"));
            assert_eq!(hit.line_id, format!("{xml_id}_p{lb}"));
            assert_eq!(hit.work_id, work);
            assert!(
                hit.text_raw.contains(text),
                "text_raw={} missing {text}",
                hit.text_raw
            );
            if xml_id == "T30n1578" {
                assert!(
                    lines.iter().all(|l| l.line_id != "T30n1578_p0268a12"),
                    "ghost line_id must not appear"
                );
            }
        }
    }
}
