//! SQLite-backed cookie store with import/export.
//!
//! Persists cookies across sessions, supports filtering by domain,
//! and can export in multiple formats (JSON, Netscape, header string).

pub mod store;
