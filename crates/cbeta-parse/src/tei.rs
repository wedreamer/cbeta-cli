//! Streaming TEI P5 line extraction (lb → next lb).

use crate::error::{Error, Result};
use crate::line::ParsedLine;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// Build `line_id` as `{xml_id}_p{lb_n}`.
pub fn line_id_from_lb(xml_id: &str, lb_n: &str) -> String {
    format!("{xml_id}_p{lb_n}")
}

/// Strip volume digits between canon letter(s) and `n`: `T30n1578` → `T1578`.
pub fn work_id_from_xml_id(xml_id: &str) -> String {
    // Find 'n' that separates volume from work number.
    if let Some(n_pos) = xml_id.find('n') {
        let prefix = &xml_id[..n_pos];
        let work_num = &xml_id[n_pos + 1..];
        // Drop trailing digits from prefix (volume), keep leading letters.
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
/// Skips `teiHeader`. Each unit is text between consecutive `<lb n="...">`
/// events. Inside `<app>`, prefers `<lem>` over `<rdg>`. Tags are stripped;
/// character order is preserved.
pub fn parse_tei_lines(xml: &str) -> Result<Vec<ParsedLine>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut xml_id = String::new();
    let mut juan: u64 = 0;
    let mut lines: Vec<ParsedLine> = Vec::new();

    let mut in_header = false;
    let mut header_depth: i32 = 0;
    let mut skip_rdg = 0_i32;
    let mut in_body = false;

    let mut current_lb: Option<String> = None;
    let mut current_text = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_vec();
                let local = local_name(&name);
                if local == b"teiHeader" {
                    in_header = true;
                    header_depth = 1;
                } else if in_header {
                    header_depth += 1;
                } else if local == b"TEI" || local == b"tei" {
                    if let Some(id) = attr_value(&e, b"id").or_else(|| attr_value(&e, b"xml:id")) {
                        xml_id = id;
                    }
                } else if local == b"body" {
                    in_body = true;
                } else if local == b"juan" || local == b"cb:juan" {
                    if let Some(n) = attr_value(&e, b"n") {
                        juan = juan_from_cb_juan(&n).unwrap_or(juan);
                    }
                } else if local == b"milestone" {
                    if attr_value(&e, b"unit").as_deref() == Some("juan") {
                        if let Some(n) = attr_value(&e, b"n") {
                            juan = juan_from_cb_juan(&n).unwrap_or(juan);
                        }
                    }
                } else if local == b"rdg" {
                    skip_rdg += 1;
                } else if local == b"lb" && in_body && !in_header {
                    flush_line(
                        &mut lines,
                        &xml_id,
                        juan,
                        &mut current_lb,
                        &mut current_text,
                    )?;
                    current_lb = attr_value(&e, b"n");
                    current_text.clear();
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_vec();
                let local = local_name(&name);
                if in_header {
                    // ignore empties in header
                } else if local == b"juan" || local == b"cb:juan" {
                    if let Some(n) = attr_value(&e, b"n") {
                        if let Ok(j) = juan_from_cb_juan(&n) {
                            juan = j;
                        }
                    }
                } else if local == b"milestone" {
                    if attr_value(&e, b"unit").as_deref() == Some("juan") {
                        if let Some(n) = attr_value(&e, b"n") {
                            if let Ok(j) = juan_from_cb_juan(&n) {
                                juan = j;
                            }
                        }
                    }
                } else if local == b"lb" && in_body {
                    flush_line(
                        &mut lines,
                        &xml_id,
                        juan,
                        &mut current_lb,
                        &mut current_text,
                    )?;
                    current_lb = attr_value(&e, b"n");
                    current_text.clear();
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_vec();
                let local = local_name(&name);
                if in_header {
                    header_depth -= 1;
                    if header_depth <= 0 {
                        in_header = false;
                        header_depth = 0;
                    }
                } else if local == b"rdg" {
                    skip_rdg = skip_rdg.saturating_sub(1);
                } else if local == b"body" {
                    flush_line(
                        &mut lines,
                        &xml_id,
                        juan,
                        &mut current_lb,
                        &mut current_text,
                    )?;
                    in_body = false;
                }
            }
            Ok(Event::Text(t)) => {
                if in_header || skip_rdg > 0 || !in_body || current_lb.is_none() {
                    // skip
                } else {
                    let text = t.unescape().map_err(|e| Error::Xml(e.to_string()))?;
                    current_text.push_str(&text);
                }
            }
            Ok(Event::CData(t)) => {
                if !in_header && skip_rdg == 0 && in_body && current_lb.is_some() {
                    let text = String::from_utf8_lossy(t.as_ref());
                    current_text.push_str(&text);
                }
            }
            Ok(Event::Eof) => {
                flush_line(
                    &mut lines,
                    &xml_id,
                    juan,
                    &mut current_lb,
                    &mut current_text,
                )?;
                break;
            }
            Ok(_) => {}
            Err(e) => return Err(e.into()),
        }
        buf.clear();
    }

    if xml_id.is_empty() {
        return Err(Error::Missing("TEI xml:id"));
    }
    Ok(lines)
}

fn flush_line(
    lines: &mut Vec<ParsedLine>,
    xml_id: &str,
    juan: u64,
    current_lb: &mut Option<String>,
    current_text: &mut String,
) -> Result<()> {
    if let Some(lb_n) = current_lb.take() {
        if xml_id.is_empty() {
            return Err(Error::Missing("TEI xml:id"));
        }
        let text_raw = current_text.trim().to_string();
        current_text.clear();
        lines.push(ParsedLine {
            line_id: line_id_from_lb(xml_id, &lb_n),
            work_id: work_id_from_xml_id(xml_id),
            juan,
            text_raw,
            lb_n,
        });
    }
    Ok(())
}

fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().rposition(|&b| b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key_bytes = attr.key.as_ref();
        let local = local_name(key_bytes);
        let matched = key_bytes == key || local == key || (key == b"id" && local == b"id");
        if matched {
            let v = attr.unescape_value().ok()?.into_owned();
            return Some(v);
        }
    }
    None
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
            // Never invent ghost a12 for T1578 fixture
            if xml_id == "T30n1578" {
                assert!(
                    lines.iter().all(|l| l.line_id != "T30n1578_p0268a12"),
                    "ghost line_id must not appear"
                );
            }
        }
    }
}
