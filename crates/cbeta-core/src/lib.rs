//! Shared protocol types and query DSL for CBETA offline search.
//!
//! This crate owns `Command` / `Hit` / `Filters` / `parse_query` used by every
//! transport (CLI, MCP, HTTP). It is **not** the `cbeta` binary — that lives in
//! `cbeta-cli`.

#![deny(missing_docs)]

mod query;
mod types;
mod verify;

pub use query::{parse_query, ParseError, ParsedQuery};
pub use types::{Action, CatalogEntry, Command, Filters, Format, Hit, IndexInfo};
pub use verify::VerifyReport;
