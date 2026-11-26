use std::fmt;

/// Errors that need special handling for exit codes.
#[derive(Debug)]
pub enum CtdError {
    /// Ctrl+C was received.
    Interrupted,
    /// Invalid arguments (exit code 2).
    Usage(String),
}

impl fmt::Display for CtdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CtdError::Interrupted => write!(f, "interrupted"),
            CtdError::Usage(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for CtdError {}

pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_INTERRUPTED: i32 = 130;
