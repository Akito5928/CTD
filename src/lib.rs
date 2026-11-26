//! CTD - fast local text search.

pub mod cli;
pub mod config;
pub mod error;
pub mod interrupt;
pub mod output;
pub mod search;
pub mod status;
pub mod store;
pub mod textio;
pub mod update;
pub mod walk;

/// Application version (matches the release version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
