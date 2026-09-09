//! CBETA TEI P5 streaming parser and text normalization.
//!
//! P0 unit is one body line between consecutive `<lb>` markers.
//! Normalization (NFKC / OpenCC / gaiji / punct) lives in [`norm`].

#![deny(missing_docs)]

mod error;
mod gaiji;
mod line;
mod norm;
mod tei;

pub use error::{Error, Result};
pub use gaiji::GaijiMap;
pub use line::ParsedLine;
pub use norm::{
    nfkc, normalize_for_search, normalize_index, normalize_query, s2t, strip_punct, t2s,
};
pub use tei::{juan_from_cb_juan, line_id_from_lb, parse_tei_lines, work_id_from_xml_id};
