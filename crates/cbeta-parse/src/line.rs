//! One TEI body line between consecutive `<lb>` markers.

/// A single CBETA line unit (from one `<lb>` to the next).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    /// `{xml:id}_p{lb@n}`, e.g. `T30n1578_p0268b21`.
    pub line_id: String,
    /// Work id with volume stripped, e.g. `T1578`.
    pub work_id: String,
    /// Juan number active when the line started.
    pub juan: u64,
    /// Character text with tags stripped (lem preferred over rdg).
    pub text_raw: String,
    /// Raw `lb@n` value, e.g. `0268b21`.
    pub lb_n: String,
}
