//! Injectable gaiji / variant maps (no xml-p5 I/O).

use std::collections::HashMap;

/// Lookup table from CBETA gaiji code (or placeholder key) to Unicode.
#[derive(Debug, Clone, Default)]
pub struct GaijiMap {
    map: HashMap<String, String>,
}

impl GaijiMap {
    /// Build a map from `(key, unicode_or_placeholder)` pairs.
    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.into(), v.into());
        }
        Self { map }
    }

    /// Resolve one gaiji key. Missing → same-width `□`.
    pub fn resolve(&self, key: &str) -> String {
        self.map
            .get(key)
            .cloned()
            .unwrap_or_else(|| "□".to_string())
    }

    /// Replace known keys in `text`.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (key, val) in &self.map {
            out = out.replace(key, val);
        }
        out
    }
}
