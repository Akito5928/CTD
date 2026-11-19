//! Ctrl+C handling.
//!
//! While waiting at a confirmation prompt, Ctrl+C terminates immediately with
//! exit code 130. During long operations the handler only sets a flag; the
//! operation checks it between files, rolls back and exits with code 130.

use crate::error::{CtdError, EXIT_INTERRUPTED};
use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static IN_PROMPT: AtomicBool = AtomicBool::new(false);

pub fn install() {
    let _ = ctrlc::set_handler(|| {
        // A second Ctrl+C (or one at a prompt) terminates immediately.
        if IN_PROMPT.load(Ordering::SeqCst) || INTERRUPTED.swap(true, Ordering::SeqCst) {
            std::process::exit(EXIT_INTERRUPTED);
        }
    });
}

pub fn set_prompting(on: bool) {
    IN_PROMPT.store(on, Ordering::SeqCst);
}

pub fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Returns `Err(Interrupted)` once Ctrl+C was received.
pub fn check() -> Result<(), CtdError> {
    if is_interrupted() {
        Err(CtdError::Interrupted)
    } else {
        Ok(())
    }
}
