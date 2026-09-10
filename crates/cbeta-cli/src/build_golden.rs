//! Pre-publish gates: golden line_id + FETCHED.yaml lock pin check.

use cbeta_index::IndexableLine;

use crate::lifecycle::fetched::{is_complete, read_fetched};
use crate::lifecycle::lock::load_lock;
use crate::scope_io::CatalogRow;

/// Golden line that must appear when T1578 is in the catalog.
pub const GOLDEN_LINE_ID: &str = "T30n1578_p0268b21";

/// Require golden `T30n1578_p0268b21` when catalog includes T1578 (or any line already has it).
///
/// # Errors
/// Missing golden line when T1578 is expected.
pub fn check_golden(lines: &[IndexableLine], catalog: &[CatalogRow]) -> Result<(), String> {
    let has_t1578 = catalog.iter().any(|r| r.work_id == "T1578")
        || lines.iter().any(|l| l.line.work_id == "T1578");
    if !has_t1578 {
        return Ok(());
    }
    if lines.iter().any(|l| l.line.line_id == GOLDEN_LINE_ID) {
        return Ok(());
    }
    Err(format!(
        "golden line missing: expected {GOLDEN_LINE_ID} when T1578 is in scope"
    ))
}

/// When FETCHED.yaml exists and is complete, xml-p5 commit must match lock pins.
/// Missing FETCHED (mini fixture) → skip.
///
/// # Errors
/// Pin mismatch or incomplete FETCHED when present.
pub fn verify_lock(tag: &str) -> Result<(), String> {
    let fetched = match read_fetched(tag) {
        Ok(Some(doc)) => doc,
        Ok(None) => return Ok(()),
        Err(e) => return Err(e),
    };
    let lock = load_lock()?;
    let Some(pins) = lock.releases.get(tag) else {
        return Err(format!("no lock pins for tag {tag}"));
    };
    if !is_complete(tag, pins) {
        return Err(format!(
            "FETCHED.yaml for {tag} incomplete or pin mismatch; run: cbeta fetch --release {tag}"
        ));
    }
    if fetched.sources.xml_p5.commit != pins.xml_p5 {
        return Err(format!(
            "xml-p5 commit {} != lock pin {}",
            fetched.sources.xml_p5.commit, pins.xml_p5
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbeta_parse::ParsedLine;

    fn line(id: &str, work: &str) -> IndexableLine {
        IndexableLine {
            line: ParsedLine {
                line_id: id.into(),
                work_id: work.into(),
                juan: 1,
                text_raw: "x".into(),
                lb_n: "1".into(),
            },
            title: String::new(),
            author: String::new(),
            citation: String::new(),
            cbeta_tag: "2026R2".into(),
        }
    }

    #[test]
    fn golden_ok_when_present() {
        let lines = vec![line(GOLDEN_LINE_ID, "T1578")];
        let cat = vec![CatalogRow {
            work_id: "T1578".into(),
            canon: "T".into(),
            path: "a.xml".into(),
            title: String::new(),
            author: String::new(),
            dynasty: String::new(),
            category: String::new(),
            work_type: String::new(),
        }];
        assert!(check_golden(&lines, &cat).is_ok());
    }

    #[test]
    fn golden_err_when_t1578_without_line() {
        let lines = vec![line("T08n0235_p0001a01", "T0235")];
        let cat = vec![CatalogRow {
            work_id: "T1578".into(),
            canon: "T".into(),
            path: "a.xml".into(),
            title: String::new(),
            author: String::new(),
            dynasty: String::new(),
            category: String::new(),
            work_type: String::new(),
        }];
        let err = check_golden(&lines, &cat).unwrap_err();
        assert!(err.contains(GOLDEN_LINE_ID));
    }

    #[test]
    fn golden_skip_without_t1578() {
        let lines = vec![line("T08n0235_p0001a01", "T0235")];
        assert!(check_golden(&lines, &[]).is_ok());
    }
}
