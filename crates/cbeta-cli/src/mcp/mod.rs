//! MCP tool surface for `cbeta serve` (stdio / HTTP transport).

pub(crate) mod handlers;
mod query;
pub(crate) mod tools;

pub use tools::CbetaMcp;
