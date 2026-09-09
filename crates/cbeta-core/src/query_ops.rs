//! Internal helpers for CBReader operator splitting (near/before/boolean/NOT).

use super::{ParseError, ParsedQuery};

/// Split `raw` on `sep`, trim, drop empties.
pub(crate) fn split_terms(raw: &str, sep: char) -> Vec<String> {
    raw.split(sep)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Split trailing `NOT` / `-` exclusions from a right-hand fragment.
pub(crate) fn split_excluded(right: &str) -> (String, Vec<String>) {
    let right = right.trim();
    if let Some((pos, rest)) = right.split_once(" NOT ") {
        let excluded: Vec<String> = rest
            .split(" NOT ")
            .flat_map(|chunk| chunk.split('-'))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return (pos.trim().to_string(), excluded);
    }
    if let Some((pos, rest)) = right.split_once('-') {
        let excluded: Vec<String> = rest
            .split('-')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !excluded.is_empty() {
            return (pos.trim().to_string(), excluded);
        }
    }
    (right.to_string(), Vec::new())
}

/// Parse `LEFT NEAR/N RIGHT` or `LEFT BEFORE/N RIGHT` (optional trailing NOT/-).
pub(crate) fn parse_near_like(
    raw: &str,
    token: &str,
    mode: &str,
    ordered: bool,
) -> Result<Option<ParsedQuery>, ParseError> {
    let needle = format!(" {token}/");
    let Some(idx) = raw.find(&needle) else {
        return Ok(None);
    };
    let left = raw[..idx].trim().to_string();
    if left.is_empty() {
        return Ok(None);
    }
    let rest = raw[idx + needle.len()..].trim();
    let Some((n, right)) = rest.split_once(' ') else {
        return Ok(None);
    };
    let Ok(within) = n.parse::<u32>() else {
        return Ok(None);
    };
    let (right_term, excluded) = split_excluded(right);
    if right_term.is_empty() {
        return Err(ParseError(format!(
            "{token}/N requires a term after the window"
        )));
    }
    Ok(Some(ParsedQuery {
        raw: raw.to_string(),
        mode: mode.into(),
        terms: vec![left, right_term],
        within_chars: Some(within),
        wildcard: None,
        ordered: Some(ordered),
        bool_op: None,
        excluded,
    }))
}

/// Parse `A - B` or `A NOT B` into keyword + excluded.
pub(crate) fn parse_exclusion(raw: &str) -> Option<ParsedQuery> {
    if raw.contains(" NOT ") {
        let mut parts = raw.split(" NOT ");
        let head = parts.next()?.trim();
        if head.is_empty() {
            return None;
        }
        let mut terms = vec![head.to_string()];
        let mut excluded = Vec::new();
        for part in parts {
            for chunk in part.split('-') {
                let t = chunk.trim();
                if !t.is_empty() {
                    excluded.push(t.to_string());
                }
            }
        }
        if head.contains('-') {
            let mut segs = head.split('-');
            if let Some(first) = segs.next() {
                let first = first.trim();
                if first.is_empty() {
                    return None;
                }
                terms = vec![first.to_string()];
                for s in segs {
                    let t = s.trim();
                    if !t.is_empty() {
                        excluded.insert(0, t.to_string());
                    }
                }
            }
        }
        if excluded.is_empty() {
            return None;
        }
        return Some(ParsedQuery {
            raw: raw.to_string(),
            mode: "keyword".into(),
            terms,
            within_chars: None,
            wildcard: None,
            ordered: None,
            bool_op: None,
            excluded,
        });
    }
    if raw.contains('-') {
        let mut segs = raw.split('-');
        let first = segs.next()?.trim();
        if first.is_empty() {
            return None;
        }
        let excluded: Vec<String> = segs
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if excluded.is_empty() {
            return None;
        }
        return Some(ParsedQuery {
            raw: raw.to_string(),
            mode: "keyword".into(),
            terms: vec![first.to_string()],
            within_chars: None,
            wildcard: None,
            ordered: None,
            bool_op: None,
            excluded,
        });
    }
    None
}
