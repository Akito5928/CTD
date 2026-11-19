//! Terminal output helpers.

use crate::search::{FileHit, SearchOutput};
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

pub fn print_results<W: Write>(out: &mut W, res: &SearchOutput, context: usize) -> io::Result<()> {
    for (i, hit) in res.hits.iter().enumerate() {
        print_hit(out, i + 1, hit, context)?;
    }
    Ok(())
}

fn print_hit<W: Write>(out: &mut W, n: usize, hit: &FileHit, context: usize) -> io::Result<()> {
    writeln!(out, "[{n}] {}", hit.path)?;
    if hit.lines.is_empty() {
        writeln!(out, "    (path match)")?;
        writeln!(out)?;
        return Ok(());
    }
    for l in &hit.lines {
        writeln!(out, "    Line {}:", l.line_no)?;
        if context == 0 {
            writeln!(out, "    {}", l.snippet)?;
        } else {
            let width = (l.line_no + l.after.len()).to_string().len();
            for (no, text) in &l.before {
                writeln!(out, "      {no:>width$}: {text}")?;
            }
            writeln!(out, "    > {:>width$}: {}", l.line_no, l.snippet)?;
            for (no, text) in &l.after {
                writeln!(out, "      {no:>width$}: {text}")?;
            }
        }
        writeln!(out)?;
    }
    Ok(())
}
