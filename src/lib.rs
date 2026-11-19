//! CTD - fast local full-text search for TXT files.

pub mod cli;
pub mod db;
pub mod error;
pub mod interrupt;
pub mod output;
pub mod scan;
pub mod search;
pub mod status;
pub mod sync;
pub mod textio;
pub mod workspace;

/// Application version (matches the release version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
