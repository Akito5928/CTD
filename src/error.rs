use thiserror::Error;

/// Errors that need special handling for exit codes.
#[derive(Debug, Error)]
pub enum CtdError {
    #[error("no CTD workspace found in the current directory or any parent directory.\n  Run `ctd init` to create a workspace here.")]
    NoWorkspace,
    #[error("interrupted")]
    Interrupted,
}

pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_INTERRUPTED: i32 = 130;
