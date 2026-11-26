//! Terminal output helpers.

use crate::search::Display;
use std::io::{self, Write};

/// Set the Windows console output code page to UTF-8 (65001).
#[cfg(windows)]
pub fn init_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleOutputCP(code_page: u32) -> i32;
    }
    unsafe {
        SetConsoleOutputCP(65001);
    }
}

#[cfg(not(windows))]
pub fn init_console() {}

/// Print one result (standard output).
pub fn print_hit<W: Write>(out: &mut W, n: usize, rel: &str, d: &Display) -> io::Result<()> {
    writeln!(out, "[{n}] {rel}")?;
    match &d.line {
        Some((line_no, snippet)) => {
            writeln!(out, "    Line {line_no}: {snippet}")?;
            if d.more_lines > 0 {
                writeln!(out, "    (+{} more lines)", d.more_lines)?;
            }
        }
        None if d.unavailable => writeln!(out, "    (file could not be read)")?,
        None => writeln!(out, "    (path match)")?,
    }
    writeln!(out)
}

pub fn display_root(root: &std::path::Path) -> String {
    root.to_string_lossy().replace('\\', "/")
}
