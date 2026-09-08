use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ParseError {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[default]
    Search,
    Verify,
    Get,
    Read,
    Cite,
    Catalog,
    Info,
    Build,
    Serve,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    #[default]
    Tty,
    Plain,
    Json,
    Jsonl,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filters {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub works: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub juans: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedQuery {
    pub raw: String,
    pub mode: String,
    pub terms: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within_chars: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wildcard: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(default)]
    pub filters: Filters,
    pub format: Format,
    #[serde(default)]
    pub explain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_query: Option<ParsedQuery>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub line_id: String,
    pub work_id: String,
    pub title: String,
    pub text_raw: String,
    pub citation: String,
    pub score: f32,
}

pub fn parse_query(q: &str) -> Result<ParsedQuery, ParseError> {
    let raw = q.trim().to_string();
    if raw.contains('—')
        || raw.contains('＋')
        || raw.contains('＊')
        || raw.contains('＆')
        || raw.contains('？')
    {
        return Err(ParseError(
            "fullwidth operator rejected; use halfwidth + * & , - ? or NEAR/N".into(),
        ));
    }
    if let Some(parsed) = parse_near_like(&raw, "NEAR", "near", false) {
        return Ok(parsed);
    }
    if let Some(parsed) = parse_near_like(&raw, "BEFORE", "before", true) {
        return Ok(parsed);
    }
    if raw.contains('+') {
        let terms: Vec<String> = raw
            .split('+')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok(ParsedQuery {
            raw,
            mode: "near".into(),
            terms,
            within_chars: Some(30),
            wildcard: None,
        });
    }
    if raw.contains('*') && !raw.contains('?') {
        let terms: Vec<String> = raw
            .split('*')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok(ParsedQuery {
            raw,
            mode: "before".into(),
            terms,
            within_chars: Some(30),
            wildcard: None,
        });
    }
    if raw.contains('?') {
        return Ok(ParsedQuery {
            raw: raw.clone(),
            mode: "wildcard".into(),
            terms: vec![raw],
            within_chars: None,
            wildcard: Some(true),
        });
    }
    Ok(ParsedQuery {
        raw: raw.clone(),
        mode: "keyword".into(),
        terms: vec![raw],
        within_chars: None,
        wildcard: None,
    })
}

fn parse_near_like(raw: &str, token: &str, mode: &str, _ordered: bool) -> Option<ParsedQuery> {
    let needle = format!(" {token}/");
    let idx = raw.find(&needle)?;
    let left = raw[..idx].trim().to_string();
    let rest = raw[idx + needle.len()..].trim();
    let (n, right) = rest.split_once(' ')?;
    let within: u32 = n.parse().ok()?;
    Some(ParsedQuery {
        raw: raw.to_string(),
        mode: mode.into(),
        terms: vec![left, right.trim().to_string()],
        within_chars: Some(within),
        wildcard: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_is_near_30() {
        let p = parse_query("空性+缘生").unwrap();
        assert_eq!(p.mode, "near");
        assert_eq!(p.within_chars, Some(30));
        assert_eq!(p.terms, vec!["空性", "缘生"]);
    }

    #[test]
    fn near_16() {
        let p = parse_query("真如 NEAR/16 缘起").unwrap();
        assert_eq!(p.mode, "near");
        assert_eq!(p.within_chars, Some(16));
        assert_eq!(p.terms, vec!["真如", "缘起"]);
    }

    #[test]
    fn before_8() {
        let p = parse_query("空性 BEFORE/8 缘起").unwrap();
        assert_eq!(p.mode, "before");
        assert_eq!(p.within_chars, Some(8));
    }

    #[test]
    fn lotus_wildcard() {
        let p = parse_query("莲?色").unwrap();
        assert_eq!(p.mode, "wildcard");
    }

    #[test]
    fn reject_fullwidth_plus() {
        assert!(parse_query("空性＋缘生").is_err());
    }
}
